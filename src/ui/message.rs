//! One transcript item, as KodAct draws it:
//! - your message: a white card on the right;
//! - a reply: flush with the column, with no header (your message above it
//!   already starts the turn); it holds text (markdown), thinking (a
//!   disclosure), tool calls (one row each; Bash as a terminal block, a
//!   picture as a thumbnail) and permission prompts;
//! - the end of a turn: how long it took;
//! - a notice (interrupted, an error).

use std::time::{Duration, SystemTime};

use gpui::{
    div, img, percentage, prelude::*, px, relative, Animation, AnimationExt, AnyElement,
    ClipboardItem, Entity, FontWeight, ObjectFit, SharedString, Transformation,
};

use super::icon;
use super::markdown::{self, CopyContext};
use super::terminal;
use crate::state::{
    AppState, Block, Decider, DiffKind, Message, PermInfo, Rating, Review, ToolCall, ToolImage,
    ToolKind, ToolStatus, Verdict,
};
use crate::theme::{self, Theme};

pub struct ItemContext {
    pub state: Entity<AppState>,
    /// Your message's mentions (`/skill`, `/command`, `@path`), shown bold as
    /// in the box: a skill in its colour, the others in the accent.
    pub marks: Vec<(std::ops::Range<usize>, super::autocomplete::Mark)>,
    /// Where this chat's agent runs: the remote's label and icon, or None
    /// for this computer. Commands and approvals carry it as a badge.
    pub place: Option<(SharedString, &'static str)>,
    pub chat: usize,
    pub index: usize,
    pub copied: Option<SharedString>,
    pub busy: bool,
    pub last: bool,
    /// The file open beside the chat, so its card shows it is open.
    pub open_doc: Option<crate::state::FileRef>,
    /// Device pixels per logical pixel, for diagrams drawn to exact size.
    pub scale: f32,
    /// The rating given to the reply this turn end closes.
    pub rating: Option<Rating>,
    /// No later turn: this one's actions stay shown, not only on hover.
    pub newest_turn: bool,
    /// The chat ended on Stop and its last row offers Continue.
    pub can_continue: bool,
    /// Nothing you sent comes after this item: it belongs to the newest turn.
    pub in_last_turn: bool,
    /// The item just before is an error: a turn's footer says it failed.
    pub after_error: bool,
    /// Earlier versions from your message here on (edits and retries), and
    /// whether their list is open.
    pub versions: Vec<super::versions::Row>,
    pub versions_open: bool,
    /// Just closed: its list is drawn while it leaves.
    pub versions_leaving: bool,
    /// Diagrams showing their source (`AppState::diagram_code`).
    pub sources: std::sync::Arc<std::collections::HashSet<SharedString>>,
    pub zooms: std::sync::Arc<std::collections::HashMap<SharedString, f32>>,
    /// What the newest turn changed in the project's memory, under its reply.
    pub memory: Option<crate::state::MemoryChange>,
    /// The runs of calls opened to their steps, by their first block (in
    /// the detailed view, the ones closed).
    pub open_groups: Vec<usize>,
    /// Transient presentation-only toggles (`ui_toggles` from AppState).
    pub ui_toggles: std::sync::Arc<std::collections::HashSet<String>>,
    /// The chat's transcript view is Detailed: runs and thoughts open.
    pub detailed: bool,
    /// The block a jump just landed on, to mark it for a moment.
    pub flash: Option<usize>,
    /// A jump to a block of this message: where the block sits in it.
    pub reveal: Option<std::rc::Rc<Reveal>>,
    /// The waiting question card's "Other" field, when that card is here.
    pub other: Option<gpui::Entity<gpui::component::input::InputState>>,
    /// How long each finished thought here took, in seconds, by block.
    pub thoughts: Vec<(usize, u64)>,
    /// This reply's "Edited N files" card shows every file.
    pub edited_open: bool,
    /// The chat's sub-agents, for their rows' details.
    pub agents: std::rc::Rc<Vec<crate::state::AgentRun>>,
    /// Number of sub-agent calls in earlier messages of this chat.
    pub agent_offset: usize,
    /// Currently inspected sub-agent id in the Agents panel, if active.
    pub inspected_agent: Option<String>,
}

/// Where a jump's block is drawn, so the list can scroll it into view: the
/// window tops of its message and of its row, set as they are laid out.
pub struct Reveal {
    pub block: usize,
    pub root: std::cell::Cell<Option<gpui::Pixels>>,
    pub row: std::cell::Cell<Option<gpui::Pixels>>,
}

/// `el`, noting its top in `cell` as it is laid out.
fn noted(el: AnyElement, cell: std::rc::Rc<Reveal>, root: bool) -> AnyElement {
    div()
        .relative()
        .child(el)
        .child(
            gpui::canvas(
                move |bounds, _, _| {
                    let top = Some(bounds.origin.y);
                    if root {
                        cell.root.set(top);
                    } else {
                        cell.row.set(top);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .into_any_element()
}

/// The row for block `b`, noted when a jump looks for it.
pub(super) fn mark(el: AnyElement, b: usize, cx: &ItemContext) -> AnyElement {
    match &cx.reveal {
        Some(r) if r.block == b => noted(el, r.clone(), false),
        _ => el,
    }
}

pub fn render(m: &Message, cx: &ItemContext, t: &Theme) -> AnyElement {
    match m {
        Message::User {
            text,
            sent,
            attachments,
            ..
        } => user(text, *sent, attachments, cx, t),
        Message::Assistant { blocks } => match &cx.reveal {
            Some(r) => noted(assistant(blocks, cx, t), r.clone(), true),
            None => assistant(blocks, cx, t),
        },
        Message::TurnEnd { seconds, usage } => turn_footer(*seconds, usage.as_ref(), cx, t),
        Message::Notice { text, error } => notice(text, *error, cx, t),
    }
}

/// A small icon button under a message.
fn action_button(
    id: SharedString,
    path: &'static str,
    color: gpui::Hsla,
    tip: &'static str,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .role(gpui::accesskit::Role::Button)
        .aria_label(tip)
        .size(t.size.hit_sm)
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(path, t.size.icon_sm, color))
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
}

/// Under a finished reply: Copy, Good, Bad, Regenerate, then how long it
/// took. The newest reply's row is always shown; older ones on hover.
fn turn_footer(
    seconds: f32,
    usage: Option<&crate::context::Usage>,
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    let key = format!("{}-{}", cx.chat, cx.index);
    let group: SharedString = format!("turn-{key}").into();
    let copied = cx.copied.as_deref() == Some(format!("turn-{key}").as_str());
    let (chat, end) = (cx.chat, cx.index);
    let (s_copy, s_good, s_bad, s_again) = (
        cx.state.clone(),
        cx.state.clone(),
        cx.state.clone(),
        cx.state.clone(),
    );
    let rated = |r: Rating| cx.rating == Some(r);
    let pinned = cx.newest_turn || copied || cx.rating.is_some();
    let actions = div()
        .flex()
        .items_center()
        .gap(px(2.))
        .when(!pinned, |d| {
            d.invisible().group_hover(group.clone(), |s| s.visible())
        })
        .child(
            action_button(
                format!("copy-turn-{key}").into(),
                if copied {
                    "icons/check.svg"
                } else {
                    "icons/copy.svg"
                },
                if copied {
                    t.extra.success
                } else {
                    t.colors.text_muted
                },
                "Copy the reply",
                t,
            )
            .on_click(move |_, _, app| s_copy.update(app, |s, cx| s.copy_turn(chat, end, cx))),
        )
        .child(
            action_button(
                format!("good-{key}").into(),
                "icons/thumbs-up.svg",
                if rated(Rating::Good) {
                    t.extra.success
                } else {
                    t.colors.text_muted
                },
                "Good reply",
                t,
            )
            .when(rated(Rating::Good), |d| d.bg(t.extra.success_tint))
            .on_click(move |_, _, app| {
                s_good.update(app, |s, cx| s.rate(chat, end, Rating::Good, cx))
            }),
        )
        .child(
            action_button(
                format!("bad-{key}").into(),
                "icons/thumbs-down.svg",
                if rated(Rating::Bad) {
                    t.extra.danger
                } else {
                    t.colors.text_muted
                },
                "Bad reply",
                t,
            )
            .when(rated(Rating::Bad), |d| d.bg(t.extra.danger_tint))
            // Sending a rating is wired later (`stubs::FEEDBACK_UPLOAD`); the rating itself is kept.
            .on_click(move |_, _, app| {
                s_bad.update(app, |s, cx| s.rate(chat, end, Rating::Bad, cx))
            }),
        )
        .when(!cx.busy, |d| {
            d.child(
                action_button(
                    format!("again-{key}").into(),
                    "icons/rotate-ccw.svg",
                    t.colors.text_muted,
                    "Regenerate this reply",
                    t,
                )
                .on_click(move |_, _, app| {
                    s_again.update(app, |s, cx| s.regenerate(chat, end, cx))
                }),
            )
        });
    let memory = cx
        .memory
        .as_ref()
        .map(|m| memory_line(m, chat, cx.state.clone(), t));
    let row = div()
        .group(group)
        .flex()
        .items_center()
        .gap(t.space.sm)
        // The time first, so the actions that show on hover sit after it
        // rather than leave a gap before it.
        .child(
            div()
                .id(SharedString::from(format!("turn-took-{key}")))
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(if cx.after_error {
                    // A failed turn made no reply to count.
                    format!(
                        "Failed after {}",
                        took(seconds, None).trim_start_matches("Took ")
                    )
                } else {
                    took(seconds, usage)
                })
                .when_some(usage.map(read_and_wrote), |d, tip| {
                    d.tooltip(move |w, cx| {
                        gpui::component::tooltip::Tooltip::new(tip.clone()).build(w, cx)
                    })
                }),
        )
        .child(actions);
    match memory {
        Some(line) => div()
            .flex()
            .flex_col()
            .gap(t.space.xs)
            .child(row)
            .child(line)
            .into_any_element(),
        None => row.into_any_element(),
    }
}

/// Under the newest reply, when it changed the project's memory: what it
/// remembered or forgot, every fact in the tooltip, and Undo.
fn memory_line(
    m: &crate::state::MemoryChange,
    chat: usize,
    state: Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let quote = |f: &str| {
        let f: String = f.chars().take(60).collect();
        format!("“{f}”")
    };
    let text = match (m.added.first(), m.removed.first()) {
        (Some(a), _) if m.added.len() == 1 => format!("Remembered {}", quote(a)),
        (Some(_), _) => format!("Remembered {} things", m.added.len()),
        (None, Some(r)) if m.removed.len() == 1 => format!("Forgot {}", quote(r)),
        _ => format!("Forgot {} things", m.removed.len()),
    };
    let tip: SharedString = m
        .added
        .iter()
        .map(|f| format!("+ {f}"))
        .chain(m.removed.iter().map(|f| format!("− {f}")))
        .collect::<Vec<_>>()
        .join("\n")
        .into();
    div()
        .id(SharedString::from(format!("memory-line-{chat}")))
        .flex()
        .items_center()
        .gap(t.space.xs)
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(icon("icons/notebook-pen.svg", t.size.icon_xs, t.colors.coral))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text),
        )
        .child(div().text_color(t.colors.text_subtle).child("·"))
        .child(
            div()
                .id(SharedString::from(format!("memory-undo-{chat}")))
                .cursor_pointer()
                .text_color(t.colors.text)
                .hover(|st| st.underline())
                .child("Undo")
                .on_click(move |_, _, cx| state.update(cx, |s, cx| s.undo_memory(chat, cx))),
        )
        .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(tip.clone()).build(w, cx))
        .into_any_element()
}

/// Under a reply: how long it took and how long the reply is ("Took 4.8s ·
/// 1.2K-token reply"); "~" when the count is estimated.
fn took(seconds: f32, usage: Option<&crate::context::Usage>) -> String {
    let time = if seconds >= 60. {
        format!("Took {}m {:.0}s", (seconds / 60.).floor(), seconds % 60.)
    } else {
        format!("Took {seconds:.1}s")
    };
    match usage.filter(|u| u.output > 0) {
        Some(u) => format!(
            "{time} · {}{}-token reply",
            if u.estimated { "~" } else { "" },
            crate::context::short(u.output)
        ),
        None => time,
    }
}

/// The tooltip: what the model read to write it.
fn read_and_wrote(u: &crate::context::Usage) -> String {
    let read = u.input + u.cache_creation + u.cache_read;
    let mut s = format!(
        "The model read {} tokens of this chat and wrote {} (about {} words)",
        crate::context::short(read),
        crate::context::short(u.output),
        crate::context::short(u.output * 3 / 4)
    );
    if u.cache_read > 0 {
        s.push_str(&format!(
            " · {} of it from the cache",
            crate::context::short(u.cache_read)
        ));
    }
    if u.estimated {
        s.push_str(" · counted from the text, no model reported it");
    }
    s
}

/// Something the app says: Stop (with Continue on the last one), or an error
/// (wrapped in full, with Retry and Copy).
/// A notice that is not an error: an event in the chat's course (a goal
/// checked or met, the reply stopped, the context compacted). A quiet line,
/// as Claude's: a tinted mark, the event in a few strong words, then the rest.
fn event_line(
    text: &SharedString,
    interrupted: bool,
    key: &str,
    cx: &ItemContext,
    t: &Theme,
    link: impl Fn(String, &'static str, &'static str) -> gpui::Stateful<gpui::Div>,
) -> AnyElement {
    let words = text.as_ref();
    let (path, accent, tint) = if words.starts_with("Goal achieved") {
        ("icons/target.svg", t.extra.success, t.extra.success_tint)
    } else if words.starts_with("Goal") {
        ("icons/target.svg", t.colors.primary, t.colors.primary_tint)
    } else if interrupted {
        (
            "icons/circle-pause.svg",
            t.colors.text_muted,
            t.colors.hover,
        )
    } else if words.starts_with("Compact") {
        ("icons/layers.svg", t.colors.text_muted, t.colors.hover)
    } else {
        ("icons/info.svg", t.colors.text_muted, t.colors.hover)
    };
    // "Goal not met yet: what is missing" leads with its event.
    let (lead, rest) = match words.split_once(": ") {
        Some((lead, rest)) if lead.len() <= 32 => (format!("{lead}:"), Some(rest.to_string())),
        _ => (words.to_string(), None),
    };
    div()
        .flex()
        .items_start()
        .gap(t.space.sm)
        .py(t.space.xs)
        .text_size(t.text.body_sm)
        .child(
            div()
                .flex_none()
                .size(t.size.icon + px(4.))
                .rounded_full()
                .bg(tint)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(path, t.size.icon_xs, accent)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pt(px(1.))
                .line_height(relative(1.5))
                .text_color(t.colors.text_muted)
                .child(
                    gpui::StyledText::new(match &rest {
                        Some(r) => format!("{lead} {r}"),
                        None => lead.clone(),
                    })
                    .with_highlights([(
                        0..lead.len(),
                        gpui::HighlightStyle {
                            color: Some(t.colors.text_strong),
                            font_weight: Some(FontWeight::MEDIUM),
                            ..Default::default()
                        },
                    )]),
                ),
        )
        .when(interrupted && cx.last && cx.can_continue, |d| {
            d.child(
                link(format!("continue-{key}"), "Continue", "icons/play.svg").on_click(
                    |_, window, app| window.dispatch_action(Box::new(super::ContinueReply), app),
                ),
            )
        })
        // A reply cut off when KodAct closed: one press picks it up.
        .when(
            words == crate::state::CLOSED_MID_REPLY && cx.last && !cx.busy,
            |d| {
                let (state, chat) = (cx.state.clone(), cx.chat);
                d.child(
                    link(format!("resume-{key}"), "Resume", "icons/play.svg").on_click(
                        move |_, _, app| {
                            state.update(app, |s, cx| {
                                if s.current == Some(chat) {
                                    s.send("Continue where you left off.".into(), cx);
                                }
                            })
                        },
                    ),
                )
            },
        )
        .into_any_element()
}

fn notice(text: &SharedString, error: bool, cx: &ItemContext, t: &Theme) -> AnyElement {
    let key = format!("{}-{}", cx.chat, cx.index);
    let (fg, bg) = (t.extra.danger, t.extra.danger_tint);
    let interrupted = !error && text.as_ref() == crate::state::INTERRUPTED;
    let copy_key: SharedString = format!("notice-{key}").into();
    let copied = cx.copied.as_ref() == Some(&copy_key);
    let (state_copy, state_retry, to_copy) = (cx.state.clone(), cx.state.clone(), text.to_string());
    let (chat, at) = (cx.chat, cx.index);
    let hover = t.colors.hover;
    let link = |id: String, label: &'static str, path: &'static str| {
        div()
            .id(SharedString::from(id))
            .flex()
            .flex_none()
            .items_center()
            .gap(t.space.xs)
            .px(t.space.sm)
            .h(t.size.control - px(2.))
            .rounded(t.radius.button)
            .cursor_pointer()
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(fg)
            .hover(move |s| s.bg(hover))
            .child(icon(path, t.size.icon_xs, fg))
            .child(label)
    };
    if !error {
        return event_line(text, interrupted, &key, cx, t, link);
    }
    div()
        .flex()
        .items_start()
        .gap(t.space.sm)
        .px(t.space.md)
        .py(t.space.sm)
        .rounded(t.radius.card)
        .bg(bg)
        .text_color(fg)
        .child(div().pt(px(2.)).child(icon(
            if error {
                "icons/triangle-alert.svg"
            } else {
                "icons/circle-pause.svg"
            },
            t.size.icon_sm,
            fg,
        )))
        // A known error in words, the server's reply under it; else the
        // reply itself, wrapped rather than run off the card.
        .child(match crate::error_words::explain(text).filter(|_| error) {
            Some((headline, todo)) => div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .line_height(relative(1.45))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(headline))
                .child(div().text_color(t.colors.text).child(todo))
                .child({
                    let full = text.clone();
                    div()
                        .id(SharedString::from(format!("notice-raw-{key}")))
                        .mt(t.space.xs)
                        .line_clamp(2)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(text.clone())
                        .tooltip(move |w, cx| {
                            gpui::component::tooltip::Tooltip::new(full.clone()).build(w, cx)
                        })
                })
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .line_height(relative(1.45))
                .child(text.clone())
                .into_any_element(),
        })
        .when(error && cx.in_last_turn && !cx.busy, |d| {
            d.child(
                link(format!("retry-{key}"), "Retry", "icons/rotate-ccw.svg").on_click(
                    move |_, _, app| {
                        state_retry.update(app, |s, cx| {
                            let q = s
                                .chats
                                .iter()
                                .find(|c| c.id == chat)
                                .and_then(|c| crate::state::turn_question(&c.messages, at));
                            if let Some(q) = q {
                                s.retry(chat, q, cx);
                            }
                        })
                    },
                ),
            )
        })
        .when(error, |d| {
            d.child(
                link(
                    format!("copy-notice-{key}"),
                    if copied { "Copied" } else { "Copy" },
                    if copied {
                        "icons/check.svg"
                    } else {
                        "icons/copy.svg"
                    },
                )
                .on_click(move |_, _, app| {
                    app.write_to_clipboard(ClipboardItem::new_string(to_copy.clone()));
                    let key = copy_key.clone();
                    state_copy.update(app, |s, cx| s.mark_copied(key, cx));
                }),
            )
        })
        .into_any_element()
}

