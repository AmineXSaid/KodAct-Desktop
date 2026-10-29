//! The Session column beside a chat, as Claude Code's: what you'd want to
//! read at a glance, in cards that fold.
//!
//! - **Environment**: where the agent runs (this computer, or the remote
//!   host), the folder it works in, and its branch.
//! - **Context**: how full the window is, a bar in a colour per part, and
//!   each part in tokens and per cent (ForgeCLI's own count,
//!   `get_context_usage`); the tools, memory files and skills behind them.
//! - **Changes**: what the chat changed, file by file, with Create PR.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, Hsla, SharedString};

use super::icon;
use crate::context::{short, ContextUsage};
use crate::state::{AppState, Block, Message, ToolKind, ToolStatus};
use crate::theme::Theme;

/// A file the chat changed, with its first change in the transcript.
struct Changed {
    name: SharedString,
    adds: usize,
    dels: usize,
    at: (usize, usize),
}

/// Each file the chat edited or wrote, in the order it first changed.
fn changes(messages: &[Message]) -> Vec<Changed> {
    let mut out: Vec<(SharedString, Changed)> = vec![];
    for (m, msg) in messages.iter().enumerate() {
        let Message::Assistant { blocks } = msg else {
            continue;
        };
        for (b, block) in blocks.iter().enumerate() {
            let Block::Tool(c) = block else { continue };
            if !matches!(c.kind, ToolKind::Edit | ToolKind::Write) || c.status != ToolStatus::Done {
                continue;
            }
            let adds = c
                .diff
                .iter()
                .filter(|l| l.kind == crate::state::DiffKind::Add)
                .count();
            let dels = c
                .diff
                .iter()
                .filter(|l| l.kind == crate::state::DiffKind::Del)
                .count();
            match out.iter_mut().find(|(t, _)| *t == c.target) {
                Some((_, f)) => {
                    f.adds += adds;
                    f.dels += dels;
                }
                None => {
                    let name = c
                        .target
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(&c.target)
                        .to_string();
                    out.push((
                        c.target.clone(),
                        Changed {
                            name: name.into(),
                            adds,
                            dels,
                            at: (m, b),
                        },
                    ));
                }
            }
        }
    }
    out.into_iter().map(|(_, f)| f).collect()
}

/// The colour a part of the context takes in the bar and its legend.
fn part_colour(name: &str, t: &Theme) -> Hsla {
    match name {
        "Messages" => t.colors.sky,
        "System prompt" => t.colors.marigold,
        "Built-in tools" => t.extra.success,
        "MCP tools" => t.extra.accent,
        "Skills" => t.extra.skill,
        "Memory files" => t.extra.memory,
        _ => t.colors.text_muted,
    }
}

fn pct(n: u64, of: u64) -> String {
    if of == 0 {
        return "-".into();
    }
    format!("{:.1}%", n as f64 * 100. / of as f64)
}

/// A card: its name and a chevron (a click folds it), then its body.
fn card(
    t: &Theme,
    state: &Entity<AppState>,
    name: &'static str,
    folded: bool,
    body: AnyElement,
) -> AnyElement {
    let hover = t.colors.hover;
    let st = state.clone();
    div()
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .child(
            div()
                .id(SharedString::from(format!("session-{name}")))
                .flex()
                .items_center()
                .gap(t.space.xs)
                .h(t.size.row)
                .px(t.space.md)
                .cursor_pointer()
                .rounded(t.radius.card)
                .hover(move |d| d.bg(hover))
                .text_size(t.text.body_sm)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_strong)
                .child(name)
                .child(icon(
                    if folded {
                        "icons/chevron-right.svg"
                    } else {
                        "icons/chevron-down.svg"
                    },
                    t.size.icon_xs,
                    t.colors.text_subtle,
                ))
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.fold_session_card(name, cx))),
        )
        .when(!folded, |d| {
            d.child(div().px(t.space.md).pb(t.space.md).child(body))
        })
        .into_any_element()
}

/// A row: an icon in a fixed slot, the words, and something at its end.
fn row(
    t: &Theme,
    glyph: &'static str,
    words: impl Into<SharedString>,
    end: Option<AnyElement>,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .min_h(t.size.menu_row)
        .text_size(t.text.body_sm)
        .text_color(t.colors.text)
        .child(icon(glyph, t.size.icon_sm, t.colors.text_muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(words.into()),
        )
        .children(end)
}

/// A small icon button at a row's end.
fn end_button(
    t: &Theme,
    id: &'static str,
    glyph: &'static str,
    tip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .p(px(3.))
        .rounded(t.radius.small)
        .cursor_pointer()
        .hover(move |d| d.bg(hover))
        .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(tip).build(w, cx))
        .child(icon(glyph, t.size.icon_sm, t.colors.text_muted))
}

