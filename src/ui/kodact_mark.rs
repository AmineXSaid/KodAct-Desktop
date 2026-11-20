//! KodAct's mark: `./` + `<`, three geometric strokes that together read as
//! a stylized **K** with a coding sprite (`dot`, `slash`, `angle`), matching
//! the developer brand identity (`kodact/dot.svg`, `kodact/slash.svg`,
//! `kodact/angle.svg`, served by `assets.rs`).
//!
//! At rest (`./`+`<`), the dot and angle shine in electric lime (`#b7ff1a` in
//! dark mode, `#4d6e00` in light mode) against the high-contrast slash
//! (`text_strong`), adapting automatically to light and dark surfaces.
//!
//! - [`mark`]: the mark at rest.
//! - [`working`]: the mark while KodAct works: each element (`.` → `/` → `<`)
//!   sweeps through a staggered 3D perspective turn (Y-axis foreshortening,
//!   depth lift, and light flare). Reduced motion: at rest.
//! - [`landing`]: the mark arriving (onboarding, the opener): dot, slash, and
//!   angle land sequentially into place and cool from white heat.
//! - [`struck`]: one push: the angle `<` drives ahead and flares.
//! - [`wordmark`]: the mark, then KodAct, its Act in the theme's colour.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, svg, Animation, AnimationExt, AnyElement, App, ElementId, Hsla,
    Transformation, Window,
};

use super::motion;
use crate::theme::{ActiveTheme, Theme};

/// The mark's canonical canvas and the geometry of its three strokes:
/// `.` (`DOT`), `/` (`SLASH`), `<` (`ANGLE`), forming `./<` ("K").
const VIEW: (f32, f32) = (260., 252.);

/// Bottom-left code dot (16-gon circle centred at (35, 205), r = 18).
const DOT: [(f32, f32); 16] = [
    (53.00, 205.00),
    (51.63, 211.89),
    (47.73, 217.73),
    (41.89, 221.63),
    (35.00, 223.00),
    (28.11, 221.63),
    (22.27, 217.73),
    (18.37, 211.89),
    (17.00, 205.00),
    (18.37, 198.11),
    (22.27, 192.27),
    (28.11, 188.37),
    (35.00, 187.00),
    (41.89, 188.37),
    (47.73, 192.27),
    (51.63, 198.11),
];

/// Crisp 14° tilted slash `/` ascending through the centre.
const SLASH: [(f32, f32); 4] = [
    (68.0, 224.0),
    (96.0, 232.0),
    (148.0, 22.0),
    (120.0, 14.0),
];

/// Right chevron `<` kissing the slash midpoint to form the K.
const ANGLE: [(f32, f32); 6] = [
    (114.0, 130.0),
    (223.0, 22.0),
    (243.0, 42.0),
    (154.0, 130.0),
    (243.0, 218.0),
    (223.0, 238.0),
];

const JOIN: f32 = 3.;
/// How much of the box's width the mark fills.
const FILL: f32 = 0.92;
/// The stroke SVGs are drawn in a 64-unit box.
const BOX: f32 = 64.;
/// One lap of the 3D working animation, in milliseconds.
const LAP_MS: u64 = 1800;
/// How far a push drives the angle ahead, as a fraction of the box.
const PUSH: f32 = 0.11;

/// Which stroke of the `./<` K mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stroke {
    Dot,
    Slash,
    Angle,
}

impl Stroke {
    const ALL: [Stroke; 3] = [Stroke::Dot, Stroke::Slash, Stroke::Angle];

    fn name(self) -> &'static str {
        match self {
            Stroke::Dot => "dot",
            Stroke::Slash => "slash",
            Stroke::Angle => "angle",
        }
    }

    fn index(self) -> usize {
        match self {
            Stroke::Dot => 0,
            Stroke::Slash => 1,
            Stroke::Angle => 2,
        }
    }
}

/// Code-lime highlight for the dot and `<` angle strokes.
fn code_lime(t: &Theme) -> Hsla {
    if t.is_dark {
        gpui::rgb(0xb7ff1a).into()
    } else {
        gpui::rgb(0x4d6e00).into()
    }
}

