//! The side panel: a file Forge made, opened beside the chat. A document is
//! rendered as one (`markdown::Look::Document`); a diagram is drawn natively
//! (`diagram_view.rs`), fitted to the panel or at its actual size, and can
//! open in the browser where it is interactive. The header names the file
//! and holds Copy, Download and Close; Escape closes it too.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, relative, Animation, AnimationExt, AnyElement, ClipboardItem, Entity,
    FontWeight, SharedString,
};

use super::icon;
use super::markdown::{self, CopyContext, Look};
use crate::state::{AppState, FileRef};
use crate::theme::Theme;

const COPY_KEY: &str = "doc-pane-copy";

/// How much of the height the panel takes when split below the chat.
const BELOW_SHARE: f32 = 0.45;

/// A header button: icon-only, named in its tooltip, keyboard-reachable.
fn tool_button(id: &'static str, path: &'static str, tip: &'static str) -> super::button::Button {
    super::button::Button::icon(id, path, tip).compact()
}

/// What the panel needs from the window: its width and how dense the display is.
pub struct Fit {
    /// The panel's width, in logical pixels.
    pub width: f32,
    /// Device pixels per logical pixel.
    pub scale: f32,
    /// A diagram fits the panel (else it shows at its own size).
    pub fit: bool,
    /// Split below the chat (else beside it).
    pub below: bool,
    /// Opened from the panel's Files tab: the header leads back to it.
    pub from_panel: bool,
    /// A document shows its text instead of its pages.
    pub text: bool,
}

/// How wide the panel is when the session area is `main` wide: half for a
/// document, more for a diagram, whose labels shrink with it.
pub fn width(main: f32, diagram: bool, t: &Theme) -> f32 {
    (main * if diagram { 0.6 } else { 0.5 }).max(f32::from(t.size.doc_pane))
}

/// An Office file or PDF, as `office.rs` read it: None while it is read.
pub type Read<'a> = Option<&'a Result<std::sync::Arc<crate::office::Preview>, String>>;

