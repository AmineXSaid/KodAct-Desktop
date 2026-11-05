//! KodAct's mark and wordmark, as KodAct CLI draws them (`tui/brand.rs`), so
//! the window and the terminal carry one brand.
//!
//! - [`tile`]: the mark, a lime tile holding an ink cursor bar and a run
//!   triangle: code, then act. Small, it is drawn from its geometry (crisp
//!   at icon sizes); from [`PIXEL_FROM`] up, from the CLI's 16-pixel grid,
//!   square pixels as the brand's material. A one-pixel shadow, down and
//!   right, gives it an edge on paper.
//! - [`working`]: the mark at work, KodAct's signature move: the cursor bar
//!   and the run triangle take turns, code then act, a beat each.
//! - [`wordmark`]: the mark and the name set as type, "Kod" in the strong
//!   ink and "Act" in the brand: the title bar's, as the CLI's one-row name.
//! - [`pixel_wordmark`]: the name in the CLI's pixel letters, "Kod" paper
//!   (ink by day), "Act" lime (olive by day), over a shadow one pixel down
//!   and right: About, the opener.
//!
//! Each layer is an SVG path served by the asset source (`kodact/…`, see
//! [`svg_asset`]) and tinted from the theme, so nothing here names a colour.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, svg, Animation, AnimationExt, AnyElement, App, ElementId, FontWeight,
    Hsla, IntoElement, Window,
};

use super::motion;
use crate::theme::{ActiveTheme, Theme};

/// The wordmark's letters (KodAct CLI's `GLYPHS`): seven pixels wide and ten
/// tall, capitals K and A, lowercase o, d, c and t on one baseline.
const GLYPHS: [[&str; 10]; 6] = [
    // K
    ["XX...XX", "XX..XX.", "XX.XX..", "XXXX...", "XXX....", "XXX....", "XXXX...", "XX.XX..", "XX..XX.", "XX...XX"],
    // o
    [".......", ".......", ".......", ".XXXXX.", "XXXXXXX", "XX...XX", "XX...XX", "XX...XX", "XXXXXXX", ".XXXXX."],
    // d
    [".....XX", ".....XX", ".....XX", ".XXXXXX", "XXXXXXX", "XX...XX", "XX...XX", "XX...XX", "XXXXXXX", ".XXXXXX"],
    // A
    ["..XXX..", ".XXXXX.", "XX...XX", "XX...XX", "XX...XX", "XXXXXXX", "XXXXXXX", "XX...XX", "XX...XX", "XX...XX"],
    // c
    [".......", ".......", ".......", ".XXXXXX", "XXXXXXX", "XX.....", "XX.....", "XX.....", "XXXXXXX", ".XXXXXX"],
    // t
    [".......", ".XX....", ".XX....", "XXXXXX.", "XXXXXX.", ".XX....", ".XX....", ".XX....", ".XXXXXX", "..XXXXX"],
];

/// Pixels between letters.
const GAP: usize = 2;
/// The wordmark in pixels, without its shadow.
pub const WORD_W: usize = 6 * 7 + 5 * GAP;
pub const WORD_H: usize = 10;

/// From this size up the tile is drawn from the CLI's pixel grid.
pub const PIXEL_FROM: f32 = 40.;
/// The pixel tile's grid.
const GRID: usize = 16;

/// A layer of the wordmark.
#[derive(Clone, Copy, PartialEq)]
enum Word {
    /// "Kod".
    Kod,
    /// "Act".
    Act,
    /// Every letter, one pixel down and right.
    Shadow,
}

/// Whether the wordmark has a letter's pixel at (`x`, `y`), and in which letter.
fn letter_at(x: isize, y: isize) -> Option<usize> {
    if x < 0 || y < 0 || y >= WORD_H as isize {
        return None;
    }
    let (x, y) = (x as usize, y as usize);
    let step = 7 + GAP;
    let (gi, gx) = (x / step, x % step);
    (gi < GLYPHS.len() && gx < 7 && GLYPHS[gi][y].as_bytes()[gx] == b'X').then_some(gi)
}

/// Rows of pixels as an SVG path, each run of filled pixels one rectangle.
fn runs_path(w: usize, h: usize, filled: impl Fn(usize, usize) -> bool) -> String {
    let mut d = String::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            if !filled(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && filled(x, y) {
                x += 1;
            }
            d.push_str(&format!("M{start} {y}h{}v1h-{}Z", x - start, x - start));
        }
    }
    d
}

