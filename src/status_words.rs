//! What KodAct CLI's status messages (`system/<subtype>`) say, in a line of
//! the chat: the Finish check's verdict and the "changes not checked yet"
//! reminders, a summary of a long conversation written or failing, a
//! background command finishing, a file that no longer parses after an edit,
//! the loop guard, nudges and a tool call KodAct CLI repaired.
//!
//! Each line leads with what happened ("Finish check:", short enough to be
//! bold in `ui/message.rs`'s event line), then says what KodAct did about
//! it. Messages that are bookkeeping (a summary ahead dropped, a retry, the
//! empty task list a conversation starts with) say nothing.

use serde_json::Value;

/// The line for `system/<subtype>` with these fields, or `None` when it
/// isn't one to show.
pub fn words(subtype: &str, v: &Value) -> Option<String> {
    let kind = v["kind"].as_str().unwrap_or_default();
    Some(match subtype {
        "verification" => verification(kind, v)?,
        "finish" => finish(v)?,

        // A long conversation's summary.
        "summary_ahead" => {
            "Compacting: writing a summary of the conversation ahead of time, while it goes on."
                .into()
        }
        "summary_ahead_applied" => {
            "Compacted: the summary written ahead now stands in for the earlier conversation; the latest steps are kept word for word.".into()
        }
        "summary_ahead_failed" => format!(
            "Compaction summary failed: {}. It is written again when the conversation is full.",
            reason(v)
        ),
        "compact_boundary" if v["compact_metadata"]["trigger"] == "auto" => {
            "Compacted: the conversation was full, so a summary now stands in for its earlier part."
                .into()
        }
        "compact_failed" => match v["model"].as_str() {
            Some(m) => format!(
                "Compaction summary failed: {} ({m}). The chat's own model writes it instead.",
                reason(v)
            ),
            None => format!("Compaction summary failed: {}. Trying again.", reason(v)),
        },
        "compact_fallback" => "Compacted without a summary: two attempts failed, so a note of your requests, the files changed and the task list stands in for the earlier conversation.".into(),

        "background_exit" => background(v)?,
        "diagnostics" => diagnostics(v)?,

        "loop_guard" => loop_guard(kind, v),
        "tool_input_repaired" => format!(
            "Tool call repaired: {}'s input ({}) was fixed before it ran.",
            str_or(v, "tool", "a tool"),
            str_or(v, "problem", "malformed")
        ),
        "tool_call_from_text" => format!(
            "Tool call repaired: {} written out as text ran as {}.",
            list(&v["tools"], "a call"),
            if v["tools"].as_array().is_some_and(|a| a.len() > 1) {
                "tool calls"
            } else {
                "a tool call"
            }
        ),
        "act_reminder" => act_reminder(kind, v)?,
        "nudge" => nudge(kind, v)?,
        "next_step_reminder" => {
            "Nudge: the reply said what it would do next without doing it; KodAct was asked to go on.".into()
        }
        "todo_reminder" => format!(
            "Nudge: tasks still open ({}); KodAct was asked to finish them or say why not.",
            list(&v["open"], "some")
        ),
        "model_fallback" => format!(
            "Switched model: {} was unavailable, so {} answers this turn.",
            str_or(v, "from", "the model"),
            str_or(v, "to", "another model")
        ),
        "reply_shrunk" => format!(
            "Shorter reply: {} tokens left in the context window, so the reply was capped at {}.",
            v["window"].as_u64().zip(v["prompt"].as_u64()).map_or("Few".into(), |(w, p)| w.saturating_sub(p).to_string()),
            v["to"].as_u64().map_or("less".into(), |t| t.to_string())
        ),
        "time_warning" => "Time almost up: KodAct was told to wrap up before the run's limit.".into(),
        _ => return None,
    })
}

fn verification(kind: &str, v: &Value) -> Option<String> {
    Some(match kind {
        "unchecked" => {
            let what = match v["files"].as_array().map(Vec::len) {
                Some(0) | None => "the work changed things".to_string(),
                Some(_) => format!("{} changed", list(&v["files"], "files")),
            };
            format!("Changes not checked yet: {what} without running the project's checks. KodAct was asked to run them.")
        }
        "last_check_failed" => format!(
            "Last check failed: {} failed after the last change. KodAct was asked to fix it or say why.",
            code(v, "command")
        ),
        "built_not_run" => format!(
            "Built, not run: {} built {}, which never ran. KodAct was asked to run it.",
            code(v, "command"),
            str_or(v, "artifact", "something")
        ),
        "no_requirements" => {
            "Finish check: the finish listed no requirements; KodAct was asked to list them.".into()
        }
        "unmet_requirements" => format!(
            "Finish check: not shown met yet: {}. KodAct keeps going.",
            list(&v["unmet"], "some requirements")
        ),
        "no_finish" => "Finish check: the run ended without a finish; KodAct was asked for one.".into(),
        "finish_check" => {
            let why = v["reason"].as_str().filter(|r| !r.trim().is_empty());
            match v["verdict"].as_str().unwrap_or_default() {
                "met" => format!("Finish check: met. {}", why.unwrap_or("The request is done.")),
                "not_met" => format!(
                    "Finish check: not met yet. {} KodAct keeps going.",
                    why.unwrap_or("Something is missing.")
                ),
                "impossible" => format!(
                    "Finish check: can't be met. {}",
                    why.unwrap_or("The request can't be done as asked.")
                ),
                "error" => format!(
                    "Finish check couldn't run: {}",
                    why.unwrap_or("the checking model failed")
                ),
                _ => format!(
                    "Finish check: unclear. {}",
                    why.unwrap_or("The check gave no verdict.")
                ),
            }
        }
        _ => return None,
    })
}