/// A stroke's outline in a `size` box with its origin at the top left: the
/// mark centred, `FILL` of the box wide.
pub(crate) fn outline(stroke: Stroke, size: f32) -> Vec<(f32, f32)> {
    let s = size * FILL / VIEW.0;
    let (ox, oy) = ((size - VIEW.0 * s) / 2., (size - VIEW.1 * s) / 2.);
    let pts: &[(f32, f32)] = match stroke {
        Stroke::Dot => &DOT,
        Stroke::Slash => &SLASH,
        Stroke::Angle => &ANGLE,
    };
    pts.iter().map(|(x, y)| (ox + x * s, oy + y * s)).collect()
}

/// One stroke as an SVG for `assets.rs`: `kodact/dot.svg`, `kodact/slash.svg`,
/// or `kodact/angle.svg`. `None` for any other path.
pub(crate) fn stroke_svg(path: &str) -> Option<Vec<u8>> {
    let stroke = match path.strip_prefix("kodact/")?.strip_suffix(".svg")? {
        "dot" => Stroke::Dot,
        "slash" => Stroke::Slash,
        "angle" => Stroke::Angle,
        _ => return None,
    };
    let d: String = outline(stroke, BOX)
        .iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{}{x:.3} {y:.3}", if i == 0 { "M" } else { "L" }))
        .collect();
    let join = JOIN * BOX * FILL / VIEW.0;
    Some(
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path fill="#000" stroke="#000" stroke-width="{join:.3}" stroke-linejoin="round" d="{d}Z"/></svg>"##
        )
        .into_bytes(),
    )
}

/// The mark at one moment, `size` px square.
#[derive(IntoElement)]
pub struct Mark {
    size: f32,
    /// 0..1: how lit the mark is: 1 white, through the bright signal, to 0
    /// settled in its own colour (`glowing`).
    heat: f32,
    /// While the strokes are still arriving (`landing`): how far along, 0..1.
    landing: Option<f32>,
    /// How far ahead (to the right) the `<` angle sits, as a fraction of the box.
    ahead: f32,
    /// Each stroke's opacity (dot, slash, angle).
    lit: [f32; 3],
    /// Each stroke's 3D active pulse (0..1) and Y-axis twist angle (radians).
    tilt: [(f32, f32); 3],
    /// Optional uniform/live tint overriding the default lime + ink pair.
    color: Option<Hsla>,
}

impl Mark {
    /// Draw the mark in this colour instead of the default sprite palette.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

/// The lime glow ramp so default landing/working flares never clash with teal.
fn sprite_ramp(t: &Theme) -> [Hsla; 4] {
    if t.is_dark {
        [
            gpui::rgb(0x283a05).into(),
            gpui::rgb(0x74b800).into(),
            gpui::rgb(0xcfff4d).into(),
            gpui::rgb(0xf6ffe6).into(),
        ]
    } else {
        [
            gpui::rgb(0x223300).into(),
            gpui::rgb(0x4d6e00).into(),
            gpui::rgb(0x8bbf1a).into(),
            gpui::rgb(0xf4fbe6).into(),
        ]
    }
}

impl RenderOnce for Mark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let x = &t.extra;
        let ramp = if self.color.is_some() {
            [
                x.mark_heat_iron,
                x.mark_heat_ember,
                x.mark_heat,
                x.mark_heat_white,
            ]
        } else {
            sprite_ramp(t)
        };
        let lime = code_lime(t);
        let ink = t.colors.text_strong;
        let mut d = div().relative().size(px(self.size)).flex_none();
        for stroke in Stroke::ALL {
            let idx = stroke.index();
            let base = match (self.color, stroke) {
                (Some(c), Stroke::Slash) => mix(c, ink, 0.55),
                (Some(c), _) => c,
                (None, Stroke::Slash) => ink,
                (None, _) => lime,
            };
            let (pulse, yaw) = self.tilt[idx];
            let local_heat = (self.heat * stroke_heat(stroke)).max(pulse * 0.35);
            let color = if local_heat > 0.001 {
                glowing(base, local_heat, ramp)
            } else {
                base
            };
            let (dx, dy, alpha) = self
                .landing
                .map_or((0., 0., 1.), |p| arrive(stroke, p, self.size));
            let lead = if stroke == Stroke::Angle {
                self.ahead * self.size
            } else {
                0.
            };
            let lift_y = -0.07 * self.size * pulse;
            let shift_x = 0.025 * self.size * pulse;
            let sx = yaw.cos().abs().max(0.45);
            let sy = 1.0 + 0.14 * pulse;
            let cx_comp = 0.5 * (1.0 - sx) * self.size;
            let cy_comp = 0.5 * (1.0 - sy) * self.size;

            let mut el = svg()
                .path(format!("kodact/{}.svg", stroke.name()))
                .absolute()
                .top(px(dy + lift_y + cy_comp))
                .left(px(dx + lead + shift_x + cx_comp))
                .size(px(self.size))
                .opacity(alpha * self.lit[idx])
                .text_color(color);
            if pulse > 0.001 || yaw.abs() > 0.001 {
                el = el.with_transformation(Transformation::scale(gpui::size(sx, sy)));
            }
            d = d.child(el);
        }
        d
    }
}

