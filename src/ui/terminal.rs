//! A shell command, drawn as Forge draws it: a terminal block.
//!
//! - The input line: a `$` prompt and the command, syntax coloured (the
//!   command word, keywords, flags, strings, variables, operators, numbers,
//!   comments), with a copy button that shows on hover.
//! - The output, under a hairline: its ANSI colours kept and mapped onto
//!   Forge's terminal palette. Long output shows its last lines under a
//!   fade, with "Show N earlier lines"; a running command shows a blinking
//!   block cursor; an empty result says so.
//! - A footer: the exit code as a badge, the line count, and the toggle.

use std::time::Duration;

use gpui::{
    div, font, linear_color_stop, linear_gradient, prelude::*, px, relative, Animation,
    AnimationExt, AnyElement, ClipboardItem, Entity, FontStyle, FontWeight, Hsla, SharedString,
    StyledText, TextRun,
};

use super::icon;
use crate::state::{AppState, ToolCall, ToolStatus};
use crate::theme::Theme;

/// Output longer than this shows only its last lines until expanded.
const TAIL_LINES: usize = 10;

// ------------------------------------------------------------ highlighting

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tok {
    Command,
    Keyword,
    Flag,
    Str,
    Var,
    Op,
    Num,
    Comment,
    Plain,
}

const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "select", "time", "!",
];

/// Split a command line into coloured tokens. The first word of each
/// command (at the start, and after `|`, `&&`, `||`, `;`, or a keyword like
/// `then`) is the command; `NAME=value` before it is a variable.
pub fn highlight(line: &str) -> Vec<(Tok, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out: Vec<(Tok, String)> = vec![];
    let mut push = |tok: Tok, s: String| match out.last_mut() {
        Some((t, text)) if *t == tok => text.push_str(&s),
        _ => out.push((tok, s)),
    };
    let mut expect_command = true;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            push(Tok::Plain, c.to_string());
            i += 1;
        } else if c == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
            push(Tok::Comment, chars[i..].iter().collect());
            break;
        } else if c == '\'' || c == '"' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != c {
                j += if c == '"' && chars[j] == '\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(chars.len());
            push(Tok::Str, chars[i..end].iter().collect());
            i = end;
            expect_command = false;
        } else if c == '$' {
            let mut j = i + 1;
            if chars.get(j) == Some(&'{') {
                while j < chars.len() && chars[j] != '}' {
                    j += 1;
                }
                j = (j + 1).min(chars.len());
            } else {
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
            }
            push(Tok::Var, chars[i..j].iter().collect());
            i = j;
            expect_command = false;
        } else if let Some(op) = operator(&chars[i..]) {
            push(Tok::Op, op.clone());
            i += op.chars().count();
            expect_command = matches!(op.as_str(), "|" | "||" | "&&" | ";" | "&" | "(");
        } else {
            let mut j = i;
            while j < chars.len()
                && !chars[j].is_whitespace()
                && operator(&chars[j..]).is_none()
                && !matches!(chars[j], '\'' | '"' | '$')
            {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let tok = if expect_command && word.contains('=') && !word.starts_with('=') {
                Tok::Var
            } else if expect_command && KEYWORDS.contains(&word.as_str()) {
                Tok::Keyword
            } else if expect_command {
                expect_command = false;
                Tok::Command
            } else if word.starts_with('-') {
                Tok::Flag
            } else if !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) {
                Tok::Num
            } else {
                Tok::Plain
            };
            if tok == Tok::Keyword {
                expect_command = !matches!(word.as_str(), "fi" | "done" | "esac");
            }
            push(tok, word);
            i = j;
        }
    }
    out
}

fn operator(rest: &[char]) -> Option<String> {
    const OPS: &[&str] = &[
        "2>&1", "&>", "2>", "&&", "||", ">>", "|", ";", ">", "<", "&", "(", ")",
    ];
    let s: String = rest.iter().take(4).collect();
    OPS.iter()
        .find(|op| s.starts_with(**op))
        .map(|op| op.to_string())
}

// ----------------------------------------------------------------- ANSI

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Style {
    pub fg: Option<u8>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
}

