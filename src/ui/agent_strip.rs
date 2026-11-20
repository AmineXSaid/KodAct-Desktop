//! The Agents panel's strip of agent pills: one per agent, with its status
//! icon and name, the current one emphasised. With many agents it scrolls
//! sideways; the current pill is scrolled into view when it changes, and
//! the pills fade into the panel's edge instead of being cut mid-word.

use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, ScrollHandle, SharedString};

use crate::state::{AgentRun, AppState, ToolStatus};
use crate::theme::Theme;

/// The strip. `equal` lays a few agents out in equal widths, no scrolling;
/// `scroll` is the strip's own handle (kept in `AppState`), and `scrolled_to`
/// the pill it last brought into view.
pub fn render(
    runs: &[AgentRun],
    active: usize,
    equal: bool,
    scroll: &ScrollHandle,
    scrolled_to: &std::cell::Cell<Option<usize>>,
    state: &Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    // Bring the current pill into view once, when it changes, so a pick in
    // the jumper or a step with ← → shows its pill; scrolling by hand is
    // left alone otherwise.
    if !equal && scrolled_to.get() != Some(active) {
        scroll.scroll_to_item(active);
        scrolled_to.set(Some(active));
    }
    let hover = t.colors.hover;
    let pills = runs.iter().enumerate().map(|(i, r)| {
        let on = i == active;
        let st = super::agent_roster::status(r);
        let selector = state.clone();
        let id = r.id.clone();
        let inner = div()
            .flex()
            .items_center()
            .gap(t.space.tight)
            .h(t.size.control)
            .px(t.space.snug)
            .rounded(t.radius.compact)
            .border_1()
            .border_color(if on {
                t.extra.border_strong
            } else {
                gpui::transparent_black()
            })
            .when(on, |d| d.bg(t.colors.surface).shadow(t.elevation_grain()))
            .hover(move |s| s.bg(hover))
            .child(super::agents_view::status_icon(
                st,
                t,
                t.size.icon_xs,
                format!("agent-tab-spin-{i}"),
            ))
            .child(
                div()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_size(t.text.caption)
                    .font_weight(if on {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if on {
                        t.colors.text_strong
                    } else {
                        t.colors.text_muted
                    })
                    .child(r.task.clone()),
            );
        let pill = super::button::KeyRow::new(
            ("agent-tab", i),
            SharedString::from(format!(
                "Agent {} of {}: {}, {}",
                i + 1,
                runs.len(),
                r.task,
                match st {
                    ToolStatus::Running => "running",
                    ToolStatus::Failed => "failed",
                    ToolStatus::Declined => "stopped",
                    ToolStatus::Interrupted => "interrupted",
                    ToolStatus::Done => "done",
                }
            )),
            inner,
        )
        .radius(t.radius.compact)
        .expanded(on)
        .on_activate(move |_, cx| selector.update(cx, |s, cx| s.select_agent(&id, cx)));
        if equal {
            div().flex_1().min_w_0().child(pill).into_any_element()
        } else {
            // Whole names when they fit; long ones end in an ellipsis inside
            // their pill, never at the panel's edge.
            div()
                .flex_none()
                .max_w(px(240.))
                .child(pill)
                .into_any_element()
        }
    });
    let strip = div()
        .id("agent-tabs-scroll")
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .when(!equal, |d| d.overflow_x_scroll().track_scroll(scroll))
        .children(pills)
        // Room past the last pill, so it can scroll clear of the fade.
        .when(!equal, |d| d.child(div().flex_none().w(t.size.edge_fade)));
    if equal {
        return strip.into_any_element();
    }
    // The fades: over the strip's ends, in the switcher's own ground.
    let ground = t.extra.sunk;
    let scrolled = f32::from(scroll.offset().x) < -0.5;
    div()
        .relative()
        .w_full()
        .min_w_0()
        .child(strip)
        .when(scrolled, |d| {
            d.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(t.size.edge_fade)
                    .bg(gpui::linear_gradient(
                        90.,
                        gpui::linear_color_stop(ground, 0.),
                        gpui::linear_color_stop(ground.opacity(0.), 1.),
                    )),
            )
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(t.size.edge_fade)
                .bg(gpui::linear_gradient(
                    90.,
                    gpui::linear_color_stop(ground.opacity(0.), 0.),
                    gpui::linear_color_stop(ground, 1.),
                )),
        )
        .into_any_element()
}
