//! The pieces every Settings editor of ForgeCLI's files is built from, so
//! Slash Commands, Agents, Skills, Hooks, MCP Servers and Permissions read as
//! one family: a header with the one primary action, a list of cards, an
//! inline delete confirm, labelled fields, and the empty, loading and error
//! states.

use gpui::component::input::{Input, InputState, Textarea, TextareaState};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Entity, FontWeight, Hsla, IntoElement, SharedString,
};

use super::*;
use crate::theme::{self, Theme};

/// A section's title and its one-line explanation, with its action on the right.
pub fn header(t: &Theme, title: &str, hint: &str, action: Option<AnyElement>) -> gpui::Div {
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
                .gap(px(2.))
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .line_height(relative(1.4))
                        .child(hint.to_string()),
                ),
        )
        .children(action)
}

/// A small group heading inside a list ("You", "Project").
pub fn group(t: &Theme, label: impl Into<SharedString>) -> gpui::Div {
    div()
        .pt(t.space.sm)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .child(label.into())
}

/// A list item's card.
pub fn card(t: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(t.space.md)
        .px(px(14.))
        .py(px(10.))
        .rounded(t.radius.card)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
}

/// A card's title line and its detail lines.
pub fn card_text(
    t: &Theme,
    title: impl Into<SharedString>,
    mono: bool,
    detail: Option<String>,
    note: Option<(Hsla, String)>,
) -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.body_sm)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_strong)
                .when(mono, |d| d.font_family(t.font_mono.clone()))
                .child(title.into()),
        )
        .children(detail.filter(|d| !d.is_empty()).map(|d| {
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(d)
        }))
        .children(note.map(|(c, n)| {
            div()
                .text_size(t.text.caption)
                .text_color(c)
                .line_height(relative(1.4))
                .child(n)
        }))
}

/// The empty state: what goes here and how to add the first one.
pub fn empty(t: &Theme, text: &str) -> gpui::Div {
    div()
        .p(px(18.))
        .rounded(t.radius.card)
        .border_1()
        .border_color(t.colors.border)
        .text_size(t.text.body_sm)
        .text_color(t.colors.text_muted)
        .line_height(relative(1.45))
        .child(text.to_string())
}

/// While the files are first read.
pub fn loading(t: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(icon(
            "icons/loader-circle.svg",
            t.size.icon_xs,
            t.colors.text_muted,
        ))
        .child("Reading ForgeCLI's files…")
}

/// Files that couldn't be read: named, so they can be fixed.
pub fn problems(t: &Theme, list: &[String]) -> Option<gpui::Div> {
    (!list.is_empty()).then(|| {
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(t.space.md)
            .rounded(t.radius.card)
            .bg(t.extra.danger_tint)
            .children(list.iter().take(6).map(|p| {
                div()
                    .flex()
                    .items_start()
                    .gap(px(6.))
                    .text_size(t.text.caption)
                    .text_color(t.extra.danger)
                    .line_height(relative(1.4))
                    .child(div().pt(px(1.)).child(icon(
                        "icons/triangle-alert.svg",
                        t.size.icon_xs,
                        t.extra.danger,
                    )))
                    .child(div().flex_1().min_w_0().child(p.clone()))
            }))
    })
}

/// A form's message: an error to fix, or what happened.
pub fn status(t: &Theme, error: &Option<SharedString>) -> Option<gpui::Div> {
    error.as_ref().map(|e| {
        div()
            .flex()
            .items_start()
            .gap(px(6.))
            .text_size(t.text.caption)
            .text_color(t.extra.danger)
            .line_height(relative(1.4))
            .child(div().pt(px(1.)).child(icon(
                "icons/circle-x.svg",
                t.size.icon_xs,
                t.extra.danger,
            )))
            .child(div().flex_1().child(e.clone()))
    })
}

/// A field's label, with an optional hint under the box.
pub fn field(t: &Theme, label: &str, hint: Option<&str>, control: AnyElement) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .text_size(t.text.caption)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_muted)
                .child(label.to_string()),
        )
        .child(control)
        .children(hint.map(|h| {
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .line_height(relative(1.4))
                .child(h.to_string())
        }))
}

/// A one-line text box.
pub fn line(t: &Theme, input: &Entity<InputState>, mono: bool) -> AnyElement {
    div()
        .h(px(32.))
        .px(px(2.))
        .flex()
        .items_center()
        .rounded(t.radius.button)
        .bg(t.colors.canvas)
        .border_1()
        .border_color(t.colors.border)
        .when(mono, |d| d.font_family(t.font_mono.clone()))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(input)
                    .appearance(false)
                    .text_size(t.text.body_sm),
            ),
        )
        .into_any_element()
}

/// A box for a prompt or instructions.
pub fn area(t: &Theme, input: &Entity<TextareaState>, mono: bool) -> AnyElement {
    div()
        .rounded(t.radius.button)
        .bg(t.colors.canvas)
        .border_1()
        .border_color(t.colors.border)
        .p(t.space.sm)
        .text_size(t.text.body_sm)
        .when(mono, |d| d.font_family(t.font_mono.clone()))
        .child(Textarea::new(input).appearance(false).h(t.size.prompt_box))
        .into_any_element()
}