/// Split output into styled pieces, reading SGR escapes (`ESC [ … m`):
/// colours 30-37 and 90-97 (bright maps onto the same eight), 39, bold,
/// dim, italic and reset. Other escapes are dropped.
pub fn ansi(text: &str) -> Vec<(Style, String)> {
    let mut out: Vec<(Style, String)> = vec![];
    let mut style = Style::default();
    let mut rest = text;
    while let Some(at) = rest.find('\x1b') {
        if at > 0 {
            out.push((style, rest[..at].to_string()));
        }
        let after = &rest[at + 1..];
        let Some(body) = after.strip_prefix('[') else {
            rest = after;
            continue;
        };
        let end = body
            .find(|c: char| c.is_ascii_alphabetic())
            .unwrap_or(body.len());
        if body[end..].starts_with('m') {
            let codes: Vec<u32> = body[..end]
                .split(';')
                .map(|c| c.parse().unwrap_or(0))
                .collect();
            for code in if codes.is_empty() { vec![0] } else { codes } {
                match code {
                    0 => style = Style::default(),
                    1 => style.bold = true,
                    2 => style.dim = true,
                    3 => style.italic = true,
                    22 => (style.bold, style.dim) = (false, false),
                    23 => style.italic = false,
                    30..=37 => style.fg = Some((code - 30) as u8),
                    90..=97 => style.fg = Some((code - 90) as u8),
                    39 => style.fg = None,
                    _ => {}
                }
            }
        }
        rest = &body[(end + 1).min(body.len())..];
    }
    if !rest.is_empty() {
        out.push((style, rest.to_string()));
    }
    out
}

// ---------------------------------------------------------------- drawing

fn tok_color(tok: Tok, t: &Theme) -> Hsla {
    let k = &t.terminal;
    match tok {
        Tok::Command => k.command,
        Tok::Keyword => k.keyword,
        Tok::Flag => k.flag,
        Tok::Str => k.string,
        Tok::Var => k.variable,
        Tok::Op => k.operator,
        Tok::Num => k.number,
        Tok::Comment => k.comment,
        Tok::Plain => k.foreground,
    }
}

