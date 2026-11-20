//! A run of tool calls as one line, as Claude Desktop shows its work: the
//! calls in a row (and the thinking between them) fold into a summary,
//! "Edited 6 files, ran 2 commands", with the change's size and any
//! failures; while it runs, the step in progress. It opens to the steps, a
//! quiet line each, and each step opens to its change or output. Text KodAct
//! writes to you ends a run, and anything waiting on you stands alone.

use std::ops::Range;

use gpui::{div, prelude::*, px, AnyElement, SharedString};

use super::icon;
use super::message::{self, ItemContext};
use crate::state::{Block, DiffKind, ToolCall, ToolKind, ToolStatus};
use crate::theme::Theme;

/// How many calls a run needs to fold into one line.
const MIN_CALLS: usize = 2;

/// What block `b` is to a run.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Part {
    /// A step of the run: a call, or finished thinking.
    Step,
    /// Shown elsewhere, or not at all: a call's allowed answer (shown under
    /// it), a call whose denial or question card stands for it.
    Hidden,
    /// Ends the run: text, a card, anything waiting on you.
    Breaks,
}

fn part(blocks: &[Block], b: usize, streaming: bool) -> Part {
    match &blocks[b] {
        // The task list's card stands on its own; the reply's later lists
        // only update it, so they don't break a run either.
        Block::Tool(t) if super::todo_card::is_todo(t) => {
            if super::todo_card::slot(blocks, b).is_some() {
                Part::Breaks
            } else {
                Part::Hidden
            }
        }
        Block::Tool(_) if message::awaits_you(blocks, b) => Part::Breaks,
        Block::Tool(_) if message::denied_call(blocks, b) || message::asked_call(blocks, b) => {
            Part::Hidden
        }
        Block::Tool(_) => Part::Step,
        // The thought still being written stays in view.
        Block::Thinking { .. } if streaming && b + 1 == blocks.len() => Part::Breaks,
        Block::Thinking { .. } => Part::Step,
        Block::Permission { .. } if message::folded_answer(blocks, b) => Part::Hidden,
        _ => Part::Breaks,
    }
}

/// The runs of a reply that fold into one line: each the longest stretch of
/// steps with at least `MIN_CALLS` calls. `streaming`: the last block is
/// still being written.
pub fn runs(blocks: &[Block], streaming: bool) -> Vec<Range<usize>> {
    let mut out = vec![];
    let mut b = 0;
    while b < blocks.len() {
        if part(blocks, b, streaming) != Part::Step {
            b += 1;
            continue;
        }
        let start = b;
        while b < blocks.len() && part(blocks, b, streaming) != Part::Breaks {
            b += 1;
        }
        // Trailing hidden blocks belong to whatever comes next.
        let mut end = b;
        while end > start && part(blocks, end - 1, streaming) == Part::Hidden {
            end -= 1;
        }
        let calls = (start..end)
            .filter(|&i| {
                matches!(blocks[i], Block::Tool(_)) && part(blocks, i, streaming) == Part::Step
            })
            .count();
        if calls >= MIN_CALLS {
            out.push(start..end);
        }
        b = b.max(end).max(start + 1);
    }
    out
}

/// The calls a run shows, with their block index.
fn calls<'a>(
    blocks: &'a [Block],
    run: &Range<usize>,
    streaming: bool,
) -> Vec<(usize, &'a ToolCall)> {
    run.clone()
        .filter(|&i| part(blocks, i, streaming) == Part::Step)
        .filter_map(|i| match &blocks[i] {
            Block::Tool(c) => Some((i, c)),
            _ => None,
        })
        .collect()
}

