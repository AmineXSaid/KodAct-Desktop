//! A file KodAct made, as a card in its reply: a small page that previews the
//! document, its title and type, and Download. A diagram's card shows the
//! diagram itself, drawn natively. Clicking the card opens the file beside
//! the chat (`doc_pane.rs`); clicking it again closes it.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::state::{AppState, FileRef};
use crate::theme::Theme;

/// The tile for a file that is not an Office file or a PDF: its type's
/// icon on a quiet square the size of an app's mark, so every card lines up.
fn page(file: &FileRef, t: &Theme) -> AnyElement {
    div()
        .flex_none()
        .size(px(MARK))
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .bg(t.extra.sunk)
        .border_1()
        .border_color(t.colors.border)
        .child(
            gpui::img(super::file_icon::asset(
                &file.name,
                false,
                file.path.as_deref().unwrap_or(&file.name),
            ))
            .flex_none()
            .size(t.size.icon_lg),
        )
        .into_any_element()
}

/// The widest a card gets.
const CARD: f32 = 440.;
/// An app's mark on a card: about the page tile's height.
const MARK: f32 = 40.;

/// Title, type and Download: the card's text row.
fn label_row(
    file: &FileRef,
    key: &str,
    state: Entity<AppState>,
    kind: &str,
    t: &Theme,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(t.space.md)
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(file.title()),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(format!("{kind} · {}", file.kind())),
                ),
        )
        .child(download_button(file, key, state, t))
}

/// Download: a pill with its icon, saving the file where you choose.
pub fn download_button(
    file: &FileRef,
    key: &str,
    state: Entity<AppState>,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    // The same ghost pill as the View beside it: two actions, one shape.
    let to_save = file.clone();
    crate::theme::ghost_button(t, "Download")
        .id(SharedString::from(format!("file-dl-{key}")))
        .flex_none()
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            state.update(cx, |s, cx| s.download(&to_save, cx));
        })
}

/// How a diagram shows in a reply: drawn at `zoom` times its own size (1 is
/// 100%), and whether its description is open as text. `None`: the compact
/// card (the Documents list).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiagramView {
    pub zoom: f32,
    pub text_open: bool,
}

/// The keys a reply's diagram keeps its zoom and text state under.
pub fn zoom_key(key: &str) -> SharedString {
    format!("file-diagram-{key}").into()
}
pub fn text_key(key: &str) -> SharedString {
    format!("file-diagram-text-{key}").into()
}

pub fn render(
    file: &FileRef,
    key: &str,
    state: Entity<AppState>,
    open: bool,
    writing: bool,
    scale: f32,
    view: Option<DiagramView>,
    t: &Theme,
) -> AnyElement {
    if file.is_diagram() {
        if let Some(v) = view {
            if let Some(card) = inline_diagram(file, key, state.clone(), v, scale, t) {
                return card;
            }
        }
        return diagram_card(file, key, state, open, scale, t);
    }
    // Hovered, the edge darkens; open, it stays the accent.
    let edge = if open {
        t.colors.primary.opacity(0.6)
    } else {
        t.colors.text_subtle.opacity(0.5)
    };
    let opener = state.clone();
    let to_open = file.clone();
    div()
        .id(SharedString::from(format!("file-{key}")))
        .w_full()
        .max_w(px(CARD))
        .flex()
        .items_center()
        .gap(t.space.md)
        .p(t.space.sm)
        .pr(t.space.md)
        .bg(t.colors.surface)
        .border_1()
        .border_color(if open {
            t.colors.primary.opacity(0.6)
        } else {
            t.colors.border
        })
        .rounded(t.radius.card)
        .cursor_pointer()
        .hover(move |st| st.border_color(edge))
        // No preview in the chat: the file's VS Code icon on a quiet
        // square; it opens beside the chat. (The welcome page's Recent
        // artifacts show previews.)
        .child(page(file, t))
        .child(
            label_row(
                file,
                key,
                state,
                // Being made this turn: it redraws as it changes.
                if writing { "Writing…" } else { file.label() },
                t,
            )
            .flex_1()
            .min_w_0(),
        )
        .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx)))
        .into_any_element()
}

