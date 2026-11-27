//! The Reasoning chip, beside the model's: how much the model thinks before
//! it answers. Its menu says what asking the model found
//! (`reasoning_detect`): the levels it takes, that it always reasons at one
//! level, or that it doesn't reason. Auto and each level it takes are rows;
//! "Check again" asks the model once more.

use gpui::{div, prelude::*, px, App, Entity, FontWeight, IntoElement, SharedString};

use super::icon;
use crate::endpoints::Effort;
use crate::reasoning_detect::Reasoning;
use crate::state::AppState;
use crate::theme::Theme;

/// What the chip says now.
fn label(s: &AppState) -> SharedString {
    let Some(p) = s.active_profile() else {
        return "Reasoning".into();
    };
    if s.reasoning_checking.contains(&p.name) {
        return "Checking…".into();
    }
    match &p.capabilities.reasoning {
        None => "Reasoning".into(),
        Some(Reasoning::None) => "No reasoning".into(),
        Some(Reasoning::Fixed) => "Always on".into(),
        Some(Reasoning::Levels(levels)) => match s.effort.filter(|e| levels.contains(e)) {
            Some(e) => e.label().into(),
            None => "Auto".into(),
        },
    }
}

/// The chip. `skip` says a click is the one that just closed the menu.
pub fn trigger(
    t: &Theme,
    state: &Entity<AppState>,
    cx: &App,
    skip: impl Fn(&gpui::ClickEvent, &mut App) -> bool + 'static,
) -> impl IntoElement {
    let s = state.read(cx);
    let open = s.reasoning_open;
    let text = label(s);
    // A high level shows as the model menu tinted it.
    let fg = match s.effort {
        Some(Effort::Xhigh) if text.as_ref() == Effort::Xhigh.label() => t.extra.warning,
        Some(Effort::Max) if text.as_ref() == Effort::Max.label() => t.extra.danger,
        _ => t.colors.text_muted,
    };
    let state = state.clone();
    super::button::selector(t, "reasoning-trigger", open)
        .flex_none()
        .aria_label(SharedString::from(format!("Reasoning: {text}")))
        .child(icon("icons/reasoning.svg", t.size.icon_sm, t.colors.text_muted))
        .child(div().text_color(fg).whitespace_nowrap().child(text))
        .child(super::button::chevron(t, open))
        .when(!open, |d| {
            d.tooltip(|window, cx| {
                gpui::component::tooltip::Tooltip::new(
                    "How much the model thinks before it answers",
                )
                .build(window, cx)
            })
        })
        .on_click(move |ev, _, cx| {
            if skip(ev, cx) {
                return;
            }
            state.update(cx, |s, cx| {
                s.reasoning_open = !s.reasoning_open;
                s.modes_open = false;
                s.picker_open = false;
                s.design_menu = None;
                s.project_menu = false;
                s.context_open = false;
                cx.notify();
            })
        })
}

/// One row: its name over a line, and a check when it is the one in use.
fn row(
    t: &Theme,
    id: SharedString,
    name: &'static str,
    note: &'static str,
    picked: bool,
) -> gpui::Stateful<gpui::Div> {
    let active_bg = t.colors.hover;
    div()
        .id(id)
        .mx(px(4.))
        .px(px(8.))
        .py(px(5.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(t.radius.button)
        .cursor_pointer()
        .when(picked, |d| d.bg(active_bg))
        .hover(move |d| d.bg(active_bg))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_color(t.colors.text_strong)
                        .font_weight(if picked { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                        .child(name),
                )
                .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(note)),
        )
        .child(
            div()
                .w(px(14.))
                .flex_none()
                .when(picked, |d| d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.text))),
        )
}

fn note(e: Effort) -> &'static str {
    match e {
        Effort::Low => "Quick answers, little thinking",
        Effort::Medium => "A balance of speed and depth",
        Effort::High => "Thinks it through",
        Effort::Xhigh => "Long thinking for hard problems",
        Effort::Max => "As much thinking as it takes",
    }
}

/// The menu, above the chip. A press anywhere outside it calls `on_out`.
pub fn menu(
    t: &Theme,
    state: &Entity<AppState>,
    window: &gpui::Window,
    cx: &App,
    on_out: impl Fn(gpui::Point<gpui::Pixels>, &mut App) + 'static,
    leaving: bool,
) -> impl IntoElement {
    let s = state.read(cx);
    let profile = s.active_profile();
    let model: SharedString = profile
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No model".into())
        .into();
    let checking = profile.is_some_and(|p| s.reasoning_checking.contains(&p.name));
    let found = profile.and_then(|p| p.capabilities.reasoning.clone());
    let summary: SharedString = if checking {
        "Asking the model: a few short requests…".into()
    } else {
        match &found {
            Some(r) => r.summary().into(),
            None => "Not checked yet.".into(),
        }
    };
    let levels = found.as_ref().map(Reasoning::levels).unwrap_or_default();
    let current = s.effort.filter(|e| levels.contains(e));

    let mut rows = div().flex().flex_col().gap(px(2.));
    if !levels.is_empty() {
        let st = state.clone();
        rows = rows.child(
            row(t, "reasoning-auto".into(), "Auto", "The model decides", current.is_none())
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.set_effort(None, cx))),
        );
        for e in levels {
            let st = state.clone();
            rows = rows.child(
                row(t, SharedString::from(format!("reasoning-{}", e.cli_name())), e.label(), note(e), current == Some(e))
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.set_effort(Some(e), cx))),
            );
        }
    }

    let st = state.clone();
    let hover = t.colors.hover;
    let check_again = div()
        .id("reasoning-refresh")
        .mx(px(4.))
        .mt(px(4.))
        .px(px(8.))
        .py(px(6.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(t.radius.button)
        .text_color(t.colors.text)
        .when(checking, |d| d.opacity(0.5))
        .when(!checking, |d| d.cursor_pointer().hover(move |st| st.bg(hover)))
        .child(icon("icons/refresh-cw.svg", t.size.icon_sm, t.colors.text_muted))
        .child(if checking { "Checking…" } else { "Check again" })
        .when(!checking, |d| {
            d.on_click(move |_, _, cx| st.update(cx, |s, cx| s.detect_reasoning(cx)))
        });

    let menu = crate::theme::fit_window(div().id("reasoning-menu"), window, t, t.size.viewport_margin, None)
        .when(!leaving, |d| d.on_mouse_down_out(move |ev, _, cx| on_out(ev.position, cx)))
        .relative()
        .w(px(300.))
        .pt(px(6.))
        .pb(px(4.))
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_popover())
        .occlude()
        .text_size(t.text.body_sm)
        .child(
            div()
                .px(px(12.))
                .py(px(4.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(format!("Reasoning · {model}"))),
                )
                .child(div().text_size(t.text.caption).text_color(t.colors.text).child(summary)),
        )
        .child(rows)
        .child(div().mx(px(12.)).mt(px(4.)).h(px(1.)).bg(t.colors.border))
        .child(check_again);
    // Above its chip, its right edge on the chip's: the chip sits at the
    // composer's right, beside the model's.
    div().absolute().top_0().right_0().child(
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::BottomRight)
                .snap_to_window_with_margin(t.size.viewport_margin)
                .child(div().pb(px(8.)).child(super::motion::composer_menu(menu, "reasoning", 0., leaving, t))),
        )
        .with_priority(2),
    )
}