/// The run in words: "Edited 6 files, read 3 files, ran 2 commands". Files
/// count once however often they were touched; what failed or was skipped
/// counts apart.
pub fn summary(calls: &[&ToolCall]) -> String {
    use std::collections::HashSet;
    let ok: Vec<&&ToolCall> = calls
        .iter()
        .filter(|c| {
            !matches!(
                c.status,
                ToolStatus::Failed | ToolStatus::Declined | ToolStatus::Interrupted
            )
        })
        .collect();
    let files = |kinds: &[ToolKind]| {
        ok.iter()
            .filter(|c| kinds.contains(&c.kind))
            .map(|c| c.target.as_ref())
            .collect::<HashSet<_>>()
            .len()
    };
    let count = |kinds: &[ToolKind]| ok.iter().filter(|c| kinds.contains(&c.kind)).count();
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            one.to_string()
        } else {
            format!("{n} {many}")
        }
    };
    let mut parts = vec![];
    let edited = files(&[ToolKind::Edit, ToolKind::Write]);
    if edited > 0 {
        parts.push(format!("edited {}", plural(edited, "a file", "files")));
    }
    let read = files(&[ToolKind::Read]);
    if read > 0 {
        parts.push(format!("read {}", plural(read, "a file", "files")));
    }
    let ran = count(&[ToolKind::Bash]);
    if ran > 0 {
        parts.push(format!("ran {}", plural(ran, "a command", "commands")));
    }
    let searched = count(&[ToolKind::Search]);
    if searched > 0 {
        parts.push(match searched {
            1 => "searched once".to_string(),
            n => format!("searched {n} times"),
        });
    }
    let web = count(&[ToolKind::Web, ToolKind::WebSearch]);
    if web > 0 {
        parts.push(match web {
            1 => "used the web once".to_string(),
            n => format!("used the web {n} times"),
        });
    }
    // Sub-agents are named as agents, not as "a tool", and every agent
    // counts (as the Agents panel counts them); its failures are added
    // apart, beside the line.
    let agents = calls
        .iter()
        .filter(|c| crate::state::is_agent_tool(&c.target))
        .count();
    if agents > 0 {
        parts.push(format!("ran {}", plural(agents, "an agent", "agents")));
    }
    let ok_agents = ok
        .iter()
        .filter(|c| crate::state::is_agent_tool(&c.target))
        .count();
    let other = count(&[ToolKind::Other]) - ok_agents;
    if other > 0 {
        parts.push(format!("used {}", plural(other, "a tool", "tools")));
    }
    if parts.is_empty() {
        parts.push(format!("tried {}", plural(calls.len(), "a step", "steps")));
    }
    let text = parts.join(", ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

/// A call as the live line says it: "Editing src/store.rs", or a command's
/// description.
fn step_label(c: &ToolCall) -> String {
    if crate::state::is_agent_tool(&c.target) {
        return format!("Agent: {}", c.note.as_deref().unwrap_or("a sub-task"));
    }
    match (&c.note, c.kind) {
        (Some(n), ToolKind::Bash) => n.to_string(),
        _ => format!("{} {}", c.kind.verb(c.status), c.shown_target()),
    }
}

/// Category used for grouping consecutive `Done` steps inside an expanded `work_group`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SubKind {
    Read,
    Edit,
    Write,
    Bash,
    Search,
    Web,
    WebSearch,
    Agent,
    Other,
}

impl SubKind {
    fn of(c: &ToolCall) -> Self {
        if crate::state::is_agent_tool(&c.target) {
            return SubKind::Agent;
        }
        match c.kind {
            ToolKind::Read => SubKind::Read,
            ToolKind::Edit => SubKind::Edit,
            ToolKind::Write => SubKind::Write,
            ToolKind::Bash => SubKind::Bash,
            ToolKind::Search => SubKind::Search,
            ToolKind::Web => SubKind::Web,
            ToolKind::WebSearch => SubKind::WebSearch,
            ToolKind::Other => SubKind::Other,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            SubKind::Agent => "icons/boxes.svg",
            SubKind::Read => ToolKind::Read.icon(),
            SubKind::Edit => ToolKind::Edit.icon(),
            SubKind::Write => ToolKind::Write.icon(),
            SubKind::Bash => ToolKind::Bash.icon(),
            SubKind::Search => ToolKind::Search.icon(),
            SubKind::Web => ToolKind::Web.icon(),
            SubKind::WebSearch => ToolKind::WebSearch.icon(),
            SubKind::Other => ToolKind::Other.icon(),
        }
    }

