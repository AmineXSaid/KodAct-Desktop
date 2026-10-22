//! The side panel's Agents tab: Forge's sub-agents at work, live. Forge
//! sits at the top with what you asked; each agent it started hangs off a
//! spine below it, as a card: its kind, its task, its state and how long it
//! has run, a strip with a cell per step it took, and what it is doing now.
//! A card opens to show the agent's instructions, every step and its answer.
//!
//! Read from `AppState::agents`, which the sub-agents' own messages fill
//! (`backend.rs` reads them from the stream; they used to be dropped).

use std::time::{Duration, Instant};

use gpui::{
    div, percentage, prelude::*, px, relative, Animation, AnimationExt, AnyElement, Entity,
    FontWeight, Hsla, SharedString, Transformation,
};

use super::icon;
use crate::state::{AgentRun, AppState, ToolStatus};
use crate::theme::Theme;

/// "8s", "1m 04s", "1h 02m".
pub fn elapsed(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    }
}

/// A colour per agent kind, from the theme's own colours, the same each time.
fn kind_color(kind: &str, t: &Theme) -> Hsla {
    let all = [
        t.colors.primary,
        t.colors.marigold,
        t.extra.success,
        t.colors.sky,
        t.colors.coral,
    ];
    let h = kind
        .bytes()
        .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    all[h % all.len()]
}

fn status_of(run: &AgentRun) -> ToolStatus {
    match (run.ended, run.failed) {
        (None, _) => ToolStatus::Running,
        // You stopped it: not a failure, and not done.
        (Some(_), _) if run.stopped => ToolStatus::Declined,
        (Some(_), true) => ToolStatus::Failed,
        (Some(_), false) => ToolStatus::Done,
    }
}

/// The small ghost Stop on a running card, and "Stop all" in the header.
fn stop_button(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    label: &str,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .px(t.space.sm)
        .py(px(2.))
        .rounded(t.radius.button)
        .border_1()
        .border_color(t.colors.border)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(
            "icons/square.svg",
            t.size.icon_xs,
            t.colors.text_muted,
        ))
        .child(label.to_string())
}

fn spinner(t: &Theme, color: Hsla, key: String) -> AnyElement {
    icon("icons/loader-circle.svg", t.size.icon_sm, color)
        .with_animation(
            SharedString::from(key),
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, d| svg.with_transformation(Transformation::rotate(percentage(d))),
        )
        .into_any_element()
}

fn stat(t: &Theme, value: String, label: &str, color: Hsla) -> gpui::Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(t.space.md)
        .py(t.space.sm)
        .rounded(t.radius.button)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .child(
            div()
                .text_size(t.text.subheading)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(value),
        )
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(label.to_string()),
        )
}

pub struct View<'a> {
    pub runs: &'a [AgentRun],
    /// What you asked, which Forge split across its agents.
    pub request: Option<SharedString>,
    /// Forge's mark for the top of the tree.
    pub logo: AnyElement,
    /// The chat's turn is still going.
    pub working: bool,
}

