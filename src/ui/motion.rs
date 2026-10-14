//! Motion settings shared by the animated pieces.

/// Motion turned down: `FORGE_REDUCED_MOTION=1` shows still frames instead.
pub fn reduced_motion() -> bool {
    std::env::var("FORGE_REDUCED_MOTION").is_ok_and(|v| v == "1")
}

fn once(ms: u64) -> gpui::Animation {
    let ms = if reduced_motion() { 1 } else { ms };
    gpui::Animation::new(std::time::Duration::from_millis(ms)).with_easing(gpui::ease_out_quint())
}

/// A menu or dialog rising into place as it opens: it fades in from a few
/// pixels lower. Plays each time the element appears.
pub fn pop_in(t: &crate::theme::Theme) -> gpui::Animation {
    once(t.size.pop_in_ms)
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
