//! The Knowledge card on a project page's rail (`knowledge.rs`): the
//! project's reference files, each Always (its text in every new chat) or
//! On demand (read by KodAct when it matters), with a meter of what Always
//! spends against its budget.
//!
//! Add files with the picker or by dropping them on the card; write a note
//! in place; or add from GitHub: a repository's files and folders, picked
//! from its tree, synced on demand (`github_knowledge.rs`). A click opens a file in the side panel; × takes it out (a copy
//! KodAct made is deleted, a file of the project's own stays).

use gpui::component::input::{Input, InputState, TextareaState};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, IntoElement,
    PathPromptOptions, Render, SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::*;
use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme};

pub struct ProjectKnowledge {
    state: Entity<AppState>,
    /// Writing a note: its title and text.
    title: Entity<InputState>,
    note: Entity<TextareaState>,
    writing: bool,
    /// Adding from GitHub: the repository typed, its tree once read, what
    /// is ticked and which folders are open.
    gh_open: bool,
    gh_input: Entity<InputState>,
    gh_loading: bool,
    gh_error: Option<String>,
    gh_tree: Option<crate::github_knowledge::Tree>,
    gh_picked: std::collections::BTreeSet<String>,
    gh_dirs: std::collections::BTreeSet<String>,
    _subs: Vec<Subscription>,
}

