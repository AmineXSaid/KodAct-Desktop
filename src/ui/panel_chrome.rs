//! The chrome every right-side surface shares: the side bar's tabs, the
//! Session column and the file view each hang their title row, tool row and
//! status line from here, so they stand at one height, on one background,
//! in one type (title 14 semibold, toolbar labels 12 medium, body 13).
//!
//! - [`header`]: the title row, `panel_header` (44px) tall.
//! - [`toolbar`]: the second row, `panel_toolbar` (36px) tall, in groups of
//!   compact icon buttons set apart by hairlines. Below a width threshold the
//!   trailing groups collapse into a "..." popover (`collapsed_from`).
//! - [`footer`]: the status line, `panel_footer` (32px) tall.
//! - [`close_button`]: the one close control.

use gpui::{
    deferred, div, prelude::*, px, AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement,
    SharedString, Window,
};

use super::button::Button;
use crate::theme::Theme;

/// A title in the panel's type: 14 semibold.
pub fn title(t: &Theme, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(t.text.body)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.text_strong)
        .child(text.into())
}

/// The title row: `leading` controls, the title (it takes the room that is
/// left), then `trailing` controls, on the panel's own background under a
/// hairline.
pub fn header(
    t: &Theme,
    title: impl IntoElement,
    leading: Vec<AnyElement>,
    trailing: Vec<AnyElement>,
) -> gpui::Div {
    div()
        .relative()
        .flex_none()
        .h(t.size.panel_header)
        .flex()
        .items_center()
        .gap(t.space.xs)
        .pl(t.space.md)
        .pr(t.space.sm)
        .bg(t.extra.panel)
        .border_b_1()
        .border_color(t.colors.border)
        .children(leading)
        .child(div().flex_1().min_w_0().flex().items_center().child(title))
        .children(trailing)
}

/// The one close control: a compact ghost icon button.
pub fn close_button(
    id: impl Into<ElementId>,
    tip: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    Button::icon(id, "icons/x.svg", tip)
        .compact()
        .on_click(on_click)
        .into_any_element()
}

/// A hairline between two groups of tools.
fn divider(t: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .w(t.size.hairline)
        .h(px(16.))
        .mx(t.space.xs)
        .bg(t.colors.border)
}

/// One group of tools and the width it needs: `n` buttons of `tool_btn` and
/// the gaps between them.
pub struct Group {
    pub tools: Vec<AnyElement>,
    /// How wide it draws, in px.
    pub width: f32,
}

#[allow(dead_code)] // the file view's toolbar (next step) is its first user
impl Group {
    /// A group of `n` compact buttons (each `tool_btn` wide).
    pub fn of(t: &Theme, tools: Vec<AnyElement>) -> Self {
        let n = tools.len() as f32;
        let width = n * f32::from(t.size.tool_btn) + (n - 1.).max(0.) * GAP;
        Self { tools, width }
    }
}

const GAP: f32 = 2.;
/// A divider's width with its margins, and the "..." button's width.
const DIVIDER_W: f32 = 9.;

/// How many leading groups fit in `available` px (a divider between each),
/// leaving room for the "..." button when any are left out: the trailing
/// groups collapse first. All of them when they fit.
pub fn collapsed_from(widths: &[f32], more_w: f32, available: f32) -> usize {
    let total: f32 = widths.iter().sum::<f32>() + DIVIDER_W * widths.len().saturating_sub(1) as f32;
    if total <= available {
        return widths.len();
    }
    let mut used = more_w;
    let mut shown = 0;
    for w in widths {
        let next = used + w + if shown > 0 { DIVIDER_W } else { 0. };
        if next > available {
            break;
        }
        used = next;
        shown += 1;
    }
    shown
}

/// The tool row. `width` is the panel's width; groups that do not fit move,
/// last first, into a "..." popover (`more_open` says whether it is open;
/// `on_more` toggles it).
#[allow(dead_code)] // the file view's toolbar (next step) is its first user
pub fn toolbar(
    t: &Theme,
    width: f32,
    groups: Vec<Group>,
    more_open: bool,
    on_more: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Div {
    let more_w = f32::from(t.size.tool_btn);
    let pad = f32::from(t.space.md + t.space.sm);
    let widths: Vec<f32> = groups.iter().map(|g| g.width).collect();
    let shown = collapsed_from(&widths, more_w, width - pad);
    let mut row = div()
        .flex_none()
        .h(t.size.panel_toolbar)
        .flex()
        .items_center()
        .gap(px(GAP))
        .pl(t.space.md)
        .pr(t.space.sm)
        .bg(t.extra.panel)
        .border_b_1()
        .border_color(t.colors.border);
    let mut hidden: Vec<Group> = Vec::new();
    for (i, g) in groups.into_iter().enumerate() {
        if i < shown {
            if i > 0 {
                row = row.child(divider(t));
            }
            row = row.child(div().flex().items_center().gap(px(GAP)).children(g.tools));
        } else {
            hidden.push(g);
        }
    }
    if hidden.is_empty() {
        return row;
    }
    let menu = more_open.then(|| {
        deferred(super::motion::unfold(
            div()
                .absolute()
                .top(t.size.panel_toolbar)
                .right(t.space.sm)
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .p(t.space.sm)
                .bg(t.colors.surface)
                .border_1()
                .border_color(t.colors.border)
                .rounded(t.radius.card)
                .shadow(t.elevation_popover())
                .children(
                    hidden
                        .into_iter()
                        .map(|g| div().flex().items_center().gap(px(GAP)).children(g.tools)),
                ),
            "panel-toolbar-more",
            false,
            false,
            t,
        ))
        .with_priority(2)
    });
    row.child(div().flex_1())
        .child(
            Button::icon("panel-toolbar-more", "icons/ellipsis.svg", "More tools")
                .compact()
                .selected(more_open)
                .on_click(on_more),
        )
        .children(menu)
        .relative()
}

/// The status line: what is on the left (a position, a size) and what is on
/// the right (a zoom, a count), 12px in the muted ink.
#[allow(dead_code)] // the file view's status line (next step) is its first user
pub fn footer(t: &Theme, left: impl IntoElement, right: impl IntoElement) -> gpui::Div {
    div()
        .flex_none()
        .h(t.size.panel_footer)
        .flex()
        .items_center()
        .justify_between()
        .gap(t.space.md)
        .px(t.space.md)
        .bg(t.extra.panel)
        .border_t_1()
        .border_color(t.colors.border)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .child(left)
        .child(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_shows_while_it_fits() {
        assert_eq!(collapsed_from(&[60., 60., 90.], 28., 400.), 3);
    }

    #[test]
    fn the_trailing_groups_collapse_first() {
        // 60 + 9 + 60 + 9 + 90 = 228 does not fit in 200; the "..." button
        // (28) keeps its place, so two groups stay (28 + 60 + 9 + 60 = 157).
        assert_eq!(collapsed_from(&[60., 60., 90.], 28., 200.), 2);
        assert_eq!(collapsed_from(&[60., 60., 90.], 28., 120.), 1);
        assert_eq!(collapsed_from(&[60., 60., 90.], 28., 40.), 0);
    }
}
