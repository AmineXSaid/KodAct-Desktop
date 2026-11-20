//! The opener: a short film as the app opens (`design/opener-mock.html`).
//!
//! KodAct's mark lights in the dark, from a deep turquoise to almost
//! white, and locks with a flash, a shockwave and sparks. Then it settles
//! through the bright signal to its own colour, and leads the wordmark,
//! KodAct (`kodact_mark::wordmark`). The code calls the mark's place
//! `cube`, from the mark this film was first cut for.
//!
//! - [`Cut::First`] (no name saved yet), 8.3 s: the three lines slam in on
//!   the beat ("Describe it. KodAct builds it. You approve."), a viewfinder
//!   frames it, a teal ring runs through a field of dots as the cube cools,
//!   and the line "KodAct asks before it changes anything." Then the film
//!   fades away and the cube rises into the naming screen's hero.
//! - [`Cut::Again`] (every launch after), 2.6 s: heat, lock, cool into the
//!   wordmark, then an iris opens from the cube onto the app.
//!
//! It plays only with `KODACT_OPENER=1`; by default the app opens
//! straight into its window. Any key or click skips it. Reduced motion: no
//! opener at all.
//!
//! Every frame is a pure function of the time since it started (`scene`),
//! on a 1920 x 1080 stage scaled to fit the window, over all of it. It is drawn in the dark
//! theme's colours in either theme, as a film is; the handoff fades it into
//! whatever the window shows. `KODACT_OPENER_AT=<seconds>` holds it still at
//! one moment, for screenshots.

use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, point, prelude::*, px, size, App, Bounds, Context, EventEmitter, FocusHandle,
    Font, FontWeight, Hsla, Pixels, Point, Render, SharedString, Task, TextAlign, TextRun, Window,
};

use super::{kodact_mark, motion};
use crate::theme::Theme;

/// Which film plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    /// The first launch: ends on the naming screen.
    First,
    /// Every launch after: ends in the app.
    Again,
}

/// What the workspace does as the film goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenerEvent {
    /// The film is handing over: the first cut's naming screen opens under it.
    Handoff,
    /// The film is over (or skipped): take it away.
    Done,
}

/// The stage the film is drawn on, before scaling.
const STAGE_W: f32 = 1920.;
const STAGE_H: f32 = 1080.;
/// The beat grid: 128 BPM, four beats to the bar.
const BEAT: f32 = 60. / 128.;
const BAR: f32 = BEAT * 4.;
/// The first cut opens 0.6 s before the lock, which falls on bar 2.
const SHIFT: f32 = BAR - 0.6;
/// The ground and the cube's places on the stage.
const GROUND_Y: f32 = 760.;
const VANISH: (f32, f32) = (STAGE_W / 2., 470.);
const LOCK_AT: Place = Place {
    x: 960.,
    y: 540.,
    s: 300.,
};
const LOW_AT: Place = Place {
    x: 960.,
    y: 690.,
    s: 250.,
};
/// The wordmark: its letters' size and baseline.
const WORD_SIZE: f32 = 170.;
const WORD_BASE: f32 = 585.;
/// The naming screen's hero cube (`onboarding.rs`): its box and the cube in it.
const HERO_BOX: f32 = 132.;
const HERO_CUBE: f32 = 88.;
/// The naming screen's steps, laid out in a box this tall.
const STEPS_H: f32 = 480.;

/// A place for the cube: its centre and its size, on the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    x: f32,
    y: f32,
    s: f32,
}

fn lerp(a: Place, b: Place, k: f32) -> Place {
    Place {
        x: a.x + (b.x - a.x) * k,
        y: a.y + (b.y - a.y) * k,
        s: a.s + (b.s - a.s) * k,
    }
}

/// When things happen in one cut, in seconds from the start.
#[derive(Clone, Copy, Debug)]
struct Timing {
    end: f32,
    lock: f32,
    cooled: f32,
    low: Option<f32>,
    lockup: f32,
    tag: Option<f32>,
    words: &'static [(f32, f32, &'static str)],
    hud: bool,
    /// The first cut: when the naming screen takes over.
    name: Option<f32>,
    /// The second: when the iris opens onto the app.
    iris: Option<f32>,
}

/// A beat on the first cut's grid, `bar.beat`, as seconds from its start.
const fn on(bar: f32, beat: f32) -> f32 {
    (bar - 1.) * BAR + (beat - 1.) * BEAT - SHIFT
}

const WORDS: [(f32, f32, &str); 3] = [
    (on(2., 3.), on(3., 1.), "DESCRIBE IT."),
    (on(3., 1.), on(3., 3.), "KODACT BUILDS IT."),
    (on(3., 3.), on(4., 2.), "YOU APPROVE."),
];

impl Cut {
    fn timing(self) -> Timing {
        match self {
            Cut::First => Timing {
                end: 8.3,
                lock: on(2., 1.),
                cooled: on(4., 1.),
                low: Some(on(2., 3.) - 0.25),
                lockup: on(4., 3.),
                tag: Some(on(4., 4.)),
                words: &WORDS,
                hud: true,
                name: Some(8.0 - SHIFT),
                iris: None,
            },
            Cut::Again => Timing {
                end: 2.6,
                lock: 0.45,
                cooled: 1.35,
                low: None,
                lockup: 1.15,
                tag: None,
                words: &[],
                hud: false,
                name: None,
                iris: Some(2.0),
            },
        }
    }

