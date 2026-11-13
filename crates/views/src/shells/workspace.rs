use std::time::Duration;

use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{AnyView, App, Bounds, Context, Entity, FocusHandle, Pixels, Render, StyleRefinement};
use gpui::{Window, deferred, div, point, size};
use input::{
    VimBottom, VimLeft, VimNext, VimPaneDown, VimPaneLeft, VimPaneRight, VimPaneUp, VimPrevious,
    VimRight, VimScrollDown, VimScrollHalfDown, VimScrollHalfUp, VimScrollUp, VimTop,
    WORKSPACE_CONTEXT,
};
use state::{AppSettings, Playback, Queue, SideTab, Sonora};
use ui::{
    Activate, ActiveTheme as _, Deselect, Entrance, Remove, SelectNext, SelectPrevious, entering,
    shown_listing, veiled,
};

use crate::chrome::{
    AppNotice, Chrome, JamCode, JamJoin, PlayerBar, SessionCode, SessionJoin, SidebarLeft,
    SidebarRight, SwipeCue, TitleBarOptions, ToastStack,
};
use crate::shared::confirm::Confirm;
use crate::shared::menus::CardMenu;
use crate::shared::motion;
use crate::shared::panes::{self, Direction, Id};
use crate::shared::playlist_editor::PlaylistEditor;
use crate::shared::soloist::SoloistSetup;
use crate::shared::surface;
use crate::shared::tag_editor::TagEditor;
use crate::shared::widevine::WidevinePrompt;
use crate::shells::Shell;

/// The paint layer toasts take. Dialogs paint at 0 and menus at 1, so a toast shows over an
/// open dialog and a menu opened after it still shows over the toast.
const TOAST_LAYER: usize = 1;

pub(crate) struct Workspace {
    sidebar: Entity<SidebarLeft>,
    player_bar: Entity<PlayerBar>,
    settings: Entity<AppSettings>,
    sidebar_right: Entity<SidebarRight>,
    playlist_editor: Entity<PlaylistEditor>,
    tag_editor: Entity<TagEditor>,
    confirm: Entity<Confirm>,
    card_menu: Entity<CardMenu>,
    widevine: Entity<WidevinePrompt>,
    soloist: Entity<SoloistSetup>,
    jam_join: Entity<JamJoin>,
    jam_code: Entity<JamCode>,
    session_join: Entity<SessionJoin>,
    session_code: Entity<SessionCode>,
    toasts: Entity<ToastStack>,
    notice: Entity<AppNotice>,
    swipe_cue: Entity<SwipeCue>,
    content: AnyView,
    /// Whether the page is Home, where `/` opens Search.
    home: bool,
    /// A screen's own header, floated over the top of the page and outside its transition.
    /// The page pads itself to start beneath it.
    header: Option<AnyView>,
    /// Whether the page is laid out cached. A page that redraws part of itself every frame is
    /// not, since a cached view that redraws builds every view in it again, and it caches its
    /// own parts instead.
    cached: bool,
    transition: Option<Entrance>,
    focus: FocusHandle,
}

