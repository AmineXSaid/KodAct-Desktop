//! What Forge is doing right now, as the working line under a reply shows
//! it: a small animated mark for the kind of work, then the step in words
//! ("Reading src/main.rs", "Running cargo test"). One mark moves per reply;
//! the reply's own header mark stays still.
//!
//! Each mark is a Lucide icon in the muted ink with one moving part in the
//! accent, so it reads at a glance and never competes with the words:
//!
//! | Activity | Mark |
//! |---|---|
//! | Waiting for the model | the cube turning |
//! | Thinking | a brain, sparks lighting round it |
//! | Reading a file | a page, a highlight sweeping down its lines |
//! | Editing a file | a pencil drawing a short line |
//! | Writing code | brackets, a caret blinking between them |
//! | Writing a document or the plan | the hand with the quill (`writing_hand.rs`) |
//! | Running a command | a prompt, its cursor blinking |
//! | Searching the project | a magnifier circling |
//! | Searching the web | a globe turning |
//! | Fetching a page | an arrow dropping into a tray |
//! | One agent | a bot nodding as it works |
//! | Several agents | three cubes lighting in turn |
//! | Another tool (MCP) | a plug sliding into its socket |
//! | Writing the reply | a bubble with typing dots |
//! | Waiting for you | a hand raised, gently waving |
//! | Compacting | layers pressing together |
//!
//! Reduced motion: each mark at rest.

use std::f32::consts::TAU;
use std::time::Duration;

use gpui::{
    div, point, prelude::*, px, radians, size, svg, Animation, AnimationExt, AnyElement, Div,
    ElementId, Hsla, Transformation,
};

use crate::theme::Theme;

/// The mark's box.
const BOX: f32 = 18.;

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

impl Activity {
    /// One lap of its motion.
    fn lap_ms(self) -> u64 {
        match self {
            Activity::Running | Activity::WritingCode => 1000,
            Activity::Replying | Activity::Agents => 1200,
            Activity::Searching | Activity::SearchingWeb => 1800,
            Activity::Fetching | Activity::Tool => 1400,
            _ => 1600,
        }
    }
}

/// An icon `size_px` square, in `color`.
fn glyph(path: &'static str, size_px: f32, color: Hsla) -> gpui::Svg {
    svg()
        .path(path)
        .size(px(size_px))
        .flex_none()
        .text_color(color)
}

/// A piece placed at `(x, y)` inside the box.
fn at(x: f32, y: f32, child: impl IntoElement) -> Div {
    div().absolute().left(px(x)).top(px(y)).child(child)
}

/// A dot of `d` px.
fn dot(d: f32, color: Hsla) -> Div {
    div().size(px(d)).rounded(px(d / 2.)).bg(color)
}

/// 0 → 1 → 0 over the lap, smoothly.
fn swell(k: f32) -> f32 {
    0.5 - 0.5 * (k * TAU).cos()
}

/// The mark for `activity`, `id` naming its animation.
pub fn mark(activity: Activity, t: &Theme, id: impl Into<ElementId>) -> AnyElement {
    if activity == Activity::WritingDoc {
        return super::writing_hand::writing(t, id);
    }
    if activity == Activity::Waiting {
        return super::turning_logo(BOX, id);
    }
    let base = div().relative().size(px(BOX)).flex_none();
    let theme = t.clone();
    if super::motion::reduced_motion() {
        return base.child(frame(activity, &theme, 0.)).into_any_element();
    }
    base.with_animation(
        id,
        Animation::new(Duration::from_millis(activity.lap_ms())).repeat(),
        move |d, k| d.child(frame(activity, &theme, k)),
    )
    .into_any_element()
}

