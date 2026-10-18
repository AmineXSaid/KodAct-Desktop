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
//! Once a chat begins it stays behind the transcript as a faint trace
//! (`set_backdrop`), and while Forge is replying a slow wave of light runs
//! down through it, so the background says the work is going on; it goes
//! still again when the reply ends.
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
/// A ripple (`DotField::pulse`): how fast its ring runs out, how wide it
/// is, how far it goes, and how long it lasts.
const RIPPLE_SPEED: f32 = 420.;
const RIPPLE_BAND: f32 = 34.;
const RIPPLE_REACH: f32 = 700.;
const RIPPLE_S: f32 = 1.8;
/// Behind a chat: how much of its home-screen strength the field keeps,
/// how far a working wave lifts it at its crest, the wave's speed (radians
/// a second) and how many crests span the field.
const CHAT_REST: f32 = 0.22;
const WORK_LIFT: f32 = 2.2;
const WORK_SPEED: f32 = 3.4;
const WORK_WAVES: f32 = 0.65;
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

/// A soft blob: 1 at `(cx, cy)`, falling off over `(rx, ry)` pixels.
fn blob(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> f32 {
    let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
    (-(dx * dx + dy * dy)).exp()
}

/// Smooth value noise, 0..1: stable, low-frequency variation that keeps the
/// clouds' edges and density from reading as a regular halftone.
fn noise(x: f32, y: f32, salt: u64) -> f32 {
    let (i, j) = (x.floor(), y.floor());
    let (fx, fy) = (x - i, y - y.floor());
    let (sx, sy) = (fx * fx * (3. - 2. * fx), fy * fy * (3. - 2. * fy));
    let (i, j) = (i as i64, j as i64);
    let at = |a: i64, b: i64| hash(a, b, salt);
    let top = at(i, j) * (1. - sx) + at(i + 1, j) * sx;
    let bottom = at(i, j + 1) * (1. - sx) + at(i + 1, j + 1) * sx;
    top * (1. - sy) + bottom * sy
}

fn fbm(x: f32, y: f32, salt: u64) -> f32 {
    0.65 * noise(x, y, salt) + 0.35 * noise(x * 2.1, y * 2.1, salt + 1)
}

/// The distance from `(x, y)` to a rounded rectangle (negative inside).
fn rect_distance(x: f32, y: f32, r: [f32; 4], radius: f32) -> f32 {
    let [l, t, rr, b] = r;
    let (cx, cy) = ((l + rr) / 2., (t + b) / 2.);
    let (hx, hy) = ((rr - l) / 2. - radius, (b - t) / 2. - radius);
    let (qx, qy) = ((x - cx).abs() - hx, (y - cy).abs() - hy);
    qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - radius
}

/// Where the content sits when it hasn't been measured yet: a column at
/// the field's centre.
pub fn default_content(w: f32, h: f32) -> [f32; 4] {
    let (hw, hh) = ((w * 0.45).min(296.), (h * 0.3).min(150.));
    [w / 2. - hw, h / 2. - hh, w / 2. + hw, h / 2. + hh]
}

/// Behind a chat, what the field keeps clear: the reading column, centred
/// and running the field's full height, so the dots stay in the margins
/// beside the transcript and never lift behind its words.
pub fn chat_content(w: f32, h: f32, column: f32) -> [f32; 4] {
    let half = column.min(w) / 2.;
    [w / 2. - half, 0., w / 2. + half, h]
}

/// How strongly the field shows at `(x, y)` in a `w`×`h` field around the
/// content (`[left, top, right, bottom]`, in the field's coordinates).
///
/// Two clouds hug the content's outer edges, one beside the heading and one
/// past the composer's lower right, sized by the room each side has. Their
/// outlines are warped and their density varied by slow noise, so they
/// dissolve rather than end; a quiet margin keeps every dot off the content,
/// and the field fades before the window's edges.
pub fn weight(x: f32, y: f32, w: f32, h: f32, content: [f32; 4]) -> f32 {
    if w <= 0. || h <= 0. {
        return 0.;
    }
    let [l, t, r, b] = content;
    let wx = x + (fbm(x / 260., y / 260., 11) - 0.5) * 90.;
    let wy = y + (fbm(x / 260. + 7., y / 260. + 3., 13) - 0.5) * 90.;
    let (room_l, room_r) = (l.max(1.), (w - r).max(1.));
    let beside = blob(
        wx,
        wy,
        l - room_l * 0.18,
        t + (b - t) * 0.12,
        (room_l * 0.7).max(170.),
        (h * 0.25).max(140.),
    );
    let below = blob(
        wx,
        wy,
        r + room_r * 0.12,
        b + (h - b) * 0.3,
        (room_r * 0.75).max(200.),
        (h * 0.26).max(150.),
    );
    let texture = 0.55 + 0.45 * fbm(x / 140. + 31., y / 140. + 17., 17);
    // The quiet margin narrows in a small window, so the field shrinks into
    // the room that's left instead of vanishing.
    let ramp = (w.min(h) * 0.15).clamp(48., 120.);
    let quiet = smoothstep(28., 28. + ramp, rect_distance(x, y, content, 24.));
    let edge = x.min(w - x).min(y).min(h - y) / (w.min(h) * 0.08).max(24.);
    let edge = smoothstep(0., 1., edge);
    (beside.max(below) * texture * quiet * edge).clamp(0., 1.)
}

/// The field's marks for a `w`×`h` field on a `gap` grid around `content`.
/// The grid is anchored at the field's centre.
pub fn layout(w: f32, h: f32, gap: f32, content: [f32; 4]) -> Vec<Mark> {
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
            let wt = weight(x, y, w, h, content);
            // Mostly faded rather than thinned: thinning alone reads as a
            // halftone, so only the faintest edges lose dots.
            if wt < 0.02 || hash(i, j, 1) > wt * 2.4 + 0.05 {
                continue;
            }
            let pick = hash(i, j, 2);
            let kind = if pick < 0.014 {
                Kind::Accent
            } else if pick < 0.018 && wt > 0.4 {
                Kind::Square
            } else if pick < 0.021 && wt > 0.4 {
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
    /// 0..1, toward 1 behind a chat (`set_backdrop`): the field steps back.
    quiet: f32,
    /// 0..1, toward 1 while the chat it sits behind is replying.
    work: f32,
}

/// Where the field was painted, and when.
type Painted = (Bounds<Pixels>, Instant);

#[derive(Default)]
struct Cache {
    key: [i32; 7],
    marks: Rc<Vec<Mark>>,
}

/// What paint and the ticker share across frames.
#[derive(Default)]
struct Shared {
    /// Where the field was last painted, and when: the ticker leans away
    /// from the pointer inside it, and rests once it is no longer painted.
    painted: Cell<Option<Painted>>,
    /// The content the field arranges itself around, in window coordinates,
    /// as `content_probe` measured it last frame.
    content: Cell<Option<Bounds<Pixels>>>,
    /// Where the content's centre was last painted, and how far the dots
    /// still trail behind it: when the sidebar opens or the window resizes,
    /// the field glides to its new place instead of jumping.
    anchor: Cell<Option<Point<Pixels>>>,
    slide: Cell<Point<f32>>,
    /// Ripples running out from the content's centre: when each began, and
    /// how strong it is (1 a strike, less a keystroke).
    ripples: RefCell<Vec<(Instant, f32)>>,
    /// Set each frame by the view showing the field: behind a chat, and
    /// whether that chat is replying.
    backdrop: Cell<bool>,
    working: Cell<bool>,
}

pub struct DotField {
    motion: Motion,
    /// The keyboard focus that brightens the field (the composer's).
    focus: Option<FocusHandle>,
    cache: Rc<RefCell<Cache>>,
    shared: Rc<Shared>,
    /// Reduced motion: no ticker, so render sets the backdrop directly.
    still: bool,
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
                quiet: 0.,
                work: 0.,
            },
            focus,
            cache: Rc::default(),
            shared: Rc::default(),
            still,
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
            .shared
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
        // Stepping back behind a chat takes about half a second; the wave
        // comes and goes a little slower. Steps are capped to one fast tick,
        // so the first step after a slow (breathing) tick doesn't jump.
        let step = dt.min(EASE_MS as f32 / 1000.);
        let quiet = self.shared.backdrop.get() as u8 as f32;
        m.quiet += (quiet - m.quiet) * (step * 4.).min(1.);
        let work = (self.shared.working.get() && self.shared.backdrop.get()) as u8 as f32;
        m.work += (work - m.work) * (step * 2.5).min(1.);
        if m.work < 0.002 {
            m.work = 0.;
        }
        // The glide after a layout change: quick, then settling.
        let slide = self.shared.slide.get();
        let decay = (-dt * 9.).exp();
        let slide = point(slide.x * decay, slide.y * decay);
        let gliding = slide.x.abs() + slide.y.abs() > 0.3;
        self.shared
            .slide
            .set(if gliding { slide } else { point(0., 0.) });
        let rippling = {
            let mut r = self.shared.ripples.borrow_mut();
            r.retain(|(at, _)| at.elapsed().as_secs_f32() < RIPPLE_S);
            !r.is_empty()
        };
        let easing = (m.target.x - m.lean.x).abs() + (m.target.y - m.lean.y).abs() > 0.05
            || (want - m.glow).abs() > 0.01
            || gliding
            || rippling
            || (quiet - m.quiet).abs() > 0.01
            || m.work > 0.;
        cx.notify();
        Duration::from_millis(if easing { EASE_MS } else { BREATHE_MS })
    }
}

