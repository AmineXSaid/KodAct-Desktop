//! The Modes menu, as KodAct for VS Code has it: a footer button (the mode's
//! icon and name) that opens a menu above it, right-aligned. A "Modes"
//! header with the Shift+Tab hint, then one row per mode: icon, name over
//! its description, and a check on the current one. Picking Bypass the
//! first time shows its warning in the menu before it is allowed.

use gpui::{div, prelude::*, px, App, Entity, FontWeight, Hsla, IntoElement, SharedString};

use super::icon;
use crate::modes::ModeId;
use crate::state::AppState;
use crate::theme::Theme;

/// A mode's tint (Manual has none), as the extension tints its footer,
/// focus ring and send button.
pub fn tint(t: &Theme, mode: ModeId) -> Option<Hsla> {
    match mode {
        ModeId::Manual => None,
        ModeId::AcceptEdits => Some(t.extra.mode_edit),
        ModeId::Plan => Some(t.extra.mode_plan),
        ModeId::Bypass => Some(t.extra.mode_bypass),
    }
}

/// `⇧`, `tab`: a key in the header hint.
fn key(t: &Theme, text: &'static str) -> impl IntoElement {
    div()
        .px(px(4.))
        .py(px(1.))
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.small)
        .child(text)
}

/// A compact button for the warning's two answers: both the same height
/// and text, the filled one in `fill`, the other a quiet text button.
fn small_button(
    t: &Theme,
    id: &'static str,
    label: &'static str,
    fill: Option<Hsla>,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .h(px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(t.radius.button)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .map(|d| match fill {
            Some(c) => d
                .bg(c)
                .text_color(t.colors.primary_fg)
                .hover(|st| st.opacity(0.9)),
            None => d.text_color(t.colors.text).hover(move |st| st.bg(hover)),
        })
        .child(label)
}

/// The footer button. `skip` says a click is the one that just closed the
/// menu (pressing the button while it's open), so it doesn't reopen it.
pub fn trigger(
    t: &Theme,
    state: &Entity<AppState>,
    cx: &App,
    skip: impl Fn(&gpui::ClickEvent, &mut App) -> bool + 'static,
) -> impl IntoElement {
    let s = state.read(cx);
    let (mode, open) = (s.mode, s.modes_open);
    let fg = tint(t, mode).unwrap_or(t.colors.text);
    let state = state.clone();
    super::button::selector(t, "mode-trigger", open)
        .aria_label("Permission mode")
        .text_color(fg)
        .child(icon(mode.icon(), t.size.icon_sm, fg))
        .child(
            div()
                .max_w(px(200.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(mode.label()),
        )
        .child(super::button::chevron(t, open))
        // Not over its own open menu.
        .when(!open, |d| {
            let tip: SharedString = format!(
                "{}. Click to change, or press Shift+Tab to cycle.",
                mode.description()
            )
            .into();
            d.tooltip(move |window, cx| {
                gpui::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
            })
        })
        .on_click(move |ev, _, cx| {
            if skip(ev, cx) {
                return;
            }
            state.update(cx, |s, cx| {
                s.modes_open = !s.modes_open;
                s.bypass_warning = false;
                s.picker_open = false;
                s.design_menu = None;
                s.project_menu = false;
                s.context_open = false;
                s.title_menu = None;
                s.title_menu_dismissed = None;
                cx.notify();
            })
        })
}

/// The menu, above the trigger. A press anywhere outside it calls `on_out`.
pub fn menu(
    t: &Theme,
    state: &Entity<AppState>,
    window: &gpui::Window,
    cx: &App,
    on_out: impl Fn(gpui::Point<gpui::Pixels>, &mut App) + 'static,
    leaving: bool,
) -> impl IntoElement {
    let s = state.read(cx);
    let (current, allowed, warning) = (s.mode, s.bypass_allowed, s.bypass_warning);
    let active_bg = t.colors.hover;
    let mut rows = div().flex().flex_col().gap(px(2.));
    for mode in ModeId::ALL {
        let picked = mode == current;
        let st = state.clone();
        let row_fg = tint(t, mode).unwrap_or(t.colors.text);
        rows = rows.child(
            div()
                .id(SharedString::from(format!("mode-{}", mode.label())))
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
                .child(icon(mode.icon(), t.size.icon_lg, row_fg))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().text_color(t.colors.text_strong).font_weight(if picked { FontWeight::MEDIUM } else { FontWeight::NORMAL }).child(mode.label()))
                        .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(match mode {
                            ModeId::Bypass if !allowed => "Asks you first. KodAct will not ask for approval before running potentially dangerous commands",
                            m => m.description(),
                        })),
                )
                .child(div().w(px(14.)).flex_none().when(picked, |d| d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.text))))
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.set_mode(mode, cx))),
        );
    }

    let warning_card = warning.then(|| {
        let (allow, cancel) = (state.clone(), state.clone());
        let danger = t.extra.mode_bypass;
        div()
            .mx(px(8.))
            .mt(px(6.))
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(t.radius.button)
            .bg(t.extra.danger_tint)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(icon("icons/triangle-alert.svg", t.size.icon_sm, danger))
                    .child(div().flex_1().min_w_0().text_size(t.text.caption).text_color(t.colors.text).child(
                        "Bypass permissions lets KodAct run any command and edit any file without asking, including commands that delete files or reach the network. Use it only in a sandbox or a throwaway copy.",
                    )),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(6.))
                    .child(small_button(t, "bypass-cancel", "Cancel", None).on_click(move |_, _, cx| {
                        cancel.update(cx, |s, cx| {
                            s.bypass_warning = false;
                            cx.notify();
                        })
                    }))
                    .child(small_button(t, "bypass-allow", "Allow bypass", Some(t.extra.fill_danger)).on_click(move |_, _, cx| allow.update(cx, |s, cx| s.allow_bypass(cx)))),
            )
    });

    // It opens upward from its chip and is kept inside the window: moved
    // down when the chip sits high, and scrolling when the window is short.
    let menu = crate::theme::fit_window(
        div().id("modes-menu"),
        window,
        t,
        t.size.viewport_margin,
        None,
    )
    .when(!leaving, |d| {
        d.on_mouse_down_out(move |ev, _, cx| on_out(ev.position, cx))
    })
    .relative()
    .w(px(320.))
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
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child("Modes"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .text_size(t.text.caption - px(1.))
                    .text_color(t.colors.text_subtle)
                    .opacity(0.8)
                    .child(key(t, "⇧"))
                    .child("+")
                    .child(key(t, "tab"))
                    .child("to switch"),
            ),
    )
    .child(rows)
    .children(warning_card);
    div().absolute().top_0().right_0().child(
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::BottomRight)
                .snap_to_window_with_margin(t.size.viewport_margin)
                .child(
                    div()
                        .pb(px(8.))
                        .child(super::motion::composer_menu(menu, "modes", 0., leaving, t)),
                ),
        )
        .with_priority(2),
    )
}
