//! A file Forge made, as a card in its reply: a small page that previews the
//! document, its title and type, and Download. Clicking the card opens the
//! file beside the chat (`doc_pane.rs`); clicking it again closes it.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::state::{AppState, FileRef};
use crate::theme::Theme;

/// The page tile: a sheet with a few lines of "text" and the file's type.
fn page(file: &FileRef, t: &Theme) -> AnyElement {
    let line = |w: f32| div().h(px(2.)).w(relative(w)).rounded(t.radius.pill).bg(t.colors.text_subtle.opacity(0.35));
    div()
        .flex_none()
        .w(px(36.))
        .h(px(44.))
        .flex()
        .flex_col()
        .justify_between()
        .p(px(6.))
        .bg(t.extra.stage)
        .border_1()
        .border_color(t.colors.border)
        // Concentric with the card: its 12px corner less the 8px between them.
        .rounded(t.radius.small)
        .child(div().flex().flex_col().gap(px(3.)).child(line(0.9)).child(line(0.7)).child(line(0.8)))
        .child(
            div()
                .text_size(px(9.))
                .line_height(relative(1.))
                .font_weight(FontWeight::BOLD)
                .font_family(t.font_mono.clone())
                .text_color(t.colors.primary)
                .child(file.kind()),
        )
        .into_any_element()
}

pub fn render(file: &FileRef, key: &str, state: Entity<AppState>, open: bool, t: &Theme) -> AnyElement {
    let hover = t.colors.hover;
    // Hovered, the edge darkens; open, it stays the accent.
    let edge = if open { t.colors.primary.opacity(0.6) } else { t.colors.text_subtle.opacity(0.5) };
    let (opener, saver) = (state.clone(), state);
    let (to_open, to_save) = (file.clone(), file.clone());
    let kind = if file.is_markdown() { "Document".to_string() } else { "File".to_string() };
    div()
        .id(SharedString::from(format!("file-{key}")))
        .w_full()
        .max_w(px(440.))
        .flex()
        .items_center()
        .gap(t.space.md)
        .p(t.space.sm)
        .pr(t.space.md)
        .bg(t.colors.surface)
        .border_1()
        .border_color(if open { t.colors.primary.opacity(0.6) } else { t.colors.border })
        .rounded(t.radius.card)
        .cursor_pointer()
        .hover(move |st| st.border_color(edge))
        .child(page(file, t))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child(file.title()))
                .child(div().text_size(t.text.caption).text_color(t.colors.text_muted).child(format!("{kind} · {}", file.kind()))),
        )
        .child(
            div()
                .id(SharedString::from(format!("file-dl-{key}")))
                .flex_none()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .h(t.size.control)
                .px(t.space.sm)
                .rounded(t.radius.button)
                .border_1()
                .border_color(t.colors.border)
                .text_size(t.text.caption)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text)
                .hover(move |st| st.bg(hover))
                .child(icon("icons/download.svg", t.size.icon_sm, t.colors.text_muted))
                .child("Download")
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    saver.update(cx, |s, cx| s.download(&to_save, cx));
                }),
        )
        .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx)))
        .into_any_element()
}
