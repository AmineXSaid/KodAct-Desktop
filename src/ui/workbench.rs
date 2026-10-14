//! The side panel's own tabs, shown when no file is open in it: the chat's
//! **Plan** (the plan it proposed and its task list), its **Files** (every
//! Markdown file, workbook, PDF, Word file, deck and diagram it made, each
//! opening in the file view, `doc_pane.rs`) and the **Browser** (the page
//! the browser is on, its latest screenshot and its recent steps).
//!
//! Everything here is read from the chat's blocks, so a chat reloaded from
//! disk shows its plan and files again; only the screenshot is kept in memory.

use gpui::{
    div, img, prelude::*, px, relative, AnyElement, Entity, FontWeight, ObjectFit, SharedString,
};

use super::icon;
use super::markdown::{self, CopyContext, Look};
use crate::state::{AppState, Block, FileRef, Message, PanelTab, ToolStatus};
use crate::theme::Theme;

/// The chat's latest plan, as Markdown.
pub fn plan(messages: &[Message]) -> Option<String> {
    tools(messages)
        .filter(|t| t.target == "ExitPlanMode")
        .last()
        .map(|t| {
            t.diff
                .iter()
                .map(|l| l.text.as_ref())
                .collect::<Vec<_>>()
                .join("\n")
        })
}

/// The chat's latest task list: (status, text).
pub fn todos(messages: &[Message]) -> Vec<(ToolStatus, String)> {
    let Some(list) = tools(messages).filter(|t| t.target == "TodoWrite").last() else {
        return vec![];
    };
    list.diff
        .iter()
        .filter_map(|l| {
            let (mark, text) = l.text.split_at_checked(3)?;
            let status = match mark {
                "[x]" => ToolStatus::Done,
                "[~]" => ToolStatus::Running,
                _ => ToolStatus::Declined,
            };
            Some((status, text.trim().to_string()))
        })
        .collect()
}

/// Every file the chat made, newest first, each name once.
pub fn files(messages: &[Message]) -> Vec<FileRef> {
    let mut out: Vec<FileRef> = vec![];
    for m in messages.iter().rev() {
        if let Message::Assistant { blocks } = m {
            for b in blocks.iter().rev() {
                if let Block::File(f) = b {
                    if !out.iter().any(|o| o.name == f.name) {
                        out.push(f.clone());
                    }
                }
            }
        }
    }
    out
}

/// The browser's steps in the chat, oldest first: (action, what, status).
pub fn browser_steps(messages: &[Message]) -> Vec<(String, Option<String>, ToolStatus)> {
    tools(messages)
        .filter_map(|t| {
            let action = t.target.strip_prefix(crate::browser::TOOL_PREFIX)?;
            let action = action
                .strip_prefix("browser_")
                .unwrap_or(action)
                .replace('_', " ");
            Some((action, t.note.as_ref().map(|n| n.to_string()), t.status))
        })
        .collect()
}

/// The page the browser is on: the last URL it navigated to or reported.
pub fn browser_url(messages: &[Message]) -> Option<String> {
    tools(messages)
        .filter(|t| t.target.starts_with(crate::browser::TOOL_PREFIX))
        .filter_map(|t| {
            if t.target.ends_with("_get_state") {
                serde_json::from_str::<serde_json::Value>(&t.output)
                    .ok()
                    .and_then(|v| v["url"].as_str().map(str::to_string))
            } else if t.target.ends_with("_navigate") {
                t.note.as_ref().map(|n| n.to_string())
            } else {
                None
            }
        })
        .last()
}

fn tools(messages: &[Message]) -> impl Iterator<Item = &crate::state::ToolCall> {
    messages
        .iter()
        .flat_map(|m| match m {
            Message::Assistant { blocks } => blocks.as_slice(),
            _ => &[],
        })
        .filter_map(|b| match b {
            Block::Tool(t) => Some(t),
            _ => None,
        })
}

fn tab(
    t: &Theme,
    state: &Entity<AppState>,
    which: PanelTab,
    label: String,
    on: bool,
) -> AnyElement {
    let state = state.clone();
    let hover = t.colors.hover;
    div()
        .id(SharedString::from(format!("panel-tab-{which:?}")))
        .px(t.space.md)
        .h(t.size.control)
        .flex()
        .items_center()
        .rounded(t.radius.button)
        .cursor_pointer()
        .text_size(t.text.body_sm)
        .font_weight(if on {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(if on {
            t.colors.text_strong
        } else {
            t.colors.text_muted
        })
        .when(on, |d| d.bg(t.colors.hover))
        .when(!on, |d| d.hover(move |s| s.bg(hover)))
        .child(label)
        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.show_panel(which, cx)))
        .into_any_element()
}