/// The run's own Finish: verified or not, and how many of the requirements
/// it lists it showed met (with evidence).
fn finish(v: &Value) -> Option<String> {
    let reqs = v["requirements"].as_array()?;
    let met = reqs.iter().filter(|r| r["met"] == true).count();
    let verdict = if v["verified"] == true {
        "verified"
    } else {
        "not verified"
    };
    Some(match reqs.len() {
        0 => format!("Finished, {verdict}: no requirements listed."),
        n => format!("Finished, {verdict}: {met} of {n} requirements shown met."),
    })
}

fn background(v: &Value) -> Option<String> {
    let shells = v["shells"].as_array().filter(|s| !s.is_empty())?;
    let one = |s: &Value| {
        let how = match s["exit_code"].as_i64() {
            Some(0) => "succeeded".to_string(),
            Some(c) => format!("failed (exit {c})"),
            None => "stopped".to_string(),
        };
        let secs = s["seconds"]
            .as_u64()
            .map(|n| format!(" after {}", seconds(n)))
            .unwrap_or_default();
        format!("{} {how}{secs}", code(s, "command"))
    };
    Some(match shells.as_slice() {
        [s] => format!("Background command finished: {}.", one(s)),
        all => format!(
            "Background commands finished: {}.",
            all.iter().map(one).collect::<Vec<_>>().join("; ")
        ),
    })
}

fn diagnostics(v: &Value) -> Option<String> {
    let file = v["file"].as_str();
    if let (Some(f), Some(parser)) = (file, v["parser"].as_str()) {
        return Some(format!(
            "File no longer parses: {f} ({parser}) after the edit. KodAct was shown the error."
        ));
    }
    let n = v["errors"].as_u64().unwrap_or(0);
    if n == 0 {
        return None;
    }
    let errors = if n == 1 {
        "1 new error".to_string()
    } else {
        format!("{n} new errors")
    };
    let server = v["server"]
        .as_str()
        .map(|s| format!(" ({s})"))
        .unwrap_or_default();
    Some(match file {
        Some(f) => format!("Errors after an edit: {errors} in {f}{server}. KodAct was shown them."),
        None => format!("Errors after a command: {errors}{server}. KodAct was shown them."),
    })
}

fn loop_guard(kind: &str, v: &Value) -> String {
    let tool = str_or(v, "tool", "tool");
    let count = v["count"].as_u64().unwrap_or(0);
    match kind {
        "repeated_failure" => format!("Loop guard: the same {tool} call failed {count} times. KodAct was told to change approach."),
        "refused" => format!("Loop guard: a {tool} call that failed {count} times the same way wasn't run again."),
        "no_progress" => format!("Loop guard: {count} {tool} calls in a row changed nothing. KodAct was told to change approach."),
        "oscillating_edits" => format!(
            "Loop guard: {} was edited back and forth. KodAct was told to settle on one version.",
            str_or(v, "file", "a file")
        ),
        "error_streak" => format!("Loop guard: {count} errors in a row. KodAct was told to step back and rethink."),
        _ => "Loop guard: KodAct seemed stuck and was told to change approach.".into(),
    }
}

fn act_reminder(kind: &str, v: &Value) -> Option<String> {
    let secs = v["secs"].as_u64().map(seconds);
    Some(match kind {
        "repeating" => "Nudge: the reply was repeating itself; it was stopped and KodAct was told.".into(),
        "reasoning_cutoff" => "Nudge: the reply ran out of room while still reasoning; KodAct was asked to act on what it has.".into(),
        "empty_reply" => "Nudge: the reply was empty; KodAct was asked to go on.".into(),
        "broken_stream" => format!(
            "Reply cut off: an error broke it{}; it is sent again from its notes.",
            secs.map(|s| format!(" after {s}")).unwrap_or_default()
        ),
        _ => return None,
    })
}

