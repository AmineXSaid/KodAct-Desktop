//! One transcript item, as Forge draws it:
//! - your message: a white card on the right;
//! - a reply: a timeline row whose dot shows its first tool call's status,
//!   holding text (markdown), thinking (a disclosure), tool calls (one row
//!   each, opening to the diff or the output) and permission prompts;
//! - the end of a turn: how long it took and what it cost;
//! - a notice (interrupted, an error).

use std::time::Duration;

use gpui::{
    div, percentage, prelude::*, px, relative, Animation, AnimationExt, AnyElement, Entity, FontWeight, Hsla,
    SharedString, Transformation,
};

use super::icon;
use super::markdown::{self, CopyContext};
use crate::state::{AppState, Block, DiffKind, Message, ToolCall, ToolStatus, Verdict};
use crate::theme::{self, Theme};

pub struct ItemContext {
    pub state: Entity<AppState>,
    pub chat: usize,
    pub index: usize,
    pub copied: Option<SharedString>,
    pub busy: bool,
    pub last: bool,
}

pub fn render(m: &Message, cx: &ItemContext, t: &Theme) -> AnyElement {
    match m {
        Message::User { text } => div()
            .flex()
            .justify_end()
            .child(
                div()
                    .max_w(relative(0.85))
                    .px(t.space.lg)
                    .py(t.space.md)
                    .bg(t.colors.surface)
                    .border_1()
                    .border_color(t.colors.border)
                    .rounded(t.radius.card)
                    .text_size(t.text.body)
                    .line_height(relative(t.size.line_height_chat))
                    .child(text.clone()),
            )
            .into_any_element(),
        Message::Assistant { blocks } => assistant(m, blocks, cx, t),
        Message::TurnEnd { seconds, cost } => div()
            .pl(t.space.xl)
            .text_size(t.text.caption)
            .text_color(t.colors.text_subtle)
            .child(format!("{seconds:.1}s · ${cost:.4}"))
            .into_any_element(),
        Message::Notice { text, error } => div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .px(t.space.md)
            .py(t.space.sm)
            .rounded(t.radius.card)
            .bg(if *error { t.extra.danger_tint } else { t.colors.hover })
            .text_color(if *error { t.extra.danger } else { t.colors.text_muted })
            .child(icon(if *error { "icons/triangle-alert.svg" } else { "icons/circle-pause.svg" }, t.size.icon_sm, if *error { t.extra.danger } else { t.colors.text_muted }))
            .child(text.clone())
            .into_any_element(),
    }
}

fn dot_color(status: Option<ToolStatus>, t: &Theme) -> Hsla {
    match status {
        Some(ToolStatus::Done) => t.extra.success,
        Some(ToolStatus::Failed) => t.extra.danger,
        Some(ToolStatus::Declined) => t.colors.text_subtle,
        Some(ToolStatus::Running) => t.colors.sky,
        None => t.colors.text_subtle,
    }
}

fn assistant(m: &Message, blocks: &[Block], cx: &ItemContext, t: &Theme) -> AnyElement {
    let gutter = t.space.xl;
    div()
        .flex()
        .gap(t.space.xs)
        .child(
            // The timeline: a dot level with the first line.
            div()
                .w(gutter - t.space.xs)
                .flex_none()
                .flex()
                .justify_center()
                .pt(t.space.sm + px(2.))
                .child(div().size(t.size.dot).rounded(t.radius.pill).bg(dot_color(m.dot(), t))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(t.space.md)
                .children(blocks.iter().enumerate().map(|(b, block)| self::block(block, b, cx, t))),
        )
        .into_any_element()
}

fn block(block: &Block, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    match block {
        Block::Text(text) => markdown::render(text, t, &CopyContext { state: cx.state.clone(), key: format!("{}-{}-{b}", cx.chat, cx.index), copied: cx.copied.clone() }),
        Block::Thinking { text, open } => thinking(text, *open, b, cx, t),
        Block::Tool(call) => tool(call, b, cx, t),
        Block::Permission { title, detail, verdict } => permission(title, detail, *verdict, b, cx, t),
    }
}

fn toggle(cx: &ItemContext, b: usize) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    let (state, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
    move |_, _, app| state.update(app, |s, cx| s.toggle_block(chat, msg, b, cx))
}

fn chevron(open: bool, t: &Theme) -> gpui::Svg {
    icon(if open { "icons/chevron-down.svg" } else { "icons/chevron-right.svg" }, t.size.icon_sm, t.colors.text_subtle)
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
                .id(SharedString::from(format!("think-{}-{}-{b}", cx.chat, cx.index)))
                .flex()
                .items_center()
                .gap(t.space.xs)
                .cursor_pointer()
                .text_color(t.colors.text_muted)
                .hover(move |s| s.text_color(hover))
                .child(icon("icons/brain.svg", t.size.icon_sm, t.colors.text_subtle))
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
                    .text_size(t.text.body_sm)
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
            .with_animation("tool-running", Animation::new(Duration::from_millis(900)).repeat(), |svg, d| svg.with_transformation(Transformation::rotate(percentage(d))))
            .into_any_element(),
        ToolStatus::Done => icon("icons/circle-check.svg", t.size.icon_sm, t.extra.success).into_any_element(),
        ToolStatus::Failed => icon("icons/circle-x.svg", t.size.icon_sm, t.extra.danger).into_any_element(),
        ToolStatus::Declined => icon("icons/circle-slash.svg", t.size.icon_sm, t.colors.text_subtle).into_any_element(),
    }
}

