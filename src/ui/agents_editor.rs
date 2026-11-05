//! Settings → Agents: sub-agents KodAct CLI can hand a task to (its Task
//! tool), each a Markdown file in `agents/`: a name and a description (when
//! to use it: the model reads it), the tools it may use, an optional model,
//! and its instructions. Create, edit and delete; the next chat's KodAct CLI
//! lists it with its built-in agents (general-purpose, Explore, Plan).

use std::path::PathBuf;

use gpui::component::input::{InputState, TextareaState};
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::*;
use crate::forge_files::{self as ff, Scope};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

pub struct AgentsEditor {
    state: Entity<AppState>,
    /// The form is open: editing this file, or (None) a new agent.
    form: Option<Option<PathBuf>>,
    scope: Scope,
    name: Entity<InputState>,
    description: Entity<InputState>,
    model: Entity<InputState>,
    prompt: Entity<TextareaState>,
    /// None: every tool.
    tools: Option<Vec<String>>,
    error: Option<SharedString>,
    saving: bool,
    confirm: Option<PathBuf>,
    _sub: Subscription,
}

impl AgentsEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _sub: cx.observe(&state, |_, _, cx| cx.notify()),
            name: cx.new(|cx| InputState::new(window, cx).placeholder("reviewer")),
            description: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("e.g. Reviews a diff for bugs. Use it after a change is made.")
            }),
            model: cx.new(|cx| InputState::new(window, cx).placeholder("Empty: the chat's model")),
            prompt: cx.new(|cx| {
                TextareaState::new(window, cx).placeholder(
                    "You are a careful reviewer. Read the diff, then report bugs with file:line…",
                )
            }),
            state,
            form: None,
            scope: Scope::Project,
            tools: None,
            error: None,
            saving: false,
            confirm: None,
        }
    }

    pub fn back(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.form.is_some() || self.confirm.is_some();
        self.form = None;
        self.confirm = None;
        cx.notify();
        open
    }

    fn open(&mut self, a: Option<ff::Agent>, window: &mut Window, cx: &mut Context<Self>) {
        let set = |i: &Entity<InputState>, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            i.update(cx, |i, cx| i.set_value(v.to_string(), window, cx))
        };
        set(&self.name, a.as_ref().map_or("", |a| &a.name), window, cx);
        set(
            &self.description,
            a.as_ref().map_or("", |a| &a.description),
            window,
            cx,
        );
        set(&self.model, a.as_ref().map_or("", |a| &a.model), window, cx);
        let prompt = a
            .as_ref()
            .map_or(String::new(), |a| a.prompt.trim().to_string());
        self.prompt
            .update(cx, |i, cx| i.set_value(prompt, window, cx));
        self.tools = a.as_ref().and_then(|a| a.tools.clone());
        self.scope = a.as_ref().map_or(Scope::Project, |a| a.scope);
        self.form = Some(a.map(|a| a.path));
        self.error = None;
        self.saving = false;
        self.name.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    fn toggle_tool(&mut self, tool: &str, cx: &mut Context<Self>) {
        let list = self.tools.get_or_insert_with(Vec::new);
        if let Some(i) = list.iter().position(|t| t == tool) {
            list.remove(i);
        } else {
            list.push(tool.to_string());
        }
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(old) = self.form.clone() else { return };
        let val =
            |i: &Entity<InputState>, cx: &Context<Self>| i.read(cx).value().trim().to_string();
        let (name, description, model) = (
            val(&self.name, cx),
            val(&self.description, cx),
            val(&self.model, cx),
        );
        let prompt = self.prompt.read(cx).value().to_string();
        let tools = self.tools.clone();
        let quick = ff::check_name(&name, false).err().or_else(|| {
            if description.is_empty() {
                Some("Say when to use it: the model reads the description to decide.".into())
            } else if prompt.trim().is_empty() {
                Some("Write the agent's instructions.".into())
            } else if tools.as_ref().is_some_and(|t| t.is_empty()) {
                Some("Pick at least one tool, or allow every tool.".into())
            } else {
                None
            }
        });
        if let Some(e) = quick {
            self.error = Some(e.into());
            return cx.notify();
        }
        let scope = self.scope;
        self.saving = true;
        self.error = None;
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Saved the {name} agent. New chats can hand it tasks."),
                move |project| {
                    ff::save_agent(
                        scope,
                        project,
                        old.as_deref(),
                        &name,
                        &description,
                        tools.as_deref(),
                        &model,
                        &prompt,
                    )
                    .map(|_| ())
                },
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match r {
                    Ok(()) => this.form = None,
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let s = self.state.read(cx);
        let project = s.project_dir.clone();
        let catalog = s.engine.catalog.clone();
        let add = theme::primary_button(t, "New agent")
            .id("new-agent")
            .on_click(cx.listener(|this, _, window, cx| this.open(None, window, cx)))
            .into_any_element();
        let mut out = div().flex().flex_col().gap(px(10.)).child(kit::header(
            t,
            "Agents",
            "KodAct can hand a task to a sub-agent, which works in its own context and reports back. KodAct CLI has three built in: general-purpose, Explore and Plan.",
            Some(add),
        ));
        let Some(catalog) = catalog else {
            return out.child(kit::loading(t)).into_any_element();
        };
        out = out.children(kit::problems(t, &catalog.problems));
        if catalog.agents.is_empty() {
            out = out.child(kit::empty(
                t,
                "No agents of your own yet. Make one for a job you hand off often, with only the tools it needs: a read-only reviewer, a test writer.",
            ));
        }
        for scope in [Scope::Project, Scope::User] {
            let rows: Vec<&ff::Agent> =
                catalog.agents.iter().filter(|a| a.scope == scope).collect();
            if rows.is_empty() {
                continue;
            }
            out = out.child(kit::group(
                t,
                match scope {
                    Scope::Project => "This project",
                    Scope::User => "Yours, in every project",
                },
            ));
            for a in rows {
                out = out.child(self.row(t, a, cx));
            }
        }
        let dirs = [
            (
                Scope::Project,
                "open-project-agents",
                "Open the project's folder",
            ),
            (Scope::User, "open-user-agents", "Open your folder"),
        ];
        let mut links = div().flex().gap(px(14.)).pt(px(4.));
        for (scope, id, label) in dirs {
            if let Some(d) = ff::kind_dir("agents", scope, &project) {
                links = links.child(
                    kit::link(t, id, "icons/folder-open.svg", label)
                        .on_click(move |_, _, cx| kit::reveal(&d, cx)),
                );
            }
        }
        out.child(links).into_any_element()
    }

    fn row(&self, t: &Theme, a: &ff::Agent, cx: &mut Context<Self>) -> gpui::AnyElement {
        let key = a.path.display().to_string();
        if self.confirm.as_ref() == Some(&a.path) {
            let (path, name) = (a.path.clone(), a.name.clone());
            return kit::confirm_row(
                t,
                format!("Delete the {} agent?", a.name),
                format!(
                    "This deletes {}.",
                    kit::short(&a.path, &self.state.read(cx).project_dir)
                ),
                kit::cancel_button(t, "cancel-delete").on_click(cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                })),
                theme::danger_button(t, "Delete")
                    .id("confirm-delete")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = None;
                        let (path, name) = (path.clone(), name.clone());
                        this.state.update(cx, |s, cx| {
                            s.engine_write(
                                format!("Deleted the {name} agent."),
                                move |_| std::fs::remove_file(&path).map_err(|e| e.to_string()),
                                cx,
                            )
                            .detach()
                        });
                    })),
            )
            .into_any_element();
        }
        let tools = match &a.tools {
            None => "Every tool".to_string(),
            Some(t) => t.join(", "),
        };
        let model = if a.model.is_empty() {
            String::new()
        } else {
            format!(" · {}", a.model)
        };
        let note = a.problem.clone().map(|p| {
            (
                t.extra.danger,
                format!(
                    "{}: {p}.",
                    kit::short(&a.path, &self.state.read(cx).project_dir)
                ),
            )
        });
        let (edit, del) = (a.clone(), a.path.clone());
        kit::card(t)
            .child(icon("icons/boxes.svg", t.size.icon, t.colors.text_muted))
            .child(kit::card_text(
                t,
                a.name.clone(),
                false,
                Some(format!("{}{model} · {}", tools, a.description)),
                note,
            ))
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("edit-agent-{key}")),
                    "icons/pencil.svg",
                    "Edit",
                    false,
                )
                .on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.open(Some(edit.clone()), window, cx)
                    }),
                ),
            )
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("rm-agent-{key}")),
                    "icons/trash.svg",
                    "Delete",
                    true,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm = Some(del.clone());
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn form(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let editing = matches!(self.form, Some(Some(_)));
        let mut scopes = div().flex().gap(px(6.));
        for (scope, label) in [
            (Scope::Project, "This project (.kodact/agents)"),
            (Scope::User, "Yours, every project"),
        ] {
            scopes = scopes.child(
                kit::chip(
                    t,
                    SharedString::from(format!("agent-scope-{}", scope.label())),
                    label,
                    self.scope == scope,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.scope = scope;
                    cx.notify();
                })),
            );
        }
        let all = self.tools.is_none();
        let mut tools = div().flex().flex_wrap().gap(px(6.)).child(
            kit::chip(t, "tools-all", "Every tool", all).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.tools = if all {
                        Some(vec!["Read".into(), "Glob".into(), "Grep".into()])
                    } else {
                        None
                    };
                    cx.notify();
                },
            )),
        );
        for tool in ff::TOOLS {
            let on = self
                .tools
                .as_ref()
                .is_some_and(|l| l.iter().any(|x| x == tool));
            tools = tools.child(
                kit::chip(t, SharedString::from(format!("tool-{tool}")), *tool, on)
                    .when(all, |d| d.opacity(0.5))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_tool(tool, cx))),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(kit::back(t, "agent-back", "Agents").on_click(cx.listener(|this, _, _, cx| {
                this.back(cx);
            })))
            .child(kit::header(
                t,
                if editing { "Edit agent" } else { "New agent" },
                "The description tells KodAct when to hand this agent a task; the instructions are its system prompt.",
                None,
            ))
            .child(kit::field(t, "Saved in", None, scopes.into_any_element()))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().w(px(220.)).child(kit::field(t, "Name", None, kit::line(t, &self.name, true))))
                    .child(div().flex_1().min_w_0().child(kit::field(t, "Model", None, kit::line(t, &self.model, true)))),
            )
            .child(kit::field(t, "When to use it", None, kit::line(t, &self.description, false)))
            .child(kit::field(
                t,
                "Tools",
                Some("Every tool means all but handing off to another agent. Pick tools to keep it to those."),
                tools.into_any_element(),
            ))
            .child(kit::field(t, "Instructions", None, kit::area(t, &self.prompt, false)))
            .children(kit::status(t, &self.error))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(kit::cancel_button(t, "agent-cancel").on_click(cx.listener(|this, _, _, cx| {
                        this.back(cx);
                    })))
                    .child(kit::save_button(t, "agent-save", self.saving).when(!self.saving, |d| {
                        d.on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                    })),
            )
            .into_any_element()
    }
}

impl Render for AgentsEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.form.is_some() {
            self.form(&t, cx)
        } else {
            self.list(&t, cx)
        }
    }
}
