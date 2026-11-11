//! The heat rail: Forge's signature down the left of a run's opened steps.
//! Each step is a node on one hairline, in the status language the app
//! shares (the working line, the endpoint badges): ember while it works,
//! ink once done, danger when it failed, warning when it was stopped.
//! While the run is live the line itself glows ember, as metal still hot
//! from the strike; when the run ends it cools to a hairline.

use gpui::{div, prelude::*, px, Animation, AnimationExt, AnyElement, ElementId, Hsla};

use crate::state::ToolStatus;
use crate::theme::Theme;

/// A step's state on the rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    Working,
    Done,
    Failed,
    Stopped,
}

impl Node {
    pub fn of(status: ToolStatus) -> Node {
        match status {
            ToolStatus::Running => Node::Working,
            ToolStatus::Failed | ToolStatus::Declined => Node::Failed,
            ToolStatus::Interrupted => Node::Stopped,
            _ => Node::Done,
        }
    }

    /// The shared status colour.
    pub fn color(self, t: &Theme) -> Hsla {
        match self {
            Node::Working => t.extra.live,
            Node::Done => t.colors.text_muted,
            Node::Failed => t.extra.danger,
            Node::Stopped => t.extra.warning,
        }
    }
}

/// The rail's gutter for one step: its node, and the line through it (from
/// the node up when it isn't the first, down when it isn't the last).
pub fn gutter(
    node: Node,
    first: bool,
    last: bool,
    hot: bool,
    id: impl Into<ElementId>,
    t: &Theme,
) -> AnyElement {
    const WIDE: f32 = 14.;
    const DOT: f32 = 8.;
    // The node sits on the step's first line, centred on its quiet line.
    let centre = 3. + f32::from(t.size.quiet_line) / 2.;
    let line = if hot {
        t.extra.live.opacity(0.55)
    } else {
        t.colors.border
    };
    let x = (WIDE - 1.) / 2.;
    let stroke = || div().absolute().left(px(x)).w(px(1.)).bg(line);
    let color = node.color(t);
    let dot = div()
        .absolute()
        .top(px(centre - DOT / 2.))
        .left(px((WIDE - DOT) / 2.))
        .size(px(DOT))
        .rounded_full()
        .bg(color)
        // A ring of the surface around it, so the line seems to pass behind.
        .border_1()
        .border_color(t.colors.surface);
    let dot: AnyElement = if node == Node::Working && !super::motion::reduced_motion() {
        dot.with_animation(
            id,
            Animation::new(std::time::Duration::from_millis(1200))
                .repeat()
                .with_easing(gpui::ease_in_out),
            move |d, k| {
                // Glows and settles, as an ember breathing.
                let glow = 0.55 + 0.45 * (1. - (2. * k - 1.).abs());
                d.bg(color.opacity(glow))
            },
        )
        .into_any_element()
    } else {
        dot.into_any_element()
    };
    div()
        .relative()
        .flex_none()
        .w(px(WIDE))
        .self_stretch()
        .when(!first, |d| d.child(stroke().top_0().h(px(centre))))
        .when(!last, |d| d.child(stroke().top(px(centre)).bottom_0()))
        .child(dot)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::Node;
    use crate::state::ToolStatus;

    #[test]
    fn each_status_has_its_place_on_the_rail() {
        assert_eq!(Node::of(ToolStatus::Running), Node::Working);
        assert_eq!(Node::of(ToolStatus::Done), Node::Done);
        assert_eq!(Node::of(ToolStatus::Failed), Node::Failed);
        assert_eq!(Node::of(ToolStatus::Interrupted), Node::Stopped);
    }
}