/// `just now`, `4 min ago`, `5 hours ago`, `3 days ago`.
fn sent_ago(sent: SystemTime) -> String {
    let secs = sent.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => match secs / 3600 {
            1 => "1 hour ago".into(),
            h => format!("{h} hours ago"),
        },
        _ => match secs / 86_400 {
            1 => "yesterday".into(),
            d => format!("{d} days ago"),
        },
    }
}

/// Your message: a card on the right. Hovering it shows when you sent it
/// and three actions under it: retry, edit, copy.
fn user(
    text: &SharedString,
    sent: SystemTime,
    attachments: &[crate::attach::Attachment],
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    let key = format!("{}-{}", cx.chat, cx.index);
    let group: SharedString = format!("user-{key}").into();
    let copy_key: SharedString = format!("msg-{key}").into();
    let copied = cx.copied.as_ref() == Some(&copy_key);
    let hover = t.colors.hover;
    let action = |name: &str, path: &'static str, tip_color: gpui::Hsla| {
        div()
            .id(SharedString::from(format!("{name}-{key}")))
            .size(t.size.control - px(4.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(t.radius.button)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(icon(path, t.size.icon_sm, tip_color))
    };
    let (state, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
    let (state_edit, state_copy) = (state.clone(), state.clone());
    let to_copy = text.to_string();
    let actions = div()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(2.))
        .when(!copied, |d| {
            d.invisible().group_hover(group.clone(), |s| s.visible())
        })
        .child(
            div()
                .pr(t.space.xs)
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(sent_ago(sent)),
        )
        .child(
            action("retry", "icons/rotate-ccw.svg", t.colors.text_muted)
                .on_click(move |_, _, app| state.update(app, |s, cx| s.retry(chat, msg, cx))),
        )
        .child(
            action("edit", "icons/pencil.svg", t.colors.text_muted).on_click(
                move |_, window, app| {
                    state_edit.update(app, |s, cx| s.edit(chat, msg, cx));
                    window.dispatch_action(Box::new(super::FocusComposer), app);
                },
            ),
        )
        .child(
            action(
                "copy",
                if copied {
                    "icons/check.svg"
                } else {
                    "icons/copy.svg"
                },
                if copied {
                    t.extra.success
                } else {
                    t.colors.text_muted
                },
            )
            .on_click(move |_, _, app| {
                app.write_to_clipboard(ClipboardItem::new_string(to_copy.clone()));
                let key = copy_key.clone();
                state_copy.update(app, |s, cx| s.mark_copied(key, cx));
            }),
        );
    div()
        .group(group)
        .flex()
        .flex_col()
        .items_end()
        .gap(px(2.))
        .children(super::attachments::sent(t, attachments, &key, &cx.state))
        // A message of only attachments has no text card.
        .when(!text.trim().is_empty(), |d| {
            d.child(
                // A soft filled bubble, no frame: your words, set apart by
                // their ground alone.
                div()
                    .max_w(relative(0.8))
                    .px(t.space.lg)
                    .py(t.space.sm + px(2.))
                    .bg(t.colors.surface)
                    .rounded(t.radius.card)
                    .text_size(t.size.chat_text)
                    .line_height(relative(t.size.line_height_chat))
                    .flex()
                    .flex_col()
                    .gap(t.space.sm)
                    .children(user_parts(text, &key, cx, t)),
            )
        })
        .child(actions)
        .when(!cx.versions.is_empty(), |d| {
            d.child(super::versions::render(
                t,
                &cx.state,
                cx.chat,
                cx.index,
                &cx.versions,
                cx.versions_open,
                cx.versions_leaving,
            ))
        })
        .into_any_element()
}

/// Your message as you typed it, as Claude shows it: plain text, line
/// breaks kept, only the mentions coloured. Nothing in it is read as
/// Markdown, so a pasted transcript is never taken for code.
fn user_parts(text: &SharedString, key: &str, cx: &ItemContext, t: &Theme) -> Vec<AnyElement> {
    let body = text.trim_matches('\n');
    if body.trim().is_empty() {
        return vec![];
    }
    let from = text.find(body).unwrap_or(0);
    let marks: Vec<_> = cx
        .marks
        .iter()
        .filter(|(m, _)| m.start >= from && m.end <= from + body.len())
        .map(|(m, k)| (m.start - from..m.end - from, *k))
        .collect();
    vec![markdown::selectable_plain(
        SharedString::from(format!("user-text-{key}")),
        &SharedString::from(body.to_string()),
        &marks,
        t,
    )]
}

fn assistant(blocks: &[Block], cx: &ItemContext, t: &Theme) -> AnyElement {
    // Answers in a row that say the same thing show as one row.
    let mut rows = vec![];
    // Calls in a row fold into one line (`work_group`).
    let mut work = super::work_group::runs(blocks, cx.busy && cx.last)
        .into_iter()
        .peekable();
    let mut b = 0;
    while b < blocks.len() {
        let (from, before) = (b, rows.len());
        let group = assistant_row(blocks, &mut b, &mut rows, &mut work, cx, t);
        // A jump's block inside the row just made: note the row. An open
        // run's steps note themselves, closer.
        if let Some(r) = &cx.reveal {
            let opened = group && (cx.detailed != cx.open_groups.contains(&from));
            if rows.len() > before && (from..b).contains(&r.block) && !opened {
                let last = rows.pop().expect("a row was pushed");
                rows.push(noted(last, r.clone(), false));
            }
        }
    }
    // A finished reply that changed files ends on what it changed.
    let edited = super::changed_files::changed(blocks);
    if !edited.is_empty() && !(cx.busy && cx.last) {
        rows.push(super::changed_files::render(&edited, cx.edited_open, cx, t));
    }
    // Rows inside a reply sit closer than paragraphs do.
    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .children(rows)
        .into_any_element()
}

/// The row starting at block `*b`, if it shows one; moves `*b` past what
/// it covers. True when it is a run of calls folded into one line.
fn assistant_row(
    blocks: &[Block],
    b: &mut usize,
    rows: &mut Vec<AnyElement>,
    work: &mut std::iter::Peekable<std::vec::IntoIter<std::ops::Range<usize>>>,
    cx: &ItemContext,
    t: &Theme,
) -> bool {
    let at = *b;
    let run = same_answers(blocks, at);
    if let Some(w) = work.next_if(|w| w.start == at) {
        *b = w.end;
        rows.push(super::work_group::render(blocks, w, cx, t));
        return true;
    }
    if denied_call(blocks, at) || folded_answer(blocks, at) || asked_call(blocks, at) {
        // A denied call never ran: its denial says all there is. An
        // allowed one shows once, its row carrying the answer.
        *b += 1;
    } else if let Some(p) = answer_for(blocks, at) {
        rows.push(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(self::block(blocks, at, cx, t))
                .child(approval_line(&blocks[p], cx, t))
                .into_any_element(),
        );
        *b += 1;
    } else if run > 1 {
        rows.push(answer_group(&blocks[at..at + run], blocks, at, cx, t));
        *b += run;
    } else {
        rows.push(self::block(blocks, at, cx, t));
        *b += 1;
    }
    false
}

/// The allowed answer about the call at `b`, when it names `b` as its row.
/// A question card stands on its own, so it is never folded into its call.
pub(super) fn answer_for(blocks: &[Block], b: usize) -> Option<usize> {
    if !matches!(blocks.get(b), Some(Block::Tool(_))) {
        return None;
    }
    blocks.iter().enumerate().skip(b + 1).find_map(|(i, blk)| {
        matches!(blk, Block::Permission { verdict: Verdict::Allowed | Verdict::AllowedAlways, info, .. } if info.row == Some(b) && info.questions.is_empty())
            .then_some(i)
    })
}

/// `b` is an AskUserQuestion call: its question card says all there is.
pub(super) fn asked_call(blocks: &[Block], b: usize) -> bool {
    matches!(blocks[b], Block::Tool(_))
        && blocks[b + 1..].iter().any(|blk| {
            matches!(blk, Block::Permission { info, .. } if info.row == Some(b) && !info.questions.is_empty())
        })
}

/// `b` is an allowed answer shown on its call's own row instead.
pub(super) fn folded_answer(blocks: &[Block], b: usize) -> bool {
    match &blocks[b] {
        Block::Permission {
            verdict: Verdict::Allowed | Verdict::AllowedAlways,
            info,
            ..
        } => info
            .row
            .is_some_and(|r| r < b && answer_for(blocks, r) == Some(b)),
        _ => false,
    }
}

/// Under a call's row: who allowed it, where and when.
pub(super) fn approval_line(answer: &Block, cx: &ItemContext, t: &Theme) -> AnyElement {
    let Block::Permission { verdict, info, .. } = answer else {
        return div().into_any_element();
    };
    let mut parts = vec![format!("{} {}", verdict_word(*verdict), decider(info))];
    parts.push(match &cx.place {
        Some((label, _)) => format!("on {label}"),
        None => "on this computer".into(),
    });
    parts.extend(info.at.map(super::versions::ago));
    div()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .pl(t.space.md)
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(icon(
            "icons/shield-check.svg",
            t.size.icon_xs,
            t.extra.success,
        ))
        // One line, within the column: a long rule ends in an ellipsis.
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(parts.join(" · ")),
        )
        .into_any_element()
}

