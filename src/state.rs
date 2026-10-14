//! App state: chats, the transcript model, models and modes. `send` hands
//! your message to the chat's ForgeCLI process (`backend.rs`) and its events
//! fill the reply; with `FORGE_DEMO=1` it plays a scripted reply instead, so
//! every part of the UI can be seen working without a model. The chats the
//! sidebar starts with are demo data.
//!
//! The transcript follows Forge's (the VS Code extension): a user turn, then
//! assistant turns made of blocks. A tool call is one row with its input and
//! output; its status colours the turn's timeline dot.

use std::time::{Duration, SystemTime};

use gpui::{AppContext, Context, SharedString, Task};

use crate::modes::ModeId;

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
    /// Any other tool (a sub-agent, an MCP tool): its name is the target.
    Other,
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
            (ToolKind::Other, true) => "Using",
            (ToolKind::Other, false) => "Used",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ToolKind::Read => "icons/file-text.svg",
            ToolKind::Edit | ToolKind::Write => "icons/file-pen.svg",
            ToolKind::Bash => "icons/terminal.svg",
            ToolKind::Search => "icons/search.svg",
            ToolKind::Web => "icons/globe.svg",
            ToolKind::Other => "icons/cpu.svg",
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
    /// What you did about the change it made (Keep / Revert).
    pub review: Review,
}

impl ToolCall {
    /// What its row calls it: the browser's tools as `browser · click`, a
    /// plan and a task list by name, anything else as its target.
    pub fn shown_target(&self) -> SharedString {
        if let Some(action) = self.target.strip_prefix(crate::browser::TOOL_PREFIX) {
            let action = action.strip_prefix("browser_").unwrap_or(action);
            return format!("browser · {}", action.replace('_', " ")).into();
        }
        match self.target.as_ref() {
            "ExitPlanMode" => "plan".into(),
            "TodoWrite" => "task list".into(),
            "Task" | "Agent" => "agent".into(),
            _ => self.target.clone(),
        }
    }
}

/// A file change's review: not looked at yet, kept, or reverted on disk.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Review {
    #[default]
    Open,
    Kept,
    Reverted,
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
    Thinking {
        text: SharedString,
        open: bool,
    },
    Tool(ToolCall),
    Permission {
        title: SharedString,
        detail: SharedString,
        verdict: Verdict,
    },
    /// A file Forge made for you: a card in the reply, opened beside the chat.
    File(FileRef),
}

/// A sub-agent Forge started (a `Task` / `Agent` tool call), as the
/// Agents panel shows it.
#[derive(Clone, Debug)]
pub struct AgentRun {
    /// The tool call that started it.
    pub id: String,
    /// Its kind (`subagent_type`): Explore, general-purpose, a custom agent.
    pub kind: String,
    /// What it is for, in a few words.
    pub task: String,
    /// What it was told.
    pub prompt: String,
    pub started: std::time::Instant,
    pub ended: Option<std::time::Instant>,
    pub failed: bool,
    pub steps: Vec<AgentStep>,
    /// Its latest words; once it ends, its answer.
    pub said: String,
}

/// One tool call a sub-agent made.
#[derive(Clone, Debug)]
pub struct AgentStep {
    pub id: String,
    pub kind: ToolKind,
    /// What it acts on, or a command's description.
    pub what: String,
    /// A command's own description stands alone ("Run the tests").
    pub described: bool,
    pub status: ToolStatus,
}

impl AgentStep {
    /// "Reading src/main.rs" while it runs, "Read src/main.rs" after.
    pub fn label(&self) -> String {
        if self.described {
            self.what.clone()
        } else {
            format!("{} {}", self.kind.verb(self.status), self.what)
        }
    }
}

/// The tools that start a sub-agent.
pub fn is_agent_tool(name: &str) -> bool {
    matches!(name, "Task" | "Agent")
}

/// The panel's browser as the UI shows it.
#[derive(Default)]
pub struct WebPage {
    /// The latest picture of the page.
    pub frame: Option<std::sync::Arc<gpui::RenderImage>>,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub tabs: Vec<crate::web_view::TabInfo>,
    /// The tab on screen.
    pub active: String,
    /// Why there is no browser, when there isn't.
    pub error: Option<String>,
}

/// What the side panel shows when no file is open in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelTab {
    /// The chat's latest plan and task list.
    Plan,
    /// Every file the chat made: Markdown, workbooks, PDFs, diagrams.
    Files,
    /// Its sub-agents at work.
    Agents,
    /// What the browser is on, its latest screenshot, its recent steps.
    Browser,
}

/// A file Forge made: shown as a card in the reply (`ui/file_card.rs`),
/// opened in the side panel (`ui/doc_pane.rs`), saved by Download.
#[derive(Clone, Debug, PartialEq)]
pub struct FileRef {
    /// Its file name: `settings-migration-plan.md`.
    pub name: SharedString,
    /// Where it was written in the project, if it was.
    pub path: Option<SharedString>,
    /// Its text: Markdown, a diagram's page. Empty for a binary file.
    pub content: SharedString,
    /// A binary file's bytes: a workbook, a Word file, a deck, a PDF.
    pub bytes: Option<std::sync::Arc<[u8]>>,
}

impl FileRef {
    /// What the card calls it: the document's first heading, else its name
    /// without the extension.
    pub fn title(&self) -> SharedString {
        if self.is_diagram() {
            if let Some(t) = crate::diagram::title(&self.content) {
                return t.into();
            }
        }
        if self.is_markdown() {
            if let Some(h) = self.content.lines().find_map(|l| l.strip_prefix("# ")) {
                return h.trim().to_string().into();
            }
        }
        self.name
            .rsplit_once('.')
            .map_or(self.name.to_string(), |(stem, _)| stem.to_string())
            .into()
    }

    /// Its type, as the card labels it: `MD`, `PDF`, `DOCX`.
    pub fn kind(&self) -> SharedString {
        self.name
            .rsplit_once('.')
            .map_or("FILE".to_string(), |(_, ext)| ext.to_ascii_uppercase())
            .into()
    }

    /// An Office file or a PDF, read natively (`office.rs`).
    pub fn office(&self) -> Option<crate::office::Kind> {
        crate::office::Kind::of(&self.name)
    }

    /// What a card calls it: Document, Diagram, Spreadsheet, Presentation.
    pub fn label(&self) -> &'static str {
        if let Some(k) = self.office() {
            k.label()
        } else if self.is_diagram() {
            "Diagram"
        } else if self.is_markdown() {
            "Document"
        } else {
            "File"
        }
    }

    /// The file as saved: its bytes, else its text.
    pub fn data(&self) -> Vec<u8> {
        self.bytes
            .as_deref()
            .map(<[u8]>::to_vec)
            .unwrap_or_else(|| self.content.as_bytes().to_vec())
    }

    pub fn is_markdown(&self) -> bool {
        matches!(self.kind().as_ref(), "MD" | "MARKDOWN")
    }

    /// A diagram Forge draws itself (`diagram.rs`): an SVG, or a Diagram
    /// Design page holding one.
    pub fn is_diagram(&self) -> bool {
        crate::diagram::is_diagram(&self.name, &self.content)
    }
}

impl Message {
    /// Your message, without attachments.
    #[cfg(test)]
    pub fn user(text: SharedString, sent: SystemTime) -> Self {
        Message::User {
            text,
            sent,
            attachments: vec![],
            turn: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    /// Your message: its text, the files you attached, and the id ForgeCLI
    /// gave its turn (for Revert), once known.
    User {
        text: SharedString,
        sent: SystemTime,
        attachments: Vec<crate::attach::Attachment>,
        turn: Option<SharedString>,
    },
    Assistant {
        blocks: Vec<Block>,
    },
    /// The end of a turn: how long it took, and the usage the model
    /// reported for it (`context.rs`).
    TurnEnd {
        seconds: f32,
        usage: Option<crate::context::Usage>,
    },
    /// Something the app says (interrupted, an error).
    Notice {
        text: SharedString,
        error: bool,
    },
}

pub struct Chat {
    pub id: usize,
    /// The project it belongs to.
    pub project: usize,
    pub title: SharedString,
    pub minutes_ago: u32,
    pub pinned: bool,
    pub messages: Vec<Message>,
    /// Bumped on every change, so the list knows what to re-measure.
    pub version: u64,
    /// A reply finished here while you were looking at another chat.
    pub unread: bool,
    /// Its ForgeCLI conversation's id (`--session-id`, then `--resume`) and
    /// its file name in the history (`history.rs`).
    pub session: String,
    /// ForgeCLI has written this conversation: the next process resumes it.
    pub resumable: bool,
    /// When it last changed, for the sidebar's dates and the history's order.
    pub updated: SystemTime,
    /// Earlier versions of the conversation, replaced by an edit or a retry.
    pub versions: Vec<Version>,
}

/// A replaced version of a conversation: from your message at `at` on, as
/// it was, and the ForgeCLI conversation it had (restoring it resumes that).
#[derive(Clone, Debug)]
pub struct Version {
    pub at: usize,
    pub tail: Vec<Message>,
    pub session: String,
    pub resumable: bool,
    pub when: SystemTime,
}

impl Version {
    /// The first words of its reply, for the versions list.
    pub fn preview(&self) -> String {
        let text = turn_text(&self.tail, self.tail.len().saturating_sub(1));
        let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.chars().count() > 280 {
            format!("{}…", flat.chars().take(279).collect::<String>())
        } else if flat.is_empty() {
            "(no reply text)".into()
        } else {
            flat
        }
    }

    /// What you asked in it.
    pub fn asked(&self) -> String {
        match self.tail.first() {
            Some(Message::User { text, .. }) => text
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(80)
                .collect(),
            _ => String::new(),
        }
    }
}

/// The most versions kept per chat (the oldest go first).
const MAX_VERSIONS: usize = 20;

/// Where a chat stands, as its sidebar row shows it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChatStatus {
    /// A reply is playing.
    Working,
    /// A reply waits on a permission answer.
    NeedsYou,
    /// It ended on an error.
    Error,
    /// A reply finished while you were elsewhere.
    Unread,
    Idle,
}

impl Chat {
    /// A new, empty chat with a fresh conversation id.
    pub fn new(id: usize, project: usize, title: SharedString) -> Self {
        Self {
            id,
            project,
            title,
            minutes_ago: 0,
            pinned: false,
            messages: vec![],
            version: 0,
            unread: false,
            session: crate::history::new_id(),
            resumable: false,
            updated: SystemTime::now(),
            versions: vec![],
        }
    }

    /// Start a new ForgeCLI conversation for this chat (its old one can't
    /// follow: the transcript was cut back, or the endpoint changed).
    pub fn new_conversation(&mut self) {
        self.session = crate::history::new_id();
        self.resumable = false;
    }

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

/// What fills the main area.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Chat,
    /// What Claude Code reads in this project, and when (`ui/rules_view.rs`).
    Rules,
    /// Every project (`ui/projects.rs`).
    Projects,
    /// One project: its chats, and a composer to start another.
    Project(usize),
}

/// The Settings window's sections.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SettingsTab {
    General,
    Models,
    Profiles,
    Commands,
    Agents,
    Skills,
    Hooks,
    Mcp,
    Permissions,
    Sandbox,
    Network,
    Environments,
    Memory,
    Plugins,
    Guide,
    About,
}

impl SettingsTab {
    /// The editors backed by ForgeCLI's files (`forge_files.rs`): open, they
    /// read those files and keep them current.
    pub fn reads_engine_files(self) -> bool {
        matches!(
            self,
            SettingsTab::Commands
                | SettingsTab::Agents
                | SettingsTab::Skills
                | SettingsTab::Hooks
                | SettingsTab::Mcp
                | SettingsTab::Permissions
                | SettingsTab::Sandbox
                | SettingsTab::Environments
                | SettingsTab::Memory
                | SettingsTab::Plugins
                | SettingsTab::Profiles
        )
    }
}

/// The project map (`ui/project_map.rs`): the project's folders at a glance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectMap {
    pub project: std::path::PathBuf,
    /// Top-level folders, with the files under each.
    pub folders: Vec<(String, usize)>,
    /// Files at the top level.
    pub files: Vec<String>,
    pub total: usize,
    /// The walk stopped at its cap: there are more.
    pub capped: bool,
}

/// How many files the map's walk reads at most.
pub const MAP_CAP: usize = 20_000;

impl ProjectMap {
    /// Sum project-relative paths (`/`-separated) by top-level folder.
    pub fn from_files(project: std::path::PathBuf, paths: &[String]) -> Self {
        let mut folders: std::collections::BTreeMap<String, usize> = Default::default();
        let mut files = vec![];
        for p in paths {
            match p.split_once('/') {
                Some((dir, _)) => *folders.entry(dir.to_string()).or_default() += 1,
                None => files.push(p.clone()),
            }
        }
        files.sort();
        let mut folders: Vec<(String, usize)> = folders.into_iter().collect();
        // Biggest first: where the code is.
        folders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        ProjectMap {
            project,
            total: paths.len(),
            capped: paths.len() >= MAP_CAP,
            folders,
            files,
        }
    }
}

/// A file a chat changed (its Write and Edit rows), for the map.
#[derive(Clone, Debug, PartialEq)]
pub struct Edited {
    pub path: String,
    /// How many of the chat's tool calls changed it.
    pub times: usize,
    pub review: Review,
}

/// An endpoint being diagnosed (`diagnose.rs`), step by step.
pub struct Diagnosis {
    pub profile: String,
    pub steps: Vec<crate::diagnose::Step>,
    pub running: bool,
    /// Which run this is: a newer one replaces an older one's updates.
    run: u64,
}

/// What the Settings editors show: ForgeCLI's files for this project.
#[derive(Default)]
pub struct Engine {
    /// The last read; None until the first one ends.
    pub catalog: Option<crate::forge_files::Catalog>,
    /// A read is under way.
    pub loading: bool,
    /// The files' fingerprint at the last read.
    signature: u64,
    /// Whose files were read.
    project: Option<std::path::PathBuf>,
    /// Polls the files while an editor is open.
    watch: Option<Task<()>>,
    /// Each MCP server's state as `forge mcp list` last said it.
    pub mcp_status: std::collections::BTreeMap<String, String>,
    pub mcp_checking: bool,
    /// What went wrong with the last check (no ForgeCLI, it failed…).
    pub mcp_error: Option<String>,
    /// The settings profiles (`setting_profiles.rs`), read with the catalog.
    pub profiles: Vec<crate::setting_profiles::SettingsProfile>,
}

/// A folder Forge works in. Its chats, rules and memory belong to it.
#[derive(Clone, Debug)]
pub struct Project {
    pub id: usize,
    pub name: SharedString,
    pub path: std::path::PathBuf,
    pub branch: SharedString,
    pub pinned: bool,
    /// When it was last opened, in minutes before now.
    pub opened_minutes_ago: u32,
}

// ---------------------------------------------------------------- the engine's bookkeeping

/// A chat's ForgeCLI process, the task applying its events, and the
/// profile it was started on.
struct Live {
    session: crate::backend::Session,
    _pump: Task<()>,
    profile: crate::endpoints::Profile,
    /// Which process this is: events from one that was replaced are dropped.
    generation: u64,
}

/// A permission question Forge is waiting on, and where its card is.
struct Ask {
    request_id: String,
    tool_use_id: String,
    input: serde_json::Value,
    suggestions: serde_json::Value,
    msg: usize,
    block: usize,
}

// ---------------------------------------------------------------- side questions

/// The side-question card's copy, as Forge for VS Code has it.
pub const SIDE_TITLE: &str = "Side question";
pub const SIDE_EMPTY: &str =
    "Ask a quick side question below without interrupting the conversation.";
pub const SIDE_PLACEHOLDER: &str = "Ask a side question";
pub const SIDE_PENDING: &str = "Answering…";
/// The thread keeps the last 20 exchanges.
const SIDE_MAX: usize = 20;

/// One `/btw` exchange.
#[derive(Clone, Debug, PartialEq)]
pub enum SideItem {
    Answer {
        question: SharedString,
        answer: SharedString,
    },
    Failed {
        question: SharedString,
        message: SharedString,
    },
}

/// A chat's side questions: answered by ForgeCLI from the conversation,
/// without tools, while the conversation goes on untouched.
#[derive(Clone, Debug, Default)]
pub struct Side {
    pub visible: bool,
    /// Folded to its title bar, still asking and answering.
    pub minimized: bool,
    pub thread: Vec<SideItem>,
    /// Questions sent and not yet answered, oldest first.
    pub pending: std::collections::VecDeque<SharedString>,
}

impl Side {
    fn push(&mut self, item: SideItem) {
        self.thread.push(item);
        let over = self.thread.len().saturating_sub(SIDE_MAX);
        self.thread.drain(..over);
    }
}

/// `/btw` and its question (empty for a bare `/btw`), or None for anything else.
pub fn parse_btw(text: &str) -> Option<String> {
    let t = text.trim();
    let rest = t.strip_prefix("/btw").or_else(|| t.strip_prefix("/BTW"))?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim().to_string())
}

// ---------------------------------------------------------------- find, ratings

/// What the Stop notice says; its row offers Continue.
pub const INTERRUPTED: &str = "Interrupted. Tell Forge what to do instead.";

/// A reply's rating (the thumbs under a turn). Kept in memory for now:
/// sending it is a stub (`stubs::FEEDBACK_UPLOAD`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rating {
    Good,
    Bad,
}

/// Find in the conversation (Ctrl/⌘F): the query, the messages that hold
/// it, and the one shown. `seq` changes on every move, so the list scrolls.
#[derive(Clone, Debug, Default)]
pub struct Find {
    pub query: String,
    pub hits: Vec<usize>,
    pub at: usize,
    pub seq: u64,
}

impl Find {
    /// The message the find is on.
    pub fn current(&self) -> Option<usize> {
        self.hits.get(self.at).copied()
    }
}

/// Whether a message holds `needle` (already lower-case): your text, the
/// reply's prose and thinking, tool targets and output, notices.
pub fn message_matches(m: &Message, needle: &str) -> bool {
    let has = |t: &str| t.to_lowercase().contains(needle);
    match m {
        Message::User { text, .. } => has(text),
        Message::Assistant { blocks } => blocks.iter().any(|b| match b {
            Block::Text(t) => has(t),
            Block::Thinking { text, .. } => has(text),
            Block::Tool(c) => {
                has(&c.target)
                    || has(&c.output)
                    || c.note.as_ref().is_some_and(|n| has(n))
                    || c.diff.iter().any(|l| has(&l.text))
            }
            Block::Permission { title, detail, .. } => has(title) || has(detail),
            Block::File(f) => has(&f.name),
        }),
        Message::TurnEnd { .. } => false,
        Message::Notice { text, .. } => has(text),
    }
}

/// The messages of `messages` that match `query` (case-insensitive).
pub fn find_hits(messages: &[Message], query: &str) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return vec![];
    }
    messages
        .iter()
        .enumerate()
        .filter(|(_, m)| message_matches(m, &needle))
        .map(|(i, _)| i)
        .collect()
}

/// The prose of the reply that ends at `end` (a turn end or the last
/// message): every text block after your message before it.
pub fn turn_text(messages: &[Message], end: usize) -> String {
    let end = end.min(messages.len().saturating_sub(1));
    let start = messages[..=end]
        .iter()
        .rposition(|m| matches!(m, Message::User { .. }))
        .map_or(0, |u| u + 1);
    let mut parts = vec![];
    for m in &messages[start..=end] {
        if let Message::Assistant { blocks } = m {
            parts.extend(blocks.iter().filter_map(|b| {
                if let Block::Text(t) = b {
                    Some(t.trim().to_string())
                } else {
                    None
                }
            }));
        }
    }
    parts.join("\n\n")
}

/// Your message that a reply ending at `end` answers.
pub fn turn_question(messages: &[Message], end: usize) -> Option<usize> {
    messages
        .get(..=end.min(messages.len().saturating_sub(1)))?
        .iter()
        .rposition(|m| matches!(m, Message::User { .. }))
}