/// The marks for a field this size around this content: the cached ones,
/// or laid out afresh when either changed (by 4px or more).
fn marks(
    cache: &RefCell<Cache>,
    bounds: Bounds<Pixels>,
    gap: f32,
    content: [f32; 4],
) -> Rc<Vec<Mark>> {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let q = |v: f32| (v / 4.).round() as i32;
    let key = [
        q(w),
        q(h),
        gap.round() as i32,
        q(content[0]),
        q(content[1]),
        q(content[2]),
        q(content[3]),
    ];
    let mut cache = cache.borrow_mut();
    if cache.key != key || cache.marks.is_empty() {
        *cache = Cache {
            key,
            marks: Rc::new(layout(w, h, gap, content)),
        };
    }
    cache.marks.clone()
}

impl DotField {
    /// Where the field is shown: `chat` behind a conversation rather than on
    /// the home screen, and whether that conversation is `working` (Forge
    /// replying). The view showing the field calls it as it renders.
    pub fn set_backdrop(&self, chat: bool, working: bool) {
        self.shared.backdrop.set(chat);
        self.shared.working.set(working);
    }

    /// A ring of light running out through the dots from the content's
    /// centre: `strength` 1 for a strike, less for a keystroke. Nothing
    /// under reduced motion.
    pub fn pulse(&mut self, strength: f32, cx: &mut Context<Self>) {
        if super::motion::reduced_motion() || cx.reduce_motion() {
            return;
        }
        let mut r = self.shared.ripples.borrow_mut();
        // A burst of keystrokes keeps at most a few rings in flight.
        if r.len() >= 6 {
            r.remove(0);
        }
        r.push((Instant::now(), strength.clamp(0., 1.)));
        drop(r);
        cx.notify();
    }

