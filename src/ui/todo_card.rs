//! The task list as the agent keeps it (`TodoWrite`), in the transcript: a
//! bordered card with one row per task (a filled check when done, the live
//! mark on the one in progress, a dashed circle for what waits), and under
//! it a compact live bar, `··· <current task> X/Y ›`, that opens the side
//! panel's Plan tab. A reply that rewrites its list shows one card, where
//! the list first appeared, always holding the latest version: the card
//! updates in place rather than being added again. The Plan tab reads the
//! same list (`TodoList::latest`).

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::state::{AppState, Block, Message, ToolCall};
use crate::theme::Theme;

/// The tool whose calls are the task list.
pub const TOOL: &str = "TodoWrite";

/// Where a task stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoState {
    Done,
    Active,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    pub state: TodoState,
    pub text: String,
}

/// One version of the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TodoList {
    pub items: Vec<Todo>,
}

pub fn is_todo(call: &ToolCall) -> bool {
    call.target.as_ref() == TOOL
}

impl TodoList {
    /// The list a `TodoWrite` call carries: its rows as `backend::tool_call`
    /// writes them, `[x]` done, `[~]` in progress, `[ ]` to do.
    pub fn from_call(call: &ToolCall) -> Self {
        let items = call
            .diff
            .iter()
            .filter_map(|l| {
                let line = l.text.trim();
                let (state, text) = if let Some(rest) = line.strip_prefix("[x]") {
                    (TodoState::Done, rest)
                } else if let Some(rest) = line.strip_prefix("[~]") {
                    (TodoState::Active, rest)
                } else if let Some(rest) = line.strip_prefix("[ ]") {
                    (TodoState::Pending, rest)
                } else {
                    (TodoState::Pending, line)
                };
                let text = text.trim();
                (!text.is_empty()).then(|| Todo {
                    state,
                    text: text.to_string(),
                })
            })
            .collect();
        Self { items }
    }

