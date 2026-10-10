//! App state: chats, the transcript model, models and modes, and the stubs
//! that stand in for a backend. Nothing here talks to a network: `send`
//! plays a scripted reply on a timer so every part of the UI can be seen
//! working (streaming text, thinking, tool calls, a permission prompt).
//!
//! The transcript follows Forge's (the VS Code extension): a user turn, then
//! assistant turns made of blocks. A tool call is one row with its input and
//! output; its status colours the turn's timeline dot.

use std::time::Duration;

use gpui::{Context, SharedString, Task};

// ---------------------------------------------------------------- transcript

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ToolStatus {
    Running,
    Done,
    Failed,
    /// You said no when Forge asked, so it never ran.
    Declined,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ToolKind {
    Read,
    Edit,
    Write,
    Bash,
    Search,
    Web,
}

impl ToolKind {
    /// The verb Forge shows: present tense while running, past when done.
    pub fn verb(self, status: ToolStatus) -> &'static str {
        if status == ToolStatus::Declined {
            return "Skipped";
        }
        let running = status == ToolStatus::Running;
        match (self, running) {
            (ToolKind::Read, true) => "Reading",
            (ToolKind::Read, false) => "Read",
            (ToolKind::Edit, true) => "Editing",
            (ToolKind::Edit, false) => "Edited",
            (ToolKind::Write, true) => "Writing",
            (ToolKind::Write, false) => "Wrote",
            (ToolKind::Bash, true) => "Running",
            (ToolKind::Bash, false) => "Ran",
            (ToolKind::Search, true) => "Searching",
            (ToolKind::Search, false) => "Searched",
            (ToolKind::Web, true) => "Fetching",
            (ToolKind::Web, false) => "Fetched",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ToolKind::Read => "icons/file-text.svg",
            ToolKind::Edit | ToolKind::Write => "icons/file-pen.svg",
            ToolKind::Bash => "icons/terminal.svg",
            ToolKind::Search => "icons/search.svg",
            ToolKind::Web => "icons/globe.svg",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DiffKind {
    Same,
    Add,
    Del,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: SharedString,
}

#[derive(Clone, Debug)]
pub struct ToolCall {
    pub kind: ToolKind,
    /// What it acts on: a path, a command, a pattern.
    pub target: SharedString,
    /// A short note (Bash's description).
    pub note: Option<SharedString>,
    pub status: ToolStatus,
    pub output: SharedString,
    pub diff: Vec<DiffLine>,
    pub open: bool,
    /// Long output shows its last lines; this shows all of it.
    pub full: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Verdict {
    Pending,
    Allowed,
    AllowedAlways,
    Denied,
}

#[derive(Clone, Debug)]
pub enum Block {
    Text(SharedString),
    Thinking { text: SharedString, open: bool },
    Tool(ToolCall),
    Permission { title: SharedString, detail: SharedString, verdict: Verdict },
}

#[derive(Clone, Debug)]
pub enum Message {
    User { text: SharedString },
    Assistant { blocks: Vec<Block> },
    /// The end of a turn: how long it took and what it cost.
    TurnEnd { seconds: f32, cost: f32 },
    /// Something the app says (interrupted, an error).
    Notice { text: SharedString, error: bool },
}


pub struct Chat {
    pub id: usize,
    pub title: SharedString,
    pub minutes_ago: u32,
    pub pinned: bool,
    pub messages: Vec<Message>,
    /// Bumped on every change, so the list knows what to re-measure.
    pub version: u64,
}

impl Chat {
    pub fn group(&self) -> &'static str {
        if self.pinned {
            return "Pinned";
        }
        match self.minutes_ago {
            0..=1439 => "Today",
            1440..=2879 => "Yesterday",
            2880..=10079 => "Previous 7 days",
            _ => "Older",
        }
    }
}

// ---------------------------------------------------------------- models and modes

pub struct Model {
    pub id: &'static str,
    pub name: &'static str,
    pub provider: &'static str,
    pub note: &'static str,
    pub context: &'static str,
}

pub const MODELS: &[Model] = &[
    Model { id: "opus", name: "Opus 5.5", provider: "Anthropic", note: "Most capable, for complex work", context: "200k" },
    Model { id: "sonnet", name: "Sonnet 5.5", provider: "Anthropic", note: "Fast and capable", context: "1M" },
    Model { id: "haiku", name: "Haiku 5.5", provider: "Anthropic", note: "Fastest, for quick answers", context: "200k" },
    Model { id: "openrouter/qwen/qwen3-coder", name: "Qwen3 Coder", provider: "OpenRouter", note: "Open weights, tool calling", context: "262k" },
    Model { id: "ollama/gpt-oss:20b", name: "gpt-oss 20b", provider: "Ollama on this PC", note: "Local, private", context: "128k" },
];

/// What fills the main area.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Chat,
    /// What Claude Code reads in this project, and when (`ui/rules_view.rs`).
    Rules,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Ask,
    AcceptEdits,
    Plan,
    Auto,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Ask, Mode::AcceptEdits, Mode::Plan, Mode::Auto];
    pub fn label(self) -> &'static str {
        match self {
            Mode::Ask => "Ask before edits",
            Mode::AcceptEdits => "Edit automatically",
            Mode::Plan => "Plan mode",
            Mode::Auto => "Auto",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Mode::Ask => "Ask",
            Mode::AcceptEdits => "Accept edits",
            Mode::Plan => "Plan",
            Mode::Auto => "Auto",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Mode::Ask => "icons/hand.svg",
            Mode::AcceptEdits => "icons/file-pen.svg",
            Mode::Plan => "icons/map.svg",
            Mode::Auto => "icons/zap.svg",
        }
    }
    pub fn next(self) -> Mode {
        let i = Mode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Mode::ALL[(i + 1) % Mode::ALL.len()]
    }
}

