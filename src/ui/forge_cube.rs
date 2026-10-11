//! The working cube, as Forge for VS Code draws it: raw graphite stock struck
//! into a finished cube.
//!
//! One loop is four beats, each a 90-degree turn. Beat one holds the rough
//! lump; beats two to four each land a strike that knocks chunks out of the
//! block (they fly back in before the next blow) and brings it closer to true,
//! until the last leaves a perfect cube. The cube holds, then roughens back
//! into stock, and four quarter turns bring it round to where the loop began.
//!
//! A 3x3x3 block of voxels seen from above at a fixed tilt and lit from the
//! upper left, painted far to near. Ported from the extension's `cube.ts` and
//! `ForgeCube.vue`; here the finished colour is Forge blue and the sparks are
//! white-hot instead of amber.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::LazyLock;
use std::time::Duration;

use gpui::{canvas, div, point, prelude::*, px, Animation, AnimationExt, AnyElement, Bounds, ElementId, Hsla, PathBuilder, Pixels, Rgba, Window};

use crate::theme::Theme;

type Vec3 = [f32; 3];

/// How far the view looks down onto the cube: how much of the top face shows.
const TILT: f32 = 28. * std::f32::consts::PI / 180.;
/// Resting yaw: a vertical edge toward the viewer, both side faces showing.
const REST_YAW: f32 = FRAC_PI_4;

/// Turn a point about the vertical axis by `yaw`, then tilt it toward the
/// viewer: +x right, +y up, +z toward the viewer.
fn to_view(p: Vec3, yaw: f32) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    let x = p[0] * c + p[2] * s;
    let z = -p[0] * s + p[2] * c;
    let (st, ct) = TILT.sin_cos();
    [x, p[1] * ct - z * st, p[1] * st + z * ct]
}

struct Face {
    normal: Vec3,
    corners: [Vec3; 4],
}

const FACES: [Face; 6] = [
    Face { normal: [0., 1., 0.], corners: [[-1., 1., -1.], [1., 1., -1.], [1., 1., 1.], [-1., 1., 1.]] },
    Face { normal: [0., -1., 0.], corners: [[-1., -1., -1.], [1., -1., -1.], [1., -1., 1.], [-1., -1., 1.]] },
    Face { normal: [1., 0., 0.], corners: [[1., -1., -1.], [1., 1., -1.], [1., 1., 1.], [1., -1., 1.]] },
    Face { normal: [-1., 0., 0.], corners: [[-1., -1., -1.], [-1., 1., -1.], [-1., 1., 1.], [-1., -1., 1.]] },
    Face { normal: [0., 0., 1.], corners: [[-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.]] },
    Face { normal: [0., 0., -1.], corners: [[-1., -1., -1.], [1., -1., -1.], [1., 1., -1.], [-1., 1., -1.]] },
];

/// A face is drawn when its view-space normal points at the viewer.
fn faces_viewer(n: Vec3) -> bool {
    n[2] > 0.02
}

/// How lit a side face is, from 0 (turned from the light) to 1 (facing it).
fn side_light(n: Vec3) -> f32 {
    ((1. - n[0]) / 2.).clamp(0., 1.)
}

/// Deterministic noise, so the lump is the same lump everywhere it is drawn.
fn noise(n: f32) -> f32 {
    // In f64, as the extension computes it: the big multiplier needs the bits.
    let v = ((n as f64) * 127.1 + 311.7).sin() * 43758.5453;
    (v - v.floor()) as f32
}

struct Voxel {
    at: Vec3,
    jitter: Vec3,
    /// Scale on the raw stock: corners and some edges start chipped.
    chip: f32,
}

/// The 26 visible voxels of the 3x3x3 block; the core is never seen.
static VOXELS: LazyLock<Vec<Voxel>> = LazyLock::new(|| {
    let mut voxels = vec![];
    for x in -1..=1i32 {
        for y in -1..=1i32 {
            for z in -1..=1i32 {
                let exposure = x.abs() + y.abs() + z.abs();
                if exposure == 0 {
                    continue;
                }
                let n = (voxels.len() + 1) as f32;
                voxels.push(Voxel {
                    at: [x as f32, y as f32, z as f32],
                    jitter: [noise(n) - 0.5, noise(n + 0.37) - 0.5, noise(n + 0.71) - 0.5],
                    chip: if exposure == 3 {
                        0.45 + 0.25 * noise(n + 0.13)
                    } else if exposure == 2 && noise(n + 0.53) > 0.5 {
                        0.72
                    } else {
                        1.
                    },
                });
            }
        }
    }
    voxels
});

