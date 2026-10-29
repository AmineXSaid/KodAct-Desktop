//! Messages you sent while Forge works, under the working line until they
//! reach it (`AppState::queued`): each a pale bubble on your side, your words
//! first, then one quiet line saying it waits for Forge's next step, with
//! Send now (stop the reply and send it), Edit (back into the box) and
//! Remove. Once Forge takes one, it moves into the transcript where the
//! model read it.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, SharedString};

use super::button::Button;
use super::icon;
use crate::state::{AppState, Queued};
use crate::theme::Theme;

pub fn render(t: &Theme, state: &Entity<AppState>, chat: usize, queued: &[Queued]) -> AnyElement {
    if queued.is_empty() {
        return div().into_any_element();
    }
    let last = queued.len() - 1;
    div()
        .flex()
        .flex_col()
        .items_end()
        .gap(t.space.sm)
        .pt(t.space.sm)
        .children(
            queued
                .iter()
                .enumerate()
                .map(|(i, q)| bubble(t, state, chat, q, i == last)),
        )
        .into_any_element()
}

/// One queued message; the newest carries Send now, which sends them all.
fn bubble(
    t: &Theme,
    state: &Entity<AppState>,
    chat: usize,
    q: &Queued,
    newest: bool,
) -> AnyElement {
    let id = |name: &str| SharedString::from(format!("queued-{name}-{}", q.uuid));
    let (edit, remove, now) = (state.clone(), state.clone(), state.clone());
    let (uuid_edit, uuid_remove) = (q.uuid.clone(), q.uuid.clone());
    // The waiting line: what happens to it, then what you can do about it.
    let footer = div()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(icon("icons/clock.svg", t.size.icon_xs, t.colors.text_muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child("Waiting for Forge's next step"),
        )
        .when(newest, |d| {
            d.child(
                Button::ghost(id("now"), "Send now")
                    .compact()
                    .describe(format!(
                        "Stop the reply and send what you queued ({})",
                        super::shortcuts::keys("secondary-shift-enter").join("+")
                    ))
                    .tooltip(true)
                    .on_click(move |_, _, app| now.update(app, |s, cx| s.send_queued_now(cx))),
            )
        })
        .child(
            Button::icon(id("edit"), "icons/pencil.svg", "Edit")
                .compact()
                .on_click(move |_, window, app| {
                    edit.update(app, |s, cx| {
                        s.remove_queued(chat, uuid_edit.clone(), true, cx)
                    });
                    window.dispatch_action(Box::new(super::FocusComposer), app);
                }),
        )
        .child(
            Button::icon(id("remove"), "icons/x.svg", "Remove")
                .compact()
                .on_click(move |_, _, app| {
                    remove.update(app, |s, cx| {
                        s.remove_queued(chat, uuid_remove.clone(), false, cx)
                    })
                }),
        );
    div()
        .max_w(relative(0.85))
        // Wide enough that the waiting line never wraps.
        .min_w(px(340.))
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .pl(t.space.md)
        .pr(t.space.sm)
        .pt(t.space.sm)
        .pb(t.space.xs)
        // Not sent yet: lighter than a sent message, on the inset.
        .bg(t.extra.inset)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .when(!q.text.trim().is_empty(), |d| {
            d.child(
                div()
                    .pr(t.space.xs)
                    .text_size(t.size.chat_text)
                    .line_height(relative(t.size.line_height_chat))
                    .text_color(t.colors.text_body)
                    .child(q.text.clone()),
            )
        })
        // What it carries, as your sent message will show it.
        .children(super::attachments::sent(
            t,
            &q.attachments,
            &format!("queued-{}", q.uuid),
            state,
        ))
        .child(footer)
        .into_any_element()
}
