//! The composer's autocomplete: type `/` at the start of a message for
//! commands (ForgeCLI's, the project's own, the app's, the modes), or `@`
//! anywhere for a file in the project. A popover above the box lists the
//! matches, best first, with the matched letters in bold; ↑/↓ move, Enter
//! or Tab completes, Esc closes it until the word changes.
//!
//! Choosing a ForgeCLI or project command writes it into the box (`/compact `),
//! ready to send; an app action or a mode runs at once and clears the word;
//! a file is written as `@path `, which ForgeCLI attaches when it reads the
//! message.

use gpui::{div, prelude::*, px, Action, AnyElement, FontWeight, SharedString};

use super::command_palette::{fuzzy, highlighted};
use super::*;
use crate::modes::ModeId;
use crate::state::AppState;
use crate::theme::Theme;

/// The word being completed: its sign (`/` or `@`), where it starts in the
/// text (a byte offset), and what follows the sign.
#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub sign: char,
    pub start: usize,
    pub query: String,
}

/// The word at the end of `text`, if it is one to complete: `/word` as the
/// whole message so far, or `@word` after a space or at the start.
pub fn trigger(text: &str) -> Option<Trigger> {
    let start = text.rfind(char::is_whitespace).map_or(0, |i| {
        i + text[i..].chars().next().map_or(1, char::len_utf8)
    });
    let word = &text[start..];
    let sign = word.chars().next()?;
    let query = word[sign.len_utf8()..].to_string();
    match sign {
        '/' if start == 0 && !query.contains('/') => Some(Trigger { sign, start, query }),
        '@' => Some(Trigger { sign, start, query }),
        _ => None,
    }
}

/// What choosing a row does.
pub enum Pick {
    /// Replace the word with this text.
    Text(String),
    /// Clear the word, then run an app action.
    Action(Box<dyn Action>),
    /// Clear the word, then switch to this mode.
    Mode(ModeId),
}

pub struct Item {
    pub label: SharedString,
    pub detail: SharedString,
    pub group: &'static str,
    pub icon: &'static str,
    pub pick: Pick,
    pub positions: Vec<usize>,
}

/// The most rows listed.
const MAX_ROWS: usize = 40;

fn app_actions() -> Vec<(&'static str, &'static str, &'static str, Box<dyn Action>)> {
    vec![
        (
            "new",
            "Start a new chat",
            "icons/plus.svg",
            Box::new(NewChat),
        ),
        (
            "attach",
            "Attach files and images",
            "icons/paperclip.svg",
            Box::new(AttachFiles),
        ),
        (
            "find",
            "Find in this chat",
            "icons/search.svg",
            Box::new(ToggleFind),
        ),
        (
            "export",
            "Export this chat as Markdown",
            "icons/archive.svg",
            Box::new(ExportChat),
        ),
        (
            "export-json",
            "Export this chat as JSON",
            "icons/archive.svg",
            Box::new(ExportChatJson),
        ),
        (
            "context-window",
            "Open the context window breakdown",
            "icons/cpu.svg",
            Box::new(ShowContext),
        ),
        (
            "tasks",
            "Show this turn's tasks",
            "icons/list-todo.svg",
            Box::new(ToggleTasks),
        ),
        (
            "settings",
            "Open settings",
            "icons/settings.svg",
            Box::new(OpenSettings),
        ),
        (
            "shortcuts",
            "Keyboard shortcuts",
            "icons/keyboard.svg",
            Box::new(ShowShortcuts),
        ),
        (
            "theme",
            "Switch light / dark",
            "icons/moon.svg",
            Box::new(ToggleTheme),
        ),
    ]
}

