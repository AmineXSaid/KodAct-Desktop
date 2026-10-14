//! Settings → Models → Diagnose: an endpoint's checks as a step list (DNS,
//! TCP, TLS, HTTP, the key), each with pass or fail, its detail and its
//! time, filled in as each step ends (`diagnose.rs` runs them off the UI
//! thread). The model-side checks are listed after them as what they are:
//! not built yet (`stubs::CAPABILITY_PROBE`).

use gpui::{div, prelude::*, px, relative, Entity, FontWeight, IntoElement, SharedString};

use super::editor_kit as kit;
use super::{icon, soon};
use crate::diagnose::Outcome;
use crate::state::{AppState, Diagnosis};
use crate::theme::{self, Theme};

pub fn render(t: &Theme, d: &Diagnosis, state: &Entity<AppState>) -> impl IntoElement {
    let mut steps = div().flex().flex_col();
    let n = d.steps.len();
    for (i, s) in d.steps.iter().enumerate() {
        let (path, color) = match s.outcome {
            Outcome::Pass => ("icons/circle-check.svg", t.extra.success),
            Outcome::Fail => ("icons/circle-x.svg", t.extra.danger),
            Outcome::Running => ("icons/loader-circle.svg", t.colors.primary),
            Outcome::Waiting | Outcome::Skip => ("icons/circle-dashed.svg", t.colors.text_subtle),
        };
        let detail = match s.outcome {
            Outcome::Waiting => "Waiting".to_string(),
            Outcome::Running => "Checking…".to_string(),
            _ => s.detail.clone(),
        };
        steps = steps.child(
            div()
                .flex()
                .items_start()
                .gap(t.space.md)
                .py(px(10.))
                .when(i + 1 < n, |d| d.border_b_1().border_color(t.colors.border))
                .child(div().pt(px(1.)).child(icon(path, t.size.icon, color)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(t.text.body_sm)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if s.outcome == Outcome::Skip {
                                    t.colors.text_muted
                                } else {
                                    t.colors.text_strong
                                })
                                .child(s.name),
                        )
                        .child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(if s.outcome == Outcome::Fail {
                                    t.extra.danger
                                } else {
                                    t.colors.text_muted
                                })
                                .line_height(relative(1.4))
                                .child(detail),
                        ),
                )
                .children(s.ms.map(|ms| {
                    div()
                        .flex_none()
                        .text_size(t.text.caption)
                        .font_family(t.font_mono.clone())
                        .text_color(t.colors.text_subtle)
                        .child(format!("{ms} ms"))
                })),
        );
    }
    let failed = d.steps.iter().find(|s| s.outcome == Outcome::Fail);
    let summary = if d.running {
        None
    } else if let Some(f) = failed {
        Some((
            t.extra.danger_tint,
            format!(
                "Stopped at “{}”. Fix that first: the steps after it need it.",
                f.name
            ),
        ))
    } else {
        Some((
            t.extra.success_tint,
            "Everything up to the key works from this PC.".to_string(),
        ))
    };
    let (back, again, probe) = (state.clone(), state.clone(), state.clone());
    let name = d.profile.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(
            kit::back(t, "diag-back", "Endpoints").on_click(move |_, _, cx| {
                back.update(cx, |s, cx| {
                    s.diagnosis = None;
                    cx.notify();
                })
            }),
        )
        .child(kit::header(
            t,
            &format!("Diagnose {}", d.profile),
            "Each step needs the one before it, so the first failure is the one to fix.",
            Some(
                theme::ghost_button(
                    t,
                    if d.running {
                        "Checking…"
                    } else {
                        "Check again"
                    },
                )
                .id("diag-again")
                .when(!d.running, |b| {
                    b.on_click(move |_, _, cx| again.update(cx, |s, cx| s.diagnose(&name, cx)))
                })
                .into_any_element(),
            ),
        ))
        .child(
            div()
                .px(px(14.))
                .rounded(t.radius.card)
                .border_1()
                .border_color(t.colors.border)
                .bg(t.colors.surface)
                .child(steps),
        )
        .children(summary.map(|(bg, text)| {
            div()
                .p(t.space.md)
                .rounded(t.radius.card)
                .bg(bg)
                .text_size(t.text.caption)
                .text_color(t.colors.text)
                .child(text)
        }))
        .child(kit::group(t, "The model itself"))
        .child(
            soon(
                div()
                    .id("capability-probe")
                    .flex()
                    .items_center()
                    .gap(t.space.md)
                    .px(px(14.))
                    .py(px(10.))
                    .rounded(t.radius.card)
                    .border_1()
                    .border_color(t.colors.border)
                    .cursor_pointer()
                    .child(icon("icons/sparkles.svg", t.size.icon, t.colors.text_muted))
                    .child(kit::card_text(
                        t,
                        SharedString::from("Streaming, tools, images, thinking"),
                        false,
                        Some("Checks what the model can do, one short request each.".into()),
                        None,
                    )),
                crate::stubs::CAPABILITY_PROBE,
            )
            .on_click(move |_, _, cx| {
                probe.update(cx, |s, cx| s.stub(crate::stubs::CAPABILITY_PROBE, cx))
            }),
        )
}