/// Whole minutes since `t` (0 for the future).
fn minutes_since(t: SystemTime) -> u32 {
    t.elapsed()
        .map(|d| (d.as_secs() / 60).min(u32::MAX as u64) as u32)
        .unwrap_or(0)
}

/// Said while ForgeCLI summarizes the conversation, and when it has.
pub const COMPACTING: &str = "Compacting the conversation…";
pub const COMPACTED: &str = "Compacted: earlier messages were summarized to free context. Forge still knows what was decided.";

/// Said under a reply that was still running when Forge closed.
pub const CLOSED_MID_REPLY: &str =
    "Forge was closed before this reply finished. Send a message to pick it up.";

/// A `rewind_files` request for ForgeCLI's turn `turn`.
fn json_rewind(turn: &str, dry_run: bool) -> serde_json::Value {
    serde_json::json!({ "subtype": "rewind_files", "user_message_id": turn, "dry_run": dry_run })
}

/// Whether ForgeCLI's absolute path `full` is the tool row's `target`
/// (absolute, or relative to the project).
fn same_file(full: &str, target: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/");
    let (f, t) = (norm(full), norm(target));
    f == t || f.ends_with(&format!("/{}", t.trim_start_matches("./")))
}

/// What Continue sends: Forge picks up where Stop cut it off.
const CONTINUE_PROMPT: &str = "Continue from where you stopped.";

/// `FORGE_DEMO=1`: replies are the scripted demo instead of ForgeCLI.
fn demo_mode() -> bool {
    std::env::var("FORGE_DEMO").is_ok_and(|v| v == "1")
}

/// What the confirm window asks.
#[derive(Clone, Debug, PartialEq)]
pub enum Confirm {
    /// Delete this chat?
    Delete(usize),
    /// Put these files back as they were before your message `msg`?
    Revert {
        chat: usize,
        msg: usize,
        files: Vec<String>,
    },
}

/// A command `/` offers, and the group it is listed under.
#[derive(Clone, Debug)]
pub struct CompletionCommand {
    pub name: String,
    pub args: String,
    pub description: String,
    pub group: &'static str,
}

/// A control request sent to a chat's ForgeCLI, and what its answer is for.
#[derive(Clone, Debug)]
pub enum Request {
    /// "Which files would reverting to before this message change?"
    RevertPlan { chat: usize, msg: usize },
    /// Revert them.
    Revert { chat: usize, msg: usize },
}

// ---------------------------------------------------------------- the state

pub struct AppState {
    /// What Forge calls the person; None until the first-run screen sets it.
    pub user_name: Option<SharedString>,
    /// Their avatar (`avatars.rs`): chosen on the first-run screen, else one
    /// given at random and kept.
    pub avatar: SharedString,
    avatar_saved: bool,
    /// This launch began on the first-run screen.
    first_run: bool,
    /// The first-run screen is showing (also reopened to change the name).
    pub onboarding: bool,
    pub projects: Vec<Project>,
    /// The project you're in.
    pub current_project: usize,
    /// The sidebar lists every project's chats, not only this project's.
    pub all_chats: bool,
    /// The title bar's project switcher, and the sidebar's chat filter, are open.
    pub switcher_open: bool,
    pub filter_open: bool,
    /// The current project's name, shown in the title bar.
    pub project: SharedString,
    /// The project's folder: `FORGE_PROJECT_DIR`, else where Forge started.
    pub project_dir: std::path::PathBuf,
    pub screen: Screen,
    pub branch: SharedString,
    pub chats: Vec<Chat>,
    pub current: Option<usize>,
    pub sidebar_open: bool,
    pub search: SharedString,
    /// Endpoint profiles (each one endpoint and its model); None while
    /// they load. No model is built in.
    pub endpoints: Option<crate::endpoints::Endpoints>,
    /// Each profile's last check (`health.rs`).
    pub health: crate::health::Store,
    /// The effort level, when the model takes one; None is Auto.
    pub effort: Option<crate::endpoints::Effort>,
    /// Checks waiting for a slot, and how many are running.
    check_queue: std::collections::VecDeque<(String, bool)>,
    checks_running: usize,
    /// Each profile's newest check, so an older one's result is dropped.
    check_gen: std::collections::HashMap<String, u64>,
    sync_timer: Option<Task<()>>,
    /// Picks the new-chat greeting (`greeting.rs`): changes with each new
    /// chat, and every few minutes while the greeting shows.
    pub greeting_seed: u64,
    greeting_timer: Option<Task<()>>,
    /// The current chat's mode (Expert, Manual, Edit automatically, Plan, Bypass).
    pub mode: ModeId,
    /// Modes chats keep when reopened (see `ModeId::kept`), by chat.
    kept_modes: std::collections::HashMap<usize, ModeId>,
    /// The mode new chats start in (Settings > General).
    pub default_mode: ModeId,
    /// Bypass permissions has been allowed, behind its warning.
    pub bypass_allowed: bool,
    /// The web browser tools (`browser_mcp.rs`) are given to new ForgeCLI sessions.
    pub browser: bool,
    /// The Modes menu is open; its Bypass warning is showing.
    pub modes_open: bool,
    pub bypass_warning: bool,
    pub dark: bool,
    pub palette_open: bool,
    pub picker_open: bool,
    /// The code block whose Copy was just pressed (shows "Copied" briefly).
    pub copied: Option<SharedString>,
    /// The file open in the side panel, beside the chat.
    pub doc: Option<FileRef>,
    /// The side panel's tab when no file is open in it: the plan, the
    /// chat's files or the browser (`ui/workbench.rs`). None: closed.
    pub panel: Option<PanelTab>,
    /// The browser in the side panel (`web_view.rs`), once started.
    pub web: Option<std::sync::Arc<crate::web_view::WebView>>,
    /// What it shows: the page, its tabs, whether it is loading.
    pub web_page: WebPage,
    /// Each chat's sub-agents, in the order they started (`agents_view.rs`).
    pub agents: std::collections::HashMap<usize, Vec<AgentRun>>,
    /// Agent cards opened to show their steps, by the agent's id.
    pub agents_open: std::collections::HashSet<String>,
    /// A diagram in the panel fits its width (else it shows at actual size).
    pub doc_fit: bool,
    /// The side panel sits below the chat instead of beside it.
    pub doc_below: bool,
    /// An Office file or PDF in the panel, read: by file name, the preview
    /// or why it couldn't be read. None while it is being read.
    pub doc_preview: Option<(
        SharedString,
        Result<std::sync::Arc<crate::office::Preview>, String>,
    )>,
    /// The sheet shown, for a workbook.
    pub doc_sheet: usize,
    copied_reset: Option<Task<()>>,
    /// A short message at the bottom of the window (stubs say what they would do).
    pub toast: Option<SharedString>,
    toast_reset: Option<Task<()>>,
    /// The replies being played, by chat. Each keeps going while you look at
    /// another chat; dropping one stops it.
    replies: std::collections::HashMap<usize, Task<()>>,
    /// Each chat's ForgeCLI process (`backend.rs`), kept between turns, and
    /// the task that applies its events.
    sessions: std::collections::HashMap<usize, Live>,
    /// Turn results still to come from interrupted turns, by chat: until
    /// they arrive, what the process says belongs to the stopped turn.
    stale_turns: std::collections::HashMap<usize, u32>,
    /// Where each tool call's row is (message, block), by chat and call id.
    tool_rows: std::collections::HashMap<(usize, String), (usize, usize)>,
    /// The permission question each chat is waiting on.
    asks: std::collections::HashMap<usize, Ask>,
    /// When each running turn started.
    turn_started: std::collections::HashMap<usize, std::time::Instant>,
    /// Chats whose next text starts a new block rather than running on.
    fresh_block: std::collections::HashSet<usize>,
    /// The Settings window is open, on this section.
    pub settings_open: bool,
    pub settings_tab: SettingsTab,
    /// Settings should open the Add endpoint form (taken by the view).
    pub settings_add: bool,
    /// Each chat's side questions (`/btw`).
    pub side: std::collections::HashMap<usize, Side>,
    /// The side card's input should take focus (it was just opened).
    pub side_focus: bool,
    /// Text for the composer to take (an edited message).
    pub draft: Option<String>,
    /// Ratings given to replies, by chat and the turn's end message.
    pub ratings: std::collections::HashMap<(usize, usize), Rating>,
    /// Find in the conversation, while its bar is open.
    pub find: Option<Find>,
    /// The chat whose title is being edited in the sidebar.
    pub renaming: Option<usize>,
    /// The chat whose menu (⋯ or right-click) is open.
    pub chat_menu: Option<usize>,
    /// The chat waiting on "Delete?" to be confirmed.
    pub confirm: Option<Confirm>,
    /// The keyboard shortcuts sheet is open.
    pub shortcuts_open: bool,
    /// Diagrams in replies showing their source instead (by block key).
    pub diagram_code: std::sync::Arc<std::collections::HashSet<SharedString>>,
    /// Chats are still loading: screens show placeholders.
    pub loading: bool,
    load: Option<Task<()>>,
    /// Where chats are kept between launches (`history.rs`); None keeps them in memory only.
    history: Option<std::path::PathBuf>,
    /// Chats changed since they were last written, and the pending write.
    dirty: std::collections::HashSet<usize>,
    save_task: Option<Task<()>>,
    /// Chats whose process is resuming a conversation and hasn't spoken yet:
    /// if it ends first, the conversation couldn't be reopened.
    resuming: std::collections::HashSet<usize>,
    /// Keeps the sidebar's "5m" / "2h" true as time passes.
    clock: Option<Task<()>>,
    /// Counts ForgeCLI processes started, to tell their events apart.
    generations: u64,
    /// Files waiting in the composer to go with the next message.
    pub attachments: Vec<crate::attach::Attachment>,
    /// The last attachment id given (a chip is matched to its result by it).
    attachment_seq: u64,
    /// Pasted images so far, to number their names.
    pasted: usize,
    /// Answers ForgeCLI gives at once, in the order asked, by chat: `true`
    /// for a command typed in the composer (`/status`), `false` for `/btw`.
    immediate_queue: std::collections::HashMap<usize, std::collections::VecDeque<bool>>,
    /// Control requests waiting on ForgeCLI's answer, by request id.
    requests: std::collections::HashMap<String, Request>,
    /// For `@` completion: the project's files, read once per project.
    file_index: Option<(std::path::PathBuf, std::sync::Arc<Vec<String>>)>,
    /// For `/` completion: the project's own commands (`.forge/commands`).
    custom_commands: Option<(std::path::PathBuf, Vec<crate::commands::Command>)>,
    /// A read of either is under way.
    pub completions_loading: bool,
    /// The context window's breakdown is open (the composer's meter).
    pub context_open: bool,
    /// The versions list open under your message: (chat, message).
    pub versions_open: Option<(usize, usize)>,
    /// A message to scroll to (its move's number, its index), and the count.
    pub jump: Option<(u64, usize)>,
    jump_seq: u64,
    /// The task panel and the approvals list are open.
    pub tasks_open: bool,
    pub approvals_open: bool,
    /// Chats whose running turn is a compaction.
    compacting: std::collections::HashSet<usize>,
    /// ForgeCLI's own files, for the Settings editors.
    pub engine: Engine,
    /// Settings → Models → Diagnose: the endpoint checked and its steps.
    pub diagnosis: Option<Diagnosis>,
    /// Counts diagnoses, so a stale run's steps never land on a new one.
    diagnosis_seq: u64,
    /// The project map is open, and its last read (None while reading).
    pub map_open: bool,
    pub map: Option<ProjectMap>,
    /// The settings profile each chat's ForgeCLI gets (`--settings`).
    pub settings_profile: Option<String>,
    next_id: usize,
}

