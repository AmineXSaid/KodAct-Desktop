//! The keyboard shortcuts sheet (F1, or "Keyboard shortcuts" in the
//! palette): every binding `main.rs` makes, grouped, with its keys drawn
//! for this platform. A test reads `main.rs` and fails when a binding is
//! missing here, so the sheet can't drift from the keymap.

use gpui::{div, prelude::*, px, AnimationExt, Entity, FontWeight, IntoElement, SharedString};

use super::icon;
use crate::state::AppState;
use crate::theme::{self, Theme};

/// (keystroke as bound in `main.rs`, what it does, its group).
pub const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("secondary-k", "Search or run a command", "General"),
    ("f1", "Keyboard shortcuts", "General"),
    ("secondary-,", "Settings", "General"),
    ("secondary-shift-l", "Light or dark theme", "General"),
    ("secondary-b", "Show or hide the sidebar", "General"),
    ("secondary-q", "Quit", "General"),
    ("secondary-n", "New chat", "Chats"),
    ("f2", "Rename this chat", "Chats"),
    ("secondary-shift-p", "Projects", "Chats"),
    ("secondary-shift-r", "Rules and memory", "Chats"),
    ("secondary-l", "Focus the composer", "Conversation"),
    ("secondary-u", "Attach files and images", "Conversation"),
    ("secondary-shift-t", "This turn's tasks", "Conversation"),
    ("secondary-shift-a", "Chats that need you", "Chats"),
    ("secondary-shift-m", "The project map", "Chats"),
    (
        "secondary-alt-o",
        "Remote: WSL, SSH host or container",
        "Chats",
    ),
    (
        "secondary-shift-e",
        "Side panel: plan, files, browser",
        "Conversation",
    ),
    (
        "secondary-\\",
        "Side panel beside or below the chat",
        "Conversation",
    ),
    ("secondary-f", "Find in the conversation", "Conversation"),
    ("secondary-j", "Jump to the latest message", "Conversation"),
    ("secondary-shift-c", "Copy the last reply", "Conversation"),
    ("secondary-/", "Choose a model", "Conversation"),
    ("shift-tab", "Next permission mode", "Conversation"),
    (
        "escape",
        "Stop the reply, or close what's open",
        "Conversation",
    ),
];

pub const GROUPS: [&str; 3] = ["General", "Chats", "Conversation"];

/// The keys of a binding as this platform writes them: `Ctrl Shift L`, `⌘⇧L`.
pub fn keys(binding: &str) -> Vec<String> {
    let mac = cfg!(target_os = "macos");
    binding
        .split('-')
        .filter(|p| !p.is_empty())
        .map(|p| match p {
            "secondary" => if mac { "⌘" } else { "Ctrl" }.to_string(),
            "shift" => if mac { "⇧" } else { "Shift" }.to_string(),
            "alt" => if mac { "⌥" } else { "Alt" }.to_string(),
            "escape" => "Esc".to_string(),
            "tab" => "Tab".to_string(),
            "enter" => "Enter".to_string(),
            k if k.len() == 1 => k.to_uppercase(),
            k => k.to_uppercase(),
        })
        .collect()
}

pub fn keycap(t: &Theme, text: String) -> impl IntoElement {
    div()
        .min_w(px(20.))
        .px(t.space.xs + px(2.))
        .py(px(1.))
        .flex()
        .justify_center()
        .rounded(t.radius.small)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.extra.inset)
        .font_family(t.font_mono.clone())
        .text_size(t.text.caption)
        .text_color(t.colors.text)
        .child(text)
}

