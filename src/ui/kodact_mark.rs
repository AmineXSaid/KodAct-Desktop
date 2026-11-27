//! KodAct's mark: `./` + `<`, three geometric strokes that together read as
//! a stylized **K** with a coding sprite (`dot`, `slash`, `angle`), matching
//! the developer brand identity (`kodact/dot.svg`, `kodact/slash.svg`,
//! `kodact/angle.svg`, served by `assets.rs`).
//!
//! At rest (`./`+`<`), the dot and angle are KodAct's burgundy (the theme's
//! `mark_top`: `#8c1d40` on paper, `#e8809f` on ink) against the
//! high-contrast slash (`text_strong`), in light and dark alike.
//!
//! - [`mark`]: the mark at rest.
//! - [`working`]: the mark while KodAct works, as a line typed at a prompt:
//!   `.`, `/` and `<` are typed in left to right, the `<` runs (a nudge
//!   ahead) while the dot blinks like a cursor, then they are backspaced
//!   away, to a faint trace, and typed again (`coding`). Reduced motion: at
//!   rest.
//! - [`landing`]: the mark arriving (onboarding): dot, slash, and angle land
//!   sequentially into place and cool from white heat.
//! - [`struck`]: one push: the angle `<` drives ahead and flares.
//! - [`wordmark`]: the mark, then KodAct, its Act in the mark's colour.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, svg, Animation, AnimationExt, AnyElement, App, ElementId, Hsla, Window,
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
const SLASH: [(f32, f32); 4] = [(68.0, 224.0), (96.0, 232.0), (148.0, 22.0), (120.0, 14.0)];

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
/// One lap of the working motion (typed, run, cleared), in milliseconds.
const LAP_MS: u64 = 2000;
/// How faint a stroke not typed yet (or cleared) is: a trace, never gone.
const TRACE: f32 = 0.2;
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
    if stroke == Stroke::Dot {
        let s = BOX * FILL / VIEW.0;
        let (ox, oy) = ((BOX - VIEW.0 * s) / 2., (BOX - VIEW.1 * s) / 2.);
        let (cx, cy) = (ox + 35.0 * s, oy + 205.0 * s);
        let r = (18.0 + JOIN * 0.5) * s;
        return Some(
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle fill="#000" cx="{cx:.3}" cy="{cy:.3}" r="{r:.3}"/></svg>"##
            )
            .into_bytes(),
        );
    }
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
    /// Each stroke's sideways offset, as a fraction of the box (typing).
    shift: [f32; 3],
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

impl RenderOnce for Mark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme();
        let x = &t.extra;
        let ramp = [
            x.mark_heat_iron,
            x.mark_heat_ember,
            x.mark_heat,
            x.mark_heat_white,
        ];
        let brand = x.mark_top;
        let ink = t.colors.text_strong;
        let mut d = div().relative().size(px(self.size)).flex_none();
        for stroke in Stroke::ALL {
            let idx = stroke.index();
            let base = match (self.color, stroke) {
                (Some(c), Stroke::Slash) => mix(c, ink, 0.55),
                (Some(c), _) => c,
                (None, Stroke::Slash) => ink,
                (None, _) => brand,
            };
            let local_heat = self.heat * stroke_heat(stroke);
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
            d = d.child(
                svg()
                    .path(format!("kodact/{}.svg", stroke.name()))
                    .absolute()
                    .top(px(dy))
                    .left(px(dx + lead + self.shift[idx] * self.size))
                    .size(px(self.size))
                    .opacity(alpha * self.lit[idx])
                    .text_color(color),
            );
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
        shift: [0.; 3],
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

/// One frame of the working motion at lap progress `k` (0..1): each
/// stroke's opacity and sideways offset, and how far the `<` runs ahead.
///
/// - 0.00–0.30 typed: `.`, `/`, `<` in turn come up from a trace and slide
///   into place from a little to the left, as characters land;
/// - 0.30–0.80 run: the `<` nudges ahead once (0.36–0.54) while the dot
///   blinks twice like a cursor (0.48–0.56, 0.64–0.72);
/// - 0.80–0.95 cleared: `<`, `/`, `.` back out to a trace, right to left;
/// - 0.95–1.00 a beat at a trace before the next line.
pub(crate) fn coding(k: f32) -> ([f32; 3], [f32; 3], f32) {
    let k = k.rem_euclid(1.);
    let ease = |x: f32| {
        let x = x.clamp(0., 1.);
        1. - (1. - x).powi(3)
    };
    let mut lit = [TRACE; 3];
    let mut shift = [0.; 3];
    for i in 0..3 {
        let typed = ease((k - 0.10 * i as f32) / 0.10);
        let cleared = ease((k - (0.80 + 0.05 * (2 - i) as f32)) / 0.05);
        let on = typed * (1. - cleared);
        lit[i] = TRACE + (1. - TRACE) * on;
        // Typed in from the left; backspaced away to the left.
        shift[i] = -0.06 * (1. - typed) - 0.04 * cleared;
    }
    // The cursor: the dot blinks twice while the line runs.
    if (0.48..0.56).contains(&k) || (0.64..0.72).contains(&k) {
        lit[0] = TRACE;
    }
    // The run: the `<` nudges ahead and back.
    let run = ((k - 0.36) / 0.18).clamp(0., 1.);
    let ahead = 0.08 * (run * std::f32::consts::PI).sin();
    (lit, shift, ahead)
}

/// The mark while KodAct works: a line typed at a prompt, run, cleared and
/// typed again (`coding`). Reduced motion: [`mark`].
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
                let (lit, shift, ahead) = coding(k);
                d.child(tinted(Mark {
                    lit,
                    shift,
                    ahead,
                    ..mark(size)
                }))
            },
        )
        .into_any_element()
}