impl ProjectKnowledge {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let gh_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. owner/repo, or a github.com link")
        });
        Self {
            _subs: vec![
                cx.observe(&state, |_, _, cx| cx.notify()),
                // Enter in the repository field reads it.
                cx.subscribe(
                    &gh_input,
                    |this, _, ev: &gpui::component::input::InputEvent, cx| {
                        if matches!(ev, gpui::component::input::InputEvent::PressEnter { .. }) {
                            this.gh_look(cx);
                        }
                    },
                ),
            ],
            state,
            title: cx
                .new(|cx| InputState::new(window, cx).placeholder("Title (e.g. Tone of voice)")),
            note: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("What every chat here should know: facts, decisions, a glossary…")
            }),
            writing: false,
            gh_open: false,
            gh_input,
            gh_loading: false,
            gh_error: None,
            gh_tree: None,
            gh_picked: Default::default(),
            gh_dirs: Default::default(),
        }
    }

    /// Read the typed repository's tree, off the UI thread.
    fn gh_look(&mut self, cx: &mut Context<Self>) {
        let typed = self.gh_input.read(cx).value().to_string();
        let Some(r) = crate::github_knowledge::parse(&typed) else {
            self.gh_error =
                Some("Type a repository as owner/repo, or paste its github.com link.".into());
            return cx.notify();
        };
        self.gh_loading = true;
        self.gh_error = None;
        self.gh_tree = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let folder = r.folder.clone();
            let got = cx
                .background_spawn(async move { crate::github_knowledge::tree(&r) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.gh_loading = false;
                match got {
                    Ok(t) => {
                        this.gh_picked.clear();
                        this.gh_dirs.clear();
                        // A link into a folder opens on it, ticked.
                        if let Some(f) = folder {
                            let mut open = String::new();
                            for part in f.split('/') {
                                if !open.is_empty() {
                                    open.push('/');
                                }
                                open.push_str(part);
                                this.gh_dirs.insert(open.clone());
                            }
                            this.gh_picked.insert(f);
                        }
                        this.gh_tree = Some(t);
                    }
                    Err(e) => this.gh_error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Whether `path` is ticked, itself or through a folder above it.
    fn gh_ticked(&self, path: &str) -> bool {
        self.gh_picked.contains(path)
            || self
                .gh_picked
                .iter()
                .any(|p| path.starts_with(&format!("{p}/")))
    }

    /// Tick or untick `path`. Unticking a path under a ticked folder ticks
    /// that folder's other children instead, down to it.
    fn gh_toggle(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(tree) = &self.gh_tree else { return };
        if self.gh_ticked(path) {
            if let Some(top) = self
                .gh_picked
                .iter()
                .find(|p| path.starts_with(&format!("{p}/")))
                .cloned()
            {
                // Open the ticked folder down to `path`, ticking what's beside it.
                self.gh_picked.remove(&top);
                let mut at = top;
                while at != path {
                    let next = path[at.len() + 1..]
                        .split('/')
                        .next()
                        .map(|seg| format!("{at}/{seg}"))
                        .unwrap_or_default();
                    for n in &tree.nodes {
                        let child = n.path.strip_prefix(&format!("{at}/"));
                        if child.is_some_and(|c| !c.contains('/')) && n.path != next {
                            self.gh_picked.insert(n.path.clone());
                        }
                    }
                    at = next;
                }
            } else {
                self.gh_picked.remove(path);
            }
        } else {
            let under = format!("{path}/");
            self.gh_picked.retain(|p| !p.starts_with(&under));
            self.gh_picked.insert(path.to_string());
        }
        cx.notify();
    }

    fn gh_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.gh_open = false;
        self.gh_tree = None;
        self.gh_error = None;
        self.gh_picked.clear();
        self.gh_dirs.clear();
        self.gh_input
            .update(cx, |i, cx| i.set_value("", window, cx));
        cx.notify();
    }

    /// The picker: the repository field, then its tree to tick.
    fn gh_panel(&self, id: usize, t: &crate::theme::Theme, cx: &mut Context<Self>) -> AnyElement {
        let hover = t.colors.hover;
        let field = div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(kit::line(t, &self.gh_input, false)),
            )
            .child(
                theme::ghost_button(
                    t,
                    if self.gh_loading {
                        "Reading…"
                    } else {
                        "Look"
                    },
                )
                .id("gh-look")
                .on_click(cx.listener(|this, _, _, cx| this.gh_look(cx))),
            );
        let mut list = div()
            .id("gh-tree")
            .flex()
            .flex_col()
            .max_h(px(320.))
            .overflow_y_scroll()
            .rounded(t.radius.field)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.colors.canvas)
            .py(t.space.xs);
        let mut summary = None;
        if let Some(tree) = &self.gh_tree {
            for (i, n) in tree.nodes.iter().enumerate() {
                // Shown when every folder above it is open.
                let parents: Vec<&str> = n
                    .path
                    .match_indices('/')
                    .map(|(i, _)| &n.path[..i])
                    .collect();
                if !parents.iter().all(|p| self.gh_dirs.contains(*p)) {
                    continue;
                }
                let depth = parents.len() as f32;
                let name = n.path.rsplit('/').next().unwrap_or(&n.path).to_string();
                let ticked = self.gh_ticked(&n.path);
                let partial = !ticked
                    && n.dir
                    && self
                        .gh_picked
                        .iter()
                        .any(|p| p.starts_with(&format!("{}/", n.path)));
                let open = self.gh_dirs.contains(&n.path);
                let (path, path2) = (n.path.clone(), n.path.clone());
                let dir = n.dir;
                list = list.child(
                    div()
                        .id(("gh-node", i))
                        .flex()
                        .items_center()
                        .gap(t.space.tight)
                        .h(t.size.row)
                        .pl(t.space.sm + px(16. * depth))
                        .pr(t.space.sm)
                        .cursor_pointer()
                        .hover(move |st| st.bg(hover))
                        .child(
                            div()
                                .id(("gh-chevron", i))
                                .w(t.size.icon_sm)
                                .flex_none()
                                .when(dir, |d| {
                                    d.child(icon(
                                        if open {
                                            "icons/chevron-down.svg"
                                        } else {
                                            "icons/chevron-right.svg"
                                        },
                                        t.size.icon_xs,
                                        t.colors.text_subtle,
                                    ))
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            if !this.gh_dirs.remove(&path2) {
                                                this.gh_dirs.insert(path2.clone());
                                            }
                                            cx.notify();
                                        },
                                    ))
                                }),
                        )
                        .child(icon(
                            if ticked {
                                "icons/square-check.svg"
                            } else if partial {
                                "icons/square-minus.svg"
                            } else {
                                "icons/square.svg"
                            },
                            t.size.icon_sm,
                            if ticked || partial {
                                t.colors.primary
                            } else {
                                t.colors.text_subtle
                            },
                        ))
                        .child(
                            gpui::img(super::file_icon::asset(&name, dir, &n.path))
                                .flex_none()
                                .size(t.size.icon),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.body_sm)
                                .text_color(t.colors.text)
                                .child(name),
                        )
                        .when(!dir, |d| {
                            d.child(
                                div()
                                    .flex_none()
                                    .text_size(t.text.caption)
                                    .text_color(if n.size > crate::github_knowledge::FILE_MAX {
                                        t.extra.warning
                                    } else {
                                        t.colors.text_subtle
                                    })
                                    .child(if n.size > crate::github_knowledge::FILE_MAX {
                                        "too large".to_string()
                                    } else {
                                        size(n.size)
                                    }),
                            )
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.gh_toggle(&path, cx))),
                );
            }
            let picks: Vec<String> = self.gh_picked.iter().cloned().collect();
            let files = crate::github_knowledge::expand(&tree.nodes, &picks);
            let bytes: u64 = tree
                .nodes
                .iter()
                .filter(|n| files.contains(&n.path))
                .map(|n| n.size)
                .sum();
            let capped = files.len() >= crate::github_knowledge::FILES_MAX;
            let ready = !files.is_empty();
            let tree2 = tree.clone();
            // The count on its own line, the buttons under it: the rail is
            // too narrow for both side by side.
            summary = Some(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(t.space.sm)
                    .child(
                        div()
                            .w_full()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(match files.len() {
                                0 => format!(
                                    "{} · {}: tick files or folders",
                                    tree.repo, tree.reference
                                ),
                                n => format!(
                                    "{n} file{} · {}{}",
                                    if n == 1 { "" } else { "s" },
                                    size(bytes),
                                    if capped { " · the first 300" } else { "" }
                                ),
                            }),
                    )
                    .child(
                        theme::text_button(t, "Cancel")
                            .id("gh-cancel")
                            .on_click(cx.listener(|this, _, window, cx| this.gh_close(window, cx))),
                    )
                    .child(
                        theme::primary_button(t, "Add to knowledge")
                            .id("gh-add")
                            .when(!ready, |d| d.opacity(0.5))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let picks: Vec<String> = this.gh_picked.iter().cloned().collect();
                                if picks.is_empty() {
                                    return;
                                }
                                let tree = tree2.clone();
                                this.state.update(cx, |s, cx| {
                                    s.knowledge_github_add(id, tree, picks, cx)
                                });
                                this.gh_close(window, cx);
                            })),
                    ),
            );
        }
        let has_tree = self.gh_tree.is_some();
        let truncated = self.gh_tree.as_ref().is_some_and(|t| t.truncated);
        div()
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .p(t.space.md)
            .rounded(t.radius.field)
            .bg(t.extra.inset)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(icon("icons/folder-git-2.svg", t.size.icon_sm, t.colors.text_muted))
                    .child(
                        div()
                            .flex_1()
                            .text_size(t.text.body_sm)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_strong)
                            .child("Add from GitHub"),
                    )
                    .when(!has_tree, |d| {
                        d.child(
                            theme::text_button(t, "Cancel")
                                .id("gh-cancel-top")
                                .on_click(cx.listener(|this, _, window, cx| this.gh_close(window, cx))),
                        )
                    }),
            )
            .child(field)
            .children(self.gh_error.clone().map(|e| {
                div()
                    .text_size(t.text.caption)
                    .text_color(t.extra.danger)
                    .child(e)
            }))
            .when(!has_tree && self.gh_error.is_none(), |d| {
                d.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .line_height(relative(1.4))
                        .child("KodAct copies the files you tick into this project, so every chat can read them; Sync brings them up to the branch's latest. Private repositories need gh signed in (gh auth login)."),
                )
            })
            // The tree rises in once GitHub has listed it.
            .when(has_tree, |d| {
                let key = self.gh_tree.as_ref().map(|t| t.commit.clone()).unwrap_or_default();
                d.child(super::motion::rise_in(
                    list,
                    SharedString::from(format!("gh-tree-in-{key}")),
                    4.,
                    t,
                ))
            })
            .when(truncated, |d| {
                d.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.extra.warning)
                        .child("This repository is very large: GitHub listed part of it. Paste a link to a folder to see all of that folder."),
                )
            })
            .children(summary)
            .into_any_element()
    }

    fn pick(&self, id: usize, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add to knowledge".into()),
        });
        let state = self.state.clone();
        cx.spawn(async move |_, cx| {
            let result = picked.await;
            state.update(cx, |s, cx| match result {
                Ok(Ok(Some(paths))) => s.knowledge_add(id, paths, cx),
                Ok(Ok(None)) => {}
                _ => s.show_toast("The file picker couldn't open on this system.", cx),
            });
        })
        .detach();
    }

    fn save_note(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value().to_string();
        let text = self.note.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        self.title.update(cx, |i, cx| i.set_value("", window, cx));
        self.note.update(cx, |i, cx| i.set_value("", window, cx));
        self.writing = false;
        self.state
            .update(cx, |s, cx| s.knowledge_add_text(id, title, text, cx));
        cx.notify();
    }
}