fn svg_doc(w: usize, h: usize, d: &str) -> Vec<u8> {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" shape-rendering="crispEdges"><path d="{d}"/></svg>"#
    )
    .into_bytes()
}

/// One layer of the wordmark, on a canvas one pixel larger than the letters
/// each way, so every layer lines up with the shadow.
fn word_svg(layer: Word) -> Vec<u8> {
    let (w, h) = (WORD_W + 1, WORD_H + 1);
    let d = runs_path(w, h, |x, y| {
        let (x, y) = (x as isize, y as isize);
        match layer {
            Word::Kod => letter_at(x, y).is_some_and(|g| g < 3),
            Word::Act => letter_at(x, y).is_some_and(|g| g >= 3),
            Word::Shadow => letter_at(x - 1, y - 1).is_some(),
        }
    });
    svg_doc(w, h, &d)
}

/// The tile on KodAct CLI's grid (`mark_rows`): `M` the tile, `K` ink.
fn tile_rows(n: usize) -> Vec<Vec<u8>> {
    let n = n.max(8);
    let f = |v: f32| (v * n as f32).round() as usize;
    let bar0 = f(0.20);
    let bar1 = f(0.32).max(bar0 + 1);
    let tri0 = f(0.42).max(bar1 + 1);
    let tri1 = f(0.80).max(tri0 + 2);
    let (top, bottom) = (f(0.24), f(0.76));
    let mid = (top + bottom) as f32 / 2.0;
    let half = (bottom - top) as f32 / 2.0;
    (0..n)
        .map(|y| {
            (0..n)
                .map(|x| {
                    let r = if n >= 16 { 2 } else { 1 };
                    let (cx, cy) = (x.min(n - 1 - x), y.min(n - 1 - y));
                    if cx + cy < r {
                        return b'.';
                    }
                    let inside = y >= top && y < bottom;
                    let bar = inside && x >= bar0 && x < bar1;
                    let reach = 1.0 - ((y as f32 + 0.5 - mid).abs() / half);
                    let tri = inside
                        && x >= tri0
                        && (x as f32) < tri0 as f32 + (tri1 - tri0) as f32 * reach;
                    if bar {
                        b'B'
                    } else if tri {
                        b'R'
                    } else {
                        b'M'
                    }
                })
                .collect()
        })
        .collect()
}

/// A layer of the tile.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    /// The lime tile, whole: the glyphs are inked over it, so a glyph
    /// fading in a working beat fades into the tile.
    Tile,
    /// The cursor bar.
    Bar,
    /// The run triangle.
    Run,
}

/// The tile's layers on the pixel grid (the tile's outline is its shadow too).
fn pixel_tile_svg(part: Part) -> Vec<u8> {
    let rows = tile_rows(GRID);
    svg_doc(
        GRID,
        GRID,
        &runs_path(GRID, GRID, |x, y| match part {
            Part::Tile => rows[y][x] != b'.',
            Part::Bar => rows[y][x] == b'B',
            Part::Run => rows[y][x] == b'R',
        }),
    )
}

/// The tile's layers drawn from its geometry, in a 64-unit box: the same
/// proportions as the grid (bar 20 to 32%, triangle 42 to 80%, both 24 to
/// 76% high), corners rounded a sixth.
fn vector_tile_svg(part: Part) -> Vec<u8> {
    let s = 64.;
    let (top, bottom) = (0.24 * s, 0.76 * s);
    let d = match part {
        Part::Tile => {
            let r = s / 6.;
            format!(
                "M{r} 0H{a}A{r} {r} 0 0 1 {s} {r}V{a}A{r} {r} 0 0 1 {a} {s}H{r}A{r} {r} 0 0 1 0 {a}V{r}A{r} {r} 0 0 1 {r} 0Z",
                a = s - r,
            )
        }
        Part::Bar => format!("M{} {top}H{}V{bottom}H{}Z", 0.20 * s, 0.32 * s, 0.20 * s),
        Part::Run => format!(
            "M{t0} {top}L{t1} {mid}L{t0} {bottom}Z",
            t0 = 0.42 * s,
            t1 = 0.80 * s,
            mid = s / 2.
        ),
    };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="{d}"/></svg>"#
    )
    .into_bytes()
}

fn svg_doc_vector(d: &str) -> Vec<u8> {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="{d}"/></svg>"#)
        .into_bytes()
}