    fn label(self, count: usize) -> String {
        match self {
            SubKind::Read => format!("Read {count} files"),
            SubKind::Edit => format!("Edited {count} files"),
            SubKind::Write => format!("Wrote {count} files"),
            SubKind::Bash => format!("Ran {count} commands"),
            SubKind::Search => format!("Searched {count} times"),
            SubKind::Web => format!("Fetched {count} pages"),
            SubKind::WebSearch => format!("Searched web {count} times"),
            SubKind::Agent => format!("Ran {count} agents"),
            SubKind::Other => format!("Used {count} tools"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum GroupedStep {
    Single(usize),
    Group { kind: SubKind, indices: Vec<usize> },
}

/// Group consecutive `>= 2` completed (`Done`) calls of the same `SubKind` into
/// one row with a count; active, failed, stopped, or individually opened items
/// always remain standalone.
fn group_steps(blocks: &[Block], shown: &[usize]) -> Vec<GroupedStep> {
    let eligible_kind = |i: usize| -> Option<SubKind> {
        let Block::Tool(c) = &blocks[i] else {
            return None;
        };
        if c.status != ToolStatus::Done
            || !c.images.is_empty()
            || message::answer_for(blocks, i).is_some()
            || c.open
        {
            return None;
        }
        Some(SubKind::of(c))
    };

    let mut out = Vec::new();
    let mut idx = 0;
    while idx < shown.len() {
        let block_i = shown[idx];
        let Some(kind) = eligible_kind(block_i) else {
            out.push(GroupedStep::Single(block_i));
            idx += 1;
            continue;
        };
        let mut end = idx + 1;
        while end < shown.len() && eligible_kind(shown[end]) == Some(kind) {
            end += 1;
        }
        if end - idx >= 2 {
            out.push(GroupedStep::Group {
                kind,
                indices: shown[idx..end].to_vec(),
            });
        } else {
            out.push(GroupedStep::Single(block_i));
        }
        idx = end;
    }
    out
}

/// Concise unique paths touched (preferring edited/written files, then read),
/// returned as up to 2 short labels + optional `+N` count.
fn touched_paths(calls: &[&ToolCall]) -> Option<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut paths: Vec<String> = Vec::new();
    let short_name = |p: &str| {
        let parts: Vec<&str> = p.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
        if parts.len() <= 2 {
            p.to_string()
        } else {
            format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
        }
    };
    for &kind_pass in &[true, false] {
        for c in calls {
            let is_edit = matches!(c.kind, ToolKind::Edit | ToolKind::Write);
            let is_read = c.kind == ToolKind::Read;
            if (kind_pass && !is_edit) || (!kind_pass && !is_read) {
                continue;
            }
            if c.status == ToolStatus::Failed {
                continue;
            }
            let name = short_name(c.target.as_ref());
            if seen.insert(name.clone()) {
                paths.push(name);
            }
        }
    }
    if paths.is_empty() {
        return None;
    }
    if paths.len() <= 2 {
        Some(paths.join(", "))
    } else {
        Some(format!("{}, {} +{}", paths[0], paths[1], paths.len() - 2))
    }
}

fn render_step_row(blocks: &[Block], i: usize, cx: &ItemContext, t: &Theme) -> Option<AnyElement> {
    match &blocks[i] {
        Block::Tool(c) => {
            let is_agent = crate::state::is_agent_tool(&c.target);
            let answer = message::answer_for(blocks, i);
            let step = message::flash(
                div()
                    .flex()
                    .flex_col()
                    .child(message::tool_header(c, i, blocks, cx, t))
                    .when(!is_agent, |d| d.children(message::tool_body(c, i, cx, t)))
                    .children(answer.map(|p| message::approval_line(&blocks[p], cx, t))),
                i,
                cx,
                t,
            );
            Some(message::mark(step, i, cx))
        }
        Block::Thinking { .. } => Some(message::block(blocks, i, cx, t)),
        _ => None,
    }
}

/// The run at `run` as one line that opens to its steps; failures and active
/// problems stay visibly exposed even while collapsed.
pub fn render(blocks: &[Block], run: Range<usize>, cx: &ItemContext, t: &Theme) -> AnyElement {
    let streaming = cx.busy && cx.last;
    let calls = calls(blocks, &run, streaming);
    let all: Vec<&ToolCall> = calls.iter().map(|(_, c)| *c).collect();
    // Collapsed by default unless Detailed mode is active (or toggled open).
    let open = cx.detailed != cx.open_groups.contains(&run.start);
    let failed = all
        .iter()
        .filter(|c| c.status == ToolStatus::Failed)
        .count();
    let stopped = all
        .iter()
        .filter(|c| c.status == ToolStatus::Interrupted)
        .count();
    let (adds, dels) = all
        .iter()
        .filter(|c| c.status != ToolStatus::Failed)
        .flat_map(|c| c.diff.iter())
        .fold((0, 0), |(a, d), l| match l.kind {
            DiffKind::Add => (a + 1, d),
            DiffKind::Del => (a, d + 1),
            DiffKind::Same => (a, d),
        });
    // Still growing: the reply runs and nothing has come after it yet.
    let live = streaming && run.end == blocks.len();
    let running = all.iter().rev().find(|c| c.status == ToolStatus::Running);
    let words = summary(&all);
    let (state, chat, msg, start) = (cx.state.clone(), cx.chat, cx.index, run.start);
    let strong = t.colors.text;
    let live_step = running.filter(|_| live);
    let agents_running = all
        .iter()
        .filter(|c| c.status == ToolStatus::Running && crate::state::is_agent_tool(&c.target))
        .count();
    let touched = (!open && live_step.is_none()).then(|| touched_paths(&all)).flatten();
    let pictures = message::thumbs(
        run.clone()
            .filter_map(|i| match &blocks[i] {
                Block::Tool(c) => Some(c.images.iter()),
                _ => None,
            })
            .flatten(),
        t,
    );

    // A run of sub-agents is counted as the Agents panel counts them
    // (`agent_roster`): "3 agents working", then "Ran 10 agents" with its
    // failures beside it, every agent included.
    let roster = super::agent_roster::Counts::of_calls(&all);
    let only_agents = roster.total > 0 && roster.total == all.len();
    let header_title = match live_step {
        Some(c) if crate::state::is_agent_tool(&c.target) && agents_running > 1 => {
            super::agent_roster::Counts { running: agents_running, ..roster }.run_line()
        }
        Some(c) => step_label(c),
        None if only_agents => roster.run_line(),
        None => words.clone(),
    };
    let header_icon: AnyElement = if let Some(c) = live_step {
        if crate::state::is_agent_tool(&c.target) {
            super::turning_logo_in(
                f32::from(t.size.icon_xs),
                format!("work-turn-{chat}-{msg}-{start}"),
                t.extra.live,
            )
        } else {
            super::tasks::status_icon(
                ToolStatus::Running,
                t,
                format!("work-spin-{chat}-{msg}-{start}").into(),
            )
        }
    } else if failed > 0 {
        icon("icons/circle-x.svg", t.size.icon_xs, t.extra.danger).into_any_element()
    } else if stopped > 0 {
        icon("icons/triangle-alert.svg", t.size.icon_xs, t.extra.warning).into_any_element()
    } else {
        icon("icons/circle-check.svg", t.size.icon_xs, t.extra.success).into_any_element()
    };

    let header_inner = div()
        .flex()
        .items_center()
        .gap(t.space.xs + px(2.))
        .min_h(t.size.quiet_line)
        .px(px(2.))
        .text_size(t.text.body_sm)
        .text_color(t.colors.text_muted)
        .hover(move |s| s.text_color(strong))
        .child(header_icon)
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_weight(if live_step.is_some() || failed > 0 {
                    gpui::FontWeight::MEDIUM
                } else {
                    gpui::FontWeight::NORMAL
                })
                .child(header_title.clone()),
        )
        .when_some(touched, |d, paths| {
            d.child(
                div()
                    .min_w_0()
                    .max_w(px(240.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(format!("· {paths}")),
            )
        })
        .when(failed > 0, |d| {
            d.child(
                div()
                    .flex_none()
                    .text_size(t.text.caption)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.extra.danger)
                    .child(format!("· {failed} failed")),
            )
        })
        .when(stopped > 0, |d| {
            d.child(
                div()
                    .flex_none()
                    .text_size(t.text.caption)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(t.extra.warning)
                    .child(format!("· {stopped} stopped")),
            )
        })
        .when(adds + dels > 0 && live_step.is_none(), |d| {
            d.child(message::diff_counts(adds, dels, t))
        })
        .child(icon(
            if open {
                "icons/chevron-down.svg"
            } else {
                "icons/chevron-right.svg"
            },
            t.size.icon_xs,
            t.colors.text_subtle,
        ))
        .when_some(pictures.filter(|_| !open), |d, th| {
            d.child(div().flex_1()).child(th)
        });

    let header = super::button::KeyRow::new(
        SharedString::from(format!("work-{chat}-{msg}-{start}")),
        SharedString::from(format!(
            "{header_title}{}{}, {}",
            if failed > 0 {
                format!(", {failed} failed")
            } else {
                String::new()
            },
            if stopped > 0 {
                format!(", {stopped} stopped")
            } else {
                String::new()
            },
            if open { "expanded" } else { "collapsed" }
        )),
        header_inner,
    )
    .w_full()
    .expanded(open)
    .on_activate(move |_, app| {
        state.update(app, |s, cx| s.toggle_group(chat, msg, start, cx));
    });

    let shown: Vec<usize> = run
        .clone()
        .filter(|&i| part(blocks, i, streaming) == Part::Step)
        .collect();

    // Never hide a failure or warning behind a collapsed group.
    let problem_rows: Vec<AnyElement> = if !open {
        shown
            .iter()
            .copied()
            .filter(|&i| {
                matches!(
                    &blocks[i],
                    Block::Tool(c) if matches!(c.status, ToolStatus::Failed | ToolStatus::Interrupted)
                )
            })
            .filter_map(|i| render_step_row(blocks, i, cx, t))
            .collect()
    } else {
        vec![]
    };
    let exposed_problems = (!problem_rows.is_empty()).then(|| {
        div()
            .flex()
            .flex_col()
            .gap(t.space.xs)
            .mt(t.space.xs)
            .children(problem_rows)
    });

    let grouped = group_steps(blocks, &shown);
    let mut step_rows: Vec<(super::heat_rail::Node, usize, AnyElement)> = Vec::new();
    for entry in grouped {
        match entry {
            GroupedStep::Single(i) => {
                if let Some(el) = render_step_row(blocks, i, cx, t) {
                    let node = match &blocks[i] {
                        Block::Tool(c) => super::heat_rail::Node::of(c.status),
                        _ => super::heat_rail::Node::Done,
                    };
                    step_rows.push((node, i, el));
                }
            }
            GroupedStep::Group { kind, indices } => {
                let first_i = indices[0];
                let count = indices.len();
                let sub_key = format!("subgrp-{chat}-{msg}-{start}-{first_i}");
                let sub_open = cx.ui_toggles.contains(&sub_key);
                let label = kind.label(count);
                let total_lines: usize = indices
                    .iter()
                    .filter_map(|&i| match &blocks[i] {
                        Block::Tool(c) if !c.output.trim().is_empty() => {
                            Some(c.output.trim_end().lines().count())
                        }
                        _ => None,
                    })
                    .sum();
                let (adds, dels) = indices
                    .iter()
                    .filter_map(|&i| match &blocks[i] {
                        Block::Tool(c) => Some(c.diff.iter()),
                        _ => None,
                    })
                    .flatten()
                    .fold((0, 0), |(a, d), l| match l.kind {
                        DiffKind::Add => (a + 1, d),
                        DiffKind::Del => (a, d + 1),
                        DiffKind::Same => (a, d),
                    });

                let st_sub = cx.state.clone();
                let sub_toggle = sub_key.clone();
                let sub_hover = t.colors.text;
                let group_header = super::button::KeyRow::new(
                    SharedString::from(format!("subgrp-btn-{chat}-{msg}-{start}-{first_i}")),
                    SharedString::from(format!(
                        "{label}, {}",
                        if sub_open { "expanded" } else { "collapsed" }
                    )),
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.xs + px(2.))
                        .min_h(t.size.quiet_line)
                        .px(px(2.))
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_muted)
                        .hover(move |s| s.text_color(sub_hover))
                        .child(icon(kind.icon(), t.size.icon_xs, t.colors.text_muted))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(t.colors.text)
                                .child(label),
                        )
                        .when(total_lines > 0 && adds + dels == 0, |d| {
                            d.child(
                                div()
                                    .flex_none()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_subtle)
                                    .child(format!("· {total_lines} lines")),
                            )
                        })
                        .when(adds + dels > 0, |d| d.child(message::diff_counts(adds, dels, t)))
                        .child(icon(
                            if sub_open {
                                "icons/chevron-down.svg"
                            } else {
                                "icons/chevron-right.svg"
                            },
                            t.size.icon_xs,
                            t.colors.text_subtle,
                        )),
                )
                .w_full()
                .expanded(sub_open)
                .on_activate(move |_, app| {
                    st_sub.update(app, |s, cx| s.toggle_ui(sub_toggle.clone(), Some(chat), cx));
                });

                let el = div()
                    .flex()
                    .flex_col()
                    .child(group_header)
                    .when(sub_open, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .pl(t.space.md)
                                .mt(px(2.))
                                .border_l_1()
                                .border_color(t.colors.border)
                                .children(indices.iter().filter_map(|&i| render_step_row(blocks, i, cx, t))),
                        )
                    })
                    .into_any_element();
                step_rows.push((super::heat_rail::Node::Done, first_i, el));
            }
        }
    }
    let count = step_rows.len();
    let steps = open.then(|| {
        div()
            .flex()
            .flex_col()
            .mt(t.space.xs)
            .rounded(t.radius.card)
            .border_1()
            .border_color(t.colors.border)
            .overflow_hidden()
            .children(step_rows.into_iter().enumerate().map(|(n, (node, i, row))| {
                div()
                    .flex()
                    .pl(t.space.sm)
                    .child(super::heat_rail::gutter(
                        node,
                        n == 0,
                        n + 1 == count,
                        live,
                        SharedString::from(format!("rail-{chat}-{msg}-{i}")),
                        t,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pl(t.space.sm)
                            .pr(t.space.md)
                            .py(px(3.))
                            .when(n > 0, |d| d.border_t_1().border_color(t.colors.border))
                            .child(row),
                    )
            }))
    });

    div()
        .flex()
        .flex_col()
        .child(header)
        .children(exposed_problems)
        .children(steps)
        .into_any_element()
}

