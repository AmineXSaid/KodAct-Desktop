//! `/btw`: the side-question card. A floating card over the chat, as KodAct
//! for VS Code has it: questions answered from this conversation's context,
//! without tools, while the conversation goes on. It grows for a long answer,
//! folds to its title bar and closes with Esc; the thread keeps the last 20
//! exchanges.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, Context, Entity, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window,
};

use super::*;
use crate::state::{AppState, SideItem, SIDE_EMPTY, SIDE_PENDING, SIDE_PLACEHOLDER, SIDE_TITLE};
use crate::theme::ActiveTheme;

/// The card's width and its thread's height, at rest and grown; the least
/// they shrink to in a small window.
const SIDE_W: f32 = 360.;
const SIDE_H: f32 = 300.;
const SIDE_W_GROWN: f32 = 560.;
const SIDE_H_GROWN: f32 = 560.;
const SIDE_W_MIN: f32 = 240.;
const SIDE_H_MIN: f32 = 120.;
/// Where it sits: in from the right, above the composer's row; and what of
/// the window's height isn't thread (title bars, its own head and field).
const SIDE_EDGE: f32 = 20.;
const SIDE_BOTTOM: f32 = 150.;
const SIDE_CHROME: f32 = 190.;

pub struct SideChat {
    state: Entity<AppState>,
    input: Entity<InputState>,
    /// The card, kept while it leaves.
    presence: super::motion::Presence<crate::state::Side>,
    _subs: Vec<Subscription>,
}

impl SideChat {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(SIDE_PLACEHOLDER));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                if state.read(cx).side_focus {
                    state.update(cx, |s, _| s.side_focus = false);
                    this.input.update(cx, |i, cx| i.focus(window, cx));
                }
                cx.notify();
            }),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.send(window, cx);
                }
            }),
        ];
        Self {
            state,
            input,
            presence: Default::default(),
            _subs: subs,
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let q = self.input.read(cx).value().trim().to_string();
        if q.is_empty() {
            return;
        }
        self.state.update(cx, |s, cx| s.ask_side(q, cx));
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
    }
}