/// Every voxel in view space at a roughness from 0 (finished) to 1 (raw
/// stock), far to near. `overlap` widens each voxel to close seams.
fn place_voxels(rough: f32, yaw: f32, pitch: f32, overlap: f32) -> Vec<(Vec3, f32)> {
    let spread = 1. + 0.05 * rough;
    let jitter = 0.26 * rough;
    let mut placed: Vec<(Vec3, f32)> = VOXELS
        .iter()
        .map(|v| {
            let p = [0, 1, 2].map(|i| (v.at[i] * spread + v.jitter[i] * jitter) * pitch);
            (to_view(p, yaw), pitch / 2. * (1. + (v.chip - 1.) * rough) + overlap)
        })
        .collect();
    placed.sort_by(|a, b| a.0[2].total_cmp(&b.0[2]));
    placed
}

/// Milliseconds per beat; four beats make one loop.
const BEAT: f32 = 650.;
pub const LOOP: Duration = Duration::from_millis(4 * BEAT as u64);
/// Roughness once each beat's strike has landed: stock, one, two, three strikes.
const ROUGH: [f32; 4] = [1., 0.55, 0.22, 0.];
/// Where each knocked-out chunk flies, as a fraction of the box.
const CHUNKS: [(f32, f32); 4] = [(0.44, -0.38), (-0.46, 0.2), (0.3, 0.46), (-0.28, -0.46)];

fn clamp01(v: f32) -> f32 {
    v.clamp(0., 1.)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}
fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 { 4. * t.powi(3) } else { 1. - (-2. * t + 2.).powi(3) / 2. }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    beat: usize,
    yaw: f32,
    rough: f32,
    /// Impact: positive squashes the block, negative is the rebound.
    squash: f32,
    /// How far the chunks are out of the block, 0..1.
    burst: f32,
}

fn frame_at(ms: f32) -> Frame {
    let beat = (ms / BEAT).floor() as usize % 4;
    let f = (ms % BEAT) / BEAT;
    let struck = beat > 0;
    Frame {
        beat,
        yaw: REST_YAW + (beat as f32 + ease_in_out(clamp01((f - 0.12) / 0.34))) * FRAC_PI_2,
        rough: if struck { lerp(ROUGH[beat - 1], ROUGH[beat], ease_out(clamp01(f / 0.2))) } else { ease_in_out(clamp01((f - 0.2) / 0.55)) },
        squash: if !struck || f >= 0.36 {
            0.
        } else if f < 0.14 {
            (f / 0.14 * std::f32::consts::PI).sin()
        } else {
            -0.3 * ((f - 0.14) / 0.22 * std::f32::consts::PI).sin()
        },
        burst: if !struck {
            0.
        } else if f < 0.16 {
            ease_out(f / 0.16)
        } else if f < 0.78 {
            1. - ease_in_out((f - 0.16) / 0.62)
        } else {
            0.
        },
    }
}

/// A finished frame, for reduced motion.
const RESTING: Frame = Frame { beat: 3, yaw: REST_YAW, rough: 0., squash: 0., burst: 0. };

/// `color-mix(in srgb, a p%, b)`.
fn mix(a: Hsla, p: f32, b: Hsla) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.into(), b.into());
    Rgba { r: a.r * p + b.r * (1. - p), g: a.g * p + b.g * (1. - p), b: a.b * p + b.b * (1. - p), a: 1. }.into()
}

fn hex(v: u32) -> Hsla {
    gpui::rgb(v).into()
}

struct Inks {
    top: Hsla,
    lit: Hsla,
    shade: Hsla,
    stock_top: Hsla,
    stock_lit: Hsla,
    stock_shade: Hsla,
    spark: Hsla,
}

impl Inks {
    /// The extension's inks around `current` (the colour the cube is painted).
    fn new(current: Hsla, t: &Theme) -> Self {
        let (light, dark, stock) = (hex(0xffffff), hex(0x050408), hex(0x646163));
        let stocked = mix(stock, 0.88, current);
        Self {
            top: mix(current, 0.52, light),
            lit: mix(current, 0.86, light),
            shade: mix(current, 0.68, dark),
            stock_top: mix(stocked, 0.7, light),
            stock_lit: stocked,
            stock_shade: mix(stocked, 0.62, dark),
            // White-hot rather than the extension's amber: the blue/white cube.
            spark: mix(t.colors.sky, 0.15, light),
        }
    }
}