fn command_text(cmd: &str, t: &Theme) -> StyledText {
    let mut text = String::new();
    let mut runs = vec![];
    for (tok, s) in highlight(cmd) {
        let mut f = font(t.font_mono.clone());
        if tok == Tok::Command {
            f.weight = FontWeight::SEMIBOLD;
        }
        if tok == Tok::Comment {
            f.style = FontStyle::Italic;
        }
        runs.push(TextRun {
            len: s.len(),
            font: f,
            color: tok_color(tok, t),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        text.push_str(&s);
    }
    StyledText::new(text).with_runs(runs)
}

fn output_text(line: &str, t: &Theme) -> StyledText {
    let mut text = String::new();
    let mut runs = vec![];
    for (style, s) in ansi(line) {
        let mut f = font(t.font_mono.clone());
        if style.bold {
            f.weight = FontWeight::BOLD;
        }
        if style.italic {
            f.style = FontStyle::Italic;
        }
        let mut color = style
            .fg
            .map(|n| t.terminal.ansi[n as usize])
            .unwrap_or(t.terminal.foreground);
        if style.dim {
            color = color.opacity(0.6);
        }
        runs.push(TextRun {
            len: s.len(),
            font: f,
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        text.push_str(&s);
    }
    if text.is_empty() {
        // Keep blank lines their height.
        text.push(' ');
        runs.push(TextRun {
            len: 1,
            font: font(t.font_mono.clone()),
            color: t.terminal.foreground,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    StyledText::new(text).with_runs(runs)
}

/// What the block needs from its place in the transcript.
pub struct Where {
    pub state: Entity<AppState>,
    /// Where the chat's agent runs, for "Show in terminal".
    pub place: Option<(SharedString, &'static str)>,
    pub chat: usize,
    pub msg: usize,
    pub block: usize,
    pub copied: Option<SharedString>,
    /// A reply is running in this chat (Run again waits for it).
    pub busy: bool,
}

/// The terminal block for a shell tool call.
pub fn block(call: &ToolCall, at: &Where, t: &Theme) -> AnyElement {
    let k = t.terminal;
    let key = format!("{}-{}-{}", at.chat, at.msg, at.block);
    let group: SharedString = format!("term-{key}").into();
    let copy_key: SharedString = format!("cmd-{key}").into();
    let copied = at.copied.as_ref() == Some(&copy_key);
    let running = call.status == ToolStatus::Running;

    // The input line, with its copy button.
    let command = call.target.to_string();
    let state = at.state.clone();
    let copy = div()
        .id(SharedString::from(format!("copy-{key}")))
        .absolute()
        .top(px(4.))
        .right(px(4.))
        .p(px(4.))
        .rounded(t.radius.small)
        .bg(t.colors.surface)
        .border_1()
        .border_color(k.divider)
        .cursor_pointer()
        .when(!copied, |d| {
            d.invisible().group_hover(group.clone(), |s| s.visible())
        })
        .child(icon(
            if copied {
                "icons/check.svg"
            } else {
                "icons/copy.svg"
            },
            t.size.icon_sm,
            if copied { t.extra.success } else { k.muted },
        ))
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
            let key = copy_key.clone();
            state.update(cx, |s, cx| s.mark_copied(key, cx));
        });
    let input = div()
        .group(group.clone())
        .relative()
        .flex()
        .gap(px(8.))
        .pl(px(10.))
        .pr(px(36.))
        .py(px(7.))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .text_color(k.prompt)
                .child("$"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(command_text(&call.target, t)),
        )
        .child(copy);

    // The output.
    let text = call.output.to_string();
    let lines: Vec<&str> = if text.is_empty() {
        vec![]
    } else {
        text.lines().collect()
    };
    let hidden = if call.full {
        0
    } else {
        lines.len().saturating_sub(TAIL_LINES)
    };
    let mut out = div().relative().px(px(10.)).py(px(7.)).flex().flex_col();
    if running {
        out = out.child(div().w(px(7.)).h(px(14.)).bg(k.prompt).with_animation(
            SharedString::from(format!("cursor-{key}")),
            Animation::new(Duration::from_millis(1000)).repeat(),
            |d, delta| d.opacity(if delta < 0.5 { 1. } else { 0. }),
        ));
    } else if call.status == ToolStatus::Declined {
        out = out.child(
            div()
                .italic()
                .text_color(k.muted)
                .child("Not run: you declined it."),
        );
    } else if lines.is_empty() {
        out = out.child(div().italic().text_color(k.muted).child("No output"));
    } else {
        let shown = lines[hidden..]
            .iter()
            .map(|l| div().whitespace_nowrap().child(output_text(l, t)));
        out = out.children(shown);
        if hidden > 0 {
            // The cut-off top fades out, as in Forge.
            out = out.child(div().absolute().top_0().left_0().right_0().h(px(22.)).bg(
                linear_gradient(
                    0.,
                    linear_color_stop(k.background, 0.),
                    linear_color_stop(k.background.opacity(0.), 1.),
                ),
            ));
        }
    }
    let out: AnyElement = if call.full && lines.len() > TAIL_LINES {
        div()
            .id(SharedString::from(format!("out-{key}")))
            .max_h(px(360.))
            .overflow_y_scroll()
            .child(out)
            .into_any_element()
    } else {
        out.into_any_element()
    };

    // The footer: exit code, size, and the toggle.
    let footer = (!running && call.status != ToolStatus::Declined).then(|| {
        let code = if call.status == ToolStatus::Failed {
            1
        } else {
            0
        };
        let (fg, bg) = if code == 0 {
            (t.extra.success, t.extra.success_tint)
        } else {
            (t.extra.danger, t.extra.danger_tint)
        };
        let (state, chat, msg, b) = (at.state.clone(), at.chat, at.msg, at.block);
        let (copier, rerun, output, cmd) = (
            at.state.clone(),
            at.state.clone(),
            call.output.to_string(),
            call.target.to_string(),
        );
        let out_key: SharedString = format!("out-{key}").into();
        let out_copied = at.copied.as_ref() == Some(&out_key);
        let fg_hover = k.foreground;
        let bg_hover = k.divider;
        div()
            .min_h(px(26.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_color(k.divider)
            .font_family(t.font_sans.clone())
            .text_size(px(11.))
            .text_color(k.muted)
            .child(
                div()
                    .px(px(5.))
                    .rounded(px(3.))
                    .bg(bg)
                    .text_color(fg)
                    .font_family(t.font_mono.clone())
                    .child(format!("exit {code}")),
            )
            .child(format!(
                "{} line{}",
                lines.len(),
                if lines.len() == 1 { "" } else { "s" }
            ))
            .child(div().flex_1())
            .child(
                footer_button(
                    &format!("copy-out-{key}"),
                    if out_copied { "Copied" } else { "Copy output" },
                    if out_copied {
                        "icons/check.svg"
                    } else {
                        "icons/copy.svg"
                    },
                    fg_hover,
                    bg_hover,
                    k.muted,
                    t,
                )
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(crate::export::plain(&output)));
                    let key = out_key.clone();
                    copier.update(cx, |s, cx| s.mark_copied(key, cx));
                }),
            )
            .child({
                let feed = crate::state::AgentFeed::of(
                    call,
                    at.place.as_ref().map(|(l, i)| (l.to_string(), *i)),
                );
                let shower = at.state.clone();
                footer_button(
                    &format!("show-{key}"),
                    "Show in terminal",
                    "icons/terminal.svg",
                    fg_hover,
                    bg_hover,
                    k.muted,
                    t,
                )
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new(
                        "Open this command and its output in a terminal tab",
                    )
                    .build(window, cx)
                })
                .on_click(move |_, window, cx| {
                    shower.update(cx, |s, _| s.agent_feed = Some(feed.clone()));
                    window.dispatch_action(Box::new(super::ShowAgentOutput), cx);
                })
            })
            .when(!at.busy, |d| {
                d.child(
                    footer_button(
                        &format!("rerun-{key}"),
                        "Run again",
                        "icons/rotate-ccw.svg",
                        fg_hover,
                        bg_hover,
                        k.muted,
                        t,
                    )
                    .tooltip(|window, cx| {
                        gpui::component::tooltip::Tooltip::new(
                            "Ask Forge to run this command again (your mode's permissions apply)",
                        )
                        .build(window, cx)
                    })
                    .on_click(move |_, _, cx| rerun.update(cx, |s, cx| s.rerun(&cmd, cx))),
                )
            })
            .when(lines.len() > TAIL_LINES, |d| {
                d.child(
                    div()
                        .id(SharedString::from(format!("more-{key}")))
                        .h(px(20.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .rounded(px(3.))
                        .cursor_pointer()
                        .hover(move |s| s.text_color(fg_hover).bg(bg_hover))
                        .child(if call.full {
                            "Show less".to_string()
                        } else {
                            format!(
                                "Show {hidden} earlier line{}",
                                if hidden == 1 { "" } else { "s" }
                            )
                        })
                        .on_click(move |_, _, cx| {
                            state.update(cx, |s, cx| s.toggle_full(chat, msg, b, cx))
                        }),
                )
            })
    });

    div()
        .border_t_1()
        .border_color(t.colors.border)
        .bg(k.background)
        .font_family(t.font_mono.clone())
        .text_size(px(12.))
        .line_height(relative(1.45))
        .text_color(k.foreground)
        .child(input)
        .child(div().h(px(1.)).bg(k.divider))
        .child(out)
        .children(footer)
        .into_any_element()
}

/// A small text button in the terminal's footer.
fn footer_button(
    id: &str,
    label: &'static str,
    path: &'static str,
    fg_hover: gpui::Hsla,
    bg_hover: gpui::Hsla,
    muted: gpui::Hsla,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(id.to_string()))
        .h(px(20.))
        .px(px(6.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded(px(3.))
        .cursor_pointer()
        .hover(move |s| s.text_color(fg_hover).bg(bg_hover))
        .child(icon(path, t.size.icon_xs, muted))
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<(Tok, &str)> {
        // Leak for terse assertions in tests only.
        highlight(line)
            .into_iter()
            .filter(|(t, _)| *t != Tok::Plain)
            .map(|(t, s)| (t, &*Box::leak(s.into_boxed_str())))
            .collect()
    }

    #[test]
    fn colours_a_pipeline() {
        assert_eq!(
            kinds("FOO=1 cargo test --release 2>&1 | grep \"ok\" && echo $HOME # done"),
            vec![
                (Tok::Var, "FOO=1"),
                (Tok::Command, "cargo"),
                (Tok::Flag, "--release"),
                (Tok::Op, "2>&1"),
                (Tok::Op, "|"),
                (Tok::Command, "grep"),
                (Tok::Str, "\"ok\""),
                (Tok::Op, "&&"),
                (Tok::Command, "echo"),
                (Tok::Var, "$HOME"),
                (Tok::Comment, "# done"),
            ]
        );
    }

    #[test]
    fn keywords_open_a_command() {
        let k = kinds("if true; then ls -la 3; fi");
        assert_eq!(k[0], (Tok::Keyword, "if"));
        assert_eq!(k[1], (Tok::Command, "true"));
        assert!(k.contains(&(Tok::Keyword, "then")));
        assert!(k.contains(&(Tok::Command, "ls")));
        assert!(k.contains(&(Tok::Num, "3")));
        assert!(k.contains(&(Tok::Keyword, "fi")));
    }

    #[test]
    fn reads_ansi_colours() {
        let parts = ansi("test a ... \x1b[32mok\x1b[0m\n\x1b[1;31merror\x1b[39m!");
        assert_eq!(parts[0], (Style::default(), "test a ... ".into()));
        assert_eq!(parts[1].0.fg, Some(2));
        assert_eq!(parts[1].1, "ok");
        assert!(parts[3].0.bold && parts[3].0.fg == Some(1));
        assert_eq!(
            parts[4],
            (
                Style {
                    bold: true,
                    ..Default::default()
                },
                "!".into()
            )
        );
        assert_eq!(
            ansi("\x1b[92mfine\x1b[0m"),
            vec![(
                Style {
                    fg: Some(2),
                    ..Default::default()
                },
                "fine".into()
            )]
        );
    }
}
