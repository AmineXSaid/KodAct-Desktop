//! Which faces Forge draws with.
//!
//! The app bundles only openly licensed fonts: each theme sets its own
//! (Archivo for Calmly smooth, Inter, Geist, IBM Plex…), code in a mono.
//!
//! Anthropic Sans, when this computer has it, sets every face in every
//! theme: interface, replies, headings (its Display cut), and code, in
//! Anthropic Mono when that cut is there, else in the Text cut.
//! Fonts are read from Forge's `fonts` folder next to its settings and from
//! the `fonts` folder beside the app, where a personal build's installer
//! puts them (`installer/private-fonts`), as well as from the system. It is
//! Anthropic's own typeface: never in this repository; without it, the
//! themes keep theirs.

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

/// The faces that, when present, set all the text but code: the first
/// found of each, for text and for headings.
pub const BRAND_TEXT: &[&str] = &["Anthropic Sans Text", "Anthropic Sans"];
pub const BRAND_DISPLAY: &[&str] = &[
    "Anthropic Sans Display",
    "Anthropic Sans Text",
    "Anthropic Sans",
];
/// Code, with Anthropic Sans: its own mono, when there is one.
pub const BRAND_MONO: &[&str] = &["Anthropic Mono", "Anthropic Sans Mono"];

struct Faces {
    sans: SharedString,
    display: SharedString,
    /// Anthropic Sans, as the text system names it, when it is available:
    /// (text, headings).
    brand: Option<(SharedString, SharedString)>,
    /// Code's face with Anthropic Sans: its mono, else its text cut.
    brand_mono: Option<SharedString>,
}

static FACES: OnceLock<Faces> = OnceLock::new();

/// The interface face.
pub fn sans() -> SharedString {
    FACES
        .get()
        .map_or_else(|| SANS_BUNDLED.into(), |f| f.sans.clone())
}

/// Anthropic Sans, when this computer has it: (text, headings).
pub fn brand() -> Option<(SharedString, SharedString)> {
    FACES.get().and_then(|f| f.brand.clone())
}

/// Code's face when Anthropic Sans sets the app, so nothing else draws.
pub fn brand_mono() -> Option<SharedString> {
    FACES.get().and_then(|f| f.brand_mono.clone())
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

/// The `fonts` folder beside the app's executable.
pub fn app_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|d| d.join("fonts"))
}

/// At start, before the theme: load the fonts folders, then settle the
/// faces from everything the text system can see.
pub fn choose(cx: &App) {
    for dir in [app_dir(), user_dir()].into_iter().flatten() {
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
    let brand = brand_faces(&available);
    let brand_mono = brand
        .as_ref()
        .map(|(text, _)| find(&available, BRAND_MONO).unwrap_or_else(|| text.clone()));
    // Which of Anthropic Sans's families the app found, so a build can be
    // checked against the files it was given.
    let found: Vec<&String> = available
        .iter()
        .filter(|n| n.to_ascii_lowercase().contains("anthropic"))
        .collect();
    if !found.is_empty() {
        eprintln!("fonts: Anthropic families found: {found:?}; code in {brand_mono:?}");
    }
    let _ = FACES.set(Faces {
        sans: pick(&available, SANS_PREFERRED, SANS_BUNDLED),
        display: pick(&available, DISPLAY_PREFERRED, DISPLAY_BUNDLED),
        brand,
        brand_mono,
    });
}

/// The first of `families` the text system has, as it names it (any case).
pub fn find(available: &[String], families: &[&str]) -> Option<SharedString> {
    families.iter().find_map(|want| {
        available
            .iter()
            .find(|n| n.eq_ignore_ascii_case(want))
            .map(|n| SharedString::from(n.clone()))
    })
}

/// Anthropic Sans's text and heading faces, when there is a text face.
pub fn brand_faces(available: &[String]) -> Option<(SharedString, SharedString)> {
    let text = find(available, BRAND_TEXT)?;
    let display = find(available, BRAND_DISPLAY).unwrap_or_else(|| text.clone());
    Some((text, display))
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
    fn anthropic_sans_is_found_only_when_there() {
        let cuts = vec![
            "Anthropic Sans Display".to_string(),
            "Anthropic Sans Text".into(),
            "Segoe UI".into(),
        ];
        let (text, display) = brand_faces(&cuts).unwrap();
        assert_eq!(
            (text.as_ref(), display.as_ref()),
            ("Anthropic Sans Text", "Anthropic Sans Display")
        );
        // One family for both, in any case.
        let one = vec!["anthropic sans".to_string()];
        assert_eq!(
            brand_faces(&one).map(|b| b.1).as_deref(),
            Some("anthropic sans")
        );
        // A Display cut alone isn't enough for body text.
        assert_eq!(brand_faces(&["Anthropic Sans Display".to_string()]), None);
        assert_eq!(brand_faces(&["Segoe UI".to_string()]), None);
        // Never bundled: the app's own fonts don't include it.
        let bundled =
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts")).unwrap();
        assert!(bundled.filter_map(|e| e.ok()).all(|e| !e
            .file_name()
            .to_string_lossy()
            .to_lowercase()
            .contains("anthropic")));
        assert!(!include_str!("assets.rs")
            .to_lowercase()
            .contains("anthropic"));
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
