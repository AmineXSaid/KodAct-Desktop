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
    "archive", "arrow-up", "at-sign", "bot", "brain", "check", "chevron-down", "chevron-right", "chevron-up",
    "circle-check", "circle-pause", "circle-slash", "circle-x", "clock", "command", "copy", "cpu", "ellipsis", "eye", "file-pen",
    "file-text", "folder-open", "git-branch", "globe", "hand", "image", "info", "keyboard", "list-todo",
    "loader-circle", "map", "maximize-2", "message-square", "minus", "moon", "panel-left", "paperclip", "pencil",
    "pin", "plus", "refresh-cw", "rotate-ccw", "search", "settings", "shield-check", "sparkles", "split", "square",
    "sun", "terminal", "trash", "triangle-alert", "x", "zap",
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS.iter().find(|(p, _)| *p == path).map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS.iter().filter(|(p, _)| p.starts_with(path)).map(|(p, _)| SharedString::from(*p)).collect())
    }
}

/// Register the bundled faces, so the app looks the same on every machine.
pub fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.otf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.otf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.otf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Bold.otf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Italic.otf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/SourceSerif4-Regular.ttf")),
    ];
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("could not load the bundled fonts: {e}");
    }
}
