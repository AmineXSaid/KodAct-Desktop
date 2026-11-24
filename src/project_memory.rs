//! A project's memory: what KodAct learns while working there (decisions,
//! preferences, names, conventions), kept as plain Markdown you can read
//! and change, in `.kodact/memory.md` (`.forge/memory.md` in a project made
//! before the rename, `names.rs`):
//!
//! ```text
//! # Memory
//!
//! ## Decisions
//! - Invoices are stored in integer cents.
//! ```
//!
//! A new chat gets it, with the rules for adding to it, ahead of its first
//! message (`state.rs` `send_to_forge`); KodAct records with its own file
//! tools. After a turn, what changed is shown under the reply with Undo.

use std::path::{Path, PathBuf};

/// The memory file, relative to the project: `.kodact/memory.md`, or in
/// ForgeCLI's `.forge/` while the project has only that folder.
pub fn file(project: &Path) -> String {
    crate::names::in_project_dir(project, "memory.md")
}

/// A memory larger than this isn't sent (it would crowd the chat out); the
/// card says so.
pub const MAX: usize = 64 * 1024;

pub fn path(project: &Path) -> PathBuf {
    project.join(file(project))
}

/// The memory as it is now; empty when there is none (blocking).
pub fn read(project: &Path) -> String {
    std::fs::read_to_string(path(project)).unwrap_or_default()
}

/// Write the memory (blocking); empty text removes the file.
pub fn write(project: &Path, text: &str) -> std::io::Result<()> {
    crate::project_ctx::write(&path(project), text)
}

/// Its facts by topic, in order: ("Decisions", ["Invoices are…"]). Facts
/// before any topic are under "".
pub fn topics(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    for line in text.lines() {
        let l = line.trim();
        if let Some(t) = l.strip_prefix("## ") {
            out.push((t.trim().to_string(), vec![]));
        } else if let Some(f) = l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")) {
            if out.is_empty() {
                out.push((String::new(), vec![]));
            }
            out.last_mut().expect("pushed").1.push(f.trim().to_string());
        }
    }
    out.retain(|(_, f)| !f.is_empty());
    out
}

fn facts(text: &str) -> Vec<String> {
    topics(text).into_iter().flat_map(|(_, f)| f).collect()
}

/// What changed from `before` to `after`: facts added, facts removed.
pub fn diff(before: &str, after: &str) -> (Vec<String>, Vec<String>) {
    let (a, b) = (facts(before), facts(after));
    let added = b.iter().filter(|f| !a.contains(f)).cloned().collect();
    let removed = a.iter().filter(|f| !b.contains(f)).cloned().collect();
    (added, removed)
}

/// The memory without one fact (and without a topic it leaves empty).
pub fn forget(text: &str, fact: &str) -> String {
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|l| {
            let l = l.trim();
            l.strip_prefix("- ")
                .or_else(|| l.strip_prefix("* "))
                .is_none_or(|f| f.trim() != fact)
        })
        .collect();
    // A topic heading with no facts under it goes too.
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().starts_with("## ") {
            let empty = lines[i + 1..]
                .iter()
                .take_while(|l| !l.trim().starts_with("## "))
                .all(|l| l.trim().is_empty());
            if empty {
                lines.remove(i);
                continue;
            }
        }
        i += 1;
    }
    let mut out = lines.join("\n").trim_end().to_string();
    if topics(&out).is_empty() {
        out.clear();
    }
    out
}

/// The memory with `fact` added under `topic` (made if missing), as KodAct
/// writes it.
pub fn add(text: &str, topic: &str, fact: &str) -> String {
    let mut lines: Vec<String> = if text.trim().is_empty() {
        vec!["# Memory".into()]
    } else {
        text.trim_end().lines().map(String::from).collect()
    };
    let head = format!("## {topic}");
    match lines.iter().position(|l| l.trim() == head) {
        Some(at) => {
            // After the topic's last fact.
            let end = lines[at + 1..]
                .iter()
                .position(|l| l.trim().starts_with("## "))
                .map_or(lines.len(), |i| at + 1 + i);
            let mut insert = end;
            while insert > at + 1 && lines[insert - 1].trim().is_empty() {
                insert -= 1;
            }
            lines.insert(insert, format!("- {fact}"));
        }
        None => {
            lines.push(String::new());
            lines.push(head);
            lines.push(format!("- {fact}"));
        }
    }
    lines.join("\n")
}

/// What a new chat gets: the memory, and how to keep it in `file`
/// (relative to the project).
pub fn prompt(text: &str, file: &str) -> String {
    let memory = if text.trim().is_empty() {
        "(Nothing yet.)".to_string()
    } else if text.len() > MAX {
        format!("(It is too large to include: read {file} when it matters.)")
    } else {
        text.trim().to_string()
    };
    format!(
        "<project_memory file=\"{file}\">\n{memory}\n</project_memory>\n\
This is what you have learned working in this project. Rely on it. When the user tells you something durable about the \
project or how they work (a decision, a preference, a name, a convention, a correction), record it in {file}: one short fact \
per line as `- fact`, under a `## Topic` heading (Decisions, Preferences, People, Conventions, or a better one), creating \
the file with `# Memory` at the top if needed. Change or remove a fact that turns out wrong. Never record secrets, keys or \
passwords, or things that only matter for this one conversation. Say in one short line when you remembered something.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEM: &str = "# Memory\n\n## Decisions\n- Amounts are integer cents.\n- No ORMs.\n\n## People\n- Sam reviews the API.\n";

    #[test]
    fn memory_reads_as_topics_and_diffs_by_fact() {
        let t = topics(MEM);
        assert_eq!(t.len(), 2);
        assert_eq!(
            t[0],
            (
                "Decisions".into(),
                vec!["Amounts are integer cents.".into(), "No ORMs.".into()]
            )
        );
        let after = format!("{MEM}- Deploys are on Tuesdays.\n");
        assert_eq!(
            diff(MEM, &after),
            (vec!["Deploys are on Tuesdays.".into()], vec![])
        );
        let (added, removed) = diff(MEM, &forget(MEM, "No ORMs."));
        assert!(added.is_empty());
        assert_eq!(removed, vec!["No ORMs.".to_string()]);
    }

    #[test]
    fn a_fact_joins_its_topic_or_starts_one() {
        let m = add(MEM, "Decisions", "Deploys are on Tuesdays.");
        assert_eq!(topics(&m)[0].1.last().unwrap(), "Deploys are on Tuesdays.");
        let m = add(&m, "Tone", "Plain words.");
        assert_eq!(
            topics(&m).last().unwrap(),
            &("Tone".to_string(), vec!["Plain words.".to_string()])
        );
        assert!(add("", "Preferences", "Dark mode.")
            .starts_with("# Memory\n\n## Preferences\n- Dark mode."));
    }

    #[test]
    fn forgetting_the_last_fact_of_a_topic_drops_the_topic() {
        let m = forget(MEM, "Sam reviews the API.");
        assert!(!m.contains("## People"), "{m}");
        assert!(m.contains("## Decisions"));
        let none = forget(&forget(&m, "No ORMs."), "Amounts are integer cents.");
        assert_eq!(none, "", "no facts left, no memory");
    }

    #[test]
    fn a_new_chat_gets_the_memory_and_how_to_keep_it() {
        let p = prompt(MEM, ".kodact/memory.md");
        assert!(p.contains("- No ORMs.") && p.contains(".kodact/memory.md") && p.contains("Never record secrets"));
        assert!(prompt("", ".kodact/memory.md").contains("(Nothing yet.)"));
    }
}
