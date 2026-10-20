//! Messages you sent while Forge works, under the working line until they
//! reach it (`AppState::queued`): each a pale bubble on your side, saying it
//! reaches Forge at its next step, with Edit (back into the box) and Remove.
//! Once Forge takes one, it moves into the transcript where the model read it.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, SharedString};

use super::button::Button;
use super::icon;
use crate::state::{AppState, Queued};
use crate::theme::Theme;

pub fn render(t: &Theme, state: &Entity<AppState>, chat: usize, queued: &[Queued]) -> AnyElement {
    if queued.is_empty() {
        return div().into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .items_end()
        .gap(t.space.sm)
        .pt(t.space.sm)
        .children(queued.iter().map(|q| bubble(t, state, chat, q)))
        .into_any_element()
}

fn bubble(t: &Theme, state: &Entity<AppState>, chat: usize, q: &Queued) -> AnyElement {
    let id = |name: &str| SharedString::from(format!("queued-{name}-{}", q.uuid));
    let (edit, remove) = (state.clone(), state.clone());
    let (uuid_edit, uuid_remove) = (q.uuid.clone(), q.uuid.clone());
    div()
        .max_w(relative(0.85))
        .flex()
        .flex_col()
        .gap(t.space.xs)
        .px(t.space.md)
        .py(t.space.sm)
        // Not sent yet: lighter than a sent message, on the inset.
        .bg(t.extra.inset)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(icon("icons/clock.svg", t.size.icon_xs, t.colors.text_muted))
                .child(div().flex_1().child("Queued · reaches Forge at its next step"))
                .child(
                    Button::icon(id("edit"), "icons/pencil.svg", "Edit")
                        .compact()
                        .on_click(move |_, window, app| {
                            edit.update(app, |s, cx| s.remove_queued(chat, uuid_edit.clone(), true, cx));
                            window.dispatch_action(Box::new(super::FocusComposer), app);
                        }),
                )
                .child(
                    Button::icon(id("remove"), "icons/x.svg", "Remove")
                        .compact()
                        .on_click(move |_, _, app| {
                            remove.update(app, |s, cx| s.remove_queued(chat, uuid_remove.clone(), false, cx))
                        }),
                ),
        )
        .child(
            div()
                .text_size(t.size.chat_text)
                .line_height(relative(t.size.line_height_chat))
                .text_color(t.colors.text_body)
                .child(q.text.clone()),
        )
        .min_w(px(220.))
        .into_any_element()
}