/// The asset source's `kodact/…` paths: the wordmark's and the tile's layers.
pub(crate) fn svg_asset(path: &str) -> Option<Vec<u8>> {
    Some(match path.strip_prefix("kodact/")? {
        "word-kod.svg" => word_svg(Word::Kod),
        "word-act.svg" => word_svg(Word::Act),
        "word-shadow.svg" => word_svg(Word::Shadow),
        "tile.svg" => vector_tile_svg(Part::Tile),
        "tile-bar.svg" => vector_tile_svg(Part::Bar),
        "tile-run.svg" => vector_tile_svg(Part::Run),
        "tile-px.svg" => pixel_tile_svg(Part::Tile),
        "tile-px-bar.svg" => pixel_tile_svg(Part::Bar),
        "tile-px-run.svg" => pixel_tile_svg(Part::Run),
        // The run triangle alone, centred (a touch right, as a triangle's
        // weight sits left): Send.
        "run.svg" => svg_doc_vector("M5.2 3.2L13 8L5.2 12.8Z"),
        _ => return None,
    })
}

/// The mark's colours: the tile, the ink of its glyphs, its shadow.
#[derive(Clone, Copy)]
pub struct Colors {
    pub tile: Hsla,
    pub ink: Hsla,
    pub shadow: Hsla,
}

impl Colors {
    pub fn of(t: &Theme) -> Self {
        Colors {
            tile: t.extra.mark_top,
            ink: t.extra.mark_left,
            shadow: t.extra.mark_right,
        }
    }
}

/// The mark, `size` px square (its shadow just outside, down and right).
#[derive(IntoElement)]
pub struct Tile {
    size: f32,
    /// While working: how far through the beat, 0..1 (`working`).
    beat: Option<f32>,
}

/// The mark at rest.
pub fn tile(size: f32) -> Tile {
    Tile { size, beat: None }
}

/// The mark `k` (0..1) through a working beat: the cursor bar at 0, the
/// run triangle at 0.5 (a moment the opener and the first run animate).
pub fn tile_at_beat(size: f32, k: f32) -> Tile {
    Tile {
        size,
        beat: Some(k),
    }
}

/// How the glyphs share a working beat: the cursor bar, then the run
/// triangle, each easing between full and faint (code, then act).
fn beat_alphas(k: f32) -> (f32, f32) {
    let wave = 0.5 - 0.5 * (k * std::f32::consts::TAU).cos(); // 0 → 1 → 0
    let eased = wave * wave * (3. - 2. * wave);
    let faint = 0.28;
    (1. - (1. - faint) * eased, faint + (1. - faint) * eased)
}

impl RenderOnce for Tile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = Colors::of(cx.theme());
        let pixel = self.size >= PIXEL_FROM;
        let (tile, bar, run) = if pixel {
            ("kodact/tile-px.svg", "kodact/tile-px-bar.svg", "kodact/tile-px-run.svg")
        } else {
            ("kodact/tile.svg", "kodact/tile-bar.svg", "kodact/tile-run.svg")
        };
        // The shadow: one grid pixel, or a sixteenth of the mark, at least 1px.
        let off = (self.size / GRID as f32).round().max(1.);
        let (bar_a, run_a) = self.beat.map_or((1., 1.), beat_alphas);
        let layer = |path: &'static str, color: Hsla, at: f32| {
            svg()
                .path(path)
                .absolute()
                .top(px(at))
                .left(px(at))
                .size(px(self.size))
                .text_color(color)
        };
        div()
            .relative()
            .flex_none()
            .size(px(self.size))
            .child(layer(tile, c.shadow, off))
            .child(layer(tile, c.tile, 0.))
            .child(layer(bar, c.ink, 0.).opacity(bar_a))
            .child(layer(run, c.ink, 0.).opacity(run_a))
    }
}

/// The length of one working beat: code, then act.
const BEAT: Duration = Duration::from_millis(1400);

/// The mark at work: its cursor bar and run triangle take turns, a beat
/// each. Reduced motion: the mark at rest.
pub fn working(size: f32, id: impl Into<ElementId>) -> AnyElement {
    if motion::reduced_motion() {
        return tile(size).into_any_element();
    }
    div()
        .flex_none()
        .size(px(size))
        .with_animation(id, Animation::new(BEAT).repeat(), move |d, k| {
            d.child(tile_at_beat(size, k))
        })
        .into_any_element()
}

/// The mark and the name set as type, `size` px: "Kod" in the strong ink,
/// "Act" in the brand (as KodAct CLI's one-row name).
pub fn wordmark(size: f32, t: &Theme) -> AnyElement {
    let word = |s: &'static str, color: Hsla| {
        div()
            .font_family(t.font_sans.clone())
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(size))
            .line_height(px(size))
            .text_color(color)
            .child(s)
    };
    div()
        .flex()
        .items_center()
        .gap(px((size * 0.45).round()))
        .child(tile((size * 1.15).round()))
        .child(
            div()
                .flex()
                .child(word("Kod", t.colors.text_strong))
                .child(word("Act", t.extra.live)),
        )
        .into_any_element()
}

