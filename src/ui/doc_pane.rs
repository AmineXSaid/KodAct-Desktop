//! The side panel: a file Forge made, opened beside the chat and rendered as
//! a document (`markdown::Look::Document`). Its header names the file and
//! holds Copy, Download and Close; Escape closes it too.

use std::time::Duration;

use gpui::{div, prelude::*, px, relative, Animation, AnimationExt, AnyElement, ClipboardItem, Entity, FontWeight, SharedString};

use super::icon;
use super::markdown::{self, CopyContext, Look};
use crate::state::{AppState, FileRef};
use crate::theme::Theme;

const COPY_KEY: &str = "doc-pane-copy";

fn tool_button(t: &Theme, id: &'static str, path: &'static str, tip: &'static str) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex_none()
        .size(t.size.control)
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |st| st.bg(hover))
        .child(icon(path, t.size.icon, t.colors.text_muted))
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
}

pub fn render(file: &FileRef, state: Entity<AppState>, copied: Option<SharedString>, t: &Theme) -> AnyElement {
    let did_copy = copied.as_deref() == Some(COPY_KEY);
    let (copier, saver, closer) = (state.clone(), state.clone(), state.clone());
    let (text, to_save) = (file.content.clone(), file.clone());

    let header = div()
        .flex_none()
        .h(px(48.))
        .flex()
        .items_center()
        .gap(t.space.sm)
        .pl(t.space.lg)
        .pr(t.space.sm)
        .border_b_1()
        .border_color(t.colors.border)
        .child(icon("icons/file-text.svg", t.size.icon_sm, t.colors.text_muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_baseline()
                .gap(t.space.sm)
                .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child(file.title()))
                .child(div().flex_none().text_size(t.text.caption).text_color(t.colors.text_muted).child(file.kind())),
        )
        .child(
            tool_button(t, "doc-copy", if did_copy { "icons/check.svg" } else { "icons/copy.svg" }, "Copy").on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                copier.update(cx, |s, cx| s.mark_copied(COPY_KEY.into(), cx));
            }),
        )
        .child(tool_button(t, "doc-download", "icons/download.svg", "Download").on_click(move |_, _, cx| saver.update(cx, |s, cx| s.download(&to_save, cx))))
        .child(tool_button(t, "doc-close", "icons/x.svg", "Close").on_click(move |_, _, cx| closer.update(cx, |s, cx| s.close_doc(cx))));

    let page: AnyElement = if file.is_markdown() {
        markdown::render_as(&file.content, t, &CopyContext { state, key: "doc".into(), copied }, Look::Document)
    } else {
        div().font_family(t.font_mono.clone()).text_size(t.text.caption).line_height(relative(1.5)).text_color(t.colors.text).child(file.content.clone()).into_any_element()
    };

    div()
        .flex_none()
        .w(relative(0.5))
        .min_w(t.size.doc_pane)
        .h_full()
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_l_1()
        .border_color(t.colors.border)
        .child(header)
        .child(
            div()
                .id("doc-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(div().flex().justify_center().px(t.space.xxl).pt(t.space.xl + t.space.xs).pb(t.space.xxl * 2.).child(div().w_full().max_w(t.size.doc_measure).child(page))),
        )
        // In with a short ease-out: it fades and settles 16px from the right.
        .with_animation(
            SharedString::from(format!("doc-in-{}", file.name)),
            Animation::new(Duration::from_millis(220)).with_easing(gpui::ease_out_quint()),
            |d, k| d.opacity(k).relative().left(px(16. * (1. - k))),
        )
        .into_any_element()
}
