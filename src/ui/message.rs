//! One transcript item, as Forge draws it:
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
    /// Earlier versions from your message here on (edits and retries), and
    /// whether their list is open.
    pub versions: Vec<super::versions::Row>,
    pub versions_open: bool,
    /// Diagrams showing their source (`AppState::diagram_code`).
    pub sources: std::sync::Arc<std::collections::HashSet<SharedString>>,
    pub zooms: std::sync::Arc<std::collections::HashMap<SharedString, f32>>,
    /// What the newest turn changed in the project's memory, under its reply.
    pub memory: Option<crate::state::MemoryChange>,
    /// The runs of calls opened to their steps, by their first block (in
    /// the detailed view, the ones closed).
    pub open_groups: Vec<usize>,
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
    let memory = cx.memory.as_ref().map(|m| memory_line(m, chat, cx.state.clone(), t));
    let row = div()
        .group(group)
        .flex()
        .items_center()
        .gap(t.space.sm)
        .child(actions)
        .child(
            div()
                .id(SharedString::from(format!("turn-took-{key}")))
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(took(seconds, usage))
                .when_some(usage.map(read_and_wrote), |d, tip| {
                    d.tooltip(move |w, cx| {
                        gpui::component::tooltip::Tooltip::new(tip.clone()).build(w, cx)
                    })
                }),
        );
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
        .child(icon("icons/brain.svg", t.size.icon_xs, t.colors.coral))
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
fn notice(text: &SharedString, error: bool, cx: &ItemContext, t: &Theme) -> AnyElement {
    let key = format!("{}-{}", cx.chat, cx.index);
    let (fg, bg) = if error {
        (t.extra.danger, t.extra.danger_tint)
    } else {
        (t.colors.text_muted, t.colors.hover)
    };
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
        // Long errors (a proxy's whole response) wrap rather than run off the card.
        .child(
            div()
                .flex_1()
                .min_w_0()
                .line_height(relative(1.45))
                .child(text.clone()),
        )
        .when(interrupted && cx.last && cx.can_continue, |d| {
            d.child(
                link(format!("continue-{key}"), "Continue", "icons/play.svg").on_click(
                    |_, window, app| window.dispatch_action(Box::new(super::ContinueReply), app),
                ),
            )
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
            ))
        })
        .into_any_element()
}

/// Your message in parts: what you wrote, styled as it was in the box, and
/// each fenced block (code or a log you pasted) as a code block of its own,
/// with its language's colours and a copy button.
fn user_parts(text: &SharedString, key: &str, cx: &ItemContext, t: &Theme) -> Vec<AnyElement> {
    let mut out = vec![];
    let plain = |r: std::ops::Range<usize>, out: &mut Vec<AnyElement>| {
        let body = text[r.clone()].trim_matches('\n');
        if body.trim().is_empty() {
            return;
        }
        let from = r.start + text[r.clone()].find(body).unwrap_or(0);
        let marks: Vec<_> = cx
            .marks
            .iter()
            .filter(|(m, _)| m.start >= from && m.end <= from + body.len())
            .map(|(m, k)| (m.start - from..m.end - from, *k))
            .collect();
        out.push(markdown::selectable_plain(
            SharedString::from(format!("user-text-{key}-{}", out.len())),
            &SharedString::from(body.to_string()),
            &marks,
            t,
        ));
    };
    let (mut at, mut open, mut pos) = (0, None::<usize>, 0);
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            match open {
                None => {
                    plain(at..pos, &mut out);
                    open = Some(pos);
                }
                Some(start) => {
                    let end = pos + line.len();
                    out.push(markdown::render(
                        &text[start..end],
                        t,
                        &CopyContext {
                            state: cx.state.clone(),
                            key: format!("{}-{}-u{}", cx.chat, cx.index, out.len()),
                            copied: cx.copied.clone(),
                            scale: cx.scale,
                            sources: cx.sources.clone(),
                            zooms: cx.zooms.clone(),
                            streaming: false,
                        },
                    ));
                    open = None;
                    at = end;
                }
            }
        }
        pos += line.len();
    }
    // An unclosed fence stays text.
    plain(open.unwrap_or(at)..text.len(), &mut out);
    out
}

