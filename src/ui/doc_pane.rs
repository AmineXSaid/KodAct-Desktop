//! The side panel: a file KodAct made, opened beside the chat. A document is
//! rendered as one (`markdown::Look::Document`); a diagram is drawn natively
//! (`diagram_view.rs`), fitted to the panel or at its actual size, and can
//! open in the browser where it is interactive. The header names the file
//! and holds Find, Copy, Download (and Export to PDF for an Office file)
//! and Close; Escape closes it too. Alt ↑/↓ page through a document.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, relative, Animation, AnimationExt, AnyElement, ClipboardItem, Entity,
    FontWeight, SharedString,
};

use super::icon;
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
    /// Expanded across the full main stage.
    pub full: bool,
    /// Smooth full-screen transition progress in `0.0..=1.0`.
    pub full_t: f32,
    /// Opened from the panel's Files tab: the header leads back to it.
    pub from_panel: bool,
    /// A document shows its text instead of its pages.
    pub text: bool,
    /// A Markdown file or a page shows its source (else its preview).
    pub source: bool,
    /// Which version of the file this is, and how many the chat has.
    pub version: (usize, usize),
    /// The panel shows what this version changed.
    pub show_changes: bool,
    /// The page or slide picked to comment on.
    pub page_pick: Option<usize>,
    /// The body's scroll, whose items are the pages.
    pub scroll: gpui::ScrollHandle,
}

/// What this version changed: None while it is worked out, `Some(None)`
/// when it cannot be.
pub type ChangesRead<'a> = Option<&'a Option<crate::doc_versions::Changes>>;

/// How wide the panel is when the session area is `main` wide: half for a
/// document, more for a diagram, whose labels shrink with it.
pub fn width(main: f32, diagram: bool, t: &Theme) -> f32 {
    (main * if diagram { 0.6 } else { 0.5 }).max(f32::from(t.size.doc_pane))
}

/// An Office file or PDF, as `office.rs` read it: None while it is read.
pub type Read<'a> = Option<&'a Result<std::sync::Arc<crate::office::Preview>, String>>;

