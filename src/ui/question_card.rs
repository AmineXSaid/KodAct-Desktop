//! A question Forge asks you (AskUserQuestion), as a card of its own rather
//! than a permission prompt: the question, its options as rows to pick
//! (several questions as tabs), Skip and Submit; your own words go in the
//! message box. Once answered it is one quiet row: what you chose.

use gpui::{div, prelude::*, px, AnyElement, FontWeight, SharedString};

use super::button::Button;
use super::icon;
use super::message::{env_badge, ItemContext};
use crate::state::{Decider, PermInfo, Question, Verdict};
use crate::theme::{self, Theme};

/// What a model marks its suggested option with.
const RECOMMENDED: &str = "(Recommended)";

/// The card at block `b`: waiting on you, or answered.
pub fn render(
    verdict: Verdict,
    info: &PermInfo,
    b: usize,
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    if verdict == Verdict::Pending {
        pending(info, b, cx, t)
    } else {
        answered(verdict, info, cx, t)
    }
}

/// An option's label without its "(Recommended)" mark, and whether it had one.
fn recommended(label: &str) -> (&str, bool) {
    match label.trim_end().strip_suffix(RECOMMENDED) {
        Some(rest) => (rest.trim_end(), true),
        None => (label, false),
    }
}

fn pending(info: &PermInfo, b: usize, cx: &ItemContext, t: &Theme) -> AnyElement {
    let qs = &info.questions;
    let step = info.step.min(qs.len().saturating_sub(1));
    let Some(q) = qs.get(step) else {
        return div().into_any_element();
    };
    let id = |name: &str| SharedString::from(format!("q-{name}-{}-{}-{b}", cx.chat, cx.index));
    let (chat, msg) = (cx.chat, cx.index);
    let state = cx.state.clone();
    let hover = t.colors.hover;

    // Several questions: a tab each, named by its header, ticked once answered.
    let tabs = (qs.len() > 1).then(|| {
        div()
            .flex()
            .flex_wrap()
            .gap(t.space.xs)
            .children(qs.iter().enumerate().map(|(i, question)| {
                let st = state.clone();
                let name = if question.header.is_empty() {
                    format!("Question {}", i + 1)
                } else {
                    question.header.clone()
                };
                let tab = div()
                    .id(SharedString::from(format!("q-tab-{chat}-{msg}-{b}-{i}")))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .h(t.size.row)
                    .px(t.space.md)
                    .rounded(t.radius.pill)
                    .border_1()
                    .border_color(t.colors.border)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .cursor_pointer()
                    .when(question.answered(), |d| {
                        d.child(icon("icons/check.svg", t.size.icon_xs, t.extra.success))
                    })
                    .child(name)
                    .on_click(move |_, _, app| {
                        st.update(app, |s, cx| s.question_tab(chat, msg, b, i, cx))
                    });
                if i == step {
                    theme::selected(t, tab.text_color(t.colors.text_strong)).into_any_element()
                } else {
                    tab.hover(move |s| s.bg(hover)).into_any_element()
                }
            }))
    });

    let options = div().flex().flex_col().gap(t.space.xs).children(
        q.options.iter().enumerate().map(|(o, opt)| {
            let picked = q.picked.contains(&o);
            let (label, rec) = recommended(&opt.label);
            let mark = match (q.multi, picked) {
                (true, true) => "icons/square-check.svg",
                (true, false) => "icons/square.svg",
                (false, true) => "icons/circle-check.svg",
                (false, false) => "icons/circle.svg",
            };
            let st = state.clone();
            let row = div()
                .id(SharedString::from(format!("q-opt-{chat}-{msg}-{b}-{step}-{o}")))
                .role(if q.multi {
                    gpui::accesskit::Role::CheckBox
                } else {
                    gpui::accesskit::Role::RadioButton
                })
                .aria_label(SharedString::from(label.to_string()))
                .flex()
                .items_start()
                .gap(t.space.sm)
                .px(t.space.md)
                .py(t.space.sm)
                .rounded(t.radius.button)
                .border_1()
                .border_color(t.colors.border)
                .cursor_pointer()
                .child(div().pt(px(2.)).child(icon(
                    mark,
                    t.size.icon_sm,
                    if picked { t.extra.accent } else { t.colors.text_subtle },
                )))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .flex_wrap()
                                .gap(t.space.sm)
                                .child(
                                    div()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t.colors.text_strong)
                                        .child(label.to_string()),
                                )
                                .when(rec, |d| {
                                    d.child(
                                        theme::pill(
                                            t,
                                            t.extra.accent_soft,
                                            t.colors.text_strong,
                                            "Recommended",
                                        )
                                        .text_size(t.text.caption),
                                    )
                                }),
                        )
                        .when(!opt.description.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(t.text.body_sm)
                                    .text_color(t.colors.text_muted)
                                    .child(opt.description.clone()),
                            )
                        }),
                )
                .on_click(move |_, _, app| {
                    st.update(app, |s, cx| s.pick_option(chat, msg, b, step, o, cx))
                });
            if picked {
                theme::selected(t, row).into_any_element()
            } else {
                row.hover(move |s| s.bg(hover)).into_any_element()
            }
        }),
    );

    // What you typed instead of an option.
    let own = q.own.clone().map(|text| {
        theme::selected(
            t,
            div()
                .flex()
                .items_start()
                .gap(t.space.sm)
                .px(t.space.md)
                .py(t.space.sm)
                .rounded(t.radius.button)
                .border_1()
                .border_color(t.colors.border),
        )
        .child(div().pt(px(2.)).child(icon(
            "icons/pencil.svg",
            t.size.icon_sm,
            t.extra.accent,
        )))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child("Your answer"),
                )
                .child(div().text_color(t.colors.text_strong).child(text)),
        )
    });

    let last = step + 1 >= qs.len();
    let all = qs.iter().all(Question::answered);
    let (skip, back, next, submit) = (state.clone(), state.clone(), state.clone(), state.clone());
    let footer = div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap(t.space.sm)
        .child(
            Button::ghost(id("skip"), "Skip")
                .describe("Forge goes on with its own assumptions.")
                .tooltip(true)
                .on_click(move |_, _, app| {
                    skip.update(app, |s, cx| s.answer(chat, msg, b, Verdict::Denied, cx))
                }),
        )
        .child(div().flex_1())
        .when(step > 0, |d| {
            d.child(Button::secondary(id("back"), "Back").on_click(move |_, _, app| {
                back.update(app, |s, cx| s.question_tab(chat, msg, b, step - 1, cx))
            }))
        })
        .child(if last {
            Button::accent(id("submit"), "Submit answers")
                .disabled(!all)
                .on_click(move |_, _, app| {
                    submit.update(app, |s, cx| s.submit_answers(chat, msg, b, cx))
                })
                .into_any_element()
        } else {
            Button::accent(id("next"), "Next")
                .disabled(!q.answered())
                .on_click(move |_, _, app| {
                    next.update(app, |s, cx| s.question_tab(chat, msg, b, step + 1, cx))
                })
                .into_any_element()
        });

    let body = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(t.space.md)
        .p(t.space.lg)
        .child(
            div()
                .flex()
                .items_start()
                .gap(t.space.md)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.space.xs)
                                .child(icon("icons/hand.svg", t.size.icon_xs, t.extra.accent))
                                .child(theme::eyebrow(t, "Needs you")),
                        )
                        .child(
                            div()
                                .text_size(t.text.body + px(2.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                                .child(q.text.clone()),
                        )
                        .when(q.multi, |d| {
                            d.child(
                                div()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .child("Choose any that apply."),
                            )
                        }),
                )
                .child(env_badge(&cx.place, t)),
        )
        .children(tabs)
        .child(options)
        .children(own)
        .child(footer)
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child("Or type your own answer in the box below."),
        );
    div()
        .flex()
        .overflow_hidden()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_grain())
        .child(div().w(px(3.)).flex_none().bg(t.extra.accent))
        .child(body)
        .into_any_element()
}

