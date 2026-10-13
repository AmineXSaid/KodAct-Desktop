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
    /// A folder (its path ends in `/`): Enter adds it, Tab opens it in the list.
    Folder(String),
}

pub struct Item {
    pub label: SharedString,
    pub detail: SharedString,
    pub group: &'static str,
    pub icon: &'static str,
    pub pick: Pick,
    pub positions: Vec<usize>,
    /// A row of the @ list: a file or a folder of the project.
    pub file: bool,
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
                    file: false,
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
        drop(push);
        return file_items(q, &s.completion_files());
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

/// `@path` as ForgeCLI reads it: quoted when it has a space.
pub fn mention(path: &str) -> String {
    if path.contains(char::is_whitespace) {
        format!("@\"{path}\"")
    } else {
        format!("@{path}")
    }
}

/// An entry's name and its folder: `src/ui/a.rs` → (`a.rs`, `src/ui`).
fn split(entry: &str) -> (&str, &str) {
    let trimmed = entry.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(i) => (&trimmed[i + 1..], &trimmed[..i]),
        None => (trimmed, ""),
    }
}

/// The @ list, as the extension shows it: files and folders, the project's
/// top level when nothing is typed, a folder's contents once its path ends
/// in `/`, and otherwise the best matches by name (then by path).
pub fn file_items(q: &str, entries: &[String]) -> Vec<Item> {
    let item = |entry: &String, positions: Vec<usize>| {
        let folder = entry.ends_with('/');
        let (name, dir) = split(entry);
        Item {
            label: name.to_string().into(),
            detail: dir.to_string().into(),
            group: if folder { "Folders" } else { "Files" },
            icon: "",
            pick: if folder {
                Pick::Folder(entry.clone())
            } else {
                Pick::Text(format!("{} ", mention(entry)))
            },
            positions,
            file: true,
        }
    };
    let (base, rest) = match q.rfind('/') {
        Some(i) => (&q[..=i], &q[i + 1..]),
        None => ("", q),
    };
    let known_base = base.is_empty() || entries.iter().any(|e| e == base);
    let mut scored: Vec<(i32, usize, Item)> = vec![];
    if known_base && (base.len() > 0 || rest.is_empty()) {
        // One folder's contents (the top level when nothing is typed).
        for (order, e) in entries.iter().enumerate() {
            let parent = e.trim_end_matches('/').rfind('/').map_or("", |i| &e[..=i]);
            if parent != base {
                continue;
            }
            if let Some((score, pos)) = fuzzy(rest, split(e).0) {
                scored.push((score, order, item(e, pos)));
            }
        }
    } else {
        for (order, e) in entries.iter().enumerate() {
            let (name, _) = split(e);
            // A match in the name beats one spread over the path.
            if let Some((score, pos)) = fuzzy(rest, name).filter(|_| base.is_empty()) {
                // A name that starts with what you typed comes first (scores are ×10).
                let prefix = if name.to_lowercase().starts_with(&rest.to_lowercase()) {
                    1_000
                } else {
                    0
                };
                scored.push((score * 2 + prefix, order, item(e, pos)));
            } else if let Some((score, _)) = fuzzy(q, e) {
                scored.push((score, order, item(e, vec![])));
            }
        }
    }
    if !rest.is_empty() {
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    scored
        .into_iter()
        .take(MAX_ROWS)
        .map(|(_, _, i)| i)
        .collect()
}

/// A folder path that fits: its tail, which says the most (`…/ui/blocks`).
fn tail(path: &str, max: usize) -> String {
    let n = path.chars().count();
    if n <= max {
        return path.to_string();
    }
    let skip = n - max + 1;
    format!("…{}", path.chars().skip(skip).collect::<String>())
}

/// Where the list opens: at the caret (window coordinates of the word's
/// start, and the line's height), above it, or below it on the new-chat
/// screen where the box sits mid-window. None: over the box's left edge.
#[derive(Clone, Copy)]
pub struct Place {
    pub caret: Option<(gpui::Point<gpui::Pixels>, gpui::Pixels)>,
    pub below: bool,
}

/// The popover. `selected` is the row under the keyboard; a click on a row
/// calls `on_pick` with its index.
pub fn render(
    t: &Theme,
    rows: &[Item],
    selected: usize,
    loading: bool,
    place: Place,
    ranked: bool,
    on_pick: impl Fn(usize, &mut gpui::Window, &mut gpui::App) + Clone + 'static,
    on_hover: impl Fn(usize, &mut gpui::App) + Clone + 'static,
) -> AnyElement {
    let files = rows.first().is_some_and(|r| r.file);
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
        // names its group instead of a header over them. The @ list has no
        // groups: its icons say file or folder.
        if !ranked && !item.file && item.group != last_group {
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
        let row = div()
            .id(("ac", i))
            .flex()
            .items_center()
            .rounded(t.radius.button)
            .cursor_pointer()
            .when(on, |d| d.bg(t.colors.hover))
            .on_mouse_move(move |_, _, cx| hover(i, cx))
            .on_click(move |_, window, cx| pick(i, window, cx));
        let row = if item.file {
            // Name first, its folder after it, dimmed and cut from the left.
            let folder = matches!(item.pick, Pick::Folder(_));
            let (glyph, color) = super::file_icon::for_name(&item.label, folder, t);
            row.gap(t.space.sm)
                .h(t.size.control)
                .px(t.space.sm)
                .child(icon(glyph, t.size.icon_sm, color))
                .child(
                    div()
                        .flex_none()
                        .max_w(px(240.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(if on {
                            t.colors.text_strong
                        } else {
                            t.colors.text
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
                        .child(tail(&item.detail, 32)),
                )
                .when(folder, |d| {
                    d.child(icon(
                        "icons/chevron-right.svg",
                        t.size.icon_xs,
                        t.colors.text_subtle,
                    ))
                })
        } else {
            row.gap(t.space.md)
                .h(t.size.row)
                .px(t.space.md)
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
                        .max_w(px(220.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
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
                .when(ranked, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(item.group),
                    )
                })
        };
        list = list.child(row);
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
    let hints: &[&str] = if files {
        &["↑↓ choose", "Enter add", "Tab open folder", "Esc close"]
    } else {
        &["↑↓ choose", "Enter or Tab complete", "Esc close"]
    };
    let mut footer = div()
        .flex()
        .gap(t.space.lg)
        .px(t.space.md)
        .py(t.space.xs)
        .border_t_1()
        .border_color(t.colors.border)
        .text_size(t.text.caption)
        .text_color(t.colors.text_subtle);
    for h in hints {
        footer = footer.child(*h);
    }
    let card = div()
        .id("autocomplete")
        .occlude()
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_popover())
        .child(list)
        .child(footer);
    match place.caret {
        // At the word being completed, as an editor's completion list.
        Some((at, line)) => {
            let gap = px(6.);
            let (pos, corner) = if place.below {
                (
                    gpui::point(at.x - t.space.md, at.y + line + gap),
                    gpui::Anchor::TopLeft,
                )
            } else {
                (
                    gpui::point(at.x - t.space.md, at.y - gap),
                    gpui::Anchor::BottomLeft,
                )
            };
            gpui::anchored()
                .position(pos)
                .anchor(corner)
                .snap_to_window_with_margin(t.space.sm)
                .child(card.w(t.size.completion_width))
                .into_any_element()
        }
        None => card
            .absolute()
            .map(|d| {
                if place.below {
                    d.top_full().mt(px(8.))
                } else {
                    d.bottom_full().mb(px(8.))
                }
            })
            .left_0()
            .w(t.size.completion_width)
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::{file_items, mention, trigger, Pick};

    fn entries() -> Vec<String> {
        [
            "src/",
            "src/ui/",
            "Cargo.toml",
            "README.md",
            "src/main.rs",
            "src/state.rs",
            "src/ui/composer.rs",
            "docs/a b.md",
            "docs/",
        ]
        .map(String::from)
        .to_vec()
    }

    fn labels(q: &str) -> Vec<String> {
        file_items(q, &entries())
            .iter()
            .map(|i| i.label.to_string())
            .collect()
    }

    #[test]
    fn the_at_list_shows_folders_and_files_like_the_extension() {
        // Nothing typed: the top level, as given (folders first in the index).
        assert_eq!(labels(""), ["src", "Cargo.toml", "README.md", "docs"]);
        // A folder's path: what is in it.
        assert_eq!(labels("src/"), ["ui", "main.rs", "state.rs"]);
        assert_eq!(labels("src/ui/"), ["composer.rs"]);
        assert_eq!(labels("src/st"), ["state.rs"]);
        // Anything else: by name first.
        assert_eq!(labels("comp")[0], "composer.rs");
        // Starting with the query beats a scattered match.
        let more: Vec<String> = ["src/ui/command_palette.rs", "src/ui/composer.rs"]
            .map(String::from)
            .to_vec();
        assert_eq!(file_items("comp", &more)[0].label.as_ref(), "composer.rs");
        let rows = file_items("comp", &entries());
        assert_eq!(rows[0].detail.as_ref(), "src/ui");
        assert!(matches!(&rows[0].pick, Pick::Text(t) if t == "@src/ui/composer.rs "));
        let ui = file_items("src/", &entries());
        assert!(matches!(&ui[0].pick, Pick::Folder(p) if p == "src/ui/"));
        // A space in a path is quoted, as ForgeCLI reads it.
        assert_eq!(mention("docs/a b.md"), "@\"docs/a b.md\"");
    }

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
