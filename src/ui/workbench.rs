//! The side panel's own tabs, shown when no file is open in it: the chat's
//! **Plan** (the plan it proposed and its task list), its **Files** (every
//! Markdown file, workbook, PDF, Word file, deck and diagram it made, each
//! opening in the file view, `doc_pane.rs`) and the **Browser** (a live
//! browser, `browser_view.rs`).
//!
//! The plan and files are read from the chat's blocks, so a chat reloaded
//! from disk shows them again.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use super::markdown::{self, CopyContext, Look};
use crate::state::{AppState, Block, FileRef, Message, PanelTab, ToolStatus};
use crate::theme::Theme;

/// Every plan KodAct proposed in the chat, oldest first: each revision after
/// your feedback is a version of its own.
pub fn plans(messages: &[Message]) -> Vec<String> {
    tools(messages)
        .filter(|t| t.target == "ExitPlanMode")
        .map(|t| {
            t.diff
                .iter()
                .map(|l| l.text.as_ref())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

/// A file name for a plan: its title in lowercase words joined by dashes.
fn file_stem(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        "plan".into()
    } else {
        words.join("-")
    }
}

/// A plan's name: its first heading, else "Plan".
fn plan_title(md: &str) -> String {
    md.lines()
        .map(str::trim)
        .find(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "Plan".into())
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

/// KodAct is driving the browser: a browser tool runs in the chat now.
pub fn agent_browsing(messages: &[Message]) -> bool {
    tools(messages).any(|t| {
        t.status == ToolStatus::Running && t.target.starts_with(crate::browser::TOOL_PREFIX)
    })
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
    crate::theme::section_label(t, text)
}

pub struct View<'a> {
    pub tab: PanelTab,
    pub messages: &'a [Message],
    /// The Browser tab's live view (`browser_view.rs`).
    pub browser: gpui::AnyView,
    /// The chat's sub-agents, and the cards opened.
    pub agents: &'a [crate::state::AgentRun],
    pub agents_open: &'a std::collections::HashSet<String>,
    /// KodAct's mark, for the top of the agents' tree.
    pub logo: AnyElement,
    /// The chat's turn is still going.
    pub working: bool,
    pub copied: Option<SharedString>,
    pub width: f32,
    pub below: bool,
    pub scale: f32,
    /// The plan version shown (None: the latest), and Preview or Source.
    pub plan_version: Option<usize>,
    pub source: bool,
}

/// The bar over a plan, as over a document in Claude's panel: what it is,
/// its versions, Preview or Markdown, Copy and Download.
fn plan_bar(
    t: &Theme,
    state: &Entity<AppState>,
    md: &str,
    at: usize,
    total: usize,
    source: bool,
    copied: bool,
) -> AnyElement {
    let title = plan_title(md);
    let step = |id: &'static str, path: &'static str, tip: &'static str, to: Option<usize>| {
        let state = state.clone();
        let on = to.is_some();
        icon_button(t, id, path, tip)
            .when(!on, |d| d.opacity(0.35).cursor_default())
            .on_click(move |_, _, cx| {
                if let Some(to) = to {
                    state.update(cx, |s, cx| s.show_plan_version(to, total, cx));
                }
            })
    };
    let (to_preview, to_source, copier, saver) =
        (state.clone(), state.clone(), state.clone(), state.clone());
    let (text, file) = (
        md.to_string(),
        FileRef {
            name: format!("{}.md", file_stem(&title)).into(),
            path: None,
            content: md.to_string().into(),
            bytes: None,
        },
    );
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .px(t.space.lg)
        .py(t.space.sm)
        .border_b_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .child(
            div()
                .flex_none()
                .size(t.size.control)
                .rounded(t.radius.button)
                .bg(t.colors.primary_tint)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(
                    "icons/scroll-text.svg",
                    t.size.icon_sm,
                    t.colors.primary,
                )),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(if total > 1 {
                            format!("Plan · v{} of {total}", at + 1)
                        } else {
                            "Plan · Markdown".to_string()
                        }),
                ),
        )
        .when(total > 1, |d| {
            d.child(step(
                "plan-older",
                "icons/chevron-left.svg",
                "Earlier version",
                at.checked_sub(1),
            ))
            .child(step(
                "plan-newer",
                "icons/chevron-right.svg",
                "Later version",
                (at + 1 < total).then_some(at + 1),
            ))
        })
        .child(
            super::button::Segmented::new("plan-views", "View")
                .option("Preview", !source, move |_, _, cx| {
                    to_preview.update(cx, |s, cx| s.set_source_view(false, cx))
                })
                .option("Markdown", source, move |_, _, cx| {
                    to_source.update(cx, |s, cx| s.set_source_view(true, cx))
                }),
        )
        .child(
            icon_button(
                t,
                "plan-copy",
                if copied {
                    "icons/check.svg"
                } else {
                    "icons/copy.svg"
                },
                "Copy the plan",
            )
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
                copier.update(cx, |s, cx| s.mark_copied(PLAN_COPY.into(), cx));
            }),
        )
        .child(
            icon_button(t, "plan-download", "icons/download.svg", "Download")
                .on_click(move |_, _, cx| saver.update(cx, |s, cx| s.download(&file, cx))),
        )
        .into_any_element()
}

const PLAN_COPY: &str = "panel-plan-copy";

