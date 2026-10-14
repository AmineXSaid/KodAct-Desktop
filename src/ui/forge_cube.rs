//! Forge's cube, as Forge for VS Code draws it (`components/forge/cube.ts`,
//! `ForgeCube.vue`, `ForgeCubeMark.vue`): a 3x3x3 block of voxels seen from
//! above at a fixed tilt, lit from the upper left, in the brand purple.
//!
//! Two forms. [`mark`] is the finished cube, three flat faces. [`working`] is
//! the working indicator: raw graphite stock struck into a finished cube. One
//! loop is four beats, each a quarter turn; beat one holds the rough lump,
//! beats two to four each land a strike that throws chunks off (they glow like
//! sparks and fly back in) and bring the block closer to true. Reduced motion:
//! the finished cube, still.
//!
//! The cube takes a tint, as the extension's spinner does per permission mode
//! ([`tint`]); its faces are that colour mixed with white and near-black.

use std::time::Duration;

use gpui::{
    canvas, div, point, prelude::*, px, Animation, AnimationExt, AnyElement, Bounds, ElementId,
    Hsla, PathBuilder, Pixels, Rgba, Window,
};

use crate::modes::ModeId;
use crate::theme::Theme;

type Vec3 = [f32; 3];

/// How far the view looks down onto the cube: how much of the top shows.
const TILT: f32 = 28. * std::f32::consts::PI / 180.;
/// Resting yaw: a vertical edge toward the viewer, both side faces showing.
const REST_YAW: f32 = std::f32::consts::FRAC_PI_4;
/// One beat; four make a loop.
const BEAT_MS: f32 = 650.;
/// Roughness once each beat's strike has landed: stock, then three strikes.
const ROUGH: [f32; 4] = [1., 0.55, 0.22, 0.];
/// Where each knocked-out chunk flies, as a fraction of the box.
const CHUNKS: [(f32, f32); 4] = [(0.44, -0.38), (-0.46, 0.2), (0.3, 0.46), (-0.28, -0.46)];

struct Face {
    normal: Vec3,
    corners: [Vec3; 4],
}

const FACES: [Face; 6] = [
    Face {
        normal: [0., 1., 0.],
        corners: [[-1., 1., -1.], [1., 1., -1.], [1., 1., 1.], [-1., 1., 1.]],
    },
    Face {
        normal: [0., -1., 0.],
        corners: [
            [-1., -1., -1.],
            [1., -1., -1.],
            [1., -1., 1.],
            [-1., -1., 1.],
        ],
    },
    Face {
        normal: [1., 0., 0.],
        corners: [[1., -1., -1.], [1., 1., -1.], [1., 1., 1.], [1., -1., 1.]],
    },
    Face {
        normal: [-1., 0., 0.],
        corners: [
            [-1., -1., -1.],
            [-1., 1., -1.],
            [-1., 1., 1.],
            [-1., -1., 1.],
        ],
    },
    Face {
        normal: [0., 0., 1.],
        corners: [[-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.]],
    },
    Face {
        normal: [0., 0., -1.],
        corners: [
            [-1., -1., -1.],
            [1., -1., -1.],
            [1., 1., -1.],
            [-1., 1., -1.],
        ],
    },
];

/// Turn a point about the vertical axis by `yaw`, then tilt it toward the
/// viewer. View space: +x right, +y up, +z toward the viewer.
fn to_view(p: Vec3, yaw: f32) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    let x = p[0] * c + p[2] * s;
    let z = -p[0] * s + p[2] * c;
    let (st, ct) = TILT.sin_cos();
    [x, p[1] * ct - z * st, p[1] * st + z * ct]
}

/// A face is drawn when its view-space normal points at the viewer.
fn faces_viewer(n: Vec3) -> bool {
    n[2] > 0.02
}

/// How lit a side face is: 0 turned from the light (upper left), 1 facing it.
fn side_light(n: Vec3) -> f32 {
    ((1. - n[0]) / 2.).clamp(0., 1.)
}

/// Deterministic noise: the lump is the same lump everywhere it is drawn.
fn noise(n: f32) -> f32 {
    let v = (n * 127.1 + 311.7).sin() * 43758.545;
    v - v.floor()
}

