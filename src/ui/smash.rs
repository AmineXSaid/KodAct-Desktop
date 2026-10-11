//! Forge's logo moment: the hammer smashes the diamond-ore cube.
//!
//! One shot, about 2.6s, drawn natively (GPUI has no DOM, and draws SVG
//! files as still, single-colour icons, so a CSS- or GSAP-animated SVG would
//! not move here):
//!
//! 1. anticipation: the hammer lifts and rotates back;
//! 2. the swing: a sharply accelerating overhead arc, with a two-ghost trail;
//! 3. impact: a blue-white flash and pixel cross, a micro-shake of the cube
//!    alone, pixel sparks, and cracks running out from the strike;
//! 4. the cube breaks along those cracks into its own pixel chunks (a
//!    Voronoi cut of the sprite, so every chunk keeps the cube's exact
//!    pixels), each with its own direction, velocity, spin, drag and
//!    gravity, plus glowing cyan shards and drifting motes;
//! 5. everything slows and fades; the hammer returns to rest and the cube is
//!    whole again, so the moment ends on the logo.
//!
//! Only SVG elements can rotate in GPUI, and an SVG draws in one colour, so
//! each sprite (hammer, every chunk, the shards) is served as one generated
//! SVG layer per ink (`svg_asset`), tinted with that ink and stacked; rotating
//! the stack rotates the whole sprite. The art is the original pixel art:
//! the hammer from Forge for VS Code, the cube from `mark.rs`. `scene(t)` is a
//! pure function of time, so any frame can be drawn and inspected.

use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::sync::LazyLock;

use gpui::{div, point, prelude::*, px, radians, svg, AnimationExt, AnyElement, BoxShadow, Div, Hsla, Pixels, SharedString, Transformation};

use super::mark::{self, ART32};
use crate::theme::Theme;

/// The whole moment, in milliseconds.
pub const DURATION_MS: f32 = 2600.;

// ------------------------------------------------------------------ art

/// Forge's hammer, as Forge for VS Code draws it (16x16, lit top left).
const HAMMER: [&str; 16] = [
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
/// Where the hand holds it: the pivot of every swing, in art units.
const GRIP: (f32, f32) = (2.0, 13.5);
/// The striking head's centre, in art units.
const HEAD: (f32, f32) = (10.5, 4.5);

fn hammer_ink(c: char, t: &Theme) -> Option<Hsla> {
    let k = &t.colors;
    let white = k.primary_fg;
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

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b): (gpui::Rgba, gpui::Rgba) = (a.into(), b.into());
    gpui::Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: 1. }.into()
}

/// A crystal shard: a small plus of diamond with a glint.
const SHARD: [&str; 5] = [".bb..", "bcdb.", ".bcb.", "..b..", "....."];

/// Where the cube breaks: seeds for a Voronoi cut of the 32x32 sprite, spread
/// over its three faces. The first lies near the strike.
const SEEDS: [(f32, f32); 9] = [(13., 6.), (20., 8.), (7., 11.), (25., 13.), (5., 20.), (12., 18.), (21., 19.), (27., 23.), (15., 27.)];
/// Where the hammer lands on the cube, in its 32x32 grid (top face, left).
const STRIKE: (f32, f32) = (12., 7.);

struct Chunks {
    /// The chunk each pixel belongs to (by row, column); `None` where empty.
    of: Vec<Vec<Option<usize>>>,
    /// Each chunk's centre, in grid units.
    centre: Vec<(f32, f32)>,
}

static CHUNKS: LazyLock<Chunks> = LazyLock::new(|| {
    let n = ART32.len();
    let mut of = vec![vec![None; n]; n];
    let mut sum = vec![(0f32, 0f32, 0f32); SEEDS.len()];
    for (y, row) in ART32.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            if c == '.' {
                continue;
            }
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let i = (0..SEEDS.len()).min_by(|&a, &b| dist2(SEEDS[a], (px, py)).total_cmp(&dist2(SEEDS[b], (px, py)))).unwrap();
            of[y][x] = Some(i);
            sum[i] = (sum[i].0 + px, sum[i].1 + py, sum[i].2 + 1.);
        }
    }
    let centre = sum.iter().map(|(x, y, k)| (x / k.max(1.), y / k.max(1.))).collect();
    Chunks { of, centre }
});