/// "12 KB", "3.4 MB".
fn size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{} KB", b.div_ceil(1024)),
        b => format!("{:.1} MB", b as f64 / (1024. * 1024.)),
    }
}

impl Render for ProjectKnowledge {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Screen::Project(id) = s.screen else {
            return div().into_any_element();
        };
        let Some(k) = s
            .project_brief
            .as_ref()
            .filter(|(b, _)| *b == id)
            .map(|(_, b)| b.knowledge.clone())
        else {
            return div().into_any_element();
        };
        let scale = window.scale_factor();
        let hover = t.colors.hover;
        let on_demand = k.items.iter().filter(|i| !i.included && !i.missing).count();
        let over = k
            .items
            .iter()
            .filter(|i| i.always && !i.included && !i.missing && i.tokens > 0)
            .count();

        // The meter: what Always spends of its budget.
        let share = (k.included_tokens as f32 / k.budget.max(1) as f32).min(1.);
        let meter = div()
            .flex()
            .flex_col()
            .gap(t.space.xs)
            .child(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded(t.radius.pill)
                    .overflow_hidden()
                    .bg(t.colors.hover)
                    .child(
                        div()
                            .h_full()
                            .w(relative(if k.included_tokens > 0 {
                                share.max(0.012)
                            } else {
                                0.
                            }))
                            .bg(if share > 0.9 {
                                t.extra.warning
                            } else {
                                t.extra.success
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child(format!(
                        "Always: {} of {} tokens · {on_demand} On demand",
                        crate::context::short(k.included_tokens),
                        crate::context::short(k.budget)
                    )),
            );

        // A repository's files show as the repository, below.
        let rows = k
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| !crate::names::starts_in_project_dir(&i.rel, "knowledge/github/"))
            .map(|(i, item)| {
                let name = item.rel.rsplit('/').next().unwrap_or(&item.rel).to_string();
                let folder = item
                    .rel
                    .rsplit_once('/')
                    .map(|(d, _)| d.to_string())
                    .unwrap_or_default();
                let mark: AnyElement = match crate::office::Kind::of(&name) {
                    Some(kind) => super::file_mark::render(kind, 20., scale),
                    None => gpui::img(super::file_icon::asset(&name, false, &item.rel))
                        .flex_none()
                        .size(t.size.icon)
                        .into_any_element(),
                };
                let detail = if item.missing {
                    "missing from the folder".to_string()
                } else if item.tokens == 0 {
                    format!("{} · no text to send", size(item.bytes))
                } else {
                    format!(
                        "{} · {} tokens",
                        size(item.bytes),
                        crate::context::short(item.tokens)
                    )
                };
                let (open, flip, drop) =
                    (self.state.clone(), self.state.clone(), self.state.clone());
                let (rel, rel2, rel3) = (item.rel.clone(), item.rel.clone(), item.rel.clone());
                let always = item.always;
                let note = (item.always && !item.included && item.tokens > 0)
                    .then_some("Over the budget: read on demand");
                div()
                    .id(("knowledge-row", i))
                    .group("knowledge-row")
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .px(t.space.sm)
                    .py(t.space.xs)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover))
                    .child(div().w(px(20.)).flex().justify_center().child(mark))
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
                                    .text_color(if item.missing {
                                        t.colors.text_subtle
                                    } else {
                                        t.colors.text
                                    })
                                    .child(name),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(t.text.caption)
                                    .text_color(if note.is_some() {
                                        t.extra.warning
                                    } else {
                                        t.colors.text_subtle
                                    })
                                    .child(note.map(String::from).unwrap_or_else(|| {
                                        if folder.is_empty() || crate::knowledge::is_dir(&folder) {
                                            detail.clone()
                                        } else {
                                            format!("{folder} · {detail}")
                                        }
                                    })),
                            ),
                    )
                    .when(item.tokens > 0, |d| {
                        d.child(
                            kit::chip(
                                &t,
                                ("knowledge-mode", i),
                                if always { "Always" } else { "On demand" },
                                always,
                            )
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                let rel = rel2.clone();
                                flip.update(cx, |s, cx| {
                                    s.knowledge_set_always(id, rel, !always, cx)
                                })
                            }),
                        )
                    })
                    .child(
                        div()
                            .invisible()
                            .group_hover("knowledge-row", |d| d.visible())
                            .child(
                                super::button::Button::icon(
                                    SharedString::from(format!("knowledge-remove-{i}")),
                                    "icons/x.svg",
                                    "Take it out of the knowledge",
                                )
                                .compact()
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    let rel = rel3.clone();
                                    drop.update(cx, |s, cx| s.knowledge_remove(id, rel, cx))
                                }),
                            ),
                    )
                    .on_click(move |_, _, cx| {
                        let rel = rel.clone();
                        open.update(cx, |s, cx| s.open_project_file(rel, cx))
                    })
            });

        let writer = self.writing.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(
                    div()
                        .px(t.space.sm)
                        .rounded(t.radius.field)
                        .border_1()
                        .border_color(t.colors.border)
                        .bg(t.colors.canvas)
                        .child(Input::new(&self.title).appearance(false)),
                )
                .child(kit::area(&t, &self.note, false))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(t.space.sm)
                        .child(theme::text_button(&t, "Cancel").id("note-cancel").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.writing = false;
                                cx.notify();
                            }),
                        ))
                        .child(
                            theme::primary_button(&t, "Add note")
                                .id("note-save")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.save_note(id, window, cx)
                                })),
                        ),
                )
        });

        let repos = s
            .project_brief
            .as_ref()
            .filter(|(b, _)| *b == id)
            .map(|(_, b)| b.github.clone())
            .unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let repo_rows = repos.into_iter().enumerate().map(|(i, src)| {
            let (sync, drop) = (self.state.clone(), self.state.clone());
            let (r1, r2) = (src.repo.clone(), src.repo.clone());
            let minutes = (now.saturating_sub(src.synced_at) / 60) as u32;
            let when = if minutes == 0 {
                "synced just now".to_string()
            } else {
                format!("synced {} ago", super::sidebar::ago(minutes))
            };
            div()
                .id(("knowledge-repo", i))
                .group("knowledge-repo")
                .flex()
                .items_center()
                .gap(t.space.sm)
                .px(t.space.sm)
                .py(t.space.xs)
                .rounded(t.radius.button)
                .hover(move |st| st.bg(hover))
                .child(div().w(px(20.)).flex().justify_center().child(icon(
                    "icons/folder-git-2.svg",
                    t.size.icon_sm,
                    t.colors.text_muted,
                )))
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
                                .text_color(t.colors.text)
                                .child(src.repo.clone()),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(format!(
                                    "{} · {} file{} · {when}",
                                    src.reference,
                                    src.files.len(),
                                    if src.files.len() == 1 { "" } else { "s" }
                                )),
                        ),
                )
                .child(
                    theme::text_button(&t, "Sync")
                        .id(("knowledge-repo-sync", i))
                        .text_size(t.text.caption)
                        .on_click(move |_, _, cx| {
                            let repo = r1.clone();
                            sync.update(cx, |s, cx| s.knowledge_github_sync(id, repo, cx))
                        }),
                )
                .child(
                    div()
                        .invisible()
                        .group_hover("knowledge-repo", |d| d.visible())
                        .child(
                            super::button::Button::icon(
                                SharedString::from(format!("knowledge-repo-remove-{i}")),
                                "icons/x.svg",
                                "Take this repository out of the knowledge",
                            )
                            .compact()
                            .on_click(move |_, _, cx| {
                                let repo = r2.clone();
                                drop.update(cx, |s, cx| s.knowledge_github_remove(id, repo, cx))
                            }),
                        ),
                )
        });
        let gh_panel = self.gh_open.then(|| {
            super::motion::rise_in(
                div().child(self.gh_panel(id, &t, cx)),
                "gh-panel-in",
                6.,
                &t,
            )
        });
        let empty = k.items.is_empty();
        div()
            .id("knowledge-card")
            .flex()
            .flex_col()
            .gap(t.space.md)
            .p(t.space.lg)
            .rounded(t.radius.card)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            // Files dropped on the card join the knowledge.
            .drag_over::<gpui::ExternalPaths>({
                let (edge, wash) = (t.colors.primary, t.colors.primary_tint);
                move |d, _, _, _| d.border_color(edge).bg(wash)
            })
            .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                let paths = paths.paths().to_vec();
                this.state.update(cx, |s, cx| s.knowledge_add(id, paths, cx));
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.text_muted)
                            .child("Knowledge"),
                    )
                    .when(!empty, |d| {
                        d.child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(format!("{} file{}", k.items.len(), if k.items.len() == 1 { "" } else { "s" })),
                        )
                    }),
            )
            .when(!empty, |d| d.child(meter))
            .when(over > 0, |d| {
                d.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.extra.warning)
                        .line_height(relative(1.4))
                        .child(format!(
                            "{over} Always file{} past the budget: KodAct reads {} on demand instead.",
                            if over == 1 { " is" } else { "s are" },
                            if over == 1 { "it" } else { "them" }
                        )),
                )
            })
            .when(empty && !self.writing, |d| {
                d.child(
                    div()
                        .p(t.space.md)
                        .rounded(t.radius.field)
                        .border_1()
                        .border_dashed()
                        .border_color(t.colors.border)
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_subtle)
                        .line_height(relative(1.4))
                        .child("Drop specs, briefs or data here. KodAct reads them when they matter; set the ones every chat needs to Always."),
                )
            })
            .child(div().flex().flex_col().gap(px(2.)).children(rows).children(repo_rows))
            .children(writer)
            .children(gh_panel)
            .when(!self.writing && !self.gh_open, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(t.space.sm)
                        .child(
                            theme::ghost_button(&t, "Add files")
                                .id("knowledge-add")
                                .on_click(cx.listener(move |this, _, _, cx| this.pick(id, cx))),
                        )
                        .child(
                            theme::ghost_button(&t, "Write a note")
                                .id("knowledge-note")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.writing = true;
                                    this.title.update(cx, |i, cx| i.focus(window, cx));
                                    cx.notify();
                                })),
                        )
                        .child(
                            theme::ghost_button(&t, "Add from GitHub")
                                .id("knowledge-github")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.gh_open = true;
                                    this.gh_input.update(cx, |i, cx| i.focus(window, cx));
                                    cx.notify();
                                })),
                        ),
                )
            })
            .into_any_element()
    }
}