    /// When the workspace takes over: the naming screen opens under the
    /// fading film, or the film is done.
    fn handoff(self) -> f32 {
        let m = self.timing();
        m.name.unwrap_or(m.end)
    }
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0., 1.)
}
fn smooth(x: f32) -> f32 {
    let x = clamp01(x);
    x * x * (3. - 2. * x)
}
fn out_cubic(x: f32) -> f32 {
    1. - (1. - clamp01(x)).powi(3)
}
fn in_out(x: f32) -> f32 {
    let x = clamp01(x);
    if x < 0.5 {
        4. * x * x * x
    } else {
        1. - (-2. * x + 2.).powi(3) / 2.
    }
}

/// A small seeded generator (mulberry32), so the sparks fly the same way each time.
fn rng(seed: u32) -> impl FnMut() -> f32 {
    let mut s = seed;
    move || {
        s = s.wrapping_add(0x6D2B_79F5);
        let mut x = (s ^ (s >> 15)).wrapping_mul(1 | s);
        x = x.wrapping_add((x ^ (x >> 7)).wrapping_mul(61 | x)) ^ x;
        (x ^ (x >> 14)) as f32 / 4_294_967_296.
    }
}

/// A spark thrown off by the lock: its velocity, life and size.
#[derive(Clone, Copy)]
struct Spark {
    vx: f32,
    vy: f32,
    life: f32,
    size: f32,
}

/// An ember rising off the cooling cube.
#[derive(Clone, Copy)]
struct Ember {
    x: f32,
    vx: f32,
    vy: f32,
    life: f32,
    size: f32,
}

fn sparks() -> Vec<Spark> {
    let mut r = rng(11);
    (0..70)
        .map(|_| {
            let a = -std::f32::consts::FRAC_PI_2 + (r() - 0.5) * 2.6;
            let v = 380. + r() * 820.;
            Spark {
                vx: a.cos() * v,
                vy: a.sin() * v,
                life: 0.5 + r() * 0.7,
                size: 2. + r() * 3.,
            }
        })
        .collect()
}

fn embers() -> Vec<Ember> {
    let mut r = rng(23);
    (0..90)
        .map(|_| Ember {
            x: (r() - 0.5) * 220.,
            vx: (r() - 0.5) * 60.,
            vy: -(120. + r() * 160.),
            life: 0.9 + r() * 0.8,
            size: 1.5 + r() * 2.5,
        })
        .collect()
}

/// The film's colours: the dark theme's, in either theme.
#[derive(Clone, Copy)]
struct Palette {
    dark: Hsla,
    paper: Hsla,
    muted: Hsla,
    subtle: Hsla,
    teal: Hsla,
    mark: Hsla,
    /// The heat ramp: iron, ember, glow, white.
    ramp: [Hsla; 4],
}

impl Palette {
    fn new(dark: &Theme) -> Self {
        let x = &dark.extra;
        Self {
            dark: x.chrome,
            paper: dark.colors.text_strong,
            muted: dark.colors.text_muted,
            subtle: dark.colors.text_subtle,
            teal: x.mark_top,
            mark: x.mark_top,
            ramp: [
                x.mark_heat_iron,
                x.mark_heat_ember,
                x.mark_heat,
                x.mark_heat_white,
            ],
        }
    }

    /// Metal at heat `h`: iron at 0.12, ember at 1/3, the glow at 2/3, white at 1.
    fn heat(&self, h: f32) -> Hsla {
        let h = h.clamp(0.12, 1.);
        let [iron, ember, glow, white] = self.ramp;
        let stops = [(0.12, iron), (1. / 3., ember), (2. / 3., glow), (1., white)];
        let i = stops
            .iter()
            .position(|(at, _)| h <= *at)
            .unwrap_or(3)
            .max(1);
        let ((a, ca), (b, cb)) = (stops[i - 1], stops[i]);
        mix(ca, cb, (h - a) / (b - a))
    }
}

fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    let (a, b, k) = (a.to_rgb(), b.to_rgb(), clamp01(k));
    gpui::Rgba {
        r: a.r + (b.r - a.r) * k,
        g: a.g + (b.g - a.g) * k,
        b: a.b + (b.b - a.b) * k,
        a: a.a + (b.a - a.a) * k,
    }
    .into()
}

/// How the stage maps onto the film's box: scaled to fit, centred.
#[derive(Clone, Copy, Debug)]
struct Fit {
    s: f32,
    ox: f32,
    oy: f32,
}

impl Fit {
    fn new(w: f32, h: f32) -> Self {
        let s = (w / STAGE_W).min(h / STAGE_H);
        Self {
            s,
            ox: (w - STAGE_W * s) / 2.,
            oy: (h - STAGE_H * s) / 2.,
        }
    }
    fn x(&self, x: f32) -> f32 {
        self.ox + x * self.s
    }
    fn y(&self, y: f32) -> f32 {
        self.oy + y * self.s
    }
    /// A stage point as a point in a painted box at `origin`.
    fn at(&self, origin: Point<Pixels>, x: f32, y: f32) -> Point<Pixels> {
        origin + point(px(self.x(x)), px(self.y(y)))
    }
}

/// The wordmark's layout: where the mark, "Kod" and "Act" sit on the stage.
#[derive(Clone, Copy, Debug)]
struct Word {
    x0: f32,
    wk: f32,
    o: f32,
    gap: f32,
    cube: Place,
}

