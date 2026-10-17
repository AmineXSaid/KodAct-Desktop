//! The home screen's field of dots: a fine grid of tiny warm dots that
//! gathers beside the greeting and toward the lower right, dissolves toward
//! the middle (where the heading and the composer sit, kept quiet) and fades
//! out before the edges, so it melts into the window's paper. A few dots
//! carry the accent; fewer still are tiny outlined squares or diamonds.
//!
//! It is painted, not laid out: one `canvas` draws every dot as a quad, from
//! geometry computed once per size (positions on a grid anchored at the
//! centre, each dot's weight, seed and kind) and kept until the field is
//! resized. Nothing in it takes input; it sits behind the content.
//!
//! It is barely alive: the dots breathe out of step with each other over
//! nine seconds, the whole field leans a few pixels away from the pointer,
//! and it brightens a touch while the composer has the keyboard. A ticker
//! drives that at a few frames a second (faster only while it eases toward
//! the pointer), and rests whenever the window is inactive or the field
//! isn't on screen. Reduced motion (`FORGE_REDUCED_MOTION=1`, or the
//! system's setting) leaves a still field and no ticker at all.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, point, prelude::*, px, size, Bounds, ContentMask, Context, Corners, FocusHandle,
    Hsla, Pixels, Point, Render, Window,
};

use crate::theme::{ActiveTheme, Theme};

/// The furthest the field leans from the pointer, in pixels.
const LEAN: f32 = 4.;
/// One breath, in seconds.
const BREATH_S: f32 = 9.;
/// How far a breath swings a dot's opacity, as a share of it.
const BREATH_DEPTH: f32 = 0.22;
/// Ticks: easing toward the pointer, breathing, and resting (hidden).
const EASE_MS: u64 = 33;
const BREATHE_MS: u64 = 220;
const REST_MS: u64 = 600;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Dot,
    Accent,
    Square,
    Diamond,
}

/// One mark of the field, placed relative to the field's top-left corner.
#[derive(Clone, Copy, Debug)]
pub struct Mark {
    pub at: Point<f32>,
    /// 0..1: how strongly the mark shows, from the field's shape.
    pub weight: f32,
    /// 0..1, stable per grid cell: its breathing phase and its depth.
    pub seed: f32,
    pub kind: Kind,
}

