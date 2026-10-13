//! A file's icon and its colour, from its name: the @ list draws one per
//! row, so a folder, a source file, a config and a picture tell apart at a
//! glance (Lucide glyphs, theme colours).

use gpui::Hsla;

use crate::theme::Theme;

/// The icon and colour for `name` (a folder when `folder`).
pub fn for_name(name: &str, folder: bool, t: &Theme) -> (&'static str, Hsla) {
    if folder {
        return ("icons/folder.svg", t.colors.marigold);
    }
    let lower = name.to_lowercase();
    if lower == "dockerfile" || lower == "makefile" || lower == "justfile" {
        return ("icons/file-terminal.svg", t.extra.success);
    }
    if lower.starts_with(".env") || lower == ".gitignore" || lower == ".editorconfig" {
        return ("icons/file-cog.svg", t.colors.text_muted);
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "go" | "java" | "kt" | "c"
        | "h" | "cc" | "cpp" | "hpp" | "cs" | "swift" | "rb" | "php" | "vue" | "svelte" | "lua"
        | "dart" | "scala" | "ex" | "exs" | "zig" | "html" | "css" | "scss" | "sql" | "wgsl" => {
            ("icons/file-code.svg", t.colors.sky)
        }
        "json" | "jsonc" | "json5" => ("icons/file-json.svg", t.colors.marigold),
        "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "xml" | "plist" | "properties" => {
            ("icons/file-cog.svg", t.colors.text_muted)
        }
        "lock" => ("icons/file-lock.svg", t.colors.text_subtle),
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => {
            ("icons/file-terminal.svg", t.extra.success)
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" | "bmp" => {
            ("icons/file-image.svg", t.colors.primary)
        }
        "pdf" => ("icons/file-type.svg", t.extra.danger),
        "doc" | "docx" | "odt" | "rtf" => ("icons/file-type.svg", t.colors.sky),
        "ppt" | "pptx" | "odp" => ("icons/presentation.svg", t.colors.marigold),
        "xls" | "xlsx" | "xlsm" | "ods" | "csv" | "tsv" => {
            ("icons/file-spreadsheet.svg", t.extra.success)
        }
        "zip" | "tar" | "gz" | "tgz" | "7z" | "rar" | "xz" | "bz2" | "zst" => {
            ("icons/file-archive.svg", t.colors.text_muted)
        }
        "pcap" | "pcapng" | "cap" => ("icons/network.svg", t.colors.sky),
        "db" | "sqlite" | "sqlite3" => ("icons/database.svg", t.colors.marigold),
        "md" | "mdx" | "txt" | "rst" | "log" => ("icons/file-text.svg", t.colors.text_muted),
        _ => ("icons/file.svg", t.colors.text_muted),
    }
}
