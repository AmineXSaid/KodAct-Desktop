//! Placeholders while something loads: quiet shapes where the content will
//! be, with a band of light sweeping across them. Each screen draws its own
//! layout from `bar` so the shapes match what replaces them.

use std::time::Duration;

use gpui::{div, linear_color_stop, linear_gradient, prelude::*, px, relative, Animation, AnimationExt, AnyElement, DefiniteLength, Pixels, SharedString};

use crate::theme::Theme;

/// One placeholder shape, `w` wide (a length or a fraction of its row) and
/// `h` high, with the sweep running across it.
pub fn bar(t: &Theme, id: impl Into<SharedString>, w: impl Into<DefiniteLength>, h: Pixels) -> AnyElement {
    let w: DefiniteLength = w.into();
    let base = t.colors.hover;
    let glow = t.colors.surface.opacity(if t.is_dark { 0.08 } else { 0.9 });
    div()
        .relative()
        .overflow_hidden()
        .w(w)
        .h(h)
        .rounded(t.radius.small)
        .bg(base)
        .child(
            // The band: two half-gradients, clear to bright to clear.
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(relative(0.6))
                .flex()
                .child(div().h_full().w(relative(0.5)).bg(linear_gradient(90., linear_color_stop(glow.opacity(0.), 0.), linear_color_stop(glow, 1.))))
                .child(div().h_full().w(relative(0.5)).bg(linear_gradient(90., linear_color_stop(glow, 0.), linear_color_stop(glow.opacity(0.), 1.))))
                .with_animation(id.into(), Animation::new(Duration::from_millis(1500)).repeat().with_easing(gpui::ease_in_out), |d, delta| d.left(relative(-0.6 + delta * 1.6))),
        )
        .into_any_element()
}

/// The sidebar's chat list, loading: a few date groups of rows.
pub fn chat_list(t: &Theme) -> AnyElement {
    let widths = [0.72, 0.55, 0.8, 0.62, 0.7, 0.5, 0.66];
    let mut col = div().flex().flex_col().px(t.space.md).pt(t.space.lg).gap(t.space.md);
    for (i, w) in widths.iter().enumerate() {
        if i == 0 || i == 3 || i == 5 {
            col = col.child(div().pt(if i == 0 { px(0.) } else { t.space.sm }).child(bar(t, format!("sk-group-{i}"), px(56.), px(8.))));
        }
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(bar(t, format!("sk-dot-{i}"), px(8.), px(8.)))
                .child(div().flex_1().child(bar(t, format!("sk-row-{i}"), relative(*w), px(11.))))
                .child(bar(t, format!("sk-ago-{i}"), px(18.), px(8.))),
        );
    }
    col.into_any_element()
}

/// A conversation, loading: your message, a reply's lines, a tool row.
pub fn transcript(t: &Theme) -> AnyElement {
    div()
        .w_full()
        .flex()
        .justify_center()
        .px(t.space.xl)
        .pt(t.space.xl)
        .child(
            div()
                .w_full()
                .max_w(t.size.column)
                .flex()
                .flex_col()
                .gap(t.space.md)
                .child(div().flex().justify_end().child(bar(t, "sk-you", px(280.), px(36.))))
                .child(div().pt(t.space.md).child(bar(t, "sk-head", px(64.), px(12.))))
                .child(bar(t, "sk-l1", relative(0.92), px(11.)))
                .child(bar(t, "sk-l2", relative(0.78), px(11.)))
                .child(bar(t, "sk-tool1", relative(1.), px(36.)))
                .child(bar(t, "sk-tool2", relative(1.), px(36.)))
                .child(bar(t, "sk-l3", relative(0.86), px(11.)))
                .child(bar(t, "sk-l4", relative(0.6), px(11.))),
        )
        .into_any_element()
}

/// A list of files, loading (the Rules screen).
pub fn file_list(t: &Theme) -> AnyElement {
    let mut col = div().flex().flex_col().gap(t.space.md).pt(t.space.lg);
    for i in 0..6 {
        col = col.child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(bar(t, format!("sk-file-icon-{i}"), px(14.), px(14.)))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(bar(t, format!("sk-file-{i}"), relative(0.5 + (i % 3) as f32 * 0.12), px(11.)))
                        .child(bar(t, format!("sk-file-sub-{i}"), relative(0.7), px(8.))),
                ),
        );
    }
    col.into_any_element()
}
