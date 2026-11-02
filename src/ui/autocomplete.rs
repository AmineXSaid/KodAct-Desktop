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

/// A mention in a message: a `/name` ForgeCLI runs leading it (a skill or
/// a command), or `@path` naming one of the project's files or folders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Skill,
    Command,
    File,
}

/// The mentions in `text`, by byte range. In the box a word counts once a
/// space follows it (`sent` false); in a sent message its end counts too.
pub fn marks(text: &str, s: &AppState, sent: bool) -> Vec<(std::ops::Range<usize>, Mark)> {
    let mut out = vec![];
    let finished =
        |end: usize| text[end..].starts_with(char::is_whitespace) || (sent && end == text.len());
    if let Some(rest) = text.strip_prefix('/') {
        let end = 1 + rest.find(char::is_whitespace).unwrap_or(rest.len());
        let name = &text[1..end];
        if finished(end) {
            if let Some(c) = s.completion_commands().into_iter().find(|c| c.name == name) {
                let mark = if c.group == "Skills" {
                    Mark::Skill
                } else {
                    Mark::Command
                };
                out.push((0..end, mark));
            }
        }
    }
    let files = s.completion_files();
    let mut at = 0;
    for word in text.split_inclusive(char::is_whitespace) {
        let start = at;
        at += word.len();
        let word = word.trim_end();
        let Some(path) = word.strip_prefix('@') else {
            continue;
        };
        let end = start + word.len();
        if path.is_empty() || !finished(end) {
            continue;
        }
        let known = files
            .iter()
            .any(|f| f == path || f.trim_end_matches('/') == path.trim_end_matches('/'));
        if known {
            out.push((start..end, Mark::File));
        }
    }
    out
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
    /// A row of the @ list: a file or a folder of the project.
    pub file: bool,
    /// A skill: its name shows bold, in the skills' colour.
    pub skill: bool,
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
            "icons/file-text.svg",
            Box::new(ExportChat),
        ),
        (
            "export-json",
            "Export this chat as JSON",
            "icons/file-json.svg",
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
            "browser",
            "Turn the web browser on or off for Forge's agents",
            "icons/globe.svg",
            Box::new(ToggleBrowser),
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
                    skill: group == "Skills",
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
            let icon = if c.group == "Skills" {
                "icons/scroll-text.svg"
            } else {
                "icons/terminal.svg"
            };
            push(order, label, detail, c.group, icon, Pick::Text(text));
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

/// The most rows the @ list shows.
const MAX_FILE_ROWS: usize = 100;

/// The @ list, as the VS Code extension has it: one flat list, ranked.
/// Nothing typed: the project's top level, sorted by name. Otherwise every file and
/// folder whose name or path matches, a match in the name counting double;
/// a `/` in the query first keeps to the paths under its folder part, so
/// `src/ui/comp` and a pasted `src/ui/composer.rs` both find their file.
/// Choosing a row (Enter or Tab, file or folder) writes `@path `.
pub fn file_items(q: &str, entries: &[String]) -> Vec<Item> {
    let item = |entry: &String, positions: Vec<usize>| {
        let folder = entry.ends_with('/');
        let (name, _) = split(entry);
        Item {
            label: name.to_string().into(),
            detail: entry.trim_end_matches('/').to_string().into(),
            group: if folder { "Folders" } else { "Files" },
            icon: "",
            pick: Pick::Text(format!("{} ", mention(entry))),
            positions,
            file: true,
            skill: false,
        }
    };
    let q = q.trim_start_matches("./");
    if q.is_empty() {
        let mut top: Vec<&String> = entries
            .iter()
            .filter(|e| !e.trim_end_matches('/').contains('/'))
            .collect();
        // As the extension sorts them: by name, byte order, folders and
        // files together.
        top.sort_by(|a, b| a.trim_end_matches('/').cmp(b.trim_end_matches('/')));
        return top
            .into_iter()
            .take(MAX_FILE_ROWS)
            .map(|e| item(e, vec![]))
            .collect();
    }
    let lower = q.to_lowercase();
    let (base, rest) = match lower.rfind('/') {
        Some(i) => (&lower[..=i], &q[i + 1..]),
        None => ("", q),
    };
    let mut scored: Vec<(i32, usize, Item)> = vec![];
    for (order, e) in entries.iter().enumerate() {
        let path = e.to_lowercase();
        // Under the folder typed, not the folder itself.
        if !path.starts_with(base) || path == base {
            continue;
        }
        let (name, _) = split(e);
        // The path itself, as typed or pasted: first.
        let exact = path.trim_end_matches('/') == lower.trim_end_matches('/');
        let by_name = fuzzy(rest, name).map(|(score, pos)| {
            let prefix = name.to_lowercase().starts_with(&rest.to_lowercase());
            (score * 2 + if prefix { 1_000 } else { 0 }, pos)
        });
        let by_path = fuzzy(q, e).map(|(score, _)| (score, vec![]));
        let Some((mut score, pos)) = by_name.into_iter().chain(by_path).max_by_key(|m| m.0) else {
            continue;
        };
        if exact {
            score += 100_000;
        }
        // Under the folder typed, its own entries before deeper ones.
        let depth = path[base.len()..]
            .trim_end_matches('/')
            .matches('/')
            .count() as i32;
        score -= depth * 5;
        // Tests after the code they test, when the match is as good.
        if path.contains("test") || path.contains("spec") {
            score -= 15;
        }
        scored.push((score, order, item(e, pos)));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(MAX_FILE_ROWS)
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
#[allow(clippy::too_many_arguments)]
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
                    .text_color(t.colors.text_muted)
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
            // Name first, its whole path after it, dimmed and cut from the left.
            let folder = item.group == "Folders";
            // The extension's own icons, in their colours.
            let glyph = super::file_icon::asset(&item.label, folder, &item.detail);
            row.gap(t.space.sm)
                .h(t.size.hit_sm - px(4.))
                .px(t.space.sm)
                .text_size(t.text.body_sm)
                .child(gpui::img(glyph).flex_none().size(t.size.icon))
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
                        .text_color(t.colors.text_muted)
                        // Cut from the left only: the rest of the row is
                        // the name's, so the path gets what is left.
                        .child(tail(
                            &item.detail,
                            60usize.saturating_sub(item.label.chars().count()),
                        )),
                )
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
                        .map(|d| {
                            if item.skill {
                                // A skill reads as one: bold, in its colour.
                                d.font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.extra.skill)
                                    .child(item.label.clone())
                            } else {
                                d.child(highlighted(&item.label, &item.positions, t))
                            }
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(item.detail.clone()),
                )
                .when(ranked, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
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
                .text_color(t.colors.text_muted)
                .child(if loading {
                    "Looking through the project…"
                } else {
                    "No matches"
                }),
        );
    }
    let hints: &[&str] = if files {
        &["↑↓ choose", "Enter or Tab add", "Esc close"]
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
        .text_color(t.colors.text_muted);
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
    fn the_at_list_is_one_ranked_list_like_the_extension() {
        // Nothing typed: the top level, sorted as the extension sorts it.
        assert_eq!(labels(""), ["Cargo.toml", "README.md", "docs", "src"]);
        // A name anywhere in the project, without its folders typed.
        assert_eq!(labels("comp")[0], "composer.rs");
        // A folder part keeps to what is under it; its own entries first.
        assert_eq!(labels("src/")[..2], ["ui", "main.rs"]);
        assert_eq!(labels("src/ui/comp"), ["composer.rs"]);
        assert_eq!(labels("src/st"), ["state.rs"]);
        // A pasted path comes first, as it is.
        let rows = file_items("src/ui/composer.rs", &entries());
        assert_eq!(rows[0].detail.as_ref(), "src/ui/composer.rs");
        assert!(matches!(&rows[0].pick, Pick::Text(t) if t == "@src/ui/composer.rs "));
        // Starting with the query beats a scattered match.
        let more: Vec<String> = ["src/ui/command_palette.rs", "src/ui/composer.rs"]
            .map(String::from)
            .to_vec();
        assert_eq!(file_items("comp", &more)[0].label.as_ref(), "composer.rs");
        // A folder is added like a file.
        let ui = file_items("src/u", &entries());
        assert!(matches!(&ui[0].pick, Pick::Text(t) if t == "@src/ui/ "));
        // A folder deep down, from its name or its path.
        let deep: Vec<String> = ["a/", "a/b/", "a/b/tls_assistant/", "a/b/tls_assistant/x.py"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            file_items("tls", &deep)[0].detail.as_ref(),
            "a/b/tls_assistant"
        );
        assert_eq!(
            file_items("a/b/tl", &deep)[0].detail.as_ref(),
            "a/b/tls_assistant"
        );
        // Typing on after a mention inserted mid-sentence completes the new word.
        assert_eq!(
            trigger("/simplify @comp").map(|t| t.query),
            Some("comp".into())
        );
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