/// The pixel wordmark's colours: "Kod", "Act", the shadow.
pub fn word_colors(t: &Theme) -> [Hsla; 3] {
    [t.colors.text_strong, t.extra.live, t.extra.brand_shadow]
}

/// The name in KodAct CLI's pixel letters, each pixel `k` px square (whole
/// pixels keep the edges crisp), over its shadow; `colors` as
/// [`word_colors`].
pub fn pixel_wordmark(k: f32, colors: [Hsla; 3]) -> AnyElement {
    let [kod, act, shadow] = colors;
    let (w, h) = ((WORD_W + 1) as f32 * k, (WORD_H + 1) as f32 * k);
    let layer = |path: &'static str, color: Hsla| {
        svg()
            .path(path)
            .absolute()
            .top_0()
            .left_0()
            .w(px(w))
            .h(px(h))
            .text_color(color)
    };
    div()
        .relative()
        .flex_none()
        .w(px(w))
        .h(px(h))
        .child(layer("kodact/word-shadow.svg", shadow))
        .child(layer("kodact/word-kod.svg", kod))
        .child(layer("kodact/word-act.svg", act))
        .into_any_element()
}

/// The tile's size beside a pixel wordmark of `k` px a pixel: as tall as
/// the letters and their shadow, on whole pixels of its own grid.
pub fn lockup_tile(k: f32) -> f32 {
    (GRID as f32 * ((WORD_H + 1) as f32 * k / GRID as f32).round()).max(PIXEL_FROM)
}

/// The pixel tile and the pixel wordmark side by side, as the CLI's front
/// page sets them, `k` px a pixel.
pub fn lockup(k: f32, t: &Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(3. * k))
        .child(tile(lockup_tile(k)))
        .child(pixel_wordmark(k, word_colors(t)))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_letters_are_the_clis() {
        // K's first row, and "Act" starting at the fourth glyph.
        assert_eq!(letter_at(0, 0), Some(0));
        assert_eq!(letter_at(2, 0), None);
        assert_eq!(letter_at((3 * (7 + GAP)) as isize + 2, 0), Some(3));
        assert_eq!(WORD_W, 52);
        for g in GLYPHS {
            assert!(g.iter().all(|row| row.len() == 7));
        }
    }

    #[test]
    fn every_layer_is_an_svg_path() {
        for p in [
            "word-kod", "word-act", "word-shadow", "tile", "tile-bar", "tile-run", "tile-px",
            "tile-px-bar", "tile-px-run", "run",
        ] {
            let svg = String::from_utf8(svg_asset(&format!("kodact/{p}.svg")).unwrap()).unwrap();
            assert!(svg.starts_with("<svg") && svg.contains(" d=\"M"), "{p} draws nothing");
        }
        assert!(svg_asset("kodact/other.svg").is_none());
        assert!(svg_asset("icons/x.svg").is_none());
    }

    #[test]
    fn the_pixel_tile_has_its_bar_and_triangle_inside_the_corners() {
        let rows = tile_rows(GRID);
        let count = |c: u8| rows.iter().flatten().filter(|&&p| p == c).count();
        assert!(count(b'B') >= 8 && count(b'R') >= 12, "the glyphs show");
        // Rounded: the corner pixels are empty.
        assert_eq!(rows[0][0], b'.');
        assert_eq!(rows[GRID - 1][GRID - 1], b'.');
        // The bar sits left of the triangle on every row it is on.
        for row in &rows {
            if let (Some(b), Some(r)) = (
                row.iter().rposition(|&p| p == b'B'),
                row.iter().position(|&p| p == b'R'),
            ) {
                assert!(b < r);
            }
        }
    }

    #[test]
    fn a_beat_hands_over_from_the_bar_to_the_triangle() {
        let (bar0, run0) = beat_alphas(0.);
        let (bar_mid, run_mid) = beat_alphas(0.5);
        assert!(bar0 > 0.99 && run0 < 0.3, "code first");
        assert!(bar_mid < 0.3 && run_mid > 0.99, "then act");
        let (bar1, run1) = beat_alphas(1.);
        assert!((bar1 - bar0).abs() < 1e-4 && (run1 - run0).abs() < 1e-4, "and round again");
    }
}