fn dist2(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

// --------------------------------------------------------- generated SVGs

/// One ink of a sprite, as an SVG whose view box is centred on `centre`
/// (so the element rotates about it). `radius` is half the view box.
fn layer(pixels: impl Iterator<Item = (usize, usize)>, centre: (f32, f32), radius: f32) -> Vec<u8> {
    let mut rows: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (x, y) in pixels {
        rows.entry(y).or_default().push(x);
    }
    let mut rects = String::new();
    for (y, mut xs) in rows {
        xs.sort_unstable();
        let mut i = 0;
        while i < xs.len() {
            let start = xs[i];
            while i + 1 < xs.len() && xs[i + 1] == xs[i] + 1 {
                i += 1;
            }
            rects.push_str(&format!("<rect x=\"{start}\" y=\"{y}\" width=\"{}\" height=\"1\"/>", xs[i] - start + 1));
            i += 1;
        }
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" shape-rendering=\"crispEdges\">{rects}</svg>",
        centre.0 - radius,
        centre.1 - radius,
        radius * 2.,
        radius * 2.
    )
    .into_bytes()
}

fn inks(art: &[&str], keep: impl Fn(usize, usize) -> bool) -> Vec<char> {
    let mut found: Vec<char> = vec![];
    for (y, row) in art.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            if c != '.' && keep(x, y) && !found.contains(&c) {
                found.push(c);
            }
        }
    }
    found
}

fn pixels_of<'a>(art: &'a [&'a str], ink: char, keep: impl Fn(usize, usize) -> bool + 'a) -> impl Iterator<Item = (usize, usize)> + 'a {
    art.iter().enumerate().flat_map(move |(y, row)| row.chars().enumerate().filter(move |(_, c)| *c == ink).map(move |(x, _)| (x, y))).filter(move |(x, y)| keep(*x, *y))
}

/// Serve `smash/...` assets: the sprite layers, generated from the pixel art.
pub fn svg_asset(path: &str) -> Option<Vec<u8>> {
    let rest = path.strip_prefix("smash/")?.strip_suffix(".svg")?;
    let parts: Vec<&str> = rest.split('/').collect();
    let ink = |s: &str| s.parse::<u32>().ok().and_then(char::from_u32);
    match parts.as_slice() {
        ["hammer", c] => {
            let c = ink(c)?;
            Some(layer(pixels_of(&HAMMER, c, |_, _| true), GRIP, 16.))
        }
        ["chunk", i, c] => {
            let (i, c): (usize, char) = (i.parse().ok()?, ink(c)?);
            let chunks = &*CHUNKS;
            Some(layer(pixels_of(&ART32, c, move |x, y| chunks.of[y][x] == Some(i)), chunks.centre[i], 16.))
        }
        ["cracks", band] => {
            let band: usize = band.parse().ok()?;
            Some(layer(crack_pixels(band).into_iter(), (16., 16.), 16.))
        }
        ["shard", c] => {
            let c = ink(c)?;
            Some(layer(pixels_of(&SHARD, c, |_, _| true), (2.5, 2.5), 2.5))
        }
        _ => None,
    }
}

/// Pixels on the seams between chunks, in three bands outward from the
/// strike, so the cracks can run across the cube.
fn crack_pixels(band: usize) -> Vec<(usize, usize)> {
    let chunks = &*CHUNKS;
    let n = ART32.len();
    let mut out = vec![];
    for y in 0..n {
        for x in 0..n {
            let Some(here) = chunks.of[y][x] else { continue };
            let seam = [(1i32, 0i32), (0, 1)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                nx < n as i32 && ny < n as i32 && chunks.of[ny as usize][nx as usize].is_some_and(|o| o != here)
            });
            if !seam {
                continue;
            }
            let d = dist2(STRIKE, (x as f32, y as f32)).sqrt();
            let b = if d < 8. { 0 } else if d < 15. { 1 } else { 2 };
            if b == band {
                out.push((x, y));
            }
        }
    }
    out
}

fn ink_path(prefix: &str, c: char) -> SharedString {
    format!("smash/{prefix}/{}.svg", c as u32).into()
}

// ----------------------------------------------------------------- timing