/// The mark at `k` of its lap (0..1).
fn frame(activity: Activity, t: &Theme, k: f32) -> Div {
    let (ink, accent) = (t.colors.text_muted, t.colors.primary);
    let layer = div().absolute().size(px(BOX));
    match activity {
        Activity::Thinking => {
            // Three sparks round the brain, each lighting in its third.
            let spots = [(13.5, 0.5), (15., 9.), (0.5, 4.)];
            let mut l = layer.child(at(2., 2., glyph("icons/brain.svg", 14., ink)));
            for (i, (x, y)) in spots.into_iter().enumerate() {
                let phase = (k * 3. - i as f32).clamp(0., 1.);
                l = l.child(at(x, y, dot(3., accent).opacity(swell(phase))));
            }
            l
        }
        Activity::Reading => {
            // A highlight sweeping down the page's lines.
            let y = 7. + 7. * swell(k * 0.5);
            layer
                .child(at(1., 1., glyph("icons/file-text.svg", 16., ink)))
                .child(at(
                    5.5,
                    y,
                    div()
                        .w(px(7.))
                        .h(px(2.))
                        .rounded(px(1.))
                        .bg(accent.opacity(0.7)),
                ))
        }
        Activity::Editing => {
            // The pencil drawing a short line, then lifting back.
            let p = swell(k);
            let x = 1. + 8. * p;
            layer
                .child(at(
                    1.,
                    15.,
                    div().w(px(x)).h(px(1.5)).rounded(px(1.)).bg(accent),
                ))
                .child(at(
                    x - 1.,
                    1. - 1.5 * (k * TAU * 2.).sin().abs(),
                    glyph("icons/pencil.svg", 13., ink),
                ))
        }
        Activity::WritingCode => {
            let on = k < 0.55;
            layer
                .child(at(1., 1., glyph("icons/code-xml.svg", 16., ink)))
                .child(at(
                    8.25,
                    5.,
                    div()
                        .w(px(1.5))
                        .h(px(8.))
                        .rounded(px(1.))
                        .bg(accent)
                        .opacity(if on { 1. } else { 0. }),
                ))
        }
        Activity::Running => {
            let on = k < 0.55;
            layer
                .child(at(1., 1., glyph("icons/terminal.svg", 16., ink)))
                .child(at(
                    10.,
                    12.,
                    div()
                        .w(px(5.))
                        .h(px(2.5))
                        .rounded(px(0.5))
                        .bg(accent)
                        .opacity(if on { 1. } else { 0.15 }),
                ))
        }
        Activity::Searching => {
            // The magnifier circling, as an eye scans a page.
            let a = k * TAU;
            layer.child(at(
                2.5 + 2.5 * a.cos(),
                2.5 + 2.5 * a.sin(),
                glyph("icons/search.svg", 13., accent),
            ))
        }
        Activity::SearchingWeb => {
            // A meridian narrowing and widening: the globe turns.
            let w = 2. + 12. * (k * TAU / 2.).cos().abs();
            layer
                .child(at(1., 1., glyph("icons/globe.svg", 16., ink)))
                .child(at(
                    9. - w / 2.,
                    2.,
                    div()
                        .w(px(w))
                        .h(px(14.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(accent),
                ))
        }
        Activity::Fetching => {
            // An arrow dropping into a tray, fading only as it lands.
            let p = (k * 1.2).min(1.);
            let fade = if p > 0.7 { 1. - (p - 0.7) / 0.3 } else { 1. };
            layer
                .child(at(
                    2.,
                    15.,
                    div().w(px(14.)).h(px(1.5)).rounded(px(1.)).bg(ink),
                ))
                .child(
                    at(4., 1. + 6. * p, glyph("icons/arrow-down.svg", 10., accent)).opacity(fade),
                )
        }
        Activity::Agent => {
            let bob = -1.5 * (k * TAU).sin();
            layer
                .child(at(1., 1. + bob, glyph("icons/bot.svg", 16., ink)))
                .child(at(8., 0.5 + bob, dot(2.5, accent).opacity(swell(k * 2.))))
        }
        Activity::Agents => {
            // Three cubes lighting in turn: agents working side by side.
            let mut l = layer;
            for i in 0..3 {
                let lit = ((k * 3.).floor() as usize) == i;
                l = l.child(at(
                    i as f32 * 6.,
                    6.,
                    div().size(px(5.)).rounded(px(1.)).bg(if lit {
                        accent
                    } else {
                        ink.opacity(0.35)
                    }),
                ));
            }
            l
        }
        Activity::Tool => {
            // The plug sliding into its socket, a spark as it meets.
            let p = swell(k);
            layer
                .child(at(
                    14.,
                    4.,
                    div().w(px(2.)).h(px(10.)).rounded(px(1.)).bg(ink),
                ))
                .child(at(
                    -2. + 3. * p,
                    2.,
                    glyph("icons/plug.svg", 14., ink)
                        .with_transformation(Transformation::rotate(radians(TAU / 4.))),
                ))
                .child(at(
                    12.,
                    7.5,
                    dot(2.5, accent).opacity(if p > 0.85 { 1. } else { 0. }),
                ))
        }
        Activity::Replying => {
            // Typing dots in a bubble, each rising in turn.
            let mut l = layer.child(at(1., 1., glyph("icons/message-square.svg", 16., ink)));
            for i in 0..3 {
                let phase = (k * 2. - i as f32 * 0.25).rem_euclid(2.).min(1.);
                l = l.child(at(
                    5. + i as f32 * 3.,
                    6.5 - 1.5 * swell(phase),
                    dot(2., accent),
                ));
            }
            l
        }
        Activity::AwaitingYou => {
            // A hand raised, waving gently: your turn.
            let a = 0.22 * (k * TAU).sin();
            layer.child(at(
                2.,
                2.,
                glyph("icons/hand.svg", 14., accent)
                    .with_transformation(Transformation::rotate(radians(a))),
            ))
        }
        Activity::Compacting => {
            // Layers pressing together and easing apart.
            let s = 1. - 0.35 * swell(k);
            layer
                .child(at(
                    1.,
                    1.,
                    glyph("icons/layers.svg", 16., ink).with_transformation(
                        Transformation::scale(size(1., s)).with_translation(point(px(0.), px(0.))),
                    ),
                ))
                .child(at(7.5, 8., dot(3., accent).opacity(swell(k))))
        }
        // Drawn by their own elements above.
        Activity::Waiting | Activity::WritingDoc => layer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swell_starts_and_ends_at_rest() {
        assert!(swell(0.).abs() < 1e-6);
        assert!((swell(0.5) - 1.).abs() < 1e-6);
        assert!(swell(1.).abs() < 1e-5);
    }
}
