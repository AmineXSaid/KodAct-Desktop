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
use crate::state::{DiffKind, DiffLine, ToolCall, ToolKind, ToolStatus};

/// What the process said, in the order it said it.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Reply text, as it streams.
    Text(String),
    /// The model's thinking, as it streams.
    Thinking(String),
    /// A new text or thinking block starts: it doesn't run on from the last.
    Break,
    /// A tool call, with its full input.
    ToolUse { id: String, name: String, input: Value },
    /// A tool call's result.
    ToolResult { id: String, output: String, is_error: bool },
    /// Forge asks before running a tool.
    Ask { request_id: String, tool_use_id: String, tool_name: String, input: Value, suggestions: Value },
    /// Forge withdrew a question (the turn was interrupted).
    CancelAsk { request_id: String },
    /// An immediate command's answer (`/btw`): it ends no turn.
    Immediate { text: String, is_error: bool },
    /// A turn ended.
    TurnEnd { ms: u64, usage: Option<crate::context::Usage>, error: Option<String> },
    /// The process couldn't start a run (bad key, unknown model...).
    Error(String),
    /// The process is gone; `detail` is the end of what it wrote on stderr.
    Exited { code: Option<i32>, detail: String },
}

// ---------------------------------------------------------------- launching

/// The `forge` executable to run. Never this app itself: Windows names are
/// case-insensitive, so a `Forge.exe` app would also answer to `forge.exe`
/// (and launching it as the engine opens a second window).
pub fn find_cli() -> Option<PathBuf> {
    let me = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok());
    let not_me = |p: &PathBuf| p.is_file() && p.canonicalize().ok() != me;
    if let Some(p) = std::env::var_os("FORGE_CLI").map(PathBuf::from).filter(not_me) {
        return Some(p);
    }
    let name = if cfg!(windows) { "forge.exe" } else { "forge" };
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
        for beside in [dir.join(name), dir.join("bin").join(name)] {
            if not_me(&beside) {
                return Some(beside);
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(not_me)
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
/// here, so the model draws diagrams Forge can render in the chat
/// (`mermaid.rs`) rather than ASCII art.
pub const SURFACE_PROMPT: &str = "You are running inside Forge Desktop, which renders your replies as Markdown. \
When a diagram would explain structure or flow better than prose (architecture, a request's path, states, a sequence of calls), \
draw it in a ```mermaid fenced block: flowchart (any direction, subgraphs allowed), sequenceDiagram or stateDiagram-v2. \
Forge draws these inline. An ```svg block with one <svg> also renders. Never draw diagrams with ASCII art.";

/// Which ForgeCLI conversation a process runs: a new one under the id the
/// desktop chose, or one already on disk, resumed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Conversation<'a> {
    New(&'a str),
    Resume(&'a str),
}

pub fn launch(profile: &Profile, mode: ModeId, bypass_allowed: bool, conversation: Conversation) -> Result<Launch, String> {
    let token = crate::health::token(&profile.auth)?;
    let mut args: Vec<String> = ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--include-partial-messages", "--permission-prompt-tool", "stdio", "--model"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.push(profile.model.clone());
    args.extend(crate::modes::cli_args(mode, bypass_allowed));
    args.extend(["--append-system-prompt".to_string(), SURFACE_PROMPT.to_string()]);
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
        let lines: Vec<String> = profile.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect();
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
        cmd.args(&launch.args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // Keys and endpoints this app may have inherited must not mix with the profile's.
        for k in ["FORGE_BASE_URL", "FORGE_API_KEY", "FORGE_AUTH_TOKEN", "FORGE_OPENAI_BASE_URL", "FORGE_OPENAI_API_KEY", "FORGE_CUSTOM_HEADERS", "FORGE_MODEL"] {
            cmd.env_remove(k);
        }
        cmd.envs(launch.env.iter().map(|(k, v)| (k, v)));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| format!("could not start {}: {e}", exe.display()))?;
        let stdin = child.stdin.take();
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
                    let cut = (cut..t.len()).find(|i| t.is_char_boundary(*i)).unwrap_or(t.len());
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

        Ok(Session { stdin: Arc::new(Mutex::new(stdin)), child, events: rx })
    }

    fn write(&self, v: Value) -> Result<(), String> {
        let mut guard = self.stdin.lock().unwrap();
        let w = guard.as_mut().ok_or("Forge's input is closed")?;
        writeln!(w, "{v}").and_then(|_| w.flush()).map_err(|e| format!("could not reach Forge: {e}"))
    }

    pub fn send(&self, text: &str) -> Result<(), String> {
        self.write(user_line(text))
    }

    pub fn answer(&self, request_id: &str, allow: bool, always: bool, input: &Value, suggestions: &Value) -> Result<(), String> {
        self.write(answer_line(request_id, allow, always, input, suggestions))
    }

    fn control(&self, request: Value) -> Result<(), String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = format!("forge-gpui-{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        self.write(json!({ "type": "control_request", "request_id": id, "request": request }))
    }

    pub fn interrupt(&self) -> Result<(), String> {
        self.control(json!({ "subtype": "interrupt" }))
    }

    pub fn set_model(&self, model: &str) -> Result<(), String> {
        self.control(json!({ "subtype": "set_model", "model": model }))
    }

    pub fn set_mode(&self, mode: ModeId) -> Result<(), String> {
        self.control(json!({ "subtype": "set_permission_mode", "mode": mode.permission().cli_name() }))
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

pub fn answer_line(request_id: &str, allow: bool, always: bool, input: &Value, suggestions: &Value) -> Value {
    let response = if allow {
        let mut r = json!({ "behavior": "allow", "updatedInput": input });
        if always && suggestions.as_array().is_some_and(|a| !a.is_empty()) {
            r["updatedPermissions"] = suggestions.clone();
        }
        r
    } else {
        json!({ "behavior": "deny", "message": "The user declined this. Ask what to do instead." })
    };
    json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } })
}

// ---------------------------------------------------------------- the stream

/// Turns stdout lines into events. Text and thinking come from the streamed
/// deltas; the complete `assistant` message that follows them adds only its
/// tool calls (and its text, when nothing streamed, for a provider that
/// doesn't stream). A sub-agent's messages stay inside its tool call.
#[derive(Default)]
pub struct Parser {
    streamed: bool,
}

impl Parser {
    pub fn line(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
        let nested = v.get("parent_tool_use_id").is_some_and(|p| !p.is_null());
        let mut out = vec![];
        match v["type"].as_str() {
            Some("stream_event") if !nested => {
                let ev = &v["event"];
                if ev["type"] == "content_block_start" && matches!(ev["content_block"]["type"].as_str(), Some("text" | "thinking")) {
                    out.push(Event::Break);
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
                            out.push(Event::Thinking(d["thinking"].as_str().unwrap_or_default().into()));
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
                        Some("text") if !streamed => out.extend([Event::Break, Event::Text(b["text"].as_str().unwrap_or_default().into())]),
                        Some("thinking") if !streamed => out.extend([Event::Break, Event::Thinking(b["thinking"].as_str().unwrap_or_default().into())]),
                        Some("tool_use") => out.push(Event::ToolUse {
                            id: b["id"].as_str().unwrap_or_default().into(),
                            name: b["name"].as_str().unwrap_or_default().into(),
                            input: b["input"].clone(),
                        }),
                        _ => {}
                    }
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
            Some("control_cancel_request") => out.push(Event::CancelAsk { request_id: v["request_id"].as_str().unwrap_or_default().into() }),
            Some("result") if !nested => {
                // An immediate command's answer (`/status`...) is not a turn.
                if v["immediate"] == true {
                    out.push(Event::Immediate { text: v["result"].as_str().unwrap_or_default().trim().to_string(), is_error: v["is_error"] == true });
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
                    let errors: Vec<String> = v["errors"].as_array().into_iter().flatten().filter_map(|e| e.as_str().map(str::to_string)).collect();
                    if !errors.is_empty() {
                        errors.join("\n")
                    } else {
                        v["result"].as_str().filter(|s| !s.is_empty()).unwrap_or("The turn ended with an error.").to_string()
                    }
                });
                out.push(Event::TurnEnd { ms: v["duration_ms"].as_u64().unwrap_or(0), usage: usage.filter(|u| u.total() > 0), error });
            }
            Some("error") => out.push(Event::Error(v["error"]["message"].as_str().unwrap_or("Forge could not start the run.").into())),
            _ => {}
        }
        out
    }
}

/// A tool result's text: a string, or the text blocks of a list.
fn result_text(c: &Value) -> String {
    match c {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
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
        "Edit" => (ToolKind::Edit, s("file_path"), None, edit_diff(&s("old_string"), &s("new_string"))),
        "MultiEdit" => {
            let mut d = vec![];
            for e in input["edits"].as_array().into_iter().flatten() {
                d.extend(edit_diff(e["old_string"].as_str().unwrap_or_default(), e["new_string"].as_str().unwrap_or_default()));
            }
            (ToolKind::Edit, s("file_path"), None, d)
        }
        "NotebookEdit" => (ToolKind::Edit, s("notebook_path"), None, vec![]),
        "Write" => (ToolKind::Write, s("file_path"), None, edit_diff("", &s("content"))),
        "Bash" | "PowerShell" => (ToolKind::Bash, s("command"), input["description"].as_str().map(str::to_string), vec![]),
        "Grep" | "Glob" | "LS" => {
            let mut t = s("pattern");
            if let Some(p) = input["path"].as_str().filter(|p| !p.is_empty()) {
                t = format!("{t} in {p}");
            }
            (ToolKind::Search, t, None, vec![])
        }
        "WebFetch" => (ToolKind::Web, s("url"), None, vec![]),
        "WebSearch" => (ToolKind::Web, s("query"), None, vec![]),
        other => {
            let summary = input["description"].as_str().or(input["prompt"].as_str()).map(|d| d.chars().take(80).collect::<String>());
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
    }
}

/// Removed lines, then added ones; long text is cut.
fn edit_diff(old: &str, new: &str) -> Vec<DiffLine> {
    const MAX: usize = 400;
    let del = old.lines().map(|l| DiffLine { kind: DiffKind::Del, text: l.to_string().into() });
    let add = new.lines().map(|l| DiffLine { kind: DiffKind::Add, text: l.to_string().into() });
    del.chain(add).take(MAX).collect()
}

/// What a permission card says about a tool call.
pub fn ask_text(tool: &str, input: &Value) -> (String, String) {
    let call = tool_call(tool, input);
    let title = match call.kind {
        ToolKind::Edit | ToolKind::Write => format!("Allow Forge to edit {}?", file_name(&call.target)),
        ToolKind::Bash => "Allow Forge to run this command?".to_string(),
        ToolKind::Web => "Allow Forge to use the web?".to_string(),
        ToolKind::Read | ToolKind::Search => "Allow Forge to read this?".to_string(),
        ToolKind::Other => format!("Allow Forge to use {tool}?"),
    };
    (title, call.target.to_string())
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
        assert_eq!(ev[3], Event::ToolResult { id: "t1".into(), output: "a\nb".into(), is_error: false });
        assert!(matches!(&ev[4], Event::TurnEnd { ms: 1500, usage: Some(u), error: None } if u.total() == 15));
        assert_eq!(ev.len(), 5);
    }

    #[test]
    fn unstreamed_text_comes_from_the_full_message() {
        let ev = parse(&[json!({"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Hi"}]}})]);
        assert_eq!(ev, vec![Event::Break, Event::Text("Hi".into())]);
    }

    #[test]
    fn sub_agent_lines_are_skipped_and_immediate_results_end_no_turn() {
        let ev = parse(&[
            json!({"type":"assistant","parent_tool_use_id":"task1","message":{"content":[{"type":"text","text":"inner"}]}}),
            json!({"type":"result","immediate":true,"num_turns":0,"is_error":false,"result":"The answer.\n"}),
        ]);
        assert_eq!(ev, vec![Event::Immediate { text: "The answer.".into(), is_error: false }]);
    }

    #[test]
    fn permission_requests_and_errors() {
        let ev = parse(&[
            json!({"type":"control_request","request_id":"forge-req-1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"/p/a.txt","content":"x"},"permission_suggestions":[{"type":"addRules"}]}}),
            json!({"type":"result","is_error":true,"stop_reason":"end_turn","duration_ms":3,"errors":["HTTP 401: bad key"]}),
            json!({"type":"result","is_error":true,"stop_reason":"interrupted","duration_ms":3}),
        ]);
        assert!(matches!(&ev[0], Event::Ask { request_id, tool_name, .. } if request_id == "forge-req-1" && tool_name == "Write"));
        assert!(matches!(&ev[1], Event::TurnEnd { error: Some(e), .. } if e.contains("401")));
        assert!(matches!(&ev[2], Event::TurnEnd { error: None, .. }));
    }

    #[test]
    fn answers_carry_the_input_and_always_carries_the_rules() {
        let input = json!({"command":"ls"});
        let sugg = json!([{"type":"addRules"}]);
        let a = answer_line("r1", true, true, &input, &sugg);
        assert_eq!(a["response"]["request_id"], "r1");
        assert_eq!(a["response"]["response"]["behavior"], "allow");
        assert_eq!(a["response"]["response"]["updatedInput"], input);
        assert_eq!(a["response"]["response"]["updatedPermissions"], sugg);
        let once = answer_line("r1", true, false, &input, &sugg);
        assert!(once["response"]["response"].get("updatedPermissions").is_none());
        assert_eq!(answer_line("r1", false, false, &input, &sugg)["response"]["response"]["behavior"], "deny");
    }

    #[test]
    fn the_endpoint_goes_in_variables_never_arguments() {
        let p: Profile = serde_json::from_value(json!({"name":"x","baseUrl":"https://gw.example/v1","wire":"anthropic","model":"m1","auth":{"type":"bearer","token":"${env:FORGE_GPUI_TEST_KEY}"}})).unwrap();
        std::env::set_var("FORGE_GPUI_TEST_KEY", "sk-test");
        let id = "0b9a1f2e-3c4d-4e5f-8a6b-7c8d9e0f1a2b";
        let l = launch(&p, ModeId::Manual, false, Conversation::New(id)).unwrap();
        assert!(l.args.windows(2).any(|w| w == ["--session-id", id]));
        let r = launch(&p, ModeId::Manual, false, Conversation::Resume(id)).unwrap();
        assert!(r.args.windows(2).any(|w| w == ["--resume", id]) && !r.args.iter().any(|a| a == "--session-id"));
        assert!(!l.args.iter().any(|a| a.contains("sk-test")));
        assert!(l.env.contains(&("FORGE_BASE_URL".into(), "https://gw.example".into())));
        assert!(l.env.contains(&("FORGE_API_KEY".into(), "sk-test".into())));
        assert!(l.args.windows(2).any(|w| w == ["--model", "m1"]));
        assert!(l.args.windows(2).any(|w| w == ["--permission-mode", "default"]));
        // The model is told its diagrams are drawn here.
        assert!(l.args.windows(2).any(|w| w[0] == "--append-system-prompt" && w[1].contains("```mermaid")));
        let o: Profile = serde_json::from_value(json!({"name":"o","baseUrl":"http://127.0.0.1:11434","wire":"openai","model":"q"})).unwrap();
        let l = launch(&o, ModeId::Manual, false, Conversation::New(id)).unwrap();
        assert!(l.env.contains(&("FORGE_OPENAI_BASE_URL".into(), "http://127.0.0.1:11434/v1".into())));
    }

    #[test]
    fn tool_rows() {
        let c = tool_call("Edit", &json!({"file_path":"src/a.rs","old_string":"a","new_string":"b\nc"}));
        assert_eq!(c.kind, ToolKind::Edit);
        assert_eq!(c.diff.len(), 3);
        let c = tool_call("Grep", &json!({"pattern":"fn main","path":"src"}));
        assert_eq!(c.target.as_ref(), "fn main in src");
        assert_eq!(tool_call("Task", &json!({"description":"look"})).kind, ToolKind::Other);
    }
}
