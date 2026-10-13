//! A chat's running mark, as Forge for VS Code draws it: in the same 8px box
//! as a status dot, a 2x2 grid of 3px voxels where one lit cell steps round
//! clockwise in beats (pixel movement, not a glow) with a fading trail of
//! the cells it just left. Forge's cube, at dot size. Reduced motion: a
//! still 2x2 voxel.

use std::time::Duration;

use gpui::{div, prelude::*, px, Animation, AnimationExt, AnyElement, Div, ElementId, Hsla};

/// One lap, top-left → top-right → bottom-right → bottom-left.
const LAP: Duration = Duration::from_millis(880);
const CELLS: [(f32, f32); 4] = [(0., 0.), (5., 0.), (5., 5.), (0., 5.)];
/// Lead, trail, tail, ghost: how lit a cell is, by beats since the lead left it.
const FADE: [f32; 4] = [1., 0.55, 0.25, 0.12];

fn grid(color: Hsla, lead: usize) -> Div {
    let mut d = div().relative().size(px(8.)).flex_none();
    for (i, (x, y)) in CELLS.iter().enumerate() {
        let alpha = if lead == usize::MAX {
            if i == 0 {
                1.
            } else {
                FADE[1]
            }
        } else {
            FADE[(lead + 4 - i) % 4]
        };
        d = d.child(
            div()
                .absolute()
                .left(px(*x))
                .top(px(*y))
                .size(px(3.))
                .rounded(px(0.5))
                .bg(color.opacity(alpha)),
        );
    }
    d
}

/// The running mark in `color` (the busy status colour).
pub fn running(color: Hsla, id: impl Into<ElementId>) -> AnyElement {
    if super::motion::reduced_motion() {
        return grid(color, usize::MAX).into_any_element();
    }
    div()
        .size(px(8.))
        .flex_none()
        .with_animation(id, Animation::new(LAP).repeat(), move |d, delta| {
            d.child(grid(color, ((delta * 4.) as usize).min(3)))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::FADE;

    #[test]
    fn the_lead_cell_is_brightest_and_its_trail_fades() {
        assert!(FADE.windows(2).all(|w| w[0] > w[1]));
    }
}
