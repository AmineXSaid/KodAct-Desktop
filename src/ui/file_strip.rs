//! A file KodAct made, as a card in its reply: a header (the kind's mark, the
//! title, what it is: "PowerPoint · 12 slides") with Download, Open in its app
//! and Open, over a strip of the parts it is made of (`strip.rs`): slides,
//! pages, sheets, sections, stretches of code. The first frame is where the
//! file opens; any frame opens it there. A file of one part shows that part
//! large instead (a deck of one slide, a one-page PDF). The card itself opens
//! the file beside the chat.

use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, Hsla, ObjectFit, SharedString};

use super::icon;
use crate::state::{AppState, FileRef};
use crate::strip::{Art, Frame, Shape, Strip};
use crate::theme::Theme;

/// The widest a card with a preview gets.
const CARD_W: f32 = 540.;
/// The widest a card without one gets (text, code): its header alone.
const FOLIO_W: f32 = 400.;
/// The rows a sheet's frame is drawn as, filled or blank.
const SHEET_ROWS: usize = 6;
/// A strip frame's height at most (two portrait pages would be too tall).
const FRAME_MAX_H: f32 = 104.;
/// A part shown large, at most this tall.
const HERO_MAX_H: f32 = 288.;
/// A one-page document's page, its top shown across the card this tall.
const HERO_PAGE_H: f32 = 210.;
/// A page's top margin, as a share of its height (an inch of eleven).
const PAGE_MARGIN: f32 = 0.08;
/// Text inside a frame: a miniature, read at a glance.
const MINI: f32 = 7.5;
/// A section's heading inside its frame.
const MINI_HEAD: f32 = 10.5;
/// The kind's mark beside the title.
const MARK: f32 = 40.;

/// A kind's mark: its icon, and the colour it is drawn in (from the theme's
/// own tokens, with their soft fill behind it).
fn look(file: &FileRef, shape: Option<Shape>, t: &Theme) -> (&'static str, Hsla, Hsla) {
    use crate::office::Kind;
    let soft = |c: Hsla| c.opacity(if t.is_dark { 0.18 } else { 0.12 });
    match (file.office(), shape) {
        (Some(Kind::Pptx), _) => (
            "icons/presentation.svg",
            t.extra.warning,
            t.extra.warning_tint,
        ),
        (Some(Kind::Docx), _) => ("icons/file-text.svg", t.code.type_, soft(t.code.type_)),
        (Some(Kind::Pdf), _) => ("icons/file-type.svg", t.extra.danger, t.extra.danger_tint),
        (Some(Kind::Xlsx), _) | (None, Some(Shape::Sheet)) => (
            "icons/file-spreadsheet.svg",
            t.extra.success,
            t.extra.success_tint,
        ),
        (None, Some(Shape::Picture)) => ("icons/image.svg", t.code.function, soft(t.code.function)),
        _ if file.is_markdown() => (
            "icons/scroll-text.svg",
            t.code.keyword,
            soft(t.code.keyword),
        ),
        _ => ("icons/file-code.svg", t.colors.text_muted, t.extra.inset),
    }
}

/// The app a document opens in, for its button's label.
fn app(file: &FileRef) -> Option<&'static str> {
    use crate::office::Kind;
    Some(match file.office()? {
        Kind::Pptx => "PowerPoint",
        Kind::Docx => "Word",
        Kind::Xlsx => "Excel",
        Kind::Pdf => "your PDF reader",
    })
}

/// A frame's width over its height.
fn aspect(shape: Shape, art: &Art) -> f32 {
    match (shape, art) {
        (Shape::Page | Shape::Picture, Art::Page { width, height, .. }) if *height > 0 => {
            *width as f32 / *height as f32
        }
        (Shape::Page, _) => 8.5 / 11.,
        (Shape::Section, _) => 4. / 3.,
        (Shape::Lines, _) => 16. / 10.,
        _ => 16. / 9.,
    }
}

