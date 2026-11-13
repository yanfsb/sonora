use gpui::{App, Global, Window};
use ui::Listing;

use crate::shared::panes::Id;

type Ready = Box<dyn Fn(&App) -> bool>;
type Focus = Box<dyn Fn(&Window, &App) -> bool>;
type Motion = Box<dyn Fn(&mut Window, &mut App) -> bool>;
type Action = Box<dyn Fn(&mut Window, &mut App)>;

pub(crate) struct Surface {
    pane: Id,
    focus: Focus,
    ready: Ready,
    selected: Ready,
    next: Motion,
    previous: Motion,
    left: Motion,
    right: Motion,
    enter: Action,
    activate: Action,
    escape: Action,
    first: Option<Motion>,
    last: Option<Motion>,
    page_up: Option<Motion>,
    page_down: Option<Motion>,
}

impl Surface {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn custom(
        focus: impl Fn(&Window, &App) -> bool + 'static,
        ready: impl Fn(&App) -> bool + 'static,
        selected: impl Fn(&App) -> bool + 'static,
        next: impl Fn(&mut Window, &mut App) -> bool + 'static,
        previous: impl Fn(&mut Window, &mut App) -> bool + 'static,
        left: impl Fn(&mut Window, &mut App) -> bool + 'static,
        right: impl Fn(&mut Window, &mut App) -> bool + 'static,
        enter: impl Fn(&mut Window, &mut App) + 'static,
        activate: impl Fn(&mut Window, &mut App) + 'static,
        escape: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            pane: Id::Content,
            focus: Box::new(focus),
            ready: Box::new(ready),
            selected: Box::new(selected),
            next: Box::new(next),
            previous: Box::new(previous),
            left: Box::new(left),
            right: Box::new(right),
            enter: Box::new(enter),
            activate: Box::new(activate),
            escape: Box::new(escape),
            first: None,
            last: None,
            page_up: None,
            page_down: None,
        }
    }

    pub(crate) fn edges(
        mut self,
        first: impl Fn(&mut Window, &mut App) -> bool + 'static,
        last: impl Fn(&mut Window, &mut App) -> bool + 'static,
        page_up: impl Fn(&mut Window, &mut App) -> bool + 'static,
        page_down: impl Fn(&mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.first = Some(Box::new(first));
        self.last = Some(Box::new(last));
        self.page_up = Some(Box::new(page_up));
        self.page_down = Some(Box::new(page_down));
        self
    }

    pub(crate) fn table(table: &dyn Listing) -> Self {
        Self::listing(table)
    }

    pub(crate) fn listing(listing: &dyn Listing) -> Self {
        let focused = listing.boxed();
        let ready = listing.boxed();
        let selected = listing.boxed();
        let next = listing.boxed();
        let previous = listing.boxed();
        let enter = listing.boxed();
        let activate = listing.boxed();
        let escape = listing.boxed();
        let first = listing.boxed();
        let last = listing.boxed();
        let page_up = listing.boxed();
        let page_down = listing.boxed();

        Self::custom(
            move |window, cx| focused.focused(window, cx),
            move |cx| ready.row_count(cx) > 0,
            move |cx| selected.selected(cx),
            move |window, cx| {
                next.select_next(window, cx);
                true
            },
            move |window, cx| {
                previous.select_previous(window, cx);
                true
            },
            |_, _| false,
            |_, _| false,
            move |window, cx| {
                if enter.selected(cx) {
                    enter.activate(cx);
                } else {
                    enter.select_next(window, cx);
                }
            },
            move |_, cx| activate.activate(cx),
            move |_, cx| escape.deselect(cx),
        )
        .edges(
            move |window, cx| {
                first.select_first(window, cx);
                true
            },
            move |window, cx| {
                last.select_last(window, cx);
                true
            },
            move |window, cx| {
                page_up.select_page(false, window, cx);
                true
            },
            move |window, cx| {
                page_down.select_page(true, window, cx);
                true
            },
        )
    }

    fn can_move(&self, cx: &App) -> bool {
        (self.ready)(cx)
    }

    fn focused(&self, window: &Window, cx: &App) -> bool {
        (self.focus)(window, cx)
    }

    fn selected(&self, cx: &App) -> bool {
        (self.selected)(cx)
    }

    fn next(&self, window: &mut Window, cx: &mut App) -> bool {
        self.can_move(cx) && (self.next)(window, cx)
    }

    fn previous(&self, window: &mut Window, cx: &mut App) -> bool {
        self.can_move(cx) && (self.previous)(window, cx)
    }

    fn left(&self, window: &mut Window, cx: &mut App) -> bool {
        self.can_move(cx) && (self.left)(window, cx)
    }

    fn right(&self, window: &mut Window, cx: &mut App) -> bool {
        self.can_move(cx) && (self.right)(window, cx)
    }

    fn enter(&self, window: &mut Window, cx: &mut App) {
        if self.can_move(cx) {
            (self.enter)(window, cx);
        }
    }

    fn activate(&self, window: &mut Window, cx: &mut App) {
        if self.focused(window, cx) && self.can_move(cx) && self.selected(cx) {
            (self.activate)(window, cx);
        }
    }

    fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        if !self.focused(window, cx) {
            return false;
        }
        (self.escape)(window, cx);
        true
    }

    fn edge(&self, last: bool, window: &mut Window, cx: &mut App) -> bool {
        let action = match last {
            true => self.last.as_ref(),
            false => self.first.as_ref(),
        };
        self.can_move(cx) && action.is_some_and(|action| action(window, cx))
    }

    fn page(&self, down: bool, window: &mut Window, cx: &mut App) -> bool {
        let action = match down {
            true => self.page_down.as_ref(),
            false => self.page_up.as_ref(),
        };
        self.can_move(cx) && action.is_some_and(|action| action(window, cx))
    }
}

