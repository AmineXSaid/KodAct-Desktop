//! The Memory card on a project page's rail (`crate::project_memory`): what
//! Forge has learned working in the project, by topic, each fact a hover
//! away from being forgotten; the whole file editable in place; a switch to
//! stop keeping it. Plain Markdown in `.forge/memory.md`, nothing hidden.

use gpui::component::input::TextareaState;
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::editor_kit as kit;

use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme};

pub struct ProjectMemory {
    state: Entity<AppState>,
    editor: Entity<TextareaState>,
    editing: bool,
    /// "Forget everything" was clicked once: the next click does it.
    confirm_clear: bool,
    _subs: Vec<Subscription>,
}

impl ProjectMemory {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _subs: vec![cx.observe(&state, |_, _, cx| cx.notify())],
            state,
            editor: cx.new(|cx| {
                TextareaState::new(window, cx).placeholder("# Memory\n\n## Decisions\n- …")
            }),
            editing: false,
            confirm_clear: false,
        }
    }
}

impl Render for ProjectMemory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Screen::Project(id) = s.screen else {
            return div().into_any_element();
        };
        let Some(on) = s.project(id).map(|p| p.memory_on) else {
            return div().into_any_element();
        };
        let Some(memory) = s
            .project_brief
            .as_ref()
            .filter(|(b, _)| *b == id)
            .map(|(_, b)| b.memory.clone())
        else {
            return div().into_any_element();
        };
        let topics = crate::project_memory::topics(&memory);
        let count: usize = topics.iter().map(|(_, f)| f.len()).sum();
        let hover = t.colors.hover;
        let toggler = self.state.clone();

        let header = div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(crate::theme::section_label(&t, "Memory")),
            )
            .when(count > 0, |d| {
                d.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(format!("{count} fact{}", if count == 1 { "" } else { "s" })),
                )
            })
            .child(
                super::button::Switch::new("memory-on", on, "Keep a memory").on_toggle(
                    move |v, _, cx| toggler.update(cx, |s, cx| s.set_project_memory(id, v, cx)),
                ),
            );

        let body: AnyElement = if self.editing {
            div()
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(kit::area(&t, &self.editor, true))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(t.space.sm)
                        .child(
                            theme::text_button(&t, "Cancel")
                                .id("memory-cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.editing = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            theme::primary_button(&t, "Save")
                                .id("memory-save")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let text = this.editor.read(cx).value().to_string();
                                    this.editing = false;
                                    this.state
                                        .update(cx, |s, cx| s.edit_memory(id, move |_| text, cx));
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element()
        } else if topics.is_empty() {
            div()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_subtle)
                .line_height(relative(1.4))
                .child(if on {
                    "Nothing yet. Forge writes down what it learns here (decisions, preferences, names) and you see each one as it happens, with Undo."
                } else {
                    "Off: chats here start without a memory, and Forge doesn't keep one."
                })
                .into_any_element()
        } else {
            let mut list = div().flex().flex_col().gap(t.space.sm);
            let mut n: usize = 0;
            for (topic, facts) in topics {
                let mut group = div().flex().flex_col().gap(px(2.));
                if !topic.is_empty() {
                    group = group.child(
                        div()
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_muted)
                            .child(topic),
                    );
                }
                for fact in facts {
                    let forget = self.state.clone();
                    let f = fact.clone();
                    group = group.child(
                        div()
                            .id(("memory-fact", n))
                            .group("memory-fact")
                            .flex()
                            .items_start()
                            .gap(t.space.sm)
                            .px(t.space.xs)
                            .py(px(3.))
                            .rounded(t.radius.compact)
                            .hover(move |st| st.bg(hover))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(t.text.body_sm)
                                    .text_color(if on {
                                        t.colors.text_body
                                    } else {
                                        t.colors.text_subtle
                                    })
                                    .line_height(relative(1.4))
                                    .child(fact),
                            )
                            .child(
                                div()
                                    .invisible()
                                    .group_hover("memory-fact", |d| d.visible())
                                    .child(
                                        super::button::Button::icon(
                                            SharedString::from(format!("memory-forget-{n}")),
                                            "icons/x.svg",
                                            "Forget this",
                                        )
                                        .compact()
                                        .on_click(
                                            move |_, _, cx| {
                                                let f = f.clone();
                                                forget.update(cx, |s, cx| {
                                                    s.edit_memory(
                                                        id,
                                                        move |now| {
                                                            crate::project_memory::forget(now, &f)
                                                        },
                                                        cx,
                                                    )
                                                })
                                            },
                                        ),
                                    ),
                            ),
                    );
                    n += 1;
                }
                list = list.child(group);
            }
            list.into_any_element()
        };

        let has = count > 0;
        let confirm = self.confirm_clear;
        let footer = (!self.editing).then(|| {
            let m = memory.clone();
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(
                    theme::ghost_button(&t, if has { "Edit" } else { "Write it" })
                        .id("memory-edit")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let text = m.trim_end().to_string();
                            this.editor.update(cx, |e, cx| {
                                e.set_value(text, window, cx);
                                e.focus(window, cx);
                            });
                            this.editing = true;
                            this.confirm_clear = false;
                            cx.notify();
                        })),
                )
                .child(div().flex_1())
                .when(has, |d| {
                    d.child(
                        theme::text_button(
                            &t,
                            if confirm {
                                "Click again to forget all"
                            } else {
                                "Forget everything"
                            },
                        )
                        .id("memory-clear")
                        .text_size(t.text.caption)
                        .when(confirm, |b| b.text_color(t.extra.danger))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.confirm_clear {
                                this.confirm_clear = false;
                                this.state
                                    .update(cx, |s, cx| s.edit_memory(id, |_| String::new(), cx));
                            } else {
                                this.confirm_clear = true;
                            }
                            cx.notify();
                        })),
                    )
                })
        });

        div()
            .flex()
            .flex_col()
            .gap(t.space.md)
            .p(t.space.lg)
            .rounded(t.radius.card)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .child(header)
            .child(body)
            .children(footer)
            .into_any_element()
    }
}
