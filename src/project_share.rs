//! Sharing a project's setup: what makes Forge good in it (its shared
//! instructions, its knowledge, and if you choose its memory), never your
//! own `FORGE.local.md`.
//!
//! - **With the repo**: commit the setup files, so everyone who clones the
//!   project gets the same Forge.
//! - **As a file**: a `.forgeproject` (a zip: the files under `setup/`, and
//!   `project.json` with the name and description) to send to anyone; they
//!   import it into a project of theirs. Importing never overwrites a file
//!   that is already there.
//!
//! Everything here blocks: run it off the UI thread.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::json;

pub const EXT: &str = "forgeproject";

/// The project's setup files that exist, relative to it, with forward
/// slashes; `.forge/memory.md` only with `memory`.
pub fn setup_files(project: &Path, memory: bool) -> Vec<String> {
    let mut out = vec![];
    let shared = crate::project_ctx::shared_file(project);
    if shared.is_file() {
        if let Ok(rel) = shared.strip_prefix(project) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    for f in [".forge/knowledge.json", ".forge/knowledge.md"] {
        if project.join(f).is_file() {
            out.push(f.to_string());
        }
    }
    if let Ok(dir) = std::fs::read_dir(project.join(crate::knowledge::DIR)) {
        let mut files: Vec<String> = dir
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| format!("{}/{}", crate::knowledge::DIR, e.file_name().to_string_lossy()))
            .collect();
        files.sort();
        out.extend(files);
    }
    if memory && project.join(crate::project_memory::FILE).is_file() {
        out.push(crate::project_memory::FILE.to_string());
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
    // Counted as you'd recognise them: Forge's own list and compiled block
    // are committed with them, not counted.
    git(project, &args).ok().map(|o| {
        o.lines()
            .filter(|l| !l.trim().is_empty() && !l.contains(".forge/knowledge."))
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
    let mut commit = vec!["commit", "-m", "Forge project setup: instructions and knowledge", "--"];
    commit.extend(files.iter().map(String::as_str));
    git(project, &commit)
        .map(|_| "Committed the project's setup. Push it, and everyone who clones the project gets it.".to_string())
        .or_else(|e| {
            if e.contains("nothing to commit") || e.contains("no changes added") {
                Ok("The setup is already committed.".to_string())
            } else {
                Err(e)
            }
        })
}

/// Export the setup to `<dir>/<name>.forgeproject`; returns its path.
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
        "format": "forge-project-setup",
        "version": 1,
        "name": name,
        "description": description,
        "files": files,
    });
    zip.start_file("project.json", opts).map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(&meta).unwrap_or_default().as_bytes())
        .map_err(|e| e.to_string())?;
    for rel in &files {
        let bytes = std::fs::read(project.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        zip.start_file(format!("setup/{rel}"), opts).map_err(|e| e.to_string())?;
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

/// Import a `.forgeproject` into `project`, never overwriting a file.
pub fn import(file: &Path, project: &Path) -> Result<Imported, String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(f).map_err(|_| "this isn't a Forge project setup".to_string())?;
    let is_setup = zip
        .by_name("project.json")
        .ok()
        .and_then(|mut m| {
            let mut s = String::new();
            m.read_to_string(&mut s).ok()?;
            serde_json::from_str::<serde_json::Value>(&s).ok()
        })
        .is_some_and(|v| v["format"] == "forge-project-setup");
    if !is_setup {
        return Err("this isn't a Forge project setup".into());
    }
    let mut out = Imported::default();
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
        if shown.ends_with("FORGE.local.md") {
            continue;
        }
        let to = project.join(rel);
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
        assert!(!files.iter().any(|f| f.contains("memory") || f.contains("local")));
        assert!(setup_files(&a, true).contains(&crate::project_memory::FILE.to_string()));

        let out = project("out");
        let file = export(&a, "Billing API", "The API", true, &out).unwrap();
        assert_eq!(file.file_name().unwrap(), "Billing API.forgeproject");

        let b = project("b");
        std::fs::write(b.join("FORGE.md"), "Their own.").unwrap();
        let got = import(&file, &b).unwrap();
        assert_eq!(got.kept, vec!["FORGE.md".to_string()]);
        assert!(got.added.iter().any(|f| f.ends_with("glossary.md")));
        assert_eq!(std::fs::read_to_string(b.join("FORGE.md")).unwrap(), "Their own.");
        assert!(b.join(crate::project_memory::FILE).is_file());
        assert!(!b.join("FORGE.local.md").exists());
        // Something else isn't imported.
        std::fs::write(out.join("x.forgeproject"), b"not a zip").unwrap();
        assert!(import(&out.join("x.forgeproject"), &b).is_err());
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
