//! Sharing a project's setup: what makes KodAct good in it (its shared
//! instructions, its knowledge, and if you choose its memory), never your
//! own `KODACT.local.md`.
//!
//! - **With the repo**: commit the setup files, so everyone who clones the
//!   project gets the same KodAct.
//! - **As a file**: a `.kodactproject` (a zip: the files under `setup/`, and
//!   `project.json` with the name and description) to send to anyone; they
//!   import it into a project of theirs. Importing never overwrites a file
//!   that is already there.
//!
//! Everything here blocks: run it off the UI thread.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::json;

pub const EXT: &str = "kodactproject";
/// The format a setup's `project.json` names, and Forge's.
const FORMAT: &str = "kodact-project-setup";
const OLD_FORMAT: &str = "forge-project-setup";

/// The project's setup files that exist, relative to it, with forward
/// slashes; the memory (`.kodact/memory.md`) only with `memory`.
pub fn setup_files(project: &Path, memory: bool) -> Vec<String> {
    let mut out = vec![];
    let shared = crate::project_ctx::shared_file(project);
    if shared.is_file() {
        if let Ok(rel) = shared.strip_prefix(project) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    for f in ["knowledge.json", "knowledge.md"] {
        let f = crate::names::in_project_dir(project, f);
        if project.join(&f).is_file() {
            out.push(f);
        }
    }
    let knowledge = crate::knowledge::dir(project);
    if let Ok(dir) = std::fs::read_dir(project.join(&knowledge)) {
        let mut files: Vec<String> = dir
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| {
                format!("{knowledge}/{}", e.file_name().to_string_lossy())
            })
            .collect();
        files.sort();
        out.extend(files);
    }
    let memory_file = crate::project_memory::file(project);
    if memory && project.join(&memory_file).is_file() {
        out.push(memory_file);
    }
    out
}

fn git(project: &Path, args: &[&str]) -> Result<String, String> {
    let mut c = std::process::Command::new("git");
    c.args(args)
        .current_dir(project)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = c.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr)
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("git failed")
            .trim()
            .to_string())
    }
}

/// How many of the setup files git would commit (new or changed); None
/// when the project isn't a repository.
pub fn uncommitted(project: &Path, memory: bool) -> Option<usize> {
    if !project.join(".git").exists() {
        return None;
    }
    let files = setup_files(project, memory);
    if files.is_empty() {
        return Some(0);
    }
    let mut args = vec!["status", "--porcelain", "--untracked-files=all", "--"];
    args.extend(files.iter().map(String::as_str));
    // Counted as you'd recognise them: KodAct's own list and compiled block
    // are committed with them, not counted.
    git(project, &args).ok().map(|o| {
        o.lines()
            .filter(|l| {
                !l.trim().is_empty()
                    && !crate::names::PROJECT_DIRS
                        .iter()
                        .any(|d| l.contains(&format!("{d}/knowledge.")))
            })
            .count()
    })
}

/// Commit the setup files (only them), so the project's clones get them.
pub fn commit(project: &Path, memory: bool) -> Result<String, String> {
    let files = setup_files(project, memory);
    if files.is_empty() {
        return Err("there are no instructions or knowledge to share yet".into());
    }
    let mut add = vec!["add", "--"];
    add.extend(files.iter().map(String::as_str));
    git(project, &add)?;
    let mut commit = vec![
        "commit",
        "-m",
        "KodAct project setup: instructions and knowledge",
        "--",
    ];
    commit.extend(files.iter().map(String::as_str));
    git(project, &commit)
        .map(|_| {
            "Committed the project's setup. Push it, and everyone who clones the project gets it."
                .to_string()
        })
        .or_else(|e| {
            if e.contains("nothing to commit") || e.contains("no changes added") {
                Ok("The setup is already committed.".to_string())
            } else {
                Err(e)
            }
        })
}

