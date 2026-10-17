//! Forge's mark, the ingot: a cube of three flat faces seen from above, its
//! top in the accent like metal still hot from the forge. Each face is a
//! one-colour SVG made from the geometry here (`ingot/NN-F.svg`, served by
//! `assets.rs`) and tinted from the theme (`mark_top`, `mark_left`,
//! `mark_right`), so it is drawn smooth-edged at any size, on any GPU, and
//! follows light and dark.
//!
//! - [`mark`]: the cube at rest, at a corner (the isometric view).
//! - [`turning`]: the cube while Forge works, a quarter turn at a time,
//!   easing in and out with a short rest between. Reduced motion: at rest.
//! - [`wordmark`]: "forge" in the display serif, its o the cube.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, FRAC_PI_4, SQRT_2, TAU};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, svg, Animation, AnimationExt, AnyElement, App, ElementId, FontWeight,
    Hsla, Window,
};

use super::motion;
use crate::theme::{ActiveTheme, Theme};

/// How far the view looks down on the cube: the true isometric angle,
/// atan(1/√2), so at rest all three faces are the same size.
const PITCH: f32 = 0.615_479_7;
/// At rest the cube shows a corner: its left and right faces equal.
const REST: f32 = FRAC_PI_4;
/// How much of the box the cube fills, top corner to bottom corner.
const FILL: f32 = 0.86;
/// A quarter turn, then a rest, in milliseconds (the old turning logo's).
const TURN_MS: f32 = 900.;
const REST_MS: f32 = 450.;
/// Views in one full turn, 5° apart; a quarter turn is 18 of them.
const FRAMES: usize = 72;
/// The face SVGs are drawn in a 64-unit box, a unit apart.
const BOX: f32 = 64.;
const GAP: f32 = 1.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Face {
    Top,
    /// A side: which of the four (0..4, +x, +z, -x, -z), and how far right
    /// it faces: 0 is the left face at rest, 1 the right.
    Side(usize, f32),
}

/// The cube's visible faces at `yaw`, in a `size` box with its origin at the
/// top left, each face drawn `gap` px in from its edges so the faces read
/// apart. Back to front: the sides, then the top.
pub(crate) fn faces(yaw: f32, size: f32, gap: f32) -> Vec<(Face, Vec<(f32, f32)>)> {
    let (sin, cos) = yaw.sin_cos();
    let (sp, cp) = PITCH.sin_cos();
    // The tallest the cube gets (at a corner), so the scale never pumps as it turns.
    let scale = size * FILL / (SQRT_2 * sp + cp);
    let c = size / 2.;
    let project = |x: f32, y: f32, z: f32| {
        let xr = x * cos + z * sin;
        let zr = -x * sin + z * cos;
        (c + xr * scale, c + (-y * cp + zr * sp) * scale)
    };
    let mut out = Vec::new();
    for (n, (nx, nz)) in [(1., 0.), (0., 1.), (-1., 0.), (0., -1.)]
        .into_iter()
        .enumerate()
    {
        // Facing the viewer, or edge on (and then not drawn).
        if -nx * sin + nz * cos <= 1e-3 {
            continue;
        }
        let (tx, tz) = (-nz * 0.5, nx * 0.5);
        let (cx, cz) = (nx * 0.5, nz * 0.5);
        let quad = vec![
            project(cx - tx, 0.5, cz - tz),
            project(cx + tx, 0.5, cz + tz),
            project(cx + tx, -0.5, cz + tz),
            project(cx - tx, -0.5, cz - tz),
        ];
        let right = ((nx * cos + nz * sin) / FRAC_1_SQRT_2 + 1.) / 2.;
        if let Some(q) = inset(&quad, gap) {
            out.push((Face::Side(n, right.clamp(0., 1.)), q));
        }
    }
    let top = vec![
        project(-0.5, 0.5, -0.5),
        project(0.5, 0.5, -0.5),
        project(0.5, 0.5, 0.5),
        project(-0.5, 0.5, 0.5),
    ];
    if let Some(q) = inset(&top, gap) {
        out.push((Face::Top, q));
    }
    out
}