fn assistant(blocks: &[Block], cx: &ItemContext, t: &Theme) -> AnyElement {
    // Answers in a row that say the same thing show as one row.
    let mut rows = vec![];
    // Calls in a row fold into one line (`work_group`).
    let mut work = super::work_group::runs(blocks, cx.busy && cx.last).into_iter().peekable();
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
        Block::Permission { title, verdict, info, .. }
            if info.questions.is_empty()
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
        Block::File(file) => super::file_card::render(
            file,
            &format!("{}-{}-{b}", cx.chat, cx.index),
            cx.state.clone(),
            cx.open_doc.as_ref() == Some(file),
            cx.scale,
            t,
        ),
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
        (false, Some((_, s))) if *s >= 1 => format!("Thought for {}", super::chat_view::elapsed(*s)),
        _ => "Thought".to_string(),
    };
    let key = format!("{}-{}-{b}", cx.chat, cx.index);
    let long = text.len() > 900;
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .id(SharedString::from(format!("think-{key}")))
                .role(gpui::accesskit::Role::Button)
                .aria_label(SharedString::from(format!(
                    "{label}, {}",
                    if open { "hide the thinking" } else { "show the thinking" }
                )))
                .flex()
                .items_center()
                .gap(t.space.xs + px(2.))
                .min_h(t.size.quiet_line)
                .cursor_pointer()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .hover(move |s| s.text_color(strong))
                .when(streaming, |d| {
                    d.child(
                        icon("icons/sparkles.svg", t.size.icon_xs, t.extra.accent).with_animation(
                            SharedString::from(format!("think-pulse-{key}")),
                            Animation::new(Duration::from_millis(1600)).repeat(),
                            |svg, d| svg.opacity(0.45 + 0.55 * (d * std::f32::consts::PI).sin()),
                        ),
                    )
                })
                .child(label)
                .child(icon(
                    if open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    t.size.icon_xs,
                    t.colors.text_subtle,
                ))
                .on_click(toggle(cx, b)),
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
        .when(adds > 0, |d| d.child(chip(format!("+{adds}"), t.extra.success, t.extra.success_tint)))
        .when(dels > 0, |d| d.child(chip(format!("−{dels}"), t.extra.danger, t.extra.danger_tint)))
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
                .child(img(i.image.clone()).size_full().object_fit(ObjectFit::Contain))
        }))
}

/// What a call's line names: a file by its name (its folder is in the
/// opened body), anything else as its row says it.
fn target_label(call: &ToolCall) -> SharedString {
    match call.kind {
        ToolKind::Read | ToolKind::Edit | ToolKind::Write => {
            let t = call.target.as_ref();
            t.rsplit(['/', '\\']).next().unwrap_or(t).to_string().into()
        }
        _ => call.shown_target(),
    }
}

