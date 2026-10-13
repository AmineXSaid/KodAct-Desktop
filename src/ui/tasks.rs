//! The task panel: the latest turn's tool calls and sub-agents, in one
//! place. Collapsed (the default) it is a small pill over the transcript's
//! top-right corner: how many are running, and how many in all; open, it
//! lists each with its status, and a click shows that row in the transcript.

use std::time::Duration;

use gpui::{
    div, percentage, prelude::*, px, Animation, AnimationExt, AnyElement, Entity, FontWeight,
    SharedString, Transformation,
};

use super::icon;
use crate::state::{AppState, Block, Message, ToolKind, ToolStatus};
use crate::theme::{self, Theme};

/// One task: where its row is, and how it reads.
pub struct Task {
    pub msg: usize,
    pub kind: ToolKind,
    pub status: ToolStatus,
    pub label: String,
}

/// The tasks of the newest turn, in order.
pub fn tasks(messages: &[Message]) -> Vec<Task> {
    let from = messages
        .iter()
        .rposition(|m| matches!(m, Message::User { .. }))
        .map_or(0, |i| i + 1);
    let mut out = vec![];
    for (i, m) in messages.iter().enumerate().skip(from) {
        if let Message::Assistant { blocks } = m {
            for b in blocks {
                if let Block::Tool(t) = b {
                    // A command reads as its description ("Run the tests"), as its row does.
                    let label = match (&t.note, t.kind) {
                        (Some(n), ToolKind::Bash) => n.to_string(),
                        (Some(n), ToolKind::Other) => format!("{} · {n}", t.target),
                        _ => format!("{} {}", t.kind.verb(t.status), t.target),
                    };
                    out.push(Task {
                        msg: i,
                        kind: t.kind,
                        status: t.status,
                        label,
                    });
                }
            }
        }
    }
    out
}

fn status_icon(status: ToolStatus, t: &Theme, key: SharedString) -> AnyElement {
    match status {
        ToolStatus::Running => icon("icons/loader-circle.svg", t.size.icon_sm, t.colors.sky)
            .with_animation(
                key,
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

/// The pill, and the list when open; None when the turn has no tasks.
pub fn render(t: &Theme, state: &Entity<AppState>, cx: &gpui::App) -> Option<AnyElement> {
    let s = state.read(cx);
    let chat = s.chat()?;
    let list = tasks(&chat.messages);
    if list.is_empty() {
        return None;
    }
    let running = list
        .iter()
        .filter(|t| t.status == ToolStatus::Running)
        .count();
    let failed = list
        .iter()
        .filter(|t| t.status == ToolStatus::Failed)
        .count();
    let open = s.tasks_open;
    let hover = t.colors.hover;
    let toggle = state.clone();
    let summary = match (running, failed) {
        (0, 0) => format!(
            "{} {} done",
            list.len(),
            if list.len() == 1 { "task" } else { "tasks" }
        ),
        (0, f) => format!("{} tasks · {f} failed", list.len()),
        (r, _) => format!(
            "{r} running · {} {}",
            list.len(),
            if list.len() == 1 { "task" } else { "tasks" }
        ),
    };
    let pill = div()
        .id("tasks-pill")
        .flex()
        .items_center()
        .gap(t.space.xs)
        .h(t.size.control)
        .px(t.space.md)
        .rounded(t.radius.pill)
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .shadow(t.elevation_popover())
        .cursor_pointer()
        .hover(move |d| d.bg(hover))
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text)
        .child(if running > 0 {
            status_icon(ToolStatus::Running, t, "tasks-pill-spin".into())
        } else {
            icon("icons/list-todo.svg", t.size.icon_sm, t.colors.text_muted).into_any_element()
        })
        .child(summary)
        .child(icon(
            if open {
                "icons/chevron-up.svg"
            } else {
                "icons/chevron-down.svg"
            },
            t.size.icon_xs,
            t.colors.text_subtle,
        ))
        .on_click(move |_, _, cx| toggle.update(cx, |s, cx| s.toggle_tasks(cx)));
    let panel = open.then(|| {
        let mut rows = div()
            .id("tasks-list")
            .flex()
            .flex_col()
            .max_h(t.size.popover_list)
            .overflow_y_scroll();
        for (i, task) in list.iter().enumerate() {
            let (st, msg) = (state.clone(), task.msg);
            rows = rows.child(
                div()
                    .id(("task", i))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .h(t.size.row)
                    .px(t.space.sm)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .hover(move |d| d.bg(hover))
                    .child(status_icon(
                        task.status,
                        t,
                        SharedString::from(format!("task-spin-{i}")),
                    ))
                    .child(icon(task.kind.icon(), t.size.icon_xs, t.colors.text_subtle))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text)
                            .child(task.label.clone()),
                    )
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.jump_to(msg, cx))),
            );
        }
        theme::popover(t)
            .w(px(340.))
            .child(
                div()
                    .px(t.space.sm)
                    .pt(t.space.xs)
                    .pb(t.space.xs)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child("This turn's tasks · click one to see it"),
            )
            .child(rows)
    });
    Some(
        div()
            .absolute()
            .top(t.space.sm)
            .right(t.space.lg)
            .flex()
            .flex_col()
            .items_end()
            .gap(t.space.xs)
            .child(pill)
            .children(panel)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Review, ToolCall};

    fn tool(kind: ToolKind, target: &str, status: ToolStatus) -> Block {
        Block::Tool(ToolCall {
            kind,
            target: target.into(),
            note: None,
            status,
            output: "".into(),
            diff: vec![],
            open: false,
            full: false,
            review: Review::Open,
        })
    }

    #[test]
    fn only_the_newest_turn_counts() {
        let m = vec![
            Message::user("one".into(), std::time::SystemTime::now()),
            Message::Assistant {
                blocks: vec![tool(ToolKind::Read, "old.rs", ToolStatus::Done)],
            },
            Message::user("two".into(), std::time::SystemTime::now()),
            Message::Assistant {
                blocks: vec![
                    tool(ToolKind::Edit, "a.rs", ToolStatus::Done),
                    tool(ToolKind::Bash, "cargo test", ToolStatus::Running),
                ],
            },
        ];
        let t = tasks(&m);
        assert_eq!(
            t.iter()
                .map(|t| (t.msg, t.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(3, "Edited a.rs"), (3, "Running cargo test")]
        );
    }
}
