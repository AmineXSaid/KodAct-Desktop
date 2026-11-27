//! The side panel's Agents tab: a focused sub-agent inspector.
//!
//! Switch between sub-agents across the top bar (or `<` `>`), expand
//! "Message from parent agent" for the prompt, and follow an interleaved
//! chronological stream of prose and expandable tool steps (active step
//! outlined in the primary accent). Full View expands the inspector across
//! the session column.

use std::time::{Duration, Instant};

use gpui::{
    div, percentage, prelude::*, px, relative, Animation, AnimationExt, AnyElement, Entity,
    FontWeight, Hsla, SharedString, Transformation,
};

use super::icon;
use super::markdown::{self, CopyContext, Look};
use crate::state::{AgentEntry, AgentRun, AgentStep, AppState, ToolStatus};
use crate::theme::Theme;

/// Quiet this long while it runs, an agent looks stuck and says so.
const AGENT_STALLED: Duration = Duration::from_secs(120);

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

fn status_of(run: &AgentRun) -> ToolStatus {
    super::agent_roster::status(run)
}

/// The small ghost Stop pill used in the header and footer.
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
        .h(px(26.))
        .px(t.space.sm + px(2.))
        .rounded(t.radius.compact)
        .border_1()
        .border_color(t.extra.border_strong)
        .bg(t.colors.surface)
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

fn spinner(size: gpui::Pixels, color: Hsla, key: String) -> AnyElement {
    icon("icons/loader-circle.svg", size, color)
        .with_animation(
            SharedString::from(key),
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, d| svg.with_transformation(Transformation::rotate(percentage(d))),
        )
        .into_any_element()
}

/// An agent's status as an icon (the strip, the jumper, the footer).
pub fn status_icon(status: ToolStatus, t: &Theme, size: gpui::Pixels, key: String) -> AnyElement {
    run_status_icon(status, t, size, key)
}

fn run_status_icon(status: ToolStatus, t: &Theme, size: gpui::Pixels, key: String) -> AnyElement {
    match status {
        ToolStatus::Running => spinner(size, t.extra.live, key),
        ToolStatus::Failed => div()
            .size(size)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(px(6.)).rounded(px(3.)).bg(t.extra.danger))
            .into_any_element(),
        ToolStatus::Declined => {
            icon("icons/square.svg", size, t.colors.text_subtle).into_any_element()
        }
        ToolStatus::Interrupted => {
            icon("icons/triangle-alert.svg", size, t.extra.warning).into_any_element()
        }
        ToolStatus::Done => {
            icon("icons/check.svg", size, t.colors.text_muted).into_any_element()
        }
    }
}

fn nav_icon_button(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    path: &'static str,
    tip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex_none()
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.compact)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(icon(path, t.size.icon_sm, t.colors.text_muted))
        .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(tip).build(w, cx))
}

pub struct View<'a> {
    pub runs: &'a [AgentRun],
    pub selected: Option<&'a str>,
    pub full: bool,
    pub copied: Option<SharedString>,
    pub scale: f32,
    /// The pill strip's scroll, and the pill it last brought into view.
    pub strip: &'a gpui::ScrollHandle,
    pub strip_at: &'a std::cell::Cell<Option<usize>>,
    /// The jumper row to focus next (`AppState::agent_jumper_focus`).
    pub jumper_focus: &'a std::cell::Cell<Option<usize>>,
}

/// Group consecutive `AgentEntry::Step` indices of the same `ToolKind` when `>= 2`
/// are `Done` (and collapsed), leaving `Running`, `Failed`, `Interrupted`,
/// `Declined`, or individually opened steps standalone.
#[derive(Clone, Debug, PartialEq)]
enum InspectorItem {
    Text {
        entry_ix: usize,
    },
    TextBatch {
        entry_indices: Vec<usize>,
    },
    Step {
        step_idx: usize,
    },
    StepGroup {
        kind: crate::state::ToolKind,
        step_indices: Vec<usize>,
    },
}