fn clamp01(x: f32) -> f32 {
    x.clamp(0., 1.)
}
fn span(t: f32, from: f32, to: f32) -> f32 {
    clamp01((t - from) / (to - from))
}
fn ease_in_out(x: f32) -> f32 {
    if x < 0.5 { 4. * x * x * x } else { 1. - (-2. * x + 2.).powi(3) / 2. }
}
fn ease_out(x: f32) -> f32 {
    1. - (1. - x).powi(3)
}

/// Beats of the moment, in ms.
const LIFT: (f32, f32) = (80., 380.);
const SWING: (f32, f32) = (380., 500.);
const IMPACT: f32 = 500.;
const SPLIT: f32 = 640.;
const RETURN: (f32, f32) = (1950., 2450.);
const REFORM: (f32, f32) = (2150., 2550.);

/// The hammer's rotation (degrees, clockwise) and lift (px) at `t`.
fn hammer_pose(t: f32) -> (f32, f32) {
    let (rest, back, hit) = (-28., 27., -147.);
    let rot = if t < LIFT.0 {
        rest
    } else if t < LIFT.1 {
        rest + (back - rest) * ease_in_out(span(t, LIFT.0, LIFT.1))
    } else if t < SWING.1 {
        // Accelerates all the way into the cube.
        back + (hit - back) * span(t, SWING.0, SWING.1).powi(3)
    } else if t < 600. {
        hit + 13. * ease_out(span(t, IMPACT, 600.))
    } else if t < 900. {
        hit + 13. - 6. * ease_in_out(span(t, 600., 900.))
    } else if t < RETURN.0 {
        hit + 7.
    } else {
        hit + 7. + (rest - hit - 7.) * ease_in_out(span(t, RETURN.0, RETURN.1))
    };
    let lift = if t < LIFT.1 { -4. * ease_in_out(span(t, LIFT.0, LIFT.1)) } else { -4. * (1. - span(t, SWING.0, SWING.1)) };
    (rot, lift)
}

/// A small, fixed pseudo-random number in -1..1 for `i` and `salt`.
fn noise(i: usize, salt: u32) -> f32 {
    let mut x = (i as u32).wrapping_mul(2_654_435_761) ^ salt.wrapping_mul(40_503);
    x ^= x >> 13;
    x = x.wrapping_mul(1_274_126_177);
    ((x >> 8) as f32 / (1u32 << 24) as f32) * 2. - 1.
}

// ---------------------------------------------------------------- drawing

/// Canvas size, and where things sit on it (px).
pub const SIZE: (f32, f32) = (240., 176.);
const CUBE_CELL: f32 = 2.;
const CUBE_AT: (f32, f32) = (72., 72.); // top-left of the 64px cube
const HAMMER_CELL: f32 = 4.;

fn strike_px() -> (f32, f32) {
    (CUBE_AT.0 + STRIKE.0 * CUBE_CELL, CUBE_AT.1 + STRIKE.1 * CUBE_CELL)
}

/// The grip's position, chosen so the head lands on the strike point.
fn pivot_px() -> (f32, f32) {
    let (hx, hy) = (HEAD.0 - GRIP.0, HEAD.1 - GRIP.1);
    let len = (hx * hx + hy * hy).sqrt() * HAMMER_CELL;
    let base = hy.atan2(hx);
    let at_hit = base + (-147f32).to_radians();
    let (sx, sy) = strike_px();
    (sx - len * at_hit.cos(), sy - len * at_hit.sin() - 6.)
}

/// A stack of tinted layers, rotated together about the element's centre.
fn sprite(layers: impl Iterator<Item = (SharedString, Hsla)>, size: Pixels, angle_deg: f32, opacity: f32) -> Div {
    let mut d = div().absolute().size(size);
    for (path, color) in layers {
        d = d.child(
            svg()
                .path(path)
                .absolute()
                .top_0()
                .left_0()
                .size(size)
                .text_color(color.opacity(color.a * opacity))
                .with_transformation(Transformation::rotate(radians(angle_deg.to_radians()))),
        );
    }
    d
}

