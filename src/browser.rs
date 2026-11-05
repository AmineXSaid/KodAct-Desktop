//! The web browser for KodAct CLI's agents. With it on, each chat's KodAct CLI
//! gets KodAct's own browser tools (`browser_mcp.rs`, served by this same
//! executable): they drive the browser in the side panel (`web_view.rs`),
//! so you watch every step live and can take over. The tools reach the model
//! as `mcp__kodact-browser__navigate`, `…__click`, `…__eval`…, and ask
//! permission like any other tool.
//!
//! Nothing is installed for it: the tools are this program, and the browser
//! is Playwright's Chromium, Chrome or Edge. The switch is a plain file in
//! KodAct's config folder (`browser.txt`).

use std::path::{Path, PathBuf};

/// The prefix of the browser's tool names, as KodAct CLI calls them.
pub const TOOL_PREFIX: &str = "mcp__kodact-browser__";

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

/// The log of the browser's downloads, beside the focus file: the panel
/// writes a line as each starts (`started\t<name>`) and ends
/// (`saved\t<path>`, `failed\t<name>`); the agent's tools report them.
pub fn downloads_log(focus: &Path) -> PathBuf {
    focus.with_file_name("browser-downloads.txt")
}

/// The file the panel and the agent's tools keep the current tab in.
pub fn focus_file() -> PathBuf {
    crate::profile::dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("browser-focus.txt")
}

/// The `--mcp-config` JSON that gives a KodAct CLI the browser tools: this
/// executable as an MCP server, on the browser at `devtools` (its
/// `http://127.0.0.1:<port>`).
pub fn mcp_config(exe: &Path, devtools: &str, focus: &Path) -> String {
    serde_json::json!({ "mcpServers": { crate::browser_mcp::SERVER: {
        "command": exe.to_string_lossy(),
        "args": ["--browser-mcp", devtools, focus.to_string_lossy()],
    } } })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes() {
        assert_eq!(
            decode_base64("SGVsbG8sIEtvZEFjdCE=").unwrap(),
            b"Hello, KodAct!"
        );
        assert_eq!(decode_base64("SGk").unwrap(), b"Hi");
        assert!(decode_base64("**").is_none());
    }

    #[test]
    fn the_config_runs_forge_itself_as_the_server() {
        let c: serde_json::Value = serde_json::from_str(&mcp_config(
            Path::new("/app/kodact-desktop"),
            "http://127.0.0.1:9222",
            Path::new("/cfg/focus.txt"),
        ))
        .unwrap();
        let s = &c["mcpServers"]["kodact-browser"];
        assert_eq!(s["command"], "/app/kodact-desktop");
        assert_eq!(
            s["args"],
            serde_json::json!(["--browser-mcp", "http://127.0.0.1:9222", "/cfg/focus.txt"])
        );
        assert!(TOOL_PREFIX.contains(crate::browser_mcp::SERVER));
    }
}