/// A tool call's one line, as quiet as Claude's: "Read main.rs", "Edited
/// lib.rs +3 −1", a command by its description. Muted text, no frame and no
/// icon for a call that worked; a mark only while it runs, or when it
/// failed or was stopped (with why). Clicking it opens the body.
pub(super) fn tool_header(
    call: &ToolCall,
    b: usize,
    blocks: &[Block],
    cx: &ItemContext,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let strong = t.colors.text;
    let failed = call.status == ToolStatus::Failed;
    // A failed change was never made, a stopped one may not have been: no
    // counts for either.
    let unknown = call.status == ToolStatus::Interrupted;
    let adds = call.diff.iter().filter(|l| l.kind == DiffKind::Add && !failed && !unknown).count();
    let dels = call.diff.iter().filter(|l| l.kind == DiffKind::Del && !failed && !unknown).count();
    let shell = call.kind == ToolKind::Bash;
    let has_body = has_body(call);
    let why = failure_line(call).filter(|_| !shell);
    let waiting = call.status == ToolStatus::Running && awaits_you(blocks, b);
    let verb = if call.status == ToolStatus::Declined
        && call.output.starts_with(crate::state::AGENT_STOPPED)
    {
        // A sub-agent you stopped reads "Stopped", not "Skipped".
        "Stopped"
    } else {
        call.kind.verb(call.status)
    };
    let one_line = |d: gpui::Div| d.min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis();
    div()
        .id(SharedString::from(format!(
            "tool-{}-{}-{b}",
            cx.chat, cx.index
        )))
        .role(gpui::accesskit::Role::Button)
        .aria_label(SharedString::from(format!(
            "{verb} {}{}{}",
            call.shown_target(),
            why.as_ref().map(|w| format!(": {w}")).unwrap_or_default(),
            if !has_body {
                ""
            } else if call.open {
                ", hide details"
            } else {
                ", show details"
            }
        )))
        .flex()
        .items_center()
        .gap(t.space.xs + px(2.))
        .min_h(t.size.quiet_line)
        .text_size(t.text.body_sm)
        .text_color(t.colors.text_muted)
        .when(has_body, |d| d.cursor_pointer().hover(move |s| s.text_color(strong)))
        // A mark only when there is something to say: it runs, waits on
        // you, failed, was stopped or skipped.
        .children(match call.status {
            _ if waiting => Some(icon("icons/hand.svg", t.size.icon_xs, t.extra.accent).into_any_element()),
            ToolStatus::Done => None,
            ToolStatus::Failed => {
                // The whole error, on hover.
                let full = SharedString::from(call.output.trim().to_string());
                Some(
                    div()
                        .id(SharedString::from(format!("tool-why-{}-{}-{b}", cx.chat, cx.index)))
                        .flex_none()
                        .child(status_icon(call.status, t))
                        .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(full.clone()).build(w, cx))
                        .into_any_element(),
                )
            }
            s => Some(div().flex_none().child(status_icon(s, t)).into_any_element()),
        })
        .map(|d| match (&call.note, shell) {
            // A command reads as its description; the command itself is in
            // the terminal block it opens to.
            (Some(note), true) => d.child(one_line(div()).text_color(t.colors.text).child(note.clone())),
            _ => d
                .child(div().flex_none().child(verb))
                .child(
                    one_line(div())
                        // With a note or a reason, the target keeps its room
                        // and the words after it are cut instead.
                        .when(call.note.is_some() || why.is_some(), |d| d.flex_none().max_w(px(260.)))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text)
                        .child(target_label(call)),
                )
                .children(call.note.clone().filter(|_| why.is_none()).map(|n| {
                    one_line(div()).text_color(t.colors.text_subtle).child(n)
                })),
        })
        // Why it failed, in its own words, cut to the line.
        .children(why.map(|w| {
            one_line(div())
                .text_color(if unknown { t.extra.warning } else { t.extra.danger })
                .child(w)
        }))
        .when(adds + dels > 0, |d| d.child(diff_counts(adds, dels, t)))
        .when(has_body, |d| {
            d.child(icon(
                if call.open {
                    "icons/chevron-down.svg"
                } else {
                    "icons/chevron-right.svg"
                },
                t.size.icon_xs,
                t.colors.text_subtle,
            ))
        })
        // Opened, the pictures show whole below instead.
        .when_some(thumbs(call.images.iter(), t).filter(|_| !call.open), |d, th| {
            d.child(div().flex_1()).child(th)
        })
        .on_click(toggle(cx, b))
}

