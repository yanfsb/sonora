use gpui::{App, Entity, Global, Pixels, ScrollHandle, Window, point};
use ui::{Glide, Scrollbar};

#[derive(Clone)]
struct Horizontal {
    scroll: ScrollHandle,
    glide: Glide,
}

#[derive(Default)]
struct Active {
    page: Option<&'static str>,
    vertical: Option<Entity<Scrollbar>>,
    horizontal: Option<Horizontal>,
}

impl Global for Active {}

pub(crate) fn clear(cx: &mut App) {
    if cx.has_global::<Active>() {
        let active = cx.global_mut::<Active>();
        active.page = None;
        active.vertical = None;
        active.horizontal = None;
    }
}

pub(crate) fn page(id: &'static str, bar: &Entity<Scrollbar>, cx: &mut App) {
    let active = cx.default_global::<Active>();
    if active.page != Some(id) {
        active.horizontal = None;
    }
    active.page = Some(id);
    active.vertical = Some(bar.clone());
}

pub(crate) fn set_horizontal(scroll: &ScrollHandle, glide: &Glide, cx: &mut App) {
    cx.default_global::<Active>().horizontal = Some(Horizontal {
        scroll: scroll.clone(),
        glide: glide.clone(),
    });
}

pub(crate) fn clear_horizontal(cx: &mut App) {
    if cx.has_global::<Active>() {
        cx.global_mut::<Active>().horizontal = None;
    }
}

pub(crate) fn promote_horizontal(scroll: &ScrollHandle, glide: &Glide, cx: &mut App) {
    set_horizontal(scroll, glide, cx);
}

pub(crate) fn touch_horizontal(scroll: &ScrollHandle, glide: &Glide, cx: &mut App) {
    glide.stop_spring(scroll);
    promote_horizontal(scroll, glide, cx);
}

pub(crate) fn vertical(delta: Pixels, window: &mut Window, cx: &mut App) -> bool {
    let Some(bar) = cx
        .try_global::<Active>()
        .and_then(|active| active.vertical.clone())
    else {
        return false;
    };
    bar.update(cx, |bar, cx| {
        bar.move_by(delta, window);
        cx.notify();
    });
    true
}

pub(crate) fn half(down: bool, window: &mut Window, cx: &mut App) -> bool {
    let Some(bar) = cx
        .try_global::<Active>()
        .and_then(|active| active.vertical.clone())
    else {
        return false;
    };
    let amount = bar.read(cx).scroll().bounds().size.height / 2.;
    vertical(
        match down {
            true => amount,
            false => -amount,
        },
        window,
        cx,
    )
}

pub(crate) fn top(window: &mut Window, cx: &mut App) -> bool {
    let Some(bar) = cx
        .try_global::<Active>()
        .and_then(|active| active.vertical.clone())
    else {
        return false;
    };
    bar.update(cx, |bar, cx| {
        bar.top();
        bar.refresh_view(cx);
        window.refresh();
        cx.notify();
    });
    true
}

pub(crate) fn bottom(window: &mut Window, cx: &mut App) -> bool {
    let Some(bar) = cx
        .try_global::<Active>()
        .and_then(|active| active.vertical.clone())
    else {
        return false;
    };
    bar.update(cx, |bar, cx| {
        bar.bottom();
        bar.refresh_view(cx);
        window.refresh();
        cx.notify();
    });
    true
}

pub(crate) fn horizontal_move(right: bool, window: &mut Window, cx: &mut App) -> bool {
    let Some(horizontal) = cx
        .try_global::<Active>()
        .and_then(|active| active.horizontal.clone())
    else {
        return false;
    };
    let width = horizontal.scroll.bounds().size.width;
    let reach = horizontal.scroll.max_offset().x;
    if width <= Pixels::ZERO || reach <= Pixels::ZERO {
        return false;
    }
    let at = horizontal.glide.goal(&horizontal.scroll);
    let next = match right {
        true => at.x - width,
        false => at.x + width,
    };
    horizontal
        .glide
        .aim(&horizontal.scroll, point(next, at.y), window);
    true
}
