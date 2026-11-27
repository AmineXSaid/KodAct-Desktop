//! The sub-agents of a chat as both the transcript and the Agents panel
//! count and describe them, so the two always agree: the run line ("3
//! agents working", "Ran 10 agents · 1 failed"), the panel's jumper filters
//! (All, Running, Failed), which agent is current, and the step and status
//! wording ("Failed · 1 step · 10s", just "Failed" with no steps). Pure: no
//! UI here, so every rule is tested.

use std::time::Duration;

use crate::state::{AgentRun, ToolCall, ToolStatus};

/// Where an agent stands.
pub fn status(run: &AgentRun) -> ToolStatus {
    match (run.ended, run.failed) {
        (None, _) => ToolStatus::Running,
        // You stopped it: not a failure, and not done.
        (Some(_), _) if run.stopped => ToolStatus::Declined,
        (Some(_), true) => ToolStatus::Failed,
        (Some(_), false) => ToolStatus::Done,
    }
}

/// Quiet this long while it runs, an agent looks stuck and says so.
pub const STALLED_AFTER: Duration = Duration::from_secs(120);

/// Still running, but nothing heard from it for `STALLED_AFTER`: its row in
/// the transcript shows it as a problem, in the warning colour.
pub fn is_stalled(run: &AgentRun, now: std::time::Instant) -> bool {
    run.ended.is_none() && now.saturating_duration_since(run.heard) >= STALLED_AFTER
}

/// A stalled agent's status line: "No word for 2m 5s".
pub fn stalled_line(run: &AgentRun, now: std::time::Instant) -> String {
    format!(
        "No word for {}",
        super::chat_view::elapsed(now.saturating_duration_since(run.heard).as_secs())
    )
}

/// The jumper's filter segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Running,
    Failed,
}

impl Filter {
    pub fn keeps(self, run: &AgentRun) -> bool {
        match self {
            Filter::All => true,
            Filter::Running => status(run) == ToolStatus::Running,
            Filter::Failed => status(run) == ToolStatus::Failed,
        }
    }
}

/// How many agents there are in each state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub running: usize,
    pub failed: usize,
    pub stopped: usize,
}

impl Counts {
    pub fn of(runs: &[AgentRun]) -> Self {
        let mut c = Counts {
            total: runs.len(),
            ..Default::default()
        };
        for r in runs {
            match status(r) {
                ToolStatus::Running => c.running += 1,
                ToolStatus::Failed => c.failed += 1,
                ToolStatus::Declined => c.stopped += 1,
                _ => {}
            }
        }
        c
    }

    /// The transcript's counts, from the reply's own agent calls (`Task`
    /// tool calls): one per agent, in the same states as the panel's runs.
    pub fn of_calls(calls: &[&ToolCall]) -> Self {
        let agents: Vec<&&ToolCall> = calls
            .iter()
            .filter(|c| crate::state::is_agent_tool(&c.target))
            .collect();
        Counts {
            total: agents.len(),
            running: agents
                .iter()
                .filter(|c| c.status == ToolStatus::Running)
                .count(),
            failed: agents
                .iter()
                .filter(|c| c.status == ToolStatus::Failed)
                .count(),
            stopped: agents
                .iter()
                .filter(|c| matches!(c.status, ToolStatus::Declined | ToolStatus::Interrupted))
                .count(),
        }
    }

    /// The count a jumper segment shows.
    pub fn for_filter(&self, f: Filter) -> usize {
        match f {
            Filter::All => self.total,
            Filter::Running => self.running,
            Filter::Failed => self.failed,
        }
    }

    /// The run line's words: "3 agents working" while any run, "Ran 10
    /// agents" once all have ended (failures are added beside it, in their
    /// colour, as "· 1 failed").
    pub fn run_line(&self) -> String {
        if self.running > 0 {
            format!("{} working", agents(self.running))
        } else {
            format!("Ran {}", agents(self.total))
        }
    }

    /// The panel's footer while some still run: "3 of 10 agents running".
    pub fn running_line(&self) -> String {
        format!("{} of {} running", self.running, agents(self.total))
    }

    /// The dot on the jumper: something needs your attention.
    pub fn needs_attention(&self) -> bool {
        self.failed > 0
    }
}

