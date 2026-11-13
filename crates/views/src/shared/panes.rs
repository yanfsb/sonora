use gpui::{App, Bounds, FocusHandle, Global, Pixels, Window, px};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Id {
    Content,
    Left,
    Right,
}

struct Pane {
    id: Id,
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
}

#[derive(Default)]
struct Registry(Vec<Pane>);

impl Global for Registry {}

pub(crate) fn clear(cx: &mut App) {
    cx.default_global::<Registry>().0.clear();
}

pub(crate) fn register(id: Id, focus: FocusHandle, bounds: Bounds<Pixels>, cx: &mut App) {
    cx.default_global::<Registry>()
        .0
        .push(Pane { id, focus, bounds });
}

pub(crate) fn focused(id: Id, window: &Window, cx: &App) -> bool {
    let Some(registry) = cx.try_global::<Registry>() else {
        return false;
    };
    if let Some(pane) = registry.0.iter().find(|pane| pane.focus.is_focused(window)) {
        return pane.id == id;
    }
    registry
        .0
        .iter()
        .any(|pane| pane.id == id && pane.focus.contains_focused(window, cx))
}

pub(crate) fn focus(direction: Direction, window: &mut Window, cx: &mut App) -> bool {
    let Some(registry) = cx.try_global::<Registry>() else {
        return false;
    };
    let Some(current) = registry
        .0
        .iter()
        .find(|pane| pane.focus.is_focused(window))
        .or_else(|| {
            registry
                .0
                .iter()
                .find(|pane| pane.focus.contains_focused(window, cx))
        })
    else {
        return false;
    };
    let origin = center(current.bounds);
    let target = registry
        .0
        .iter()
        .filter(|pane| pane.id != current.id && in_direction(origin, pane.bounds, direction))
        .min_by(|left, right| {
            score(origin, left.bounds, direction).total_cmp(&score(origin, right.bounds, direction))
        })
        .map(|pane| pane.focus.clone());
    let Some(target) = target else {
        return false;
    };
    window.focus(&target, cx);
    true
}

fn center(bounds: Bounds<Pixels>) -> (f32, f32) {
    (
        bounds.origin.x / px(1.) + bounds.size.width / px(2.),
        bounds.origin.y / px(1.) + bounds.size.height / px(2.),
    )
}

fn in_direction(origin: (f32, f32), bounds: Bounds<Pixels>, direction: Direction) -> bool {
    let target = center(bounds);
    match direction {
        Direction::Left => target.0 < origin.0,
        Direction::Right => target.0 > origin.0,
        Direction::Up => target.1 < origin.1,
        Direction::Down => target.1 > origin.1,
    }
}

fn score(origin: (f32, f32), bounds: Bounds<Pixels>, direction: Direction) -> f32 {
    let target = center(bounds);
    let (major, minor) = match direction {
        Direction::Left | Direction::Right => {
            ((target.0 - origin.0).abs(), (target.1 - origin.1).abs())
        }
        Direction::Up | Direction::Down => {
            ((target.1 - origin.1).abs(), (target.0 - origin.0).abs())
        }
    };
    major + minor * 2.
}
