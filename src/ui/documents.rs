//! Documents: every file Forge made, across your chats, newest first, as
//! Claude Code lists its artifacts. A card opens the file beside the page;
//! the line under it names the chat it came from, and opens that chat.

use gpui::{div, prelude::*, AnyElement, Entity, FontWeight, SharedString};

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
    let list = files.into_iter().enumerate().map(|(i, (f, chat, title))| {
        let is_open = open
            .as_ref()
            .is_some_and(|(n, p)| *n == f.name && *p == f.path);
        let st = state.clone();
        div()
            .flex()
            .flex_col()
            .gap(t.space.xs)
            .child(super::file_card::render(
                &f,
                &format!("documents-{i}"),
                state.clone(),
                is_open,
                scale,
                None,
                t,
            ))
            .child(
                div()
                    .id(("document-chat", i))
                    .px(t.space.xs)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .cursor_pointer()
                    .hover(|d| d.underline())
                    .child(format!("From “{title}”"))
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.open_chat(chat, cx))),
            )
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
                .gap(t.space.lg)
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
                                    "Files Forge makes in your chats (documents, sheets, decks, diagrams) gather here.".to_string()
                                } else {
                                    format!(
                                        "{count} {} Forge made in your chats, newest first.",
                                        if count == 1 { "file" } else { "files" }
                                    )
                                }),
                        ),
                )
                .children(list),
        )
        .into_any_element()
}