/// A diagram in a reply, readable where it is: a bar with its name, zoom
/// (− 100% +, Fit), Open and Download, then the drawing at its own size
/// (100%: its labels as drawn), scrolling sideways when wider than the
/// column, and its description as text on request.
fn inline_diagram(
    file: &FileRef,
    key: &str,
    state: Entity<AppState>,
    v: DiagramView,
    scale: f32,
    t: &Theme,
) -> Option<AnyElement> {
    let svg = crate::diagram::extract_svg(&file.content)?;
    let svg = super::diagram_view::themed(svg, t.is_dark);
    let natural = super::diagram_view::natural_width(&svg)?;
    let frame = f32::from(t.size.column) - 2.;
    let width = (natural * v.zoom).max(80.);
    let wider = width > frame + 0.5;
    let drawing = super::diagram_view::image(&svg, width, scale)?;
    let zkey = zoom_key(key);
    let zoom_by = |id: &str, glyph: &'static str, tip: &'static str, by: Option<f32>| {
        let (state, zkey) = (state.clone(), zkey.clone());
        icon_button(&format!("{id}-{key}"), glyph, tip, t).on_click(move |_, _, cx| {
            state.update(cx, |s, cx| s.zoom_diagram(zkey.clone(), by, cx))
        })
    };
    // Fit: the zoom that makes the drawing as wide as the frame.
    let fit = (frame / natural) / v.zoom;
    let fit_state = (state.clone(), zkey.clone());
    let opener = state.clone();
    let to_open = file.clone();
    let texter = state.clone();
    let tkey = text_key(key);
    let description = crate::diagram::description(&file.content);
    let hover = t.colors.hover;
    let bar = div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .px(t.space.md)
        .py(t.space.xs)
        .border_b_1()
        .border_color(t.colors.border)
        .child(icon("icons/network.svg", t.size.icon_sm, t.colors.primary))
        // In a narrow column the hint gives way first; the title keeps
        // enough to read and shortens last.
        .child(
            div()
                .min_w(px(96.))
                .flex_shrink(0.1)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_strong)
                .child(file.title()),
        )
        .child(
            div()
                .min_w_0()
                .flex_shrink(1.)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(if wider {
                    "scroll sideways for the rest".to_string()
                } else {
                    format!("Diagram · {}", file.kind())
                }),
        )
        .child(div().flex_1())
        // − 100% +: the middle sets 100% again.
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(2.))
                .px(px(2.))
                .rounded(t.radius.pill)
                .border_1()
                .border_color(t.colors.border)
                .child(zoom_by(
                    "zoom-out",
                    "icons/minus.svg",
                    "Zoom out",
                    Some(1. / 1.25),
                ))
                .child(
                    div()
                        .id(SharedString::from(format!("zoom-reset-{key}")))
                        .min_w(px(44.))
                        .h(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.radius.pill)
                        .cursor_pointer()
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption)
                        .text_color(t.colors.text)
                        .hover(move |d| d.bg(hover))
                        .tooltip(|window, cx| {
                            gpui::component::tooltip::Tooltip::new("Its own size (100%)")
                                .build(window, cx)
                        })
                        .child(format!("{}%", (v.zoom * 100.).round()))
                        .on_click({
                            let (state, zkey) = (state.clone(), zkey.clone());
                            move |_, _, cx| {
                                state.update(cx, |s, cx| s.zoom_diagram(zkey.clone(), None, cx))
                            }
                        }),
                )
                .child(zoom_by("zoom-in", "icons/plus.svg", "Zoom in", Some(1.25))),
        )
        .child(
            div()
                .id(SharedString::from(format!("zoom-fit-{key}")))
                .flex_none()
                .h(t.size.control)
                .px(t.space.sm)
                .flex()
                .items_center()
                .rounded(t.radius.pill)
                .cursor_pointer()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .hover(move |d| d.bg(hover))
                .child("Fit")
                .on_click(move |_, _, cx| {
                    fit_state.0.update(cx, |s, cx| {
                        s.zoom_diagram(fit_state.1.clone(), Some(fit), cx)
                    })
                }),
        )
        .child(
            icon_button(
                &format!("open-{key}"),
                "icons/maximize-2.svg",
                "Open beside the chat",
                t,
            )
            .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx))),
        )
        .child(download_button(file, key, state.clone(), t));
    let body = div()
        .id(SharedString::from(format!("diagram-pan-{key}")))
        .w_full()
        .overflow_x_scroll()
        .bg(t.extra.stage)
        .child(
            div()
                .flex()
                .when(!wider, |d| d.justify_center())
                .child(div().flex_none().child(drawing)),
        );
    let text = description.map(|desc| {
        div()
            .border_t_1()
            .border_color(t.colors.border)
            .child(
                div()
                    .id(SharedString::from(format!("diagram-text-{key}")))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .cursor_pointer()
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text)
                    .child(icon(
                        if v.text_open {
                            "icons/chevron-down.svg"
                        } else {
                            "icons/chevron-right.svg"
                        },
                        t.size.icon_sm,
                        t.colors.text_muted,
                    ))
                    .child("Read as text")
                    .on_click(move |_, _, cx| {
                        texter.update(cx, |s, cx| s.toggle_diagram_code(tkey.clone(), cx))
                    }),
            )
            .when(v.text_open, |d| {
                d.child(
                    div()
                        .px(t.space.md)
                        .pb(t.space.md)
                        .text_color(t.colors.text_body)
                        .child(desc),
                )
            })
    });
    Some(
        div()
            .id(SharedString::from(format!("file-{key}")))
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .rounded(t.radius.card)
            .child(bar)
            .child(body)
            .children(text)
            .into_any_element(),
    )
}

