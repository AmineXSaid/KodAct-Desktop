//! First run: Forge introduces itself and asks what to call you. The name
//! is kept (`profile.rs`) and shown in the sidebar. Reopened from the
//! command palette ("Change your name") with the current name filled in.

use gpui::component::input::{Input, InputEvent, InputState};
use std::time::Duration;

use gpui::{div, prelude::*, px, relative, Animation, AnimationExt, Context, Entity, FontWeight, IntoElement, Render, Subscription, Window};

use crate::state::AppState;
use super::{icon, shortcut};
use crate::theme::ActiveTheme;

pub struct Onboarding {
    state: Entity<AppState>,
    input: Entity<InputState>,
    error: Option<&'static str>,
    /// Bumped to play the logo moment again.
    replays: usize,
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
        Self { state, input, error: None, replays: 0, _subs: subs }
    }

    /// Show it: the current name filled in, the field focused.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.read(cx).user_name.clone().unwrap_or_default();
        self.error = None;
        self.replays += 1;
        self.replays += 1;
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
        let empty = self.input.read(cx).value().trim().is_empty();
        let hover = t.colors.primary_hover;

        // The logo moment: the hammer smashes the cube (click to replay).
        let logo = div()
            .id("smash-logo")
            .cursor_pointer()
            .child(super::smash::animated(&t, self.replays))
            .on_click(cx.listener(|this, _, _, cx| {
                this.replays += 1;
                cx.notify();
            }));

        let field = div()
            .w_full()
            .h(px(52.))
            .pl(t.space.lg)
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(t.space.sm)
            .rounded(t.radius.card)
            .bg(t.colors.surface)
            .border_1()
            .border_color(if self.error.is_some() { t.extra.danger } else { t.colors.border })
            .text_size(t.text.body)
            .child(div().flex_1().child(Input::new(&self.input).appearance(false)))
            .child(
                div()
                    .id("onboarding-continue")
                    .size(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius.button)
                    .bg(if empty { t.colors.hover } else { t.colors.primary })
                    .when(!empty, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                    .child(icon("icons/arrow-right.svg", t.size.icon, if empty { t.colors.text_subtle } else { t.colors.primary_fg }))
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );

        div().flex_1().flex().items_center().justify_center().bg(t.colors.canvas).pb(t.space.xxl * 2.).child(
            div()
                .w(px(480.))
                .flex()
                .flex_col()
                .items_center()
                .child(logo)
                .child(
                    div()
                        .mt(t.space.xl + t.space.sm)
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.primary)
                        .child(if returning { "YOUR NAME" } else { "WELCOME TO FORGE" }),
                )
                .child(
                    div()
                        .mt(t.space.sm)
                        .text_center()
                        .font_family(t.font_serif.clone())
                        .text_size(t.size.display - px(8.))
                        .line_height(relative(1.15))
                        .text_color(t.colors.text_strong)
                        .child(if returning { "What should we call you now?" } else { "Hello. What should we call you?" }),
                )
                .child(
                    div()
                        .mt(t.space.md)
                        .text_center()
                        .text_size(t.text.body_sm)
                        .line_height(relative(1.55))
                        .text_color(t.colors.text_muted)
                        .child("Forge greets you by name and shows it in the sidebar. It stays on this computer."),
                )
                .child(div().mt(t.space.xl).w(px(420.)).child(field))
                .child(
                    div()
                        .mt(t.space.sm)
                        .h(t.text.caption * 1.6)
                        .w(px(420.))
                        .pl(t.space.lg)
                        .text_size(t.text.caption)
                        .text_color(t.extra.danger)
                        .children(self.error),
                )
                .child(
                    div()
                        .mt(t.space.md)
                        .flex()
                        .items_center()
                        .gap(t.space.xs)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child("Press")
                        .child(kbd_chip(&t, "Enter"))
                        .child("to continue · change it later with")
                        .child(kbd_chip(&t, shortcut("K"))),
                )
                .with_animation("onboarding-in", Animation::new(Duration::from_millis(t.motion_ms * 2)).with_easing(gpui::ease_out_quint()), |d, delta| {
                    d.opacity(delta).mt(px(10. * (1. - delta)))
                }),
        )
    }
}

/// A key, drawn as a small chip.
fn kbd_chip(t: &crate::theme::Theme, label: impl Into<gpui::SharedString>) -> impl IntoElement {
    div()
        .px(t.space.xs + px(2.))
        .rounded(t.radius.small)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .child(label.into())
}
