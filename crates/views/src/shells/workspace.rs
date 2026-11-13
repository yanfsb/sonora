use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{AnyView, App, Bounds, Context, Entity, FocusHandle, Render, StyleRefinement};
use gpui::{Window, div, point, size};
use input::{
    HOME_CONTEXT, VIM_CONTEXT, VimBottom, VimLeft, VimNext, VimPaneDown, VimPaneLeft, VimPaneRight,
    VimPaneUp, VimPrevious, VimRight, VimScrollDown, VimScrollHalfDown, VimScrollHalfUp,
    VimScrollUp, VimTop, WORKSPACE_CONTEXT,
};
use state::{AppSettings, Playback, Queue, SideTab, Sonora};
use ui::{
    Activate, ActiveTheme as _, Deselect, Remove, SelectNext, SelectPrevious, ease_out_expo,
    entrance_span, shown_listing, veiled,
};

use crate::chrome::{
    Chrome, PlayerBar, SidebarLeft, SidebarRight, TitleBarOptions, ToastStack, UpdateNotice,
};
use crate::shared::confirm::Confirm;
use crate::shared::motion;
use crate::shared::panes::{self, Direction, Id};
use crate::shared::playlist_editor::PlaylistEditor;
use crate::shared::surface;
use crate::shared::tag_editor::TagEditor;
use crate::shells::Shell;

#[derive(Clone, Copy)]
struct ContentTransition {
    started: Instant,
    span: Duration,
}

impl ContentTransition {
    fn hidden(self) -> f32 {
        if self.span.is_zero() {
            return 0.;
        }
        let elapsed = self.started.elapsed().as_secs_f32();
        let progress = (elapsed / self.span.as_secs_f32()).clamp(0., 1.);
        1. - ease_out_expo(progress)
    }

    fn running(self) -> bool {
        self.started.elapsed() < self.span
    }
}

