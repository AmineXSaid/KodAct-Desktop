//! KodAct's mark and wordmark, the same as KodAct CLI's (`tui/brand.rs`), so
//! the window and the terminal carry one brand: a cursor bar and a run
//! triangle, code then act.
//!
//! - [`symbol`]: the bar and the triangle alone, in the colour opposite the
//!   ground (lime on ink, ink on paper): the mark inside the app, at home.
//! - [`working`]: the symbol at work, KodAct's signature move: the bar and
//!   the triangle take turns, a beat each. Only the working line uses it;
//!   anything else that runs takes the running icon.
//! - [`tile`]: the symbol in its tile, which takes the opposite of the
//!   ground too: a lime tile on ink, an ink tile (a lime symbol) on paper.
//!   Never below [`TILE_MIN`]; no shadow, the colours carry the edge. About,
//!   onboarding, the opener.
//! - [`wordmark`]: the name as type, "Kod" quiet and "Act" in the brand line:
//!   the title bar's (the OS already shows the icon there).
//! - [`pixel_wordmark`]: the name in the CLI's pixel letters, "Kod" in the
//!   strong ink, "Act" lime (olive by day): About and the opener only.
//!
//! The rule: one mark per screen, and the mark's lime only touches ink.
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
#[rustfmt::skip]
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
/// The wordmark in pixels.
pub const WORD_W: usize = 6 * 7 + 5 * GAP;
pub const WORD_H: usize = 10;

/// The smallest tile: below it the bar is under 2px and the tile reads as a
/// square of colour, so smaller places take the symbol or nothing.
pub const TILE_MIN: f32 = 16.;

/// A letter of the wordmark: "Kod" or "Act".
#[derive(Clone, Copy, PartialEq)]
enum Word {
    Kod,
    Act,
}

/// Whether the wordmark has a letter's pixel at (`x`, `y`), and in which letter.
fn letter_at(x: usize, y: usize) -> Option<usize> {
    if y >= WORD_H {
        return None;
    }
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

fn svg_doc(view_box: &str, d: &str, crisp: bool) -> Vec<u8> {
    let edges = if crisp {
        r#" shape-rendering="crispEdges""#
    } else {
        ""
    };
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}"{edges}><path d="{d}"/></svg>"#)
        .into_bytes()
}

/// One word of the pixel wordmark, on the whole wordmark's canvas.
fn word_svg(layer: Word) -> Vec<u8> {
    let d = runs_path(WORD_W, WORD_H, |x, y| match layer {
        Word::Kod => letter_at(x, y).is_some_and(|g| g < 3),
        Word::Act => letter_at(x, y).is_some_and(|g| g >= 3),
    });
    svg_doc(&format!("0 0 {WORD_W} {WORD_H}"), &d, true)
}

/// A layer of the mark.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    /// The tile, whole: the glyphs are inked over it, so a glyph fading in a
    /// working beat fades into the tile.
    Tile,
    /// The cursor bar.
    Bar,
    /// The run triangle.
    Run,
}

/// The mark's geometry in a 64-unit box (KodAct CLI's proportions: bar 20 to
/// 32%, triangle 42 to 80%, both 24 to 76% high; the tile's corners rounded
/// a sixth).
fn part_path(part: Part) -> String {
    let s = 64.;
    let (top, bottom) = (0.24 * s, 0.76 * s);
    match part {
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
    }
}

/// The symbol's box inside the 64-unit tile: the glyphs and a margin, its
/// width to height [`SYMBOL_RATIO`].
const SYMBOL_BOX: &str = "10 14 44 36";
/// The symbol's height over its width.
pub const SYMBOL_RATIO: f32 = 36. / 44.;

/// The asset source's `kodact/…` paths: the wordmark's, the tile's and the
/// symbol's layers.
pub(crate) fn svg_asset(path: &str) -> Option<Vec<u8>> {
    let tile = |part| svg_doc("0 0 64 64", &part_path(part), false);
    let symbol = |part| svg_doc(SYMBOL_BOX, &part_path(part), false);
    Some(match path.strip_prefix("kodact/")? {
        "word-kod.svg" => word_svg(Word::Kod),
        "word-act.svg" => word_svg(Word::Act),
        "tile.svg" => tile(Part::Tile),
        "tile-bar.svg" => tile(Part::Bar),
        "tile-run.svg" => tile(Part::Run),
        "symbol-bar.svg" => symbol(Part::Bar),
        "symbol-run.svg" => symbol(Part::Run),
        // The run triangle alone, centred (a touch right, as a triangle's
        // weight sits left): Send.
        "run.svg" => svg_doc("0 0 16 16", "M5.2 3.2L13 8L5.2 12.8Z", false),
        _ => return None,
    })
}

/// The mark's colours, each the opposite of the ground: the tile (and the
/// symbol alone), and the glyphs inside the tile.
#[derive(Clone, Copy)]
pub struct Colors {
    pub tile: Hsla,
    pub ink: Hsla,
}

impl Colors {
    pub fn of(t: &Theme) -> Self {
        Colors {
            tile: t.extra.mark_top,
            ink: t.extra.mark_left,
        }
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

/// The mark in its tile, `size` px square (at least [`TILE_MIN`]).
#[derive(IntoElement)]
pub struct Tile {
    size: f32,
    /// While working: how far through the beat, 0..1.
    beat: Option<f32>,
}

/// The tile at rest.
pub fn tile(size: f32) -> Tile {
    Tile {
        size: size.max(TILE_MIN),
        beat: None,
    }
}

/// The tile `k` (0..1) through a working beat: the cursor bar at 0, the run
/// triangle at 0.5 (a moment the opener and onboarding animate).
pub fn tile_at_beat(size: f32, k: f32) -> Tile {
    Tile {
        size: size.max(TILE_MIN),
        beat: Some(k),
    }
}

impl RenderOnce for Tile {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = Colors::of(cx.theme());
        let (bar_a, run_a) = self.beat.map_or((1., 1.), beat_alphas);
        let layer = |path: &'static str, color: Hsla| {
            svg()
                .path(path)
                .absolute()
                .top_0()
                .left_0()
                .size(px(self.size))
                .text_color(color)
        };
        div()
            .relative()
            .flex_none()
            .size(px(self.size))
            .child(layer("kodact/tile.svg", c.tile))
            .child(layer("kodact/tile-bar.svg", c.ink).opacity(bar_a))
            .child(layer("kodact/tile-run.svg", c.ink).opacity(run_a))
    }
}

