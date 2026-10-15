//! Forge's cube, as Forge for VS Code draws it (`components/forge/cube.ts`):
//! a 3x3x3 block of voxels, with a hairline seam between them, seen from
//! above at a 28° tilt through a narrow perspective lens, lit from the upper
//! left, in the brand purple.
//!
//! Each face takes a flat ink chosen by where it points: the top is light,
//! and a side blends from shade to lit as it turns toward the light, so the
//! block warms and cools as it turns.
//!
//! Two forms. [`mark`] is the cube at rest. [`working`] turns it half a turn
//! at a time, easing in and out, then rests; a cube looks the same after half
//! a turn, so the loop never jumps. Reduced motion: the cube at rest.
//!
//! The cube takes a tint, as the extension's spinner does per permission mode
//! ([`tint`]); its inks are that colour mixed with white and near-black.

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
/// The camera's distance from the cube's centre, in cube units (the block
/// spans -1..1).
const DISTANCE: f32 = 9.2;
/// Half the lens's view at the cube's centre, in cube units. The
/// extension's stage (a 24° lens) leaves a wide margin round the block; at
/// icon size that margin is wasted, so the view is narrowed to fit the block.
const VIEW_HALF: f32 = 1.42;
/// Centre to centre between voxels, and a voxel's side: the gap is the seam.
const STEP: f32 = 2. / 3.;
const VOXEL: f32 = STEP * 0.94;
/// Half a turn, eased in and out, then a rest before the next.
const TURN_MS: f32 = 2000.;
const HOLD_MS: f32 = 540.;

struct Face {
    normal: Vec3,
    corners: [Vec3; 4],
}

/// A unit cube's faces, corners in order round each face.
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

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The camera, in view space.
const EYE: Vec3 = [0., 0., DISTANCE];

/// The 26 voxel centres the eye can reach; the core is never seen.
fn voxels() -> Vec<Vec3> {
    let mut out = Vec::with_capacity(26);
    for x in -1i32..=1 {
        for y in -1i32..=1 {
            for z in -1i32..=1 {
                if x != 0 || y != 0 || z != 0 {
                    out.push([x as f32 * STEP, y as f32 * STEP, z as f32 * STEP]);
                }
            }
        }
    }
    out
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4. * t.powi(3)
    } else {
        1. - (-2. * t + 2.).powi(3) / 2.
    }
}

/// The yaw `ms` into a loop of one half turn and its rest.
fn yaw_at(ms: f32) -> f32 {
    REST_YAW + ease_in_out((ms / TURN_MS).clamp(0., 1.)) * std::f32::consts::PI
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
    under: Hsla,
}

impl Inks {
    fn new(color: Hsla, t: &Theme) -> Self {
        let (light, dark) = (t.extra.cube_light, t.extra.cube_dark);
        let shade = mix(color, 0.68, dark);
        Inks {
            top: mix(color, 0.52, light),
            lit: mix(color, 0.86, light),
            shade,
            under: mix(shade, 0.6, dark),
        }
    }