impl Word {
    fn new(wk: f32, wa: f32) -> Self {
        // The mark leads the name, as the title bar's wordmark sets it,
        // centred on the capitals.
        let (o, gap) = (
            kodact_mark::mark_box(WORD_SIZE),
            WORD_SIZE * kodact_mark::WORD_GAP,
        );
        let x0 = STAGE_W / 2. - (o + gap + wk + wa) / 2.;
        Self {
            x0,
            wk,
            o,
            gap,
            cube: Place {
                x: x0 + o / 2.,
                y: WORD_BASE - WORD_SIZE * kodact_mark::WORD_CAP / 2.,
                s: o,
            },
        }
    }
}

/// One moment of the film: everything `paint` and the cube need.
#[derive(Clone, Copy, Debug)]
struct Scene {
    t: f32,
    /// The cube's heat (1 white hot) and where it is.
    heat: f32,
    cube: Place,
    /// 0..1 before the lock: how far the mark has lit in the dark.
    kindle: f32,
    forged: bool,
    shake: (f32, f32),
    /// 0..1: how far the film has faded into the naming screen, or the
    /// iris opened onto the app.
    away: f32,
    iris: f32,
}

fn scene(cut: Cut, t: f32, word: Word, hero: Place) -> Scene {
    let m = cut.timing();
    let t = t.clamp(0., m.end);
    let forged = t >= m.lock;
    let heat = if forged {
        1. - smooth((t - m.lock) / (m.cooled - m.lock))
    } else {
        1.
    };
    let mut cube = LOCK_AT;
    if let Some(low) = m.low {
        if t > low {
            cube = lerp(LOCK_AT, LOW_AT, in_out((t - low) / 0.45));
        }
    }
    if t > m.lockup {
        cube = lerp(cube, word.cube, in_out((t - m.lockup) / 0.42));
    }
    let mut away = 0.;
    if let Some(name) = m.name {
        if t > name {
            cube = lerp(cube, hero, in_out((t - name - 0.15) / 0.7));
            away = smooth((t - name) / 0.9);
        }
    }
    let iris = m.iris.map_or(0., |at| in_out((t - at) / 0.6));
    let mut shake = (0., 0.);
    for at in std::iter::once(m.lock).chain(m.words.iter().map(|w| w.0)) {
        let a = t - at;
        if (0. ..0.18).contains(&a) {
            let k = (1. - a / 0.18) * if at == m.lock { 14. } else { 8. };
            shake.0 += (a * 140.).sin() * k;
            shake.1 += (a * 117.).cos() * k;
        }
    }
    Scene {
        t,
        heat,
        cube,
        kindle: smooth(t / m.lock),
        forged,
        shake,
        away,
        iris,
    }
}

pub struct Opener {
    cut: Cut,
    start: Instant,
    /// `KODACT_OPENER_AT`: hold the film at this moment.
    held: Option<f32>,
    focus: FocusHandle,
    palette: Palette,
    sparks: Vec<Spark>,
    embers: Vec<Ember>,
    handed_off: bool,
    done: bool,
    _clock: Task<()>,
}

impl EventEmitter<OpenerEvent> for Opener {}

impl Opener {
    /// Whether an opener plays at all: not under reduced motion.
    /// Off unless asked for (`KODACT_OPENER=1`): the app opens straight
    /// into its window, and the welcome's own rise is the one moment of
    /// motion at start-up. Reduced motion: never.
    pub fn plays() -> bool {
        std::env::var("KODACT_OPENER").is_ok_and(|v| v == "1") && !motion::reduced_motion()
    }

    pub fn new(cut: Cut, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let held = std::env::var("KODACT_OPENER_AT")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // The clock: hand over, then finish, unless held for a screenshot.
        let clock = cx.spawn(async move |this, cx| {
            if held.is_some() {
                return;
            }
            let handoff = cut.handoff();
            let end = cut.timing().end;
            cx.background_executor()
                .timer(Duration::from_secs_f32(handoff))
                .await;
            let _ = this.update(cx, |o, cx| o.hand_off(cx));
            cx.background_executor()
                .timer(Duration::from_secs_f32((end - handoff).max(0.)))
                .await;
            let _ = this.update(cx, |o, cx| o.finish(cx));
        });
        Self {
            cut,
            start: Instant::now(),
            held,
            focus,
            // The film is the chosen theme at night, in either mode.
            palette: Palette::new(&Theme::of(gpui_theme(cx).look, true)),
            sparks: sparks(),
            embers: embers(),
            handed_off: false,
            done: false,
            _clock: clock,
        }
    }

    fn hand_off(&mut self, cx: &mut Context<Self>) {
        if !self.handed_off {
            self.handed_off = true;
            cx.emit(OpenerEvent::Handoff);
        }
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        self.hand_off(cx);
        if !self.done {
            self.done = true;
            cx.emit(OpenerEvent::Done);
        }
    }

    /// Take the keyboard back (something else opened as the window did).
    pub fn take_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    /// Any key or click: straight to where the film was going.
    fn skip(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
    }

    fn now(&self) -> f32 {
        self.held
            .unwrap_or_else(|| self.start.elapsed().as_secs_f32())
    }
}