    /// An invisible element to place inside the content the field should
    /// arrange itself around (the home screen's heading, composer and
    /// starters): it measures that content every frame.
    pub fn content_probe(&self) -> impl IntoElement {
        let shared = self.shared.clone();
        canvas(
            move |bounds, _, _| bounds,
            move |_, bounds, window, _| {
                if shared.content.get() != Some(bounds) {
                    shared.content.set(Some(bounds));
                    // The field painted first, from last frame's content:
                    // draw once more so it settles around this one.
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .inset_0()
    }
}

/// A mark's opacity now: its weight, breathing out of step with the others.
fn alpha(mark: &Mark, phase: f32, glow: f32, peak: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let breath = 1. + BREATH_DEPTH * ((phase / BREATH_S + mark.seed) * tau).sin();
    // A smooth falloff: faint dots fade further, so dense and sparse
    // regions shade into each other.
    mark.weight.powf(1.1) * peak * breath * (1. + 0.14 * glow)
}

/// Behind a chat, a mark's share of its home-screen opacity: a faint rest,
/// lifted by the working wave as it passes (`y` down a field `h` tall).
fn backdrop(quiet: f32, work: f32, phase: f32, y: f32, h: f32) -> f32 {
    let rest = 1. - (1. - CHAT_REST) * quiet;
    let k = std::f32::consts::TAU * WORK_WAVES / h.max(1.);
    let wave = 0.5 + 0.5 * (phase * WORK_SPEED - y * k).sin();
    rest * (1. + WORK_LIFT * work * wave)
}

/// How much a ripple lights a dot `d` px from its centre, `secs` after it
/// began: 0..1, highest on its ring, fading as the ring runs out.
fn ripple_at(d: f32, secs: f32, strength: f32) -> f32 {
    let r = secs * RIPPLE_SPEED;
    let band = (1. - (d - r).abs() / RIPPLE_BAND).max(0.);
    band * strength * (1. - r / RIPPLE_REACH).max(0.)
}

#[allow(clippy::too_many_arguments)]
fn paint_marks(
    bounds: Bounds<Pixels>,
    marks: &[Mark],
    motion: Motion,
    slide: Point<f32>,
    ripples: &[(f32, f32)],
    origin: Option<Point<Pixels>>,
    t: &Theme,
    window: &mut Window,
) {
    let peak = if t.is_dark { 0.75 } else { 0.95 };
    let (base, accent) = (t.extra.dot, t.extra.accent);
    let d = f32::from(t.size.dot);
    // Dots this small are drawn on whole device pixels, square below 3px:
    // a soft round edge on a 2px dot fades it to almost nothing on a real
    // GPU at 100% scaling.
    let scale = window.scale_factor();
    let snap = |v: Pixels| px((f32::from(v) * scale).round() / scale);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for m in marks {
            // Deeper dots lean further: a hint of depth, nothing more.
            let depth = 0.55 + 0.45 * m.seed;
            let at = point(
                bounds.origin.x + px(m.at.x + motion.lean.x * depth + slide.x),
                bounds.origin.y + px(m.at.y + motion.lean.y * depth + slide.y),
            );
            // A ripple brightens the dots on its ring and tints them the accent.
            let lit = origin.map_or(0., |o| {
                let (dx, dy) = (f32::from(at.x - o.x), f32::from(at.y - o.y) * 1.35);
                let d = (dx * dx + dy * dy).sqrt();
                ripples
                    .iter()
                    .map(|(secs, s)| ripple_at(d, *secs, *s))
                    .fold(0., f32::max)
            });
            // Only where the field shows: the quiet zone round the words stays quiet.
            let lit = lit * m.weight.sqrt();
            let back = backdrop(
                motion.quiet,
                motion.work,
                motion.phase,
                m.at.y,
                f32::from(bounds.size.height),
            );
            let a = ((alpha(m, motion.phase, motion.glow, peak) + 0.55 * lit) * back).min(1.);
            let tint = |c: Hsla, k: f32| Hsla {
                a: c.a * a * k,
                ..c
            };
            match m.kind {
                Kind::Dot | Kind::Accent => {
                    let (c, size_k) = if m.kind == Kind::Accent || lit > 0.5 {
                        (tint(accent, 1.), 1.3)
                    } else {
                        (tint(base, 1.), 0.9 + 0.3 * m.weight)
                    };
                    let device = (d * size_k * scale).round().max(2.);
                    let s = px(device / scale);
                    let round = if device >= 3. { s / 2. } else { px(0.) };
                    window.paint_quad(gpui::quad(
                        Bounds::new(point(snap(at.x - s / 2.), snap(at.y - s / 2.)), size(s, s)),
                        Corners::all(round),
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
        let mut motion = self.motion;
        if self.still {
            motion.quiet = self.shared.backdrop.get() as u8 as f32;
        }
        let gap = f32::from(t.size.dot_gap);
        let column = f32::from(t.size.column);
        let (cache, shared) = (self.cache.clone(), self.shared.clone());
        div().absolute().inset_0().child(
            canvas(
                {
                    let shared = shared.clone();
                    move |bounds, _, _| {
                        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                        // The content, in the field's own coordinates.
                        let probed = shared.content.get().filter(|_| !shared.backdrop.get());
                        let content = probed.map_or_else(
                            || {
                                if shared.backdrop.get() {
                                    chat_content(w, h, column)
                                } else {
                                    default_content(w, h)
                                }
                            },
                            |c| {
                                let o = c.origin - bounds.origin;
                                let (x, y) = (f32::from(o.x), f32::from(o.y));
                                [
                                    x,
                                    y,
                                    x + f32::from(c.size.width),
                                    y + f32::from(c.size.height),
                                ]
                            },
                        );
                        // The content moved (the sidebar opened, the window
                        // resized): the dots start where they were and glide.
                        let centre = bounds.origin
                            + point(
                                px((content[0] + content[2]) / 2.),
                                px((content[1] + content[3]) / 2.),
                            );
                        if let Some(old) = shared.anchor.get() {
                            let d = old - centre;
                            if f32::from(d.x).abs() + f32::from(d.y).abs() > 1. {
                                let s = shared.slide.get();
                                shared
                                    .slide
                                    .set(point(s.x + f32::from(d.x), s.y + f32::from(d.y)));
                            }
                        }
                        shared.anchor.set(Some(centre));
                        marks(&cache, bounds, gap, content)
                    }
                },
                move |bounds, marks, window, _| {
                    shared.painted.set(Some((bounds, Instant::now())));
                    let ripples: Vec<(f32, f32)> = shared
                        .ripples
                        .borrow()
                        .iter()
                        .map(|(at, s)| (at.elapsed().as_secs_f32(), *s))
                        .collect();
                    paint_marks(
                        bounds,
                        &marks,
                        motion,
                        shared.slide.get(),
                        &ripples,
                        shared.anchor.get(),
                        &t,
                        window,
                    );
                },
            )
            .size_full(),
        )
    }
}

#[cfg(test)]
mod ripple_tests {
    use super::*;

    #[test]
    fn behind_a_chat_the_field_is_faint_and_the_wave_lifts_it_only_while_working() {
        // The home screen: untouched.
        assert_eq!(backdrop(0., 0., 1.3, 200., 700.), 1.);
        // Behind a chat at rest: a faint trace, the same everywhere and always.
        let rest: Vec<f32> = (0..40)
            .map(|i| backdrop(1., 0., i as f32 * 0.1, i as f32 * 17., 700.))
            .collect();
        assert!(rest.iter().all(|a| (a - CHAT_REST).abs() < 1e-6));
        // Working: the wave lifts it, never below the rest, and moves.
        let at = |phase: f32| backdrop(1., 1., phase, 300., 700.);
        let samples: Vec<f32> = (0..60).map(|i| at(i as f32 * 0.05)).collect();
        let (lo, hi) = samples
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), a| (l.min(*a), h.max(*a)));
        assert!(lo >= CHAT_REST - 1e-6 && hi > CHAT_REST * 2., "{lo}..{hi}");
        // At its crest still well under the home screen's strength.
        assert!(hi < 0.8, "{hi}");
    }

    #[test]
    fn a_ripple_lights_its_ring_and_fades_as_it_runs_out() {
        // Half a second in, the ring is 210px out.
        assert!(ripple_at(210., 0.5, 1.) > 0.6);
        assert_eq!(ripple_at(100., 0.5, 1.), 0.);
        assert!(ripple_at(210., 0.5, 0.4) < ripple_at(210., 0.5, 1.));
        // Past its reach it lights nothing.
        assert_eq!(ripple_at(RIPPLE_REACH, RIPPLE_REACH / RIPPLE_SPEED, 1.), 0.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_content_stays_quiet_and_the_edges_fade() {
        let (w, h) = (1000., 700.);
        let c = default_content(w, h);
        // Behind the heading and the composer, and just outside them: nothing.
        assert_eq!(weight(w * 0.5, h * 0.5, w, h, c), 0.);
        assert_eq!(weight(c[0] - 6., h * 0.5, w, h, c), 0.);
        // At the window's edge: nothing, so the field never ends on a line.
        assert_eq!(weight(0., h * 0.3, w, h, c), 0.);
        assert_eq!(weight(w * 0.8, h, w, h, c), 0.);
        // Beside the heading and past the composer's lower right: the clouds.
        let near = |x0: f32, y0: f32| {
            (-3..=3)
                .flat_map(|i| (-3..=3).map(move |j| (i, j)))
                .map(|(i, j)| weight(x0 + i as f32 * 14., y0 + j as f32 * 14., w, h, c))
                .fold(0f32, f32::max)
        };
        assert!(near(c[0] - 90., c[1] + 40.) > 0.4);
        assert!(near(c[2] + 60., c[3] + 90.) > 0.4);
    }

    #[test]
    fn the_field_follows_the_content() {
        let (w, h) = (1000., 700.);
        // Content moved left (a panel opened on the right): the quiet goes with it.
        let left = [80., 200., 520., 500.];
        assert_eq!(weight(300., 350., w, h, left), 0.);
        // However it is laid out, no dot comes within 28px of the content.
        for content in [left, default_content(w, h), [30., 130., 630., 430.]] {
            for m in layout(w, h, 14., content) {
                let d = rect_distance(m.at.x, m.at.y, content, 24.);
                assert!(d >= 28., "a dot {d:.1}px from the content");
            }
        }
    }

    #[test]
    fn behind_a_chat_the_reading_column_stays_clear() {
        let (w, h) = (1180., 810.);
        let c = chat_content(w, h, 680.);
        let marks = layout(w, h, 14., c);
        // Dots in the margins on both sides, none within 28px of the column.
        assert!(marks.iter().any(|m| m.at.x < c[0]));
        assert!(marks.iter().any(|m| m.at.x > c[2]));
        assert!(marks.iter().all(|m| m.at.x < c[0] - 27. || m.at.x > c[2] + 27.));
    }

    #[test]
    fn the_field_is_stable_and_sparse() {
        let c = default_content(1000., 700.);
        let a = layout(1000., 700., 14., c);
        let b = layout(1000., 700., 14., c);
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