pub fn render(v: View, state: Entity<AppState>, t: &Theme) -> AnyElement {
    let files = files(v.messages);
    let all_plans = plans(v.messages);
    let plan_at = v
        .plan_version
        .filter(|i| *i < all_plans.len())
        .unwrap_or(all_plans.len().saturating_sub(1));
    let plan = all_plans.get(plan_at).cloned();
    let todos = todos(v.messages);
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
        .child({
            // Each view's one figure: tasks done, agents finished, files made.
            let done = todos.iter().filter(|(s, _)| *s == ToolStatus::Done).count();
            let running = v.agents.iter().filter(|r| r.ended.is_none()).count();
            let of = |a: usize, b: usize| (b > 0).then(|| format!("{a}/{b}"));
            let item = |which, label, icon, readout, live, key| super::lens::Item {
                which,
                label,
                icon,
                readout,
                live,
                key,
            };
            super::lens::Lens::new(
                vec![
                    item(
                        PanelTab::Plan,
                        "Plan",
                        "icons/route.svg",
                        of(done, todos.len()),
                        false,
                        "1",
                    ),
                    item(
                        PanelTab::Agents,
                        "Agents",
                        "icons/bot.svg",
                        of(v.agents.len() - running, v.agents.len()),
                        running > 0,
                        "2",
                    ),
                    item(
                        PanelTab::Files,
                        "Files",
                        "icons/folder.svg",
                        (!files.is_empty()).then(|| files.len().to_string()),
                        false,
                        "3",
                    ),
                    item(
                        PanelTab::Browser,
                        "Browser",
                        "icons/globe.svg",
                        None,
                        false,
                        "4",
                    ),
                ],
                v.tab,
                state.clone(),
            )
        })
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
    // The Agents overview sits on the sidebar's deeper ground, so its
    // cards stand off it; the other tabs are pages on the surface.
    let ground = if v.tab == PanelTab::Agents {
        t.extra.pane
    } else {
        t.colors.surface
    };
    let body: AnyElement = match v.tab {
        PanelTab::Plan if plan.is_none() && todos.is_empty() => empty(
            t,
            "icons/route.svg",
            "No plan yet",
            "Switch the mode to Plan (Shift+Tab in the composer) and ask for something: the plan KodAct proposes, and its task list as it works, show here.",
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
            .children(plan.as_ref().filter(|_| v.source).map(|md| {
                super::source_view::render("plan-source", md, "markdown", t)
            }))
            .children(plan.as_ref().filter(|_| !v.source).map(|md| {
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.sm)
                    .when(plan_at + 1 < all_plans.len(), |d| {
                        d.child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.extra.warning)
                                .child(format!(
                                    "An earlier version. KodAct's latest plan is version {}.",
                                    all_plans.len()
                                )),
                        )
                    })
                    .child(markdown::render_as(
                        md,
                        t,
                        &CopyContext {
                            streaming: false,
                            state: state.clone(),
                            key: "panel-plan".into(),
                            copied: v.copied.clone(),
                            scale: v.scale,
                            sources: Default::default(),
                            zooms: Default::default(),
                        },
                        Look::Document,
                    ))
            }))
            .into_any_element(),
        PanelTab::Agents => super::agents_view::render(
            super::agents_view::View {
                runs: v.agents,
                request: v.messages.iter().rev().find_map(|m| match m {
                    Message::User { text, .. } => Some(text.clone()),
                    _ => None,
                }),
                logo: v.logo,
                working: v.working,
            },
            state.clone(),
            v.agents_open,
            t,
        ),
        PanelTab::Files if files.is_empty() => empty(
            t,
            "icons/folder-open.svg",
            "No files yet",
            "Documents, spreadsheets, PDFs, decks and diagrams KodAct makes in this chat are listed here. Click one to read it in this panel.",
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
        PanelTab::Browser => div().size_full().child(v.browser.clone()).into_any_element(),
    };

    let fills = v.tab == PanelTab::Browser;
    div()
        .flex_none()
        .when(!v.below, |d| d.w(px(v.width)).h_full().border_l_1())
        .when(v.below, |d| d.w_full().h(relative(0.45)).border_t_1())
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_color(t.colors.border)
        .child(header)
        .children(plan.as_ref().filter(|_| v.tab == PanelTab::Plan).map(|md| {
            plan_bar(
                t,
                &state,
                md,
                plan_at,
                all_plans.len(),
                v.source,
                v.copied.as_deref() == Some(PLAN_COPY),
            )
        }))
        .child(
            div()
                .id("panel-body")
                .flex_1()
                .min_h_0()
                .bg(ground)
                // The browser fills the panel and scrolls its own page;
                // the other views scroll here.
                .map(|d| if fills { d.flex().flex_col() } else { d.overflow_y_scroll() })
                // Each view arrives as the capsule lands on it: it fades in
                // and rises a few pixels into place.
                .child(super::motion::rise_in(
                    div().when(fills, |d| d.flex_1().min_h_0()).child(body),
                    SharedString::from(format!("panel-view-{:?}", v.tab)),
                    6.,
                    t,
                )),
        )
        .into_any_element()
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
        assert_eq!(plans(&m).pop().unwrap(), "# Plan\n\n1. Read\n2. Write");
        let t = todos(&m);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0], (ToolStatus::Done, "Read".into()));
        assert_eq!(t[1].0, ToolStatus::Running);
        assert_eq!(t[2], (ToolStatus::Declined, "Test".into()));
    }

    #[test]
    fn forge_is_browsing_while_a_browser_tool_runs() {
        let mut m = reply(&[(
            "mcp__kodact-browser__navigate",
            json!({"url": "https://a.example"}),
        )]);
        assert!(agent_browsing(&m));
        if let Message::Assistant { blocks } = &mut m[0] {
            if let Block::Tool(t) = &mut blocks[0] {
                t.status = ToolStatus::Done;
            }
        }
        assert!(!agent_browsing(&m));
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