/// How lit a stroke runs against the mark's heat: `<` brightest at the tip,
/// then `/`, then `.`.
fn stroke_heat(stroke: Stroke) -> f32 {
    match stroke {
        Stroke::Angle => 1.0,
        Stroke::Slash => 0.90,
        Stroke::Dot => 0.82,
    }
}

fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    let k = k.clamp(0., 1.);
    let (a, b) = (a.to_rgb(), b.to_rgb());
    gpui::Rgba {
        r: a.r + (b.r - a.r) * k,
        g: a.g + (b.g - a.g) * k,
        b: a.b + (b.b - a.b) * k,
        a: a.a + (b.a - a.a) * k,
    }
    .into()
}

/// A stroke at heat `h` (0..1), as a signal settles: white at 1, the bright
/// signal at 2/3, its deep form at 1/3, near dark at `LOW`; below that the
/// dark gives way to the stroke's own colour `cool`.
fn glowing(cool: Hsla, h: f32, [low, deep, bright, white]: [Hsla; 4]) -> Hsla {
    let h = h.clamp(0., 1.);
    let stops = [(LOW, low), (1. / 3., deep), (2. / 3., bright), (1., white)];
    if h <= LOW {
        let x = h / LOW;
        return mix(cool, low, x * x * (3. - 2. * x));
    }
    let i = stops
        .iter()
        .position(|(at, _)| h <= *at)
        .unwrap_or(3)
        .max(1);
    let ((a, ca), (b, cb)) = (stops[i - 1], stops[i]);
    mix(ca, cb, (h - a) / (b - a))
}

/// Where a settling stroke is at its darkest, just before its own colour returns.
const LOW: f32 = 0.12;

/// Where a stroke is while the mark arrives (`p` 0..1): the dot pops in,
/// the slash sweeps up from the bottom-left, and the `<` angle locks into it.
fn arrive(stroke: Stroke, p: f32, size: f32) -> (f32, f32, f32) {
    let (start, from) = match stroke {
        Stroke::Dot => (0.00, (-0.18, 0.22)),
        Stroke::Slash => (0.14, (-0.30, 0.32)),
        Stroke::Angle => (0.28, (0.42, 0.0)),
    };
    let t = ((p - start) / 0.5).clamp(0., 1.);
    let (c1, c3) = (1.701_58, 2.701_58);
    let e = 1. + c3 * (t - 1.).powi(3) + c1 * (t - 1.).powi(2);
    let rest = 1. - e;
    (
        from.0 * size * rest,
        from.1 * size * rest,
        (t * 2.5).min(1.),
    )
}

/// How lit a push gets: the bright signal, not white.
const STRIKE_HEAT: f32 = 0.7;

/// The mark at rest, `size` px square.
pub fn mark(size: f32) -> Mark {
    Mark {
        size,
        heat: 0.,
        landing: None,
        ahead: 0.,
        lit: [1., 1., 1.],
        tilt: [(0., 0.); 3],
        color: None,
    }
}

/// The mark arriving: its strokes `land` (0..1) into place, the whole mark
/// at `heat` (1 white, 0 settled in its colour).
pub fn landing(size: f32, land: f32, heat: f32) -> Mark {
    Mark {
        heat: heat.clamp(0., 1.),
        landing: (land < 1.).then_some(land.max(0.)),
        ..mark(size)
    }
}

/// The mark pushed: `k` (0..1) through one push, the `<` angle driving ahead
/// and flaring at the middle, back in place and settled at the end.
pub fn struck(size: f32, k: f32) -> Mark {
    let bump = (k.clamp(0., 1.) * std::f32::consts::PI).sin();
    Mark {
        heat: STRIKE_HEAT * bump,
        ahead: PUSH * bump,
        ..mark(size)
    }
}