/// A small icon button in the diagram's bar, its name as a tooltip.
fn icon_button(
    id: &str,
    path: &'static str,
    tip: &'static str,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(SharedString::from(id.to_string()))
        .flex_none()
        .size(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.pill)
        .cursor_pointer()
        .hover(move |d| d.bg(hover))
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
        .child(icon(path, t.size.icon_sm, t.colors.text_muted))
}

/// A diagram's compact card (the Documents list): the drawing on top, then
/// its title and Download.
fn diagram_card(
    file: &FileRef,
    key: &str,
    state: Entity<AppState>,
    open: bool,
    scale: f32,
    t: &Theme,
) -> AnyElement {
    let edge = if open {
        t.colors.primary.opacity(0.6)
    } else {
        t.colors.text_subtle.opacity(0.5)
    };
    let opener = state.clone();
    let to_open = file.clone();
    // The drawing sits 8px inside the card, its corners concentric with it.
    let inner = CARD - 2. - 2. * f32::from(t.space.sm);
    let drawing = crate::diagram::extract_svg(&file.content).and_then(|svg| {
        super::diagram_view::fluid(&super::diagram_view::themed(svg, t.is_dark), inner, scale)
    });
    div()
        .id(SharedString::from(format!("file-{key}")))
        .w_full()
        .max_w(px(CARD))
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .p(t.space.sm)
        .pb(t.space.sm + px(2.))
        .bg(t.colors.surface)
        .border_1()
        .border_color(if open {
            t.colors.primary.opacity(0.6)
        } else {
            t.colors.border
        })
        .rounded(t.radius.card)
        .cursor_pointer()
        .hover(move |st| st.border_color(edge))
        .child(
            div()
                .rounded(t.radius.small)
                .overflow_hidden()
                .border_1()
                .border_color(t.colors.border)
                .children(drawing),
        )
        .child(label_row(file, key, state, "Diagram", t).px(t.space.xs))
        .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx)))
        .into_any_element()
}

/// One file in a gallery: the file, the key its card state lives under,
/// and its first page once drawn.
pub struct Tile {
    pub file: FileRef,
    pub key: String,
    pub thumb: Option<std::sync::Arc<gpui::Image>>,
}

/// How long ago, as the welcome page's artifact cards date themselves:
/// "now", "5m ago", "3h ago", "2d ago" within the week, then "Nov 17".
pub fn when_label(at: std::time::SystemTime) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(at)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match secs {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        86_400..=604_799 => format!("{}d ago", secs / 86_400),
        _ => chrono::DateTime::<chrono::Local>::from(at)
            .format("%b %-d")
            .to_string(),
    }
}