/// A convex polygon with every edge moved `d` inward, or `None` when it is
/// too thin to keep (a face turning edge on).
fn inset(poly: &[(f32, f32)], d: f32) -> Option<Vec<(f32, f32)>> {
    let n = poly.len();
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f32>()
        / 2.;
    if area.abs() < 4. * d * d + 0.05 {
        return None;
    }
    let sign = area.signum();
    // Each edge as a point on its moved line and a direction.
    let lines: Vec<((f32, f32), (f32, f32))> = (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let (ex, ey) = (b.0 - a.0, b.1 - a.1);
            let len = (ex * ex + ey * ey).sqrt().max(1e-6);
            // Inward is to the left of each edge for a positive (y-down: clockwise) winding.
            let (nx, ny) = (-ey / len * sign, ex / len * sign);
            ((a.0 + nx * d, a.1 + ny * d), (ex, ey))
        })
        .collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let ((p, r), (q, s)) = (lines[(i + n - 1) % n], lines[i]);
        let cross = r.0 * s.1 - r.1 * s.0;
        if cross.abs() < 1e-6 {
            return None;
        }
        let t = ((q.0 - p.0) * s.1 - (q.1 - p.1) * s.0) / cross;
        out.push((p.0 + r.0 * t, p.1 + r.1 * t));
    }
    // An inset that turned inside out has flipped its winding.
    let after: f32 = (0..n)
        .map(|i| {
            let (a, b) = (out[i], out[(i + 1) % n]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    (after.signum() == sign).then_some(out)
}

fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    gpui::Rgba {
        r: a.r + (b.r - a.r) * k,
        g: a.g + (b.g - a.g) * k,
        b: a.b + (b.b - a.b) * k,
        a: a.a + (b.a - a.a) * k,
    }
    .into()
}

fn frame_yaw(frame: usize) -> f32 {
    REST + (frame % FRAMES) as f32 * TAU / FRAMES as f32
}

/// One face of the cube at a view, as an SVG for `assets.rs`:
/// `ingot/NN-F.svg`, NN the view (00..71), F the face (`t` the top, `0`..`3`
/// a side). `None` for any other path, or a face that view doesn't show.
pub(crate) fn face_svg(path: &str) -> Option<Vec<u8>> {
    let rest = path.strip_prefix("ingot/")?.strip_suffix(".svg")?;
    let (frame, face) = rest.split_once('-')?;
    let frame = frame.parse::<usize>().ok().filter(|f| *f < FRAMES)?;
    let side = match face {
        "t" => None,
        n => Some(n.parse::<usize>().ok()?),
    };
    let (_, poly) = faces(frame_yaw(frame), BOX, GAP)
        .into_iter()
        .find(|(f, _)| match (f, side) {
            (Face::Top, None) => true,
            (Face::Side(n, _), Some(want)) => *n == want,
            _ => false,
        })?;
    let d: String = poly
        .iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{}{x:.3} {y:.3}", if i == 0 { "M" } else { "L" }))
        .collect();
    Some(
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="{d}Z"/></svg>"#
        )
        .into_bytes(),
    )
}

/// The cube at one view, `size` px square, in the theme's mark colours.
#[derive(IntoElement)]
pub struct Ingot {
    size: f32,
    frame: usize,
}

impl RenderOnce for Ingot {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let x = &cx.theme().extra;
        let mut d = div().relative().size(px(self.size)).flex_none();
        for (face, _) in faces(frame_yaw(self.frame), BOX, GAP) {
            let (name, color) = match face {
                Face::Top => ("t".to_string(), x.mark_top),
                Face::Side(n, k) => (n.to_string(), mix(x.mark_left, x.mark_right, k)),
            };
            d = d.child(
                svg()
                    .path(format!("ingot/{:02}-{name}.svg", self.frame))
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(self.size))
                    .text_color(color),
            );
        }
        d
    }
}

/// The mark at rest, `size` px square.
pub fn mark(size: f32) -> Ingot {
    Ingot { size, frame: 0 }
}

/// The mark turning while Forge works. Reduced motion: [`mark`].
pub fn turning(size: f32, id: impl Into<ElementId>) -> AnyElement {
    if motion::reduced_motion() {
        return mark(size).into_any_element();
    }
    let beat = TURN_MS + REST_MS;
    let lap = Duration::from_millis((beat * 4.) as u64);
    div()
        .size(px(size))
        .flex_none()
        .with_animation(id, Animation::new(lap).repeat(), move |d, k| {
            d.child(Ingot {
                size,
                frame: frame_of(yaw_at(k * beat * 4., beat)),
            })
        })
        .into_any_element()
}

/// The nearest drawn view to `yaw`.
fn frame_of(yaw: f32) -> usize {
    (((yaw - REST) / TAU * FRAMES as f32).round() as i64).rem_euclid(FRAMES as i64) as usize
}

/// Where the cube faces `ms` into a lap: each quarter eases out of its rest,
/// turns, and rests again.
fn yaw_at(ms: f32, beat: f32) -> f32 {
    let quarter = ((ms / beat) as usize).min(3);
    let t = ((ms - quarter as f32 * beat) / TURN_MS).clamp(0., 1.);
    let eased = t * t * (3. - 2. * t);
    REST + (quarter as f32 + eased) * FRAC_PI_2
}

