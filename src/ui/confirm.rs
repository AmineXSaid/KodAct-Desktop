//! "Delete this chat?": a small window over a scrim. Enter deletes, Esc or
//! a click outside cancels. It takes the keyboard when it opens.

use gpui::{div, prelude::*, px, AnimationExt, Context, Entity, FocusHandle, FontWeight, IntoElement, KeyDownEvent, Render, Subscription, Window};

use crate::state::AppState;
use crate::theme::{self, ActiveTheme};

pub struct ConfirmDelete {
    state: Entity<AppState>,
    focus: FocusHandle,
    shown: Option<usize>,
    _subs: Vec<Subscription>,
}

impl ConfirmDelete {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![cx.observe_in(&state, window, |this, state, window, cx| {
            let asking = state.read(cx).confirm_delete;
            if asking.is_some() && asking != this.shown {
                window.focus(&this.focus, cx);
            }
            this.shown = asking;
            cx.notify();
        })];
        Self { state, focus: cx.focus_handle(), shown: None, _subs: subs }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.confirm_delete = None;
            cx.notify();
        });
        window.dispatch_action(Box::new(super::FocusComposer), cx);
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.state.read(cx).confirm_delete {
            self.state.update(cx, |s, cx| s.delete_chat(id, cx));
        }
        window.dispatch_action(Box::new(super::FocusComposer), cx);
    }
}

impl Render for ConfirmDelete {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Some(chat) = s.confirm_delete.and_then(|id| s.chats.iter().find(|c| c.id == id)) else { return div().into_any_element() };
        let title = chat.title.clone();
        let running = s.status(chat) == crate::state::ChatStatus::Working;
        div()
            .id("confirm-scrim")
            .track_focus(&self.focus)
            .key_context("ConfirmDelete")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(t.extra.scrim)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "escape" => this.cancel(window, cx),
                    "enter" => this.confirm(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| this.cancel(window, cx)))
            .child(
                theme::popover(&t)
                    .id("confirm")
                    .occlude()
                    .w(px(400.))
                    .p(t.space.xl)
                    .flex()
                    .flex_col()
                    .gap(t.space.md)
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(div().text_size(t.text.subheading).font_family(t.font_display.clone()).text_color(t.colors.text_strong).child("Delete this chat?"))
                    .child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text).overflow_hidden().whitespace_nowrap().text_ellipsis().child(title))
                    .child(div().text_color(t.colors.text_muted).line_height(gpui::relative(1.45)).child(if running {
                        "Its reply is still running and will be stopped. The chat and its side questions leave Forge; this can't be undone."
                    } else {
                        "The chat and its side questions leave Forge. Files Forge changed in your project stay as they are. This can't be undone."
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(t.space.sm)
                            .pt(t.space.sm)
                            .child(theme::text_button(&t, "Cancel").id("confirm-cancel").on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))))
                            .child(
                                theme::danger_button(&t, "Delete")
                                    .id("confirm-delete")
                                    .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                            ),
                    )
                    .with_animation("confirm-in", gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms)).with_easing(gpui::ease_in_out), |d, delta| d.opacity(delta)),
            )
            .into_any_element()
    }
}
