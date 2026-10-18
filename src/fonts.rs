//! Which faces Forge draws with.
//!
//! The app bundles only openly licensed fonts. Its one interface face is
//! Archivo, in 400 and 600, for text and headings alike; code is JetBrains
//! Mono. Hanken Grotesk and Source Serif 4 stay bundled for older content
//! that names them. Fonts placed as .ttf/.otf files in Forge's `fonts`
//! folder (next to its settings) are still read at start, so a document can
//! use them, but they never replace Archivo.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gpui::{App, SharedString};

/// Interface text: the first of these found, else the bundled face. None
/// is preferred over Archivo: the system is set in it.
pub const SANS_PREFERRED: &[&str] = &[];
pub const SANS_BUNDLED: &str = "Archivo";
/// Large lines (the greeting, headings): Archivo too, in its heavier cut.
pub const DISPLAY_PREFERRED: &[&str] = &[];
pub const DISPLAY_BUNDLED: &str = "Archivo";

/// A user font file larger than this is skipped, and at most this many are read.
const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 64;

struct Faces {
    sans: SharedString,
    display: SharedString,
}

static FACES: OnceLock<Faces> = OnceLock::new();

/// The interface face.
pub fn sans() -> SharedString {
    FACES
        .get()
        .map_or_else(|| SANS_BUNDLED.into(), |f| f.sans.clone())
}

/// The display serif.
pub fn display() -> SharedString {
    FACES
        .get()
        .map_or_else(|| DISPLAY_BUNDLED.into(), |f| f.display.clone())
}

/// The first preferred family that is available (any case), else `fallback`.
pub fn pick(available: &[String], preferred: &[&str], fallback: &str) -> SharedString {
    preferred
        .iter()
        .find_map(|want| {
            available
                .iter()
                .find(|name| name.eq_ignore_ascii_case(want))
        })
        .map(|name| SharedString::from(name.clone()))
        .unwrap_or_else(|| fallback.into())
}

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

/// At start, before the theme: load the fonts folder, then settle the faces
/// from everything the text system can see.
pub fn choose(cx: &App) {
    if let Some(dir) = user_dir() {
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
    let _ = FACES.set(Faces {
        sans: pick(&available, SANS_PREFERRED, SANS_BUNDLED),
        display: pick(&available, DISPLAY_PREFERRED, DISPLAY_BUNDLED),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archivo_sets_everything_whatever_is_installed() {
        let installed = vec![
            "Anthropic Sans".to_string(),
            "Hanken Grotesk".into(),
            "Segoe UI".into(),
        ];
        assert_eq!(pick(&installed, SANS_PREFERRED, SANS_BUNDLED), "Archivo");
        assert_eq!(
            pick(&installed, DISPLAY_PREFERRED, DISPLAY_BUNDLED),
            "Archivo"
        );
        // The picking itself still prefers, in order and in any case.
        let want = ["B Face", "A Face"];
        let have = vec!["a face".to_string(), "b face".into()];
        assert_eq!(pick(&have, &want, "x"), "b face");
        assert_eq!(pick(&[], &want, "x"), "x");
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
