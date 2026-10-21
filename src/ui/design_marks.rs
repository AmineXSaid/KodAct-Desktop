//! The marks of the design bar's Insert tools, drawn over the page as you
//! make them (`design::marks` keeps them; this draws them): strokes on a
//! canvas, labels and notes as text. They are in the page's CSS pixels, so
//! they follow its scroll and the zoom.

use gpui::{canvas, div, point, prelude::*, px, Bounds, Hsla, IntoElement, PathBuilder, Pixels};

use crate::design::marks::{arrow_head, Mark, Shape};
use crate::theme::Theme;

/// Where a point of the page is in the panel: less the page's scroll,
/// times the zoom.
#[derive(Clone, Copy)]
pub struct Place {
    pub scroll: (f32, f32),
    pub zoom: f32,
}

impl Place {
    pub fn at(self, (x, y): (f64, f64)) -> (f32, f32) {
        (
            (x as f32 - self.scroll.0) * self.zoom,
            (y as f32 - self.scroll.1) * self.zoom,
        )
    }
}

/// One stroke of a mark, in panel pixels from the page's top left.
fn stroke(m: &Mark, place: Place) -> Vec<Vec<(f32, f32)>> {
    let p: Vec<(f32, f32)> = m.points.iter().map(|q| place.at(*q)).collect();
    if p.len() < 2 {
        return vec![];
    }
    let (a, b) = (p[0], p[p.len() - 1]);
    match m.shape {
        Shape::Rect => vec![vec![a, (b.0, a.1), b, (a.0, b.1), a]],
        Shape::Line => vec![vec![a, b]],
        Shape::Arrow => {
            let (from, to) = (m.points[0], m.points[m.points.len() - 1]);
            let [h1, h2] = arrow_head(from, to, 14.0 / f64::from(place.zoom));
            vec![vec![a, b], vec![place.at(h1), b, place.at(h2)]]
        }
        Shape::Pencil | Shape::Pen => vec![p],
        // An oval is drawn as one (below); labels and notes are text.
        Shape::Oval | Shape::Text | Shape::Note => vec![],
    }
}

/// The marks, and the one being drawn, over the page.
pub fn render(marks: Vec<Mark>, place: Place, t: &Theme) -> impl IntoElement {
    let ink: Hsla = t.colors.primary;
    let width = px(3.);
    // The text ones, as a label or a sticky note.
    let words: Vec<_> = marks
        .iter()
        .filter(|m| m.shape.typed() && !m.points.is_empty())
        .map(|m| {
            let (x, y) = place.at(m.points[0]);
            let el = div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .text_color(ink)
                .child(m.text.clone());
            // Their words grow and shrink with the page, as on its picture
            // (a note 14px, a label 16px).
            if m.shape == Shape::Note {
                el.w(px(200. * place.zoom))
                    .px(t.space.snug)
                    .py(t.space.sm)
                    .rounded(t.radius.small)
                    .bg(t.colors.surface)
                    .border_1()
                    .border_color(ink)
                    .text_size(px(14. * place.zoom))
                    .shadow(t.elevation_popover())
            } else {
                el.whitespace_nowrap()
                    .text_size(px(16. * place.zoom))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
            }
        })
        .collect();
    let ovals: Vec<(f32, f32, f32, f32)> = marks
        .iter()
        .filter(|m| m.shape == Shape::Oval && m.points.len() >= 2)
        .map(|m| {
            let (x, y, w, h) = m.bounds();
            let (x, y) = place.at((x, y));
            (x, y, w as f32 * place.zoom, h as f32 * place.zoom)
        })
        .collect();
    let lines: Vec<Vec<(f32, f32)>> = marks.iter().flat_map(|m| stroke(m, place)).collect();
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    let at = |(x, y): (f32, f32)| {
                        point(bounds.origin.x + px(x), bounds.origin.y + px(y))
                    };
                    for line in &lines {
                        let mut b = PathBuilder::stroke(width);
                        b.move_to(at(line[0]));
                        for q in &line[1..] {
                            b.line_to(at(*q));
                        }
                        if let Ok(path) = b.build() {
                            window.paint_path(path, ink);
                        }
                    }
                    // An oval: two half arcs round its box.
                    for &(x, y, w, h) in &ovals {
                        let radii = point(px(w / 2.), px(h / 2.));
                        let (left, right) = (at((x, y + h / 2.)), at((x + w, y + h / 2.)));
                        let mut b = PathBuilder::stroke(width);
                        b.move_to(left);
                        b.arc_to(radii, px(0.), false, true, right);
                        b.arc_to(radii, px(0.), false, true, left);
                        if let Ok(path) = b.build() {
                            window.paint_path(path, ink);
                        }
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .children(words)
}
