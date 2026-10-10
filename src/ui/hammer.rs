//! Forge's mark: the war hammer, drawn as a 16x16 pixel sprite (the same
//! sprite as Forge for VS Code). Its inks are mixed from theme tokens:
//! midnight outline, a blue head lit towards sky, a coral-to-marigold grip.

use gpui::{div, prelude::*, Div, Hsla, Pixels, Rgba};

use crate::theme::Theme;

/// One character per pixel, lit from the top left; `.` is empty. Head inks
/// run dark to light (O outline, 1-7); grip inks are letters (K darkest to P).
const ART: [&str; 16] = [
    ".......OOO......",
    "......O453O.....",
    ".....O4743OO....",
    ".....O6432O5O...",
    ".....O432O43OOO.",
    ".....O32O42O354O",
    "......OOO2O3753O",
    ".......PLO36432O",
    "......NJ.O4321O.",
    ".....PL...OOOO..",
    "....NJ..........",
    "...ML...........",
    ".OOK............",
    "O64O............",
    "O42O............",
    ".OO.............",
];

/// `a` moved towards `b` by `t` (0..1), in RGB.
fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: 1. }.into()
}

fn ink(c: char, t: &Theme) -> Option<Hsla> {
    let k = &t.colors;
    let white = t.colors.primary_fg;
    Some(match c {
        'O' | 'K' => k.midnight,
        '1' => mix(k.primary, k.midnight, 0.45),
        '2' => k.primary,
        '3' => mix(k.primary, k.sky, 0.5),
        '4' => k.sky,
        '5' => mix(k.sky, white, 0.55),
        '6' => mix(k.sky, white, 0.75),
        '7' => white,
        'J' => mix(k.coral, k.midnight, 0.4),
        'L' => k.coral,
        'M' => mix(k.coral, k.marigold, 0.55),
        'N' => k.marigold,
        'P' => mix(k.marigold, white, 0.6),
        _ => return None,
    })
}

/// The hammer at `size` (a multiple of 16 keeps every pixel crisp).
pub fn hammer(t: &Theme, size: Pixels) -> Div {
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