/// What a tool call's line opens to, in one quiet frame: a file change
/// headed by its full path (with Keep and Revert), a command's terminal
/// block, or what went in and came out. A failed call shows its error
/// first, and the change it would have made, marked as not made.
pub(super) fn tool_body(call: &ToolCall, b: usize, cx: &ItemContext, t: &Theme) -> Option<AnyElement> {
    if !(call.open && has_body(call)) {
        return None;
    }
    let frame = || {
        div()
            .mt(t.space.xs)
            .mb(t.space.sm)
            .rounded(t.radius.button)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.colors.surface)
            .overflow_hidden()
            .flex()
            .flex_col()
    };
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
        return Some(frame().child(terminal::block(call, &at, t)).into_any_element());
    }
    // A file's change is headed by where it is, and what you can do about it.
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
    if matches!(call.status, ToolStatus::Failed | ToolStatus::Interrupted) {
        let stopped = call.status == ToolStatus::Interrupted;
        return Some(
            frame()
                .children(path_bar)
                .child(
                    div()
                        .px(t.space.md)
                        .py(t.space.sm)
                        .bg(if stopped { t.extra.warning_tint } else { t.extra.danger_tint })
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption + px(0.5))
                        .text_color(t.colors.text)
                        .child(call.output.trim().to_string()),
                )
                .when(!call.diff.is_empty(), |d| {
                    d.child(
                        div()
                            .px(t.space.md)
                            .pt(t.space.sm)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(if stopped { "May or may not have been applied" } else { "Not applied" }),
                    )
                    .child(diff_lines(&call.diff, lang, t).opacity(0.75))
                })
                .into_any_element(),
        );
    }
    Some(if !call.diff.is_empty() {
        frame()
            .children(path_bar)
            .child(diff_lines(&call.diff, lang, t))
            .into_any_element()
    } else {
        frame()
            .children(path_bar)
            .child(io_row("IN", call.target.clone(), "", t))
            // A file's contents are coloured by its language.
            .child(io_row(
                "OUT",
                call.output.clone(),
                if call.kind == ToolKind::Read { lang } else { "" },
                t,
            ))
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
    // A lone call is a quiet line, as one in a run is; its body opens under it.
    let row = div()
        .flex()
        .flex_col()
        .child(tool_header(call, b, blocks, cx, t))
        .children(tool_body(call, b, cx, t));
    flash(row, b, cx, t)
}

/// A finished file change's review: Keep and Revert… while undecided; then
/// "Kept", or "Reverted" with Re-apply (which ForgeCLI can't do yet).
fn review_controls(call: &ToolCall, b: usize, cx: &ItemContext, t: &Theme) -> Option<AnyElement> {
    if !matches!(call.kind, ToolKind::Edit | ToolKind::Write) || call.status != ToolStatus::Done {
        return None;
    }
    let review = file_review(call, b, cx, t);
    // A web page Forge wrote here: a look at it in the side panel's browser.
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
            .child(
                super::soon(
                    small(
                        format!("reapply-{key}"),
                        "Re-apply",
                        "icons/refresh-cw.svg",
                        t.colors.text_muted,
                    ),
                    crate::stubs::REAPPLY,
                )
                .on_click(move |_, _, app| {
                    app.stop_propagation();
                    state.update(app, |s, cx| s.stub(crate::stubs::REAPPLY, cx));
                }),
            )
            .into_any_element(),
    };
    Some(el)
}

