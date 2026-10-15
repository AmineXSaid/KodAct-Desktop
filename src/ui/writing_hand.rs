//! Forge writing: a hand holding a quill moves along a line, and the ink
//! follows the nib in a cursive wave. Shown on the working line while Forge
//! writes a plan or a document, where the turning cube shows the rest of the
//! time. Reduced motion: the hand at rest at the start of the line.

use std::time::Duration;

use gpui::{div, prelude::*, px, svg, Animation, AnimationExt, AnyElement, ElementId};

use crate::theme::Theme;

/// The drawing's box, the line's box, and where it sits under the hand.
const SIZE: f32 = 24.;
const LINE_W: f32 = 48.;
const LINE_H: f32 = 12.;
const LINE_TOP: f32 = 12.;
/// The nib, in the hand drawing (a 32 grid drawn at SIZE).
const NIB: (f32, f32) = (4. * SIZE / 32., 28.5 * SIZE / 32.);
/// The wave `ink-line.svg` is drawn on: y = MID + AMP·sin(2πx / PERIOD).
const MID: f32 = 6.;
const AMP: f32 = 3.5;
const PERIOD: f32 = 7.;
/// Where the line starts and ends in its box.
const FROM: f32 = 1.;
const TO: f32 = 47.;
/// Writing across, then a beat before the next line.
const WRITE_MS: f32 = 2400.;
const REST_MS: f32 = 500.;

/// The wave's height at `x` in the line's box.
fn wave(x: f32) -> f32 {
    MID + AMP * (2. * std::f32::consts::PI * x / PERIOD).sin()
}

fn hand(t: &Theme) -> gpui::Div {
    div()
        .relative()
        .size(px(SIZE))
        .child(
            svg()
                .absolute()
                .size(px(SIZE))
                .path("icons/writing-hand.svg")
                .text_color(t.colors.text_muted),
        )
        .child(
            svg()
                .absolute()
                .size(px(SIZE))
                .path("icons/writing-quill.svg")
                .text_color(t.colors.primary),
        )
}

/// The writing hand, `id` naming its animation.
pub fn writing(t: &Theme, id: impl Into<ElementId>) -> AnyElement {
    let width = LINE_W + SIZE - NIB.0;
    let base = div().relative().w(px(width)).h(px(SIZE)).flex_none();
    if super::motion::reduced_motion() {
        return base
            .child(
                div()
                    .absolute()
                    .left(px(FROM))
                    .top(px(LINE_TOP + wave(FROM) - NIB.1))
                    .child(hand(t)),
            )
            .into_any_element();
    }
    let theme = t.clone();
    let lap = Duration::from_millis((WRITE_MS + REST_MS) as u64);
    base.with_animation(id, Animation::new(lap).repeat(), move |d, k| {
        let ms = k * (WRITE_MS + REST_MS);
        let p = (ms / WRITE_MS).min(1.);
        // The line fades out in the rest, ready to be written again.
        let fade = if ms > WRITE_MS {
            1. - (ms - WRITE_MS) / REST_MS
        } else {
            1.
        };
        let x = FROM + (TO - FROM) * p;
        // A hand writing lifts a little between letters.
        let lift = if ms > WRITE_MS {
            -2. * (1. - fade)
        } else {
            -0.6 * (2. * std::f32::consts::PI * x / PERIOD).cos().max(0.)
        };
        d.child(
            // The ink so far: the line, uncovered up to the nib.
            div()
                .absolute()
                .left(px(NIB.0))
                .top(px(LINE_TOP))
                .w(px(x))
                .h(px(LINE_H))
                .overflow_hidden()
                .opacity(fade)
                .child(
                    svg()
                        .w(px(LINE_W))
                        .h(px(LINE_H))
                        .path("icons/ink-line.svg")
                        .text_color(theme.colors.primary),
                ),
        )
        .child(
            div()
                .absolute()
                .left(px(x))
                .top(px(LINE_TOP + wave(x) - NIB.1 + lift))
                .child(hand(&theme)),
        )
    })
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wave the nib follows is the one the line is drawn on, so the
    /// nib stays on the ink.
    #[test]
    fn the_nib_follows_the_drawn_line() {
        let drawn = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/ink-line.svg"
        ))
        .unwrap();
        let d = drawn
            .split("d=\"M")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        for pair in d.split(" L") {
            let mut it = pair.split(' ').map(|v| v.parse::<f32>().unwrap());
            let (x, y) = (it.next().unwrap(), it.next().unwrap());
            assert!((wave(x) - y).abs() < 0.01, "at {x}: {y} vs {}", wave(x));
        }
    }
}