/// `b` is a call whose question still waits on you.
pub(super) fn awaits_you(blocks: &[Block], b: usize) -> bool {
    blocks[b + 1..].iter().any(|blk| {
        matches!(blk, Block::Permission { verdict: Verdict::Pending, info, .. } if info.row == Some(b))
    })
}

/// `b` is a call whose question was denied (the question names it as its row).
pub(super) fn denied_call(blocks: &[Block], b: usize) -> bool {
    matches!(blocks[b], Block::Tool(_))
        && blocks[b + 1..].iter().any(|blk| {
            matches!(blk, Block::Permission { verdict: Verdict::Denied, info, .. } if info.row == Some(b))
        })
}

/// How many answered questions from `b` on say the same (same question,
/// same answer): 0 when `b` isn't one.
fn same_answers(blocks: &[Block], b: usize) -> usize {
    let key = |blk: &Block| match blk {
        Block::Permission {
            title,
            verdict,
            info,
            ..
        } if info.questions.is_empty()
            && matches!(
                verdict,
                Verdict::Allowed | Verdict::AllowedAlways | Verdict::Denied
            ) =>
        {
            Some((title.clone(), *verdict))
        }
        _ => None,
    };
    let Some(first) = blocks.get(b).and_then(key) else {
        return 0;
    };
    blocks[b..]
        .iter()
        .take_while(|blk| key(blk).as_ref() == Some(&first))
        .count()
}

/// `newest`: the reply's last block, the one still growing while it streams.
pub(super) fn block(blocks: &[Block], b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    let (block, newest) = (&blocks[b], b + 1 == blocks.len());
    match block {
        Block::Text(text) => markdown::render(
            text,
            t,
            &CopyContext {
                state: cx.state.clone(),
                key: format!("{}-{}-{b}", cx.chat, cx.index),
                copied: cx.copied.clone(),
                scale: cx.scale,
                sources: cx.sources.clone(),
                zooms: cx.zooms.clone(),
                streaming: cx.busy && cx.last && newest,
            },
        ),
        Block::Thinking { text, open } => thinking(text, *open != cx.detailed, newest, b, cx, t),
        Block::Tool(call) => tool(call, b, blocks, cx, t),
        Block::Permission { verdict, info, .. } if !info.questions.is_empty() => {
            super::question_card::render(*verdict, info, b, cx, t)
        }
        Block::Permission {
            title,
            detail,
            verdict,
            info,
        } => permission(title, detail, *verdict, info, blocks, b, cx, t),
        Block::File(file) => {
            let key = format!("{}-{}-{b}", cx.chat, cx.index);
            let view = super::file_card::DiagramView {
                zoom: cx
                    .zooms
                    .get(&super::file_card::zoom_key(&key))
                    .copied()
                    .unwrap_or(1.),
                text_open: cx.sources.contains(&super::file_card::text_key(&key)),
            };
            super::file_card::render(
                file,
                &key,
                cx.state.clone(),
                cx.open_doc.as_ref() == Some(file),
                cx.scale,
                Some(view),
                t,
            )
        }
    }
}

fn toggle(
    cx: &ItemContext,
    b: usize,
) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    let (state, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
    move |_, _, app| state.update(app, |s, cx| s.toggle_block(chat, msg, b, cx))
}

fn chevron(open: bool, t: &Theme) -> gpui::Svg {
    icon(
        if open {
            "icons/chevron-down.svg"
        } else {
            "icons/chevron-right.svg"
        },
        t.size.icon_sm,
        t.colors.text_subtle,
    )
}

