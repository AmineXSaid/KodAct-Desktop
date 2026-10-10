//! Forge's mark: a block of diamond ore, drawn as a 16x16 pixel sprite. Seen
//! corner-on: a lit top face, a mid left face and a shaded right face of
//! steel-blue stone (with darker chunks), each with a vein of diamond that
//! glows brighter where the light falls. Every ink is
//! mixed from theme tokens.

use gpui::{div, prelude::*, Div, Hsla, Pixels, Rgba};

use crate::theme::Theme;

/// One character per pixel; `.` is empty.
/// Stone: `T` top, `L` left, `R` right (lower case: a darker chunk), `O`
/// the outline. Ore, one chunky vein per face: `A` top, `B` left, `C`
/// right, `W` a glint.
const ART: [&str; 16] = [
    "......OOOO......",
    "....OOTTTTOO....",
    "..OOTTtAWTTTOO..",
    "OOTTTtAAAATTtTOO",
    "OLLTTTTAATTTTRRO",
    "OLlLLTtTTTTRRrRO",
    "OLLLLLLTTRRRRRRO",
    "OLBBLLLLRRRRRrRO",
    "OLBWBLlLRRCCRRRO",
    "OLLBLLLLRRCWCRrO",
    "OlLLLLBLRRRCRRRO",
    "OLLlLBBLRrRRRCRO",
    "OLLLLLlLRRRRRRRO",
    "..OOLLLLRRrROO..",
    "....OOLLRROO....",
    "......OOOO......",
];

/// `a` moved towards `b` by `t` (0..1), in RGB.
fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: 1. }.into()
}

/// Steel-blue stone: midnight lifted towards white, most of its blue taken out.
fn stone(t: &Theme, light: f32) -> Hsla {
    let c = mix(t.colors.midnight, t.colors.primary_fg, light);
    Hsla { s: c.s * 0.35, ..c }
}

fn ink(c: char, t: &Theme) -> Option<Hsla> {
    let k = &t.colors;
    let white = k.primary_fg;
    Some(match c {
        'O' => stone(t, 0.14),
        'T' => stone(t, 0.74),
        't' => stone(t, 0.62),
        'L' => stone(t, 0.5),
        'l' => stone(t, 0.41),
        'R' => stone(t, 0.31),
        'r' => stone(t, 0.24),
        'A' => mix(k.sky, white, 0.25),
        'B' => k.sky,
        'C' => mix(k.sky, k.primary, 0.6),
        'W' => mix(k.sky, white, 0.85),
        _ => return None,
    })
}

/// The mark at `size` (a multiple of 16 keeps every pixel crisp).
pub fn mark(t: &Theme, size: Pixels) -> Div {
    let cell = size / 16.;
    let mut root = div().relative().size(size).flex_none();
    for (y, row) in ART.iter().enumerate() {
        let chars: Vec<char> = row.chars().collect();
        let mut x = 0;
        while x < chars.len() {
            let c = chars[x];
            let start = x;
            while x < chars.len() && chars[x] == c {
                x += 1;
            }
            if let Some(color) = ink(c, t) {
                root = root.child(div().absolute().left(cell * start as f32).top(cell * y as f32).w(cell * (x - start) as f32).h(cell).bg(color));
            }
        }
    }
    root
}

#[cfg(test)]
mod tests {
    use super::ART;

    #[test]
    fn the_sprite_is_square_and_symmetric_in_shape() {
        for row in ART {
            assert_eq!(row.chars().count(), 16);
            let filled: Vec<bool> = row.chars().map(|c| c != '.').collect();
            let mirrored: Vec<bool> = filled.iter().rev().copied().collect();
            assert_eq!(filled, mirrored, "{row}");
        }
    }
}
