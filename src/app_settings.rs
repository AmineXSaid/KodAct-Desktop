//! Forge's own settings: `settings.json` in its config folder (beside
//! `name.txt`), for what has no screen of its own yet: remote connections and
//! the terminal dock. Keys are dotted, as an editor's settings are; a missing
//! or broken file means the defaults, and odd values are held in range.
//!
//! ```json
//! {
//!   "remote.ssh.configFile": "~/.ssh/config",
//!   "remote.ssh.extraArgs": ["-o", "ProxyJump=bastion"],
//!   "remote.serverInstallDir": "~/.forge",
//!   "remote.reconnect.maxAttempts": 8,
//!   "terminal.defaultShell": "pwsh.exe",
//!   "terminal.scrollback": 10000,
//!   "terminal.fontSize": 13,
//!   "terminal.cursorStyle": "block",
//!   "forge.autoApproveSafeCommands": true,
//!   "forge.fallbackModel": "claude-haiku-4-5",
//!   "forge.maxTurns": 60,
//!   "forge.maxBudgetUsd": 5.0,
//!   "forge.maxTimeMinutes": 30,
//!   "forge.worktree": false,
//!   "forge.autonomous": false,
//!   "forge.maxThinkingTokens": 16000,
//!   "forge.addDirs": ["~/shared/design-tokens"],
//!   "forge.agent": "reviewer",
//!   "forge.tools": ["Read", "Grep", "Glob"],
//!   "forge.allowedTools": ["Bash(git status)"],
//!   "forge.disallowedTools": ["WebFetch"],
//!   "forge.sandbox": "workspace-write",
//!   "forge.systemPrompt": "You are a careful reviewer.",
//!   "forge.systemPromptFile": "~/prompts/review.md",
//!   "forge.excludeDynamicSystemPromptSections": false,
//!   "forge.strictMcpConfig": false,
//!   "forge.settingSources": "user,project",
//!   "forge.bare": false,
//!   "forge.safeMode": false,
//!   "forge.betas": ["context-1m-2025-08-07"],
//!   "forge.debug": "api,hooks",
//!   "forge.debugFile": "~/forge-debug.log",
//!   "forge.autocompact": "200k",
//!   "forge.includeHookEvents": true,
//!   "forge.noSessionPersistence": false,
//!   "forge.noInput": false,
//!   "forge.jsonSchema": {"type": "object"},
//!   "forge.extraArgs": ["--color", "never"]
//! }
//! ```
//!
//! The `forge.*` keys after the first reach ForgeCLI as flags when a chat's
//! process starts: a fallback model (`--fallback-model`, comma-separated for
//! several), limits on a session (`--max-turns`, `--max-budget-usd`,
//! `--max-time`), each new chat in its own git worktree (`--worktree`), and
//! unattended runs that attempt rather than ask (`--autonomous`), and the
//! thinking budget of models that take one in tokens rather than an effort
//! (`FORGE_MAX_THINKING_TOKENS`), folders outside the project Forge may work
//! in (`--add-dir`), and an agent every chat runs as, its prompt and tools
//! (`--agent`).
//!
//! The rest pass ForgeCLI's own flags under their own names: which tools are
//! offered, allowed and denied, the shell sandbox, the system prompt, which
//! settings and MCP servers load, minimal and safe starts, API betas, debug
//! logs, the compaction window, hook events in the stream, sessions not kept
//! on disk (such a chat starts afresh each time its process does), never
//! asking, and a JSON Schema every reply's structured result follows (shown
//! under the reply). `forge.extraArgs` takes any other flag, except the ones
//! the chat itself runs on (print mode, the stream formats, the session and
//! the permission channel) and skipping permissions, which Settings allows.
//!
//! ```json
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
    /// Where ForgeCLI and its plugin go on a remote.
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
    #[serde(rename = "forge.autoApproveSafeCommands")]
    pub auto_approve_safe_commands: bool,
    /// Models to fall back to when the chosen one is overloaded or missing.
    #[serde(rename = "forge.fallbackModel")]
    pub fallback_model: Option<String>,
    /// Most agentic turns a chat's session may take.
    #[serde(rename = "forge.maxTurns")]
    pub max_turns: Option<u32>,
    /// Most a chat's session may spend on model calls, in US dollars.
    #[serde(rename = "forge.maxBudgetUsd")]
    pub max_budget_usd: Option<f64>,
    /// A time limit Forge is told and warned near, in minutes.
    #[serde(rename = "forge.maxTimeMinutes")]
    pub max_time_minutes: Option<u32>,
    /// Each new chat in a project works in its own git worktree.
    #[serde(rename = "forge.worktree")]
    pub worktree: bool,
    /// Runs that attempt rather than ask: no question or plan tools.
    #[serde(rename = "forge.autonomous")]
    pub autonomous: bool,
    /// The thinking budget, in tokens, of models that take one.
    #[serde(rename = "forge.maxThinkingTokens")]
    pub max_thinking_tokens: Option<u32>,
    /// Folders outside the project Forge may read and edit.
    #[serde(rename = "forge.addDirs")]
    pub add_dirs: Vec<String>,
    /// The agent every chat runs as (`.forge/agents/<name>.md`).
    #[serde(rename = "forge.agent")]
    pub agent: Option<String>,
    /// The built-in tools offered (`[]` offers none).
    #[serde(rename = "forge.tools")]
    pub tools: Option<Vec<String>>,
    /// Tool rules allowed without asking, as permission rules write them.
    #[serde(rename = "forge.allowedTools")]
    pub allowed_tools: Vec<String>,
    /// Tool rules always refused.
    #[serde(rename = "forge.disallowedTools")]
    pub disallowed_tools: Vec<String>,
    /// The shell sandbox: `off`, `read-only` or `workspace-write`.
    #[serde(rename = "forge.sandbox")]
    pub sandbox: Option<String>,
    /// A system prompt in place of ForgeCLI's own.
    #[serde(rename = "forge.systemPrompt")]
    pub system_prompt: Option<String>,
    /// The same, read from a file.
    #[serde(rename = "forge.systemPromptFile")]
    pub system_prompt_file: Option<String>,
    /// Per-machine prompt sections move into the first message.
    #[serde(rename = "forge.excludeDynamicSystemPromptSections")]
    pub exclude_dynamic_sections: bool,
    /// Only the MCP servers Forge passes (the browser, design tools).
    #[serde(rename = "forge.strictMcpConfig")]
    pub strict_mcp_config: bool,
    /// Which settings files load: `user`, `project`, `local`, comma-separated.
    #[serde(rename = "forge.settingSources")]
    pub setting_sources: Option<String>,
    /// No hooks or memory files.
    #[serde(rename = "forge.bare")]
    pub bare: bool,
    /// Every customization off.
    #[serde(rename = "forge.safeMode")]
    pub safe_mode: bool,
    /// API beta headers.
    #[serde(rename = "forge.betas")]
    pub betas: Vec<String>,
    /// Debug logging, optionally filtered (`""` for everything).
    #[serde(rename = "forge.debug")]
    pub debug: Option<String>,
    /// Where debug logs go.
    #[serde(rename = "forge.debugFile")]
    pub debug_file: Option<String>,
    /// The auto-compact window: `auto`, or 100k to 1M tokens.
    #[serde(rename = "forge.autocompact")]
    pub autocompact: Option<String>,
    /// Hooks report in the chat as they run.
    #[serde(rename = "forge.includeHookEvents")]
    pub include_hook_events: bool,
    /// Sessions are not kept on disk; a chat can't be resumed.
    #[serde(rename = "forge.noSessionPersistence")]
    pub no_session_persistence: bool,
    /// Never ask: what would ask is refused.
    #[serde(rename = "forge.noInput")]
    pub no_input: bool,
    /// A JSON Schema each turn's structured result follows.
    #[serde(rename = "forge.jsonSchema")]
    pub json_schema: Option<serde_json::Value>,
    /// Any other ForgeCLI flags, as written on a command line.
    #[serde(rename = "forge.extraArgs")]
    pub extra_args: Vec<String>,
}

