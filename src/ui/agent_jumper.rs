//! The Agents panel's jumper: a "1/10 ▾" button between ← and →, with a dot
//! when an agent needs your attention, that opens a list of every agent.
//! The list filters by All (N), Running (N) and Failed (N); each row shows
//! the agent's status icon, name, kind, steps and time, the current one
//! highlighted. Picking a row selects that agent and closes the list;
//! Opening it puts the keyboard on the current row: ↑ ↓ move between
//! rows, Enter or Space picks. Escape closes it too (the workspace's Esc, `ui::AGENT_JUMPER`), as does
//! a click outside. Ctrl/⌘ Shift J opens it from anywhere.

use std::time::Instant;

use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, SharedString};

use super::agent_roster::{self, Counts, Filter};
use super::{icon, AGENT_JUMPER};
use crate::state::{AgentRun, AppState};
use crate::theme::Theme;

const FILTER_RUNNING: &str = "agent-filter:running";
const FILTER_FAILED: &str = "agent-filter:failed";

/// The filter the jumper shows, kept with the panel's other toggles.
pub fn filter(ui_toggles: &std::collections::HashSet<String>) -> Filter {
    if ui_toggles.contains(FILTER_RUNNING) {
        Filter::Running
    } else if ui_toggles.contains(FILTER_FAILED) {
        Filter::Failed
    } else {
        Filter::All
    }
}

/// The "1/10 ▾" button.
pub fn button(
    active: usize,
    runs: &[AgentRun],
    open: bool,
    state: &Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let counts = Counts::of(runs);
    let hover = t.colors.hover;
    let toggle = state.clone();
    div()
        .id("agent-jumper")
        .role(gpui::accesskit::Role::Button)
        .aria_label(SharedString::from(format!(
            "Agent {} of {}{}, {} the list (Ctrl Shift J)",
            active + 1,
            runs.len(),
            if counts.needs_attention() {
                format!(", {} failed", counts.failed)
            } else {
                String::new()
            },
            if open { "close" } else { "open" }
        )))
        .aria_toggled(if open {
            gpui::accesskit::Toggled::True
        } else {
            gpui::accesskit::Toggled::False
        })
        .tab_index(0)
        .flex_none()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .h(t.size.control)
        .px(t.space.sm)
        .rounded(t.radius.compact)
        .border_1()
        .border_color(if open {
            t.extra.border_strong
        } else {
            gpui::transparent_black()
        })
        .when(open, |d| d.bg(t.colors.surface))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text)
        .child(format!("{}/{}", active + 1, runs.len()))
        .when(counts.needs_attention(), |d| {
            d.child(
                div()
                    .size(t.space.tight)
                    .rounded(t.radius.pill)
                    .bg(t.extra.danger),
            )
        })
        .child(icon(
            if open {
                "icons/chevron-up.svg"
            } else {
                "icons/chevron-down.svg"
            },
            t.size.icon_xs,
            t.colors.text_subtle,
        ))
        .on_mouse_down(gpui::MouseButton::Left, |_, window, _| window.prevent_default())
        .on_click({
            let toggle = toggle.clone();
            move |_, _, cx| toggle.update(cx, |s, cx| s.toggle_agent_jumper(cx))
        })
        .on_key_down(move |ev, _, cx| {
            if matches!(ev.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
                toggle.update(cx, |s, cx| s.toggle_agent_jumper(cx));
            }
        })
        .into_any_element()
}