// ---------------------------------------------------------------- the state

pub struct AppState {
    /// What Forge calls the person; None until the first-run screen sets it.
    pub user_name: Option<SharedString>,
    /// The first-run screen is showing (also reopened to change the name).
    pub onboarding: bool,
    /// The project's folder name, shown in the title bar.
    pub project: SharedString,
    /// The project's folder: `FORGE_PROJECT_DIR`, else where Forge started.
    pub project_dir: std::path::PathBuf,
    pub screen: Screen,
    pub branch: SharedString,
    pub chats: Vec<Chat>,
    pub current: Option<usize>,
    pub sidebar_open: bool,
    pub search: SharedString,
    pub model: usize,
    pub mode: Mode,
    pub dark: bool,
    pub palette_open: bool,
    pub picker_open: bool,
    /// The code block whose Copy was just pressed (shows "Copied" briefly).
    pub copied: Option<SharedString>,
    copied_reset: Option<Task<()>>,
    /// A short message at the bottom of the window (stubs say what they would do).
    pub toast: Option<SharedString>,
    toast_reset: Option<Task<()>>,
    /// Tokens used / context size, for the composer's meter.
    pub context_used: f32,
    /// The reply being played, if any. Dropping it stops the reply.
    reply: Option<Task<()>>,
    /// When the current turn started (for the working line's timer).
    pub turn_started: Option<std::time::Instant>,
    next_id: usize,
}