/// The mark, then "KodAct" set at `height` px, its Act in the mark's burgundy.
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
        .child(letters("Act", t.extra.mark_top))
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
        let dot = String::from_utf8(stroke_svg("kodact/dot.svg").expect("dot served")).unwrap();
        assert!(dot.starts_with("<svg") && dot.contains("<circle "), "{dot}");
        for name in ["slash", "angle"] {
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
    fn working_types_the_line_runs_it_and_clears_it() {
        // Before typing: all three a trace.
        let (lit, _, _) = coding(0.0);
        assert!(lit.iter().all(|l| (l - TRACE).abs() < 1e-4), "{lit:?}");
        // Typed left to right: the dot before the slash before the angle.
        let (lit, shift, _) = coding(0.12);
        assert!(lit[0] > lit[1] && lit[1] > lit[2], "{lit:?}");
        assert!(shift[2] < 0., "the angle still sliding in from the left");
        // Typed and run: all lit and home, the angle ahead.
        let (lit, shift, ahead) = coding(0.45);
        assert!(lit.iter().all(|l| (l - 1.).abs() < 1e-3), "{lit:?}");
        assert!(shift.iter().all(|s| s.abs() < 1e-3), "{shift:?}");
        assert!(ahead > 0.05);
        // The cursor blinks: the dot dims, the rest stay lit.
        let (lit, _, _) = coding(0.52);
        assert!((lit[0] - TRACE).abs() < 1e-4 && lit[1] > 0.99, "{lit:?}");
        // Cleared right to left: the angle gone before the dot.
        let (lit, _, _) = coding(0.86);
        assert!(lit[2] < lit[1] && lit[1] <= lit[0], "{lit:?}");
        // The lap closes where it starts, so the loop doesn't jump.
        let (a, b) = (coding(0.999), coding(0.0));
        for i in 0..3 {
            assert!((a.0[i] - b.0[i]).abs() < 0.02, "lit {i}");
        }
        // Never fully gone.
        for k in 0..200 {
            let (lit, _, _) = coding(k as f32 / 200.);
            assert!(lit.iter().all(|l| *l >= TRACE - 1e-4));
        }
    }

    #[test]
    fn heat_runs_white_to_bright_to_deep_to_dark_then_the_stroke() {
        let rgb = |c: u32| -> Hsla { gpui::rgb(c).into() };
        let ramp = [rgb(0x2a0d17), rgb(0x7a1f3d), rgb(0xe0889f), rgb(0xfff6f8)];
        let burgundy = rgb(0x7a1f3d);
        let close = |a: Hsla, b: Hsla| {
            let (a, b) = (a.to_rgb(), b.to_rgb());
            (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() < 0.02
        };
        assert!(close(glowing(burgundy, 1., ramp), ramp[3]), "white");
        assert!(close(glowing(burgundy, 2. / 3., ramp), ramp[2]), "bright");
        assert!(close(glowing(burgundy, 1. / 3., ramp), ramp[1]), "deep");
        assert!(close(glowing(burgundy, LOW, ramp), ramp[0]), "dark");
        assert!(close(glowing(burgundy, 0., ramp), burgundy), "settled");
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