/// One of the welcome page's Recent artifacts: the file's first page as a
/// sheet of paper rising from the card's well, cut by the footer, which
/// holds its title and when it was made. Nothing else: no type, no mark.
pub fn artifact(
    tile: Tile,
    when: String,
    state: Entity<AppState>,
    scale: f32,
    t: &Theme,
) -> AnyElement {
    let Tile { file, key, thumb } = tile;
    let opener = state.clone();
    let to_open = file.clone();
    let edge = t.colors.text_subtle.opacity(0.5);
    let group = SharedString::from(format!("artifact-{key}"));
    let sheet: AnyElement = match (&thumb, file.office()) {
        (Some(img), _) => gpui::img(img.clone())
            .size_full()
            .object_fit(gpui::ObjectFit::Cover)
            .into_any_element(),
        (None, Some(kind)) => div()
            .size_full()
            .flex()
            .justify_center()
            .pt(t.space.lg)
            .child(super::file_mark::render(kind, MARK, scale))
            .into_any_element(),
        // A diagram drawn, as it opens.
        (None, None) if file.is_diagram() => crate::diagram::extract_svg(&file.content)
            .and_then(|svg| {
                super::diagram_view::fluid(
                    &super::diagram_view::themed(svg, t.is_dark),
                    180.,
                    scale,
                )
            })
            .map(|d| {
                div()
                    .size_full()
                    .flex()
                    .justify_center()
                    .pt(t.space.sm)
                    .overflow_hidden()
                    .child(d)
                    .into_any_element()
            })
            .unwrap_or_else(|| text_preview(&file, t)),
        (None, None) if file.is_html() => page_preview(&file.content, t),
        (None, None) => text_preview(&file, t),
    };
    div()
        .id(SharedString::from(format!("artifact-{key}")))
        .group(group.clone())
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .overflow_hidden()
        .cursor_pointer()
        .hover(move |st| st.border_color(edge))
        .child(
            div()
                .relative()
                .w_full()
                .h(px(ARTIFACT_WELL_H))
                .overflow_hidden()
                .bg(t.artifact_well(file.path.as_deref().unwrap_or(&file.name)))
                .border_b_1()
                .border_color(t.colors.border)
                // The page, inset at the top and sides, running on past
                // the well's foot.
                .child(
                    div()
                        .absolute()
                        .top(t.space.lg)
                        .left(t.space.lg + px(2.))
                        .right(t.space.lg + px(2.))
                        .h(px(ARTIFACT_WELL_H * 2.))
                        .overflow_hidden()
                        .rounded_t(t.radius.small)
                        .bg(if thumb.is_some() {
                            gpui::white()
                        } else {
                            t.colors.surface
                        })
                        .border_1()
                        .border_color(t.colors.border)
                        .shadow(t.elevation_grain())
                        .child(sheet),
                )
                .child(
                    div()
                        .absolute()
                        .top(t.space.xs)
                        .right(t.space.xs)
                        .invisible()
                        .group_hover(group, |st| st.visible())
                        .child(
                            super::button::Button::icon(
                                SharedString::from(format!("artifact-dl-{key}")),
                                "icons/download.svg",
                                "Download",
                            )
                            .compact()
                            .on_click({
                                let (state, file) = (state.clone(), file.clone());
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    state.update(cx, |s, cx| s.download(&file, cx));
                                }
                            }),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .px(t.space.md)
                .py(t.space.sm + px(2.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.body_sm)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child(file.title()),
                )
                .child(
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(when),
                ),
        )
        .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx)))
        .into_any_element()
}

/// The height of an artifact card's well, where its page shows.
const ARTIFACT_WELL_H: f32 = 84.;

/// A text file's top in a tile: its first lines at a size that reads, a
/// Markdown file's first heading set larger.
fn text_preview(file: &FileRef, t: &Theme) -> AnyElement {
    let markdown = file.is_markdown();
    let lines: Vec<String> = file
        .content
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .take(12)
        .map(|l| l.chars().take(80).collect())
        .collect();
    div()
        .size_full()
        .px(t.space.md)
        .pt(t.space.md)
        .flex()
        .flex_col()
        .gap(px(2.))
        .children(lines.into_iter().enumerate().map(|(i, l)| {
            let heading = markdown && (i == 0 || l.starts_with('#'));
            div()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_size(px(if heading && i == 0 { 12. } else { 9. }))
                .line_height(relative(1.3))
                .font_family(if markdown {
                    t.font_reading.clone()
                } else {
                    t.font_mono.clone()
                })
                .when(heading, |d| d.font_weight(FontWeight::SEMIBOLD))
                .text_color(if heading {
                    t.colors.text_strong
                } else {
                    t.colors.text_muted
                })
                .child(if markdown {
                    l.trim_start_matches('#').trim().to_string()
                } else {
                    l
                })
        }))
        .into_any_element()
}

/// A web page in a tile as it reads, not as it is written: its title as a
/// heading, then its visible words (scripts, styles and tags left out).
fn page_preview(html: &str, t: &Theme) -> AnyElement {
    let (title, words) = page_words(html);
    div()
        .size_full()
        .px(t.space.md)
        .pt(t.space.md)
        .flex()
        .flex_col()
        .gap(px(4.))
        .children(title.map(|title| {
            div()
                .text_size(px(13.))
                .line_height(relative(1.25))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_strong)
                .child(title)
        }))
        .child(
            div()
                .text_size(px(10.))
                .line_height(relative(1.45))
                .text_color(t.colors.text_muted)
                .child(words.chars().take(400).collect::<String>()),
        )
        .into_any_element()
}

