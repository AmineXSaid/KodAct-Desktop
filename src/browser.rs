//! The web browser: with it on, each chat's ForgeCLI gets browser-use's MCP
//! server (github.com/browser-use/browser-use, MIT), so Forge can open pages,
//! click, type, read a page's state, take screenshots and switch tabs
//! (`mcp__browser-use__browser_navigate`, `…_click`, `…_get_state`…). Its
//! tool calls ask permission like any other.
//!
//! It runs as its own process through `uvx` (from uv), pinned to one
//! version, so nothing is installed into the app and nothing is linked into
//! it. It drives the browser Forge runs and shows in the side panel
//! (`web_view.rs`), rather than opening a window of its own. The switch is a plain file in Forge's config folder (`browser.txt`).

use std::path::{Path, PathBuf};

/// The browser-use release Forge runs.
pub const PACKAGE: &str = "browser-use[cli]==0.13.11";
/// The MCP server's name, so its tools are `mcp__browser-use__…`.
pub const SERVER: &str = "browser-use";
/// The prefix of the browser's tool names, as ForgeCLI calls them.
pub const TOOL_PREFIX: &str = "mcp__browser-use__";

/// Standard base64 (padding optional, whitespace skipped): a screenshot in
/// a tool result.
pub fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

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

/// The `--mcp-config` JSON that starts browser-use with `uvx`. With
/// `config_dir`, browser-use reads its settings from there
/// (`BROWSER_USE_CONFIG_DIR`): `write_config` puts the address of Forge's
/// own browser in it, so the agent drives the browser the panel shows.
pub fn mcp_config(uvx: &Path, config_dir: Option<&Path>) -> String {
    let mut server = serde_json::json!({
        "command": uvx.to_string_lossy(),
        "args": ["--from", PACKAGE, "browser-use", "--mcp"],
    });
    if let Some(dir) = config_dir {
        server["env"] = serde_json::json!({ "BROWSER_USE_CONFIG_DIR": dir.to_string_lossy() });
    }
    serde_json::json!({ "mcpServers": { SERVER: server } }).to_string()
}

/// browser-use's settings for Forge: one browser profile, connected to the
/// browser at `cdp_url` instead of opening one of its own.
pub fn write_config(dir: &Path, cdp_url: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let config = serde_json::json!({
        "browser_profile": { "forge": {
            "id": "forge", "default": true, "created_at": "2026-01-01T00:00:00",
            "cdp_url": cdp_url, "headless": true, "keep_alive": true,
        } },
        "llm": {},
        "agent": {},
    });
    std::fs::write(
        dir.join("config.json"),
        serde_json::to_string_pretty(&config)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes() {
        assert_eq!(
            decode_base64("SGVsbG8sIEZvcmdlIQ==").unwrap(),
            b"Hello, Forge!"
        );
        assert_eq!(decode_base64("SGk").unwrap(), b"Hi");
        assert!(decode_base64("**").is_none());
    }

    #[test]
    fn the_config_runs_the_pinned_server_through_uvx() {
        let c: serde_json::Value =
            serde_json::from_str(&mcp_config(Path::new("/u/uvx"), Some(Path::new("/cfg"))))
                .unwrap();
        assert_eq!(
            c["mcpServers"]["browser-use"]["env"]["BROWSER_USE_CONFIG_DIR"],
            "/cfg"
        );
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
