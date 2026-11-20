//! A sub-agent in the transcript, as one row: its status mark, its full
//! title (never cut at a fixed width), its kind as a chip and "Inspect ›" on
//! the right; under it how it went ("Failed · 1 step · 10s",
//! `agent_roster::status_line`), and for a failed, stopped or stalled agent
//! the failure note in its colour, on a tinted ground with 4px corners and
//! no coloured side bar. Pressing it (or Enter / Space) opens that agent in
//! the Agents panel.

use gpui::{div, prelude::*, px, relative, AnyElement, FontWeight, SharedString};

use super::icon;
use super::message::ItemContext;
use crate::state::{Block, ToolCall, ToolStatus};
use crate::theme::Theme;

/// Quiet this long while it runs, an agent looks stuck and says so.
const AGENT_STALLED: std::time::Duration = std::time::Duration::from_secs(120);

/// The row: status mark,
/// task title, optional kind badge, single concise status/step readout, optional
/// inline Stop button, and Inspect action. Failures & warnings render a compact
/// error strip directly below so failures are never hidden.
pub fn render(
    call: &ToolCall,
    b: usize,
    blocks: &[Block],
    cx: &ItemContext,
    t: &Theme,
) -> AnyElement {
    let nth = cx.agent_offset
        + blocks[..b]
            .iter()
            .filter(|blk| matches!(blk, Block::Tool(tc) if crate::state::is_agent_tool(&tc.target)))
            .count();
    let fallback_task = call.note.clone().unwrap_or_else(|| "Sub-agent task".into());
    let run = cx
        .agents
        .get(nth)
        .or_else(|| cx.agents.iter().find(|r| r.task == fallback_task.as_ref()));

    let key = format!("{}-{}-{b}", cx.chat, cx.index);
    let title: String = run
        .map(|r| r.task.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_task.to_string());
    let kind = run.map(|r| r.kind.clone()).filter(|s| !s.is_empty());

    let running = run.is_some_and(|r| r.ended.is_none()) || call.status == ToolStatus::Running;
    let stopping = run.is_some_and(|r| r.stopping);
    let stopped = run.is_some_and(|r| r.stopped)
        || call.status == ToolStatus::Declined
        || call.output.starts_with(crate::state::AGENT_STOPPED);
    let interrupted = !stopped && call.status == ToolStatus::Interrupted;
    let failed =
        !stopped && !interrupted && (run.is_some_and(|r| r.failed) || call.status == ToolStatus::Failed);
    let inspected = run.is_some_and(|r| cx.inspected_agent.as_deref() == Some(r.id.as_str()));

    let now = std::time::Instant::now();
    let stalled = run.is_some_and(|r| {
        r.ended.is_none()
            && call.status == ToolStatus::Running
            && now.duration_since(r.heard) >= AGENT_STALLED
    });
    let elapsed_str = run.map(|r| {
        let secs = r
            .ended
            .unwrap_or(now)
            .saturating_duration_since(r.started)
            .as_secs();
        super::chat_view::elapsed(secs)
    });
    let took = run.map(|r| r.ended.unwrap_or(now).saturating_duration_since(r.started));
    let step_count = run.map_or(0, |r| r.steps.len());
    let live_step = if running && !stopping && !stalled {
        run.and_then(|r| r.steps.last().map(|s| s.label()))
    } else {
        None
    };

    let mark: AnyElement = if stalled {
        icon("icons/triangle-alert.svg", t.size.icon_xs, t.extra.warning).into_any_element()
    } else if running {
        super::turning_logo_in(
            f32::from(t.size.icon_xs),
            SharedString::from(format!("agent-turn-{key}")),
            t.extra.live,
        )
    } else if stopped {
        icon("icons/square.svg", t.size.icon_xs, t.colors.text_subtle).into_any_element()
    } else if interrupted {
        icon("icons/triangle-alert.svg", t.size.icon_xs, t.extra.warning).into_any_element()
    } else if failed {
        icon("icons/circle-x.svg", t.size.icon_xs, t.extra.danger).into_any_element()
    } else {
        icon("icons/circle-check.svg", t.size.icon_xs, t.extra.success).into_any_element()
    };

    let status_summary = if stalled {
        format!(
            "No word for {}",
            super::chat_view::elapsed(run.map_or(0, |r| now.duration_since(r.heard).as_secs()))
        )
    } else if stopped || interrupted || failed {
        // As the Agents panel words it (`agent_roster`): "Failed · 1 step ·
        // 10s", just "Failed" with no steps.
        let st = if stopped {
            ToolStatus::Declined
        } else if interrupted {
            ToolStatus::Interrupted
        } else {
            ToolStatus::Failed
        };
        super::agent_roster::status_line(st, step_count, took)
    } else if running {
        let verb = if stopping { "Stopping" } else { "Running" };
        match (live_step.as_deref(), elapsed_str) {
            (Some(st), Some(el)) => format!("{st} · {el}"),
            (Some(st), None) => st.to_string(),
            (None, Some(el)) => format!("{verb} · {el}"),
            (None, None) => verb.to_string(),
        }
    } else {
        match (step_count, elapsed_str) {
            (0, Some(el)) => format!("Done · {el}"),
            (0, None) => "Done".to_string(),
            (n, Some(el)) => format!("{n} step{} · {el}", if n == 1 { "" } else { "s" }),
            (n, None) => format!("{n} step{}", if n == 1 { "" } else { "s" }),
        }
    };

    // Never hide a sub-agent failure or warning behind collapse.
    let problem_note: Option<String> = if failed || interrupted || stalled {
        let from_step = run
            .and_then(|r| {
                r.steps
                    .iter()
                    .rev()
                    .find(|s| matches!(s.status, ToolStatus::Failed | ToolStatus::Interrupted))
                    .and_then(|s| {
                        let first = s.output.lines().map(str::trim).find(|l| !l.is_empty());
                        match first {
                            Some(line) => Some(format!("{}: {line}", s.label())),
                            None => Some(format!("{} failed", s.label())),
                        }
                    })
            });
        from_step
            .or_else(|| {
                super::message::failure_line(call).map(|l| l.to_string()).or_else(|| {
                    run.and_then(|r| {
                        r.said
                            .lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .map(|l| l.to_string())
                    })
                })
            })
            .or_else(|| {
                stalled.then(|| "Agent appears stalled — no recent progress".to_string())
            })
    } else {
        None
    };

    let hover_bg = t.colors.hover;
    let opener = cx.state.clone();
    let (chat, msg) = (cx.chat, cx.index);
    let stop_id = run.map(|r| r.id.clone());
    let stopper = cx.state.clone();
    let stopper_key = cx.state.clone();

    let row_inner = div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(t.space.sm)
        .py(t.space.xs + px(2.))
        // Rows hold, so a card's corners: a pill's would eat the text.
        .rounded(t.radius.card)
        .when(inspected, |d| d.bg(t.colors.primary_tint.opacity(0.65)))
        .when(failed && !inspected, |d| d.bg(t.extra.danger_tint.opacity(0.65)))
        .when((interrupted || stalled) && !failed && !inspected, |d| {
            d.bg(t.extra.warning_tint.opacity(0.65))
        })
        .hover(move |s| s.bg(hover_bg))
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(t.space.xs + px(2.))
                .min_h(t.size.quiet_line)
                .child(div().flex_none().child(mark))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.body_sm)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child(title.clone()),
                )
                .when_some(kind, |d, k| {
                    d.child(
                        div()
                            .flex_none()
                            .px(t.space.xs + px(2.))
                            .py(px(1.))
                            .rounded(t.radius.small)
                            .bg(t.colors.hover)
                            .text_size(t.text.caption - px(0.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_muted)
                            .child(k),
                    )
                })
                .when_some(
                    stop_id.filter(|_| running && !stopping),
                    |d, id| {
                        let btn_hover = t.colors.hover;
                        let id_key = id.clone();
                        d.child(
                            div()
                                .id(SharedString::from(format!("agent-row-stop-{key}")))
                                .role(gpui::accesskit::Role::Button)
                                .aria_label(SharedString::from(format!("Stop agent {title}")))
                                .tab_index(0)
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .px(t.space.sm)
                                .h(px(20.))
                                .rounded(t.radius.compact)
                                .border_1()
                                .border_color(t.extra.border_strong)
                                .bg(t.colors.surface)
                                .text_size(t.text.caption)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.colors.text_muted)
                                .cursor_pointer()
                                .hover(move |s| s.bg(btn_hover))
                                .child(icon(
                                    "icons/square.svg",
                                    t.size.icon_xs,
                                    t.colors.text_muted,
                                ))
                                .child("Stop")
                                .on_click(move |_, _, app| {
                                    app.stop_propagation();
                                    stopper.update(app, |s, cx| s.stop_agent(&id, cx));
                                })
                                .on_key_down(move |ev, _, app| {
                                    if matches!(ev.keystroke.key.as_str(), "enter" | "space") {
                                        app.stop_propagation();
                                        stopper_key.update(app, |s, cx| s.stop_agent(&id_key, cx));
                                    }
                                }),
                        )
                    },
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if inspected {
                            t.colors.primary
                        } else {
                            t.colors.text_muted
                        })
                        .child(if inspected { "Inspecting" } else { "Inspect" })
                        .child(icon(
                            "icons/chevron-right.svg",
                            t.size.icon_xs,
                            if inspected {
                                t.colors.primary
                            } else {
                                t.colors.text_subtle
                            },
                        )),
                ),
        )
        // Under the title, aligned with it: how it went, then what went wrong.
        .child(
            div()
                .pl(t.size.icon_xs + t.space.xs + px(2.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.caption)
                .text_color(if stalled || interrupted {
                    t.extra.warning
                } else if running {
                    t.extra.live
                } else if failed {
                    t.extra.danger
                } else {
                    t.colors.text_subtle
                })
                .child(status_summary.clone()),
        )
        .when_some(problem_note, |d, note| {
            d.child(
                div()
                    .pl(t.size.icon_xs + t.space.xs + px(2.))
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.caption)
                    .line_height(relative(1.4))
                    .text_color(if failed {
                        t.extra.danger
                    } else {
                        t.extra.warning
                    })
                    .child(note),
            )
        });

    super::button::KeyRow::new(
        SharedString::from(format!("tool-{key}")),
        SharedString::from(format!(
            "Agent: {title}, {status_summary}. Press to inspect in Agents panel"
        )),
        row_inner,
    )
    .w_full()
    .on_activate(move |_, app| {
        opener.update(app, |s, cx| s.open_agent_for_block(chat, msg, b, cx));
    })
    .into_any_element()
}
