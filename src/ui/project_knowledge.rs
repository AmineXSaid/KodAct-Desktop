//! The Knowledge card on a project page's rail (`knowledge.rs`): the
//! project's reference files, each Always (its text in every new chat) or
//! On demand (read by KodAct when it matters), with a meter of what Always
//! spends against its budget.
//!
//! Add files with the picker or by dropping them on the card; write a note
//! in place. A click opens a file in the side panel; × takes it out (a copy
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
    _subs: Vec<Subscription>,
}

impl ProjectKnowledge {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _subs: vec![cx.observe(&state, |_, _, cx| cx.notify())],
            state,
            title: cx
                .new(|cx| InputState::new(window, cx).placeholder("Title (e.g. Tone of voice)")),
            note: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("What every chat here should know: facts, decisions, a glossary…")
            }),
            writing: false,
        }
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

        let rows = k.items.iter().enumerate().map(|(i, item)| {
            let name = item.rel.rsplit('/').next().unwrap_or(&item.rel).to_string();
            let folder = item
                .rel
                .rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            let mark: AnyElement = match crate::office::Kind::of(&name) {
                Some(kind) => super::file_mark::render(kind, 20., scale),
                None => icon("icons/file-text.svg", t.size.icon_sm, t.colors.text_muted)
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
            let (open, flip, drop) = (self.state.clone(), self.state.clone(), self.state.clone());
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
                            flip.update(cx, |s, cx| s.knowledge_set_always(id, rel, !always, cx))
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
                            .child("KNOWLEDGE"),
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
            .child(div().flex().flex_col().gap(px(2.)).children(rows))
            .children(writer)
            .when(!self.writing, |d| {
                d.child(
                    div()
                        .flex()
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
                        ),
                )
            })
            .into_any_element()
    }
}
