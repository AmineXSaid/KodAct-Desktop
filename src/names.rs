//! KodAct's names on disk, and ForgeCLI's from before the rename.
//!
//! The engine (KodAct, once ForgeCLI) keeps a project's old names working
//! until its own exist (`kodact-config`'s `renamed`): a project's folder is
//! `.kodact`, or `.forge` while that is the only one; a folder's
//! instructions are `KODACT.md`, or `FORGE.md` while that is the only one.
//! The app reads and writes through the same rule, so it edits the very file
//! the engine loads, and never makes a `.kodact` beside an old `.forge` (the
//! engine would then stop reading the old folder's settings).

use std::path::{Path, PathBuf};

/// A project's folder names, KodAct's first.
pub const PROJECT_DIRS: [&str; 2] = [".kodact", ".forge"];

/// `base/new`, unless only `base/old` exists.
pub fn renamed(base: &Path, new: &str, old: &str) -> PathBuf {
    let n = base.join(new);
    let o = base.join(old);
    if !n.exists() && o.exists() {
        o
    } else {
        n
    }
}

/// The name of a project's own folder: `.kodact`, or `.forge` when only
/// that one exists.
pub fn project_dir_name(project: &Path) -> &'static str {
    if !project.join(".kodact").exists() && project.join(".forge").exists() {
        ".forge"
    } else {
        ".kodact"
    }
}

/// A project's own folder (settings, knowledge, memory, attachments).
pub fn project_dir(project: &Path) -> PathBuf {
    project.join(project_dir_name(project))
}

/// `<project folder>/<rest>`, relative to the project, with `/` between.
pub fn in_project_dir(project: &Path, rest: &str) -> String {
    format!("{}/{rest}", project_dir_name(project))
}

/// Whether `rel` (relative to a project, `/` between) starts with
/// `<project folder>/<rest>` under either name. No disk access.
pub fn starts_in_project_dir(rel: &str, rest: &str) -> bool {
    PROJECT_DIRS
        .iter()
        .any(|d| rel.starts_with(&format!("{d}/{rest}")))
}

/// A folder's instructions file: `KODACT.md`, or `FORGE.md` when only that
/// one exists.
pub fn instructions(dir: &Path) -> PathBuf {
    renamed(dir, "KODACT.md", "FORGE.md")
}

/// A folder's own-computer instructions: `KODACT.local.md`, or
/// `FORGE.local.md` when only that one exists.
pub fn local_instructions(dir: &Path) -> PathBuf {
    renamed(dir, "KODACT.local.md", "FORGE.local.md")
}

/// Whether `name` is an instructions file's name, old or new.
pub fn is_instructions_name(name: &str) -> bool {
    matches!(name, "KODACT.md" | "FORGE.md")
}

/// Whether `name` is an own-computer instructions file's name, old or new.
pub fn is_local_instructions_name(name: &str) -> bool {
    matches!(name, "KODACT.local.md" | "FORGE.local.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_keeps_its_old_names_until_the_new_ones_exist() {
        let d = std::env::temp_dir().join(format!("kodact-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(
            project_dir(&d),
            d.join(".kodact"),
            "a new project gets KodAct's folder"
        );
        assert_eq!(instructions(&d), d.join("KODACT.md"));
        assert_eq!(local_instructions(&d), d.join("KODACT.local.md"));
        std::fs::create_dir_all(d.join(".forge")).unwrap();
        std::fs::write(d.join("FORGE.md"), "old").unwrap();
        std::fs::write(d.join("FORGE.local.md"), "mine").unwrap();
        assert_eq!(
            project_dir(&d),
            d.join(".forge"),
            "an old project keeps ForgeCLI's"
        );
        assert_eq!(in_project_dir(&d, "memory.md"), ".forge/memory.md");
        assert_eq!(instructions(&d), d.join("FORGE.md"));
        assert_eq!(local_instructions(&d), d.join("FORGE.local.md"));
        std::fs::create_dir_all(d.join(".kodact")).unwrap();
        std::fs::write(d.join("KODACT.md"), "new").unwrap();
        assert_eq!(
            project_dir(&d),
            d.join(".kodact"),
            "KodAct's wins once it exists"
        );
        assert_eq!(instructions(&d), d.join("KODACT.md"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn paths_match_under_either_name() {
        assert!(starts_in_project_dir(".forge/knowledge.json", "knowledge."));
        assert!(starts_in_project_dir(".kodact/knowledge.md", "knowledge."));
        assert!(!starts_in_project_dir("docs/knowledge.md", "knowledge."));
        assert!(is_instructions_name("FORGE.md") && is_instructions_name("KODACT.md"));
        assert!(is_local_instructions_name("KODACT.local.md"));
    }
}