/// The answered card: a line saying so, then each question and its answer.
fn answered(verdict: Verdict, info: &PermInfo, cx: &ItemContext, t: &Theme) -> AnyElement {
    let n = info.questions.len();
    let noun = if n == 1 { "question" } else { "questions" };
    let skipped = verdict == Verdict::Denied;
    let head = match (&info.by, skipped) {
        (Decider::Stopped, _) => "Not answered: the reply was stopped".to_string(),
        (_, true) => format!("Skipped {n} {noun}"),
        (_, false) => format!("Answered {n} {noun}"),
    };
    let mut meta = vec![];
    if !skipped {
        meta.push("by you".to_string());
    }
    meta.push(match &cx.place {
        Some((label, _)) => format!("on {label}"),
        None => "on this computer".into(),
    });
    meta.extend(info.at.map(super::versions::ago));
    div()
        .flex()
        .items_start()
        .gap(t.space.sm)
        .px(t.space.md)
        .py(t.space.sm)
        .rounded(t.radius.button)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .child(div().pt(px(1.)).child(icon(
            "icons/circle-help.svg",
            t.size.icon_sm,
            if skipped {
                t.colors.text_subtle
            } else {
                t.extra.success
            },
        )))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_wrap()
                        .gap(t.space.sm)
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.colors.text_strong)
                                .child(head),
                        )
                        .child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(meta.join(" · ")),
                        ),
                )
                .children(info.questions.iter().map(|q| {
                    let asked = if q.header.is_empty() {
                        q.text.clone()
                    } else {
                        q.header.clone()
                    };
                    let said = match q.answer() {
                        Some(a) if !skipped => recommended(&a).0.to_string(),
                        _ => "—".into(),
                    };
                    div()
                        .flex()
                        .items_start()
                        .gap(t.space.sm)
                        .min_w_0()
                        .text_size(t.text.body_sm)
                        .child(
                            div()
                                .flex_none()
                                .max_w(px(220.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_color(t.colors.text_muted)
                                .child(asked),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(t.colors.text_subtle)
                                .child("→"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(t.colors.text)
                                .child(said),
                        )
                })),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::recommended;

    #[test]
    fn the_recommended_mark_becomes_a_pill() {
        assert_eq!(recommended("Tauri v2 (Recommended)"), ("Tauri v2", true));
        assert_eq!(recommended("Leptos"), ("Leptos", false));
    }
}
