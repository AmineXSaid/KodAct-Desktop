//! The working indicator from Forge for VS Code (`ForgeCube.vue`, `cube.ts`):
//! raw graphite stock struck into a finished cube.
//!
//! One loop is four beats, each a quarter turn. Beat one holds the rough
//! lump; beats two to four each land a strike that knocks chunks out of the
//! block (they fly back in before the next blow) and bring it closer to
//! true, until the last leaves a perfect cube. It then roughens back into
//! stock and the loop begins again. 26 voxels, painted far to near, on a
//! canvas. Graphite, lit from above; the finished top takes the theme's
//! live colour; the chunks glow like sparks.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::OnceLock;
use std::time::Duration;

use gpui::{
    canvas, div, point, prelude::*, px, Animation, AnimationExt, AnyElement, Bounds, ElementId,
    Hsla, PathBuilder, Pixels, Window,
};

use super::motion;

type Vec3 = [f32; 3];

/// How far the view looks down onto the cube: how much of the top shows.
const TILT: f32 = 28. * std::f32::consts::PI / 180.;
/// Resting yaw: a vertical edge toward the viewer, both sides showing.
const REST_YAW: f32 = FRAC_PI_4;
/// Milliseconds per beat; four beats make one loop.
const BEAT_MS: f32 = 650.;
/// Roughness once each beat's strike has landed: stock, one, two, three strikes.
const ROUGH: [f32; 4] = [1., 0.55, 0.22, 0.];
/// Where each knocked-out chunk flies, as a fraction of the box.
const CHUNKS: [(f32, f32); 4] = [(0.44, -0.38), (-0.46, 0.2), (0.3, 0.46), (-0.28, -0.46)];

/// Turn a point about the vertical axis by `yaw`, then tilt it toward the
/// viewer: +x right, +y up, +z toward the viewer.
fn to_view(p: Vec3, yaw: f32) -> Vec3 {
    let (s, c) = yaw.sin_cos();
    let (st, ct) = TILT.sin_cos();
    let x = p[0] * c + p[2] * s;
    let z = -p[0] * s + p[2] * c;
    [x, p[1] * ct - z * st, p[1] * st + z * ct]
}

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

/// How lit a side is, 0 (turned from the light) to 1 (facing it); the
/// light sits to the upper left.
fn side_light(n: Vec3) -> f32 {
    ((1. - n[0]) / 2.).clamp(0., 1.)
}

/// Deterministic noise, so the lump is the same lump everywhere.
fn noise(n: f32) -> f32 {
    let v = (n * 127.1 + 311.7).sin() * 43758.547;
    v - v.floor()
}

struct Voxel {
    at: Vec3,
    jitter: Vec3,
    /// Scale on the raw stock: corners and some edges start chipped.
    chip: f32,
}

/// The 26 visible voxels of the 3×3×3 block; the core is never seen.
fn voxels() -> &'static [Voxel] {
    static V: OnceLock<Vec<Voxel>> = OnceLock::new();
    V.get_or_init(|| {
        let mut out = vec![];
        for x in -1..=1i32 {
            for y in -1..=1i32 {
                for z in -1..=1i32 {
                    let exposure = x.abs() + y.abs() + z.abs();
                    if exposure == 0 {
                        continue;
                    }
                    let n = out.len() as f32 + 1.;
                    out.push(Voxel {
                        at: [x as f32, y as f32, z as f32],
                        jitter: [noise(n) - 0.5, noise(n + 0.37) - 0.5, noise(n + 0.71) - 0.5],
                        chip: match exposure {
                            3 => 0.45 + 0.25 * noise(n + 0.13),
                            2 if noise(n + 0.53) > 0.5 => 0.72,
                            _ => 1.,
                        },
                    });
                }
            }
        }
        out
    })
}

/// Every voxel in view space at a roughness from 0 (finished) to 1 (stock),
/// sorted far to near.
fn place(rough: f32, yaw: f32, pitch: f32, overlap: f32) -> Vec<(Vec3, f32)> {
    let spread = 1. + 0.05 * rough;
    let jitter = 0.26 * rough;
    let mut v: Vec<(Vec3, f32)> = voxels()
        .iter()
        .map(|v| {
            let c = |i: usize| (v.at[i] * spread + v.jitter[i] * jitter) * pitch;
            (
                to_view([c(0), c(1), c(2)], yaw),
                pitch / 2. * (1. + (v.chip - 1.) * rough) + overlap,
            )
        })
        .collect();
    v.sort_by(|a, b| a.0[2].total_cmp(&b.0[2]));
    v
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

/// A finished frame, for reduced motion.
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
        yaw: REST_YAW + (beat as f32 + ease_in_out(((f - 0.12) / 0.34).clamp(0., 1.))) * FRAC_PI_2,
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

/// The cube's inks: graphite, its finished top in `color` (the one colour
/// on screen is the thing at work), the sparks forge orange.
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

fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    gpui::Rgba {
        r: a.r + (b.r - a.r) * k,
        g: a.g + (b.g - a.g) * k,
        b: a.b + (b.b - a.b) * k,
        a: 1.,
    }
    .into()
}

