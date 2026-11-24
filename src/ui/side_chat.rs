//! `/btw`: the side-question card. A floating card over the chat, as KodAct
//! for VS Code has it: questions answered from this conversation's context,
//! without tools, while the conversation goes on. It grows for a long answer,
//! folds to its title bar and closes with Esc; the thread keeps the last 20
//! exchanges.

use gpui::component::input::{InputEvent, Textarea, TextareaState};
use gpui::{
    div, prelude::*, px, AnimationExt as _, Context, Entity, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::*;
use crate::state::{AppState, SideItem, SIDE_EMPTY, SIDE_PENDING, SIDE_PLACEHOLDER, SIDE_TITLE};
use crate::theme::ActiveTheme;

/// The card's width and its thread's height, at rest and grown; the least
/// they shrink to in a small window.
const SIDE_W: f32 = 360.;
const SIDE_H: f32 = 300.;
/// Grown: 520 wide and 60% of the window tall.
const SIDE_W_GROWN: f32 = 520.;
const SIDE_H_SHARE: f32 = 0.6;
/// The card's head (36) and the rule and field under the thread.
const SIDE_HEAD: f32 = 36.;
const SIDE_W_MIN: f32 = 240.;
const SIDE_H_MIN: f32 = 120.;
/// Where it sits: in from the right, above the composer's row; and what of
/// the window's height isn't thread (title bars, its own head and field).
const SIDE_EDGE: f32 = 20.;
const SIDE_BOTTOM: f32 = 150.;
const SIDE_CHROME: f32 = 190.;
/// The rule and the field under the thread.
const SIDE_FOOT: f32 = 60.;

pub struct SideChat {
    state: Entity<AppState>,
    input: Entity<TextareaState>,
    /// The card, kept while it leaves.
    presence: super::motion::Presence<crate::state::Side>,
    /// Its body, kept while it folds to the title bar.
    fold: super::motion::Presence<()>,
    /// Folded last frame, and opening out of that now.
    was_folded: bool,
    unfolding: bool,
    /// Its width and thread height last frame and before the last change,
    /// and how many changes there were: each one eases (`motion::resize`).
    sized: Option<(f32, f32)>,
    sized_from: (f32, f32),
    turns: usize,
    sized_at: std::time::Instant,
    folds: usize,
    _subs: Vec<Subscription>,
}

impl SideChat {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // A field that grows to four lines, Enter sends, one border: the
        // card's own field, the placeholder shown whole.
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 4)
                .submit_on_enter(true)
                .placeholder(SIDE_PLACEHOLDER)
        });
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                if state.read(cx).side_focus {
                    state.update(cx, |s, _| s.side_focus = false);
                    this.input.update(cx, |i, cx| i.focus(window, cx));
                }
                cx.notify();
            }),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { shift: false, .. }) {
                    this.send(window, cx);
                }
            }),
        ];
        Self {
            state,
            input,
            presence: Default::default(),
            fold: Default::default(),
            was_folded: false,
            unfolding: false,
            sized: None,
            sized_from: (SIDE_W, SIDE_H),
            turns: 0,
            sized_at: std::time::Instant::now(),
            folds: 0,
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
        let copied = self.state.read(cx).copied.clone();
        // Its size: a card, or grown for a long answer; never past the window.
        let view = window.viewport_size();
        let (want_w, want_h) = if side.expanded {
            (
                SIDE_W_GROWN,
                f32::from(view.height) * SIDE_H_SHARE - SIDE_HEAD - SIDE_FOOT,
            )
        } else {
            (SIDE_W, SIDE_H)
        };
        let to_w = want_w.min((f32::from(view.width) - 2. * SIDE_EDGE).max(SIDE_W_MIN));
        let to_h = want_h.min((f32::from(view.height) - SIDE_BOTTOM - SIDE_CHROME).max(SIDE_H_MIN));
        // Growing or shrinking eases from the size it had.
        if self.sized != Some((to_w, to_h)) {
            self.sized_from = self.sized.unwrap_or((to_w, to_h));
            self.sized = Some((to_w, to_h));
            self.sized_at = std::time::Instant::now();
            self.turns += 1;
        } else if self.sized_at.elapsed()
            > std::time::Duration::from_millis(super::motion::played(t.size.panel_in_ms))
        {
            // Settled: drawn afresh (unfolded, reopened), it starts there.
            self.sized_from = (to_w, to_h);
        }
        let (from_w, from_h) = self.sized_from;
        let turns = self.turns;
        // Folding keeps the body for the beat it takes to close; opening out
        // of the fold unrolls it (a card just summoned arrives whole).
        if !leaving {
            if self.was_folded != side.minimized {
                self.unfolding = self.was_folded;
                self.folds += 1;
            }
            self.was_folded = side.minimized;
        }
        let (unfolding, folds) = (self.unfolding, self.folds);
        let body_f = self
            .fold
            .frame((!side.minimized).then_some(()), t.size.panel_in_ms, window);
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
            .h(px(SIDE_HEAD))
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
            .max_h(px(to_h))
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
                SideItem::Answer { question, answer } => {
                    let key: SharedString = format!("side-answer-{}", answer.len()).into();
                    let (copy_state, text) = (state.clone(), answer.clone());
                    let did = copied.as_ref() == Some(&key);
                    let key2 = key.clone();
                    exchange(
                        question.clone(),
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(super::markdown::render(
                                answer,
                                &t,
                                &super::markdown::CopyContext {
                                    streaming: false,
                                    state: state.clone(),
                                    key: key.to_string(),
                                    copied: copied.clone(),
                                    scale: window.scale_factor(),
                                    sources: Default::default(),
                                    zooms: Default::default(),
                                },
                            ))
                            .child(
                                div().flex().child(
                                    super::button::Button::icon(
                                        SharedString::from(format!("side-copy-{}", key.as_ref())),
                                        if did {
                                            "icons/check.svg"
                                        } else {
                                            "icons/copy.svg"
                                        },
                                        "Copy the answer",
                                    )
                                    .compact()
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            text.to_string(),
                                        ));
                                        copy_state
                                            .update(cx, |s, cx| s.mark_copied(key2.clone(), cx));
                                    }),
                                ),
                            )
                            .into_any_element(),
                    )
                }
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
            .items_end()
            .gap(px(4.))
            .mx(px(10.))
            .mb(px(10.))
            .pr(px(5.))
            .pb(px(5.))
            .min_h(t.size.field_h)
            .rounded(t.radius.field)
            .bg(t.colors.canvas)
            .border_1()
            .border_color(if focused {
                t.extra.border_strong
            } else {
                t.colors.border
            })
            .child(
                div().flex_1().min_w_0().child(
                    Textarea::new(&self.input)
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
                            // The composer's Send, smaller.
                            let flat = t.extra.accent_fill.opacity(0.9);
                            let face = t.button_face(flat);
                            d.bg(t.button_face(t.extra.accent_fill))
                                .shadow(t.elevation_button())
                                .cursor_pointer()
                                .hover(move |st| st.bg(face))
                                .active(move |st| st.bg(flat).shadow(vec![]))
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
            .w(px(to_w))
            .flex()
            .flex_col()
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .rounded(t.radius.card)
            .shadow(t.elevation_popover())
            .overflow_hidden()
            // Esc closes the card while it has the keyboard, and goes no
            // further: the window's Esc is Stop (`StopReply`), which would
            // decline a permission that is waiting. The field's own Escape
            // action is taken here too.
            .capture_action(cx.listener(|this, _: &super::StopReply, _, cx| {
                this.state.update(cx, |s, cx| s.close_side(cx));
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(
                |this, _: &gpui::component::input::Escape, _, cx| {
                    this.state.update(cx, |s, cx| s.close_side(cx));
                    cx.stop_propagation();
                },
            ))
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    this.state.update(cx, |s, cx| s.close_side(cx));
                    cx.stop_propagation();
                }
            }))
            .child(head)
            .children(body_f.map(|(_, folding)| {
                let thread = thread.with_animation(
                    ("side-thread", turns),
                    super::motion::resize(&t),
                    move |d, k| d.max_h(px(from_h + (to_h - from_h) * k)),
                );
                let body = div()
                    .flex()
                    .flex_col()
                    .child(crate::theme::rule(&t))
                    .child(thread)
                    .child(input_row);
                // The most it can be: the thread, its rule and its field.
                let full = to_h + SIDE_FOOT;
                if folding {
                    body.overflow_hidden()
                        .with_animation(
                            ("side-fold", folds),
                            super::motion::resize(&t),
                            move |d, k| d.max_h(px(full * (1. - k))).opacity(1. - k),
                        )
                        .into_any_element()
                } else if unfolding {
                    body.with_animation(
                        ("side-unfold", folds),
                        super::motion::resize(&t),
                        move |d, k| {
                            d.when(k < 1., |d| d.overflow_hidden().max_h(px(full * k)))
                                .opacity(k)
                        },
                    )
                    .into_any_element()
                } else {
                    body.into_any_element()
                }
            }))
            .with_animation(
                ("side-size", turns),
                super::motion::resize(&t),
                move |d, k| d.w(px(from_w + (to_w - from_w) * k)),
            );
        let card = div().child(card);

        // Bottom right, above the composer's row.
        div()
            .absolute()
            .right(px(SIDE_EDGE))
            .bottom(px(SIDE_BOTTOM))
            .child(super::motion::notice(card, "side-chat", leaving, &t))
            .into_any_element()
    }
}
