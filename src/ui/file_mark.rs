//! Which app a file belongs to, at a glance: Excel, Word, PowerPoint and PDF
//! marks in those apps' own colours and style (`assets/marks/`, drawn for
//! Forge). Rasterised at the exact device pixels they are shown at, through
//! the diagram view's cache.

use gpui::{div, prelude::*, px, AnyElement};

use crate::office::Kind;

const EXCEL: &str = include_str!("../../assets/marks/excel.svg");
const WORD: &str = include_str!("../../assets/marks/word.svg");
const POWERPOINT: &str = include_str!("../../assets/marks/powerpoint.svg");
const PDF: &str = include_str!("../../assets/marks/pdf.svg");

fn svg(kind: Kind) -> &'static str {
    match kind {
        Kind::Xlsx => EXCEL,
        Kind::Docx => WORD,
        Kind::Pptx => POWERPOINT,
        Kind::Pdf => PDF,
    }
}

/// The mark, `size` logical pixels square.
pub fn render(kind: Kind, size: f32, scale: f32) -> AnyElement {
    let drawn = super::diagram_view::image(svg(kind), size, scale);
    div()
        .flex_none()
        .size(px(size))
        .children(drawn)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_is_a_square_drawing() {
        for kind in [Kind::Xlsx, Kind::Docx, Kind::Pptx, Kind::Pdf] {
            assert_eq!(
                crate::diagram::view_size(svg(kind)),
                Some((48., 48.)),
                "{kind:?}"
            );
        }
    }
}
