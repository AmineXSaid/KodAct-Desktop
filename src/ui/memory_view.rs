//! Settings → Memory and rules: the instruction files ForgeCLI puts in front
//! of the model in every chat (`FORGE.md` yours and the project's, or
//! `AGENTS.md`, and `FORGE.local.md` for this computer), in the order it
//! reads them. Open one in your editor, or make the project's first file.
//! Rules that load only for some paths live in Rules and memory
//! (Ctrl/⌘ Shift R).

use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::icon;
use crate::forge_files::{self as ff};
use crate::state::{AppState, SettingsTab};
use crate::theme::{ActiveTheme, Theme};

pub struct MemoryView {
    state: Entity<AppState>,
    files: Option<Vec<ff::Memory>>,
    reading: bool,
    _sub: Subscription,
}

/// What a new memory file starts with.
const STARTER: &str = "# Project notes for Forge\n\n\
Forge reads this file at the start of every chat in this project.\n\n\
- How to build and test: \n\
- Conventions to follow: \n\
- Things to avoid: \n";

impl MemoryView {
    pub fn new(state: Entity<AppState>, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _sub: cx.observe(&state, |this, state, cx| {
                let s = state.read(cx);
                if s.settings_tab == SettingsTab::Memory && !s.engine.loading {
                    this.read(cx);
                }
                cx.notify();
            }),
            state,
            files: None,
            reading: false,
        }
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            return;
        }
        self.reading = true;
        let project = self.state.read(cx).project_dir.clone();
        cx.spawn(async move |this, cx| {
            let files = cx
                .background_spawn(async move { ff::memory_files(&project) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.reading = false;
                if this.files.as_ref() != Some(&files) {
                    this.files = Some(files);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for MemoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t: Theme = cx.theme().clone();
        if self.files.is_none() {
            self.read(cx);
        }
        let project = self.state.read(cx).project_dir.clone();
        let mut out = div().flex().flex_col().gap(px(10.)).child(kit::header(
            &t,
            "Memory and rules",
            "What Forge reads at the start of every chat: your notes, then the project's, top folder first. Keep them short: every line costs context in every chat.",
            None,
        ));
        let Some(files) = self.files.clone() else {
            return out.child(kit::loading(&t));
        };
        for (i, f) in files.into_iter().enumerate() {
            let shown = kit::short(&f.path, &project);
            let detail = if f.exists {
                format!("{} · about {} tokens", f.kind, f.bytes.div_ceil(4))
            } else {
                format!("{} · not made yet", f.kind)
            };
            let path = f.path.clone();
            let action = if f.exists {
                kit::link(
                    &t,
                    SharedString::from(format!("open-mem-{i}")),
                    "icons/external-link.svg",
                    "Open",
                )
                .on_click(move |_, _, cx| cx.open_with_system(&path))
            } else {
                kit::link(
                    &t,
                    SharedString::from(format!("make-mem-{i}")),
                    "icons/plus.svg",
                    "Make it",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let path = path.clone();
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let open = path.clone();
                    let task = this.state.update(cx, |s, cx| {
                        s.engine_write(
                            format!("Made {name}. New chats read it."),
                            move |_| {
                                if path.exists() {
                                    return Ok(());
                                }
                                ff::write_atomic(&path, STARTER)
                            },
                            cx,
                        )
                    });
                    cx.spawn(async move |this, cx| {
                        if task.await.is_ok() {
                            let _ = this.update(cx, |this, cx| {
                                this.files = None;
                                cx.open_with_system(&open);
                                cx.notify();
                            });
                        }
                    })
                    .detach();
                }))
            };
            out = out.child(
                kit::card(&t)
                    .when(!f.exists, |d| d.opacity(0.7))
                    .child(icon(
                        "icons/file-text.svg",
                        t.size.icon,
                        t.colors.text_muted,
                    ))
                    .child(kit::card_text(&t, shown, true, Some(detail), None))
                    .child(action),
            );
        }
        out.child(
            div().flex().items_center().gap(px(14.)).pt(px(4.)).child(
                kit::link(
                    &t,
                    "open-rules",
                    "icons/list-todo.svg",
                    "Rules that load for some files only",
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.state.update(cx, |s, cx| {
                        s.settings_open = false;
                        cx.notify();
                    });
                    window.dispatch_action(Box::new(super::OpenRules), cx)
                })),
            ),
        )
    }
}
