//! The opener: KodAct's mark as the window opens, under a second.
//!
//! The tile rises in and plays one beat (the cursor bar, then the run
//! triangle: code, then act) while the name in KodAct CLI's pixel letters
//! slides out beside it; then the stage fades onto what the window shows.
//! [`Cut::First`] (no name saved yet) hands over to the naming screen as it
//! fades; [`Cut::Again`] fades onto the app. Drawn on the theme's own
//! ground, so it never flashes the other mode. Any key or click skips it.
//! Reduced motion: no opener at all. `KODACT_OPENER_AT=<seconds>` holds it
//! at one moment, for screenshots.

use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, px, Context, EventEmitter, FocusHandle, IntoElement, Render, Task, Window,
};

use super::{mark, motion};
use crate::theme::ActiveTheme;

/// Which opener plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cut {
    /// The first launch: ends on the naming screen.
    First,
    /// Every launch after: ends in the app.
    Again,
}

/// What the workspace does as the opener goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenerEvent {
    /// The opener is handing over: the first cut's naming screen opens under it.
    Handoff,
    /// The opener is over (or skipped): take it away.
    Done,
}

/// When things happen, in seconds from the start: the mark has risen in by
/// `LAND`; its beat runs from `BEAT_FROM` for `BEAT_FOR`; the name slides
/// out from `NAME_FROM` over `NAME_FOR`; the stage fades from `FADE_FROM`
/// and is gone at `END`.
const LAND: f32 = 0.16;
const BEAT_FROM: f32 = 0.06;
const BEAT_FOR: f32 = 0.6;
const NAME_FROM: f32 = 0.18;
const NAME_FOR: f32 = 0.28;
const FADE_FROM: f32 = 0.66;
const END: f32 = 0.9;
/// The pixel wordmark: px per pixel (whole pixels keep its edges crisp).
const K: f32 = 6.;
/// How far the mark rises, and the name slides, as they arrive.
const RISE: f32 = 10.;
const SLIDE: f32 = 28.;

impl Cut {
    /// When the naming screen (first run) or the app takes over under the
    /// fading stage.
    fn handoff(self) -> f32 {
        match self {
            Cut::First | Cut::Again => FADE_FROM,
        }
    }
}

/// One moment of the opener: everything the render needs.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Scene {
    /// The mark: how far it has risen in (0..1), and where in its beat.
    land: f32,
    beat: f32,
    /// The name: how far it has slid out (0..1).
    name: f32,
    /// The whole stage: 1 opaque, 0 gone.
    stage: f32,
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0., 1.)
}

/// Ease out, exponential: quick to arrive, soft to settle.
fn ease_out(x: f32) -> f32 {
    let x = clamp01(x);
    if x >= 1. {
        1.
    } else {
        1. - 2f32.powf(-10. * x)
    }
}

/// The opener at `t` seconds.
fn scene(t: f32) -> Scene {
    Scene {
        land: ease_out(t / LAND),
        beat: clamp01((t - BEAT_FROM) / BEAT_FOR),
        name: ease_out((t - NAME_FROM) / NAME_FOR),
        stage: 1. - clamp01((t - FADE_FROM) / (END - FADE_FROM)),
    }
}

pub struct Opener {
    cut: Cut,
    start: Instant,
    /// `KODACT_OPENER_AT`: hold the opener at this moment.
    held: Option<f32>,
    focus: FocusHandle,
    handed_off: bool,
    done: bool,
    _clock: Task<()>,
}

impl EventEmitter<OpenerEvent> for Opener {}

impl Opener {
    /// Whether an opener plays at all: not under reduced motion.
    pub fn plays() -> bool {
        !motion::reduced_motion()
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
            cx.background_executor()
                .timer(Duration::from_secs_f32(handoff))
                .await;
            let _ = this.update(cx, |o, cx| o.hand_off(cx));
            cx.background_executor()
                .timer(Duration::from_secs_f32((END - handoff).max(0.)))
                .await;
            let _ = this.update(cx, |o, cx| o.finish(cx));
        });
        Self {
            cut,
            start: Instant::now(),
            held,
            focus,
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

    /// Any key or click: straight to where the opener was going.
    fn skip(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
    }

    fn now(&self) -> f32 {
        self.held
            .unwrap_or_else(|| self.start.elapsed().as_secs_f32())
    }
}

impl Render for Opener {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.held.is_none() && !self.done {
            window.request_animation_frame();
        }
        let sc = scene(self.now());
        div()
            .id(match self.cut {
                Cut::First => "opener-first",
                Cut::Again => "opener",
            })
            .track_focus(&self.focus)
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .top_0()
            .overflow_hidden()
            .occlude()
            .bg(t.extra.stage)
            .opacity(sc.stage)
            .on_key_down(cx.listener(|this, _: &gpui::KeyDownEvent, _, cx| this.skip(cx)))
            .on_any_mouse_down(cx.listener(|this, _, _, cx| this.skip(cx)))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3. * K))
                    .child(
                        div()
                            .relative()
                            .top(px(RISE * (1. - sc.land)))
                            .opacity(sc.land)
                            .child(mark::tile_at_beat(mark::lockup_tile(K), sc.beat)),
                    )
                    .child(
                        div()
                            .relative()
                            .left(px(-SLIDE * (1. - sc.name)))
                            .opacity(sc.name)
                            .child(mark::pixel_wordmark(K, mark::word_colors(&t))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_over_in_under_a_second() {
        const { assert!(END < 1.) };
        assert_eq!(scene(END).stage, 0.);
        assert_eq!(scene(0.).stage, 1.);
        assert!(Cut::First.handoff() < END && Cut::Again.handoff() < END);
    }

    #[test]
    fn the_mark_arrives_plays_its_beat_and_the_name_follows() {
        let start = scene(0.);
        assert!(start.land < 0.01 && start.name < 0.01);
        // Landed before the name has gone far, the beat under way.
        let early = scene(LAND);
        assert!(early.land > 0.99 && early.beat > 0. && early.name < 0.5);
        // The beat is done and the name out before the stage fades.
        let held = scene(FADE_FROM);
        assert!(held.beat >= 0.99 && held.name > 0.99 && held.stage > 0.99);
    }
}
