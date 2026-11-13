use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, ElementId, Entity, EntityId, FontWeight, MouseDownEvent, Pixels,
    Point, ScrollHandle, ScrollWheelEvent, SharedString, WeakEntity, Window, div, point, px,
};
use std::cell::Cell;
use std::rc::Rc;

use music::{Album, GenreItem, GenreSection, Playlist};
use router::{Destination, navigate};
use state::Playback;
use ui::{
    ActiveTheme as _, Button, Card, Deck, Glide, Mode, Popup, Skeleton, Text, Viewport, heading,
    scrolled, snapped,
};

use crate::shared::album_grid::CardGrid;
use crate::shared::cards;
use crate::shared::menus::Item;
use crate::shared::{motion, surface};

const PLATE: Pixels = px(260.);
const LANES: usize = 5;
const ROWS: usize = 3;
const STEADY: Pixels = px(0.5);
const PENDING: usize = 3;
const RAIL_GAP: Pixels = px(16.);
const STACK_GAP: Pixels = px(32.);
const LANE_GAP: Pixels = px(8.);
const HEADING_GAP: Pixels = px(12.);
const LEADING: f32 = 1.4;
const HEADING: Pixels = px(140.);

type Rail = (ScrollHandle, Glide);

pub(crate) struct Shelves {
    id: &'static str,
    host: EntityId,
    playback: Entity<Playback>,
    rails: Vec<Rail>,
    cursor: Option<(usize, usize)>,
    active_rail: Option<usize>,
    above: Rc<Cell<Option<Pixels>>>,
    outer: Option<ScrollHandle>,
    viewport: Viewport,
    pending: u64,
    context_menu: Option<(Item, Point<Pixels>)>,
}

impl Shelves {
    pub(crate) fn new(id: &'static str, host: EntityId, playback: Entity<Playback>) -> Self {
        Self {
            id,
            host,
            playback,
            rails: Vec::new(),
            cursor: None,
            active_rail: None,
            above: Rc::new(Cell::new(None)),
            outer: None,
            viewport: Viewport::default(),
            pending: 0,
            context_menu: None,
        }
    }

    fn tag(&self, kind: &str, place: usize) -> SharedString {
        SharedString::from(format!("{}-{kind}-{place}", self.id))
    }