fn environment(
    t: &Theme,
    state: &Entity<AppState>,
    s: &AppState,
    chat: usize,
    project: usize,
) -> AnyElement {
    let remote = s.chat_remote(chat);
    let place: (&'static str, String) = match &remote {
        Some(r) => (r.target.icon(), r.target.label()),
        None => ("icons/laptop.svg", "This computer".into()),
    };
    let folder = s.project(project).filter(|p| !p.scratch);
    div()
        .flex()
        .flex_col()
        .child(row(
            t,
            place.0,
            place.1,
            Some(
                end_button(
                    t,
                    "session-remote",
                    "icons/settings.svg",
                    "Remote connection (Ctrl+Alt+O)",
                )
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(crate::OpenRemoteMenu), cx)
                })
                .into_any_element(),
            ),
        ))
        .child(row(
            t,
            "icons/folder.svg",
            folder.map_or("No folder: a plain chat".to_string(), |p| {
                p.name.to_string()
            }),
            Some({
                let st = state.clone();
                end_button(
                    t,
                    "session-move",
                    "icons/plus.svg",
                    "Move the chat to another project",
                )
                .on_click(move |ev, _, cx| {
                    let at = ev.position();
                    st.update(cx, |s, cx| {
                        s.chat_menu_from(chat, at, cx);
                        s.show_chat_sub(Some(crate::state::ChatSub::Move), cx);
                    })
                })
                .into_any_element()
            }),
        ))
        .when(folder.is_some() && !s.branch.is_empty(), |d| {
            d.child(row(t, "icons/git-branch.svg", s.branch.clone(), None))
        })
        .into_any_element()
}

fn context(t: &Theme, state: &Entity<AppState>, s: &AppState, u: &ContextUsage) -> AnyElement {
    let window = u.window.max(1);
    let used = u.used();
    let hover = t.colors.hover;
    // The bar: each part in its colour, the compaction buffer quieter, the
    // free space the track itself.
    let mut parts: Vec<(Hsla, u64)> = u
        .categories
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(name, n)| (part_colour(name, t), *n))
        .collect();
    if u.buffer() > 0 {
        parts.push((t.colors.text_subtle.opacity(0.45), u.buffer()));
    }
    let bar = div()
        .flex()
        .w_full()
        .h(px(6.))
        .gap(px(1.))
        .rounded(t.radius.pill)
        .overflow_hidden()
        .bg(t.colors.border)
        .children(parts.iter().map(|(c, n)| {
            div()
                .h_full()
                .w(relative((*n as f32 / window as f32).clamp(0.004, 1.)))
                .bg(*c)
        }));
    let legend = |swatch: Hsla, name: String, n: u64, quiet: bool| {
        div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .min_h(px(22.))
            .text_size(t.text.body_sm)
            .text_color(if quiet {
                t.colors.text_muted
            } else {
                t.colors.text
            })
            .child(div().flex_none().size(px(8.)).rounded(px(2.)).bg(swatch))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(name),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(48.))
                    .flex()
                    .justify_end()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child(short(n)),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(44.))
                    .flex()
                    .justify_end()
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if quiet {
                        t.colors.text_muted
                    } else {
                        t.colors.text
                    })
                    .child(pct(n, window)),
            )
    };
    // The lists behind the parts, each opened by a click.
    let list = |key: &'static str, label: &'static str, rows: &[(String, u64)]| {
        let open = s.session_folded.contains(&key);
        let st = state.clone();
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(SharedString::from(format!("session-list-{key}")))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .min_h(px(22.))
                    .px(px(2.))
                    .rounded(t.radius.small)
                    .cursor_pointer()
                    .hover(move |d| d.bg(hover))
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_muted)
                    .child(icon(
                        if open {
                            "icons/chevron-down.svg"
                        } else {
                            "icons/chevron-right.svg"
                        },
                        t.size.icon_xs,
                        t.colors.text_subtle,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .child(rows.len().to_string()),
                    )
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.fold_session_card(key, cx))),
            )
            .when(open, |d| {
                d.children(rows.iter().map(|(name, n)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .min_h(px(20.))
                        .pl(t.space.lg)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .font_family(t.font_mono.clone())
                                .child(name.clone()),
                        )
                        .child(short(*n))
                }))
            })
    };
    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(format!("{} used", pct(used, window).replace(".0%", "%"))),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(format!("{} / {}", short(used), short(u.window))),
                ),
        )
        .child(bar)
        .child(
            div()
                .flex()
                .flex_col()
                .children(
                    u.categories
                        .iter()
                        .map(|(name, n)| legend(part_colour(name, t), name.clone(), *n, false)),
                )
                .when(u.buffer() > 0, |d| {
                    d.child(legend(
                        t.colors.text_subtle.opacity(0.45),
                        "Autocompact buffer".into(),
                        u.buffer(),
                        true,
                    ))
                })
                .child(legend(t.colors.border, "Free space".into(), u.free(), true)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .pt(t.space.xs)
                .border_t_1()
                .border_color(t.colors.border)
                .when(!u.tools.is_empty(), |d| {
                    d.child(list("list:tools", "Tools", &u.tools))
                })
                .when(!u.memory_files.is_empty(), |d| {
                    d.child(list("list:memory", "Memory files", &u.memory_files))
                })
                .when(!u.skills.is_empty(), |d| {
                    d.child(list("list:skills", "Skills", &u.skills))
                }),
        )
        .into_any_element()
}

