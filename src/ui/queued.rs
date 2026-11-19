//! Docked panel above the composer holding messages sent while KodAct works.
//!
//! Styled after Antigravity's Queued Messages bar: header with queue count badge,
//! `"Sends after agent finishes working"`, collapsible item list, attachment tiles,
//! and inline **Send now**, **Edit**, and **Remove** actions.

use gpui::{div, img, prelude::*, px, AnyElement, Entity, FontWeight, ObjectFit, SharedString};

use super::button::Button;
use super::icon;
use crate::attach::Attachment;
use crate::state::{AppState, Queued};
use crate::theme::Theme;

pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    chat: usize,
    queued: &[Queued],
    collapsed: bool,
) -> AnyElement {
    if queued.is_empty() {
        return div().into_any_element();
    }
    let collapse_state = state.clone();
    let count = queued.len();
    let hover = t.colors.hover;
    let subtitle = if count == 1 {
        "Sends after agent finishes working"
    } else {
        "Send in order after agent finishes working"
    };

    let header = div()
        .id("queued-dock-header")
        .flex()
        .items_center()
        .justify_between()
        .gap(t.space.sm)
        .px(t.space.md)
        .py(t.space.xs + px(2.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(move |_, _, app| {
            collapse_state.update(app, |s, cx| {
                s.toggle_ui("queued-collapsed".to_string(), Some(chat), cx)
            });
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(
                    div()
                        .flex_none()
                        .text_size(t.text.body_sm)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child("Queued Messages"),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w(px(18.))
                        .h(px(18.))
                        .px(px(5.))
                        .rounded(t.radius.pill)
                        .bg(t.colors.hover)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text)
                        .child(count.to_string()),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(subtitle),
                ),
        )
        .child(icon(
            if collapsed {
                "icons/chevron-down.svg"
            } else {
                "icons/chevron-up.svg"
            },
            t.size.icon_sm,
            t.colors.text_subtle,
        ));

    div()
        .w_full()
        .flex_none()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_grain())
        .overflow_hidden()
        .child(header)
        .when(!collapsed, |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .children(queued.iter().map(|q| row(t, state, chat, q))),
            )
        })
        .into_any_element()
}

fn row(t: &Theme, state: &Entity<AppState>, chat: usize, q: &Queued) -> AnyElement {
    let id = |name: &str| SharedString::from(format!("queued-{name}-{}", q.uuid));
    let (send_now, edit, remove) = (state.clone(), state.clone(), state.clone());
    let (uuid_send, uuid_edit, uuid_remove) = (q.uuid.clone(), q.uuid.clone(), q.uuid.clone());
    let text = q.text.trim();
    let preview_text = if text.is_empty() {
        format!(
            "{} attachment{}",
            q.attachments.len(),
            if q.attachments.len() == 1 { "" } else { "s" }
        )
    } else {
        text.to_string()
    };

    div()
        .flex()
        .items_start()
        .gap(t.space.sm)
        .px(t.space.md)
        .py(t.space.sm)
        .border_t_1()
        .border_color(t.colors.border)
        .when(!q.attachments.is_empty(), |d| {
            d.child(attachment_tiles(t, &q.attachments))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(t.text.body_sm)
                .line_height(gpui::relative(1.42))
                .text_color(t.colors.text)
                .line_clamp(3)
                .child(preview_text),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(2.))
                .child(
                    Button::icon(id("send"), "icons/arrow-right.svg", "Send now")
                        .compact()
                        .on_click(move |_, _, app| {
                            send_now.update(app, |s, cx| {
                                s.send_queued_now(chat, uuid_send.clone(), cx)
                            });
                        }),
                )
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
                    Button::icon(id("remove"), "icons/trash.svg", "Remove")
                        .compact()
                        .on_click(move |_, _, app| {
                            remove.update(app, |s, cx| {
                                s.remove_queued(chat, uuid_remove.clone(), false, cx)
                            })
                        }),
                ),
        )
        .into_any_element()
}

fn attachment_tiles(t: &Theme, attachments: &[Attachment]) -> AnyElement {
    const MAX_TILES: usize = 2;
    let extra = attachments.len().saturating_sub(MAX_TILES);
    let tile = |a: &Attachment| {
        let thumb = match super::attachments::thumbnail(a) {
            Some(image) => img(image)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => icon(a.icon(), t.size.icon_xs, t.colors.text_muted).into_any_element(),
        };
        div()
            .flex_none()
            .size(px(28.))
            .rounded(t.radius.compact)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.extra.sunk)
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .child(thumb)
    };

    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.))
        .pt(px(2.))
        .children(attachments.iter().take(MAX_TILES).map(tile))
        .when(extra > 0, |d| {
            d.child(
                div()
                    .flex_none()
                    .h(px(28.))
                    .min_w(px(28.))
                    .px(px(5.))
                    .rounded(t.radius.compact)
                    .border_1()
                    .border_color(t.colors.border)
                    .bg(t.extra.sunk)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_muted)
                    .child(format!("+{extra}")),
            )
        })
        .into_any_element()
}