impl Workspace {
    pub fn new(
        playback: Entity<Playback>,
        queue: Entity<Queue>,
        content: AnyView,
        cx: &mut Context<Self>,
    ) -> Self {
        let sidebar = cx.new(SidebarLeft::new);
        let sidebar_right = cx.new(|cx| SidebarRight::new(queue.clone(), playback.clone(), cx));
        let player_bar = cx.new(|cx| PlayerBar::new(playback.clone(), queue, cx));
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        Self {
            sidebar,
            player_bar,
            settings,
            sidebar_right,
            playlist_editor: PlaylistEditor::entity(cx),
            tag_editor: TagEditor::entity(cx),
            confirm: Confirm::entity(cx),
            card_menu: CardMenu::entity(cx),
            widevine: cx.new(WidevinePrompt::new),
            soloist: cx.new(SoloistSetup::new),
            jam_join: cx.new(JamJoin::new),
            jam_code: cx.new(JamCode::new),
            session_join: cx.new(SessionJoin::new),
            session_code: cx.new(SessionCode::new),
            toasts: cx.new(ToastStack::new),
            notice: cx.new(AppNotice::new),
            swipe_cue: cx.new(SwipeCue::new),
            content,
            home: false,
            header: None,
            cached: true,
            transition: None,
            focus: cx.focus_handle(),
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    fn vim_next(&mut self, _: &VimNext, window: &mut Window, cx: &mut Context<Self>) {
        let row = cx.theme().metrics.row;
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::next(Id::Left, window, cx),
            _ if panes::focused(Id::Right, window, cx) => surface::next(Id::Right, window, cx),
            _ if panes::focused(Id::Content, window, cx) => {
                surface::next(Id::Content, window, cx) || motion::vertical(row, window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_previous(&mut self, _: &VimPrevious, window: &mut Window, cx: &mut Context<Self>) {
        let row = -cx.theme().metrics.row;
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::previous(Id::Left, window, cx),
            _ if panes::focused(Id::Right, window, cx) => surface::previous(Id::Right, window, cx),
            _ if panes::focused(Id::Content, window, cx) => {
                surface::previous(Id::Content, window, cx) || motion::vertical(row, window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_left(&mut self, _: &VimLeft, window: &mut Window, cx: &mut Context<Self>) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::left(Id::Left, window, cx),
            _ if panes::focused(Id::Right, window, cx) => surface::left(Id::Right, window, cx),
            _ if panes::focused(Id::Content, window, cx) => {
                surface::left(Id::Content, window, cx) || motion::horizontal_move(false, window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_right(&mut self, _: &VimRight, window: &mut Window, cx: &mut Context<Self>) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::right(Id::Left, window, cx),
            _ if panes::focused(Id::Right, window, cx) => surface::right(Id::Right, window, cx),
            _ if panes::focused(Id::Content, window, cx) => {
                surface::right(Id::Content, window, cx) || motion::horizontal_move(true, window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_top(&mut self, _: &VimTop, window: &mut Window, cx: &mut Context<Self>) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::first(Id::Left, window, cx),
            _ if panes::focused(Id::Right, window, cx) => surface::first(Id::Right, window, cx),
            _ if panes::focused(Id::Content, window, cx) => {
                surface::first(Id::Content, window, cx) || motion::top(window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_bottom(&mut self, _: &VimBottom, window: &mut Window, cx: &mut Context<Self>) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::last(Id::Left, window, cx),
            _ if panes::focused(Id::Right, window, cx) => surface::last(Id::Right, window, cx),
            _ if panes::focused(Id::Content, window, cx) => {
                surface::last(Id::Content, window, cx) || motion::bottom(window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_scroll(&mut self, delta: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => match delta > Pixels::ZERO {
                true => surface::next(Id::Left, window, cx),
                false => surface::previous(Id::Left, window, cx),
            },
            _ if panes::focused(Id::Right, window, cx) => match delta > Pixels::ZERO {
                true => surface::next(Id::Right, window, cx),
                false => surface::previous(Id::Right, window, cx),
            },
            _ if panes::focused(Id::Content, window, cx) => {
                let selected = match delta > Pixels::ZERO {
                    true => surface::next(Id::Content, window, cx),
                    false => surface::previous(Id::Content, window, cx),
                };
                selected || motion::vertical(delta, window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_scroll_down(&mut self, _: &VimScrollDown, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_scroll(cx.theme().metrics.row, window, cx);
    }

    fn vim_scroll_up(&mut self, _: &VimScrollUp, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_scroll(-cx.theme().metrics.row, window, cx);
    }

    fn vim_scroll_half_down(
        &mut self,
        _: &VimScrollHalfDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::page(Id::Left, true, window, cx),
            _ if panes::focused(Id::Right, window, cx) => {
                surface::page(Id::Right, true, window, cx)
            }
            _ if panes::focused(Id::Content, window, cx) => {
                surface::page(Id::Content, true, window, cx) || motion::half(true, window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_scroll_half_up(
        &mut self,
        _: &VimScrollHalfUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => surface::page(Id::Left, false, window, cx),
            _ if panes::focused(Id::Right, window, cx) => {
                surface::page(Id::Right, false, window, cx)
            }
            _ if panes::focused(Id::Content, window, cx) => {
                surface::page(Id::Content, false, window, cx) || motion::half(false, window, cx)
            }
            _ => false,
        };
        if moved {
            cx.stop_propagation();
        }
    }

    fn vim_pane(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        if panes::focus(direction, window, cx) {
            cx.stop_propagation();
        }
    }

    fn vim_pane_left(&mut self, _: &VimPaneLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_pane(Direction::Left, window, cx);
    }

    fn vim_pane_down(&mut self, _: &VimPaneDown, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_pane(Direction::Down, window, cx);
    }

    fn vim_pane_up(&mut self, _: &VimPaneUp, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_pane(Direction::Up, window, cx);
    }

    fn vim_pane_right(&mut self, _: &VimPaneRight, window: &mut Window, cx: &mut Context<Self>) {
        self.vim_pane(Direction::Right, window, cx);
    }

    pub fn toggle_sidebar(&self, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| sidebar.toggle(cx));
    }

    pub fn toggle_sidebar_right(&self, cx: &mut Context<Self>) {
        self.sidebar_right.update(cx, |panel, cx| panel.toggle(cx));
    }

    pub fn show_side(&self, tab: SideTab, cx: &mut Context<Self>) {
        self.sidebar_right
            .update(cx, |panel, cx| panel.show(tab, cx));
    }

    /// Opens the right sidebar on `tab` without closing it when it already shows there.
    pub fn reveal_side(&self, tab: SideTab, cx: &mut Context<Self>) {
        self.sidebar_right
            .update(cx, |panel, cx| panel.reveal(tab, cx));
    }

    /// Shows a page, with the header it wants above it or none, and laid out `cached` or not.
    /// The three arrive together so a header can never outlive its screen.
    pub fn set_content(
        &mut self,
        content: AnyView,
        header: Option<AnyView>,
        cached: bool,
        home: bool,
        cx: &mut Context<Self>,
    ) {
        self.content = content;
        self.header = header;
        self.cached = cached;
        self.home = home;
        cx.notify();
    }

    pub fn reveal_content(&mut self, cx: &mut Context<Self>) -> Duration {
        if cx.reduce_motion() {
            self.transition = None;
            return Duration::ZERO;
        }

        let entrance = Entrance::start();
        self.transition = Some(entrance);
        cx.notify();
        entrance.span()
    }

    pub fn finish_transition(&mut self, cx: &mut Context<Self>) {
        if self.transition.take().is_some() {
            cx.notify();
        }
    }

    fn hidden(&mut self, window: &mut Window, cx: &Context<Self>) -> f32 {
        if cx.reduce_motion() {
            self.transition = None;
            return 0.;
        }
        let Some(transition) = self.transition else {
            return 0.;
        };
        if transition.running() {
            window.request_animation_frame();
        }
        transition.hidden()
    }
}

impl Shell for Workspace {
    fn title_bar(&self, content: Option<AnyView>, cx: &App) -> TitleBarOptions {
        TitleBarOptions {
            navigation: true,
            sidebar_open: self.sidebar.read(cx).is_open(),
            sidebar_right: Some(self.sidebar_right.read(cx).is_open()),
            border: true,
            content,
            transparent: false,
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let right = self.sidebar_right.read(cx).occupied_width(window);
        let reserved = self.sidebar_right.read(cx).reserved_width();
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.adapt(reserved, window, cx));
        let left = self.sidebar.read(cx).occupied_width();
        let overlay_width = self.sidebar.read(cx).overlay_width();
        Chrome::publish(left, right, reserved, cx);
        let covered = self.sidebar_right.read(cx).covers_content(window);
        let overlay = self.sidebar.read(cx).overlays();
        let bar_height = PlayerBar::height(window, cx);
        let viewport = window.viewport_size();
        let content_height = (viewport.height - bar_height).max(Pixels::ZERO);
        let left_panel = self.sidebar.read(cx);
        let left_width = left_panel.breadth();
        let left_occupied = left_panel.occupied_width();
        let left_open = left_panel.is_open();
        let left_focus = left_panel.focus_handle();
        let right_panel = self.sidebar_right.read(cx);
        let right_width = right_panel.occupied_width(window);
        let right_focus = (right_width > Pixels::ZERO).then(|| right_panel.focus_handle(cx));
        panes::clear(cx);
        panes::register(
            Id::Content,
            self.focus.clone(),
            Bounds::new(
                point(left_occupied, Pixels::ZERO),
                size(
                    (viewport.width - left_occupied - right_width).max(Pixels::ZERO),
                    content_height,
                ),
            ),
            cx,
        );
        if left_open {
            panes::register(
                Id::Left,
                left_focus,
                Bounds::new(
                    point(Pixels::ZERO, Pixels::ZERO),
                    size(left_width, content_height),
                ),
                cx,
            );
        }
        if let Some(right_focus) = right_focus {
            panes::register(
                Id::Right,
                right_focus,
                Bounds::new(
                    point(viewport.width - right_width, Pixels::ZERO),
                    size(right_width, content_height),
                ),
                cx,
            );
        }
        let underlap = self.settings.read(cx).player_bar_translucent();
        let bottom = match underlap {
            true => bar_height,
            false => Pixels::ZERO,
        };
        Chrome::set_bottom(bottom, cx);
        // A cached view is laid out from the style given here and its own root
        // style is never consulted, so it can only be cached while it is in the
        // flow at a width this knows: an overlaid sidebar places itself, and a
        // closed one hides itself and takes no space at all.
        let sidebar_width = self.sidebar.read(cx).occupied_width();
        let sidebar = match overlay || sidebar_width == gpui::Pixels::ZERO {
            true => self.sidebar.clone().into_any_element(),
            false => self
                .sidebar
                .clone()
                .cached(StyleRefinement::default().w(sidebar_width).h_full())
                .into_any_element(),
        };
        // The header is cached at the height it last measured, so a page that redraws every
        // frame leaves it be. Until it has measured itself it is laid out as it is.
        let top = crate::screens::settings::Layout::of(window, cx);
        let header = self.header.clone().map(|header| match top.measured {
            true => header
                .cached(StyleRefinement::default().w_full().h(top.top))
                .into_any_element(),
            false => header.into_any_element(),
        });
        let hidden = self.hidden(window, cx);
        // The scrim is what fades the outgoing page: it is the page's own colour at
        // full strength, so covering the content with it costs no layout and the
        // content view keeps its cache. A see-through window has no such colour. The
        // scrim would be one more translucent layer over the one the root already
        // paints, and the content area would sit visibly darker than the chrome
        // around it for the length of the transition — a quad can only add coverage,
        // never replace it. There the content carries the fade itself, which is the
        // same curve at the price of leaving the cached layout path while it runs.
        let dissolving = cx.theme().transparent;
        let scrim = match dissolving {
            true => 0.,
            false => hidden,
        };
        let backdrop = cx.theme().background;
        // That price has to be paid for real, not merely accepted: opacity is baked
        // into a primitive as it is painted, and a cached view replays its primitives
        // until something notifies it, so a fading page that stayed cached would be
        // recorded on the first frame, nearly clear, and replayed that way long after
        // the transition had ended. While it carries its own fade the content is
        // rendered uncached. Every screen sizes its root `size_full`, so the layout
        // is the one the cache would have used, and only the frames of the
        // transition pay for the render.
        let content = match (dissolving && hidden > 0.) || !self.cached {
            true => self.content.clone().into_any_element(),
            false => self
                .content
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element(),
        };

        div()
            .id("workspace")
            .role(Role::Main)
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .key_context(super::vim_context(
                WORKSPACE_CONTEXT,
                self.settings.read(cx).vim_mode(),
                self.home,
            ))
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::vim_next))
            .on_action(cx.listener(Self::vim_previous))
            .on_action(cx.listener(Self::vim_left))
            .on_action(cx.listener(Self::vim_right))
            .on_action(cx.listener(Self::vim_top))
            .on_action(cx.listener(Self::vim_bottom))
            .on_action(cx.listener(Self::vim_scroll_down))
            .on_action(cx.listener(Self::vim_scroll_up))
            .on_action(cx.listener(Self::vim_scroll_half_down))
            .on_action(cx.listener(Self::vim_scroll_half_up))
            .on_action(cx.listener(Self::vim_pane_left))
            .on_action(cx.listener(Self::vim_pane_down))
            .on_action(cx.listener(Self::vim_pane_up))
            .on_action(cx.listener(Self::vim_pane_right))
            .on_action(|_: &SelectNext, window, cx| {
                if panes::focused(Id::Content, window, cx)
                    && let Some(table) = shown_listing(cx)
                    && !table.focused(window, cx)
                {
                    table.select_next(window, cx);
                    cx.stop_propagation();
                }
            })
            .on_action(|_: &SelectPrevious, window, cx| {
                if panes::focused(Id::Content, window, cx)
                    && let Some(table) = shown_listing(cx)
                    && !table.focused(window, cx)
                {
                    table.select_previous(window, cx);
                    cx.stop_propagation();
                }
            })
            .on_action(cx.listener(|this, _: &Deselect, window, cx| {
                let pane = match () {
                    _ if panes::focused(Id::Left, window, cx) => Some(Id::Left),
                    _ if panes::focused(Id::Right, window, cx) => Some(Id::Right),
                    _ if panes::focused(Id::Content, window, cx) => Some(Id::Content),
                    _ => None,
                };
                if let Some(pane) = pane.filter(|pane| surface::present(*pane, cx)) {
                    if surface::escape(pane, window, cx) {
                        window.focus(&this.focus, cx);
                    }
                    cx.stop_propagation();
                } else if panes::focused(Id::Content, window, cx)
                    && let Some(table) = shown_listing(cx)
                {
                    table.deselect(cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|_, _: &Activate, window, cx| {
                let pane = match () {
                    _ if panes::focused(Id::Left, window, cx) => Some(Id::Left),
                    _ if panes::focused(Id::Right, window, cx) => Some(Id::Right),
                    _ if panes::focused(Id::Content, window, cx) => Some(Id::Content),
                    _ => None,
                };
                if let Some(pane) = pane.filter(|pane| surface::present(*pane, cx)) {
                    if !surface::activate(pane, window, cx) {
                        surface::enter(pane, window, cx);
                    }
                    cx.stop_propagation();
                } else if panes::focused(Id::Content, window, cx)
                    && let Some(table) = shown_listing(cx)
                    && !table.focused(window, cx)
                {
                    table.activate(cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(|_: &Remove, window, cx| {
                if panes::focused(Id::Content, window, cx)
                    && let Some(table) = shown_listing(cx)
                    && !table.focused(window, cx)
                {
                    table.remove(cx);
                    cx.stop_propagation();
                }
            })
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(!overlay, |this| this.child(sidebar))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .ml(overlay_width)
                            .when(overlay, |this| this.overflow_hidden())
                            .when(covered, |this| this.hidden())
                            // A two-finger swipe across the page steps through its history,
                            // as in a browser.
                            .on_scroll_wheel({
                                let cue = self.swipe_cue.clone();
                                move |event, window, cx| {
                                    cue.update(cx, |cue, cx| cue.follow(event, window, cx));
                                }
                            })
                            .child(
                                div()
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h_0()
                                    .when(hidden > 0., |this| this.overflow_hidden())
                                    .child(
                                        div()
                                            .absolute()
                                            .left(-overlay_width)
                                            .right_0()
                                            .top_0()
                                            .bottom_0()
                                            .flex()
                                            .flex_col()
                                            .map(|this| match dissolving {
                                                true => entering(this, hidden),
                                                false => veiled(this, hidden),
                                            })
                                            .child(content),
                                    )
                                    .when(scrim > 0., |this| {
                                        this.child(
                                            div()
                                                .absolute()
                                                .left_0()
                                                .right_0()
                                                .top_0()
                                                .bottom_0()
                                                .bg(backdrop)
                                                .opacity(scrim),
                                        )
                                    })
                                    // The header comes last, so it paints over the page and
                                    // the scrim, and spans the same width the page does
                                    // beneath an overlaid sidebar.
                                    .when_some(header, |this, header| {
                                        this.child(
                                            div()
                                                .absolute()
                                                .left(-overlay_width)
                                                .right_0()
                                                .top_0()
                                                .child(header),
                                        )
                                    }),
                            )
                            .child(self.swipe_cue.clone()),
                    )
                    .child(self.sidebar_right.clone())
                    .when(overlay, |this| this.child(self.sidebar.clone())),
            )
            .child(
                div()
                    .relative()
                    .when(underlap, |bar| bar.absolute().left_0().right_0().bottom_0())
                    .child(
                        self.player_bar
                            .clone()
                            .cached(StyleRefinement::default().w_full().h(bar_height)),
                    )
                    // Deferred above the dialogs, which are deferred too, so a toast raised
                    // while one is open shows over it and its backdrop.
                    .child(deferred(self.toasts.clone()).with_priority(TOAST_LAYER)),
            )
            // deferred so the cards sit above the sidebars' resize grips, and ahead of the
            // dialogs so those still cover them
            .child(deferred(self.notice.clone()))
            .child(self.card_menu.clone())
            .child(self.playlist_editor.clone())
            .child(self.tag_editor.clone())
            .child(self.confirm.clone())
            .child(self.widevine.clone())
            .child(self.soloist.clone())
            .child(self.jam_join.clone())
            .child(self.jam_code.clone())
            .child(self.session_join.clone())
            .child(self.session_code.clone())
    }
}