pub(crate) struct Workspace {
    sidebar: Entity<SidebarLeft>,
    player_bar: Entity<PlayerBar>,
    sidebar_right: Entity<SidebarRight>,
    settings: Entity<AppSettings>,
    home: bool,
    playlist_editor: Entity<PlaylistEditor>,
    tag_editor: Entity<TagEditor>,
    confirm: Entity<Confirm>,
    toasts: Entity<ToastStack>,
    notice: Entity<UpdateNotice>,
    content: AnyView,
    transition: Option<ContentTransition>,
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
        let player_bar = cx.new(|cx| PlayerBar::new(playback, queue, cx));
        let settings = Sonora::global(cx).settings.clone();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        Self {
            sidebar,
            player_bar,
            sidebar_right,
            settings,
            home: false,
            playlist_editor: PlaylistEditor::entity(cx),
            tag_editor: TagEditor::entity(cx),
            confirm: Confirm::entity(cx),
            toasts: cx.new(ToastStack::new),
            notice: cx.new(UpdateNotice::new),
            content,
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

    fn vim_scroll(&mut self, delta: gpui::Pixels, window: &mut Window, cx: &mut Context<Self>) {
        let moved = match () {
            _ if panes::focused(Id::Left, window, cx) => match delta > gpui::Pixels::ZERO {
                true => surface::next(Id::Left, window, cx),
                false => surface::previous(Id::Left, window, cx),
            },
            _ if panes::focused(Id::Right, window, cx) => match delta > gpui::Pixels::ZERO {
                true => surface::next(Id::Right, window, cx),
                false => surface::previous(Id::Right, window, cx),
            },
            _ if panes::focused(Id::Content, window, cx) => {
                let selected = match delta > gpui::Pixels::ZERO {
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

    #[allow(dead_code)]
    pub fn content(&self) -> &AnyView {
        &self.content
    }

    pub fn set_content(&mut self, content: AnyView, home: bool, cx: &mut Context<Self>) {
        self.content = content;
        self.home = home;
        cx.notify();
    }

    pub fn reveal_content(&mut self, cx: &mut Context<Self>) -> Duration {
        if cx.reduce_motion() {
            self.transition = None;
            return Duration::ZERO;
        }

        let span = entrance_span();
        self.transition = Some(ContentTransition {
            started: Instant::now(),
            span,
        });
        cx.notify();
        span
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

    #[allow(dead_code)]
    pub fn player_bar(&self) -> &Entity<PlayerBar> {
        &self.player_bar
    }
}

impl Shell for Workspace {
    fn title_bar(&self, content: Option<AnyView>, cx: &App) -> TitleBarOptions {
        let sidebar = self.sidebar.read(cx);

        TitleBarOptions {
            navigation: true,
            sidebar_open: sidebar.is_open(),
            sidebar_right: Some(self.sidebar_right.read(cx).is_open()),
            offset: sidebar.occupied_width(),
            border: true,
            content,
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.adapt(window, cx));
        let left = self.sidebar.read(cx).occupied_width();
        let right = self.sidebar_right.read(cx).occupied_width(window);
        Chrome::publish(left, right, cx);
        let covered = self.sidebar_right.read(cx).covers_content(window);
        let overlay = self.sidebar.read(cx).overlays();
        let bar_height = PlayerBar::height(window, cx);
        let viewport = window.viewport_size();
        let content_height = (viewport.height - bar_height).max(gpui::Pixels::ZERO);
        let left_panel = self.sidebar.read(cx);
        let left_width = left_panel.pane_width();
        let left_occupied = left_panel.occupied_width();
        let left_open = left_panel.is_open();
        let left_focus = left_panel.focus_handle();
        let right_panel = self.sidebar_right.read(cx);
        let right_width = right_panel.occupied_width(window);
        let right_focus = (right_width > gpui::Pixels::ZERO).then(|| right_panel.focus_handle(cx));
        panes::clear(cx);
        panes::register(
            Id::Content,
            self.focus.clone(),
            Bounds::new(
                point(left_occupied, gpui::Pixels::ZERO),
                size(
                    (viewport.width - left_occupied - right_width).max(gpui::Pixels::ZERO),
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
                    point(gpui::Pixels::ZERO, gpui::Pixels::ZERO),
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
                    point(viewport.width - right_width, gpui::Pixels::ZERO),
                    size(right_width, content_height),
                ),
                cx,
            );
        }
        let key_context = match (self.settings.read(cx).vim_mode(), self.home) {
            (true, true) => format!("{WORKSPACE_CONTEXT} {VIM_CONTEXT} {HOME_CONTEXT}"),
            (true, false) => format!("{WORKSPACE_CONTEXT} {VIM_CONTEXT}"),
            (false, true) => format!("{WORKSPACE_CONTEXT} {HOME_CONTEXT}"),
            (false, false) => WORKSPACE_CONTEXT.to_owned(),
        };
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
        let hidden = self.hidden(window, cx);
        let backdrop = cx.theme().background;

        div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .key_context(key_context.as_str())
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
                            .when(hidden > 0., |this| this.overflow_hidden())
                            .when(covered, |this| this.hidden())
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .top_0()
                                    .bottom_0()
                                    .flex()
                                    .flex_col()
                                    .map(|this| veiled(this, hidden))
                                    .child(
                                        self.content
                                            .clone()
                                            .cached(StyleRefinement::default().size_full()),
                                    ),
                            )
                            .when(hidden > 0., |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .right_0()
                                        .top_0()
                                        .bottom_0()
                                        .bg(backdrop)
                                        .opacity(hidden),
                                )
                            }),
                    )
                    .child(self.sidebar_right.clone())
                    .when(overlay, |this| this.child(self.sidebar.clone())),
            )
            .child(
                div()
                    .relative()
                    .child(
                        self.player_bar
                            .clone()
                            .cached(StyleRefinement::default().w_full().h(bar_height)),
                    )
                    .child(self.toasts.clone()),
            )
            .child(self.playlist_editor.clone())
            .child(self.tag_editor.clone())
            .child(self.confirm.clone())
            .child(self.notice.clone())
    }
}
