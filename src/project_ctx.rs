//! What every chat in a project gets before you type a word: the
//! instructions ForgeCLI loads (`forge_files::memory_files`: yours, each
//! folder's `FORGE.md` down to the project, its `FORGE.local.md`), each with
//! what it costs. The project page's rail shows it as one bar against the
//! model's window, and edits the project's two files: Shared (`FORGE.md`,
//! which travels with the folder) and Only me (`FORGE.local.md`).
//! Reading blocks: run [`read`] off the UI thread.

use std::path::{Path, PathBuf};

/// One thing every chat gets.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// You (every project), Project (shared), Local (this computer).
    pub kind: &'static str,
    pub path: PathBuf,
    pub tokens: u64,
}

/// A project's brief: what loads, and its two instruction files as they are.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Brief {
    pub parts: Vec<Part>,
    /// The project's shared instructions and where they are (or would be).
    pub shared: String,
    pub shared_path: PathBuf,
    /// Yours only, on this computer.
    pub mine: String,
    pub mine_path: PathBuf,
    /// How many rule files (CLAUDE.md, `.claude/rules`) the Rules screen
    /// finds for the project.
    pub rules: usize,
    /// Its knowledge (`knowledge.rs`), compiled for new chats.
    pub knowledge: crate::knowledge::Knowledge,
    /// What Forge has learned there (`project_memory.rs`).
    pub memory: String,
    /// The GitHub repositories its knowledge comes from (`github_knowledge.rs`).
    pub github: Vec<crate::github_knowledge::Source>,
    /// Its setup files (`project_share.rs`), and how many git would commit
    /// (None: not a repository).
    pub setup_files: usize,
    pub setup_uncommitted: Option<usize>,
}

impl Brief {
    pub fn total(&self) -> u64 {
        self.parts.iter().map(|p| p.tokens).sum()
    }
}

/// About four characters to a token, as the context meter counts.
pub fn tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

/// The file a project's shared instructions live in: its `FORGE.md`, else
/// `.forge/FORGE.md`, else `AGENTS.md` (ForgeCLI reads that when there is
/// no FORGE.md), else a new `FORGE.md`.
pub fn shared_file(project: &Path) -> PathBuf {
    ["FORGE.md", ".forge/FORGE.md", "AGENTS.md"]
        .iter()
        .map(|n| project.join(n))
        .find(|p| p.is_file())
        .unwrap_or_else(|| project.join("FORGE.md"))
}

/// Read what loads for `project` and its two instruction files, and
/// compile its knowledge within `budget` tokens. `memory_on`: its memory
/// goes into new chats too.
pub fn read(project: &Path, budget: u64, memory_on: bool) -> Brief {
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
    let mut parts: Vec<Part> = crate::forge_files::memory_files(project)
        .into_iter()
        .filter(|m| m.exists)
        .map(|m| Part {
            kind: m.kind,
            tokens: tokens(&read(&m.path)),
            path: m.path,
        })
        .collect();
    let knowledge = crate::knowledge::compile(project, budget);
    let memory = crate::project_memory::read(project);
    if memory_on && !memory.trim().is_empty() {
        parts.push(Part {
            kind: "Memory",
            path: crate::project_memory::path(project),
            tokens: tokens(&memory),
        });
    }
    if knowledge.included_tokens > 0 {
        parts.push(Part {
            kind: "Knowledge",
            path: project.join(crate::knowledge::DIR),
            tokens: knowledge.included_tokens,
        });
    }
    let shared_path = shared_file(project);
    let mine_path = project.join("FORGE.local.md");
    let home = crate::rules::home_dir();
    Brief {
        parts,
        shared: read(&shared_path),
        mine: read(&mine_path),
        shared_path,
        mine_path,
        rules: crate::rules::scan(project, home.as_deref()).sources.len(),
        knowledge,
        memory,
        github: crate::github_knowledge::sources(project),
        // What you'd recognise: the instructions and the knowledge files
        // (not Forge's own list and compiled block).
        setup_files: crate::project_share::setup_files(project, false)
            .iter()
            .filter(|f| !f.starts_with(".forge/knowledge."))
            .count(),
        setup_uncommitted: crate::project_share::uncommitted(project, false),
    }
}

/// Write one of a project's instruction files (blocking). Empty text
/// removes the file, so nothing loads for it.
pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if text.trim().is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut text = text.trim_end().to_string();
    text.push('\n');
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_brief_counts_what_loads_and_edits_its_files() {
        let dir = std::env::temp_dir().join(format!("forge-brief-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Nothing yet: the shared file would be a new FORGE.md.
        let b = read(&dir, 1000, true);
        assert_eq!(b.shared_path, dir.join("FORGE.md"));
        assert!(b.shared.is_empty() && b.mine.is_empty());
        // AGENTS.md is what loads when there is no FORGE.md: edit that.
        std::fs::write(dir.join("AGENTS.md"), "Use tabs.").unwrap();
        assert_eq!(shared_file(&dir), dir.join("AGENTS.md"));
        std::fs::remove_file(dir.join("AGENTS.md")).unwrap();
        // Written, counted, and removed when emptied.
        write(&dir.join("FORGE.md"), "Answer in French.\n\n").unwrap();
        write(&dir.join("FORGE.local.md"), "I review on Fridays.").unwrap();
        let b = read(&dir, 1000, true);
        assert_eq!(b.shared, "Answer in French.\n");
        let project: Vec<_> = b.parts.iter().filter(|p| p.kind != "You").collect();
        assert_eq!(project.len(), 2, "{:?}", b.parts);
        assert_eq!(project[0].tokens, tokens("Answer in French.\n"));
        assert!(b.total() >= project[0].tokens + project[1].tokens);
        write(&dir.join("FORGE.local.md"), "  ").unwrap();
        assert!(!dir.join("FORGE.local.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