fn icon_button(
    t: &Theme,
    id: &'static str,
    path: &'static str,
    tip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex_none()
        .size(t.size.control)
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |st| st.bg(hover))
        .child(icon(path, t.size.icon, t.colors.text_muted))
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
}

/// A quiet note in the middle of an empty tab, with an optional action.
fn empty(
    t: &Theme,
    path: &'static str,
    title: &str,
    body: &str,
    action: Option<AnyElement>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(t.space.sm)
        .px(t.space.xxl)
        .pt(t.space.xxl * 2.)
        .text_center()
        .child(icon(path, t.size.icon_lg, t.colors.text_subtle))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_strong)
                .child(title.to_string()),
        )
        .child(
            div()
                .max_w(px(320.))
                .text_size(t.text.body_sm)
                .line_height(relative(1.5))
                .text_color(t.colors.text_muted)
                .child(body.to_string()),
        )
        .children(action.map(|a| div().pt(t.space.sm).child(a)))
        .into_any_element()
}

fn section_title(t: &Theme, text: &str) -> gpui::Div {
    div()
        .text_size(t.text.caption)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.text_muted)
        .child(text.to_uppercase())
}

pub struct View<'a> {
    pub tab: PanelTab,
    pub messages: &'a [Message],
    pub shot: Option<std::sync::Arc<gpui::Image>>,
    pub browser_on: bool,
    pub copied: Option<SharedString>,
    pub width: f32,
    pub below: bool,
    pub scale: f32,
}