/// The run holding block `b`, if one folds it.
pub fn run_of(blocks: &[Block], b: usize, streaming: bool) -> Option<Range<usize>> {
    runs(blocks, streaming).into_iter().find(|r| r.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DiffLine, PermInfo, Review, Verdict};

    fn call(kind: ToolKind, target: &str, status: ToolStatus) -> ToolCall {
        ToolCall {
            kind,
            target: target.into(),
            note: None,
            status,
            output: "".into(),
            diff: vec![],
            open: false,
            full: false,
            review: Review::Open,
            images: vec![],
        }
    }

    fn tool(kind: ToolKind, target: &str) -> Block {
        Block::Tool(call(kind, target, ToolStatus::Done))
    }

    fn think() -> Block {
        Block::Thinking {
            text: "hm".into(),
            open: false,
        }
    }

    #[test]
    fn the_task_list_is_never_a_tool_row() {
        use serde_json::json;
        let call = |name: &str, input: serde_json::Value| {
            Block::Tool(crate::backend::tool_call(name, &input))
        };
        let todo = |status: &str| {
            call(
                "TodoWrite",
                json!({"todos": [{"content": "Build it", "status": status}]}),
            )
        };
        let blocks = vec![
            call("Read", json!({"file_path": "a.rs"})),
            call("Read", json!({"file_path": "b.rs"})),
            todo("in_progress"),
            call("Read", json!({"file_path": "c.rs"})),
            todo("completed"),
            call("Read", json!({"file_path": "d.rs"})),
        ];
        // No list is ever a step a run folds or counts as a tool.
        for b in [2, 4] {
            assert_ne!(part(&blocks, b, false), Part::Step, "block {b}");
        }
        // The first holds the card and stands on its own; the later one only
        // updates it, so it neither shows nor splits the run around it.
        assert_eq!(part(&blocks, 2, false), Part::Breaks);
        assert_eq!(part(&blocks, 4, false), Part::Hidden);
        let runs = runs(&blocks, false);
        assert!(runs.iter().all(|r| !r.contains(&2)), "{runs:?}");
        for r in &runs {
            assert!(calls(&blocks, r, false)
                .iter()
                .all(|(_, c)| !super::super::todo_card::is_todo(c)));
        }
    }

    #[test]
    fn calls_in_a_row_fold_and_text_ends_a_run() {
        let blocks = vec![
            Block::Text("Looking.".into()),
            tool(ToolKind::Read, "a.rs"),
            think(),
            tool(ToolKind::Edit, "a.rs"),
            tool(ToolKind::Edit, "b.rs"),
            Block::Text("Now the tests.".into()),
            tool(ToolKind::Bash, "cargo test"),
            Block::Text("Done.".into()),
        ];
        // The lone command stays a row of its own.
        assert_eq!(runs(&blocks, false), vec![1..5]);
    }

    #[test]
    fn a_question_waiting_on_you_stands_alone() {
        let mut running = call(ToolKind::Edit, "c.rs", ToolStatus::Running);
        running.diff = vec![DiffLine {
            kind: DiffKind::Add,
            text: "x".into(),
        }];
        let blocks = vec![
            tool(ToolKind::Read, "a.rs"),
            tool(ToolKind::Read, "b.rs"),
            Block::Tool(running),
            Block::Permission {
                title: "Allow KodAct to edit c.rs?".into(),
                detail: "".into(),
                verdict: Verdict::Pending,
                info: PermInfo {
                    row: Some(2),
                    ..Default::default()
                },
            },
        ];
        assert_eq!(runs(&blocks, true), vec![0..2]);
    }

    #[test]
    fn the_thought_being_written_stays_in_view() {
        let blocks = vec![
            tool(ToolKind::Read, "a.rs"),
            tool(ToolKind::Read, "b.rs"),
            think(),
        ];
        assert_eq!(runs(&blocks, true), vec![0..2]);
        assert_eq!(runs(&blocks, false), vec![0..3]);
    }

    #[test]
    fn the_summary_counts_files_once_and_failures_apart() {
        let calls = [
            call(ToolKind::Write, "Cargo.toml", ToolStatus::Failed),
            call(ToolKind::Read, "Cargo.toml", ToolStatus::Done),
            call(ToolKind::Edit, "Cargo.toml", ToolStatus::Done),
            call(ToolKind::Edit, "src/lib.rs", ToolStatus::Done),
            call(ToolKind::Edit, "src/lib.rs", ToolStatus::Done),
            call(ToolKind::Bash, "cargo build", ToolStatus::Done),
            call(ToolKind::Bash, "cargo test", ToolStatus::Done),
            call(ToolKind::Search, "fn main", ToolStatus::Done),
        ];
        let refs: Vec<&ToolCall> = calls.iter().collect();
        assert_eq!(
            summary(&refs),
            "Edited 2 files, read a file, ran 2 commands, searched once"
        );
        let failed = [
            call(ToolKind::Write, "a", ToolStatus::Failed),
            call(ToolKind::Write, "b", ToolStatus::Failed),
        ];
        let refs: Vec<&ToolCall> = failed.iter().collect();
        assert_eq!(summary(&refs), "Tried 2 steps");
    }

    #[test]
    fn groups_consecutive_done_steps_and_keeps_failures_or_agents_distinct() {
        let mut agent1 = call(ToolKind::Other, "Task", ToolStatus::Done);
        agent1.note = Some("Explore repo".into());
        let mut agent2 = call(ToolKind::Other, "Agent", ToolStatus::Done);
        agent2.note = Some("Audit CSS".into());
        let blocks = vec![
            tool(ToolKind::Read, "src/a.rs"),
            tool(ToolKind::Read, "src/b.rs"),
            tool(ToolKind::Read, "src/c.rs"),
            Block::Tool(call(ToolKind::Bash, "cargo test", ToolStatus::Failed)),
            Block::Tool(agent1),
            Block::Tool(agent2),
            tool(ToolKind::Other, "lint"),
        ];
        // Sub-agents fold into the run alongside reads and failing commands.
        assert_eq!(runs(&blocks, false), vec![0..7]);
        let shown: Vec<usize> = (0..blocks.len()).collect();
        assert_eq!(
            group_steps(&blocks, &shown),
            vec![
                GroupedStep::Group {
                    kind: SubKind::Read,
                    indices: vec![0, 1, 2],
                },
                GroupedStep::Single(3),
                GroupedStep::Group {
                    kind: SubKind::Agent,
                    indices: vec![4, 5],
                },
                GroupedStep::Single(6),
            ]
        );
    }

    #[test]
    fn touched_paths_prefers_edits_then_reads_with_overflow_count() {
        let calls = [
            call(ToolKind::Read, "/repo/src/one.rs", ToolStatus::Done),
            call(ToolKind::Edit, "/repo/src/two.rs", ToolStatus::Done),
            call(ToolKind::Write, "/repo/src/three.rs", ToolStatus::Done),
            call(ToolKind::Read, "/repo/src/four.rs", ToolStatus::Done),
        ];
        let refs: Vec<&ToolCall> = calls.iter().collect();
        assert_eq!(
            touched_paths(&refs).as_deref(),
            Some("src/two.rs, src/three.rs +2")
        );
    }
}