/// A grid cell's stable random number, 0..1 (SplitMix64 on the cell).
fn hash(i: i64, j: i64, salt: u64) -> f32 {
    let mut z = (i as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add(salt);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// A soft blob: 1 at `(cx, cy)`, falling off over `(sx, sy)`.
fn blob(u: f32, v: f32, cx: f32, cy: f32, sx: f32, sy: f32) -> f32 {
    let (du, dv) = ((u - cx) / sx, (v - cy) / sy);
    (-(du * du + dv * dv)).exp()
}

/// How strongly the field shows at `(x, y)` in a `w`×`h` field: two clouds
/// (beside the heading, toward the lower right) and a faint band round the
/// content, cut by a quiet oval behind the heading and the composer and
/// faded before the field's edges.
pub fn weight(x: f32, y: f32, w: f32, h: f32) -> f32 {
    if w <= 0. || h <= 0. {
        return 0.;
    }
    let (u, v) = (x / w - 0.5, y / h - 0.45);
    // The quiet middle: nothing inside the oval, dots arriving past it. Its
    // size follows the content's (a column at most ~620px wide), not only
    // the window's.
    let (qx, qy) = ((280. / w).clamp(0.18, 0.42), (165. / h).clamp(0.16, 0.34));
    let d = ((u / qx).powi(2) + (v / qy).powi(2)).sqrt();
    let quiet = smoothstep(0.9, 1.45, d);
    let clouds = blob(u, v, -0.31, -0.19, 0.19, 0.16).max(blob(u, v, 0.31, 0.29, 0.23, 0.18));
    let band = 0.16 * smoothstep(1.2, 1.9, d) * (1. - smoothstep(2.4, 3.6, d));
    // Fade out before every edge, so the field never ends on a line.
    let edge = (x.min(w - x) / (w * 0.07)).min(y.min(h - y) / (h * 0.09));
    let edge = smoothstep(0., 1., edge);
    ((clouds + band).min(1.) * quiet * edge).clamp(0., 1.)
}

/// The field's marks for a `w`×`h` field on a `gap` grid. The grid is
/// anchored at the field's centre, so it stays put as panels open beside it.
pub fn layout(w: f32, h: f32, gap: f32) -> Vec<Mark> {
    let mut out = Vec::new();
    if w <= 0. || h <= 0. || gap <= 0. {
        return out;
    }
    let (cx, cy) = (w / 2., h / 2.);
    let (ni, nj) = ((cx / gap).ceil() as i64, (cy / gap).ceil() as i64);
    for j in -nj..=nj {
        for i in -ni..=ni {
            let (x, y) = (cx + i as f32 * gap, cy + j as f32 * gap);
            if x < 0. || y < 0. || x > w || y > h {
                continue;
            }
            let wt = weight(x, y, w, h);
            // Thinned as well as faded: the density itself dissolves.
            if wt < 0.03 || hash(i, j, 1) > wt * 1.35 {
                continue;
            }
            let pick = hash(i, j, 2);
            let kind = if pick < 0.016 {
                Kind::Accent
            } else if pick < 0.021 && wt > 0.35 {
                Kind::Square
            } else if pick < 0.025 && wt > 0.35 {
                Kind::Diamond
            } else {
                Kind::Dot
            };
            out.push(Mark {
                at: point(x, y),
                weight: wt,
                seed: hash(i, j, 3),
                kind,
            });
        }
    }
    out
}

/// What the ticker moves, and what paint reads.
#[derive(Clone, Copy, Debug)]
struct Motion {
    /// Seconds of breathing so far.
    phase: f32,
    lean: Point<f32>,
    target: Point<f32>,
    /// 0..1, toward 1 while the composer has the keyboard.
    glow: f32,
}

/// Where the field was painted, and when.
type Painted = (Bounds<Pixels>, Instant);

#[derive(Default)]
struct Cache {
    size: (u32, u32, u32),
    marks: Rc<Vec<Mark>>,
}

pub struct DotField {
    motion: Motion,
    /// The keyboard focus that brightens the field (the composer's).
    focus: Option<FocusHandle>,
    cache: Rc<RefCell<Cache>>,
    /// Where the field was last painted, and when: the ticker leans away
    /// from the pointer inside it, and rests once it is no longer painted.
    painted: Rc<Cell<Option<Painted>>>,
}

impl DotField {
    pub fn new(focus: Option<FocusHandle>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let still = super::motion::reduced_motion() || cx.reduce_motion();
        let this = Self {
            motion: Motion {
                phase: 0.,
                lean: point(0., 0.),
                target: point(0., 0.),
                glow: 0.,
            },
            focus,
            cache: Rc::default(),
            painted: Rc::default(),
        };
        if !still {
            cx.spawn_in(window, async move |this, cx| {
                let mut last = Instant::now();
                loop {
                    let wait = this
                        .update_in(cx, |this, window, cx| {
                            let now = Instant::now();
                            let dt = now.duration_since(last).as_secs_f32().min(0.5);
                            last = now;
                            this.tick(dt, window, cx)
                        })
                        .ok();
                    let Some(wait) = wait else { break };
                    cx.background_executor().timer(wait).await;
                }
            })
            .detach();
        }
        this
    }

    /// One step: breathe, ease toward the pointer and the composer's focus,
    /// and repaint if the field is on screen. Returns how long to wait.
    fn tick(&mut self, dt: f32, window: &mut Window, cx: &mut Context<Self>) -> Duration {
        let shown = self
            .painted
            .get()
            .filter(|(_, at)| at.elapsed() < Duration::from_millis(REST_MS * 2));
        let Some((bounds, _)) = shown.filter(|_| window.is_window_active()) else {
            return Duration::from_millis(REST_MS);
        };
        let m = &mut self.motion;
        m.phase = (m.phase + dt) % (BREATH_S * 100.);
        // Lean away from the pointer, by where it is across the field.
        let mouse = window.mouse_position();
        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        if w > 0. && h > 0. {
            let u = (f32::from(mouse.x - bounds.origin.x) / w - 0.5).clamp(-0.5, 0.5);
            let v = (f32::from(mouse.y - bounds.origin.y) / h - 0.5).clamp(-0.5, 0.5);
            m.target = point(-u * 2. * LEAN, -v * 2. * LEAN);
        }
        let k = (dt * 6.).min(1.);
        m.lean = point(
            m.lean.x + (m.target.x - m.lean.x) * k,
            m.lean.y + (m.target.y - m.lean.y) * k,
        );
        let want = self
            .focus
            .as_ref()
            .is_some_and(|f| f.contains_focused(window, cx)) as u8 as f32;
        m.glow += (want - m.glow) * (dt * 4.).min(1.);
        let easing = (m.target.x - m.lean.x).abs() + (m.target.y - m.lean.y).abs() > 0.05
            || (want - m.glow).abs() > 0.01;
        cx.notify();
        Duration::from_millis(if easing { EASE_MS } else { BREATHE_MS })
    }
}

/// The marks for a field this size: the cached ones, or laid out afresh
/// when the size (or the grid) changed.
fn marks(cache: &RefCell<Cache>, bounds: Bounds<Pixels>, gap: f32) -> Rc<Vec<Mark>> {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let key = (w.round() as u32, h.round() as u32, gap.round() as u32);
    let mut cache = cache.borrow_mut();
    if cache.size != key {
        *cache = Cache {
            size: key,
            marks: Rc::new(layout(w, h, gap)),
        };
    }
    cache.marks.clone()
}

/// A mark's opacity now: its weight, breathing out of step with the others.
fn alpha(mark: &Mark, phase: f32, glow: f32, peak: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let breath = 1. + BREATH_DEPTH * ((phase / BREATH_S + mark.seed) * tau).sin();
    (0.22 + 0.78 * mark.weight) * peak * breath * (1. + 0.14 * glow)
}

fn paint_marks(
    bounds: Bounds<Pixels>,
    marks: &[Mark],
    motion: Motion,
    t: &Theme,
    window: &mut Window,
) {
    let peak = if t.is_dark { 0.55 } else { 0.78 };
    let (base, accent) = (t.extra.dot, t.extra.accent);
    let d = f32::from(t.size.dot);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for m in marks {
            // Deeper dots lean further: a hint of depth, nothing more.
            let depth = 0.55 + 0.45 * m.seed;
            let at = point(
                bounds.origin.x + px(m.at.x + motion.lean.x * depth),
                bounds.origin.y + px(m.at.y + motion.lean.y * depth),
            );
            let a = alpha(m, motion.phase, motion.glow, peak).min(1.);
            let tint = |c: Hsla, k: f32| Hsla {
                a: c.a * a * k,
                ..c
            };
            match m.kind {
                Kind::Dot | Kind::Accent => {
                    let (c, size_k) = if m.kind == Kind::Accent {
                        (tint(accent, 0.9), 1.25)
                    } else {
                        (tint(base, 1.), 0.8 + 0.3 * m.weight)
                    };
                    let s = px(d * size_k);
                    window.paint_quad(gpui::quad(
                        Bounds::new(at - point(s / 2., s / 2.), size(s, s)),
                        Corners::all(s / 2.),
                        c,
                        px(0.),
                        c.opacity(0.),
                        Default::default(),
                    ));
                }
                Kind::Square => {
                    let s = px(5.);
                    window.paint_quad(gpui::quad(
                        Bounds::new(at - point(s / 2., s / 2.), size(s, s)),
                        Corners::all(px(1.)),
                        base.opacity(0.),
                        px(1.),
                        tint(base, 0.9),
                        Default::default(),
                    ));
                }
                Kind::Diamond => {
                    let r = px(3.5);
                    let mut p = gpui::PathBuilder::stroke(px(1.));
                    p.move_to(at - point(px(0.), r));
                    p.line_to(at + point(r, px(0.)));
                    p.line_to(at + point(px(0.), r));
                    p.line_to(at - point(r, px(0.)));
                    p.close();
                    if let Ok(path) = p.build() {
                        window.paint_path(path, tint(accent, 0.75));
                    }
                }
            }
        }
    });
}