/// The model's reasoning, folded to one quiet line: "Thinking" (a soft
/// pulsing sparkle) while it runs, "Thought for 12s" after. Opened, it is
/// set small and muted, as Markdown, under a rule, and a long one scrolls
/// in its own well rather than pushing the reply away.
fn thinking(
    text: &SharedString,
    open: bool,
    newest: bool,
    b: usize,
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    let streaming = cx.busy && cx.last && newest;
    let strong = t.colors.text;
    let label = match (streaming, cx.thoughts.iter().find(|(i, _)| *i == b)) {
        (true, _) => "Thinking".to_string(),
        (false, Some((_, s))) if *s >= 1 => {
            format!("Thought for {}", super::chat_view::elapsed(*s))
        }
        _ => "Thought".to_string(),
    };
    let key = format!("{}-{}-{b}", cx.chat, cx.index);
    let long = text.len() > 900;
    let (st_click, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
    let header_inner = div()
        .flex()
        .items_center()
        .gap(t.space.xs + px(2.))
        .min_h(t.size.quiet_line)
        .px(px(2.))
        .text_size(t.text.body_sm)
        .text_color(t.colors.text_muted)
        .hover(move |s| s.text_color(strong))
        .when(streaming, |d| {
            d.child(super::turning_logo(
                f32::from(t.size.icon_sm) * 1.15,
                SharedString::from(format!("think-strike-{key}")),
            ))
        })
        .when(!streaming, |d| {
            d.child(icon(
                "icons/lightbulb.svg",
                t.size.icon_xs,
                t.colors.text_subtle,
            ))
        })
        .child(label.clone())
        .child(icon(
            if open {
                "icons/chevron-down.svg"
            } else {
                "icons/chevron-right.svg"
            },
            t.size.icon_xs,
            t.colors.text_subtle,
        ));

    div()
        .flex()
        .flex_col()
        .child(
            super::button::KeyRow::new(
                SharedString::from(format!("think-{key}")),
                SharedString::from(format!(
                    "{label}, {}",
                    if open { "expanded" } else { "collapsed" }
                )),
                header_inner,
            )
            .expanded(open)
            .on_activate(move |_, app| {
                st_click.update(app, |s, cx| s.toggle_block(chat, msg, b, cx));
            }),
        )
        .when(open, |d| {
            d.child(
                div()
                    .id(SharedString::from(format!("think-body-{key}")))
                    .mt(px(2.))
                    .mb(t.space.xs)
                    .ml(t.size.icon_xs / 2.)
                    .pl(t.space.md)
                    .border_l_2()
                    .border_color(t.colors.border)
                    .max_h(t.size.thinking_max)
                    .overflow_y_scroll()
                    .when(long, super::hold_wheel)
                    .child(markdown::render_as(
                        text,
                        t,
                        &CopyContext {
                            state: cx.state.clone(),
                            key: format!("think-{key}"),
                            copied: cx.copied.clone(),
                            scale: cx.scale,
                            sources: cx.sources.clone(),
                            zooms: cx.zooms.clone(),
                            streaming: false,
                        },
                        markdown::Look::Aside,
                    )),
            )
        })
        .into_any_element()
}

/// A change, a line per row: added and removed lines tinted, coloured by
/// the file's language.
fn diff_lines(diff: &[crate::state::DiffLine], lang: &str, t: &Theme) -> gpui::Div {
    div()
        .py(t.space.xs)
        .font_family(t.font_mono.clone())
        .text_size(t.text.caption + px(0.5))
        .line_height(relative(1.65))
        .children(diff.iter().map(|l| {
            let (bg, sign, fg) = match l.kind {
                DiffKind::Add => (Some(t.extra.diff_add), "+", t.extra.success),
                DiffKind::Del => (Some(t.extra.diff_del), "−", t.extra.danger),
                DiffKind::Same => (None, " ", t.colors.text_subtle),
            };
            div()
                .flex()
                .px(t.space.md)
                .when_some(bg, |d, bg| d.bg(bg))
                .child(div().w(t.space.lg).flex_none().text_color(fg).child(sign))
                .child(div().whitespace_nowrap().child(markdown::code_text(
                    if l.text.is_empty() { " " } else { &l.text },
                    lang,
                    t,
                )))
        }))
}

fn status_icon(status: ToolStatus, t: &Theme) -> AnyElement {
    match status {
        ToolStatus::Running => icon("icons/loader-circle.svg", t.size.icon_sm, t.extra.live)
            .with_animation(
                "tool-running",
                Animation::new(Duration::from_millis(900)).repeat(),
                |svg, d| svg.with_transformation(Transformation::rotate(percentage(d))),
            )
            .into_any_element(),
        ToolStatus::Done => {
            icon("icons/circle-check.svg", t.size.icon_sm, t.extra.success).into_any_element()
        }
        ToolStatus::Failed => {
            icon("icons/circle-x.svg", t.size.icon_sm, t.extra.danger).into_any_element()
        }
        ToolStatus::Declined => icon(
            "icons/circle-slash.svg",
            t.size.icon_sm,
            t.colors.text_subtle,
        )
        .into_any_element(),
        // Stopped mid-way: neither failed nor done; it may have taken effect.
        ToolStatus::Interrupted => {
            icon("icons/circle-pause.svg", t.size.icon_sm, t.extra.warning).into_any_element()
        }
    }
}

/// Where a command runs: "on SSH: box" in the accent for a remote chat,
/// "on this computer" quietly otherwise.
pub fn env_badge(place: &Option<(SharedString, &'static str)>, t: &Theme) -> AnyElement {
    let (label, glyph, fg, bg) = match place {
        Some((label, glyph)) => (
            format!("on {label}"),
            *glyph,
            t.colors.primary,
            t.colors.primary_tint,
        ),
        None => (
            "on this computer".to_string(),
            "icons/laptop.svg",
            t.colors.text_muted,
            t.extra.inset,
        ),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.space.xs)
        .h(px(18.))
        .px(t.space.sm)
        .max_w(px(200.))
        .rounded(t.radius.pill)
        .bg(bg)
        .text_size(t.text.caption)
        .font_weight(FontWeight::NORMAL)
        .text_color(fg)
        .child(icon(glyph, t.size.icon_xs, fg))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
        .into_any_element()
}

/// The first line of a failed call's error, for its header.
/// A call stopped mid-way gives its reason too: it may have taken effect.
pub(super) fn failure_line(call: &ToolCall) -> Option<SharedString> {
    if !matches!(call.status, ToolStatus::Failed | ToolStatus::Interrupted) {
        return None;
    }
    call.output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| SharedString::from(l.to_string()))
}

/// The call has something to open: a command's terminal, a change, output.
pub(super) fn has_body(call: &ToolCall) -> bool {
    call.kind == ToolKind::Bash
        || !call.diff.is_empty()
        || !call.output.is_empty()
        || !call.images.is_empty()
}

/// A change's size as two tinted chips, "+76" and "−90", as Claude Code
/// shows it; a side with nothing to count is left out.
pub(super) fn diff_counts(adds: usize, dels: usize, t: &Theme) -> gpui::Div {
    let chip = |text: String, fg: gpui::Hsla, bg: gpui::Hsla| {
        div()
            .px(t.space.xs)
            .rounded(t.radius.small)
            .bg(bg)
            .text_color(fg)
            .child(text)
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.space.xs)
        .font_family(t.font_mono.clone())
        .text_size(t.text.caption)
        .when(adds > 0, |d| {
            d.child(chip(
                format!("+{adds}"),
                t.extra.success,
                t.extra.success_tint,
            ))
        })
        .when(dels > 0, |d| {
            d.child(chip(
                format!("−{dels}"),
                t.extra.danger,
                t.extra.danger_tint,
            ))
        })
}

/// The pictures calls returned, as thumbnails pushed to the end of their
/// line: three at most, then how many more. The line opens to them.
pub(super) fn thumbs<'a>(
    images: impl Iterator<Item = &'a ToolImage>,
    t: &Theme,
) -> Option<gpui::Div> {
    const SHOWN: usize = 3;
    let all: Vec<&ToolImage> = images.collect();
    if all.is_empty() {
        return None;
    }
    Some(
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.space.xs)
            .pl(t.space.sm)
            .children(all.iter().take(SHOWN).map(|i| {
                div()
                    .flex_none()
                    .w(t.size.thumb_w)
                    .h(t.size.thumb_h)
                    .rounded(t.radius.small)
                    .overflow_hidden()
                    .border_1()
                    .border_color(t.colors.border)
                    .bg(t.colors.surface)
                    .child(
                        img(i.image.clone())
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
            }))
            .when(all.len() > SHOWN, |d| {
                d.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(format!("+{}", all.len() - SHOWN)),
                )
            }),
    )
}

