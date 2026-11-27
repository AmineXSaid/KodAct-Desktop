//! Ephemeral output modal for out-of-band slash commands (`/status`, `/usage`,
//! `/doctor`, `/diff`, `/release-notes`, `/context all`…). Inspection output
//! sits in a calm Operate card over the workspace and never enters the chat
//! transcript or model context.

use gpui::{
    div, prelude::*, px, relative, ClipboardItem, Entity, FontWeight, IntoElement, MouseButton,
    SharedString, Window,
};

use super::button::Button;
use super::icon;
use crate::state::{AppState, CommandModal};
use crate::theme::{self, Theme};

const COPY_KEY: &str = "command-modal";

/// Preserve intentional single-line breaks in CLI status/doctor reports while
/// keeping Markdown headings, lists, tables and fenced code blocks intact.
pub fn prepare_command_markdown(src: &str) -> String {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if (trimmed.starts_with("diff --git ") || trimmed.starts_with("--- a/"))
        && !trimmed.starts_with("```")
    {
        return format!("```diff\n{trimmed}\n```");
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    let mut out = String::with_capacity(trimmed.len() + lines.len() * 2);
    let mut in_fence = false;
    for (i, &line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            out.push_str(line);
        } else if in_fence {
            out.push_str(line);
        } else {
            out.push_str(line);
            let next_non_empty = lines.get(i + 1).is_some_and(|n| !n.trim().is_empty());
            let block_line = t.is_empty()
                || t.starts_with('#')
                || t.starts_with('|')
                || t.starts_with("---")
                || t.starts_with("> ");
            if next_non_empty && !block_line && !line.ends_with("  ") {
                out.push_str("  ");
            }
        }
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    modal: &CommandModal,
    window: &Window,
    leaving: bool,
    cx: &gpui::App,
) -> impl IntoElement {
    let s = state.read(cx);
    let copied = s.copied.as_deref() == Some(COPY_KEY);
    let can_copy = !modal.pending && !modal.output.trim().is_empty();
    let copy_ctx = super::markdown::CopyContext {
        state: state.clone(),
        key: "cmd-modal".to_string(),
        copied: s.copied.clone(),
        scale: window.scale_factor(),
        sources: s.diagram_code.clone(),
        zooms: s.diagram_zoom.clone(),
        streaming: modal.pending,
    };

    let (close_scrim, close_btn, copy_state) = (state.clone(), state.clone(), state.clone());
    let raw_output = modal.output.to_string();

    let copy_button = Button::ghost("command-modal-copy", if copied { "Copied" } else { "Copy" })
        .with_icon(if copied {
            "icons/check.svg"
        } else {
            "icons/copy.svg"
        })
        .compact()
        .disabled(!can_copy)
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(raw_output.clone()));
            copy_state.update(cx, |s, cx| s.mark_copied(COPY_KEY.into(), cx));
        })
        .into_any_element();

    let close_button =
        super::panel_chrome::close_button("command-modal", "Close (Esc)", move |_, _, cx| {
            close_btn.update(cx, |s, cx| {
                s.command_modal = None;
                cx.notify();
            });
        });

    let header = div()
        .flex_none()
        .h(t.size.panel_header)
        .flex()
        .items_center()
        .justify_between()
        .gap(t.space.sm)
        .pl(t.space.lg)
        .pr(t.space.sm)
        .bg(t.colors.surface)
        .border_b_1()
        .border_color(t.colors.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .min_w_0()
                .child(icon("icons/terminal.svg", t.size.icon, t.colors.text_muted))
                .child(
                    div()
                        .px(t.space.sm)
                        .py(px(2.))
                        .rounded(t.radius.small)
                        .bg(t.extra.inset)
                        .border_1()
                        .border_color(t.colors.border)
                        .text_size(t.text.body_sm)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(modal.command.clone()),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .child(copy_button)
                .child(close_button),
        );

    let body_content = if modal.pending && modal.output.trim().is_empty() {
        div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .py(t.space.md)
            .text_size(t.text.body_sm)
            .text_color(t.colors.text_muted)
            .child(super::turning_logo(14., "command-modal-working"))
            .child(SharedString::from(format!("Running {}…", modal.command)))
            .into_any_element()
    } else if modal.is_error {
        div()
            .flex()
            .items_start()
            .gap(t.space.sm)
            .p(t.space.md)
            .rounded(t.radius.card)
            .border_1()
            .border_color(t.extra.danger.opacity(0.35))
            .bg(t.extra.danger_tint)
            .child(div().pt(px(2.)).child(icon(
                "icons/triangle-alert.svg",
                t.size.icon,
                t.extra.danger,
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(t.text.body_sm)
                    .line_height(relative(1.5))
                    .text_color(t.colors.text)
                    .child(modal.output.clone()),
            )
            .into_any_element()
    } else {
        let md = prepare_command_markdown(&modal.output);
        div()
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .child(super::markdown::render(&md, t, &copy_ctx))
            .when(modal.pending, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.xs)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(super::turning_logo(12., "command-modal-streaming"))
                        .child("Receiving output…"),
                )
            })
            .into_any_element()
    };

    let card = theme::popover(t)
        .id("command-modal")
        .occlude()
        .p(px(0.))
        .w(px(640.))
        .max_w(relative(0.92))
        .overflow_hidden()
        .flex()
        .flex_col()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(header)
        .child(
            div()
                .id("command-modal-body")
                .max_h(px(460.))
                .overflow_y_scroll()
                .p(t.space.xl)
                .child(body_content),
        );

    super::motion::backdrop(
        div()
            .id("command-modal-scrim")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(t.extra.scrim)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                close_scrim.update(cx, |s, cx| {
                    s.command_modal = None;
                    cx.notify();
                });
            })
            .child(super::motion::dialog(card, "command-modal", leaving, t)),
        "command-modal",
        t.extra.scrim,
        leaving,
        t,
    )
}

#[cfg(test)]
mod tests {
    use super::prepare_command_markdown;

    #[test]
    fn preserves_status_line_breaks_and_fences_raw_diffs() {
        let status = "Version: 0.4.2\nModel: Qwen3 Coder\n\n| Item | State |\n| --- | --- |\n| MCP | Ready |";
        let prepared = prepare_command_markdown(status);
        assert!(prepared.contains("Version: 0.4.2  \nModel: Qwen3 Coder"));
        assert!(prepared.contains("| Item | State |\n| --- | --- |"));

        let raw_diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs";
        assert!(prepare_command_markdown(raw_diff).starts_with("```diff\n"));
    }
}