/// The sheet, over a scrim; a click outside or Esc closes it.
pub fn render(t: &Theme, state: &Entity<AppState>) -> impl IntoElement {
    let mut groups = div().flex().flex_col().gap(t.space.lg);
    for g in GROUPS {
        let mut rows = div().flex().flex_col();
        for (binding, label, _) in SHORTCUTS.iter().filter(|(_, _, group)| *group == g) {
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.md)
                    .h(t.size.row)
                    .border_b_1()
                    .border_color(t.colors.border)
                    .child(div().flex_1().text_color(t.colors.text).child(*label))
                    .child(
                        div()
                            .flex()
                            .gap(t.space.xs)
                            .children(keys(binding).into_iter().map(|k| keycap(t, k))),
                    ),
            );
        }
        groups = groups.child(
            div()
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .child(
                    div()
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_subtle)
                        .child(g),
                )
                .child(rows),
        );
    }
    let (close, close_btn) = (state.clone(), state.clone());
    let hover = t.colors.hover;
    let title: SharedString = "Keyboard shortcuts".into();
    div()
        .id("shortcuts-scrim")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(t.extra.scrim)
        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
            close.update(cx, |s, cx| {
                s.shortcuts_open = false;
                cx.notify();
            })
        })
        .child(
            theme::popover(t)
                .id("shortcuts")
                .occlude()
                .w(px(460.))
                .max_h(gpui::relative(0.85))
                .p(t.space.xl)
                .flex()
                .flex_col()
                .gap(t.space.lg)
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(icon("icons/keyboard.svg", t.size.icon, t.colors.primary))
                        .child(
                            div()
                                .flex_1()
                                .text_size(t.text.subheading)
                                .font_family(t.font_display.clone())
                                .text_color(t.colors.text_strong)
                                .child(title),
                        )
                        .child(
                            div()
                                .id("shortcuts-close")
                                .size(t.size.control)
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(t.radius.pill)
                                .cursor_pointer()
                                .hover(move |d| d.bg(hover))
                                .child(icon("icons/x.svg", t.size.icon_sm, t.colors.text_muted))
                                .on_click(move |_, _, cx| {
                                    close_btn.update(cx, |s, cx| {
                                        s.shortcuts_open = false;
                                        cx.notify();
                                    })
                                }),
                        ),
                )
                .child(
                    div()
                        .id("shortcuts-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(groups),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child("Every command is also in the palette, with or without a shortcut."),
                )
                .with_animation(
                    "shortcuts-in",
                    gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms))
                        .with_easing(gpui::ease_in_out),
                    |d, delta| d.opacity(delta),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `KeyBinding::new("…"` in main.rs is on the sheet, and the sheet
    /// shows nothing main.rs doesn't bind.
    #[test]
    fn the_sheet_matches_the_keymap() {
        let main = include_str!("../main.rs");
        // As written in the source: `\\` there is one backslash.
        let bound: Vec<String> = main
            .split("KeyBinding::new(\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .map(|k| k.replace("\\\\", "\\"))
            .collect();
        assert!(bound.len() > 10, "{bound:?}");
        for b in &bound {
            assert!(
                SHORTCUTS.iter().any(|(k, _, _)| k == b),
                "{b} is bound in main.rs but missing from the shortcuts sheet"
            );
        }
        for (k, _, _) in SHORTCUTS {
            assert!(
                bound.iter().any(|b| b == k),
                "{k} is on the sheet but main.rs doesn't bind it"
            );
        }
    }

    /// The README's key table lists every binding on the sheet.
    #[test]
    fn the_readme_lists_every_key() {
        let readme = include_str!("../../README.md");
        for (_, what, _) in SHORTCUTS {
            assert!(
                readme.contains(&format!("| {what} |")),
                "README.md's key table is missing \"{what}\""
            );
        }
    }

    #[test]
    fn keys_read_naturally() {
        if !cfg!(target_os = "macos") {
            assert_eq!(keys("secondary-shift-l"), vec!["Ctrl", "Shift", "L"]);
            assert_eq!(keys("escape"), vec!["Esc"]);
            assert_eq!(keys("f1"), vec!["F1"]);
            assert_eq!(keys("secondary-,"), vec!["Ctrl", ","]);
        }
    }
}