/// The rows for `trig`, best first.
pub fn items(trig: &Trigger, s: &AppState) -> Vec<Item> {
    let q = trig.query.as_str();
    let mut scored: Vec<(i32, usize, Item)> = vec![];
    let mut push = |order: usize,
                    label: String,
                    detail: String,
                    group: &'static str,
                    icon: &'static str,
                    pick: Pick| {
        if let Some((score, positions)) = fuzzy(q, &label) {
            scored.push((
                score,
                order,
                Item {
                    label: label.into(),
                    detail: detail.into(),
                    group,
                    icon,
                    pick,
                    positions,
                },
            ));
        }
    };
    if trig.sign == '/' {
        let mut order = 0;
        for c in s.completion_commands() {
            let label = format!("/{}", c.name);
            let detail = if c.args.is_empty() {
                c.description.clone()
            } else {
                format!("{} · {}", c.args, c.description)
            };
            let text = format!("/{} ", c.name);
            push(
                order,
                label,
                detail,
                c.group,
                "icons/terminal.svg",
                Pick::Text(text),
            );
            order += 1;
        }
        for (name, what, icon, action) in app_actions() {
            push(
                order,
                format!("/{name}"),
                what.to_string(),
                "Forge Desktop",
                icon,
                Pick::Action(action),
            );
            order += 1;
        }
        for m in ModeId::ALL
            .into_iter()
            .filter(|m| *m != ModeId::Bypass || s.bypass_allowed)
        {
            let name = m.label().to_lowercase().replace(' ', "-");
            push(
                order,
                format!("/{name}"),
                format!("Switch to {} mode", m.label()),
                "Modes",
                m.icon(),
                Pick::Mode(m),
            );
            order += 1;
        }
    } else {
        for (order, f) in s.completion_files().iter().enumerate() {
            push(
                order,
                f.clone(),
                String::new(),
                "Files",
                "icons/file-text.svg",
                Pick::Text(format!("@{f} ")),
            );
        }
    }
    if !q.is_empty() {
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    scored
        .into_iter()
        .take(MAX_ROWS)
        .map(|(_, _, i)| i)
        .collect()
}

/// The popover. `selected` is the row under the keyboard; a click on a row
/// calls `on_pick` with its index.
pub fn render(
    t: &Theme,
    rows: &[Item],
    selected: usize,
    loading: bool,
    below: bool,
    ranked: bool,
    on_pick: impl Fn(usize, &mut gpui::Window, &mut gpui::App) + Clone + 'static,
    on_hover: impl Fn(usize, &mut gpui::App) + Clone + 'static,
) -> AnyElement {
    let mut list = div()
        .id("autocomplete-list")
        .flex()
        .flex_col()
        .max_h(t.size.popover_list)
        .overflow_y_scroll()
        .p(t.space.xs);
    let mut last_group = "";
    for (i, item) in rows.iter().enumerate() {
        // Ranked by match, rows of one group no longer sit together: each
        // names its group instead of a header over them.
        if !ranked && item.group != last_group {
            last_group = item.group;
            list = list.child(
                div()
                    .px(t.space.md)
                    .pt(t.space.sm)
                    .pb(t.space.xs)
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_subtle)
                    .child(item.group),
            );
        }
        let on = i == selected;
        let (pick, hover) = (on_pick.clone(), on_hover.clone());
        list = list.child(
            div()
                .id(("ac", i))
                .flex()
                .items_center()
                .gap(t.space.md)
                .h(t.size.row)
                .px(t.space.md)
                .rounded(t.radius.button)
                .cursor_pointer()
                .when(on, |d| d.bg(t.colors.hover))
                .child(icon(
                    item.icon,
                    t.size.icon_sm,
                    if on {
                        t.colors.text
                    } else {
                        t.colors.text_muted
                    },
                ))
                .child(
                    div()
                        .flex_none()
                        .max_w(px(260.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(if item.group == "Files" {
                            t.font_mono.clone()
                        } else {
                            t.font_sans.clone()
                        })
                        .child(highlighted(&item.label, &item.positions, t)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(item.detail.clone()),
                )
                .when(ranked && item.group != "Files", |d| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(item.group),
                    )
                })
                .on_mouse_move(move |_, _, cx| hover(i, cx))
                .on_click(move |_, window, cx| pick(i, window, cx)),
        );
    }
    if rows.is_empty() {
        list = list.child(
            div()
                .px(t.space.md)
                .py(t.space.md)
                .text_color(t.colors.text_subtle)
                .child(if loading {
                    "Looking through the project…"
                } else {
                    "No matches"
                }),
        );
    }
    div()
        .id("autocomplete")
        .absolute()
        // Above the box in a chat; below it on the new-chat screen, where
        // the box sits mid-window with little room above.
        .map(|d| {
            if below {
                d.top_full().mt(px(8.))
            } else {
                d.bottom_full().mb(px(8.))
            }
        })
        .left_0()
        .right_0()
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_popover())
        .child(list)
        .child(
            div()
                .flex()
                .gap(t.space.lg)
                .px(t.space.lg)
                .py(t.space.xs)
                .border_t_1()
                .border_color(t.colors.border)
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child("↑↓ choose")
                .child("Enter or Tab complete")
                .child("Esc close"),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::trigger;

    #[test]
    fn the_word_at_the_end_is_completed() {
        assert_eq!(
            trigger("/comp").map(|t| (t.sign, t.start, t.query)),
            Some(('/', 0, "comp".into()))
        );
        assert_eq!(trigger("/").map(|t| t.query), Some(String::new()));
        // A command only leads a message.
        assert_eq!(trigger("run /comp"), None);
        assert_eq!(
            trigger("look at @src/ma").map(|t| (t.sign, t.start, t.query)),
            Some(('@', 8, "src/ma".into()))
        );
        assert_eq!(trigger("@").map(|t| t.start), Some(0));
        // Finished words aren't.
        assert_eq!(trigger("/compact "), None);
        assert_eq!(trigger("mail me@x.y"), None);
        assert_eq!(trigger("plain"), None);
        assert_eq!(
            trigger("é @ü").map(|t| (t.start, t.query)),
            Some((3, "ü".into()))
        );
    }
}
