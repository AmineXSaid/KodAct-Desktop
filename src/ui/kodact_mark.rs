//! KodAct's mark: two strokes on one slope, a step up from the left, then a
//! chevron pointing right. Code goes in, impact comes out. Each stroke is a
//! one-colour SVG made from the geometry here (`kodact/step.svg`,
//! `kodact/chevron.svg`, served by `assets.rs`) and tinted from the theme
//! (`mark_top`), so it is drawn smooth-edged at any size, on any GPU, and
//! follows light and dark.
//!
//! - [`mark`]: the mark at rest.
//! - [`working`]: the mark while KodAct works: the step lights, then the
//!   chevron, which leans ahead and settles. Reduced motion: at rest.
//! - [`landing`]: the mark arriving (onboarding, the opener): the step
//!   rises in, the chevron follows it through and settles.
//! - [`struck`]: one push: the chevron drives ahead and flares.
//! - [`wordmark`]: the mark, then KodAct, its Act in the theme's colour.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, svg, Animation, AnimationExt, AnyElement, App, ElementId, Hsla, Window,
};

use super::motion;
use crate::theme::{ActiveTheme, Theme};

/// The mark as drawn (`design/brand/kodact-mark.svg` in the KodAct
/// repository): its box, its two strokes, and the round join's width.
const VIEW: (f32, f32) = (505.7, 302.);
const STEP: [(f32, f32); 6] = [
    (4., 242.),
    (177.17, 100.),
    (241., 100.),
    (295.88, 145.),
    (244., 145.),
    (125.7, 242.),
];
const CHEVRON: [(f32, f32); 9] = [
    (316., 4.),
    (497.71, 153.),
    (320.88, 298.),
    (171., 298.),
    (307.59, 186.),
    (347., 186.),
    (378.71, 160.),
    (277.49, 77.),
    (226.98, 77.),
];
const JOIN: f32 = 8.;
/// How much of the box's width the mark fills.
const FILL: f32 = 0.92;
/// The stroke SVGs are drawn in a 64-unit box.
const BOX: f32 = 64.;
/// One lap of the working mark, in milliseconds.
const LAP_MS: u64 = 1500;
/// How far the chevron leans ahead as it lights, and how far a push drives
/// it, as fractions of the box.
const LEAN: f32 = 0.05;
const PUSH: f32 = 0.11;
/// A stroke waiting its turn while the mark works.
const DIM: f32 = 0.38;

/// Which stroke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Stroke {
    Step,
    Chevron,
}

impl Stroke {
    fn name(self) -> &'static str {
        match self {
            Stroke::Step => "step",
            Stroke::Chevron => "chevron",
        }
    }
}

/// A stroke's outline in a `size` box with its origin at the top left: the
/// mark centred, `FILL` of the box wide.
pub(crate) fn outline(stroke: Stroke, size: f32) -> Vec<(f32, f32)> {
    let s = size * FILL / VIEW.0;
    let (ox, oy) = ((size - VIEW.0 * s) / 2., (size - VIEW.1 * s) / 2.);
    let pts: &[(f32, f32)] = match stroke {
        Stroke::Step => &STEP,
        Stroke::Chevron => &CHEVRON,
    };
    pts.iter().map(|(x, y)| (ox + x * s, oy + y * s)).collect()
}

