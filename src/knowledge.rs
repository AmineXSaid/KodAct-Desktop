//! A project's knowledge: reference files every chat in it can draw on
//! (specs, briefs, data, notes), as Claude's project knowledge is, but kept
//! as files in the project and in your control.
//!
//! - Files you add from elsewhere are copied into `.forge/knowledge/`;
//!   files already in the project are referenced where they are.
//! - Each is **Always** (its text goes into every new chat) or **On demand**
//!   (listed; Forge reads it with its own tools when it matters). Always
//!   files fit a budget (a share of the model's window); past it, the rest
//!   are listed On demand, and the rail says so.
//! - `.forge/knowledge.json` keeps the list and the choices;
//!   `.forge/knowledge.md` is the block compiled from them, sent ahead of a
//!   new chat's first message (`state.rs` `send_to_forge`), so its size is
//!   never limited by a command line.
//!
//! Everything here blocks: run it off the UI thread.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const DIR: &str = ".forge/knowledge";
const MANIFEST: &str = ".forge/knowledge.json";
const COMPILED: &str = ".forge/knowledge.md";
/// The largest text file read for its words.
const TEXT_MAX: u64 = 4 * 1024 * 1024;
/// The largest Office file or PDF read for its words.
const DOC_MAX: u64 = 64 * 1024 * 1024;
/// Always files' share of the model's window, and the budget when the
/// window isn't known.
pub const SHARE: f32 = 0.3;
pub const DEFAULT_BUDGET: u64 = 60_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Manifest {
    files: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Entry {
    /// From the project's folder, with forward slashes.
    path: String,
    #[serde(default)]
    always: bool,
}

/// One knowledge file, as the rail shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub rel: String,
    pub bytes: u64,
    /// What its words cost (0 for a file with no text: an image, a binary).
    pub tokens: u64,
    pub always: bool,
    /// Always, and within the budget: its text goes into new chats.
    pub included: bool,
    /// The file is gone from the folder.
    pub missing: bool,
}

/// Knowledge as compiled: its files, and what the Always ones cost.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Knowledge {
    pub items: Vec<Item>,
    pub budget: u64,
    pub included_tokens: u64,
}