fn inks(color: Hsla) -> Inks {
    let light: Hsla = gpui::rgb(0xffffff).into();
    let dark: Hsla = gpui::rgb(0x050408).into();
    let stock: Hsla = gpui::rgb(0x646163).into();
    Inks {
        top: mix(color, light, 0.12),
        lit: mix(stock, light, 0.25),
        shade: mix(stock, dark, 0.15),
        stock_top: mix(stock, light, 0.30),
        stock_lit: stock,
        stock_shade: mix(stock, dark, 0.38),
        spark: gpui::rgb(0xd99530).into(),
    }
}

fn paint(bounds: Bounds<Pixels>, frame: Frame, ink: Inks, window: &mut Window) {
    let size = f32::from(bounds.size.width.min(bounds.size.height));
    let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let mid = size / 2.;
    // The finished cube fills about four fifths of its box.
    let pitch = size * 0.168;
    let (sx, sy) = (1. + 0.08 * frame.squash, 1. - 0.16 * frame.squash);
    let finish = 1. - frame.rough;
    // A hair of overlap closes the seams between touching voxels.
    let overlap = 0.35 * size / 24.;

    let faces: Vec<(&Face, Vec3, [Vec3; 4])> = FACES
        .iter()
        .map(|f| {
            (
                f,
                to_view(f.normal, frame.yaw),
                f.corners.map(|c| to_view(c, frame.yaw)),
            )
        })
        .filter(|(_, n, _)| n[2] > 0.02)
        .collect();

    let fill = |corners: &[(f32, f32)], color: Hsla, alpha: f32, window: &mut Window| {
        if alpha <= 0.004 {
            return;
        }
        let mut p = PathBuilder::fill();
        for (i, (x, y)) in corners.iter().enumerate() {
            let at = point(px(ox + x), px(oy + y));
            if i == 0 {
                p.move_to(at);
            } else {
                p.line_to(at);
            }
        }
        p.close();
        if let Ok(path) = p.build() {
            window.paint_path(path, color.opacity(alpha.min(1.)));
        }
    };

    // One small cube: its visible faces, lit, blended from stock to finish.
    let cube =
        |cx: f32, cy: f32, centre: Vec3, half: f32, alpha: f32, heat: f32, window: &mut Window| {
            for (face, normal, corners) in &faces {
                let pts: Vec<(f32, f32)> = corners
                    .iter()
                    .map(|c| {
                        (
                            cx + (centre[0] + c[0] * half) * sx,
                            cy - (centre[1] + c[1] * half) * sy,
                        )
                    })
                    .collect();
                if face.normal[1] == 1. {
                    fill(&pts, ink.stock_top, alpha, window);
                    fill(&pts, ink.top, alpha * finish, window);
                } else {
                    let light = side_light(*normal);
                    fill(&pts, ink.stock_shade, alpha, window);
                    fill(&pts, ink.stock_lit, alpha * light, window);
                    fill(&pts, ink.shade, alpha * finish, window);
                    fill(&pts, ink.lit, alpha * light * finish, window);
                }
                // Sparks: a struck chunk glows hot, brightest on its lit faces.
                if heat > 0. {
                    let glow = if face.normal[1] == 1. {
                        0.9
                    } else {
                        0.35 + 0.55 * side_light(*normal)
                    };
                    fill(&pts, ink.spark, alpha * heat * glow, window);
                }
            }
        };

    for (centre, half) in place(frame.rough, frame.yaw, pitch, overlap) {
        cube(mid, mid, centre, half, 1., 0., window);
    }
    // Chunks burst out of the block and are pulled back in, glowing as they
    // fly and cooling on the way back; each strike throws a different three.
    if frame.burst > 0. {
        let half = pitch * 0.2 * (0.6 + 0.4 * frame.burst);
        let alpha = (frame.burst * 2.5).clamp(0., 1.);
        for i in [frame.beat % 4, (frame.beat + 2) % 4, (frame.beat + 3) % 4] {
            let (dx, dy) = CHUNKS[i];
            cube(
                mid + dx * size * frame.burst,
                mid + dy * size * frame.burst,
                [0., 0., 0.],
                half,
                alpha,
                frame.burst,
                window,
            );
        }
    }
}

fn still(size: f32, frame: Frame, ink: Inks) -> gpui::Canvas<()> {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint(bounds, frame, ink, window),
    )
    .size(px(size))
}

/// The cube being forged while Forge works, `size` px square, its finished
/// faces in `color`. Reduced motion: the finished cube at rest.
pub fn working(size: f32, id: impl Into<ElementId>, color: Hsla) -> AnyElement {
    let ink = inks(color);
    if motion::reduced_motion() {
        return div()
            .size(px(size))
            .flex_none()
            .child(still(size, RESTING, ink))
            .into_any_element();
    }
    let lap = BEAT_MS * 4.;
    div()
        .size(px(size))
        .flex_none()
        .with_animation(
            id,
            Animation::new(Duration::from_millis(lap as u64)).repeat(),
            move |d, k| d.child(still(size, frame_at(k * lap), ink)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loop_strikes_stock_into_a_cube_and_back() {
        // Beat one holds the lump; the last strike leaves it true.
        assert_eq!(frame_at(0.).rough, 0.);
        assert!(frame_at(BEAT_MS * 0.9).rough > 0.9);
        assert!(frame_at(BEAT_MS * 3.9).rough < 0.01);
        // Chunks fly only on a strike.
        assert_eq!(frame_at(BEAT_MS * 0.5).burst, 0.);
        assert!(frame_at(BEAT_MS * 1.16).burst > 0.9);
        assert_eq!(voxels().len(), 26);
    }
}