/// What a frame holds, filling the box it is put in.
fn art(frame: &Frame, shape: Shape, t: &Theme) -> AnyElement {
    // Its frame is rounded with a 1px edge: the art's ground and picture
    // follow that curve themselves (a parent clips children square).
    let inner = t.radius.small - px(1.);
    let base = div().size_full().overflow_hidden().rounded(inner);
    match &frame.art {
        // A page or slide as it prints: white paper in both themes.
        Art::Page { image, .. } => base
            .bg(gpui::white())
            .child(
                gpui::img(image.clone())
                    .size_full()
                    .rounded(inner)
                    .object_fit(if shape == Shape::Slide {
                        ObjectFit::Cover
                    } else {
                        ObjectFit::Contain
                    }),
            )
            .into_any_element(),
        // Rows keep a sheet's proportions: a short sheet leaves blank rows
        // below, as it would in Excel, instead of stretching its own.
        Art::Cells(rows) => base
            .bg(t.colors.surface)
            .flex()
            .flex_col()
            .children((0..rows.len().max(SHEET_ROWS)).map(|r| {
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(r == 0, |d| d.bg(t.extra.success).rounded_t(inner))
                    .when(r > 0 && r % 2 == 0, |d| d.bg(t.extra.inset))
                    .children(
                        rows.get(r)
                            .into_iter()
                            .flatten()
                            .map(move |(text, number)| {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .px(px(3.))
                                    .flex()
                                    .items_center()
                                    .when(*number, |d| d.justify_end())
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(MINI))
                                    .font_family(t.font_mono.clone())
                                    .when(r == 0, |d| {
                                        d.font_weight(FontWeight::SEMIBOLD)
                                            .text_color(t.extra.accent_fg)
                                    })
                                    .when(r > 0, |d| d.text_color(t.colors.text))
                                    .child(text.clone())
                            }),
                    )
            }))
            .into_any_element(),
        Art::Section { heading, body } => base
            .bg(t.colors.surface)
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(MINI_HEAD))
                    .line_height(gpui::relative(1.15))
                    .font_family(t.font_display.clone())
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_strong)
                    .child(heading.clone()),
            )
            .child(
                div()
                    .text_size(px(MINI))
                    .line_height(gpui::relative(1.45))
                    .text_color(t.colors.text_muted)
                    .child(body.clone()),
            )
            .into_any_element(),
        Art::Lines(lines) => base
            .bg(t.extra.inset)
            .px(px(8.))
            .py(px(6.))
            .flex()
            .flex_col()
            .font_family(t.font_mono.clone())
            .text_size(px(MINI))
            .line_height(gpui::relative(1.5))
            .children(lines.iter().enumerate().map(|(i, l)| {
                div()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    // A definition's line in the function colour, the rest quiet.
                    .text_color(if i == 0 && is_definition(l) {
                        t.code.function
                    } else {
                        t.colors.text_muted
                    })
                    .child(indented(l))
            }))
            .into_any_element(),
    }
}

/// A line with its leading spaces kept: text layout would collapse them,
/// and code without its indentation reads wrong.
fn indented(l: &str) -> String {
    let body = l.trim_start_matches([' ', '\t']);
    let lead: usize = l[..l.len() - body.len()]
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    format!("{}{body}", "\u{a0}".repeat(lead))
}

fn is_definition(l: &str) -> bool {
    let l = l
        .trim_start_matches("pub ")
        .trim_start_matches("export ")
        .trim_start_matches("async ");
    ["def ", "fn ", "function ", "func ", "class "]
        .iter()
        .any(|k| l.starts_with(k))
}

/// The noun for what "+N" stands for.
fn more_words(shape: Shape) -> &'static str {
    match shape {
        Shape::Slide => "More slides",
        Shape::Page => "More pages",
        Shape::Sheet => "More sheets",
        Shape::Section => "More sections",
        Shape::Lines | Shape::Picture => "More",
    }
}

/// The widest a strip frame of this aspect gets: portrait pages stop at
/// `FRAME_MAX_H` tall, so a two-page PDF is not a wall of paper.
fn frame_max_w(ratio: f32) -> f32 {
    FRAME_MAX_H * ratio + 6.
}