impl Render for DotField {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let motion = self.motion;
        let gap = f32::from(t.size.dot_gap);
        let painted = self.painted.clone();
        let cache = self.cache.clone();
        div().absolute().inset_0().child(
            canvas(
                move |bounds, _, _| marks(&cache, bounds, gap),
                move |bounds, marks, window, _| {
                    painted.set(Some((bounds, Instant::now())));
                    paint_marks(bounds, &marks, motion, &t, window);
                },
            )
            .size_full(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_stays_quiet_and_the_edges_fade() {
        let (w, h) = (1000., 700.);
        // Behind the heading and the composer: nothing.
        assert_eq!(weight(w * 0.5, h * 0.45, w, h), 0.);
        assert_eq!(weight(w * 0.5, h * 0.55, w, h), 0.);
        // At the window's edge: nothing, so the field never ends on a line.
        assert_eq!(weight(0., h * 0.3, w, h), 0.);
        assert_eq!(weight(w * 0.8, h, w, h), 0.);
        // Beside the heading and toward the lower right: the clouds.
        assert!(weight(w * 0.21, h * 0.28, w, h) > 0.5);
        assert!(weight(w * 0.81, h * 0.74, w, h) > 0.5);
    }

    #[test]
    fn the_field_is_stable_and_sparse() {
        let a = layout(1000., 700., 14.);
        let b = layout(1000., 700., 14.);
        assert_eq!(a.len(), b.len());
        assert!(a
            .iter()
            .zip(&b)
            .all(|(x, y)| x.at == y.at && x.kind == y.kind));
        // Far fewer marks than grid cells: a field, not a carpet.
        let cells = (1000. / 14.) * (700. / 14.);
        assert!((a.len() as f32) < cells * 0.35, "{} marks", a.len());
        assert!(a.len() > 200, "{} marks", a.len());
        // Accents are rare.
        let accents = a.iter().filter(|m| m.kind != Kind::Dot).count();
        assert!(accents * 20 < a.len(), "{accents} accents in {}", a.len());
    }

    #[test]
    fn a_breath_stays_subtle() {
        let m = Mark {
            at: point(0., 0.),
            weight: 1.,
            seed: 0.3,
            kind: Kind::Dot,
        };
        let samples: Vec<f32> = (0..90)
            .map(|i| alpha(&m, i as f32 * 0.1, 0., 0.6))
            .collect();
        let (lo, hi) = samples
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), a| (l.min(*a), h.max(*a)));
        assert!(hi / lo < 1.6, "{lo}..{hi}");
        assert!(hi <= 1.);
    }
}