fn build_inspector_items(run: &AgentRun, has_final_answer: bool) -> Vec<InspectorItem> {
    use crate::state::ToolKind;

    let is_groupable_step = |step_idx: usize| -> Option<ToolKind> {
        let s = run.steps.get(step_idx)?;
        if s.status == ToolStatus::Done && !s.open {
            Some(s.kind)
        } else {
            None
        }
    };

    let has_warning_text = |txt: &str| {
        let lower = txt.to_ascii_lowercase();
        lower.contains("error:") || lower.contains("failed") || lower.contains("warning:")
    };

    if run.entries.is_empty() {
        let mut out = Vec::new();
        let mut i = 0;
        while i < run.steps.len() {
            let Some(kind) = is_groupable_step(i) else {
                out.push(InspectorItem::Step { step_idx: i });
                i += 1;
                continue;
            };
            let mut end = i + 1;
            while end < run.steps.len() && is_groupable_step(end) == Some(kind) {
                end += 1;
            }
            if end - i >= 2 {
                out.push(InspectorItem::StepGroup {
                    kind,
                    step_indices: (i..end).collect(),
                });
            } else {
                out.push(InspectorItem::Step { step_idx: i });
            }
            i = end;
        }
        return out;
    }

    let total = run.entries.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < total {
        match &run.entries[i] {
            AgentEntry::Step(step_idx) => {
                let Some(kind) = is_groupable_step(*step_idx) else {
                    out.push(InspectorItem::Step {
                        step_idx: *step_idx,
                    });
                    i += 1;
                    continue;
                };
                let mut end = i + 1;
                while end < total {
                    if let AgentEntry::Step(next_step) = &run.entries[end] {
                        if is_groupable_step(*next_step) == Some(kind) {
                            end += 1;
                            continue;
                        }
                    }
                    break;
                }
                if end - i >= 2 {
                    let step_indices = run.entries[i..end]
                        .iter()
                        .filter_map(|e| match e {
                            AgentEntry::Step(si) => Some(*si),
                            AgentEntry::Text(_) => None,
                        })
                        .collect();
                    out.push(InspectorItem::StepGroup { kind, step_indices });
                } else {
                    out.push(InspectorItem::Step {
                        step_idx: *step_idx,
                    });
                }
                i = end;
            }
            AgentEntry::Text(txt) => {
                let is_last_entry = i + 1 == total;
                // Keep live tail text (before any final answer) or warnings expanded;
                // collapse earlier intermediate prose notes so the user doesn't scroll past noise.
                let keep_visible = has_warning_text(txt) || (is_last_entry && !has_final_answer);
                if keep_visible {
                    out.push(InspectorItem::Text { entry_ix: i });
                    i += 1;
                } else {
                    let mut batch = vec![i];
                    let mut end = i + 1;
                    while end < total {
                        if let AgentEntry::Text(next_txt) = &run.entries[end] {
                            let next_is_last = end + 1 == total;
                            if !has_warning_text(next_txt) && !(next_is_last && !has_final_answer) {
                                batch.push(end);
                                end += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    out.push(InspectorItem::TextBatch {
                        entry_indices: batch,
                    });
                    i = end;
                }
            }
        }
    }
    out
}

fn kind_group_label(kind: crate::state::ToolKind, n: usize) -> String {
    use crate::state::ToolKind;
    match kind {
        ToolKind::Read => format!("Read {n} files"),
        ToolKind::Edit => format!("Edited {n} files"),
        ToolKind::Write => format!("Wrote {n} files"),
        ToolKind::Bash => format!("Ran {n} commands"),
        ToolKind::Search => format!("Searched {n} times"),
        ToolKind::Web => format!("Fetched {n} pages"),
        ToolKind::WebSearch => format!("Searched web {n} times"),
        ToolKind::Other => format!("Used {n} tools"),
    }
}

pub fn render(
    v: View,
    state: Entity<AppState>,
    open: &std::collections::HashSet<String>,
    ui_toggles: &std::collections::HashSet<String>,
    t: &Theme,
) -> AnyElement {
    let now = Instant::now();
    if v.runs.is_empty() {
        return div()
            .flex_1()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(t.space.sm)
            .px(t.space.xxl)
            .pt(t.space.xxl * 2.)
            .text_center()
            .child(icon(
                "icons/network.svg",
                t.size.icon_lg,
                t.colors.text_subtle,
            ))
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
                    .child("Ask KodAct to split the work across sub-agents (\"use sub-agents to review each module\"). Each agent shows here as it works: what it was asked, every step it takes, and what it found."),
            )
            .into_any_element();
    }

    let active_idx = super::agent_roster::current(v.runs, v.selected);
    let run = &v.runs[active_idx];
    let status = status_of(run);
    let running = v.runs.iter().filter(|r| r.ended.is_none()).count();
    let stoppable = v
        .runs
        .iter()
        .filter(|r| r.ended.is_none() && !r.stopping)
        .count();
    let stalled = run.ended.is_none() && now.duration_since(run.heard) >= AGENT_STALLED;
    let took = run
        .ended
        .unwrap_or(now)
        .saturating_duration_since(run.started);

    // ---- 1. Top Agent Switcher Bar ----
    let compact_split = !v.full && v.runs.len() > 1;
    let equal_row = compact_split && v.runs.len() <= 3;

    let prev_state = state.clone();
    let next_state = state.clone();
    let full_state = state.clone();
    let full_hover = t.colors.hover;
    let jumper_open = ui_toggles.contains(super::AGENT_JUMPER);
    let jumper_button = super::agent_jumper::button(active_idx, v.runs, jumper_open, &state, t);
    let jumper_popover = jumper_open.then(|| {
        super::agent_jumper::popover(
            v.runs,
            active_idx,
            ui_toggles,
            compact_split,
            v.jumper_focus.take(),
            &state,
            t,
        )
    });

    let nav_cluster = div()
        .relative()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.))
        .when(v.runs.len() > 1, |d| {
            d.child(
                nav_icon_button(t, "agent-prev", "icons/arrow-left.svg", "Previous agent")
                    .on_click(move |_, _, cx| {
                        prev_state.update(cx, |s, cx| s.cycle_agent(-1, cx))
                    }),
            )
            .child(jumper_button)
            .child(
                nav_icon_button(t, "agent-next", "icons/arrow-right.svg", "Next agent")
                    .on_click(move |_, _, cx| {
                        next_state.update(cx, |s, cx| s.cycle_agent(1, cx))
                    }),
            )
            .children(jumper_popover)
        });

    let full_button = super::button::KeyRow::new(
        "agent-full-view",
        SharedString::from(if v.full {
            "Switch to split view"
        } else {
            "Expand inspector to full view"
        }),
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.))
            .h(px(26.))
            .px(t.space.sm + px(2.))
            .rounded(t.radius.compact)
            .border_1()
            .border_color(if v.full {
                t.colors.primary.opacity(0.45)
            } else {
                t.extra.border_strong
            })
            .bg(if v.full {
                t.colors.primary_tint
            } else {
                t.colors.surface
            })
            .hover(move |s| s.bg(full_hover))
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(if v.full {
                t.colors.primary
            } else {
                t.colors.text
            })
            .child(icon(
                if v.full {
                    "icons/split.svg"
                } else {
                    "icons/maximize-2.svg"
                },
                t.size.icon_xs,
                if v.full {
                    t.colors.primary
                } else {
                    t.colors.text_muted
                },
            ))
            .child(if v.full { "Split view" } else { "Full view" }),
    )
    .expanded(v.full)
    .on_activate(move |_, cx| full_state.update(cx, |s, cx| s.toggle_panel_full(cx)));

    let tabs_strip = super::agent_strip::render(
        v.runs,
        active_idx,
        equal_row,
        v.strip,
        v.strip_at,
        &state,
        t,
    );

    let switcher_base = div()
        .flex_none()
        .px(t.space.md)
        .py(t.space.xs + px(2.))
        .border_b_1()
        .border_color(t.colors.border)
        .bg(t.extra.sunk);

    let switcher = if compact_split {
        switcher_base
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(t.space.sm)
                    .child(nav_cluster)
                    .child(full_button),
            )
            .child(tabs_strip)
    } else {
        switcher_base
            .flex()
            .items_center()
            .gap(t.space.sm)
            .child(div().flex_1().min_w_0().child(tabs_strip))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(nav_cluster)
                    .child(full_button),
            )
    };

    // ---- 2. Focused Agent Header + At-a-Glance Summary Strip ----
    let stopper = state.clone();
    let stop_id = run.id.clone();
    let status_text = if stalled {
        format!(
            "No word from it for {}",
            elapsed(now.duration_since(run.heard))
        )
    } else {
        match status {
            ToolStatus::Running if run.stopping => "Stopping…".to_string(),
            ToolStatus::Running => format!("Running · {}", elapsed(took)),
            ToolStatus::Failed => format!("Failed · {}", elapsed(took)),
            ToolStatus::Declined => format!("Stopped · {}", elapsed(took)),
            ToolStatus::Interrupted => format!("Interrupted · {}", elapsed(took)),
            ToolStatus::Done => format!("Done in {}", elapsed(took)),
        }
    };
    let status_color = if stalled {
        t.extra.warning
    } else {
        match status {
            ToolStatus::Running => t.colors.primary,
            ToolStatus::Failed => t.extra.danger,
            ToolStatus::Interrupted => t.extra.warning,
            ToolStatus::Declined => t.colors.text_subtle,
            ToolStatus::Done => t.colors.text_muted,
        }
    };

    let done_count = run
        .steps
        .iter()
        .filter(|s| s.status == ToolStatus::Done)
        .count();
    let fail_count = run
        .steps
        .iter()
        .filter(|s| matches!(s.status, ToolStatus::Failed | ToolStatus::Interrupted))
        .count();

    let mut touched_files: Vec<String> = Vec::new();
    for s in &run.steps {
        if matches!(
            s.kind,
            crate::state::ToolKind::Read | crate::state::ToolKind::Edit | crate::state::ToolKind::Write
        ) && !s.what.trim().is_empty()
            && !touched_files.iter().any(|p| p == &s.what)
        {
            touched_files.push(s.what.clone());
        }
    }

    let header_block = div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap(t.space.md)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(t.text.subheading)
                        .line_height(relative(1.3))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(run.task.clone()),
                )
                .when(status == ToolStatus::Running && !run.stopping, |d| {
                    d.child(
                        stop_button(t, "agent-head-stop", "Stop").on_click(move |_, _, cx| {
                            stopper.update(cx, |s, cx| s.stop_agent(&stop_id, cx));
                        }),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(t.space.sm)
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .when(!run.kind.is_empty(), |d| {
                    d.child(
                        div()
                            .px(t.space.sm)
                            .py(px(1.))
                            .rounded(t.radius.small)
                            .bg(t.colors.hover)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text)
                            .child(run.kind.clone()),
                    )
                })
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(status_color)
                        .child(status_text),
                )
                .when(!run.steps.is_empty(), |d| {
                    d.child("·").child(super::agent_roster::step_summary(
                        done_count,
                        fail_count,
                        run.steps.len(),
                    ))
                })
                .when(fail_count > 0, |d| {
                    d.child(
                        div()
                            .px(t.space.xs + px(2.))
                            .py(px(1.))
                            .rounded(t.radius.small)
                            .bg(t.extra.danger_tint)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.extra.danger)
                            .child(format!("{fail_count} failed")),
                    )
                }),
        )
        .when(!touched_files.is_empty(), |d| {
            let extra = touched_files.len().saturating_sub(3);
            d.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(4.))
                    .children(touched_files.iter().take(3).map(|path| {
                        div()
                            .px(t.space.xs + px(2.))
                            .py(px(1.))
                            .rounded(t.radius.small)
                            .bg(t.extra.sunk)
                            .border_1()
                            .border_color(t.colors.border)
                            .font_family(t.font_mono.clone())
                            .text_size(t.text.caption - px(0.5))
                            .text_color(t.colors.text_muted)
                            .child(path.clone())
                    }))
                    .when(extra > 0, |d| {
                        d.child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(format!("+{extra} more")),
                        )
                    }),
            )
        });

    // ---- 3. Answer First: Final Response (Prominent above Steps when Ready) ----
    let last_text_in_entries = run.entries.iter().rev().find_map(|e| match e {
        AgentEntry::Text(txt) => Some(txt.trim()),
        AgentEntry::Step(_) => None,
    });
    let norm = |s: &str| {
        s.replace('`', "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let said_trimmed = run.said.trim();
    let has_final_answer = !said_trimmed.is_empty();
    let final_matches_last_entry =
        has_final_answer && last_text_in_entries.map(norm) == Some(norm(said_trimmed));

    let final_answer = has_final_answer.then(|| {
        div()
            .p(t.space.md)
            .rounded(t.radius.card)
            .border_1()
            .border_color(if status == ToolStatus::Failed {
                t.extra.danger.opacity(0.45)
            } else {
                t.extra.border_strong
            })
            .bg(if status == ToolStatus::Failed {
                t.extra.danger_tint.opacity(0.35)
            } else {
                t.colors.surface
            })
            .flex()
            .flex_col()
            .gap(t.space.xs + px(2.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .child(icon(
                        if status == ToolStatus::Failed {
                            "icons/triangle-alert.svg"
                        } else {
                            "icons/message-square.svg"
                        },
                        t.size.icon_xs,
                        if status == ToolStatus::Failed {
                            t.extra.danger
                        } else {
                            t.colors.primary
                        },
                    ))
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if status == ToolStatus::Failed {
                                t.extra.danger
                            } else {
                                t.colors.text_strong
                            })
                            .child(if status == ToolStatus::Running {
                                "Latest update"
                            } else if status == ToolStatus::Failed {
                                "Failure summary"
                            } else {
                                "Final response"
                            }),
                    ),
            )
            .child(markdown::render_as(
                &run.said,
                t,
                &CopyContext {
                    state: state.clone(),
                    key: format!("agent-{}-final", run.id),
                    copied: v.copied.clone(),
                    scale: v.scale,
                    sources: Default::default(),
                    zooms: Default::default(),
                    streaming: false,
                },
                Look::Chat,
            ))
    });

    // ---- 4. Collapsible "Message from parent agent" Card ----
    let prompt_open = open.contains(&run.id);
    let prompt_toggle = state.clone();
    let prompt_id = run.id.clone();
    let prompt_hover = t.colors.hover;
    let prompt_card = (!run.prompt.trim().is_empty()).then(|| {
        let header = super::button::KeyRow::new(
            SharedString::from(format!("agent-prompt-{}", run.id)),
            SharedString::from(format!(
                "Message from parent agent, {}",
                if prompt_open { "expanded" } else { "collapsed" }
            )),
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .px(t.space.md)
                .py(t.space.xs + px(2.))
                .hover(move |s| s.bg(prompt_hover))
                .child(icon(
                    "icons/message-square.svg",
                    t.size.icon_xs,
                    t.colors.text_muted,
                ))
                .child(
                    div()
                        .flex_none()
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text)
                        .child("Parent instruction"),
                )
                .when(!prompt_open, |d| {
                    d.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(one_line(&run.prompt)),
                    )
                })
                .when(prompt_open, |d| d.child(div().flex_1()))
                .child(icon(
                    if prompt_open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    t.size.icon_xs,
                    t.colors.text_subtle,
                )),
        )
        .w_full()
        .expanded(prompt_open)
        .on_activate(move |_, cx| {
            prompt_toggle.update(cx, |s, cx| s.toggle_agent(&prompt_id, cx));
        });

        div()
            .rounded(t.radius.card)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.extra.inset)
            .overflow_hidden()
            .child(header)
            .when(prompt_open, |d| {
                d.child(
                    div()
                        .border_t_1()
                        .border_color(t.colors.border)
                        .px(t.space.md)
                        .py(t.space.sm + px(2.))
                        .text_size(t.text.body_sm)
                        .line_height(relative(1.5))
                        .text_color(t.colors.text)
                        .child(run.prompt.trim().to_string()),
                )
            })
    });

    // ---- 5. Grouped Activity Stream (Every Failure Always Exposed) ----
    let items = build_inspector_items(run, has_final_answer);
    let mut stream_items: Vec<AnyElement> = vec![];
    for item in items {
        match item {
            InspectorItem::Text { entry_ix } => {
                // Avoid duplicating the Final Response block shown above.
                if final_matches_last_entry && entry_ix + 1 == run.entries.len() {
                    continue;
                }
                if let AgentEntry::Text(text) = &run.entries[entry_ix] {
                    let is_last = entry_ix + 1 == run.entries.len();
                    stream_items.push(
                        div()
                            .px(px(2.))
                            .child(markdown::render_as(
                                text,
                                t,
                                &CopyContext {
                                    state: state.clone(),
                                    key: format!("agent-{}-txt-{entry_ix}", run.id),
                                    copied: v.copied.clone(),
                                    scale: v.scale,
                                    sources: Default::default(),
                                    zooms: Default::default(),
                                    streaming: status == ToolStatus::Running && is_last,
                                },
                                Look::Aside,
                            ))
                            .into_any_element(),
                    );
                }
            }
            InspectorItem::TextBatch { entry_indices } => {
                // Filter out an entry that duplicates final_answer.
                let filtered: Vec<usize> = entry_indices
                    .into_iter()
                    .filter(|&ei| !(final_matches_last_entry && ei + 1 == run.entries.len()))
                    .collect();
                if filtered.is_empty() {
                    continue;
                }
                let first_ei = filtered[0];
                let note_key = format!("agent-notes-{}-{first_ei}", run.id);
                let notes_open = ui_toggles.contains(&note_key);
                let notes_hover = t.colors.hover;
                let note_state = state.clone();
                let note_toggle_key = note_key.clone();
                let count = filtered.len();
                let label = format!(
                    "Intermediate reasoning ({count} note{})",
                    if count == 1 { "" } else { "s" }
                );
                let note_header = super::button::KeyRow::new(
                    SharedString::from(format!("agent-notes-btn-{}-{first_ei}", run.id)),
                    SharedString::from(format!(
                        "{label}, {}",
                        if notes_open { "expanded" } else { "collapsed" }
                    )),
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .min_h(px(28.))
                        .px(t.space.sm)
                        .hover(move |s| s.bg(notes_hover))
                        .child(icon("icons/lightbulb.svg", t.size.icon_xs, t.colors.text_subtle))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(label),
                        )
                        .child(icon(
                            if notes_open {
                                "icons/chevron-down.svg"
                            } else {
                                "icons/chevron-right.svg"
                            },
                            t.size.icon_xs,
                            t.colors.text_subtle,
                        )),
                )
                .w_full()
                .expanded(notes_open)
                .on_activate(move |_, cx| {
                    note_state.update(cx, |s, cx| s.toggle_ui(note_toggle_key.clone(), None, cx));
                });

                stream_items.push(
                    div()
                        .rounded(t.radius.compact)
                        .border_1()
                        .border_color(t.colors.border)
                        .overflow_hidden()
                        .child(note_header)
                        .when(notes_open, |d| {
                            d.child(
                                div()
                                    .border_t_1()
                                    .border_color(t.colors.border)
                                    .bg(t.extra.inset)
                                    .px(t.space.sm + px(2.))
                                    .py(t.space.sm)
                                    .flex()
                                    .flex_col()
                                    .gap(t.space.xs)
                                    .children(filtered.iter().filter_map(|&ei| {
                                        let AgentEntry::Text(text) = &run.entries[ei] else {
                                            return None;
                                        };
                                        Some(markdown::render_as(
                                            text,
                                            t,
                                            &CopyContext {
                                                state: state.clone(),
                                                key: format!("agent-{}-note-{ei}", run.id),
                                                copied: v.copied.clone(),
                                                scale: v.scale,
                                                sources: Default::default(),
                                                zooms: Default::default(),
                                                streaming: false,
                                            },
                                            Look::Aside,
                                        ))
                                    })),
                            )
                        })
                        .into_any_element(),
                );
            }
            InspectorItem::Step { step_idx } => {
                if let Some(step) = run.steps.get(step_idx) {
                    stream_items.push(render_step(
                        run,
                        step_idx,
                        step,
                        ui_toggles,
                        state.clone(),
                        t,
                    ));
                }
            }
            InspectorItem::StepGroup { kind, step_indices } => {
                let first_si = step_indices[0];
                let grp_key = format!("agent-grp-{}-{first_si}", run.id);
                let grp_open = ui_toggles.contains(&grp_key);
                let count = step_indices.len();
                let label = kind_group_label(kind, count);
                let total_secs: u64 = step_indices
                    .iter()
                    .filter_map(|&si| {
                        let s = run.steps.get(si)?;
                        Some(s.ended?.saturating_duration_since(s.started).as_secs())
                    })
                    .sum();
                let grp_hover = t.colors.hover;
                let grp_state = state.clone();
                let grp_toggle = grp_key.clone();
                let grp_header = super::button::KeyRow::new(
                    SharedString::from(format!("agent-grp-btn-{}-{first_si}", run.id)),
                    SharedString::from(format!(
                        "{label}, {}",
                        if grp_open { "expanded" } else { "collapsed" }
                    )),
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .min_h(px(30.))
                        .px(t.space.sm)
                        .py(px(4.))
                        .hover(move |s| s.bg(grp_hover))
                        .child(icon(kind.icon(), t.size.icon_xs, t.colors.text_muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.body_sm)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.colors.text)
                                .child(label),
                        )
                        .when(total_secs >= 1, |d| {
                            d.child(
                                div()
                                    .flex_none()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_subtle)
                                    .child(elapsed(Duration::from_secs(total_secs))),
                            )
                        })
                        .child(icon(
                            if grp_open {
                                "icons/chevron-down.svg"
                            } else {
                                "icons/chevron-right.svg"
                            },
                            t.size.icon_sm,
                            t.colors.text_subtle,
                        )),
                )
                .w_full()
                .expanded(grp_open)
                .on_activate(move |_, cx| {
                    grp_state.update(cx, |s, cx| s.toggle_ui(grp_toggle.clone(), None, cx));
                });

                stream_items.push(
                    div()
                        .rounded(t.radius.card)
                        .border_1()
                        .border_color(if grp_open {
                            t.colors.border
                        } else {
                            gpui::transparent_black()
                        })
                        .overflow_hidden()
                        .child(grp_header)
                        .when(grp_open, |d| {
                            d.child(
                                div()
                                    .border_t_1()
                                    .border_color(t.colors.border)
                                    .pl(t.space.sm)
                                    .py(t.space.xs)
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .children(step_indices.iter().filter_map(|&si| {
                                        let step = run.steps.get(si)?;
                                        Some(render_step(
                                            run,
                                            si,
                                            step,
                                            ui_toggles,
                                            state.clone(),
                                            t,
                                        ))
                                    })),
                            )
                        })
                        .into_any_element(),
                );
            }
        }
    }
    if stream_items.is_empty() && status == ToolStatus::Running {
        stream_items.push(
            div()
                .px(px(2.))
                .py(t.space.xs)
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .child("Starting sub-agent…")
                .into_any_element(),
        );
    }

    let activity_label = if !run.steps.is_empty() {
        Some(format!(
            "Activity ({} step{})",
            run.steps.len(),
            if run.steps.len() == 1 { "" } else { "s" }
        ))
    } else {
        None
    };

    let body_content = div()
        .w_full()
        .when(v.full, |d| d.max_w(t.size.column).mx_auto())
        .flex()
        .flex_col()
        .gap(t.space.sm + px(2.))
        .px(t.space.lg)
        .pt(t.space.md + px(2.))
        .pb(t.space.xxl)
        .child(header_block)
        // Answer first: show Final response / Failure summary prominently before scrolling through steps.
        .children(final_answer)
        .children(prompt_card)
        .when_some(activity_label, |d, title| {
            d.child(
                div()
                    .pt(t.space.xs)
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text_subtle)
                    .child(title),
            )
        })
        .when(!stream_items.is_empty(), |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(stream_items),
            )
        });

    // ---- 5. Bottom Grounded Footer Bar ----
    let has_copyable = !run.said.trim().is_empty()
        || run.steps.iter().any(|s| !s.output.trim().is_empty());
    let copy_key = format!("agent-{}", run.id);
    let did_copy = v.copied.as_deref() == Some(copy_key.as_str());
    let copier = state.clone();
    let copy_id = run.id.clone();
    let stop_all_state = state;
    let copy_hover = t.colors.hover;

    let footer_summary = if running > 0 {
        if v.runs.len() > 1 {
            super::agent_roster::Counts::of(v.runs).running_line()
        } else {
            super::agent_roster::status_line(ToolStatus::Running, run.steps.len(), Some(took))
        }
    } else if v.runs.len() > 1 {
        super::agent_roster::agent_line(active_idx, v.runs.len(), run.steps.len(), took)
    } else {
        super::agent_roster::status_line(super::agent_roster::status(run), run.steps.len(), Some(took))
    };

    let footer = div()
        .flex_none()
        .h(px(42.))
        .px(t.space.md)
        .border_t_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .flex()
        .items_center()
        .gap(t.space.sm)
        .child(run_status_icon(
            status,
            t,
            t.size.icon_sm,
            format!("agent-foot-spin-{}", run.id),
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(footer_summary),
        )
        .when(has_copyable, |d| {
            d.child(
                div()
                    .id(SharedString::from(format!("agent-copy-{}", run.id)))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .h(px(26.))
                    .px(t.space.sm + px(2.))
                    .rounded(t.radius.compact)
                    .border_1()
                    .border_color(t.extra.border_strong)
                    .bg(t.colors.surface)
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if did_copy {
                        t.extra.success
                    } else {
                        t.colors.text
                    })
                    .cursor_pointer()
                    .hover(move |s| s.bg(copy_hover))
                    .child(icon(
                        if did_copy {
                            "icons/check.svg"
                        } else {
                            "icons/copy.svg"
                        },
                        t.size.icon_xs,
                        if did_copy {
                            t.extra.success
                        } else {
                            t.colors.text_muted
                        },
                    ))
                    .child(if did_copy { "Copied" } else { "Copy output" })
                    .on_click(move |_, _, cx| {
                        copier.update(cx, |s, cx| s.copy_agent_output(&copy_id, cx));
                    }),
            )
        })
        .when(
            stoppable > 1 || (stoppable == 1 && status != ToolStatus::Running),
            |d| {
                let label = if stoppable > 1 {
                    "Stop all"
                } else {
                    "Stop running"
                };
                d.child(
                    stop_button(t, "agent-footer-stop-all", label)
                        .tooltip(|w, cx| {
                            gpui::component::tooltip::Tooltip::new(
                                "Stop every running agent; KodAct carries on with what they found (Ctrl+Shift+Esc)",
                            )
                            .build(w, cx)
                        })
                        .on_click(move |_, _, cx| {
                            stop_all_state.update(cx, |s, cx| s.stop_agents(cx));
                        }),
                )
            },
        );

    div()
        .flex_1()
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .child(switcher)
        .child(
            div()
                .id("agents-feed-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body_content),
        )
        .child(footer)
        .into_any_element()
}

