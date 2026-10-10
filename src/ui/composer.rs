//! The composer: a white card with a multi-line input that grows up to a
//! limit, and a row of controls: attach (ghost), permission mode, the model
//! picker (a popover), the context meter, and Send, the one filled button
//! (Stop while a reply plays). Enter sends; Shift+Enter adds a line.

use gpui::component::input::{InputEvent, Textarea, TextareaState};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window,
};

use super::*;
use crate::state::{AppState, Mode, MODELS};
use crate::theme::{self, ActiveTheme};

pub struct Composer {
    state: Entity<AppState>,
    input: Entity<TextareaState>,
    _subs: Vec<Subscription>,
}

impl Composer {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let t = cx.theme().clone();
        // The tallest the input grows, in lines of chat text.
        let max_rows = (f32::from(t.size.composer_max) / (f32::from(t.text.body) * 1.5)).floor().max(2.) as usize;
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, max_rows).placeholder("Reply to Claude… (@ to mention a file)").submit_on_enter(true));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, .. } => this.submit(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
        ];
        Self { state, input, _subs: subs }
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

    fn model_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let current = self.state.read(cx).model;
        let hover = t.colors.hover;
        let mut list = div().flex().flex_col();
        let mut last_provider = "";
        for (i, m) in MODELS.iter().enumerate() {
            if m.provider != last_provider {
                last_provider = m.provider;
                list = list.child(
                    div()
                        .px(t.space.md)
                        .pt(t.space.sm)
                        .pb(t.space.xs)
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_subtle)
                        .child(m.provider),
                );
            }
            let state = self.state.clone();
            list = list.child(
                div()
                    .id(("model", i))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(div().child(m.name))
                            .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(m.note)),
                    )
                    .child(div().text_size(t.text.caption).font_family(t.font_mono.clone()).text_color(t.colors.text_subtle).child(m.context))
                    .child(div().w(t.size.icon).when(i == current, |d| d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.primary))))
                    .on_click(move |_, _, cx| {
                        state.update(cx, |s, cx| {
                            s.model = i;
                            s.picker_open = false;
                            cx.notify();
                        })
                    }),
            );
        }
        theme::popover(&t)
            .absolute()
            .bottom(t.size.control + t.space.md)
            .left_0()
            .w(t.size.picker)
            .child(list)
            .child(
                div()
                    .mt(t.space.xs)
                    .pt(t.space.sm)
                    .pb(t.space.xs)
                    .px(t.space.md)
                    .border_t_1()
                    .border_color(t.colors.border)
                    .flex()
                    .justify_between()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child("Providers are set up in Settings")
                    .child(shortcut("/")),
            )
    }
}

impl Render for Composer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let (busy, mode, picker_open, model, used) = (s.busy(), s.mode, s.picker_open, s.model, s.context_used);
        let empty = self.text(cx).trim().is_empty();
        let hover = t.colors.hover;

        let pill = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(t.space.xs + px(2.))
                .h(t.size.control)
                .px(t.space.sm + px(2.))
                .rounded(t.radius.pill)
                .cursor_pointer()
                .text_size(t.text.body_sm - px(1.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_muted)
                .hover(move |st| st.bg(hover))
        };

        let (mode_bg, mode_fg) = match mode {
            Mode::Plan => (Some(t.colors.midnight), t.colors.primary_fg),
            Mode::AcceptEdits => (None, t.extra.success),
            Mode::Auto => (Some(t.extra.warning_tint), t.extra.warning),
            Mode::Ask => (None, t.colors.text_muted),
        };

        let send_ready = busy || !empty;
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
                let hover = t.colors.primary_hover;
                d.bg(if send_ready { t.colors.primary } else { t.colors.hover })
                    .when(send_ready, |d| d.cursor_pointer().hover(move |st| st.bg(hover)))
                    .child(icon("icons/arrow-up.svg", t.size.icon, if send_ready { t.colors.primary_fg } else { t.colors.text_subtle }))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.state.read(cx).busy() {
                    this.state.update(cx, |s, cx| s.stop(cx));
                } else {
                    this.submit(window, cx);
                }
            }));

        div()
            .key_context("Composer")
            .w_full()
            .flex()
            .flex_col()
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .rounded(t.radius.card)
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
                            .bg(t.colors.primary_tint)
                            .cursor_pointer()
                            .child(icon("icons/paperclip.svg", t.size.icon_sm, t.colors.primary))
                            .on_click(cx.listener(|this, _, _, cx| this.state.update(cx, |s, cx| s.show_toast("Attaching files and images is a stub in this UI shell.", cx)))),
                    )
                    .child(
                        pill("mode")
                            .when_some(mode_bg, |d, bg| d.bg(bg))
                            .text_color(mode_fg)
                            .child(icon(mode.icon(), t.size.icon_sm, mode_fg))
                            .child(mode.short())
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(CycleMode), cx)),
                    )
                    .child(
                        div()
                            .relative()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                if this.state.read(cx).picker_open {
                                    this.state.update(cx, |s, cx| {
                                        s.picker_open = false;
                                        cx.notify();
                                    });
                                }
                            }))
                            .child(
                                pill("model")
                                    .child(icon("icons/cpu.svg", t.size.icon_sm, t.colors.text_muted))
                                    .child(SharedString::from(MODELS[model].name))
                                    .child(icon(if picker_open { "icons/chevron-up.svg" } else { "icons/chevron-down.svg" }, t.size.icon_sm - px(2.), t.colors.text_subtle))
                                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleModelPicker), cx)),
                            )
                            .when(picker_open, |d| d.child(self.model_picker(cx))),
                    )
                    .child(div().flex_1())
                    // The context meter: how full the window is.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .mr(t.space.sm)
                            .text_size(t.text.caption)
                            .text_color(if used > 0.8 { t.extra.warning } else { t.colors.text_subtle })
                            .child(
                                div()
                                    .w(px(36.))
                                    .h(px(4.))
                                    .rounded(t.radius.pill)
                                    .bg(t.colors.border)
                                    .child(div().h_full().w(relative(used)).rounded(t.radius.pill).bg(if used > 0.8 { t.colors.marigold } else { t.colors.text_subtle })),
                            )
                            .child(format!("{:.0}%", used * 100.)),
                    )
                    .child(send),
            )
    }
}