impl Render for SideChat {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        // Closed: it stays a beat while it drops away, as a notice does.
        let now = self.state.read(cx).side_shown().cloned();
        let Some((side, leaving)) = self.presence.frame(now, t.size.window_out_ms, window) else {
            return div().into_any_element();
        };
        let hover = t.colors.hover;
        let state = self.state.clone();
        // Its size: a card, or grown for a long answer; never past the window.
        let view = window.viewport_size();
        let (want_w, want_h) = if side.expanded {
            (SIDE_W_GROWN, SIDE_H_GROWN)
        } else {
            (SIDE_W, SIDE_H)
        };
        let card_w = px(want_w.min((f32::from(view.width) - 2. * SIDE_EDGE).max(SIDE_W_MIN)));
        let thread_h =
            px(want_h.min((f32::from(view.height) - SIDE_BOTTOM - SIDE_CHROME).max(SIDE_H_MIN)));
        let head_button = |id: &'static str, path: &'static str, tip: &'static str| {
            div()
                .id(id)
                .size(t.size.control)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover(move |st| st.bg(hover))
                .child(icon(path, t.size.icon_sm, t.colors.text_muted))
                .tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tip).build(window, cx)
                })
        };
        let answered = side
            .thread
            .iter()
            .filter(|i| matches!(i, SideItem::Answer { .. }))
            .count();
        let status: Option<SharedString> = if !side.minimized {
            None
        } else if !side.pending.is_empty() {
            Some(SIDE_PENDING.into())
        } else if answered > 0 {
            Some(
                format!(
                    "{answered} {}",
                    if answered == 1 { "answer" } else { "answers" }
                )
                .into(),
            )
        } else {
            None
        };

        let head = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(36.))
            .pl(px(12.))
            .pr(px(6.))
            .child(icon(
                "icons/message-square.svg",
                t.size.icon_sm,
                t.colors.primary,
            ))
            .child(
                div()
                    .text_size(t.text.body_sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_strong)
                    .child(SIDE_TITLE),
            )
            .children(status.map(|s| {
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(s)
            }))
            .child(div().flex_1())
            .when(!side.thread.is_empty() && !side.minimized, |d| {
                let state = state.clone();
                d.child(
                    head_button("side-clear", "icons/trash.svg", "Clear side questions")
                        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.clear_side(cx))),
                )
            })
            .child({
                let state = state.clone();
                let grown = side.expanded && !side.minimized;
                head_button(
                    "side-grow",
                    if grown {
                        "icons/minimize-2.svg"
                    } else {
                        "icons/maximize-2.svg"
                    },
                    if grown { "Smaller" } else { "Larger" },
                )
                .on_click(move |_, _, cx| state.update(cx, |s, cx| s.toggle_side_expand(cx)))
            })
            .child({
                let state = state.clone();
                head_button(
                    "side-min",
                    if side.minimized {
                        "icons/chevron-up.svg"
                    } else {
                        "icons/minus.svg"
                    },
                    if side.minimized { "Expand" } else { "Minimise" },
                )
                .on_click(move |_, _, cx| state.update(cx, |s, cx| s.toggle_side_min(cx)))
            })
            .child({
                let state = state.clone();
                head_button("side-close", "icons/x.svg", "Close (Esc)")
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| s.close_side(cx)))
            });

        let exchange = |question: SharedString, body: gpui::AnyElement| {
            // The question as the chat sets yours: a bubble, to the right.
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .self_end()
                        .max_w(gpui::relative(0.86))
                        .px(px(10.))
                        .py(px(5.))
                        .rounded(t.radius.card)
                        .bg(t.colors.hover)
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_strong)
                        .line_height(gpui::relative(1.4))
                        .child(question),
                )
                .child(body)
        };
        let mut thread = div()
            .id("side-thread")
            .flex()
            .flex_col()
            .gap(px(14.))
            .max_h(thread_h)
            .overflow_y_scroll()
            .px(px(14.))
            .py(px(12.));
        if side.thread.is_empty() && side.pending.is_empty() {
            thread = thread.child(
                div()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_subtle)
                    .line_height(gpui::relative(1.45))
                    .child(SIDE_EMPTY),
            );
        }
        for item in &side.thread {
            thread = thread.child(match item {
                SideItem::Answer { question, answer } => exchange(
                    question.clone(),
                    div()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text)
                        .line_height(gpui::relative(1.5))
                        .child(answer.clone())
                        .into_any_element(),
                ),
                SideItem::Failed { question, message } => exchange(
                    question.clone(),
                    div()
                        .text_size(t.text.body_sm)
                        .text_color(t.extra.danger)
                        .child(message.clone())
                        .into_any_element(),
                ),
            });
        }
        for q in &side.pending {
            thread = thread.child(exchange(
                q.clone(),
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_subtle)
                    .child(icon(
                        "icons/loader-circle.svg",
                        t.size.icon_xs,
                        t.colors.text_subtle,
                    ))
                    .child(SIDE_PENDING)
                    .into_any_element(),
            ));
        }

        // The field: the composer's, smaller. Its edge darkens with focus,
        // and Send is filled only once there is something to send.
        let focused = gpui::Focusable::focus_handle(self.input.read(cx), cx).is_focused(window);
        let typed = !self.input.read(cx).value().trim().is_empty();
        let input_row = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .mx(px(10.))
            .mb(px(10.))
            .pr(px(5.))
            .h(t.size.field_h)
            .rounded(t.radius.card)
            .bg(t.colors.canvas)
            .border_1()
            .border_color(if focused {
                t.extra.border_strong
            } else {
                t.colors.border
            })
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.input)
                        .appearance(false)
                        .text_size(t.text.body_sm),
                ),
            )
            .child(
                div()
                    .id("side-send")
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .map(|d| {
                        if typed {
                            d.bg(t.extra.accent)
                                .cursor_pointer()
                                .hover(|st| st.opacity(0.9))
                        } else {
                            d.bg(t.colors.hover)
                        }
                    })
                    .child(icon(
                        "icons/arrow-up.svg",
                        t.size.icon_sm,
                        if typed {
                            t.extra.accent_fg
                        } else {
                            t.colors.text_subtle
                        },
                    ))
                    .tooltip(|window, cx| {
                        gpui::component::tooltip::Tooltip::new("Send side question")
                            .build(window, cx)
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
            );

        let card = div()
            .id("side-chat")
            .occlude()
            .w(card_w)
            .flex()
            .flex_col()
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .rounded(t.radius.card)
            .shadow(t.elevation_popover())
            .overflow_hidden()
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    this.state.update(cx, |s, cx| s.close_side(cx));
                    cx.stop_propagation();
                }
            }))
            .child(head)
            .when(!side.minimized, |d| {
                d.child(crate::theme::rule(&t))
                    .child(thread)
                    .child(input_row)
            });

        // Bottom right, above the composer's row.
        div()
            .absolute()
            .right(px(SIDE_EDGE))
            .bottom(px(SIDE_BOTTOM))
            .child(super::motion::notice(card, "side-chat", leaving, &t))
            .into_any_element()
    }
}
