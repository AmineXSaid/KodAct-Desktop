//! Forge's mark: a block of diamond ore, seen corner-on and lit from the top
//! left. Gunmetal stone with raised, bevelled chunks, and crystal veins that
//! glow brighter where the light falls. Two sprites: a detailed 32x32 one
//! for 32px and up (rendered from a textured cube model, see
//! `ART32`), and a hand-drawn 16x16 one that stays legible small. Every ink
//! is mixed from theme tokens.

use gpui::{div, prelude::*, Div, Hsla, Pixels, Rgba};

use crate::theme::Theme;

/// The detailed sprite: an isometric cube whose faces carry an original
/// 16x16 ore texture, supersampled down. `.` empty, `O` outline, `1`-`9`
/// stone from dark to light, `a`-`d` crystal from deep to glint.
pub(crate) const ART32: [&str; 32] = [
    "...............OO...............",
    ".............OO77OO.............",
    "...........OO776666OO...........",
    ".........OO7666667766OO.........",
    ".......OO99767767679999OO.......",
    ".....OO664477cccc67456798OO.....",
    "...OO776677777cccdd66446666OO...",
    ".OO66cccc6677cccccbcc66cccc77OO.",
    "O887777bb667776bb77666886bb7788O",
    "O567887799986776699968998679843O",
    "O5446889999986777679954678743cbO",
    "O54454677767877dd65469998843cbaO",
    "O44dc5544779987cbcc776657623c33O",
    "Odccbc4454568998667985337113365O",
    "Ocbccb445454977999753533323c353O",
    "O55bccc454446665653333133cba332O",
    "O54dcba5555443255533333cbcb3333O",
    "O64cbb54555dc44592333ccbb333333O",
    "O9654a55544cbb56413cbbcbb333333O",
    "O787554485864a56323cb2cba3333c3O",
    "O439755467887555333333c3333cba3O",
    "O744674443466766334733333cbcb33O",
    "O99738754443233644742333cbbb333O",
    "O99985664dc4354563521333ccba333O",
    ".OO886434cbcd545523333333cb33OO.",
    "...OO6524dcbccc551333333333OO...",
    ".....OO45cbb4ba5323c33333OO.....",
    ".......OO54a55463cba333OO.......",
    ".........OO454463cb33OO.........",
    "...........OO546333OO...........",
    ".............OO53OO.............",
    "...............OO...............",
];

/// The small sprite, one character per pixel; `.` is empty.
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
pub(crate) fn stone(t: &Theme, light: f32) -> Hsla {
    let c = mix(t.colors.midnight, t.colors.primary_fg, light);
    Hsla { s: c.s * 0.35, ..c }
}

pub(crate) fn ink(c: char, t: &Theme) -> Option<Hsla> {
    let k = &t.colors;
    let white = k.primary_fg;
    Some(match c {
        '1'..='9' => stone(t, 0.15 + (c as u32 - '1' as u32) as f32 * 0.062),
        'a' => mix(k.primary, k.midnight, 0.22),
        'b' => mix(k.sky, k.primary, 0.45),
        'c' => mix(k.sky, white, 0.28),
        'd' => mix(k.sky, white, 0.88),
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

/// The mark at `size`: the detailed sprite from 32px, the small one below
/// (multiples of 32 / 16 keep every pixel crisp).
pub fn mark(t: &Theme, size: Pixels) -> Div {
    let art: &[&str] = if size >= gpui::px(32.) { &ART32 } else { &ART };
    let cell = size / art.len() as f32;
    let mut root = div().relative().size(size).flex_none();
    for (y, row) in art.iter().enumerate() {
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
    use super::{ART, ART32};

    #[test]
    fn the_sprites_are_square_and_symmetric_in_shape() {
        for row in ART32 {
            assert_eq!(row.chars().count(), 32);
            let filled: Vec<bool> = row.chars().map(|c| c != '.').collect();
            let mirrored: Vec<bool> = filled.iter().rev().copied().collect();
            assert_eq!(filled, mirrored, "{row}");
        }
        for row in ART {
            assert_eq!(row.chars().count(), 16);
            let filled: Vec<bool> = row.chars().map(|c| c != '.').collect();
            let mirrored: Vec<bool> = filled.iter().rev().copied().collect();
            assert_eq!(filled, mirrored, "{row}");
        }
    }
}
