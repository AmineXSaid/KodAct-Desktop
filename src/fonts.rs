//! KodAct's faces, carried in the app: KodAct Sans for every word (the
//! interface, replies, headings) and KodAct Mono for code, paths and keys.
//! They are KodAct's cuts of Host Grotesk and JetBrains Mono (SIL Open Font
//! License 1.1, `assets/fonts/OFL-*.txt`): their variable fonts cut into the
//! static weights the app sets (400, 500, 600, 700 and italics; 400, 700
//! and italic), the mono without its `->` ligatures so code reads as typed,
//! and named KodAct's own so a copy installed on the computer (ligatures
//! and all) is never drawn in their place. The wordmark is not
//! a font: it is drawn from KodAct CLI's pixel letters (`ui/mark.rs`).
//!
//! Fonts you put in KodAct's `fonts` folder beside its settings are loaded
//! too, so a face of your own can be named in a theme.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use gpui::App;

/// Every word but code.
pub const TEXT: &str = "KodAct Sans";
/// Headings and large lines: the same face, set heavier (`Theme::display_weight`).
pub const DISPLAY: &str = "KodAct Sans";
/// Code, paths, commands and keys.
pub const MONO: &str = "KodAct Mono";

/// The regular cut of the interface face, for the diagram renderer.
const TEXT_REGULAR: &[u8] = include_bytes!("../assets/fonts/HostGrotesk-Regular.ttf");
/// The regular cut of the code face, likewise.
const MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

/// The faces the app carries.
const BUNDLED: &[&[u8]] = &[
    TEXT_REGULAR,
    include_bytes!("../assets/fonts/HostGrotesk-Medium.ttf"),
    include_bytes!("../assets/fonts/HostGrotesk-SemiBold.ttf"),
    include_bytes!("../assets/fonts/HostGrotesk-Bold.ttf"),
    include_bytes!("../assets/fonts/HostGrotesk-Italic.ttf"),
    include_bytes!("../assets/fonts/HostGrotesk-SemiBoldItalic.ttf"),
    MONO_REGULAR,
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
];

/// A user font file larger than this is skipped, and at most this many are read.
const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 64;

/// KodAct's own fonts folder: `fonts` beside its settings.
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

/// At start, before the theme: KodAct's faces, then any in the user's
/// fonts folder.
pub fn choose(cx: &App) {
    let bundled = BUNDLED.iter().map(|b| Cow::Borrowed(*b)).collect();
    if let Err(e) = cx.text_system().add_fonts(bundled) {
        eprintln!("fonts: KodAct's faces could not be loaded: {e}");
    }
    if let Some(dir) = user_dir() {
        let files = read_dir_fonts(&dir);
        if !files.is_empty() {
            if let Err(e) = cx
                .text_system()
                .add_fonts(files.into_iter().map(Into::into).collect())
            {
                eprintln!("fonts: could not load the fonts in {}: {e}", dir.display());
            }
        }
    }
}

/// The interface face's regular cut, for the diagram renderer, which reads
/// its faces as bytes (`assets.rs`).
pub fn text_regular() -> &'static [u8] {
    TEXT_REGULAR
}

/// The code face's regular cut, likewise.
pub fn mono_regular() -> &'static [u8] {
    MONO_REGULAR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kodacts_faces_are_carried_with_their_licences() {
        assert_eq!((TEXT, DISPLAY, MONO), ("KodAct Sans", "KodAct Sans", "KodAct Mono"));
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts"));
        for licence in ["OFL-HostGrotesk.txt", "OFL-JetBrainsMono.txt"] {
            let text = std::fs::read_to_string(dir.join(licence)).unwrap();
            assert!(text.contains("SIL Open Font License, Version 1.1"), "{licence}");
        }
        // Every bundled file is a TrueType font.
        for b in BUNDLED {
            assert_eq!(&b[..4], &[0, 1, 0, 0], "a TrueType font");
        }
    }

    #[test]
    fn reads_only_font_files() {
        let dir = std::env::temp_dir().join(format!("kodact-fonts-{}", std::process::id()));
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