/// The pictures a call returned, opened: each whole, letterboxed in a frame
/// of one size (the path bar above already names a file's).
fn pictures(call: &ToolCall, t: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_wrap()
        .gap(t.space.md)
        .p(t.space.md)
        .children(call.images.iter().map(|i| {
            div()
                .w(t.size.tool_image)
                .h(t.size.tool_image * 0.625)
                .rounded(t.radius.button)
                .overflow_hidden()
                .bg(t.colors.canvas)
                .child(
                    img(i.image.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
        }))
}

/// Maximum lines rendered initially for tool output / diffs before "Show more".
const OUTPUT_FOLD: usize = 10;

/// What a call's line names: a file by up to its last two path components
/// (`src/app.ts`, `…/dir/file.rs`), anything else as its row says it.
fn target_label(call: &ToolCall) -> SharedString {
    match call.kind {
        ToolKind::Read | ToolKind::Edit | ToolKind::Write => {
            let t = call.target.as_ref();
            let parts: Vec<&str> = t.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
            if parts.len() <= 2 {
                call.target.clone()
            } else {
                format!("…/{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]).into()
            }
        }
        _ => call.shown_target(),
    }
}

pub(super) fn is_problem(status: ToolStatus) -> bool {
    matches!(status, ToolStatus::Failed | ToolStatus::Interrupted)
}

pub(super) fn prob_fold_key(chat: usize, msg: usize, b: usize) -> String {
    format!("prob-fold-{chat}-{msg}-{b}")
}

/// Effective open state of a tool block: errors/warnings start expanded by
/// default (never hidden behind a collapse) until toggled via `ui_toggles`.
pub(super) fn tool_is_open(call: &ToolCall, b: usize, cx: &ItemContext) -> bool {
    if is_problem(call.status) {
        !cx.ui_toggles.contains(&prob_fold_key(cx.chat, cx.index, b))
    } else {
        call.open
    }
}

/// Concise single-line outcome suffix (e.g. `"42 lines"`, `"exit 0"`, `"failed"`).
pub(super) fn tool_outcome(call: &ToolCall) -> Option<String> {
    match call.status {
        ToolStatus::Running => None,
        ToolStatus::Declined => {
            if call.output.starts_with(crate::state::AGENT_STOPPED) {
                None
            } else {
                Some("skipped".into())
            }
        }
        ToolStatus::Interrupted => Some("stopped".into()),
        ToolStatus::Failed => {
            if call.kind == ToolKind::Bash {
                Some("failed".into())
            } else if call.output.trim().is_empty() {
                Some("failed".into())
            } else {
                None
            }
        }
        ToolStatus::Done => {
            let trimmed = call.output.trim();
            let lines = if trimmed.is_empty() {
                0
            } else {
                trimmed.lines().count()
            };
            match call.kind {
                ToolKind::Read if lines > 0 => Some(format!(
                    "{lines} line{}",
                    if lines == 1 { "" } else { "s" }
                )),
                ToolKind::Bash if lines > 0 => Some(format!(
                    "exit 0 · {lines} line{}",
                    if lines == 1 { "" } else { "s" }
                )),
                ToolKind::Bash => Some("exit 0".into()),
                ToolKind::Search => {
                    let hits = trimmed.lines().filter(|l| !l.trim().is_empty()).count();
                    if hits > 0 {
                        Some(format!("{hits} match{}", if hits == 1 { "" } else { "es" }))
                    } else {
                        Some("no matches".into())
                    }
                }
                ToolKind::Edit | ToolKind::Write if call.diff.is_empty() => Some("saved".into()),
                ToolKind::Edit | ToolKind::Write => None,
                _ if lines > 0 => Some(format!(
                    "{lines} line{}",
                    if lines == 1 { "" } else { "s" }
                )),
                _ => None,
            }
        }
    }
}

/// Keyboard-accessible "Show more / Show less" toggle row backed by `ui_toggles`.
pub(super) fn show_more_toggle(
    id: String,
    toggle_key: String,
    more_label: String,
    chat: Option<usize>,
    expanded: bool,
    state: Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let label = if expanded {
        "Show less".to_string()
    } else {
        more_label
    };
    let hover = t.colors.hover;
    let accent = t.colors.primary;
    let inner = div()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .px(t.space.md)
        .h(px(26.))
        .border_t_1()
        .border_color(t.colors.border)
        .bg(t.extra.sunk)
        .hover(move |s| s.bg(hover))
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(accent)
        .child(icon(
            if expanded {
                "icons/chevron-up.svg"
            } else {
                "icons/chevron-down.svg"
            },
            t.size.icon_xs,
            accent,
        ))
        .child(label.clone());
    super::button::KeyRow::new(
        SharedString::from(id),
        SharedString::from(format!(
            "{label}, {}",
            if expanded { "expanded" } else { "collapsed" }
        )),
        inner,
    )
    .w_full()
    .radius(px(0.))
    .expanded(expanded)
    .on_activate(move |_, app| {
        state.update(app, |s, cx| s.toggle_ui(toggle_key.clone(), chat, cx));
    })
    .into_any_element()
}

/// A tool call's one quiet line: `icon + action + target + outcome`. Clicking
/// (or pressing Enter/Space) toggles details; errors stay expanded by default.
pub(super) fn tool_header(
    call: &ToolCall,
    b: usize,
    blocks: &[Block],
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    if crate::state::is_agent_tool(&call.target) {
        return super::agent_row::render(call, b, blocks, cx, t);
    }
    let strong = t.colors.text;
    let failed = call.status == ToolStatus::Failed;
    let unknown = call.status == ToolStatus::Interrupted;
    let adds = call
        .diff
        .iter()
        .filter(|l| l.kind == DiffKind::Add && !failed && !unknown)
        .count();
    let dels = call
        .diff
        .iter()
        .filter(|l| l.kind == DiffKind::Del && !failed && !unknown)
        .count();
    let shell = call.kind == ToolKind::Bash;
    let has_body = has_body(call);
    let shown_open = has_body && tool_is_open(call, b, cx);
    let why = failure_line(call).filter(|_| !shell);
    let outcome = tool_outcome(call);
    let waiting = call.status == ToolStatus::Running && awaits_you(blocks, b);
    let verb = if call.status == ToolStatus::Declined
        && call.output.starts_with(crate::state::AGENT_STOPPED)
    {
        "Stopped"
    } else {
        call.kind.verb(call.status)
    };
    let mono_target = matches!(
        call.kind,
        ToolKind::Read | ToolKind::Edit | ToolKind::Write | ToolKind::Search | ToolKind::Bash
    );
    let one_line = |d: gpui::Div| {
        d.min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
    };
    let key = format!("{}-{}-{b}", cx.chat, cx.index);
    let inner = div()
        .flex()
        .items_center()
        .gap(t.space.xs + px(2.))
        .min_h(t.size.quiet_line)
        .px(px(2.))
        .text_size(t.text.body_sm)
        .text_color(t.colors.text_muted)
        .when(has_body, |d| d.hover(move |s| s.text_color(strong)))
        .child(match call.status {
            _ if waiting => {
                icon("icons/hand.svg", t.size.icon_xs, t.extra.accent).into_any_element()
            }
            ToolStatus::Done => icon(call.kind.icon(), t.size.icon_xs, t.colors.text_muted)
                .into_any_element(),
            ToolStatus::Failed => {
                let full = SharedString::from(call.output.trim().to_string());
                div()
                    .id(SharedString::from(format!("tool-why-{key}")))
                    .flex_none()
                    .child(status_icon(call.status, t))
                    .when(!full.is_empty(), |d| {
                        d.tooltip(move |w, cx| {
                            gpui::component::tooltip::Tooltip::new(full.clone()).build(w, cx)
                        })
                    })
                    .into_any_element()
            }
            s => status_icon(s, t),
        })
        .map(|d| match (&call.note, shell) {
            (Some(note), true) => d
                .child(
                    div()
                        .flex_none()
                        .text_color(if failed {
                            t.extra.danger
                        } else if unknown {
                            t.extra.warning
                        } else {
                            t.colors.text_muted
                        })
                        .child(if failed || unknown { verb } else { "Bash" }),
                )
                .child(
                    one_line(div())
                        .text_color(t.colors.text)
                        .font_weight(FontWeight::MEDIUM)
                        .child(note.clone()),
                ),
            _ => d
                .child(
                    div()
                        .flex_none()
                        .when(failed, |d| d.text_color(t.extra.danger))
                        .when(unknown, |d| d.text_color(t.extra.warning))
                        .child(verb),
                )
                .child(
                    one_line(div())
                        .when(call.note.is_some() || why.is_some(), |d| {
                            d.flex_none().max_w(px(260.))
                        })
                        .when(mono_target, |d| d.font_family(t.font_mono.clone()))
                        .text_size(if mono_target {
                            t.text.caption + px(0.5)
                        } else {
                            t.text.body_sm
                        })
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text)
                        .child(target_label(call)),
                )
                .children(
                    call.note
                        .clone()
                        .filter(|_| why.is_none() && outcome.is_none())
                        .map(|n| one_line(div()).text_color(t.colors.text_subtle).child(n)),
                ),
        })
        .children(why.as_ref().map(|w| {
            one_line(div())
                .text_color(if unknown {
                    t.extra.warning
                } else {
                    t.extra.danger
                })
                .child(format!("· {w}"))
        }))
        .children(outcome.as_ref().map(|out| {
            div()
                .flex_none()
                .text_size(t.text.caption)
                .text_color(if failed {
                    t.extra.danger
                } else if unknown {
                    t.extra.warning
                } else {
                    t.colors.text_subtle
                })
                .child(format!("· {out}"))
        }))
        .when(adds + dels > 0, |d| d.child(diff_counts(adds, dels, t)))
        .when(has_body, |d| {
            d.child(icon(
                if shown_open {
                    "icons/chevron-down.svg"
                } else {
                    "icons/chevron-right.svg"
                },
                t.size.icon_xs,
                t.colors.text_subtle,
            ))
        })
        .when_some(
            thumbs(call.images.iter(), t).filter(|_| !shown_open),
            |d, th| d.child(div().flex_1()).child(th),
        );

    let aria = format!(
        "{verb} {}{}{}",
        call.shown_target(),
        outcome
            .as_deref()
            .or_else(|| why.as_deref())
            .map(|o| format!(", {o}"))
            .unwrap_or_default(),
        if !has_body {
            ""
        } else if shown_open {
            ", expanded"
        } else {
            ", collapsed"
        }
    );
    let mut row = super::button::KeyRow::new(SharedString::from(format!("tool-{key}")), aria, inner)
        .w_full();
    if has_body {
        let (st, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
        let problem = is_problem(call.status);
        let fold_key = prob_fold_key(chat, msg, b);
        row = row.expanded(shown_open).on_activate(move |_, app| {
            st.update(app, |s, cx| {
                if problem {
                    s.toggle_ui(fold_key.clone(), Some(chat), cx);
                } else {
                    s.toggle_block(chat, msg, b, cx);
                }
            });
        });
    }
    row.into_any_element()
}

/// What a tool call's line opens to, in one quiet frame: a file change
/// headed by its full path (with Keep and Revert), a command's terminal
/// block, or what went in and came out. A failed/interrupted call starts
/// expanded with its error/warning first, and long outputs (>10 lines)
/// collapse cleanly behind a "Show more" toggle without losing text.
pub(super) fn tool_body(
    call: &ToolCall,
    b: usize,
    cx: &ItemContext,
    t: &Theme,
) -> Option<AnyElement> {
    if !(tool_is_open(call, b, cx) && has_body(call)) {
        return None;
    }
    let frame = || {
        div()
            .mt(t.space.xs)
            .mb(t.space.xs)
            .rounded(t.radius.button)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.colors.surface)
            .overflow_hidden()
            .flex()
            .flex_col()
    };
    let key = format!("{}-{}-{b}", cx.chat, cx.index);
    let lang = markdown::lang_of(&call.target);
    if call.kind == ToolKind::Bash {
        let at = terminal::Where {
            state: cx.state.clone(),
            place: cx.place.clone(),
            chat: cx.chat,
            msg: cx.index,
            block: b,
            copied: cx.copied.clone(),
            busy: cx.busy,
        };
        return Some(
            frame()
                .child(terminal::block(call, &at, t))
                .into_any_element(),
        );
    }
    let file = matches!(call.kind, ToolKind::Read | ToolKind::Edit | ToolKind::Write);
    let path_bar = file.then(|| {
        div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .px(t.space.md)
            .min_h(t.size.row + px(4.))
            .border_b_1()
            .border_color(t.colors.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child(call.target.clone()),
            )
            .children(review_controls(call, b, cx, t))
    });
    if !call.images.is_empty() {
        return Some(
            frame()
                .children(path_bar)
                .child(pictures(call, t))
                .into_any_element(),
        );
    }
    let diff_more_key = format!("diff-more-{key}");
    let diff_expanded = cx.ui_toggles.contains(&diff_more_key);
    let diff_truncated = call.diff.len() > OUTPUT_FOLD;
    let shown_diff = if diff_truncated && !diff_expanded {
        &call.diff[..OUTPUT_FOLD]
    } else {
        call.diff.as_slice()
    };

    let out_more_key = format!("out-more-{key}");
    let out_expanded = cx.ui_toggles.contains(&out_more_key);

    if matches!(call.status, ToolStatus::Failed | ToolStatus::Interrupted) {
        let stopped = call.status == ToolStatus::Interrupted;
        let err_trunc = super::truncate_lines(&call.output, OUTPUT_FOLD);
        let err_text = if err_trunc.truncated && !out_expanded {
            err_trunc.shown.to_string()
        } else {
            call.output.trim().to_string()
        };
        return Some(
            frame()
                .border_color(if stopped {
                    t.extra.warning.opacity(0.45)
                } else {
                    t.extra.danger.opacity(0.45)
                })
                .children(path_bar)
                .when(!call.output.trim().is_empty(), |d| {
                    d.child(
                        div()
                            .px(t.space.md)
                            .py(t.space.sm)
                            .bg(if stopped {
                                t.extra.warning_tint
                            } else {
                                t.extra.danger_tint
                            })
                            .font_family(t.font_mono.clone())
                            .text_size(t.text.caption + px(0.5))
                            .line_height(relative(1.45))
                            .text_color(t.colors.text)
                            .child(err_text),
                    )
                    .when(err_trunc.truncated, |d| {
                        let more_lbl = if err_trunc.hidden_lines > 0 {
                            format!("Show {} more lines", err_trunc.hidden_lines)
                        } else {
                            "Show full error".to_string()
                        };
                        d.child(show_more_toggle(
                            format!("err-more-{key}"),
                            out_more_key.clone(),
                            more_lbl,
                            Some(cx.chat),
                            out_expanded,
                            cx.state.clone(),
                            t,
                        ))
                    })
                })
                .when(!call.diff.is_empty(), |d| {
                    d.child(
                        div()
                            .px(t.space.md)
                            .pt(t.space.sm)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(if stopped {
                                "May or may not have been applied"
                            } else {
                                "Not applied"
                            }),
                    )
                    .child(diff_lines(shown_diff, lang, t).opacity(0.75))
                    .when(diff_truncated, |d| {
                        d.child(show_more_toggle(
                            format!("diff-more-btn-{key}"),
                            diff_more_key.clone(),
                            format!("Show {} more lines", call.diff.len() - OUTPUT_FOLD),
                            Some(cx.chat),
                            diff_expanded,
                            cx.state.clone(),
                            t,
                        ))
                    })
                })
                .into_any_element(),
        );
    }
    Some(if !call.diff.is_empty() {
        frame()
            .children(path_bar)
            .child(diff_lines(shown_diff, lang, t))
            .when(diff_truncated, |d| {
                d.child(show_more_toggle(
                    format!("diff-more-btn-{key}"),
                    diff_more_key,
                    format!("Show {} more lines", call.diff.len() - OUTPUT_FOLD),
                    Some(cx.chat),
                    diff_expanded,
                    cx.state.clone(),
                    t,
                ))
            })
            .into_any_element()
    } else {
        let out_trunc = super::truncate_lines(&call.output, OUTPUT_FOLD);
        let out_text: SharedString = if out_trunc.truncated && !out_expanded {
            out_trunc.shown.to_string().into()
        } else {
            call.output.clone()
        };
        let out_lang = if call.kind == ToolKind::Read { lang } else { "" };
        frame()
            .children(path_bar)
            .child(io_row("IN", call.target.clone(), "", t))
            .when(!call.output.trim().is_empty(), |d| {
                d.child(io_row("OUT", out_text, out_lang, t))
                    .when(out_trunc.truncated, |d| {
                        let more_lbl = if out_trunc.hidden_lines > 0 {
                            format!("Show {} more lines", out_trunc.hidden_lines)
                        } else {
                            "Show full output".to_string()
                        };
                        d.child(show_more_toggle(
                            format!("out-more-btn-{key}"),
                            out_more_key,
                            more_lbl,
                            Some(cx.chat),
                            out_expanded,
                            cx.state.clone(),
                            t,
                        ))
                    })
            })
            .into_any_element()
    })
}

/// A row a jump landed on: an accent ring that fades, so the eye finds it.
pub(super) fn flash(row: gpui::Div, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    if cx.flash != Some(b) {
        return row.into_any_element();
    }
    let ring = t.extra.accent;
    // The ring sits just outside the row, so nothing moves when it shows.
    row.rounded(t.radius.button)
        .mx(-t.space.sm)
        .px(t.space.sm - px(2.))
        .border_2()
        .border_color(ring)
        .with_animation(
            SharedString::from(format!("flash-{}-{}-{b}", cx.chat, cx.index)),
            Animation::new(Duration::from_millis(1600)).with_easing(gpui::ease_in_out),
            move |d, delta| d.border_color(ring.opacity(1. - delta)),
        )
        .into_any_element()
}

fn tool(call: &ToolCall, b: usize, blocks: &[Block], cx: &ItemContext, t: &Theme) -> AnyElement {
    // The task list is a card, never a plain tool row: the reply's first
    // `TodoWrite` holds it, showing the latest list; later ones add nothing.
    if super::todo_card::is_todo(call) {
        return match super::todo_card::slot(blocks, b) {
            Some(latest) => {
                let list = match &blocks[latest] {
                    Block::Tool(c) => super::todo_card::TodoList::from_call(c),
                    _ => super::todo_card::TodoList::from_call(call),
                };
                let key = format!("{}-{}", cx.chat, cx.index);
                flash(
                    div().child(super::todo_card::render(&list, &key, &cx.state, t)),
                    b,
                    cx,
                    t,
                )
            }
            None => div().into_any_element(),
        };
    }
    let is_agent = crate::state::is_agent_tool(&call.target);
    // A lone call is a quiet line, as one in a run is; its body opens under it
    // (sub-agent cards open in the right-side Agents inspector instead).
    let row = div()
        .flex()
        .flex_col()
        .child(tool_header(call, b, blocks, cx, t))
        .when(!is_agent, |d| d.children(tool_body(call, b, cx, t)));
    flash(row, b, cx, t)
}

/// A finished file change's review: Keep and Revert… while undecided; then
/// "Kept", or "Reverted" with Re-apply (which KodAct CLI can't do yet).
fn review_controls(call: &ToolCall, b: usize, cx: &ItemContext, t: &Theme) -> Option<AnyElement> {
    if !matches!(call.kind, ToolKind::Edit | ToolKind::Write) || call.status != ToolStatus::Done {
        return None;
    }
    let review = file_review(call, b, cx, t);
    // A web page KodAct wrote here: a look at it in the side panel's browser.
    let page = matches!(
        markdown::lang_of(&call.target)
            .to_ascii_lowercase()
            .as_str(),
        "html" | "htm"
    );
    if !page || cx.place.is_some() || call.review == Review::Reverted {
        return review;
    }
    let (state, chat, path) = (cx.state.clone(), cx.chat, call.target.to_string());
    let hover = t.colors.hover;
    let key = format!("{}-{}-{b}", cx.chat, cx.index);
    let preview = div()
        .id(SharedString::from(format!("preview-{key}")))
        .flex()
        .items_center()
        .gap(px(3.))
        .px(t.space.sm)
        .h(t.size.control - px(4.))
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.primary)
        .child(icon("icons/globe.svg", t.size.icon_xs, t.colors.primary))
        .child("Preview")
        .tooltip(|window, cx| {
            gpui::component::tooltip::Tooltip::new("Open the page in the side panel's browser")
                .build(window, cx)
        })
        .on_click(move |_, _, app| {
            app.stop_propagation();
            state.update(app, |s, cx| s.preview_page(chat, &path, cx));
        });
    Some(
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(preview)
            .children(review)
            .into_any_element(),
    )
}

/// Keep or revert a file change, or what was decided.
fn file_review(call: &ToolCall, b: usize, cx: &ItemContext, t: &Theme) -> Option<AnyElement> {
    let key = format!("{}-{}-{b}", cx.chat, cx.index);
    let hover = t.colors.hover;
    let small = |id: String, label: &'static str, path: &'static str, color: gpui::Hsla| {
        div()
            .id(SharedString::from(id))
            .flex()
            .items_center()
            .gap(px(3.))
            .px(t.space.sm)
            .h(t.size.control - px(4.))
            .rounded(t.radius.button)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(icon(path, t.size.icon_xs, color))
            .child(label)
    };
    let (state, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
    let el = match call.review {
        Review::Open if !cx.busy => {
            let keep = state.clone();
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .child(
                    small(
                        format!("keep-{key}"),
                        "Keep",
                        "icons/check.svg",
                        t.colors.text_muted,
                    )
                    .on_click(move |_, _, app| {
                        app.stop_propagation();
                        keep.update(app, |s, cx| s.keep_change(chat, msg, b, cx));
                    }),
                )
                .child(
                    small(
                        format!("revert-{key}"),
                        "Revert…",
                        "icons/rotate-ccw.svg",
                        t.colors.text_muted,
                    )
                    .tooltip(|window, cx| {
                        gpui::component::tooltip::Tooltip::new(
                            "Put the files back as they were before this message",
                        )
                        .build(window, cx)
                    })
                    .on_click(move |_, _, app| {
                        app.stop_propagation();
                        state.update(app, |s, cx| s.ask_revert(chat, msg, cx));
                    }),
                )
                .into_any_element()
        }
        Review::Open => return None,
        Review::Kept => div()
            .text_size(t.text.caption)
            .text_color(t.colors.text_subtle)
            .child("Kept")
            .into_any_element(),
        Review::Reverted => div()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(theme::pill(
                t,
                t.colors.hover,
                t.colors.text_muted,
                "Reverted",
            ))
            .into_any_element(),
    };
    Some(el)
}

/// KodAct's tool I/O row: a small label, then the text in a quiet well.
fn io_row(label: &'static str, text: SharedString, lang: &str, t: &Theme) -> impl IntoElement {
    div()
        .flex()
        .gap(t.space.md)
        .px(t.space.md)
        .py(t.space.sm)
        .child(
            div()
                .w(px(30.))
                .flex_none()
                .pt(px(2.))
                .text_size(t.text.caption - px(1.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_subtle)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px(t.space.md)
                .py(t.space.sm)
                .rounded(t.radius.button)
                .bg(t.extra.inset)
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption)
                .line_height(relative(1.5))
                .text_color(t.colors.text)
                .child(markdown::code_text(text.trim_end(), lang, t)),
        )
}

/// What an answered question's call did, from its row: "exit 0",
/// "failed", "+3 −1 lines", or nothing while it runs or when unknown.
fn outcome(info: &PermInfo, blocks: &[Block]) -> Option<String> {
    let Some(Block::Tool(call)) = info.row.and_then(|r| blocks.get(r)) else {
        return None;
    };
    let (add, del) = call.diff.iter().fold((0, 0), |(a, d), l| match l.kind {
        DiffKind::Add => (a + 1, d),
        DiffKind::Del => (a, d + 1),
        _ => (a, d),
    });
    match (call.status, call.kind) {
        (ToolStatus::Running | ToolStatus::Declined, _) => None,
        (ToolStatus::Interrupted, _) => Some("stopped, outcome unknown".into()),
        (ToolStatus::Failed, ToolKind::Bash) => Some("exit 1, failed".into()),
        (ToolStatus::Failed, _) => Some("failed".into()),
        (_, ToolKind::Bash) => Some("exit 0".into()),
        (_, ToolKind::Edit | ToolKind::Write) if add + del > 0 => {
            Some(format!("+{add} −{del} lines"))
        }
        (_, ToolKind::Edit | ToolKind::Write) => Some("edited".into()),
        _ => Some("done".into()),
    }
}

/// Who answered: "by you", "by the rule Bash(npm test:*)", or that the
/// reply was stopped while it waited.
fn decider(info: &PermInfo) -> String {
    match &info.by {
        Decider::Stopped => "the reply was stopped".into(),
        // A long rule (a whole command) is cut: the row stays one line.
        Decider::Rule(r) if r.chars().count() > 48 => {
            format!(
                "by the rule {}…",
                r.chars().take(47).collect::<String>().trim_end()
            )
        }
        Decider::Rule(r) => format!("by the rule {r}"),
        Decider::Mode(m) => format!("by switching to {m}"),
        Decider::You => "by you".into(),
    }
}

/// "Allowed", "Always allowed" or "Denied".
fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Denied => "Denied",
        Verdict::AllowedAlways => "Always allowed",
        _ => "Allowed",
    }
}

