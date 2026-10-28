//! A file's icon, from its name. The @ list uses the VS Code extension's
//! full-colour icons (`asset`); other lists use a Lucide glyph in a theme
//! colour (`for_name`).

use gpui::Hsla;

use super::file_icon_map::{EXTENSIONS, FOLDERS, NAMES, SVGS};

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

/// The extension's icon for a file or folder (its `FileIcon`): a folder by
/// its name, without a leading dot; a file by its whole name, then by its
/// suffixes from the longest (`spec.ts` before `ts`). `path` is the
/// project-relative path, for `.github/workflows`.
pub fn asset(name: &str, folder: bool, path: &str) -> &'static str {
    let lower = name.trim_end_matches('/').to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    let find = |table: &[(&str, &'static str)], key: &str| {
        table
            .binary_search_by(|(k, _)| (*k).cmp(key))
            .ok()
            .map(|i| table[i].1)
    };
    let icon = if folder {
        if path.to_lowercase().contains("github/workflows") {
            Some("folder-gh-workflows")
        } else {
            find(FOLDERS, base.trim_start_matches('.'))
        }
        .unwrap_or("folder")
    } else {
        find(NAMES, base)
            .or_else(|| find(EXTENSIONS, base))
            .or_else(|| {
                base.match_indices('.')
                    .find_map(|(i, _)| find(EXTENSIONS, &base[i + 1..]))
            })
            .unwrap_or("file")
    };
    svg_path(icon)
        .or_else(|| svg_path(if folder { "folder" } else { "file" }))
        .unwrap_or("file-icons/file.svg")
}

fn svg_path(icon: &str) -> Option<&'static str> {
    SVGS.iter().map(|(p, _)| *p).find(|p| {
        p.strip_prefix("file-icons/")
            .and_then(|p| p.strip_suffix(".svg"))
            == Some(icon)
    })
}

#[cfg(test)]
mod tests {
    use super::asset;

    #[test]
    fn icons_are_the_extensions() {
        assert_eq!(asset("main.py", false, "main.py"), "file-icons/python.svg");
        assert_eq!(asset("lib.rs", false, "src/lib.rs"), "file-icons/rust.svg");
        assert_eq!(
            asset(".gitignore", false, ".gitignore"),
            "file-icons/git.svg"
        );
        assert_eq!(
            asset("a.spec.ts", false, "a.spec.ts"),
            "file-icons/test-ts.svg"
        );
        assert_eq!(
            asset("weird.zzz", false, "weird.zzz"),
            "file-icons/file.svg"
        );
        assert_eq!(
            asset(".vscode", true, ".vscode"),
            "file-icons/folder-vscode.svg"
        );
        assert_eq!(asset("src", true, "src"), "file-icons/folder-src.svg");
        assert_eq!(
            asset("nothing-special", true, "x/nothing-special"),
            "file-icons/folder.svg"
        );
    }
}
