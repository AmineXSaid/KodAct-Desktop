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
pub const BELOW_SHARE: f32 = 0.45;

/// A header button: icon-only, named in its tooltip, keyboard-reachable.
fn tool_button(id: &'static str, path: &'static str, tip: &'static str) -> super::button::Button {
    super::button::Button::icon(id, path, tip).compact()
}

/// What the panel needs from the window: its width and how dense the display is.
#[derive(Clone)]
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
    /// The room for a whole page: the body's height.
    pub height: f32,
    /// How this file is scaled, and the percentage fitted pages draw at
    /// (written back as they are drawn).
    pub zoom: crate::doc_zoom::Zoom,
    pub fitted: std::sync::Arc<std::sync::atomic::AtomicU16>,
    /// The thumbnail rail is shown.
    pub thumbs: bool,
    /// The toolbar's open menu.
    pub menu: Option<crate::state::DocMenu>,
    /// Where a drag to pan a picture last was.
    pub drag: std::rc::Rc<std::cell::Cell<Option<gpui::Point<gpui::Pixels>>>>,
    /// The file has edits not saved, and (for a text file) the text edited.
    pub dirty: bool,
    /// The width a web page previews at.
    pub device: crate::state::Device,
    pub edit_text: Option<SharedString>,
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
    page_field: AnyElement,
    text_editor: AnyElement,
    browser: AnyElement,
    office_editor: AnyElement,
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
    let html = file.is_html();
    let two_views = office.is_none() && (diagram || html || file.is_markdown());
    let source = two_views && fit.source;
    let text_view = fit.text;
    let page_pick = fit.page_pick;
    // The pages as drawn: how many, and whether they are slides.
    let pages: Option<(usize, bool)> = match read {
        Some(Ok(p)) => match p.as_ref() {
            crate::office::Preview::Doc(d) if !d.pages.is_empty() => {
                Some((d.pages.len(), d.slides))
            }
            _ => None,
        },
        _ => None,
    };
    let has_pages = pages.is_some();
    // A workbook: its sheets, scaled by the zoom (a fit is 100%).
    let sheets_read = match read {
        Some(Ok(p)) => match p.as_ref() {
            crate::office::Preview::Sheets(s) => Some(s),
            _ => None,
        },
        _ => None,
    };
    let sheet_zoom = match fit.zoom {
        crate::doc_zoom::Zoom::Percent(p) => f32::from(p) / 100.,
        _ => 1.,
    };
    if sheets_read.is_some() {
        fit.fitted.store(100, std::sync::atomic::Ordering::Relaxed);
    }
    // A Word file or deck's words can be edited; a PDF's only read.
    let words_editable = matches!(
        office,
        Some(crate::office::Kind::Docx | crate::office::Kind::Pptx)
    );
    let image = file.is_image() && !showing_changes;
    let paged = (has_pages && !text_view && !showing_changes) || image;
    let zoomable = paged || sheets_read.is_some();
    let viewer = state.clone();
    let back = state.clone();
    let (finder, exporter, to_export) = (state.clone(), state.clone(), file.clone());
    let exports = matches!(
        office,
        Some(crate::office::Kind::Docx | crate::office::Kind::Pptx | crate::office::Kind::Xlsx)
    );
    let dirty_sheet = fit.dirty || (office == Some(crate::office::Kind::Xlsx) && !sheet_edits.is_empty());
    let previewing_html = html && !source && !showing_changes;
    let editing = (source || (office.is_none() && !binary && !file.is_markdown() && !diagram && !html))
        && super::text_editor::editable(file)
        && !showing_changes;
    let full = fit.full;
    let full_t = fit.full_t;
    let fuller = state.clone();

    // The status line: where you are, the versions, the scale.
    let top = fit.scroll.top_item();
    let position = match pages {
        _ if previewing_html => {
            let d = fit.device;
            d.width().map_or(d.label().to_string(), |w| format!("{} · {w} px", d.label()))
        }
        _ if sheets_read.is_some() => sheets_read
            .and_then(|sh| sh.get(sheet))
            .map(|sh| {
                format!(
                    "{} · {} rows × {} columns{}",
                    sh.name,
                    sh.total_rows,
                    sh.total_cols,
                    if sheet_edits.is_empty() {
                        String::new()
                    } else {
                        format!(" · {} edit{}", sheet_edits.len(), if sheet_edits.len() == 1 { "" } else { "s" })
                    }
                )
            })
            .unwrap_or_default(),
        _ if words_editable && text_view => {
            format!("Editing text{}", if fit.dirty { " · not saved" } else { "" })
        }
        _ if editing => super::text_editor::counts(fit.edit_text.as_ref().unwrap_or(&file.content)),
        _ if image => file
            .bytes
            .as_ref()
            .and_then(|b| super::image_view::info(&file.name, b))
            .map(|i| super::image_view::describe(&i))
            .unwrap_or_default(),
        Some((n, slides)) if paged => format!(
            "{} {} of {n}",
            if slides { "Slide" } else { "Page" },
            super::page_field::label(top, n)
        ),
        _ => String::new(),
    };
    let zoom_text = match (fit.zoom, sheets_read.is_some()) {
        (crate::doc_zoom::Zoom::Percent(p), _) => format!("{p}%"),
        (_, true) => "100%".to_string(),
        (z, false) => z.label(),
    };
    let bar_state = state.clone();
    let footer = move || {
        let right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(t.space.md)
            .when(total > 1, |d| {
                d.child(super::doc_changes::version_controls(
                    t,
                    bar_state.clone(),
                    at,
                    total,
                    showing_changes,
                    summary.clone(),
                ))
            })
            .when(zoomable, |d| d.child(zoom_text));
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .child(comments)
                .child(super::panel_chrome::footer(t, position, right)),
        )
    };

    // Row 1: back, the file, its version and edits; then full screen,
    // split, close. The name always shows; the tools are on row 2.
    let mark = match office {
        Some(kind) => super::file_mark::render(kind, f32::from(t.size.icon_lg), fit.scale),
        None if diagram => icon("icons/network.svg", t.size.icon_sm, t.colors.sky).into_any_element(),
        None => gpui::img(super::file_icon::asset(
            &file.name,
            false,
            file.path.as_deref().unwrap_or(&file.name),
        ))
        .flex_none()
        .size(t.size.icon)
        .into_any_element(),
    };
    let chip_state = state.clone();
    let title_block = div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .child(mark)
        .child(
            div()
                .min_w(px(48.))
                .flex_shrink(1.)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.body)
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
        .when(total > 1, |d| {
            d.child(
                div()
                    .id("doc-version-chip")
                    .flex_none()
                    .px(t.space.sm)
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .rounded(t.radius.pill)
                    .border_1()
                    .border_color(t.colors.border)
                    .cursor_pointer()
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text)
                    .when(showing_changes, |d| d.bg(t.colors.primary_tint))
                    .tooltip(|w, cx| {
                        gpui::component::tooltip::Tooltip::new("What this version changed")
                            .build(w, cx)
                    })
                    .child(format!("v{} of {total}", at + 1))
                    .on_click(move |_, _, cx| {
                        chip_state.update(cx, |s, cx| {
                            if s.doc_versions().1 > 0 {
                                s.toggle_doc_changes(cx)
                            }
                        })
                    }),
            )
        })
        .when(dirty_sheet, |d| {
            d.child(
                div()
                    .id("doc-edited")
                    .flex_none()
                    .size(px(8.))
                    .rounded(t.radius.pill)
                    .bg(t.extra.warning)
                    .tooltip(|w, cx| {
                        gpui::component::tooltip::Tooltip::new("Edited, not saved").build(w, cx)
                    }),
            )
        });
    let full_btn: AnyElement = tool_button(
        "doc-full",
        if full {
            "icons/minimize-2.svg"
        } else {
            "icons/maximize-2.svg"
        },
        if full {
            "Exit full screen (Alt+F)"
        } else {
            "Full screen (Alt+F)"
        },
    )
    .on_click(move |_, _, cx| fuller.update(cx, |s, cx| s.toggle_doc_full(cx)))
    .into_any_element();
    let leading: Vec<AnyElement> = from_panel
        .then(|| {
            tool_button("doc-back", "icons/arrow-left.svg", "All files")
                .on_click(move |_, _, cx| {
                    back.update(cx, |s, cx| s.show_panel(crate::state::PanelTab::Files, cx))
                })
                .into_any_element()
        })
        .into_iter()
        .collect();
    let mut trailing = vec![full_btn];
    if !full {
        trailing.push(
            tool_button(
                "doc-split",
                "icons/split.svg",
                if fit.below {
                    "Put it beside the chat"
                } else {
                    "Put it below the chat"
                },
            )
            .on_click(|_, window, cx| window.dispatch_action(Box::new(super::ToggleSplit), cx))
            .into_any_element(),
        );
    }
    trailing.push(super::panel_chrome::close_button(
        "doc-close",
        "Close",
        move |_, _, cx| closer.update(cx, |s, cx| s.close_doc(cx)),
    ));
    let row1 = super::panel_chrome::header(t, title_block, leading, trailing);

    // Row 2: the tools, in groups.
    let btn = |g: &mut Vec<AnyElement>, b: super::button::Button| g.push(b.into_any_element());
    let mut groups: Vec<super::panel_chrome::Group> = Vec::new();
    // View.
    {
        let mut g: Vec<AnyElement> = Vec::new();
        if two_views {
            let (to_preview, to_source) = (state.clone(), state.clone());
            g.push(
                super::button::Segmented::new("doc-views", "View")
                    .option("Preview", !source, move |_, _, cx| {
                        to_preview.update(cx, |s, cx| s.set_source_view(false, cx))
                    })
                    .option(
                        if diagram || html { "Code" } else { "Markdown" },
                        source,
                        move |_, _, cx| to_source.update(cx, |s, cx| s.set_source_view(true, cx)),
                    )
                    .into_any_element(),
            );
        }
        if has_pages {
            let (to_pages, to_text) = (viewer.clone(), viewer.clone());
            g.push(
                super::button::Segmented::new("doc-pages-text", "View")
                    .option("Pages", !text_view, move |_, _, cx| {
                        to_pages.update(cx, |s, cx| {
                            if s.doc_text {
                                s.toggle_doc_text(cx)
                            }
                        })
                    })
                    .option(if words_editable { "Edit text" } else { "Text" }, text_view, move |_, _, cx| {
                        to_text.update(cx, |s, cx| {
                            if !s.doc_text {
                                s.toggle_doc_text(cx)
                            }
                        })
                    })
                    .into_any_element(),
            );
        }
        if diagram && !source {
            btn(
                &mut g,
                tool_button(
                    "doc-fit",
                    "icons/maximize-2.svg",
                    if fit.fit { "Actual size" } else { "Fit to panel" },
                )
                .on_click(move |_, _, cx| fitter.update(cx, |s, cx| s.toggle_doc_fit(cx))),
            );
            btn(
                &mut g,
                tool_button("doc-browser", "icons/globe.svg", "Open in browser").on_click(
                    move |_, _, cx| opener.update(cx, |s, cx| s.open_externally(&to_open, cx)),
                ),
            );
        }
        if !g.is_empty() {
            let w = (two_views as u8 as f32) * 170.
                + (has_pages as u8 as f32) * 130.
                + (diagram && !source) as u8 as f32 * 58.;
            groups.push(super::panel_chrome::Group { tools: g, width: w });
        }
    }
    // A web page's preview: the width it is laid out at, and Reload.
    if previewing_html {
        use crate::state::Device;
        let (reload, dev) = (state.clone(), state.clone());
        let mut seg = super::button::Segmented::new("doc-device", "Width");
        for d in [Device::Desktop, Device::Tablet, Device::Phone] {
            let st = dev.clone();
            seg = seg.option(d.label(), fit.device == d, move |_, _, cx| {
                st.update(cx, |s, cx| s.set_web_device(d, cx))
            });
        }
        groups.push(super::panel_chrome::Group {
            tools: vec![
                seg.into_any_element(),
                tool_button("doc-reload", "icons/refresh-cw.svg", "Reload the page")
                    .on_click(move |_, _, cx| {
                        reload.update(cx, |s, cx| s.web_send(crate::web_view::Cmd::Reload, cx))
                    })
                    .into_any_element(),
            ],
            width: 236. + 30.,
        });
    }
    // Navigate: previous, the page field, next, thumbnails.
    if let (Some((n, _)), true) = (pages, paged && !image) {
        let (prev, next, thumbs) = (state.clone(), state.clone(), state.clone());
        let on = fit.thumbs;
        let g = vec![
            tool_button("doc-prev", "icons/chevron-up.svg", "Previous page (Alt+Up)")
                .disabled(top == 0)
                .on_click(move |_, _, cx| prev.update(cx, |s, cx| s.step_doc_page(-1, cx)))
                .into_any_element(),
            page_field,
            tool_button("doc-next", "icons/chevron-down.svg", "Next page (Alt+Down)")
                .disabled(top + 1 >= n)
                .on_click(move |_, _, cx| next.update(cx, |s, cx| s.step_doc_page(1, cx)))
                .into_any_element(),
            tool_button("doc-thumbs", "icons/panel-left.svg", "Thumbnails (Ctrl+Alt+T)")
                .selected(on)
                .on_click(move |_, _, cx| thumbs.update(cx, |s, cx| s.toggle_doc_thumbs(cx)))
                .into_any_element(),
        ];
        groups.push(super::panel_chrome::Group {
            tools: g,
            width: 28. * 3. + 82. + 6.,
        });
    }
    // Zoom: out, the scale (a menu of steps and fits), in.
    let zoom_group_at = groups.iter().map(|g| g.width + 9.).sum::<f32>();
    if zoomable {
        let (zout, zin, zmenu) = (state.clone(), state.clone(), state.clone());
        let zoom_text_btn = match (fit.zoom, sheets_read.is_some()) {
            (crate::doc_zoom::Zoom::Percent(p), _) => format!("{p}%"),
            (_, true) => "100%".to_string(),
            (z, false) => z.label(),
        };
        let g = vec![
            tool_button("doc-zoom-out", "icons/zoom-out.svg", "Zoom out (Ctrl+-)")
                .on_click(move |_, _, cx| zout.update(cx, |s, cx| s.zoom_doc(false, cx)))
                .into_any_element(),
            super::button::Button::ghost("doc-zoom-menu", zoom_text_btn)
                .compact()
                .selected(fit.menu == Some(crate::state::DocMenu::Zoom))
                .on_click(move |_, _, cx| {
                    zmenu.update(cx, |s, cx| s.toggle_doc_menu(crate::state::DocMenu::Zoom, cx))
                })
                .into_any_element(),
            tool_button("doc-zoom-in", "icons/zoom-in.svg", "Zoom in (Ctrl+=)")
                .on_click(move |_, _, cx| zin.update(cx, |s, cx| s.zoom_doc(true, cx)))
                .into_any_element(),
        ];
        groups.push(super::panel_chrome::Group {
            tools: g,
            width: 28. + 82. + 28. + 4.,
        });
    }
    // Edit: undo, redo, and the one accent action while something is unsaved.
    if editing || ((words_editable || sheets_read.is_some()) && fit.dirty) {
        let (save, discard) = (state.clone(), state.clone());
        let mut g: Vec<AnyElement> = if !editing {
            vec![]
        } else {
            vec![
            tool_button("doc-undo", "icons/undo-2.svg", "Undo")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(super::UndoDocEdit), cx))
                .into_any_element(),
            tool_button("doc-redo", "icons/redo-2.svg", "Redo")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(super::RedoDocEdit), cx))
                .into_any_element(),
            ]
        };
        let mut w = if editing { 28. * 2. + 2. } else { 0. };
        if fit.dirty {
            g.push(
                super::button::Button::accent("doc-save", "Save")
                    .compact()
                    .on_click(move |_, _, cx| save.update(cx, |s, cx| s.save_doc_edits(cx)))
                    .into_any_element(),
            );
            g.push(
                super::button::Button::ghost("doc-discard", "Discard")
                    .compact()
                    .on_click(move |_, _, cx| {
                        discard.update(cx, |s, cx| s.discard_doc_edits(cx))
                    })
                    .into_any_element(),
            );
            w += 64. + 76.;
        }
        groups.push(super::panel_chrome::Group { tools: g, width: w });
    }
    // Review: find.
    groups.push(super::panel_chrome::Group::of(
        t,
        vec![tool_button("doc-find", "icons/search.svg", "Find in this file (Ctrl+Shift+F)")
            .on_click(move |_, _, cx| finder.update(cx, |s, cx| s.toggle_doc_find(cx)))
            .into_any_element()],
    ));
    // Share: export, open in its app, copy, download. Collapses first.
    {
        let mut g: Vec<AnyElement> = Vec::new();
        if exports {
            btn(
                &mut g,
                tool_button("doc-export", "icons/file-down.svg", "Export to PDF").on_click(
                    move |_, _, cx| exporter.update(cx, |s, cx| s.export_pdf(&to_export, cx)),
                ),
            );
        }
        if office.is_some() {
            btn(
                &mut g,
                tool_button(
                    "doc-external",
                    "icons/external-link.svg",
                    "Open with default app",
                )
                .on_click(move |_, _, cx| {
                    launcher.update(cx, |s, cx| s.open_externally(&external, cx))
                }),
            );
        }
        if image {
            let (copy_image, bytes) = (state.clone(), file.clone());
            btn(
                &mut g,
                tool_button("doc-copy", "icons/copy.svg", "Copy the picture").on_click(
                    move |_, _, cx| {
                        if let Some(img) = bytes
                            .bytes
                            .as_ref()
                            .zip(super::image_view::format_of(&bytes.name))
                            .map(|(b, f)| gpui::Image::from_bytes(f, b.to_vec()))
                        {
                            cx.write_to_clipboard(ClipboardItem::new_image(&img));
                        }
                        copy_image.update(cx, |s, cx| s.show_toast("Picture copied.", cx));
                    },
                ),
            );
        }
        if !binary {
            btn(
                &mut g,
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
            );
        }
        btn(
            &mut g,
            tool_button("doc-download", "icons/download.svg", "Download")
                .on_click(move |_, _, cx| saver.update(cx, |s, cx| s.download(&to_save, cx))),
        );
        groups.push(super::panel_chrome::Group::of(t, g));
    }
    let more_state = state.clone();
    let row2 = super::panel_chrome::toolbar(
        t,
        fit.width,
        groups,
        fit.menu == Some(crate::state::DocMenu::More),
        move |_, _, cx| {
            more_state.update(cx, |s, cx| s.toggle_doc_menu(crate::state::DocMenu::More, cx))
        },
    );
    // The zoom menu hangs under the scale button.
    let zoom_menu = (zoomable && fit.menu == Some(crate::state::DocMenu::Zoom)).then(|| {
        let item = |id: SharedString, label: String, on: bool, z: crate::doc_zoom::Zoom| {
            let st = state.clone();
            div()
                .id(id)
                .h(t.size.menu_row)
                .px(t.space.sm)
                .flex()
                .items_center()
                .rounded(t.radius.small)
                .cursor_pointer()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text)
                .when(on, |d| crate::theme::selected(t, d))
                .when(!on, |d| d.hover(|s| s.bg(t.colors.hover)))
                .child(label)
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.set_doc_zoom(z, cx)))
        };
        let mut menu = div()
            .absolute()
            .top(t.size.panel_header + t.size.panel_toolbar)
            .left(t.space.md + px(zoom_group_at + 4.))
            .w(px(140.))
            .flex()
            .flex_col()
            .p(t.space.xs)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .rounded(t.radius.card)
            .shadow(t.elevation_popover());
        for p in crate::doc_zoom::STEPS {
            menu = menu.child(item(
                format!("doc-zoom-{p}").into(),
                format!("{p}%"),
                fit.zoom == crate::doc_zoom::Zoom::Percent(p),
                crate::doc_zoom::Zoom::Percent(p),
            ));
        }
        menu = menu
            .child(item(
                "doc-zoom-width".into(),
                "Fit width".into(),
                fit.zoom == crate::doc_zoom::Zoom::FitWidth,
                crate::doc_zoom::Zoom::FitWidth,
            ))
            .child(item(
                "doc-zoom-page".into(),
                "Fit page".into(),
                fit.zoom == crate::doc_zoom::Zoom::FitPage,
                crate::doc_zoom::Zoom::FitPage,
            ));
        gpui::deferred(super::motion::unfold(menu, "doc-zoom-menu", false, false, t))
            .with_priority(3)
    });
    // The find bar sits under the toolbar.
    let header = div()
        .relative()
        .flex_none()
        .flex()
        .flex_col()
        .child(row1)
        .child(row2)
        .child(find)
        .children(zoom_menu);
    let wheel = state.clone();
    let body = div()
        .id("doc-body")
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_y_scroll()
        // Ctrl + the wheel zooms the pages, as the toolbar's buttons do.
        .on_scroll_wheel(move |ev, _, cx| {
            if ev.modifiers.control && zoomable {
                let up = match ev.delta {
                    gpui::ScrollDelta::Pixels(p) => p.y > px(0.),
                    gpui::ScrollDelta::Lines(l) => l.y > 0.,
                };
                wheel.update(cx, |s, cx| s.zoom_doc(up, cx));
                cx.stop_propagation();
            }
        });
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
        return finish_at(&fit, full_t, file, header, body, footer(), fit.width, t);
    }
    let body = if image {
        // The picture, panned by dragging.
        let (down, moved, up) = (fit.drag.clone(), fit.drag.clone(), fit.drag.clone());
        let scroll = fit.scroll.clone();
        body.track_scroll(&fit.scroll)
            .overflow_x_scroll()
            .on_mouse_down(gpui::MouseButton::Left, move |ev, _, _| {
                down.set(Some(ev.position))
            })
            .on_mouse_move(move |ev, window, _| {
                if ev.pressed_button != Some(gpui::MouseButton::Left) {
                    moved.set(None);
                    return;
                }
                if let Some(last) = moved.get() {
                    let (o, d) = (scroll.offset(), ev.position - last);
                    scroll.set_offset(gpui::point(o.x + d.x, o.y + d.y));
                    moved.set(Some(ev.position));
                    window.refresh();
                }
            })
            .on_mouse_up(gpui::MouseButton::Left, move |_, _, _| up.set(None))
            .child(super::image_view::render(file, &fit, t))
    } else if previewing_html {
        // The page, in the panel's live browser.
        body.overflow_hidden().child(div().size_full().child(browser))
    } else if editing {
        // The source in an editor: a file that is only text, or a page or
        // Markdown file switched to its source. The editor scrolls itself.
        body.overflow_hidden().child(text_editor)
    } else if source || (office.is_none() && !binary && !file.is_markdown() && !diagram) {
        // Too large to edit: the source, read-only.
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
                    // The formula bar stays under the toolbar; the sheets
                    // scroll under it.
                    let formula = div()
                        .flex_none()
                        .px(t.space.md)
                        .py(t.space.xs)
                        .min_h(t.size.panel_toolbar)
                        .flex()
                        .items_center()
                        .bg(t.extra.panel)
                        .border_b_1()
                        .border_color(t.colors.border)
                        .child(div().w_full().child(sheet_editor));
                    return finish_at(
                        &fit,
                        full_t,
                        file,
                        header.child(formula),
                        body.child(div().p(t.space.lg).child(super::sheet_view::render(
                            sheets,
                            sheet,
                            pick,
                            changed_cells,
                            sheet_edits,
                            sheet_extra.0,
                            sheet_extra.1,
                            sheet_zoom,
                            state,
                            t,
                        ))),
                        footer(),
                        fit.width,
                        t,
                    );
                }
                crate::office::Preview::Doc(d) => {
                    let text_view = text_view || d.pages.is_empty();
                    if !text_view {
                        return finish_at(
                            &fit,
                            full_t,
                            file,
                            header,
                            div()
                                .flex_1()
                                .min_h_0()
                                .flex()
                                .children(fit.thumbs.then(|| thumb_rail(d, &fit, state.clone(), t)))
                                .child(pages_view(
                                    body.bg(t.extra.inset),
                                    d,
                                    &fit,
                                    page_pick,
                                    state,
                                    t,
                                )),
                            footer(),
                            fit.width,
                            t,
                        );
                    }
                    if words_editable {
                        return finish_at(
                            &fit,
                            full_t,
                            file,
                            header,
                            body.child(office_editor),
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

    finish_at(&fit, full_t, file, header, body, footer(), fit.width, t)
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

/// The thumbnail rail: each page small, the one at the top marked; a click
/// brings that page to the top.
fn thumb_rail(
    d: &crate::office::Doc,
    fit: &Fit,
    state: Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let now = fit.scroll.top_item().min(d.pages.len().saturating_sub(1));
    let w = f32::from(t.size.thumb_rail) - 2. * f32::from(t.space.md) - 2.;
    div()
        .id("doc-thumbs")
        .flex_none()
        .w(t.size.thumb_rail)
        .h_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .items_center()
        .gap(t.space.md)
        .py(t.space.md)
        .bg(t.extra.panel)
        .border_r_1()
        .border_color(t.colors.border)
        .children(d.pages.iter().enumerate().map(|(i, page)| {
            let st = state.clone();
            let on = i == now;
            div()
                .id(("doc-thumb", i))
                .flex()
                .flex_col()
                .items_center()
                .gap(t.space.xs)
                .cursor_pointer()
                .child(
                    div()
                        .w(px(w))
                        .h(px(w * page.height as f32 / page.width.max(1) as f32))
                        .rounded(t.radius.small)
                        .overflow_hidden()
                        .border_1()
                        .border_color(if on { t.colors.primary } else { t.colors.border })
                        .child(
                            gpui::img(page.image.clone())
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .font_weight(if on { FontWeight::MEDIUM } else { FontWeight::NORMAL })
                        .text_color(if on { t.colors.text_strong } else { t.colors.text_muted })
                        .child((i + 1).to_string()),
                )
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.show_doc_page(i, cx)))
        }))
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
    let room = fit.width
        - 2. * f32::from(t.space.xl)
        - 2.
        - if fit.thumbs { f32::from(t.size.thumb_rail) } else { 0. };
    let room_h = (fit.height - 2. * f32::from(t.space.xl)).max(120.);
    let shown = d.pages.len();
    let noun = if d.slides { "Slide" } else { "Page" };
    // 100% is a page's own size; fitted views report what they draw at.
    let width_of = |page: &crate::office::Page| {
        // 100% is the page at 96 dpi: a letter or A4 sheet, a 13.3in slide.
        let natural = if d.slides { 1280. } else { 794. };
        let aspect = page.height as f32 / page.width.max(1) as f32;
        let w = fit.zoom.width(natural, aspect, room, room_h, !d.slides);
        (w, natural)
    };
    if let Some(first) = d.pages.first() {
        let (w, natural) = width_of(first);
        fit.fitted.store(
            crate::doc_zoom::percent_of(w, natural),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    let wide = d.pages.first().is_some_and(|p| width_of(p).0 > room);
    body.track_scroll(&fit.scroll)
        .when(wide, |d| d.overflow_x_scroll())
        .flex()
        .flex_col()
        .when(!wide, |d| d.items_center())
        .when(wide, |d| d.items_start())
        .gap(t.space.lg)
        .p(t.space.xl)
        .children(d.pages.iter().enumerate().map(|(i, page)| {
            let w = width_of(page).0;
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
    fit: &Fit,
    full_t: f32,
    file: &FileRef,
    header: gpui::Div,
    body: impl IntoElement,
    footer: Option<gpui::Div>,
    width: f32,
    t: &Theme,
) -> AnyElement {
    let below = fit.below;
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
            d.flex_none().w_full().h_full().border_t_1()
        })
        .flex()
        .flex_col()
        .bg(t.extra.panel)
        .border_color(border_color)
        .child(header)
        .child(body)
        .children(footer)
        // In with a short ease-out: it fades and settles 16px from the right
        // (opened from the Files tab, the push moves it instead).
        .with_animation(
            SharedString::from(format!("doc-in-{}", file.name)),
            Animation::new(Duration::from_millis(super::motion::played(
                if fit.from_panel { 1 } else { t.motion_ms },
            )))
            .with_easing(gpui::ease_out_quint()),
            |d, k| d.opacity(k).relative().left(px(16. * (1. - k))),
        )
        .into_any_element()
}
