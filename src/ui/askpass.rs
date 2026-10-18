//! The SSH sign-in dialog: a password or a key's passphrase that ssh asks
//! for (`askpass.rs`), typed here instead of a terminal. Masked, with an eye
//! to show it; Enter continues, Esc cancels. The answer is kept in memory for
//! this run only.

use gpui::component::input::{Input, InputState};
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, KeyDownEvent, Render, Subscription, Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme};

pub struct AskpassDialog {
    state: Entity<AppState>,
    input: Entity<InputState>,
    /// The prompt on screen: a new one starts with an empty box.
    shown: Option<String>,
    /// What had the keyboard before the dialog took it: it gets it back.
    before: Option<gpui::FocusHandle>,
    _subs: Vec<Subscription>,
}

/// The dialog's title for what ssh asked.
pub fn title(prompt: &str) -> &'static str {
    let p = prompt.to_lowercase();
    if p.contains("passphrase") {
        "Unlock your SSH key"
    } else if p.contains("password") {
        "Sign in over SSH"
    } else if p.contains("verification code") || p.contains("one-time") || p.contains("otp") {
        "Enter your verification code"
    } else {
        "SSH asks"
    }
}

impl AskpassDialog {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type it here")
                .masked(true)
        });
        let subs = vec![cx.observe_in(&state, window, |this, state, window, cx| {
            let prompt = state.read(cx).askpass.front().map(|r| r.prompt.clone());
            if prompt != this.shown {
                if this.shown.is_none() && prompt.is_some() {
                    this.before = window.focused(cx);
                }
                this.shown = prompt.clone();
                if prompt.is_some() {
                    this.input.update(cx, |i, cx| {
                        i.set_value("", window, cx);
                        i.focus(window, cx);
                    });
                }
            }
            cx.notify();
        })];
        Self {
            state,
            input,
            shown: None,
            before: None,
            _subs: subs,
        }
    }

    fn answer(&mut self, ok: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        // Clear the box at once: the secret doesn't linger in the field.
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
        self.state
            .update(cx, |s, cx| s.answer_askpass(ok.then_some(text), cx));
        if self.state.read(cx).askpass.is_empty() {
            match self.before.take() {
                Some(f) => window.focus(&f, cx),
                None => window.dispatch_action(Box::new(super::FocusComposer), cx),
            }
        }
    }
}

impl Render for AskpassDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let Some(prompt) = self
            .state
            .read(cx)
            .askpass
            .front()
            .map(|r| r.prompt.clone())
        else {
            return div().into_any_element();
        };
        let card = theme::fit_window(
            theme::popover(&t).id("askpass"),
            window,
            &t,
            t.size.viewport_margin,
            None,
        )
        .relative()
        .occlude()
        .w(px(420.))
        .p(t.space.xl)
        .flex()
        .flex_col()
        .gap(t.space.md)
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(icon("icons/key-round.svg", t.size.icon, t.colors.primary))
                .child(
                    div()
                        .text_size(t.text.subheading)
                        .font_family(t.font_display.clone())
                        .font_weight(t.display_weight)
                        .text_color(t.colors.text_strong)
                        .child(title(&prompt)),
                ),
        )
        .child(
            div()
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption)
                .text_color(t.colors.text_body)
                .child(prompt.clone()),
        )
        .child(Input::new(&self.input).mask_toggle())
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child("Kept in memory until Forge closes, never saved."),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(t.space.sm)
                .pt(t.space.sm)
                .child(
                    theme::text_button(&t, "Cancel")
                        .id("askpass-cancel")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.answer(false, window, cx)),
                        ),
                )
                .child(
                    theme::primary_button(&t, "Continue")
                        .id("askpass-ok")
                        .on_click(cx.listener(|this, _, window, cx| this.answer(true, window, cx))),
                ),
        );
        div()
            .id("askpass-scrim")
            .key_context("Askpass")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(t.extra.scrim)
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "enter" => this.answer(true, window, cx),
                    "escape" => this.answer(false, window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(super::motion::edge_draw(
                card,
                "askpass-in",
                t.colors.primary,
                0.,
                &t,
            ))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::title;

    #[test]
    fn titles_say_what_is_asked() {
        assert_eq!(title("dev@box's password:"), "Sign in over SSH");
        assert_eq!(
            title("Enter passphrase for key '/home/me/.ssh/id_ed25519':"),
            "Unlock your SSH key"
        );
        assert_eq!(title("Verification code:"), "Enter your verification code");
        assert_eq!(title("Something else:"), "SSH asks");
    }
}