/// A choice chip (scope, event, transport).
pub fn chip(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .h(t.size.control)
        .px(px(10.))
        .rounded(t.radius.compact)
        .border_1()
        .border_color(if on {
            t.colors.primary
        } else {
            t.colors.border
        })
        .bg(if on {
            t.colors.primary_tint
        } else {
            t.colors.surface
        })
        .text_size(t.text.caption)
        .text_color(if on { t.colors.primary } else { t.colors.text })
        .cursor_pointer()
        .when(!on, |d| d.hover(move |s| s.bg(hover)))
        .child(label.into())
}

/// A round icon button on a card (edit, delete…), with its tooltip.
pub fn icon_button(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    path: &'static str,
    tip: &'static str,
    danger: bool,
) -> gpui::Stateful<gpui::Div> {
    let h = if danger {
        t.extra.danger_tint
    } else {
        t.colors.hover
    };
    div()
        .id(id)
        .size(t.size.control)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |s| s.bg(h))
        .child(icon(path, t.size.icon_sm, t.colors.text_muted))
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
}

/// A card turned into "Delete …?": what goes, and the two answers.
pub fn confirm_row(
    t: &Theme,
    question: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    cancel: gpui::Stateful<gpui::Div>,
    delete: gpui::Stateful<gpui::Div>,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(t.space.md)
        .px(px(14.))
        .py(px(10.))
        .rounded(t.radius.card)
        .border_1()
        .border_color(t.extra.danger.opacity(0.5))
        .bg(t.extra.danger_tint)
        .child(card_text(
            t,
            question,
            false,
            Some(detail.into().to_string()),
            None,
        ))
        .child(cancel)
        .child(delete)
}

/// The two buttons that end a form.
pub fn cancel_button(t: &Theme, id: &'static str) -> gpui::Stateful<gpui::Div> {
    theme::text_button(t, "Cancel")
        .text_size(t.text.body_sm)
        .id(id)
}

pub fn save_button(t: &Theme, id: &'static str, saving: bool) -> gpui::Stateful<gpui::Div> {
    theme::primary_button(t, if saving { "Saving…" } else { "Save" })
        .id(id)
        .when(saving, |d| d.opacity(0.6))
}

/// "← Back" above a form.
pub fn back(t: &Theme, id: &'static str, label: &str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(4.))
        .cursor_pointer()
        .text_size(t.text.caption)
        .text_color(t.colors.primary)
        .child(format!("← {label}"))
}

/// A small link-style action ("Open the folder", "Check again").
pub fn link(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    path: &'static str,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(5.))
        .cursor_pointer()
        .text_size(t.text.caption)
        .text_color(t.colors.primary)
        .child(icon(path, t.size.icon_xs, t.colors.primary))
        .child(label.into())
}

/// The scope pill on a card.
pub fn scope_pill(t: &Theme, label: &'static str) -> gpui::Div {
    theme::pill(t, t.colors.hover, t.colors.text_muted, label).flex_none()
}

/// A path as people read it: relative inside the project, `~/…` under the
/// home folder.
pub fn short(path: &std::path::Path, project: &std::path::Path) -> String {
    match path.strip_prefix(project) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => crate::rules::display_path(path, crate::rules::home_dir().as_deref()),
    }
}

/// Open a folder in the system's file manager (made first, if missing).
pub fn reveal(dir: &std::path::Path, cx: &mut gpui::App) {
    let _ = std::fs::create_dir_all(dir);
    cx.open_with_system(dir);
}

/// One settings key as each file holds it (yours, the project's, this
/// PC's), for the editors of a single key.
pub type KeyByLayer = Vec<(
    crate::forge_files::Layer,
    Result<Option<serde_json::Value>, String>,
)>;

/// Read `key` from the three settings files (blocking: call off the UI thread).
pub fn read_key(project: &std::path::Path, key: &str) -> KeyByLayer {
    crate::forge_files::Layer::ALL
        .iter()
        .map(|l| (*l, crate::forge_files::setting(project, *l, key)))
        .collect()
}

/// The three settings files as chips, with what each one reaches.
pub fn layer_chips<V: 'static>(
    t: &Theme,
    id: &'static str,
    current: crate::forge_files::Layer,
    cx: &mut gpui::Context<V>,
    pick: impl Fn(&mut V, crate::forge_files::Layer, &mut gpui::Context<V>) + Copy + 'static,
) -> gpui::Div {
    use crate::forge_files::Layer;
    let mut row = div().flex().flex_wrap().gap(px(6.));
    for (layer, label) in [
        (Layer::Local, "This project, this PC"),
        (Layer::Project, "This project, shared"),
        (Layer::User, "Every project"),
    ] {
        row = row.child(
            chip(
                t,
                SharedString::from(format!("{id}-{layer:?}")),
                label,
                current == layer,
            )
            .on_click(cx.listener(move |this, _, _, cx| pick(this, layer, cx))),
        );
    }
    row
}