/// The muted line under an answer: who, where, when, and what came of it.
fn answer_meta(verdict: Verdict, info: &PermInfo, blocks: &[Block], cx: &ItemContext) -> String {
    let mut parts = vec![match info.by {
        Decider::Stopped => format!("{}: {}", verdict_word(verdict), decider(info)),
        _ => format!("{} {}", verdict_word(verdict), decider(info)),
    }];
    parts.push(match &cx.place {
        Some((label, _)) => format!("on {label}"),
        None => "on this computer".into(),
    });
    if let Some(at) = info.at {
        parts.push(super::versions::ago(at));
    }
    if verdict != Verdict::Denied {
        parts.extend(outcome(info, blocks));
    }
    parts.join(" · ")
}

/// What the call said it was for ("Clean the build"), from its own row.
fn call_note(info: &PermInfo, blocks: &[Block]) -> Option<SharedString> {
    match blocks.get(info.row?)? {
        Block::Tool(c) => c.note.clone().filter(|n| !n.trim().is_empty()),
        _ => None,
    }
}

/// The call an answer was about, in mono, cut to one line (all of it in
/// the tooltip).
fn subject(text: &SharedString, id: SharedString, t: &Theme) -> AnyElement {
    let full = text.clone();
    let line: String = text.lines().next().unwrap_or_default().to_string();
    div()
        .id(id)
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .font_family(t.font_mono.clone())
        .text_size(t.text.caption)
        .text_color(t.colors.text)
        .child(line)
        .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(full.clone()).build(w, cx))
        .into_any_element()
}