pub fn render(
    file: &FileRef,
    state: Entity<AppState>,
    copied: Option<SharedString>,
    fit: Fit,
    read: Read,
    sheet: usize,
    pick: super::sheet_view::Pick,
    t: &Theme,
) -> AnyElement {
    let did_copy = copied.as_deref() == Some(COPY_KEY);
    let (copier, saver, closer, fitter, opener) = (
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
        state.clone(),
    );
    let (text, to_save, to_open) = (file.content.clone(), file.clone(), file.clone());
    let diagram = file.is_diagram();
    let office = file.office();
    let binary = file.bytes.is_some();
    let external = to_open.clone();
    let launcher = state.clone();

    let from_panel = fit.from_panel;
    let text_view = fit.text;
    let has_pages = matches!(read, Some(Ok(p)) if matches!(p.as_ref(), crate::office::Preview::Doc(d) if !d.pages.is_empty()));
    let viewer = state.clone();
    let back = state.clone();
    let header = div()
        .flex_none()
        .h(px(48.))
        .flex()
        .items_center()
        .gap(t.space.sm)
        .pl(if from_panel { t.space.sm } else { t.space.lg })
        .pr(t.space.sm)
        .border_b_1()
        .border_color(t.colors.border)
        .when(from_panel, |d| {
            d.child(
                tool_button("doc-back", "icons/arrow-left.svg", "All files").on_click(
                    move |_, _, cx| {
                        back.update(cx, |s, cx| s.show_panel(crate::state::PanelTab::Files, cx))
                    },
                ),
            )
        })
        .child(match office {
            Some(kind) => super::file_mark::render(kind, f32::from(t.size.icon_lg), fit.scale),
            None => {
                icon("icons/file-text.svg", t.size.icon_sm, t.colors.text_muted).into_any_element()
            }
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_baseline()
                .gap(t.space.sm)
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(file.title()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(file.kind()),
                ),
        )
        .when(diagram, |d| {
            d.child(
                tool_button(
                    "doc-fit",
                    "icons/maximize-2.svg",
                    if fit.fit {
                        "Actual size"
                    } else {
                        "Fit to panel"
                    },
                )
                .on_click(move |_, _, cx| fitter.update(cx, |s, cx| s.toggle_doc_fit(cx))),
            )
            .child(
                tool_button("doc-browser", "icons/globe.svg", "Open in browser").on_click(
                    move |_, _, cx| opener.update(cx, |s, cx| s.open_externally(&to_open, cx)),
                ),
            )
        })
        .when(has_pages, |d| {
            d.child(
                tool_button(
                    "doc-view",
                    if text_view {
                        "icons/layers.svg"
                    } else {
                        "icons/file-text.svg"
                    },
                    if text_view {
                        "Show the pages"
                    } else {
                        "Show the text"
                    },
                )
                .on_click(move |_, _, cx| viewer.update(cx, |s, cx| s.toggle_doc_text(cx))),
            )
        })
        .when(office.is_some(), |d| {
            d.child(
                tool_button(
                    "doc-external",
                    "icons/external-link.svg",
                    "Open with default app",
                )
                .on_click(move |_, _, cx| {
                    launcher.update(cx, |s, cx| s.open_externally(&external, cx))
                }),
            )
        })
        .when(!binary, |d| {
            d.child(
                tool_button(
                    "doc-copy",
                    if did_copy {
                        "icons/check.svg"
                    } else {
                        "icons/copy.svg"
                    },
                    if diagram { "Copy source" } else { "Copy" },
                )
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                    copier.update(cx, |s, cx| s.mark_copied(COPY_KEY.into(), cx));
                }),
            )
        })
        .child(
            tool_button(
                "doc-split",
                "icons/split.svg",
                if fit.below {
                    "Put it beside the chat"
                } else {
                    "Put it below the chat"
                },
            )
            .on_click(|_, window, cx| window.dispatch_action(Box::new(super::ToggleSplit), cx)),
        )
        .child(
            tool_button("doc-download", "icons/download.svg", "Download")
                .on_click(move |_, _, cx| saver.update(cx, |s, cx| s.download(&to_save, cx))),
        )
        .child(
            tool_button("doc-close", "icons/x.svg", "Close")
                .on_click(move |_, _, cx| closer.update(cx, |s, cx| s.close_doc(cx))),
        );

    let body = div().id("doc-body").flex_1().min_h_0().overflow_y_scroll();
    let body = if diagram {
        // The drawing, with its one-line description above it. At actual
        // size a drawing wider than the panel scrolls sideways.
        let pad = t.space.xl;
        let room = fit.width - 1. - 2. * f32::from(pad);
        let svg = super::diagram_view::themed(
            crate::diagram::extract_svg(&file.content).unwrap_or_default(),
            t.is_dark,
        );
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
                .children(crate::diagram::description(&file.content).map(|d| {
                    div()
                        .text_size(t.text.body_sm)
                        .line_height(relative(1.5))
                        .text_color(t.colors.text_body)
                        .child(d)
                }))
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
    } else if office.is_some() {
        let quiet = |text: String| {
            div()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .child(text)
                .into_any_element()
        };
        let content: AnyElement = match read {
            None => quiet(format!("Reading {}…", file.name)),
            Some(Err(e)) => quiet(format!(
                "Forge could not read {}: {e}. Open it with its app instead.",
                file.name
            )),
            Some(Ok(p)) => match p.as_ref() {
                crate::office::Preview::Sheets(sheets) => {
                    return finish_at(
                        fit.below,
                        file,
                        header,
                        body.child(
                            div()
                                .p(t.space.xl)
                                .child(super::sheet_view::render(sheets, sheet, pick, state, t)),
                        ),
                        fit.width,
                        t,
                    )
                }
                crate::office::Preview::Doc(d) => {
                    let text_view = text_view || d.pages.is_empty();
                    if !text_view {
                        return finish_at(
                            fit.below,
                            file,
                            header,
                            body.bg(t.extra.inset).child(pages_view(d, &fit, t)),
                            fit.width,
                            t,
                        );
                    }
                    let note = if d.drawing {
                        Some("Laying out the pages as they print…")
                    } else if d.pages.is_empty()
                        && office != Some(crate::office::Kind::Pdf)
                        && crate::office_render::soffice().is_none()
                    {
                        Some("Install LibreOffice to see the pages as they print. Its text is below.")
                    } else {
                        None
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(t.space.lg)
                        .children(note.map(|n| {
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(n)
                        }))
                        .child(markdown::render_as(
                            &d.text,
                            t,
                            &CopyContext {
                                streaming: false,
                                state,
                                key: "doc".into(),
                                copied,
                                scale: fit.scale,
                                sources: Default::default(),
                            },
                            Look::Document,
                        ))
                        .into_any_element()
                }
            },
        };
        body.child(
            div()
                .flex()
                .justify_center()
                .px(t.space.xxl)
                .pt(t.space.xl + t.space.xs)
                .pb(t.space.xxl * 2.)
                .child(div().w_full().max_w(t.size.doc_measure).child(content)),
        )
    } else {
        let page: AnyElement = if file.is_markdown() {
            markdown::render_as(
                &file.content,
                t,
                &CopyContext {
                    streaming: false,
                    state,
                    key: "doc".into(),
                    copied,
                    scale: fit.scale,
                    sources: Default::default(),
                },
                Look::Document,
            )
        } else {
            div()
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption)
                .line_height(relative(1.5))
                .text_color(t.colors.text)
                .child(file.content.clone())
                .into_any_element()
        };
        body.child(
            div()
                .flex()
                .justify_center()
                .px(t.space.xxl)
                .pt(t.space.xl + t.space.xs)
                .pb(t.space.xxl * 2.)
                .child(div().w_full().max_w(t.size.doc_measure).child(page)),
        )
    };

    finish_at(fit.below, file, header, body, fit.width, t)
}