impl AppState {
    pub fn new() -> Self {
        // No chats until you start one: the demo ones live in tests.
        let chats: Vec<Chat> = vec![];
        let next_id = chats.len() + 1;
        let user_name: Option<SharedString> = crate::profile::load_name().map(Into::into);
        let saved_avatar = crate::profile::load_avatar();
        let project_dir = std::env::var_os("FORGE_PROJECT_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let projects = initial_projects(&project_dir);
        Self {
            onboarding: user_name.is_none(),
            first_run: user_name.is_none(),
            user_name,
            avatar_saved: saved_avatar.is_some(),
            avatar: saved_avatar
                .unwrap_or_else(|| {
                    crate::avatars::random(
                        SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(7),
                    )
                    .to_string()
                })
                .into(),
            project: projects[0].name.clone(),
            project_dir,
            branch: projects[0].branch.clone(),
            current_project: projects[0].id,
            projects,
            all_chats: false,
            switcher_open: false,
            filter_open: false,
            screen: Screen::Chat,
            current: None,
            chats,
            sidebar_open: true,
            search: "".into(),
            endpoints: None,
            health: Default::default(),
            effort: None,
            check_queue: Default::default(),
            checks_running: 0,
            check_gen: Default::default(),
            sync_timer: None,
            greeting_seed: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            greeting_timer: None,
            mode: ModeId::Manual,
            kept_modes: Default::default(),
            default_mode: ModeId::Manual,
            bypass_allowed: false,
            browser: crate::browser::enabled(),
            modes_open: false,
            bypass_warning: false,
            dark: false,
            palette_open: false,
            picker_open: false,
            copied: None,
            doc: None,
            panel: None,
            web: None,
            web_page: WebPage::default(),
            agents: std::collections::HashMap::new(),
            agents_open: std::collections::HashSet::new(),
            doc_fit: true,
            doc_below: false,
            doc_preview: None,
            doc_sheet: 0,
            copied_reset: None,
            toast: None,
            toast_reset: None,
            replies: Default::default(),
            sessions: Default::default(),
            stale_turns: Default::default(),
            tool_rows: Default::default(),
            asks: Default::default(),
            turn_started: Default::default(),
            fresh_block: Default::default(),
            settings_open: false,
            settings_tab: SettingsTab::General,
            settings_add: false,
            side: Default::default(),
            side_focus: false,
            draft: None,
            ratings: Default::default(),
            find: None,
            renaming: None,
            chat_menu: None,
            confirm: None,
            shortcuts_open: false,
            diagram_code: Default::default(),
            loading: true,
            load: None,
            history: crate::history::dir(),
            dirty: Default::default(),
            save_task: None,
            resuming: Default::default(),
            clock: None,
            generations: 0,
            attachments: vec![],
            attachment_seq: 0,
            pasted: 0,
            immediate_queue: Default::default(),
            requests: Default::default(),
            file_index: None,
            custom_commands: None,
            completions_loading: false,
            context_open: false,
            versions_open: None,
            jump: None,
            jump_seq: 0,
            tasks_open: false,
            approvals_open: false,
            compacting: Default::default(),
            engine: Default::default(),
            diagnosis: None,
            diagnosis_seq: 0,
            map_open: false,
            map: None,
            settings_profile: crate::setting_profiles::active(),
            next_id,
        }
    }

    /// Load the chats. In the full app this reads the session index off the
    /// UI thread; this shell's chats are built in, so it only stands in for
    /// that read's time, so the loading placeholders can be seen.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        // Your chats from before, read off the UI thread.
        let dir = self.history.clone();
        self.load = Some(cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move {
                    let all = dir
                        .as_deref()
                        .map(crate::history::load_all)
                        .unwrap_or_default();
                    all.into_iter()
                        .map(|c| {
                            let m = crate::history::messages_of(&c);
                            let v = crate::history::versions_of(&c);
                            (c, m, v)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.restore(saved, cx);
                s.loading = false;
                cx.notify();
            });
        }));
        // Anything not yet written is written as the app closes.
        cx.on_app_quit(|s, _| {
            s.flush_history();
            async {}
        })
        .detach();
        self.clock = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(60))
                .await;
            if this.update(cx, |s, cx| s.tick_ages(cx)).is_err() {
                break;
            }
        }));
        self.load_endpoints(false, cx);
        // Someone who never chose keeps the avatar they were given.
        if !self.avatar_saved && !self.onboarding {
            self.save_avatar(cx);
        }
        // A fresh greeting every few minutes while the new-chat screen shows.
        const GREETING_EVERY: Duration = Duration::from_secs(5 * 60);
        self.greeting_timer = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(GREETING_EVERY).await;
            let shown = this.update(cx, |s, cx| {
                if s.screen == Screen::Chat && s.current.is_none() {
                    s.greeting_seed = s.greeting_seed.wrapping_add(1);
                    cx.notify();
                }
            });
            if shown.is_err() {
                break;
            }
        }));
    }

    // ------------------------------------------------------------ models

    /// Read the profiles and their saved checks off the UI thread, then
    /// check whatever is due and keep checking on the interval.
    /// Read endpoints.json (off the UI thread); `check_all` then checks every
    /// model, as refresh does after you edit the file.
    pub fn load_endpoints(&mut self, check_all: bool, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let (endpoints, health) = cx
                .background_spawn(async move {
                    let e = crate::endpoints::load();
                    let h = crate::health::load(&e.profiles);
                    (e, h)
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                let minutes = endpoints.sync_interval_minutes;
                s.endpoints = Some(endpoints);
                s.health = health;
                if check_all {
                    s.sync_all(cx);
                } else {
                    s.sync_due(cx);
                }
                if minutes > 0. {
                    let every = Duration::from_secs_f64(minutes * 60.);
                    s.sync_timer = Some(cx.spawn(async move |this, cx| loop {
                        cx.background_executor().timer(every).await;
                        if this.update(cx, |s, cx| s.sync_due(cx)).is_err() {
                            break;
                        }
                    }));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The profile in use.
    pub fn active_profile(&self) -> Option<&crate::endpoints::Profile> {
        self.endpoints.as_ref().and_then(|e| e.active())
    }

    /// Pick a profile (the model window's row); kept for next time.
    pub fn select_profile(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(e) = self.endpoints.as_mut() else {
            return;
        };
        if !e.profiles.iter().any(|p| p.name == name) {
            return;
        }
        e.selected = Some(name.to_string());
        self.picker_open = false;
        // An effort the new model doesn't take is dropped.
        let levels = self
            .active_profile()
            .map(|p| p.effort_levels())
            .unwrap_or_default();
        if self.effort.is_some_and(|l| !levels.contains(&l)) {
            self.effort = None;
        }
        let name = name.to_string();
        cx.background_spawn(async move {
            let _ = crate::endpoints::save_selected(&name);
        })
        .detach();
        cx.notify();
    }

    /// Check every profile that is due (at start, and on the timer).
    pub fn sync_due(&mut self, cx: &mut Context<Self>) {
        let Some(e) = &self.endpoints else { return };
        let now = crate::health::now_ms();
        let due: Vec<String> = e
            .profiles
            .iter()
            .filter(|p| {
                crate::health::is_due(
                    self.health.get(&p.name).and_then(|h| h.last_synced_at),
                    now,
                    e.sync_interval_minutes,
                )
            })
            .map(|p| p.name.clone())
            .collect();
        self.check(due, true, cx);
    }

    /// The model window's refresh: check them all now.
    pub fn sync_all(&mut self, cx: &mut Context<Self>) {
        let names = self
            .endpoints
            .as_ref()
            .map(|e| e.profiles.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();
        self.check(names, false, cx);
    }

    /// Any check is running (the refresh button turns).
    pub fn checking(&self) -> bool {
        self.health.values().any(|h| h.syncing)
    }

    fn check(&mut self, names: Vec<String>, background: bool, cx: &mut Context<Self>) {
        for name in names {
            // A new check of a profile replaces one still waiting.
            self.check_queue.retain(|(n, _)| *n != name);
            *self.check_gen.entry(name.clone()).or_default() += 1;
            let fingerprint = self
                .endpoints
                .as_ref()
                .and_then(|e| e.profiles.iter().find(|p| p.name == name))
                .map(|p| p.fingerprint())
                .unwrap_or_default();
            let entry = self.health.entry(name.clone()).or_default();
            if entry.fingerprint != fingerprint {
                *entry = crate::health::EndpointHealth {
                    fingerprint,
                    ..Default::default()
                };
            }
            entry.syncing = true;
            self.check_queue.push_back((name, background));
        }
        self.pump_checks(cx);
        cx.notify();
    }

    /// Start waiting checks while there are free slots.
    fn pump_checks(&mut self, cx: &mut Context<Self>) {
        while self.checks_running < crate::health::SWEEP_PARALLELISM {
            let Some((name, background)) = self.check_queue.pop_front() else {
                break;
            };
            let Some(profile) = self
                .endpoints
                .as_ref()
                .and_then(|e| e.profiles.iter().find(|p| p.name == name))
                .cloned()
            else {
                // Gone from the list: nothing to check, so stop showing it as checking.
                if let Some(h) = self.health.get_mut(&name) {
                    h.syncing = false;
                }
                continue;
            };
            let generation = self.check_gen.get(&name).copied().unwrap_or_default();
            self.checks_running += 1;
            cx.spawn(async move |this, cx| {
                let timeout = crate::health::timeout_for(&profile, background);
                let probed = profile.clone();
                let (outcome, listed) = cx
                    .background_spawn(async move {
                        (
                            crate::health::probe(&probed, timeout),
                            crate::health::listed_context(&probed, timeout),
                        )
                    })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    s.checks_running -= 1;
                    if s.check_gen.get(&profile.name) == Some(&generation) {
                        crate::health::apply(&mut s.health, &profile, outcome, listed);
                        let store = s.health.clone();
                        cx.background_spawn(async move { crate::health::save(&store) })
                            .detach();
                    }
                    s.pump_checks(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    // ------------------------------------------------------------ history

    /// Chats read back at start become the sidebar's, each in its project
    /// (a project folder not open yet is added).
    fn restore(
        &mut self,
        saved: Vec<(crate::history::SavedChat, Vec<Message>, Vec<Version>)>,
        cx: &mut Context<Self>,
    ) {
        let mut added_project = false;
        for (c, messages, versions) in saved {
            if self.chats.iter().any(|x| x.session == c.session) {
                continue;
            }
            let project = match self.projects.iter().find(|p| p.path == c.project) {
                Some(p) => p.id,
                None => {
                    let id = self.projects.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                    let name = c
                        .project
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| c.project.display().to_string());
                    self.projects.push(Project {
                        id,
                        name: name.into(),
                        branch: git_branch(&c.project).into(),
                        path: c.project.clone(),
                        pinned: false,
                        opened_minutes_ago: 60,
                    });
                    added_project = true;
                    id
                }
            };
            let id = self.next_id;
            self.next_id += 1;
            let updated = crate::history::updated_of(&c);
            let mut messages = messages;
            // A reply cut off by closing the app says so, rather than just stopping.
            if matches!(
                messages.last(),
                Some(Message::Assistant { .. }) | Some(Message::User { .. })
            ) {
                messages.push(Message::Notice {
                    text: CLOSED_MID_REPLY.into(),
                    error: false,
                });
            }
            self.chats.push(Chat {
                minutes_ago: minutes_since(updated),
                pinned: c.pinned,
                messages,
                session: c.session,
                resumable: c.resumable,
                updated,
                versions,
                ..Chat::new(id, project, c.title.into())
            });
        }
        // Pinned first is the sidebar's job; here, the most recent first.
        self.chats.sort_by_key(|c| std::cmp::Reverse(c.updated));
        if added_project {
            let paths: Vec<std::path::PathBuf> = self
                .projects
                .iter()
                .skip(1)
                .map(|p| p.path.clone())
                .collect();
            cx.background_spawn(async move { save_projects(paths) })
                .detach();
        }
    }

    /// A chat changed: it moves to now, and is written shortly (changes
    /// close together are written once).
    fn touch(&mut self, chat: usize, cx: &mut Context<Self>) {
        if let Some(c) = self.chat_mut(chat) {
            c.updated = SystemTime::now();
            c.minutes_ago = 0;
        }
        self.mark_dirty(chat, cx);
    }

    /// Write a chat soon, without changing its date (a rename, a pin).
    fn mark_dirty(&mut self, chat: usize, cx: &mut Context<Self>) {
        if self.history.is_none() {
            return;
        }
        self.dirty.insert(chat);
        if self.save_task.is_some() {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(600))
                .await;
            let _ = this.update(cx, |s, cx| {
                s.save_task = None;
                let batch = s.take_dirty();
                if let Some(dir) = s.history.clone() {
                    cx.background_spawn(async move {
                        for c in batch {
                            if let Err(e) = crate::history::save(&dir, &c) {
                                eprintln!("could not save chat {}: {e}", c.session);
                            }
                        }
                    })
                    .detach();
                }
            });
        }));
    }

    /// The chats waiting to be written, in their on-disk form.
    fn take_dirty(&mut self) -> Vec<crate::history::SavedChat> {
        let ids: Vec<usize> = self.dirty.drain().collect();
        ids.iter()
            .filter_map(|id| self.chats.iter().find(|c| c.id == *id))
            // An empty chat (a new one never sent) isn't kept.
            .filter(|c| !c.messages.is_empty())
            .map(|c| {
                let dir = self
                    .project(c.project)
                    .map(|p| p.path.clone())
                    .unwrap_or_else(|| self.project_dir.clone());
                crate::history::save_form(c, &dir)
            })
            .collect()
    }

    /// Write what's pending now (the app is closing).
    fn flush_history(&mut self) {
        self.save_task = None;
        let batch = self.take_dirty();
        if let Some(dir) = &self.history {
            for c in batch {
                let _ = crate::history::save(dir, &c);
            }
        }
    }

    fn tick_ages(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for c in self.chats.iter_mut() {
            let m = minutes_since(c.updated);
            if m != c.minutes_ago {
                c.minutes_ago = m;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub fn chat(&self) -> Option<&Chat> {
        self.current
            .and_then(|id| self.chats.iter().find(|c| c.id == id))
    }

    fn chat_mut(&mut self, id: usize) -> Option<&mut Chat> {
        self.chats.iter_mut().find(|c| c.id == id)
    }

    /// A reply is playing in the chat you're looking at.
    pub fn busy(&self) -> bool {
        self.current
            .is_some_and(|id| self.replies.contains_key(&id))
    }

    /// Where a chat stands, for its row in the sidebar.
    pub fn status(&self, chat: &Chat) -> ChatStatus {
        if self.replies.contains_key(&chat.id) {
            return if self.waiting_on_permission(chat.id) {
                ChatStatus::NeedsYou
            } else {
                ChatStatus::Working
            };
        }
        match chat.messages.last() {
            Some(Message::Notice { error: true, .. }) => ChatStatus::Error,
            _ if chat.unread => ChatStatus::Unread,
            _ => ChatStatus::Idle,
        }
    }

    pub fn project(&self, id: usize) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// Work in another project: the title bar, the Rules screen and new
    /// chats follow it. The open chat stays only if it belongs there.
    pub fn select_project(&mut self, id: usize, cx: &mut Context<Self>) {
        let Some(p) = self.projects.iter_mut().find(|p| p.id == id) else {
            return;
        };
        p.opened_minutes_ago = 0;
        let (name, path, branch) = (p.name.clone(), p.path.clone(), p.branch.clone());
        self.current_project = id;
        self.project = name;
        self.project_dir = path;
        self.branch = branch;
        self.switcher_open = false;
        if self.chat().is_some_and(|c| c.project != id) {
            self.current = None;
            self.sync_mode();
        }
        cx.notify();
    }

    /// Open a project's page.
    pub fn show_project(&mut self, id: usize, cx: &mut Context<Self>) {
        self.select_project(id, cx);
        self.screen = Screen::Project(id);
        cx.notify();
    }

    /// Add a folder as a project (or open it, if it already is one).
    pub fn add_project(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        if let Some(p) = self.projects.iter().find(|p| p.path == path) {
            let id = p.id;
            return self.show_project(id, cx);
        }
        let id = self.projects.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.projects.push(Project {
            id,
            name: name.into(),
            branch: git_branch(&path).into(),
            path,
            pinned: false,
            opened_minutes_ago: 0,
        });
        let paths: Vec<std::path::PathBuf> = self
            .projects
            .iter()
            .skip(1)
            .map(|p| p.path.clone())
            .collect();
        cx.background_spawn(async move { save_projects(paths) })
            .detach();
        self.show_project(id, cx);
    }

    pub fn toggle_project_pin(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
            p.pinned = !p.pinned;
        }
        cx.notify();
    }

    pub fn open_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(project) = self.chats.iter().find(|c| c.id == id).map(|c| c.project) {
            if project != self.current_project {
                self.select_project(project, cx);
            }
        }
        self.screen = Screen::Chat;
        if self.current != Some(id) {
            self.doc = None;
            self.find = None;
        }
        self.current = Some(id);
        self.sync_mode();
        if let Some(c) = self.chat_mut(id) {
            c.unread = false;
        }
        cx.notify();
    }

    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Chat;
        self.current = None;
        self.doc = None;
        self.find = None;
        self.greeting_seed = self.greeting_seed.wrapping_add(1);
        self.sync_mode();
        cx.notify();
    }

    /// The mode for the chat now showing: what it kept, else the setting's.
    fn sync_mode(&mut self) {
        self.mode = self
            .current
            .and_then(|id| self.kept_modes.get(&id).copied())
            .unwrap_or_else(|| crate::modes::initial(self.default_mode, self.bypass_allowed));
    }

    fn keep_mode(&mut self) {
        let Some(id) = self.current else { return };
        if self.mode.kept(self.bypass_allowed) {
            self.kept_modes.insert(id, self.mode);
        } else {
            self.kept_modes.remove(&id);
        }
    }

    /// Pick a mode. Bypass, not yet allowed, shows its warning instead.
    pub fn set_mode(&mut self, mode: ModeId, cx: &mut Context<Self>) {
        if mode == ModeId::Bypass && !self.bypass_allowed {
            self.modes_open = true;
            self.bypass_warning = true;
        } else {
            self.mode = mode;
            self.modes_open = false;
            self.bypass_warning = false;
            self.keep_mode();
            if let Some(live) = self.current.and_then(|id| self.sessions.get(&id)) {
                let _ = live.session.set_mode(mode);
            }
        }
        cx.notify();
    }

    /// Shift+Tab.
    pub fn cycle_mode(&mut self, cx: &mut Context<Self>) {
        let next = crate::modes::next(self.mode, self.bypass_allowed);
        self.set_mode(next, cx);
    }

    /// The Bypass warning's "Allow": from now on Bypass is offered.
    pub fn allow_bypass(&mut self, cx: &mut Context<Self>) {
        self.bypass_allowed = true;
        self.set_mode(ModeId::Bypass, cx);
    }

    pub fn toggle_pin(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(c) = self.chat_mut(id) {
            c.pinned = !c.pinned;
        }
        self.mark_dirty(id, cx);
        cx.notify();
    }

    /// Save the name from the first-run screen. Returns an error to show.
    pub fn set_user_name(
        &mut self,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        let name = crate::profile::clean(name).ok_or("Type a name first.")?;
        if crate::profile::save_name(&name).is_err() {
            // Still usable for this run; the next launch asks again.
            self.show_toast(
                "Your name could not be saved; Forge will ask again next time.",
                cx,
            );
        }
        self.user_name = Some(name.into());
        cx.notify();
        Ok(())
    }

    /// Pick an avatar (the first-run screen, or "Change your avatar").
    pub fn set_avatar(&mut self, name: &str, cx: &mut Context<Self>) {
        if !crate::avatars::is_known(name) {
            return;
        }
        self.avatar = name.to_string().into();
        self.save_avatar(cx);
        cx.notify();
    }

    fn save_avatar(&mut self, cx: &mut Context<Self>) {
        self.avatar_saved = true;
        let name = self.avatar.to_string();
        cx.background_spawn(async move {
            let _ = crate::profile::save_avatar(&name);
        })
        .detach();
    }

    /// The first-run screen is done. A first run ends on a new chat, greeting
    /// you; changing your name or avatar returns to where you were.
    pub fn finish_onboarding(&mut self, cx: &mut Context<Self>) {
        self.onboarding = false;
        if std::mem::take(&mut self.first_run) {
            self.new_chat(cx);
        }
        cx.notify();
    }

    /// Run the reply to one of your messages again: the conversation goes
    /// back to that message, and Forge answers it afresh.
    pub fn retry(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        let Some(text) = self.take_from(chat, msg, cx) else {
            return;
        };
        self.send(text, cx);
    }

    /// Edit one of your messages: the conversation goes back to before it,
    /// and its text waits in the composer to be changed and sent.
    pub fn edit(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        if let Some(text) = self.take_from(chat, msg, cx) {
            self.draft = Some(text);
            cx.notify();
        }
    }

    /// Cut the chat back to before message `msg` (one of yours), returning
    /// its text. Stops a reply in progress first.
    fn take_from(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) -> Option<String> {
        self.replies.remove(&chat);
        // Forge's conversation can't be cut back: the next message starts a new one.
        self.sessions.remove(&chat);
        while self.side.get(&chat).is_some_and(|s| !s.pending.is_empty()) {
            self.answer_side(
                chat,
                "The conversation was restarted before it answered.".into(),
                true,
            );
        }
        self.asks.remove(&chat);
        self.stale_turns.remove(&chat);
        self.turn_started.remove(&chat);
        self.tool_rows.retain(|(c, _), _| *c != chat);
        // Ratings of the replies being cut go with them.
        self.ratings.retain(|(c, m), _| *c != chat || *m < msg);
        let c = self.chat_mut(chat)?;
        let Some(Message::User { text, .. }) = c.messages.get(msg) else {
            return None;
        };
        let text = text.to_string();
        // What is cut is kept as a version, to view or restore.
        c.versions.push(Version {
            at: msg,
            tail: c.messages[msg..].to_vec(),
            session: c.session.clone(),
            resumable: c.resumable,
            when: SystemTime::now(),
        });
        let over = c.versions.len().saturating_sub(MAX_VERSIONS);
        c.versions.drain(..over);
        c.messages.truncate(msg);
        c.version += 1;
        c.new_conversation();
        self.touch(chat, cx);
        cx.notify();
        Some(text)
    }

    pub fn mark_copied(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.copied = Some(key);
        self.copied_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |s, cx| {
                s.copied = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The endpoint setup guide, in the side panel.
    pub fn open_settings(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.settings_open = true;
        self.picker_open = false;
        self.modes_open = false;
        self.palette_open = false;
        self.settings_tab(tab, cx);
    }

    /// Show a Settings section; an editor of ForgeCLI's files reads them
    /// and starts watching them.
    pub fn settings_tab(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.settings_tab = tab;
        if tab != SettingsTab::Models {
            self.diagnosis = None;
        }
        if tab.reads_engine_files() {
            self.reload_engine(cx);
            self.watch_engine(cx);
        }
        cx.notify();
    }

    /// Read ForgeCLI's files again (off the UI thread).
    pub fn reload_engine(&mut self, cx: &mut Context<Self>) {
        let project = self.project_dir.clone();
        if self.engine.project.as_ref() != Some(&project) {
            self.engine.catalog = None;
            self.engine.mcp_status.clear();
            self.engine.mcp_error = None;
        }
        self.engine.loading = true;
        cx.spawn(async move |this, cx| {
            let (catalog, sig, profiles) = cx
                .background_spawn({
                    let project = project.clone();
                    async move {
                        let sig = crate::forge_files::signature(&project)
                            ^ crate::setting_profiles::stamp();
                        (
                            crate::forge_files::load(&project),
                            sig,
                            crate::setting_profiles::list(),
                        )
                    }
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                // The / list reads the project's commands again.
                s.custom_commands = None;
                // A project switched mid-read: that read is stale.
                if s.project_dir == project {
                    s.engine.catalog = Some(catalog);
                    s.engine.profiles = profiles;
                    s.engine.signature = sig;
                    s.engine.project = Some(project);
                    s.engine.loading = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// While Settings shows an editor of ForgeCLI's files, look at them every
    /// two seconds and read them again when one changed (an edit in another
    /// program, a `git pull`).
    fn watch_engine(&mut self, cx: &mut Context<Self>) {
        if self.engine.watch.is_some() {
            return;
        }
        self.engine.watch = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            let Ok(Some(project)) = this.update(cx, |s, _| {
                (s.settings_open && s.settings_tab.reads_engine_files())
                    .then(|| s.project_dir.clone())
            }) else {
                let _ = this.update(cx, |s, _| s.engine.watch = None);
                return;
            };
            let sig = cx
                .background_spawn(async move {
                    crate::forge_files::signature(&project) ^ crate::setting_profiles::stamp()
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if sig != s.engine.signature && !s.engine.loading {
                    s.reload_engine(cx);
                }
            });
        }));
    }

    /// Open or close the project map; opening reads the project again.
    pub fn toggle_map(&mut self, cx: &mut Context<Self>) {
        self.map_open = !self.map_open;
        self.palette_open = false;
        if self.map_open {
            let project = self.project_dir.clone();
            if self.map.as_ref().is_some_and(|m| m.project != project) {
                self.map = None;
            }
            cx.spawn(async move |this, cx| {
                let map = cx
                    .background_spawn({
                        let project = project.clone();
                        async move {
                            let files = crate::rules::project_files(&project, MAP_CAP);
                            ProjectMap::from_files(project, &files)
                        }
                    })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    if s.project_dir == map.project {
                        s.map = Some(map);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// The files the current chat changed, most changed first, as
    /// project-relative paths where they're inside the project.
    pub fn edited_files(&self) -> Vec<Edited> {
        let Some(chat) = self.chat() else {
            return vec![];
        };
        let mut out: Vec<Edited> = vec![];
        for m in &chat.messages {
            let Message::Assistant { blocks } = m else {
                continue;
            };
            for b in blocks {
                let Block::Tool(t) = b else { continue };
                if !matches!(t.kind, ToolKind::Edit | ToolKind::Write)
                    || t.status != ToolStatus::Done
                {
                    continue;
                }
                let path = std::path::Path::new(t.target.as_ref());
                let shown = path
                    .strip_prefix(&self.project_dir)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| t.target.to_string());
                match out.iter_mut().find(|e| e.path == shown) {
                    Some(e) => {
                        e.times += 1;
                        e.review = t.review;
                    }
                    None => out.push(Edited {
                        path: shown,
                        times: 1,
                        review: t.review,
                    }),
                }
            }
        }
        out.sort_by(|a, b| b.times.cmp(&a.times).then(a.path.cmp(&b.path)));
        out
    }

    /// Diagnose an endpoint: DNS, TCP, TLS, HTTP, then the key, off the UI
    /// thread, each step shown as it ends.
    pub fn diagnose(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(profile) = self
            .endpoints
            .as_ref()
            .and_then(|e| e.profiles.iter().find(|p| p.name == name).cloned())
        else {
            return;
        };
        self.diagnosis_seq += 1;
        let run = self.diagnosis_seq;
        self.diagnosis = Some(Diagnosis {
            profile: name.to_string(),
            steps: crate::diagnose::waiting(),
            running: true,
            run,
        });
        let (tx, rx) = async_channel::unbounded::<(usize, crate::diagnose::Step)>();
        cx.background_spawn(async move {
            let key = crate::health::token(&profile.auth).ok().flatten();
            crate::diagnose::run(&profile, key, |i, step| {
                let _ = tx.try_send((i, step));
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            while let Ok((i, step)) = rx.recv().await {
                let alive = this.update(cx, |s, cx| {
                    if let Some(d) = s.diagnosis.as_mut().filter(|d| d.run == run) {
                        d.steps[i] = step;
                        cx.notify();
                        true
                    } else {
                        false
                    }
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
            let _ = this.update(cx, |s, cx| {
                if let Some(d) = s.diagnosis.as_mut().filter(|d| d.run == run) {
                    d.running = false;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Ask ForgeCLI which MCP servers start (`forge mcp list`, in the project).
    pub fn check_mcp(&mut self, cx: &mut Context<Self>) {
        let Some(cli) = crate::backend::find_cli() else {
            self.engine.mcp_error = Some(
                "ForgeCLI isn't installed beside Forge, so the servers can't be checked.".into(),
            );
            return cx.notify();
        };
        let project = self.project_dir.clone();
        self.engine.mcp_checking = true;
        self.engine.mcp_error = None;
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_spawn(async move {
                    let mut cmd = std::process::Command::new(&cli);
                    cmd.args(["mcp", "list"])
                        .current_dir(&project)
                        .env("NO_COLOR", "1")
                        .stdin(std::process::Stdio::null());
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        cmd.creation_flags(0x0800_0000);
                    }
                    cmd.output()
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.engine.mcp_checking = false;
                match out {
                    Ok(o) => {
                        let text = String::from_utf8_lossy(&o.stdout);
                        s.engine.mcp_status = crate::forge_files::parse_mcp_list(&text);
                        if s.engine.mcp_status.is_empty() && !o.status.success() {
                            let err = String::from_utf8_lossy(&o.stderr);
                            s.engine.mcp_error = Some(
                                err.lines()
                                    .last()
                                    .unwrap_or("forge mcp list failed")
                                    .to_string(),
                            );
                        }
                    }
                    Err(e) => s.engine.mcp_error = Some(format!("Couldn't run ForgeCLI: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Run a change to ForgeCLI's files off the UI thread, then read them
    /// again and say how it went.
    pub fn engine_write(
        &mut self,
        done: impl Into<String>,
        f: impl FnOnce(&std::path::Path) -> Result<(), String> + Send + 'static,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let project = self.project_dir.clone();
        let done = done.into();
        cx.spawn(async move |this, cx| {
            let r = cx.background_spawn(async move { f(&project) }).await;
            let _ = this.update(cx, |s, cx| {
                match &r {
                    Ok(()) => s.show_toast(done, cx),
                    Err(e) => s.show_toast(e.clone(), cx),
                }
                s.reload_engine(cx);
            });
            r
        })
    }

    /// Settings > Models, on the Add endpoint form.
    pub fn add_endpoint(&mut self, cx: &mut Context<Self>) {
        self.settings_add = true;
        self.open_settings(SettingsTab::Models, cx);
    }

    pub fn show_endpoint_guide(&mut self, cx: &mut Context<Self>) {
        self.picker_open = false;
        self.palette_open = false;
        if !matches!(self.screen, Screen::Chat) {
            self.screen = Screen::Chat;
        }
        self.doc = Some(FileRef {
            name: "endpoint-setup.md".into(),
            path: None,
            content: crate::endpoints::GUIDE.into(),
            bytes: None,
        });
        cx.notify();
    }

    /// Open endpoints.json in your editor, writing a starter file first if
    /// there is none.
    pub fn open_endpoints_file(&mut self, cx: &mut Context<Self>) {
        self.picker_open = false;
        self.palette_open = false;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let file = cx
                .background_spawn(async { crate::endpoints::ensure_file() })
                .await;
            let _ = this.update(cx, |s, cx| match file {
                Ok(path) => {
                    cx.open_with_system(&path);
                    s.show_toast(
                        "Opened endpoints.json. Save it, then press refresh in the model menu.",
                        cx,
                    );
                }
                Err(e) => s.show_toast(format!("Could not open endpoints.json: {e}"), cx),
            });
        })
        .detach();
    }

    /// Open a file in the side panel (the same file again closes it). An
    /// Office file or PDF is read off the UI thread; the panel says so meanwhile.
    pub fn open_doc(&mut self, file: FileRef, cx: &mut Context<Self>) {
        if self.doc.as_ref() == Some(&file) {
            self.doc = None;
            cx.notify();
            return;
        }
        self.doc_sheet = 0;
        if let (Some(_), Some(bytes)) = (file.office(), file.bytes.clone()) {
            let ready = self
                .doc_preview
                .as_ref()
                .is_some_and(|(n, _)| *n == file.name);
            if !ready {
                self.doc_preview = None;
                let name = file.name.clone();
                cx.spawn(async move |this, cx| {
                    let n = name.to_string();
                    let read = cx
                        .background_spawn(async move {
                            crate::office::preview(&n, &bytes).map(std::sync::Arc::new)
                        })
                        .await;
                    let _ = this.update(cx, |s, cx| {
                        s.doc_preview = Some((name, read));
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        self.doc = Some(file);
        cx.notify();
    }

    pub fn select_sheet(&mut self, i: usize, cx: &mut Context<Self>) {
        self.doc_sheet = i;
        cx.notify();
    }

    pub fn toggle_doc_fit(&mut self, cx: &mut Context<Self>) {
        self.doc_fit = !self.doc_fit;
        cx.notify();
    }

    /// Open a file in the app the system has for it: a diagram's page in the
    /// browser (where it is interactive), a workbook in Excel. Written to
    /// Forge's temp folder off the UI thread, then handed to the system.
    pub fn open_externally(&mut self, file: &FileRef, cx: &mut Context<Self>) {
        let dir = std::env::temp_dir().join("forge-files");
        let (name, bytes) = (crate::downloads::clean_name(&file.name), file.data());
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_spawn(async move {
                    std::fs::create_dir_all(&dir)?;
                    let path = dir.join(name);
                    std::fs::write(&path, bytes).map(|_| path)
                })
                .await;
            let _ = this.update(cx, |s, cx| match written {
                Ok(path) => cx.open_with_system(&path),
                Err(e) => s.show_toast(format!("Could not open it: {e}"), cx),
            });
        })
        .detach();
    }

    /// A plan, or the browser starting, shows in the side panel when it
    /// isn't showing a file and the chat is the one on screen.
    fn panel_follows(&mut self, chat: usize, tool: &str) {
        let tab = if tool == "ExitPlanMode" {
            Some(PanelTab::Plan)
        } else if tool.starts_with(crate::browser::TOOL_PREFIX) && self.panel.is_none() {
            Some(PanelTab::Browser)
        } else {
            None
        };
        if let Some(tab) = tab.filter(|_| self.doc.is_none() && self.current == Some(chat)) {
            self.panel = Some(tab);
        }
    }

    /// Open or close an agent's card.
    pub fn toggle_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.agents_open.remove(id) {
            self.agents_open.insert(id.to_string());
        }
        cx.notify();
    }

    /// What the engine says about sub-agents: one starting (a `Task` or
    /// `Agent` call), its steps, its words, its end.
    fn on_agent_event(&mut self, chat: usize, ev: &crate::backend::Event) {
        use crate::backend::Event;
        let now = std::time::Instant::now();
        let runs = self.agents.entry(chat).or_default();
        match ev {
            Event::ToolUse { id, name, input } if is_agent_tool(name) => {
                let s = |k: &str| input[k].as_str().unwrap_or_default().to_string();
                let kind = s("subagent_type");
                runs.push(AgentRun {
                    id: id.clone(),
                    kind: if kind.is_empty() {
                        "agent".into()
                    } else {
                        kind
                    },
                    task: {
                        let d = s("description");
                        if d.is_empty() {
                            "A sub-task".into()
                        } else {
                            d
                        }
                    },
                    prompt: s("prompt"),
                    started: now,
                    ended: None,
                    failed: false,
                    steps: vec![],
                    said: String::new(),
                });
                if self.doc.is_none() && self.current == Some(chat) {
                    self.panel = Some(PanelTab::Agents);
                }
            }
            Event::ToolResult {
                id,
                output,
                is_error,
            } => {
                if let Some(r) = runs.iter_mut().find(|r| &r.id == id) {
                    r.ended = Some(now);
                    r.failed = *is_error;
                    if !output.trim().is_empty() {
                        r.said = output.clone();
                    }
                    for s in r
                        .steps
                        .iter_mut()
                        .filter(|s| s.status == ToolStatus::Running)
                    {
                        s.status = ToolStatus::Done;
                    }
                }
            }
            Event::SubTool {
                parent,
                id,
                name,
                input,
            } => {
                if let Some(r) = runs.iter_mut().find(|r| &r.id == parent) {
                    let call = crate::backend::tool_call(name, input);
                    let (what, described) = match (&call.note, call.kind) {
                        (Some(n), ToolKind::Bash) => (n.to_string(), true),
                        _ => (call.shown_target().to_string(), false),
                    };
                    r.steps.push(AgentStep {
                        id: id.clone(),
                        kind: call.kind,
                        what,
                        described,
                        status: ToolStatus::Running,
                    });
                }
            }
            Event::SubResult {
                parent,
                id,
                is_error,
            } => {
                if let Some(s) = runs
                    .iter_mut()
                    .find(|r| &r.id == parent)
                    .and_then(|r| r.steps.iter_mut().find(|s| &s.id == id))
                {
                    s.status = if *is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Done
                    };
                }
            }
            Event::SubText { parent, text } => {
                if let Some(r) = runs.iter_mut().find(|r| &r.id == parent) {
                    r.said = text.clone();
                }
            }
            // A turn that ends with agents still going stopped them.
            Event::TurnEnd { .. } | Event::Exited { .. } => {
                for r in runs.iter_mut().filter(|r| r.ended.is_none()) {
                    r.ended = Some(now);
                    r.failed = true;
                }
            }
            _ => {}
        }
    }

    /// Show the side panel on `tab`, closing any file open in it. The
    /// browser starts when its tab first shows.
    pub fn show_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        self.doc = None;
        self.panel = Some(tab);
        if tab == PanelTab::Browser {
            self.ensure_web(cx);
        }
        cx.notify();
    }

    /// The panel's browser, started if it isn't running: its DevTools
    /// address, or None when there is no Chromium to run.
    pub fn ensure_web(&mut self, cx: &mut Context<Self>) -> Option<String> {
        if let Some(w) = &self.web {
            return Some(w.cdp_url.clone());
        }
        let Some(exe) = crate::web_view::find_chromium() else {
            self.web_page.error = Some(
                "Forge needs Chromium, Chrome or Edge for the browser. Install one, or run: npx playwright install chromium"
                    .into(),
            );
            cx.notify();
            return None;
        };
        let profile = crate::profile::dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("browser");
        match crate::web_view::start(&exe, &profile, &crate::browser::focus_file()) {
            Ok((view, updates)) => {
                let cdp = view.cdp_url.clone();
                self.web = Some(std::sync::Arc::new(view));
                self.web_page = WebPage {
                    loading: true,
                    ..WebPage::default()
                };
                cx.spawn(async move |this, cx| {
                    while let Ok(u) = updates.recv().await {
                        if this.update(cx, |s, cx| s.on_web(u, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                cx.notify();
                Some(cdp)
            }
            Err(e) => {
                self.web_page.error = Some(format!("The browser could not start: {e}"));
                cx.notify();
                None
            }
        }
    }

    fn on_web(&mut self, u: crate::web_view::Update, cx: &mut Context<Self>) {
        use crate::web_view::Update;
        match u {
            Update::Frame(img) => {
                // The last picture leaves the GPU's atlas as the next arrives.
                if let Some(old) = self.web_page.frame.replace(img) {
                    cx.drop_image(old, None);
                }
            }
            Update::Page { url, title } => {
                self.web_page.url = url;
                self.web_page.title = title;
            }
            Update::Tabs(tabs, on) => {
                self.web_page.tabs = tabs;
                self.web_page.active = on;
            }
            Update::Loading(l) => self.web_page.loading = l,
            Update::Closed(why) => {
                self.web = None;
                if let Some(old) = self.web_page.frame.take() {
                    cx.drop_image(old, None);
                }
                self.web_page = WebPage {
                    error: Some(format!("The browser stopped: {why}.")),
                    ..WebPage::default()
                };
            }
        }
        cx.notify();
    }

    /// Something for the panel's browser to do (starting it if need be).
    pub fn web_send(&mut self, cmd: crate::web_view::Cmd, cx: &mut Context<Self>) {
        if self.web.is_none() {
            self.web_page.error = None;
            self.ensure_web(cx);
        }
        if let Some(w) = &self.web {
            w.send(cmd);
        }
    }

    /// Open or close the side panel (on the plan when it opens).
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        if self.doc.is_some() || self.panel.is_some() {
            self.doc = None;
            self.panel = None;
        } else {
            self.panel = Some(PanelTab::Plan);
        }
        cx.notify();
    }

    pub fn close_panel(&mut self, cx: &mut Context<Self>) {
        self.doc = None;
        self.panel = None;
        cx.notify();
    }

    pub fn close_doc(&mut self, cx: &mut Context<Self>) {
        if self.doc.take().is_some() {
            cx.notify();
        }
    }

    /// Save a copy into Downloads, off the UI thread, and say where it went.
    pub fn download(&mut self, file: &FileRef, cx: &mut Context<Self>) {
        let Some(dir) = crate::downloads::dir() else {
            self.show_toast("Could not find your Downloads folder.", cx);
            return;
        };
        let (name, bytes) = (file.name.to_string(), file.data());
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move { crate::downloads::save(&dir, &name, &bytes) })
                .await;
            let _ = this.update(cx, |s, cx| match saved {
                Ok(path) => s.show_toast(
                    format!(
                        "Saved to Downloads as {}",
                        path.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ),
                    cx,
                ),
                Err(e) => s.show_toast(format!("Could not save it: {e}"), cx),
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ autocomplete

    /// Read what `/` and `@` complete from, for the project you're in, if
    /// it hasn't been read yet (off the UI thread).
    pub fn ensure_completions(&mut self, cx: &mut Context<Self>) {
        let dir = self.project_dir.clone();
        let fresh = self.file_index.as_ref().is_some_and(|(d, _)| *d == dir)
            && self
                .custom_commands
                .as_ref()
                .is_some_and(|(d, _)| *d == dir);
        if fresh || self.completions_loading {
            return;
        }
        self.completions_loading = true;
        cx.spawn(async move |this, cx| {
            let d = dir.clone();
            let (files, commands) = cx
                .background_spawn(async move {
                    (
                        crate::rules::project_entries(&d, 20_000),
                        crate::commands::custom(&d),
                    )
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.completions_loading = false;
                s.file_index = Some((dir.clone(), std::sync::Arc::new(files)));
                s.custom_commands = Some((dir, commands));
                cx.notify();
            });
        })
        .detach();
    }

    /// The commands `/` offers: the project's own, then ForgeCLI's.
    pub fn completion_commands(&self) -> Vec<CompletionCommand> {
        let custom = self
            .custom_commands
            .as_ref()
            .filter(|(d, _)| *d == self.project_dir)
            .map(|(_, c)| c.clone())
            .unwrap_or_default();
        custom
            .into_iter()
            .map(|c| CompletionCommand {
                name: c.name,
                args: c.args,
                description: c.description,
                group: "This project",
            })
            .chain(
                crate::commands::builtin()
                    .into_iter()
                    .map(|c| CompletionCommand {
                        name: c.name,
                        args: c.args,
                        description: c.description,
                        group: "ForgeCLI",
                    }),
            )
            .collect()
    }

    /// The files `@` offers.
    pub fn completion_files(&self) -> std::sync::Arc<Vec<String>> {
        self.file_index
            .as_ref()
            .filter(|(d, _)| *d == self.project_dir)
            .map(|(_, f)| f.clone())
            .unwrap_or_default()
    }

    /// "Run again" on a command: Forge is asked to run it again, as a
    /// message of yours, so the mode's permissions apply as for any command.
    pub fn rerun(&mut self, command: &str, cx: &mut Context<Self>) {
        if self.busy() {
            return self.show_toast("Wait for the reply to finish before running it again.", cx);
        }
        let fence = if command.contains("```") {
            "````"
        } else {
            "```"
        };
        self.send(
            format!(
                "Run this command again and tell me what changed:\n\n{fence}sh\n{command}\n{fence}"
            ),
            cx,
        );
    }

    // ------------------------------------------------------------ versions

    pub fn toggle_versions(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        self.versions_open = if self.versions_open == Some((chat, msg)) {
            None
        } else {
            Some((chat, msg))
        };
        if let Some(c) = self.chat_mut(chat) {
            c.version += 1;
        }
        cx.notify();
    }

    // ------------------------------------------------------------ jump, tasks, approvals

    /// Show message `msg` of the chat showing (the list scrolls to it).
    pub fn jump_to(&mut self, msg: usize, cx: &mut Context<Self>) {
        self.jump_seq += 1;
        self.jump = Some((self.jump_seq, msg));
        cx.notify();
    }

    pub fn toggle_tasks(&mut self, cx: &mut Context<Self>) {
        self.tasks_open = !self.tasks_open;
        cx.notify();
    }

    pub fn toggle_approvals(&mut self, cx: &mut Context<Self>) {
        self.approvals_open = !self.approvals_open && !self.pending_approvals().is_empty();
        cx.notify();
    }

    /// The chats waiting on a permission answer: (chat, its title, what it
    /// asks, where the question is), the one you're looking at first.
    pub fn pending_approvals(&self) -> Vec<(usize, SharedString, SharedString, usize)> {
        let mut out: Vec<_> = self
            .chats
            .iter()
            .filter_map(|c| {
                let (msg, title) =
                    c.messages
                        .iter()
                        .enumerate()
                        .rev()
                        .find_map(|(i, m)| match m {
                            Message::Assistant { blocks } => blocks.iter().find_map(|b| match b {
                                Block::Permission {
                                    title,
                                    verdict: Verdict::Pending,
                                    ..
                                } => Some((i, title.clone())),
                                _ => None,
                            }),
                            _ => None,
                        })?;
                // Only a question a running reply still waits on.
                self.replies
                    .contains_key(&c.id)
                    .then(|| (c.id, c.title.clone(), title, msg))
            })
            .collect();
        out.sort_by_key(|(id, ..)| Some(*id) != self.current);
        out
    }

    /// Open a chat at the question it waits on.
    pub fn reveal_approval(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        self.approvals_open = false;
        self.open_chat(chat, cx);
        self.jump_to(msg, cx);
    }

    // ------------------------------------------------------------ JSON export and share

    /// The chat as JSON: its whole transcript, tool calls included (the
    /// history's format, so Forge can read it back).
    fn chat_json(&self, id: usize) -> Option<(String, String)> {
        let c = self.chats.iter().find(|c| c.id == id)?;
        let dir = self
            .project(c.project)
            .map(|p| p.path.clone())
            .unwrap_or_else(|| self.project_dir.clone());
        let json = serde_json::to_string_pretty(&crate::history::save_form(c, &dir)).ok()?;
        Some((
            crate::export::file_name(&c.title).replace(".md", ".json"),
            json,
        ))
    }

    /// Write a chat to Downloads as JSON, off the UI thread.
    pub fn export_chat_json(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some((name, json)) = self.chat_json(id) else {
            return;
        };
        let Some(dir) = crate::downloads::dir() else {
            return self.show_toast("Could not find your Downloads folder.", cx);
        };
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(
                    async move { crate::downloads::save(&dir, &name, json.as_bytes()) },
                )
                .await;
            let _ = this.update(cx, |s, cx| match saved {
                Ok(path) => s.show_toast(format!("Exported to {}", path.display()), cx),
                Err(e) => s.show_toast(format!("Could not export it: {e}"), cx),
            });
        })
        .detach();
        cx.notify();
    }

    /// Share: the JSON goes to the clipboard now; a link waits on upload (a stub).
    pub fn share_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some((_, json)) = self.chat_json(id) else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
        self.stub(crate::stubs::SHARE_LINK, cx);
    }

    /// Bring back version `v`: the conversation from its message on becomes
    /// what it was then (the one showing now is kept as a version in turn),
    /// and the next message resumes that version's ForgeCLI conversation.
    pub fn restore_version(&mut self, chat: usize, v: usize, cx: &mut Context<Self>) {
        self.versions_open = None;
        if self.replies.contains_key(&chat) {
            return self.show_toast(
                "Wait for the reply to finish, or stop it, before restoring.",
                cx,
            );
        }
        // The running process holds the other version's conversation.
        self.sessions.remove(&chat);
        self.asks.remove(&chat);
        self.tool_rows.retain(|(c, _), _| *c != chat);
        self.ratings.retain(|(c, _), _| *c != chat);
        let Some(c) = self.chat_mut(chat) else { return };
        if v >= c.versions.len() {
            return;
        }
        let old = c.versions.remove(v);
        let at = old.at.min(c.messages.len());
        let now = Version {
            at,
            tail: c.messages[at..].to_vec(),
            session: c.session.clone(),
            resumable: c.resumable,
            when: SystemTime::now(),
        };
        if !now.tail.is_empty() {
            c.versions.push(now);
        }
        c.messages.truncate(at);
        c.messages.extend(old.tail);
        c.session = old.session;
        c.resumable = old.resumable;
        c.version += 1;
        self.touch(chat, cx);
        self.show_toast("Restored that version.", cx);
        cx.notify();
    }

    // ------------------------------------------------------------ context

    /// Summarize the conversation to free context: ForgeCLI's `/compact`,
    /// shown as a notice rather than a message of yours.
    pub fn compact(&mut self, cx: &mut Context<Self>) {
        self.context_open = false;
        let Some(id) = self.current else { return };
        if demo_mode() {
            return self.stub(crate::stubs::COMPACT_DEMO, cx);
        }
        if self.busy() {
            return self.show_toast("Wait for the reply to finish before compacting.", cx);
        }
        let Some(profile) = self.active_profile().cloned() else {
            return self.show_toast(
                "Set up an endpoint first: compacting runs on the model.",
                cx,
            );
        };
        if !self.sessions.contains_key(&id) {
            if let Err(e) = self.start_session(id, profile, cx) {
                return self.show_toast(format!("Couldn't start Forge to compact: {e}"), cx);
            }
        }
        match self.sessions.get(&id).map(|l| l.session.send("/compact")) {
            Some(Ok(())) => {
                self.notice(id, COMPACTING, false);
                self.compacting.insert(id);
                self.replies.insert(id, Task::ready(()));
                self.turn_started.insert(id, std::time::Instant::now());
            }
            Some(Err(e)) => self.show_toast(format!("Couldn't compact: {e}"), cx),
            None => {}
        }
        cx.notify();
    }

    pub fn toggle_context(&mut self, cx: &mut Context<Self>) {
        self.context_open = !self.context_open && self.chat().is_some();
        self.picker_open = false;
        self.modes_open = false;
        cx.notify();
    }

    // ------------------------------------------------------------ attachments

    /// Attach files (the picker, a drop, a paste of copied files). Each
    /// shows as a chip at once, "Preparing…", while it is read, copied into
    /// the project and, for an archive, opened (`attach::prepare`, off the
    /// UI thread); a message can't go until they are ready.
    pub fn add_attachment_paths(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let room = crate::attach::MAX_FILES.saturating_sub(self.attachments.len());
        if room == 0 {
            return self.show_toast(
                format!("A message takes up to {} files.", crate::attach::MAX_FILES),
                cx,
            );
        }
        let skipped = paths.len().saturating_sub(room);
        let paths: Vec<_> = paths
            .into_iter()
            .filter(|p| !self.attachments.iter().any(|a| a.path.as_ref() == Some(p)))
            .take(room)
            .collect();
        if skipped > 0 {
            self.show_toast(
                format!(
                    "Attached the first files; a message takes up to {}.",
                    crate::attach::MAX_FILES
                ),
                cx,
            );
        }
        let project = self.project_dir.clone();
        for path in paths {
            self.attachment_seq += 1;
            let id = self.attachment_seq;
            self.attachments.push(crate::attach::Attachment {
                id,
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
                    .into(),
                size: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                path: Some(path.clone()),
                pending: true,
                ..Default::default()
            });
            let project = project.clone();
            cx.spawn(async move |this, cx| {
                let made = cx
                    .background_spawn(async move { crate::attach::prepare(&path, &project, id) })
                    .await;
                let _ = this.update(cx, |s, cx| s.attachment_ready(id, made, cx));
            })
            .detach();
        }
        cx.notify();
    }

    /// A prepared attachment takes its chip's place; if the chip was removed
    /// meanwhile, what was copied into the project goes too.
    fn attachment_ready(
        &mut self,
        id: u64,
        made: Result<crate::attach::Attachment, String>,
        cx: &mut Context<Self>,
    ) {
        let slot = self.attachments.iter().position(|a| a.id == id);
        match (made, slot) {
            (Ok(a), Some(i)) => {
                if let Some(n) = a.note.as_deref().filter(|_| {
                    matches!(
                        a.kind,
                        crate::attach::Kind::Archive | crate::attach::Kind::File
                    )
                }) {
                    self.show_toast(format!("{}: {n}", a.name), cx);
                }
                self.attachments[i] = a;
            }
            (Ok(a), None) => {
                if let Some(d) = a.staged_dir {
                    cx.background_spawn(async move { crate::stage::discard(&d) })
                        .detach();
                }
            }
            (Err(e), slot) => {
                if let Some(i) = slot {
                    self.attachments.remove(i);
                }
                self.show_toast(format!("Couldn't attach {e}"), cx);
            }
        }
        cx.notify();
    }

    /// An image pasted from the clipboard. It is scaled down if big; one
    /// that can't be decoded is saved into the project and goes by path.
    pub fn add_pasted_image(&mut self, bytes: Vec<u8>, ext: &str, cx: &mut Context<Self>) {
        if self.attachments.len() >= crate::attach::MAX_FILES {
            return self.show_toast(
                format!("A message takes up to {} files.", crate::attach::MAX_FILES),
                cx,
            );
        }
        self.pasted += 1;
        self.attachment_seq += 1;
        let (n, id) = (self.pasted, self.attachment_seq);
        self.attachments.push(crate::attach::Attachment {
            id,
            name: format!("Pasted image {n}.{ext}").into(),
            size: bytes.len() as u64,
            pending: true,
            ..Default::default()
        });
        let project = self.project_dir.clone();
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(
                    async move { crate::attach::pasted_image(bytes, n, &project, id) },
                )
                .await;
            let _ = this.update(cx, |s, cx| s.attachment_ready(id, made, cx));
        })
        .detach();
        cx.notify();
    }

    /// Remove a chip; what it copied into the project goes with it.
    pub fn remove_attachment(&mut self, i: usize, cx: &mut Context<Self>) {
        if i < self.attachments.len() {
            let a = self.attachments.remove(i);
            if let Some(d) = a.staged_dir {
                cx.background_spawn(async move { crate::stage::discard(&d) })
                    .detach();
            }
            cx.notify();
        }
    }

    /// Turn the web browser on or off. Chats that aren't replying restart
    /// their ForgeCLI on their next message (resuming the conversation), so
    /// the change reaches them; a running reply keeps what it started with.
    /// Idle chats' ForgeCLI processes end, so each starts again (resuming its
    /// conversation) with the new arguments on its next message.
    fn restart_idle_engines(&mut self) {
        let working: Vec<usize> = self
            .chats
            .iter()
            .filter(|c| self.status(c) == ChatStatus::Working || !c.resumable)
            .map(|c| c.id)
            .collect();
        self.sessions.retain(|id, _| working.contains(id));
    }

    /// Use a settings profile (None: none, just the settings files).
    pub fn use_settings_profile(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        if let Err(e) = crate::setting_profiles::set_active(name.as_deref()) {
            return self.show_toast(format!("Couldn't save the choice: {e}"), cx);
        }
        self.settings_profile = name.clone();
        self.restart_idle_engines();
        self.show_toast(
            match name {
                Some(n) => {
                    format!("Using the {n} profile. Chats pick it up on their next message.")
                }
                None => "No profile: chats use your settings files alone.".into(),
            },
            cx,
        );
        cx.notify();
    }

    pub fn toggle_browser(&mut self, cx: &mut Context<Self>) {
        let on = !self.browser;
        if on && crate::web_view::find_chromium().is_none() {
            return self.show_toast(
                "Forge's browser needs Chromium, Chrome or Edge. Install one (or run: npx playwright install chromium), then turn it on again.",
                cx,
            );
        }
        if let Err(e) = crate::browser::set_enabled(on) {
            return self.show_toast(format!("Couldn't save the setting: {e}"), cx);
        }
        self.browser = on;
        self.restart_idle_engines();
        self.show_toast(
            if on {
                "Web browser on: Forge's agents drive the browser in the side panel (open, click, type, read, run scripts, screenshots) while you watch. Each step asks first."
            } else {
                "Web browser off."
            },
            cx,
        );
        cx.notify();
    }

    /// Show a diagram's source, or its drawing again.
    pub fn toggle_diagram_code(&mut self, key: SharedString, cx: &mut Context<Self>) {
        let set = std::sync::Arc::make_mut(&mut self.diagram_code);
        if !set.remove(&key) {
            set.insert(key);
        }
        if let Some(id) = self.current {
            if let Some(c) = self.chat_mut(id) {
                c.version += 1;
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------ keep / revert a turn's changes

    /// Keep a file change: its Keep / Revert buttons go.
    pub fn keep_change(&mut self, chat: usize, msg: usize, block: usize, cx: &mut Context<Self>) {
        self.update_tool(chat, msg, block, |t| t.review = Review::Kept);
        self.mark_dirty(chat, cx);
        cx.notify();
    }

    /// Revert the changes made since your message before `msg`: ForgeCLI's
    /// checkpoints say which files that touches (a dry run), and the confirm
    /// window lists them before anything is put back.
    pub fn ask_revert(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        if demo_mode() {
            return self.stub(crate::stubs::REVERT_DEMO, cx);
        }
        let Some(c) = self.chats.iter().find(|c| c.id == chat) else {
            return;
        };
        let Some(q) = turn_question(&c.messages, msg) else {
            return;
        };
        let Some(Message::User {
            turn: Some(turn), ..
        }) = c.messages.get(q)
        else {
            // A message from before Forge learnt turn ids (or one never accepted).
            return self.stub(crate::stubs::REVERT_NO_CHECKPOINT, cx);
        };
        let turn = turn.to_string();
        if self.replies.contains_key(&chat) {
            return self.show_toast(
                "Wait for the reply to finish, or stop it, before reverting.",
                cx,
            );
        }
        if let Err(e) = self.control(
            chat,
            json_rewind(&turn, true),
            Request::RevertPlan { chat, msg: q },
            cx,
        ) {
            self.show_toast(format!("Couldn't ask Forge what to revert: {e}"), cx);
        }
    }

    /// The confirm window's Revert.
    pub fn revert(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        self.confirm = None;
        let Some(Message::User {
            turn: Some(turn), ..
        }) = self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| c.messages.get(msg))
        else {
            return;
        };
        let turn = turn.to_string();
        if let Err(e) = self.control(
            chat,
            json_rewind(&turn, false),
            Request::Revert { chat, msg },
            cx,
        ) {
            self.show_toast(format!("Couldn't revert: {e}"), cx);
        }
        cx.notify();
    }

    /// Send a control request to a chat's ForgeCLI (starting it, resumed,
    /// if it isn't running), remembering what the answer is for.
    fn control(
        &mut self,
        chat: usize,
        request: serde_json::Value,
        what: Request,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.sessions.contains_key(&chat) {
            let profile = self
                .active_profile()
                .cloned()
                .ok_or("set up an endpoint first")?;
            self.start_session(chat, profile, cx)?;
        }
        let id = self
            .sessions
            .get(&chat)
            .ok_or("Forge isn't running")?
            .session
            .request(request)?;
        self.requests.insert(id, what);
        Ok(())
    }

    fn on_reply(
        &mut self,
        req: Request,
        result: Result<serde_json::Value, String>,
        cx: &mut Context<Self>,
    ) {
        let files = |v: &serde_json::Value| -> Result<Vec<String>, String> {
            if v["canRewind"] == false {
                return Err(v["error"]
                    .as_str()
                    .unwrap_or("Forge has no checkpoint for this message")
                    .to_string());
            }
            Ok(v["filesChanged"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str().map(str::to_string))
                .collect())
        };
        match (req, result.and_then(|v| files(&v))) {
            (Request::RevertPlan { .. }, Ok(f)) if f.is_empty() => self.show_toast(
                "Nothing to revert: no files changed since this message.",
                cx,
            ),
            (Request::RevertPlan { chat, msg }, Ok(files)) => {
                self.confirm = Some(Confirm::Revert { chat, msg, files });
                cx.notify();
            }
            (Request::Revert { chat, msg }, Ok(files)) => {
                // Every file change from that message on, to a file put back, reads "Reverted".
                if let Some(c) = self.chat_mut(chat) {
                    for m in c.messages.iter_mut().skip(msg) {
                        if let Message::Assistant { blocks } = m {
                            for b in blocks.iter_mut() {
                                if let Block::Tool(t) = b {
                                    if matches!(t.kind, ToolKind::Edit | ToolKind::Write)
                                        && files.iter().any(|f| same_file(f, &t.target))
                                    {
                                        t.review = Review::Reverted;
                                    }
                                }
                            }
                        }
                    }
                    c.version += 1;
                }
                let n = files.len();
                self.notice(
                    chat,
                    format!(
                        "Reverted {n} {} to how {} before this message.",
                        if n == 1 { "file" } else { "files" },
                        if n == 1 { "it was" } else { "they were" }
                    ),
                    false,
                );
                self.touch(chat, cx);
            }
            (_, Err(e)) => self.show_toast(format!("Couldn't revert: {e}"), cx),
        }
        cx.notify();
    }

    /// A control with nothing behind it yet says so, in the registry's words.
    pub fn stub(&mut self, id: &str, cx: &mut Context<Self>) {
        self.show_toast(crate::stubs::toast(id), cx);
    }

    // ------------------------------------------------------------ replies: copy, regenerate, continue, rate

    /// Copy the prose of the reply that ends at `end`.
    pub fn copy_turn(&mut self, chat: usize, end: usize, cx: &mut Context<Self>) {
        let Some(c) = self.chats.iter().find(|c| c.id == chat) else {
            return;
        };
        let text = turn_text(&c.messages, end);
        if text.is_empty() {
            return self.show_toast("This reply has no text to copy.", cx);
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.mark_copied(format!("turn-{chat}-{end}").into(), cx);
    }

    /// Answer the question behind the reply ending at `end` again.
    pub fn regenerate(&mut self, chat: usize, end: usize, cx: &mut Context<Self>) {
        let Some(q) = self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| turn_question(&c.messages, end))
        else {
            return;
        };
        self.retry(chat, q, cx);
    }

    /// The newest reply of the chat showing, answered again.
    pub fn regenerate_last(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.chat() else { return };
        let (id, end) = (c.id, c.messages.len().saturating_sub(1));
        if c.messages.is_empty() || self.busy() {
            return;
        }
        self.regenerate(id, end, cx);
    }

    /// Pick a stopped reply back up. The chat's ForgeCLI process still holds
    /// the conversation after Stop, so Forge is asked to go on; when the
    /// process is gone the conversation can't be resumed (a stub until P2).
    pub fn continue_reply(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current else { return };
        if self.busy() {
            return;
        }
        // The live process still holds the conversation, and one ForgeCLI
        // wrote can be resumed (`--resume`).
        let resumable = self.chats.iter().any(|c| c.id == id && c.resumable);
        if demo_mode() || self.sessions.contains_key(&id) || resumable {
            self.send(CONTINUE_PROMPT.to_string(), cx);
        } else {
            self.stub(crate::stubs::CONTINUE_NO_SESSION, cx);
        }
    }

    /// Whether the chat showing ended on Stop (its last row offers Continue).
    pub fn can_continue(&self) -> bool {
        !self.busy() && self.chat().is_some_and(|c| matches!(c.messages.last(), Some(Message::Notice { text, error: false }) if text.as_ref() == INTERRUPTED))
    }

    /// Rate a reply; the same rating again takes it back.
    pub fn rate(&mut self, chat: usize, end: usize, rating: Rating, cx: &mut Context<Self>) {
        let key = (chat, end);
        if self.ratings.get(&key) == Some(&rating) {
            self.ratings.remove(&key);
        } else {
            self.ratings.insert(key, rating);
            self.stub(crate::stubs::FEEDBACK_UPLOAD, cx);
        }
        if let Some(c) = self.chat_mut(chat) {
            c.version += 1;
        }
        cx.notify();
    }

    // ------------------------------------------------------------ chats: rename, duplicate, export, delete

    pub fn open_chat_menu(&mut self, id: Option<usize>, cx: &mut Context<Self>) {
        self.chat_menu = id;
        cx.notify();
    }

    /// Start editing a chat's title in the sidebar (shown first, if hidden).
    pub fn start_rename(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        self.renaming = Some(id);
        self.sidebar_open = true;
        self.palette_open = false;
        if let Some(c) = self.chats.iter().find(|c| c.id == id) {
            // A chat hidden by the search or the project filter is shown again.
            if !self.search.is_empty()
                && !c.title.to_lowercase().contains(&self.search.to_lowercase())
            {
                self.search = "".into();
            }
            if c.project != self.current_project {
                self.all_chats = true;
            }
        }
        cx.notify();
    }

    /// Give a chat a new title; an empty one keeps the old.
    pub fn rename_chat(&mut self, id: usize, title: &str, cx: &mut Context<Self>) {
        self.renaming = None;
        let title: String = title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(120)
            .collect();
        if let Some(c) = self.chat_mut(id).filter(|_| !title.is_empty()) {
            c.title = title.into();
        }
        self.mark_dirty(id, cx);
        cx.notify();
    }

    pub fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        if self.renaming.take().is_some() {
            cx.notify();
        }
    }

    /// A copy of a chat, its messages included, opened next to it. The copy
    /// starts its own ForgeCLI conversation on its first message.
    pub fn duplicate_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some(c) = self.chats.iter().find(|c| c.id == id) else {
            return;
        };
        let busy = self.replies.contains_key(&id);
        let mut messages = c.messages.clone();
        // A reply still running is copied as it stands.
        if busy {
            messages.push(Message::Notice {
                text: "Copied while a reply was running; it ends here.".into(),
                error: false,
            });
        }
        let mut copy = Chat::new(
            self.next_id,
            c.project,
            format!("{} (copy)", c.title).into(),
        );
        copy.messages = messages;
        self.next_id += 1;
        let at = self.chats.iter().position(|c| c.id == id).unwrap_or(0);
        let new_id = copy.id;
        self.chats.insert(at, copy);
        self.touch(new_id, cx);
        self.open_chat(new_id, cx);
        self.show_toast("Duplicated the chat.", cx);
    }

    /// Write a chat to Downloads as Markdown, off the UI thread, and say where.
    pub fn export_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some(c) = self.chats.iter().find(|c| c.id == id) else {
            return;
        };
        let Some(dir) = crate::downloads::dir() else {
            return self.show_toast("Could not find your Downloads folder.", cx);
        };
        let project = self
            .project(c.project)
            .map(|p| p.name.to_string())
            .unwrap_or_default();
        let (name, text) = (
            crate::export::file_name(&c.title),
            crate::export::markdown(c, &project),
        );
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(
                    async move { crate::downloads::save(&dir, &name, text.as_bytes()) },
                )
                .await;
            let _ = this.update(cx, |s, cx| match saved {
                Ok(path) => s.show_toast(format!("Exported to {}", path.display()), cx),
                Err(e) => s.show_toast(format!("Could not export it: {e}"), cx),
            });
        })
        .detach();
        cx.notify();
    }

    /// Ask before deleting (the confirm window answers).
    pub fn ask_delete(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        self.palette_open = false;
        self.confirm = Some(Confirm::Delete(id));
        cx.notify();
    }

    /// Delete a chat: its reply stops, its process ends, and it leaves the list.
    pub fn delete_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        self.confirm = None;
        self.replies.remove(&id);
        self.sessions.remove(&id);
        self.asks.remove(&id);
        self.stale_turns.remove(&id);
        self.turn_started.remove(&id);
        self.fresh_block.remove(&id);
        self.tool_rows.retain(|(c, _), _| *c != id);
        self.side.remove(&id);
        self.kept_modes.remove(&id);
        self.ratings.retain(|(c, _), _| *c != id);
        let title = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.clone());
        // Its file goes too (ForgeCLI's own log of the conversation stays, as it does for the CLI).
        self.dirty.remove(&id);
        if let (Some(dir), Some(session)) = (
            self.history.clone(),
            self.chats
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.session.clone()),
        ) {
            cx.background_spawn(async move { crate::history::remove(&dir, &session) })
                .detach();
        }
        self.chats.retain(|c| c.id != id);
        if self.current == Some(id) {
            self.new_chat(cx);
        }
        if self.renaming == Some(id) {
            self.renaming = None;
        }
        if let Some(t) = title {
            self.show_toast(format!("Deleted \u{201c}{t}\u{201d}."), cx);
        }
        cx.notify();
    }

    // ------------------------------------------------------------ find

    /// Open the find bar (it keeps its query if it was open).
    pub fn open_find(&mut self, cx: &mut Context<Self>) {
        if self.chat().is_some_and(|c| !c.messages.is_empty()) && self.find.is_none() {
            self.find = Some(Find::default());
            cx.notify();
        }
    }

    pub fn close_find(&mut self, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            if let Some(id) = self.current {
                if let Some(c) = self.chat_mut(id) {
                    c.version += 1;
                }
            }
            cx.notify();
        }
    }

    /// A new query: the hits are found again, and the newest one is shown
    /// (you are most likely looking for something recent).
    pub fn set_find_query(&mut self, query: &str, cx: &mut Context<Self>) {
        let hits = self
            .chat()
            .map(|c| find_hits(&c.messages, query))
            .unwrap_or_default();
        let Some(f) = self.find.as_mut() else { return };
        f.query = query.to_string();
        f.at = hits.len().saturating_sub(1);
        f.hits = hits;
        f.seq += 1;
        if let Some(id) = self.current {
            if let Some(c) = self.chat_mut(id) {
                c.version += 1;
            }
        }
        cx.notify();
    }

    /// The next (`forward`) or previous hit, wrapping.
    pub fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        // The conversation may have grown since the query was typed.
        let query = match &self.find {
            Some(f) => f.query.clone(),
            None => return,
        };
        let hits = self
            .chat()
            .map(|c| find_hits(&c.messages, &query))
            .unwrap_or_default();
        let Some(f) = self.find.as_mut() else { return };
        let current = f.current();
        f.hits = hits;
        let n = f.hits.len();
        if n == 0 {
            return cx.notify();
        }
        let here = current
            .and_then(|m| f.hits.iter().position(|h| *h == m))
            .unwrap_or(f.at.min(n - 1));
        f.at = if forward {
            (here + 1) % n
        } else {
            (here + n - 1) % n
        };
        f.seq += 1;
        if let Some(id) = self.current {
            if let Some(c) = self.chat_mut(id) {
                c.version += 1;
            }
        }
        cx.notify();
    }

    pub fn show_toast(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toast = Some(text.into());
        self.toast_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(2600))
                .await;
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
                let text: Vec<&str> = blocks
                    .iter()
                    .filter_map(|b| {
                        if let Block::Text(t) = b {
                            Some(t.as_ref())
                        } else {
                            None
                        }
                    })
                    .collect();
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
    pub fn answer(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        verdict: Verdict,
        cx: &mut Context<Self>,
    ) {
        if let Some(c) = self.chat_mut(chat) {
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                if let Some(Block::Permission { verdict: v, .. }) = blocks.get_mut(block) {
                    *v = verdict;
                }
            }
            c.version += 1;
        }
        // A real question: Forge waits on this answer.
        if self
            .asks
            .get(&chat)
            .is_some_and(|a| a.msg == msg && a.block == block)
            && verdict != Verdict::Pending
        {
            let ask = self.asks.remove(&chat).expect("checked above");
            let allow = matches!(verdict, Verdict::Allowed | Verdict::AllowedAlways);
            if !allow {
                if let Some(&(m, b)) = self.tool_rows.get(&(chat, ask.tool_use_id.clone())) {
                    self.update_tool(chat, m, b, |t| {
                        t.status = ToolStatus::Declined;
                        t.output = "You declined this.".into();
                    });
                }
            }
            let sent = self.sessions.get(&chat).map(|l| {
                l.session.answer(
                    &ask.request_id,
                    allow,
                    verdict == Verdict::AllowedAlways,
                    &ask.input,
                    &ask.suggestions,
                )
            });
            if let Some(Err(e)) = sent {
                self.notice(chat, e, true);
                self.finish_turn(chat, cx);
            }
        }
        cx.notify();
    }

    /// Stop the reply being played (the composer's Stop, Escape).
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current else { return };
        if self.replies.remove(&id).is_none() {
            return;
        }
        self.turn_started.remove(&id);
        if let Some(live) = self.sessions.get(&id) {
            // What Forge says until the stopped turn's result belongs to that turn.
            if live.session.interrupt().is_ok() {
                *self.stale_turns.entry(id).or_default() += 1;
            } else {
                self.sessions.remove(&id);
            }
        }
        if let Some(ask) = self.asks.remove(&id) {
            self.answer(id, ask.msg, ask.block, Verdict::Denied, cx);
        }
        {
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
                c.messages.push(Message::Notice {
                    text: INTERRUPTED.into(),
                    error: false,
                });
                c.version += 1;
            }
        }
        self.touch(id, cx);
        cx.notify();
    }

    /// Add your message, and have Forge answer it (`send_to_forge`).
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        // `/btw` opens the side card (and asks), even while a reply runs.
        if let Some(question) = parse_btw(&text) {
            return self.ask_side(question, cx);
        }
        if (text.is_empty() && self.attachments.is_empty()) || self.busy() {
            return;
        }
        if let Some(a) = self.attachments.iter().find(|a| a.pending) {
            return self.show_toast(
                format!(
                    "{} is still being prepared; send when its chip is ready.",
                    a.name
                ),
                cx,
            );
        }
        let attachments = std::mem::take(&mut self.attachments);
        let id = match self.current {
            Some(id) => id,
            None => {
                let id = self.next_id;
                self.next_id += 1;
                let first = text.lines().next().unwrap_or_default();
                let first = if first.is_empty() {
                    attachments
                        .first()
                        .map(|a| a.name.to_string())
                        .unwrap_or_default()
                } else {
                    first.to_string()
                };
                let title: String = first.chars().take(48).collect();
                self.chats
                    .insert(0, Chat::new(id, self.current_project, title.into()));
                self.current = Some(id);
                self.keep_mode();
                id
            }
        };
        self.screen = Screen::Chat;
        if let Some(c) = self.chat_mut(id) {
            c.messages.push(Message::User {
                text: text.clone().into(),
                sent: SystemTime::now(),
                attachments: attachments.iter().map(|a| a.for_history()).collect(),
                turn: None,
            });
            c.version += 1;
        }
        self.touch(id, cx);
        if demo_mode() {
            self.play_script(id, text, cx);
        } else {
            self.send_to_forge(id, text, attachments, cx);
        }
        cx.notify();
    }

    /// `FORGE_DEMO=1`: replies are the scripted demo, no ForgeCLI needed.
    fn play_script(&mut self, id: usize, text: String, cx: &mut Context<Self>) {
        self.replies.insert(
            id,
            cx.spawn(async move |this, cx| {
                for step in script(&text) {
                    let wait = step.delay();
                    cx.background_executor()
                        .timer(Duration::from_millis(wait))
                        .await;
                    // A permission prompt holds the reply until it is answered.
                    loop {
                        let waiting = this
                            .update(cx, |s, _| s.waiting_on_permission(id))
                            .unwrap_or(false);
                        if !waiting {
                            break;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(120))
                            .await;
                    }
                    let denied = this
                        .update(cx, |s, _| s.last_permission_denied(id))
                        .unwrap_or(false);
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
                    s.replies.remove(&id);
                    // Finished while you were elsewhere: it waits as unread.
                    if s.current != Some(id) {
                        if let Some(c) = s.chat_mut(id) {
                            c.unread = true;
                        }
                    }
                    cx.notify();
                });
            }),
        );
        cx.notify();
    }

    // ------------------------------------------------------------ the engine

    /// Send your message to the chat's ForgeCLI process, starting it first
    /// when the chat has none (or the endpoint changed under it).
    fn send_to_forge(
        &mut self,
        id: usize,
        text: String,
        attachments: Vec<crate::attach::Attachment>,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.active_profile().cloned() else {
            return self.notice(id, "Set up an endpoint first: Forge needs a model to answer. Open the model menu to add one.", true);
        };
        if let Some(live) = self.sessions.get(&id) {
            if live.profile != profile {
                let same_endpoint = live.profile.base_url == profile.base_url
                    && live.profile.wire == profile.wire
                    && live.profile.auth == profile.auth
                    && live.profile.headers == profile.headers;
                if same_endpoint && live.session.set_model(&profile.model).is_ok() {
                    self.sessions.get_mut(&id).expect("checked above").profile = profile.clone();
                } else {
                    self.sessions.remove(&id);
                    if let Some(c) = self.chat_mut(id) {
                        c.new_conversation();
                    }
                    self.notice(id, format!("Now on {}: a new endpoint starts a new conversation, so Forge doesn't see the messages above.", profile.display()), false);
                }
            }
        }
        if !self.sessions.contains_key(&id) {
            if let Err(e) = self.start_session(id, profile, cx) {
                return self.notice(id, e, true);
            }
        }
        let sent = self
            .sessions
            .get(&id)
            .map(|l| {
                if attachments.is_empty() {
                    l.session.send(&text)
                } else {
                    l.session.send_content(crate::attach::content_blocks(
                        &text,
                        &attachments,
                        l.profile.wire == crate::endpoints::Wire::Anthropic,
                    ))
                }
            })
            .unwrap_or(Err("Forge isn't running.".into()));
        if let Err(e) = sent {
            self.sessions.remove(&id);
            return self.notice(id, e, true);
        }
        // A command ForgeCLI answers at once (`/status`) starts no turn: its
        // answer comes back as an immediate result.
        if attachments.is_empty() && crate::commands::is_immediate(&text) {
            self.immediate_queue.entry(id).or_default().push_back(true);
            return;
        }
        self.replies.insert(id, Task::ready(()));
        self.turn_started.insert(id, std::time::Instant::now());
    }

    fn start_session(
        &mut self,
        id: usize,
        profile: crate::endpoints::Profile,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let exe = crate::backend::find_cli().ok_or("ForgeCLI wasn't found. Put forge.exe beside ForgeDesktop.exe, or set FORGE_CLI to its path.")?;
        let (session_id, resumable) = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .map(|c| (c.session.clone(), c.resumable))
            .unwrap_or_else(|| (crate::history::new_id(), false));
        let conversation = if resumable {
            crate::backend::Conversation::Resume(&session_id)
        } else {
            crate::backend::Conversation::New(&session_id)
        };
        let mut launch =
            crate::backend::launch(&profile, self.mode, self.bypass_allowed, conversation)?;
        // The web browser: Forge's own browser tools (this executable, as
        // an MCP server) on the browser in the side panel.
        if self.browser {
            if let (Some(cdp), Ok(exe)) = (self.ensure_web(cx), std::env::current_exe()) {
                launch.args.extend([
                    "--mcp-config".to_string(),
                    crate::browser::mcp_config(&exe, &cdp, &crate::browser::focus_file()),
                ]);
            }
        }
        // The settings profile, layered above the other settings files.
        if let Some(path) = self
            .settings_profile
            .as_deref()
            .and_then(crate::setting_profiles::path_of)
        {
            launch.args.extend([
                "--settings".to_string(),
                path.to_string_lossy().into_owned(),
            ]);
        }
        if resumable {
            self.resuming.insert(id);
        } else {
            self.resuming.remove(&id);
        }
        let project = self.chats.iter().find(|c| c.id == id).map(|c| c.project);
        let dir = project
            .and_then(|p| self.project(p))
            .map(|p| p.path.clone())
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| self.project_dir.clone());
        let session = crate::backend::Session::spawn(&exe, &launch, &dir)?;
        let events = session.events.clone();
        self.generations += 1;
        let generation = self.generations;
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(ev) = events.recv().await {
                if this
                    .update(cx, |s, cx| s.on_event(id, generation, ev, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.stale_turns.remove(&id);
        self.sessions.insert(
            id,
            Live {
                session,
                _pump: pump,
                profile,
                generation,
            },
        );
        Ok(())
    }

    fn on_event(
        &mut self,
        chat: usize,
        generation: u64,
        ev: crate::backend::Event,
        cx: &mut Context<Self>,
    ) {
        use crate::backend::Event;
        // A process that was replaced or dropped (edit, retry, a new endpoint,
        // a failed resume) may still have events queued: they aren't this chat's.
        if self.sessions.get(&chat).map(|l| l.generation) != Some(generation) {
            return;
        }
        self.on_agent_event(chat, &ev);
        if !matches!(ev, Event::Exited { .. } | Event::Error(_)) {
            self.resuming.remove(&chat);
        } else if self.resuming.remove(&chat) {
            // It failed before saying anything ("no session …", then exit):
            // the conversation couldn't be reopened (removed, or written on
            // another machine). Go on in a new one, sending the message
            // again. Dropping the process drops its events too.
            self.sessions.remove(&chat);
            self.asks.remove(&chat);
            self.stale_turns.remove(&chat);
            let pending = self
                .replies
                .contains_key(&chat)
                .then(|| {
                    self.chats.iter().find(|c| c.id == chat).and_then(|c| {
                        c.messages.iter().rev().find_map(|m| {
                            if let Message::User {
                                text, attachments, ..
                            } = m
                            {
                                Some((text.to_string(), attachments.clone()))
                            } else {
                                None
                            }
                        })
                    })
                })
                .flatten();
            if let Some(c) = self.chat_mut(chat) {
                c.new_conversation();
            }
            self.notice(chat, "Forge couldn't reopen this conversation's earlier context, so it goes on as a new conversation.", false);
            if let Some((text, attachments)) = pending {
                self.replies.remove(&chat);
                self.send_to_forge(chat, text, attachments, cx);
            }
            self.touch(chat, cx);
            return cx.notify();
        }
        // After Stop, the stopped turn's last words are dropped up to its result.
        if let Some(n) = self.stale_turns.get_mut(&chat).filter(|n| **n > 0) {
            match ev {
                Event::TurnEnd { .. } => {
                    *n -= 1;
                    return;
                }
                Event::Ask {
                    request_id,
                    input,
                    suggestions,
                    ..
                } => {
                    if let Some(l) = self.sessions.get(&chat) {
                        let _ = l
                            .session
                            .answer(&request_id, false, false, &input, &suggestions);
                    }
                    return;
                }
                Event::Exited { .. } | Event::Immediate { .. } => {}
                _ => return,
            }
        }
        match ev {
            // Sub-agents fill the Agents panel (`on_agent_event`), not the transcript.
            Event::SubTool { .. } | Event::SubResult { .. } | Event::SubText { .. } => {}
            Event::Break => {
                self.fresh_block.insert(chat);
            }
            Event::Text(t) if self.fresh_block.remove(&chat) => {
                self.push_block(chat, Block::Text(t.into()));
            }
            Event::Thinking(t) if self.fresh_block.remove(&chat) => {
                self.push_block(
                    chat,
                    Block::Thinking {
                        text: t.into(),
                        open: false,
                    },
                );
            }
            Event::Text(t) => self.apply(chat, Step::Say(t), cx),
            Event::Thinking(t) => self.apply(chat, Step::Think(t), cx),
            Event::ToolUse { id, name, input } => {
                self.panel_follows(chat, &name);
                if let Some(at) =
                    self.push_block(chat, Block::Tool(crate::backend::tool_call(&name, &input)))
                {
                    self.tool_rows.insert((chat, id), at);
                }
            }
            Event::ToolResult {
                id,
                output,
                is_error,
            } => {
                if let Some(&(m, b)) = self.tool_rows.get(&(chat, id)) {
                    let mut written = None;
                    self.update_tool(chat, m, b, |t| {
                        if t.status != ToolStatus::Declined {
                            t.status = if is_error {
                                ToolStatus::Failed
                            } else {
                                ToolStatus::Done
                            };
                            t.output = output.into();
                            if !is_error && t.kind == ToolKind::Write {
                                written = Some(t.target.to_string());
                            }
                        }
                    });
                    // A document Forge wrote gets its card in the reply.
                    if let Some(file) = written.and_then(|p| self.written_file(chat, &p)) {
                        self.push_block(chat, Block::File(file));
                    }
                }
            }
            Event::Ask {
                request_id,
                tool_use_id,
                tool_name,
                input,
                suggestions,
            } => {
                let (title, detail) = crate::backend::ask_text(&tool_name, &input);
                if let Some((msg, block)) = self.push_block(
                    chat,
                    Block::Permission {
                        title: title.into(),
                        detail: detail.into(),
                        verdict: Verdict::Pending,
                    },
                ) {
                    self.asks.insert(
                        chat,
                        Ask {
                            request_id,
                            tool_use_id,
                            input,
                            suggestions,
                            msg,
                            block,
                        },
                    );
                }
            }
            Event::CancelAsk { request_id } => {
                if let Some(ask) = self.asks.get(&chat).filter(|a| a.request_id == request_id) {
                    let (m, b) = (ask.msg, ask.block);
                    self.asks.remove(&chat);
                    self.answer(chat, m, b, Verdict::Denied, cx);
                }
            }
            Event::Immediate { text, is_error } => {
                // The oldest question asked of this kind gets it: a typed command's
                // answer goes in the chat, a `/btw` answer in the side card.
                let command = self
                    .immediate_queue
                    .get_mut(&chat)
                    .and_then(|q| q.pop_front())
                    .unwrap_or(false);
                if command {
                    self.push_block(
                        chat,
                        Block::Text(if text.is_empty() {
                            "(no output)".into()
                        } else {
                            format!("```text\n{text}\n```").into()
                        }),
                    );
                    if is_error {
                        self.notice(chat, "ForgeCLI reported an error for this command.", true);
                    }
                    self.touch(chat, cx);
                } else {
                    self.answer_side(chat, text, is_error);
                }
            }
            Event::UserAccepted { uuid } => {
                // The newest of your messages without an id is the one just accepted.
                if let Some(c) = self.chat_mut(chat) {
                    if let Some(Message::User { turn, .. }) = c
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| matches!(m, Message::User { turn: None, .. }))
                    {
                        *turn = Some(uuid.into());
                    }
                }
                self.mark_dirty(chat, cx);
            }
            Event::ControlReply { request_id, result } => {
                if let Some(req) = self.requests.remove(&request_id) {
                    self.on_reply(req, result, cx);
                }
            }
            Event::TurnEnd { ms, usage, error } => {
                // A compaction's turn ends in its notice, not a reply's end:
                // "Compacted" when it worked, its error when it didn't.
                if self.compacting.remove(&chat) {
                    if let Some(c) = self.chat_mut(chat) {
                        c.messages.retain(|m| !matches!(m, Message::Notice { text, .. } if text.as_ref() == COMPACTING));
                        c.resumable = true;
                    }
                    match error {
                        Some(e) => self.notice(chat, e, true),
                        None => self.notice(chat, COMPACTED, false),
                    }
                    return self.finish_turn(chat, cx);
                }
                if let Some(e) = error {
                    self.notice(chat, e, true);
                }
                let seconds = if ms > 0 {
                    ms as f32 / 1000.
                } else {
                    self.turn_started
                        .get(&chat)
                        .map(|t| t.elapsed().as_secs_f32())
                        .unwrap_or(0.)
                };
                if let Some(c) = self.chat_mut(chat) {
                    c.messages.push(Message::TurnEnd { seconds, usage });
                    c.version += 1;
                    // ForgeCLI has written the conversation: it can be resumed.
                    c.resumable = true;
                }
                self.finish_turn(chat, cx);
            }
            Event::Error(e) => {
                self.notice(chat, e, true);
                self.finish_turn(chat, cx);
            }
            Event::Exited { code, detail } => {
                self.sessions.remove(&chat);
                while self.side.get(&chat).is_some_and(|s| !s.pending.is_empty()) {
                    self.answer_side(chat, "Forge stopped before it answered.".into(), true);
                }
                self.asks.remove(&chat);
                self.stale_turns.remove(&chat);
                if self.replies.contains_key(&chat) {
                    let why = match code {
                        Some(c) => format!("Forge stopped (exit code {c})."),
                        None => "Forge stopped.".to_string(),
                    };
                    let text = if detail.is_empty() {
                        why
                    } else {
                        format!("{why}\n{detail}")
                    };
                    self.notice(chat, text, true);
                    self.finish_turn(chat, cx);
                }
            }
        }
        cx.notify();
    }

    /// A document Forge wrote, read back for its card: Markdown, a diagram,
    /// an Office file or a PDF. None for anything else (code has its diff).
    fn written_file(&self, chat: usize, path: &str) -> Option<FileRef> {
        let name = path.rsplit(['/', '\\']).next()?.to_string();
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        if !matches!(
            ext.as_str(),
            "md" | "markdown" | "svg" | "html" | "htm" | "docx" | "xlsx" | "pptx" | "pdf"
        ) {
            return None;
        }
        let project = self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| self.project(c.project))
            .map(|p| p.path.clone())
            .unwrap_or_else(|| self.project_dir.clone());
        let full = {
            let p = std::path::Path::new(path);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                project.join(p)
            }
        };
        let bytes = std::fs::read(&full).ok()?;
        // An HTML page gets a card only when it is a diagram (Diagram
        // Design's pages are); other pages stay a tool row.
        if matches!(ext.as_str(), "html" | "htm")
            && !crate::diagram::is_diagram(&name, &String::from_utf8_lossy(&bytes))
        {
            return None;
        }
        let binary = matches!(ext.as_str(), "docx" | "xlsx" | "pptx" | "pdf");
        Some(FileRef {
            name: name.into(),
            path: Some(full.display().to_string().into()),
            content: if binary {
                "".into()
            } else {
                String::from_utf8_lossy(&bytes).into_owned().into()
            },
            bytes: binary.then(|| std::sync::Arc::from(bytes.into_boxed_slice())),
        })
    }

    /// Add a block to the reply being written (starting one if needed); where it went.
    fn push_block(&mut self, chat: usize, block: Block) -> Option<(usize, usize)> {
        let c = self.chat_mut(chat)?;
        if !matches!(c.messages.last(), Some(Message::Assistant { .. })) {
            c.messages.push(Message::Assistant { blocks: vec![] });
        }
        let m = c.messages.len() - 1;
        let Some(Message::Assistant { blocks }) = c.messages.get_mut(m) else {
            return None;
        };
        blocks.push(block);
        let b = blocks.len() - 1;
        c.version += 1;
        Some((m, b))
    }

    fn update_tool(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        f: impl FnOnce(&mut ToolCall),
    ) {
        let Some(c) = self.chat_mut(chat) else { return };
        if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
            if let Some(Block::Tool(t)) = blocks.get_mut(block) {
                f(t);
            }
        }
        c.version += 1;
    }

    fn notice(&mut self, chat: usize, text: impl Into<SharedString>, error: bool) {
        if let Some(c) = self.chat_mut(chat) {
            c.messages.push(Message::Notice {
                text: text.into(),
                error,
            });
            c.version += 1;
        }
    }

    /// The turn is over: the composer is free again.
    fn finish_turn(&mut self, chat: usize, cx: &mut Context<Self>) {
        self.touch(chat, cx);
        self.replies.remove(&chat);
        self.turn_started.remove(&chat);
        if let Some(c) = self.chat_mut(chat) {
            for m in c.messages.iter_mut() {
                if let Message::Assistant { blocks } = m {
                    for b in blocks.iter_mut() {
                        if let Block::Tool(t) = b {
                            if t.status == ToolStatus::Running {
                                t.status = ToolStatus::Failed;
                            }
                        }
                    }
                }
            }
            c.version += 1;
        }
        if self.current != Some(chat) {
            if let Some(c) = self.chat_mut(chat) {
                c.unread = true;
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------ side questions

    /// The side card of the chat showing, if it is open.
    pub fn side_shown(&self) -> Option<&Side> {
        self.current
            .and_then(|id| self.side.get(&id))
            .filter(|s| s.visible)
    }

    /// Open the side card and, when there is a question, ask it. A new chat
    /// gets its chat first: side questions belong to a conversation.
    pub fn ask_side(&mut self, question: String, cx: &mut Context<Self>) {
        let id = match self.current {
            Some(id) => id,
            None => {
                let id = self.next_id;
                self.next_id += 1;
                self.chats
                    .insert(0, Chat::new(id, self.current_project, SIDE_TITLE.into()));
                self.current = Some(id);
                self.keep_mode();
                id
            }
        };
        self.screen = Screen::Chat;
        let side = self.side.entry(id).or_default();
        side.visible = true;
        side.minimized = false;
        self.side_focus = true;
        let question = question.trim().to_string();
        if !question.is_empty() {
            self.side
                .entry(id)
                .or_default()
                .pending
                .push_back(question.clone().into());
            if demo_mode() {
                self.answer_side(id, format!("(Demo) A side answer to \"{question}\": ForgeCLI answers these from the conversation, without tools."), false);
            } else if let Err(e) = self.send_side(id, &question, cx) {
                self.answer_side(id, e, true);
            }
        }
        cx.notify();
    }

    fn send_side(
        &mut self,
        id: usize,
        question: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.sessions.contains_key(&id) {
            let profile = self
                .active_profile()
                .cloned()
                .ok_or("Set up an endpoint first: side questions need a model.")?;
            self.start_session(id, profile, cx)?;
        }
        let live = self.sessions.get(&id).ok_or("Forge isn't running.")?;
        live.session.send(&format!("/btw {question}"))?;
        self.immediate_queue.entry(id).or_default().push_back(false);
        Ok(())
    }

    /// The oldest waiting question gets its answer (or its error).
    fn answer_side(&mut self, chat: usize, text: String, is_error: bool) {
        let Some(side) = self.side.get_mut(&chat) else {
            return;
        };
        let Some(question) = side.pending.pop_front() else {
            return;
        };
        side.push(if is_error || text.is_empty() {
            let message = if text.is_empty() {
                "Forge didn't return an answer for this side question.".to_string()
            } else {
                text
            };
            SideItem::Failed {
                question,
                message: message.into(),
            }
        } else {
            SideItem::Answer {
                question,
                answer: text.into(),
            }
        });
    }

    pub fn close_side(&mut self, cx: &mut Context<Self>) {
        if let Some(side) = self.current.and_then(|id| self.side.get_mut(&id)) {
            side.visible = false;
        }
        cx.notify();
    }

    pub fn toggle_side_min(&mut self, cx: &mut Context<Self>) {
        if let Some(side) = self.current.and_then(|id| self.side.get_mut(&id)) {
            side.minimized = !side.minimized;
            self.side_focus = !side.minimized;
        }
        cx.notify();
    }

    pub fn clear_side(&mut self, cx: &mut Context<Self>) {
        if let Some(side) = self.current.and_then(|id| self.side.get_mut(&id)) {
            side.thread.clear();
        }
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
        if let Step::Agent(ev) = &step {
            self.on_agent_event(chat, ev);
            // The agent's own row in the reply, as a real one has.
            match ev {
                crate::backend::Event::ToolUse { name, input, .. } => {
                    let call = crate::backend::tool_call(name, input);
                    self.apply(
                        chat,
                        Step::ToolStart(
                            call.kind,
                            call.target.to_string(),
                            call.note.map(|n| n.to_string()),
                        ),
                        cx,
                    );
                }
                crate::backend::Event::ToolResult { .. } => {
                    // The oldest agent row still running is the one done.
                    if let Some(Message::Assistant { blocks }) =
                        self.chat_mut(chat).and_then(|c| c.messages.last_mut())
                    {
                        if let Some(Block::Tool(t)) = blocks.iter_mut().find(|b| {
                            matches!(b, Block::Tool(t) if is_agent_tool(&t.target) && t.status == ToolStatus::Running)
                        }) {
                            t.status = ToolStatus::Done;
                        }
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }
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
                _ => blocks.push(Block::Thinking {
                    text: t.into(),
                    open: false,
                }),
            },
            (Step::Say(t), Some(blocks)) => match blocks.last_mut() {
                Some(Block::Text(text)) => *text = format!("{text}{t}").into(),
                _ => blocks.push(Block::Text(t.into())),
            },
            (Step::ToolStart(kind, target, note), Some(blocks)) => {
                blocks.push(Block::Tool(ToolCall {
                    kind,
                    target: target.into(),
                    note: note.map(Into::into),
                    status: ToolStatus::Running,
                    output: "".into(),
                    diff: vec![],
                    // A command's terminal block is open from the start, as in Forge.
                    open: kind == ToolKind::Bash,
                    full: false,
                    review: Review::Open,
                }))
            }
            (Step::ToolDone(output, diff), Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, Block::Tool(_)))
                {
                    t.status = ToolStatus::Done;
                    t.output = output.into();
                    t.diff = diff;
                }
            }
            (Step::Use(name, input, output), Some(blocks)) => {
                let mut t = crate::backend::tool_call(name, &input);
                t.status = ToolStatus::Done;
                t.output = output.into();
                blocks.push(Block::Tool(t));
                self.panel_follows(chat, name);
            }
            (Step::File(f), Some(blocks)) => blocks.push(Block::File(f)),
            (Step::Ask(title, detail), Some(blocks)) => blocks.push(Block::Permission {
                title: title.into(),
                detail: detail.into(),
                verdict: Verdict::Pending,
            }),
            (Step::Denied, Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, Block::Tool(_)))
                {
                    t.status = ToolStatus::Declined;
                    t.output = "You declined this edit.".into();
                }
                blocks.push(Block::Text(
                    "Understood, I won't change the file. Tell me how you'd like to proceed."
                        .into(),
                ));
            }
            (Step::End { seconds }, _) => {
                // A mock reply has no model to report usage: count it from the text.
                let usage = crate::context::estimate(&c.messages);
                c.messages.push(Message::TurnEnd {
                    seconds,
                    usage: Some(usage),
                });
                self.touch(chat, cx);
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
    /// A tool call as ForgeCLI sends it (name and input), done with its output.
    Use(&'static str, serde_json::Value, String),
    /// A file the reply made: its card.
    File(FileRef),
    /// What the engine says about a sub-agent, as it would say it.
    Agent(crate::backend::Event),
    End {
        seconds: f32,
    },
}

impl Step {
    fn delay(&self) -> u64 {
        match self {
            Step::Think(_) | Step::Say(_) => 45,
            Step::ToolStart(..) => 350,
            Step::ToolDone(..) => 900,
            Step::Ask(..) => 300,
            Step::Use(..) => 500,
            Step::File(..) => 300,
            Step::Agent(..) => 380,
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
    lines
        .iter()
        .map(|(k, t)| DiffLine {
            kind: *k,
            text: (*t).into(),
        })
        .collect()
}

/// The demo's drawn answer: a flowchart and a sequence, in Mermaid.
const DEMO_DIAGRAM: &str = "Here is how a message travels through Forge.\n\n```mermaid\nflowchart TD\n  A[You type a message] --> B{Endpoint set up?}\n  B -->|no| C[Settings › Models]\n  C --> D\n  B -->|yes| D[ForgeCLI process]\n  D -.->|stream-json| E([Reply in the chat])\n  D --> F[(Session log)]\n```\n\nAnd the turn itself, step by step:\n\n```mermaid\nsequenceDiagram\n  autonumber\n  participant U as You\n  participant D as Forge\n  participant C as ForgeCLI\n  U->>D: Send\n  D->>C: user turn\n  loop each tool call\n    C-->>D: tool_use / tool_result\n  end\n  C-->>D: result\n  D-->>U: Reply\n```";

/// The demo's charts: a pie and an xychart, drawn natively.
const DEMO_CHARTS: &str = "Where the time went:\n\n```mermaid\npie title Time per task\n  \"Reading\" : 42\n  \"Writing\" : 30\n  \"Tests\" : 12\n```\n\nAnd per month:\n\n```mermaid\nxychart-beta\n  title \"Replies\"\n  x-axis [jan, feb, mar, apr]\n  y-axis \"Replies\" 0 --> 100\n  bar [20, 45, 60, 80]\n  line [25, 40, 70, 90]\n```\n";

/// The demo's ASCII answer, for checking that box drawing keeps its columns.
const DEMO_ASCII: &str = "Here it is in ASCII.\n\n```text\n┌──────────────┐     ┌──────────────┐\n│  Forge app   │ ──▶ │   ForgeCLI   │\n└──────┬───────┘     └──────┬───────┘\n       │ stream-json        │ HTTPS\n       ▼                    ▼\n┌──────────────┐     ┌──────────────┐\n│  Chat view   │ ◀── │    Model     │\n└──────────────┘     └──────────────┘\n```\n\nAnd the same without a fence:\n\n+--------+      +---------+\n| Client | ---> | Server  |\n+--------+      +---------+\n     |               |\n     v               v\n  [ UI ]         [ DB ]\n";

/// The demo's sub-agents: three reviewers at once, each with its own steps
/// and answer, as the engine reports them.
fn demo_agents() -> Vec<Step> {
    use crate::backend::Event;
    use serde_json::json;
    let start = |id: &str, kind: &str, task: &str, prompt: &str| {
        Step::Agent(Event::ToolUse {
            id: id.into(),
            name: "Task".into(),
            input: json!({ "subagent_type": kind, "description": task, "prompt": prompt }),
        })
    };
    let tool = |parent: &str, id: &str, name: &str, input: serde_json::Value| {
        Step::Agent(Event::SubTool {
            parent: parent.into(),
            id: id.into(),
            name: name.into(),
            input,
        })
    };
    let done = |parent: &str, id: &str| {
        Step::Agent(Event::SubResult {
            parent: parent.into(),
            id: id.into(),
            is_error: false,
        })
    };
    let say = |parent: &str, text: &str| {
        Step::Agent(Event::SubText {
            parent: parent.into(),
            text: text.into(),
        })
    };
    let end = |id: &str, out: &str| {
        Step::Agent(Event::ToolResult {
            id: id.into(),
            output: out.into(),
            is_error: false,
        })
    };
    let mut s = words(
        "I'll split the review across three agents: one per area, working at the same time.",
        Step::Say,
    );
    s.extend([
        start("a1", "Explore", "Map the chat pipeline", "Find how a message travels from the composer to ForgeCLI and back. List the files and the functions on the path."),
        start("a2", "code-reviewer", "Review the side panel", "Review src/ui/workbench.rs and src/ui/doc_pane.rs for bugs and dead code. Report findings with file:line."),
        start("a3", "test-runner", "Check the tests", "Run the test suite, and list any test that is slow or flaky."),
        tool("a1", "a1t1", "Grep", json!({ "pattern": "fn send", "path": "src" })),
        tool("a2", "a2t1", "Read", json!({ "file_path": "src/ui/workbench.rs" })),
        tool("a3", "a3t1", "Bash", json!({ "command": "cargo test", "description": "Run the tests" })),
        done("a1", "a1t1"),
        tool("a1", "a1t2", "Read", json!({ "file_path": "src/state.rs" })),
        done("a2", "a2t1"),
        tool("a2", "a2t2", "Read", json!({ "file_path": "src/ui/doc_pane.rs" })),
        done("a1", "a1t2"),
        tool("a1", "a1t3", "Read", json!({ "file_path": "src/backend.rs" })),
        say("a2", "workbench.rs looks sound; checking how doc_pane leads back to the file list."),
        done("a2", "a2t2"),
        tool("a2", "a2t3", "Grep", json!({ "pattern": "from_panel", "path": "src/ui" })),
        done("a1", "a1t3"),
        say("a1", "The path: composer.rs → AppState::send → backend::Session::send (stream-json) → Parser::line → AppState::on_event → the transcript."),
        end("a1", "The path: composer.rs → AppState::send → backend::Session::send (stream-json) → Parser::line → AppState::on_event → the transcript."),
        done("a2", "a2t3"),
        done("a3", "a3t1"),
        tool("a3", "a3t2", "Bash", json!({ "command": "cargo test -- --report-time", "description": "Time each test" })),
        end("a2", "No bugs found. One note: the panel's Files list dedupes by name, so two files with the same name in different folders show once."),
        done("a3", "a3t2"),
        end("a3", "235 tests pass. The slowest is the real-browser test (about 20 s, ignored by default); none is flaky."),
    ]);
    s.extend(words("All three are done: the chat path is mapped, the side panel review found no bugs (one note on duplicate names), and the 235 tests pass.", Step::Say));
    s
}

/// The demo's planned task: a plan, its task list, a look in the browser
/// and the files it made, which the side panel's tabs show.
fn demo_plan() -> Vec<Step> {
    use serde_json::json;
    let file = |name: &str, path: &str, content: &str, bytes: Option<&'static [u8]>| {
        Step::File(FileRef {
            name: name.to_string().into(),
            path: Some(path.to_string().into()),
            content: content.to_string().into(),
            bytes: bytes.map(Into::into),
        })
    };
    let mut s = vec![
        Step::Use(
            "ExitPlanMode",
            json!({"plan": "## Update the Q3 budget\n\n1. **Read** the budget sheet and the finance team's notes.\n2. **Check** the licence prices on the vendor's page.\n3. **Update** licences and travel, then recalculate the totals.\n4. **Write** a one-page memo and a PDF for the review.\n\nNothing else in the workbook changes."}),
            String::new(),
        ),
        Step::Use(
            "TodoWrite",
            json!({"todos": [
                {"content": "Read the budget sheet", "status": "completed"},
                {"content": "Check licence prices", "status": "completed"},
                {"content": "Update and recalculate", "status": "in_progress"},
                {"content": "Write the memo and the PDF", "status": "pending"}
            ]}),
            String::new(),
        ),
        Step::Use(
            "mcp__forge-browser__navigate",
            json!({"url": "https://vendor.example/pricing"}),
            "Navigated to: https://vendor.example/pricing".into(),
        ),
        Step::Use(
            "mcp__forge-browser__state",
            json!({}),
            r#"{"url": "https://vendor.example/pricing#teams", "title": "Pricing"}"#.into(),
        ),
        Step::Use(
            "mcp__forge-browser__click",
            json!({"index": 7}),
            "Clicked element 7".into(),
        ),
        Step::Use(
            "mcp__forge-browser__text",
            json!({"query": "the Teams plan's price per seat"}),
            "Teams: 14 per seat per month".into(),
        ),
    ];
    s.extend(words("I checked the vendor's prices, updated the budget and wrote it up. The files are in the side panel.", Step::Say));
    s.extend([
        file(
            "settings-migration-plan.md",
            "docs/settings-migration-plan.md",
            include_str!("../assets/docs/settings-migration-plan.md"),
            None,
        ),
        file(
            "q3-budget.xlsx",
            "finance/q3-budget.xlsx",
            "",
            Some(include_bytes!("../assets/docs/samples/q3-budget.xlsx")),
        ),
        file(
            "q3-budget.pdf",
            "finance/q3-budget.pdf",
            "",
            Some(include_bytes!("../assets/docs/samples/q3-budget.pdf")),
        ),
        file(
            "forge-chat-path.html",
            "docs/forge-chat-path.html",
            include_str!("../assets/docs/forge-chat-path.html"),
            None,
        ),
    ]);
    s
}

fn script(prompt: &str) -> Vec<Step> {
    let mut s = vec![];
    s.extend(words(
        &format!("The user asked: \"{}\". I should read the entry point first, change only what is needed, then run the tests to prove it.", prompt.lines().next().unwrap_or_default()),
        Step::Think,
    ));
    // Asked to draw something: the reply draws it, as a Mermaid diagram.
    let lower = prompt.to_lowercase();
    if lower.contains("agent") {
        s.extend(demo_agents());
        s.push(Step::End { seconds: 14.2 });
        return s;
    }
    if lower.contains("plan") {
        s.extend(demo_plan());
        s.push(Step::End { seconds: 5.2 });
        return s;
    }
    if lower.contains("chart") {
        s.extend(words(DEMO_CHARTS, Step::Say));
        s.push(Step::End { seconds: 2.4 });
        return s;
    }
    if lower.contains("ascii") {
        s.extend(words(DEMO_ASCII, Step::Say));
        s.push(Step::End { seconds: 3.1 });
        return s;
    }
    if ["diagram", "draw", "flow", "sequence", "architecture"]
        .iter()
        .any(|w| lower.contains(w))
    {
        s.extend(words(DEMO_DIAGRAM, Step::Say));
        s.push(Step::End { seconds: 6.4 });
        return s;
    }
    s.extend(words(
        "Let me look at how the greeting is wired before changing anything.",
        Step::Say,
    ));
    s.push(Step::ToolStart(ToolKind::Read, "src/main.rs".into(), None));
    s.push(Step::ToolDone(
        "fn main() {\n    println!(\"Hello, world\");\n}\n".into(),
        vec![],
    ));
    s.push(Step::ToolStart(ToolKind::Edit, "src/main.rs".into(), None));
    s.push(Step::Ask(
        "Edit src/main.rs?".into(),
        "Replace the greeting with a function main calls, and add a test for it.".into(),
    ));
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
    s.push(Step::ToolStart(
        ToolKind::Bash,
        "cargo test".into(),
        Some("Run the tests".into()),
    ));
    s.push(Step::ToolDone("   \x1b[1;32mCompiling\x1b[0m hello v0.1.0 (/home/amine/dev/kryptonite)\n    \x1b[1;32mFinished\x1b[0m `test` profile [unoptimized + debuginfo] target(s) in 1.84s\n     \x1b[1;32mRunning\x1b[0m unittests src/main.rs (target/debug/deps/hello-3f2a9c1e)\n\nrunning 3 tests\ntest tests::greets_by_name ... \x1b[32mok\x1b[0m\ntest tests::greeting_is_static ... \x1b[32mok\x1b[0m\ntest tests::main_prints_the_greeting ... \x1b[32mok\x1b[0m\n\ntest result: \x1b[32mok\x1b[0m. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n   \x1b[1;32mDoc-tests\x1b[0m hello\n\nrunning 0 tests\n\ntest result: \x1b[32mok\x1b[0m. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s".into(), vec![]));
    s.extend(words(
        "Done. The greeting now lives in `greeting()` and `main` prints it.\n\n**What changed**\n- `src/main.rs`: a new `greeting()` function, used by `main`.\n- A unit test, `greets_by_name`, checks the text.\n\n```rust\npub fn greeting() -> &'static str {\n    \"Hello, Forge\"\n}\n```\n\nAll tests pass.",
        Step::Say,
    ));
    s.push(Step::End { seconds: 14.2 });
    s
}

// ---------------------------------------------------------------- mock chats

/// The folder Forge started in, and a few others to switch between.
/// The projects: the folder Forge started in, then the ones you opened
/// before (`projects.txt` in the config folder).
fn initial_projects(here: &std::path::Path) -> Vec<Project> {
    let project = |id, path: std::path::PathBuf, opened| Project {
        id,
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
            .into(),
        branch: git_branch(&path).into(),
        path,
        pinned: false,
        opened_minutes_ago: opened,
    };
    let mut out = vec![project(1, here.to_path_buf(), 0)];
    for path in saved_projects() {
        if path.is_dir() && !out.iter().any(|p| p.path == path) {
            out.push(project(out.len() + 1, path, 60));
        }
    }
    out
}

fn projects_file() -> Option<std::path::PathBuf> {
    crate::profile::dir().map(|d| d.join("projects.txt"))
}

fn saved_projects() -> Vec<std::path::PathBuf> {
    projects_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| {
            t.lines()
                .filter(|l| !l.trim().is_empty())
                .map(std::path::PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Keep the folders you opened for next time (blocking: off the UI thread).
fn save_projects(paths: Vec<std::path::PathBuf>) {
    let Some(f) = projects_file() else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let text: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    let _ = std::fs::write(f, text.join("\n"));
}

/// The checked-out branch, read from `.git/HEAD`; empty when it isn't a repository.
pub fn git_branch(dir: &std::path::Path) -> String {
    let head = dir.join(".git").join("HEAD");
    std::fs::read_to_string(head)
        .ok()
        .map(|h| {
            h.trim()
                .strip_prefix("ref: refs/heads/")
                .map(str::to_string)
                .unwrap_or_else(|| h.trim().chars().take(7).collect())
        })
        .unwrap_or_default()
}

#[cfg(test)]
fn minutes_ago(minutes: u32) -> SystemTime {
    SystemTime::now() - Duration::from_secs(minutes as u64 * 60)
}

#[cfg(test)]
fn mock_chats() -> Vec<Chat> {
    let rich = vec![
        Message::user("Fix the greeting in src/main.rs so it says Hello, Forge, and add a test for it.".into(), minutes_ago(9)),
        Message::Assistant {
            blocks: vec![
                Block::Thinking {
                    text: "The greeting is printed directly in `main`. To test it I should move the string into a function that `main` calls, then assert on that function.".into(),
                    open: false,
                },
                Block::Text("I'll move the greeting into a small function so it can be tested, then update `main`.".into()),
                Block::Tool(ToolCall { kind: ToolKind::Read, target: "src/main.rs".into(), note: None, status: ToolStatus::Done, output: "fn main() {\n    println!(\"Hello, world\");\n}\n".into(), diff: vec![], open: false, full: false, review: Review::Open }),
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
                    full: false, review: Review::Open,
                }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Bash,
                    target: "cargo test".into(),
                    note: Some("Run the tests".into()),
                    status: ToolStatus::Done,
                    output: "   \x1b[1;32mCompiling\x1b[0m hello v0.1.0 (/home/amine/dev/kryptonite)\n    \x1b[1;32mFinished\x1b[0m `test` profile [unoptimized + debuginfo] target(s) in 1.84s\n     \x1b[1;32mRunning\x1b[0m unittests src/main.rs (target/debug/deps/hello-3f2a9c1e)\n\nrunning 3 tests\ntest tests::greets_by_name ... \x1b[32mok\x1b[0m\ntest tests::greeting_is_static ... \x1b[32mok\x1b[0m\ntest tests::main_prints_the_greeting ... \x1b[32mok\x1b[0m\n\ntest result: \x1b[32mok\x1b[0m. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n   \x1b[1;32mDoc-tests\x1b[0m hello\n\nrunning 0 tests\n\ntest result: \x1b[32mok\x1b[0m. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s".into(),
                    diff: vec![],
                    open: true,
                    full: false, review: Review::Open,
                }),
            ],
        },
        Message::Assistant {
            blocks: vec![Block::Text(
                "Done. The greeting now lives in `greeting()` and `main` prints it.\n\n## What changed\n\n- `src/main.rs`: new **`greeting()`** function, used by `main`.\n- A unit test, *greets_by_name*, checks the text.\n\n```rust\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn greets_by_name() {\n        assert_eq!(super::greeting(), \"Hello, Forge\");\n    }\n}\n```\n\n> Run `cargo test` any time to check it again.\n\n1. Build with `cargo build`.\n2. Run it with `cargo run`.".into(),
            )],
        },
        Message::TurnEnd { seconds: 18.4, usage: None },
        Message::user("Can you also check the hermes review pipeline for unused imports?".into(), minutes_ago(4)),
        Message::Assistant {
            blocks: vec![
                Block::Tool(ToolCall { kind: ToolKind::Web, target: "https://eslint.org/docs/latest/rules/no-unused-vars".into(), note: None, status: ToolStatus::Done, output: "no-unused-vars: disallow unused variables, including imports.".into(), diff: vec![], open: false, full: false, review: Review::Open }),
                Block::Tool(ToolCall { kind: ToolKind::Write, target: "docs/lint-notes.md".into(), note: None, status: ToolStatus::Done, output: "".into(), diff: diff(&[(DiffKind::Add, "# Lint notes"), (DiffKind::Add, ""), (DiffKind::Add, "Run `pnpm lint --filter hermes` before pushing.")]), open: false, full: false, review: Review::Open }),
                Block::Tool(ToolCall { kind: ToolKind::Search, target: "^use .*;$ in src/hermes".into(), note: None, status: ToolStatus::Done, output: "src/hermes/review.ts:3\nsrc/hermes/store.ts:1".into(), diff: vec![], open: false, full: false, review: Review::Open }),
                Block::Tool(ToolCall { kind: ToolKind::Bash, target: "pnpm lint --filter hermes".into(), note: Some("Lint the package".into()), status: ToolStatus::Failed, output: "\x1b[4msrc/hermes/review.ts\x1b[24m\n  \x1b[2m3:10\x1b[22m  \x1b[31merror\x1b[39m  'diffLines' is defined but never used  \x1b[2m@typescript-eslint/no-unused-vars\x1b[22m\n\n\x1b[31m\x1b[1m✖ 1 problem (1 error, 0 warnings)\x1b[22m\x1b[39m\n\x1b[2m ELIFECYCLE \x1b[22m Command failed with exit code 1.".into(), diff: vec![], open: true, full: false, review: Review::Open }),
                Block::Text("One unused import: `diffLines` in `src/hermes/review.ts`. Want me to remove it?".into()),
            ],
        },
        Message::TurnEnd { seconds: 6.1, usage: None },
    ];
    let simple = |title: &str, minutes: u32| {
        vec![
            Message::user(title.to_string().into(), minutes_ago(minutes)),
            Message::Assistant {
                blocks: vec![Block::Text(
                    format!(
                        "This is an older conversation about *{}*.",
                        title.to_lowercase()
                    )
                    .into(),
                )],
            },
            Message::TurnEnd {
                seconds: 3.2,
                usage: None,
            },
        ]
    };
    let done = |mut t: ToolCall| {
        t.status = ToolStatus::Done;
        Block::Tool(t)
    };
    // A reply that wrote a plan: the write, a short note, and the file's card.
    const PLAN: &str = include_str!("../assets/docs/settings-migration-plan.md");
    let plan = vec![
        Message::user("Plan the migration of our settings to the new schema. Write it up so I can review it.".into(), minutes_ago(41)),
        Message::Assistant {
            blocks: vec![
                // The plan it proposed, its task list, and a look at the docs
                // in the browser: the side panel's Plan and Browser tabs.
                done(crate::backend::tool_call("ExitPlanMode", &serde_json::json!({"plan": "## Migrate settings to the v2 schema\n\n1. **Read** `src/settings.rs` and list every field.\n2. **Map** each field to its v2 key; keep unknown keys as they are.\n3. **Back up** `settings.json` to `settings.v1.json` for one release.\n4. **Write** the migration, run it once on start-up, and log what moved.\n5. **Test** a round trip, and a read-only file on Windows.\n\nNothing is deleted: the backup stays until the next release."}))),
                done(crate::backend::tool_call("TodoWrite", &serde_json::json!({"todos": [
                    {"content": "Read the current settings struct", "status": "completed"},
                    {"content": "Map every field to its v2 key", "status": "completed"},
                    {"content": "Write up the migration plan", "status": "completed"},
                    {"content": "Round-trip test", "status": "in_progress"},
                    {"content": "Windows run with a read-only file", "status": "pending"}
                ]}))),
                done(crate::backend::tool_call("mcp__forge-browser__navigate", &serde_json::json!({"url": "https://serde.rs/field-attrs.html"}))),
                done(crate::backend::tool_call("mcp__forge-browser__state", &serde_json::json!({}))),
                done(crate::backend::tool_call("mcp__forge-browser__click", &serde_json::json!({"index": 12}))),
                done(crate::backend::tool_call("mcp__forge-browser__text", &serde_json::json!({"query": "how #[serde(alias)] works"}))),
                Block::Tool(ToolCall { kind: ToolKind::Read, target: "src/settings.rs".into(), note: None, status: ToolStatus::Done, output: "pub struct Settings { pub theme: String, pub model: String, pub permissions: Vec<String> }".into(), diff: vec![], open: false, full: false, review: Review::Open }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Write,
                    target: "docs/settings-migration-plan.md".into(),
                    note: None,
                    status: ToolStatus::Done,
                    output: "".into(),
                    diff: diff(&PLAN.lines().take(6).map(|l| (DiffKind::Add, l)).collect::<Vec<_>>()),
                    open: false,
                    full: false, review: Review::Open,
                }),
                Block::Text("Here is the plan. It moves every setting to the v2 schema without losing anything, and keeps a backup for one release. Two checklist items are still open: the round-trip test and a Windows run with a read-only file.".into()),
                Block::File(FileRef { name: "settings-migration-plan.md".into(), path: Some("docs/settings-migration-plan.md".into()), content: PLAN.into(), bytes: None }),
            ],
        },
        Message::TurnEnd { seconds: 24.7, usage: None },
    ];
    // A reply that drew a diagram with the bundled Diagram Design skill.
    const CHAT_PATH: &str = include_str!("../assets/docs/forge-chat-path.html");
    let drawn = vec![
        Message::user("Draw how a chat in Forge reaches the model, and where the API key gets added.".into(), minutes_ago(13)),
        Message::Assistant {
            blocks: vec![
                Block::Tool(ToolCall {
                    kind: ToolKind::Write,
                    target: "docs/chat-path.html".into(),
                    note: None,
                    status: ToolStatus::Done,
                    output: "".into(),
                    diff: diff(&[(DiffKind::Add, "<!DOCTYPE html>"), (DiffKind::Add, "<title>How a chat reaches a model</title>"), (DiffKind::Add, "<svg viewBox=\"0 0 960 480\" …>")]),
                    open: false,
                    full: false, review: Review::Open,
                }),
                Block::Text("Here it is. The gateway is the one accent: it is the only piece that ever holds a key, and it reads it from Credential Manager on the way out.".into()),
                Block::File(FileRef { name: "chat-path.html".into(), path: Some("docs/chat-path.html".into()), content: CHAT_PATH.into(), bytes: None }),
            ],
        },
        Message::TurnEnd { seconds: 31.2, usage: None },
    ];
    // A reply that edited a workbook and wrote it up with the bundled
    // office-documents skill: the real files its tools made.
    let office = |name: &str, bytes: &'static [u8]| {
        Block::File(FileRef {
            name: name.into(),
            path: Some(format!("finance/{name}").into()),
            content: "".into(),
            bytes: Some(std::sync::Arc::from(bytes)),
        })
    };
    let bash = |command: &str, note: &str, output: &str| {
        Block::Tool(ToolCall {
            kind: ToolKind::Bash,
            target: command.into(),
            note: Some(note.into()),
            status: ToolStatus::Done,
            output: output.into(),
            diff: vec![],
            open: false,
            full: false,
            review: Review::Open,
        })
    };
    let budget = vec![
        Message::user("Licences came in at 700 and travel is forecast at 520. Update the Q3 budget, then write it up as a memo, a short deck and a PDF.".into(), minutes_ago(7)),
        Message::Assistant {
            blocks: vec![
                bash("python xlsx_tool.py read finance/q3-budget.xlsx --sheet Budget --formulas", "Read the budget", "## Budget (7 rows × 4 columns)\n| 3 | Licences | 800 | 640 | =C3-B3 |\n| 6 | Travel | 450 |  | =C6-B6 |\n| 7 | Total | =SUM(B2:B6) | =SUM(C2:C6) | =C7-B7 |"),
                bash("python xlsx_tool.py set finance/q3-budget.xlsx finance/q3-budget.xlsx --cell \"Budget!C3=700\" --cell \"Budget!C6=520\" && python xlsx_tool.py recalc finance/q3-budget.xlsx finance/q3-budget.xlsx", "Update licences and travel, then recalculate", "set 2 cell(s); wrote finance/q3-budget.xlsx\nrecalculated; wrote finance/q3-budget.xlsx"),
                bash("python docx_tool.py create finance/q3-budget-memo.docx --markdown memo.md && python pptx_tool.py create finance/q3-budget.pptx --outline deck.md && python pdf_tool.py convert finance/q3-budget-memo.docx finance/q3-budget.pdf", "Write the memo, the deck and the PDF", "wrote finance/q3-budget-memo.docx\nwrote 4 slide(s) to finance/q3-budget.pptx\nwrote finance/q3-budget.pdf"),
                Block::Text("Done. The Q3 forecast is now **5,500 EUR**, up 350 on Q2:\n\n- `Budget!C3` Licences: 640 → **700**\n- `Budget!C6` Travel: empty → **520**\n\nThe change column and the totals are formulas, so I recalculated the workbook and read it back: Travel is +70 and the total change is +350. The memo, the 4-slide deck and the PDF all say the same.".into()),
                office("q3-budget.xlsx", include_bytes!("../assets/docs/samples/q3-budget.xlsx")),
                office("q3-budget-memo.docx", include_bytes!("../assets/docs/samples/q3-budget-memo.docx")),
                office("q3-budget.pptx", include_bytes!("../assets/docs/samples/q3-budget.pptx")),
                office("q3-budget.pdf", include_bytes!("../assets/docs/samples/q3-budget.pdf")),
            ],
        },
        Message::TurnEnd { seconds: 42.6, usage: None },
    ];
    let with_error = |mut m: Vec<Message>| {
        m.push(Message::Notice {
            text: "The agent stopped unexpectedly (exit code 1). Send a message to try again."
                .into(),
            error: true,
        });
        m
    };
    let mut chats = vec![
        Chat {
            minutes_ago: 4,
            pinned: true,
            messages: rich,
            unread: false,
            ..Chat::new(1, 1, "Fix the greeting and add a test".into())
        },
        Chat {
            minutes_ago: 120,
            pinned: false,
            messages: simple("Explain the hermes review pipeline", 120),
            unread: true,
            ..Chat::new(2, 4, "Explain the hermes review pipeline".into())
        },
        Chat {
            minutes_ago: 300,
            pinned: false,
            messages: simple("Speed up the session store", 300),
            unread: false,
            ..Chat::new(3, 1, "Speed up the session store".into())
        },
        Chat {
            minutes_ago: 1500,
            pinned: false,
            messages: simple("Write a README for the CLI", 1500),
            unread: false,
            ..Chat::new(4, 3, "Write a README for the CLI".into())
        },
        Chat {
            minutes_ago: 1800,
            pinned: false,
            messages: with_error(simple("Why does cargo test hang on Windows?", 1800)),
            unread: false,
            ..Chat::new(5, 1, "Why does cargo test hang on Windows?".into())
        },
        Chat {
            minutes_ago: 40,
            pinned: false,
            messages: plan,
            unread: false,
            ..Chat::new(6, 1, "Migrate settings to the new schema".into())
        },
        Chat {
            minutes_ago: 6,
            pinned: false,
            messages: budget,
            unread: false,
            ..Chat::new(9, 1, "Update the Q3 budget and write it up".into())
        },
        Chat {
            minutes_ago: 12,
            pinned: false,
            messages: drawn,
            unread: false,
            ..Chat::new(8, 1, "Draw how a chat reaches the model".into())
        },
        Chat {
            minutes_ago: 20000,
            pinned: false,
            messages: simple("Genesis redesign: landing page", 20000),
            unread: false,
            ..Chat::new(7, 2, "Genesis redesign: landing page".into())
        },
    ];
    // The mock replies' usage, counted from their text as each turn ended.
    for c in &mut chats {
        for i in 0..c.messages.len() {
            if let Message::TurnEnd {
                usage: None,
                seconds,
            } = c.messages[i]
            {
                c.messages[i] = Message::TurnEnd {
                    seconds,
                    usage: Some(crate::context::estimate(&c.messages[..i])),
                };
            }
        }
    }
    chats.sort_by_key(|c| (!c.pinned, c.minutes_ago));
    chats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_looks_through_every_kind_of_message() {
        let chats = mock_chats();
        let rich = &chats.iter().find(|c| c.id == 1).unwrap().messages;
        // Your message, a tool's target and a reply's prose.
        assert_eq!(find_hits(rich, "HELLO, FORGE"), vec![0, 1, 2]);
        assert_eq!(find_hits(rich, "diffLines"), vec![5]);
        assert!(find_hits(rich, "   ").is_empty());
        assert!(find_hits(rich, "no such words anywhere").is_empty());
    }

    #[test]
    fn a_turns_text_spans_its_replies_and_stops_at_your_message() {
        let chats = mock_chats();
        let rich = &chats.iter().find(|c| c.id == 1).unwrap().messages;
        let first = turn_text(rich, 3);
        assert!(
            first.starts_with("I'll move the greeting")
                && first.contains("Done. The greeting now lives"),
            "{first}"
        );
        assert!(!first.contains("diffLines"));
        let second = turn_text(rich, 6);
        assert!(second.starts_with("One unused import"), "{second}");
        assert_eq!(turn_question(rich, 3), Some(0));
        assert_eq!(turn_question(rich, 6), Some(4));
    }
}