fn manifest(project: &Path) -> Manifest {
    std::fs::read_to_string(project.join(MANIFEST))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_manifest(project: &Path, m: &Manifest) -> Result<(), String> {
    let path = project.join(MANIFEST);
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn rel_of(project: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(project)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}

/// A file's words: text as it is, an Office file or PDF as `office::text`
/// reads it; None for anything else.
pub fn text_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_string();
    let len = std::fs::metadata(path).ok()?.len();
    if let Some(kind) = crate::office::Kind::of(&name) {
        if len > DOC_MAX {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        return crate::office::text(kind, &bytes).ok();
    }
    if len > TEXT_MAX {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    // Text, not a binary that happens to be small.
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// The project's knowledge: the listed files, then any file dropped into
/// `.forge/knowledge/` by hand (On demand).
fn entries(project: &Path) -> Vec<Entry> {
    let mut files = manifest(project).files;
    if let Ok(dir) = std::fs::read_dir(project.join(DIR)) {
        let mut found: Vec<String> = dir
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| rel_of(project, &e.path()))
            .collect();
        found.sort();
        for rel in found {
            if !files.iter().any(|f| f.path == rel) {
                files.push(Entry {
                    path: rel,
                    always: false,
                });
            }
        }
    }
    files
}

/// Read the knowledge and write the block new chats get, Always files in
/// list order until `budget` tokens are spent. Returns what it found.
pub fn compile(project: &Path, budget: u64) -> Knowledge {
    let mut items = vec![];
    let mut block = String::new();
    let mut on_demand = vec![];
    let mut spent = 0;
    for e in entries(project) {
        let abs = project.join(&e.path);
        let meta = std::fs::metadata(&abs).ok();
        let text = meta.as_ref().and_then(|_| text_of(&abs));
        let tokens = text.as_deref().map_or(0, crate::project_ctx::tokens);
        let included = e.always && text.is_some() && spent + tokens <= budget;
        if included {
            spent += tokens;
            block.push_str(&format!(
                "<file path=\"{}\">\n{}\n</file>\n\n",
                e.path,
                text.as_deref().unwrap_or_default().trim_end()
            ));
        } else if meta.is_some() {
            on_demand.push(e.path.clone());
        }
        items.push(Item {
            rel: e.path,
            bytes: meta.as_ref().map_or(0, |m| m.len()),
            tokens,
            always: e.always,
            included,
            missing: meta.is_none(),
        });
    }
    let path = project.join(COMPILED);
    if items.iter().any(|i| !i.missing) {
        let mut out = String::from(
            "<project_knowledge>\nThis project's reference files, chosen by the user for every chat in it. Use them as the source of truth on what they cover, and say which file an answer comes from.\n\n",
        );
        out.push_str(&block);
        if !on_demand.is_empty() {
            out.push_str("Also in the project's knowledge, to read with your file tools when a question touches them:\n");
            for p in &on_demand {
                out.push_str(&format!("- {p}\n"));
            }
        }
        out.push_str("</project_knowledge>\n");
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(&path, out);
    } else {
        let _ = std::fs::remove_file(&path);
    }
    Knowledge {
        items,
        budget,
        included_tokens: spent,
    }
}

/// The block a new chat's first message carries, as last compiled.
pub fn block(project: &Path) -> Option<String> {
    let path = project.join(COMPILED);
    // A compiled block is bounded by its budget; this only guards a file
    // edited by hand.
    if std::fs::metadata(&path).ok()?.len() > 8 * 1024 * 1024 {
        return None;
    }
    std::fs::read_to_string(path)
        .ok()
        .filter(|t| !t.trim().is_empty())
}

/// A free name in `dir` for `name`: itself, else `name (2).ext`, …
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Add files: one in the project is referenced where it is; one from
/// elsewhere is copied into `.forge/knowledge/`. New files start On demand.
/// Returns how many were added.
pub fn add(project: &Path, paths: &[PathBuf]) -> Result<usize, String> {
    let mut m = manifest(project);
    let mut added = 0;
    for p in paths.iter().filter(|p| p.is_file()) {
        let rel = match rel_of(project, p) {
            Some(rel) if !rel.starts_with(".forge/knowledge.") => rel,
            _ => {
                let dir = project.join(DIR);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let name = p
                    .file_name()
                    .map(|n| crate::stage::safe_name(&n.to_string_lossy()))
                    .unwrap_or_else(|| "file".into());
                let to = free_name(&dir, &name);
                std::fs::copy(p, &to).map_err(|e| format!("{}: {e}", p.display()))?;
                rel_of(project, &to).ok_or("the copy isn't in the project")?
            }
        };
        if !m.files.iter().any(|f| f.path == rel) {
            m.files.push(Entry {
                path: rel,
                always: false,
            });
            added += 1;
        }
    }
    save_manifest(project, &m)?;
    Ok(added)
}

/// Add a note you wrote, as a Markdown file in `.forge/knowledge/`, Always.
pub fn add_text(project: &Path, title: &str, text: &str) -> Result<String, String> {
    let dir = project.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem = crate::rules::slug(title).unwrap_or_else(|| "note".into());
    let to = free_name(&dir, &format!("{stem}.md"));
    let body = if title.trim().is_empty() {
        format!("{}\n", text.trim_end())
    } else {
        format!("# {}\n\n{}\n", title.trim(), text.trim_end())
    };
    std::fs::write(&to, body).map_err(|e| e.to_string())?;
    let rel = rel_of(project, &to).ok_or("the note isn't in the project")?;
    let mut m = manifest(project);
    m.files.retain(|f| f.path != rel);
    m.files.push(Entry {
        path: rel.clone(),
        always: true,
    });
    save_manifest(project, &m)?;
    Ok(rel)
}

/// Always, or On demand.
pub fn set_always(project: &Path, rel: &str, always: bool) -> Result<(), String> {
    let mut m = Manifest {
        files: entries(project),
    };
    match m.files.iter_mut().find(|f| f.path == rel) {
        Some(f) => f.always = always,
        None => return Err(format!("{rel} isn't in the project's knowledge")),
    }
    save_manifest(project, &m)
}

/// Take a file out of the knowledge: a copy Forge made is deleted; a file
/// of the project's own stays where it is.
pub fn remove(project: &Path, rel: &str) -> Result<(), String> {
    let mut m = Manifest {
        files: entries(project),
    };
    m.files.retain(|f| f.path != rel);
    if rel.starts_with(&format!("{DIR}/")) {
        let _ = std::fs::remove_file(project.join(rel));
    }
    save_manifest(project, &m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-kn-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn knowledge_is_added_pinned_compiled_and_removed() {
        let p = project("a");
        let outside = std::env::temp_dir().join(format!("forge-kn-spec-{}.md", std::process::id()));
        std::fs::write(&outside, "The API answers in JSON.").unwrap();
        std::fs::write(p.join("README.md"), "Run with pnpm dev.").unwrap();
        // From elsewhere: copied in. From the project: referenced.
        assert_eq!(add(&p, &[outside.clone(), p.join("README.md")]).unwrap(), 2);
        let copy = format!("{DIR}/{}", outside.file_name().unwrap().to_string_lossy());
        assert!(p.join(&copy).is_file());
        // Both start On demand: listed, no text sent.
        let k = compile(&p, 1000);
        assert_eq!(k.items.len(), 2);
        assert!(k.items.iter().all(|i| !i.always && !i.included));
        let b = block(&p).unwrap();
        assert!(b.contains(&format!("- {copy}")) && b.contains("- README.md"));
        assert!(!b.contains("answers in JSON"));
        // Always: its words go in.
        set_always(&p, &copy, true).unwrap();
        let k = compile(&p, 1000);
        assert!(k.items.iter().find(|i| i.rel == copy).unwrap().included);
        assert!(block(&p).unwrap().contains("The API answers in JSON."));
        // Past the budget it is listed instead.
        let k = compile(&p, 2);
        assert!(!k.items.iter().find(|i| i.rel == copy).unwrap().included);
        assert_eq!(k.included_tokens, 0);
        // A note is written as Markdown, Always.
        let note = add_text(&p, "Tone of voice", "Plain words.").unwrap();
        assert_eq!(note, format!("{DIR}/tone-of-voice.md"));
        assert!(compile(&p, 1000).items.iter().any(|i| i.rel == note && i.included));
        // Removing a copy deletes it; the project's own file stays.
        remove(&p, &copy).unwrap();
        remove(&p, "README.md").unwrap();
        assert!(!p.join(&copy).exists() && p.join("README.md").exists());
        assert_eq!(compile(&p, 1000).items.len(), 1);
        remove(&p, &note).unwrap();
        compile(&p, 1000);
        assert_eq!(block(&p), None, "no knowledge, no block");
        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn a_file_dropped_in_by_hand_is_knowledge_too() {
        let p = project("b");
        std::fs::create_dir_all(p.join(DIR)).unwrap();
        std::fs::write(p.join(DIR).join("brief.txt"), "Launch in May.").unwrap();
        std::fs::write(p.join(DIR).join("logo.bin"), [0u8, 1, 2, 0]).unwrap();
        let k = compile(&p, 1000);
        assert_eq!(k.items.len(), 2);
        let bin = k.items.iter().find(|i| i.rel.ends_with("logo.bin")).unwrap();
        assert_eq!(bin.tokens, 0, "a binary has no words to send");
        let _ = std::fs::remove_dir_all(&p);
    }
}