    /// A face's ink: the top's, the underside's, or a side's between shade
    /// and lit by how far it faces the light (upper left).
    fn of(&self, up: f32, view_normal: Vec3) -> Hsla {
        if up > 0.5 {
            self.top
        } else if up < -0.5 {
            self.under
        } else {
            let lit = ((1. - view_normal[0]) / 2.).clamp(0., 1.);
            mix(self.lit, lit, self.shade)
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

fn fill(window: &mut Window, corners: &[gpui::Point<Pixels>], color: Hsla) {
    let mut path = PathBuilder::fill();
    path.move_to(corners[0]);
    for c in &corners[1..] {
        path.line_to(*c);
    }
    path.close();
    if let Ok(p) = path.build() {
        window.paint_path(p, color);
    }
}

/// The block at `yaw` in `bounds`: voxels far to near, each its faces that
/// look at the eye, so nearer voxels paint over farther ones.
fn draw(bounds: Bounds<Pixels>, yaw: f32, view_half: f32, inks: Inks, window: &mut Window) {
    let side = f32::from(bounds.size.width.min(bounds.size.height));
    let (mx, my) = (f32::from(bounds.center().x), f32::from(bounds.center().y));
    // Pixels per cube unit at the cube's centre; nearer points come out larger.
    let scale = side / 2. / view_half;
    let project = |p: Vec3| {
        let k = DISTANCE / (DISTANCE - p[2]) * scale;
        point(px(mx + p[0] * k), px(my - p[1] * k))
    };
    let faces: Vec<(f32, Vec3, [Vec3; 4])> = FACES
        .iter()
        .map(|f| {
            (
                f.normal[1],
                to_view(f.normal, yaw),
                f.corners.map(|c| to_view(c, yaw)),
            )
        })
        .collect();
    let mut placed: Vec<Vec3> = voxels().into_iter().map(|v| to_view(v, yaw)).collect();
    placed.sort_by(|a, b| {
        let (da, db) = (
            dot(sub(EYE, *a), sub(EYE, *a)),
            dot(sub(EYE, *b), sub(EYE, *b)),
        );
        db.total_cmp(&da)
    });
    let half = VOXEL / 2.;
    for centre in placed {
        for (up, normal, corners) in &faces {
            let face_centre = [
                centre[0] + normal[0] * half,
                centre[1] + normal[1] * half,
                centre[2] + normal[2] * half,
            ];
            if dot(*normal, sub(EYE, face_centre)) <= 0. {
                continue;
            }
            let pts: Vec<_> = corners
                .iter()
                .map(|c| {
                    project([
                        centre[0] + c[0] * half,
                        centre[1] + c[1] * half,
                        centre[2] + c[2] * half,
                    ])
                })
                .collect();
            fill(window, &pts, inks.of(*up, *normal));
        }
    }
}

/// The cube at rest in `color`, `size` px square.
pub fn mark(size: f32, color: Hsla, t: &Theme) -> AnyElement {
    let inks = Inks::new(color, t);
    div()
        .size(px(size))
        .flex_none()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| draw(bounds, REST_YAW, VIEW_HALF, inks, window),
            )
            .size_full(),
        )
        .into_any_element()
}

/// The working indicator in `color`, `size` px square: half turns and rests.
pub fn working(size: f32, color: Hsla, t: &Theme, id: impl Into<ElementId>) -> AnyElement {
    let inks = Inks::new(color, t);
    let base = div().size(px(size)).flex_none();
    if super::motion::reduced_motion() {
        return base
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, w, _| draw(b, REST_YAW, VIEW_HALF, inks, w),
                )
                .size_full(),
            )
            .into_any_element();
    }
    let lap = Duration::from_millis((TURN_MS + HOLD_MS) as u64);
    base.with_animation(id, Animation::new(lap).repeat(), move |d, k| {
        let yaw = yaw_at(k * (TURN_MS + HOLD_MS));
        d.child(
            canvas(
                |_, _, _| {},
                move |b, _, w, _| draw(b, yaw, VIEW_HALF, inks, w),
            )
            .size_full(),
        )
    })
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_has_26_voxels_with_seams() {
        assert_eq!(voxels().len(), 26);
        const { assert!(VOXEL < STEP) };
    }

    #[test]
    fn a_loop_is_half_a_turn_then_a_rest() {
        assert_eq!(yaw_at(0.), REST_YAW);
        let end = REST_YAW + std::f32::consts::PI;
        assert!((yaw_at(TURN_MS) - end).abs() < 1e-5);
        // Resting: it stays where the turn left it.
        assert!((yaw_at(TURN_MS + HOLD_MS * 0.9) - end).abs() < 1e-5);
    }

    #[test]
    fn three_faces_of_the_block_show_at_rest() {
        let n = FACES
            .iter()
            .filter(|f| dot(to_view(f.normal, REST_YAW), EYE) > 0.)
            .count();
        assert_eq!(n, 3);
    }

    #[test]
    fn a_side_warms_as_it_turns_to_the_light() {
        let t = crate::theme::Theme::light();
        let inks = Inks::new(t.extra.cube_brand, &t);
        let toward = inks.of(0., [-1., 0., 0.]).to_rgb();
        let away = inks.of(0., [1., 0., 0.]).to_rgb();
        assert!(toward.r + toward.g + toward.b > away.r + away.g + away.b);
    }

    #[test]
    fn mixing_is_css_color_mix() {
        let white: Hsla = gpui::rgb(0xffffff).into();
        let black: Hsla = gpui::rgb(0x000000).into();
        let grey = mix(white, 0.5, black).to_rgb();
        assert!((grey.r - 0.5).abs() < 0.01 && (grey.g - 0.5).abs() < 0.01);
    }
}
