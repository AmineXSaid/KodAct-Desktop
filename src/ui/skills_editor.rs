//! Settings → Skills: folders of instructions KodAct CLI loads when they fit
//! the task (`skills/<name>/SKILL.md`). Only a skill's name and description
//! sit in the context until the model picks it. List them (yours, the
//! project's, and the ones built into KodAct), turn one on or off, and
//! create, edit or delete. Off moves the folder to `skills/.disabled/`,
//! where KodAct CLI never looks, since it has no setting for this.

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

pub struct SkillsEditor {
    state: Entity<AppState>,
    /// The form is open: editing this folder, or (None) a new skill.
    form: Option<Option<PathBuf>>,
    scope: Scope,
    name: Entity<InputState>,
    description: Entity<InputState>,
    body: Entity<TextareaState>,
    error: Option<SharedString>,
    saving: bool,
    confirm: Option<PathBuf>,
    /// The Install panel (`skill_install.rs`), shown while `AppState::skills_install`.
    install: Entity<super::skill_install::SkillInstall>,
    _sub: Subscription,
}

impl SkillsEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _sub: cx.observe(&state, |_, _, cx| cx.notify()),
            name: cx.new(|cx| InputState::new(window, cx).placeholder("pdf")),
            description: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Reads PDF files. Use it when the user shares or mentions a PDF.")
            }),
            body: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Run pdftotext -layout on the file, then…")
            }),
            install: cx
                .new(|cx| super::skill_install::SkillInstall::new(state.clone(), window, cx)),
            state,
            form: None,
            scope: Scope::Project,
            error: None,
            saving: false,
            confirm: None,
        }
    }

    pub fn back(&mut self, cx: &mut Context<Self>) -> bool {
        if self.state.read(cx).skills_install {
            self.install.update(cx, |i, cx| i.close(cx));
            return true;
        }
        let open = self.form.is_some() || self.confirm.is_some();
        self.form = None;
        self.confirm = None;
        cx.notify();
        open
    }

    fn open(&mut self, sk: Option<ff::Skill>, window: &mut Window, cx: &mut Context<Self>) {
        let set = |i: &Entity<InputState>, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            i.update(cx, |i, cx| i.set_value(v.to_string(), window, cx))
        };
        set(&self.name, sk.as_ref().map_or("", |s| &s.name), window, cx);
        set(
            &self.description,
            sk.as_ref().map_or("", |s| &s.description),
            window,
            cx,
        );
        let body = sk
            .as_ref()
            .map_or(String::new(), |s| s.body.trim().to_string());
        self.body.update(cx, |i, cx| i.set_value(body, window, cx));
        self.scope = sk.as_ref().map_or(Scope::Project, |s| s.scope);
        self.form = Some(sk.map(|s| s.dir));
        self.error = None;
        self.saving = false;
        self.name.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(old) = self.form.clone() else { return };
        let name = self.name.read(cx).value().trim().to_string();
        let description = self.description.read(cx).value().trim().to_string();
        let body = self.body.read(cx).value().to_string();
        let quick = ff::check_name(&name, false).err().or_else(|| {
            if description.is_empty() {
                Some(
                    "Say what it does and when to use it: the model reads only this to pick it."
                        .into(),
                )
            } else if body.trim().is_empty() {
                Some("Write the skill's instructions.".into())
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
                format!("Saved the {name} skill. New chats can use it."),
                move |project| {
                    ff::save_skill(scope, project, old.as_deref(), &name, &description, &body)
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
        let add = div()
            .flex()
            .gap(px(8.))
            .child(
                theme::ghost_button(t, "Install skill")
                    .id("install-skill")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |s, cx| s.install_skill(cx));
                    })),
            )
            .child(
                theme::primary_button(t, "New skill")
                    .id("new-skill")
                    .on_click(cx.listener(|this, _, window, cx| this.open(None, window, cx))),
            )
            .into_any_element();
        let mut out = div().flex().flex_col().gap(px(10.)).child(kit::header(
            t,
            "Skills",
            "A skill is a folder of instructions (and any files they use). Only its name and description sit in the context; KodAct reads the rest when the task fits. Skills installed for Claude Code work here too.",
            Some(add),
        ));
        let Some(catalog) = catalog else {
            return out.child(kit::loading(t)).into_any_element();
        };
        if catalog.skills.is_empty() {
            out = out.child(kit::empty(
                t,
                "No skills of your own yet. Make one for know-how KodAct should pull in only when it needs it: a file format, a deploy procedure, a house style.",
            ));
        }
        for scope in [Scope::Project, Scope::User] {
            let rows: Vec<&ff::Skill> =
                catalog.skills.iter().filter(|s| s.scope == scope).collect();
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
            for sk in rows {
                out = out.child(self.row(t, sk, cx));
            }
        }
        // Built in: KodAct's plugin (and KodAct CLI's own, named in the hint).
        out = out.child(kit::group(t, "Built into KodAct"));
        for (name, description) in crate::plugin::skills() {
            out = out.child(
                kit::card(t)
                    .child(icon(
                        "icons/wand-sparkles.svg",
                        t.size.icon,
                        t.colors.text_muted,
                    ))
                    .child(kit::card_text(t, name, true, Some(description), None))
                    .child(kit::scope_pill(t, "Built in")),
            );
        }
        out = out.child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child("KodAct CLI also brings its own: /code-review, /security-review, /simplify, /verify, /init and more. A skill of yours with the same name replaces one."),
        );
        let mut links = div().flex().gap(px(14.)).pt(px(4.));
        for (scope, id, label) in [
            (
                Scope::Project,
                "open-project-skills",
                "Open the project's folder",
            ),
            (Scope::User, "open-user-skills", "Open your folder"),
        ] {
            if let Some(d) = ff::kind_dir("skills", scope, &project) {
                links = links.child(
                    kit::link(t, id, "icons/folder-open.svg", label)
                        .on_click(move |_, _, cx| kit::reveal(&d, cx)),
                );
            }
        }
        out.child(links).into_any_element()
    }

    fn row(&self, t: &Theme, sk: &ff::Skill, cx: &mut Context<Self>) -> gpui::AnyElement {
        let key = sk.dir.display().to_string();
        if self.confirm.as_ref() == Some(&sk.dir) {
            let (dir, name) = (sk.dir.clone(), sk.name.clone());
            let files = if sk.extra_files == 0 {
                "its SKILL.md".to_string()
            } else {
                format!(
                    "SKILL.md and {} other file{}",
                    sk.extra_files,
                    if sk.extra_files == 1 { "" } else { "s" }
                )
            };
            return kit::confirm_row(
                t,
                format!("Delete the {} skill?", sk.name),
                format!(
                    "This deletes {} ({files}).",
                    kit::short(&sk.dir, &self.state.read(cx).project_dir)
                ),
                kit::cancel_button(t, "cancel-delete").on_click(cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                })),
                theme::danger_button(t, "Delete")
                    .id("confirm-delete")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = None;
                        let (dir, name) = (dir.clone(), name.clone());
                        this.state.update(cx, |s, cx| {
                            s.engine_write(
                                format!("Deleted the {name} skill."),
                                move |_| std::fs::remove_dir_all(&dir).map_err(|e| e.to_string()),
                                cx,
                            )
                            .detach()
                        });
                    })),
            )
            .into_any_element();
        }
        let enabled = sk.enabled;
        let detail = if sk.extra_files > 0 {
            format!(
                "{} · {} more file{}",
                sk.description,
                sk.extra_files,
                if sk.extra_files == 1 { "" } else { "s" }
            )
        } else {
            sk.description.clone()
        };
        let note = sk
            .problem
            .clone()
            .map(|p| (t.extra.danger, format!("SKILL.md has {p}.")));
        let (edit, del, toggle_dir, name) =
            (sk.clone(), sk.dir.clone(), sk.dir.clone(), sk.name.clone());
        let claude = ff::is_claude_code(&sk.dir);
        let installed = crate::skill_install::origin(&sk.dir);
        let project = self.state.read(cx).project_dir.clone();
        let note = note.or_else(|| {
            installed.as_ref().map(|o| {
                // `github.com/owner/repo`, or a folder as people read it.
                let from = match o.source.split_once("://") {
                    Some((_, rest)) => rest.to_string(),
                    None => kit::short(std::path::Path::new(&o.source), &project),
                };
                // One line: a long path keeps its end, where the name is.
                let n = from.chars().count();
                let from = if n > 64 {
                    format!("…{}", from.chars().skip(n - 63).collect::<String>())
                } else {
                    from
                };
                (t.colors.text_muted, format!("From {from}"))
            })
        });
        let remote = self
            .state
            .read(cx)
            .current_remote()
            .filter(|_| sk.scope == Scope::User);
        let (update_dir, update_name) = (sk.dir.clone(), sk.name.clone());
        let copy = sk.clone();
        kit::card(t)
            .when(!enabled, |d| d.opacity(0.65))
            .child(icon(
                "icons/wand-sparkles.svg",
                t.size.icon,
                t.colors.text_muted,
            ))
            .child(kit::card_text(t, sk.name.clone(), true, Some(detail), note))
            .when(claude, |d| d.child(kit::scope_pill(t, "Claude Code")))
            .when(remote.is_some(), |d| {
                d.child(
                    kit::icon_button(
                        t,
                        SharedString::from(format!("copy-skill-{key}")),
                        "icons/share.svg",
                        "Copy to this project's machine (WSL, SSH)",
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.copy_to_remote(&copy, cx))),
                )
            })
            .when(installed.is_some(), |d| {
                d.child(
                    kit::icon_button(
                        t,
                        SharedString::from(format!("update-skill-{key}")),
                        "icons/refresh-cw.svg",
                        "Update",
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let (dir, name) = (update_dir.clone(), update_name.clone());
                        this.state.update(cx, |s, cx| {
                            s.engine_write(
                                format!("Updated the {name} skill."),
                                move |_| crate::skill_install::update_skill(&dir),
                                cx,
                            )
                            .detach()
                        });
                    })),
                )
            })
            .child(
                kit::chip(
                    t,
                    SharedString::from(format!("toggle-skill-{key}")),
                    if enabled { "On" } else { "Off" },
                    enabled,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let (dir, name) = (toggle_dir.clone(), name.clone());
                    this.state.update(cx, |s, cx| {
                        s.engine_write(
                            if enabled {
                                format!("Turned {name} off. New chats won't load it.")
                            } else {
                                format!("Turned {name} on.")
                            },
                            move |_| ff::set_skill_enabled(&dir, !enabled).map(|_| ()),
                            cx,
                        )
                        .detach()
                    });
                })),
            )
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("edit-skill-{key}")),
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
                    SharedString::from(format!("rm-skill-{key}")),
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

    /// Copy one of your skills to the machine the current project is on (WSL, SSH),
    /// into its `~/.claude/skills`, where KodAct CLI there finds it.
    fn copy_to_remote(&mut self, sk: &ff::Skill, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        let Some(remote) = s.current_remote() else {
            return;
        };
        let extra = s.remote_extra(&remote.target);
        let (dir, name) = (sk.dir.clone(), sk.name.clone());
        let label = remote.target.label();
        self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Copied the {name} skill to {label}."),
                move |_| {
                    let found = crate::skill_install::read_skill(&dir)
                        .ok_or("its SKILL.md can't be read")?;
                    let source = crate::skill_install::Source::Folder(dir.clone());
                    let root = dir.parent().unwrap_or(&dir).to_path_buf();
                    crate::skill_install::install_skill_remote(
                        &found, &source, &root, &remote, true, true, &extra,
                    )
                    .map(|_| ())
                },
                cx,
            )
            .detach()
        });
    }

    fn form(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let editing = matches!(self.form, Some(Some(_)));
        let mut scopes = div().flex().gap(px(6.));
        for (scope, label) in [
            (Scope::Project, "This project (.kodact/skills)"),
            (Scope::User, "Yours, every project"),
        ] {
            scopes = scopes.child(
                kit::chip(
                    t,
                    SharedString::from(format!("skill-scope-{}", scope.label())),
                    label,
                    self.scope == scope,
                )
                // A skill's folder stays where it is when edited.
                .when(editing && self.scope != scope, |d| d.opacity(0.5))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !matches!(this.form, Some(Some(_))) {
                        this.scope = scope;
                        cx.notify();
                    }
                })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(kit::back(t, "skill-back", "Skills").on_click(cx.listener(|this, _, _, cx| {
                this.back(cx);
            })))
            .child(kit::header(
                t,
                if editing { "Edit skill" } else { "New skill" },
                "Write the description for the model: what the skill does and when it applies. The instructions load only then.",
                None,
            ))
            .child(kit::field(t, "Saved in", None, scopes.into_any_element()))
            .child(div().w(px(260.)).child(kit::field(t, "Name", None, kit::line(t, &self.name, true))))
            .child(kit::field(t, "What it does, and when to use it", None, kit::line(t, &self.description, false)))
            .child(kit::field(
                t,
                "Instructions",
                Some("Saved as SKILL.md. Put scripts or references beside it in the skill's folder."),
                kit::area(t, &self.body, false),
            ))
            .children(kit::status(t, &self.error))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(kit::cancel_button(t, "skill-cancel").on_click(cx.listener(|this, _, _, cx| {
                        this.back(cx);
                    })))
                    .child(kit::save_button(t, "skill-save", self.saving).when(!self.saving, |d| {
                        d.on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                    })),
            )
            .into_any_element()
    }
}

impl Render for SkillsEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.state.read(cx).skills_install {
            return self.install.clone().into_any_element();
        }
        if self.form.is_some() {
            self.form(&t, cx)
        } else {
            self.list(&t, cx)
        }
    }
}