/// The list, anchored under the button; `left` anchors it to the button's
/// left edge (the narrow panel), else its right.
pub fn popover(
    runs: &[AgentRun],
    active: usize,
    ui_toggles: &std::collections::HashSet<String>,
    left: bool,
    focus_to: Option<usize>,
    state: &Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let now = Instant::now();
    let counts = Counts::of(runs);
    let on = filter(ui_toggles);
    let segment = |f: Filter, label: &str| {
        let selected = on == f;
        let setter = state.clone();
        let hover = t.colors.hover;
        div()
            .id(SharedString::from(format!("agent-filter-{f:?}")))
            .role(gpui::accesskit::Role::Button)
            .tab_index(0)
            .flex_none()
            .h(t.size.control)
            .px(t.space.sm)
            .flex()
            .items_center()
            .rounded(t.radius.segment)
            .when(selected, |d| {
                d.bg(t.colors.surface)
                    .border_1()
                    .border_color(t.extra.border_strong)
            })
            .when(!selected, |d| d.hover(move |s| s.bg(hover)))
            .cursor_pointer()
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(if selected {
                t.colors.text_strong
            } else {
                t.colors.text_muted
            })
            .child(format!("{label} ({})", counts.for_filter(f)))
            .on_click(move |_, _, cx| {
                setter.update(cx, |s, cx| {
                    s.ui_toggles.remove(FILTER_RUNNING);
                    s.ui_toggles.remove(FILTER_FAILED);
                    match f {
                        Filter::Running => {
                            s.ui_toggles.insert(FILTER_RUNNING.into());
                        }
                        Filter::Failed => {
                            s.ui_toggles.insert(FILTER_FAILED.into());
                        }
                        Filter::All => {}
                    }
                    cx.notify();
                })
            })
    };
    let shown = agent_roster::listed(runs, on);
    // The row that takes the focus: the one asked for (↑ ↓), else, as the
    // list opens, the current agent, or the first when the filter hides it.
    let focus_row = focus_to.and_then(|want| {
        shown
            .iter()
            .copied()
            .find(|&i| i == want)
            .or_else(|| shown.iter().copied().find(|&i| i == active))
            .or(shown.first().copied())
    });
    let shown_keys = std::rc::Rc::new(shown.clone());
    let rows = shown.iter().copied().enumerate().map(|(pos, i)| {
        let r = &runs[i];
        let current = i == active;
        let picker = state.clone();
        let id = r.id.clone();
        let hover = t.colors.hover;
        let took = r.ended.unwrap_or(now).saturating_duration_since(r.started);
        let inner = div()
            .w_full()
            .px(t.space.sm)
            .py(t.space.tight)
            .rounded(t.radius.button)
            .when(current, |d| d.bg(t.colors.primary_tint))
            .when(!current, |d| d.hover(move |s| s.bg(hover)))
            .flex()
            .items_center()
            .gap(t.space.sm)
            .child(super::agents_view::status_icon(
                agent_roster::status(r),
                t,
                t.size.icon_xs,
                format!("agent-pop-spin-{i}"),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.body_sm)
                            .font_weight(if current {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .text_color(t.colors.text_strong)
                            .child(r.task.clone()),
                    )
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(
                                [
                                    (!r.kind.is_empty()).then(|| r.kind.clone()),
                                    Some(agent_roster::steps(r.steps.len())),
                                    Some(super::agents_view::elapsed(took)),
                                ]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" · "),
                            ),
                    ),
            );
        let row = super::button::KeyRow::new(
            ("agent-jump-row", i),
            SharedString::from(format!("Agent {} of {}: {}", i + 1, runs.len(), r.task)),
            inner,
        )
        .radius(t.radius.button)
        .expanded(current)
        .focus_now(focus_row == Some(i))
        .on_activate(move |_, cx| {
            picker.update(cx, |s, cx| s.jump_to_agent(&id, cx))
        });
        // ↑ ↓ walk the rows, stopping at the ends; Enter or Space picks.
        let mover = state.clone();
        let order = shown_keys.clone();
        div()
            .w_full()
            .on_key_down(move |ev, _, cx| {
                let to = match ev.keystroke.key.as_str() {
                    "down" => order.get(pos + 1),
                    "up" => pos.checked_sub(1).and_then(|p| order.get(p)),
                    _ => return,
                };
                cx.stop_propagation();
                if let Some(&to) = to {
                    mover.update(cx, |s, cx| {
                        s.agent_jumper_focus.set(Some(to));
                        cx.notify();
                    });
                }
            })
            .child(row)
    });
    let closer = state.clone();
    let panel = crate::theme::popover(t)
        .id("agent-jumper-popover")
        .occlude()
        .w(px(330.))
        .p(t.space.tight)
        .flex()
        .flex_col()
        .gap(t.space.xs)
        .on_mouse_down_out(move |ev, _, cx| {
            closer.update(cx, |s, cx| s.dismiss_ui(AGENT_JUMPER, ev.position, cx));
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .p(px(2.))
                .rounded(t.radius.segment)
                .bg(t.extra.sunk)
                .child(segment(Filter::All, "All"))
                .child(segment(Filter::Running, "Running"))
                .child(segment(Filter::Failed, "Failed")),
        )
        .child(
            div()
                .id("agent-jumper-list")
                .max_h(px(320.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(2.))
                .children(rows),
        );
    div()
        .absolute()
        .bottom_0()
        .when(left, |d| d.left_0())
        .when(!left, |d| d.right_0())
        .child(
            gpui::deferred(
                gpui::anchored()
                    .anchor(if left {
                        gpui::Anchor::TopLeft
                    } else {
                        gpui::Anchor::TopRight
                    })
                    .snap_to_window_with_margin(t.size.viewport_margin)
                    .child(div().pt(t.space.xs).child(super::motion::drop_menu(
                        panel,
                        "agent-jumper-popover",
                        false,
                        t,
                    ))),
            )
            .with_priority(2),
        )
        .into_any_element()
}
