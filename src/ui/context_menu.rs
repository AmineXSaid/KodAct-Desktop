//! The context window, opened from the composer's meter (or "Show context"):
//! how much of the model's window this conversation uses, what fills it
//! (your messages, replies, tool output, thinking, files: a split of the
//! reported total, counted from the text), how it grew turn by turn, and
//! Compact, which has KodAct CLI summarize the conversation to free room.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::context::{self, short};
use crate::state::AppState;
use crate::theme::{self, Theme};

/// The popover, above the composer's controls.
pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    win: &gpui::Window,
    // Some(leaving) while it shows or leaves.
    shown: Option<bool>,
    cx: &gpui::App,
) -> Option<AnyElement> {
    let leaving = shown?;
    let s = state.read(cx);
    let chat = s.chat()?;
    let profile = s.active_profile();
    let window = profile.and_then(|p| p.context_window(s.health.get(&p.name)));
    let usage = context::last_usage(&chat.messages);
    let used = usage.map(|u| u.total());
    let split = context::breakdown(&chat.messages);
    let turns = context::turns(&chat.messages);
    let model: SharedString = profile
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No model chosen".into())
        .into();

    let fraction = match (used, window) {
        (Some(u), Some(w)) if w > 0 => Some(u as f32 / w as f32),
        _ => None,
    };
    let tone = |f: f32| {
        if f > 1. {
            t.extra.danger
        } else if f > 0.8 {
            t.extra.warning
        } else {
            t.colors.primary
        }
    };
    let headline: String = match (used, window) {
        (Some(u), Some(w)) => format!("{} of {} tokens", short(u), short(w)),
        (Some(u), None) => format!("{} tokens; this model's window isn't known", short(u)),
        (None, _) if chat.messages.iter().any(|m| matches!(m, crate::state::Message::Notice { text, .. } if text.as_ref() == crate::state::COMPACTED)) => "Compacted. The next reply reports the new size.".into(),
        (None, _) => "Not measured yet: the next reply reports it.".into(),
    };

    // The bar: one track, filled to the share used.
    let bar = |f: f32, c: gpui::Hsla| {
        div()
            .h(px(6.))
            .w_full()
            .rounded(t.radius.pill)
            .bg(t.colors.hover)
            .child(
                div()
                    .h_full()
                    .w(relative(f.clamp(0., 1.)))
                    .rounded(t.radius.pill)
                    .bg(c),
            )
    };

    let mut kinds = div().flex().flex_col().gap(px(6.));
    let total = used.unwrap_or(split.total());
    for (label, n) in split.scaled(total) {
        if n == 0 {
            continue;
        }
        let f = n as f32 / total.max(1) as f32;
        kinds = kinds.child(
            div()
                .flex()
                .items_center()
                .gap(t.space.md)
                .child(
                    div()
                        .w(px(150.))
                        .flex_none()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text)
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(bar(f, t.colors.text_subtle)))
                .child(
                    div()
                        .w(px(56.))
                        .flex_none()
                        .text_right()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(format!("~{}", short(n))),
                ),
        );
    }

    let mut growth = div()
        .id("context-turns")
        .flex()
        .flex_col()
        .gap(px(4.))
        // Its card opens upward from the composer, the totals above the list.
        .max_h(px(132.).min(crate::theme::fit_height(
            win,
            t,
            t.size.titlebar + t.size.composer_clearance * 2.,
        )))
        .overflow_y_scroll();
    let peak = turns
        .iter()
        .map(|(_, u)| u.total())
        .max()
        .unwrap_or(1)
        .max(window.unwrap_or(0));
    for (i, (asked, u)) in turns.iter().enumerate().rev().take(12) {
        growth = growth.child(
            div()
                .flex()
                .items_center()
                .gap(t.space.md)
                .child(
                    div()
                        .w(px(18.))
                        .flex_none()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(format!("{}", i + 1)),
                )
                .child(
                    div()
                        .w(px(132.))
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(if asked.is_empty() {
                            "—".to_string()
                        } else {
                            asked.clone()
                        }),
                )
                .child(div().flex_1().min_w_0().child(bar(
                    u.total() as f32 / peak.max(1) as f32,
                    t.colors.primary.opacity(0.55),
                )))
                .child(
                    div()
                        .w(px(56.))
                        .flex_none()
                        .text_right()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(short(u.total())),
                ),
        );
    }

    let (compact, close) = (state.clone(), state.clone());
    let section = |title: &'static str| {
        div()
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.text_subtle)
            .child(title)
    };
    let hover = t.colors.hover;
    Some(super::motion::composer_menu(
        theme::popover(t)
            .id("context-window")
            .occlude()
            .absolute()
            .bottom_full()
            .left_0()
            .w(px(420.))
            .p(t.space.lg)
            .flex()
            .flex_col()
            .gap(t.space.md)
            .on_mouse_down_out(move |_, _, cx| {
                close.update(cx, |s, cx| {
                    if s.context_open {
                        s.context_open = false;
                        cx.notify();
                    }
                })
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(icon("icons/model.svg", t.size.icon_sm, t.colors.text_muted))
                    .child(div().flex_1().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child("Context window"))
                    .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(model)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().flex().items_baseline().gap(t.space.sm).child(div().text_size(t.text.subheading).text_color(fraction.map(tone).unwrap_or(t.colors.text_strong)).child(fraction.map(|f| format!("{:.0}%", (f * 100.).round())).unwrap_or_else(|| "—".into()))).child(div().text_size(t.text.caption).text_color(t.colors.text_muted).child(headline)))
                    .children(fraction.map(|f| bar(f, tone(f))))
                    .when(usage.is_some_and(|u| u.estimated), |d| d.child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child("Counted from the text: this reply's model reported no usage."))),
            )
            .when(split.total() > 0, |d| d.child(section("What fills it (approximate)")).child(kinds))
            .when(!turns.is_empty(), |d| d.child(section("How it grew, turn by turn")).child(growth))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.md)
                    .pt(t.space.xs)
                    .border_t_1()
                    .border_color(t.colors.border)
                    .child(div().flex_1().min_w_0().pt(t.space.sm).text_size(t.text.caption).text_color(t.colors.text_subtle).line_height(relative(1.4)).child("Compacting summarizes the conversation so far, so it takes far less room. KodAct keeps what was decided."))
                    .child(
                        div()
                            .id("compact")
                            .mt(t.space.sm)
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .px(t.space.md)
                            .py(px(6.))
                            .rounded(t.radius.button)
                            .border_1()
                            .border_color(t.colors.border)
                            .cursor_pointer()
                            .hover(move |d| d.bg(hover))
                            .text_size(t.text.body_sm)
                            .font_weight(FontWeight::MEDIUM)
                            .child(icon("icons/minus.svg", t.size.icon_sm, t.colors.text_muted))
                            .child("Compact")
                            .on_click(move |_, _, cx| compact.update(cx, |s, cx| s.compact(cx))),
                    ),
            )
            .relative(),
        "context-window",
        8.,
        leaving,
        t,
    ))
}
