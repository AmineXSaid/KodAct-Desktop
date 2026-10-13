//! The web browser: with it on, each chat's ForgeCLI gets browser-use's MCP
//! server (github.com/browser-use/browser-use, MIT), so Forge can open pages,
//! click, type, read a page's state, take screenshots and switch tabs
//! (`mcp__browser-use__browser_navigate`, `…_click`, `…_get_state`…). Its
//! tool calls ask permission like any other.
//!
//! It runs as its own process through `uvx` (from uv), pinned to one
//! version, so nothing is installed into the app and nothing is linked into
//! it. The switch is a plain file in Forge's config folder (`browser.txt`).

use std::path::{Path, PathBuf};

/// The browser-use release Forge runs.
pub const PACKAGE: &str = "browser-use[cli]==0.13.11";
/// The MCP server's name, so its tools are `mcp__browser-use__…`.
pub const SERVER: &str = "browser-use";

pub fn enabled() -> bool {
    crate::profile::dir()
        .and_then(|d| std::fs::read_to_string(d.join("browser.txt")).ok())
        .is_some_and(|s| s.trim() == "on")
}

pub fn set_enabled(on: bool) -> std::io::Result<()> {
    let dir = crate::profile::dir().ok_or_else(|| std::io::Error::other("no config folder"))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("browser.txt"), if on { "on" } else { "off" })
}

/// `uvx`, on the PATH or where uv's installer puts it.
pub fn uvx() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "uvx.exe" } else { "uvx" };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if let Some(h) = home {
        dirs.push(h.join(".local").join("bin"));
        dirs.push(h.join(".cargo").join("bin"));
    }
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// The `--mcp-config` JSON that starts browser-use with `uvx`.
pub fn mcp_config(uvx: &Path) -> String {
    serde_json::json!({
        "mcpServers": {
            SERVER: {
                "command": uvx.to_string_lossy(),
                "args": ["--from", PACKAGE, "browser-use", "--mcp"],
            }
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_runs_the_pinned_server_through_uvx() {
        let c: serde_json::Value = serde_json::from_str(&mcp_config(Path::new("/u/uvx"))).unwrap();
        let s = &c["mcpServers"]["browser-use"];
        assert_eq!(s["command"], "/u/uvx");
        assert_eq!(
            s["args"],
            serde_json::json!([
                "--from",
                "browser-use[cli]==0.13.11",
                "browser-use",
                "--mcp"
            ])
        );
    }
}