impl AppState {
    pub fn new() -> Self {
        let chats = mock_chats();
        let next_id = chats.len() + 1;
        let user_name: Option<SharedString> = crate::profile::load_name().map(Into::into);
        let project_dir = std::env::var_os("FORGE_PROJECT_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        Self {
            onboarding: user_name.is_none(),
            user_name,
            project: project_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into()).into(),
            project_dir,
            screen: Screen::Chat,
            branch: "fix/hermes_upgrade".into(),
            current: Some(chats[0].id),
            chats,
            sidebar_open: true,
            search: "".into(),
            model: 0,
            mode: Mode::Ask,
            dark: false,
            palette_open: false,
            picker_open: false,
            copied: None,
            copied_reset: None,
            toast: None,
            toast_reset: None,
            context_used: 0.31,
            reply: None,
            turn_started: None,
            next_id,
        }
    }

    pub fn chat(&self) -> Option<&Chat> {
        self.current.and_then(|id| self.chats.iter().find(|c| c.id == id))
    }

    fn chat_mut(&mut self, id: usize) -> Option<&mut Chat> {
        self.chats.iter_mut().find(|c| c.id == id)
    }

    pub fn busy(&self) -> bool {
        self.reply.is_some()
    }

    pub fn open_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        self.stop(cx);
        self.screen = Screen::Chat;
        self.current = Some(id);
        cx.notify();
    }

    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.stop(cx);
        self.screen = Screen::Chat;
        self.current = None;
        cx.notify();
    }

    pub fn toggle_pin(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(c) = self.chat_mut(id) {
            c.pinned = !c.pinned;
        }
        cx.notify();
    }

    /// Save the name from the first-run screen. Returns an error to show.
    pub fn set_user_name(&mut self, name: &str, cx: &mut Context<Self>) -> Result<(), &'static str> {
        let name = crate::profile::clean(name).ok_or("Type a name first.")?;
        if crate::profile::save_name(&name).is_err() {
            // Still usable for this run; the next launch asks again.
            self.show_toast("Your name could not be saved; Forge will ask again next time.", cx);
        }
        self.user_name = Some(name.into());
        self.onboarding = false;
        cx.notify();
        Ok(())
    }

    pub fn mark_copied(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.copied = Some(key);
        self.copied_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            let _ = this.update(cx, |s, cx| {
                s.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn show_toast(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some(text.into());
        self.toast_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(2600)).await;
            let _ = this.update(cx, |s, cx| {
                s.toast = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The text of the newest reply, for "Copy last reply".
    pub fn last_reply(&self) -> Option<String> {
        self.chat()?.messages.iter().rev().find_map(|m| match m {
            Message::Assistant { blocks } => {
                let text: Vec<&str> = blocks.iter().filter_map(|b| if let Block::Text(t) = b { Some(t.as_ref()) } else { None }).collect();
                (!text.is_empty()).then(|| text.join("\n\n"))
            }
            _ => None,
        })
    }

    /// Flip a thinking block or a tool row open/closed.
    pub fn toggle_block(&mut self, chat: usize, msg: usize, block: usize, cx: &mut Context<Self>) {
        if let Some(c) = self.chat_mut(chat) {
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                match blocks.get_mut(block) {
                    Some(Block::Thinking { open, .. }) => *open = !*open,
                    Some(Block::Tool(t)) => t.open = !t.open,
                    _ => {}
                }
                c.version += 1;
            }
        }
        cx.notify();
    }

    /// Show all of a tool's output, or only its last lines again.
    pub fn toggle_full(&mut self, chat: usize, msg: usize, block: usize, cx: &mut Context<Self>) {
        if let Some(c) = self.chat_mut(chat) {
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                if let Some(Block::Tool(t)) = blocks.get_mut(block) {
                    t.full = !t.full;
                }
                c.version += 1;
            }
        }
        cx.notify();
    }

    /// Answer the permission prompt the reply is waiting on.
    pub fn answer(&mut self, chat: usize, msg: usize, block: usize, verdict: Verdict, cx: &mut Context<Self>) {
        if let Some(c) = self.chat_mut(chat) {
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                if let Some(Block::Permission { verdict: v, .. }) = blocks.get_mut(block) {
                    *v = verdict;
                }
            }
            c.version += 1;
        }
        cx.notify();
    }

    /// Stop the reply being played (the composer's Stop, Escape).
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if self.reply.take().is_none() {
            return;
        }
        self.turn_started = None;
        if let Some(id) = self.current {
            if let Some(c) = self.chat_mut(id) {
                for m in c.messages.iter_mut() {
                    if let Message::Assistant { blocks } = m {
                        for b in blocks.iter_mut() {
                            if let Block::Tool(t) = b {
                                if t.status == ToolStatus::Running {
                                    t.status = ToolStatus::Failed;
                                    t.output = "Interrupted.".into();
                                }
                            }
                        }
                    }
                }
                c.messages.push(Message::Notice { text: "Interrupted. Tell Forge what to do instead.".into(), error: false });
                c.version += 1;
            }
        }
        cx.notify();
    }

    /// Stub for the backend: add the message and play a scripted reply.
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        if text.is_empty() || self.busy() {
            return;
        }
        let id = match self.current {
            Some(id) => id,
            None => {
                let id = self.next_id;
                self.next_id += 1;
                let title: String = text.lines().next().unwrap_or_default().chars().take(48).collect();
                self.chats.insert(0, Chat { id, title: title.into(), minutes_ago: 0, pinned: false, messages: vec![], version: 0 });
                self.current = Some(id);
                id
            }
        };
        if let Some(c) = self.chat_mut(id) {
            c.messages.push(Message::User { text: text.clone().into() });
            c.minutes_ago = 0;
            c.version += 1;
        }
        self.turn_started = Some(std::time::Instant::now());
        self.reply = Some(cx.spawn(async move |this, cx| {
            for step in script(&text) {
                let wait = step.delay();
                cx.background_executor().timer(Duration::from_millis(wait)).await;
                // A permission prompt holds the reply until it is answered.
                loop {
                    let waiting = this
                        .update(cx, |s, _| s.waiting_on_permission(id))
                        .unwrap_or(false);
                    if !waiting {
                        break;
                    }
                    cx.background_executor().timer(Duration::from_millis(120)).await;
                }
                let denied = this.update(cx, |s, _| s.last_permission_denied(id)).unwrap_or(false);
                let step = if denied { Step::Denied } else { step };
                let done = matches!(step, Step::End { .. } | Step::Denied);
                if this.update(cx, |s, cx| s.apply(id, step, cx)).is_err() {
                    return;
                }
                if done {
                    break;
                }
            }
            let _ = this.update(cx, |s, cx| {
                s.reply = None;
                s.turn_started = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn waiting_on_permission(&self, chat: usize) -> bool {
        self.chats.iter().find(|c| c.id == chat).is_some_and(|c| {
            matches!(c.messages.last(), Some(Message::Assistant { blocks }) if blocks.iter().any(|b| matches!(b, Block::Permission { verdict: Verdict::Pending, .. })))
        })
    }

    fn last_permission_denied(&self, chat: usize) -> bool {
        self.chats.iter().find(|c| c.id == chat).is_some_and(|c| {
            matches!(c.messages.last(), Some(Message::Assistant { blocks }) if matches!(blocks.last(), Some(Block::Permission { verdict: Verdict::Denied, .. })))
        })
    }

    fn apply(&mut self, chat: usize, step: Step, cx: &mut Context<Self>) {
        let Some(c) = self.chat_mut(chat) else { return };
        let ends_with_assistant = matches!(c.messages.last(), Some(Message::Assistant { .. }));
        if !ends_with_assistant && !matches!(step, Step::End { .. }) {
            c.messages.push(Message::Assistant { blocks: vec![] });
        }
        let blocks = match c.messages.last_mut() {
            Some(Message::Assistant { blocks }) => Some(blocks),
            _ => None,
        };
        match (step, blocks) {
            (Step::Think(t), Some(blocks)) => match blocks.last_mut() {
                Some(Block::Thinking { text, .. }) => *text = format!("{text}{t}").into(),
                _ => blocks.push(Block::Thinking { text: t.into(), open: false }),
            },
            (Step::Say(t), Some(blocks)) => match blocks.last_mut() {
                Some(Block::Text(text)) => *text = format!("{text}{t}").into(),
                _ => blocks.push(Block::Text(t.into())),
            },
            (Step::ToolStart(kind, target, note), Some(blocks)) => blocks.push(Block::Tool(ToolCall {
                kind,
                target: target.into(),
                note: note.map(Into::into),
                status: ToolStatus::Running,
                output: "".into(),
                diff: vec![],
                // A command's terminal block is open from the start, as in Forge.
                open: kind == ToolKind::Bash,
                full: false,
            })),
            (Step::ToolDone(output, diff), Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks.iter_mut().rev().find(|b| matches!(b, Block::Tool(_))) {
                    t.status = ToolStatus::Done;
                    t.output = output.into();
                    t.diff = diff;
                }
            }
            (Step::Ask(title, detail), Some(blocks)) => blocks.push(Block::Permission { title: title.into(), detail: detail.into(), verdict: Verdict::Pending }),
            (Step::Denied, Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks.iter_mut().rev().find(|b| matches!(b, Block::Tool(_))) {
                    t.status = ToolStatus::Declined;
                    t.output = "You declined this edit.".into();
                }
                blocks.push(Block::Text("Understood, I won't change the file. Tell me how you'd like to proceed.".into()));
            }
            (Step::End { seconds, cost }, _) => {
                c.messages.push(Message::TurnEnd { seconds, cost });
                self.context_used = (self.context_used + 0.04).min(0.97);
            }
            _ => {}
        }
        if let Some(c) = self.chat_mut(chat) {
            c.version += 1;
        }
        cx.notify();
    }
}

// ---------------------------------------------------------------- the scripted reply

enum Step {
    Think(String),
    Say(String),
    ToolStart(ToolKind, String, Option<String>),
    ToolDone(String, Vec<DiffLine>),
    Ask(String, String),
    Denied,
    End { seconds: f32, cost: f32 },
}

impl Step {
    fn delay(&self) -> u64 {
        match self {
            Step::Think(_) | Step::Say(_) => 45,
            Step::ToolStart(..) => 350,
            Step::ToolDone(..) => 900,
            Step::Ask(..) => 300,
            Step::Denied | Step::End { .. } => 250,
        }
    }
}

fn words(s: &str, f: fn(String) -> Step) -> Vec<Step> {
    let mut out = vec![];
    let mut cur = String::new();
    for ch in s.chars() {
        cur.push(ch);
        if ch == ' ' || ch == '\n' {
            out.push(f(std::mem::take(&mut cur)));
        }
    }
    if !cur.is_empty() {
        out.push(f(cur));
    }
    out
}

fn diff(lines: &[(DiffKind, &str)]) -> Vec<DiffLine> {
    lines.iter().map(|(k, t)| DiffLine { kind: *k, text: (*t).into() }).collect()
}

fn script(prompt: &str) -> Vec<Step> {
    let mut s = vec![];
    s.extend(words(
        &format!("The user asked: \"{}\". I should read the entry point first, change only what is needed, then run the tests to prove it.", prompt.lines().next().unwrap_or_default()),
        Step::Think,
    ));
    s.extend(words("Let me look at how the greeting is wired before changing anything.", Step::Say));
    s.push(Step::ToolStart(ToolKind::Read, "src/main.rs".into(), None));
    s.push(Step::ToolDone("fn main() {\n    println!(\"Hello, world\");\n}\n".into(), vec![]));
    s.push(Step::ToolStart(ToolKind::Edit, "src/main.rs".into(), None));
    s.push(Step::Ask("Edit src/main.rs?".into(), "Replace the greeting with a function main calls, and add a test for it.".into()));
    s.push(Step::ToolDone(
        "The file was updated.".into(),
        diff(&[
            (DiffKind::Add, "pub fn greeting() -> &'static str {"),
            (DiffKind::Add, "    \"Hello, Forge\""),
            (DiffKind::Add, "}"),
            (DiffKind::Add, ""),
            (DiffKind::Same, "fn main() {"),
            (DiffKind::Del, "    println!(\"Hello, world\");"),
            (DiffKind::Add, "    println!(\"{}\", greeting());"),
            (DiffKind::Same, "}"),
        ]),
    ));
    s.push(Step::ToolStart(ToolKind::Bash, "cargo test".into(), Some("Run the tests".into())));
    s.push(Step::ToolDone("   \x1b[1;32mCompiling\x1b[0m hello v0.1.0 (/home/amine/dev/kryptonite)\n    \x1b[1;32mFinished\x1b[0m `test` profile [unoptimized + debuginfo] target(s) in 1.84s\n     \x1b[1;32mRunning\x1b[0m unittests src/main.rs (target/debug/deps/hello-3f2a9c1e)\n\nrunning 3 tests\ntest tests::greets_by_name ... \x1b[32mok\x1b[0m\ntest tests::greeting_is_static ... \x1b[32mok\x1b[0m\ntest tests::main_prints_the_greeting ... \x1b[32mok\x1b[0m\n\ntest result: \x1b[32mok\x1b[0m. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n   \x1b[1;32mDoc-tests\x1b[0m hello\n\nrunning 0 tests\n\ntest result: \x1b[32mok\x1b[0m. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s".into(), vec![]));
    s.extend(words(
        "Done. The greeting now lives in `greeting()` and `main` prints it.\n\n**What changed**\n- `src/main.rs`: a new `greeting()` function, used by `main`.\n- A unit test, `greets_by_name`, checks the text.\n\n```rust\npub fn greeting() -> &'static str {\n    \"Hello, Forge\"\n}\n```\n\nAll tests pass.",
        Step::Say,
    ));
    s.push(Step::End { seconds: 14.2, cost: 0.0312 });
    s
}

// ---------------------------------------------------------------- mock chats

fn mock_chats() -> Vec<Chat> {
    let rich = vec![
        Message::User { text: "Fix the greeting in src/main.rs so it says Hello, Forge, and add a test for it.".into() },
        Message::Assistant {
            blocks: vec![
                Block::Thinking {
                    text: "The greeting is printed directly in `main`. To test it I should move the string into a function that `main` calls, then assert on that function.".into(),
                    open: false,
                },
                Block::Text("I'll move the greeting into a small function so it can be tested, then update `main`.".into()),
                Block::Tool(ToolCall { kind: ToolKind::Read, target: "src/main.rs".into(), note: None, status: ToolStatus::Done, output: "fn main() {\n    println!(\"Hello, world\");\n}\n".into(), diff: vec![], open: false, full: false }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Edit,
                    target: "src/main.rs".into(),
                    note: None,
                    status: ToolStatus::Done,
                    output: "".into(),
                    diff: diff(&[
                        (DiffKind::Add, "pub fn greeting() -> &'static str {"),
                        (DiffKind::Add, "    \"Hello, Forge\""),
                        (DiffKind::Add, "}"),
                        (DiffKind::Add, ""),
                        (DiffKind::Same, "fn main() {"),
                        (DiffKind::Del, "    println!(\"Hello, world\");"),
                        (DiffKind::Add, "    println!(\"{}\", greeting());"),
                        (DiffKind::Same, "}"),
                    ]),
                    open: true,
                    full: false,
                }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Bash,
                    target: "cargo test".into(),
                    note: Some("Run the tests".into()),
                    status: ToolStatus::Done,
                    output: "   \x1b[1;32mCompiling\x1b[0m hello v0.1.0 (/home/amine/dev/kryptonite)\n    \x1b[1;32mFinished\x1b[0m `test` profile [unoptimized + debuginfo] target(s) in 1.84s\n     \x1b[1;32mRunning\x1b[0m unittests src/main.rs (target/debug/deps/hello-3f2a9c1e)\n\nrunning 3 tests\ntest tests::greets_by_name ... \x1b[32mok\x1b[0m\ntest tests::greeting_is_static ... \x1b[32mok\x1b[0m\ntest tests::main_prints_the_greeting ... \x1b[32mok\x1b[0m\n\ntest result: \x1b[32mok\x1b[0m. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n   \x1b[1;32mDoc-tests\x1b[0m hello\n\nrunning 0 tests\n\ntest result: \x1b[32mok\x1b[0m. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s".into(),
                    diff: vec![],
                    open: true,
                    full: false,
                }),
            ],
        },
        Message::Assistant {
            blocks: vec![Block::Text(
                "Done. The greeting now lives in `greeting()` and `main` prints it.\n\n## What changed\n\n- `src/main.rs`: new **`greeting()`** function, used by `main`.\n- A unit test, *greets_by_name*, checks the text.\n\n```rust\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn greets_by_name() {\n        assert_eq!(super::greeting(), \"Hello, Forge\");\n    }\n}\n```\n\n> Run `cargo test` any time to check it again.\n\n1. Build with `cargo build`.\n2. Run it with `cargo run`.".into(),
            )],
        },
        Message::TurnEnd { seconds: 18.4, cost: 0.0412 },
        Message::User { text: "Can you also check the hermes review pipeline for unused imports?".into() },
        Message::Assistant {
            blocks: vec![
                Block::Tool(ToolCall { kind: ToolKind::Web, target: "https://eslint.org/docs/latest/rules/no-unused-vars".into(), note: None, status: ToolStatus::Done, output: "no-unused-vars: disallow unused variables, including imports.".into(), diff: vec![], open: false, full: false }),
                Block::Tool(ToolCall { kind: ToolKind::Write, target: "docs/lint-notes.md".into(), note: None, status: ToolStatus::Done, output: "".into(), diff: diff(&[(DiffKind::Add, "# Lint notes"), (DiffKind::Add, ""), (DiffKind::Add, "Run `pnpm lint --filter hermes` before pushing.")]), open: false, full: false }),
                Block::Tool(ToolCall { kind: ToolKind::Search, target: "^use .*;$ in src/hermes".into(), note: None, status: ToolStatus::Done, output: "src/hermes/review.ts:3\nsrc/hermes/store.ts:1".into(), diff: vec![], open: false, full: false }),
                Block::Tool(ToolCall { kind: ToolKind::Bash, target: "pnpm lint --filter hermes".into(), note: Some("Lint the package".into()), status: ToolStatus::Failed, output: "\x1b[4msrc/hermes/review.ts\x1b[24m\n  \x1b[2m3:10\x1b[22m  \x1b[31merror\x1b[39m  'diffLines' is defined but never used  \x1b[2m@typescript-eslint/no-unused-vars\x1b[22m\n\n\x1b[31m\x1b[1m✖ 1 problem (1 error, 0 warnings)\x1b[22m\x1b[39m\n\x1b[2m ELIFECYCLE \x1b[22m Command failed with exit code 1.".into(), diff: vec![], open: true, full: false }),
                Block::Text("One unused import: `diffLines` in `src/hermes/review.ts`. Want me to remove it?".into()),
            ],
        },
        Message::TurnEnd { seconds: 6.1, cost: 0.0087 },
    ];
    let simple = |title: &str| {
        vec![
            Message::User { text: title.to_string().into() },
            Message::Assistant { blocks: vec![Block::Text(format!("This is an older conversation about *{}*.", title.to_lowercase()).into())] },
            Message::TurnEnd { seconds: 3.2, cost: 0.004 },
        ]
    };
    let mut chats = vec![
        Chat { id: 1, title: "Fix the greeting and add a test".into(), minutes_ago: 4, pinned: true, messages: rich, version: 0 },
        Chat { id: 2, title: "Explain the hermes review pipeline".into(), minutes_ago: 120, pinned: false, messages: simple("Explain the hermes review pipeline"), version: 0 },
        Chat { id: 3, title: "Speed up the session store".into(), minutes_ago: 300, pinned: false, messages: simple("Speed up the session store"), version: 0 },
        Chat { id: 4, title: "Write a README for the CLI".into(), minutes_ago: 1500, pinned: false, messages: simple("Write a README for the CLI"), version: 0 },
        Chat { id: 5, title: "Why does cargo test hang on Windows?".into(), minutes_ago: 1800, pinned: false, messages: simple("Why does cargo test hang on Windows?"), version: 0 },
        Chat { id: 6, title: "Migrate settings to the new schema".into(), minutes_ago: 5000, pinned: false, messages: simple("Migrate settings to the new schema"), version: 0 },
        Chat { id: 7, title: "Genesis redesign: landing page".into(), minutes_ago: 20000, pinned: false, messages: simple("Genesis redesign: landing page"), version: 0 },
    ];
    chats.sort_by_key(|c| (!c.pinned, c.minutes_ago));
    chats
}
