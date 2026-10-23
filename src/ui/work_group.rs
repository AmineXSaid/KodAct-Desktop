//! A run of tool calls as one line, as Claude Desktop shows its work: the
//! calls in a row (and the thinking between them) fold into a summary,
//! "Edited 6 files, ran 2 commands", with the change's size and any
//! failures; while it runs, the step in progress. It opens to the steps, a
//! quiet line each, and each step opens to its change or output. Text Forge
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
    let other = count(&[ToolKind::Other]);
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
    match (&c.note, c.kind) {
        (Some(n), ToolKind::Bash) => n.to_string(),
        _ => format!("{} {}", c.kind.verb(c.status), c.shown_target()),
    }
}

/// The run at `run` as one line that opens to its steps.
pub fn render(blocks: &[Block], run: Range<usize>, cx: &ItemContext, t: &Theme) -> AnyElement {
    let streaming = cx.busy && cx.last;
    let calls = calls(blocks, &run, streaming);
    let all: Vec<&ToolCall> = calls.iter().map(|(_, c)| *c).collect();
    // Detailed, a run starts open and a click closes it.
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
    let pictures = message::thumbs(
        run.clone()
            .filter_map(|i| match &blocks[i] {
                Block::Tool(c) => Some(c.images.iter()),
                _ => None,
            })
            .flatten(),
        t,
    );

    // One quiet line, as Claude shows its work: "Edited 7 files, ran 2
    // commands (1 failed) ›". While it works, the step in progress.
    let header = div()
        .id(SharedString::from(format!("work-{chat}-{msg}-{start}")))
        .role(gpui::accesskit::Role::Button)
        .aria_label(SharedString::from(format!(
            "{words}{}{}, {}",
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
            if open { "hide steps" } else { "show steps" }
        )))
        .flex()
        .items_center()
        .gap(t.space.xs + px(2.))
        .min_h(t.size.quiet_line)
        .text_size(t.text.body_sm)
        .text_color(t.colors.text_muted)
        .cursor_pointer()
        .hover(move |s| s.text_color(strong))
        .children(live_step.map(|_| {
            super::tasks::status_icon(
                ToolStatus::Running,
                t,
                format!("work-spin-{chat}-{msg}-{start}").into(),
            )
        }))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(match live_step {
                    Some(c) => step_label(c),
                    None => words.clone(),
                }),
        )
        .when(failed > 0, |d| {
            d.child(
                div()
                    .flex_none()
                    .text_color(t.extra.danger)
                    .child(format!("({failed} failed)")),
            )
        })
        // Stopped mid-way: their outcome is unknown, so they are counted apart.
        .when(stopped > 0, |d| {
            d.child(
                div()
                    .flex_none()
                    .text_color(t.extra.warning)
                    .child(format!("({stopped} stopped)")),
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
        // The pictures the run's calls returned, at the end of the line.
        // Opened, the rows show their own.
        .when_some(pictures.filter(|_| !open), |d, th| {
            d.child(div().flex_1()).child(th)
        })
        .on_click(move |_, _, app| state.update(app, |s, cx| s.toggle_group(chat, msg, start, cx)));

    // Opened: the steps in one framed list, a row each between hairlines;
    // a row opens in place to its change, output or pictures.
    let steps = open.then(|| {
        div()
            .flex()
            .flex_col()
            .mt(t.space.xs)
            .rounded(t.radius.card)
            .border_1()
            .border_color(t.colors.border)
            .overflow_hidden()
            .children(
                run.clone()
                    .filter(|&i| part(blocks, i, streaming) == Part::Step)
                    .enumerate()
                    .filter_map(|(n, i)| {
                        let row = match &blocks[i] {
                            Block::Tool(c) => {
                                let answer = message::answer_for(blocks, i);
                                let step = message::flash(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(message::tool_header(c, i, blocks, cx, t))
                                        .children(message::tool_body(c, i, cx, t))
                                        .children(
                                            answer
                                                .map(|p| message::approval_line(&blocks[p], cx, t)),
                                        ),
                                    i,
                                    cx,
                                    t,
                                );
                                message::mark(step, i, cx)
                            }
                            Block::Thinking { .. } => message::block(blocks, i, cx, t),
                            _ => return None,
                        };
                        Some(
                            div()
                                .px(t.space.md)
                                .py(px(3.))
                                .when(n > 0, |d| d.border_t_1().border_color(t.colors.border))
                                .child(row),
                        )
                    }),
            )
    });

    div()
        .flex()
        .flex_col()
        .child(header)
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
            review: Review::Open, images: vec![],
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
                title: "Allow Forge to edit c.rs?".into(),
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
}
