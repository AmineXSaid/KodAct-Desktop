//! Settings → Skills → Install: skills from a GitHub repository (or a link to
//! a folder in one), a git URL, a folder or a `.zip` / `.skill` file, into
//! Claude Code's folders (yours, `~/.claude/skills`, or the project's
//! `.claude/skills`), so they work in Forge and in Claude Code alike. In a
//! chat on WSL or SSH they install on that machine, where its ForgeCLI runs.
//!
//! First it looks: what the source holds and, for each skill, what it would
//! run and allow without asking. Nothing runs while installing.

use std::collections::BTreeSet;
use std::path::PathBuf;

use gpui::component::input::InputState;
use gpui::{
    div, prelude::*, px, relative, Context, Entity, IntoElement, PathPromptOptions, Render,
    SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::icon;
use crate::skill_install::{self as si, Found, FoundSkill, Source};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

/// Where the skills go.
#[derive(Clone, Copy, PartialEq)]
enum Into {
    /// `~/.claude/skills`: every project.
    Yours,
    /// The project's `.claude/skills`: shared with everyone on it.
    Project,
}

/// What the panel holds once it has looked.
struct Looked {
    source: Source,
    /// The folder looked in (inside `work` unless the source is a folder).
    root: PathBuf,
    /// Downloads for this look, removed when done.
    work: PathBuf,
    found: Found,
    /// The skills to install, by index.
    picked: BTreeSet<usize>,
    /// Those already installed where they'd go: installing replaces them.
    there: BTreeSet<usize>,
}

pub struct SkillInstall {
    state: Entity<AppState>,
    source: Entity<InputState>,
    into: Into,
    looked: Option<Looked>,
    busy: Option<&'static str>,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl SkillInstall {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let source = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("github.com/owner/repo, owner/repo, a folder, or a .zip")
        });
        Self {
            _subs: vec![
                cx.observe(&state, |_, _, cx| cx.notify()),
                cx.subscribe_in(
                    &source,
                    window,
                    |this, _, ev: &gpui::component::input::InputEvent, _, cx| {
                        if matches!(ev, gpui::component::input::InputEvent::PressEnter { .. })
                            && this.busy.is_none()
                        {
                            this.look(cx);
                        }
                    },
                ),
            ],
            source,
            state,
            into: Into::Yours,
            looked: None,
            busy: None,
            error: None,
        }
    }

    /// Back to the skills list: what was downloaded goes.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.discard();
        self.error = None;
        self.busy = None;
        self.state.update(cx, |s, cx| {
            s.skills_install = false;
            cx.notify();
        });
    }

    fn discard(&mut self) {
        if let Some(l) = self.looked.take() {
            let work = l.work;
            std::thread::spawn(move || {
                let _ = std::fs::remove_dir_all(work);
            });
        }
    }

    /// Where the skills go on this computer (None on a remote).
    fn local_root(&self, cx: &Context<Self>) -> Option<PathBuf> {
        let s = self.state.read(cx);
        if s.current_remote().is_some() {
            return None;
        }
        match self.into {
            Into::Yours => crate::forge_files::compat_home().map(|h| h.join("skills")),
            Into::Project => Some(s.project_dir.join(".claude").join("skills")),
        }
    }

    fn pick(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: Some(
                if folder {
                    "Use this folder"
                } else {
                    "Use this file"
                }
                .into(),
            ),
        });
        let input = self.source.clone();
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                if let Some(p) = paths.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| {
                        input.update(cx, |i, cx| i.set_value(p.display().to_string(), window, cx));
                        this.look(cx);
                    });
                }
            }
        })
        .detach();
    }

    /// Fetch the source and see what it holds.
    fn look(&mut self, cx: &mut Context<Self>) {
        let text = self.source.read(cx).value().to_string();
        let cwd = self.state.read(cx).project_dir.clone();
        let source = match si::parse_source(&text, &cwd) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.into());
                return cx.notify();
            }
        };
        self.discard();
        self.error = None;
        self.busy = Some("Looking…");
        let root_here = self.local_root(cx);
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_spawn(async move {
                    let work = si::work_dir();
                    match si::fetch(&source, &work) {
                        Ok(root) => {
                            let found = si::scan(&root);
                            Ok(Looked {
                                there: found
                                    .skills
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, s)| {
                                        root_here
                                            .as_ref()
                                            .is_some_and(|r| si::skill_target(s, r).exists())
                                    })
                                    .map(|(i, _)| i)
                                    .collect(),
                                picked: found
                                    .skills
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, s)| s.problem.is_none())
                                    .map(|(i, _)| i)
                                    .collect(),
                                source,
                                root,
                                work,
                                found,
                            })
                        }
                        Err(e) => {
                            let _ = std::fs::remove_dir_all(&work);
                            Err(e)
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match r {
                    Ok(l) if l.found.is_empty() => {
                        this.error = Some(
                            "No skills there: a skill is a folder with a SKILL.md in it.".into(),
                        );
                        let _ = std::fs::remove_dir_all(&l.work);
                    }
                    Ok(l) => this.looked = Some(l),
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn install(&mut self, cx: &mut Context<Self>) {
        let Some(l) = self.looked.as_ref() else {
            return;
        };
        let skills: Vec<FoundSkill> = l
            .picked
            .iter()
            .filter_map(|i| l.found.skills.get(*i).cloned())
            .collect();
        if skills.is_empty() {
            return;
        }
        let (source, root) = (l.source.clone(), l.root.clone());
        let s = self.state.read(cx);
        let remote = s.current_remote().map(|r| (s.remote_extra(&r.target), r));
        let local = self.local_root(cx);
        let user = self.into == Into::Yours;
        let names: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
        let place = match &remote {
            Some((_, r)) => format!(" on {}", r.target.label()),
            None => String::new(),
        };
        let done = match names.as_slice() {
            [one] => format!(
                "Installed the {one} skill{place}. New chats can use it, here and in Claude Code."
            ),
            _ => format!(
                "Installed {} skills{place}. New chats can use them, here and in Claude Code.",
                names.len()
            ),
        };
        self.busy = Some("Installing…");
        self.error = None;
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                done,
                move |_| {
                    for sk in &skills {
                        match (&remote, &local) {
                            (Some((extra, r)), _) => {
                                si::install_skill_remote(sk, &source, &root, r, user, true, extra)
                                    .map(|_| ())?
                            }
                            (None, Some(dest)) => {
                                si::install_skill(sk, &source, &root, dest, true).map(|_| ())?
                            }
                            (None, None) => {
                                return Err("Claude Code's folder is unknown here.".into())
                            }
                        }
                    }
                    Ok(())
                },
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match r {
                    Ok(()) => this.close(cx),
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// A plugin: copied under Forge's plugins folder and added to `pluginDirs`.
    fn install_plugin(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.looked.as_ref().and_then(|l| l.found.plugin.clone()) else {
            return;
        };
        let Some(root) = si::plugins_root() else {
            return;
        };
        let source = self
            .looked
            .as_ref()
            .map(|l| l.source.label())
            .unwrap_or_default();
        let name = p.name.clone();
        self.busy = Some("Installing…");
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Installed the {name} plugin. New chats load it."),
                move |project| si::install_local_plugin(&p, &source, &root, project),
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match r {
                    Ok(()) => this.close(cx),
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn add_marketplace(&mut self, cx: &mut Context<Self>) {
        let Some(l) = self.looked.as_ref() else {
            return;
        };
        let Some(m) = l.found.marketplace.clone() else {
            return;
        };
        let source = l.source.label();
        match si::remember_marketplace(&m.name, Some(&source)) {
            Ok(()) => {
                self.close(cx);
                self.state.update(cx, |s, cx| {
                    s.show_toast(format!("Added the {} marketplace.", m.name), cx);
                    s.open_marketplaces(cx);
                });
            }
            Err(e) => {
                self.error = Some(e.into());
                cx.notify();
            }
        }
    }

    fn skill_card(
        &self,
        t: &Theme,
        i: usize,
        sk: &FoundSkill,
        l: &Looked,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let on = l.picked.contains(&i);
        let mut notes: Vec<(gpui::Hsla, String)> = vec![];
        if let Some(p) = &sk.problem {
            notes.push((t.extra.danger, format!("Can't be installed: {p}.")));
        }
        if !sk.commands.is_empty() {
            notes.push((
                t.extra.warning,
                format!(
                    "Runs before each use: {}",
                    sk.commands
                        .iter()
                        .map(|c| format!("`{}`", c.lines().next().unwrap_or(c)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        if !sk.allowed_tools.is_empty() {
            notes.push((
                t.colors.text_muted,
                format!(
                    "Allowed without asking while it runs: {}",
                    sk.allowed_tools.join(", ")
                ),
            ));
        }
        if !sk.scripts.is_empty() {
            notes.push((
                t.colors.text_muted,
                format!("Scripts: {}", sk.scripts.join(", ")),
            ));
        }
        if l.there.contains(&i) {
            notes.push((
                t.colors.text_muted,
                "Installed already: this replaces it.".into(),
            ));
        }
        let detail = format!(
            "{} · {} file{}",
            sk.description,
            sk.files,
            if sk.files == 1 { "" } else { "s" }
        );
        let mut text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(kit::card_text(t, sk.name.clone(), true, Some(detail), None));
        for (c, n) in notes {
            text = text.child(
                div()
                    .text_size(t.text.caption)
                    .text_color(c)
                    .line_height(relative(1.4))
                    .child(n),
            );
        }
        kit::card(t)
            .items_start()
            .when(!on, |d| d.opacity(0.7))
            .child(div().pt(px(2.)).child(icon(
                "icons/wand-sparkles.svg",
                t.size.icon,
                t.colors.text_muted,
            )))
            .child(text)
            .when(sk.problem.is_none(), |d| {
                d.child(
                    kit::chip(
                        t,
                        SharedString::from(format!("pick-skill-{i}")),
                        if on { "Included" } else { "Skipped" },
                        on,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(l) = this.looked.as_mut() {
                            if !l.picked.remove(&i) {
                                l.picked.insert(i);
                            }
                        }
                        cx.notify();
                    })),
                )
            })
            .into_any_element()
    }
}

impl Render for SkillInstall {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let remote = s.current_remote().map(|r| r.target.label());
        let project_name = s
            .project_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let busy = self.busy;
        let mut out = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(kit::back(&t, "install-back", "Skills").on_click(cx.listener(|this, _, _, cx| this.close(cx))))
            .child(kit::header(
                &t,
                "Install skills",
                "From a GitHub repository (or a link to one folder in it), a git URL, a folder or a .zip. Forge keeps them in the same folders as Claude Code, so they work in both. Nothing they carry runs while they install.",
                None,
            ));

        let find = if self.looked.is_none() {
            theme::primary_button(&t, busy.unwrap_or("Find skills"))
        } else {
            theme::ghost_button(&t, busy.unwrap_or("Look again"))
        }
        .id("install-find")
        .when(busy.is_some(), |d| d.opacity(0.6))
        .when(busy.is_none(), |d| {
            d.on_click(cx.listener(|this, _, _, cx| this.look(cx)))
        });
        out = out.child(kit::field(
            &t,
            "Where from",
            Some("owner/repo picks every skill in the repository; a link to a folder picks that one."),
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(div().flex().gap(px(8.)).items_center().child(div().flex_1().min_w_0().child(kit::line(&t, &self.source, true))).child(find))
                .child(
                    div()
                        .flex()
                        .gap(px(14.))
                        .child(kit::link(&t, "install-folder", "icons/folder-open.svg", "Choose a folder").on_click(cx.listener(|this, _, window, cx| this.pick(true, window, cx))))
                        .child(kit::link(&t, "install-zip", "icons/file-archive.svg", "Choose a .zip or .skill file").on_click(cx.listener(|this, _, window, cx| this.pick(false, window, cx)))),
                )
                .into_any_element(),
        ));

        let mut into = div().flex().gap(px(6.));
        for (v, label) in [
            (
                Into::Yours,
                "Yours, every project (~/.claude/skills)".to_string(),
            ),
            (
                Into::Project,
                format!("{project_name} only (.claude/skills)"),
            ),
        ] {
            into = into.child(
                kit::chip(
                    &t,
                    SharedString::from(format!("install-into-{}", v == Into::Yours)),
                    label,
                    self.into == v,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.into = v;
                    cx.notify();
                })),
            );
        }
        let hint = match &remote {
            Some(r) => format!("On {r}, where this project's chats run."),
            None => "Claude Code uses the same folders, so skills work in both.".to_string(),
        };
        out = out.child(kit::field(
            &t,
            "Install into",
            Some(&hint),
            into.into_any_element(),
        ));
        out = out.children(kit::status(&t, &self.error));

        if let Some(l) = &self.looked {
            if let Some(m) = &l.found.marketplace {
                out = out.child(kit::group(&t, "A plugin marketplace")).child(
                    kit::card(&t)
                        .child(icon("icons/puzzle.svg", t.size.icon, t.colors.text_muted))
                        .child(kit::card_text(
                            &t,
                            m.name.clone(),
                            true,
                            Some(format!(
                                "{} plugin{} to choose from. {}",
                                m.plugins.len(),
                                if m.plugins.len() == 1 { "" } else { "s" },
                                m.description
                            )),
                            None,
                        ))
                        .child(
                            theme::primary_button(&t, "Add marketplace")
                                .id("install-add-market")
                                .on_click(cx.listener(|this, _, _, cx| this.add_marketplace(cx))),
                        ),
                );
            }
            if let Some(p) = &l.found.plugin {
                let can = remote.is_none();
                out = out.child(kit::group(&t, "A plugin")).child(
                    kit::card(&t)
                        .child(icon("icons/puzzle.svg", t.size.icon, t.colors.text_muted))
                        .child(kit::card_text(
                            &t,
                            format!("{}{}", p.name, p.version.as_ref().map(|v| format!(" {v}")).unwrap_or_default()),
                            true,
                            Some(p.description.clone()),
                            (!can).then(|| (t.colors.text_muted, "Plugins install on this computer; open a local project to add it.".to_string())),
                        ))
                        .when(can, |d| {
                            d.child(theme::ghost_button(&t, "Install plugin").id("install-plugin").on_click(cx.listener(|this, _, _, cx| this.install_plugin(cx))))
                        }),
                );
            }
            if !l.found.skills.is_empty() {
                let n = l.found.skills.len();
                out = out.child(kit::group(
                    &t,
                    format!("{n} skill{} found", if n == 1 { "" } else { "s" }),
                ));
                let cards: Vec<gpui::AnyElement> = l
                    .found
                    .skills
                    .iter()
                    .enumerate()
                    .map(|(i, sk)| self.skill_card(&t, i, sk, l, cx))
                    .collect();
                out = out.children(cards);
                let picked = l.picked.len();
                let label = match (busy, picked) {
                    (Some(b), _) => b.to_string(),
                    (None, 1) => "Install 1 skill".to_string(),
                    (None, n) => format!("Install {n} skills"),
                };
                out = out.child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            kit::cancel_button(&t, "install-cancel")
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        )
                        .child(
                            theme::primary_button(&t, label)
                                .id("install-go")
                                .when(busy.is_some() || picked == 0, |d| d.opacity(0.6))
                                .when(busy.is_none() && picked > 0, |d| {
                                    d.on_click(cx.listener(|this, _, _, cx| this.install(cx)))
                                }),
                        ),
                );
            }
        }
        out
    }
}