fn changed(t: &Theme, state: &Entity<AppState>, files: &[Changed], pr: bool) -> AnyElement {
    let hover = t.colors.hover;
    let (adds, dels) = files
        .iter()
        .fold((0, 0), |(a, d), f| (a + f.adds, d + f.dels));
    if files.is_empty() {
        return div()
            .text_size(t.text.body_sm)
            .text_color(t.colors.text_muted)
            .child("No files changed yet.")
            .into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .min_h(t.size.menu_row)
                .child(icon("icons/file-code.svg", t.size.icon_sm, t.colors.text_muted))
                .child(super::message::diff_counts(adds, dels, t))
                .child(div().flex_1())
                .when(pr, |d| {
                    let st = state.clone();
                    d.child(
                        super::button::Button::secondary("session-pr", "Create PR")
                            .compact()
                            .on_click(move |_, _, cx| {
                                st.update(cx, |s, cx| {
                                    s.send(
                                        "Commit these changes on a new branch, push it, and open a pull request \
                                         with gh: a clear title, and a summary of what changed and why."
                                            .into(),
                                        cx,
                                    )
                                })
                            }),
                    )
                }),
        )
        .children(files.iter().enumerate().map(|(i, f)| {
            let (st, (m, b)) = (state.clone(), f.at);
            div()
                .id(("session-file", i))
                .flex()
                .items_center()
                .gap(t.space.sm)
                .min_h(px(24.))
                .px(px(2.))
                .rounded(t.radius.small)
                .cursor_pointer()
                .hover(move |d| d.bg(hover))
                .text_size(t.text.body_sm)
                .text_color(t.colors.text)
                // The kind of file in the muted ink: colour here is for the
                // change's size, and a brand's red would read as a failure.
                .child(super::icon(
                    super::file_icon::for_name(&f.name, false, t).0,
                    t.size.icon,
                    t.colors.text_muted,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(f.name.clone()),
                )
                .child(super::message::diff_counts(f.adds, f.dels, t))
                // To where it changed, in the conversation.
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.jump_to_block(m, b, cx)))
        }))
        .into_any_element()
}

/// The column for chat `chat`.
pub fn render(t: &Theme, state: &Entity<AppState>, s: &AppState, chat: usize) -> AnyElement {
    let Some(c) = s.chats.iter().find(|c| c.id == chat) else {
        return div().into_any_element();
    };
    let folded = |name: &str| s.session_folded.iter().any(|f| *f == name);
    let files = changes(&c.messages);
    let in_folder = s.project(c.project).is_some_and(|p| !p.scratch);
    let usage = s.context_usage.get(&chat);
    div()
        .id("session-column")
        .flex_none()
        .w(t.size.session_panel)
        .h_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(t.space.md)
        .p(t.space.md)
        .pl(px(0.))
        .child(card(
            t,
            state,
            "Environment",
            folded("Environment"),
            environment(t, state, s, chat, c.project),
        ))
        .child(card(
            t,
            state,
            "Context",
            folded("Context"),
            match usage {
                Some(u) => context(t, state, s, u),
                None => div()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_muted)
                    .child("Forge counts what fills the context once the chat has a reply.")
                    .into_any_element(),
            },
        ))
        .child(card(
            t,
            state,
            "Changes",
            folded("Changes"),
            changed(t, state, &files, in_folder && !s.busy()),
        ))
        .into_any_element()
}