/// A page's `<title>` (or first heading) and its visible text, collapsed.
pub fn page_words(html: &str) -> (Option<String>, String) {
    let lower = html.to_ascii_lowercase();
    let between = |open: &str, close: &str| -> Option<String> {
        let a = lower.find(open)?;
        let a = a + lower[a..].find('>')? + 1;
        let b = a + lower[a..].find(close)?;
        Some(html[a..b].trim().to_string())
    };
    let title = between("<title", "</title>")
        .or_else(|| between("<h1", "</h1>"))
        .map(|t| strip_tags(&t))
        .filter(|t| !t.is_empty());
    let body_at = lower.find("<body").unwrap_or(0);
    let mut out = String::new();
    let (mut rest, mut lrest) = (&html[body_at..], &lower[body_at..]);
    // Drop what never shows: scripts, styles, the head.
    loop {
        let next = ["<script", "<style", "<head"]
            .iter()
            .filter_map(|tag| lrest.find(tag).map(|i| (i, *tag)))
            .min();
        match next {
            Some((i, tag)) => {
                out.push_str(&rest[..i]);
                let close = format!("</{}", &tag[1..]);
                match lrest[i..].find(&close) {
                    Some(j) => {
                        let end = i + j + close.len();
                        let end = end + lrest[end..].find('>').map_or(0, |k| k + 1);
                        rest = &rest[end..];
                        lrest = &lrest[end..];
                    }
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    let words = strip_tags(&out);
    let words = match &title {
        Some(t) => words
            .strip_prefix(t.as_str())
            .unwrap_or(&words)
            .trim()
            .to_string(),
        None => words,
    };
    (title, words)
}

/// Text with its tags dropped, its common entities read and its spaces collapsed.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod page_tests {
    use super::page_words;

    #[test]
    fn a_page_reads_as_its_title_and_visible_words() {
        let html = "<!doctype html><html><head><title>Release notes</title>\
            <style>body{color:red}</style></head><body><h1>Release notes</h1>\
            <script>let x = 1;</script><p>Files open &amp; edit.</p></body></html>";
        let (title, words) = page_words(html);
        assert_eq!(title.as_deref(), Some("Release notes"));
        assert_eq!(words, "Files open & edit.");
    }
}

/// Several files a reply made, framed as one list rather than a stack of
/// cards: a quiet count over rows a hairline apart, each its type's VS Code
/// icon, its name and type, and Download, a quiet icon. A row
/// opens its file beside the chat; the open one carries the selected wash.
pub fn list(
    files: Vec<(FileRef, String)>,
    state: Entity<AppState>,
    open: Option<&FileRef>,
    t: &Theme,
) -> AnyElement {
    let n = files.len();
    let rows = files.into_iter().enumerate().map(|(i, (file, key))| {
        let is_open = open == Some(&file);
        let (opener, to_open) = (state.clone(), file.clone());
        let (saver, to_save) = (state.clone(), file.clone());
        let hover = t.colors.hover;
        // Every type in the VS Code explorer's icons (Material Icon Theme),
        // bare, as a file list in an editor shows them.
        let mark = gpui::img(super::file_icon::asset(
            &file.name,
            false,
            file.path.as_deref().unwrap_or(&file.name),
        ))
        .flex_none()
        .size(t.size.icon_lg);
        let row = div()
            .id(SharedString::from(format!("file-row-{key}")))
            .flex()
            .items_center()
            .gap(t.space.md)
            .px(t.space.md)
            .py(t.space.sm)
            .cursor_pointer()
            .when(i > 0, |d| d.border_t_1().border_color(t.colors.border))
            .hover(move |st| st.bg(hover))
            .child(mark)
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
                            .text_size(t.text.body_sm)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_strong)
                            .child(file.title()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(format!("{} · {}", file.label(), file.kind())),
                    ),
            )
            .child(
                super::button::Button::icon(
                    SharedString::from(format!("file-row-dl-{key}")),
                    "icons/download.svg",
                    "Download",
                )
                .compact()
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    saver.update(cx, |s, cx| s.download(&to_save, cx));
                }),
            )
            .on_click(move |_, _, cx| opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx)));
        if is_open {
            crate::theme::selected(t, row).into_any_element()
        } else {
            row.into_any_element()
        }
    });
    div()
        .w_full()
        .max_w(px(CARD))
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .overflow_hidden()
        .shadow(t.elevation_grain())
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .px(t.space.md)
                .pt(t.space.sm)
                .pb(t.space.xs)
                .text_size(t.text.caption)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_muted)
                .child(icon("icons/folder.svg", t.size.icon_xs, t.colors.text_muted))
                .child(format!("{n} files")),
        )
        .child(div().border_t_1().border_color(t.colors.border).children(rows))
        .into_any_element()
}