/// A shaped line of text in `font` at `size` px, in `color`.
fn shape(text: &str, font: &Font, size: f32, color: Hsla, window: &Window) -> gpui::ShapedLine {
    window.text_system().shape_line(
        SharedString::from(text.to_string()),
        px(size),
        &[TextRun {
            len: text.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    )
}

/// Paint a line with its baseline at `base` (a y in the painted box), its
/// left edge at `x`.
fn paint_text(line: &gpui::ShapedLine, x: Pixels, base: Pixels, window: &mut Window, cx: &mut App) {
    let lh = line.ascent + line.descent;
    let _ = line.paint(
        point(x, base - line.ascent),
        lh,
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

/// A soft glow: an ellipse `w` x `h` (stage px) at a stage point, only its
/// shadow, blurred out by `blur`, so it has no edge.
fn glow(
    f: Fit,
    (x, y): (f32, f32),
    (w, h): (f32, f32),
    color: Hsla,
    alpha: f32,
    blur: f32,
) -> gpui::Div {
    let (w, h, b) = (w * f.s, h * f.s, blur * f.s);
    div()
        .absolute()
        .left(px(f.x(x) - w / 2.))
        .top(px(f.y(y) - h / 2.))
        .w(px(w))
        .h(px(h))
        .rounded_full()
        .shadow(vec![gpui::BoxShadow {
            color: color.opacity(alpha),
            offset: point(px(0.), px(0.)),
            blur_radius: px(b),
            spread_radius: px(b * 0.15),
            inset: false,
        }])
}

/// Everything painted rather than laid out: the ground, the dot ring, the
/// shockwave, the sparks and embers, the words, the wordmark's letters and
/// the viewfinder. `top` paints the layer in front of the cube.
#[allow(clippy::too_many_arguments)]
fn paint_layer(
    bounds: Bounds<Pixels>,
    cut: Cut,
    sc: Scene,
    word: Word,
    pal: Palette,
    fonts: &Fonts,
    sparks: &[Spark],
    embers: &[Ember],
    top: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let m = cut.timing();
    let f = Fit::new(f32::from(bounds.size.width), f32::from(bounds.size.height));
    let o = bounds.origin;
    let (sx, sy) = sc.shake;
    let at = |x: f32, y: f32| f.at(o, x + sx, y + sy);
    let t = sc.t;
    let quad = |window: &mut Window, p: Point<Pixels>, r: f32, c: Hsla| {
        let r = px(r);
        window.paint_quad(gpui::fill(
            Bounds::new(p - point(r, r), size(r * 2., r * 2.)),
            c,
        ));
    };
    if !top {
        // The ground: a floor in perspective, fading in toward the viewer.
        let ground = (1. - smooth((t - m.lockup) / 0.5)) * (1. - sc.away);
        if ground > 0. {
            let line = |window: &mut Window, a: Point<Pixels>, b: Point<Pixels>, alpha: f32| {
                let mut p = gpui::PathBuilder::stroke(px(1.5 * f.s.max(0.6)));
                p.move_to(a);
                p.line_to(b);
                if let Ok(path) = p.build() {
                    window.paint_path(path, pal.subtle.opacity(alpha * ground));
                }
            };
            let fade =
                |y: f32| clamp01((y - (GROUND_Y - 120.)) / (STAGE_H - GROUND_Y + 120.)) * 0.16;
            for i in -14..=14 {
                let x = STAGE_W / 2. + i as f32 * 150.;
                // Each line in eight pieces, so it fades in as it nears.
                let (x0, y0) = (VANISH.0 + (x - VANISH.0) * 0.35, GROUND_Y - 60.);
                let (x1, y1) = (VANISH.0 + (x - VANISH.0) * 2.4, STAGE_H);
                for k in 0..8 {
                    let (a, b) = (k as f32 / 8., (k + 1) as f32 / 8.);
                    let (ya, yb) = (y0 + (y1 - y0) * a, y0 + (y1 - y0) * b);
                    line(
                        window,
                        at(x0 + (x1 - x0) * a, ya),
                        at(x0 + (x1 - x0) * b, yb),
                        fade((ya + yb) / 2.),
                    );
                }
            }
            for j in 0..9 {
                let y = GROUND_Y - 60. + (j as f32 / 8.).powf(1.8) * (STAGE_H - GROUND_Y + 60.);
                let (l, r) = (
                    o + point(px(0.), px(f.y(y + sy))),
                    o + point(bounds.size.width, px(f.y(y + sy))),
                );
                line(window, l, r, fade(y));
            }
        }
        // The teal ring through a field of dots as the cube cools (first cut).
        let ring_age = t - m.cooled;
        if cut == Cut::First && ring_age > -0.01 && sc.away < 1. {
            let reach = ring_age * 900.;
            let step = 42.;
            let mut y = 30.;
            while y < STAGE_H {
                let mut x = 30.;
                while x < STAGE_W {
                    let d = (x - sc.cube.x).hypot((y - sc.cube.y) * 1.3);
                    let band = (1. - (d - reach).abs() / 70.).max(0.);
                    let a = (0.07 * smooth(ring_age / 0.6) * (1. - d / 1200.).max(0.)
                        + band * 0.9 * (1. - reach / 1700.).max(0.))
                        * (1. - sc.away);
                    if a > 0.01 {
                        let c = if band > 0.45 { pal.teal } else { pal.muted };
                        quad(window, at(x, y), 2. * f.s.max(0.5), c.opacity(a));
                    }
                    x += step;
                }
                y += step;
            }
        }
        // The lock: a shockwave of heat running out, sparks, then embers rising.
        if sc.forged {
            let a = (t - m.lock) / 0.7;
            if a < 1. {
                let r = 60. + 820. * out_cubic(a);
                let mut p = gpui::PathBuilder::stroke(px((10. * (1. - a) + 1.) * f.s));
                for i in 0..=96 {
                    let th = i as f32 / 96. * TAU;
                    let q = at(LOCK_AT.x + r * th.cos(), LOCK_AT.y + r * 0.62 * th.sin());
                    if i == 0 {
                        p.move_to(q)
                    } else {
                        p.line_to(q)
                    }
                }
                if let Ok(path) = p.build() {
                    let c = if a < 0.3 { pal.ramp[3] } else { pal.ramp[2] };
                    window.paint_path(path, c.opacity(0.7 * (1. - a)));
                }
            }
            let since = t - m.lock;
            for s in sparks {
                if since > s.life {
                    continue;
                }
                let k = 1. - since / s.life;
                let (x, y) = (
                    LOCK_AT.x + s.vx * since,
                    LOCK_AT.y + s.vy * since + 900. * since * since,
                );
                quad(
                    window,
                    at(x, y),
                    s.size * f.s / 2.,
                    pal.heat(0.35 + 0.6 * k).opacity(k),
                );
            }
            if t <= m.lockup {
                for (i, e) in embers.iter().enumerate() {
                    let born = m.lock + i as f32 * (m.cooled - m.lock) / 90.;
                    let a = t - born;
                    if a < 0. || a > e.life {
                        continue;
                    }
                    let hh = 1. - smooth((born - m.lock) / (m.cooled - m.lock));
                    let k = 1. - a / e.life;
                    let (x, y) = (
                        sc.cube.x + e.x + e.vx * a,
                        sc.cube.y - sc.cube.s * 0.3 + e.vy * a,
                    );
                    quad(
                        window,
                        at(x, y),
                        e.size * f.s / 2.,
                        pal.heat(0.3 + 0.6 * hh).opacity(k * hh),
                    );
                }
            }
        }
        return;
    }
    // The three lines, slammed on the beat (first cut).
    for &(from, until, text) in m.words {
        if t < from || t >= until + 0.12 {
            continue;
        }
        let a = t - from;
        let e = out_cubic(a / 0.22);
        let out = if t > until {
            1. - (t - until) / 0.12
        } else {
            1.
        };
        let scale = 1. + 0.6 * (1. - e);
        let alpha = clamp01(a / 0.06) * out;
        let fs = 132. * scale * f.s;
        let line = shape(text, &fonts.words, fs, pal.paper.opacity(alpha), window);
        let w = f32::from(line.width);
        let base = px(f.y(300. + sy)) + (line.ascent - line.descent) / 2.;
        let approve = text == "YOU APPROVE.";
        // The check after "You approve." draws itself half a beat in.
        let (cs, gap) = (104. * scale * f.s, 40. * scale * f.s);
        let total = if approve { w + gap + cs } else { w };
        let x0 = f.x(STAGE_W / 2. + sx) - total / 2.;
        paint_text(&line, o.x + px(x0), o.y + base, window, cx);
        if approve {
            let ck = smooth((a - BEAT / 2.) / 0.2);
            if ck > 0. {
                let cx0 = x0 + w + gap;
                let cy = f32::from(base) - f32::from(line.ascent - line.descent) / 2.;
                let p0 = (cx0, cy);
                let p1 = (cx0 + cs * 0.35, cy + cs * 0.32);
                let p2 = (cx0 + cs, cy - cs * 0.42);
                let mut p = gpui::PathBuilder::stroke(px(16. * scale * f.s));
                p.move_to(o + point(px(p0.0), px(p0.1)));
                if ck < 0.45 {
                    let k = ck / 0.45;
                    p.line_to(
                        o + point(px(p0.0 + (p1.0 - p0.0) * k), px(p0.1 + (p1.1 - p0.1) * k)),
                    );
                } else {
                    let k = (ck - 0.45) / 0.55;
                    p.line_to(o + point(px(p1.0), px(p1.1)));
                    p.line_to(
                        o + point(px(p1.0 + (p2.0 - p1.0) * k), px(p1.1 + (p2.1 - p1.1) * k)),
                    );
                }
                if let Ok(path) = p.build() {
                    window.paint_path(path, pal.teal.opacity(out));
                }
            }
        }
    }
    // The wordmark: "Kod" and "Act" slide out from behind the mark; in the
    // first cut they move off again for the naming screen.
    if t >= m.lockup && sc.iris < 0.3 {
        let k = out_cubic((t - m.lockup - 0.18) / 0.5)
            * (1. - smooth(sc.away * 2.5))
            * (1. - sc.iris / 0.3);
        let slide = 60. * (1. - (k + sc.away).min(1.));
        let part = 160. * sc.away;
        let base = o.y + px(f.y(WORD_BASE));
        let kod = shape(
            "Kod",
            &fonts.word,
            WORD_SIZE * f.s,
            pal.paper.opacity(k),
            window,
        );
        let kx = word.x0 + word.o + word.gap - slide + part;
        paint_text(&kod, o.x + px(f.x(kx)), base, window, cx);
        let act = shape(
            "Act",
            &fonts.word,
            WORD_SIZE * f.s,
            pal.teal.opacity(k),
            window,
        );
        paint_text(&act, o.x + px(f.x(kx + word.wk)), base, window, cx);
        if let Some(tag) = m.tag {
            let tg = smooth((t - tag) / 0.4) * (1. - smooth(sc.away * 2.5));
            if tg > 0. {
                let line = shape(
                    "KodAct asks before it changes anything.",
                    &fonts.sans,
                    34. * f.s,
                    pal.muted.opacity(tg),
                    window,
                );
                let x = f.x(STAGE_W / 2.) - f32::from(line.width) / 2.;
                paint_text(
                    &line,
                    o.x + px(x),
                    o.y + px(f.y(WORD_BASE + 92. + 10. * (1. - tg))),
                    window,
                    cx,
                );
            }
        }
    }
    // The viewfinder (first cut): corners, the chapter, the heat, the timecode.
    if m.hud {
        let hud = clamp01(t / 0.15) * (1. - smooth((t - m.lockup) / 0.3));
        if hud > 0. {
            let (mg, len) = (56., 44.);
            for (x, y, dx, dy) in [
                (mg, mg, 1., 1.),
                (STAGE_W - mg, mg, -1., 1.),
                (mg, STAGE_H - mg, 1., -1.),
                (STAGE_W - mg, STAGE_H - mg, -1., -1.),
            ] {
                let mut p = gpui::PathBuilder::stroke(px(2. * f.s.max(0.5)));
                p.move_to(f.at(o, x, y + dy * len));
                p.line_to(f.at(o, x, y));
                p.line_to(f.at(o, x + dx * len, y));
                if let Ok(path) = p.build() {
                    window.paint_path(path, pal.paper.opacity(0.32 * hud));
                }
            }
            let (w1, w3) = (m.words[0].0, m.words[2].0);
            let chapter = if t < w1 {
                "01 — HEAT"
            } else if t < w3 {
                "02 — SHAPE"
            } else {
                "03 — APPROVE"
            };
            let fs = 22. * f.s;
            let ink = pal.paper.opacity(0.55 * hud);
            let mid = |y: f32, l: &gpui::ShapedLine| o.y + px(f.y(y)) + (l.ascent - l.descent) / 2.;
            let l = shape(chapter, &fonts.mono, fs, ink, window);
            paint_text(&l, o.x + px(f.x(mg + 30.)), mid(mg + 30., &l), window, cx);
            let l = shape("KODACT", &fonts.mono, fs, ink, window);
            paint_text(
                &l,
                o.x + px(f.x(STAGE_W - mg - 30.)) - l.width,
                mid(mg + 30., &l),
                window,
                cx,
            );
            let fr = (t * 24.) as u32;
            let tc = format!("00:00:{:02}:{:02}", fr / 24, fr % 24);
            let l = shape(&tc, &fonts.mono, fs, ink, window);
            paint_text(
                &l,
                o.x + px(f.x(STAGE_W - mg - 30.)) - l.width,
                mid(STAGE_H - mg - 30., &l),
                window,
                cx,
            );
            let hot = if sc.forged {
                sc.heat
            } else {
                0.14 + 0.8 * sc.kindle
            };
            let dot = if sc.heat > 0.05 {
                pal.ramp[2]
            } else {
                pal.teal
            };
            quad(
                window,
                f.at(o, mg + 38., STAGE_H - mg - 30.),
                6. * f.s,
                dot.opacity(0.8 * hud),
            );
            let label = if sc.heat > 0.05 {
                format!("{}°C", (400. + 1100. * hot).round() as u32)
            } else {
                "COOL".to_string()
            };
            let l = shape(&label, &fonts.mono, fs, ink, window);
            paint_text(
                &l,
                o.x + px(f.x(mg + 56.)),
                mid(STAGE_H - mg - 30., &l),
                window,
                cx,
            );
        }
    }
}

/// The film's faces: the words, the wordmark, the line under it, the viewfinder.
#[derive(Clone)]
struct Fonts {
    words: Font,
    /// The wordmark's: the interface face in semibold, as `kodact_mark::wordmark`.
    word: Font,
    sans: Font,
    mono: Font,
}

impl Render for Opener {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = gpui_theme(cx);
        let cut = self.cut;
        let m = cut.timing();
        let now = self.now();
        if self.held.is_none() && !self.done {
            window.request_animation_frame();
        }
        let fonts = Fonts {
            words: Font {
                weight: FontWeight::BOLD,
                ..gpui::font(t.font_sans.clone())
            },
            word: Font {
                weight: FontWeight::SEMIBOLD,
                ..gpui::font(t.font_sans.clone())
            },
            sans: gpui::font(t.font_sans.clone()),
            mono: gpui::font(t.font_mono.clone()),
        };
        // The film's box: the whole window, title bar and all, as a film
        // plays; the naming screen sits below the title bar.
        let vp = window.viewport_size();
        let (bw, bh) = (f32::from(vp.width), f32::from(vp.height).max(1.));
        let bar = f32::from(t.size.titlebar);
        let f = Fit::new(bw, bh);
        let probe =
            |s: &str| f32::from(shape(s, &fonts.word, WORD_SIZE, self.palette.paper, window).width);
        let word = Word::new(probe("Kod"), probe("Act"));
        // The naming screen's hero cube, as `onboarding.rs` lays it out, on the stage.
        let steps_top = bar
            + f32::from(t.space.xl)
            + ((bh - bar - f32::from(t.space.xl) - f32::from(t.space.xxl) * 2.) - STEPS_H).max(0.)
                / 2.;
        let hero = Place {
            x: (bw / 2. - f.ox) / f.s,
            y: (steps_top + HERO_BOX / 2. - 6. - f.oy) / f.s,
            s: HERO_CUBE / f.s,
        };
        let sc = scene(cut, now, word, hero);
        let pal = self.palette;
        let (sx, sy) = sc.shake;

        // The light the cube gives off: in the dark as it heats, then its
        // heat, then teal as it cools; and the pool it casts on the ground.
        let ground = (1. - smooth((sc.t - m.lockup) / 0.5)) * (1. - sc.away);
        let (light, light_c) = if !sc.forged {
            (sc.kindle * sc.kindle, pal.heat(0.3 + 0.6 * sc.kindle))
        } else if sc.heat > 0.05 {
            (0.9 * sc.heat + 0.1, pal.heat(0.4 + 0.6 * sc.heat))
        } else {
            (0.5, pal.teal)
        };
        let pool = glow(
            f,
            (960. + sx, GROUND_Y + 40. + sy),
            (760., 180.),
            light_c,
            0.32 * light * ground,
            160.,
        );
        let radiance = if !sc.forged {
            Some(glow(
                f,
                (LOCK_AT.x + sx, LOCK_AT.y + sy),
                (260. + 160. * sc.kindle, 260. + 160. * sc.kindle),
                pal.ramp[2],
                0.4 * sc.kindle * sc.kindle,
                140.,
            ))
        } else if sc.heat > 0.02 {
            let s = sc.cube.s;
            Some(glow(
                f,
                (sc.cube.x + sx, sc.cube.y + sy),
                (s * 0.9, s * 0.9),
                pal.ramp[2],
                0.45 * sc.heat,
                s * 0.6,
            ))
        } else {
            None
        };
        // The flash as it locks, white.
        let flash = sc
            .forged
            .then(|| (sc.t - m.lock) / 0.35)
            .filter(|a| *a < 1.)
            .map(|a| {
                glow(
                    f,
                    (LOCK_AT.x, LOCK_AT.y),
                    (320., 320.),
                    pal.ramp[3],
                    0.8 * (1. - a),
                    260.,
                )
            });
        // The cube: heating in the dark, then cooling; gone as the iris opens.
        let cube_alpha = if sc.forged {
            1.
        } else {
            smooth(sc.t / (m.lock * 0.35))
        } * (1. - smooth(sc.iris / 0.25));
        let hot = if sc.forged {
            sc.heat
        } else {
            0.14 + 0.8 * sc.kindle
        };
        let cs = sc.cube.s * f.s;
        let cube = div()
            .absolute()
            .left(px(f.x(sc.cube.x + sx) - cs / 2.))
            .top(px(f.y(sc.cube.y + sy) - cs / 2.))
            .opacity(cube_alpha)
            .child(kodact_mark::landing(cs, 1., hot).color(pal.mark));

        let (sparks, embers) = (self.sparks.clone(), self.embers.clone());
        let back_fonts = fonts.clone();
        let back = canvas(
            |_, _, _| {},
            move |b, _, window, cx| {
                paint_layer(
                    b,
                    cut,
                    sc,
                    word,
                    pal,
                    &back_fonts,
                    &sparks,
                    &embers,
                    false,
                    window,
                    cx,
                )
            },
        )
        .absolute()
        .size_full();
        let front = canvas(
            |_, _, _| {},
            move |b, _, window, cx| {
                paint_layer(b, cut, sc, word, pal, &fonts, &[], &[], true, window, cx)
            },
        )
        .absolute()
        .size_full();

        // The dark behind it all; for the second cut's exit, an iris: a
        // ring of the dark so wide it covers the window, opening from the cube.
        let backdrop = if sc.iris > 0. {
            let r = 30. + sc.iris * (bw.hypot(bh) + 60.);
            let edge = bw.hypot(bh) * 2.;
            let (cx0, cy0) = (f.x(word.cube.x), f.y(word.cube.y));
            let mut ring = div()
                .absolute()
                .left(px(cx0 - r - edge))
                .top(px(cy0 - r - edge))
                .size(px((r + edge) * 2.))
                .rounded_full()
                .border_color(pal.dark);
            let w = Some(gpui::AbsoluteLength::from(px(edge)));
            ring.style().border_widths = gpui::EdgesRefinement {
                top: w,
                right: w,
                bottom: w,
                left: w,
            };
            ring
        } else {
            div().absolute().inset_0().bg(pal.dark)
        };

        div()
            .id("opener")
            .track_focus(&self.focus)
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .top_0()
            .overflow_hidden()
            .occlude()
            .opacity(1. - sc.away)
            .on_key_down(cx.listener(|this, _: &gpui::KeyDownEvent, _, cx| this.skip(cx)))
            .on_any_mouse_down(cx.listener(|this, _, _, cx| this.skip(cx)))
            .child(backdrop)
            .when(sc.iris < 1., |d| {
                d.child(back)
                    .child(pool)
                    .children(radiance)
                    .child(cube)
                    .children(flash)
                    .child(front)
            })
    }
}

fn gpui_theme(cx: &App) -> Theme {
    use crate::theme::ActiveTheme;
    cx.theme().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word() -> Word {
        Word::new(60., 260.)
    }
    const HERO: Place = Place {
        x: 960.,
        y: 330.,
        s: 132.,
    };

    #[test]
    fn the_first_cut_runs_on_the_beat() {
        let m = Cut::First.timing();
        assert!((m.lock - 0.6).abs() < 1e-4, "{}", m.lock);
        // Every line lands a whole number of beats after the lock.
        for w in m.words {
            let beats = (w.0 - m.lock) / BEAT;
            assert!((beats - beats.round()).abs() < 1e-3, "{beats}");
        }
        assert!(m.name.unwrap() < m.end && m.lockup < m.name.unwrap());
        assert!(m.cooled < m.lockup);
    }

    #[test]
    fn it_heats_in_the_dark_then_locks_white_and_cools() {
        let early = scene(Cut::First, 0.05, word(), HERO);
        assert!(!early.forged && early.kindle < 0.05);
        let lock = scene(Cut::First, 0.61, word(), HERO);
        assert!(lock.forged && lock.heat > 0.99);
        let cool = scene(Cut::First, Cut::First.timing().cooled + 0.01, word(), HERO);
        assert!(cool.heat < 1e-3);
    }

    #[test]
    fn the_cube_ends_in_the_wordmark_then_the_hero() {
        let w = word();
        let m = Cut::First.timing();
        let at_word = scene(Cut::First, m.lockup + 0.5, w, HERO).cube;
        assert!((at_word.x - w.cube.x).abs() < 0.5 && (at_word.s - w.cube.s).abs() < 0.5);
        let at_hero = scene(Cut::First, m.end, w, HERO).cube;
        assert!((at_hero.y - HERO.y).abs() < 0.5 && (at_hero.s - HERO.s).abs() < 0.5);
        assert!(scene(Cut::First, m.end, w, HERO).away > 0.99);
    }

    #[test]
    fn the_second_cut_is_short_and_opens_onto_the_app() {
        let m = Cut::Again.timing();
        assert!(m.end < 3. && m.words.is_empty() && !m.hud && m.name.is_none());
        assert!(scene(Cut::Again, m.end, word(), HERO).iris > 0.99);
        assert_eq!(Cut::Again.handoff(), m.end);
    }

    #[test]
    fn it_fits_the_stage_to_any_window() {
        // Wider than 16:9: the height decides, and the stage is centred across.
        let wide = Fit::new(2000., 600.);
        assert!((STAGE_H * wide.s - 600.).abs() < 1e-3);
        assert!(wide.oy.abs() < 1e-3 && (wide.ox * 2. + STAGE_W * wide.s - 2000.).abs() < 1e-3);
        // Taller: the width decides, and it is centred down.
        let tall = Fit::new(800., 1200.);
        assert!((STAGE_W * tall.s - 800.).abs() < 1e-3);
        assert!(tall.ox.abs() < 1e-3 && (tall.oy * 2. + STAGE_H * tall.s - 1200.).abs() < 1e-3);
        // Either way the whole stage is inside the window.
        for f in [wide, tall, Fit::new(1280., 860.)] {
            assert!(f.x(0.) >= -1e-3 && f.y(0.) >= -1e-3);
        }
    }

    #[test]
    fn it_shakes_only_on_the_lock_and_the_lines() {
        let m = Cut::First.timing();
        let at = |t: f32| scene(Cut::First, t, word(), HERO).shake;
        let moved = |(x, y): (f32, f32)| x.abs() + y.abs() > 0.5;
        assert!(moved(at(m.lock + 0.02)));
        assert!(moved(at(WORDS[1].0 + 0.02)));
        // Still before the lock and between the hits.
        assert_eq!(at(m.lock - 0.1), (0., 0.));
        assert_eq!(at(m.lock + 0.3), (0., 0.));
        assert_eq!(at(m.cooled), (0., 0.));
        // The lock hits harder than a line.
        let size = |(x, y): (f32, f32)| x.hypot(y);
        assert!(size(at(m.lock + 0.005)) > size(at(WORDS[0].0 + 0.005)));
    }

    #[test]
    fn the_generator_matches_the_mock() {
        // The first draws of the mock's mulberry32 (design/opener-mock.html),
        // so the sparks and embers fly exactly as they do there.
        let mut r = rng(11);
        for want in [0.511_587, 0.529_946_4, 0.608_118_6] {
            assert!((r() - want).abs() < 1e-6);
        }
        assert!((rng(23)() - 0.092_722_67).abs() < 1e-6);
    }

    #[test]
    fn sparks_fly_the_same_way_every_time() {
        let (a, b) = (sparks(), sparks());
        assert_eq!(a.len(), 70);
        assert!(a.iter().zip(&b).all(|(x, y)| x.vx == y.vx && x.vy == y.vy));
        // Upward, mostly.
        assert!(a.iter().filter(|s| s.vy < 0.).count() > 50);
    }

    #[test]
    fn heat_runs_from_iron_to_white() {
        let p = Palette::new(&Theme::dark());
        let close = |a: Hsla, b: Hsla| {
            let (a, b) = (a.to_rgb(), b.to_rgb());
            (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() < 0.02
        };
        assert!(close(p.heat(1.), p.ramp[3]));
        assert!(close(p.heat(0.), p.ramp[0]));
        assert!(close(p.heat(2. / 3.), p.ramp[2]));
    }
}