/// One projected 3D polygon ready for painter's-algorithm drawing.
#[derive(Clone, Debug)]
struct Poly3d {
    pts: Vec<(f32, f32)>,
    z: f32,
    color: Hsla,
}

#[derive(Clone, Copy)]
struct Vec3(f32, f32, f32);

impl Vec3 {
    fn dot(self, o: Self) -> f32 {
        self.0 * o.0 + self.1 * o.1 + self.2 * o.2
    }
    fn sub(self, o: Self) -> Self {
        Self(self.0 - o.0, self.1 - o.1, self.2 - o.2)
    }
    fn cross(self, o: Self) -> Self {
        Self(
            self.1 * o.2 - self.2 * o.1,
            self.2 * o.0 - self.0 * o.2,
            self.0 * o.1 - self.1 * o.0,
        )
    }
    fn norm(self) -> Self {
        let len = (self.0 * self.0 + self.1 * self.1 + self.2 * self.2)
            .sqrt()
            .max(1e-6);
        Self(self.0 / len, self.1 / len, self.2 / len)
    }
}

/// Rotate `v` around X by `rx`, then Y by `ry`.
fn rot_xy(v: Vec3, rx: f32, ry: f32) -> Vec3 {
    let (sx, cx) = rx.sin_cos();
    let y1 = v.1 * cx - v.2 * sx;
    let z1 = v.1 * sx + v.2 * cx;
    let (sy, cy) = ry.sin_cos();
    let x2 = v.0 * cy + z1 * sy;
    let z2 = -v.0 * sy + z1 * cy;
    Vec3(x2, y1, z2)
}

/// Project all three extruded 3D strokes (`.`, `/`, `<`) at lap progress `k`
/// (`0..1`) into back-to-front sorted shaded 2D polygons.
fn project_3d_mark(
    k: f32,
    size: f32,
    lime: Hsla,
    ink: Hsla,
    shadow_lime: Hsla,
    shadow_ink: Hsla,
    highlight: Hsla,
) -> Vec<Poly3d> {
    let s = size * FILL / VIEW.0;
    let (ox, oy) = ((size - VIEW.0 * s) / 2., (size - VIEW.1 * s) / 2.);
    // Local extrusion half-depth and camera focal distance in VIEW coordinates.
    let dz = 15.0_f32;
    let focal = 560.0_f32;
    let light = Vec3(0.42, -0.58, 0.70).norm();
    let tau = std::f32::consts::TAU;

    let stroke_pts = |stroke: Stroke| -> &'static [(f32, f32)] {
        match stroke {
            Stroke::Dot => &DOT,
            Stroke::Slash => &SLASH,
            Stroke::Angle => &ANGLE,
        }
    };

    let mut polys = Vec::with_capacity(48);
    for stroke in Stroke::ALL {
        let idx = stroke.index();
        let pts = stroke_pts(stroke);
        let n = pts.len() as f32;
        let (cx, cy) = pts
            .iter()
            .fold((0., 0.), |(ax, ay), (x, y)| (ax + x / n, ay + y / n));

        let phase = k * tau - (idx as f32) * 0.90;
        let ry = 0.68 * phase.sin() + 0.16 * (phase * 2.0).sin();
        let rx = 0.34 * phase.cos();
        let dz_lift = 14.0 * phase.sin();
        let dy_float = -5.5 * (0.5 + 0.5 * phase.sin());
        let pulse = 0.5 + 0.5 * phase.sin();

        let xf = |x: f32, y: f32, z: f32| -> (Vec3, (f32, f32)) {
            let r = rot_xy(Vec3(x - cx, y - cy, z), rx, ry);
            let w = Vec3(cx + r.0, cy + dy_float + r.1, r.2 + dz_lift);
            let persp = focal / (focal - w.2).max(120.);
            let px_x = ox + (VIEW.0 * 0.5 + (w.0 - VIEW.0 * 0.5) * persp) * s;
            let px_y = oy + (VIEW.1 * 0.5 + (w.1 - VIEW.1 * 0.5) * persp) * s;
            (w, (px_x, px_y))
        };

        let front: Vec<(Vec3, (f32, f32))> = pts.iter().map(|&(x, y)| xf(x, y, dz)).collect();
        let back: Vec<(Vec3, (f32, f32))> = pts.iter().map(|&(x, y)| xf(x, y, -dz)).collect();

        let (base, shadow) = match stroke {
            Stroke::Slash => (ink, shadow_ink),
            _ => (lime, shadow_lime),
        };

        let front_n = rot_xy(Vec3(0., 0., 1.), rx, ry).norm();
        let back_n = Vec3(-front_n.0, -front_n.1, -front_n.2);

        // Rear cap
        let back_z = back.iter().map(|(v, _)| v.2).sum::<f32>() / n;
        let back_nd = back_n.dot(light).clamp(0., 1.);
        polys.push(Poly3d {
            pts: back.iter().map(|(_, p)| *p).collect(),
            z: back_z,
            color: mix(shadow, base, 0.20 + 0.25 * back_nd),
        });

        // Side extrusion quads
        let len = pts.len();
        for i in 0..len {
            let j = (i + 1) % len;
            let (f0, p_f0) = front[i];
            let (f1, p_f1) = front[j];
            let (b1, p_b1) = back[j];
            let (b0, p_b0) = back[i];
            let side_n = f1.sub(f0).cross(b0.sub(f0)).norm();
            // Outward normal magnitude vs light
            let nd = side_n.dot(light).abs().clamp(0., 1.);
            let avg_z = (f0.2 + f1.2 + b1.2 + b0.2) * 0.25;
            polys.push(Poly3d {
                pts: vec![p_f0, p_f1, p_b1, p_b0],
                z: avg_z,
                color: mix(shadow, base, 0.32 + 0.42 * nd),
            });
        }

        // Front cap with specular highlight as stroke pivots toward viewer
        let front_z = front.iter().map(|(v, _)| v.2).sum::<f32>() / n;
        let nd = front_n.dot(light).clamp(0., 1.);
        let spec = (nd.powi(3) * 0.26 + pulse * 0.10).clamp(0., 0.36);
        polys.push(Poly3d {
            pts: front.iter().map(|(_, p)| *p).collect(),
            z: front_z,
            color: mix(base, highlight, spec),
        });
    }

    polys.sort_by(|a, b| a.z.total_cmp(&b.z));
    polys
}