/// A frame's box: ringed in the primary colour when it is where the file
/// opens, its art at the frame's aspect inside.
fn framed(inner: impl IntoElement, ratio: f32, ring: bool, t: &Theme) -> gpui::Div {
    let hover = t.colors.text_subtle.opacity(0.6);
    div()
        .w_full()
        .p(px(2.))
        .rounded(t.radius.small + px(2.))
        .border_1()
        .border_color(if ring {
            t.extra.accent
        } else {
            gpui::transparent_black()
        })
        .child(
            div()
                .w_full()
                .aspect_ratio(ratio)
                .overflow_hidden()
                .rounded(t.radius.small)
                .border_1()
                .border_color(t.colors.border)
                .hover(move |st| st.border_color(hover))
                .child(inner),
        )
}

fn caption(text: impl Into<SharedString>, t: &Theme) -> gpui::Div {
    div()
        .px(px(2.))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(text.into())
}

fn frame_cell(
    file: &FileRef,
    frame: &Frame,
    shape: Shape,
    first: bool,
    key: &str,
    state: Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let ratio = aspect(shape, &frame.art);
    let (f, at) = (file.clone(), frame.at);
    div()
        .id(SharedString::from(format!("strip-{key}-{at}")))
        .flex_1()
        .min_w_0()
        .max_w(px(frame_max_w(ratio)))
        .flex()
        .flex_col()
        .gap(px(6.))
        .cursor_pointer()
        .child(framed(art(frame, shape, t), ratio, first, t))
        .when(!frame.caption.is_empty(), |d| {
            d.child(caption(frame.caption.clone(), t))
        })
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            state.update(cx, |s, cx| s.open_doc_at(f.clone(), at, cx));
        })
        .into_any_element()
}

/// The strip, or its one part shown large.
fn strip_view(
    file: &FileRef,
    strip: &Strip,
    key: &str,
    state: Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let shape = strip.shape;
    let shown = strip.frames.len();
    let more = strip.total.saturating_sub(shown);
    // One part: large, across the card.
    if shown == 1 && more == 0 {
        let frame = &strip.frames[0];
        let ratio = aspect(shape, &frame.art);
        let (f, at) = (file.clone(), frame.at);
        let body: AnyElement = match (&frame.art, shape) {
            // One page: its top, across the card, as it prints, from where
            // its text starts.
            (Art::Page { image, .. }, Shape::Page) => div()
                .w_full()
                .max_h(px(HERO_PAGE_H))
                .overflow_hidden()
                .rounded(t.radius.small)
                .border_1()
                .border_color(t.colors.border)
                .bg(gpui::white())
                .child(
                    gpui::img(image.clone())
                        .w_full()
                        .rounded_t(t.radius.small - px(1.))
                        .aspect_ratio(ratio)
                        // A margin in a share of the width: the page's height
                        // is the width over its aspect.
                        .mt(gpui::relative(-PAGE_MARGIN / ratio)),
                )
                .into_any_element(),
            _ => div()
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(HERO_MAX_H * ratio))
                        .aspect_ratio(ratio)
                        .overflow_hidden()
                        .rounded(t.radius.small)
                        .border_1()
                        .border_color(t.colors.border)
                        .child(art(frame, shape, t)),
                )
                .into_any_element(),
        };
        return div()
            .id(SharedString::from(format!("strip-{key}-hero")))
            .w_full()
            .cursor_pointer()
            .child(body)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                state.update(cx, |s, cx| s.open_doc_at(f.clone(), at, cx));
            })
            .into_any_element();
    }
    let first_ratio = strip
        .frames
        .first()
        .map_or(16. / 9., |f| aspect(shape, &f.art));
    let opener = state.clone();
    let to_open = file.clone();
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(10.))
        .children(
            strip
                .frames
                .iter()
                .enumerate()
                .map(|(i, f)| frame_cell(file, f, shape, i == 0, key, state.clone(), t)),
        )
        .when(more > 0, |d| {
            d.child(
                div()
                    .id(SharedString::from(format!("strip-{key}-more")))
                    .flex_1()
                    .min_w_0()
                    .max_w(px(frame_max_w(first_ratio)))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .cursor_pointer()
                    .child(framed(
                        div()
                            .size_full()
                            .bg(t.extra.inset)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(t.text.body)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_muted)
                            .child(format!("+{more}")),
                        first_ratio,
                        false,
                        t,
                    ))
                    .child(caption(more_words(shape), t))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        opener.update(cx, |s, cx| s.open_doc_at(to_open.clone(), 0, cx));
                    }),
            )
        })
        .into_any_element()
}