/// One stroke as an SVG for `assets.rs`: `kodact/step.svg` or
/// `kodact/chevron.svg`. `None` for any other path.
pub(crate) fn stroke_svg(path: &str) -> Option<Vec<u8>> {
    let stroke = match path.strip_prefix("kodact/")?.strip_suffix(".svg")? {
        "step" => Stroke::Step,
        "chevron" => Stroke::Chevron,
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

/// The mark at one moment, `size` px square, in the theme's mark colour.
#[derive(IntoElement)]
pub struct Mark {
    size: f32,
    /// 0..1: how lit the mark is: 1 white, through the bright signal, to 0
    /// settled in its own colour (`glowing`).
    heat: f32,
    /// While the strokes are still arriving (`landing`): how far along, 0..1.
    landing: Option<f32>,
    /// How far ahead (to the right) the chevron sits, as a fraction of the box.
    ahead: f32,
    /// Each stroke's opacity (step, chevron).
    lit: (f32, f32),
    /// The mark's colour when not the theme's: the opener draws the dark
    /// theme's mark in either theme, a working line its live colour.
    color: Option<Hsla>,
}

impl Mark {
    /// Draw the mark in this colour instead of the theme's.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for Mark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let x = &cx.theme().extra;
        let ramp = [
            x.mark_heat_iron,
            x.mark_heat_ember,
            x.mark_heat,
            x.mark_heat_white,
        ];
        let cool = self.color.unwrap_or(x.mark_top);
        let mut d = div().relative().size(px(self.size)).flex_none();
        for stroke in [Stroke::Step, Stroke::Chevron] {
            let color = glowing(cool, self.heat * stroke_heat(stroke), ramp);
            let (dx, dy, alpha) = self
                .landing
                .map_or((0., 0., 1.), |p| arrive(stroke, p, self.size));
            let (lead, lit) = match stroke {
                Stroke::Step => (0., self.lit.0),
                Stroke::Chevron => (self.ahead * self.size, self.lit.1),
            };
            d = d.child(
                svg()
                    .path(format!("kodact/{}.svg", stroke.name()))
                    .absolute()
                    .top(px(dy))
                    .left(px(dx + lead))
                    .size(px(self.size))
                    .opacity(alpha * lit)
                    .text_color(color),
            );
        }
        d
    }
}

/// How lit a stroke runs against the mark's heat: the chevron, where the
/// impact is, brightest; the step a little behind it.
fn stroke_heat(stroke: Stroke) -> f32 {
    match stroke {
        Stroke::Chevron => 1.,
        Stroke::Step => 0.85,
    }
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

/// Where a stroke is while the mark arrives (`p` 0..1): the step rises in
/// along its slope, then the chevron comes through from behind it, each
/// overshooting a touch and settling. Returns its offset in px and its opacity.
fn arrive(stroke: Stroke, p: f32, size: f32) -> (f32, f32, f32) {
    let (start, from) = match stroke {
        Stroke::Step => (0., (-0.34, 0.28)),
        Stroke::Chevron => (0.25, (-0.5, 0.)),
    };
    let t = ((p - start) / 0.5).clamp(0., 1.);
    // Ease out with a slight overshoot ("back").
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
        lit: (1., 1.),
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

/// The mark pushed: `k` (0..1) through one push, the chevron driving ahead
/// and flaring at the middle, back in place and settled at the end.
pub fn struck(size: f32, k: f32) -> Mark {
    let bump = (k.clamp(0., 1.) * std::f32::consts::PI).sin();
    Mark {
        heat: STRIKE_HEAT * bump,
        ahead: PUSH * bump,
        ..mark(size)
    }
}

/// The mark while KodAct works, in the theme's colour or `color`.
/// Reduced motion: [`mark`].
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
                let (step, chevron) = beat(k);
                d.child(tinted(Mark {
                    ahead: LEAN * chevron,
                    lit: (DIM + (1. - DIM) * step, DIM + (1. - DIM) * chevron),
                    ..mark(size)
                }))
            },
        )
        .into_any_element()
}

/// How lit each stroke is `k` (0..1) into a lap: the step swells first,
/// the chevron after it, then both rest before the next lap.
fn beat(k: f32) -> (f32, f32) {
    let swell = |centre: f32| {
        let x = (1. - (k - centre).abs() / 0.24).clamp(0., 1.);
        x * x * (3. - 2. * x)
    };
    (swell(0.26), swell(0.52))
}

