use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, ElementId, Entity, EntityId, Pixels, Point, ScrollHandle,
    ScrollWheelEvent, SharedString, Stateful, WeakEntity, Window, div, point, px,
};
use std::cell::Cell;
use std::rc::Rc;

use music::{GenreItem, GenreSection};
use router::{Destination, navigate};
use state::Playback;
use ui::{
    ActiveTheme as _, Button, Card, Deck, Glide, Mode, Skeleton, Text, Viewport, heading, scrolled,
    snapped,
};

use crate::shared::album_grid::CardGrid;
use crate::shared::cards;
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

pub(crate) struct Shelves {
    id: &'static str,
    host: EntityId,
    playback: Entity<Playback>,
    rails: Vec<(ScrollHandle, Glide)>,
    cursor: Option<(usize, usize)>,
    active_rail: Option<usize>,
    above: Rc<Cell<Option<Pixels>>>,
    outer: Option<ScrollHandle>,
    viewport: Viewport,
    pending: u64,
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
        }
    }

    fn tag(&self, kind: &str, place: usize) -> SharedString {
        SharedString::from(format!("{}-{kind}-{place}", self.id))
    }

    /// Draws loading rows through the page edges, with their headings inside the content inset.
    pub(crate) fn pending(&self, width: Pixels, cx: &App) -> Vec<AnyElement> {
        let theme = *cx.theme();
        let layout = CardGrid::layout(width);

        (0..PENDING)
            .map(|shelf| {
                div()
                    .flex()
                    .flex_col()
                    .mx(-theme.metrics.inset)
                    .gap_3()
                    .child(
                        Skeleton::new()
                            .mx(theme.metrics.inset)
                            .w(HEADING)
                            .h(theme.text(Text::Large))
                            .rounded(theme.radius),
                    )
                    .child(
                        div()
                            .flex()
                            .px(theme.metrics.inset)
                            .gap_4()
                            .overflow_hidden()
                            .children((0..=layout.columns).map(|place| {
                                Card::skeleton(self.tag("pending", shelf * 100 + place))
                                    .tile(layout.card)
                            })),
                    )
                    .into_any_element()
            })
            .collect()
    }

    pub(crate) fn reset(&mut self) {
        self.rails.clear();
        self.cursor = None;
        self.active_rail = None;
        self.pending = self.pending.wrapping_add(1);
    }

    pub(crate) fn viewport(&self, scroll: &ScrollHandle, window: &Window) -> Viewport {
        let seen = scroll.bounds().size.height;
        let top = match self.above.get() {
            Some(above) => scroll.bounds().origin.y - above,
            None => Pixels::ZERO,
        };
        Viewport::measured(top, seen, window)
    }

    /// Virtualizes full-width sections while keeping headings and lists inside the content inset.
    pub(crate) fn render(
        &mut self,
        sections: Rc<Vec<GenreSection>>,
        mode: Mode,
        width: Pixels,
        outer: ScrollHandle,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.outer = Some(outer.clone());
        self.viewport = self.viewport(&outer, window);
        while self.rails.len() < sections.len() {
            let mut glide = Glide::default();
            glide.watch(self.host);
            self.rails.push((ScrollHandle::new(), glide));
        }
        for (scroll, glide) in &self.rails {
            glide.sync(scroll);
        }

        let heights: Vec<Pixels> = sections
            .iter()
            .map(|section| self.height(section, mode, width, window, cx))
            .collect();
        self.register_surface(sections.clone(), mode, width, cx);
        if matches!(mode, Mode::Grid) {
            let columns = CardGrid::layout(width).columns;
            let tops = Deck::tops(&heights, STACK_GAP);
            let bottom = self.viewport.top + self.viewport.height;
            let visible = |place: usize, section: &GenreSection| {
                let top = tops[place];
                top < bottom
                    && top + heights[place] > self.viewport.top
                    && section.items.len() > columns
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
                    Mode::List => shelves.lane(place, section, width, window, cx),
                }
            });

        // Sections span the page so the deck's row clips do not cut off sideways cards.
        div()
            .relative()
            .mx(-cx.theme().metrics.inset)
            .child(stack)
            .into_any_element()
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
        self.active_rail = Some(next.0);
        moved
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

    fn follow_cursor(
        &mut self,
        sections: &[GenreSection],
        mode: Mode,
        width: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reveal_section(sections, mode, width, window, cx);
        self.reveal_cursor(mode, width, window);
        self.defer_reveal(mode, width, window, cx);
    }

    fn promote_selected(owner: &WeakEntity<Self>, cx: &mut App) {
        let Some(view) = owner.upgrade() else {
            return;
        };
        let Some((section, _)) = view.read(cx).cursor else {
            return;
        };
        let Some((scroll, glide)) = view
            .read(cx)
            .rails
            .get(section)
            .map(|(scroll, glide)| (scroll.clone(), glide.clone()))
        else {
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
            GenreItem::Artist(artist) => {
                navigate(Destination::Artist(artist.id.clone().into()), cx)
            }
            GenreItem::Track(track) => {
                let track = track.clone();
                self.playback
                    .update(cx, |playback, cx| playback.play_radio(&track, cx));
            }
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
        let step =
            |owner: WeakEntity<Self>, sections: Rc<Vec<GenreSection>>, by: isize, across: usize| {
                move |window: &mut Window, cx: &mut App| {
                    let moved = owner
                        .update(cx, |view, cx| {
                            let moved = view.move_cursor(&sections, by, across);
                            if moved {
                                view.follow_cursor(&sections, mode, width, window, cx);
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false);
                    if moved {
                        Self::promote_selected(&owner, cx);
                    }
                    moved
                }
            };
        let ready = sections.clone();
        let next = step(me.clone(), sections.clone(), 1, columns);
        let previous = step(me.clone(), sections.clone(), -1, columns);
        let left = step(me.clone(), sections.clone(), -1, 1);
        let right = step(me.clone(), sections.clone(), 1, 1);
        let enter_sections = sections.clone();
        let enter_owner = me.clone();
        let activate_sections = sections.clone();
        let activate_owner = me.clone();
        let focus_owner = me.clone();
        let selected_owner = me.clone();
        let escape_owner = me.clone();
        let first_owner = me.clone();
        let first_sections = sections.clone();
        let last_owner = me.clone();
        let last_sections = sections.clone();
        let page_up_owner = me.clone();
        let page_up_sections = sections.clone();
        let page_down_owner = me;
        let page_down_sections = sections;
        surface::register(
            surface::Surface::custom(
                move |_, cx| {
                    focus_owner
                        .upgrade()
                        .is_some_and(|view| view.read(cx).cursor.is_some())
                },
                move |_| ready.iter().any(|section| !section.items.is_empty()),
                move |cx| {
                    selected_owner
                        .upgrade()
                        .is_some_and(|view| view.read(cx).cursor.is_some())
                },
                next,
                previous,
                left,
                right,
                move |window, cx| {
                    let moved = enter_owner
                        .update(cx, |view, cx| match view.cursor.is_some() {
                            true => {
                                view.activate_card(&enter_sections, cx);
                                false
                            }
                            false => {
                                let moved = view.move_cursor(&enter_sections, 1, columns);
                                if moved {
                                    view.follow_cursor(&enter_sections, mode, width, window, cx);
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
                move |_, cx| {
                    activate_owner
                        .update(cx, |view, cx| view.activate_card(&activate_sections, cx))
                        .ok();
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
                            let cards = card_places(&first_sections);
                            let Some(cursor) = cards.first().copied() else {
                                return false;
                            };
                            view.cursor = Some(cursor);
                            view.active_rail = Some(cursor.0);
                            view.follow_cursor(&first_sections, mode, width, window, cx);
                            cx.notify();
                            true
                        })
                        .unwrap_or(false)
                },
                move |window, cx| {
                    last_owner
                        .update(cx, |view, cx| {
                            let cards = card_places(&last_sections);
                            let Some(cursor) = cards.last().copied() else {
                                return false;
                            };
                            view.cursor = Some(cursor);
                            view.active_rail = Some(cursor.0);
                            view.follow_cursor(&last_sections, mode, width, window, cx);
                            cx.notify();
                            true
                        })
                        .unwrap_or(false)
                },
                move |window, cx| {
                    page_up_owner
                        .update(cx, |view, cx| {
                            let page = columns.max(1) * ROWS.max(1);
                            let moved = view.move_cursor(&page_up_sections, -1, page);
                            if moved {
                                view.follow_cursor(&page_up_sections, mode, width, window, cx);
                                cx.notify();
                            }
                            moved
                        })
                        .unwrap_or(false)
                },
                move |window, cx| {
                    page_down_owner
                        .update(cx, |view, cx| {
                            let page = columns.max(1) * ROWS.max(1);
                            let moved = view.move_cursor(&page_down_sections, 1, page);
                            if moved {
                                view.follow_cursor(&page_down_sections, mode, width, window, cx);
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

    /// Lays out a section as inset columns without horizontal scrolling.
    fn lane(
        &self,
        place: usize,
        section: &GenreSection,
        width: Pixels,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let lanes = lanes(width);
        let total = section.items.len().min(lanes * ROWS);
        let cards = section
            .items
            .iter()
            .take(total)
            .enumerate()
            .map(|(index, item)| {
                self.card(
                    place * 100 + index,
                    item,
                    None,
                    self.cursor == Some((place, index)),
                    cx,
                )
                .aria_position_in_set(index + 1)
                .aria_size_of_set(total)
                .into_any_element()
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .px(cx.theme().metrics.inset)
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_end()
                    .h(head(window, cx))
                    .child(shelf_heading(
                        self.tag("heading", place),
                        i18n::translate(&section.title),
                        cx,
                    )),
            )
            .child(spread(cards, lanes))
            .into_any_element()
    }

    /// Clips a section's cards at the page edges and keeps its heading aligned with the content.
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
                    .px(cx.theme().metrics.inset)
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .h(head(window, cx))
                    .child(shelf_heading(
                        self.tag("heading", place),
                        i18n::translate(&section.title),
                        cx,
                    ))
                    .when(crowded, |this| {
                        this.child(self.arrows(place, &handle, &glide, me))
                    }),
            )
            .child(
                div()
                    .id((self.id, place))
                    .w_full()
                    .px(cx.theme().metrics.inset)
                    .h(tall)
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&handle)
                    .on_scroll_wheel({
                        let owner = me.clone();
                        let scroll = handle.clone();
                        move |event: &ScrollWheelEvent, window, cx| {
                            ui::hold_swipe(event, &scroll, cx);
                            if !sideways(event) {
                                return;
                            }
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
                            .rows(section.items.iter().map(|_| card))
                            .gap(RAIL_GAP)
                            .draw(move |index, _, cx| {
                                let Some(view) = drawn.upgrade() else {
                                    return div().into_any_element();
                                };
                                let Some(items) = feed.get(place).map(|section| &section.items)
                                else {
                                    return div().into_any_element();
                                };
                                let Some(item) = items.get(index) else {
                                    return div().into_any_element();
                                };

                                let shelves = view.read(cx);
                                shelves
                                    .card(
                                        place * 100 + index,
                                        item,
                                        Some(card),
                                        shelves.cursor == Some((place, index)),
                                        cx,
                                    )
                                    .aria_position_in_set(index + 1)
                                    .aria_size_of_set(items.len())
                                    .into_any_element()
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
        cx: &App,
    ) -> Card {
        cards::item_card(slot("item", id), item, &self.playback, cx)
            .map(|card| dressed(card.chosen(chosen), tile, cx))
    }
}

pub(crate) fn plate(
    id: impl Into<ElementId>,
    genre: &music::Genre,
    tile: Option<Pixels>,
    cx: &App,
) -> Card {
    cards::genre_card(id, genre).map(|card| dressed(card, tile, cx))
}

pub(crate) fn grid(
    id: &'static str,
    genres: Rc<Vec<music::Genre>>,
    width: Pixels,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let lanes = lanes(width);
    let row = snapped(cx.theme().metrics.list_row, window);
    let rows = genres.len().div_ceil(lanes);

    Deck::new(id)
        .rows((0..rows).map(|_| row))
        .gap(LANE_GAP)
        .draw(move |place, _, cx| {
            let first = place * lanes;
            let cells = (first..(first + lanes).min(genres.len())).map(|index| {
                plate((id, index), &genres[index], None, cx)
                    .aria_position_in_set(index + 1)
                    .aria_size_of_set(genres.len())
            });

            div()
                .flex()
                .w_full()
                .gap_2()
                .children(cells.map(|cell| div().flex().flex_col().flex_1().min_w_0().child(cell)))
                .into_any_element()
        })
        .into_any_element()
}

/// How many columns a grid of genre plates `width` wide lays out.
pub(crate) fn lanes(width: Pixels) -> usize {
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

fn card_places(sections: &[GenreSection]) -> Vec<(usize, usize)> {
    sections
        .iter()
        .enumerate()
        .flat_map(|(section, content)| (0..content.items.len()).map(move |item| (section, item)))
        .collect()
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

/// One horizontal card rail outside a shelf page: a heading with paging arrows over a
/// sideways scroller. Artist and album pages draw their recommendations with these, so
/// neither rebuilds the scroll plumbing a shelf keeps for its sections.
pub(crate) struct Rail {
    scroll: ScrollHandle,
    glide: Glide,
}

impl Rail {
    /// A rail that reports its scrolls to `host`, the page holding it.
    pub(crate) fn new(host: EntityId) -> Self {
        let mut glide = Glide::default();
        glide.watch(host);
        Self {
            scroll: ScrollHandle::new(),
            glide,
        }
    }

    /// Catches the glide up with its scroller before the frame draws it.
    pub(crate) fn sync(&self) {
        self.glide.sync(&self.scroll);
    }

    /// Puts the rail back at its first card at once, for when it starts showing other cards.
    pub(crate) fn rewind(&self) {
        self.glide.jump(&self.scroll, Point::default());
    }

    /// The rail under its heading: the title with paging arrows beside it while the cards
    /// overflow, the kind pills when the rail has kinds to narrow by, and the cards in a
    /// sideways scroller. `notify` repaints the page holding the rail, which the arrows
    /// need to refresh themselves.
    pub(crate) fn render(
        &self,
        spec: RailSpec,
        window: &Window,
        cx: &mut App,
        notify: &Rc<dyn Fn(&mut App)>,
        draw: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> AnyElement {
        let RailSpec {
            tag,
            place,
            title,
            count,
            tile,
            columns,
            tabs,
        } = spec;
        let tall = Card::tile_height(tile, window, cx);
        let inset = cx.theme().metrics.inset;
        let scrolled = self.scroll.clone();
        let glided = self.glide.clone();
        let arrows = match count > columns {
            true => Some(arrows(tag, place, &self.scroll, &self.glide, notify)),
            false => None,
        };

        div()
            .flex()
            .flex_col()
            .mx(-inset)
            .gap_3()
            .child(
                div()
                    .flex()
                    .px(inset)
                    .items_end()
                    .justify_between()
                    .gap_4()
                    .h(head(window, cx))
                    .child(shelf_heading(
                        SharedString::from(format!("{tag}-heading-{place}")),
                        title,
                        cx,
                    ))
                    .children(arrows),
            )
            .children(tabs.map(|tabs| div().px(inset).child(tabs)))
            .child(
                div()
                    .id((tag, place))
                    .w_full()
                    .px(inset)
                    .h(tall)
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.scroll)
                    .on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
                        ui::hold_swipe(event, &scrolled, cx);
                        if !sideways(event) {
                            return;
                        }
                        glided.nudge(&scrolled, window);
                    })
                    .child(
                        Deck::new(SharedString::from(format!("{tag}-rail-{place}")))
                            .across()
                            .rows(std::iter::repeat_n(tile, count))
                            .gap(RAIL_GAP)
                            .draw(draw),
                    ),
            )
            .into_any_element()
    }

    /// The rail while its cards are still on their way: the heading over skeleton tiles.
    pub(crate) fn pending(
        title: SharedString,
        tile: Pixels,
        columns: usize,
        window: &Window,
        cx: &mut App,
    ) -> AnyElement {
        let tall = Card::tile_height(tile, window, cx);
        let inset = cx.theme().metrics.inset;

        div()
            .flex()
            .flex_col()
            .mx(-inset)
            .gap_3()
            .child(
                div()
                    .flex()
                    .px(inset)
                    .items_end()
                    .h(head(window, cx))
                    .child(shelf_heading(
                        SharedString::from(format!("rail-pending-heading-{title}")),
                        title,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .px(inset)
                    .h(tall)
                    .gap(RAIL_GAP)
                    .overflow_hidden()
                    .children((0..=columns).map(|place| {
                        Card::skeleton(("rail-pending", place))
                            .tile(tile)
                            .into_any_element()
                    })),
            )
            .into_any_element()
    }
}

/// What a rail draws: its heading and the cards behind it. `tile` is how wide one card
/// stands and `columns` how many fit across, which is what decides whether the paging
/// arrows show. `tabs` is the tab row, drawn between the heading and the cards.
pub(crate) struct RailSpec {
    pub tag: &'static str,
    pub place: usize,
    pub title: SharedString,
    pub count: usize,
    pub tile: Pixels,
    pub columns: usize,
    pub tabs: Option<AnyElement>,
}

/// The paging arrows of a rail head, which is what tells a crowded rail from a short one.
fn arrows(
    tag: &'static str,
    place: usize,
    scroll: &ScrollHandle,
    glide: &Glide,
    notify: &Rc<dyn Fn(&mut App)>,
) -> AnyElement {
    let at = glide.goal(scroll).x;
    let reach = scroll.max_offset().x;

    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .child(
            arrow(
                SharedString::from(format!("{tag}-previous-{place}")),
                false,
                scroll,
                glide,
                notify,
            )
            .disabled(at >= -STEADY),
        )
        .child(
            arrow(
                SharedString::from(format!("{tag}-next-{place}")),
                true,
                scroll,
                glide,
                notify,
            )
            .disabled(reach > Pixels::ZERO && at <= STEADY - reach),
        )
        .into_any_element()
}

fn arrow(
    id: SharedString,
    forward: bool,
    scroll: &ScrollHandle,
    glide: &Glide,
    notify: &Rc<dyn Fn(&mut App)>,
) -> Button {
    let scroll = scroll.clone();
    let glide = glide.clone();
    let notify = notify.clone();

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
            slide(&scroll, &glide, forward, window);
            notify(cx);
        })
}

fn slot(kind: &'static str, place: usize) -> ElementId {
    ElementId::NamedInteger(SharedString::new_static(kind), place as u64)
}

/// Whether a wheel event should glide a rail. Only a notched wheel moving sideways does, since a
/// trackpad scrolls the rail itself and a vertical notch belongs to the page.
fn sideways(event: &ScrollWheelEvent) -> bool {
    !event.delta.precise() && event.delta.pixel_delta(px(1.)).x != Pixels::ZERO
}

/// A shelf or rail title as a level two heading, so screen readers can jump from one shelf to
/// the next. `id` has to be unique on the page.
fn shelf_heading(id: impl Into<ElementId>, title: SharedString, cx: &App) -> Stateful<Div> {
    heading(title.clone(), cx)
        .id(id)
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(title)
}