/// The mark while KodAct works: its three strokes (`.`, `/`, `<`) rendered as
/// extruded 3D geometric prisms turning in depth space with shaded side walls
/// and specular highlights. Reduced motion: [`mark`].
pub fn working(size: f32, id: impl Into<ElementId>, color: Option<Hsla>) -> AnyElement {
    let tinted = move |m: Mark| match color {
        Some(c) => m.color(c),
        None => m,
    };
    if motion::reduced_motion() {
        return tinted(mark(size)).into_any_element();
    }
    div()
        .size(px(size))
        .flex_none()
        .with_animation(
            id,
            Animation::new(Duration::from_millis(LAP_MS)).repeat(),
            move |d, k| {
                d.child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, _, window, cx| {
                            let t = cx.theme();
                            let lime = color.unwrap_or_else(|| code_lime(t));
                            let ink = match color {
                                Some(c) => mix(c, t.colors.text_strong, 0.55),
                                None => t.colors.text_strong,
                            };
                            let (shadow_lime, shadow_ink, highlight) = if t.is_dark {
                                (
                                    gpui::rgb(0x1f2e03).into(),
                                    gpui::rgb(0x3a404c).into(),
                                    gpui::rgb(0xf6ffe6).into(),
                                )
                            } else {
                                (
                                    gpui::rgb(0x1d2b00).into(),
                                    gpui::rgb(0x4b4844).into(),
                                    gpui::rgb(0xffffff).into(),
                                )
                            };
                            let s = f32::from(bounds.size.width).min(f32::from(bounds.size.height));
                            let ox = bounds.origin.x + px((f32::from(bounds.size.width) - s) * 0.5);
                            let oy = bounds.origin.y + px((f32::from(bounds.size.height) - s) * 0.5);
                            for poly in project_3d_mark(
                                k,
                                s,
                                lime,
                                ink,
                                shadow_lime,
                                shadow_ink,
                                highlight,
                            ) {
                                if poly.pts.len() < 3 {
                                    continue;
                                }
                                let mut pb = gpui::PathBuilder::fill();
                                pb.move_to(gpui::point(
                                    ox + px(poly.pts[0].0),
                                    oy + px(poly.pts[0].1),
                                ));
                                for &(x, y) in &poly.pts[1..] {
                                    pb.line_to(gpui::point(ox + px(x), oy + px(y)));
                                }
                                pb.close();
                                if let Ok(path) = pb.build() {
                                    window.paint_path(path, poly.color);
                                }
                            }
                        },
                    )
                    .size_full(),
                )
            },
        )
        .into_any_element()
}

