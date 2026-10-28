//! The task panel: the latest turn's tool calls and sub-agents, in one
//! place. Collapsed (the default) it is a small pill over the transcript's
//! top-right corner: how many are running, and how many in all; open, it
//! lists each with its status, and a click shows that row in the transcript.

use std::time::Duration;

use gpui::{
    div, percentage, prelude::*, px, Animation, AnimationExt, AnyElement, Entity, SharedString,
    Transformation,
};

use super::icon;
use crate::state::{AppState, Block, Message, ToolKind, ToolStatus};
use crate::theme::{self, Theme};

/// One task: where its row is, and how it reads.
pub struct Task {
    pub msg: usize,
    /// Its row's block in that message.
    pub block: usize,
    pub kind: ToolKind,
    pub status: ToolStatus,
    pub label: String,
    /// An agent you stopped (its status reads Declined, like a denial).
    pub stopped: bool,
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
            for (block, b) in blocks.iter().enumerate() {
                if let Block::Tool(t) = b {
                    // A command reads as its description ("Run the tests"), as its row does.
                    let label = match (&t.note, t.kind) {
                        (Some(n), _) if t.target.as_ref() == crate::backend::QUESTION_TOOL => {
                            format!("Asked you {n}")
                        }
                        (Some(n), ToolKind::Bash) => n.to_string(),
                        (Some(n), ToolKind::Other) => format!("{} · {n}", t.shown_target()),
                        _ => format!("{} {}", t.kind.verb(t.status), t.shown_target()),
                    };
                    out.push(Task {
                        msg: i,
                        block,
                        kind: t.kind,
                        status: t.status,
                        label,
                        stopped: t.status == ToolStatus::Declined
                            && t.output.starts_with(crate::state::AGENT_STOPPED),
                    });
                }
            }
        }
    }
    out
}

pub(super) fn status_icon(status: ToolStatus, t: &Theme, key: SharedString) -> AnyElement {
    match status {
        ToolStatus::Running => icon("icons/loader-circle.svg", t.size.icon_sm, t.extra.live)
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
        // Stopped mid-way: neither failed nor done; it may have taken effect.
        ToolStatus::Interrupted => {
            icon("icons/circle-pause.svg", t.size.icon_sm, t.extra.warning).into_any_element()
        }
    }
}

/// The pill's words: what is running, else what each task came to.
fn summary(all: usize, running: usize, failed: usize, denied: usize, stopped: usize) -> String {
    let noun = if all == 1 { "task" } else { "tasks" };
    if running > 0 {
        return format!("{running} running · {all} {noun}");
    }
    let done = all - failed - denied - stopped;
    let mut parts = vec![];
    if done > 0 {
        parts.push(format!("{done} done"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    if denied > 0 {
        parts.push(format!("{denied} denied"));
    }
    if stopped > 0 {
        parts.push(format!("{stopped} stopped"));
    }
    if parts.len() == 1 && done > 0 {
        return format!("{done} {noun} done");
    }
    parts.join(" · ")
}

/// The pill, and the list when open; None when the turn has no tasks.
pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    window: &gpui::Window,
    // The list: Some(leaving) while it shows or leaves.
    shown: Option<bool>,
    cx: &gpui::App,
) -> Option<AnyElement> {
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
    // Declined is not done: you said no, or you stopped the agent.
    let stopped = list
        .iter()
        .filter(|t| t.stopped || t.status == ToolStatus::Interrupted)
        .count();
    let denied = list
        .iter()
        .filter(|t| t.status == ToolStatus::Declined && !t.stopped)
        .count();
    // Only while there is something to watch: work running, or something
    // that failed or was stopped. Ctrl+Shift+T shows it any time.
    if running + failed + stopped == 0 && !open {
        return None;
    }
    let summary = summary(list.len(), running, failed, denied, stopped);
    // A quiet title-bar button, never floating over the conversation.
    let pill = div()
        .id("tasks-pill")
        .occlude()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .h(t.size.control)
        .px(t.space.sm)
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |d| d.bg(hover))
        .text_size(t.text.caption)
        .text_color(if failed > 0 {
            t.extra.danger
        } else {
            t.colors.text_muted
        })
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
    let panel = shown.map(|leaving| {
        let mut rows = div()
            .id("tasks-list")
            .flex()
            .flex_col()
            .max_h(t.size.popover_list)
            .overflow_y_scroll()
            // More tasks than fit: the list keeps the wheel from the chat under it.
            .when(list.len() > 8, super::hold_wheel);
        for (i, task) in list.iter().enumerate() {
            let (st, msg, block) = (state.clone(), task.msg, task.block);
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
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| s.jump_to_block(msg, block, cx))
                    }),
            );
        }
        let close = state.clone();
        gpui::deferred(super::motion::unfold(
            theme::fit_window(
                theme::popover(t).id("tasks-popover"),
                window,
                t,
                t.size.titlebar + t.size.control,
                None,
            )
            .occlude()
            .absolute()
            .top(t.size.control + t.space.xs)
            .right_0()
            .w(px(340.))
            .on_mouse_down_out(move |_, _, cx| {
                close.update(cx, |s, cx| {
                    if s.tasks_open {
                        s.tasks_open = false;
                        cx.notify();
                    }
                })
            })
            .child(
                div()
                    .px(t.space.sm)
                    .pt(t.space.xs)
                    .pb(t.space.xs)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child("This turn's tasks · click one to see it"),
            )
            .child(rows),
            "tasks-popover",
            false,
            leaving,
            t,
        ))
        .with_priority(2)
    });
    Some(
        div()
            .relative()
            .child(pill)
            .children(panel)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Review, ToolCall};

    #[test]
    fn the_pill_says_what_each_task_came_to() {
        assert_eq!(summary(3, 2, 0, 0, 0), "2 running · 3 tasks");
        assert_eq!(summary(1, 0, 0, 0, 0), "1 task done");
        assert_eq!(summary(2, 0, 0, 1, 0), "1 done · 1 denied");
        assert_eq!(summary(3, 0, 0, 0, 3), "3 stopped");
        assert_eq!(summary(4, 0, 1, 0, 1), "2 done · 1 failed · 1 stopped");
    }

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
            images: vec![],
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
                .map(|t| (t.msg, t.block, t.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(3, 0, "Edited a.rs"), (3, 1, "Running cargo test")]
        );
    }
}
