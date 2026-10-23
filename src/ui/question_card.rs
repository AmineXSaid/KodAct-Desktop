//! A question Forge asks you (AskUserQuestion), as a card of its own rather
//! than a permission prompt: the question and its options as one list to
//! pick from, "Other" last with a field for your own words (the message box
//! works too), steps when there are several questions, Skip and Submit.
//! Once answered it is one quiet row: what you chose.

use gpui::{div, prelude::*, px, AnyElement, FontWeight, SharedString};

use super::button::Button;
use super::icon;
use super::message::ItemContext;
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

    // Several questions: steps named by their headers, ticked once answered,
    // each one a click away.
    let steps = (qs.len() > 1).then(|| {
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
                let chip = div()
                    .id(SharedString::from(format!("q-tab-{chat}-{msg}-{b}-{i}")))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .h(t.size.row)
                    .px(t.space.sm)
                    .rounded(t.radius.button)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .cursor_pointer()
                    .child(if question.answered() && i != step {
                        icon("icons/check.svg", t.size.icon_xs, t.extra.success).into_any_element()
                    } else {
                        div()
                            .text_color(t.colors.text_subtle)
                            .child(format!("{}", i + 1))
                            .into_any_element()
                    })
                    .child(name)
                    .on_click(move |_, _, app| {
                        st.update(app, |s, cx| s.question_tab(chat, msg, b, i, cx))
                    });
                if i == step {
                    theme::selected(t, chip.text_color(t.colors.text_strong)).into_any_element()
                } else {
                    chip.hover(move |s| s.bg(hover)).into_any_element()
                }
            }))
    });

    // One list, a row per option and "Other" last, divided by hairlines.
    let row = |key: String| {
        div()
            .id(SharedString::from(key))
            .flex()
            .items_start()
            .gap(t.space.sm)
            .px(t.space.md)
            .py(t.space.sm)
            .cursor_pointer()
    };
    let mut rows: Vec<AnyElement> = vec![];
    for (o, opt) in q.options.iter().enumerate() {
        let picked = q.picked.contains(&o);
        let (label, rec) = recommended(&opt.label);
        let mark = match (q.multi, picked) {
            (true, true) => "icons/square-check.svg",
            (true, false) => "icons/square.svg",
            (false, true) => "icons/circle-check.svg",
            (false, false) => "icons/circle.svg",
        };
        let st = state.clone();
        let el = row(format!("q-opt-{chat}-{msg}-{b}-{step}-{o}"))
            .role(if q.multi {
                gpui::accesskit::Role::CheckBox
            } else {
                gpui::accesskit::Role::RadioButton
            })
            .aria_label(SharedString::from(label.to_string()))
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
                                    theme::pill(t, t.extra.accent_soft, t.colors.text_strong, "Recommended")
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
            .on_click(move |_, _, app| st.update(app, |s, cx| s.pick_option(chat, msg, b, step, o, cx)));
        rows.push(if picked {
            theme::selected(t, el).into_any_element()
        } else {
            el.hover(move |s| s.bg(hover)).into_any_element()
        });
    }
    // "Other": your own words, typed right here (Enter answers).
    let other_on = q.own.is_some();
    let (st, field) = (state.clone(), cx.other.clone());
    let other = row(format!("q-other-{chat}-{msg}-{b}-{step}"))
        .role(gpui::accesskit::Role::RadioButton)
        .aria_label(SharedString::from("Other: type your own answer"))
        .child(div().pt(px(2.)).child(icon(
            "icons/pencil.svg",
            t.size.icon_sm,
            if other_on { t.extra.accent } else { t.colors.text_subtle },
        )))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child("Other"),
                )
                .child(match (other_on, &cx.other) {
                    (true, Some(input)) => div()
                        .child(gpui::component::input::Input::new(input))
                        .into_any_element(),
                    (true, None) => div()
                        .text_color(t.colors.text)
                        .child(q.own.clone().unwrap_or_default())
                        .into_any_element(),
                    _ => div()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_muted)
                        .child("Type your own answer")
                        .into_any_element(),
                }),
        )
        .on_click(move |_, window, app| {
            st.update(app, |s, cx| s.choose_other(chat, msg, b, step, cx));
            if let Some(input) = &field {
                input.update(app, |i, cx| i.focus(window, cx));
            }
        });
    rows.push(if other_on {
        theme::selected(t, other).into_any_element()
    } else {
        other.hover(move |s| s.bg(hover)).into_any_element()
    });
    let options = div()
        .flex()
        .flex_col()
        .rounded(t.radius.button)
        .border_1()
        .border_color(t.colors.border)
        .overflow_hidden()
        .children(rows.into_iter().enumerate().map(|(i, r)| {
            div()
                .when(i > 0, |d| d.border_t_1().border_color(t.colors.border))
                .child(r)
        }));

    let last = step + 1 >= qs.len();
    let all = qs.iter().all(Question::answered);
    let (skip, back, next, submit) = (state.clone(), state.clone(), state.clone(), state.clone());
    let quiet = t.colors.text;
    let footer = div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap(t.space.sm)
        // Skip is a quiet word, never as loud as Submit.
        .child(
            div()
                .id(id("skip"))
                .role(gpui::accesskit::Role::Button)
                .aria_label(SharedString::from("Skip: Forge goes on with its own assumptions"))
                .px(t.space.xs)
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .cursor_pointer()
                .hover(move |s| s.text_color(quiet))
                .child("Skip")
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
        // Several questions: their steps, and where you are. The question
        // below is the heading; no label above it.
        .children(steps.map(|steps| {
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(steps)
                .child(div().flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(format!("{} of {}", step + 1, qs.len())),
                )
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
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
        .child(options)
        .child(footer);
    div()
        .flex()
        .overflow_hidden()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_grain())
        .child(body)
        .into_any_element()
}

/// The answered card, as quiet as a run of calls: a muted line saying so,
/// then each question and your answer under a rule.
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
    if let Some((label, _)) = &cx.place {
        meta.push(format!("on {label}"));
    }
    meta.extend(info.at.map(super::versions::ago));
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.xs + px(2.))
                .min_h(t.size.quiet_line)
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .child(head)
                .when(!meta.is_empty(), |d| {
                    d.child(div().text_color(t.colors.text_subtle).child(format!("· {}", meta.join(" · "))))
                }),
        )
        .when(!skipped, |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .mt(px(2.))
                    .ml(t.size.icon_xs / 2.)
                    .pl(t.space.md)
                    .border_l_2()
                    .border_color(t.colors.border)
                    .children(info.questions.iter().map(|q| {
                        let asked = if q.header.is_empty() {
                            q.text.clone()
                        } else {
                            q.header.clone()
                        };
                        let said = q.answer().map(|a| recommended(&a).0.to_string()).unwrap_or_default();
                        div()
                            .flex()
                            .items_start()
                            .gap(t.space.sm)
                            .min_w_0()
                            .text_size(t.text.body_sm)
                            .child(div().flex_none().text_color(t.colors.text_muted).child(asked))
                            .child(div().flex_none().text_color(t.colors.text_subtle).child("→"))
                            .child(div().flex_1().min_w_0().text_color(t.colors.text).child(said))
                    })),
            )
        })
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