fn draw(bounds: Bounds<Pixels>, frame: Frame, inks: &Inks, window: &mut Window) {
    let size = f32::from(bounds.size.width);
    let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let mid = size / 2.;
    // The finished cube fills about four fifths of its box.
    let pitch = size * 0.168;
    let sx = 1. + 0.08 * frame.squash;
    let sy = 1. - 0.16 * frame.squash;
    let finish = 1. - frame.rough;
    let overlap = 0.35 * size / 24.;

    let faces: Vec<(&Face, Vec3, [Vec3; 4])> =
        FACES.iter().map(|f| (f, to_view(f.normal, frame.yaw), f.corners.map(|c| to_view(c, frame.yaw)))).filter(|(_, n, _)| faces_viewer(*n)).collect();

    let cube = |window: &mut Window, cx: f32, cy: f32, centre: Vec3, half: f32, alpha: f32, heat: f32| {
        for (face, normal, corners) in &faces {
            let fill = |window: &mut Window, color: Hsla, a: f32| {
                if a <= 0.004 {
                    return;
                }
                let mut b = PathBuilder::fill();
                for (i, c) in corners.iter().enumerate() {
                    let p = point(px(ox + cx + (centre[0] + c[0] * half) * sx), px(oy + cy - (centre[1] + c[1] * half) * sy));
                    if i == 0 { b.move_to(p) } else { b.line_to(p) }
                }
                b.close();
                if let Ok(path) = b.build() {
                    window.paint_path(path, color.opacity(a.min(1.)));
                }
            };
            let light = side_light(*normal);
            if face.normal[1] == 1. {
                fill(window, inks.stock_top, alpha);
                fill(window, inks.top, alpha * finish);
            } else {
                fill(window, inks.stock_shade, alpha);
                fill(window, inks.stock_lit, alpha * light);
                fill(window, inks.shade, alpha * finish);
                fill(window, inks.lit, alpha * light * finish);
            }
            // Sparks: a struck chunk glows hot, brightest on its lit faces.
            if heat > 0. {
                fill(window, inks.spark, alpha * heat * if face.normal[1] == 1. { 0.9 } else { 0.35 + 0.55 * light });
            }
        }
    };

    for (centre, half) in place_voxels(frame.rough, frame.yaw, pitch, overlap) {
        cube(window, mid, mid, centre, half, 1., 0.);
    }
    // Chunks burst from inside the block and are pulled back in, glowing as
    // they fly and cooling on the way back. Each strike throws a different three.
    if frame.burst > 0. {
        let half = pitch * 0.2 * (0.6 + 0.4 * frame.burst);
        let alpha = clamp01(frame.burst * 2.5);
        for i in [frame.beat % 4, (frame.beat + 2) % 4, (frame.beat + 3) % 4] {
            cube(window, mid + CHUNKS[i].0 * size * frame.burst, mid + CHUNKS[i].1 * size * frame.burst, [0., 0., 0.], half, alpha, frame.burst);
        }
    }
}

fn still(t: &Theme, size: Pixels, color: Hsla, frame: Frame) -> impl IntoElement {
    let inks = Inks::new(color, t);
    canvas(|_, _, _| {}, move |bounds, _, window, _| draw(bounds, frame, &inks, window)).size(size).flex_none()
}

/// The finished cube, still: a reply that is done.
pub fn finished(t: &Theme, size: Pixels, color: Hsla) -> AnyElement {
    still(t, size, color, RESTING).into_any_element()
}

/// The working cube at `size` (24px beside the working line), painted in
/// `color` (Forge blue, or the mode's tint). Reduced motion: the finished cube.
pub fn cube(t: &Theme, size: Pixels, color: Hsla, id: impl Into<ElementId>) -> AnyElement {
    if super::motion::reduced_motion() {
        return still(t, size, color, RESTING).into_any_element();
    }
    let theme = t.clone();
    div()
        .size(size)
        .flex_none()
        .with_animation(id, Animation::new(LOOP).repeat(), move |d, delta| d.child(still(&theme, size, color, frame_at(delta * 4. * BEAT))))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_26_voxels_and_corners_start_chipped() {
        assert_eq!(VOXELS.len(), 26);
        assert!(VOXELS.iter().filter(|v| v.at.iter().all(|c| *c != 0.)).all(|v| v.chip < 0.71));
    }

    #[test]
    fn the_loop_closes() {
        // The loop starts as stock at rest and ends back there, a full turn on.
        let (a, b) = (frame_at(0.), frame_at(4. * BEAT - 0.001));
        assert!((a.rough - 0.).abs() < 1e-3 && (b.rough - 0.).abs() < 1e-3);
        assert!(((b.yaw - a.yaw) - 4. * FRAC_PI_2).abs() < 1e-2);
        assert_eq!(b.burst, 0.);
        // Each strike throws chunks, then pulls them back before the next.
        assert!(frame_at(BEAT * 1.15).burst > 0.9);
        assert_eq!(frame_at(BEAT * 1.9).burst, 0.);
    }

    #[test]
    fn voxels_paint_far_to_near() {
        let placed = place_voxels(0.5, REST_YAW, 4., 0.);
        assert!(placed.windows(2).all(|w| w[0].0[2] <= w[1].0[2]));
    }
}