fn nudge(kind: &str, v: &Value) -> Option<String> {
    Some(match kind {
        "thinking_time_box" => format!(
            "Nudge: the reply reasoned for {} without acting; it was stopped and KodAct was asked to take a step.",
            v["secs"].as_u64().map_or("too long".into(), seconds)
        ),
        "adaptive_effort" => format!(
            "Reasoning effort {} → {}: {}.",
            str_or(v, "from", "?"),
            str_or(v, "to", "?"),
            str_or(v, "why", "adjusted")
        ),
        "shell_edit" => format!(
            "File no longer parses: {} ({}) after a shell command edited it. KodAct was shown the error.",
            str_or(v, "file", "a file"),
            str_or(v, "parser", "parser")
        ),
        "constraints" => {
            let mut missing = Vec::new();
            if v["missing_files"].as_array().is_some_and(|a| !a.is_empty()) {
                missing.push(format!("files not made yet ({})", list(&v["missing_files"], "")));
            }
            if v["unrun_commands"].as_array().is_some_and(|a| !a.is_empty()) {
                missing.push(format!("commands not run ({})", list(&v["unrun_commands"], "")));
            }
            if v["over_limit"].as_array().is_some_and(|a| !a.is_empty()) {
                missing.push("files over their size limit".into());
            }
            if missing.is_empty() {
                return None;
            }
            format!(
                "Nudge: your request names {}. KodAct was asked to do it or say why not.",
                missing.join(" and ")
            )
        }
        "unpinned_installs" => {
            let list: Vec<String> = v["packages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| {
                    let name = p["name"].as_str()?;
                    Some(match p["version"].as_str() {
                        Some(ver) => format!("{name} {ver}"),
                        None => name.to_string(),
                    })
                })
                .collect();
            if list.is_empty() {
                return None;
            }
            format!(
                "Nudge: {} installed with no version; KodAct was told its checks covered that version only.",
                list.join(", ")
            )
        }
        "passing_snapshot" => format!(
            "Nudge: a version that passed its checks was saved ({}); KodAct was told it can go back to it.",
            str_or(v, "label", "a snapshot")
        ),
        "size_limit" => format!(
            "Nudge: {} is over its size limit ({} of {} bytes).",
            str_or(v, "file", "a file"),
            v["size"].as_u64().unwrap_or(0),
            v["limit"].as_u64().unwrap_or(0)
        ),
        "stale_deliverable" => format!(
            "Nudge: {} hasn't been rewritten since earlier steps changed what it depends on.",
            list(&v["files"], "a result file")
        ),
        "limit_verdict" => format!(
            "Nudge: {} of {} lines meet the request's limits.",
            v["met"].as_u64().unwrap_or(0),
            v["lines"].as_u64().unwrap_or(0)
        ),
        // The empty task list a conversation starts with, and the clock
        // notes of a timed run, are routine.
        "todo_tracking" if v["empty"] == true => return None,
        "todo_tracking" => "Nudge: the task list wasn't updated for a while; KodAct was reminded of it.".into(),
        _ => return None,
    })
}

/// A field's text, or `or`.
fn str_or<'a>(v: &'a Value, key: &str, or: &'a str) -> &'a str {
    v[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(or)
}

/// A command, in backticks, cut to a line.
fn code(v: &Value, key: &str) -> String {
    let c = str_or(v, key, "a command")
        .lines()
        .next()
        .unwrap_or_default();
    let c: String = if c.chars().count() > 80 {
        c.chars().take(79).chain(['…']).collect()
    } else {
        c.to_string()
    };
    format!("`{c}`")
}

/// The first three of a list of words, then how many more.
fn list(v: &Value, or: &str) -> String {
    let items: Vec<String> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| {
            x.as_str()
                .map(str::to_string)
                .or_else(|| x["file"].as_str().map(str::to_string))
        })
        .collect();
    match items.len() {
        0 => or.to_string(),
        n if n <= 3 => items.join(", "),
        n => format!("{} and {} more", items[..3].join(", "), n - 3),
    }
}

