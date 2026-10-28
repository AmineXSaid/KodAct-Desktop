//! The status bar at the foot of the window, as in VS Code: the remote
//! indicator leftmost (`remote_menu.rs`; its quick pick opens above it),
//! then the project's branch, and the terminal's button on the right (the
//! model is in the composer).

use gpui::{div, prelude::*, px, AnyElement, Entity};

use super::icon;
use crate::state::AppState;
use crate::theme::Theme;

pub fn render(
    s: &AppState,
    state: Entity<AppState>,
    picker: Entity<super::remote_menu::RemotePicker>,
    t: &Theme,
) -> AnyElement {
    let (hover, open) = (t.colors.hover, s.terminal_open);
    div()
        .relative()
        .flex_none()
        .h(t.size.statusbar)
        .flex()
        .items_center()
        .gap(t.space.md)
        .bg(t.extra.chrome)
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(super::remote_menu::indicator(s, state, t))
        .when(!s.branch.is_empty(), |d| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .child(icon(
                        "icons/git-branch.svg",
                        t.size.icon_xs,
                        t.colors.text_muted,
                    ))
                    .child(s.branch.clone()),
            )
        })
        .child(div().flex_1())
        .child(
            div()
                .id("status-terminal")
                .mr(t.space.sm)
                .size(t.size.statusbar - px(4.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.small)
                .cursor_pointer()
                .when(open, |d| d.bg(hover))
                .hover(move |d| d.bg(hover))
                .child(icon(
                    "icons/terminal.svg",
                    t.size.icon_sm,
                    if open {
                        t.colors.text
                    } else {
                        t.colors.text_muted
                    },
                ))
                .tooltip(move |w, cx| {
                    gpui::component::tooltip::Tooltip::new(if open {
                        "Hide the terminal (Ctrl+`)"
                    } else {
                        "Show the terminal (Ctrl+`)"
                    })
                    .build(w, cx)
                })
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::ToggleTerminal), cx)
                }),
        )
        // Always there: the picker draws itself while open and as it leaves.
        .child(gpui::deferred(picker).with_priority(3))
        .into_any_element()
}

/// Under the title bar while the current remote doesn't answer: that it's
/// reconnecting, or why it gave up, with Retry and Close remote connection.
pub fn banner(s: &AppState, state: Entity<AppState>, t: &Theme) -> Option<AnyElement> {
    let r = s.current_remote()?;
    let failed = s
        .remote_failed
        .as_ref()
        .filter(|(target, _)| *target == r.target)
        .map(|(_, why)| why.clone());
    let lost = s
        .remote_lost
        .as_ref()
        .is_some_and(|(target, _)| *target == r.target);
    if failed.is_none() && !lost {
        return None;
    }
    let (bg, fg, text): (gpui::Hsla, gpui::Hsla, gpui::SharedString) = match &failed {
        Some(why) => (
            t.extra.danger_tint,
            t.extra.danger,
            format!("Couldn't reach {}. {why}", r.target.label()).into(),
        ),
        None => (
            t.extra.warning_tint,
            t.extra.warning,
            "Connection lost. Reconnecting…".into(),
        ),
    };
    let (retry, close) = (state.clone(), state);
    Some(
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(t.space.md)
            .px(t.space.lg)
            .py(t.space.xs)
            .bg(bg)
            .border_b_1()
            .border_color(t.colors.border)
            .text_size(t.text.caption)
            .child(icon("icons/triangle-alert.svg", t.size.icon_xs, fg))
            .child(div().flex_1().min_w_0().text_color(fg).child(text))
            .child(
                crate::theme::ghost_button(t, "Retry")
                    .id("remote-retry")
                    .on_click(move |_, _, cx| retry.update(cx, |s, cx| s.retry_remote(cx))),
            )
            .child(
                crate::theme::text_button(t, "Close remote connection")
                    .id("remote-banner-close")
                    .on_click(move |_, _, cx| close.update(cx, |s, cx| s.close_remote(cx))),
            )
            .into_any_element(),
    )
}