/// A file's card in a reply. `strip`: built (`Some(Some)`), not drawable
/// (`Some(None)`), or still being built (`None`, a placeholder holds its
/// place).
#[allow(clippy::too_many_arguments)]
pub fn render(
    file: &FileRef,
    key: &str,
    state: Entity<AppState>,
    open: bool,
    writing: bool,
    strip: Option<Option<std::sync::Arc<Strip>>>,
    t: &Theme,
) -> AnyElement {
    let built = strip.clone().flatten();
    let (mark, fg, bg) = look(file, built.as_ref().map(|s| s.shape), t);
    let detail = if writing {
        "Writing…".to_string()
    } else {
        built
            .as_ref()
            .map(|s| s.detail.clone())
            .unwrap_or_else(|| format!("{} · {}", file.label(), file.kind()))
    };
    let on_disk = file
        .path
        .as_ref()
        .map(|p| std::path::PathBuf::from(p.as_ref()));
    let edge = if open {
        t.extra.accent.opacity(0.6)
    } else {
        t.colors.text_subtle.opacity(0.5)
    };
    let mut header = div()
        .flex()
        .items_center()
        .gap(t.space.md)
        .child(
            div()
                .flex_none()
                .size(px(MARK))
                .rounded(t.radius.button)
                .bg(bg)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(mark, t.size.icon_lg, fg)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(t.font_display.clone())
                        .text_size(t.text.subheading)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child(file.title()),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(detail),
                ),
        );
    let (st, f) = (state.clone(), file.clone());
    header = header.child(
        super::button::Button::icon(
            SharedString::from(format!("file-dl-{key}")),
            "icons/download.svg",
            "Download",
        )
        .compact()
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            st.update(cx, |s, cx| s.download(&f, cx));
        }),
    );
    if let (Some(app), true) = (app(file), on_disk.is_some()) {
        let (st, f) = (state.clone(), file.clone());
        header = header.child(
            super::button::Button::icon(
                SharedString::from(format!("file-app-{key}")),
                "icons/external-link.svg",
                format!("Open in {app}"),
            )
            .compact()
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                st.update(cx, |s, cx| s.open_in_app(&f, cx));
            }),
        );
    }
    let (st, f) = (state.clone(), file.clone());
    header = header.child(
        super::button::Button::secondary(SharedString::from(format!("file-open-{key}")), "Open")
            .compact()
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                st.update(cx, |s, cx| s.open_doc(f.clone(), cx));
            }),
    );
    // Only documents and pictures show what they look like; text and code
    // are read in the panel, so their card is the header alone (a folio).
    let previewed = file.office().is_some() || file.is_image();
    let body: Option<AnyElement> = match strip.filter(|_| previewed) {
        None if !previewed => None,
        None => Some(
            div()
                .flex()
                .gap(px(10.))
                .children((0..3).map(|i| {
                    super::skeleton::bar(
                        t,
                        SharedString::from(format!("strip-wait-{key}-{i}")),
                        gpui::relative(0.32),
                        px(62.),
                    )
                }))
                .into_any_element(),
        ),
        Some(None) => None,
        Some(Some(s)) => Some(strip_view(file, &s, key, state.clone(), t)),
    };
    let opener = state;
    let to_open = file.clone();
    div()
        .id(SharedString::from(format!("file-{key}")))
        .w_full()
        .max_w(px(if previewed { CARD_W } else { FOLIO_W }))
        .flex()
        .flex_col()
        .gap(t.space.md)
        .p(px(16.))
        .bg(t.colors.surface)
        .border_1()
        .border_color(if open {
            t.extra.accent.opacity(0.6)
        } else {
            t.colors.border
        })
        .rounded(t.radius.card)
        .shadow(t.elevation_grain())
        .cursor_pointer()
        .hover(move |st| st.border_color(edge))
        .child(header)
        .children(body)
        .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx)))
        .into_any_element()
}