    /// The chat's latest list, for the Plan tab.
    pub fn latest(messages: &[Message]) -> Self {
        messages
            .iter()
            .rev()
            .find_map(|m| match m {
                Message::Assistant { blocks } => blocks.iter().rev().find_map(|b| match b {
                    Block::Tool(t) if is_todo(t) => Some(Self::from_call(t)),
                    _ => None,
                }),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn total(&self) -> usize {
        self.items.len()
    }

    pub fn done(&self) -> usize {
        self.items
            .iter()
            .filter(|t| t.state == TodoState::Done)
            .count()
    }

    /// The task being worked on: the one in progress, else the first not
    /// done. `None` once everything is done.
    pub fn current(&self) -> Option<&Todo> {
        self.items
            .iter()
            .find(|t| t.state == TodoState::Active)
            .or_else(|| self.items.iter().find(|t| t.state != TodoState::Done))
    }

    /// What the live bar says: the current task, or how it ended.
    pub fn summary(&self) -> String {
        match self.current() {
            Some(t) => t.text.clone(),
            None if self.total() == 0 => "No tasks".to_string(),
            None => format!("All {} tasks done", self.total()),
        }
    }

    /// `X/Y`, as the bar shows it.
    pub fn count(&self) -> String {
        format!("{}/{}", self.done(), self.total())
    }
}

/// What block `b` of a reply shows, if it is a `TodoWrite` call: the card,
/// holding the reply's latest list (`Some(index of the latest call)`), on
/// the reply's first `TodoWrite`; nothing (`None`) on the later ones, which
/// only update that card.
pub fn slot(blocks: &[Block], b: usize) -> Option<usize> {
    let calls: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, x)| matches!(x, Block::Tool(t) if is_todo(t)))
        .map(|(i, _)| i)
        .collect();
    (calls.first() == Some(&b)).then(|| *calls.last().unwrap_or(&b))
}

/// A task's mark, the same in the reply's card and the side panel's Plan:
/// done, a small solid disc with a check reversed out of it (quiet ink, not
/// a colour); in progress, KodAct's working mark; to do, a thin ring.
pub fn mark(state: TodoState, key: SharedString, t: &Theme) -> AnyElement {
    let size = t.size.icon_sm;
    match state {
        TodoState::Done => {
            icon("icons/circle-check.svg", size, t.colors.text_muted).into_any_element()
        }
        TodoState::Active => super::turning_logo(f32::from(size), key),
        TodoState::Pending => {
            icon("icons/circle.svg", size, t.colors.text_subtle).into_any_element()
        }
    }
}

/// The card and its live bar for `list`; `key` names its animations.
pub fn render(list: &TodoList, key: &str, state: &Entity<AppState>, t: &Theme) -> AnyElement {
    let rows = list.items.iter().enumerate().map(|(i, todo)| {
        let mark = mark(
            todo.state,
            SharedString::from(format!("todo-run-{key}-{i}")),
            t,
        );
        let (fg, weight) = match todo.state {
            TodoState::Done => (t.colors.text_muted, FontWeight::NORMAL),
            TodoState::Active => (t.colors.text_strong, FontWeight::MEDIUM),
            TodoState::Pending => (t.colors.text, FontWeight::NORMAL),
        };
        div()
            .flex()
            .items_start()
            .gap(t.space.sm)
            .child(div().pt(px(2.)).flex_none().child(mark))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(t.text.body_sm)
                    .line_height(relative(1.45))
                    .font_weight(weight)
                    .text_color(fg)
                    .child(todo.text.clone()),
            )
    });
    let card = div()
        .rounded(t.radius.card)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .px(t.space.md)
        .py(t.space.snug)
        .flex()
        .flex_col()
        .gap(t.space.xs)
        .when(list.items.is_empty(), |d| {
            d.child(
                div()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_muted)
                    .child("The task list is empty."),
            )
        })
        .children(rows);

    let all_done = list.total() > 0 && list.done() == list.total();
    let hover = t.colors.hover;
    let opener = state.clone();
    let bar = div()
        .id(SharedString::from(format!("todo-bar-{key}")))
        .role(gpui::accesskit::Role::Button)
        .aria_label(SharedString::from(format!(
            "{}, {} done. Open the plan",
            list.summary(),
            list.count()
        )))
        .tab_index(0)
        .mt(t.space.xs)
        .px(t.space.xs)
        .py(t.space.xs)
        .rounded(t.radius.button)
        .flex()
        .items_center()
        .gap(t.space.sm)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(move |_, _, cx| {
            opener.update(cx, |s, cx| s.show_panel(crate::state::PanelTab::Plan, cx));
        })
        .child(icon(
            "icons/ellipsis.svg",
            t.size.icon_sm,
            if all_done {
                t.extra.success
            } else {
                t.colors.primary
            },
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .child(list.summary()),
        )
        .child(
            div()
                .flex_none()
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(list.count()),
        )
        .child(icon(
            "icons/chevron-right.svg",
            t.size.icon_xs,
            t.colors.text_subtle,
        ));
    div()
        .flex()
        .flex_col()
        .child(card)
        .child(bar)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(todos: serde_json::Value) -> ToolCall {
        crate::backend::tool_call(TOOL, &json!({ "todos": todos }))
    }

    #[test]
    fn the_card_is_built_from_the_todo_write_input() {
        let list = TodoList::from_call(&call(json!([
            {"content": "Read the budget sheet", "status": "completed"},
            {"content": "Update and recalculate", "status": "in_progress"},
            {"content": "Write the memo", "status": "pending"}
        ])));
        assert_eq!(
            list.items,
            vec![
                Todo {
                    state: TodoState::Done,
                    text: "Read the budget sheet".into()
                },
                Todo {
                    state: TodoState::Active,
                    text: "Update and recalculate".into()
                },
                Todo {
                    state: TodoState::Pending,
                    text: "Write the memo".into()
                },
            ]
        );
    }

    #[test]
    fn counts_x_of_y_and_finds_the_current_task() {
        let list = TodoList::from_call(&call(json!([
            {"content": "A", "status": "completed"},
            {"content": "B", "status": "completed"},
            {"content": "C", "status": "in_progress"},
            {"content": "D", "status": "pending"}
        ])));
        assert_eq!((list.done(), list.total()), (2, 4));
        assert_eq!(list.count(), "2/4");
        assert_eq!(list.current().map(|t| t.text.as_str()), Some("C"));
        assert_eq!(list.summary(), "C");

        // Nothing in progress: the first not done is current.
        let waiting = TodoList::from_call(&call(json!([
            {"content": "A", "status": "completed"},
            {"content": "B", "status": "pending"}
        ])));
        assert_eq!(waiting.current().map(|t| t.text.as_str()), Some("B"));

        // All done: no current task, and the bar says so.
        let finished = TodoList::from_call(&call(json!([
            {"content": "A", "status": "completed"}
        ])));
        assert_eq!(finished.current(), None);
        assert_eq!(finished.summary(), "All 1 tasks done");
        assert_eq!(finished.count(), "1/1");
    }

    #[test]
    fn a_reply_shows_one_card_with_its_latest_list() {
        let first = call(json!([{"content": "A", "status": "in_progress"}]));
        let read = crate::backend::tool_call("Read", &json!({"file_path": "a.rs"}));
        let second = call(json!([{"content": "A", "status": "completed"}]));
        let blocks = vec![Block::Tool(first), Block::Tool(read), Block::Tool(second)];
        // The first call holds the card, showing the latest list (block 2).
        assert_eq!(slot(&blocks, 0), Some(2));
        // The later call adds nothing; non-todo blocks have no slot.
        assert_eq!(slot(&blocks, 2), None);
        assert_eq!(slot(&blocks, 1), None);
    }

    #[test]
    fn the_plan_tab_reads_the_chats_latest_list() {
        let messages = vec![
            Message::Assistant {
                blocks: vec![Block::Tool(call(
                    json!([{"content": "Old", "status": "pending"}]),
                ))],
            },
            Message::Assistant {
                blocks: vec![Block::Tool(call(json!([
                    {"content": "New", "status": "completed"},
                    {"content": "Next", "status": "pending"}
                ])))],
            },
        ];
        let list = TodoList::latest(&messages);
        assert_eq!(list.count(), "1/2");
        assert_eq!(list.items[0].text, "New");
        assert_eq!(TodoList::latest(&[]), TodoList::default());
    }
}