/// `42 s`, `3 min 5 s`.
fn seconds(n: u64) -> String {
    match (n / 60, n % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

/// A failure's reason in words.
fn reason(v: &Value) -> String {
    match v["reason"].as_str().unwrap_or_default() {
        "empty_summary" => "an empty summary".into(),
        "" => "an error".into(),
        r => r.replace('_', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_finish_check_says_its_verdict() {
        let w = words(
            "verification",
            &json!({"kind": "finish_check", "verdict": "not_met", "reason": "tests still fail."}),
        )
        .unwrap();
        assert_eq!(
            w,
            "Finish check: not met yet. tests still fail. KodAct keeps going."
        );
        assert!(words(
            "verification",
            &json!({"kind": "finish_check", "verdict": "met"})
        )
        .unwrap()
        .starts_with("Finish check: met."));
        let unchecked = words(
            "verification",
            &json!({"kind": "unchecked", "files": ["a.rs", "b.rs"], "shell": false}),
        )
        .unwrap();
        let f = words("finish", &json!({"summary": "s", "verified": false, "requirements": [{"met": true}, {"met": false}]})).unwrap();
        assert_eq!(f, "Finished, not verified: 1 of 2 requirements shown met.");
        assert!(
            unchecked.starts_with("Changes not checked yet: a.rs, b.rs changed"),
            "{unchecked}"
        );
    }

    #[test]
    fn summaries_written_or_failing() {
        assert!(words("summary_ahead", &json!({}))
            .unwrap()
            .starts_with("Compacting:"));
        let failed = words(
            "compact_failed",
            &json!({"trigger": "auto", "reason": "empty_summary", "model": "small"}),
        )
        .unwrap();
        assert_eq!(failed, "Compaction summary failed: an empty summary (small). The chat's own model writes it instead.");
        assert!(words("compact_fallback", &json!({"trigger": "auto"})).is_some());
        // A compaction you asked for has its own notice.
        assert_eq!(
            words(
                "compact_boundary",
                &json!({"compact_metadata": {"trigger": "manual"}})
            ),
            None
        );
        assert!(words(
            "compact_boundary",
            &json!({"compact_metadata": {"trigger": "auto"}})
        )
        .is_some());
        assert_eq!(
            words(
                "summary_ahead_dropped",
                &json!({"reason": "conversation_changed"})
            ),
            None
        );
    }

    #[test]
    fn background_commands_and_broken_files() {
        let one = words("background_exit", &json!({"shells": [{"id": "b1", "command": "npm run build", "exit_code": 1, "seconds": 65}]})).unwrap();
        assert_eq!(
            one,
            "Background command finished: `npm run build` failed (exit 1) after 1 min 5 s."
        );
        let parse = words(
            "diagnostics",
            &json!({"file": "src/a.py", "parser": "python"}),
        )
        .unwrap();
        assert_eq!(
            parse,
            "File no longer parses: src/a.py (python) after the edit. KodAct was shown the error."
        );
        assert_eq!(
            words(
                "diagnostics",
                &json!({"file": "a.rs", "server": "rust-analyzer", "errors": 0})
            ),
            None
        );
    }

    #[test]
    fn guards_nudges_and_repairs() {
        let g = words("loop_guard", &json!({"kind": "repeated_failure", "tool": "Bash", "count": 3, "after_changes": false})).unwrap();
        assert!(g.starts_with("Loop guard: the same Bash call failed 3 times."));
        let r = words(
            "tool_input_repaired",
            &json!({"tool": "Edit", "problem": "incomplete JSON"}),
        )
        .unwrap();
        assert_eq!(
            r,
            "Tool call repaired: Edit's input (incomplete JSON) was fixed before it ran."
        );
        assert!(words("tool_call_from_text", &json!({"tools": ["Read"]}))
            .unwrap()
            .ends_with("ran as a tool call."));
        assert!(
            words("nudge", &json!({"kind": "thinking_time_box", "secs": 120}))
                .unwrap()
                .contains("2 min")
        );
        assert_eq!(
            words("nudge", &json!({"kind": "todo_tracking", "empty": true})),
            None
        );
        assert_eq!(words("nudge", &json!({"kind": "clock", "used_s": 5})), None);
        let c = words("nudge", &json!({"kind": "constraints", "missing_files": ["out.txt"], "unrun_commands": [], "over_limit": []})).unwrap();
        assert_eq!(c, "Nudge: your request names files not made yet (out.txt). KodAct was asked to do it or say why not.");
        let u = words("nudge", &json!({"kind": "unpinned_installs", "packages": [{"name": "transformers", "version": "5.24.0"}]})).unwrap();
        assert_eq!(u, "Nudge: transformers 5.24.0 installed with no version; KodAct was told its checks covered that version only.");
        assert_eq!(words("api_retry", &json!({"attempt": 1})), None);
    }

    #[test]
    fn each_lead_fits_the_event_line() {
        // `ui/message.rs` bolds what comes before ": " when it is 32 bytes at most.
        for (sub, v) in [
            ("verification", json!({"kind": "unchecked", "files": []})),
            ("summary_ahead_failed", json!({"reason": "x"})),
            (
                "background_exit",
                json!({"shells": [{"command": "x", "exit_code": 0}]}),
            ),
            ("diagnostics", json!({"file": "f", "parser": "p"})),
            ("loop_guard", json!({"kind": "error_streak", "count": 4})),
            ("tool_input_repaired", json!({})),
        ] {
            let w = words(sub, &v).unwrap();
            let lead = w.split_once(": ").unwrap().0;
            assert!(lead.len() <= 32, "{lead}");
        }
    }
}
