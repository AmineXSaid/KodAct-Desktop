//! Which faces Forge draws with: Anthropic Sans, and nothing else.
//!
//! Its Text cut sets every word (interface, replies, code, the terminal,
//! diagrams); its Display cut sets the large lines. The app bundles no
//! font. Anthropic Sans is read from the `fonts` folder beside the app,
//! where the installer puts it (`installer/private-fonts`, a personal
//! build's, never committed), from Forge's `fonts` folder next to its
//! settings, and from the system.

use std::path::{Path, PathBuf};

use gpui::App;

/// Every word but the large lines.
pub const TEXT: &str = "Anthropic Sans Text";
/// The greeting and headings.
pub const DISPLAY: &str = "Anthropic Sans Display";

/// A user font file larger than this is skipped, and at most this many are read.
const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 64;

/// Forge's own fonts folder: `fonts` beside its settings.
pub fn user_dir() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("fonts"))
}

/// The .ttf / .otf files in `dir`, in name order, within the limits.
pub fn read_dir_fonts(dir: &Path) -> Vec<Vec<u8>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("ttf") || x.eq_ignore_ascii_case("otf"))
        })
        .filter(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE)
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .take(MAX_FILES)
        .filter_map(|p| std::fs::read(p).ok())
        .collect()
}

/// The `fonts` folder beside the app's executable.
pub fn app_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|d| d.join("fonts"))
}

/// Where Anthropic Sans is read from, in order: beside the app (the
/// installer's copy), Forge's own folder, and in a development build the
/// checkout's `installer/private-fonts`.
pub fn dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [app_dir(), user_dir()].into_iter().flatten().collect();
    if cfg!(debug_assertions) {
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("installer/private-fonts"));
    }
    dirs
}

/// At start, before the theme: load the fonts folders, and say so when
/// Anthropic Sans is nowhere (the system then substitutes its own face).
pub fn choose(cx: &App) {
    for dir in dirs() {
        let files = read_dir_fonts(&dir);
        if !files.is_empty() {
            if let Err(e) = cx
                .text_system()
                .add_fonts(files.into_iter().map(Into::into).collect())
            {
                eprintln!("could not load the fonts in {}: {e}", dir.display());
            }
        }
    }
    let available = cx.text_system().all_font_names();
    for want in [TEXT, DISPLAY] {
        if !has(&available, want) {
            eprintln!(
                "fonts: {want} is not installed; put Anthropic Sans in {:?}",
                user_dir()
            );
        }
    }
}

/// Whether the text system has `family` (any case).
pub fn has(available: &[String], family: &str) -> bool {
    available.iter().any(|n| n.eq_ignore_ascii_case(family))
}

/// The regular Text cut's file, for the diagram renderer, which reads its
/// faces as bytes (`assets.rs`).
pub fn text_regular() -> Option<Vec<u8>> {
    dirs().into_iter().find_map(|d| {
        let entries = std::fs::read_dir(&d).ok()?;
        let path = entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(str::to_ascii_lowercase)
                .is_some_and(|n| n.starts_with("anthropicsans-text-regular"))
        })?;
        std::fs::read(path).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_sans_is_the_only_face_and_none_is_bundled() {
        assert!(has(&["anthropic sans text".to_string()], TEXT));
        assert!(!has(&["Segoe UI".to_string()], TEXT));
        // The app carries no font of its own.
        assert!(
            !std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts")).exists()
        );
        assert!(!include_str!("assets.rs").contains("assets/fonts/"));
    }

    #[test]
    fn reads_only_font_files() {
        let dir = std::env::temp_dir().join(format!("forge-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.ttf"), b"x").unwrap();
        std::fs::write(dir.join("b.OTF"), b"y").unwrap();
        std::fs::write(dir.join("c.woff2"), b"z").unwrap();
        std::fs::write(dir.join("notes.txt"), b"n").unwrap();
        assert_eq!(read_dir_fonts(&dir), vec![b"x".to_vec(), b"y".to_vec()]);
        assert!(read_dir_fonts(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