/// Export the setup to `<dir>/<name>.kodactproject`; returns its path.
pub fn export(
    project: &Path,
    name: &str,
    description: &str,
    memory: bool,
    dir: &Path,
) -> Result<PathBuf, String> {
    let files = setup_files(project, memory);
    if files.is_empty() {
        return Err("there are no instructions or knowledge to share yet".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stem = crate::templates::folder_name(name).unwrap_or_else(|| "project".into());
    let mut path = dir.join(format!("{stem}.{EXT}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}).{EXT}"));
        n += 1;
    }
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let meta = json!({
        "format": FORMAT,
        "version": 1,
        "name": name,
        "description": description,
        "files": files,
    });
    zip.start_file("project.json", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(
        serde_json::to_string_pretty(&meta)
            .unwrap_or_default()
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    for rel in &files {
        let bytes = std::fs::read(project.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        zip.start_file(format!("setup/{rel}"), opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

/// What an import did: files added, and files left alone because the
/// project already had them.
#[derive(Debug, Default, PartialEq)]
pub struct Imported {
    pub added: Vec<String>,
    pub kept: Vec<String>,
}

/// Import a `.kodactproject` into `project`, never overwriting a file.
pub fn import(file: &Path, project: &Path) -> Result<Imported, String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut zip =
        zip::ZipArchive::new(f).map_err(|_| "this isn't a KodAct project setup".to_string())?;
    let is_setup = zip
        .by_name("project.json")
        .ok()
        .and_then(|mut m| {
            let mut s = String::new();
            m.read_to_string(&mut s).ok()?;
            serde_json::from_str::<serde_json::Value>(&s).ok()
        })
        .is_some_and(|v| v["format"] == FORMAT || v["format"] == OLD_FORMAT);
    if !is_setup {
        return Err("this isn't a KodAct project setup".into());
    }
    let mut out = Imported::default();
    // A setup names its folder and instructions as the project it came from
    // did (`.forge/`, `FORGE.md` before the rename); they land under this
    // project's own names, where the engine reads them (`names.rs`).
    let dir_name = crate::names::project_dir_name(project);
    let instructions = crate::names::instructions(project)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "KODACT.md".into());
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        // Only what is under setup/, and never outside the project.
        let Some(inner) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rel) = inner.strip_prefix("setup") else {
            continue;
        };
        if entry.is_dir() || rel.as_os_str().is_empty() {
            continue;
        }
        let shown = rel.to_string_lossy().replace('\\', "/");
        // Your own instructions never travel in a setup.
        if crate::names::is_local_instructions_name(shown.rsplit('/').next().unwrap_or(&shown)) {
            continue;
        }
        let shown = match shown.split_once('/') {
            Some((top, rest)) if crate::names::PROJECT_DIRS.contains(&top) => {
                format!("{dir_name}/{rest}")
            }
            _ if crate::names::is_instructions_name(&shown) => instructions.clone(),
            _ => shown,
        };
        let to = project.join(&shown);
        if to.exists() {
            out.kept.push(shown);
            continue;
        }
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut bytes = vec![];
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        std::fs::write(&to, bytes).map_err(|e| e.to_string())?;
        out.added.push(shown);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-share-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_setup_round_trips_and_import_never_overwrites() {
        let a = project("a");
        std::fs::write(a.join("FORGE.md"), "Use pnpm.").unwrap();
        std::fs::write(a.join("FORGE.local.md"), "Mine only.").unwrap();
        crate::knowledge::add_text(&a, "Glossary", "Dunning: reminders.").unwrap();
        crate::project_memory::write(&a, "# Memory\n\n## Decisions\n- Cents.").unwrap();
        let files = setup_files(&a, false);
        assert!(files.contains(&"FORGE.md".to_string()));
        assert!(files.iter().any(|f| f.ends_with("glossary.md")));
        assert!(!files
            .iter()
            .any(|f| f.contains("memory") || f.contains("local")));
        assert!(setup_files(&a, true).contains(&crate::project_memory::file(&a)));

        let out = project("out");
        let file = export(&a, "Billing API", "The API", true, &out).unwrap();
        assert_eq!(file.file_name().unwrap(), "Billing API.kodactproject");

        let b = project("b");
        std::fs::write(b.join("FORGE.md"), "Their own.").unwrap();
        let got = import(&file, &b).unwrap();
        assert_eq!(got.kept, vec!["FORGE.md".to_string()]);
        assert!(got.added.iter().any(|f| f.ends_with("glossary.md")));
        assert_eq!(
            std::fs::read_to_string(b.join("FORGE.md")).unwrap(),
            "Their own."
        );
        assert!(b.join(crate::project_memory::file(&b)).is_file());
        assert!(!b.join("FORGE.local.md").exists());
        // Something else isn't imported.
        std::fs::write(out.join("x.kodactproject"), b"not a zip").unwrap();
        assert!(import(&out.join("x.kodactproject"), &b).is_err());
        for d in [a, b, out] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_folder_that_isnt_a_repository_has_nothing_to_commit() {
        let p = project("c");
        assert_eq!(uncommitted(&p, false), None);
        let _ = std::fs::remove_dir_all(&p);
    }
}