/// The mark, then "KodAct" set at `height` px, matching `.`/`<` in electric lime.
pub fn wordmark(height: f32, t: &Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .child(
            // Centred on the capitals, which sit a little above the line's
            // middle (the line keeps room below for descenders).
            div()
                .relative()
                .top(px(-height * WORD_LIFT))
                .mr(px(height * WORD_GAP))
                .child(mark(mark_box(height))),
        )
        .child(name(height, t))
        .into_any_element()
}

/// "KodAct" alone, as the wordmark sets it but without the mark: for the
/// title bar, where the name is enough.
pub fn name(height: f32, t: &Theme) -> AnyElement {
    let letters = |s: &'static str, color: Hsla| {
        div()
            .font_family(t.font_sans.clone())
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_size(px(height))
            .line_height(px(height))
            .text_color(color)
            .child(s)
    };
    div()
        .flex()
        .items_center()
        .child(letters("Kod", t.colors.text_strong))
        .child(letters("Act", code_lime(t)))
        .into_any_element()
}

/// The capitals' height, and how far their middle sits above the line's,
/// as fractions of the type size.
pub(crate) const WORD_CAP: f32 = 0.72;
const WORD_LIFT: f32 = 0.05;
/// The space between the mark's box and the K, as a fraction of the type size.
pub(crate) const WORD_GAP: f32 = 0.12;