/// An answered question as one row: a shield in the answer's tone, the
/// call's icon and what it ran on, then who answered, where, when and how
/// it went.
#[allow(clippy::too_many_arguments)]
fn answered(
    title: &SharedString,
    detail: &SharedString,
    verdict: Verdict,
    info: &PermInfo,
    blocks: &[Block],
    b: usize,
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    let denied = verdict == Verdict::Denied;
    let tone = if denied {
        t.extra.danger
    } else {
        t.extra.success
    };
    let what = if detail.trim().is_empty() {
        title.clone()
    } else {
        detail.clone()
    };
    // A bare absolute path reads as the card showed it: its last folders.
    let what = if is_bare_path(&what) {
        short_path(&what)
    } else {
        what
    };
    // A plan is named by its heading, in words, not by the call's note.
    let plan_name = (title.as_ref() == crate::backend::PLAN_ASK_TITLE)
        .then(|| match info.row.and_then(|r| blocks.get(r)) {
            Some(Block::Tool(c)) => c
                .diff
                .iter()
                .map(|l| l.text.trim())
                .find(|l| l.starts_with('#'))
                .map(|l| l.trim_start_matches('#').trim().to_string()),
            _ => None,
        })
        .map(|n| n.unwrap_or_else(|| "The plan".into()));
    let subject_row: AnyElement = match plan_name {
        Some(name) => div()
            .min_w_0()
            .truncate()
            .text_size(t.text.body_sm)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.text_strong)
            .child(name)
            .into_any_element(),
        None => subject(
            &what,
            SharedString::from(format!("perm-what-{}-{}-{b}", cx.chat, cx.index)),
            t,
        ),
    };
    div()
        .flex()
        .items_start()
        .gap(t.space.sm)
        .px(t.space.md)
        .py(t.space.sm)
        .rounded(t.radius.button)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .child(div().pt(px(1.)).child(icon(
            if denied {
                "icons/shield-x.svg"
            } else {
                "icons/shield-check.svg"
            },
            t.size.icon_sm,
            tone,
        )))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .min_w_0()
                        .children(
                            info.kind
                                .map(|k| icon(k.icon(), t.size.icon_xs, t.colors.text_muted)),
                        )
                        .child(subject_row)
                        .children(call_note(info, blocks).map(|n| {
                            div()
                                .flex_none()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(n)
                        })),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(if denied {
                            t.extra.danger
                        } else {
                            t.colors.text_muted
                        })
                        .child(answer_meta(verdict, info, blocks, cx)),
                )
                // What you told KodAct to do instead, in your words.
                .children(info.note.as_ref().map(|n| {
                    div()
                        .mt(t.space.xs)
                        .pl(t.space.sm)
                        .border_l_2()
                        .border_color(t.colors.border)
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_body)
                        .child(n.clone())
                })),
        )
        .into_any_element()
}

/// Answers in a row that say the same, as one row ("Allowed 3 commands")
/// that opens to list them.
fn answer_group(
    run: &[Block],
    blocks: &[Block],
    b: usize,
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    let Some(Block::Permission { verdict, info, .. }) = run.first() else {
        return div().into_any_element();
    };
    let (verdict, open) = (*verdict, info.open);
    let denied = verdict == Verdict::Denied;
    let tone = if denied {
        t.extra.danger
    } else {
        t.extra.success
    };
    let n = run.len();
    let noun = match info.kind {
        Some(ToolKind::Bash) => "commands",
        Some(ToolKind::Edit | ToolKind::Write) => "edits",
        Some(ToolKind::Web | ToolKind::WebSearch) => "web requests",
        Some(ToolKind::Read | ToolKind::Search) => "reads",
        _ => "requests",
    };
    // The newest answer's time stands for the group.
    let last = run.iter().rev().find_map(|blk| match blk {
        Block::Permission { info, .. } => info.at,
        _ => None,
    });
    let first_what = run.iter().find_map(|blk| match blk {
        Block::Permission { title, detail, .. } => Some(if detail.trim().is_empty() {
            title.clone()
        } else {
            detail.clone()
        }),
        _ => None,
    });
    let mut meta = vec![
        decider(info),
        match &cx.place {
            Some((label, _)) => format!("on {label}"),
            None => "on this computer".into(),
        },
    ];
    meta.extend(last.map(super::versions::ago));
    let hover = t.colors.hover;
    let header = div()
        .id(SharedString::from(format!(
            "perm-group-{}-{}-{b}",
            cx.chat, cx.index
        )))
        .flex()
        .items_center()
        .gap(t.space.sm)
        .px(t.space.md)
        .py(t.space.sm)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(toggle(cx, b))
        .child(icon(
            if denied {
                "icons/shield-x.svg"
            } else {
                "icons/shield-check.svg"
            },
            t.size.icon_sm,
            tone,
        ))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_strong)
                .child(format!("{} {n} {noun}", verdict_word(verdict))),
        )
        // Closed, it names the first call and counts the rest.
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .when(!open, |d| {
                    d.children(first_what.clone().map(|w| {
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(t.font_mono.clone())
                            .text_size(t.text.caption)
                            .text_color(t.colors.text)
                            .child(w.lines().next().unwrap_or_default().to_string())
                    }))
                    .child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(format!("+{}", n - 1)),
                    )
                }),
        )
        .child(
            div()
                .flex_none()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(meta.join(" · ")),
        )
        .child(chevron(open, t));
    div()
        .flex()
        .flex_col()
        .rounded(t.radius.button)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .overflow_hidden()
        .child(header)
        .when(open, |d| {
            d.child(crate::theme::rule(t)).child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.xs)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .children(run.iter().enumerate().filter_map(|(j, blk)| {
                        let Block::Permission {
                            title,
                            detail,
                            info,
                            ..
                        } = blk
                        else {
                            return None;
                        };
                        let what = if detail.trim().is_empty() {
                            title.clone()
                        } else {
                            detail.clone()
                        };
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .min_w_0()
                                .children(
                                    info.kind.map(|k| {
                                        icon(k.icon(), t.size.icon_xs, t.colors.text_muted)
                                    }),
                                )
                                .child(div().flex_1().min_w_0().child(subject(
                                    &what,
                                    SharedString::from(format!(
                                        "perm-item-{}-{}-{b}-{j}",
                                        cx.chat, cx.index
                                    )),
                                    t,
                                )))
                                .children(outcome(info, blocks).filter(|_| !denied).map(|o| {
                                    div()
                                        .flex_none()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .child(o)
                                })),
                        )
                    })),
            )
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn permission(
    title: &SharedString,
    detail: &SharedString,
    verdict: Verdict,
    info: &PermInfo,
    blocks: &[Block],
    b: usize,
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    let answer = |v: Verdict| {
        let (state, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, app: &mut gpui::App| {
            state.update(app, |s, cx| s.answer(chat, msg, b, v, cx))
        }
    };
    if matches!(
        verdict,
        Verdict::Allowed | Verdict::AllowedAlways | Verdict::Denied
    ) {
        return answered(title, detail, verdict, info, blocks, b, cx, t);
    }

    if verdict != Verdict::Pending {
        let (label, bg, fg) = match verdict {
            Verdict::Denied => ("Denied", t.extra.danger_tint, t.extra.danger),
            Verdict::KeepPlanning => ("Keep planning", t.extra.warning_tint, t.extra.warning),
            Verdict::PlanAutoAccept => (
                "Approved · Edit automatically",
                t.extra.success_tint,
                t.extra.success,
            ),
            Verdict::PlanApprove => ("Approved · Manual", t.extra.success_tint, t.extra.success),
            Verdict::AllowedAlways => ("Always allowed", t.extra.success_tint, t.extra.success),
            _ => ("Allowed", t.extra.success_tint, t.extra.success),
        };
        return div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .text_color(t.colors.text_muted)
            .min_w_0()
            .child(theme::pill(t, bg, fg, label).flex_none())
            .child(div().min_w_0().truncate().child(title.clone()))
            .into_any_element();
    }
    let id = |name: &str| SharedString::from(format!("perm-{name}-{}-{}-{b}", cx.chat, cx.index));
    let is_plan = title.as_ref() == crate::backend::PLAN_ASK_TITLE;
    let is_js = title.as_ref() == crate::backend::JS_ASK_TITLE;
    // The call it asks about: what you allow is shown, not described.
    let call = info.row.and_then(|r| match blocks.get(r) {
        Some(Block::Tool(c)) => Some(c),
        _ => None,
    });
    let diff = call.filter(|c| !c.diff.is_empty());
    let command = call
        .filter(|c| c.kind == ToolKind::Bash)
        .map(|c| c.target.clone());
    use super::button::Button;
    // What it is about, as the header names it: the verb and its icon.
    let (verb, glyph): (SharedString, &'static str) = match (call.map(|c| c.kind), is_plan, is_js) {
        (_, true, _) => ("Plan".into(), "icons/scroll-text.svg"),
        (_, _, true) => ("Run a script".into(), "icons/file-code.svg"),
        (Some(ToolKind::Edit), ..) => ("Edit".into(), "icons/pencil.svg"),
        (Some(ToolKind::Write), ..) => ("Write".into(), "icons/file-plus.svg"),
        (Some(ToolKind::Bash), ..) => ("Run".into(), "icons/terminal.svg"),
        (Some(ToolKind::Web | ToolKind::WebSearch), ..) => ("Fetch".into(), "icons/globe.svg"),
        _ => (title.clone(), "icons/hand.svg"),
    };
    let target = call
        .filter(|c| !is_plan && !is_js && c.kind != ToolKind::Bash && c.kind != ToolKind::Other)
        .map(|c| short_path(&c.target));
    // A change: its size, and whether it makes a new file.
    let counts = diff.map(|c| {
        let (adds, dels) = c.diff.iter().fold((0, 0), |(a, d), l| match l.kind {
            DiffKind::Add => (a + 1, d),
            DiffKind::Del => (a, d + 1),
            DiffKind::Same => (a, d),
        });
        let new_file = c.kind == ToolKind::Write
            && dels == 0
            && c.diff.iter().all(|l| l.kind == DiffKind::Add);
        (adds, dels, new_file)
    });
    let chip = |text: &'static str| {
        div()
            .flex_none()
            .px(t.space.sm)
            .h(px(18.))
            .flex()
            .items_center()
            .rounded(t.radius.pill)
            .border_1()
            .border_color(t.colors.border)
            .text_size(t.text.caption)
            .text_color(t.colors.text_muted)
            .child(text)
    };
    let header = div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .px(t.space.lg)
        .py(t.space.md)
        .border_b_1()
        .border_color(t.colors.border)
        .child(icon(glyph, t.size.icon_sm, t.colors.primary))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_strong)
                .child(verb),
        )
        .children(target.map(|path| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(path)
        }))
        .when(counts.is_some_and(|c| c.2), |d| d.child(chip("New file")))
        .children(counts.map(|(a, d, _)| diff_counts(a, d, t)))
        .child(div().flex_1())
        .when(!is_plan, |d| d.child(env_badge(&cx.place, t)));
    // The code it would run or write: a change folds to its first lines
    // until you open all of it; a command or script shows whole.
    let folded = !info.open;
    let subject: Option<AnyElement> = if is_js {
        let long = detail.lines().count() > 10 || detail.len() > 600;
        Some(
            div()
                .id(id("script"))
                .max_h(px(240.))
                .overflow_y_scroll()
                .when(long, super::hold_wheel)
                .bg(t.extra.inset)
                .p(t.space.md)
                .font_family(t.font_mono.clone())
                .text_size(t.text.body_sm)
                .child(super::markdown::code_text(detail, "js", t))
                .into_any_element(),
        )
    } else if let (true, Some(c)) = (is_plan, diff) {
        // A plan reads as the reply it is: Markdown, not its source.
        let plan: String = c
            .diff
            .iter()
            .filter(|l| l.kind != DiffKind::Del)
            .map(|l| l.text.as_ref())
            .collect::<Vec<_>>()
            .join("\n");
        Some(
            div()
                .id(id("plan"))
                .max_h(px(360.))
                .overflow_y_scroll()
                .when(plan.lines().count() > 14, super::hold_wheel)
                .px(t.space.lg)
                .pt(t.space.md)
                .child(markdown::render(
                    &plan,
                    t,
                    &CopyContext {
                        state: cx.state.clone(),
                        key: format!("plan-{}-{}-{b}", cx.chat, cx.index),
                        copied: cx.copied.clone(),
                        scale: cx.scale,
                        sources: cx.sources.clone(),
                        zooms: cx.zooms.clone(),
                        streaming: false,
                    },
                ))
                .into_any_element(),
        )
    } else if let Some(c) = diff {
        let lang = markdown::lang_of(&c.target);
        let total = c.diff.len();
        Some(if folded && total > ASK_FOLD {
            div()
                .relative()
                .overflow_hidden()
                .bg(t.extra.inset)
                .child(diff_lines(&c.diff[..ASK_FOLD], lang, t))
                // The cut fades into the card, so it reads as more below.
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(28.))
                        .bg(gpui::linear_gradient(
                            180.,
                            gpui::linear_color_stop(t.colors.surface.opacity(0.), 0.),
                            gpui::linear_color_stop(t.colors.surface, 1.),
                        )),
                )
                .into_any_element()
        } else {
            div()
                .id(id("diff"))
                .max_h(px(320.))
                .overflow_y_scroll()
                .when(total > 16, super::hold_wheel)
                .bg(t.extra.inset)
                .child(diff_lines(&c.diff, lang, t))
                .into_any_element()
        })
    } else {
        command.map(|cmd| {
            div()
                .id(id("command"))
                .max_h(px(240.))
                .overflow_y_scroll()
                .when(cmd.lines().count() > 10, super::hold_wheel)
                .bg(t.extra.inset)
                .p(t.space.md)
                .font_family(t.font_mono.clone())
                .text_size(t.text.body_sm)
                .child(super::markdown::code_text(&cmd, "sh", t))
                .into_any_element()
        })
    };
    let more = diff
        .filter(|_| !is_plan)
        .map(|c| c.diff.len())
        .filter(|n| *n > ASK_FOLD)
        .map(|n| {
            div()
                .id(id("more"))
                .w_full()
                .px(t.space.lg)
                .pt(t.space.sm)
                .text_size(t.text.caption)
                .text_color(t.colors.primary)
                .cursor_pointer()
                .hover(|d| d.underline())
                .child(if folded {
                    format!("Show all {n} lines")
                } else {
                    "Show fewer lines".to_string()
                })
                .on_click(toggle(cx, b))
        });
    let always = crate::state::always_label(info.offer.as_deref());
    let always_tip = match (&info.offer, info.scope) {
        (Some(rule), Some(scope)) => format!("Adds the rule {rule}, {scope}."),
        (Some(rule), None) => format!("Adds the rule {rule}."),
        _ => "KodAct won't ask again for this.".to_string(),
    };
    // Said in words when nothing above shows it.
    let words = (subject.is_none() && !detail.is_empty())
        .then(|| div().text_color(t.colors.text_body).child(detail.clone()));
    let footer = div()
        .flex()
        .flex_col()
        .gap(t.space.md)
        .px(t.space.lg)
        .pt(t.space.md)
        .pb(t.space.lg)
        // The question is the heading of what you answer.
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(t.text.body + px(1.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(title.clone()),
                )
                .children(words),
        )
        // A long script scrolls: say so, so its end isn't missed.
        .when(
            is_js && (detail.lines().count() > 10 || detail.len() > 600),
            |d| {
                d.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(format!(
                            "{} lines, {} characters: scroll the script to read all of it.",
                            detail.lines().count(),
                            detail.chars().count()
                        )),
                )
            },
        )
        .child(
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap(t.space.sm)
                // A plan: start on it (edits automatically, or each one
                // asked), or keep planning; as KodAct for VS Code has it.
                .when(is_plan, |d| {
                    d.child(
                        Button::primary(id("plan-auto"), "Yes, edit automatically")
                            .on_click(answer(Verdict::PlanAutoAccept)),
                    )
                    .child(
                        Button::secondary(id("plan-manual"), "Yes, ask before edits")
                            .on_click(answer(Verdict::PlanApprove)),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::ghost(id("keep"), "No, keep planning")
                            .on_click(answer(Verdict::KeepPlanning)),
                    )
                })
                // A tool: allow it this once (the one filled action, Enter),
                // always (named for what it allows, Shift Enter), or deny it
                // (a quiet way out, Esc). Each key works from an empty composer.
                .when(!is_plan, |d| {
                    d.child(
                        Button::accent(id("allow"), "Allow")
                            .hint(ASK_KEYS.0)
                            .on_click(answer(Verdict::Allowed)),
                    )
                    .when(!info.every_time, |d| {
                        d.child(
                            Button::secondary(id("always"), always)
                                .hint(ASK_KEYS.1)
                                .describe(always_tip)
                                .tooltip(true)
                                .on_click(answer(Verdict::AllowedAlways)),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Button::ghost(id("deny"), "Deny")
                            .hint(ASK_KEYS.2)
                            .on_click(answer(Verdict::Denied)),
                    )
                }),
        );
    // A lifted card: its lift and its one accent button say it waits on
    // you; no coloured edge.
    div()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_grain())
        .child(header)
        .children(subject)
        .children(more)
        .child(footer)
        .into_any_element()
}

