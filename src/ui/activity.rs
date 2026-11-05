//! What KodAct is doing right now, as the working line under a reply shows
//! it: the strike mark, then the step in words ("Reading src/main.rs",
//! "Running cargo test"), how long, and how much it has written.
//!
//! The mark is KodAct's own: four ember dots standing as a diamond, the ◆ of
//! KodAct's mark. On each beat a blow lands, as a hammer on hot metal: the
//! diamond squashes flat, the struck dots flash white-hot, then it springs
//! back and they cool to ember. The next blow comes from the side, as a
//! smith turns the work between strikes. One mark for every kind of work;
//! the words say which. Reduced motion: the diamond at rest.

use std::time::Duration;

use gpui::{div, prelude::*, px, Animation, AnimationExt, AnyElement, Div, ElementId, Hsla};

/// The kinds of work the working line names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Waiting,
    Thinking,
    Reading,
    Editing,
    WritingCode,
    WritingDoc,
    Running,
    Searching,
    SearchingWeb,
    Fetching,
    Agent,
    Agents,
    Tool,
    Replying,
    AwaitingYou,
    Compacting,
}

/// Two beats: a blow from above, then one from the side.
const LAP: Duration = Duration::from_millis(1800);
/// Within a beat (0..1): at rest, the hammer falling, held on the anvil,
/// springing back.
const BLOW: (f32, f32, f32, f32) = (0.40, 0.52, 0.60, 0.86);
/// The diamond's points, (x, y) in 0..1 of the box: top, right, bottom, left.
const DIAMOND: [(f32, f32); 4] = [(0.5, 0.13), (0.87, 0.5), (0.5, 0.87), (0.13, 0.5)];
/// Struck flat: how much the struck axis gives, and the other spreads.
const SQUASH: f32 = 0.55;
const SPREAD: f32 = 0.18;

/// A quick fall: slow at the top, fast at the anvil.
fn fall(x: f32) -> f32 {
    x * x
}

/// Back up with a small overshoot, as struck metal rings.
fn spring(x: f32) -> f32 {
    let c = 1.7;
    1. + (c + 1.) * (x - 1.).powi(3) + c * (x - 1.).powi(2)
}

/// How hard the blow has landed at `h` (0..1) of a beat: 0 at rest, 1 on
/// the anvil, a little below 0 as it rings back.
fn struck(h: f32) -> f32 {
    let (rest, down, held, back) = BLOW;
    if h < rest {
        0.
    } else if h < down {
        fall((h - rest) / (down - rest))
    } else if h < held {
        1.
    } else if h < back {
        1. - spring((h - held) / (back - held))
    } else {
        0.
    }
}

/// `a` moved `k` (0..1) of the way to `b`.
fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let l = |x: f32, y: f32| x + (y - x) * k;
    gpui::Rgba {
        r: l(a.r, b.r),
        g: l(a.g, b.g),
        b: l(a.b, b.b),
        a: l(a.a, b.a),
    }
    .into()
}

/// The four dots at `k` (0..1) of a lap, in a box `size` px square.
fn dots(ember: Hsla, heat: Hsla, size: f32, k: f32) -> Div {
    let mut d = div().relative().size(px(size)).flex_none();
    let dot = size * 0.22;
    // From above on the first beat, from the side on the second.
    let side = k >= 0.5;
    let f = struck((k * 2.).fract());
    let (sx, sy) = if side {
        (1. - SQUASH * f, 1. + SPREAD * f)
    } else {
        (1. + SPREAD * f, 1. - SQUASH * f)
    };
    for (i, (x, y)) in DIAMOND.iter().enumerate() {
        let (x, y) = (0.5 + (x - 0.5) * sx, 0.5 + (y - 0.5) * sy);
        // The dots the hammer meets glow white-hot, and cool as it lifts.
        let hit = if side { i % 2 == 1 } else { i % 2 == 0 };
        let glow = if hit { f.clamp(0., 1.) * 0.75 } else { 0. };
        d = d.child(
            div()
                .absolute()
                .left(px(x * size - dot / 2.))
                .top(px(y * size - dot / 2.))
                .size(px(dot))
                .rounded(px(dot / 2.))
                .bg(mix(ember, heat, glow)),
        );
    }
    d
}

/// The strike mark, `size` px square: dots in `ember` that flash toward
/// `heat` when struck; `id` names its animation.
pub fn strike(ember: Hsla, heat: Hsla, size: f32, id: impl Into<ElementId>) -> AnyElement {
    if super::motion::reduced_motion() {
        return dots(ember, heat, size, 0.).into_any_element();
    }
    div()
        .size(px(size))
        .flex_none()
        .with_animation(id, Animation::new(LAP).repeat(), move |d, k| {
            d.child(dots(ember, heat, size, k))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{struck, BLOW};

    #[test]
    fn a_beat_rests_then_strikes_and_rings_back() {
        assert_eq!(struck(0.), 0.);
        assert_eq!(struck(BLOW.0 - 0.01), 0.);
        assert_eq!(struck((BLOW.1 + BLOW.2) / 2.), 1.);
        assert!(struck(BLOW.3 + 0.01).abs() < 1e-6);
        // Lifting, it rings past rest a little before it settles.
        assert!((0..100)
            .map(|i| BLOW.2 + (BLOW.3 - BLOW.2) * i as f32 / 100.)
            .any(|h| struck(h) < 0.));
    }
}
