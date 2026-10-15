//! "Needs you": every chat waiting on a permission answer, in one place.
//! While any is waiting, the title bar shows a badge with the count; it
//! opens the list (the chat, what it asks), and Reveal opens that chat at
//! its question.

use gpui::{div, prelude::*, px, AnimationExt, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::state::AppState;
use crate::theme::{self, Theme};

/// The title bar's badge and its list; None while nothing waits.
pub fn render(t: &Theme, state: &Entity<AppState>, cx: &gpui::App) -> Option<AnyElement> {
    let s = state.read(cx);
    let pending = s.pending_approvals();
    if pending.is_empty() {
        return None;
    }
    let open = s.approvals_open;
    let n = pending.len();
    let toggle = state.clone();
    let tint = t.extra.warning_tint;
    let badge = div()
        .id("approvals")
        // Occludes the title bar's drag area (see workspace.rs).
        .occlude()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .h(t.size.control)
        .px(t.space.sm)
        .rounded(t.radius.pill)
        .bg(t.extra.warning_tint)
        .text_color(t.extra.warning)
        .text_size(t.text.caption)
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |d| d.bg(tint.opacity(0.9)))
        .child(icon("icons/hand.svg", t.size.icon_sm, t.extra.warning))
        .child(format!("{n} {} you", if n == 1 { "needs" } else { "need" }))
        .tooltip(|window, cx| {
            gpui::component::tooltip::Tooltip::new("Chats waiting on your answer").build(window, cx)
        })
        .on_click(move |_, _, cx| toggle.update(cx, |s, cx| s.toggle_approvals(cx)))
        .with_animation(
            "approvals-in",
            gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms)),
            |d, delta| d.opacity(delta),
        );
    let hover = t.colors.hover;
    let list = open.then(|| {
        let mut rows = div().flex().flex_col();
        for (i, (chat, title, asks, msg)) in pending.iter().cloned().enumerate() {
            let st = state.clone();
            rows = rows.child(
                div()
                    .id(("approval", i))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .hover(move |d| d.bg(hover))
                    .child(
                        div()
                            .size(px(7.))
                            .flex_none()
                            .rounded(t.radius.pill)
                            .bg(t.colors.marigold),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.colors.text_strong)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .child(asks),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .text_size(t.text.caption)
                            .text_color(t.colors.primary)
                            .child(icon("icons/eye.svg", t.size.icon_xs, t.colors.primary))
                            .child("Reveal"),
                    )
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| s.reveal_approval(chat, msg, cx))
                    }),
            );
        }
        let close = state.clone();
        gpui::deferred(
            theme::popover(t)
                .id("approvals-list")
                .occlude()
                .absolute()
                .top(t.size.control + t.space.xs)
                .right_0()
                .w(px(340.))
                .on_mouse_down_out(move |_, _, cx| {
                    close.update(cx, |s, cx| {
                        if s.approvals_open {
                            s.approvals_open = false;
                            cx.notify();
                        }
                    })
                })
                .child(
                    div()
                        .px(t.space.md)
                        .pt(t.space.xs)
                        .pb(t.space.xs)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(SharedString::from("Waiting on your answer")),
                )
                .child(rows),
        )
        .with_priority(2)
    });
    Some(
        div()
            .relative()
            .child(badge)
            .children(list)
            .into_any_element(),
    )
}
