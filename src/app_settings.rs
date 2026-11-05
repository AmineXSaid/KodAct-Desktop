//! KodAct's own settings: `settings.json` in its config folder (beside
//! `name.txt`), for what has no screen of its own yet: remote connections and
//! the terminal dock. Keys are dotted, as an editor's settings are; a missing
//! or broken file means the defaults, and odd values are held in range.
//!
//! ```json
//! {
//!   "remote.ssh.configFile": "~/.ssh/config",
//!   "remote.ssh.extraArgs": ["-o", "ProxyJump=bastion"],
//!   "remote.serverInstallDir": "~/.kodact",
//!   "remote.reconnect.maxAttempts": 8,
//!   "terminal.defaultShell": "pwsh.exe",
//!   "terminal.scrollback": 10000,
//!   "terminal.fontSize": 13,
//!   "terminal.cursorStyle": "block",
//!   "kodact.autoApproveSafeCommands": true
//! }
//! ```
//!
//! The dock's open state and width are kept apart, in `window.json`: the app
//! writes that one, people write this one.

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// An ssh config file instead of `~/.ssh/config` (`ssh -F`).
    #[serde(rename = "remote.ssh.configFile")]
    pub ssh_config_file: Option<String>,
    /// More arguments for every `ssh`, before the host.
    #[serde(rename = "remote.ssh.extraArgs")]
    pub ssh_extra_args: Vec<String>,
    /// Where KodAct CLI and its plugin go on a remote.
    #[serde(rename = "remote.serverInstallDir")]
    pub server_install_dir: String,
    /// Reconnect tries after a remote stops answering, before giving up.
    #[serde(rename = "remote.reconnect.maxAttempts")]
    pub reconnect_max_attempts: u32,
    /// The shell a new terminal runs (else the first one found).
    #[serde(rename = "terminal.defaultShell")]
    pub default_shell: Option<String>,
    /// Lines kept above a terminal's screen.
    #[serde(rename = "terminal.scrollback")]
    pub scrollback: usize,
    #[serde(rename = "terminal.fontSize")]
    pub font_size: f32,
    #[serde(rename = "terminal.cursorStyle")]
    pub cursor_style: CursorStyle,
    /// Harmless shell commands run without a prompt: reads in every mode,
    /// edits to project files in Edit automatically. Deletions always ask.
    /// Forge (before the rename) called it `forge.autoApproveSafeCommands`.
    #[serde(
        rename = "kodact.autoApproveSafeCommands",
        alias = "forge.autoApproveSafeCommands"
    )]
    pub auto_approve_safe_commands: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ssh_config_file: None,
            ssh_extra_args: vec![],
            server_install_dir: "~/.kodact".into(),
            reconnect_max_attempts: 8,
            default_shell: None,
            scrollback: 10_000,
            font_size: 13.,
            cursor_style: CursorStyle::Block,
            auto_approve_safe_commands: true,
        }
    }
}

impl Settings {
    /// Parse a settings file; anything unreadable falls back to the default,
    /// and numbers are held to what works.
    pub fn parse(text: &str) -> Settings {
        let mut s: Settings = serde_json::from_str(text).unwrap_or_default();
        s.scrollback = s.scrollback.min(100_000);
        s.font_size = if s.font_size.is_finite() {
            s.font_size.clamp(8., 32.)
        } else {
            13.
        };
        s.reconnect_max_attempts = s.reconnect_max_attempts.clamp(1, 100);
        // An install dir is a plain path: no quotes, no shell syntax.
        let dir = s.server_install_dir.trim().trim_end_matches('/');
        let plain = !dir.is_empty()
            && (dir.starts_with('/') || dir == "~" || dir.starts_with("~/"))
            && !dir.contains(['\'', '"', '`', '$', '\\', '\n', ';', '&', '|']);
        s.server_install_dir = if plain {
            dir.to_string()
        } else {
            "~/.kodact".into()
        };
        s.default_shell = s.default_shell.filter(|x| !x.trim().is_empty());
        s.ssh_config_file = s.ssh_config_file.filter(|x| !x.trim().is_empty());
        s
    }

    /// What every `ssh` gets before the host: the config file and the extra
    /// arguments.
    pub fn ssh_args(&self) -> Vec<String> {
        let mut out = vec![];
        if let Some(f) = &self.ssh_config_file {
            out.extend(["-F".to_string(), expand_home(f)]);
        }
        out.extend(self.ssh_extra_args.iter().cloned());
        out
    }
}

/// `~/x` on this computer, as a full path.
pub fn expand_home(p: &str) -> String {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match (p.strip_prefix("~/"), home) {
        (Some(rest), Some(h)) => h.join(rest).to_string_lossy().into_owned(),
        _ => p.to_string(),
    }
}

fn path() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("settings.json"))
}

/// The settings as last read, and the file's time then.
static CURRENT: RwLock<Option<(Option<SystemTime>, Settings)>> = RwLock::new(None);