struct Voxel {
    at: Vec3,
    jitter: Vec3,
    /// Scale on the raw stock: corners and some edges start chipped.
    chip: f32,
}

/// The 26 visible voxels of the block; the core is never seen.
fn voxels() -> Vec<Voxel> {
    let mut out = Vec::with_capacity(26);
    for x in -1i32..=1 {
        for y in -1i32..=1 {
            for z in -1i32..=1 {
                let exposure = x.abs() + y.abs() + z.abs();
                if exposure == 0 {
                    continue;
                }
                let n = (out.len() + 1) as f32;
                let chip = if exposure == 3 {
                    0.45 + 0.25 * noise(n + 0.13)
                } else if exposure == 2 && noise(n + 0.53) > 0.5 {
                    0.72
                } else {
                    1.
                };
                out.push(Voxel {
                    at: [x as f32, y as f32, z as f32],
                    jitter: [noise(n) - 0.5, noise(n + 0.37) - 0.5, noise(n + 0.71) - 0.5],
                    chip,
                });
            }
        }
    }
    out
}

/// Every voxel in view space at a roughness from 0 (finished) to 1 (raw
/// stock), far to near so painting in order occludes correctly.
fn place_voxels(rough: f32, yaw: f32, pitch: f32, overlap: f32) -> Vec<(Vec3, f32)> {
    let spread = 1. + 0.05 * rough;
    let jitter = 0.26 * rough;
    let mut placed: Vec<(Vec3, f32)> = voxels()
        .iter()
        .map(|v| {
            let at = |i: usize| (v.at[i] * spread + v.jitter[i] * jitter) * pitch;
            (
                to_view([at(0), at(1), at(2)], yaw),
                pitch / 2. * (1. + (v.chip - 1.) * rough) + overlap,
            )
        })
        .collect();
    placed.sort_by(|a, b| a.0[2].total_cmp(&b.0[2]));
    placed
}

#[derive(Clone, Copy)]
struct Frame {
    beat: usize,
    yaw: f32,
    rough: f32,
    /// Impact: positive squashes the block, negative is the rebound.
    squash: f32,
    /// How far the chunks are out of the block, 0..1.
    burst: f32,
}

/// The finished cube at rest (the mark, and reduced motion).
const RESTING: Frame = Frame {
    beat: 3,
    yaw: REST_YAW,
    rough: 0.,
    squash: 0.,
    burst: 0.,
};

fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4. * t.powi(3)
    } else {
        1. - (-2. * t + 2.).powi(3) / 2.
    }
}

