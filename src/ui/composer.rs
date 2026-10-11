//! The composer: a white card with a multi-line input that grows up to a
//! limit, and a row of controls: attach (ghost), the model picker (a
//! popover), the context meter (when it's known), the Modes menu, and Send, the one filled button
//! (Stop while a reply plays). Enter sends; Shift+Enter adds a line.

use gpui::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, IntoElement, Render, SharedString,
    Subscription, Window,
};

use super::*;
use crate::state::AppState;
use crate::theme::ActiveTheme;

pub struct Composer {
    state: Entity<AppState>,
    input: Entity<TextareaState>,
    /// The model window's search box, and its row under the keyboard.
    filter: Entity<InputState>,
    active: Option<String>,
    picker_was_open: bool,
    /// Where the press that just closed a menu happened: the click it ends
    /// in, on that menu's own button, must not open it again.
    dismissed_at: Option<gpui::Point<gpui::Pixels>>,
    /// Holds the keyboard while the model window is open (no search box),
    /// so the arrows move through it instead of the text.
    menu_focus: gpui::FocusHandle,
    _subs: Vec<Subscription>,
}

impl Composer {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let t = cx.theme().clone();
        // The tallest the input grows, in lines of chat text.
        let max_rows = (f32::from(t.size.composer_max) / (f32::from(t.text.body) * 1.5)).floor().max(2.) as usize;
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, max_rows).placeholder("Ask Forge anything… (@ to mention a file)").submit_on_enter(true));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search models…"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                // An edited message comes back here to be changed and sent.
                if let Some(text) = state.update(cx, |s, _| s.draft.take()) {
                    this.fill(&text, window, cx);
                }
                // The model window opening: an empty search, the current row active.
                let s = state.read(cx);
                let (open, current) = (s.picker_open, s.active_profile().map(|p| p.name.clone()));
                let rows = s.endpoints.as_ref().map(|e| e.profiles.len()).unwrap_or(0);
                if open && !this.picker_was_open {
                    this.active = current;
                    this.filter.update(cx, |f, cx| f.set_value("", window, cx));
                    if rows >= super::model_menu::FILTER_THRESHOLD {
                        this.filter.update(cx, |f, cx| f.focus(window, cx));
                    } else {
                        window.focus(&this.menu_focus, cx);
                    }
                }
                if !open && this.picker_was_open && (this.menu_focus.is_focused(window) || gpui::Focusable::focus_handle(this.filter.read(cx), cx).is_focused(window)) {
                    this.input.update(cx, |i, cx| i.focus(window, cx));
                }
                this.picker_was_open = open;
                cx.notify();
            }),
            cx.subscribe_in(&filter, window, |_, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, .. } => this.submit(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
        ];
        Self { state, input, filter, active: None, picker_was_open: false, dismissed_at: None, menu_focus: cx.focus_handle(), _subs: subs }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| i.focus(window, cx));
    }

    /// Put text in the box (a suggestion chip) and focus it.
    pub fn fill(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| {
            i.set_value(text.to_string(), window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    fn text(&self, cx: &gpui::App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).busy() {
            return;
        }
        let text = self.text(cx);
        if text.trim().is_empty() {
            return;
        }
        self.state.update(cx, |s, cx| s.send(text, cx));
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
    }

    /// Whether this click ends the press that just closed a menu (and so
    /// should do nothing). Clears the record either way.
    fn just_dismissed(&mut self, ev: &gpui::ClickEvent) -> bool {
        let at = self.dismissed_at.take();
        matches!(ev, gpui::ClickEvent::Mouse(m) if Some(m.down.position) == at)
    }

    /// The model window's keys: arrows move (wrapping, through the effort
    /// row), Enter picks, Esc clears the search, then closes.
    fn menu_key(&mut self, ev: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.read(cx).picker_open {
            return;
        }
        let query = self.filter.read(cx).value().to_string();
        let keys = super::model_menu::keys(self.state.read(cx), &query);
        let at = self.active.as_ref().and_then(|a| keys.iter().position(|k| k == a));
        match ev.keystroke.key.as_str() {
            "down" | "up" if !keys.is_empty() => {
                let n = keys.len();
                let i = match (at, ev.keystroke.key.as_str()) {
                    (None, _) => 0,
                    (Some(i), "down") => (i + 1) % n,
                    (Some(i), _) => (i + n - 1) % n,
                };
                self.active = Some(keys[i].clone());
            }
            "enter" => {
                if let Some(key) = self.active.clone() {
                    self.state.update(cx, |s, cx| {
                        if key == super::model_menu::EFFORT_ROW {
                            let levels = s.active_profile().map(|p| p.effort_levels()).unwrap_or_default();
                            s.effort = super::model_menu::next_effort(&levels, s.effort);
                            cx.notify();
                        } else if super::model_menu::rows(s, &query).0.iter().any(|r| r.name == key && !r.disabled) {
                            s.select_profile(&key, cx);
                        }
                    });
                }
            }
            "escape" => {
                if query.is_empty() {
                    self.state.update(cx, |s, cx| {
                        s.picker_open = false;
                        cx.notify();
                    });
                    self.focus(window, cx);
                } else {
                    self.filter.update(cx, |f, cx| f.set_value("", window, cx));
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for Composer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let state = self.state.clone();
        let empty = self.text(cx).trim().is_empty();
        let hover = t.colors.hover;

        // A menu closes on a press outside it (each listens on itself: it
        // floats above the composer, outside the composer's own box).
        let me = cx.entity().downgrade();
        let dismiss = move |pos: gpui::Point<gpui::Pixels>, models: bool, cx: &mut gpui::App| {
            let _ = me.update(cx, |c, cx| {
                c.dismissed_at = Some(pos);
                c.state.update(cx, |s, cx| {
                    if models {
                        s.picker_open = false;
                    } else {
                        s.modes_open = false;
                        s.bypass_warning = false;
                    }
                    cx.notify();
                });
            });
        };
        let (dismiss_models, dismiss_modes) = (dismiss.clone(), dismiss);
        let me = cx.entity().downgrade();
        let skip = move |ev: &gpui::ClickEvent, cx: &mut gpui::App| -> bool { me.update(cx, |c, _| c.just_dismissed(ev)).unwrap_or(false) };

        // The pieces that read the state are built first.
        let mode_trigger = super::mode_menu::trigger(&t, &state, cx, skip);
        let modes_menu = self.state.read(cx).modes_open.then(|| super::mode_menu::menu(&t, &state, cx, move |pos, cx| dismiss_modes(pos, false, cx)));
        let query = self.filter.read(cx).value().to_string();
        let search = div()
            .m(px(4.))
            .child(Input::new(&self.filter).appearance(false).text_size(t.text.body_sm))
            .px(px(8.))
            .py(px(4.))
            .rounded(px(4.))
            .bg(t.colors.hover)
            .into_any_element();
        let this = cx.entity().downgrade();
        let on_hover = move |key: String, cx: &mut gpui::App| {
            let _ = this.update(cx, |c, cx| {
                if c.active.as_deref() != Some(key.as_str()) {
                    c.active = Some(key);
                    cx.notify();
                }
            });
        };
        let s = self.state.read(cx);
        let (busy, mode, picker_open) = (s.busy(), s.mode, s.picker_open);
        let model_menu = picker_open.then(|| super::model_menu::render(&t, &state, s, &query, self.active.as_deref(), search, on_hover, move |pos, cx| dismiss_models(pos, true, cx)));
        let model_name: SharedString = match (&s.endpoints, s.active_profile()) {
            (None, _) => "Model".into(),
            (Some(_), None) => "Set up an endpoint first".into(),
            (Some(_), Some(p)) => p.display().to_string().into(),
        };
        let pill_effort = super::model_menu::pill_effort(&t, s);
        let tint = super::mode_menu::tint(&t, mode);
        // The context meter: this conversation against the window of the
        // model picked now. Hidden when either isn't known.
        let profile = s.active_profile();
        let meter = crate::context::meter(
            s.chat().and_then(|c| crate::context::last_usage(&c.messages)),
            profile.and_then(|p| p.context_window(s.health.get(&p.name))),
        )
        .map(|m| (m, profile.map(|p| p.display().to_string()).unwrap_or_default()));

        let send_ready = busy || !empty;
        // The send button takes the mode's colour, as in the extension.
        let send_bg = match mode {
            crate::modes::ModeId::Plan | crate::modes::ModeId::Bypass => tint.unwrap_or(t.colors.primary),
            _ => t.colors.primary,
        };
        let send = div()
            .id("send")
            .size(t.size.control + px(4.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(t.radius.button)
            .when(busy, |d| {
                d.bg(t.colors.surface).border_1().border_color(t.colors.border).cursor_pointer().child(icon("icons/square.svg", t.size.icon_sm, t.colors.text))
            })
            .when(!busy, |d| {
                d.bg(if send_ready { send_bg } else { t.colors.hover })
                    .when(send_ready, |d| d.cursor_pointer().hover(move |st| st.opacity(0.9)))
                    .child(icon("icons/arrow-up.svg", t.size.icon, if send_ready { t.colors.primary_fg } else { t.colors.text_subtle }))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.state.read(cx).busy() {
                    this.state.update(cx, |s, cx| s.stop(cx));
                } else {
                    this.submit(window, cx);
                }
            }));

        let model_pill = div()
            .id("model")
            .flex()
            .items_center()
            .gap(px(4.))
            .h(px(20.))
            .px(px(8.))
            .rounded(t.radius.pill)
            .cursor_pointer()
            .text_size(t.text.body_sm - px(1.))
            .text_color(if picker_open { t.colors.primary } else { t.colors.text })
            .bg(if picker_open { t.colors.primary_tint } else { t.colors.hover })
            .hover(move |st| st.opacity(0.85))
            .child(model_name)
            .children(pill_effort.map(|(label, color)| div().text_color(color).child(label)))
            .on_click(cx.listener(|this, ev: &gpui::ClickEvent, window, cx| {
                if !this.just_dismissed(ev) {
                    window.dispatch_action(Box::new(ToggleModelPicker), cx);
                }
            }));

        div()
            .key_context("Composer")
            .track_focus(&self.menu_focus)
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .bg(t.colors.surface)
            .border_1()
            // The mode tints the composer's edge, as it tints the extension's focus ring.
            .border_color(tint.map(|c| c.opacity(0.55)).unwrap_or(t.colors.border))
            .rounded(t.radius.card)
            .capture_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| this.menu_key(ev, window, cx)))
            .children(model_menu)
            .child(
                div()
                    .px(t.space.lg)
                    .pt(t.space.md)
                    .text_size(t.text.body)
                    .line_height(relative(1.5))
                    .child(Textarea::new(&self.input).appearance(false)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .px(t.space.sm)
                    .pt(t.space.xs)
                    .pb(t.space.sm)
                    // Attach: the ghost action.
                    .child(
                        div()
                            .id("attach")
                            .size(t.size.control)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            .hover(move |st| st.bg(hover))
                            .child(icon("icons/paperclip.svg", t.size.icon_sm, t.colors.text_muted))
                            .on_click(cx.listener(|this, _, _, cx| this.state.update(cx, |s, cx| s.show_toast("Attaching files and images is a stub in this UI shell.", cx)))),
                    )
                    .child(model_pill)
                    .child(div().flex_1())
                    .children(meter.map(|(m, model)| {
                        let f = m.fraction();
                        let colour = if f > 1. { t.extra.danger } else if f > 0.8 { t.extra.warning } else { t.colors.text_subtle };
                        let tip: SharedString = format!(
                            "{}{} / {} context used · {model}{}",
                            if m.estimated { "About " } else { "" },
                            crate::context::short(m.used),
                            crate::context::short(m.window),
                            if f > 1. { ". More than this model's window: the conversation will be compacted" } else { "" },
                        )
                        .into();
                        div()
                            .id("context-meter")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .mr(t.space.xs)
                            .text_size(t.text.caption)
                            .text_color(colour)
                            .child(
                                div()
                                    .w(px(36.))
                                    .h(px(4.))
                                    .rounded(t.radius.pill)
                                    .bg(t.colors.border)
                                    .child(div().h_full().w(relative(f.min(1.))).rounded(t.radius.pill).bg(if f > 0.8 { colour } else { t.colors.text_subtle })),
                            )
                            .child(if f > 0. && f < 0.01 { "<1%".to_string() } else { format!("{:.0}%", (f * 100.).round()) })
                            .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                    }))
                    // Modes: the button and its menu above it.
                    .child(
                        div()
                            .relative()
                            .child(mode_trigger)
                            .children(modes_menu),
                    )
                    .child(send),
            )
    }
}