    pub(crate) fn pending(&self, width: Pixels, cx: &App) -> Vec<AnyElement> {
        let theme = *cx.theme();
        let layout = CardGrid::layout(width);

        (0..PENDING)
            .map(|shelf| {
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        Skeleton::new()
                            .w(HEADING)
                            .h(theme.text(Text::Large))
                            .rounded(theme.radius),
                    )
                    .child(div().flex().w_full().gap_4().overflow_hidden().children(
                        (0..layout.columns).map(|place| {
                            Card::skeleton(self.tag("pending", shelf * 100 + place))
                                .tile(layout.card)
                        }),
                    ))
                    .into_any_element()
            })
            .collect()
    }

    pub(crate) fn reset(&mut self) {
        self.rails.clear();
        self.cursor = None;
        self.active_rail = None;
        self.pending = self.pending.wrapping_add(1);
        self.context_menu = None;
    }

    fn move_cursor(&mut self, sections: &[GenreSection], step: isize, columns: usize) -> bool {
        let cards: Vec<_> = sections
            .iter()
            .enumerate()
            .flat_map(|(section, content)| {
                (0..content.items.len()).map(move |item| (section, item))
            })
            .collect();
        if cards.is_empty() {
            return false;
        }
        let current = self
            .cursor
            .and_then(|cursor| cards.iter().position(|card| *card == cursor));
        let next = match current {
            None if step < 0 => cards.len() - 1,
            None => 0,
            Some(at) => {
                (at as isize + step * columns as isize).clamp(0, cards.len() as isize - 1) as usize
            }
        };
        let next = cards[next];
        let moved = self.cursor != Some(next);
        self.cursor = Some(next);
        moved
    }

    fn edge_cursor(
        &mut self,
        sections: &[GenreSection],
        last: bool,
        mode: Mode,
        width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let cards: Vec<_> = sections
            .iter()
            .enumerate()
            .flat_map(|(section, content)| {
                (0..content.items.len()).map(move |item| (section, item))
            })
            .collect();
        let Some(cursor) = cards.get(match last {
            true => cards.len().saturating_sub(1),
            false => 0,
        }) else {
            return false;
        };
        self.cursor = Some(*cursor);
        self.active_rail = Some(cursor.0);
        self.reveal_section(sections, mode, width, window, cx);
        self.reveal_cursor(mode, width, window);
        self.defer_reveal(mode, width, window, cx);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn page_cursor(
        &mut self,
        sections: &[GenreSection],
        down: bool,
        columns: usize,
        mode: Mode,
        width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let cards: Vec<_> = sections
            .iter()
            .enumerate()
            .flat_map(|(section, content)| {
                (0..content.items.len()).map(move |item| (section, item))
            })
            .collect();
        if cards.is_empty() {
            return false;
        }
        let current = self
            .cursor
            .and_then(|cursor| cards.iter().position(|card| *card == cursor));
        let page = columns.max(1) * ROWS.max(1);
        let index = match (current, down) {
            (Some(index), true) => index.saturating_add(page).min(cards.len() - 1),
            (Some(index), false) => index.saturating_sub(page),
            (None, true) => 0,
            (None, false) => cards.len() - 1,
        };
        self.cursor = cards.get(index).copied();
        self.active_rail = self.cursor.map(|(section, _)| section);
        self.reveal_section(sections, mode, width, window, cx);
        self.reveal_cursor(mode, width, window);
        self.defer_reveal(mode, width, window, cx);
        true
    }

    fn reveal_section(
        &self,
        sections: &[GenreSection],
        mode: Mode,
        width: Pixels,
        window: &Window,
        cx: &App,
    ) {
        let Some((section, _)) = self.cursor else {
            return;
        };
        let Some(outer) = &self.outer else {
            return;
        };
        let Some(height) = sections
            .get(section)
            .map(|section| self.height(section, mode, width, window, cx))
        else {
            return;
        };
        let heights: Vec<_> = sections
            .iter()
            .map(|section| self.height(section, mode, width, window, cx))
            .collect();
        let top = Deck::tops(&heights, STACK_GAP)
            .get(section)
            .copied()
            .unwrap_or_default();
        let bottom = top + height;
        let visible_top = self.viewport.top;
        let visible_bottom = visible_top + self.viewport.height;
        let delta = match () {
            _ if top < visible_top => top - visible_top,
            _ if bottom > visible_bottom => bottom - visible_bottom,
            _ => return,
        };
        let offset = outer.offset();
        let target = (scrolled(outer) + delta).clamp(Pixels::ZERO, outer.max_offset().y);
        outer.set_offset(point(offset.x, -target));
    }

    fn defer_reveal(
        &mut self,
        mode: Mode,
        width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending = self.pending.wrapping_add(1);
        let pending = self.pending;
        let owner = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            owner
                .update(cx, |view, cx| {
                    if view.pending != pending {
                        return;
                    }
                    view.pending = 0;
                    view.reveal_cursor(mode, width, window);
                    cx.notify();
                })
                .ok();
        });
    }

    fn reveal_cursor(&self, mode: Mode, width: Pixels, window: &mut Window) {
        if mode != Mode::Grid {
            return;
        }
        let Some((section, item)) = self.cursor else {
            return;
        };
        let Some((handle, glide)) = self.rails.get(section) else {
            return;
        };
        let layout = CardGrid::layout(width);
        let viewport = handle.bounds().size.width;
        let reach = handle.max_offset().x;
        if viewport <= Pixels::ZERO || reach <= Pixels::ZERO {
            return;
        }
        let stride = layout.card + RAIL_GAP;
        let left = stride * item as f32;
        let right = left + layout.card;
        let at = (-glide.goal(handle).x).clamp(Pixels::ZERO, reach);
        let end = at + viewport;
        let target = match () {
            _ if left < at => left,
            _ if right > end => right - viewport,
            _ => return,
        };
        glide.aim(
            handle,
            point(-target.clamp(Pixels::ZERO, reach), Pixels::ZERO),
            window,
        );
    }

    fn selected_rail(&self) -> Option<(ScrollHandle, Glide)> {
        self.cursor
            .and_then(|(section, _)| self.rails.get(section))
            .map(|(handle, glide)| (handle.clone(), glide.clone()))
    }

    fn promote_selected(owner: &WeakEntity<Self>, cx: &mut App) {
        let Some(view) = owner.upgrade() else {
            return;
        };
        let Some((scroll, glide)) = view.read(cx).selected_rail() else {
            return;
        };
        motion::promote_horizontal(&scroll, &glide, cx);
    }

    fn activate_card(&self, sections: &[GenreSection], cx: &mut App) {
        let Some((section, item)) = self.cursor else {
            return;
        };
        let Some(item) = sections
            .get(section)
            .and_then(|section| section.items.get(item))
        else {
            return;
        };
        match item {
            GenreItem::Album(album) => navigate(Destination::Album(album.id.clone().into()), cx),
            GenreItem::Playlist(playlist) => {
                navigate(Destination::Playlist(playlist.id.clone().into()), cx)
            }
            GenreItem::Genre(genre) => navigate(Destination::Genre(genre.id.clone().into()), cx),
        }
    }

    fn register_surface(
        &self,
        sections: Rc<Vec<GenreSection>>,
        mode: Mode,
        width: Pixels,
        cx: &mut Context<Self>,
    ) {
        let columns = match mode {
            Mode::Grid => CardGrid::layout(width).columns,
            Mode::List => lanes(width),
        };
        let me = cx.entity().downgrade();
        let ready = sections.clone();
        let next = sections.clone();
        let previous = sections.clone();
        let left = sections.clone();
        let right = sections.clone();
        let enter = sections.clone();
        let activate = sections.clone();
        let selected_owner = me.clone();
        let next_owner = me.clone();
        let previous_owner = me.clone();
        let left_owner = me.clone();
        let right_owner = me.clone();
        let enter_owner = me.clone();
        let activate_owner = me.clone();
        let escape_owner = me.clone();
        let first_owner = me.clone();
        let last_owner = me.clone();
        let page_up_owner = me.clone();
        let page_down_owner = me.clone();
        let first = sections.clone();
        let last = sections.clone();
        let page_up = sections.clone();
        let page_down = sections;
        surface::register(
            surface::Surface::custom(
                move |_, cx| {
                    me.upgrade()
                        .is_some_and(|view| view.read(cx).cursor.is_some())
                },
                move |_| ready.iter().any(|section| !section.items.is_empty()),
                move |cx| {
                    selected_owner
                        .upgrade()
                        .is_some_and(|view| view.read(cx).cursor.is_some())
                },
                {
                    let me = next_owner;
                    move |window, cx| {
                        let moved = me
                            .update(cx, |view, cx| {
                                let moved = view.move_cursor(&next, 1, columns);
                                view.active_rail = view.cursor.map(|(place, _)| place);
                                if moved {
                                    view.reveal_section(&next, mode, width, window, cx);
                                    view.reveal_cursor(mode, width, window);
                                    view.defer_reveal(mode, width, window, cx);
                                    cx.notify();
                                }
                                moved
                            })
                            .unwrap_or(false);
                        if moved {
                            Self::promote_selected(&me, cx);
                        }
                        moved
                    }
                },
                move |window, cx| {
                    let moved = previous_owner
                        .update(cx, |view, cx| {
                            let moved = view.move_cursor(&previous, -1, columns);
                            view.active_rail = view.cursor.map(|(place, _)| place);
                            if moved {
                                view.reveal_section(&previous, mode, width, window, cx);
                                view.reveal_cursor(mode, width, window);
                                view.defer_reveal(mode, width, window, cx);
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false);
                    if moved {
                        Self::promote_selected(&previous_owner, cx);
                    }
                    moved
                },
                move |window, cx| {
                    let moved = left_owner
                        .update(cx, |view, cx| {
                            let moved = view.move_cursor(&left, -1, 1);
                            view.active_rail = view.cursor.map(|(place, _)| place);
                            if moved {
                                view.reveal_section(&left, mode, width, window, cx);
                                view.reveal_cursor(mode, width, window);
                                view.defer_reveal(mode, width, window, cx);
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false);
                    if moved {
                        Self::promote_selected(&left_owner, cx);
                    }
                    moved
                },
                move |window, cx| {
                    let moved = right_owner
                        .update(cx, |view, cx| {
                            let moved = view.move_cursor(&right, 1, 1);
                            view.active_rail = view.cursor.map(|(place, _)| place);
                            if moved {
                                view.reveal_section(&right, mode, width, window, cx);
                                view.reveal_cursor(mode, width, window);
                                view.defer_reveal(mode, width, window, cx);
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false);
                    if moved {
                        Self::promote_selected(&right_owner, cx);
                    }
                    moved
                },
                move |window, cx| {
                    let moved = enter_owner
                        .update(cx, |view, cx| match view.cursor.is_some() {
                            true => {
                                view.activate_card(&enter, cx);
                                false
                            }
                            false => {
                                let moved = view.move_cursor(&enter, 1, columns);
                                view.active_rail = view.cursor.map(|(place, _)| place);
                                if moved {
                                    view.reveal_section(&enter, mode, width, window, cx);
                                    view.reveal_cursor(mode, width, window);
                                    view.defer_reveal(mode, width, window, cx);
                                    cx.notify();
                                }
                                moved
                            }
                        })
                        .unwrap_or(false);
                    if moved {
                        Self::promote_selected(&enter_owner, cx);
                    }
                },
                {
                    move |_, cx| {
                        activate_owner
                            .update(cx, |view, cx| view.activate_card(&activate, cx))
                            .ok();
                    }
                },
                move |_, cx| {
                    escape_owner
                        .update(cx, |view, cx| {
                            view.cursor = None;
                            cx.notify();
                        })
                        .ok();
                },
            )
            .edges(
                move |window, cx| {
                    first_owner
                        .update(cx, |view, cx| {
                            let moved = view.edge_cursor(&first, false, mode, width, window, cx);
                            if moved {
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false)
                },
                move |window, cx| {
                    last_owner
                        .update(cx, |view, cx| {
                            let moved = view.edge_cursor(&last, true, mode, width, window, cx);
                            if moved {
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false)
                },
                move |window, cx| {
                    page_up_owner
                        .update(cx, |view, cx| {
                            let moved =
                                view.page_cursor(&page_up, false, columns, mode, width, window, cx);
                            if moved {
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false)
                },
                move |window, cx| {
                    page_down_owner
                        .update(cx, |view, cx| {
                            let moved = view
                                .page_cursor(&page_down, true, columns, mode, width, window, cx);
                            if moved {
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false)
                },
            ),
            cx,
        );
    }

    fn popup(&self, cx: &mut Context<Self>) -> Option<Popup> {
        let (item, at) = self.context_menu.clone()?;

        Some(
            Popup::new(at, item.menu(self.playback.clone(), false, cx)).on_close(cx.listener(
                |this, _, _, cx| {
                    this.context_menu = None;
                    cx.notify();
                },
            )),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render(
        &mut self,
        sections: Rc<Vec<GenreSection>>,
        mode: Mode,
        width: Pixels,
        viewport: Viewport,
        outer: ScrollHandle,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.outer = Some(outer);
        self.viewport = viewport;
        while self.rails.len() < sections.len() {
            let mut glide = Glide::default();
            glide.watch(self.host);
            self.rails.push((ScrollHandle::new(), glide));
        }
        for (scroll, glide) in &self.rails {
            glide.sync(scroll);
        }
        self.register_surface(sections.clone(), mode, width, cx);
        let heights: Vec<Pixels> = sections
            .iter()
            .map(|section| self.height(section, mode, width, window, cx))
            .collect();
        if matches!(mode, Mode::Grid) {
            let columns = CardGrid::layout(width).columns;
            let tops = Deck::tops(&heights, STACK_GAP);
            let bottom = viewport.top + viewport.height;
            let visible = |place: usize, section: &GenreSection| {
                let top = tops[place];
                top < bottom && top + heights[place] > viewport.top && section.items.len() > columns
            };
            let active = self.active_rail.filter(|place| {
                sections
                    .get(*place)
                    .is_some_and(|section| visible(*place, section))
            });
            let selected = self.cursor.map(|(place, _)| place).filter(|place| {
                sections
                    .get(*place)
                    .is_some_and(|section| visible(*place, section))
            });
            let place = active.or(selected).or_else(|| {
                sections
                    .iter()
                    .enumerate()
                    .find(|(place, section)| visible(*place, section))
                    .map(|(place, _)| place)
            });
            self.active_rail = place;
            if let Some(place) = place {
                let (scroll, glide) = &self.rails[place];
                motion::set_horizontal(scroll, glide, cx);
            } else {
                motion::clear_horizontal(cx);
            }
        } else {
            motion::clear_horizontal(cx);
        }
        let me = cx.entity().downgrade();
        let above = self.above.clone();

        let stack = Deck::new(self.tag("stack", 0))
            .viewport(viewport)
            .rows(heights)
            .gap(STACK_GAP)
            .on_measure(move |top, _, _| above.set(Some(top)))
            .draw(move |place, window, cx| {
                let Some(view) = me.upgrade() else {
                    return div().into_any_element();
                };
                let Some(section) = sections.get(place) else {
                    return div().into_any_element();
                };
                let holder = view.downgrade();
                let shelves = view.read(cx);

                match mode {
                    Mode::Grid => shelves.rail(place, &sections, width, &holder, window, cx),
                    Mode::List => shelves.lane(place, section, width, &holder, window, cx),
                }
            });

        div()
            .relative()
            .w_full()
            .child(stack)
            .children(self.popup(cx))
            .into_any_element()
    }

    pub(crate) fn viewport(&self, scroll: &ScrollHandle, window: &Window) -> Viewport {
        let seen = scroll.bounds().size.height;

        let top = match self.above.get() {
            Some(above) => scroll.bounds().origin.y - above,
            None => Pixels::ZERO,
        };

        Viewport::measured(top, seen, window)
    }

    fn height(
        &self,
        section: &GenreSection,
        mode: Mode,
        width: Pixels,
        window: &Window,
        cx: &App,
    ) -> Pixels {
        let theme = *cx.theme();
        let head = head(window, cx) + HEADING_GAP;
        let body = match mode {
            Mode::Grid => Card::tile_height(CardGrid::layout(width).card, window, cx),
            Mode::List => {
                let lanes = lanes(width);
                let rows = section.items.len().min(lanes * ROWS).div_ceil(lanes);
                let row = snapped(theme.metrics.list_row, window);
                row * rows as f32 + LANE_GAP * rows.saturating_sub(1) as f32
            }
        };

        head + body
    }

    fn lane(
        &self,
        place: usize,
        section: &GenreSection,
        width: Pixels,
        me: &WeakEntity<Self>,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let lanes = lanes(width);
        let cards = section
            .items
            .iter()
            .take(lanes * ROWS)
            .enumerate()
            .map(|(index, item)| {
                self.card(
                    place * 100 + index,
                    item,
                    None,
                    self.cursor == Some((place, index)),
                    me,
                    cx,
                )
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_end()
                    .h(head(window, cx))
                    .child(heading(SharedString::from(section.title.clone()), cx)),
            )
            .child(spread(cards, lanes))
            .into_any_element()
    }

    fn rail(
        &self,
        place: usize,
        sections: &Rc<Vec<GenreSection>>,
        width: Pixels,
        me: &WeakEntity<Self>,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let Some(section) = sections.get(place) else {
            return div().into_any_element();
        };
        let layout = CardGrid::layout(width);
        let (handle, glide) = self.rails[place].clone();
        let crowded = section.items.len() > layout.columns;
        let seen = match handle.bounds().size.width {
            reach if reach > Pixels::ZERO => reach,
            _ => width,
        };
        let viewport = Viewport {
            top: -handle.offset().x.min(Pixels::ZERO),
            height: seen,
        };
        let feed = sections.clone();
        let drawn = me.clone();
        let card = layout.card;
        let tall = Card::tile_height(card, window, cx);

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .h(head(window, cx))
                    .child(heading(SharedString::from(section.title.clone()), cx))
                    .when(crowded, |this| {
                        this.child(self.arrows(place, &handle, &glide, me))
                    }),
            )
            .child(
                div()
                    .id((self.id, place))
                    .w_full()
                    .h(tall)
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&handle)
                    .on_scroll_wheel({
                        let owner = me.clone();
                        let scroll = handle.clone();
                        let glide = glide.clone();
                        move |event: &ScrollWheelEvent, window, cx| {
                            owner
                                .update(cx, |view, cx| {
                                    view.active_rail = Some(place);
                                    cx.notify();
                                })
                                .ok();
                            if event.delta.precise() {
                                motion::touch_horizontal(&scroll, &glide, cx);
                                return;
                            }
                            glide.nudge(&scroll, window);
                            motion::promote_horizontal(&scroll, &glide, cx);
                        }
                    })
                    .child(
                        Deck::new(self.tag("rail", place))
                            .across()
                            .viewport(viewport)
                            .rows(section.items.iter().map(|_| card))
                            .gap(RAIL_GAP)
                            .draw(move |index, _, cx| {
                                let Some(view) = drawn.upgrade() else {
                                    return div().into_any_element();
                                };
                                let Some(item) =
                                    feed.get(place).and_then(|section| section.items.get(index))
                                else {
                                    return div().into_any_element();
                                };

                                let holder = view.downgrade();
                                let shelves = view.read(cx);
                                shelves.card(
                                    place * 100 + index,
                                    item,
                                    Some(card),
                                    shelves.cursor == Some((place, index)),
                                    &holder,
                                    cx,
                                )
                            }),
                    ),
            )
            .into_any_element()
    }

    fn arrows(
        &self,
        place: usize,
        handle: &ScrollHandle,
        glide: &Glide,
        me: &WeakEntity<Self>,
    ) -> AnyElement {
        let at = glide.goal(handle).x;
        let reach = handle.max_offset().x;

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .child(
                self.arrow(self.tag("previous", place), place, false, handle, glide, me)
                    .disabled(at >= -STEADY),
            )
            .child(
                self.arrow(self.tag("next", place), place, true, handle, glide, me)
                    .disabled(reach > Pixels::ZERO && at <= STEADY - reach),
            )
            .into_any_element()
    }

    fn arrow(
        &self,
        id: impl Into<ElementId>,
        place: usize,
        forward: bool,
        handle: &ScrollHandle,
        glide: &Glide,
        me: &WeakEntity<Self>,
    ) -> Button {
        let handle = handle.clone();
        let glide = glide.clone();
        let me = me.clone();

        Button::new(id)
            .small()
            .outline()
            .icon(match forward {
                true => "icons/chevron-right.svg",
                false => "icons/chevron-left.svg",
            })
            .tooltip(match forward {
                true => "common-next",
                false => "common-previous",
            })
            .on_click(move |_, window, cx| {
                motion::touch_horizontal(&handle, &glide, cx);
                slide(&handle, &glide, forward, window);
                me.update(cx, |view, cx| {
                    view.active_rail = Some(place);
                    cx.notify();
                })
                .ok();
            })
    }

    fn card(
        &self,
        id: usize,
        item: &GenreItem,
        tile: Option<Pixels>,
        chosen: bool,
        me: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        match item {
            GenreItem::Playlist(playlist) => self.playlist_card(id, playlist, tile, chosen, me, cx),
            GenreItem::Album(album) => self.album_card(id, album, tile, chosen, me, cx),
            GenreItem::Genre(genre) => plate(slot("genre", id), genre, tile, chosen, cx),
        }
    }

    fn playlist_card(
        &self,
        id: usize,
        playlist: &Playlist,
        tile: Option<Pixels>,
        chosen: bool,
        me: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        cards::playlist_card(slot("playlist", id), playlist, &self.playback, cx)
            .map(|card| dressed(card.chosen(chosen), tile, cx))
            .menu(opener(me, Item::Playlist(playlist.clone())))
            .into_any_element()
    }

    fn album_card(
        &self,
        id: usize,
        album: &Album,
        tile: Option<Pixels>,
        chosen: bool,
        me: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        cards::album_card(slot("album", id), album, &self.playback, cx)
            .map(|card| dressed(card.chosen(chosen), tile, cx))
            .menu(opener(me, Item::Album(album.clone())))
            .into_any_element()
    }
}

fn opener(
    me: &WeakEntity<Shelves>,
    item: Item,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    let me = me.clone();

    move |event: &MouseDownEvent, _: &mut Window, cx: &mut App| {
        let at = event.position;
        me.update(cx, |this, cx| {
            this.context_menu = Some((item.clone(), at));
            cx.notify();
        })
        .ok();
    }
}

pub(crate) fn plate(
    id: impl Into<ElementId>,
    genre: &music::Genre,
    tile: Option<Pixels>,
    chosen: bool,
    cx: &App,
) -> AnyElement {
    let opened = SharedString::from(genre.id.clone());

    Card::new(id, SharedString::from(genre.name.clone()))
        .cover(genre.cover.clone())
        .fallback("icons/music.svg")
        .weight(FontWeight::SEMIBOLD)
        .map(|card| dressed(card.chosen(chosen), tile, cx))
        .press(move |_, _, cx| navigate(Destination::Genre(opened.clone()), cx))
        .into_any_element()
}

pub(crate) fn grid(
    id: &'static str,
    genres: Rc<Vec<music::Genre>>,
    width: Pixels,
    viewport: Viewport,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let lanes = lanes(width);
    let row = snapped(cx.theme().metrics.list_row, window);
    let rows = genres.len().div_ceil(lanes);

    Deck::new(id)
        .viewport(viewport)
        .rows((0..rows).map(|_| row))
        .gap(LANE_GAP)
        .draw(move |place, _, cx| {
            let first = place * lanes;
            let cells = (first..(first + lanes).min(genres.len()))
                .map(|index| plate((id, index), &genres[index], None, false, cx));

            div()
                .flex()
                .w_full()
                .gap_2()
                .children(cells.map(|cell| div().flex().flex_col().flex_1().min_w_0().child(cell)))
                .into_any_element()
        })
        .into_any_element()
}

fn lanes(width: Pixels) -> usize {
    ((width / PLATE).floor().max(1.) as usize).min(LANES)
}

fn spread(cards: Vec<AnyElement>, lanes: usize) -> Div {
    let mut columns: Vec<Vec<AnyElement>> = (0..lanes).map(|_| Vec::new()).collect();
    for (place, card) in cards.into_iter().enumerate() {
        columns[place % lanes].push(card);
    }

    div()
        .flex()
        .w_full()
        .gap_2()
        .children(columns.into_iter().map(|column| {
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .flex_col()
                .gap_2()
                .children(column)
        }))
}

fn head(window: &Window, cx: &App) -> Pixels {
    let theme = *cx.theme();

    snapped(theme.text(Text::Title) * LEADING, window).max(theme.metrics.control_small)
}

fn dressed(card: Card, tile: Option<Pixels>, cx: &App) -> Card {
    match tile {
        Some(width) => card.tile(width).flat(),
        None => card.bg(cx.theme().secondary),
    }
}

fn slide(handle: &ScrollHandle, glide: &Glide, forward: bool, window: &mut Window) {
    let page = handle.bounds().size.width;
    let at = glide.goal(handle);
    let next = match forward {
        true => at.x - page,
        false => at.x + page,
    };

    glide.aim(handle, point(next, at.y), window);
}

fn slot(kind: &'static str, place: usize) -> ElementId {
    ElementId::NamedInteger(SharedString::new_static(kind), place as u64)
}