fn frame_at(ms: f32) -> Frame {
    let beat = (ms / BEAT_MS) as usize % 4;
    let f = (ms % BEAT_MS) / BEAT_MS;
    let struck = beat > 0;
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    Frame {
        beat,
        yaw: REST_YAW
            + (beat as f32 + ease_in_out(((f - 0.12) / 0.34).clamp(0., 1.)))
                * std::f32::consts::FRAC_PI_2,
        rough: if struck {
            lerp(
                ROUGH[beat - 1],
                ROUGH[beat],
                ease_out((f / 0.2).clamp(0., 1.)),
            )
        } else {
            ease_in_out(((f - 0.2) / 0.55).clamp(0., 1.))
        },
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

/// CSS `color-mix(in srgb, a p, b)`.
fn mix(a: Hsla, p: f32, b: Hsla) -> Hsla {
    let (a, b): (Rgba, Rgba) = (a.to_rgb(), b.to_rgb());
    Rgba {
        r: a.r * p + b.r * (1. - p),
        g: a.g * p + b.g * (1. - p),
        b: a.b * p + b.b * (1. - p),
        a: 1.,
    }
    .into()
}

/// The cube's inks for a tint, as the extension's stylesheet mixes them.
#[derive(Clone, Copy)]
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
    fn new(color: Hsla, t: &Theme) -> Self {
        let (light, dark) = (t.extra.cube_light, t.extra.cube_dark);
        let stock = mix(t.extra.cube_stock, 0.88, color);
        Inks {
            top: mix(color, 0.52, light),
            lit: mix(color, 0.86, light),
            shade: mix(color, 0.68, dark),
            stock_top: mix(stock, 0.70, light),
            stock_lit: stock,
            stock_shade: mix(stock, 0.62, dark),
            spark: t.extra.cube_spark,
        }
    }
}

/// The cube's colour for a permission mode, as the extension tints its
/// spinner: the brand, the text colour while editing automatically, the plan
/// blue, the bypass red.
pub fn tint(mode: ModeId, t: &Theme) -> Hsla {
    match mode {
        ModeId::Expert | ModeId::Manual => t.extra.cube_brand,
        ModeId::AcceptEdits => t.colors.text,
        ModeId::Plan => t.extra.mode_plan,
        ModeId::Bypass => t.extra.mode_bypass,
    }
}

fn fill(window: &mut Window, corners: &[gpui::Point<Pixels>], color: Hsla, alpha: f32) {
    if alpha <= 0.004 {
        return;
    }
    let mut path = PathBuilder::fill();
    path.move_to(corners[0]);
    for c in &corners[1..] {
        path.line_to(*c);
    }
    path.close();
    if let Ok(p) = path.build() {
        window.paint_path(p, color.opacity(alpha.min(1.)));
    }
}

/// One frame of the working indicator in `bounds`.
fn draw(bounds: Bounds<Pixels>, frame: Frame, inks: Inks, window: &mut Window) {
    let side = f32::from(bounds.size.width.min(bounds.size.height));
    let (mx, my) = (f32::from(bounds.center().x), f32::from(bounds.center().y));
    // The finished cube fills about four fifths of its box.
    let pitch = side * 0.168;
    let (sx, sy) = (1. + 0.08 * frame.squash, 1. - 0.16 * frame.squash);
    let finish = 1. - frame.rough;
    // A hair of overlap (a third of a device pixel) closes the antialiasing
    // seams between touching voxels.
    let overlap = 0.35 / window.scale_factor();
    let faces: Vec<(&Face, Vec3, [Vec3; 4])> = FACES
        .iter()
        .map(|f| {
            (
                f,
                to_view(f.normal, frame.yaw),
                f.corners.map(|c| to_view(c, frame.yaw)),
            )
        })
        .filter(|(_, n, _)| faces_viewer(*n))
        .collect();

    let cube =
        |window: &mut Window, cx: f32, cy: f32, centre: Vec3, half: f32, alpha: f32, heat: f32| {
            for (face, normal, corners) in &faces {
                let pts: Vec<_> = corners
                    .iter()
                    .map(|c| {
                        point(
                            px(cx + (centre[0] + c[0] * half) * sx),
                            px(cy - (centre[1] + c[1] * half) * sy),
                        )
                    })
                    .collect();
                if face.normal[1] == 1. {
                    fill(window, &pts, inks.stock_top, alpha);
                    fill(window, &pts, inks.top, alpha * finish);
                } else {
                    let light = side_light(*normal);
                    fill(window, &pts, inks.stock_shade, alpha);
                    fill(window, &pts, inks.stock_lit, alpha * light);
                    fill(window, &pts, inks.shade, alpha * finish);
                    fill(window, &pts, inks.lit, alpha * light * finish);
                }
                // A struck chunk glows hot, brightest on its lit faces.
                if heat > 0. {
                    let glow = if face.normal[1] == 1. {
                        0.9
                    } else {
                        0.35 + 0.55 * side_light(*normal)
                    };
                    fill(window, &pts, inks.spark, alpha * heat * glow);
                }
            }
        };

    for (centre, half) in place_voxels(frame.rough, frame.yaw, pitch, overlap) {
        cube(window, mx, my, centre, half, 1., 0.);
    }
    // Chunks burst from inside the block and are pulled back in; each strike
    // throws a different three.
    if frame.burst > 0. {
        let half = pitch * 0.2 * (0.6 + 0.4 * frame.burst);
        let alpha = (frame.burst * 2.5).clamp(0., 1.);
        for i in [frame.beat % 4, (frame.beat + 2) % 4, (frame.beat + 3) % 4] {
            let (dx, dy) = CHUNKS[i];
            cube(
                window,
                mx + dx * side * frame.burst,
                my + dy * side * frame.burst,
                [0., 0., 0.],
                half,
                alpha,
                frame.burst,
            );
        }
    }
}

/// The finished cube, three faces, filling a `size` box.
fn draw_mark(bounds: Bounds<Pixels>, inks: Inks, window: &mut Window) {
    let faces: Vec<(&Face, Vec3, [Vec3; 4])> = FACES
        .iter()
        .map(|f| {
            (
                f,
                to_view(f.normal, REST_YAW),
                f.corners.map(|c| to_view(c, REST_YAW)),
            )
        })
        .filter(|(_, n, _)| faces_viewer(*n))
        .collect();
    // Fit the drawing's bounds into the box, centred.
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (_, _, corners) in &faces {
        for c in corners {
            x0 = x0.min(c[0]);
            x1 = x1.max(c[0]);
            y0 = y0.min(-c[1]);
            y1 = y1.max(-c[1]);
        }
    }
    let side = f32::from(bounds.size.width.min(bounds.size.height));
    let scale = side / (x1 - x0).max(y1 - y0);
    let (cx, cy) = (f32::from(bounds.center().x), f32::from(bounds.center().y));
    let (mx, my) = ((x0 + x1) / 2., (y0 + y1) / 2.);
    for (face, normal, corners) in &faces {
        let pts: Vec<_> = corners
            .iter()
            .map(|c| point(px(cx + (c[0] - mx) * scale), px(cy + (-c[1] - my) * scale)))
            .collect();
        let ink = if face.normal[1] == 1. {
            inks.top
        } else if side_light(*normal) > 0.5 {
            inks.lit
        } else {
            inks.shade
        };
        fill(window, &pts, ink, 1.);
    }
}

/// The finished cube in `color`, `size` px square.
pub fn mark(size: f32, color: Hsla, t: &Theme) -> AnyElement {
    let inks = Inks::new(color, t);
    div()
        .size(px(size))
        .flex_none()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| draw_mark(bounds, inks, window),
            )
            .size_full(),
        )
        .into_any_element()
}

