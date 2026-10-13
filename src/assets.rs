//! Everything the app ships inside its binary: the icons (Lucide, ISC
//! licence, `assets/icons/LICENSE`) and the fonts (OFL, `assets/fonts`).

use std::borrow::Cow;

use gpui::{App, AssetSource, Result, SharedString};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        const ICONS: &[(&str, &[u8])] = &[
            $((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg"))),)*
        ];
    };
}

icons!(
    "archive", "arrow-right", "arrow-up", "at-sign", "brain", "check", "chevron-down", "chevron-right", "chevron-up",
    "circle-check", "circle-pause", "code-xml", "graduation-cap", "route", "scroll-text", "circle-slash", "circle-x", "clock", "copy", "cpu", "download", "ellipsis", "external-link", "eye", "file-pen",
    "file-text", "folder-open", "git-branch", "globe", "hand", "image", "info", "keyboard", "list-todo",
    "loader-circle", "map", "maximize-2", "message-square", "minus", "moon", "panel-left", "paperclip", "pencil",
    "pin", "plus", "refresh-cw", "rotate-ccw", "search", "settings", "shield-check", "sliders-horizontal", "sparkles", "split", "square", "square-check",
    "sun", "terminal", "trash", "triangle-alert", "x",
    // Reply actions, starters, jump to latest, the chat menu (P1).
    "arrow-down", "book-open", "bug", "copy-plus", "flask-conical", "history", "play", "thumbs-down", "thumbs-up", "wand-sparkles",
    // The title bar's minimize / maximize / restore / close (gpui-kit-assets, Apache-2.0).
    "window-close", "window-maximize", "window-minimize", "window-restore",
);

macro_rules! images {
    ($($name:literal),* $(,)?) => {
        /// Pictures: the Forge cube, rendered from `assets/images/cube-original.webp`
        /// at each size the app shows it (32, 48 and 96, at 1x to 3x), so it is never
        /// scaled at draw time.
        const IMAGES: &[(&str, &[u8])] = &[
            $((concat!("images/", $name, ".png"), include_bytes!(concat!("../assets/images/", $name, ".png"))),)*
        ];
    };
}

images!(
    "cube-32", "cube-40", "cube-48", "cube-56", "cube-60", "cube-64", "cube-72", "cube-80", "cube-84", "cube-96", "cube-120", "cube-144", "cube-168", "cube-192", "cube-240", "cube-288",
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // GPUI's SVG renderer loads these two paths into the font list
        // diagrams draw text with: give it Forge's faces, so a diagram's
        // labels look the same on every machine.
        match path {
            "fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf" => return Ok(Some(Cow::Borrowed(include_bytes!("../assets/fonts/HankenGrotesk-Regular.ttf")))),
            "fonts/lilex/Lilex-Regular.ttf" => return Ok(Some(Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")))),
            _ => {}
        }
        if path.starts_with("avatars/") {
            // Sized to the pixels they're drawn at (`avatars.rs`).
            return Ok(crate::avatars::load(path).map(Cow::Owned));
        }
        Ok(ICONS.iter().chain(IMAGES).find(|(p, _)| *p == path).map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS.iter().chain(IMAGES).filter(|(p, _)| p.starts_with(path)).map(|(p, _)| SharedString::from(*p)).collect())
    }
}

/// Register the bundled faces, so the app looks the same on every machine.
pub fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/HankenGrotesk-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/HankenGrotesk-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/HankenGrotesk-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/HankenGrotesk-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/HankenGrotesk-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SourceSerif4-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SourceSerif4Display-Regular.ttf")),
    ];
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("could not load the bundled fonts: {e}");
    }
}

#[cfg(test)]
mod tests {
    /// Every `icons/<name>.svg` the source draws is bundled (an unbundled
    /// icon renders blank), and every bundled icon is drawn or reserved.
    #[test]
    fn every_icon_drawn_is_bundled_and_every_bundled_icon_is_drawn() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut source = String::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "rs") && !p.ends_with("assets.rs") {
                    source.push_str(&std::fs::read_to_string(&p).unwrap());
                }
            }
        }
        let bundled: Vec<&str> = super::ICONS.iter().map(|(p, _)| *p).collect();
        for used in source.split("\"icons/").skip(1).filter_map(|r| r.split('"').next()) {
            assert!(bundled.contains(&format!("icons/{used}").as_str()), "icons/{used} is drawn but not bundled in assets.rs");
        }
        let reserved: Vec<String> = crate::stubs::RESERVED_ICONS.iter().map(|(n, _, _)| format!("icons/{n}.svg")).collect();
        for b in &bundled {
            // The title bar's window buttons are drawn by gpui-component, by name.
            let by_component = b.starts_with("icons/window-");
            assert!(by_component || source.contains(&format!("\"{b}\"")) || reserved.iter().any(|r| r == b), "{b} is bundled but nothing draws it");
        }
    }
}
