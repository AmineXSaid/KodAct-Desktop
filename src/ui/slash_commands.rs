//! Settings → Slash Commands: the prompts `/name` expands to, as KodAct CLI
//! reads them from `commands/` (yours, in KodAct CLI's config folder, and the
//! project's `.kodact/commands`). List, create, edit and delete; the next
//! chat's KodAct CLI picks a change up, and `/` in the composer lists it.

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

pub struct SlashCommands {
    state: Entity<AppState>,
    /// The form is open: editing this file, or (None) a new command.
    form: Option<Option<PathBuf>>,
    scope: Scope,
    name: Entity<InputState>,
    description: Entity<InputState>,
    hint: Entity<InputState>,
    body: Entity<TextareaState>,
    error: Option<SharedString>,
    saving: bool,
    /// The command waiting on "Delete?".
    confirm: Option<PathBuf>,
    _sub: Subscription,
}

impl SlashCommands {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _sub = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            _sub,
            name: cx.new(|cx| {
                InputState::new(window, cx).placeholder("e.g. review  (or git:review for a subfolder)")
            }),
            description: cx
                .new(|cx| InputState::new(window, cx).placeholder("Shown beside it in the / list")),
            hint: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. [file]")),
            body: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("e.g. Review $ARGUMENTS for bugs and missing tests.")
            }),
            state,
            form: None,
            scope: Scope::Project,
            error: None,
            saving: false,
            confirm: None,
        }
    }

    /// Esc: close the form first. Whether there was one to close.
    pub fn back(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.form.is_some() || self.confirm.is_some();
        self.form = None;
        self.confirm = None;
        cx.notify();
        open
    }

    fn open(&mut self, cmd: Option<ff::Command>, window: &mut Window, cx: &mut Context<Self>) {
        let c = cmd.clone();
        let set = |i: &Entity<InputState>, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            i.update(cx, |i, cx| i.set_value(v.to_string(), window, cx))
        };
        set(&self.name, c.as_ref().map_or("", |c| &c.name), window, cx);
        set(
            &self.description,
            c.as_ref().map_or("", |c| &c.own_description),
            window,
            cx,
        );
        set(&self.hint, c.as_ref().map_or("", |c| &c.hint), window, cx);
        let body = c
            .as_ref()
            .map_or(String::new(), |c| c.body.trim().to_string());
        self.body.update(cx, |i, cx| i.set_value(body, window, cx));
        self.scope = c.as_ref().map_or(Scope::Project, |c| c.scope);
        self.form = Some(c.map(|c| c.path));
        self.error = None;
        self.saving = false;
        self.name.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(old) = self.form.clone() else { return };
        let val =
            |i: &Entity<InputState>, cx: &Context<Self>| i.read(cx).value().trim().to_string();
        let (name, description, hint) = (
            val(&self.name, cx).trim_start_matches('/').to_string(),
            val(&self.description, cx),
            val(&self.hint, cx),
        );
        let body = self.body.read(cx).value().to_string();
        // Checked here first, so a typo answers at once.
        if let Err(e) = ff::check_name(&name, true) {
            self.error = Some(e.into());
            return cx.notify();
        }
        if body.trim().is_empty() {
            self.error = Some("Write the prompt the command sends.".into());
            return cx.notify();
        }
        let scope = self.scope;
        self.saving = true;
        self.error = None;
        let done = format!("Saved /{name}. New chats can use it.");
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                done,
                move |project| {
                    ff::save_command(
                        scope,
                        project,
                        old.as_deref(),
                        &name,
                        &description,
                        &hint,
                        &body,
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

    fn delete(&mut self, path: PathBuf, name: String, cx: &mut Context<Self>) {
        self.confirm = None;
        self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Deleted /{name}."),
                move |_| std::fs::remove_file(&path).map_err(|e| e.to_string()),
                cx,
            )
            .detach()
        });
        cx.notify();
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let s = self.state.read(cx);
        let project = s.project_dir.clone();
        let catalog = s.engine.catalog.clone();
        let add = theme::primary_button(t, "New command")
            .id("new-command")
            .on_click(cx.listener(|this, _, window, cx| this.open(None, window, cx)))
            .into_any_element();
        let mut out = div().flex().flex_col().gap(px(10.)).child(kit::header(
            t,
            "Slash commands",
            "A command is a saved prompt: type /name in a chat and KodAct sends it, with what you type after it as $ARGUMENTS.",
            Some(add),
        ));
        let Some(catalog) = catalog else {
            return out.child(kit::loading(t)).into_any_element();
        };
        out = out.children(kit::problems(t, &catalog.problems));
        if catalog.commands.is_empty() {
            out = out.child(kit::empty(
                t,
                "No commands yet. Make one for a prompt you send often, such as a review checklist or a release note.",
            ));
        }
        for scope in [Scope::Project, Scope::User] {
            let rows: Vec<&ff::Command> = catalog
                .commands
                .iter()
                .filter(|c| c.scope == scope)
                .collect();
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
            for c in rows {
                out = out.child(self.row(t, c, cx));
            }
        }
        let user_dir = ff::kind_dir("commands", Scope::User, &project);
        let project_dir = ff::kind_dir("commands", Scope::Project, &project);
        out.child(
            div()
                .flex()
                .gap(px(14.))
                .pt(px(4.))
                .children(project_dir.map(|d| {
                    kit::link(
                        t,
                        "open-project-commands",
                        "icons/folder-open.svg",
                        "Open the project's folder",
                    )
                    .on_click(move |_, _, cx| kit::reveal(&d, cx))
                }))
                .children(user_dir.map(|d| {
                    kit::link(
                        t,
                        "open-user-commands",
                        "icons/folder-open.svg",
                        "Open your folder",
                    )
                    .on_click(move |_, _, cx| kit::reveal(&d, cx))
                })),
        )
        .into_any_element()
    }

    fn row(&self, t: &Theme, c: &ff::Command, cx: &mut Context<Self>) -> gpui::AnyElement {
        let key = c.path.display().to_string();
        if self.confirm.as_ref() == Some(&c.path) {
            let (path, name) = (c.path.clone(), c.name.clone());
            return kit::confirm_row(
                t,
                format!("Delete /{}?", c.name),
                format!(
                    "This deletes {}.",
                    kit::short(&c.path, &self.state.read(cx).project_dir)
                ),
                kit::cancel_button(t, "cancel-delete").on_click(cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                })),
                theme::danger_button(t, "Delete")
                    .id("confirm-delete")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete(path.clone(), name.clone(), cx)
                    })),
            )
            .into_any_element();
        }
        let detail = if c.hint.is_empty() {
            c.description.clone()
        } else {
            format!("{} · {}", c.hint, c.description)
        };
        let note = c.shadowed.then(|| {
            (
                t.extra.warning,
                "Another /".to_string() + &c.name + " later in KodAct CLI's order replaces this one.",
            )
        });
        let (edit, del) = (c.clone(), c.path.clone());
        kit::card(t)
            .child(icon(
                "icons/square-slash.svg",
                t.size.icon,
                t.colors.text_muted,
            ))
            .child(kit::card_text(
                t,
                format!("/{}", c.name),
                true,
                Some(detail),
                note,
            ))
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("edit-cmd-{key}")),
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
                    SharedString::from(format!("rm-cmd-{key}")),
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
            (Scope::Project, "This project (.kodact/commands)"),
            (Scope::User, "Yours, every project"),
        ] {
            scopes = scopes.child(
                kit::chip(
                    t,
                    SharedString::from(format!("cmd-scope-{}", scope.label())),
                    label,
                    self.scope == scope,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.scope = scope;
                    cx.notify();
                })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(kit::back(t, "cmd-back", "Slash commands").on_click(cx.listener(
                |this, _, _, cx| {
                    this.back(cx);
                },
            )))
            .child(kit::header(
                t,
                if editing { "Edit command" } else { "New command" },
                "$ARGUMENTS is what follows the command; $1, $2… its words. Without them, what you type is added at the end.",
                None,
            ))
            .child(kit::field(t, "Saved in", None, scopes.into_any_element()))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().w(px(220.)).child(kit::field(
                        t,
                        "Name",
                        None,
                        kit::line(t, &self.name, true),
                    )))
                    .child(div().flex_1().min_w_0().child(kit::field(
                        t,
                        "Arguments hint",
                        None,
                        kit::line(t, &self.hint, true),
                    ))),
            )
            .child(kit::field(
                t,
                "Description",
                Some("Optional. Without it, the prompt's first line is shown."),
                kit::line(t, &self.description, false),
            ))
            .child(kit::field(t, "Prompt", None, kit::area(t, &self.body, false)))
            .children(kit::status(t, &self.error))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(kit::cancel_button(t, "cmd-cancel").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.back(cx);
                        },
                    )))
                    .child(
                        kit::save_button(t, "cmd-save", self.saving).when(!self.saving, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                        }),
                    ),
            )
            .into_any_element()
    }
}

impl Render for SlashCommands {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.form.is_some() {
            self.form(&t, cx)
        } else {
            self.list(&t, cx)
        }
    }
}