/// The settings: read from disk the first time, and again whenever the file
/// changed since (an edit takes effect at the next use, no restart).
pub fn get() -> Settings {
    let file = path();
    let stamp = file
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok());
    if let Some((when, s)) = CURRENT.read().ok().and_then(|g| g.clone()) {
        if when == stamp {
            return s;
        }
    }
    let s = file
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| Settings::parse(&t))
        .unwrap_or_default();
    if let Ok(mut g) = CURRENT.write() {
        *g = Some((stamp, s.clone()));
    }
    s
}

/// The settings file, written with the defaults when it doesn't exist yet,
/// so opening it shows every key.
pub fn ensure_file() -> Option<PathBuf> {
    let p = path()?;
    if !p.exists() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let text = serde_json::to_string_pretty(&Settings::default()).ok()?;
        std::fs::write(&p, text + "\n").ok()?;
    }
    Some(p)
}

/// The dock as it was left: open or not, and its share of the width.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub terminal_open: bool,
    pub terminal_share: f32,
    /// Light or dark, and which theme (`theme::Look::id`), from Settings.
    pub dark: bool,
    pub theme: String,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            terminal_open: false,
            terminal_share: 0.35,
            // Dark first: KodAct is mostly seen in a dark terminal.
            dark: true,
            theme: crate::theme::Look::default().id().into(),
        }
    }
}

pub fn load_window() -> WindowState {
    let mut w: WindowState = crate::profile::dir()
        .and_then(|d| std::fs::read_to_string(d.join("window.json")).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if !w.terminal_share.is_finite() {
        w.terminal_share = 0.35;
    }
    w.terminal_share = w.terminal_share.clamp(0.05, 0.6);
    w
}

pub fn save_window(w: WindowState) {
    let Some(d) = crate::profile::dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&d);
    if let Ok(t) = serde_json::to_string_pretty(&w) {
        let _ = std::fs::write(d.join("window.json"), t + "\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_read_with_defaults_and_ranges() {
        assert_eq!(Settings::parse(""), Settings::default());
        assert_eq!(Settings::parse("not json"), Settings::default());
        let s = Settings::parse(
            r#"{"terminal.scrollback": 9999999, "terminal.fontSize": 100,
                "terminal.cursorStyle": "bar", "remote.reconnect.maxAttempts": 0,
                "remote.ssh.extraArgs": ["-o", "ProxyJump=b"],
                "remote.ssh.configFile": "/etc/ssh/forge_config",
                "remote.serverInstallDir": "/opt/forge/"}"#,
        );
        assert_eq!(s.scrollback, 100_000);
        assert_eq!(s.font_size, 32.);
        assert_eq!(s.cursor_style, CursorStyle::Bar);
        assert_eq!(s.reconnect_max_attempts, 1);
        assert_eq!(s.server_install_dir, "/opt/forge");
        assert_eq!(
            s.ssh_args(),
            ["-F", "/etc/ssh/forge_config", "-o", "ProxyJump=b"]
        );
        // Shell syntax in the install dir is refused.
        let s = Settings::parse(r#"{"remote.serverInstallDir": "~/x; rm -rf ~"}"#);
        assert_eq!(s.server_install_dir, "~/.kodact");
        let s = Settings::parse(r#"{"remote.serverInstallDir": "relative/dir"}"#);
        assert_eq!(s.server_install_dir, "~/.kodact");
    }

    #[test]
    fn forges_setting_name_still_reads() {
        let s = Settings::parse(r#"{"forge.autoApproveSafeCommands": false}"#);
        assert!(!s.auto_approve_safe_commands);
        let s = Settings::parse(r#"{"kodact.autoApproveSafeCommands": false}"#);
        assert!(!s.auto_approve_safe_commands);
    }

    #[test]
    fn window_state_round_trips() {
        let w: WindowState = serde_json::from_str(r#"{"terminal_open": true}"#).unwrap();
        assert!(w.terminal_open);
        assert_eq!(w.terminal_share, 0.35);
    }

    /// The settings reach real ssh: `remote.ssh.configFile` names a config
    /// with a host only it knows, and `remote.ssh.extraArgs` adds an option.
    /// `KODACT_TEST_SSH_CONFIG` is that file, `KODACT_TEST_SSH_ALIAS` the host.
    #[test]
    #[ignore]
    fn real_ssh_settings_apply() {
        let config = std::env::var("KODACT_TEST_SSH_CONFIG").expect("KODACT_TEST_SSH_CONFIG");
        let alias = std::env::var("KODACT_TEST_SSH_ALIAS").expect("KODACT_TEST_SSH_ALIAS");
        let dir = std::env::temp_dir().join(format!("forge-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("KODACT_CONFIG_DIR", &dir);
        std::fs::write(
            dir.join("settings.json"),
            serde_json::json!({
                "remote.ssh.configFile": config,
                "remote.ssh.extraArgs": ["-o", "ConnectionAttempts=2"],
            })
            .to_string(),
        )
        .unwrap();
        assert!(
            crate::remote::ssh_hosts().contains(&alias),
            "listed from the setting's file"
        );
        let t = crate::remote::Target::Ssh(alias);
        let extra = crate::remote::Extra {
            accept_new_host_key: true,
            reverse: vec![],
        };
        let info = crate::remote::probe(&t, &extra).expect("connects through the setting's file");
        assert!(!info.arch.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