/// Forge's tool I/O row: a small label, then the text in a quiet well.
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
            format!("by the rule {}…", r.chars().take(47).collect::<String>().trim_end())
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
                        .child(subject(
                            &what,
                            SharedString::from(format!("perm-what-{}-{}-{b}", cx.chat, cx.index)),
                            t,
                        ))
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
                // What you told Forge to do instead, in your words.
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
            .child(theme::pill(t, bg, fg, label))
            .child(title.clone())
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
    let command = call.filter(|c| c.kind == ToolKind::Bash).map(|c| c.target.clone());
    use super::button::Button;
    // A scrolling well for code: the change, the command, the script.
    let well = |name: &str, long: bool| {
        div()
            .id(id(name))
            .max_h(px(240.))
            .overflow_y_scroll()
            .when(long, super::hold_wheel)
            .rounded(t.radius.button)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.extra.inset)
    };
    let subject: AnyElement = if is_js {
        let long = detail.lines().count() > 10 || detail.len() > 600;
        well("script", long)
            .p(t.space.md)
            .font_family(t.font_mono.clone())
            .text_size(t.text.body_sm)
            .child(super::markdown::code_text(detail, "js", t))
            .into_any_element()
    } else if let Some(c) = diff {
        let (adds, dels) = c.diff.iter().fold((0, 0), |(a, d), l| match l.kind {
            DiffKind::Add => (a + 1, d),
            DiffKind::Del => (a, d + 1),
            DiffKind::Same => (a, d),
        });
        div()
            .flex()
            .flex_col()
            .gap(t.space.xs)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .text_size(t.text.caption)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(t.font_mono.clone())
                            .text_color(t.colors.text_muted)
                            .child(c.target.clone()),
                    )
                    .child(diff_counts(adds, dels, t)),
            )
            .child(
                well("diff", c.diff.len() > 12)
                    .child(diff_lines(&c.diff, markdown::lang_of(&c.target), t)),
            )
            .into_any_element()
    } else if let Some(cmd) = command {
        well("command", cmd.lines().count() > 10)
            .p(t.space.md)
            .font_family(t.font_mono.clone())
            .text_size(t.text.body_sm)
            .child(super::markdown::code_text(&cmd, "sh", t))
            .into_any_element()
    } else {
        div()
            .text_color(t.colors.text_body)
            .child(detail.clone())
            .into_any_element()
    };
    let always = crate::state::always_label(info.offer.as_deref());
    let always_tip = match (&info.offer, info.scope) {
        (Some(rule), Some(scope)) => format!("Adds the rule {rule}, {scope}."),
        (Some(rule), None) => format!("Adds the rule {rule}."),
        _ => "Forge won't ask again for this.".to_string(),
    };
    let body = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(t.space.md)
        .p(t.space.lg)
        .child(
            div()
                .flex()
                .items_start()
                .gap(t.space.md)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        // The question is the heading: no label above it.
                        .child(
                            div()
                                .text_size(t.text.body + px(2.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                                .child(title.clone()),
                        ),
                )
                .when(!is_plan, |d| d.child(env_badge(&cx.place, t))),
        )
        .child(subject)
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
                // asked), or keep planning; as Forge for VS Code has it.
                .when(is_plan, |d| {
                    d.child(
                        Button::ghost(id("keep"), "No, keep planning")
                            .on_click(answer(Verdict::KeepPlanning)),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::secondary(id("plan-manual"), "Yes, ask before edits")
                            .on_click(answer(Verdict::PlanApprove)),
                    )
                    .child(
                        Button::primary(id("plan-auto"), "Yes, edit automatically")
                            .on_click(answer(Verdict::PlanAutoAccept)),
                    )
                })
                // A tool: deny it (a quiet way out), allow it this once (the
                // filled action), or always, named for what it allows. The
                // broad grant is never the loudest button.
                .when(!is_plan, |d| {
                    d.child(
                        Button::ghost(id("deny"), "Deny")
                            .on_click(answer(Verdict::Denied)),
                    )
                    .child(div().flex_1())
                    .when(!info.every_time, |d| {
                        d.child(
                            Button::secondary(id("always"), always)
                                .describe(always_tip)
                                .tooltip(true)
                                .on_click(answer(Verdict::AllowedAlways)),
                        )
                    })
                    .child(
                        Button::accent(id("allow"), "Allow once")
                            .on_click(answer(Verdict::Allowed)),
                    )
                }),
        );
    // A lifted card: its lift and its one accent button say it waits on
    // you; no coloured edge.
    div()
        .flex()
        .overflow_hidden()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_grain())
        .child(body)
        .into_any_element()
}

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
}