pub fn render(
    v: View,
    state: Entity<AppState>,
    open: &std::collections::HashSet<String>,
    t: &Theme,
) -> AnyElement {
    let now = Instant::now();
    if v.runs.is_empty() {
        return div()
            .flex()
            .flex_col()
            .items_center()
            .gap(t.space.sm)
            .px(t.space.xxl)
            .pt(t.space.xxl * 2.)
            .text_center()
            .child(icon("icons/network.svg", t.size.icon_lg, t.colors.text_subtle))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text_strong)
                    .child("No agents yet"),
            )
            .child(
                div()
                    .max_w(px(320.))
                    .text_size(t.text.body_sm)
                    .line_height(relative(1.5))
                    .text_color(t.colors.text_muted)
                    .child("Ask Forge to split the work across sub-agents (\"use sub-agents to review each module\"). Each agent shows here as it works: what it was asked, every step it takes, and what it found."),
            )
            .into_any_element();
    }
    let running = v.runs.iter().filter(|r| r.ended.is_none()).count();
    let stoppable = v
        .runs
        .iter()
        .filter(|r| r.ended.is_none() && !r.stopping)
        .count();
    let failed = v.runs.iter().filter(|r| r.failed).count();
    let stopped = v.runs.iter().filter(|r| r.stopped).count();
    let steps: usize = v.runs.iter().map(|r| r.steps.len()).sum();
    let first = v.runs.iter().map(|r| r.started).min().unwrap_or(now);
    let last = if running > 0 {
        now
    } else {
        v.runs.iter().filter_map(|r| r.ended).max().unwrap_or(now)
    };

    let summary = div()
        .flex()
        .gap(t.space.sm)
        .child(stat(
            t,
            v.runs.len().to_string(),
            "agents",
            t.colors.text_strong,
        ))
        .child(stat(
            t,
            running.to_string(),
            "running",
            if running > 0 {
                t.colors.primary
            } else {
                t.colors.text_muted
            },
        ))
        .child(stat(t, steps.to_string(), "steps", t.colors.text_strong))
        .child(stat(
            t,
            elapsed(last.saturating_duration_since(first)),
            "elapsed",
            t.colors.text_strong,
        ));

    // Forge at the root of the tree.
    let root = div()
        .flex()
        .items_center()
        .gap(t.space.md)
        .p(t.space.md)
        .rounded(t.radius.card)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .child(div().flex_none().child(v.logo))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                                .child("Forge"),
                        )
                        .child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(if running > 0 {
                                    t.colors.primary
                                } else {
                                    t.colors.text_muted
                                })
                                .child(if running > 0 {
                                    format!("coordinating {running} of {}", v.runs.len())
                                } else if v.working {
                                    "putting it together".to_string()
                                } else if failed > 0 {
                                    format!("done, {failed} failed")
                                } else if stopped > 0 {
                                    format!("done, {stopped} stopped")
                                } else {
                                    "all agents done".to_string()
                                }),
                        ),
                )
                .children(v.request.clone().map(|r| {
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_muted)
                        .child(r)
                })),
        )
        .when(stoppable > 1, |d| {
            let state = state.clone();
            d.child(
                stop_button(t, "agents-stop-all", "Stop all")
                    .tooltip(|w, cx| {
                        gpui::component::tooltip::Tooltip::new(
                            "Stop every agent; Forge carries on with what they found (Ctrl+Shift+Esc)",
                        )
                        .build(w, cx)
                    })
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        state.update(cx, |s, cx| s.stop_agents(cx));
                    }),
            )
        });

    let cards = v.runs.iter().enumerate().map(|(i, run)| {
        let status = status_of(run);
        let color = kind_color(&run.kind, t);
        let is_open = open.contains(&run.id);
        let toggler = state.clone();
        let stopper = state.clone();
        let id = run.id.clone();
        let stop_id = run.id.clone();
        let took = run
            .ended
            .unwrap_or(now)
            .saturating_duration_since(run.started);
        let (status_icon, status_color): (AnyElement, Hsla) = match status {
            ToolStatus::Running => (
                spinner(t, t.colors.primary, format!("agent-{i}")),
                t.colors.primary,
            ),
            ToolStatus::Failed => (
                icon("icons/circle-x.svg", t.size.icon_sm, t.extra.danger).into_any_element(),
                t.extra.danger,
            ),
            ToolStatus::Declined => (
                icon("icons/square.svg", t.size.icon_sm, t.colors.text_subtle).into_any_element(),
                t.colors.text_subtle,
            ),
            _ => (
                icon("icons/circle-check.svg", t.size.icon_sm, t.extra.success).into_any_element(),
                t.extra.success,
            ),
        };
        let now_doing = run.steps.last().map(|s| s.label());
        let hover = t.colors.hover;

        // A cell per step: blue while it runs, green done, red failed.
        let strip = div()
            .flex()
            .flex_wrap()
            .gap(px(3.))
            .children(run.steps.iter().take(60).map(|s| {
                div().size(px(8.)).rounded(px(2.)).bg(match s.status {
                    ToolStatus::Running => t.colors.primary,
                    ToolStatus::Done => t.extra.success.opacity(0.75),
                    ToolStatus::Failed => t.extra.danger,
                    ToolStatus::Declined => t.colors.text_subtle,
                    ToolStatus::Interrupted => t.extra.warning,
                })
            }));

        let card = div()
            .id(("agent-card", i))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .p(t.space.md)
            .rounded(t.radius.card)
            .border_1()
            .border_color(if status == ToolStatus::Running {
                t.colors.primary.opacity(0.45)
            } else {
                t.colors.border
            })
            .bg(t.colors.surface)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(move |_, _, cx| toggler.update(cx, |s, cx| s.toggle_agent(&id, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(status_icon)
                    .child(
                        div()
                            .flex_none()
                            .px(t.space.sm)
                            .py(px(1.))
                            .rounded(t.radius.pill)
                            .bg(color.opacity(0.14))
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(color)
                            .child(run.kind.clone()),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(status_color)
                            .child(match status {
                                ToolStatus::Running if run.stopping => "stopping…".to_string(),
                                ToolStatus::Running => format!("running · {}", elapsed(took)),
                                ToolStatus::Failed => format!("failed · {}", elapsed(took)),
                                ToolStatus::Declined | ToolStatus::Interrupted => {
                                    format!("stopped · {}", elapsed(took))
                                }
                                ToolStatus::Done => format!("done · {}", elapsed(took)),
                            }),
                    )
                    .when(status == ToolStatus::Running && !run.stopping, |d| {
                        d.child(
                            stop_button(t, ("agent-stop", i), "Stop")
                                .tooltip(|w, cx| {
                                    gpui::component::tooltip::Tooltip::new(
                                        "Stop this agent; the others go on",
                                    )
                                    .build(w, cx)
                                })
                                .on_click(move |_, _, cx| {
                                    // The card's own click opens it; this one only stops.
                                    cx.stop_propagation();
                                    stopper.update(cx, |s, cx| s.stop_agent(&stop_id, cx));
                                }),
                        )
                    })
                    .child(icon(
                        if is_open {
                            "icons/chevron-up.svg"
                        } else {
                            "icons/chevron-down.svg"
                        },
                        t.size.icon_sm,
                        t.colors.text_subtle,
                    )),
            )
            .child(
                div()
                    .text_size(t.text.body_sm)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text_strong)
                    .child(run.task.clone()),
            )
            .when(!run.steps.is_empty(), |d| d.child(strip))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child(format!(
                        "{} step{}",
                        run.steps.len(),
                        if run.steps.len() == 1 { "" } else { "s" }
                    ))
                    .when_some(
                        now_doing.filter(|_| status == ToolStatus::Running),
                        |d, doing| {
                            d.child("·").child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_color(t.colors.text)
                                    .child(doing),
                            )
                        },
                    ),
            )
            .when(is_open, |d| {
                d.child(crate::theme::rule(t))
                    .when(!run.prompt.is_empty(), |d| {
                        d.child(section(t, "Asked")).child(
                            div()
                                .text_size(t.text.caption)
                                .line_height(relative(1.5))
                                .text_color(t.colors.text_muted)
                                .child(clip(&run.prompt, 600)),
                        )
                    })
                    .when(!run.steps.is_empty(), |d| {
                        d.child(section(t, "Steps")).child(
                            div().flex().flex_col().gap(t.space.xs).children(
                                run.steps.iter().enumerate().map(|(j, s)| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(t.space.sm)
                                        .text_size(t.text.caption)
                                        .child(super::tasks::status_icon(
                                            s.status,
                                            t,
                                            format!("agent-{i}-step-{j}").into(),
                                        ))
                                        .child(icon(
                                            s.kind.icon(),
                                            t.size.icon_xs,
                                            t.colors.text_subtle,
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .text_color(t.colors.text)
                                                .child(s.label()),
                                        )
                                }),
                            ),
                        )
                    })
                    .when(!run.said.trim().is_empty(), |d| {
                        d.child(section(
                            t,
                            if status == ToolStatus::Running {
                                "Saying"
                            } else {
                                "Answer"
                            },
                        ))
                        .child(
                            div()
                                .p(t.space.sm)
                                .rounded(t.radius.small)
                                .bg(t.extra.inset)
                                .text_size(t.text.caption)
                                .line_height(relative(1.5))
                                .text_color(t.colors.text)
                                .child(clip(&run.said, 1200)),
                        )
                    })
            });

        // The branch: a node on the spine, an elbow to the card. A new
        // agent's branch rises into place as it starts.
        let branch = div()
            .flex()
            .items_start()
            .child(
                div()
                    .flex_none()
                    .w(px(28.))
                    .h(px(28.))
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .left(px(10.))
                            .top(px(16.))
                            .w(px(18.))
                            .h(px(2.))
                            .rounded(t.radius.line)
                            .bg(t.colors.border),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(6.))
                            .top(px(12.))
                            .size(px(10.))
                            .rounded(t.radius.pill)
                            .bg(status_color)
                            .border_2()
                            .border_color(t.colors.surface),
                    ),
            )
            .child(card);
        super::motion::row_in(
            branch,
            SharedString::from(format!("agent-in-{}", run.id)),
            0,
            t,
        )
    });

    div()
        .flex()
        .flex_col()
        .gap(t.space.lg)
        .px(t.space.xl)
        .pt(t.space.xl)
        .pb(t.space.xxl * 2.)
        .child(summary)
        .child(
            div().flex().flex_col().child(root).child(
                // The spine down from Forge, with an agent off each node.
                div()
                    .relative()
                    .pt(t.space.md)
                    .child(
                        div()
                            .absolute()
                            .left(px(10.))
                            .top_0()
                            .bottom(px(14.))
                            .w(px(2.))
                            .rounded(t.radius.line)
                            .bg(t.colors.border),
                    )
                    .child(div().flex().flex_col().gap(t.space.md).children(cards)),
            ),
        )
        .into_any_element()
}

fn section(t: &Theme, text: &str) -> gpui::Div {
    div()
        .text_size(t.text.caption)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(t.colors.text_muted)
        .child(text.to_uppercase())
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.trim().to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>().trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_short() {
        assert_eq!(elapsed(Duration::from_secs(8)), "8s");
        assert_eq!(elapsed(Duration::from_secs(64)), "1m 04s");
        assert_eq!(elapsed(Duration::from_secs(3720)), "1h 02m");
    }
}