/// The mark, then "KodAct" set at `height` px, Act in the theme's colour.
pub fn wordmark(height: f32, t: &Theme) -> AnyElement {
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
        .child(
            // Centred on the capitals, which sit a little above the line's
            // middle (the line keeps room below for descenders).
            div()
                .relative()
                .top(px(-height * WORD_LIFT))
                .mr(px(height * WORD_GAP))
                .child(mark(mark_box(height))),
        )
        .child(letters("Kod", t.colors.text_strong))
        .child(letters("Act", t.colors.primary))
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
        let all: Vec<(f32, f32)> = [Stroke::Step, Stroke::Chevron]
            .into_iter()
            .flat_map(|s| outline(s, 64.))
            .collect();
        let (x0, y0, x1, y1) = bbox(&all);
        // Its strokes sit a join's half inside the drawing's edge.
        let half = JOIN / 2. * 64. * FILL / VIEW.0;
        assert!((x0 - half - (64. - 64. * FILL) / 2.).abs() < 0.01, "{x0}");
        // (The drawing sits half a unit left of its box's middle.)
        assert!((x0 + x1 - 64.).abs() < 0.6 && (y0 + y1 - 64.).abs() < 0.2);
        assert!(x0 > half && y0 > half && x1 < 64. - half && y1 < 64. - half);
    }

    #[test]
    fn the_step_leads_into_the_chevron_which_points_right() {
        let (step, chevron) = (
            bbox(&outline(Stroke::Step, 64.)),
            bbox(&outline(Stroke::Chevron, 64.)),
        );
        // The step starts at the left edge; the chevron's tip is the right edge.
        assert!(step.0 < chevron.0 && chevron.2 > step.2);
        // The chevron is the taller stroke, the step inside its height.
        assert!(chevron.1 < step.1 && chevron.3 > step.3);
    }

    #[test]
    fn both_strokes_are_served_and_nothing_else() {
        for name in ["step", "chevron"] {
            let svg = stroke_svg(&format!("kodact/{name}.svg")).expect("served");
            let svg = String::from_utf8(svg).unwrap();
            assert!(svg.starts_with("<svg") && svg.contains(" d=\"M"), "{svg}");
            assert!(svg.contains("stroke-linejoin=\"round\""), "{svg}");
        }
        assert!(stroke_svg("kodact/tip.svg").is_none());
        assert!(stroke_svg("icons/x.svg").is_none());
    }

    #[test]
    fn landing_brings_both_strokes_home_and_hides_them_before() {
        for stroke in [Stroke::Step, Stroke::Chevron] {
            let (dx, dy, a) = arrive(stroke, 1., 88.);
            assert!(dx.abs() < 1e-3 && dy.abs() < 1e-3 && a == 1., "{stroke:?}");
        }
        // Before its turn the chevron is hidden, behind its place.
        let (dx, _, a) = arrive(Stroke::Chevron, 0.2, 88.);
        assert!(a == 0. && dx < -30.);
        // The step lands first.
        assert!(arrive(Stroke::Step, 0.4, 88.).2 > arrive(Stroke::Chevron, 0.4, 88.).2);
        // Landed, the arriving mark is the mark itself.
        assert!(landing(88., 1., 0.).landing.is_none());
    }

    #[test]
    fn a_push_drives_the_chevron_ahead_and_flares_on_the_way() {
        assert!(struck(20., 0.).ahead.abs() < 1e-6);
        assert!((struck(20., 0.5).ahead - PUSH).abs() < 1e-3);
        assert!((struck(20., 0.5).heat - STRIKE_HEAT).abs() < 1e-3);
        assert!(struck(20., 1.).heat < 1e-3 && struck(20., 1.).ahead < 1e-3);
    }

    #[test]
    fn working_lights_the_step_then_the_chevron_then_rests() {
        let (step, chevron) = beat(0.26);
        assert!(step > 0.99 && chevron < 0.01);
        let (step, chevron) = beat(0.52);
        assert!(chevron > 0.99 && step < 0.01);
        // A lap begins and ends at rest, so it loops without a jump.
        for k in [0., 0.8, 1.] {
            assert_eq!(beat(k), (0., 0.), "{k}");
        }
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
        // The chevron runs brighter than the step.
        assert!(stroke_heat(Stroke::Chevron) > stroke_heat(Stroke::Step));
    }

    #[test]
    fn the_wordmark_s_mark_stands_a_little_over_the_capitals() {
        // The mark's own height in its box, against the capitals' at that size.
        let size = 40.;
        let tall = mark_box(size) * FILL * VIEW.1 / VIEW.0;
        assert!((tall / (size * WORD_CAP) - 1.2).abs() < 1e-3);
    }
}
