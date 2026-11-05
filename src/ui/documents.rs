//! Documents: every file KodAct made, across your chats, newest first. Each
//! is a row (its mark, its title, its type and the chat it came from) with
//! View, which opens it in the side panel, and Download; nothing is drawn
//! in place until you ask for it. The chat's name opens that chat.

use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, SharedString};

use crate::state::{AppState, Block, FileRef, Message};
use crate::theme::Theme;

/// Each file once (its newest version), with the chat that made it.
pub fn all(s: &AppState) -> Vec<(FileRef, usize, SharedString)> {
    let mut chats: Vec<_> = s.chats.iter().filter(|c| !c.archived).collect();
    chats.sort_by(|a, b| b.updated.cmp(&a.updated));
    let mut out: Vec<(FileRef, usize, SharedString)> = vec![];
    for c in chats {
        for m in c.messages.iter().rev() {
            let Message::Assistant { blocks } = m else {
                continue;
            };
            for b in blocks.iter().rev() {
                if let Block::File(f) = b {
                    if !out
                        .iter()
                        .any(|(o, _, _)| o.name == f.name && o.path == f.path)
                    {
                        out.push((f.clone(), c.id, c.title.clone()));
                    }
                }
            }
        }
    }
    out
}

pub fn render(t: &Theme, state: &Entity<AppState>, s: &AppState, scale: f32) -> AnyElement {
    let files = all(s);
    let open = s.doc.as_ref().map(|d| (d.name.clone(), d.path.clone()));
    let count = files.len();
    let hover = t.colors.hover;
    let list = files.into_iter().enumerate().map(|(i, (f, chat, title))| {
        let is_open = open
            .as_ref()
            .is_some_and(|(n, p)| *n == f.name && *p == f.path);
        let (go_chat, view, view_row) = (state.clone(), state.clone(), state.clone());
        let (to_view, to_view_row) = (f.clone(), f.clone());
        let key = format!("documents-{i}");
        let mark: AnyElement = match f.office() {
            Some(kind) => super::file_mark::render(kind, 24., scale),
            None => super::icon(
                if crate::diagram::extract_svg(&f.content).is_some() {
                    "icons/network.svg"
                } else {
                    "icons/file-text.svg"
                },
                t.size.icon,
                t.colors.text_muted,
            )
            .into_any_element(),
        };
        div()
            .id(("document-row", i))
            .flex()
            .items_center()
            .gap(t.space.md)
            .px(t.space.md)
            .py(t.space.sm)
            .rounded(t.radius.card)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.colors.surface)
            .cursor_pointer()
            .when(is_open, |d| crate::theme::selected(t, d))
            .when(!is_open, |d| d.hover(move |st| st.bg(hover)))
            .child(
                div()
                    .w(px(28.))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(mark),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_strong)
                            .child(f.title()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .min_w_0()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(div().flex_none().child(f.kind()))
                            .child("·")
                            .child(
                                div()
                                    .id(("document-chat", i))
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .cursor_pointer()
                                    .hover(|d| d.underline())
                                    .child(format!("From \u{201c}{title}\u{201d}"))
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        go_chat.update(cx, |s, cx| s.open_chat(chat, cx))
                                    }),
                            ),
                    ),
            )
            .child(
                crate::theme::ghost_button(t, if is_open { "Viewing" } else { "View" })
                    .id(("document-view", i))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        let f = to_view.clone();
                        view.update(cx, |s, cx| s.open_doc(f, cx))
                    }),
            )
            .child(super::file_card::download_button(
                &f,
                &key,
                state.clone(),
                t,
            ))
            .on_click(move |_, _, cx| {
                let f = to_view_row.clone();
                view_row.update(cx, |s, cx| s.open_doc(f, cx))
            })
            .into_any_element()
    });
    div()
        .id("documents")
        .size_full()
        .overflow_y_scroll()
        .flex()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w(t.size.column)
                .px(t.space.xl)
                .py(t.space.xxl)
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(t.space.xs)
                        .child(
                            div()
                                .font_family(t.font_display.clone())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(t.text.heading_sm)
                                .text_color(t.colors.text_strong)
                                .child("Documents"),
                        )
                        .child(
                            div()
                                .text_size(t.text.body)
                                .text_color(t.colors.text_muted)
                                .child(if count == 0 {
                                    "Files KodAct makes in your chats (documents, sheets, decks, diagrams) gather here.".to_string()
                                } else {
                                    format!(
                                        "{count} {} KodAct made in your chats, newest first.",
                                        if count == 1 { "file" } else { "files" }
                                    )
                                }),
                        ),
                )
                .children(list),
        )
        .into_any_element()
}
