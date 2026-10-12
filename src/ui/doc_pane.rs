//! The side panel: a file Forge made, opened beside the chat. A document is
//! rendered as one (`markdown::Look::Document`); a diagram is drawn natively
//! (`diagram_view.rs`), fitted to the panel or at its actual size, and can
//! open in the browser where it is interactive. The header names the file
//! and holds Copy, Download and Close; Escape closes it too.

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

/// What the panel needs from the window: its width and how dense the display is.
pub struct Fit {
    /// The panel's width, in logical pixels.
    pub width: f32,
    /// Device pixels per logical pixel.
    pub scale: f32,
    /// A diagram fits the panel (else it shows at its own size).
    pub fit: bool,
}

/// How wide the panel is when the session area is `main` wide: half for a
/// document, more for a diagram, whose labels shrink with it.
pub fn width(main: f32, diagram: bool, t: &Theme) -> f32 {
    (main * if diagram { 0.6 } else { 0.5 }).max(f32::from(t.size.doc_pane))
}

pub fn render(file: &FileRef, state: Entity<AppState>, copied: Option<SharedString>, fit: Fit, t: &Theme) -> AnyElement {
    let did_copy = copied.as_deref() == Some(COPY_KEY);
    let (copier, saver, closer, fitter, opener) = (state.clone(), state.clone(), state.clone(), state.clone(), state.clone());
    let (text, to_save, to_open) = (file.content.clone(), file.clone(), file.clone());
    let diagram = file.is_diagram();

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
        .when(diagram, |d| {
            d.child(tool_button(t, "doc-fit", "icons/maximize-2.svg", if fit.fit { "Actual size" } else { "Fit to panel" }).on_click(move |_, _, cx| fitter.update(cx, |s, cx| s.toggle_doc_fit(cx))))
                .child(tool_button(t, "doc-browser", "icons/globe.svg", "Open in browser").on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_in_browser(&to_open, cx))))
        })
        .child(
            tool_button(t, "doc-copy", if did_copy { "icons/check.svg" } else { "icons/copy.svg" }, if diagram { "Copy source" } else { "Copy" }).on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                copier.update(cx, |s, cx| s.mark_copied(COPY_KEY.into(), cx));
            }),
        )
        .child(tool_button(t, "doc-download", "icons/download.svg", "Download").on_click(move |_, _, cx| saver.update(cx, |s, cx| s.download(&to_save, cx))))
        .child(tool_button(t, "doc-close", "icons/x.svg", "Close").on_click(move |_, _, cx| closer.update(cx, |s, cx| s.close_doc(cx))));

    let body = div().id("doc-body").flex_1().min_h_0().overflow_y_scroll();
    let body = if diagram {
        // The drawing, with its one-line description above it. At actual
        // size a drawing wider than the panel scrolls sideways.
        let pad = t.space.xl;
        let room = fit.width - 1. - 2. * f32::from(pad);
        let svg = super::diagram_view::themed(crate::diagram::extract_svg(&file.content).unwrap_or_default(), t.is_dark);
        let svg = svg.as_ref();
        let natural = super::diagram_view::natural_width(svg).unwrap_or(room);
        let shown = if fit.fit { room } else { natural };
        body.child(
            div()
                .flex()
                .flex_col()
                .gap(t.space.md)
                .px(pad)
                .pt(pad)
                .pb(t.space.xxl)
                .children(crate::diagram::description(&file.content).map(|d| div().text_size(t.text.body_sm).line_height(relative(1.5)).text_color(t.colors.text_body).child(d)))
                .child(
                    div()
                        .id("diagram-scroll")
                        .w_full()
                        .overflow_x_scroll()
                        .rounded(t.radius.small)
                        .border_1()
                        .border_color(t.colors.border)
                        .children(super::diagram_view::image(svg, shown, fit.scale)),
                ),
        )
    } else {
        let page: AnyElement = if file.is_markdown() {
            markdown::render_as(&file.content, t, &CopyContext { state, key: "doc".into(), copied }, Look::Document)
        } else {
            div().font_family(t.font_mono.clone()).text_size(t.text.caption).line_height(relative(1.5)).text_color(t.colors.text).child(file.content.clone()).into_any_element()
        };
        body.child(div().flex().justify_center().px(t.space.xxl).pt(t.space.xl + t.space.xs).pb(t.space.xxl * 2.).child(div().w_full().max_w(t.size.doc_measure).child(page)))
    };

    div()
        .flex_none()
        .w(px(fit.width))
        .h_full()
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_l_1()
        .border_color(t.colors.border)
        .child(header)
        .child(body)
        // In with a short ease-out: it fades and settles 16px from the right.
        .with_animation(
            SharedString::from(format!("doc-in-{}", file.name)),
            Animation::new(Duration::from_millis(220)).with_easing(gpui::ease_out_quint()),
            |d, k| d.opacity(k).relative().left(px(16. * (1. - k))),
        )
        .into_any_element()
}