/// A document's pages as they print, each as wide as the panel allows on
/// the quiet inset with a hairline round it; a deck's slides with their
/// speaker notes under each.
fn pages_view(d: &crate::office::Doc, fit: &Fit, t: &Theme) -> gpui::Div {
    let room = fit.width - 2. * f32::from(t.space.xl) - 2.;
    let shown = d.pages.len();
    let noun = if d.slides { "Slide" } else { "Page" };
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(t.space.lg)
        .p(t.space.xl)
        .children(d.pages.iter().enumerate().map(|(i, page)| {
            let w = if d.slides {
                room
            } else {
                room.min(page.width as f32 / fit.scale.max(1.) * 1.2)
            };
            let note = d.notes.get(i).filter(|n| !n.trim().is_empty()).cloned();
            div()
                .w(px(w))
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .child(
                    div()
                        .w(px(w))
                        .h(px(w * page.height as f32 / page.width as f32))
                        .border_1()
                        .border_color(t.colors.border)
                        .when(d.slides, |d| d.rounded(t.radius.small).overflow_hidden())
                        .child(
                            gpui::img(page.image.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .text_center()
                        .child(format!("{noun} {} of {}", i + 1, d.total.max(shown))),
                )
                .children(note.map(|n| {
                    div()
                        .p(t.space.sm)
                        .rounded(t.radius.small)
                        .bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.border)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_body)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_muted)
                                .child("Speaker notes"),
                        )
                        .child(n)
                }))
        }))
        .when(d.total > shown, |el| {
            el.child(
                div()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_muted)
                    .child(format!(
                        "The first {shown} of {} are shown here. Open it with its app for the rest.",
                        d.total
                    )),
            )
        })
}

/// The panel around its header and body: beside the chat, a fixed width
/// with a hairline on its left; below it, the full width and part of the
/// height with a hairline on top. The card surface, and a short entrance.
fn finish_at(
    below: bool,
    file: &FileRef,
    header: gpui::Div,
    body: gpui::Stateful<gpui::Div>,
    width: f32,
    t: &Theme,
) -> AnyElement {
    div()
        .flex_none()
        .when(!below, |d| d.w(px(width)).h_full().border_l_1())
        .when(below, |d| d.w_full().h(relative(BELOW_SHARE)).border_t_1())
        .flex()
        .flex_col()
        .bg(t.colors.surface)
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
