//! Forge's questions as a form (`AskUserQuestion`; in a design chat, Open
//! Design's discovery form and direction picker): each question with its
//! choices to click, one or several, and Send. What you type in the
//! composer meanwhile answers the first question without a choice.

use gpui::{div, prelude::*, AnyElement, Entity, FontWeight, IntoElement, SharedString};

use super::icon;
use crate::state::{AppState, QuestionForm};
use crate::theme::{self, Theme};

pub fn render(
    form: &QuestionForm,
    state: &Entity<AppState>,
    (chat, msg, block): (usize, usize, usize),
    t: &Theme,
) -> AnyElement {
    let id = |s: String| SharedString::from(format!("qf-{chat}-{msg}-{block}-{s}"));
    let questions = form.questions.iter().enumerate().map(|(q, question)| {
        let picked = &form.picked[q];
        let options = question
            .options
            .iter()
            .enumerate()
            .map(|(o, (label, about))| {
                let on = picked.contains(&o);
                let st = state.clone();
                let hover = t.colors.hover;
                div()
                    .id(id(format!("{q}-{o}")))
                    .flex()
                    .items_start()
                    .gap(t.space.sm)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .rounded(t.radius.button)
                    .border_1()
                    .border_color(if on {
                        t.colors.primary
                    } else {
                        t.colors.border
                    })
                    .cursor_pointer()
                    .when(on, |d| d.bg(t.colors.primary_tint))
                    .when(!on, |d| d.hover(move |s| s.bg(hover)))
                    .child(div().flex_none().pt(gpui::px(2.)).child(icon(
                        match (question.multi, on) {
                            (true, true) => "icons/square-check.svg",
                            (true, false) => "icons/square.svg",
                            (false, true) => "icons/circle-dot.svg",
                            (false, false) => "icons/circle.svg",
                        },
                        t.size.icon_sm,
                        if on {
                            t.colors.primary
                        } else {
                            t.colors.text_subtle
                        },
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_color(t.colors.text_strong)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(label.clone()),
                            )
                            .when(!about.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .child(about.clone()),
                                )
                            }),
                    )
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| s.form_pick(chat, msg, block, q, o, cx))
                    })
            });
        div()
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .when(!question.header.is_empty(), |d| {
                        d.child(theme::pill(
                            t,
                            t.colors.hover,
                            t.colors.text_muted,
                            question.header.clone(),
                        ))
                    })
                    .child(
                        div()
                            .text_color(t.colors.text_strong)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(question.question.clone()),
                    )
                    .when(question.multi, |d| {
                        d.child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child("Pick any"),
                        )
                    }),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(if question.options.len() > 2 { 2 } else { 1 })
                    .gap(t.space.sm)
                    .children(options),
            )
    });
    let ready = form.complete(false);
    let st = state.clone();
    let send = theme::primary_button(t, "Send answers")
        .id(id("send".into()))
        .when(!ready, |d| d.opacity(0.5))
        .on_click(move |_, _, cx| {
            if ready {
                st.update(cx, |s, cx| s.form_send(chat, msg, block, None, cx))
            }
        });
    div()
        .flex()
        .flex_col()
        .gap(t.space.lg)
        .p(t.space.lg)
        .rounded(t.radius.card)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .text_color(t.colors.text_muted)
                .child(icon(
                    "icons/message-square.svg",
                    t.size.icon_sm,
                    t.colors.text_muted,
                ))
                .child("Forge asks"),
        )
        .children(questions)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(t.space.md)
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child("Or type your own answer in the box below."),
                )
                .child(send),
        )
        .into_any_element()
}