fn hammer(t: &Theme, rot: f32, lift: f32, opacity: f32) -> Div {
    let size = px(32. * HAMMER_CELL);
    let (px_, py_) = pivot_px();
    let layers = inks(&HAMMER, |_, _| true).into_iter().filter_map(|c| hammer_ink(c, t).map(|ink| (ink_path("hammer", c), ink)));
    sprite(layers, size, rot, opacity).left(px(px_ - 16. * HAMMER_CELL)).top(px(py_ - 16. * HAMMER_CELL + lift))
}

/// The scene at `t` ms: everything positioned on a transparent canvas.
pub fn scene(t: &Theme, ms: f32) -> Div {
    let mut root = div().relative().w(px(SIZE.0)).h(px(SIZE.1)).flex_none();
    let chunks = &*CHUNKS;
    let (sx, sy) = strike_px();

    // The cube: whole until it splits (shaking at impact), then in pieces;
    // whole again at the end.
    let shake = if (IMPACT..IMPACT + 130.).contains(&ms) {
        let k = 1. - span(ms, IMPACT, IMPACT + 130.);
        let w = (ms - IMPACT) / 1000. * 2. * PI * 38.;
        (1.8 * k * w.sin(), 1.2 * k * (w * 1.3).cos())
    } else {
        (0., 0.)
    };
    let ts = ((ms - SPLIT) / 1000.).max(0.);
    let fade = 1. - ease_in_out(span(ms, 1300., 2050.));
    for i in 0..SEEDS.len() {
        let (cx, cy) = chunks.centre[i];
        let (mut x, mut y, mut rot, mut alpha) = (CUBE_AT.0 + cx * CUBE_CELL + shake.0, CUBE_AT.1 + cy * CUBE_CELL + shake.1, 0., 1.);
        if ms >= SPLIT {
            // Out from the strike, kicked up, slowed by drag, pulled down.
            let (dx, dy) = (cx - STRIKE.0, cy - STRIKE.1);
            let len = (dx * dx + dy * dy).sqrt().max(0.5);
            let speed = 95. + 40. * noise(i, 1);
            let (vx, vy) = (dx / len * speed + 25. * noise(i, 2), dy / len * speed * 0.6 - 95. - 35. * noise(i, 3));
            let tau = 0.55;
            let travel = tau * (1. - (-ts / tau).exp());
            x += vx * travel;
            y += vy * travel + 0.5 * 150. * ts * ts;
            rot = (260. + 200. * noise(i, 4)) * noise(i, 5).signum() * travel;
            alpha = fade;
        }
        if ms >= REFORM.0 {
            // Whole again: the cube fades back in at rest.
            alpha = ease_in_out(span(ms, REFORM.0, REFORM.1));
            (x, y, rot) = (CUBE_AT.0 + cx * CUBE_CELL, CUBE_AT.1 + cy * CUBE_CELL, 0.);
        }
        if alpha <= 0.01 {
            continue;
        }
        let size = px(32. * CUBE_CELL);
        // Crystal-bearing chunks glow while they fly.
        let crystal = ART32.iter().enumerate().any(|(yy, row)| row.chars().enumerate().any(|(xx, c)| matches!(c, 'c' | 'd') && chunks.of[yy][xx] == Some(i)));
        if crystal && ms >= SPLIT && ms < REFORM.0 {
            let glow = alpha * (1. - span(ms, SPLIT + 250., 1700.)) * 0.55;
            root = root.child(
                div()
                    .absolute()
                    .left(px(x - 6.))
                    .top(px(y - 6.))
                    .size(px(12.))
                    .rounded(px(12.))
                    .shadow(vec![BoxShadow { color: t.colors.sky.opacity(glow), offset: point(px(0.), px(0.)), blur_radius: px(14.), spread_radius: px(4.), inset: false }]),
            );
        }
        let layers = inks(&ART32, |xx, yy| chunks.of[yy][xx] == Some(i)).into_iter().filter_map(|c| mark::ink(c, t).map(|ink| (format!("smash/chunk/{i}/{}.svg", c as u32).into(), ink)));
        root = root.child(sprite(layers, size, rot, alpha).left(px(x - 16. * CUBE_CELL)).top(px(y - 16. * CUBE_CELL)));
    }

    // Cracks run out from the strike, then the cube parts along them.
    if (IMPACT..SPLIT + 20.).contains(&ms) {
        for band in 0..3 {
            let shown = span(ms, IMPACT + 8. + band as f32 * 32., IMPACT + 30. + band as f32 * 32.);
            if shown > 0. {
                root = root.child(
                    svg()
                        .path(SharedString::from(format!("smash/cracks/{band}.svg")))
                        .absolute()
                        .left(px(CUBE_AT.0 + shake.0))
                        .top(px(CUBE_AT.1 + shake.1))
                        .size(px(32. * CUBE_CELL))
                        .text_color(mix(t.colors.midnight, t.colors.sky, 0.15).opacity(shown)),
                );
            }
        }
    }

    // Glowing crystal shards, faster and higher than the chunks.
    if (SPLIT..1800.).contains(&ms) {
        let alpha = 1. - span(ms, 1150., 1800.);
        for i in 0..5 {
            let a = -PI / 2. + 1.25 * noise(i, 11);
            let speed = 175. + 45. * noise(i, 12);
            let tau = 0.4;
            let travel = tau * (1. - (-ts / tau).exp());
            let (x, y) = (sx + a.cos() * speed * travel, sy + a.sin() * speed * travel + 0.5 * 120. * ts * ts);
            let rot = 540. * noise(i, 13).signum() * travel / tau;
            root = root.child(
                div()
                    .absolute()
                    .left(px(x - 4.))
                    .top(px(y - 4.))
                    .size(px(8.))
                    .rounded(px(8.))
                    .shadow(vec![BoxShadow { color: t.colors.sky.opacity(0.75 * alpha), offset: point(px(0.), px(0.)), blur_radius: px(10.), spread_radius: px(2.), inset: false }]),
            );
            let layers = inks(&SHARD, |_, _| true).into_iter().filter_map(|c| mark::ink(c, t).map(|ink| (ink_path("shard", c), ink)));
            root = root.child(sprite(layers, px(10.), rot, alpha).left(px(x - 5.)).top(px(y - 5.)));
        }
    }

    // Motes: tiny cyan pixels drifting up from the break.
    if ((SPLIT + 20.)..2300.).contains(&ms) {
        for i in 0..12 {
            let born = SPLIT + 20. + 60. * (noise(i, 21) + 1.);
            let life = span(ms, born, born + 1100.);
            if life <= 0. || life >= 1. {
                continue;
            }
            let alpha = (life * 5.).min(1.) * (1. - life);
            let (x0, y0) = (CUBE_AT.0 + 32. + 26. * noise(i, 22), CUBE_AT.1 + 30. + 16. * noise(i, 23));
            let x = x0 + 8. * (life * PI * 2. + noise(i, 24) * 3.).sin();
            let y = y0 - (28. + 14. * noise(i, 25)) * ease_out(life);
            root = root.child(div().absolute().left(px(x)).top(px(y)).size(px(2.)).bg(mix(t.colors.sky, t.colors.primary_fg, 0.45).opacity(alpha)));
        }
    }

    // The hammer, with a short two-ghost trail through the swing.
    if ((SWING.0 + 30.)..(IMPACT + 30.)).contains(&ms) {
        for (lag, alpha) in [(46., 0.16), (23., 0.32)] {
            let (rot, lift) = hammer_pose(ms - lag);
            root = root.child(hammer(t, rot, lift, alpha));
        }
    }
    let (rot, lift) = hammer_pose(ms);
    root = root.child(hammer(t, rot, lift, 1.));

    // Impact: a blue-white flash, a pixel cross, and sparks.
    if (IMPACT..IMPACT + 200.).contains(&ms) {
        let k = span(ms, IMPACT, IMPACT + 200.);
        let r = 8. + 14. * ease_out(k);
        root = root.child(
            div()
                .absolute()
                .left(px(sx - r))
                .top(px(sy - r))
                .size(px(r * 2.))
                .rounded(px(r * 2.))
                .bg(t.colors.primary_fg.opacity(0.9 * (1. - k)))
                .shadow(vec![BoxShadow { color: t.colors.sky.opacity(0.8 * (1. - k)), offset: point(px(0.), px(0.)), blur_radius: px(18.), spread_radius: px(6.), inset: false }]),
        );
        let c = 1. - span(ms, IMPACT, IMPACT + 110.);
        if c > 0. {
            let white = t.colors.primary_fg.opacity(c);
            root = root.child(div().absolute().left(px(sx - 2.)).top(px(sy - 14.)).w(px(4.)).h(px(28.)).bg(white));
            root = root.child(div().absolute().left(px(sx - 14.)).top(px(sy - 2.)).w(px(28.)).h(px(4.)).bg(white));
        }
    }
    if (IMPACT..IMPACT + 340.).contains(&ms) {
        let k = span(ms, IMPACT, IMPACT + 340.);
        for i in 0..10 {
            let a = -PI / 2. + 1.45 * noise(i, 31);
            let dist = (20. + 16. * noise(i, 32).abs()) * ease_out(k);
            let (x, y) = (sx + a.cos() * dist, sy + a.sin() * dist + 10. * k * k);
            let size = if i % 3 == 0 { 3. } else { 2. };
            let color = if i % 2 == 0 { t.colors.primary_fg } else { mix(t.colors.sky, t.colors.primary_fg, 0.3) };
            root = root.child(div().absolute().left(px(x)).top(px(y)).size(px(size)).bg(color.opacity(1. - k)));
        }
    }
    root
}

