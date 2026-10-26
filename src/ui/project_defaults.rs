//! The Defaults card on a project page's rail: the model opening the
//! project picks, and the mode its new chats start in. "Yours" leaves the
//! choice to you, as it is everywhere else.

use gpui::{div, prelude::*, AnyElement, FontWeight, IntoElement, SharedString};

use super::editor_kit as kit;
use crate::modes::ModeId;
use crate::state::AppState;
use crate::theme::Theme;

pub fn card(t: &Theme, s: &AppState, id: usize, state: gpui::Entity<AppState>) -> AnyElement {
    let Some(p) = s.project(id) else {
        return div().into_any_element();
    };
    let (model, mode) = (p.model.clone(), p.default_mode);
    let label = |text: &str| {
        div()
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.text_muted)
            .child(text.to_string())
    };

    let mut models = div().flex().flex_wrap().gap(t.space.xs);
    let profiles: Vec<(String, String)> = s
        .endpoints
        .as_ref()
        .map(|e| {
            e.profiles
                .iter()
                .map(|p| (p.name.clone(), p.display().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let choices = std::iter::once((None, "Yours".to_string()))
        .chain(profiles.into_iter().map(|(n, d)| (Some(n), d)));
    for (i, (name, shown)) in choices.enumerate() {
        let on = model == name;
        let st = state.clone();
        let keep_mode = mode;
        models = models.child(
            kit::chip(t, SharedString::from(format!("def-model-{i}")), shown, on).on_click(
                move |_, _, cx| {
                    let name = name.clone();
                    st.update(cx, |s, cx| s.set_project_defaults(id, name, keep_mode, cx))
                },
            ),
        );
    }

    let mut modes = div().flex().flex_wrap().gap(t.space.xs);
    let mode_choices = std::iter::once((None, "Yours")).chain(
        ModeId::ALL
            .into_iter()
            .filter(|m| *m != ModeId::Bypass || s.bypass_allowed)
            .map(|m| (Some(m), m.label())),
    );
    for (i, (m, shown)) in mode_choices.enumerate() {
        let on = mode == m;
        let st = state.clone();
        let keep_model = model.clone();
        modes = modes.child(
            kit::chip(t, SharedString::from(format!("def-mode-{i}")), shown, on).on_click(
                move |_, _, cx| {
                    let keep = keep_model.clone();
                    st.update(cx, |s, cx| s.set_project_defaults(id, keep, m, cx))
                },
            ),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .p(t.space.lg)
        .rounded(t.radius.card)
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .child(
            div()
                .text_size(t.text.caption)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_muted)
                .child("DEFAULTS"),
        )
        .child(label("Model"))
        .child(models)
        .child(label("Mode for new chats"))
        .child(modes)
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child("Opening the project picks its model; its new chats start in its mode."),
        )
        .into_any_element()
}
