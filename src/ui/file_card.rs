//! A file KodAct made, as a card in its reply: a small page that previews the
//! document, its title and type, and Download. A diagram's card shows the
//! diagram itself, drawn natively. Clicking the card opens the file beside
//! the chat (`doc_pane.rs`); clicking it again closes it.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::state::{AppState, FileRef};
use crate::theme::Theme;

/// The page tile: a sheet with a few lines of "text" and the file's type.
fn page(file: &FileRef, t: &Theme) -> AnyElement {
    let line = |w: f32| {
        div()
            .h(px(2.))
            .w(relative(w))
            .rounded(t.radius.line)
            .bg(t.colors.text_subtle.opacity(0.35))
    };
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
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(line(0.9))
                .child(line(0.7))
                .child(line(0.8)),
        )
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
        // An Office file or PDF shows its app's mark; anything else a page.
        .child(match file.office() {
            Some(kind) => super::file_mark::render(kind, MARK, scale),
            None => page(file, t),
        })
        .child(
            label_row(file, key, state, file.label(), t)
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