/// "forge" in the display serif at `height` px, its o the cube sitting on
/// the baseline at the letters' x-height.
pub fn wordmark(height: f32, t: &Theme) -> AnyElement {
    let letters = |s: &'static str| {
        div()
            .font_family(t.font_display.clone())
            .font_weight(FontWeight::NORMAL)
            .text_size(px(height))
            .line_height(px(height * 1.1))
            .text_color(t.colors.text_strong)
            .child(s)
    };
    div()
        .flex()
        .items_baseline()
        .child(letters("f"))
        .child(
            // A box aligned to the baseline hangs below it by the cube's
            // margin and more; lift it so the cube stands on the baseline,
            // a hair over the x-height, as an o does.
            div()
                .relative()
                .top(px(-height * 0.125))
                .ml(px(-height * 0.05))
                .mr(px(-height * 0.03))
                .child(mark(height * 0.62)),
        )
        .child(letters("rge"))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(fs: &[(Face, Vec<(f32, f32)>)]) -> (f32, f32, f32, f32) {
        let pts = fs.iter().flat_map(|(_, p)| p.iter().copied());
        pts.fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, (x, y)| {
            (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y))
        })
    }

    #[test]
    fn at_rest_it_shows_a_corner_and_its_top() {
        let fs = faces(REST, 64., 0.);
        assert_eq!(fs.len(), 3);
        assert_eq!(fs[2].0, Face::Top);
        let mut sides: Vec<f32> = fs
            .iter()
            .filter_map(|(f, _)| match f {
                Face::Side(_, k) => Some(*k),
                Face::Top => None,
            })
            .collect();
        sides.sort_by(f32::total_cmp);
        assert!(sides[0] < 0.01 && sides[1] > 0.99, "{sides:?}");
        // Centred, filling FILL of the height, as wide as the corner view is.
        let (x0, y0, x1, y1) = bbox(&fs);
        assert!((y1 - y0 - 64. * FILL).abs() < 0.01, "{y0} {y1}");
        assert!((x0 + x1 - 64.).abs() < 0.01 && (y0 + y1 - 64.).abs() < 0.01);
    }

    #[test]
    fn it_never_grows_or_leaves_its_box_as_it_turns() {
        for i in 0..=72 {
            let yaw = i as f32 * std::f32::consts::TAU / 72.;
            let (x0, y0, x1, y1) = bbox(&faces(yaw, 32., 0.5));
            assert!(x0 >= 0. && y0 >= 0. && x1 <= 32. && y1 <= 32., "{yaw}");
        }
    }

    #[test]
    fn a_quarter_turn_looks_the_same_as_rest() {
        let a = faces(REST, 40., 0.6);
        let b = faces(REST + FRAC_PI_2, 40., 0.6);
        let (ba, bb) = (bbox(&a), bbox(&b));
        assert!((ba.0 - bb.0).abs() < 1e-3 && (ba.3 - bb.3).abs() < 1e-3);
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn the_turn_rests_then_eases_a_quarter() {
        let beat = TURN_MS + REST_MS;
        assert!((yaw_at(0., beat) - REST).abs() < 1e-6);
        assert!((yaw_at(TURN_MS, beat) - (REST + FRAC_PI_2)).abs() < 1e-6);
        assert!((yaw_at(beat - 1., beat) - (REST + FRAC_PI_2)).abs() < 1e-6);
        assert!((yaw_at(TURN_MS / 2., beat) - (REST + FRAC_PI_4)).abs() < 1e-4);
    }

    #[test]
    fn every_view_serves_its_faces_and_nothing_else() {
        for frame in 0..FRAMES {
            for (face, _) in faces(frame_yaw(frame), BOX, GAP) {
                let id = match face {
                    Face::Top => "t".to_string(),
                    Face::Side(n, _) => n.to_string(),
                };
                let svg = face_svg(&format!("ingot/{frame:02}-{id}.svg")).expect("served");
                let svg = String::from_utf8(svg).unwrap();
                assert!(svg.starts_with("<svg") && svg.contains(" d=\"M"), "{svg}");
            }
        }
        // At rest the back faces (+x, -z) are hidden; out of range is nothing.
        assert!(face_svg("ingot/00-0.svg").is_none());
        assert!(face_svg("ingot/00-3.svg").is_none());
        assert!(face_svg("ingot/72-t.svg").is_none());
        assert!(face_svg("icons/x.svg").is_none());
    }

    #[test]
    fn the_turn_lands_on_whole_views() {
        assert_eq!(frame_of(REST), 0);
        assert_eq!(frame_of(REST + FRAC_PI_2), FRAMES / 4);
        assert_eq!(frame_of(REST + TAU), 0);
        assert_eq!(frame_of(REST - TAU / FRAMES as f32), FRAMES - 1);
    }

    #[test]
    fn faces_keep_a_gap_and_an_edge_on_face_is_dropped() {
        let (gx0, _, gx1, _) = bbox(&faces(REST, 64., 1.));
        let (x0, _, x1, _) = bbox(&faces(REST, 64., 0.));
        assert!(gx0 > x0 && gx1 < x1, "the gap moves the edges in");
        // Square on, only the front side and the top show.
        assert_eq!(faces(0., 64., 1.).len(), 2);
    }
}