#[derive(Default)]
struct Registry(Vec<Surface>, u64);

impl Global for Registry {}

pub(crate) fn present(pane: Id, cx: &App) -> bool {
    cx.try_global::<Registry>()
        .is_some_and(|registry| registry.0.iter().any(|surface| surface.pane == pane))
}

pub(crate) fn clear(cx: &mut App) {
    if cx.has_global::<Registry>() {
        let registry = cx.global_mut::<Registry>();
        registry.0.clear();
        registry.1 = registry.1.wrapping_add(1);
    }
}

pub(crate) fn register(surface: Surface, cx: &mut App) {
    register_for(Id::Content, surface, cx);
}

pub(crate) fn register_for(pane: Id, mut surface: Surface, cx: &mut App) {
    surface.pane = pane;
    let registry = cx.default_global::<Registry>();
    registry.0.retain(|current| current.pane != pane);
    registry.0.push(surface);
}

pub(crate) fn remove(pane: Id, cx: &mut App) {
    if cx.has_global::<Registry>() {
        cx.global_mut::<Registry>()
            .0
            .retain(|surface| surface.pane != pane);
    }
}

pub(crate) fn first(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    with_surface(pane, cx, |surface, cx| surface.edge(false, window, cx))
}

pub(crate) fn last(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    with_surface(pane, cx, |surface, cx| surface.edge(true, window, cx))
}

pub(crate) fn page(pane: Id, down: bool, window: &mut Window, cx: &mut App) -> bool {
    with_surface(pane, cx, |surface, cx| surface.page(down, window, cx))
}

pub(crate) fn next(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    with_surface(pane, cx, |surface, cx| surface.next(window, cx))
}

pub(crate) fn previous(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    with_surface(pane, cx, |surface, cx| surface.previous(window, cx))
}

pub(crate) fn left(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    with_surface(pane, cx, |surface, cx| surface.left(window, cx))
}

pub(crate) fn right(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    with_surface(pane, cx, |surface, cx| surface.right(window, cx))
}

pub(crate) fn enter(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    let Some(surface) = take(pane, cx) else {
        return false;
    };
    if surface.pane != pane {
        put(surface, cx);
        return false;
    }
    let revision = revision(cx);
    let ready = surface.can_move(cx);
    if ready {
        surface.enter(window, cx);
    }
    restore(surface, revision, cx);
    ready
}

pub(crate) fn activate(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    let Some(surface) = take(pane, cx) else {
        return false;
    };
    if surface.pane != pane {
        put(surface, cx);
        return false;
    }
    let revision = revision(cx);
    let active = surface.can_move(cx) && surface.selected(cx);
    if active {
        surface.activate(window, cx);
    }
    restore(surface, revision, cx);
    active
}

pub(crate) fn escape(pane: Id, window: &mut Window, cx: &mut App) -> bool {
    let Some(surface) = take(pane, cx) else {
        return false;
    };
    if surface.pane != pane {
        put(surface, cx);
        return false;
    }
    let revision = revision(cx);
    let escaped = surface.escape(window, cx);
    restore(surface, revision, cx);
    escaped
}

fn with_surface<F>(pane: Id, cx: &mut App, action: F) -> bool
where
    F: FnOnce(&Surface, &mut App) -> bool,
{
    let Some(surface) = take(pane, cx) else {
        return false;
    };
    let moved = action(&surface, cx);
    put(surface, cx);
    moved
}

fn revision(cx: &App) -> u64 {
    cx.try_global::<Registry>().map_or(0, |registry| registry.1)
}

fn restore(surface: Surface, expected: u64, cx: &mut App) {
    if revision(cx) == expected {
        put(surface, cx);
    }
}

fn take(pane: Id, cx: &mut App) -> Option<Surface> {
    if !cx.has_global::<Registry>() {
        return None;
    }
    let registry = cx.global_mut::<Registry>();
    let place = registry.0.iter().position(|surface| surface.pane == pane)?;
    Some(registry.0.swap_remove(place))
}

fn put(surface: Surface, cx: &mut App) {
    cx.default_global::<Registry>().0.push(surface);
}
