//! One transcript item, as Forge draws it:
//! - your message: a white card on the right;
//! - a reply: flush with the column, under a small "Forge" header on the
//!   first reply of a turn; it holds text (markdown), thinking (a
//!   disclosure), tool calls (one row each; Bash as a terminal block) and
//!   permission prompts;
//! - the end of a turn: how long it took;
//! - a notice (interrupted, an error).

use std::time::{Duration, SystemTime};

use gpui::{
    div, percentage, prelude::*, px, relative, Animation, AnimationExt, AnyElement, ClipboardItem,
    Entity, FontWeight, SharedString, Transformation,
};

use super::icon;
use super::markdown::{self, CopyContext};
use super::terminal;
use crate::state::{
    AppState, Block, Decider, DiffKind, Message, PermInfo, Rating, Review, ToolCall, ToolKind,
    ToolStatus, Verdict,
};
use crate::theme::{self, Theme};

pub struct ItemContext {
    pub state: Entity<AppState>,
    /// Where this chat's agent runs: the remote's label and icon, or None
    /// for this computer. Commands and approvals carry it as a badge.
    pub place: Option<(SharedString, &'static str)>,
    pub chat: usize,
    pub index: usize,
    pub copied: Option<SharedString>,
    pub busy: bool,
    pub last: bool,
    /// The first reply after your message: it carries the turn's header.
    pub first_in_turn: bool,
    /// This turn is the one still running.
    pub live_turn: bool,
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
}

pub fn render(m: &Message, cx: &ItemContext, t: &Theme) -> AnyElement {
    match m {
        Message::User {
            text,
            sent,
            attachments,
            ..
        } => user(text, *sent, attachments, cx, t),
        Message::Assistant { blocks } => assistant(blocks, cx, t),
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
    div()
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
        )
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
        .children(super::attachments::sent(t, attachments, &key))
        // A message of only attachments has no text card.
        .when(!text.trim().is_empty(), |d| {
            d.child(
                div()
                    .max_w(relative(0.85))
                    .px(t.space.md)
                    .py(t.space.sm)
                    .bg(t.colors.surface)
                    .border_1()
                    .border_color(t.colors.border)
                    .rounded(t.radius.card)
                    .text_size(t.size.chat_text)
                    .line_height(relative(t.size.line_height_chat))
                    .child(text.clone()),
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

/// The head of a turn: Forge's logo and name. The logo turns while the turn
/// runs, and is still after.
fn turn_header(cx: &ItemContext, t: &Theme) -> AnyElement {
    const CUBE: f32 = 20.;
    let mark = if cx.live_turn {
        super::turning_logo(
            CUBE,
            SharedString::from(format!("turn-cube-{}-{}", cx.chat, cx.index)),
        )
    } else {
        super::logo(CUBE).into_any_element()
    };
    div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .text_size(t.text.caption + px(1.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.text_muted)
        .child(mark)
        .child("Forge")
        .into_any_element()
}

fn assistant(blocks: &[Block], cx: &ItemContext, t: &Theme) -> AnyElement {
    // Answers in a row that say the same thing show as one row.
    let mut rows = vec![];
    let mut b = 0;
    while b < blocks.len() {
        let run = same_answers(blocks, b);
        if denied_call(blocks, b) || folded_answer(blocks, b) {
            // A denied call never ran: its denial says all there is. An
            // allowed one shows once, its row carrying the answer.
            b += 1;
        } else if let Some(p) = answer_for(blocks, b) {
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(self::block(blocks, b, cx, t))
                    .child(approval_line(&blocks[p], cx, t))
                    .into_any_element(),
            );
            b += 1;
        } else if run > 1 {
            rows.push(answer_group(&blocks[b..b + run], blocks, b, cx, t));
            b += run;
        } else {
            rows.push(self::block(blocks, b, cx, t));
            b += 1;
        }
    }
    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .when(cx.first_in_turn, |d| d.child(turn_header(cx, t)))
        .children(rows)
        .into_any_element()
}

/// The allowed answer about the call at `b`, when it names `b` as its row.
fn answer_for(blocks: &[Block], b: usize) -> Option<usize> {
    if !matches!(blocks.get(b), Some(Block::Tool(_))) {
        return None;
    }
    blocks.iter().enumerate().skip(b + 1).find_map(|(i, blk)| {
        matches!(blk, Block::Permission { verdict: Verdict::Allowed | Verdict::AllowedAlways, info, .. } if info.row == Some(b))
            .then_some(i)
    })
}

/// `b` is an allowed answer shown on its call's own row instead.
fn folded_answer(blocks: &[Block], b: usize) -> bool {
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
fn approval_line(answer: &Block, cx: &ItemContext, t: &Theme) -> AnyElement {
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
        .child(parts.join(" · "))
        .into_any_element()
}

/// `b` is a call whose question was denied (the question names it as its row).
fn denied_call(blocks: &[Block], b: usize) -> bool {
    matches!(blocks[b], Block::Tool(_))
        && blocks[b + 1..].iter().any(|blk| {
            matches!(blk, Block::Permission { verdict: Verdict::Denied, info, .. } if info.row == Some(b))
        })
}

/// How many answered questions from `b` on say the same (same question,
/// same answer): 0 when `b` isn't one.
fn same_answers(blocks: &[Block], b: usize) -> usize {
    let key = |blk: &Block| match blk {
        Block::Permission { title, verdict, .. }
            if matches!(
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
fn block(blocks: &[Block], b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
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
                streaming: cx.busy && cx.last && newest,
            },
        ),
        Block::Thinking { text, open } => thinking(text, *open, b, cx, t),
        Block::Tool(call) => tool(call, b, cx, t),
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

fn thinking(text: &SharedString, open: bool, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    let streaming = cx.busy && cx.last;
    let hover = t.colors.text;
    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .child(
            div()
                .id(SharedString::from(format!(
                    "think-{}-{}-{b}",
                    cx.chat, cx.index
                )))
                .flex()
                .items_center()
                .gap(t.space.xs)
                .cursor_pointer()
                .text_color(t.colors.text_muted)
                .hover(move |s| s.text_color(hover))
                .child(icon(
                    "icons/brain.svg",
                    t.size.icon_sm,
                    t.colors.text_subtle,
                ))
                .child(if streaming { "Thinking" } else { "Thought" })
                .child(chevron(open, t))
                .on_click(toggle(cx, b)),
        )
        .when(open, |d| {
            d.child(
                div()
                    .ml(px(6.))
                    .pl(t.space.md)
                    .border_l_2()
                    .border_color(t.colors.border)
                    .text_size(t.size.chat_text - px(0.5))
                    .line_height(relative(t.size.line_height_chat))
                    .text_color(t.colors.text_body)
                    .child(text.clone()),
            )
        })
        .into_any_element()
}

fn status_icon(status: ToolStatus, t: &Theme) -> AnyElement {
    match status {
        ToolStatus::Running => icon("icons/loader-circle.svg", t.size.icon_sm, t.extra.accent)
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

fn tool(call: &ToolCall, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    let hover = t.colors.hover;
    let adds = call.diff.iter().filter(|l| l.kind == DiffKind::Add).count();
    let dels = call.diff.iter().filter(|l| l.kind == DiffKind::Del).count();
    let shell = call.kind == ToolKind::Bash;
    // A command always has a body: its terminal block.
    let has_body = shell || !call.diff.is_empty() || !call.output.is_empty();
    let header = div()
        .id(SharedString::from(format!(
            "tool-{}-{}-{b}",
            cx.chat, cx.index
        )))
        .role(gpui::accesskit::Role::Button)
        .aria_label(SharedString::from(format!(
            "{} {}{}",
            call.kind.verb(call.status),
            call.shown_target(),
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
        .gap(t.space.sm)
        .px(t.space.md)
        .h(t.size.row + px(4.))
        .when(has_body, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
        .child(icon(call.kind.icon(), t.size.icon_sm, t.colors.text_muted))
        .map(|d| match (&call.note, shell) {
            // A command is headed by its description alone; the command
            // itself is in the terminal block below.
            (Some(note), true) => d.child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_weight(FontWeight::MEDIUM)
                    .child(note.clone()),
            ),
            _ => d
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::MEDIUM)
                        // A sub-agent you stopped reads "Stopped", not "Skipped".
                        .child(
                            if call.status == ToolStatus::Declined
                                && call.output.starts_with(crate::state::AGENT_STOPPED)
                            {
                                "Stopped"
                            } else {
                                call.kind.verb(call.status)
                            },
                        ),
                )
                .child(
                    div()
                        .min_w_0()
                        // With a note, the target is a short name (`browser ·
                        // eval`) and keeps its room; the note is cut instead.
                        .when(call.note.is_some(), |d| d.flex_none())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption + px(0.5))
                        .text_color(t.colors.text_muted)
                        .child(call.shown_target()),
                )
                .children(call.note.clone().map(|n| {
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(t.colors.text_subtle)
                        .text_size(t.text.caption)
                        .child(n)
                })),
        })
        .when(shell, |d| d.child(env_badge(&cx.place, t)))
        .child(div().flex_1())
        .when(adds + dels > 0, |d| {
            d.child(
                div()
                    .flex()
                    .gap(t.space.xs)
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.caption)
                    .child(div().text_color(t.extra.success).child(format!("+{adds}")))
                    .child(div().text_color(t.extra.danger).child(format!("−{dels}"))),
            )
        })
        .children(review_controls(call, b, cx, t))
        .child(status_icon(call.status, t))
        .when(has_body, |d| d.child(chevron(call.open, t)))
        .on_click(toggle(cx, b));

    let lang = markdown::lang_of(&call.target);
    let body = (call.open && has_body).then(|| {
        if shell {
            let at = terminal::Where {
                state: cx.state.clone(),
                place: cx.place.clone(),
                chat: cx.chat,
                msg: cx.index,
                block: b,
                copied: cx.copied.clone(),
                busy: cx.busy,
            };
            terminal::block(call, &at, t)
        } else if !call.diff.is_empty() {
            div()
                .border_t_1()
                .border_color(t.colors.border)
                .py(t.space.xs)
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption + px(0.5))
                .line_height(relative(1.65))
                .children(call.diff.iter().map(|l| {
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
                .into_any_element()
        } else {
            div()
                .border_t_1()
                .border_color(t.colors.border)
                .flex()
                .flex_col()
                .child(io_row("IN", call.target.clone(), "", t))
                // A file's contents are coloured by its language.
                .child(io_row(
                    "OUT",
                    call.output.clone(),
                    if call.kind == ToolKind::Read {
                        lang
                    } else {
                        ""
                    },
                    t,
                ))
                .into_any_element()
        }
    });

    // The row's edge says how it went, quietly: firmer while it runs, a
    // danger tint when it failed.
    let edge = match call.status {
        ToolStatus::Running => t.extra.border_strong,
        ToolStatus::Failed => t.extra.danger.opacity(0.45),
        _ => t.colors.border,
    };
    div()
        .bg(t.colors.surface)
        .border_1()
        .border_color(edge)
        .rounded(t.radius.card)
        .overflow_hidden()
        .child(header)
        .children(body)
        .into_any_element()
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
        Decider::Rule(r) => format!("by the rule {r}"),
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
                ),
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
    use super::button::Button;
    div()
        .flex()
        .flex_col()
        .gap(t.space.md)
        .p(t.space.lg)
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.marigold.opacity(0.55))
        .rounded(t.radius.card)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(theme::pill(
                    t,
                    t.extra.warning_tint,
                    t.extra.warning,
                    "Needs you",
                ))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(title.clone()),
                )
                .child(div().flex_1())
                .when(!is_plan, |d| d.child(env_badge(&cx.place, t))),
        )
        .child(if title.as_ref() == crate::backend::JS_ASK_TITLE {
            // The script, whole and as code: what you allow is what runs.
            div()
                .id(id("script"))
                .max_h(px(240.))
                .overflow_y_scroll()
                .p(t.space.md)
                .rounded(t.radius.button)
                .bg(t.extra.inset)
                .font_family(t.font_mono.clone())
                .text_size(t.text.body_sm)
                .child(super::markdown::code_text(detail, "js", t))
                .into_any_element()
        } else {
            div()
                .text_color(t.colors.text_body)
                .child(detail.clone())
                .into_any_element()
        })
        // A long script scrolls: say so, so its end isn't missed.
        .when(
            title.as_ref() == crate::backend::JS_ASK_TITLE
                && (detail.lines().count() > 10 || detail.len() > 600),
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
                // A tool: deny it, allow it this once (the filled action),
                // or always. The broad grant is never the loudest button.
                .when(!is_plan, |d| {
                    d.child(
                        Button::ghost(id("deny"), "Deny")
                            .with_icon("icons/x.svg")
                            .on_click(answer(Verdict::Denied)),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::secondary(id("always"), "Always allow")
                            .on_click(answer(Verdict::AllowedAlways)),
                    )
                    .child(
                        Button::primary(id("allow"), "Allow once")
                            .on_click(answer(Verdict::Allowed)),
                    )
                }),
        )
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