/// The box of the mark that leads "KodAct" set at `type_size`: the mark a
/// little taller than the capitals, as the logo sets it. The opener's
/// wordmark uses it too.
pub(crate) fn mark_box(type_size: f32) -> f32 {
    type_size * WORD_CAP * 1.2 * VIEW.0 / VIEW.1 / FILL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bbox(pts: &[(f32, f32)]) -> (f32, f32, f32, f32) {
        pts.iter()
            .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, (x, y)| {
                (b.0.min(*x), b.1.min(*y), b.2.max(*x), b.3.max(*y))
            })
    }

    #[test]
    fn the_mark_is_centred_and_stays_in_its_box() {
        let all: Vec<(f32, f32)> = Stroke::ALL
            .into_iter()
            .flat_map(|s| outline(s, 64.))
            .collect();
        let (x0, y0, x1, y1) = bbox(&all);
        let half = JOIN / 2. * 64. * FILL / VIEW.0;
        assert!((x0 + x1 - 64.).abs() < 0.5 && (y0 + y1 - 64.).abs() < 0.5);
        assert!(x0 > half && y0 > half && x1 < 64. - half && y1 < 64. - half);
    }

    #[test]
    fn dot_then_slash_then_angle_form_the_k_left_to_right() {
        let dot = bbox(&outline(Stroke::Dot, 64.));
        let slash = bbox(&outline(Stroke::Slash, 64.));
        let angle = bbox(&outline(Stroke::Angle, 64.));
        // Dot sits bottom-left of the slash; angle opens to the right edge.
        assert!(dot.2 < slash.0 + 4.0);
        assert!(dot.1 > slash.1);
        assert!(angle.2 > slash.2);
        // Slash kisses or slightly meets the apex of the `<` angle.
        assert!(angle.0 <= slash.2 && angle.0 > dot.2);
    }

    #[test]
    fn all_three_strokes_are_served_and_nothing_else() {
        for name in ["dot", "slash", "angle"] {
            let svg = stroke_svg(&format!("kodact/{name}.svg")).expect("served");
            let svg = String::from_utf8(svg).unwrap();
            assert!(svg.starts_with("<svg") && svg.contains(" d=\"M"), "{svg}");
            assert!(svg.contains("stroke-linejoin=\"round\""), "{svg}");
        }
        assert!(stroke_svg("kodact/step.svg").is_none());
        assert!(stroke_svg("kodact/chevron.svg").is_none());
        assert!(stroke_svg("icons/x.svg").is_none());
    }

    #[test]
    fn landing_brings_all_strokes_home_and_hides_later_ones() {
        for stroke in Stroke::ALL {
            let (dx, dy, a) = arrive(stroke, 1., 88.);
            assert!(dx.abs() < 1e-3 && dy.abs() < 1e-3 && a == 1., "{stroke:?}");
        }
        // Early on, dot is ahead of slash, which is ahead of `<`.
        assert!(arrive(Stroke::Dot, 0.35, 88.).2 >= arrive(Stroke::Slash, 0.35, 88.).2);
        assert!(arrive(Stroke::Slash, 0.35, 88.).2 > arrive(Stroke::Angle, 0.35, 88.).2);
        // Landed, the arriving mark is the mark itself.
        assert!(landing(88., 1., 0.).landing.is_none());
    }

    #[test]
    fn a_push_drives_the_angle_ahead_and_flares_on_the_way() {
        assert!(struck(20., 0.).ahead.abs() < 1e-6);
        assert!((struck(20., 0.5).ahead - PUSH).abs() < 1e-3);
        assert!((struck(20., 0.5).heat - STRIKE_HEAT).abs() < 1e-3);
        assert!(struck(20., 1.).heat < 1e-3 && struck(20., 1.).ahead < 1e-3);
    }

    #[test]
    fn working_projects_all_three_extruded_3d_strokes_sorted_by_depth() {
        let rgb = |c: u32| -> Hsla { gpui::rgb(c).into() };
        let polys_a = project_3d_mark(
            0.25,
            24.,
            rgb(0xb7ff1a),
            rgb(0xf4f5f8),
            rgb(0x1f2e03),
            rgb(0x3a404c),
            rgb(0xf6ffe6),
        );
        let polys_b = project_3d_mark(
            0.55,
            24.,
            rgb(0xb7ff1a),
            rgb(0xf4f5f8),
            rgb(0x1f2e03),
            rgb(0x3a404c),
            rgb(0xf6ffe6),
        );
        // 3 strokes: (front cap + back cap) * 3 + side quads (16 + 4 + 6) = 32 polygons.
        assert_eq!(polys_a.len(), 32);
        // Sorted back-to-front by z and all vertices finite inside/near the box.
        for w in polys_a.windows(2) {
            assert!(w[0].z <= w[1].z);
        }
        for p in &polys_a {
            assert!(p.pts.len() >= 3);
            for (x, y) in &p.pts {
                assert!(x.is_finite() && y.is_finite());
            }
        }
        // Advancing progress k turns the 3D shapes smoothly to new screen coordinates.
        let moved = polys_a
            .iter()
            .zip(polys_b.iter())
            .any(|(a, b)| (a.z - b.z).abs() > 0.5);
        assert!(moved);
    }

    #[test]
    fn heat_runs_white_to_bright_to_deep_to_dark_then_the_stroke() {
        let rgb = |c: u32| -> Hsla { gpui::rgb(c).into() };
        let ramp = [rgb(0x16302d), rgb(0x1f8f80), rgb(0x7df0dc), rgb(0xf2fffc)];
        let teal = rgb(0x0e7568);
        let close = |a: Hsla, b: Hsla| {
            let (a, b) = (a.to_rgb(), b.to_rgb());
            (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() < 0.02
        };
        assert!(close(glowing(teal, 1., ramp), ramp[3]), "white");
        assert!(close(glowing(teal, 2. / 3., ramp), ramp[2]), "bright");
        assert!(close(glowing(teal, 1. / 3., ramp), ramp[1]), "deep");
        assert!(close(glowing(teal, LOW, ramp), ramp[0]), "dark");
        assert!(close(glowing(teal, 0., ramp), teal), "settled");
        assert!(stroke_heat(Stroke::Angle) > stroke_heat(Stroke::Slash));
        assert!(stroke_heat(Stroke::Slash) > stroke_heat(Stroke::Dot));
    }

    #[test]
    fn the_wordmark_s_mark_stands_a_little_over_the_capitals() {
        let size = 40.;
        let tall = mark_box(size) * FILL * VIEW.1 / VIEW.0;
        assert!((tall / (size * WORD_CAP) - 1.2).abs() < 1e-3);
    }
}