fn render_step(
    run: &AgentRun,
    step_idx: usize,
    step: &AgentStep,
    ui_toggles: &std::collections::HashSet<String>,
    state: Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    use crate::state::ToolKind;

    let running = step.status == ToolStatus::Running;
    let failed = step.status == ToolStatus::Failed;
    let interrupted = step.status == ToolStatus::Interrupted;
    let is_problem = failed || interrupted;
    let has_detail = !step.detail.trim().is_empty() || !step.output.trim().is_empty();
    let fold_key = format!("agent-step-fold-{}-{step_idx}", run.id);
    let shown_open = has_detail
        && if is_problem {
            !ui_toggles.contains(&fold_key)
        } else {
            step.open
        };
    let step_hover = t.colors.hover;
    let agent_id = run.id.clone();

    let border_color = if running {
        t.colors.primary
    } else if failed {
        t.extra.danger.opacity(0.45)
    } else if interrupted {
        t.extra.warning.opacity(0.45)
    } else if shown_open {
        t.colors.border
    } else {
        gpui::transparent_black()
    };

    let left_icon = if running {
        spinner(
            t.size.icon_sm,
            t.colors.primary,
            format!("agent-{}-step-spin-{step_idx}", run.id),
        )
    } else if failed {
        icon(step.kind.icon(), t.size.icon_sm, t.extra.danger).into_any_element()
    } else if interrupted {
        icon("icons/triangle-alert.svg", t.size.icon_sm, t.extra.warning).into_any_element()
    } else {
        icon(step.kind.icon(), t.size.icon_sm, t.colors.text_muted).into_any_element()
    };

    let dur = step
        .ended
        .map(|e| e.saturating_duration_since(step.started))
        .filter(|d| d.as_secs() >= 1)
        .map(elapsed);

    let out_trimmed = step.output.trim();
    let out_lines = if out_trimmed.is_empty() {
        0
    } else {
        out_trimmed.lines().count()
    };
    let status_note = if failed {
        Some("failed".to_string())
    } else if interrupted {
        Some("stopped".to_string())
    } else if step.status == ToolStatus::Done && out_lines > 0 {
        Some(format!(
            "{out_lines} line{}",
            if out_lines == 1 { "" } else { "s" }
        ))
    } else {
        None
    };

    let mono_what = !step.described
        && matches!(
            step.kind,
            ToolKind::Read | ToolKind::Edit | ToolKind::Write | ToolKind::Search | ToolKind::Bash
        );
    let label_color = if running {
        t.colors.text_strong
    } else if failed {
        t.extra.danger
    } else if interrupted {
        t.extra.warning
    } else {
        t.colors.text
    };

    let title_row = if step.described {
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(t.text.body_sm)
            .font_weight(if running || is_problem {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .text_color(label_color)
            .child(step.what.clone())
    } else {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_baseline()
            .gap(t.space.xs + px(1.))
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .text_size(t.text.body_sm)
                    .font_weight(if running || is_problem {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if is_problem {
                        label_color
                    } else {
                        t.colors.text_muted
                    })
                    .child(step.kind.verb(step.status)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .when(mono_what, |d| d.font_family(t.font_mono.clone()))
                    .text_size(if mono_what {
                        t.text.caption + px(0.5)
                    } else {
                        t.text.body_sm
                    })
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(label_color)
                    .child(step.what.clone()),
            )
    };

    let header_inner = div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .min_h(px(30.))
        .px(t.space.sm)
        .py(px(4.))
        .when(has_detail, |d| d.hover(move |s| s.bg(step_hover)))
        .child(left_icon)
        .child(title_row)
        .when_some(status_note.clone(), |d, note| {
            d.child(
                div()
                    .flex_none()
                    .text_size(t.text.caption)
                    .font_weight(if is_problem {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if failed {
                        t.extra.danger
                    } else if interrupted {
                        t.extra.warning
                    } else {
                        t.colors.text_subtle
                    })
                    .child(note),
            )
        })
        .when_some(dur.clone(), |d, dt| {
            d.child(
                div()
                    .flex_none()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(dt),
            )
        })
        .when(has_detail, |d| {
            d.child(icon(
                if shown_open {
                    "icons/chevron-down.svg"
                } else {
                    "icons/chevron-right.svg"
                },
                t.size.icon_sm,
                t.colors.text_subtle,
            ))
        });

    let aria = format!(
        "{}{}{}",
        step.label(),
        status_note
            .as_deref()
            .map(|n| format!(", {n}"))
            .unwrap_or_default(),
        if !has_detail {
            ""
        } else if shown_open {
            ", expanded"
        } else {
            ", collapsed"
        }
    );
    let mut header = super::button::KeyRow::new(
        SharedString::from(format!("agent-{}-step-{step_idx}", run.id)),
        aria,
        header_inner,
    )
    .w_full();
    if has_detail {
        let step_state = state.clone();
        let fold_toggle = fold_key.clone();
        header = header.expanded(shown_open).on_activate(move |_, cx| {
            step_state.update(cx, |s, cx| {
                if is_problem {
                    s.toggle_ui(fold_toggle.clone(), None, cx);
                } else {
                    s.toggle_agent_step(&agent_id, step_idx, cx);
                }
            });
        });
    }

    const AGENT_FOLD_LINES: usize = 10;
    let detail_block = (shown_open && has_detail).then(|| {
        let has_det = !step.detail.trim().is_empty();
        let has_out = !step.output.trim().is_empty();
        let det_key = format!("agent-step-det-{}-{step_idx}", run.id);
        let det_expanded = ui_toggles.contains(&det_key);
        let det_trunc = super::truncate_lines(&step.detail, AGENT_FOLD_LINES);
        let shown_det = if det_trunc.truncated && !det_expanded {
            det_trunc.shown.to_string()
        } else {
            step.detail.trim().to_string()
        };

        let out_key = format!("agent-step-out-{}-{step_idx}", run.id);
        let out_expanded = ui_toggles.contains(&out_key);
        let out_trunc = super::truncate_lines(&step.output, AGENT_FOLD_LINES);
        let shown_out = if out_trunc.truncated && !out_expanded {
            out_trunc.shown.to_string()
        } else {
            step.output.trim().to_string()
        };

        div()
            .border_t_1()
            .border_color(t.colors.border)
            .bg(if failed {
                t.extra.danger_tint.opacity(0.35)
            } else if interrupted {
                t.extra.warning_tint.opacity(0.35)
            } else {
                t.extra.inset
            })
            .flex()
            .flex_col()
            .when(has_det, |d| {
                let st_det = state.clone();
                let det_lbl = if det_trunc.hidden_lines > 0 {
                    format!("Show {} more lines", det_trunc.hidden_lines)
                } else {
                    "Show full command".to_string()
                };
                d.child(
                    div()
                        .px(t.space.sm + px(2.))
                        .py(t.space.xs + px(2.))
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption)
                        .line_height(relative(1.45))
                        .text_color(t.colors.text_muted)
                        .child(shown_det),
                )
                .when(det_trunc.truncated, |d| {
                    d.child(super::message::show_more_toggle(
                        format!("agent-step-det-btn-{}-{step_idx}", run.id),
                        det_key,
                        det_lbl,
                        None,
                        det_expanded,
                        st_det,
                        t,
                    ))
                })
            })
            .when(has_out, |d| {
                let st_out = state.clone();
                let out_lbl = if out_trunc.hidden_lines > 0 {
                    format!("Show {} more lines", out_trunc.hidden_lines)
                } else {
                    "Show full output".to_string()
                };
                d.child(
                    div()
                        .when(has_det, |d| d.border_t_1().border_color(t.colors.border))
                        .px(t.space.sm + px(2.))
                        .py(t.space.sm)
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption)
                        .line_height(relative(1.5))
                        .text_color(t.colors.text)
                        .child(shown_out),
                )
                .when(out_trunc.truncated, |d| {
                    d.child(super::message::show_more_toggle(
                        format!("agent-step-out-btn-{}-{step_idx}", run.id),
                        out_key,
                        out_lbl,
                        None,
                        out_expanded,
                        st_out,
                        t,
                    ))
                })
            })
    });

    div()
        .rounded(t.radius.card)
        .border_1()
        .border_color(border_color)
        .when(running, |d| d.bg(t.colors.primary_tint.opacity(0.3)))
        .overflow_hidden()
        .child(header)
        .children(detail_block)
        .into_any_element()
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ToolKind;

    fn mk_step(id: &str, kind: ToolKind, what: &str, status: ToolStatus) -> AgentStep {
        AgentStep {
            id: id.to_string(),
            kind,
            what: what.to_string(),
            described: false,
            detail: String::new(),
            output: String::new(),
            started: Instant::now(),
            ended: (status != ToolStatus::Running).then(Instant::now),
            status,
            open: false,
        }
    }

    fn mk_run(steps: Vec<AgentStep>, entries: Vec<AgentEntry>) -> AgentRun {
        AgentRun {
            id: "a1".to_string(),
            task: "Inspect repo".to_string(),
            kind: "Explore".to_string(),
            prompt: String::new(),
            steps,
            entries,
            said: "All good.".to_string(),
            started: Instant::now(),
            heard: Instant::now(),
            ended: Some(Instant::now()),
            failed: false,
            stopping: false,
            stopped: false,
        }
    }

    #[test]
    fn times_read_short() {
        assert_eq!(elapsed(Duration::from_secs(8)), "8s");
        assert_eq!(elapsed(Duration::from_secs(64)), "1m 04s");
        assert_eq!(elapsed(Duration::from_secs(3720)), "1h 02m");
    }

    #[test]
    fn groups_consecutive_done_steps_and_preserves_failures() {
        let steps = vec![
            mk_step("1", ToolKind::Read, "a.rs", ToolStatus::Done),
            mk_step("2", ToolKind::Read, "b.rs", ToolStatus::Done),
            mk_step("3", ToolKind::Read, "c.rs", ToolStatus::Done),
            mk_step("4", ToolKind::Bash, "cargo test", ToolStatus::Failed),
            mk_step("5", ToolKind::Read, "d.rs", ToolStatus::Done),
        ];
        let entries = vec![
            AgentEntry::Text("Checking files".into()),
            AgentEntry::Text("More checks".into()),
            AgentEntry::Step(0),
            AgentEntry::Step(1),
            AgentEntry::Step(2),
            AgentEntry::Step(3),
            AgentEntry::Step(4),
        ];
        let run = mk_run(steps, entries);
        let items = build_inspector_items(&run, true);
        assert_eq!(
            items,
            vec![
                InspectorItem::TextBatch {
                    entry_indices: vec![0, 1]
                },
                InspectorItem::StepGroup {
                    kind: ToolKind::Read,
                    step_indices: vec![0, 1, 2],
                },
                InspectorItem::Step { step_idx: 3 },
                InspectorItem::Step { step_idx: 4 },
            ]
        );
    }

    #[test]
    fn exposes_warning_notes_outside_collapsed_batches() {
        let steps = vec![mk_step("1", ToolKind::Read, "a.rs", ToolStatus::Done)];
        let entries = vec![
            AgentEntry::Text("Reading configs".into()),
            AgentEntry::Text("Error: missing token".into()),
            AgentEntry::Step(0),
        ];
        let run = mk_run(steps, entries);
        let items = build_inspector_items(&run, true);
        assert_eq!(
            items,
            vec![
                InspectorItem::TextBatch {
                    entry_indices: vec![0]
                },
                InspectorItem::Text { entry_ix: 1 },
                InspectorItem::Step { step_idx: 0 },
            ]
        );
    }

    #[test]
    fn filters_ten_or_more_agents_by_status() {
        let mut runs = Vec::new();
        for i in 0..10 {
            let mut r = mk_run(vec![], vec![]);
            r.id = format!("a{i}");
            r.task = format!("Agent {i}");
            if i < 3 {
                r.ended = None;
            } else if i == 7 || i == 9 {
                r.failed = true;
            }
            runs.push(r);
        }
        // The jumper's filter, kept in the panel's toggles, picks the list.
        use super::super::agent_jumper::filter;
        use super::super::agent_roster::listed;
        let mut toggles = std::collections::HashSet::new();
        assert_eq!(listed(&runs, filter(&toggles)).len(), 10);

        toggles.insert("agent-filter:running".into());
        assert_eq!(listed(&runs, filter(&toggles)), vec![0, 1, 2]);

        toggles.remove("agent-filter:running");
        toggles.insert("agent-filter:failed".into());
        assert_eq!(listed(&runs, filter(&toggles)), vec![7, 9]);
    }
}
