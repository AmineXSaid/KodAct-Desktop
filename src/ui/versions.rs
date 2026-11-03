//! Earlier versions of a conversation: when you edit or retry a message, the
//! conversation from there on is replaced, and the old one is kept. Under
//! your message a small clock says how many there are; it opens a list of
//! them (when, what you asked, the start of the reply) to read, each with
//! Restore.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::state::AppState;
use crate::theme::{self, Theme};

/// One earlier version, as listed.
#[derive(Clone, Debug)]
pub struct Row {
    /// Its index in the chat's versions.
    pub index: usize,
    pub when: std::time::SystemTime,
    pub asked: String,
    pub preview: String,
}

/// `just now`, `4 min ago`, `2 h ago`, `3 d ago`.
pub fn ago(t: std::time::SystemTime) -> String {
    let s = t.elapsed().map(|d| d.as_secs()).unwrap_or(0);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86_400),
    }
}

/// The clock under your message, and its list when open.
pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    chat: usize,
    msg: usize,
    rows: &[Row],
    open: bool,
) -> AnyElement {
    let hover = t.colors.hover;
    let toggle = state.clone();
    let n = rows.len();
    let button = div()
        .id(SharedString::from(format!("versions-{chat}-{msg}")))
        .flex()
        .items_center()
        .gap(px(4.))
        .px(t.space.sm)
        .h(t.size.control - px(4.))
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .when(open, |d| d.bg(hover))
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(icon("icons/clock.svg", t.size.icon_xs, t.colors.text_muted))
        .child(format!(
            "{n} earlier {}",
            if n == 1 { "version" } else { "versions" }
        ))
        .tooltip(|window, cx| {
            gpui::component::tooltip::Tooltip::new(
                "This conversation was edited or retried here: see what it was",
            )
            .build(window, cx)
        })
        .on_click(move |_, _, cx| toggle.update(cx, |s, cx| s.toggle_versions(chat, msg, cx)));
    let list = open.then(|| {
        let mut list = div()
            .id(SharedString::from(format!("versions-list-{chat}-{msg}")))
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .max_h(t.size.popover_list)
            .overflow_y_scroll()
            // More versions than fit: the list keeps the wheel from the chat behind.
            .when(rows.len() > 4, super::hold_wheel);
        // Newest first.
        for row in rows.iter().rev() {
            let (st, i) = (state.clone(), row.index);
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .p(t.space.md)
                    .rounded(t.radius.button)
                    .bg(t.extra.inset)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .child(icon("icons/clock.svg", t.size.icon_xs, t.colors.text_subtle))
                            .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(ago(row.when)))
                            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(t.text.caption).font_weight(FontWeight::MEDIUM).text_color(t.colors.text).child(row.asked.clone()))
                            .child(theme::ghost_button(t, "Restore").id(SharedString::from(format!("restore-{chat}-{i}"))).text_size(t.text.caption).py(px(2.)).px(t.space.sm).on_click(move |_, _, cx| st.update(cx, |s, cx| s.restore_version(chat, i, cx)))),
                    )
                    .child(div().text_size(t.text.caption).line_height(relative(1.45)).text_color(t.colors.text_body).child(row.preview.clone())),
            );
        }
        // It unfolds under its button, as the app's inline menus do.
        super::motion::unfold(
            theme::popover(t).w(px(420.)).p(t.space.sm).flex().flex_col().gap(t.space.xs).child(div().px(t.space.xs).text_size(t.text.caption).text_color(t.colors.text_subtle).child("Read-only. Restore brings that version back; the one showing now is kept as a version.")).child(list),
            &format!("versions-{chat}-{msg}"),
            false,
            false,
            t,
        )
    });
    div()
        .flex()
        .flex_col()
        .items_end()
        .gap(t.space.xs)
        .child(button)
        .children(list)
        .into_any_element()
}
