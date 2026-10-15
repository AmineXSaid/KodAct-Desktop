//! Find in the conversation (Ctrl/⌘F): a bar over the transcript with the
//! query, "3 of 12", and up / down. Enter goes to the next match, Shift+Enter
//! to the previous, Esc closes. The list scrolls each match into view and
//! marks it (`chat_view.rs`).

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, KeyDownEvent, Render, Subscription, Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme};

pub struct FindBar {
    state: Entity<AppState>,
    query: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl FindBar {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find in this chat"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &query,
                window,
                |this, input, ev: &InputEvent, _, cx| match ev {
                    InputEvent::Change => {
                        let q = input.read(cx).value().to_string();
                        this.state.update(cx, |s, cx| s.set_find_query(&q, cx));
                    }
                    InputEvent::PressEnter { shift, .. } => {
                        let forward = !*shift;
                        this.state.update(cx, |s, cx| s.find_step(forward, cx));
                    }
                    _ => {}
                },
            ),
        ];
        Self {
            state,
            query,
            _subs: subs,
        }
    }

    /// Show the bar and put the cursor in it, the last query selected for typing over.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| s.open_find(cx));
        let q = self.query.read(cx).value().to_string();
        if !q.is_empty() {
            // Reopened: run the last query again on the conversation as it is now.
            self.state.update(cx, |s, cx| s.set_find_query(&q, cx));
        }
        self.query.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| s.close_find(cx));
        window.dispatch_action(Box::new(super::FocusComposer), cx);
    }
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let Some(find) = self.state.read(cx).find.clone() else {
            return div().into_any_element();
        };
        let hover = t.colors.hover;
        let n = find.hits.len();
        let typed = !find.query.trim().is_empty();
        let count = match (typed, n) {
            (false, _) => String::new(),
            (true, 0) => "No matches".into(),
            (true, n) => format!("{} of {n}", find.at.min(n - 1) + 1),
        };
        let button = |id: &'static str, path: &'static str, enabled: bool| {
            div()
                .id(id)
                .size(t.size.control - px(4.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                .child(icon(
                    path,
                    t.size.icon_sm,
                    if enabled {
                        t.colors.text_muted
                    } else {
                        t.colors.text_subtle.opacity(0.5)
                    },
                ))
        };
        let (up, down) = (self.state.clone(), self.state.clone());
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(t.space.xl)
            .pt(t.space.sm)
            .child(
                theme::popover(&t)
                    .key_context("FindBar")
                    .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                        if ev.keystroke.key == "escape" {
                            this.close(window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .w_full()
                    .max_w(t.size.composer_width)
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .px(t.space.sm)
                    .py(px(2.))
                    .child(icon(
                        "icons/search.svg",
                        t.size.icon_sm,
                        t.colors.text_subtle,
                    ))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.query)
                                .appearance(false)
                                .text_size(t.text.body_sm),
                        ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(if typed && n == 0 {
                                t.extra.danger
                            } else {
                                t.colors.text_subtle
                            })
                            .child(count),
                    )
                    .child(crate::theme::vrule(&t).h(t.space.lg).mx(t.space.xs))
                    .child(
                        button("find-prev", "icons/chevron-up.svg", n > 0).on_click(
                            move |_, _, cx| up.update(cx, |s, cx| s.find_step(false, cx)),
                        ),
                    )
                    .child(
                        button("find-next", "icons/chevron-down.svg", n > 0).on_click(
                            move |_, _, cx| down.update(cx, |s, cx| s.find_step(true, cx)),
                        ),
                    )
                    .child(
                        button("find-close", "icons/x.svg", true)
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    ),
            )
            .into_any_element()
    }
}