/// A path as the card's header shows it: its last three parts (the file
/// and the folders that place it), the rest as "…/".
fn short_path(path: &str) -> SharedString {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if parts.len() <= 3 {
        return path.to_string().into();
    }
    format!("…/{}", parts[parts.len() - 3..].join("/")).into()
}

/// One absolute path and nothing else ("/home/me/proj/notes.txt",
/// "C:\\work\\a.rs"): a command or a sentence is left as written.
fn is_bare_path(s: &str) -> bool {
    let s = s.trim();
    let rooted = s.starts_with('/') || s.as_bytes().get(1) == Some(&b':');
    rooted && !s.contains(char::is_whitespace) && s.contains(['/', '\\'])
}

/// A waiting change shows this many lines until you open all of it.
const ASK_FOLD: usize = 4;
/// The keys that answer a waiting tool from an empty composer: allow,
/// always allow, deny (`composer.rs`, `workspace.rs`).
pub const ASK_KEYS: (&str, &str, &str) = ("Enter", "Shift Enter", "Esc");

#[cfg(test)]
mod tests {
    use super::{denied_call, same_answers};
    use crate::state::{Block, PermInfo, Verdict};

    fn ask(title: &str, verdict: Verdict) -> Block {
        Block::Permission {
            title: title.to_string().into(),
            detail: "".into(),
            verdict,
            info: PermInfo::default(),
        }
    }

    #[test]
    fn only_a_bare_path_is_shortened() {
        assert!(super::is_bare_path("/tmp/x/proj/notes.txt"));
        assert!(super::is_bare_path("C:\\work\\proj\\a.rs"));
        assert!(!super::is_bare_path("rm junk.txt"));
        assert!(!super::is_bare_path("Delete /tmp/junk.txt"));
    }

    #[test]
    fn an_asked_path_keeps_its_last_folders() {
        assert_eq!(
            super::short_path("/tmp/x/proj/docs/diagrams/a.html").as_ref(),
            "…/docs/diagrams/a.html"
        );
        assert_eq!(super::short_path("src/main.rs").as_ref(), "src/main.rs");
        assert_eq!(
            super::short_path(r"C:\Users\me\proj\src\ui\a.rs").as_ref(),
            "…/src/ui/a.rs"
        );
    }

    #[test]
    fn identical_answers_group_and_others_stand_alone() {
        let blocks = [
            ask("Run a command", Verdict::Denied),
            ask("Run a command", Verdict::Allowed),
            ask("Run a command", Verdict::Allowed),
            ask("Run a command", Verdict::Allowed),
            ask("Edit a file", Verdict::AllowedAlways),
            Block::Text("done".into()),
            ask("Run a command", Verdict::Pending),
        ];
        assert_eq!(same_answers(&blocks, 0), 1);
        assert_eq!(same_answers(&blocks, 1), 3);
        assert_eq!(same_answers(&blocks, 2), 2);
        assert_eq!(same_answers(&blocks, 4), 1);
        // Text and unanswered questions are not answers.
        assert_eq!(same_answers(&blocks, 5), 0);
        assert_eq!(same_answers(&blocks, 6), 0);
    }

    #[test]
    fn the_footer_says_how_long_and_how_much() {
        use crate::context::Usage;
        assert_eq!(super::took(4.83, None), "Took 4.8s");
        let u = Usage {
            output: 1_250,
            ..Default::default()
        };
        assert_eq!(super::took(75., Some(&u)), "Took 1m 15s · 1.2K-token reply");
        let u = Usage {
            output: 40,
            estimated: true,
            ..Default::default()
        };
        assert_eq!(super::took(2., Some(&u)), "Took 2.0s · ~40-token reply");
    }

    #[test]
    fn an_allowed_call_shows_once_with_its_answer() {
        let call = Block::Tool(crate::backend::tool_call(
            "Edit",
            &serde_json::json!({ "file_path": "src/main.rs" }),
        ));
        let mut ok = ask("Edit a file", Verdict::AllowedAlways);
        if let Block::Permission { info, .. } = &mut ok {
            info.row = Some(0);
        }
        let blocks = [call, ok, ask("Edit a file", Verdict::Allowed)];
        assert_eq!(super::answer_for(&blocks, 0), Some(1));
        assert!(super::folded_answer(&blocks, 1));
        // An answer with no row of its own stands alone.
        assert!(!super::folded_answer(&blocks, 2));
    }

    #[test]
    fn a_denied_call_shows_only_its_denial() {
        let call = Block::Tool(crate::backend::tool_call(
            "Bash",
            &serde_json::json!({ "command": "rm -rf target" }),
        ));
        let mut denied = ask("Run a command", Verdict::Denied);
        if let Block::Permission { info, .. } = &mut denied {
            info.row = Some(0);
        }
        let blocks = [call.clone(), denied, call];
        assert!(denied_call(&blocks, 0));
        assert!(!denied_call(&blocks, 1));
        assert!(!denied_call(&blocks, 2));
    }

    #[test]
    fn tool_outcome_reports_concise_counts_and_failures() {
        let mut read = crate::backend::tool_call(
            "Read",
            &serde_json::json!({ "file_path": "/repo/src/app.ts" }),
        );
        read.status = crate::state::ToolStatus::Done;
        read.output = "line 1\nline 2\nline 3".into();
        assert_eq!(super::tool_outcome(&read).as_deref(), Some("3 lines"));
        assert_eq!(super::target_label(&read).as_ref(), "…/src/app.ts");

        let mut bash_fail = crate::backend::tool_call(
            "Bash",
            &serde_json::json!({ "command": "npm test" }),
        );
        bash_fail.status = crate::state::ToolStatus::Failed;
        assert_eq!(super::tool_outcome(&bash_fail).as_deref(), Some("failed"));
    }
}