/// Flags a chat's process runs on: Forge passes them itself, and another
/// value would break the conversation. Skipping permissions goes through
/// Settings (Bypass permissions), behind its warning.
const RESERVED_FLAGS: &[&str] = &[
    "-p",
    "--print",
    "--input-format",
    "--output-format",
    "--include-partial-messages",
    "--replay-user-messages",
    "--permission-prompt-tool",
    "--permission-prompts",
    "--session-id",
    "-r",
    "--resume",
    "-c",
    "--continue",
    "--fork-session",
    "--no-tui",
    "--dangerously-skip-permissions",
    "--allow-dangerously-skip-permissions",
    "--permission-mode",
    "--model",
    // Forge sets these once per chat; ForgeCLI takes each once.
    "--append-system-prompt",
    "--append-system-prompt-file",
    "--settings",
    "--effort",
    "-n",
    "--name",
    "-v",
    "--version",
    "-h",
    "--help",
];

/// Whether `arg` (`--flag` or `--flag=value`) is one a chat runs on.
fn reserved(arg: &str) -> bool {
    let flag = arg.split('=').next().unwrap_or(arg);
    RESERVED_FLAGS.contains(&flag)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ssh_config_file: None,
            ssh_extra_args: vec![],
            server_install_dir: "~/.forge".into(),
            reconnect_max_attempts: 8,
            default_shell: None,
            scrollback: 10_000,
            font_size: 13.,
            cursor_style: CursorStyle::Block,
            auto_approve_safe_commands: true,
            fallback_model: None,
            max_turns: None,
            max_budget_usd: None,
            max_time_minutes: None,
            worktree: false,
            autonomous: false,
            max_thinking_tokens: None,
            add_dirs: vec![],
            agent: None,
            tools: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
            sandbox: None,
            system_prompt: None,
            system_prompt_file: None,
            exclude_dynamic_sections: false,
            strict_mcp_config: false,
            setting_sources: None,
            bare: false,
            safe_mode: false,
            betas: vec![],
            debug: None,
            debug_file: None,
            autocompact: None,
            include_hook_events: false,
            no_session_persistence: false,
            no_input: false,
            json_schema: None,
            extra_args: vec![],
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
            "~/.forge".into()
        };
        s.default_shell = s.default_shell.filter(|x| !x.trim().is_empty());
        s.ssh_config_file = s.ssh_config_file.filter(|x| !x.trim().is_empty());
        // A model name is a plain word list; limits are positive.
        s.fallback_model = s
            .fallback_model
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty() && !m.starts_with('-') && !m.contains(char::is_whitespace));
        s.max_turns = s.max_turns.filter(|n| *n > 0);
        s.max_budget_usd = s.max_budget_usd.filter(|n| n.is_finite() && *n > 0.);
        s.max_time_minutes = s.max_time_minutes.filter(|n| *n > 0);
        s.max_thinking_tokens = s.max_thinking_tokens.filter(|n| *n >= 1024);
        s.add_dirs = s
            .add_dirs
            .iter()
            .map(|d| d.trim())
            .filter(|d| !d.is_empty())
            .map(expand_home)
            .collect();
        // An agent name is one word, as its file name is.
        s.agent = s.agent.map(|a| a.trim().to_string()).filter(|a| {
            !a.is_empty()
                && !a.starts_with('-')
                && a.chars().all(|c| c.is_alphanumeric() || "-_.:".contains(c))
        });
        let words = |v: &[String]| -> Vec<String> {
            v.iter()
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        };
        s.tools = s.tools.as_deref().map(words);
        s.allowed_tools = words(&s.allowed_tools);
        s.disallowed_tools = words(&s.disallowed_tools);
        s.betas = words(&s.betas);
        s.sandbox = s
            .sandbox
            .filter(|m| ["off", "read-only", "workspace-write"].contains(&m.as_str()));
        s.system_prompt = s.system_prompt.filter(|p| !p.trim().is_empty());
        s.system_prompt_file = s
            .system_prompt_file
            .filter(|p| !p.trim().is_empty())
            .map(|p| expand_home(p.trim()));
        s.debug_file = s
            .debug_file
            .filter(|p| !p.trim().is_empty())
            .map(|p| expand_home(p.trim()));
        s.setting_sources = s.setting_sources.map(|v| v.replace(' ', "")).filter(|v| {
            !v.is_empty()
                && v.split(',')
                    .all(|x| ["user", "project", "local"].contains(&x))
        });
        s.autocompact = s
            .autocompact
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty() && !a.starts_with('-'));
        s.debug = s.debug.map(|d| d.trim().to_string()).filter(|d| !d.starts_with('-'));
        // A schema written as text is read as JSON; a string that isn't is dropped.
        s.json_schema = s.json_schema.and_then(|j| match j {
            serde_json::Value::String(t) => serde_json::from_str(&t).ok(),
            serde_json::Value::Null => None,
            j => Some(j),
        });
        // A reserved flag takes its value with it, when that value stands alone.
        let mut extra = vec![];
        let mut skip = false;
        for a in s.extra_args.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
            if skip && !a.starts_with('-') {
                skip = false;
                continue;
            }
            skip = false;
            if reserved(a) {
                skip = !a.contains('=');
                continue;
            }
            extra.push(a.to_string());
        }
        s.extra_args = extra;
        s
    }

    /// The environment these settings give a chat's ForgeCLI.
    pub fn forge_env(&self) -> Vec<(String, String)> {
        self.max_thinking_tokens
            .map(|n| ("FORGE_MAX_THINKING_TOKENS".to_string(), n.to_string()))
            .into_iter()
            .collect()
    }

    /// The flags these settings give a chat's ForgeCLI; `new` is a new
    /// conversation (a worktree is made once, for its first process).
    pub fn forge_args(&self, new: bool) -> Vec<String> {
        let mut out = vec![];
        if let Some(m) = &self.fallback_model {
            out.extend(["--fallback-model".to_string(), m.clone()]);
        }
        if let Some(n) = self.max_turns {
            out.extend(["--max-turns".to_string(), n.to_string()]);
        }
        if let Some(n) = self.max_budget_usd {
            out.extend(["--max-budget-usd".to_string(), n.to_string()]);
        }
        if let Some(n) = self.max_time_minutes {
            out.extend(["--max-time".to_string(), format!("{n}m")]);
        }
        if self.worktree && new {
            out.push("--worktree".to_string());
        }
        if self.autonomous {
            out.push("--autonomous".to_string());
        }
        // `=` keeps each path one value, whatever it starts with.
        for d in &self.add_dirs {
            out.push(format!("--add-dir={d}"));
        }
        if let Some(a) = &self.agent {
            out.push(format!("--agent={a}"));
        }
        if let Some(tools) = &self.tools {
            if tools.is_empty() {
                out.push("--tools=".to_string());
            }
            out.extend(tools.iter().map(|t| format!("--tools={t}")));
        }
        out.extend(self.allowed_tools.iter().map(|t| format!("--allowedTools={t}")));
        out.extend(self.disallowed_tools.iter().map(|t| format!("--disallowedTools={t}")));
        if let Some(m) = &self.sandbox {
            out.push(format!("--sandbox={m}"));
        }
        if let Some(p) = &self.system_prompt {
            out.push(format!("--system-prompt={p}"));
        } else if let Some(f) = &self.system_prompt_file {
            out.push(format!("--system-prompt-file={f}"));
        }
        if self.exclude_dynamic_sections {
            out.push("--exclude-dynamic-system-prompt-sections".to_string());
        }
        if self.strict_mcp_config {
            out.push("--strict-mcp-config".to_string());
        }
        if let Some(v) = &self.setting_sources {
            out.push(format!("--setting-sources={v}"));
        }
        if self.bare {
            out.push("--bare".to_string());
        }
        if self.safe_mode {
            out.push("--safe-mode".to_string());
        }
        out.extend(self.betas.iter().map(|b| format!("--betas={b}")));
        if let Some(d) = &self.debug {
            out.push(format!("--debug={d}"));
        }
        if let Some(f) = &self.debug_file {
            out.push(format!("--debug-file={f}"));
        }
        if let Some(a) = &self.autocompact {
            out.push(format!("--autocompact={a}"));
        }
        if self.include_hook_events {
            out.push("--include-hook-events".to_string());
        }
        if self.no_session_persistence {
            out.push("--no-session-persistence".to_string());
        }
        if self.no_input {
            out.push("--no-input".to_string());
        }
        if let Some(j) = &self.json_schema {
            out.push(format!("--json-schema={j}"));
        }
        out.extend(self.extra_args.iter().cloned());
        out
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
            dark: false,
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
    #[test]
    fn forge_settings_become_forgecli_flags() {
        let s = Settings::parse(
            r#"{"forge.fallbackModel": "small,smaller", "forge.maxTurns": 40,
                "forge.maxBudgetUsd": 2.5, "forge.maxTimeMinutes": 15,
                "forge.worktree": true, "forge.autonomous": true}"#,
        );
        assert_eq!(
            s.forge_args(true),
            [
                "--fallback-model", "small,smaller", "--max-turns", "40", "--max-budget-usd", "2.5",
                "--max-time", "15m", "--worktree", "--autonomous"
            ]
        );
        // A resumed conversation already has its worktree.
        assert!(!s.forge_args(false).contains(&"--worktree".to_string()));
        // Nothing set, nothing passed; odd values are dropped.
        assert!(Settings::default().forge_args(true).is_empty());
        let odd = Settings::parse(
            r#"{"forge.fallbackModel": "--dangerously-skip-permissions", "forge.maxTurns": 0,
                "forge.maxBudgetUsd": -1}"#,
        );
        assert!(odd.forge_args(true).is_empty());
        let t = Settings::parse(r#"{"forge.maxThinkingTokens": 16000}"#);
        assert_eq!(t.forge_env(), [("FORGE_MAX_THINKING_TOKENS".to_string(), "16000".to_string())]);
        assert!(Settings::parse(r#"{"forge.maxThinkingTokens": 10}"#).forge_env().is_empty());
        let d = Settings::parse(
            r#"{"forge.addDirs": ["/srv/shared", " ", "-x dir"], "forge.agent": "reviewer"}"#,
        );
        assert_eq!(
            d.forge_args(false),
            ["--add-dir=/srv/shared", "--add-dir=-x dir", "--agent=reviewer"]
        );
        assert!(Settings::parse(r#"{"forge.agent": "--bare"}"#).forge_args(true).is_empty());
        assert!(Settings::parse(r#"{"forge.agent": "a b"}"#).forge_args(true).is_empty());
    }

    #[test]
    fn every_session_flag_has_a_key() {
        let s = Settings::parse(
            r#"{"forge.tools": ["Read", " "], "forge.allowedTools": ["Bash(git status)"],
                "forge.disallowedTools": ["WebFetch"], "forge.sandbox": "read-only",
                "forge.systemPrompt": "Be brief.", "forge.systemPromptFile": "/p.md",
                "forge.excludeDynamicSystemPromptSections": true, "forge.strictMcpConfig": true,
                "forge.settingSources": "user, project", "forge.bare": true, "forge.safeMode": true,
                "forge.betas": ["b1"], "forge.debug": "api", "forge.debugFile": "/d.log",
                "forge.autocompact": "200k", "forge.includeHookEvents": true,
                "forge.noSessionPersistence": true, "forge.noInput": true,
                "forge.jsonSchema": "{\"type\":\"object\"}",
                "forge.extraArgs": ["--color", "never", "--output-format", "json", "-p",
                                    "--session-id=x", "--dangerously-skip-permissions", "--quiet"]}"#,
        );
        assert_eq!(
            s.forge_args(false),
            [
                "--tools=Read",
                "--allowedTools=Bash(git status)",
                "--disallowedTools=WebFetch",
                "--sandbox=read-only",
                "--system-prompt=Be brief.",
                "--exclude-dynamic-system-prompt-sections",
                "--strict-mcp-config",
                "--setting-sources=user,project",
                "--bare",
                "--safe-mode",
                "--betas=b1",
                "--debug=api",
                "--debug-file=/d.log",
                "--autocompact=200k",
                "--include-hook-events",
                "--no-session-persistence",
                "--no-input",
                r#"--json-schema={"type":"object"}"#,
                "--color",
                "never",
                "--quiet",
            ]
        );
        // No tools at all, and values ForgeCLI would refuse.
        let n = Settings::parse(
            r#"{"forge.tools": [], "forge.sandbox": "everything", "forge.settingSources": "all",
                "forge.jsonSchema": "not json", "forge.systemPromptFile": "/f.md"}"#,
        );
        assert_eq!(n.forge_args(true), ["--tools=", "--system-prompt-file=/f.md"]);
    }

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
        assert_eq!(s.server_install_dir, "~/.forge");
        let s = Settings::parse(r#"{"remote.serverInstallDir": "relative/dir"}"#);
        assert_eq!(s.server_install_dir, "~/.forge");
    }

    #[test]
    fn window_state_round_trips() {
        let w: WindowState = serde_json::from_str(r#"{"terminal_open": true}"#).unwrap();
        assert!(w.terminal_open);
        assert_eq!(w.terminal_share, 0.35);
    }

    /// The settings reach real ssh: `remote.ssh.configFile` names a config
    /// with a host only it knows, and `remote.ssh.extraArgs` adds an option.
    /// `FORGE_TEST_SSH_CONFIG` is that file, `FORGE_TEST_SSH_ALIAS` the host.
    #[test]
    #[ignore]
    fn real_ssh_settings_apply() {
        let config = std::env::var("FORGE_TEST_SSH_CONFIG").expect("FORGE_TEST_SSH_CONFIG");
        let alias = std::env::var("FORGE_TEST_SSH_ALIAS").expect("FORGE_TEST_SSH_ALIAS");
        let dir = std::env::temp_dir().join(format!("forge-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("FORGE_CONFIG_DIR", &dir);
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
