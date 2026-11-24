//! The confirm window: "Delete this chat?", or "Revert these files?" with
//! the files KodAct CLI's dry run named, or "Allow Bypass permissions?". A small window over a scrim; Enter
//! confirms, Esc or a click outside cancels. It takes the keyboard when it
//! opens.

use gpui::{
    div, prelude::*, px, Context, Entity, FocusHandle, FontWeight, IntoElement, KeyDownEvent,
    Render, Subscription, Window,
};

use crate::state::{AppState, Confirm};
use crate::theme::{self, ActiveTheme};

pub struct ConfirmWindow {
    state: Entity<AppState>,
    focus: FocusHandle,
    shown: Option<Confirm>,
    /// Kept while it sinks away after an answer.
    presence: super::motion::Presence<Confirm>,
    _subs: Vec<Subscription>,
}

impl ConfirmWindow {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![cx.observe_in(&state, window, |this, state, window, cx| {
            let asking = state.read(cx).confirm.clone();
            if asking.is_some() && asking != this.shown {
                window.focus(&this.focus, cx);
            }
            this.shown = asking;
            cx.notify();
        })];
        Self {
            state,
            focus: cx.focus_handle(),
            shown: None,
            presence: Default::default(),
            _subs: subs,
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.confirm = None;
            cx.notify();
        });
        window.dispatch_action(Box::new(super::FocusComposer), cx);
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.state.read(cx).confirm.clone() {
            Some(Confirm::Delete(id)) => self.state.update(cx, |s, cx| s.delete_chat(id, cx)),
            Some(Confirm::Revert { chat, msg, .. }) => {
                self.state.update(cx, |s, cx| s.revert(chat, msg, cx))
            }
            Some(Confirm::Bypass) => self.state.update(cx, |s, cx| {
                s.bypass_allowed = true;
                s.confirm = None;
                cx.notify();
            }),
            None => {}
        }
        window.dispatch_action(Box::new(super::FocusComposer), cx);
    }
}

impl Render for ConfirmWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        // It owns the keys while it asks, even when the control that asked
        // (a switch in Settings) took focus on its own click afterwards.
        if self.state.read(cx).confirm.is_some() && !self.focus.contains_focused(window, cx) {
            window.focus(&self.focus, cx);
        }
        let s = self.state.read(cx);
        // Answered or dismissed: it stays a beat while it sinks away.
        let Some((asked, leaving)) =
            self.presence
                .frame(s.confirm.clone(), t.size.window_out_ms, window)
        else {
            return div().into_any_element();
        };
        let home = crate::rules::home_dir();
        let (title, subject, body, files, action): (
            &str,
            gpui::SharedString,
            &str,
            Vec<String>,
            &str,
        ) = match &asked {
            Confirm::Delete(id) => {
                let Some(chat) = s.chats.iter().find(|c| c.id == *id) else {
                    return div().into_any_element();
                };
                let running = s.status(chat) == crate::state::ChatStatus::Working;
                let body = if running {
                    "Its reply is still running and will be stopped. The chat and its side questions leave KodAct; this can't be undone."
                } else {
                    "The chat and its side questions leave KodAct. Files KodAct changed in your project stay as they are. This can't be undone."
                };
                (
                    "Delete this chat?",
                    chat.title.clone(),
                    body,
                    vec![],
                    "Delete",
                )
            }
            Confirm::Revert { chat, files, .. } => {
                // Paths read from the chat's project, as the tool rows show them.
                let root = s
                    .chats
                    .iter()
                    .find(|c| c.id == *chat)
                    .and_then(|c| s.project(c.project))
                    .map(|p| p.path.clone())
                    .unwrap_or_else(|| s.project_dir.clone());
                let shown = files
                    .iter()
                    .map(|f| {
                        let p = std::path::Path::new(f);
                        match p.strip_prefix(&root) {
                            Ok(rest) => rest.to_string_lossy().replace('\\', "/"),
                            Err(_) => crate::rules::display_path(p, home.as_deref()),
                        }
                    })
                    .collect();
                let one = files.len() == 1;
                (
                "Revert these files?",
                format!("{} {} back to how {} before this message", files.len(), if one { "file goes" } else { "files go" }, if one { "it was" } else { "they were" }).into(),
                "Later changes to them, from this turn on, are undone too. Files KodAct created are removed. The conversation stays as it is.",
                shown,
                "Revert",
                )
            }
            Confirm::Bypass => (
                "Allow Bypass permissions?",
                "KodAct runs every tool without asking you first".into(),
                "Edits, deletions, commands and web requests all go ahead with no answer card, in any project. Use it only in a throwaway folder or a sandbox. Once allowed, Bypass is offered in each chat's mode menu and by Shift+Tab; no chat changes mode until you pick it.",
                vec![],
                "Allow Bypass",
            ),
        };
        let action_icon = match asked {
            Confirm::Bypass => "icons/triangle-alert.svg",
            _ => "icons/trash.svg",
        };
        let mut list = div()
            .id("confirm-files")
            .flex()
            .flex_col()
            .max_h(t.size.popover_list)
            .overflow_y_scroll()
            .rounded(t.radius.button)
            .bg(t.extra.inset)
            .px(t.space.md)
            .py(t.space.sm);
        for f in &files {
            list = list.child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.caption)
                    .text_color(t.colors.text)
                    .child(f.clone()),
            );
        }
        super::motion::backdrop(
            div()
                .id("confirm-scrim")
                .track_focus(&self.focus)
                .key_context("Confirm")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(t.extra.scrim)
                // Esc arrives as the workspace's StopReply (its binding wins
                // over a key listener); Enter as a key.
                .on_action(cx.listener(|this, _: &super::StopReply, window, cx| {
                    this.cancel(window, cx);
                    cx.stop_propagation();
                }))
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    match ev.keystroke.key.as_str() {
                        "escape" => this.cancel(window, cx),
                        "enter" => this.confirm(window, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                )
                .child(super::motion::alert(
                    super::panel_chrome::window(
                        &t,
                        theme::popover(&t)
                            .id("confirm")
                            .relative()
                            .occlude()
                            .w(px(440.))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            }),
                        "confirm",
                        title,
                        None,
                        cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                        div()
                            .flex()
                            .flex_col()
                            .gap(t.space.md)
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.colors.text)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(subject),
                            )
                            .when(!files.is_empty(), |d| d.child(list))
                            .child(
                                div()
                                    .text_color(t.colors.text_muted)
                                    .line_height(gpui::relative(1.45))
                                    .child(body),
                            ),
                        vec![
                            super::button::Button::ghost("confirm-cancel", "Cancel")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                                )
                                .into_any_element(),
                            super::button::Button::danger("confirm-delete", action)
                                .with_icon(action_icon)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.confirm(window, cx)),
                                )
                                .into_any_element(),
                        ],
                    ),
                    "confirm",
                    t.extra.danger,
                    leaving,
                    &t,
                )),
            "confirm",
            t.extra.scrim,
            leaving,
            &t,
        )
    }
}