/// The symbol, the bar and the triangle without a tile, `size` px wide.
#[derive(IntoElement)]
pub struct Symbol {
    size: f32,
    beat: Option<f32>,
}

/// The symbol at rest: the mark inside the app.
pub fn symbol(size: f32) -> Symbol {
    Symbol { size, beat: None }
}

impl RenderOnce for Symbol {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = Colors::of(cx.theme()).tile;
        let (w, h) = (self.size, (self.size * SYMBOL_RATIO).round());
        let (bar_a, run_a) = self.beat.map_or((1., 1.), beat_alphas);
        let layer = |path: &'static str| {
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
            .child(layer("kodact/symbol-bar.svg").opacity(bar_a))
            .child(layer("kodact/symbol-run.svg").opacity(run_a))
    }
}

/// The length of one working beat: code, then act.
const BEAT: Duration = Duration::from_millis(1400);

/// The symbol at work: its cursor bar and run triangle take turns, a beat
/// each. The working line's, and nowhere else. Reduced motion: at rest.
pub fn working(size: f32, id: impl Into<ElementId>) -> AnyElement {
    if motion::reduced_motion() {
        return symbol(size).into_any_element();
    }
    div()
        .flex_none()
        .with_animation(id, Animation::new(BEAT).repeat(), move |d, k| {
            d.child(Symbol {
                size,
                beat: Some(k),
            })
        })
        .into_any_element()
}

/// The name set as type, `size` px: "Kod" quiet, "Act" in the brand line.
/// No tile: the title bar's, where the OS already shows the icon.
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
        .child(word("Kod", t.colors.text_muted))
        .child(word("Act", t.extra.live))
        .into_any_element()
}

/// The pixel wordmark's colours: "Kod", "Act".
pub fn word_colors(t: &Theme) -> [Hsla; 2] {
    [t.colors.text_strong, t.extra.live]
}

/// The name in KodAct CLI's pixel letters, each pixel `k` px square (whole
/// pixels keep the edges crisp); `colors` as [`word_colors`]. No shadow.
pub fn pixel_wordmark(k: f32, colors: [Hsla; 2]) -> AnyElement {
    let [kod, act] = colors;
    let (w, h) = (WORD_W as f32 * k, WORD_H as f32 * k);
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
        .child(layer("kodact/word-kod.svg", kod))
        .child(layer("kodact/word-act.svg", act))
        .into_any_element()
}

/// The tile's size beside a pixel wordmark of `k` px a pixel: a little
/// taller than the letters, so the two read as one line.
pub fn lockup_tile(k: f32) -> f32 {
    (WORD_H as f32 * k * 1.2).round().max(TILE_MIN)
}

/// The tile and the pixel wordmark side by side, as the CLI's front page
/// sets them, `k` px a pixel.
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
        assert_eq!(letter_at(3 * (7 + GAP) + 2, 0), Some(3));
        assert_eq!(WORD_W, 52);
        for g in GLYPHS {
            assert!(g.iter().all(|row| row.len() == 7));
        }
    }

    #[test]
    fn every_layer_is_an_svg_path() {
        for p in [
            "word-kod",
            "word-act",
            "tile",
            "tile-bar",
            "tile-run",
            "symbol-bar",
            "symbol-run",
            "run",
        ] {
            let svg = String::from_utf8(svg_asset(&format!("kodact/{p}.svg")).unwrap()).unwrap();
            assert!(
                svg.starts_with("<svg") && svg.contains(" d=\"M"),
                "{p} draws nothing"
            );
        }
        assert!(svg_asset("kodact/other.svg").is_none());
        assert!(svg_asset("icons/x.svg").is_none());
    }

    #[test]
    fn the_symbol_box_holds_both_glyphs() {
        // The bar starts at 12.8 and the triangle ends at 51.2, 15.36 to
        // 48.64 high: inside 10..54 by 14..50.
        let [x, y, w, h]: [f32; 4] = SYMBOL_BOX
            .split(' ')
            .map(|v| v.parse().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        assert!(x <= 12.8 && x + w >= 51.2 && y <= 15.36 && y + h >= 48.64);
        assert!((h / w - SYMBOL_RATIO).abs() < 1e-6);
    }

    #[test]
    fn no_tile_is_drawn_below_its_floor() {
        assert_eq!(tile(12.).size, TILE_MIN);
        assert_eq!(tile_at_beat(8., 0.).size, TILE_MIN);
        assert_eq!(tile(48.).size, 48.);
    }

    #[test]
    fn a_beat_hands_over_from_the_bar_to_the_triangle() {
        let (bar0, run0) = beat_alphas(0.);
        let (bar_mid, run_mid) = beat_alphas(0.5);
        assert!(bar0 > 0.99 && run0 < 0.3, "code first");
        assert!(bar_mid < 0.3 && run_mid > 0.99, "then act");
        let (bar1, run1) = beat_alphas(1.);
        assert!(
            (bar1 - bar0).abs() < 1e-4 && (run1 - run0).abs() < 1e-4,
            "and round again"
        );
    }
}