/// The animated logo, replayed whenever `replay` changes. Respects reduced
/// motion (`FORGE_REDUCED_MOTION=1`: the still logo), and can be frozen at a
/// moment for inspection (`FORGE_SMASH_AT_MS=480`).
pub fn animated(t: &Theme, replay: usize) -> AnyElement {
    if let Some(at) = std::env::var("FORGE_SMASH_AT_MS").ok().and_then(|v| v.parse::<f32>().ok()) {
        return scene(t, at).into_any_element();
    }
    if std::env::var("FORGE_REDUCED_MOTION").is_ok_and(|v| v == "1") {
        return scene(t, 0.).into_any_element();
    }
    let theme = t.clone();
    div()
        .w(px(SIZE.0))
        .h(px(SIZE.1))
        .flex_none()
        .with_animation(SharedString::from(format!("smash-{replay}")), gpui::Animation::new(std::time::Duration::from_millis(DURATION_MS as u64)), move |d, delta| {
            d.child(scene(&theme, delta * DURATION_MS))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cube_pixel_belongs_to_one_chunk_and_every_chunk_has_some() {
        let chunks = &*CHUNKS;
        let mut count = vec![0; SEEDS.len()];
        for (y, row) in ART32.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                assert_eq!(c != '.', chunks.of[y][x].is_some());
                if let Some(i) = chunks.of[y][x] {
                    count[i] += 1;
                }
            }
        }
        assert!(count.iter().all(|&n| n > 20), "{count:?}");
    }

    #[test]
    fn layers_are_served_and_well_formed() {
        let s = String::from_utf8(svg_asset("smash/hammer/79.svg").unwrap()).unwrap();
        assert!(s.starts_with("<svg") && s.contains("<rect") && s.ends_with("</svg>"));
        assert!(svg_asset(&format!("smash/chunk/0/{}.svg", 'O' as u32)).is_some());
        assert!(String::from_utf8(svg_asset("smash/cracks/0.svg").unwrap()).unwrap().contains("<rect"));
        assert!(svg_asset("smash/nope.svg").is_none());
    }

    #[test]
    fn the_swing_lands_on_the_strike_and_ends_at_rest() {
        assert_eq!(hammer_pose(0.).0, hammer_pose(DURATION_MS).0);
        assert!((hammer_pose(IMPACT - 0.01).0 - -147.).abs() < 0.5);
        // The head's centre at impact sits on the strike point (within a few px).
        let (px_, py_) = pivot_px();
        let (hx, hy) = (HEAD.0 - GRIP.0, HEAD.1 - GRIP.1);
        let a = hy.atan2(hx) + (-147f32).to_radians();
        let len = (hx * hx + hy * hy).sqrt() * HAMMER_CELL;
        let head = (px_ + len * a.cos(), py_ + len * a.sin());
        let (sx, sy) = strike_px();
        assert!((head.0 - sx).abs() < 1. && (head.1 - sy).abs() < 8., "{head:?} vs {:?}", (sx, sy));
    }
}