fn tool(call: &ToolCall, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    let hover = t.colors.hover;
    let adds = call.diff.iter().filter(|l| l.kind == DiffKind::Add).count();
    let dels = call.diff.iter().filter(|l| l.kind == DiffKind::Del).count();
    let has_body = !call.diff.is_empty() || !call.output.is_empty();
    let header = div()
        .id(SharedString::from(format!("tool-{}-{}-{b}", cx.chat, cx.index)))
        .flex()
        .items_center()
        .gap(t.space.sm)
        .px(t.space.md)
        .h(t.size.row + px(4.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(call.kind.icon(), t.size.icon_sm, t.colors.text_muted))
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).child(call.kind.verb(call.status)))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption + px(0.5))
                .text_color(t.colors.text_muted)
                .child(call.target.clone()),
        )
        .children(call.note.clone().map(|n| div().flex_none().text_color(t.colors.text_subtle).text_size(t.text.caption).child(n)))
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
        .child(status_icon(call.status, t))
        .when(has_body, |d| d.child(chevron(call.open, t)))
        .on_click(toggle(cx, b));

    let body = (call.open && has_body).then(|| {
        if !call.diff.is_empty() {
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
                        .child(div().whitespace_nowrap().child(if l.text.is_empty() { SharedString::from(" ") } else { l.text.clone() }))
                }))
                .into_any_element()
        } else {
            div()
                .border_t_1()
                .border_color(t.colors.border)
                .flex()
                .flex_col()
                .child(io_row("IN", call.target.clone(), t))
                .child(io_row("OUT", call.output.clone(), t))
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

/// Forge's tool I/O row: a small label, then the text in a quiet well.
fn io_row(label: &'static str, text: SharedString, t: &Theme) -> impl IntoElement {
    div()
        .flex()
        .gap(t.space.md)
        .px(t.space.md)
        .py(t.space.sm)
        .child(div().w(px(30.)).flex_none().pt(px(2.)).text_size(t.text.caption - px(1.)).font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_subtle).child(label))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px(t.space.md)
                .py(t.space.sm)
                .rounded(t.radius.button)
                .bg(t.extra.inset)
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption + px(0.5))
                .line_height(relative(1.6))
                .text_color(t.colors.text)
                .child(text),
        )
}

fn permission(title: &SharedString, detail: &SharedString, verdict: Verdict, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    let answer = |v: Verdict| {
        let (state, chat, msg) = (cx.state.clone(), cx.chat, cx.index);
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, app: &mut gpui::App| state.update(app, |s, cx| s.answer(chat, msg, b, v, cx))
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
                .child(theme::pill(t, t.extra.warning_tint, t.extra.warning, "Needs you"))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child(title.clone())),
        )
        .child(div().text_color(t.colors.text_body).child(detail.clone()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(theme::text_button(t, "Deny").id(id("deny")).on_click(answer(Verdict::Denied)))
                .child(div().flex_1())
                .child(theme::ghost_button(t, "Always allow").id(id("always")).hover(move |s| s.bg(hover)).on_click(answer(Verdict::AllowedAlways)))
                .child(theme::primary_button(t, "Allow").id(id("allow")).on_click(answer(Verdict::Allowed))),
        )
        .into_any_element()
}
