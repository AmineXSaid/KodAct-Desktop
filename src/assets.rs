//! Everything the app ships inside its binary: the icons (Lucide, ISC
//! licence, `assets/icons/LICENSE`), the file icons (Material Icon Theme,
//! MIT, `assets/file-icons/LICENSE`). No font: Anthropic Sans is read
//! from disk (`fonts.rs`).

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        const ICONS: &[(&str, &[u8])] = &[
            $((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg"))),)*
        ];
    };
}

icons!(
    "table",
    "bold",
    "italic",
    "link",
    "message-square-plus",
    "list",
    "list-ordered",
    "heading-1",
    "heading-2",
    "heading-3",
    "pilcrow",
    "square-code",
    "trash-2",
    "panel-bottom",
    "zoom-in",
    "zoom-out",
    "scan",
    "mouse-pointer-2",
    "undo-2",
    "redo-2",
    "type",
    "frame",
    "sticky-note",
    "pen-tool",
    "pencil-ruler",
    "folder-git-2",
    "square-minus",
    "arrow-up-down",
    "boxes",
    "notebook-pen",
    "gauge",
    "lightbulb",
    "paw-print",
    "circle-user",
    "folder-plus",
    "circle",
    "arrow-up-right",
    "archive",
    "git-pull-request",
    "circle-dot",
    "circle",
    "file-down",
    "unplug",
    "eraser",
    "clipboard-paste",
    "laptop",
    "key-round",
    "arrow-left",
    "panel-right",
    "arrow-right",
    "arrow-up",
    "at-sign",
    "check",
    "chevron-down",
    "chevron-left",
    "chevron-right",
    "chevron-up",
    "circle-check",
    "circle-pause",
    "target",
    "code-xml",
    "graduation-cap",
    "route",
    "scroll-text",
    "circle-slash",
    "circle-x",
    "clock",
    "copy",
    "download",
    "ellipsis",
    "external-link",
    "eye",
    "file-pen",
    "file-text",
    // Attachment chips and the @ list: a file's kind (any file attaches).
    "file",
    "folder",
    "list-tree",
    "file-code",
    "file-plus",
    "file-json",
    "file-cog",
    "file-lock",
    "file-terminal",
    "file-image",
    "file-type",
    "presentation",
    "file-spreadsheet",
    "file-archive",
    "network",
    "database",
    "folder-open",
    "git-branch",
    "globe",
    "hand",
    "image",
    "info",
    "keyboard",
    "list-todo",
    "loader-circle",
    "map",
    "cpu",
    "maximize-2",
    "minimize-2",
    "model",
    "message-square",
    "minus",
    "moon",
    "panel-left",
    "paperclip",
    "pencil",
    "pin",
    "plus",
    "refresh-cw",
    "rotate-ccw",
    "search",
    "settings",
    "shield-check",
    "shield-x",
    "layout-grid",
    "sliders-horizontal",
    "split",
    "square",
    "square-check",
    "sun",
    "terminal",
    "trash",
    "triangle-alert",
    "x",
    // Reply actions, starters, jump to latest, the chat menu (P1).
    "arrow-down",
    "book-open",
    "bug",
    "copy-plus",
    "flask-conical",
    "history",
    "play",
    "thumbs-down",
    "thumbs-up",
    "square-slash",
    "webhook",
    "plug",
    "layers",
    "box",
    "puzzle",
    "server",
    "circle-dashed",
    "stethoscope",
    // The command palette's scope tabs and the share row.
    "hash",
    "text-quote",
    "share-2",
    // The title bar: share the chat, and its ⋮ menu.
    "share",
    "ellipsis-vertical",
    // The title bar's minimize / maximize / restore / close (gpui-kit-assets, Apache-2.0).
    "window-close",
    "window-maximize",
    "window-minimize",
    "window-restore",
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // GPUI's SVG renderer loads these two paths into the font list
        // diagrams draw text with: both are Anthropic Sans Text.
        if matches!(
            path,
            "fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf" | "fonts/lilex/Lilex-Regular.ttf"
        ) {
            return Ok(crate::fonts::text_regular().map(Cow::Owned));
        }
        if path.starts_with("kodact/") {
            // The mark's strokes, drawn from its geometry (`ui/kodact_mark.rs`).
            return Ok(crate::ui::kodact_mark::stroke_svg(path).map(Cow::Owned));
        }
        if path.starts_with("avatars/") {
            // Sized to the pixels they're drawn at (`avatars.rs`).
            return Ok(crate::avatars::load(path).map(Cow::Owned));
        }
        if path.starts_with("file-icons/") {
            // The @ list's full-colour file icons (`ui/file_icon.rs`).
            return Ok(crate::ui::file_icon_map::SVGS
                .iter()
                .chain(crate::ui::file_icon_open::OPEN_SVGS.iter())
                .find(|(p, _)| *p == path)
                .map(|(_, bytes)| Cow::Borrowed(*bytes)));
        }
        Ok(ICONS
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect())
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
        for used in source
            .split("\"icons/")
            .skip(1)
            .filter_map(|r| r.split('"').next())
        {
            assert!(
                bundled.contains(&format!("icons/{used}").as_str()),
                "icons/{used} is drawn but not bundled in assets.rs"
            );
        }
        let reserved: Vec<String> = crate::stubs::RESERVED_ICONS
            .iter()
            .map(|(n, _, _)| format!("icons/{n}.svg"))
            .collect();
        for b in &bundled {
            // The title bar's window buttons are drawn by gpui-component, by name.
            let by_component = b.starts_with("icons/window-");
            assert!(
                by_component
                    || source.contains(&format!("\"{b}\""))
                    || reserved.iter().any(|r| r == b),
                "{b} is bundled but nothing draws it"
            );
        }
    }
}
