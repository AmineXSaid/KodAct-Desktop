//! Documents being made, watched while the turn runs: a deck KodAct builds
//! slide by slide shows in the chat from the first command that names it,
//! and its card and the side panel redraw each time the file settles on disk,
//! even when the command that changed it (`python build.py`) never names it.
//!
//! Polling, not a filesystem watcher: one `metadata` call per file every
//! [`EVERY`] costs nothing, works the same on every Windows filesystem
//! (network drives and OneDrive folders included, where change notifications
//! are unreliable), and needs no new dependency. A change counts once the
//! file's size and time have held still for one poll, so a deck is never read
//! half-written; the tools also write beside the file and rename it into
//! place. Office's own lock files (`~$deck.pptx`) are never watched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How often a watched file is looked at.
pub const EVERY: Duration = Duration::from_millis(600);

/// The kinds of file watched: every document the side panel shows as one
/// (Markdown, Word, Excel, PowerPoint, PDF, an HTML page, an SVG).
pub fn is_watched(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if name.starts_with("~$") || name.ends_with(".part") {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    [
        ".pptx",
        ".docx",
        ".xlsx",
        ".pdf",
        ".md",
        ".markdown",
        ".html",
        ".htm",
        ".svg",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
}

/// The documents a command line or a path names (`init deck.pptx`,
/// `--out=report.docx`, `"C:\\work\\notes.md"`), at most eight.
pub fn named(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for word in text.split_whitespace() {
        let w = word
            .trim_matches(|c: char| {
                matches!(c, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | ',' | ';')
            })
            .trim_end_matches(['.', ':']);
        let w = w.rsplit_once('=').map_or(w, |(_, v)| v);
        if w.len() > 3 && is_watched(w) && !out.iter().any(|o| o == w) {
            out.push(w.to_string());
        }
        if out.len() == 8 {
            break;
        }
    }
    out
}

/// What a file looked like at one poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub len: u64,
    pub modified: Option<SystemTime>,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        m.is_file().then(|| Stamp {
            len: m.len(),
            modified: m.modified().ok(),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct Entry {
    /// The version last shown (None: not shown yet).
    shown: Option<Stamp>,
    /// What the last poll saw, waiting to hold still.
    seen: Option<Stamp>,
}

/// The files one chat is making.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    files: HashMap<PathBuf, Entry>,
}

impl Watch {
    /// Watch `path`; true when it is new to the watch.
    pub fn add(&mut self, path: PathBuf) -> bool {
        if !is_watched(&path.to_string_lossy()) || self.files.contains_key(&path) {
            return false;
        }
        self.files.insert(path, Entry::default());
        true
    }

    /// A version of `path` was shown (its card was placed some other way):
    /// the next poll doesn't show it again.
    pub fn shown(&mut self, path: &Path, stamp: Option<Stamp>) {
        if let Some(e) = self.files.get_mut(path) {
            e.shown = stamp;
            e.seen = stamp;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.files.keys()
    }

    /// One poll, with `stamp_of` reading each file: the files whose new
    /// version has settled (the same as at the last poll, and not shown yet).
    pub fn poll(&mut self, stamp_of: impl Fn(&Path) -> Option<Stamp>) -> Vec<PathBuf> {
        let mut ready = vec![];
        for (path, e) in self.files.iter_mut() {
            let now = stamp_of(path);
            if now.is_some() && now == e.seen && now != e.shown {
                e.shown = now;
                ready.push(path.clone());
            }
            e.seen = now;
        }
        ready.sort();
        ready
    }
}

/// Folders a document the person asked for is never in: tools', caches' and
/// the skills' own render output (`preview/`, the slide images a check draws).
const SKIPPED_DIRS: [&str; 10] = [
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    "preview",
    "prev",
    "previews",
    "thumbnails",
    "dist",
    "build",
];

/// The documents in `root` changed since `since` that a command made without
/// naming them (`python build_deck.py` writes `deck.pptx`): Word, Excel and
/// PowerPoint files up to two folders down, and PDFs in `root` itself (deeper
/// ones are mostly a check's render of a deck). Bounded, so a big project costs
/// a quick look: at most `MAX_SEEN` entries, hidden and tool folders skipped.
/// Newest first.
pub fn recent_documents(root: &Path, since: SystemTime) -> Vec<PathBuf> {
    const MAX_SEEN: usize = 4000;
    let mut found: Vec<(SystemTime, PathBuf)> = vec![];
    let mut seen = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0u8)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            seen += 1;
            if seen > MAX_SEEN {
                break;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                let lower = name.to_ascii_lowercase();
                if depth < 2 && !name.starts_with('.') && !SKIPPED_DIRS.contains(&lower.as_str()) {
                    stack.push((e.path(), depth + 1));
                }
                continue;
            }
            let lower = name.to_ascii_lowercase();
            let office = [".pptx", ".docx", ".xlsx"]
                .iter()
                .any(|x| lower.ends_with(x));
            let pdf = depth == 0 && lower.ends_with(".pdf");
            if !(office || pdf) || !is_watched(&name) {
                continue;
            }
            if let Some(at) = e.metadata().ok().and_then(|m| m.modified().ok()) {
                if at >= since {
                    found.push((at, e.path()));
                }
            }
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found.into_iter().map(|(_, p)| p).take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn documents_a_script_made_are_found_without_being_named() {
        let root = std::env::temp_dir().join(format!("kodact-recent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["out", "preview", "node_modules", ".git", "a/b/c"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let before = SystemTime::now() - Duration::from_secs(5);
        for f in [
            "deck.pptx",
            "out/report.docx",
            "summary.pdf",
            "out/render.pdf",
            "preview/deck.pdf",
            "preview/deck.pptx",
            "node_modules/x.xlsx",
            ".git/y.docx",
            "a/b/c/deep.xlsx",
            "~$deck.pptx",
            "notes.md",
            "deck.pptx.part",
        ] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        let mut got: Vec<String> = recent_documents(&root, before)
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        got.sort();
        assert_eq!(got, ["deck.pptx", "out/report.docx", "summary.pdf"]);
        // Nothing changed since now.
        assert!(recent_documents(&root, SystemTime::now() + Duration::from_secs(60)).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn stamp(len: u64, secs: u64) -> Option<Stamp> {
        Some(Stamp {
            len,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)),
        })
    }

    #[test]
    fn office_files_are_watched_but_not_their_lock_files() {
        assert!(is_watched(r"C:\work\deck.pptx"));
        assert!(is_watched("out/Report.DOCX"));
        assert!(is_watched("a.pdf"));
        assert!(!is_watched(r"C:\work\~$deck.pptx"));
        assert!(!is_watched("deck.pptx.part"));
        assert!(is_watched("notes.md"));
        assert!(is_watched("page.html"));
        assert!(is_watched("chart.svg"));
        assert!(
            !is_watched("build_deck.py"),
            "a helper script is not a document"
        );
    }

    #[test]
    fn documents_are_found_in_commands() {
        assert_eq!(
            named(r#"python pptx_tool.py init "C:\work\deck.pptx" --theme kids"#),
            [r"C:\work\deck.pptx"]
        );
        assert_eq!(
            named("docx_tool.py create --out=memo.docx --markdown memo.md"),
            ["memo.docx", "memo.md"]
        );
        assert_eq!(named("xlsx_tool.py init budget.xlsx"), ["budget.xlsx"]);
        assert!(named("python build.py && ls").is_empty());
    }

    #[test]
    fn a_change_shows_once_it_holds_still() {
        let disk = RefCell::new(None);
        let read = |_: &Path| *disk.borrow();
        let mut w = Watch::default();
        assert!(w.add(PathBuf::from("deck.pptx")));
        assert!(!w.add(PathBuf::from("deck.pptx")), "watched once");
        assert!(w.poll(read).is_empty(), "not made yet");
        *disk.borrow_mut() = stamp(10, 1);
        assert!(w.poll(read).is_empty(), "just appeared: wait one poll");
        assert_eq!(w.poll(read), [PathBuf::from("deck.pptx")], "settled: shown");
        assert!(w.poll(read).is_empty(), "shown once");
        *disk.borrow_mut() = stamp(20, 2);
        assert!(w.poll(read).is_empty(), "still being written");
        *disk.borrow_mut() = stamp(30, 3);
        assert!(w.poll(read).is_empty(), "still being written");
        assert_eq!(w.poll(read), [PathBuf::from("deck.pptx")], "the new slide");
    }

    #[test]
    fn a_version_shown_another_way_is_not_shown_again() {
        let mut w = Watch::default();
        w.add(PathBuf::from("deck.pptx"));
        w.shown(Path::new("deck.pptx"), stamp(10, 1));
        assert!(w.poll(|_| stamp(10, 1)).is_empty());
    }

    #[test]
    fn stamps_read_the_disk() {
        let dir = std::env::temp_dir().join(format!("kodact-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("deck.pptx");
        let _ = std::fs::remove_file(&p);
        assert_eq!(Stamp::of(&p), None);
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(Stamp::of(&p).map(|s| s.len), Some(3));
        assert_eq!(Stamp::of(&dir), None, "a folder is not a file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