pub fn render(v: View, state: Entity<AppState>, t: &Theme) -> AnyElement {
    let files = files(v.messages);
    let plan = plan(v.messages);
    let todos = todos(v.messages);
    let steps = browser_steps(v.messages);
    let count = |n: usize| {
        if n == 0 {
            String::new()
        } else {
            format!(" {n}")
        }
    };

    let closer = state.clone();
    let header = div()
        .flex_none()
        .h(px(48.))
        .flex()
        .items_center()
        .gap(t.space.xs)
        .pl(t.space.md)
        .pr(t.space.sm)
        .border_b_1()
        .border_color(t.colors.border)
        .child(tab(
            t,
            &state,
            PanelTab::Plan,
            "Plan".into(),
            v.tab == PanelTab::Plan,
        ))
        .child(tab(
            t,
            &state,
            PanelTab::Files,
            format!("Files{}", count(files.len())),
            v.tab == PanelTab::Files,
        ))
        .child(tab(
            t,
            &state,
            PanelTab::Browser,
            "Browser".into(),
            v.tab == PanelTab::Browser,
        ))
        .child(div().flex_1())
        .child(
            icon_button(
                t,
                "panel-split",
                "icons/split.svg",
                if v.below {
                    "Put it beside the chat"
                } else {
                    "Put it below the chat"
                },
            )
            .on_click(|_, window, cx| window.dispatch_action(Box::new(super::ToggleSplit), cx)),
        )
        .child(
            icon_button(t, "panel-close", "icons/x.svg", "Close")
                .on_click(move |_, _, cx| closer.update(cx, |s, cx| s.close_panel(cx))),
        );

    let pad = |d: gpui::Div| {
        d.flex()
            .flex_col()
            .gap(t.space.lg)
            .px(t.space.xl)
            .pt(t.space.xl)
            .pb(t.space.xxl * 2.)
    };
    let body: AnyElement = match v.tab {
        PanelTab::Plan if plan.is_none() && todos.is_empty() => empty(
            t,
            "icons/route.svg",
            "No plan yet",
            "Switch the mode to Plan (Shift+Tab in the composer) and ask for something: the plan Forge proposes, and its task list as it works, show here.",
            None,
        ),
        PanelTab::Plan => pad(div())
            .when(!todos.is_empty(), |d| {
                let done = todos.iter().filter(|(s, _)| *s == ToolStatus::Done).count();
                d.child(section_title(t, &format!("Tasks · {done} of {}", todos.len())))
                    .child(div().flex().flex_col().gap(t.space.sm).children(todos.iter().enumerate().map(
                        |(i, (status, text))| {
                            div()
                                .flex()
                                .items_start()
                                .gap(t.space.sm)
                                .child(div().pt(px(2.)).child(match status {
                                    ToolStatus::Declined => icon("icons/circle-dashed.svg", t.size.icon_sm, t.colors.text_subtle)
                                        .into_any_element(),
                                    s => super::tasks::status_icon(*s, t, format!("todo-{i}").into()),
                                }))
                                .child(
                                    div()
                                        .text_size(t.text.body_sm)
                                        .line_height(relative(1.45))
                                        .text_color(if *status == ToolStatus::Done {
                                            t.colors.text_muted
                                        } else {
                                            t.colors.text
                                        })
                                        .when(*status == ToolStatus::Done, |d| d.line_through())
                                        .child(text.clone()),
                                )
                        },
                    )))
            })
            .children(plan.map(|md| {
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.sm)
                    .child(section_title(t, "Plan"))
                    .child(markdown::render_as(
                        &md,
                        t,
                        &CopyContext {
                            state: state.clone(),
                            key: "panel-plan".into(),
                            copied: v.copied.clone(),
                            scale: v.scale,
                            sources: Default::default(),
                        },
                        Look::Document,
                    ))
            }))
            .into_any_element(),
        PanelTab::Files if files.is_empty() => empty(
            t,
            "icons/folder-open.svg",
            "No files yet",
            "Documents, spreadsheets, PDFs, decks and diagrams Forge makes in this chat are listed here. Click one to read it in this panel.",
            None,
        ),
        PanelTab::Files => pad(div()).gap(t.space.xs).children(files.into_iter().enumerate().map(|(i, f)| {
            let opener = state.clone();
            let hover = t.colors.hover;
            let mark: AnyElement = match f.office() {
                Some(kind) => super::file_mark::render(kind, f32::from(t.size.icon_lg), v.scale),
                None => icon(
                    if f.is_diagram() { "icons/network.svg" } else { "icons/file-text.svg" },
                    t.size.icon,
                    t.colors.text_muted,
                )
                .into_any_element(),
            };
            div()
                .id(("panel-file", i))
                .flex()
                .items_center()
                .gap(t.space.md)
                .px(t.space.md)
                .py(t.space.sm)
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(div().flex_none().w(t.size.icon_lg).flex().justify_center().child(mark))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.body_sm)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.colors.text_strong)
                                .child(f.title()),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(format!(
                                    "{} · {}",
                                    f.kind(),
                                    f.path.clone().unwrap_or_else(|| f.name.clone())
                                )),
                        ),
                )
                .child(icon("icons/chevron-right.svg", t.size.icon_sm, t.colors.text_subtle))
                .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(f.clone(), cx)))
        }))
        .into_any_element(),
        PanelTab::Browser if !v.browser_on && steps.is_empty() => empty(
            t,
            "icons/globe.svg",
            "The browser is off",
            "Turn it on and Forge can open pages, click, type and take screenshots. What it sees shows here as it works.",
            Some(
                crate::theme::ghost_button(t, "Turn on the browser")
                    .id("panel-browser-on")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(super::ToggleBrowser), cx))
                    .into_any_element(),
            ),
        ),
        PanelTab::Browser if steps.is_empty() && v.shot.is_none() => empty(
            t,
            "icons/globe.svg",
            "Nothing browsed yet",
            "Ask Forge to open a page, fill a form or check a site. The page it is on, its latest screenshot and each step show here.",
            None,
        ),
        PanelTab::Browser => {
            let url = browser_url(v.messages);
            let room = v.width - 2. * f32::from(t.space.xl) - 2.;
            pad(div())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .h(t.size.control + px(6.))
                        .px(t.space.md)
                        .rounded(t.radius.pill)
                        .bg(t.extra.inset)
                        .border_1()
                        .border_color(t.colors.border)
                        .child(icon("icons/globe.svg", t.size.icon_sm, t.colors.text_muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .font_family(t.font_mono.clone())
                                .text_size(t.text.caption)
                                .text_color(t.colors.text)
                                .child(url.unwrap_or_else(|| "No page yet".into())),
                        ),
                )
                .child(match v.shot {
                    Some(shot) => div()
                        .w(px(room))
                        .h(px(room * 0.62))
                        .rounded(t.radius.small)
                        .overflow_hidden()
                        .border_1()
                        .border_color(t.colors.border)
                        .bg(t.extra.inset)
                        .child(img(shot).size_full().object_fit(ObjectFit::Contain))
                        .into_any_element(),
                    None => div()
                        .px(t.space.md)
                        .py(t.space.lg)
                        .rounded(t.radius.small)
                        .bg(t.extra.inset)
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_muted)
                        .child("No screenshot yet: one shows here when Forge takes it.")
                        .into_any_element(),
                })
                .child(section_title(t, "Steps"))
                .child(div().flex().flex_col().gap(t.space.sm).children(
                    steps.iter().enumerate().rev().take(30).map(|(i, (action, what, status))| {
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .text_size(t.text.body_sm)
                            .child(super::tasks::status_icon(*status, t, format!("bstep-{i}").into()))
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(t.colors.text_strong)
                                    .first_letter_uppercase(action),
                            )
                            .children(what.clone().map(|w| {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_color(t.colors.text_muted)
                                    .child(w)
                            }))
                    }),
                ))
                .into_any_element()
        }
    };

    div()
        .flex_none()
        .when(!v.below, |d| d.w(px(v.width)).h_full().border_l_1())
        .when(v.below, |d| d.w_full().h(relative(0.45)).border_t_1())
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_color(t.colors.border)
        .child(header)
        .child(
            div()
                .id("panel-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
        .into_any_element()
}

trait FirstUpper {
    fn first_letter_uppercase(self, s: &str) -> Self;
}

impl FirstUpper for gpui::Div {
    fn first_letter_uppercase(self, s: &str) -> Self {
        let mut c = s.chars();
        let text = c
            .next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default();
        self.child(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reply(calls: &[(&str, serde_json::Value)]) -> Vec<Message> {
        vec![Message::Assistant {
            blocks: calls
                .iter()
                .map(|(n, i)| Block::Tool(crate::backend::tool_call(n, i)))
                .collect(),
        }]
    }

    #[test]
    fn the_plan_and_tasks_come_from_their_tool_calls() {
        let m = reply(&[
            (
                "ExitPlanMode",
                json!({"plan": "# Plan\n\n1. Read\n2. Write"}),
            ),
            (
                "TodoWrite",
                json!({"todos": [
                    {"content": "Read", "status": "completed"},
                    {"content": "Write", "status": "in_progress"},
                    {"content": "Test", "status": "pending"}
                ]}),
            ),
        ]);
        assert_eq!(plan(&m).unwrap(), "# Plan\n\n1. Read\n2. Write");
        let t = todos(&m);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0], (ToolStatus::Done, "Read".into()));
        assert_eq!(t[1].0, ToolStatus::Running);
        assert_eq!(t[2], (ToolStatus::Declined, "Test".into()));
    }

    #[test]
    fn the_browser_view_follows_its_tools() {
        let mut m = reply(&[
            (
                "mcp__browser-use__browser_navigate",
                json!({"url": "https://a.example"}),
            ),
            ("mcp__browser-use__browser_click", json!({"index": 4})),
        ]);
        let steps = browser_steps(&m);
        assert_eq!(steps[0].0, "navigate");
        assert_eq!(steps[1].1.as_deref(), Some("element 4"));
        assert_eq!(browser_url(&m).as_deref(), Some("https://a.example"));
        // A page's state names where it really is (after a redirect, a click).
        if let Message::Assistant { blocks } = &mut m[0] {
            let mut st =
                crate::backend::tool_call("mcp__browser-use__browser_get_state", &json!({}));
            st.output = r#"{"url": "https://a.example/next"}"#.into();
            blocks.push(Block::Tool(st));
        }
        assert_eq!(browser_url(&m).as_deref(), Some("https://a.example/next"));
    }

    #[test]
    fn files_are_listed_newest_first_once_each() {
        let f = |n: &str, c: &str| {
            Block::File(FileRef {
                name: n.to_string().into(),
                path: None,
                content: c.to_string().into(),
                bytes: None,
            })
        };
        let m = vec![
            Message::Assistant {
                blocks: vec![f("plan.md", "v1"), f("data.xlsx", "")],
            },
            Message::Assistant {
                blocks: vec![f("plan.md", "v2")],
            },
        ];
        let l = files(&m);
        assert_eq!(l.len(), 2);
        assert_eq!(
            (l[0].name.as_ref(), l[0].content.as_ref()),
            ("plan.md", "v2")
        );
    }
}
