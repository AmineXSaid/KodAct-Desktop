//! First run: Forge introduces itself and asks what to call you. The name
//! is kept (`profile.rs`) and shown in the sidebar. Reopened from the
//! command palette ("Change your name") with the current name filled in.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{div, prelude::*, px, relative, Context, Entity, FontWeight, IntoElement, Render, Subscription, Window};

use super::hammer::hammer;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme};

pub struct Onboarding {
    state: Entity<AppState>,
    input: Entity<InputState>,
    error: Option<&'static str>,
    _subs: Vec<Subscription>,
}

impl Onboarding {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Your name"));
        let subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.submit(window, cx),
            InputEvent::Change => {
                this.error = None;
                cx.notify();
            }
            _ => {}
        })];
        Self { state, input, error: None, _subs: subs }
    }

    /// Show it: the current name filled in, the field focused.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.read(cx).user_name.clone().unwrap_or_default();
        self.error = None;
        self.input.update(cx, |i, cx| {
            i.set_value(current, window, cx);
            i.focus(window, cx);
        });
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.input.read(cx).value().to_string();
        match self.state.update(cx, |s, cx| s.set_user_name(&name, cx)) {
            Ok(()) => window.dispatch_action(Box::new(super::FocusComposer), cx),
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }
}

impl Render for Onboarding {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let returning = self.state.read(cx).user_name.is_some();
        div().flex_1().flex().items_center().justify_center().bg(t.colors.canvas).pb(t.space.xxl * 2.).child(
            theme::card(&t)
                .w(px(440.))
                .p(t.space.xxl)
                .flex()
                .flex_col()
                .items_center()
                .gap(t.space.lg)
                .child(hammer(&t, px(64.)))
                .child(
                    div()
                        .text_size(t.text.heading_sm + px(6.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(if returning { "What should Forge call you?" } else { "Welcome to Forge" }),
                )
                .child(
                    div()
                        .text_center()
                        .font_family(t.font_serif.clone())
                        .text_size(t.text.body + px(1.))
                        .line_height(relative(1.56))
                        .text_color(t.colors.text_body)
                        .child("Your workshop for building with AI. Before we start, what should we call you?"),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(t.space.sm)
                        .pt(t.space.sm)
                        .child(
                            div()
                                .w_full()
                                .px(t.space.md)
                                .py(t.space.xs)
                                .rounded(t.radius.button)
                                .border_1()
                                .border_color(if self.error.is_some() { t.extra.danger } else { t.colors.border })
                                .bg(t.colors.surface)
                                .text_size(t.text.body)
                                .child(Input::new(&self.input).appearance(false)),
                        )
                        .children(self.error.map(|e| div().text_size(t.text.caption).text_color(t.extra.danger).child(e))),
                )
                .child(
                    theme::primary_button(&t, if returning { "Save" } else { "Continue" })
                        .id("onboarding-continue")
                        .w_full()
                        .flex()
                        .justify_center()
                        .py(t.space.sm)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                )
                .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child("You can change it any time from the command palette.")),
        )
    }
}