fn agents(n: usize) -> String {
    if n == 1 {
        "1 agent".to_string()
    } else {
        format!("{n} agents")
    }
}

/// "1 step", "3 steps".
pub fn steps(n: usize) -> String {
    if n == 1 {
        "1 step".to_string()
    } else {
        format!("{n} steps")
    }
}

/// The agents in panel order (the order they started), each with its index:
/// the ones `filter` keeps.
pub fn listed(runs: &[AgentRun], filter: Filter) -> Vec<usize> {
    runs.iter()
        .enumerate()
        .filter(|(_, r)| filter.keeps(r))
        .map(|(i, _)| i)
        .collect()
}

/// Which agent the panel shows: the one picked, else the first still
/// running, else the first.
pub fn current(runs: &[AgentRun], picked: Option<&str>) -> usize {
    picked
        .and_then(|id| runs.iter().position(|r| r.id == id))
        .or_else(|| runs.iter().position(|r| r.ended.is_none()))
        .unwrap_or(0)
}

/// What picking a row in the jumper does: that agent becomes current and
/// the jumper closes.
pub fn jump(picked: &mut Option<String>, jumper_open: &mut bool, id: &str) {
    *picked = Some(id.to_string());
    *jumper_open = false;
}

/// An agent's status line in the transcript row and the panel: "Failed · 1
/// step · 10s", just "Failed" when it took no steps, "Stopped · 4s",
/// "Done in 9s · 1 step".
pub fn status_line(st: ToolStatus, step_count: usize, took: Option<Duration>) -> String {
    let el = took.map(super::agents_view::elapsed);
    let word = match st {
        ToolStatus::Failed => "Failed",
        ToolStatus::Declined => "Stopped",
        ToolStatus::Interrupted => "Interrupted",
        ToolStatus::Running => "Running",
        ToolStatus::Done => "Done",
    };
    if step_count == 0 && st != ToolStatus::Running {
        return word.to_string();
    }
    let mut parts = vec![word.to_string()];
    if step_count > 0 {
        parts.push(steps(step_count));
    }
    parts.extend(el);
    parts.join(" · ")
}

/// The panel's detail summary: "1 step (1 passed)", "4 steps (3 passed, 1
/// failed)".
pub fn step_summary(passed: usize, failed: usize, total: usize) -> String {
    let mut inner = vec![format!("{passed} passed")];
    if failed > 0 {
        inner.push(format!("{failed} failed"));
    }
    format!("{} ({})", steps(total), inner.join(", "))
}