#[allow(clippy::too_many_arguments)]
pub fn render(
    file: &FileRef,
    state: Entity<AppState>,
    copied: Option<SharedString>,
    fit: Fit,
    read: Read,
    sheet: usize,
    pick: super::sheet_view::Pick,
    sheet_edits: &crate::office::CellEdits,
    sheet_extra: (usize, usize),
    sheet_editor: AnyElement,
    changes: ChangesRead,
    comments: AnyElement,
    find: AnyElement,
    t: &Theme,
) -> AnyElement {
    // The version bar at the foot, when the chat has more than one version.
    let (at, total) = fit.version;
    let showing_changes = fit.show_changes && at > 0;
    let summary = match changes {
        Some(Some(crate::doc_versions::Changes::Text(p))) if showing_changes => {
            super::doc_changes::summary(p)
        }
        Some(Some(crate::doc_versions::Changes::Cells(c))) if showing_changes => {
            let n: usize = c.values().map(|m| m.len()).sum();
            Some(format!("{n} cell{} changed", if n == 1 { "" } else { "s" }))
        }
        _ => None,
    };
    let bar_state = state.clone();
    // The foot: comments on the file, then the version bar.
    let footer = move || {
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .child(comments)
                .children((total > 1).then(|| {
                    super::doc_changes::version_bar(
                        t,
                        bar_state.clone(),
                        at,
                        total,
                        showing_changes,
                        summary.clone(),
                    )
                })),
        )
    };
    let changed_cells = match changes {
        Some(Some(crate::doc_versions::Changes::Cells(c))) if showing_changes => Some(c),
        _ => None,
    };
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
    // Markdown and pages read as themselves, or as what was written.
    let two_views = office.is_none() && (diagram || file.is_markdown());
    let source = two_views && fit.source;
    let text_view = fit.text;
    let page_pick = fit.page_pick;
    let has_pages = matches!(read, Some(Ok(p)) if matches!(p.as_ref(), crate::office::Preview::Doc(d) if !d.pages.is_empty()));
    let viewer = state.clone();
    let back = state.clone();
    let (finder, exporter, to_export) = (state.clone(), state.clone(), file.clone());
    let exports = matches!(
        office,
        Some(crate::office::Kind::Docx | crate::office::Kind::Pptx | crate::office::Kind::Xlsx)
    );
    let dirty_sheet = office == Some(crate::office::Kind::Xlsx) && !sheet_edits.is_empty();
    let full = fit.full;
    let full_t = fit.full_t;
    let fuller = state.clone();
    let shimmer_alpha = if full_t > 0.005 && full_t < 0.995 {
        (full_t * std::f32::consts::PI).sin() * 0.72
    } else {
        0.0
    };
    let full_btn: AnyElement = if full {
        div()
            .id("doc-full")
            .flex_none()
            .h(px(28.))
            .px(t.space.sm)
            .rounded(t.radius.pill)
            .bg(t.colors.primary_tint)
            .border_1()
            .border_color(gpui::Hsla {
                a: 0.28,
                ..t.colors.primary
            })
            .flex()
            .items_center()
            .gap(px(5.))
            .cursor_pointer()
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.primary)
            .hover(|s| {
                s.bg(gpui::Hsla {
                    a: (t.colors.primary_tint.a + 0.08).min(1.0),
                    ..t.colors.primary_tint
                })
            })
            .child(icon("icons/minimize-2.svg", t.size.icon_xs, t.colors.primary))
            .child("Exit full screen")
            .on_click(move |_, _, cx| fuller.update(cx, |s, cx| s.toggle_doc_full(cx)))
            .into_any_element()
    } else {
        tool_button("doc-full", "icons/maximize-2.svg", "Full screen (Alt+F)")
            .on_click(move |_, _, cx| fuller.update(cx, |s, cx| s.toggle_doc_full(cx)))
            .into_any_element()
    };
    let header = div()
        .relative()
        .flex_none()
        .h(px(48.))
        .flex()
        .items_center()
        .gap(t.space.sm)
        .pl(if from_panel { t.space.sm } else { t.space.lg })
        .pr(t.space.sm)
        .border_b_1()
        .border_color(t.colors.border)
        .when(shimmer_alpha > 0.001, |d| {
            d.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h(px(2.))
                    .w(relative(full_t.clamp(0.0, 1.0)))
                    .bg(gpui::Hsla {
                        a: shimmer_alpha,
                        ..t.colors.primary
                    }),
            )
        })
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
            None if diagram => {
                icon("icons/network.svg", t.size.icon_sm, t.colors.sky).into_any_element()
            }
            None => gpui::img(super::file_icon::asset(
                &file.name,
                false,
                file.path.as_deref().unwrap_or(&file.name),
            ))
            .flex_none()
            .size(t.size.icon)
            .into_any_element(),
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
                )
                .when(dirty_sheet, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(t.radius.pill)
                            .bg(t.extra.warning_tint)
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.extra.warning)
                            .child("Edited"),
                    )
                }),
        );
    // Beside the name, in one bar: how to view it, what to do with it, and
    // Close set apart by a hairline.
    let toolbar = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(2.))
        .when(two_views, |d| {
            let (to_preview, to_source) = (state.clone(), state.clone());
            d.child(
                super::button::Segmented::new("doc-views", "View")
                    .option("Preview", !source, move |_, _, cx| {
                        to_preview.update(cx, |s, cx| s.set_source_view(false, cx))
                    })
                    .option(
                        if diagram { "Code" } else { "Markdown" },
                        source,
                        move |_, _, cx| to_source.update(cx, |s, cx| s.set_source_view(true, cx)),
                    ),
            )
        })
        .when(two_views, |d| d.child(div().w(t.space.xs)))
        .when(diagram && !source, |d| {
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
        .child(
            tool_button("doc-find", "icons/search.svg", "Find in this file")
                .on_click(move |_, _, cx| finder.update(cx, |s, cx| s.toggle_doc_find(cx))),
        )
        .when(exports, |d| {
            d.child(
                tool_button("doc-export", "icons/file-down.svg", "Export to PDF").on_click(
                    move |_, _, cx| exporter.update(cx, |s, cx| s.export_pdf(&to_export, cx)),
                ),
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
        .when(!full, |d| {
            d.child(
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
        })
        .child(
            tool_button("doc-download", "icons/download.svg", "Download")
                .on_click(move |_, _, cx| saver.update(cx, |s, cx| s.download(&to_save, cx))),
        );

    let header = header
        .child(toolbar)
        .child(div().flex_none().w(px(1.)).h(t.size.icon).bg(t.colors.border))
        .child(full_btn)
        .child(
            tool_button("doc-close", "icons/x.svg", "Close")
                .on_click(move |_, _, cx| closer.update(cx, |s, cx| s.close_doc(cx))),
        );
    // The find bar sits under the header.
    let header = div()
        .flex_none()
        .flex()
        .flex_col()
        .child(header)
        .child(find);
    let body = div().id("doc-body").flex_1().min_h_0().overflow_y_scroll();
    // What this version changed, in words (a workbook's show in its grid).
    if showing_changes && changed_cells.is_none() {
        let content: AnyElement = match changes {
            Some(Some(crate::doc_versions::Changes::Text(p))) => {
                super::doc_changes::text_view(p, t)
            }
            Some(_) => div()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .child("These two versions cannot be compared.")
                .into_any_element(),
            None => div()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .child("Working out what changed…")
                .into_any_element(),
        };
        let body = body.child(
            div()
                .flex()
                .justify_center()
                .px(t.space.xxl)
                .pt(t.space.xl + t.space.xs)
                .pb(t.space.xxl * 2.)
                .child(div().w_full().max_w(t.size.doc_measure).child(content)),
        );
        return finish_at(fit.below, full_t, file, header, body, footer(), fit.width, t);
    }
    let body = if source || (office.is_none() && !binary && !file.is_markdown() && !diagram) {
        // The source, as an editor shows it: a file that is only text, or
        // a page or Markdown file switched to its source.
        body.child(
            div()
                .px(t.space.lg)
                .pt(t.space.lg)
                .pb(t.space.xxl * 2.)
                .child(super::source_view::render(
                    "doc-source",
                    &file.content,
                    super::source_view::lang_for(&file.name),
                    t,
                )),
        )
    } else if diagram {
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
                "KodAct could not read {}: {e}. Open it with its app instead.",
                file.name
            )),
            Some(Ok(p)) => match p.as_ref() {
                crate::office::Preview::Sheets(sheets) => {
                    return finish_at(
                        fit.below,
                        full_t,
                        file,
                        header,
                        body.child(div().p(t.space.lg).child(super::sheet_view::render(
                            sheets,
                            sheet,
                            pick,
                            changed_cells,
                            sheet_edits,
                            sheet_extra.0,
                            sheet_extra.1,
                            sheet_editor,
                            state,
                            t,
                        ))),
                        footer(),
                        fit.width,
                        t,
                    )
                }
                crate::office::Preview::Doc(d) => {
                    let text_view = text_view || d.pages.is_empty();
                    if !text_view {
                        return finish_at(
                            fit.below,
                            full_t,
                            file,
                            header,
                            pages_view(body.bg(t.extra.inset), d, &fit, page_pick, state, t),
                            footer(),
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
                        .child(selectable(&file.name, &d.text, t))
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
            selectable(&file.name, &file.content, t)
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

    finish_at(fit.below, full_t, file, header, body, footer(), fit.width, t)
}

/// A document's text as you can select it, to comment on a passage or
/// copy it: Markdown, set in the reading face at the document's size.
fn selectable(name: &str, text: &str, t: &Theme) -> AnyElement {
    div()
        .text_size(t.size.doc_text)
        .line_height(relative(t.size.line_height_doc))
        .font_family(t.font_reading.clone())
        .text_color(t.colors.text_body)
        .child(
            gpui::component::text::TextView::markdown(
                SharedString::from(format!("doc-text-{name}")),
                super::markdown::fence_front_matter(text).into_owned(),
            )
            .selectable(true),
        )
        .into_any_element()
}

/// A document's pages as they print, each as wide as the panel allows on
/// the quiet inset with a hairline round it; a deck's slides with their
/// speaker notes under each.
/// The pages are the body's own items, so its scroll can bring one to the
/// top (Alt ↑/↓, a search result).
fn pages_view(
    body: gpui::Stateful<gpui::Div>,
    d: &crate::office::Doc,
    fit: &Fit,
    picked: Option<usize>,
    state: Entity<AppState>,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let room = fit.width - 2. * f32::from(t.space.xl) - 2.;
    let shown = d.pages.len();
    let noun = if d.slides { "Slide" } else { "Page" };
    body.track_scroll(&fit.scroll)
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
            let on = picked == Some(i);
            let st = state.clone();
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
                        .border_color(if on { t.colors.primary } else { t.colors.border })
                        .when(on, |d| d.shadow(t.elevation_popover()))
                        .when(d.slides, |d| d.rounded(t.radius.small).overflow_hidden())
                        .child(
                            gpui::img(page.image.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(t.space.sm)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(format!("{noun} {} of {}", i + 1, d.total.max(shown)))
                        .child(
                            div()
                                .id(SharedString::from(format!("page-comment-{i}")))
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .px(t.space.xs)
                                .rounded(t.radius.small)
                                .cursor_pointer()
                                .text_color(if on { t.colors.primary } else { t.colors.text_muted })
                                .hover(|st| st.underline())
                                .child(icon(
                                    "icons/message-square.svg",
                                    t.size.icon_xs,
                                    if on { t.colors.primary } else { t.colors.text_muted },
                                ))
                                .child("Comment")
                                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.pick_doc_page(i, cx))),
                        ),
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
    full_t: f32,
    file: &FileRef,
    header: gpui::Div,
    body: gpui::Stateful<gpui::Div>,
    footer: Option<gpui::Div>,
    width: f32,
    t: &Theme,
) -> AnyElement {
    let settled_full = full_t >= 0.999;
    let border_alpha = (1.0 - full_t).clamp(0.0, 1.0);
    let border_color = gpui::Hsla {
        a: t.colors.border.a * border_alpha,
        ..t.colors.border
    };
    div()
        .when(settled_full, |d| d.flex_1().min_w_0().h_full())
        .when(!settled_full && !below, |d| {
            d.flex_none().w(px(width)).h_full().border_l_1()
        })
        .when(!settled_full && below, |d| {
            d.flex_none().w_full().h(relative(BELOW_SHARE)).border_t_1()
        })
        .flex()
        .flex_col()
        .bg(if t.is_dark { t.extra.pane } else { t.colors.surface })
        .border_color(border_color)
        .child(header)
        .child(body)
        .children(footer)
        // In with a short ease-out: it fades and settles 16px from the right.
        .with_animation(
            SharedString::from(format!("doc-in-{}", file.name)),
            Animation::new(Duration::from_millis(t.motion_ms)).with_easing(gpui::ease_out_quint()),
            |d, k| d.opacity(k).relative().left(px(16. * (1. - k))),
        )
        .into_any_element()
}
