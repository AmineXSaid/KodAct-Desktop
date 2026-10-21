//! The comment box of the design bar's comment tool, as Claude's design
//! canvas has it: over the part of the page you picked, it names that part,
//! takes what should change, and sends it to Forge (@Forge), or keeps it as
//! a note for you. While Forge replies, Queue for Forge holds it until the
//! reply ends ("your comment goes after that"); unticked, Forge reads it at
//! its next step.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, FontWeight, IntoElement, Render, Subscription,
    Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme};

pub struct DesignCommentBox {
    state: Entity<AppState>,
    input: Entity<InputState>,
    /// @Forge: the comment asks Forge to act; without it, it is a note.
    to_forge: bool,
    /// Queue for Forge: while it replies, the comment waits for the end.
    queue: bool,
    /// The part the box is open on, so a new pick starts a fresh comment.
    on: Option<String>,
    _subs: Vec<Subscription>,
}

impl DesignCommentBox {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What should change here?"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                // A part just picked: the box is fresh, and has the keyboard.
                let picked = state
                    .read(cx)
                    .design_pick
                    .as_ref()
                    .map(|p| p.selector.clone());
                if picked != this.on {
                    this.on = picked.clone();
                    if picked.is_some() {
                        this.input.update(cx, |i, cx| i.set_value("", window, cx));
                        // Once the box is drawn: a box not drawn yet can't
                        // take the keyboard.
                        cx.on_next_frame(window, |this, window, cx| {
                            this.input.update(cx, |i, cx| i.focus(window, cx));
                        });
                    }
                }
                cx.notify();
            }),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.submit(window, cx);
                }
            }),
        ];
        Self {
            state,
            input,
            to_forge: true,
            queue: true,
            on: None,
            _subs: subs,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let (to_forge, queue) = (self.to_forge, self.queue);
        self.state
            .update(cx, |s, cx| s.design_comment(text, to_forge, queue, cx));
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
    }
}

impl Render for DesignCommentBox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Some(part) = s.design_pick.clone() else {
            return div().into_any_element();
        };
        let busy = s.busy();
        let ready = !self.input.read(cx).value().trim().is_empty();
        let (to_forge, queue) = (self.to_forge, self.queue);
        let hover = t.colors.hover;

        // @Forge: on, the comment is a request; off, a note for you.
        let mention = div()
            .id("design-comment-forge")
            .flex()
            .items_center()
            .gap(t.space.xs)
            .h(t.size.control)
            .px(t.space.sm)
            .rounded(t.radius.button)
            .cursor_pointer()
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .map(|d| {
                if to_forge {
                    d.bg(t.colors.primary_tint).text_color(t.colors.primary)
                } else {
                    d.bg(t.extra.inset)
                        .text_color(t.colors.text_muted)
                        .hover(move |s| s.bg(hover))
                }
            })
            .child(icon(
                "icons/at-sign.svg",
                t.size.icon_xs,
                if to_forge {
                    t.colors.primary
                } else {
                    t.colors.text_muted
                },
            ))
            .child("Forge")
            .tooltip(|window, cx| {
                gpui::component::tooltip::Tooltip::new(
                    "With @Forge, Forge revises the part; without, the comment is a note for you",
                )
                .build(window, cx)
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.to_forge = !this.to_forge;
                cx.notify();
            }));

        // Queue for Forge: shown while Forge replies.
        let queue_box = (busy && to_forge).then(|| {
            div()
                .id("design-comment-queue")
                .flex()
                .items_center()
                .gap(t.space.xs)
                .cursor_pointer()
                .text_size(t.text.caption)
                .text_color(t.colors.text)
                .child(icon(
                    if queue {
                        "icons/square-check.svg"
                    } else {
                        "icons/square.svg"
                    },
                    t.size.icon_sm,
                    if queue {
                        t.colors.primary
                    } else {
                        t.colors.text_muted
                    },
                ))
                .child("Queue for Forge")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.queue = !this.queue;
                    cx.notify();
                }))
        });

        let say = if busy && to_forge {
            if queue {
                "Forge is working. Your comment goes after that."
            } else {
                "Forge is working. It reads your comment at its next step."
            }
        } else if to_forge {
            "Comment on any part, and Forge can revise it."
        } else {
            "A note for you: Forge doesn't see it."
        };

        let send = super::button::send_button(
            "design-comment-send",
            if ready {
                super::button::SendState::Ready
            } else {
                super::button::SendState::Empty
            },
            &t,
            cx.listener(|this, _, window, cx| this.submit(window, cx)),
        );

        theme::popover(&t)
            .id("design-comment")
            .occlude()
            // Esc closes the comment; the tool stays on for the next one.
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.state.update(cx, |s, cx| s.design_cancel_pick(cx));
                }
            }))
            .w(px(300.))
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .p(t.space.md)
            // What it's on.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(icon(
                        "icons/message-square.svg",
                        t.size.icon_sm,
                        t.colors.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_muted)
                            .child(part.label()),
                    )
                    .child(
                        div()
                            .id("design-comment-close")
                            .flex_none()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.small)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .child(icon("icons/x.svg", t.size.icon_xs, t.colors.text_subtle))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |s, cx| s.design_cancel_pick(cx))
                            })),
                    ),
            )
            .child(
                div()
                    .px(t.space.sm)
                    .py(t.space.xs)
                    .rounded(t.radius.button)
                    .bg(t.extra.inset)
                    .child(
                        Input::new(&self.input)
                            .appearance(false)
                            .text_size(t.text.body_sm),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(mention)
                    .child(div().flex_1())
                    .children(queue_box)
                    .child(send),
            )
            .child(
                div()
                    .text_size(t.text.caption)
                    .line_height(relative(1.4))
                    .text_color(t.colors.text_subtle)
                    .child(say),
            )
            .into_any_element()
    }
}