/// The working indicator in `color`, `size` px square.
pub fn working(size: f32, color: Hsla, t: &Theme, id: impl Into<ElementId>) -> AnyElement {
    let inks = Inks::new(color, t);
    let base = div().size(px(size)).flex_none();
    if super::motion::reduced_motion() {
        return base
            .child(canvas(|_, _, _| {}, move |b, _, w, _| draw(b, RESTING, inks, w)).size_full())
            .into_any_element();
    }
    let lap = Duration::from_millis((BEAT_MS * 4.) as u64);
    base.with_animation(id, Animation::new(lap).repeat(), move |d, k| {
        let frame = frame_at(k * BEAT_MS * 4.);
        d.child(canvas(|_, _, _| {}, move |b, _, w, _| draw(b, frame, inks, w)).size_full())
    })
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_has_26_voxels_and_a_loop_ends_finished() {
        assert_eq!(voxels().len(), 26);
        // Beat one starts as finished as the last beat ended; it roughens.
        assert_eq!(frame_at(0.).rough, 0.);
        assert!((frame_at(BEAT_MS * 0.9).rough - 1.).abs() < 0.01);
        // Each strike brings it closer to true; the last leaves it finished.
        assert!(frame_at(BEAT_MS * 1.5).rough > frame_at(BEAT_MS * 2.5).rough);
        assert_eq!(frame_at(BEAT_MS * 3.9).rough, 0.);
        // Chunks fly only on a strike.
        assert_eq!(frame_at(BEAT_MS * 0.5).burst, 0.);
        assert!(frame_at(BEAT_MS * 1.1).burst > 0.);
    }

    #[test]
    fn three_faces_show_at_rest() {
        let n = FACES
            .iter()
            .filter(|f| faces_viewer(to_view(f.normal, REST_YAW)))
            .count();
        assert_eq!(n, 3);
    }

    #[test]
    fn mixing_is_css_color_mix() {
        let white: Hsla = gpui::rgb(0xffffff).into();
        let black: Hsla = gpui::rgb(0x000000).into();
        let grey = mix(white, 0.5, black).to_rgb();
        assert!((grey.r - 0.5).abs() < 0.01 && (grey.g - 0.5).abs() < 0.01);
    }
}