/// The panel's footer once nothing runs: "Agent 1 of 10 · 1 step · 9s".
/// With no steps (stopped as it started) the count is left out: "Agent 1 of
/// 10 · 0s".
pub fn agent_line(index: usize, total: usize, step_count: usize, took: Duration) -> String {
    let mut parts = vec![format!("Agent {} of {}", index + 1, total)];
    if step_count > 0 {
        parts.push(steps(step_count));
    }
    parts.push(super::agents_view::elapsed(took));
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn run(id: &str, ended: bool, failed: bool) -> AgentRun {
        let now = Instant::now();
        AgentRun {
            id: id.into(),
            kind: "Explore".into(),
            task: format!("Task {id}"),
            prompt: String::new(),
            started: now,
            ended: ended.then_some(now),
            heard: now,
            failed,
            steps: vec![],
            entries: vec![],
            said: String::new(),
            stopping: false,
            stopped: false,
        }
    }

    /// Ten agents: three still running, one failed, six done.
    fn ten() -> Vec<AgentRun> {
        (0..10)
            .map(|i| run(&format!("a{i}"), i >= 3, i == 4))
            .collect()
    }

    #[test]
    fn agents_are_listed_in_the_order_they_started() {
        let runs = ten();
        assert_eq!(listed(&runs, Filter::All), (0..10).collect::<Vec<_>>());
        assert_eq!(listed(&runs, Filter::Running), vec![0, 1, 2]);
        assert_eq!(listed(&runs, Filter::Failed), vec![4]);
    }

    #[test]
    fn the_filter_counts_add_up() {
        let c = Counts::of(&ten());
        assert_eq!(
            (c.for_filter(Filter::All), c.for_filter(Filter::Running), c.for_filter(Filter::Failed)),
            (10, 3, 1)
        );
        // Each count is the length of its list.
        for f in [Filter::All, Filter::Running, Filter::Failed] {
            assert_eq!(c.for_filter(f), listed(&ten(), f).len(), "{f:?}");
        }
        assert!(c.needs_attention());
        assert!(!Counts::of(&[run("a", true, false)]).needs_attention());
    }

    #[test]
    fn picking_in_the_jumper_selects_and_closes_it() {
        let runs = ten();
        let (mut picked, mut open) = (None, true);
        // Nothing picked: the first still running is current.
        assert_eq!(current(&runs, picked.as_deref()), 0);
        jump(&mut picked, &mut open, "a7");
        assert!(!open, "the jumper closes");
        assert_eq!(current(&runs, picked.as_deref()), 7);
        // A pick that no longer exists falls back.
        assert_eq!(current(&runs, Some("gone")), 0);
        // All ended and nothing picked: the first.
        let ended: Vec<_> = (0..3).map(|i| run(&format!("e{i}"), true, false)).collect();
        assert_eq!(current(&ended, None), 0);
    }

    #[test]
    fn the_transcript_and_the_panel_agree_on_the_totals() {
        use crate::state::ToolStatus as S;
        // The reply's agent calls, in the states the panel's runs are in.
        let runs = ten();
        let calls: Vec<ToolCall> = runs
            .iter()
            .map(|r| {
                let mut c = crate::backend::tool_call(
                    "Task",
                    &serde_json::json!({"description": r.task, "subagent_type": r.kind}),
                );
                c.status = status(r);
                c
            })
            .collect();
        let refs: Vec<&ToolCall> = calls.iter().collect();
        assert_eq!(Counts::of_calls(&refs), Counts::of(&runs));
        assert_eq!(Counts::of_calls(&refs).run_line(), "3 agents working");

        // Once all have ended, one of them failed: "Ran 10 agents", not 9.
        let mut done = calls.clone();
        for c in &mut done {
            if c.status == S::Running {
                c.status = S::Done;
            }
        }
        let refs: Vec<&ToolCall> = done.iter().collect();
        let c = Counts::of_calls(&refs);
        assert_eq!((c.run_line(), c.failed), ("Ran 10 agents".to_string(), 1));
        assert_eq!(Counts::of(&ten()).running_line(), "3 of 10 agents running");
    }

    #[test]
    fn a_quiet_running_agent_is_stalled() {
        let mut r = run("a", false, false);
        let now = r.heard;
        assert!(!is_stalled(&r, now + Duration::from_secs(119)));
        let later = now + Duration::from_secs(125);
        assert!(is_stalled(&r, later));
        assert!(stalled_line(&r, later).starts_with("No word for "));
        // An agent that has ended is never stalled.
        r.ended = Some(now);
        assert!(!is_stalled(&r, later));
    }

    #[test]
    fn step_wording_is_singular_for_one_and_absent_for_none() {
        use crate::state::ToolStatus as S;
        let ten_s = Some(Duration::from_secs(10));
        assert_eq!(status_line(S::Failed, 1, ten_s), "Failed · 1 step · 10s");
        assert_eq!(status_line(S::Failed, 3, ten_s), "Failed · 3 steps · 10s");
        assert_eq!(status_line(S::Failed, 0, ten_s), "Failed");
        assert_eq!(status_line(S::Failed, 0, None), "Failed");
        assert_eq!(status_line(S::Declined, 2, None), "Stopped · 2 steps");
        assert_eq!(status_line(S::Running, 0, Some(Duration::from_secs(3))), "Running · 3s");
        assert_eq!(steps(1), "1 step");
        assert_eq!(step_summary(1, 0, 1), "1 step (1 passed)");
        assert_eq!(step_summary(3, 1, 4), "4 steps (3 passed, 1 failed)");
        assert_eq!(
            agent_line(0, 10, 1, Duration::from_secs(9)),
            "Agent 1 of 10 · 1 step · 9s"
        );
        assert_eq!(agent_line(0, 10, 0, Duration::from_secs(0)), "Agent 1 of 10 · 0s");
        assert_eq!(Counts::of(&[run("a", false, false)]).run_line(), "1 agent working");
    }
}
