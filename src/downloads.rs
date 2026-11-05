//! Saving a file KodAct made into your Downloads folder, the way a browser
//! does: under its own name, or "name (1).md" when that is taken. Never
//! overwrites, and never writes outside the folder.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Your Downloads folder.
pub fn dir() -> Option<PathBuf> {
    crate::rules::home_dir().map(|h| h.join("Downloads"))
}

/// A bare file name: no folders, no `..`, nothing Windows refuses.
pub fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "document.md".into()
    } else {
        cleaned
    }
}

/// The first free path for `name` in `dir`: `name`, else `name (1)`…
pub fn free_path(dir: &Path, name: &str) -> PathBuf {
    let name = clean_name(name);
    let free = candidates(&name).map(|n| dir.join(n)).find(|p| !p.exists());
    free.unwrap_or_else(|| dir.join(&name))
}

/// The names to try, in order: `plan.md`, `plan (1).md`, `plan (2).md`…
fn candidates(name: &str) -> impl Iterator<Item = String> + '_ {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    std::iter::once(name.to_string()).chain((1..1000).map(move |n| format!("{stem} ({n}){ext}")))
}

/// Writes `bytes` into `dir` under `name`, or the first free variant of it.
/// Returns where it went. Blocking: call it off the UI thread.
pub fn save(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = clean_name(name);
    for candidate in candidates(&name) {
        let path = dir.join(&candidate);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(bytes)?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("every name for this file is taken"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_stay_inside_the_folder() {
        assert_eq!(clean_name("plan.md"), "plan.md");
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name(r"C:\Users\x\plan.md"), "plan.md");
        assert_eq!(clean_name("a:b?.md"), "a_b_.md");
        assert_eq!(clean_name(".."), "document.md");
        assert_eq!(clean_name(""), "document.md");
    }

    #[test]
    fn never_overwrites() {
        let dir = std::env::temp_dir().join(format!("forge-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = save(&dir, "plan.md", b"one").unwrap();
        let b = save(&dir, "plan.md", b"two").unwrap();
        let c = save(&dir, "README", b"three").unwrap();
        let d = save(&dir, "README", b"four").unwrap();
        assert_eq!(a.file_name().unwrap(), "plan.md");
        assert_eq!(b.file_name().unwrap(), "plan (1).md");
        assert_eq!(d.file_name().unwrap(), "README (1)");
        assert_eq!(std::fs::read(&a).unwrap(), b"one");
        assert_eq!(std::fs::read(&b).unwrap(), b"two");
        assert_eq!(std::fs::read(&c).unwrap(), b"three");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
