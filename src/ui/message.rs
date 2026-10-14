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
    AppState, Block, DiffKind, Message, Rating, Review, ToolCall, ToolKind, ToolStatus, Verdict,
};
use crate::theme::{self, Theme};

pub struct ItemContext {
    pub state: Entity<AppState>,
    pub chat: usize,
    pub index: usize,
    pub copied: Option<SharedString>,
    pub busy: bool,
    pub last: bool,
    /// The first reply after your message: it carries the turn's header.
    pub first_in_turn: bool,
    /// This turn is the one still running.
    pub live_turn: bool,
    /// The colour the cube is painted while this turn runs (the mode's).
    pub cube: gpui::Hsla,
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
        Message::TurnEnd { seconds, .. } => turn_footer(*seconds, cx, t),
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
        .size(t.size.control - px(4.))
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
fn turn_footer(seconds: f32, cx: &ItemContext, t: &Theme) -> AnyElement {
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
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(format!("{seconds:.1}s")),
        )
        .into_any_element()
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

/// The head of a turn: Forge's cube and name. The cube is Forge for VS
/// Code's: struck into shape while the turn runs, finished and still after.
fn turn_header(cx: &ItemContext, t: &Theme) -> AnyElement {
    const CUBE: f32 = 20.;
    let mark = if cx.live_turn {
        super::forge_cube::cube(
            t,
            px(CUBE),
            cx.cube,
            SharedString::from(format!("turn-cube-{}-{}", cx.chat, cx.index)),
        )
    } else {
        super::forge_cube::finished(t, px(CUBE), t.colors.primary)
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
    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .when(cx.first_in_turn, |d| d.child(turn_header(cx, t)))
        .children(
            blocks
                .iter()
                .enumerate()
                .map(|(b, block)| self::block(block, b, cx, t)),
        )
        .into_any_element()
}

fn block(block: &Block, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
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
            },
        ),
        Block::Thinking { text, open } => thinking(text, *open, b, cx, t),
        Block::Tool(call) => tool(call, b, cx, t),
        Block::Permission {
            title,
            detail,
            verdict,
        } => permission(title, detail, *verdict, b, cx, t),
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
        ToolStatus::Running => icon("icons/loader-circle.svg", t.size.icon_sm, t.colors.sky)
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
        .flex()
        .items_center()
        .gap(t.space.sm)
        .px(t.space.md)
        .h(t.size.row + px(4.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
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
                        .child(call.kind.verb(call.status)),
                )
                .child(
                    div()
                        .min_w_0()
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
                        .flex_none()
                        .text_color(t.colors.text_subtle)
                        .text_size(t.text.caption)
                        .child(n)
                })),
        })
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

    div()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
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

fn permission(
    title: &SharedString,
    detail: &SharedString,
    verdict: Verdict,
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
    if verdict != Verdict::Pending {
        let (label, bg, fg) = match verdict {
            Verdict::Denied => ("Denied", t.extra.danger_tint, t.extra.danger),
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
    let hover = t.colors.hover;
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
                ),
        )
        .child(div().text_color(t.colors.text_body).child(detail.clone()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(
                    theme::text_button(t, "Deny")
                        .id(id("deny"))
                        .on_click(answer(Verdict::Denied)),
                )
                .child(div().flex_1())
                .child(
                    theme::ghost_button(t, "Always allow")
                        .id(id("always"))
                        .hover(move |s| s.bg(hover))
                        .on_click(answer(Verdict::AllowedAlways)),
                )
                .child(
                    theme::primary_button(t, "Allow")
                        .id(id("allow"))
                        .on_click(answer(Verdict::Allowed)),
                ),
        )
        .into_any_element()
}
