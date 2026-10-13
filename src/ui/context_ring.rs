//! The context indicator: a small ring under the composer that fills
//! clockwise from the top as the conversation uses the model's window.

use gpui::{canvas, point, px, Bounds, Hsla, IntoElement, PathBuilder, Pixels, Styled, Window};

/// A ring `size` across, `fraction` (0..=1) of it drawn in `fill` over a
/// full `track`.
pub fn ring(
    size: Pixels,
    stroke: Pixels,
    fraction: f32,
    track: Hsla,
    fill: Hsla,
) -> impl IntoElement {
    let f = fraction.clamp(0., 1.);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint(bounds, stroke, f, track, fill, window),
    )
    .size(size)
    .flex_none()
}

fn paint(
    bounds: Bounds<Pixels>,
    stroke: Pixels,
    f: f32,
    track: Hsla,
    fill: Hsla,
    window: &mut Window,
) {
    let c = bounds.center();
    let r = (bounds.size.width.min(bounds.size.height) - stroke) / 2.;
    let at = |turn: f32| {
        let a = turn * std::f32::consts::TAU;
        point(c.x + r * a.sin(), c.y - r * a.cos())
    };
    let radii = point(r, r);
    // A whole circle is two half arcs.
    let mut circle = PathBuilder::stroke(stroke);
    circle.move_to(at(0.));
    circle.arc_to(radii, px(0.), false, true, at(0.5));
    circle.arc_to(radii, px(0.), false, true, at(0.));
    if let Ok(p) = circle.build() {
        window.paint_path(p, track);
    }
    if f <= 0. {
        return;
    }
    let mut arc = PathBuilder::stroke(stroke);
    arc.move_to(at(0.));
    if f >= 0.999 {
        arc.arc_to(radii, px(0.), false, true, at(0.5));
        arc.arc_to(radii, px(0.), false, true, at(0.));
    } else {
        // At least a dot's worth, so 1% still shows.
        arc.arc_to(radii, px(0.), f > 0.5, true, at(f.max(0.02)));
    }
    if let Ok(p) = arc.build() {
        window.paint_path(p, fill);
    }
}
