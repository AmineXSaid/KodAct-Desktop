//! Motion settings shared by the animated pieces.

/// Motion turned down: `FORGE_REDUCED_MOTION=1` shows still frames instead.
pub fn reduced_motion() -> bool {
    std::env::var("FORGE_REDUCED_MOTION").is_ok_and(|v| v == "1")
}

fn once(ms: u64) -> gpui::Animation {
    let ms = if reduced_motion() { 1 } else { ms };
    gpui::Animation::new(std::time::Duration::from_millis(ms)).with_easing(gpui::ease_out_quint())
}

/// The side panel sliding in from the right edge as it opens: its width grows
/// from nothing to `width` while the chat beside it eases narrower, and the
/// panel fades in. Below the chat (a narrow window) it is left as it is.
pub fn slide_in(
    pane: gpui::AnyElement,
    width: f32,
    below: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _};
    let anim = once(t.size.panel_in_ms);
    if below {
        // Below the chat the pane's height is a share of its parent's, and
        // it fades in on its own.
        return pane;
    }
    // The panel keeps its full width inside, pinned to the wrapper's left
    // edge, so it moves in with that edge rather than being uncovered.
    div()
        .relative()
        .flex_none()
        .h_full()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(width))
                .flex()
                .child(pane),
        )
        .with_animation("side-panel-in", anim, move |d, k| {
            d.w(px(width * k)).opacity((k * 1.6).min(1.))
        })
        .into_any_element()
}

/// A list opening row by row: row `index` starts a beat after the one above
/// it and rises into place over `pop_in_ms`. Rows past the eighth share its
/// beat, so a long list never keeps you waiting.
pub fn stagger(t: &crate::theme::Theme, index: usize) -> gpui::Animation {
    if reduced_motion() {
        return once(1);
    }
    let rise = t.size.pop_in_ms as f32;
    let delay = (t.size.stagger_ms * index.min(8) as u64) as f32;
    let total = rise + delay;
    let ease = gpui::ease_out_quint();
    gpui::Animation::new(std::time::Duration::from_millis(total as u64))
        .with_easing(move |k| ease(((k * total - delay) / rise).clamp(0., 1.)))
}

/// A staggered row: fades in from a few pixels lower, a beat after the row
/// above it. `id` names the opening, so a new page of rows plays it again.
pub fn row_in(
    row: impl gpui::IntoElement + 'static,
    id: impl Into<gpui::ElementId>,
    index: usize,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _};
    div()
        .flex_none()
        .child(row)
        .with_animation(id, stagger(t, index), |d, k| {
            d.opacity(k).mt(px(4. * (1. - k))).mb(px(-4. * (1. - k)))
        })
        .into_any_element()
}

/// `row_in` for a row that fills its parent's width (a card in a grid).
pub fn row_in_full(
    row: impl gpui::IntoElement + 'static,
    id: impl Into<gpui::ElementId>,
    index: usize,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _};
    div()
        .w_full()
        .child(row)
        .with_animation(id, stagger(t, index), |d, k| {
            d.opacity(k).mt(px(4. * (1. - k))).mb(px(-4. * (1. - k)))
        })
        .into_any_element()
}

/// A window drawing itself as it opens: a jewel bar runs down its left edge
/// while the window rises a few pixels into place and fades in, then the bar
/// fades away, leaving the window as it rests. The caller positions `body`
/// and makes it `relative`; `lift` is its bottom margin at rest, for one
/// that opens upward from what anchors it.
pub fn edge_draw(
    body: gpui::Stateful<gpui::Div>,
    id: impl Into<gpui::ElementId>,
    accent: gpui::Hsla,
    lift: f32,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, relative, AnimationExt as _};
    let inset = f32::from(t.radius.card);
    let line = t.radius.line;
    let bar = f32::from(t.size.edge_bar);
    let draw_ms = t.size.edge_draw_ms as f32;
    let ms = t.size.pop_in_ms + t.size.edge_draw_ms;
    // Linear time: the bar and the fade keep their own pace inside it.
    let anim = if reduced_motion() {
        gpui::Animation::new(std::time::Duration::from_millis(1))
    } else {
        gpui::Animation::new(std::time::Duration::from_millis(ms))
    };
    let split = draw_ms / ms as f32;
    let ease = gpui::ease_out_quint();
    body.with_animation(id, anim, move |d, k| {
        let draw = ease((k / split).min(1.));
        let rise = ease((k / (1. - split)).min(1.));
        let fade = ((k - split) / (1. - split)).clamp(0., 1.);
        d.opacity(rise)
            .mb(px(lift - 3. * (1. - rise)))
            .when(k < 1., |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(inset))
                        .bottom(px(inset))
                        .w(px(bar))
                        .child(
                            div()
                                .w_full()
                                .h(relative(draw))
                                .rounded(line)
                                .bg(accent.opacity(1. - fade)),
                        ),
                )
            })
    })
    .into_any_element()
}

/// A window or a page arriving: it fades in while it rises `rise` pixels into
/// place. Keyed by `id`, so a new page (another Settings section) plays it
/// again while the old one is simply gone: the content swaps, the frame stays.
pub fn rise_in<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: impl Into<gpui::ElementId>,
    rise: f32,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _};
    el.with_animation(id, once(t.size.pop_in_ms), move |d, k| {
        d.opacity(k).mt(px(rise * (1. - k)))
    })
    .into_any_element()
}

/// The reverse of `rise_in`: a window going away fades out while it sinks
/// `sink` pixels, over the same beat it took to arrive.
pub fn sink_out<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: impl Into<gpui::ElementId>,
    sink: f32,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _};
    let ms = if reduced_motion() {
        1
    } else {
        t.size.pop_in_ms
    };
    let anim =
        gpui::Animation::new(std::time::Duration::from_millis(ms)).with_easing(gpui::ease_in_out);
    el.with_animation(id, anim, move |d, k| d.opacity(1. - k).mt(px(sink * k)))
        .into_any_element()
}

/// How long `sink_out` takes, for whoever removes the window after it.
pub fn out_ms(t: &crate::theme::Theme) -> u64 {
    if reduced_motion() {
        1
    } else {
        t.size.pop_in_ms
    }
}

/// A backdrop dimming in (`out` false) or clearing (`out` true) under a window.
pub fn scrim<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: impl Into<gpui::ElementId>,
    color: gpui::Hsla,
    out: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, AnimationExt as _};
    el.with_animation(id, once(t.size.pop_in_ms), move |d, k| {
        let k = if out { 1. - k } else { k };
        d.bg(gpui::Hsla {
            a: color.a * k,
            ..color
        })
    })
    .into_any_element()
}
