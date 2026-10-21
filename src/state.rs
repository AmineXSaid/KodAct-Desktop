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
    /// A web search (WebSearch): the query is the target.
    WebSearch,
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
            (ToolKind::WebSearch, true) => "Searching the web for",
            (ToolKind::WebSearch, false) => "Searched the web for",
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
            ToolKind::Web | ToolKind::WebSearch => "icons/globe.svg",
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

/// A command and what it printed, for a read-only tab in the terminal dock.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentFeed {
    pub title: String,
    pub command: String,
    pub output: String,
    /// The remote's label and icon, or None for this computer.
    pub place: Option<(String, &'static str)>,
    /// It failed (exit 1 in its footer).
    pub failed: bool,
}

impl AgentFeed {
    pub fn of(c: &ToolCall, place: Option<(String, &'static str)>) -> AgentFeed {
        AgentFeed {
            title: c
                .note
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "Agent".into()),
            command: c.target.to_string(),
            output: c.output.to_string(),
            place,
            failed: c.status == ToolStatus::Failed,
        }
    }

    /// The bytes a terminal would have seen, dressed as the chat's terminal
    /// block is: the step's description, the prompt with the command in
    /// colour (`terminal::highlight`), its output, and an exit chip with the
    /// line count. ANSI colours, so the dock's palette themes them.
    pub fn bytes(&self) -> Vec<u8> {
        use crate::ui::terminal::{highlight, Tok};
        let mut out = format!("\x1b[2;3m# {}\x1b[0m\r\n", self.title);
        for (i, line) in self.command.lines().enumerate() {
            out.push_str(if i == 0 { "\x1b[1;35m$\x1b[0m " } else { "  " });
            for (tok, text) in highlight(line) {
                let sgr = match tok {
                    Tok::Command => "1;34",
                    Tok::Keyword => "35",
                    Tok::Flag => "36",
                    Tok::Str => "32",
                    Tok::Var | Tok::Num => "33",
                    Tok::Op => "31",
                    Tok::Comment => "2;3",
                    Tok::Plain => "",
                };
                if sgr.is_empty() {
                    out.push_str(&text);
                } else {
                    out.push_str(&format!("\x1b[{sgr}m{text}\x1b[0m"));
                }
            }
            out.push_str("\r\n");
        }
        out.push_str("\r\n");
        let lines = self.output.lines().count();
        for line in self.output.lines() {
            out.push_str(line);
            out.push_str("\r\n");
        }
        let (code, tone) = if self.failed { (1, "31") } else { (0, "32") };
        out.push_str(&format!(
            "\r\n\x1b[1;{tone}mexit {code}\x1b[0m \x1b[2m· {lines} line{}\x1b[0m\r\n",
            if lines == 1 { "" } else { "s" }
        ));
        out.into_bytes()
    }
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
    /// A plan approved, its edits then made without asking.
    PlanAutoAccept,
    /// A plan approved, each edit then asked about.
    PlanApprove,
    /// A plan sent back: keep planning.
    KeepPlanning,
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
        /// What the call was, who answered, when, and where its row is.
        info: PermInfo,
    },
    /// A file Forge made for you: a card in the reply, opened beside the chat.
    File(FileRef),
}

/// Who answered a permission question.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Decider {
    /// You, on the card.
    #[default]
    You,
    /// You, with "Always allow": the rule it added (`Bash(npm test:*)`).
    Rule(SharedString),
    /// Nobody: the reply was stopped while it waited, so it was denied.
    Stopped,
    /// The mode you switched to while it waited (Edit automatically).
    Mode(SharedString),
}

/// What a permission card keeps besides its words, for the row it becomes
/// once answered (`message.rs`).
#[derive(Clone, Debug, Default)]
pub struct PermInfo {
    /// The kind of call it asked about (its icon); None in older chats.
    pub kind: Option<ToolKind>,
    pub by: Decider,
    /// When it was answered.
    pub at: Option<std::time::SystemTime>,
    /// The call's own row in the same message, for its outcome.
    pub row: Option<usize>,
    /// A run of identical answers that starts here shows each one.
    pub open: bool,
    /// While it waits: the rule "Always allow" would add, and where.
    pub offer: Option<SharedString>,
    pub scope: Option<&'static str>,
    /// What you told Forge to do instead, with a denial.
    pub note: Option<SharedString>,
}

/// Where "Always allow" keeps its rule (the suggestion's `destination`).
pub fn rule_scope(suggestions: &serde_json::Value) -> Option<&'static str> {
    let dest = suggestions
        .as_array()?
        .iter()
        .find(|s| s["type"] == "addRules")?["destination"]
        .as_str()?;
    Some(match dest {
        "session" => "for the rest of this chat",
        "localSettings" => "in this project, for you",
        "projectSettings" => "in this project, for everyone",
        "userSettings" => "in every project",
        _ => return None,
    })
}

/// "Always allow", said as what it allows: `Bash(npm test:*)` is
/// "Always allow npm test", `Edit` "Always allow edits".
pub fn always_label(rule: Option<&str>) -> String {
    let Some(rule) = rule else {
        return "Always allow".into();
    };
    let (tool, content) = match rule.split_once('(') {
        Some((t, c)) => (t, c.trim_end_matches(')')),
        None => (rule, ""),
    };
    let content = content
        .trim_end_matches(":*")
        .trim_end_matches('*')
        .trim_start_matches("domain:")
        .trim();
    let what = match (tool, content) {
        ("Edit" | "Write" | "MultiEdit" | "NotebookEdit", "") => "edits".to_string(),
        ("Read", "") => "reading files".into(),
        ("WebSearch", _) => "web searches".into(),
        ("WebFetch", "") => "web pages".into(),
        ("Bash", "") => "commands".into(),
        (_, "") => tool.to_string(),
        (_, c) if c.chars().count() > 28 => {
            format!("{}…", c.chars().take(27).collect::<String>().trim_end())
        }
        (_, c) => c.to_string(),
    };
    format!("Always allow {what}")
}

/// The rule "Always allow" adds, from a question's suggestions
/// (`[{type: addRules, rules: [{toolName, ruleContent}]}]`): `Bash(npm test:*)`.
pub fn rule_name(suggestions: &serde_json::Value) -> Option<String> {
    suggestions
        .as_array()?
        .iter()
        .filter(|s| s["type"] == "addRules")
        .flat_map(|s| s["rules"].as_array().into_iter().flatten())
        .find_map(|r| {
            let tool = r["toolName"].as_str()?;
            Some(match r["ruleContent"].as_str() {
                Some(c) if !c.is_empty() => format!("{tool}({c})"),
                _ => tool.to_string(),
            })
        })
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
    /// You asked Forge to stop it, and Forge hasn't said it has yet.
    pub stopping: bool,
    /// You stopped it; the rest of the turn went on without it.
    pub stopped: bool,
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

/// How ForgeCLI's result for a sub-agent you stopped begins (its
/// `forge_tools::AGENT_STOPPED`).
pub const AGENT_STOPPED: &str = "Stopped by user";

/// What a ForgeCLI too old for `stop_task` is told when asked to stop one agent.
pub const CANT_STOP_AGENT: &str =
    "This Forge can't stop one agent; update it, or press Esc to stop the whole reply.";

/// The tools that start a sub-agent.
pub fn is_agent_tool(name: &str) -> bool {
    matches!(name, "Task" | "Agent")
}

/// The panel's browser as the UI shows it.
#[derive(Default)]
pub struct WebPage {
    /// The latest picture of the page.
    pub frame: Option<std::sync::Arc<gpui::RenderImage>>,
    /// The area that picture shows, in CSS pixels from the page's top left
    /// (0 when the browser didn't say).
    pub frame_area: (f32, f32),
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub tabs: Vec<crate::web_view::TabInfo>,
    /// The tab on screen.
    pub active: String,
    /// Why there is no browser, when there isn't.
    pub error: Option<String>,
    /// A dialog the page shows, waiting for your answer.
    pub dialog: Option<crate::web_view::PageDialog>,
    /// The pointer the page wants under the mouse (CSS name).
    pub cursor: String,
    /// The browser's last finished download, for the panel to show.
    pub last_download: Option<std::path::PathBuf>,
}

/// A page Forge is designing, shown live in the browser.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveDesign {
    pub chat: usize,
    /// The page on disk.
    pub path: std::path::PathBuf,
    /// Forge is writing it now: its draft streaming, or an edit not done.
    pub writing: bool,
    /// Written while you weren't following: the panel still shows an
    /// older draft, or another page, until you ask for it.
    pub unseen: bool,
    /// When this chat's design began: a reply after it starts afresh.
    since: std::time::Instant,
}

impl LiveDesign {
    /// The page's file name, for the badge.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
    }
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

/// The remote indicator's menu (`ui/remote_menu.rs`): which page shows and
/// what discovery found. Lists are None while they load.
#[derive(Clone, Debug, Default)]
pub struct RemoteMenu {
    pub page: RemotePage,
    pub distros: Option<Result<Vec<crate::remote::Distro>, String>>,
    pub hosts: Vec<String>,
    pub containers: Option<Result<Vec<crate::remote::Container>, String>>,
    /// Picking a folder on a target.
    pub folder: Option<RemoteFolder>,
    /// An SSH host's unknown key, waiting for "Trust".
    pub host_key: Option<(crate::remote::Target, String, String)>,
    pub error: Option<SharedString>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RemotePage {
    #[default]
    Root,
    Wsl,
    Ssh,
    Containers,
    Folder,
}

/// A folder being browsed on a target: where, and what's in it (None while
/// it loads).
#[derive(Clone, Debug)]
pub struct RemoteFolder {
    pub target: crate::remote::Target,
    pub path: String,
    pub entries: Option<Result<Vec<crate::remote::Entry>, String>>,
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
    /// follow: the transcript was cut back).
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
    /// Starred: first in the switcher and the Projects page.
    pub pinned: bool,
    /// When it was last opened, in minutes before now.
    pub opened_minutes_ago: u32,
    /// What it is for, in your words (Forge doesn't read it).
    pub description: SharedString,
    /// Archived: off the lists, its chats and files kept.
    pub archived: bool,
    /// When it was made and last opened (seconds since 1970).
    pub created: u64,
    pub opened_at: u64,
    /// Forge keeps a memory for it (`.forge/memory.md`).
    pub memory_on: bool,
    /// On your Projects page. The folder Forge was started on is a
    /// project for that run without joining the list, unless you saved it.
    pub listed: bool,
    /// The model (a profile's name) opening the project picks, and the
    /// mode its new chats start in; None: yours.
    pub model: Option<String>,
    pub default_mode: Option<ModeId>,
    /// You made it a project (Open folder…, Save as project). A remote
    /// folder you connected to is worked in without becoming one.
    pub saved: bool,
    /// Plain chats' place: no folder of yours, Forge's own (`scratch_dir`).
    /// Never listed as a project, never shown in the sidebar's head.
    pub scratch: bool,
}

impl Project {
    /// A folder, as a project now: its name and branch read from it.
    pub fn at(id: usize, path: std::path::PathBuf, saved: bool) -> Self {
        let now = now_secs();
        Project {
            id,
            name: project_name(&path).into(),
            branch: git_branch(&path).into(),
            path,
            pinned: false,
            opened_minutes_ago: 0,
            description: "".into(),
            archived: false,
            created: now,
            opened_at: now,
            memory_on: true,
            listed: saved,
            model: None,
            default_mode: None,
            saved,
            scratch: false,
        }
    }

    /// Its details as `projects.json` keeps them.
    fn record(&self) -> ProjectRecord {
        ProjectRecord {
            path: self.path.clone(),
            name: (self.name.as_ref() != project_name(&self.path)).then(|| self.name.to_string()),
            description: self.description.to_string(),
            starred: self.pinned,
            archived: self.archived,
            created: self.created,
            opened: self.opened_at,
            memory_on: self.memory_on,
            listed: self.listed,
            model: self.model.clone(),
            mode: self.default_mode.map(|m| m.key().to_string()),
        }
    }

    /// The saved details put back on it.
    fn apply(&mut self, r: &ProjectRecord) {
        if let Some(n) = r.name.as_ref().filter(|n| !n.trim().is_empty()) {
            self.name = n.clone().into();
        }
        self.description = r.description.clone().into();
        self.pinned = r.starred;
        self.archived = r.archived;
        if r.created > 0 {
            self.created = r.created;
        }
        if r.opened > 0 {
            self.opened_at = r.opened;
            self.opened_minutes_ago = (now_secs().saturating_sub(r.opened) / 60) as u32;
        }
        self.memory_on = r.memory_on;
        self.listed |= r.listed;
        self.model = r.model.clone();
        self.default_mode = r.mode.as_deref().and_then(ModeId::from_key);
    }
}

/// What a turn changed in its project's memory.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryChange {
    pub project: std::path::PathBuf,
    pub before: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// One project in `projects.json`: everything Forge keeps about it besides
/// the folder's own files.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct ProjectRecord {
    path: std::path::PathBuf,
    /// A name you gave it (None: the folder's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    starred: bool,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    created: u64,
    #[serde(default)]
    opened: u64,
    #[serde(default = "yes")]
    memory_on: bool,
    #[serde(default = "yes")]
    listed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
}

fn yes() -> bool {
    true
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
    /// Started with Expert's output style, which is fixed for a process.
    expert: bool,
    /// Relays carrying the remote's ports back here (WSL, containers):
    /// they end with the session.
    _relays: Vec<std::sync::Arc<crate::relay::Relay>>,
}

/// A permission question Forge is waiting on, and where its card is.
struct Ask {
    request_id: String,
    tool_use_id: String,
    tool_name: String,
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
pub(crate) fn demo_mode() -> bool {
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
    /// Stop these sub-agents (every running one when you chose "Stop all").
    StopAgents { chat: usize, ids: Vec<String> },
    /// Take back a queued message; `edit` puts it back in the box.
    CancelQueued { chat: usize, uuid: String, edit: bool },
}

/// The queued messages a replayed message of yours carries, taken out of
/// `list`: the one with its id (delivered mid-turn), else those its words
/// hold (they ran as a turn of their own, alone or joined, under ForgeCLI's id).
pub fn take_landed(list: &mut Vec<Queued>, uuid: &str, text: &str) -> Vec<Queued> {
    if let Some(i) = list.iter().position(|q| q.uuid == uuid) {
        return vec![list.remove(i)];
    }
    let (hit, keep) = std::mem::take(list)
        .into_iter()
        .partition(|q| !q.text.trim().is_empty() && text.contains(q.text.trim()));
    *list = keep;
    hit
}

/// A message you sent while a reply ran, waiting for Forge's next step.
#[derive(Clone, Debug, PartialEq)]
pub struct Queued {
    pub uuid: String,
    pub text: SharedString,
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
    /// The current project's folder; plain chats' place when it is none.
    pub project_dir: std::path::PathBuf,
    /// The folder Forge was started on, if any (`launch_dir`).
    pub launch_dir: Option<std::path::PathBuf>,
    /// Remote targets ForgeCLI was put on this run (`remote.rs`).
    remote_ready: std::collections::HashMap<crate::remote::Target, crate::remote::Provisioned>,
    /// A connection being made: the target and what it is doing now.
    pub remote_busy: Option<(crate::remote::Target, SharedString)>,
    /// The current remote stopped answering: how many reconnection tries so
    /// far (the banner and the indicator say "Reconnecting…").
    pub remote_lost: Option<(crate::remote::Target, u32)>,
    /// Checks the current remote answers, every 15 s (`remote_heartbeat`).
    remote_heartbeat: Option<Task<()>>,
    /// The last connection that failed, and why (the indicator turns red).
    pub remote_failed: Option<(crate::remote::Target, SharedString)>,
    /// SSH hosts whose new key the person agreed to trust.
    trusted_hosts: std::collections::HashSet<String>,
    /// The remote indicator's menu, while it's open.
    pub remote_menu: Option<RemoteMenu>,
    /// WSL folders found to have a dev container definition, or not
    /// (checked in the background when one opens, or the menu does).
    wsl_devcontainers: std::collections::HashMap<std::path::PathBuf, bool>,
    /// The browser's tool server over HTTP, for remote agents: (port, token).
    browser_http: Option<(u16, String)>,
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
    /// The theme picked in Settings (light or dark is `dark`).
    pub look: crate::theme::Look,
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
    /// The page Forge is designing: the browser shows it as it is written
    /// (`design::live`).
    pub design_live: Option<LiveDesign>,
    /// Whether the panel follows Forge's design. Off once you take the
    /// browser yourself, or stop it; on again at the next reply.
    pub design_follow: bool,
    /// Each chat's sub-agents, in the order they started (`agents_view.rs`).
    pub agents: std::collections::HashMap<usize, Vec<AgentRun>>,
    /// Agent cards opened to show their steps, by the agent's id.
    pub agents_open: std::collections::HashSet<String>,
    /// A diagram in the panel fits its width (else it shows at actual size).
    pub doc_fit: bool,
    /// The side panel sits below the chat instead of beside it.
    pub doc_below: bool,
    /// An Office file or PDF in the panel, read: by file name and content
    /// (`name#key`), the preview or why it couldn't be read. None while it
    /// is being read.
    pub doc_preview: Option<(
        SharedString,
        Result<std::sync::Arc<crate::office::Preview>, String>,
    )>,
    /// The sheet shown, for a workbook.
    pub doc_sheet: usize,
    /// A document in the panel shows its text, not its pages.
    pub doc_text: bool,
    /// The cells picked in a workbook in the panel (anchor, other end).
    pub doc_pick: crate::ui::sheet_view::Pick,
    /// The panel shows what its version changed from the one before.
    pub doc_show_changes: bool,
    /// Comments on files in the panel, as Claude Docs keeps them: each
    /// on a passage, a page or cells, until it is sent and answered.
    pub doc_comments: Vec<crate::doc_versions::Comment>,
    /// Files you attached and opened in the panel, as they came (by chat):
    /// each is its file's first version, before Forge edits the copy.
    doc_originals: Vec<(usize, FileRef)>,
    /// The project page's brief (`project_ctx.rs`): what every chat there
    /// gets, read when the page opens. None while it is read, or for a
    /// remote project (its files are on the host).
    pub project_brief: Option<(usize, std::sync::Arc<crate::project_ctx::Brief>)>,
    /// A chat's project memory as its turn began (`project_memory.rs`), to
    /// tell what the turn changed; and what the latest turn changed, shown
    /// under the reply with Undo. Kept for this run only.
    memory_before: std::collections::HashMap<usize, (std::path::PathBuf, String)>,
    pub memory_changes: std::collections::HashMap<usize, MemoryChange>,
    /// The project page's repository (`git_status.rs`): None while it is
    /// read; Some(None) when the folder isn't a repository.
    pub repo: Option<(usize, std::sync::Arc<Option<crate::git_status::Repo>>)>,
    /// Bumped each time the repository is read, so a stale refresh stops.
    repo_reads: u64,
    /// The Projects page's New project sheet is open.
    pub new_project_open: bool,
    /// The chat menu shows the projects to move the chat to.
    pub chat_menu_moving: bool,
    /// The panel's find bar is open (`ui/doc_find.rs`).
    pub doc_finding: bool,
    /// The panel's body: its pages are its items, so the keyboard and a
    /// search result can bring one to the top.
    pub doc_scroll: gpui::ScrollHandle,
    next_comment: usize,
    /// A page (or slide) of the panel's document picked to comment on, from 0.
    pub doc_page_pick: Option<usize>,
    /// Sent comments on a Word file are also written into it as Word comments.
    pub doc_word_comments: bool,
    /// Those changes, for the version they belong to (`doc_version_key`);
    /// None inside while they could not be worked out.
    pub doc_changes: Option<(SharedString, Option<crate::doc_versions::Changes>)>,
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
    /// What Forge is doing about your latest message in each chat, in a few
    /// words the model wrote (`title::headline`), for the working line.
    pub headlines: std::collections::HashMap<usize, String>,
    /// Messages sent while each chat's reply runs, oldest first, until they
    /// reach Forge (`Event::UserAccepted`).
    pub queued: std::collections::HashMap<usize, Vec<Queued>>,
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
    /// The terminal dock is showing on the right (its shells live on while
    /// it is hidden).
    pub terminal_open: bool,
    /// The dock's share of the window's width.
    pub terminal_share: f32,
    /// The dock's left edge is being dragged.
    pub terminal_resizing: bool,
    /// Each remote chat's model route in the proxy (`model_proxy.rs`), by
    /// chat: replaced when its session restarts.
    model_routes: std::collections::HashMap<usize, String>,
    /// A relay that couldn't listen on the remote says why here.
    relay_errors: async_channel::Sender<String>,
    /// SSH prompts waiting on you (a password, a key's passphrase); the
    /// first one shows as a dialog (`ui/askpass.rs`).
    pub askpass: std::collections::VecDeque<crate::askpass::Request>,
    /// A command's output to show in the dock ("Show in terminal"): its
    /// title, the text, and where it ran.
    pub agent_feed: Option<AgentFeed>,
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
    /// What the find bar searches for when it next opens (the palette found
    /// it in this chat).
    pub find_request: Option<String>,
    /// For `/` completion: the project's own commands (`.forge/commands`).
    custom_commands: Option<(std::path::PathBuf, Vec<crate::commands::Command>)>,
    /// The skills `/` offers, read with the custom commands.
    skill_commands: Vec<crate::commands::Command>,
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
        let launch = launch_dir();
        let scratch = scratch_dir();
        // Made at start, so ForgeCLI has a folder to run in; a failure shows
        // when a chat starts.
        let _ = std::fs::create_dir_all(&scratch);
        let projects = initial_projects(launch.as_deref(), &scratch);
        let first = projects
            .iter()
            .find(|p| launch.is_some() != p.scratch)
            .cloned()
            .unwrap_or_else(|| projects[0].clone());
        let project_dir = first.path.clone();
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
            project: first.name.clone(),
            project_dir,
            launch_dir: launch,
            remote_ready: Default::default(),
            remote_busy: None,
            remote_failed: None,
            remote_lost: None,
            remote_heartbeat: None,
            trusted_hosts: Default::default(),
            remote_menu: None,
            wsl_devcontainers: Default::default(),
            browser_http: None,
            branch: first.branch.clone(),
            current_project: first.id,
            projects,
            all_chats: true,
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
            dark: crate::app_settings::load_window().dark,
            look: crate::theme::Look::from_id(&crate::app_settings::load_window().theme)
                .unwrap_or_default(),
            palette_open: false,
            picker_open: false,
            copied: None,
            doc: None,
            panel: None,
            web: None,
            web_page: WebPage::default(),
            design_live: None,
            design_follow: true,
            agents: std::collections::HashMap::new(),
            agents_open: std::collections::HashSet::new(),
            doc_fit: true,
            doc_below: false,
            doc_preview: None,
            doc_text: false,
            doc_pick: None,
            doc_show_changes: false,
            doc_comments: vec![],
            doc_originals: vec![],
            doc_finding: false,
            project_brief: None,
            memory_before: Default::default(),
            memory_changes: Default::default(),
            repo: None,
            repo_reads: 0,
            new_project_open: false,
            chat_menu_moving: false,
            doc_scroll: gpui::ScrollHandle::new(),
            next_comment: 1,
            doc_page_pick: None,
            doc_word_comments: true,
            doc_changes: None,
            doc_sheet: 0,
            copied_reset: None,
            toast: None,
            toast_reset: None,
            replies: Default::default(),
            sessions: Default::default(),
            stale_turns: Default::default(),
            tool_rows: Default::default(),
            asks: Default::default(),
            headlines: Default::default(),
            queued: Default::default(),
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
            terminal_open: crate::app_settings::load_window().terminal_open,
            terminal_share: crate::app_settings::load_window().terminal_share,
            terminal_resizing: false,
            agent_feed: None,
            askpass: Default::default(),
            model_routes: Default::default(),
            relay_errors: async_channel::unbounded().0,
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
            find_request: None,
            custom_commands: None,
            skill_commands: vec![],
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
        // A relay that couldn't start: the toast says why.
        let (relay_tx, relay_rx) = async_channel::unbounded::<String>();
        self.relay_errors = relay_tx;
        cx.spawn(async move |this, cx| {
            while let Ok(m) = relay_rx.recv().await {
                if this.update(cx, |s, cx| s.show_toast(m, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // SSH passwords and passphrases are asked here, not in a terminal.
        let (ask_tx, ask_rx) = async_channel::unbounded::<crate::askpass::Request>();
        if crate::askpass::start(ask_tx).is_ok() {
            cx.spawn(async move |this, cx| {
                while let Ok(r) = ask_rx.recv().await {
                    let gone = this
                        .update(cx, |s, cx| {
                            s.askpass.push_back(r);
                            cx.notify();
                        })
                        .is_err();
                    if gone {
                        break;
                    }
                }
            })
            .detach();
        }
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
                    // A remote folder's chats come back without making it a project.
                    let saved = crate::remote::Remote::parse(&c.project).is_none();
                    self.projects.push(Project {
                        name: name.into(),
                        opened_minutes_ago: 60,
                        ..Project::at(id, c.project.clone(), saved)
                    });
                    added_project |= saved;
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
            self.save_projects(cx);
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
        p.opened_at = now_secs();
        let keep = p.saved && !p.scratch;
        let (name, path, branch) = (p.name.clone(), p.path.clone(), p.branch.clone());
        self.current_project = id;
        self.project = name;
        self.project_dir = path;
        self.branch = branch;
        self.switcher_open = false;
        if keep {
            self.save_projects(cx);
        }
        // The project's model, when it has one you still have.
        let model = self.project(id).and_then(|p| p.model.clone());
        if let Some(m) = model {
            if self.active_profile().is_some_and(|p| p.name != m) {
                self.select_profile(&m, cx);
            }
        }
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
        self.load_project_brief(id, cx);
        self.load_repo(id, cx);
        cx.notify();
    }

    /// Read project `id`'s repository off the UI thread, then again every
    /// minute while its page is open.
    pub fn load_repo(&mut self, id: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .project(id)
            .filter(|p| crate::remote::Remote::parse(&p.path).is_none())
            .map(|p| p.path.clone())
        else {
            self.repo = None;
            return;
        };
        if self.repo.as_ref().is_some_and(|(r, _)| *r != id) {
            self.repo = None;
        }
        self.repo_reads += 1;
        let read = self.repo_reads;
        cx.spawn(async move |this, cx| {
            let p = path.clone();
            let repo = cx
                .background_spawn(async move { crate::git_status::read(&p) })
                .await;
            let still = this
                .update(cx, |s, cx| {
                    if s.repo_reads != read {
                        return false;
                    }
                    if let Some(r) = &repo {
                        // The branch shown everywhere follows the repository.
                        if let Some(p) = s.projects.iter_mut().find(|p| p.id == id) {
                            p.branch = r.branch.clone().into();
                        }
                        if s.current_project == id {
                            s.branch = r.branch.clone().into();
                        }
                    }
                    s.repo = Some((id, std::sync::Arc::new(repo)));
                    cx.notify();
                    s.screen == Screen::Project(id)
                })
                .unwrap_or(false);
            if still {
                cx.background_executor().timer(Duration::from_secs(60)).await;
                let _ = this.update(cx, |s, cx| {
                    if s.repo_reads == read && s.screen == Screen::Project(id) {
                        s.load_repo(id, cx);
                    }
                });
            }
        })
        .detach();
    }

    /// Run a git change in project `id` off the UI thread, say how it went,
    /// and read the repository again.
    fn repo_change(
        &mut self,
        id: usize,
        change: impl FnOnce(&std::path::Path) -> Result<String, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.project(id).map(|p| p.path.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let done = cx.background_spawn(async move { change(&path) }).await;
            let _ = this.update(cx, |s, cx| {
                match done {
                    Ok(note) => s.show_toast(note, cx),
                    Err(e) => s.show_toast(format!("Git said: {e}"), cx),
                }
                s.load_repo(id, cx);
            });
        })
        .detach();
    }

    pub fn repo_pull(&mut self, id: usize, cx: &mut Context<Self>) {
        self.show_toast("Pulling…", cx);
        self.repo_change(id, crate::git_status::pull, cx);
    }

    pub fn repo_switch(&mut self, id: usize, branch: String, create: bool, cx: &mut Context<Self>) {
        self.repo_change(
            id,
            move |p| {
                crate::git_status::switch(p, &branch, create).map(|_| {
                    if create {
                        format!("Made {branch} and switched to it.")
                    } else {
                        format!("Switched to {branch}.")
                    }
                })
            },
            cx,
        );
    }

    /// Project `id`'s model and mode (None: yours).
    pub fn set_project_defaults(
        &mut self,
        id: usize,
        model: Option<String>,
        mode: Option<ModeId>,
        cx: &mut Context<Self>,
    ) {
        self.edit_project(id, cx, |p| {
            p.model = model.clone();
            p.default_mode = mode;
        });
        if let Some(m) = model {
            if self.current_project == id {
                self.select_profile(&m, cx);
            }
        }
        if self.current.is_none() {
            self.sync_mode();
        }
    }

    /// Run a sharing step for project `id` off the UI thread, say how it
    /// went, and read the page again.
    fn share_step(
        &mut self,
        id: usize,
        step: impl FnOnce(&std::path::Path) -> Result<String, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .project(id)
            .filter(|p| crate::remote::Remote::parse(&p.path).is_none())
            .map(|p| p.path.clone())
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let done = cx.background_spawn(async move { step(&path) }).await;
            let _ = this.update(cx, |s, cx| {
                match done {
                    Ok(note) => s.show_toast(note, cx),
                    Err(e) => s.show_toast(format!("Couldn't share it: {e}"), cx),
                }
                s.load_project_brief(id, cx);
                s.load_repo(id, cx);
            });
        })
        .detach();
    }

    /// Commit project `id`'s setup with its repository.
    pub fn share_commit(&mut self, id: usize, memory: bool, cx: &mut Context<Self>) {
        self.share_step(id, move |p| crate::project_share::commit(p, memory), cx);
    }

    /// Export project `id`'s setup as a `.forgeproject` in Downloads.
    pub fn share_export(&mut self, id: usize, memory: bool, cx: &mut Context<Self>) {
        let Some(dir) = crate::downloads::dir() else {
            return self.show_toast("Could not find your Downloads folder.", cx);
        };
        let Some(p) = self.project(id) else { return };
        let (name, description) = (p.name.to_string(), p.description.to_string());
        self.share_step(
            id,
            move |path| {
                crate::project_share::export(path, &name, &description, memory, &dir).map(|f| {
                    format!(
                        "Saved to Downloads as {}. Whoever imports it gets this project's setup.",
                        f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                    )
                })
            },
            cx,
        );
    }

    /// Import a `.forgeproject` into project `id`.
    pub fn share_import(&mut self, id: usize, file: std::path::PathBuf, cx: &mut Context<Self>) {
        self.share_step(
            id,
            move |path| {
                crate::project_share::import(&file, path).map(|r| match (r.added.len(), r.kept.len()) {
                    (0, 0) => "That setup was empty.".to_string(),
                    (a, 0) => format!("Imported {a} file{}.", if a == 1 { "" } else { "s" }),
                    (a, k) => format!(
                        "Imported {a} file{}; kept {k} this project already had ({}).",
                        if a == 1 { "" } else { "s" },
                        r.kept.join(", ")
                    ),
                })
            },
            cx,
        );
    }

    /// Open the Projects page with its New project sheet.
    pub fn open_new_project(&mut self, cx: &mut Context<Self>) {
        self.switcher_open = false;
        self.screen = Screen::Projects;
        self.new_project_open = true;
        cx.notify();
    }

    /// New project › Start fresh: a folder in `Forge Projects` made from a
    /// template, opened as a project.
    pub fn create_project(&mut self, name: String, template: &'static str, cx: &mut Context<Self>) {
        let (Some(home), Some(folder)) = (
            crate::templates::projects_home(),
            crate::templates::folder_name(&name),
        ) else {
            return self.show_toast("Give the project a name.", cx);
        };
        let dir = home.join(folder);
        let t = crate::templates::by_key(template);
        let d = dir.clone();
        let n = name.clone();
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move { crate::templates::create(&d, &n, t) })
                .await;
            let _ = this.update(cx, |s, cx| match made {
                Ok(()) => {
                    s.new_project_open = false;
                    s.add_project(dir.clone(), cx);
                    if let Some(id) = s.projects.iter().find(|p| p.path == dir).map(|p| p.id) {
                        s.rename_project(id, &name, cx);
                        if !t.description.is_empty() {
                            s.describe_project(id, t.description, cx);
                        }
                    }
                }
                Err(e) => s.show_toast(format!("Couldn't make the project: {e}"), cx),
            });
        })
        .detach();
    }

    /// New project › Clone from GitHub: `git clone` into `Forge Projects`,
    /// then open it as a project.
    pub fn clone_project(&mut self, url: String, cx: &mut Context<Self>) {
        let url = url.trim().to_string();
        let Some(dir) = crate::templates::projects_home()
            .and_then(|h| crate::templates::clone_target(&h, &url))
        else {
            return self.show_toast("Paste a repository's address, like https://github.com/owner/name.", cx);
        };
        if dir.exists() {
            // Already cloned: open it.
            self.new_project_open = false;
            return self.add_project(dir, cx);
        }
        self.show_toast(format!("Cloning into {}…", dir.display()), cx);
        let d = dir.clone();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move {
                    if let Some(parent) = d.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    let mut c = std::process::Command::new("git");
                    c.args(["clone", "--", &url])
                        .arg(&d)
                        .env("GIT_TERMINAL_PROMPT", "0")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::piped());
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                    }
                    let out = c.output().map_err(|e| match e.kind() {
                        std::io::ErrorKind::NotFound => "git isn't installed".to_string(),
                        _ => e.to_string(),
                    })?;
                    if out.status.success() {
                        Ok(())
                    } else {
                        Err(String::from_utf8_lossy(&out.stderr)
                            .lines()
                            .rfind(|l| !l.trim().is_empty())
                            .unwrap_or("git clone failed")
                            .trim()
                            .to_string())
                    }
                })
                .await;
            let _ = this.update(cx, |s, cx| match done {
                Ok(()) => {
                    s.new_project_open = false;
                    s.add_project(dir, cx);
                }
                Err(e) => s.show_toast(format!("Couldn't clone it: {e}"), cx),
            });
        })
        .detach();
    }

    /// Move chat `chat` into project `to`. Its next message runs in that
    /// folder; if the engine can't reopen the conversation there, it says
    /// so and goes on as a new conversation.
    pub fn move_chat(&mut self, chat: usize, to: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        self.chat_menu_moving = false;
        if self.replies.contains_key(&chat) {
            return self.show_toast("Wait for the reply to finish, then move the chat.", cx);
        }
        let Some(name) = self.project(to).map(|p| p.name.clone()) else {
            return;
        };
        self.sessions.remove(&chat);
        if let Some(c) = self.chat_mut(chat) {
            c.project = to;
            c.version += 1;
        }
        if self.current == Some(chat) {
            self.select_project(to, cx);
            self.current = Some(chat);
        }
        self.touch(chat, cx);
        self.show_toast(format!("Moved to {name}."), cx);
        cx.notify();
    }

    /// Start a chat in project `id` asking Forge about something (an issue,
    /// a pull request, a failing run).
    pub fn ask_forge(&mut self, id: usize, text: String, cx: &mut Context<Self>) {
        self.select_project(id, cx);
        self.new_chat(cx);
        self.send(text, cx);
    }

    /// The local folder chat `id` works in (None for a remote one).
    fn chat_project_dir(&self, id: usize) -> Option<std::path::PathBuf> {
        let project = self.chats.iter().find(|c| c.id == id)?.project;
        self.project(project)
            .filter(|p| crate::remote::Remote::parse(&p.path).is_none())
            .map(|p| p.path.clone())
    }

    /// Read what every chat in project `id` gets (off the UI thread).
    pub fn load_project_brief(&mut self, id: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .project(id)
            .filter(|p| crate::remote::Remote::parse(&p.path).is_none())
            .map(|p| p.path.clone())
        else {
            self.project_brief = None;
            return;
        };
        if self.project_brief.as_ref().is_some_and(|(b, _)| *b != id) {
            self.project_brief = None;
        }
        let budget = self.knowledge_budget();
        let memory_on = self.project(id).is_some_and(|p| p.memory_on);
        cx.spawn(async move |this, cx| {
            let brief = cx
                .background_spawn(async move { crate::project_ctx::read(&path, budget, memory_on) })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.project_brief = Some((id, std::sync::Arc::new(brief)));
                cx.notify();
            });
        })
        .detach();
    }

    /// What a project's Always knowledge may spend: a share of the window
    /// of the model picked now.
    fn knowledge_budget(&self) -> u64 {
        self.active_profile()
            .and_then(|p| p.context_window(self.health.get(&p.name)))
            .map_or(crate::knowledge::DEFAULT_BUDGET, |w| {
                (w as f32 * crate::knowledge::SHARE) as u64
            })
    }

    /// Change project `id`'s knowledge off the UI thread (`knowledge.rs`),
    /// then read the brief again; a failure says why.
    fn knowledge_change(
        &mut self,
        id: usize,
        change: impl FnOnce(&std::path::Path) -> Result<String, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .project(id)
            .filter(|p| crate::remote::Remote::parse(&p.path).is_none())
            .map(|p| p.path.clone())
        else {
            return self.show_toast("Knowledge is for projects on this computer, for now.", cx);
        };
        cx.spawn(async move |this, cx| {
            let done = cx.background_spawn(async move { change(&path) }).await;
            let _ = this.update(cx, |s, cx| {
                match done {
                    Ok(note) if !note.is_empty() => s.show_toast(note, cx),
                    Ok(_) => {}
                    Err(e) => s.show_toast(format!("Couldn't change the knowledge: {e}"), cx),
                }
                s.load_project_brief(id, cx);
            });
        })
        .detach();
    }

    /// Add files to project `id`'s knowledge.
    pub fn knowledge_add(&mut self, id: usize, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        self.knowledge_change(
            id,
            move |p| {
                crate::knowledge::add(p, &paths).map(|n| match n {
                    0 => "Those files are in the knowledge already.".into(),
                    1 => "Added to the knowledge, On demand: Forge reads it when it matters.".into(),
                    n => format!("Added {n} files to the knowledge, On demand."),
                })
            },
            cx,
        );
    }

    /// Add a note you wrote to project `id`'s knowledge.
    pub fn knowledge_add_text(&mut self, id: usize, title: String, text: String, cx: &mut Context<Self>) {
        self.knowledge_change(
            id,
            move |p| crate::knowledge::add_text(p, &title, &text).map(|_| String::new()),
            cx,
        );
    }

    pub fn knowledge_set_always(&mut self, id: usize, rel: String, always: bool, cx: &mut Context<Self>) {
        self.knowledge_change(
            id,
            move |p| crate::knowledge::set_always(p, &rel, always).map(|_| String::new()),
            cx,
        );
    }

    pub fn knowledge_remove(&mut self, id: usize, rel: String, cx: &mut Context<Self>) {
        self.knowledge_change(
            id,
            move |p| crate::knowledge::remove(p, &rel).map(|_| String::new()),
            cx,
        );
    }

    /// Save project `id`'s instructions: shared (its FORGE.md) or yours
    /// only (FORGE.local.md). Empty removes the file.
    pub fn save_instructions(&mut self, id: usize, mine: bool, text: String, cx: &mut Context<Self>) {
        let Some((_, brief)) = self.project_brief.as_ref().filter(|(b, _)| *b == id) else {
            return;
        };
        let path = if mine {
            brief.mine_path.clone()
        } else {
            brief.shared_path.clone()
        };
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move { crate::project_ctx::write(&path, &text) })
                .await;
            let _ = this.update(cx, |s, cx| match done {
                Ok(()) => {
                    s.show_toast(
                        if mine {
                            "Saved your instructions. Every new chat here follows them."
                        } else {
                            "Saved the project's instructions. Every new chat here follows them."
                        },
                        cx,
                    );
                    s.load_project_brief(id, cx);
                }
                Err(e) => s.show_toast(format!("Couldn't save the instructions: {e}"), cx),
            });
        })
        .detach();
    }

    /// Add a folder as a project (or open it, if it already is one).
    pub fn add_project(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let id = self.place_folder(path, true);
        self.save_projects(cx);
        self.show_project(id, cx);
    }

    /// Work in a folder without making it a project (a remote one you
    /// connected to): the window moves there, to a new chat, and the
    /// folder is listed under "Connected", not under your projects.
    pub fn work_in_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let id = self.place_folder(path, false);
        self.select_project(id, cx);
        self.screen = Screen::Chat;
        self.current = None;
        cx.notify();
    }

    /// The project for `path`, added when there is none; `save` makes an
    /// unsaved one a project too.
    fn place_folder(&mut self, path: std::path::PathBuf, save: bool) -> usize {
        if let Some(p) = self.projects.iter_mut().find(|p| p.path == path) {
            p.saved |= save;
            p.listed |= save;
            p.archived &= !save;
            return p.id;
        }
        let id = self.projects.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        self.projects.push(Project::at(id, path, save));
        id
    }

    /// Save as project: a folder you were only connected to joins your projects.
    pub fn save_project(&mut self, id: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
            p.saved = true;
            p.listed = true;
        }
        self.save_projects(cx);
        cx.notify();
    }

    /// Remove from projects: the folder leaves your list (its chats stay,
    /// under All projects). The folder Forge started in always stays.
    pub fn forget_project(&mut self, id: usize, cx: &mut Context<Self>) {
        if self
            .project(id)
            .is_some_and(|p| p.scratch || Some(&p.path) == self.launch_dir.as_ref())
        {
            return;
        }
        let has_chats = self.chats.iter().any(|c| c.project == id);
        if has_chats || id == self.current_project {
            if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
                p.saved = false;
                p.listed = false;
                p.pinned = false;
            }
        } else {
            self.projects.retain(|p| p.id != id);
        }
        self.save_projects(cx);
        if self.screen == Screen::Project(id) {
            self.screen = Screen::Projects;
        }
        cx.notify();
    }

    /// Write your projects to disk: the ones on your list, and the folder
    /// Forge started on (its details, without listing it). Plain chats'
    /// place and folders you only connected to aren't kept.
    fn save_projects(&self, cx: &mut Context<Self>) {
        let records: Vec<ProjectRecord> = self
            .projects
            .iter()
            .filter(|p| p.saved && !p.scratch)
            .map(|p| p.record())
            .collect();
        cx.background_spawn(async move { save_projects(&records) })
            .detach();
    }

    /// Star a project, or take its star away.
    pub fn toggle_project_pin(&mut self, id: usize, cx: &mut Context<Self>) {
        self.edit_project(id, cx, |p| p.pinned = !p.pinned);
    }

    /// Change a project's details and keep them.
    fn edit_project(&mut self, id: usize, cx: &mut Context<Self>, f: impl FnOnce(&mut Project)) {
        let Some(p) = self.projects.iter_mut().find(|p| p.id == id) else {
            return;
        };
        f(p);
        if self.current_project == id {
            self.project = p.name.clone();
        }
        self.save_projects(cx);
        cx.notify();
    }

    /// Give a project a name of your own (empty: the folder's again).
    pub fn rename_project(&mut self, id: usize, name: &str, cx: &mut Context<Self>) {
        let name = name.trim().to_string();
        self.edit_project(id, cx, |p| {
            p.name = if name.is_empty() {
                project_name(&p.path).into()
            } else {
                name.into()
            }
        });
    }

    pub fn describe_project(&mut self, id: usize, text: &str, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        self.edit_project(id, cx, |p| p.description = text.into());
    }

    /// Archive a project (off the lists, everything kept), or bring it back.
    pub fn archive_project(&mut self, id: usize, archived: bool, cx: &mut Context<Self>) {
        self.edit_project(id, cx, |p| {
            p.archived = archived;
            if archived {
                p.pinned = false;
            }
        });
        if archived && self.screen == Screen::Project(id) {
            self.screen = Screen::Projects;
        }
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

    /// Plain chats' place.
    pub fn scratch_project(&self) -> Option<usize> {
        self.projects.iter().find(|p| p.scratch).map(|p| p.id)
    }

    /// The current project is a folder of yours, not plain chats' place.
    pub fn in_folder(&self) -> bool {
        self.project(self.current_project)
            .is_some_and(|p| !p.scratch)
    }

    /// New chat, as a chat app does it: a plain chat with no folder. A
    /// project's page starts one in that project (`new_chat`).
    pub fn new_plain_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.scratch_project() {
            self.select_project(id, cx);
        }
        self.new_chat(cx);
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
        // A new chat starts in its project's mode, when it has one.
        let start = self
            .project(self.current_project)
            .and_then(|p| p.default_mode)
            .unwrap_or(self.default_mode);
        self.mode = self
            .current
            .and_then(|id| self.kept_modes.get(&id).copied())
            .unwrap_or_else(|| crate::modes::initial(start, self.bypass_allowed));
    }

    fn keep_mode(&mut self) {
        let Some(id) = self.current else { return };
        if self.mode.kept(self.bypass_allowed) {
            self.kept_modes.insert(id, self.mode);
        } else {
            self.kept_modes.remove(&id);
        }
    }

    /// A chat's mode as ForgeCLI now runs it (it changed it itself, or a
    /// plan's answer did): the mode chip follows, and the mode is kept for
    /// the chat's next session. Expert stays Expert while the CLI is in its
    /// own default mode.
    fn mode_reported(&mut self, chat: usize, mode: ModeId) {
        let current_here = if self.current == Some(chat) {
            self.mode
        } else {
            self.kept_modes.get(&chat).copied().unwrap_or(self.mode)
        };
        let mode = if mode == ModeId::Manual && current_here == ModeId::Expert {
            ModeId::Expert
        } else {
            mode
        };
        if self.current == Some(chat) {
            self.mode = mode;
            self.keep_mode();
        } else if mode.kept(self.bypass_allowed) {
            self.kept_modes.insert(chat, mode);
        } else {
            self.kept_modes.remove(&chat);
        }
    }

    /// The mode a chat's ForgeCLI runs in: the showing chat's, or what
    /// another chat kept.
    fn chat_mode(&self, chat: usize) -> ModeId {
        if self.current == Some(chat) {
            self.mode
        } else {
            self.kept_modes.get(&chat).copied().unwrap_or(self.mode)
        }
    }

    /// Whether Forge answers a permission request itself (`approve`): a
    /// command that only reads, in any mode but Bypass, or that only edits
    /// project files, in Edit automatically. Deletions always ask.
    fn auto_approves(&self, chat: usize, tool: &str, input: &serde_json::Value) -> bool {
        let (working_directory, home_directory) = match self.chat_remote(chat) {
            // Paths starting `~` there never count as inside the project.
            Some(r) => (r.path, String::new()),
            None => {
                let Some(dir) = self
                    .chats
                    .iter()
                    .find(|c| c.id == chat)
                    .and_then(|c| self.project(c.project))
                    .map(|p| p.path.to_string_lossy().into_owned())
                else {
                    return false;
                };
                let home = crate::rules::home_dir()
                    .map(|h| h.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (dir, home)
            }
        };
        crate::approve::auto_approves_command(&crate::approve::AutoApproveInput {
            tool_name: tool.to_string(),
            command: input["command"].as_str().map(str::to_string),
            permission_mode: Some(self.chat_mode(chat).permission().cli_name().to_string()),
            working_directory,
            home_directory,
            enabled: crate::app_settings::get().auto_approve_safe_commands,
        })
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
            // A question already waiting that the new mode allows is
            // answered by it, as if it had been asked now.
            if let Some(chat) = self.current {
                if let Some((msg, block)) = self.allowed_by_mode(chat) {
                    self.answer(chat, msg, block, Verdict::Allowed, cx);
                    self.set_decider(chat, msg, block, Decider::Mode(mode.label().into()));
                }
            }
        }
        cx.notify();
    }

    /// The question `chat` waits on, when its mode now allows it without
    /// asking: an edit in Edit automatically, anything with permissions
    /// bypassed, a command the mode lets run (`auto_approves`).
    fn allowed_by_mode(&self, chat: usize) -> Option<(usize, usize)> {
        let ask = self.asks.get(&chat)?;
        let mode = self.chat_mode(chat);
        let edit = matches!(
            ask.tool_name.as_str(),
            "Edit" | "MultiEdit" | "Write" | "NotebookEdit"
        );
        let allowed = match mode.permission().cli_name() {
            "bypassPermissions" => ask.tool_name != "ExitPlanMode",
            "acceptEdits" if edit => true,
            _ => self.auto_approves(chat, &ask.tool_name, &ask.input),
        };
        allowed.then_some((ask.msg, ask.block))
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
        self.show_doc(file, cx);
    }

    /// Open a Word, Excel, PowerPoint or PDF file you attached in the side
    /// panel, as one Forge wrote: the same preview, comments and versions.
    /// It points at the project's copy (what Forge edits); the file you
    /// picked is read for its first version. Anything else opens in its app.
    pub fn open_attachment(&mut self, a: crate::attach::Attachment, cx: &mut Context<Self>) {
        use crate::attach::Kind;
        let ext = a
            .name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        let viewable = matches!(a.kind, Kind::Pdf | Kind::Document(_))
            && matches!(ext.as_str(), "docx" | "xlsx" | "pptx" | "pdf");
        let Some(chat) = self.current else { return };
        let project = self
            .chat()
            .and_then(|c| self.project(c.project))
            .map(|p| p.path.clone())
            .unwrap_or_else(|| self.project_dir.clone());
        let copy = a.saved_at.as_ref().map(|rel| project.join(rel));
        if !viewable {
            match copy.as_ref().filter(|p| p.exists()).or(a.path.as_ref()) {
                Some(p) => cx.open_with_system(p),
                None => self.show_toast("This attachment has no file to open.", cx),
            }
            return;
        }
        let Some(at) = copy.clone().or_else(|| a.path.clone()) else {
            return;
        };
        let source = a.path.clone();
        let name = a.name.to_string();
        let read = cx.background_spawn(async move {
            let original = source
                .as_ref()
                .and_then(|p| std::fs::read(p).ok())
                .or_else(|| std::fs::read(&at).ok());
            let first = original
                .clone()
                .and_then(|b| file_card(name.clone(), ext.clone(), at.display().to_string(), b));
            // The copy now: the same file, or Forge's edit of it.
            let now = std::fs::read(&at)
                .ok()
                .and_then(|b| file_card(name, ext, at.display().to_string(), b));
            (first, now)
        });
        cx.spawn(async move |this, cx| {
            let (first, now) = read.await;
            let _ = this.update(cx, |s, cx| match first {
                Some(first) => {
                    if !s
                        .doc_originals
                        .iter()
                        .any(|(c, f)| *c == chat && crate::doc_versions::same_file(f, &first))
                    {
                        s.doc_originals.push((chat, first.clone()));
                    }
                    s.doc = None;
                    s.open_doc(now.unwrap_or(first), cx);
                }
                None => s.show_toast("Couldn't read that file any more.", cx),
            });
        })
        .detach();
    }

    /// The panel's file's versions in this chat (oldest first) and which
    /// one it shows.
    pub fn doc_versions(&self) -> (Vec<FileRef>, usize) {
        let (Some(file), Some(chat)) = (&self.doc, self.chat()) else {
            return (vec![], 0);
        };
        let mut all = crate::doc_versions::versions(chat, file);
        // An attached file you opened: as it came is its first version.
        if let Some((_, first)) = self
            .doc_originals
            .iter()
            .find(|(c, f)| *c == chat.id && crate::doc_versions::same_file(f, file))
        {
            if all.first() != Some(first) {
                all.insert(0, first.clone());
            }
        }
        let at = all
            .iter()
            .position(|v| v == file)
            .unwrap_or(all.len().saturating_sub(1));
        (all, at)
    }

    /// Step to the version `by` away (-1 the one before, +1 the one after).
    pub fn step_doc_version(&mut self, by: isize, cx: &mut Context<Self>) {
        let (all, at) = self.doc_versions();
        let to = at as isize + by;
        if let Some(v) = (to >= 0).then(|| all.get(to as usize)).flatten() {
            self.show_doc(v.clone(), cx);
        }
    }

    /// The panel's file's comments, oldest first.
    pub fn doc_comments_here(&self) -> Vec<crate::doc_versions::Comment> {
        let Some(file) = &self.doc else {
            return vec![];
        };
        let id = crate::doc_versions::file_id(file);
        self.doc_comments
            .iter()
            .filter(|c| c.file == id)
            .cloned()
            .collect()
    }

    /// Comment on `anchor` in the panel's file (this version).
    pub fn add_doc_comment(
        &mut self,
        anchor: crate::doc_versions::Anchor,
        note: String,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = &self.doc else { return };
        if note.trim().is_empty() {
            return;
        }
        let version = self.doc_versions().1;
        self.doc_comments.push(crate::doc_versions::Comment {
            id: self.next_comment,
            file: crate::doc_versions::file_id(file),
            version,
            anchor,
            note: note.trim().to_string(),
            sent: None,
        });
        self.next_comment += 1;
        self.doc_page_pick = None;
        self.doc_pick = None;
        cx.notify();
    }

    pub fn remove_doc_comment(&mut self, id: usize, cx: &mut Context<Self>) {
        self.doc_comments.retain(|c| c.id != id);
        cx.notify();
    }

    /// Send the panel's file's open comments to Forge as one message: each
    /// one's place, quote and request. A later version of the file answers
    /// them.
    pub fn send_doc_comments(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.doc.clone() else { return };
        if self.busy() {
            return self.show_toast("Wait for the reply to finish, then send your comments.", cx);
        }
        let id = crate::doc_versions::file_id(&file);
        let open: Vec<_> = self
            .doc_comments
            .iter()
            .filter(|c| c.file == id && c.sent.is_none())
            .cloned()
            .collect();
        if open.is_empty() {
            return;
        }
        let word = self.doc_word_comments && file.office() == Some(crate::office::Kind::Docx);
        let text = crate::doc_versions::comments_message(&file, &open, word);
        let versions = self.doc_versions().0.len();
        for c in self
            .doc_comments
            .iter_mut()
            .filter(|c| c.file == id && c.sent.is_none())
        {
            c.sent = Some(versions);
        }
        self.send(text, cx);
    }

    /// One comment, sent at once: "Ask Forge to change this".
    pub fn ask_doc_change(
        &mut self,
        anchor: crate::doc_versions::Anchor,
        note: String,
        cx: &mut Context<Self>,
    ) {
        let unsent: Vec<usize> = self
            .doc_comments_here()
            .iter()
            .filter(|c| c.sent.is_none())
            .map(|c| c.id)
            .collect();
        self.add_doc_comment(anchor, note, cx);
        // Only this one goes now; the others wait for Send.
        let held: Vec<_> = self
            .doc_comments
            .iter_mut()
            .filter(|c| unsent.contains(&c.id))
            .map(|c| {
                c.sent = Some(usize::MAX);
                c.id
            })
            .collect();
        self.send_doc_comments(cx);
        for c in self
            .doc_comments
            .iter_mut()
            .filter(|c| held.contains(&c.id))
        {
            c.sent = None;
        }
    }

    pub fn pick_doc_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.doc_page_pick = (self.doc_page_pick != Some(page)).then_some(page);
        cx.notify();
    }

    pub fn toggle_word_comments(&mut self, cx: &mut Context<Self>) {
        self.doc_word_comments = !self.doc_word_comments;
        cx.notify();
    }

    /// A reply wrote the file open in the panel again: the panel moves to
    /// the new version, as Claude's does.
    fn follow_doc(&mut self, chat: usize, file: &FileRef, cx: &mut Context<Self>) {
        let open = self
            .doc
            .as_ref()
            .is_some_and(|d| crate::doc_versions::same_file(d, file) && d != file);
        if open && self.current == Some(chat) {
            self.show_doc(file.clone(), cx);
        }
    }

    /// Show what this version changed, or the version itself.
    pub fn toggle_doc_changes(&mut self, cx: &mut Context<Self>) {
        self.doc_show_changes = !self.doc_show_changes;
        if self.doc_show_changes {
            self.load_doc_changes(cx);
        }
        cx.notify();
    }

    /// Work out what the panel's version changed from the one before, off
    /// the UI thread.
    fn load_doc_changes(&mut self, cx: &mut Context<Self>) {
        let (all, at) = self.doc_versions();
        let (Some(new), Some(old)) = (
            all.get(at).cloned(),
            at.checked_sub(1).and_then(|i| all.get(i).cloned()),
        ) else {
            self.doc_changes = None;
            return;
        };
        let key = doc_version_key(&new);
        if self.doc_changes.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        self.doc_changes = None;
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move { crate::doc_versions::changes(&old, &new) })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.doc_changes = Some((key, found));
                cx.notify();
            });
        })
        .detach();
    }

    /// Open `file` in the panel, as it is (a version, or a file anew).
    fn show_doc(&mut self, file: FileRef, cx: &mut Context<Self>) {
        self.doc_sheet = 0;
        self.doc_pick = None;
        if let (Some(_), Some(bytes)) = (file.office(), file.bytes.clone()) {
            let key: SharedString =
                format!("{}#{:x}", file.name, crate::office_render::key(&bytes)).into();
            let ready = self.doc_preview.as_ref().is_some_and(|(k, _)| *k == key);
            if !ready {
                self.doc_preview = None;
                let name = file.name.clone();
                cx.spawn(async move |this, cx| {
                    let (n, b) = (name.to_string(), bytes.clone());
                    let read = cx
                        .background_spawn(async move { crate::office::preview(&n, &b) })
                        .await;
                    let drawing = matches!(&read, Ok(crate::office::Preview::Doc(d)) if d.drawing);
                    let _ = this.update(cx, |s, cx| {
                        s.doc_preview = Some((key.clone(), read.map(std::sync::Arc::new)));
                        cx.notify();
                    });
                    if !drawing {
                        return;
                    }
                    // Then its pages as they print, once LibreOffice has drawn them.
                    let n = name.to_string();
                    let drawn = cx
                        .background_spawn(async move { crate::office::draw(&n, &bytes) })
                        .await;
                    let _ = this.update(cx, |s, cx| {
                        if let Some((k, Ok(p))) = &mut s.doc_preview {
                            if *k != key {
                                return;
                            }
                            if let crate::office::Preview::Doc(d) = std::sync::Arc::make_mut(p) {
                                d.drawing = false;
                                if let Ok((pages, total)) = drawn {
                                    (d.pages, d.total) = (pages, total);
                                }
                            }
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }
        self.doc = Some(file);
        if self.doc_show_changes {
            self.load_doc_changes(cx);
        }
        cx.notify();
    }

    /// Pages as they print, or the text: the panel's view of a document.
    /// Open or close the find bar on the panel's file.
    pub fn toggle_doc_find(&mut self, cx: &mut Context<Self>) {
        if self.doc.is_none() {
            self.show_toast("Open a file beside the chat to find in it.", cx);
            return;
        }
        self.doc_finding = !self.doc_finding;
        cx.notify();
    }

    /// The panel's document's pages (or slides) as drawn; 0 while they are
    /// not, or for a workbook.
    pub fn doc_page_count(&self) -> usize {
        match self.doc_preview.as_ref().map(|(_, r)| r) {
            Some(Ok(p)) => match p.as_ref() {
                crate::office::Preview::Doc(d) => d.pages.len(),
                _ => 0,
            },
            _ => 0,
        }
    }

    /// Bring page `i` (from 0) of the panel's document to the top, showing
    /// the pages if the text was shown.
    pub fn show_doc_page(&mut self, i: usize, cx: &mut Context<Self>) {
        let n = self.doc_page_count();
        if n == 0 {
            return;
        }
        self.doc_text = false;
        self.doc_show_changes = false;
        self.doc_scroll.scroll_to_top_of_item(i.min(n - 1));
        cx.notify();
    }

    /// The page `by` away from the one at the top (-1 back, +1 on).
    pub fn step_doc_page(&mut self, by: isize, cx: &mut Context<Self>) {
        let n = self.doc_page_count();
        if n == 0 || self.doc_text {
            return;
        }
        let at = self.doc_scroll.top_item().min(n - 1) as isize;
        self.show_doc_page((at + by).clamp(0, n as isize - 1) as usize, cx);
    }

    /// Save the panel's Word file, deck or workbook as a PDF in Downloads,
    /// laid out by LibreOffice as it prints.
    pub fn export_pdf(&mut self, file: &FileRef, cx: &mut Context<Self>) {
        use crate::office::Kind;
        match file.office() {
            Some(Kind::Pdf) => return self.download(file, cx),
            Some(_) => {}
            None => return self.show_toast("Only Word, Excel and PowerPoint files export to PDF.", cx),
        }
        if crate::office_render::soffice().is_none() {
            self.show_toast("Exporting to PDF needs LibreOffice. Install it, then try again.", cx);
            return;
        }
        let Some(dir) = crate::downloads::dir() else {
            self.show_toast("Could not find your Downloads folder.", cx);
            return;
        };
        let (name, bytes) = (file.name.to_string(), file.data());
        let pdf_name = format!(
            "{}.pdf",
            name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem)
        );
        self.show_toast(format!("Exporting {pdf_name}…"), cx);
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_spawn(async move {
                    let pdf = crate::office_render::to_pdf(&name, &bytes)?;
                    crate::downloads::save(&dir, &pdf_name, &pdf).map_err(|e| e.to_string())
                })
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
                Err(e) => s.show_toast(format!("Could not export it: {e}"), cx),
            });
        })
        .detach();
    }

    pub fn toggle_doc_text(&mut self, cx: &mut Context<Self>) {
        self.doc_text = !self.doc_text;
        cx.notify();
    }

    /// Pick a cell in the workbook in the panel; `extend` (Shift) stretches
    /// the pick from its anchor to here.
    pub fn pick_cell(&mut self, at: (usize, usize), extend: bool, cx: &mut Context<Self>) {
        self.doc_pick = match (self.doc_pick, extend) {
            (Some((anchor, _)), true) => Some((anchor, at)),
            _ => Some((at, at)),
        };
        cx.notify();
    }

    pub fn select_sheet(&mut self, i: usize, cx: &mut Context<Self>) {
        self.doc_pick = None;
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

    // ------------------------------------------------------------ the live design

    /// A page in this chat's project, on this computer: the live view shows
    /// only those (a remote chat's pages are on its host).
    fn design_path(&self, chat: usize, path: &str) -> Option<std::path::PathBuf> {
        if !crate::design::live::is_page(path) || self.chat_remote(chat).is_some() {
            return None;
        }
        let p = std::path::Path::new(path);
        if p.is_absolute() {
            return Some(p.to_path_buf());
        }
        let project = self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| self.project(c.project))
            .map(|p| p.path.clone())
            .unwrap_or_else(|| self.project_dir.clone());
        Some(project.join(p))
    }

    /// Forge starts on a page: the design this chat shows live. A design
    /// from an earlier reply starts following again.
    fn design_begin(&mut self, chat: usize, path: std::path::PathBuf) {
        let turn = self.turn_started.get(&chat).copied();
        let earlier = self
            .design_live
            .as_ref()
            .filter(|l| l.chat == chat && turn.is_none_or(|t| l.since >= t));
        let since = match earlier {
            Some(l) => l.since,
            None => {
                self.design_follow = true;
                std::time::Instant::now()
            }
        };
        self.design_live = Some(LiveDesign {
            chat,
            path,
            writing: true,
            unseen: false,
            since,
        });
    }

    /// Whether the live view acts for this chat now: it is on screen, and
    /// you haven't taken the browser.
    fn design_shown(&self, chat: usize) -> bool {
        self.design_follow && self.current == Some(chat) && self.design_live.as_ref().is_some_and(|l| l.chat == chat)
    }

    /// The browser comes to the panel, unless a document you opened is there.
    fn design_panel(&mut self) {
        if self.doc.is_none() {
            self.panel = Some(PanelTab::Browser);
        }
    }

    /// A page streaming in: shown as far as it has got.
    fn design_draft(&mut self, chat: usize, path: &str, content: &str, cx: &mut Context<Self>) {
        let Some(full) = self.design_path(chat, path) else {
            return;
        };
        self.design_begin(chat, full.clone());
        if self.design_shown(chat) && self.ensure_web(cx).is_some() {
            self.design_panel();
            let dir = full.parent().unwrap_or(&full);
            if let Some(w) = &self.web {
                w.send(crate::web_view::Cmd::Design {
                    key: crate::design::live::file_url(&full),
                    draft: Some(crate::design::live::draft_page(content, dir)),
                });
            }
        }
        cx.notify();
    }

    /// A tool call on a page: a Write's whole content is its last draft; an
    /// edit shows the page it changes.
    fn design_call(&mut self, chat: usize, name: &str, input: &serde_json::Value, cx: &mut Context<Self>) {
        let path = input["file_path"].as_str().unwrap_or_default();
        match name {
            "Write" => {
                if let Some(content) = input["content"].as_str() {
                    self.design_draft(chat, path, content, cx);
                }
            }
            "Edit" | "MultiEdit" => {
                if let Some(full) = self.design_path(chat, path) {
                    self.design_begin(chat, full);
                    self.design_load(chat, cx);
                }
            }
            _ => {}
        }
    }

    /// The page is written (or the call failed): the browser loads the file
    /// itself, reloading it when it shows it already, which keeps the scroll
    /// and a deck's slide.
    fn design_done(&mut self, chat: usize, path: &str, cx: &mut Context<Self>) {
        let Some(full) = self.design_path(chat, path) else {
            return;
        };
        let unseen = !self.design_follow;
        if let Some(l) = self.design_live.as_mut().filter(|l| l.chat == chat && l.path == full) {
            l.writing = false;
            l.unseen = unseen;
        }
        self.design_load(chat, cx);
    }

    /// Load the live page's file, when it exists, in its tab.
    fn design_load(&mut self, chat: usize, cx: &mut Context<Self>) {
        let Some(full) = self.design_live.as_ref().map(|l| l.path.clone()) else {
            return;
        };
        if self.design_shown(chat) && full.is_file() && self.ensure_web(cx).is_some() {
            self.design_panel();
            if let Some(w) = &self.web {
                w.send(crate::web_view::Cmd::Design {
                    key: crate::design::live::file_url(&full),
                    draft: None,
                });
            }
        }
        cx.notify();
    }

    /// A page's draft is shown from a temporary file: the address bar and
    /// the tab name the page itself.
    fn design_named(&self, url: String, title: String) -> (String, String) {
        use crate::design::live::{draft_file, file_url, page_key};
        let Some(l) = &self.design_live else {
            return (url, title);
        };
        let key = file_url(&l.path);
        if page_key(&url) != file_url(&draft_file(&key)) {
            return (url, title);
        }
        let title = if title.is_empty() || title.starts_with("forge-draft-") {
            l.name()
        } else {
            title
        };
        (key, title)
    }

    /// Follow Forge's design again (the badge's Show), or stop: you take
    /// the browser yourself, or press Stop following.
    pub fn design_set_follow(&mut self, on: bool, cx: &mut Context<Self>) {
        self.design_follow = on;
        if let Some(l) = self.design_live.as_mut().filter(|_| on) {
            l.unseen = false;
        }
        if let Some(chat) = self.design_live.as_ref().map(|l| l.chat).filter(|_| on) {
            self.design_load(chat, cx);
        }
        cx.notify();
    }

    /// The design the badge tells of: the chat on screen's, while it
    /// replies and Forge writes it.
    pub fn designing(&self) -> Option<&LiveDesign> {
        self.design_live
            .as_ref()
            .filter(|l| l.writing && self.current == Some(l.chat) && self.replies.contains_key(&l.chat))
    }

    /// The chat on screen's design, written while you weren't following:
    /// the badge offers to show it.
    pub fn design_ready(&self) -> Option<&LiveDesign> {
        self.design_live
            .as_ref()
            .filter(|l| !l.writing && l.unseen && self.current == Some(l.chat))
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
                    stopping: false,
                    stopped: false,
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
                // A run you stopped here (no engine, in the demo) has already ended.
                if let Some(r) = runs.iter_mut().find(|r| &r.id == id && !r.stopped) {
                    r.ended = Some(now);
                    r.stopping = false;
                    r.stopped = output.starts_with(AGENT_STOPPED);
                    r.failed = *is_error && !r.stopped;
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
                // An ended run takes no more steps (one you stopped may still report one).
                if let Some(r) = runs
                    .iter_mut()
                    .find(|r| &r.id == parent && r.ended.is_none())
                {
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
                if let Some(r) = runs
                    .iter_mut()
                    .find(|r| &r.id == parent && r.ended.is_none())
                {
                    r.said = text.clone();
                }
            }
            // A turn that ends with agents still going stopped them.
            Event::TurnEnd { .. } | Event::Exited { .. } => {
                for r in runs.iter_mut().filter(|r| r.ended.is_none()) {
                    r.ended = Some(now);
                    r.stopped = r.stopping;
                    r.failed = !r.stopping;
                    r.stopping = false;
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
            Update::Frame(img, area) => {
                self.web_page.frame_area = area;
                // The last picture leaves the GPU's atlas as the next arrives.
                if let Some(old) = self.web_page.frame.replace(img) {
                    cx.drop_image(old, None);
                }
            }
            Update::Page { url, title } => {
                let (url, title) = self.design_named(url, title);
                self.web_page.url = url;
                self.web_page.title = title;
            }
            Update::Tabs(tabs, on) => {
                self.web_page.tabs = tabs
                    .into_iter()
                    .map(|mut t| {
                        (t.url, t.title) = self.design_named(t.url, t.title);
                        t
                    })
                    .collect();
                self.web_page.active = on;
            }
            Update::Loading(l) => self.web_page.loading = l,
            Update::Dialog(d) => self.web_page.dialog = d,
            Update::Cursor(c) => {
                if c == self.web_page.cursor {
                    return;
                }
                self.web_page.cursor = c;
            }
            Update::Copied(text) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                self.show_toast("Copied from the page".to_string(), cx);
            }
            Update::Downloaded(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.show_toast(format!("Downloaded {name} to your Downloads folder"), cx);
                self.web_page.last_download = Some(path);
            }
            Update::DownloadFailed(name) => {
                self.show_toast(format!("The download of {name} did not finish"), cx);
            }
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
        self.doc_finding = false;
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
        let remote = self
            .current_remote()
            .map(|r| (self.remote_extra(&r.target), r));
        cx.spawn(async move |this, cx| {
            let d = dir.clone();
            let (files, commands, skills) = cx
                .background_spawn(async move {
                    match remote {
                        // A remote project's files come over the connection;
                        // its skills are ForgeCLI's own.
                        Some((extra, r)) => (
                            crate::remote::project_files(&r.target, &r.path, 20_000, &extra)
                                .map(crate::rules::entries_of)
                                .unwrap_or_default(),
                            vec![],
                            crate::commands::bundled_skills().collect(),
                        ),
                        None => (
                            crate::rules::project_entries(&d, 20_000),
                            crate::commands::custom(&d),
                            crate::commands::skills(&d),
                        ),
                    }
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.completions_loading = false;
                s.file_index = Some((dir.clone(), std::sync::Arc::new(files)));
                s.custom_commands = Some((dir, commands));
                s.skill_commands = skills;
                cx.notify();
            });
        })
        .detach();
    }

    /// The project's files, as `@` completes them (folders end in `/`),
    /// once read; [`Self::ensure_completions`] reads them.
    pub fn project_file_index(&self) -> Option<std::sync::Arc<Vec<String>>> {
        self.file_index
            .as_ref()
            .filter(|(d, _)| *d == self.project_dir)
            .map(|(_, f)| f.clone())
    }

    /// Open a project file (its path in the project, `/`-separated): a
    /// document in the side panel, anything else in the app the system
    /// opens it with. Read off the UI thread, over the connection when the
    /// project is remote.
    pub fn open_project_file(&mut self, rel: String, cx: &mut Context<Self>) {
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        let remote = self
            .current_remote()
            .map(|r| (self.remote_extra(&r.target), r));
        let local = self.project_dir.join(&rel);
        let read = cx.background_spawn(async move {
            match remote {
                Some((extra, r)) => {
                    let full = format!("{}/{rel}", r.path.trim_end_matches('/'));
                    crate::remote::read_file(&r.target, &full, 64 << 20, &extra)
                        .map(|b| (full, b, false))
                        .map_err(|e| e.to_string())
                }
                None => std::fs::read(&local)
                    .map(|b| (local.display().to_string(), b, true))
                    .map_err(|e| e.to_string()),
            }
        });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            let _ = this.update(cx, |s, cx| match read {
                Ok((full, bytes, is_local)) => {
                    let doc = matches!(
                        ext.as_str(),
                        "md" | "markdown"
                            | "svg"
                            | "html"
                            | "htm"
                            | "docx"
                            | "xlsx"
                            | "pptx"
                            | "pdf"
                    );
                    match doc.then(|| file_card(name.clone(), ext, full.clone(), bytes.clone())) {
                        Some(Some(file)) => {
                            s.doc = None;
                            s.open_doc(file, cx);
                        }
                        _ if is_local => cx.open_with_system(std::path::Path::new(&full)),
                        _ => {
                            let file = FileRef {
                                name: name.into(),
                                path: Some(full.into()),
                                content: "".into(),
                                bytes: Some(std::sync::Arc::from(bytes.into_boxed_slice())),
                            };
                            s.open_externally(&file, cx);
                        }
                    }
                }
                Err(e) => s.show_toast(format!("Could not open {name}: {e}"), cx),
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
                self.skill_commands
                    .iter()
                    .cloned()
                    .map(|c| CompletionCommand {
                        name: c.name,
                        args: c.args,
                        description: c.description,
                        group: "Skills",
                    }),
            )
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
        if let Request::StopAgents { chat, ids } = req {
            return self.on_stop_reply(chat, &ids, result, cx);
        }
        if let Request::CancelQueued { chat, uuid, edit } = req {
            match result {
                Ok(v) if v["cancelled"] == true => self.unqueue(chat, &uuid, edit, cx),
                Ok(_) => self.show_toast("Already with Forge: it reached the model before it could be taken back.", cx),
                Err(e) => self.show_toast(format!("Couldn't take it back: {e}"), cx),
            }
            return;
        }
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
            (Request::StopAgents { .. } | Request::CancelQueued { .. }, _) => {}
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
        self.chat_menu_moving = false;
        cx.notify();
    }

    /// Where chat `chat` can move: your other projects, and plain chats.
    pub fn move_targets(&self, chat: usize) -> Vec<(usize, SharedString)> {
        let here = self.chats.iter().find(|c| c.id == chat).map(|c| c.project);
        self.projects
            .iter()
            .filter(|p| Some(p.id) != here && !p.archived && (p.saved || p.scratch))
            .map(|p| {
                let name = if p.scratch { "Plain chats".into() } else { p.name.clone() };
                (p.id, name)
            })
            .collect()
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
    /// Ask the model for chat `id`'s title (off the UI thread). It replaces
    /// the first-words name `provisional`, unless the chat was renamed.
    fn name_chat(
        &mut self,
        id: usize,
        profile: crate::endpoints::Profile,
        first: String,
        provisional: String,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let named = cx
                .background_spawn(async move { crate::title::ask(&profile, &first) })
                .await;
            let title = match named {
                Ok(t) => t,
                Err(e) => return eprintln!("forge: chat {id} keeps its name: {e}"),
            };
            let _ = this.update(cx, |s, cx| {
                let still = s
                    .chats
                    .iter()
                    .any(|c| c.id == id && c.title.as_ref() == provisional);
                if still {
                    if let Some(c) = s.chat_mut(id) {
                        c.title = title.into();
                        c.version += 1;
                    }
                    s.touch(id, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Ask the model, apart from the chat, what Forge is doing about
    /// `message`; kept while that reply still runs.
    fn ask_headline(
        &mut self,
        id: usize,
        profile: crate::endpoints::Profile,
        message: String,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let said = cx
                .background_spawn(async move { crate::title::headline(&profile, &message) })
                .await;
            let _ = this.update(cx, |s, cx| match said {
                Ok(h) if s.replies.contains_key(&id) && !s.headlines.contains_key(&id) => {
                    s.headlines.insert(id, h);
                    cx.notify();
                }
                Ok(_) => {}
                Err(e) => eprintln!("forge: no headline for chat {id}: {e}"),
            });
        })
        .detach();
    }

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
                    Some(Block::Permission { info, .. }) => info.open = !info.open,
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

    /// Who answered a permission question, when it wasn't you on the card.
    fn set_decider(&mut self, chat: usize, msg: usize, block: usize, by: Decider) {
        if let Some(c) = self.chat_mut(chat) {
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                if let Some(Block::Permission { info, .. }) = blocks.get_mut(block) {
                    info.by = by;
                }
            }
            c.version += 1;
        }
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
        self.answer_with(chat, msg, block, verdict, None, cx)
    }

    /// Deny the question `chat` waits on, telling Forge what to do instead
    /// (you typed it in the composer while the card waited).
    pub fn deny_with_note(&mut self, chat: usize, note: String, cx: &mut Context<Self>) {
        if let Some((msg, block)) = self.pending_question(chat) {
            self.answer_with(chat, msg, block, Verdict::Denied, Some(note), cx);
        }
    }

    /// The chat you're looking at waits on your answer to a question.
    pub fn asking(&self) -> bool {
        self.busy() && self.current.and_then(|c| self.pending_question(c)).is_some()
    }

    /// Deny the question the chat you're looking at waits on, with `note`.
    pub fn deny_current_with_note(&mut self, note: String, cx: &mut Context<Self>) {
        if let Some(chat) = self.current {
            self.deny_with_note(chat, note, cx);
        }
    }

    // ------------------------------------------------------------ messages sent while Forge works

    /// Send `text` while the reply runs: shown as queued, and given to the
    /// model at its next step (between tool calls), or run next when the
    /// reply ends first (`backend::Session::queue`).
    pub fn queue_message(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let uuid = format!(
            "forge-queued-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        if !demo_mode() {
            let sent = self
                .sessions
                .get(&chat)
                .ok_or_else(|| "Forge isn't running".to_string())
                .and_then(|l| l.session.queue(&text, &uuid));
            if let Err(e) = sent {
                return self.show_toast(format!("Couldn't send it: {e}"), cx);
            }
        }
        self.queued.entry(chat).or_default().push(Queued {
            uuid,
            text: text.into(),
        });
        cx.notify();
    }

    /// Take a queued message back (`edit`: into the box, to change it).
    /// Only once ForgeCLI confirms it hadn't reached the model yet.
    pub fn remove_queued(&mut self, chat: usize, uuid: String, edit: bool, cx: &mut Context<Self>) {
        if demo_mode() || !self.sessions.contains_key(&chat) {
            return self.unqueue(chat, &uuid, edit, cx);
        }
        let sent = self
            .sessions
            .get(&chat)
            .map(|l| l.session.cancel_queued(&uuid));
        match sent {
            Some(Ok(id)) => {
                self.requests.insert(id, Request::CancelQueued { chat, uuid, edit });
            }
            Some(Err(e)) => self.show_toast(format!("Couldn't take it back: {e}"), cx),
            None => {}
        }
    }

    /// The newest queued message of the chat you're looking at, back in the box.
    pub fn edit_last_queued(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        if let Some(q) = self.queued.get(&chat).and_then(|q| q.last()).cloned() {
            self.remove_queued(chat, q.uuid, true, cx);
        }
    }

    /// Every queued message of the chat you're looking at, taken back.
    pub fn remove_all_queued(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        for q in self.queued.get(&chat).cloned().unwrap_or_default() {
            self.remove_queued(chat, q.uuid, false, cx);
        }
    }

    fn unqueue(&mut self, chat: usize, uuid: &str, edit: bool, cx: &mut Context<Self>) {
        let Some(list) = self.queued.get_mut(&chat) else { return };
        let Some(i) = list.iter().position(|q| q.uuid == uuid) else { return };
        let q = list.remove(i);
        if edit {
            self.draft = Some(q.text.to_string());
        }
        cx.notify();
    }

    /// Stopped, or Forge ended: what was still queued goes back in the box,
    /// in order, so nothing you wrote is lost.
    fn restore_queued(&mut self, chat: usize, cx: &mut Context<Self>) {
        let Some(list) = self.queued.remove(&chat).filter(|l| !l.is_empty()) else {
            return;
        };
        if let Some(l) = self.sessions.get(&chat) {
            for q in &list {
                let _ = l.session.cancel_queued(&q.uuid);
            }
        }
        if self.current == Some(chat) {
            let joined = list.iter().map(|q| q.text.to_string()).collect::<Vec<_>>().join("\n\n");
            self.draft = Some(joined);
        }
        cx.notify();
    }

    /// A replayed message of yours: the queued ones it carries reach the
    /// transcript here, as your messages, where the model read them. Returns
    /// whether it was one of them.
    fn land_queued(&mut self, chat: usize, uuid: &str, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(list) = self.queued.get_mut(&chat) else { return false };
        let landed = take_landed(list, uuid, text);
        if landed.is_empty() {
            return false;
        }
        // A turn of its own: the chat is replying again.
        if !self.replies.contains_key(&chat) {
            self.replies.insert(chat, Task::ready(()));
            self.turn_started.insert(chat, std::time::Instant::now());
        }
        if let Some(c) = self.chat_mut(chat) {
            for q in landed {
                c.messages.push(Message::User {
                    text: q.text,
                    sent: SystemTime::now(),
                    attachments: vec![],
                    turn: Some(uuid.to_string().into()),
                });
            }
            c.version += 1;
        }
        self.mark_dirty(chat, cx);
        cx.notify();
        true
    }

    /// The demo's next step: its queued messages land, as ForgeCLI's would.
    fn land_queued_demo(&mut self, chat: usize, cx: &mut Context<Self>) -> bool {
        let Some(list) = self.queued.get(&chat).filter(|l| !l.is_empty()) else {
            return false;
        };
        let text = list.iter().map(|q| q.text.to_string()).collect::<Vec<_>>().join("\n\n");
        self.land_queued(chat, "demo", &text, cx)
    }

    /// The question `chat` waits on: (message, block).
    pub fn pending_question(&self, chat: usize) -> Option<(usize, usize)> {
        if let Some(a) = self.asks.get(&chat) {
            return Some((a.msg, a.block));
        }
        let c = self.chats.iter().find(|c| c.id == chat)?;
        let msg = c.messages.len().checked_sub(1)?;
        match &c.messages[msg] {
            Message::Assistant { blocks } => blocks
                .iter()
                .rposition(|b| matches!(b, Block::Permission { verdict: Verdict::Pending, .. }))
                .map(|b| (msg, b)),
            _ => None,
        }
    }

    fn answer_with(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        verdict: Verdict,
        note: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(c) = self.chat_mut(chat) {
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                if let Some(Block::Permission {
                    verdict: v, info, ..
                }) = blocks.get_mut(block)
                {
                    *v = verdict;
                    if verdict != Verdict::Pending {
                        info.at = Some(std::time::SystemTime::now());
                        info.note = note.clone().map(Into::into);
                    }
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
            if verdict == Verdict::AllowedAlways {
                if let Some(rule) = rule_name(&ask.suggestions) {
                    self.set_decider(chat, msg, block, Decider::Rule(rule.into()));
                }
            }
            // A plan's own answers set the mode in the same reply.
            let plan = match verdict {
                Verdict::PlanAutoAccept => Some((
                    crate::backend::PlanChoice::AutoAcceptEdits,
                    ModeId::AcceptEdits,
                )),
                Verdict::PlanApprove => {
                    Some((crate::backend::PlanChoice::ApproveEdits, ModeId::Manual))
                }
                Verdict::KeepPlanning => {
                    Some((crate::backend::PlanChoice::KeepPlanning, ModeId::Plan))
                }
                _ => None,
            };
            if let Some((choice, mode)) = plan {
                let sent = self
                    .sessions
                    .get(&chat)
                    .map(|l| l.session.answer_plan(&ask.request_id, choice, &ask.input));
                self.mode_reported(chat, mode);
                if let Some(Err(e)) = sent {
                    self.notice(chat, e, true);
                    self.finish_turn(chat, cx);
                }
                cx.notify();
                return;
            }
            let allow = matches!(verdict, Verdict::Allowed | Verdict::AllowedAlways);
            if !allow {
                if let Some(&(m, b)) = self.tool_rows.get(&(chat, ask.tool_use_id.clone())) {
                    let said = note
                        .as_ref()
                        .map_or("You declined this.".to_string(), |n| format!("You declined this: {n}"));
                    self.update_tool(chat, m, b, |t| {
                        t.status = ToolStatus::Declined;
                        t.output = said.into();
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
                    note.as_deref(),
                )
            });
            if let Some(Err(e)) = sent {
                self.notice(chat, e, true);
                self.finish_turn(chat, cx);
            }
        }
        cx.notify();
    }

    /// Stop one sub-agent of the chat on screen; the rest of the turn goes on.
    pub fn stop_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        self.stop_agent_runs(Some(id), cx);
    }

    /// Stop every running sub-agent of the chat on screen (Stop all,
    /// Ctrl+Shift+Escape); Forge carries on with what they found.
    pub fn stop_agents(&mut self, cx: &mut Context<Self>) {
        self.stop_agent_runs(None, cx);
    }

    /// The chat on screen has a sub-agent still running.
    pub fn agents_running(&self) -> bool {
        self.current
            .and_then(|c| self.agents.get(&c))
            .is_some_and(|runs| runs.iter().any(|r| r.ended.is_none()))
    }

    fn stop_agent_runs(&mut self, only: Option<&str>, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        let ids: Vec<String> = self
            .agents
            .get(&chat)
            .into_iter()
            .flatten()
            .filter(|r| r.ended.is_none() && !r.stopping && only.is_none_or(|id| r.id == id))
            .map(|r| r.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        let sent = self.sessions.get(&chat).map(|l| match only {
            Some(id) => l.session.stop_agent(id),
            None => l.session.stop_agents(),
        });
        match sent {
            Some(Ok(request)) => {
                self.requests.insert(
                    request,
                    Request::StopAgents {
                        chat,
                        ids: ids.clone(),
                    },
                );
                for r in self.agents.entry(chat).or_default() {
                    if ids.contains(&r.id) {
                        r.stopping = true;
                    }
                }
            }
            Some(Err(e)) => self.show_toast(format!("Couldn't stop the agent: {e}"), cx),
            // No engine behind this chat (the demo): it stops here.
            None => self.mark_agents_stopped(chat, &ids),
        }
        cx.notify();
    }

    /// Show sub-agents as stopped, and their rows in the chat with them.
    fn mark_agents_stopped(&mut self, chat: usize, ids: &[String]) {
        let now = std::time::Instant::now();
        for r in self.agents.entry(chat).or_default() {
            if ids.contains(&r.id) && r.ended.is_none() {
                r.ended = Some(now);
                r.stopping = false;
                r.stopped = true;
                r.failed = false;
                for s in r
                    .steps
                    .iter_mut()
                    .filter(|s| s.status == ToolStatus::Running)
                {
                    s.status = ToolStatus::Declined;
                }
            }
        }
        let order: Vec<String> = self.agents[&chat].iter().map(|r| r.id.clone()).collect();
        for id in ids {
            let stop = |t: &mut ToolCall| {
                t.status = ToolStatus::Declined;
                t.output = format!("{AGENT_STOPPED}.").into();
            };
            if let Some(&(m, b)) = self.tool_rows.get(&(chat, id.clone())) {
                self.update_tool(chat, m, b, stop);
            } else if let Some(n) = order.iter().position(|o| o == id) {
                // The demo's rows have no ids: the agent's row is the nth agent row.
                if let Some(Message::Assistant { blocks }) =
                    self.chat_mut(chat).and_then(|c| c.messages.last_mut())
                {
                    if let Some(Block::Tool(t)) = blocks
                        .iter_mut()
                        .filter(|b| matches!(b, Block::Tool(t) if is_agent_tool(&t.target)))
                        .nth(n)
                    {
                        stop(t);
                    }
                }
            }
        }
        if let Some(c) = self.chat_mut(chat) {
            c.version += 1;
        }
    }

    /// ForgeCLI's answer to a stop: the agents' own results follow (and mark
    /// them stopped); a refusal clears "stopping" and says why.
    fn on_stop_reply(
        &mut self,
        chat: usize,
        ids: &[String],
        result: Result<serde_json::Value, String>,
        cx: &mut Context<Self>,
    ) {
        let Err(e) = result else { return };
        for r in self.agents.entry(chat).or_default() {
            if ids.contains(&r.id) {
                r.stopping = false;
            }
        }
        if e.contains("nsupported") || e.contains("nknown") {
            self.show_toast(CANT_STOP_AGENT, cx);
        } else if e.starts_with("No running agent") {
            // It ended on its own just before the stop arrived.
        } else {
            self.show_toast(format!("Couldn't stop the agent: {e}"), cx);
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
            self.set_decider(id, ask.msg, ask.block, Decider::Stopped);
        }
        // What you queued goes back in the box, as Claude does on Esc.
        self.restore_queued(id, cx);
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
        // ForgeCLI reads a command (`/simplify`) only from a message that is
        // its text alone, so the attachments wait for the next message.
        let command = text.starts_with('/');
        let attachments = if command {
            vec![]
        } else {
            std::mem::take(&mut self.attachments)
        };
        if command && !self.attachments.is_empty() {
            self.show_toast(
                "The command went on its own; your attachments wait for your next message.",
                cx,
            );
        }
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
                // Untitled N until the model names it (`title.rs`), and for
                // good with no model: never your first words. The demo is
                // named after them, as it plays without a model.
                let profile = self.active_profile().cloned();
                let title: String = if demo_mode() {
                    first.chars().take(48).collect()
                } else {
                    crate::title::untitled(self.chats.iter().map(|c| c.title.as_ref()))
                };
                self.chats
                    .insert(0, Chat::new(id, self.current_project, title.clone().into()));
                self.current = Some(id);
                self.keep_mode();
                if let Some(profile) = profile.filter(|_| !demo_mode() && !text.trim().is_empty()) {
                    self.name_chat(id, profile, text.clone(), title, cx);
                }
                id
            }
        };
        self.screen = Screen::Chat;
        // The working line says what Forge is doing about this message once
        // the model has said it in a few words; until then, the step.
        self.headlines.remove(&id);
        if let Some(profile) = self
            .active_profile()
            .cloned()
            .filter(|_| !demo_mode() && !text.trim().is_empty())
        {
            self.ask_headline(id, profile, text.clone(), cx);
        }
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
        self.snapshot_memory(id);
        // A demo chat is named for what its reply does, as a model would
        // name it, not by the word that started the script.
        if let (Some(title), Some(c)) = (demo_title(&text), self.chat_mut(id)) {
            let asks = c
                .messages
                .iter()
                .filter(|m| matches!(m, Message::User { .. }))
                .count();
            if asks == 1 {
                c.title = title.into();
            }
        }
        self.replies.insert(
            id,
            cx.spawn(async move |this, cx| {
                // Only a question asked live can be denied live; a step that
                // shows one already answered (`Step::Asked`) is history.
                let mut asked = false;
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
                    // What you sent meanwhile lands at the next step, as with
                    // ForgeCLI, and the demo says it heard you.
                    if matches!(step, Step::ToolStart(..) | Step::End { .. }) {
                        let _ = this.update(cx, |s, cx| {
                            if s.land_queued_demo(id, cx) {
                                s.apply(id, Step::Say("Noted, I'll take that into account. ".into()), cx);
                            }
                        });
                    }
                    let denied = asked
                        && this
                            .update(cx, |s, _| s.last_permission_denied(id))
                            .unwrap_or(false);
                    asked = matches!(step, Step::Ask(..));
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
                    // Another endpoint: Forge restarts on it and resumes this
                    // conversation, so the new model reads every message above.
                    self.sessions.remove(&id);
                }
            }
        }
        // Expert's output style is part of the process's system prompt: going
        // into or out of Expert restarts the chat's ForgeCLI, resuming the
        // conversation.
        let expert = self.mode == ModeId::Expert;
        let resumable = self.chats.iter().any(|c| c.id == id && c.resumable);
        if resumable && self.sessions.get(&id).is_some_and(|l| l.expert != expert) {
            self.sessions.remove(&id);
        }
        // A remote project's first message connects (and installs ForgeCLI
        // there if needed) in the background, then sends.
        if !self.sessions.contains_key(&id) {
            if let Some(remote) = self.chat_remote(id) {
                if !self.remote_ready.contains_key(&remote.target) {
                    return self.connect_then_send(id, remote.target, text, attachments, cx);
                }
            }
        }
        if !self.sessions.contains_key(&id) {
            if let Err(e) = self.start_session(id, profile, cx) {
                return self.notice(id, e, true);
            }
        }
        // A new conversation's first message carries the project's knowledge
        // ahead of your words (`knowledge.rs`); a command doesn't.
        let dir = self.chat_project_dir(id);
        let memory_on = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| self.project(c.project))
            .is_some_and(|p| p.memory_on);
        let knowledge = (!resumable && !text.trim_start().starts_with('/'))
            .then_some(dir.as_ref())
            .flatten()
            .and_then(|dir| {
                let mut block = crate::knowledge::block(dir).unwrap_or_default();
                if memory_on {
                    block.push_str(&crate::project_memory::prompt(&crate::project_memory::read(dir)));
                }
                (!block.trim().is_empty()).then_some(block)
            });
        self.snapshot_memory(id);
        let sent = self
            .sessions
            .get(&id)
            .map(|l| {
                if let Some(k) = &knowledge {
                    let mut content = crate::attach::content_blocks(
                        &text,
                        &attachments,
                        l.profile.wire == crate::endpoints::Wire::Anthropic,
                    );
                    if let Some(blocks) = content.as_array_mut() {
                        blocks.insert(0, serde_json::json!({ "type": "text", "text": k }));
                    }
                    l.session.send_content(content)
                } else if attachments.is_empty() {
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
        let remote = self.chat_remote(id);
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
        // an MCP server) on the browser in the side panel. A remote agent
        // reaches them over HTTP, forwarded back to this machine.
        let mut extra = crate::remote::Extra::default();
        // Ports on the remote that lead back here: (there, here).
        let mut backs: Vec<(u16, u16)> = vec![];
        if self.browser && remote.is_some() {
            if let Some((port, token)) = self.ensure_browser_http(cx) {
                let there = crate::remote::forward_port(&session_id, port);
                launch.args.extend([
                    "--mcp-config".to_string(),
                    crate::remote::browser_mcp_config(there, &token),
                ]);
                backs.push((there, port));
            }
        } else if self.browser {
            if let (Some(cdp), Ok(exe)) = (self.ensure_web(cx), std::env::current_exe()) {
                launch.args.extend([
                    "--mcp-config".to_string(),
                    crate::browser::mcp_config(&exe, &cdp, &crate::browser::focus_file()),
                ]);
            }
        }
        // The settings profile, layered above the other settings files, and
        // Expert's output style on top of it.
        let profile_path = self
            .settings_profile
            .as_deref()
            .and_then(crate::setting_profiles::path_of);
        if let Some(mut flag) = crate::modes::flag_settings(self.mode, profile_path.as_deref()) {
            // A remote ForgeCLI can't read this machine's file: send its settings.
            if remote.is_some() && !flag.trim_start().starts_with('{') {
                flag = std::fs::read_to_string(&flag).unwrap_or_else(|_| "{}".into());
            }
            launch.args.extend(["--settings".to_string(), flag]);
        }
        if resumable {
            self.resuming.insert(id);
        } else {
            self.resuming.remove(&id);
        }
        let mut relays: Vec<std::sync::Arc<crate::relay::Relay>> = vec![];
        let session = if let Some(remote) = &remote {
            let ready = self
                .remote_ready
                .get(&remote.target)
                .cloned()
                .ok_or_else(|| format!("Not connected to {} yet.", remote.target.label()))?;
            // Forge's plugin, as put on the remote.
            if let Some(i) = launch.args.iter().position(|a| a == "--plugin-dir") {
                if let Some(dir) = launch.args.get_mut(i + 1) {
                    *dir = ready.plugin.clone();
                }
            }
            // Model calls go out from this computer: the remote gets the
            // proxy's address (a port there leading back here) and a
            // placeholder key; the real key, address and headers stay here.
            if let Ok(port) = crate::model_proxy::start() {
                if let Some(old) = self.model_routes.remove(&id) {
                    crate::model_proxy::remove(&old);
                }
                let (route, remote_env) = proxied_env(&launch.env);
                let token = crate::model_proxy::add(route);
                let there = crate::remote::forward_port(&session_id, port);
                backs.push((there, port));
                let base = format!("http://127.0.0.1:{there}/p/{token}");
                launch.env = remote_env
                    .into_iter()
                    .map(|(k, v)| {
                        let v = if k.ends_with("BASE_URL") {
                            base.clone()
                        } else {
                            v
                        };
                        (k, v)
                    })
                    .collect();
                self.model_routes.insert(id, token);
            }
            // SSH carries them as -R forwards; WSL and containers through
            // ForgeCLI's relay beside the session.
            if let crate::remote::Target::Ssh(host) = &remote.target {
                extra.accept_new_host_key = self.trusted_hosts.contains(host);
                for (there, here) in &backs {
                    extra.reverse.push(format!("{there}:127.0.0.1:{here}"));
                }
            } else {
                for (there, here) in &backs {
                    match crate::relay::start(&remote.target, &ready.forge, *there, *here, &extra) {
                        Ok(r) => {
                            let r = std::sync::Arc::new(r);
                            relays.push(r.clone());
                            // Say so if it can't listen there (off this thread).
                            let label = remote.target.label();
                            let tx = self.relay_errors.clone();
                            std::thread::spawn(move || {
                                if let Err(e) = r.wait_ready(std::time::Duration::from_secs(30)) {
                                    let _ = tx.send_blocking(format!("{label}: {e}"));
                                }
                            });
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                }
            }
            crate::backend::Session::spawn_remote(remote, &ready.forge, &launch, &extra)?
        } else {
            let exe = crate::backend::find_cli().ok_or("ForgeCLI wasn't found. Reinstall Forge (it puts forge.exe in the bin folder beside the app), or set FORGE_CLI to its path.")?;
            let project = self.chats.iter().find(|c| c.id == id).map(|c| c.project);
            let dir = project
                .and_then(|p| self.project(p))
                .map(|p| p.path.clone())
                .filter(|p| p.is_dir())
                .unwrap_or_else(|| self.project_dir.clone());
            crate::backend::Session::spawn(&exe, &launch, &dir)?
        };
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
                expert: self.mode == ModeId::Expert,
                _relays: relays,
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
                            .answer(&request_id, false, false, &input, &suggestions, None);
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
            Event::ModeChanged(m) => {
                let mode = match m.as_str() {
                    "acceptEdits" => Some(ModeId::AcceptEdits),
                    "plan" => Some(ModeId::Plan),
                    "bypassPermissions" => Some(ModeId::Bypass),
                    "default" => Some(ModeId::Manual),
                    _ => None,
                };
                if let Some(mode) = mode {
                    self.mode_reported(chat, mode);
                    cx.notify();
                }
            }
            Event::Text(t) => self.apply(chat, Step::Say(t), cx),
            Event::Thinking(t) => self.apply(chat, Step::Think(t), cx),
            Event::ToolUse { id, name, input } => {
                self.panel_follows(chat, &name);
                self.design_call(chat, &name, &input, cx);
                if let Some(at) =
                    self.push_block(chat, Block::Tool(crate::backend::tool_call(&name, &input)))
                {
                    self.tool_rows.insert((chat, id), at);
                }
            }
            Event::Drafting { path, content, .. } => self.design_draft(chat, &path, &content, cx),
            Event::ToolResult {
                id,
                output,
                is_error,
            } => {
                if let Some(&(m, b)) = self.tool_rows.get(&(chat, id)) {
                    let mut written = None;
                    let mut page = None;
                    self.update_tool(chat, m, b, |t| {
                        if t.kind == ToolKind::Write && crate::design::live::is_page(&t.target) {
                            page = Some(t.target.to_string());
                        }
                        if t.status != ToolStatus::Declined {
                            t.status = if output.starts_with(AGENT_STOPPED) {
                                // A sub-agent you stopped: not a failure.
                                ToolStatus::Declined
                            } else if is_error {
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
                    // A page it wrote (or tried to): the live view shows the file.
                    if let Some(p) = page {
                        self.design_done(chat, &p, cx);
                    }
                    // A document Forge wrote gets its card in the reply.
                    if let Some(remote) = self.chat_remote(chat) {
                        if let Some(p) = written {
                            self.remote_written_file(chat, remote, p, cx);
                        }
                    } else if let Some(file) = written.and_then(|p| self.written_file(chat, &p)) {
                        self.follow_doc(chat, &file, cx);
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
                // A harmless command, for the mode the chat is in, runs
                // without a card: its tool row already shows it.
                if self.auto_approves(chat, &tool_name, &input) {
                    if let Some(l) = self.sessions.get(&chat) {
                        let _ = l
                            .session
                            .answer(&request_id, true, false, &input, &suggestions, None);
                    }
                    return;
                }
                let (title, detail) = crate::backend::ask_text(&tool_name, &input);
                let row = self.tool_rows.get(&(chat, tool_use_id.clone())).copied();
                let info = PermInfo {
                    kind: Some(crate::backend::tool_call(&tool_name, &input).kind),
                    offer: rule_name(&suggestions).map(Into::into),
                    scope: rule_scope(&suggestions),
                    ..Default::default()
                };
                if let Some((msg, block)) = self.push_block(
                    chat,
                    Block::Permission {
                        title: title.into(),
                        detail: detail.into(),
                        verdict: Verdict::Pending,
                        info,
                    },
                ) {
                    // Its outcome is read from the call's row, when that is in the same message.
                    if let Some((_, b)) = row.filter(|(m, _)| *m == msg) {
                        if let Some(Message::Assistant { blocks }) =
                            self.chat_mut(chat).and_then(|c| c.messages.get_mut(msg))
                        {
                            if let Some(Block::Permission { info, .. }) = blocks.get_mut(block) {
                                info.row = Some(b);
                            }
                        }
                    }
                    self.asks.insert(
                        chat,
                        Ask {
                            request_id,
                            tool_use_id,
                            tool_name,
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
                    self.set_decider(chat, m, b, Decider::Stopped);
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
            Event::UserAccepted { uuid, text } => {
                if self.land_queued(chat, &uuid, &text, cx) {
                    return;
                }
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
                self.restore_queued(chat, cx);
                let remote = self.chat_remote(chat);
                if self.replies.contains_key(&chat) {
                    let why = match (&remote, code) {
                        // ssh exits 255 when the connection drops.
                        (Some(r), Some(255) | None) => format!(
                            "The connection to {} ended. Your next message reconnects and picks the conversation up.",
                            r.target.label()
                        ),
                        (_, Some(c)) => format!("Forge stopped (exit code {c})."),
                        (_, None) => "Forge stopped.".to_string(),
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

    /// Open a web page Forge wrote in the side panel's browser.
    pub fn preview_page(&mut self, chat: usize, path: &str, cx: &mut Context<Self>) {
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
        match url::Url::from_file_path(&full) {
            Ok(url) if full.exists() => {
                self.show_panel(PanelTab::Browser, cx);
                self.web_send(crate::web_view::Cmd::Navigate(url.to_string()), cx);
            }
            _ => self.show_toast(format!("{} is not there any more", full.display()), cx),
        }
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
        file_card(name, ext, full.display().to_string(), bytes)
    }

    /// A document Forge wrote on a remote, read back over the connection
    /// (off this thread); its card joins the reply when it arrives.
    fn remote_written_file(
        &mut self,
        chat: usize,
        remote: crate::remote::Remote,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = path.rsplit('/').next().map(str::to_string) else {
            return;
        };
        let Some(ext) = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) else {
            return;
        };
        if !matches!(
            ext.as_str(),
            "md" | "markdown" | "svg" | "html" | "htm" | "docx" | "xlsx" | "pptx" | "pdf"
        ) {
            return;
        }
        let full = if path.starts_with('/') || path.starts_with('~') {
            path
        } else {
            format!("{}/{path}", remote.path.trim_end_matches('/'))
        };
        let extra = self.remote_extra(&remote.target);
        let task = cx.background_spawn(async move {
            crate::remote::read_file(&remote.target, &full, 64 << 20, &extra)
                .ok()
                .and_then(|bytes| file_card(name, ext, full, bytes))
        });
        cx.spawn(async move |this, cx| {
            if let Some(file) = task.await {
                let _ = this.update(cx, |s, cx| {
                    s.follow_doc(chat, &file, cx);
                    s.push_block(chat, Block::File(file));
                    s.mark_dirty(chat, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

// ---------------------------------------------------------------- remote development

impl AppState {
    /// What "Show in terminal" shows: the command asked for, or else the
    /// current chat's latest one.
    pub fn take_agent_feed(&mut self) -> Option<AgentFeed> {
        if let Some(f) = self.agent_feed.take() {
            return Some(f);
        }
        let chat = self.chat()?;
        let place = self
            .chat_remote(chat.id)
            .map(|r| (r.target.label(), r.target.icon()));
        chat.messages.iter().rev().find_map(|m| match m {
            Message::Assistant { blocks, .. } => blocks.iter().rev().find_map(|b| match b {
                Block::Tool(c) if c.kind == ToolKind::Bash => Some(AgentFeed::of(c, place.clone())),
                _ => None,
            }),
            _ => None,
        })
    }

    /// Answer the SSH prompt on screen (None cancels it); the next one, if
    /// any, shows.
    pub fn answer_askpass(&mut self, answer: Option<String>, cx: &mut Context<Self>) {
        if let Some(r) = self.askpass.pop_front() {
            let _ = r.reply.send(answer);
        }
        cx.notify();
    }

    /// Keep the dock as it is now for the next launch.
    pub fn save_window(&self) {
        crate::app_settings::save_window(crate::app_settings::WindowState {
            terminal_open: self.terminal_open,
            terminal_share: self.terminal_share,
            dark: self.dark,
            theme: self.look.id().into(),
        });
    }

    /// Open Forge's settings.json in your editor, writing one with every key
    /// at its default first if there is none. Edits apply at their next use.
    pub fn open_app_settings(&mut self, cx: &mut Context<Self>) {
        self.palette_open = false;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let file = cx
                .background_spawn(async { crate::app_settings::ensure_file() })
                .await;
            let _ = this.update(cx, |s, cx| match file {
                Some(path) => {
                    cx.open_with_system(&path);
                    s.show_toast(
                        "Opened settings.json: remote connections and the terminal. Changes apply the next time they're used.",
                        cx,
                    );
                }
                None => s.show_toast("Couldn't write settings.json in Forge's config folder.", cx),
            });
        })
        .detach();
    }

    /// Where a new terminal starts: the current project's remote (and
    /// whether its SSH host was trusted here), or its folder on this
    /// computer.
    pub fn terminal_place(&self) -> (Option<crate::remote::Remote>, std::path::PathBuf, bool) {
        let remote = self.current_remote();
        let trusted = match remote.as_ref().map(|r| &r.target) {
            Some(crate::remote::Target::Ssh(host)) => self.trusted_hosts.contains(host),
            _ => false,
        };
        let dir = self
            .project(self.current_project)
            .filter(|_| remote.is_none())
            .map(|p| std::path::PathBuf::from(&p.path))
            .filter(|p| p.is_dir())
            .or_else(|| {
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(std::path::PathBuf::from)
            })
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        (remote, dir, trusted)
    }

    /// The current project, when it lives on a remote.
    pub fn current_remote(&self) -> Option<crate::remote::Remote> {
        self.project(self.current_project)
            .and_then(|p| crate::remote::Remote::parse(&p.path))
    }

    /// Where a chat runs: its project's remote, or None for this machine.
    pub fn chat_remote(&self, chat: usize) -> Option<crate::remote::Remote> {
        self.chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| self.project(c.project))
            .and_then(|p| crate::remote::Remote::parse(&p.path))
    }

    /// ForgeCLI is on this target and answered: chats there start at once.
    pub fn remote_connected(&self, target: &crate::remote::Target) -> bool {
        self.remote_ready.contains_key(target)
    }

    fn remote_extra(&self, target: &crate::remote::Target) -> crate::remote::Extra {
        crate::remote::Extra {
            accept_new_host_key: matches!(target, crate::remote::Target::Ssh(h) if self.trusted_hosts.contains(h)),
            reverse: vec![],
        }
    }

    /// A remote chat's first message: connect, then send it. The chat
    /// shows as working meanwhile.
    fn connect_then_send(
        &mut self,
        id: usize,
        target: crate::remote::Target,
        text: String,
        attachments: Vec<crate::attach::Attachment>,
        cx: &mut Context<Self>,
    ) {
        self.replies.insert(id, Task::ready(()));
        self.turn_started.insert(id, std::time::Instant::now());
        self.connect(target, Some((id, text, attachments)), cx);
    }

    /// Put this app's ForgeCLI on a target (off this thread; what it's doing
    /// shows on the remote indicator), then send the message that waited.
    pub fn connect(
        &mut self,
        target: crate::remote::Target,
        then: Option<(usize, String, Vec<crate::attach::Attachment>)>,
        cx: &mut Context<Self>,
    ) {
        let extra = self.remote_extra(&target);
        let (tx, rx) = async_channel::unbounded::<String>();
        self.remote_failed = None;
        self.remote_busy = Some((
            target.clone(),
            format!("Connecting to {}…", target.label()).into(),
        ));
        let job = target.clone();
        let task = cx.background_spawn(async move {
            crate::remote::provision(&job, &extra, &|m: &str| {
                let _ = tx.send_blocking(m.to_string());
            })
        });
        cx.spawn(async move |this, cx| {
            while let Ok(m) = rx.recv().await {
                let gone = this
                    .update(cx, |s, cx| {
                        if let Some((_, msg)) = &mut s.remote_busy {
                            *msg = m.into();
                        }
                        cx.notify();
                    })
                    .is_err();
                if gone {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |s, cx| {
                s.remote_busy = None;
                match result {
                    Ok(ready) => {
                        if s.remote_lost.as_ref().is_some_and(|(t, _)| *t == target) {
                            s.remote_lost = None;
                        }
                        s.remote_ready.insert(target, ready);
                        s.ensure_heartbeat(cx);
                        if let Some((id, text, attachments)) = then {
                            s.replies.remove(&id);
                            s.send_to_forge(id, text, attachments, cx);
                        }
                    }
                    Err(e) => {
                        if let crate::remote::Error::UnknownHostKey { host, fingerprint } = &e {
                            let menu = s.remote_menu.get_or_insert_with(Default::default);
                            menu.host_key =
                                Some((target.clone(), host.clone(), fingerprint.clone()));
                        } else {
                            s.remote_failed = Some((target.clone(), e.to_string().into()));
                        }
                        match then {
                            Some((id, ..)) => {
                                s.notice(id, e.to_string(), true);
                                s.finish_turn(id, cx);
                            }
                            None => s.show_toast(e.to_string(), cx),
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Open or close the remote indicator's menu. Opening looks for WSL
    /// distros, SSH hosts, containers and a dev container (off this thread).
    pub fn toggle_remote_menu(&mut self, cx: &mut Context<Self>) {
        if self.remote_menu.take().is_some() {
            return cx.notify();
        }
        self.remote_menu = Some(RemoteMenu::default());
        self.check_devcontainer(cx);
        let task = cx.background_spawn(async move {
            (
                crate::remote::wsl_distros().map_err(|e| e.to_string()),
                crate::remote::ssh_hosts(),
                crate::remote::containers().map_err(|e| e.to_string()),
            )
        });
        cx.spawn(async move |this, cx| {
            let (distros, hosts, containers) = task.await;
            let _ = this.update(cx, |s, cx| {
                if let Some(m) = &mut s.remote_menu {
                    m.distros = Some(distros);
                    m.hosts = hosts;
                    m.containers = Some(containers);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The remote menu, opened on a page (from the command palette).
    pub fn open_remote_page(&mut self, page: RemotePage, cx: &mut Context<Self>) {
        self.sidebar_open = true;
        if self.remote_menu.is_none() {
            self.toggle_remote_menu(cx);
        }
        self.remote_page(page, cx);
    }

    pub fn remote_page(&mut self, page: RemotePage, cx: &mut Context<Self>) {
        if let Some(m) = &mut self.remote_menu {
            m.page = page;
            m.error = None;
        }
        cx.notify();
    }

    /// Show a folder on a target in the menu, to pick the project's folder.
    pub fn browse_remote(
        &mut self,
        target: crate::remote::Target,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let menu = self.remote_menu.get_or_insert_with(Default::default);
        menu.page = RemotePage::Folder;
        menu.error = None;
        menu.folder = Some(RemoteFolder {
            target: target.clone(),
            path: path.clone(),
            entries: None,
        });
        let extra = self.remote_extra(&target);
        let job = target.clone();
        let task = cx.background_spawn(async move { crate::remote::list_dir(&job, &path, &extra) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |s, cx| {
                let Some(m) = &mut s.remote_menu else { return };
                let Some(f) = m.folder.as_mut().filter(|f| f.target == target) else {
                    return;
                };
                match result {
                    Ok((full, entries)) => {
                        f.path = full;
                        f.entries = Some(Ok(entries));
                    }
                    Err(crate::remote::Error::UnknownHostKey { host, fingerprint }) => {
                        f.entries = Some(Err("Waiting for you to trust the host.".into()));
                        m.host_key = Some((target, host, fingerprint));
                    }
                    Err(e) => f.entries = Some(Err(e.to_string())),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// "Trust" on an unknown SSH host key: remember it (ssh then records
    /// it in known_hosts) and carry on browsing there.
    pub fn trust_host(&mut self, cx: &mut Context<Self>) {
        let Some((target, host, _)) = self.remote_menu.as_mut().and_then(|m| m.host_key.take())
        else {
            return;
        };
        self.trusted_hosts.insert(host);
        self.browse_remote(target, "~".into(), cx);
    }

    /// Work in a remote folder: it becomes the current project, and
    /// ForgeCLI is put there in the background.
    /// Opened the wrong folder on a host: its folders, from the one above
    /// this, to pick another. The connection stays up.
    pub fn change_remote_folder(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.current_remote() else {
            return self.show_toast("Connect to a WSL distro, SSH host or container first.", cx);
        };
        let up = parent_dir(&r.path);
        self.switcher_open = false;
        self.sidebar_open = true;
        if self.remote_menu.is_none() {
            self.toggle_remote_menu(cx);
        }
        self.browse_remote(r.target, up, cx);
    }

    pub fn open_remote_folder(&mut self, remote: crate::remote::Remote, cx: &mut Context<Self>) {
        self.remote_menu = None;
        // Leaving a folder on the same host that you only looked into (not
        // a project, no chats): it goes, so Recent isn't left with it.
        let left = self.current_project;
        let abandoned = self.project(left).is_some_and(|p| {
            !p.saved
                && p.path != remote.to_path()
                && crate::remote::Remote::parse(&p.path)
                    .is_some_and(|r| r.target == remote.target)
        }) && !self.chats.iter().any(|c| c.project == left);
        let target = remote.target.clone();
        // Connecting doesn't make a project: Save as project does, if you want one.
        self.work_in_folder(remote.to_path(), cx);
        if abandoned {
            self.projects.retain(|p| p.id != left);
        }
        if !self.remote_ready.contains_key(&target) {
            self.connect(target, None, cx);
        }
        self.check_devcontainer(cx);
    }

    /// The current folder as a dev container's source: a local folder, or a
    /// folder in WSL (whose Docker then runs it). None elsewhere (SSH, a
    /// container, a plain chat).
    fn devcontainer_folder(&self) -> Option<crate::remote::DevSource> {
        use crate::remote::{DevSource, Target};
        match crate::remote::Remote::parse(&self.project_dir) {
            None => self.project_dir.is_dir().then(|| DevSource::Local(self.project_dir.clone())),
            Some(r) => match r.target {
                Target::Wsl(distro) => Some(DevSource::Wsl { distro, path: r.path }),
                _ => None,
            },
        }
    }

    /// In a WSL folder: find out, off the UI thread, whether it has a dev
    /// container definition, for the menu's Reopen in container.
    fn check_devcontainer(&mut self, cx: &mut Context<Self>) {
        let Some(crate::remote::DevSource::Wsl { distro, path }) = self.devcontainer_folder() else {
            return;
        };
        let key = self.project_dir.clone();
        if self.wsl_devcontainers.contains_key(&key) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let has = cx
                .background_spawn(async move { crate::remote::wsl_has_devcontainer(&distro, &path) })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.wsl_devcontainers.insert(key, has);
                cx.notify();
            });
        })
        .detach();
    }

    /// Reopen the current folder (on this computer, or in WSL) in its dev
    /// container.
    pub fn reopen_in_container(&mut self, cx: &mut Context<Self>) {
        match self.devcontainer_folder() {
            Some(src) if self.has_devcontainer() => self.start_devcontainer(src, false, cx),
            Some(_) => self.show_toast("This folder has no .devcontainer/devcontainer.json.", cx),
            None => self.show_toast(
                "Dev containers start from a folder on this computer or in WSL.",
                cx,
            ),
        }
    }

    /// The current remote is a dev container Forge started from a folder.
    pub fn in_devcontainer(&self) -> bool {
        self.current_remote()
            .is_some_and(|r| crate::remote::is_devcontainer(&r.target))
    }

    /// The local folder of the current dev container, read off the UI
    /// thread, then `then` with it (or a toast saying why not).
    fn with_devcontainer_source(
        &mut self,
        then: impl FnOnce(&mut Self, crate::remote::DevSource, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(crate::remote::Target::Container { name, wsl, .. }) =
            self.current_remote().map(|r| r.target)
        else {
            return self.show_toast("This project isn't in a dev container.", cx);
        };
        self.remote_menu = None;
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    crate::remote::devcontainer_source(&name, wsl.as_deref())
                })
                .await;
            let _ = this.update(cx, |s, cx| match found {
                Ok(crate::remote::DevSource::Local(dir)) if !dir.is_dir() => s.show_toast(
                    format!("The folder it came from is gone: {}", dir.display()),
                    cx,
                ),
                Ok(src) => then(s, src, cx),
                Err(e) => s.show_toast(e.to_string(), cx),
            });
        })
        .detach();
        cx.notify();
    }

    /// Remote: Rebuild container. The dev container is replaced by a fresh
    /// one (image built again, postCreateCommand run again) for the same
    /// folder; from a local folder with a devcontainer.json, its container.
    pub fn rebuild_container(&mut self, cx: &mut Context<Self>) {
        if self.in_devcontainer() {
            self.with_devcontainer_source(|s, dir, cx| s.start_devcontainer(dir, true, cx), cx);
        } else if let Some(src) = self.devcontainer_folder().filter(|_| self.has_devcontainer()) {
            self.start_devcontainer(src, true, cx);
        } else {
            self.show_toast("There's no dev container here to rebuild.", cx);
        }
    }

    /// Remote: Reopen locally. Back to the folder on this computer that the
    /// dev container was started from.
    pub fn reopen_locally(&mut self, cx: &mut Context<Self>) {
        if !self.in_devcontainer() {
            return self.show_toast("This project isn't in a dev container.", cx);
        }
        self.with_devcontainer_source(
            |s, src, cx| {
                s.remote_failed = None;
                s.remote_lost = None;
                // The folder, as you were in it: a project only if it was one.
                match src {
                    crate::remote::DevSource::Local(dir) => s.work_in_folder(dir, cx),
                    crate::remote::DevSource::Wsl { distro, path } => s.open_remote_folder(
                        crate::remote::Remote {
                            target: crate::remote::Target::Wsl(distro),
                            path,
                        },
                        cx,
                    ),
                }
            },
            cx,
        );
    }

    /// Start (or with `fresh`, rebuild) the dev container for `dir` and open
    /// its folder there, saying each step in the status bar.
    fn start_devcontainer(&mut self, src: crate::remote::DevSource, fresh: bool, cx: &mut Context<Self>) {
        self.remote_menu = None;
        let placeholder = crate::remote::Target::Container {
            name: "dev container".into(),
            user: None,
            wsl: None,
        };
        let first = if fresh {
            "Rebuilding the dev container…"
        } else {
            "Starting the dev container…"
        };
        self.remote_busy = Some((placeholder, first.into()));
        let (tx, rx) = async_channel::unbounded::<String>();
        let task = cx.background_spawn(async move {
            crate::remote::devcontainer_up(&src, fresh, &|m: &str| {
                let _ = tx.send_blocking(m.to_string());
            })
        });
        cx.spawn(async move |this, cx| {
            while let Ok(m) = rx.recv().await {
                if this
                    .update(cx, |s, cx| {
                        if let Some((_, msg)) = &mut s.remote_busy {
                            *msg = m.into();
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |s, cx| {
                s.remote_busy = None;
                match result {
                    Ok(remote) => s.open_remote_folder(remote, cx),
                    Err(e) => s.show_toast(e.to_string(), cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// The current folder has a dev container definition: a local one
    /// looked at now, a WSL one as last checked (`check_devcontainer`).
    pub fn has_devcontainer(&self) -> bool {
        match self.devcontainer_folder() {
            Some(crate::remote::DevSource::Local(dir)) => {
                dir.join(".devcontainer").join("devcontainer.json").is_file()
                    || dir.join(".devcontainer.json").is_file()
            }
            Some(crate::remote::DevSource::Wsl { .. }) => self
                .wsl_devcontainers
                .get(&self.project_dir)
                .copied()
                .unwrap_or(false),
            None => false,
        }
    }

    /// Back to this machine, where Forge started: the folder it was
    /// started in, else a plain chat (no folder), as at launch.
    pub fn close_remote(&mut self, cx: &mut Context<Self>) {
        self.remote_menu = None;
        self.remote_failed = None;
        self.remote_lost = None;
        let launch = self.launch_dir.clone();
        match launch.and_then(|d| self.projects.iter().find(|p| p.path == d).map(|p| p.id)) {
            Some(id) => {
                self.select_project(id, cx);
                self.new_chat(cx);
            }
            None => self.new_plain_chat(cx),
        }
        cx.notify();
    }

    /// "Retry" on the connection banner: connect again now.
    pub fn retry_remote(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.current_remote() {
            self.remote_lost = None;
            self.connect(r.target, None, cx);
        }
    }

    /// Every 15 s, check the current remote still answers. When it stops,
    /// say so and try again with a growing pause (1, 2, 4 … 30 s); after
    /// `remote.reconnect.maxAttempts` tries (8 unless set) it shows as failed until you retry.
    fn ensure_heartbeat(&mut self, cx: &mut Context<Self>) {
        let max = crate::app_settings::get().reconnect_max_attempts;
        if self.remote_heartbeat.is_some() {
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            let mut pause = Duration::from_secs(15);
            loop {
                cx.background_executor().timer(pause).await;
                let Ok(Some((target, extra))) = this.update(cx, |s, _| {
                    let r = s.current_remote()?;
                    let watched = s.remote_ready.contains_key(&r.target)
                        || s.remote_lost
                            .as_ref()
                            .is_some_and(|(t, n)| *t == r.target && *n < max);
                    watched.then(|| (r.target.clone(), s.remote_extra(&r.target)))
                }) else {
                    if this.update(cx, |_, _| ()).is_err() {
                        return;
                    }
                    pause = Duration::from_secs(15);
                    continue;
                };
                let probe = target.clone();
                let ok = cx
                    .background_spawn(async move { crate::remote::probe(&probe, &extra) })
                    .await;
                let next = this.update(cx, |s, cx| {
                    let next = match ok {
                        Ok(_) => {
                            if s.remote_lost.take().is_some() {
                                s.remote_busy = None;
                                s.remote_failed = None;
                                s.show_toast(format!("Reconnected to {}.", target.label()), cx);
                                // Ready again (ForgeCLI is still there: no upload).
                                s.connect(target.clone(), None, cx);
                            }
                            Duration::from_secs(15)
                        }
                        Err(e) => {
                            // Chats there start again (resuming) once it answers.
                            s.remote_ready.remove(&target);
                            let tries = s.remote_lost.as_ref().map_or(0, |(_, n)| *n) + 1;
                            if tries >= max {
                                s.remote_lost = Some((target.clone(), tries));
                                s.remote_busy = None;
                                s.remote_failed = Some((target.clone(), e.to_string().into()));
                                Duration::from_secs(15)
                            } else {
                                s.remote_lost = Some((target.clone(), tries));
                                s.remote_busy = Some((
                                    target.clone(),
                                    format!("Reconnecting to {}…", target.label()).into(),
                                ));
                                Duration::from_secs(1 << (tries - 1).min(5))
                                    .min(Duration::from_secs(30))
                            }
                        }
                    };
                    cx.notify();
                    next
                });
                match next {
                    Ok(p) => pause = p,
                    Err(_) => return,
                }
            }
        });
        self.remote_heartbeat = Some(task);
    }

    /// The browser's tool server over HTTP, started on first use: (port, token).
    fn ensure_browser_http(&mut self, cx: &mut Context<Self>) -> Option<(u16, String)> {
        if let Some(h) = &self.browser_http {
            return Some(h.clone());
        }
        let cdp = self.ensure_web(cx)?;
        let token = crate::history::new_id().replace('-', "");
        let port =
            crate::browser_mcp::serve_http(&cdp, &crate::browser::focus_file(), &token).ok()?;
        self.browser_http = Some((port, token));
        self.browser_http.clone()
    }
}

/// Which version of a file a card holds: its name and a key of its content.
pub fn doc_version_key(f: &FileRef) -> SharedString {
    let key = match &f.bytes {
        Some(b) => crate::office_render::key(b),
        None => crate::office_render::key(f.content.as_bytes()),
    };
    format!("{}#{key:x}", f.name).into()
}

/// A written document's card: Markdown, a diagram, an Office file or a PDF.
fn file_card(name: String, ext: String, full: String, bytes: Vec<u8>) -> Option<FileRef> {
    {
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
            path: Some(full.into()),
            content: if binary {
                "".into()
            } else {
                String::from_utf8_lossy(&bytes).into_owned().into()
            },
            bytes: binary.then(|| std::sync::Arc::from(bytes.into_boxed_slice())),
        })
    }
}

impl AppState {
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
        self.check_memory(chat, cx);
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

    // ------------------------------------------------------------ project memory

    /// The memory as chat `id`'s turn begins, to show what the turn changed.
    fn snapshot_memory(&mut self, id: usize) {
        let Some(c) = self.chats.iter().find(|c| c.id == id) else {
            return;
        };
        if !self.project(c.project).is_some_and(|p| p.memory_on) {
            return;
        }
        if let Some(dir) = self.chat_project_dir(id) {
            let before = crate::project_memory::read(&dir);
            self.memory_before.insert(id, (dir, before));
            self.memory_changes.remove(&id);
        }
    }

    /// After a turn: what it changed in the project's memory, if anything.
    fn check_memory(&mut self, chat: usize, cx: &mut Context<Self>) {
        let Some((dir, before)) = self.memory_before.remove(&chat) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let d = dir.clone();
            let after = cx
                .background_spawn(async move { crate::project_memory::read(&d) })
                .await;
            let (added, removed) = crate::project_memory::diff(&before, &after);
            if added.is_empty() && removed.is_empty() {
                return;
            }
            let _ = this.update(cx, |s, cx| {
                s.memory_changes.insert(
                    chat,
                    MemoryChange {
                        project: dir,
                        before,
                        added,
                        removed,
                    },
                );
                if let Screen::Project(id) = s.screen {
                    s.load_project_brief(id, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Undo what the latest turn of `chat` changed in its project's memory.
    pub fn undo_memory(&mut self, chat: usize, cx: &mut Context<Self>) {
        let Some(change) = self.memory_changes.remove(&chat) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move {
                    crate::project_memory::write(&change.project, &change.before)
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                match done {
                    Ok(()) => s.show_toast("Undone: the memory is as it was before that reply.", cx),
                    Err(e) => s.show_toast(format!("Couldn't undo it: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Forge keeps a memory for project `id`, or doesn't.
    pub fn set_project_memory(&mut self, id: usize, on: bool, cx: &mut Context<Self>) {
        self.edit_project(id, cx, |p| p.memory_on = on);
        self.load_project_brief(id, cx);
    }

    /// Change project `id`'s memory (off the UI thread), then read it again.
    pub fn edit_memory(
        &mut self,
        id: usize,
        change: impl FnOnce(&str) -> String + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(dir) = self
            .project(id)
            .filter(|p| crate::remote::Remote::parse(&p.path).is_none())
            .map(|p| p.path.clone())
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move {
                    let now = crate::project_memory::read(&dir);
                    crate::project_memory::write(&dir, &change(&now))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if let Err(e) = done {
                    s.show_toast(format!("Couldn't change the memory: {e}"), cx);
                }
                s.load_project_brief(id, cx);
            });
        })
        .detach();
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
        // A page streaming: the live view, as for a real Write.
        if let Step::Draft(path, content) = &step {
            self.design_draft(chat, path, content, cx);
            return;
        }
        if let Step::Wrote(path, content) = &step {
            let made = std::path::Path::new(path)
                .parent()
                .map(std::fs::create_dir_all)
                .is_some_and(|r| r.is_ok())
                && std::fs::write(path, content).is_ok();
            if let Some(Message::Assistant { blocks }) = self.chat_mut(chat).and_then(|c| c.messages.last_mut()) {
                if let Some(Block::Tool(t)) = blocks.iter_mut().rev().find(|b| matches!(b, Block::Tool(_))) {
                    t.status = if made { ToolStatus::Done } else { ToolStatus::Failed };
                }
            }
            self.design_done(chat, path, cx);
            if let Some(file) = self.written_file(chat, path) {
                self.push_block(chat, Block::File(file));
            }
            cx.notify();
            return;
        }
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
                crate::backend::Event::ToolResult { id, .. } => {
                    // A run you stopped already shows so.
                    let stopped = self
                        .agents
                        .get(&chat)
                        .is_some_and(|runs| runs.iter().any(|r| &r.id == id && r.stopped));
                    // The oldest agent row still running is the one done.
                    if stopped {
                    } else if let Some(Message::Assistant { blocks }) =
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
            (Step::ToolDiff(diff), Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks.iter_mut().rev().find(|b| matches!(b, Block::Tool(_))) {
                    t.diff = diff;
                }
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
            (Step::File(f), Some(blocks)) => {
                blocks.push(Block::File(f.clone()));
                self.follow_doc(chat, &f, cx);
            }
            (Step::Ask(title, detail), Some(blocks)) => {
                // The call it asks about is the row just before it.
                let row = blocks.iter().rposition(|b| matches!(b, Block::Tool(_)));
                let kind = row.and_then(|r| match &blocks[r] {
                    Block::Tool(t) => Some(t.kind),
                    _ => None,
                });
                blocks.push(Block::Permission {
                    title: title.into(),
                    detail: detail.into(),
                    verdict: Verdict::Pending,
                    info: PermInfo {
                        kind,
                        row,
                        offer: Some("Edit".into()),
                        scope: Some("in this project, for you"),
                        ..Default::default()
                    },
                })
            }
            (Step::Asked(name, input, output, verdict, by, own_row), Some(blocks)) => {
                // A question answered before: its call, done, then the answer.
                // A sub-agent's call has no row of its own in the reply.
                let mut t = crate::backend::tool_call(name, &input);
                t.status = if verdict == Verdict::Denied {
                    ToolStatus::Declined
                } else {
                    ToolStatus::Done
                };
                t.output = output.into();
                t.open = false;
                let kind = t.kind;
                if own_row {
                    blocks.push(Block::Tool(t));
                }
                let (title, detail) = crate::backend::ask_text(name, &input);
                blocks.push(Block::Permission {
                    title: title.into(),
                    detail: detail.into(),
                    verdict,
                    info: PermInfo {
                        kind: Some(kind),
                        by,
                        at: Some(std::time::SystemTime::now()),
                        row: own_row.then(|| blocks.len() - 1),
                        ..Default::default()
                    },
                });
            }
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
                self.check_memory(chat, cx);
            }
            (Step::Remember(fact), _) => {
                // As Forge would: the fact written into the project's memory.
                if let Some(dir) = self.chat_project_dir(chat) {
                    let now = crate::project_memory::read(&dir);
                    let _ = crate::project_memory::write(
                        &dir,
                        &crate::project_memory::add(&now, "Preferences", &fact),
                    );
                }
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
    /// A call and the permission question it needed, already answered.
    Asked(
        &'static str,
        serde_json::Value,
        &'static str,
        Verdict,
        Decider,
        bool,
    ),
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
    /// The reply writes a fact into the project's memory.
    Remember(String),
    /// The running call's change, before it is made.
    ToolDiff(Vec<DiffLine>),
    /// A page being written, as far as it has streamed: (path, content).
    Draft(String, String),
    /// The page written: the file is made, its row done, its card shown.
    Wrote(String, String),
}

impl Step {
    fn delay(&self) -> u64 {
        match self {
            Step::Think(_) | Step::Say(_) => 45,
            Step::ToolStart(..) => 350,
            Step::ToolDiff(_) => 0,
            Step::ToolDone(..) => 900,
            Step::Ask(..) => 300,
            Step::Asked(..) => 450,
            Step::Use(..) => 500,
            Step::File(..) | Step::Remember(_) => 300,
            Step::Agent(..) => 380,
            Step::Denied | Step::End { .. } => 250,
            // About the pace a model writes a page at.
            Step::Draft(..) => 420,
            Step::Wrote(..) => 450,
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

/// The demo's page: a six-slide course introduction, written a few lines
/// at a time, so the live view shows it forming, then made on disk.
fn demo_design() -> Vec<Step> {
    const PAGE: &str = include_str!("../assets/demo/course-intro.html");
    let path = std::env::temp_dir()
        .join("forge-demo")
        .join("course-intro.html")
        .display()
        .to_string();
    let mut s = words(
        "I'll build a six-slide course introduction: the title, what people leave with, the six weeks, how a week runs, who teaches, and the first task.",
        Step::Say,
    );
    s.push(Step::ToolStart(ToolKind::Write, path.clone(), None));
    let lines: Vec<&str> = PAGE.split_inclusive('\n').collect();
    let mut so_far = String::new();
    for chunk in lines.chunks(5) {
        so_far.extend(chunk.iter().copied());
        s.push(Step::Draft(path.clone(), so_far.clone()));
    }
    s.push(Step::Wrote(path, PAGE.to_string()));
    s.extend(words(
        "\n\nIt's open beside the chat. Each slide is 16:9 and stacks on a narrow window; the colours and type are set once at the top, so a new course needs only its words.",
        Step::Say,
    ));
    s
}

/// The demo's sub-agents: three reviewers at once, each with its own steps
/// and answer, as the engine reports them.
/// Questions already answered: three of a sub-agent's commands you allowed
/// (one row, opened to list them), an edit an "Always allow" rule let
/// through, and a command you denied.
fn demo_approvals() -> Vec<Step> {
    use serde_json::json;
    let bash = |cmd: &str, what: &str| json!({ "command": cmd, "description": what });
    let mut s = words(
        "Three agents need to look around the repo, so a few commands need your OK.",
        Step::Say,
    );
    s.extend([
        Step::Asked("Bash", bash("rm -rf target", "Clean the build"), "", Verdict::Denied, Decider::You, true),
        Step::Asked("Bash", bash("git log --oneline -20", "Recent history"), "", Verdict::Allowed, Decider::You, false),
        Step::Asked("Bash", bash("find . -name '*.toml' -not -path './target/*'", "Find the manifests"), "", Verdict::Allowed, Decider::You, false),
        Step::Asked("Bash", bash("file target/release/forge", "Check the binary"), "", Verdict::Allowed, Decider::You, false),
        Step::Asked(
            "Edit",
            json!({ "file_path": "src/main.rs", "old_string": "println!(\"Hello, world\");", "new_string": "println!(\"{}\", greeting());" }),
            "The file was updated.",
            Verdict::AllowedAlways,
            Decider::Rule("Edit(src/**)".into()),
            true,
        ),
    ]);
    s.extend(words(
        "The survey is done; I left the build folder alone, as you asked.",
        Step::Say,
    ));
    s
}

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
/// The demo plan, reworked: a sentence reworded, a step changed, one added.
fn reworked_plan() -> String {
    include_str!("../assets/docs/settings-migration-plan.md")
        .replace(
            "reads the old file once, writes the new one beside it, and keeps a backup.",
            "reads the old file once, writes the new one beside it, and keeps a dated backup.",
        )
        .replace(
            "4. Keep the old file as `settings.v1.backup.json` for a release.",
            "4. Keep the old file as `settings.v1.backup.json` for two releases.\n5. Tell the person once, in a toast, that their settings moved.",
        )
}

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
            "q3-budget-memo.docx",
            "finance/q3-budget-memo.docx",
            "",
            Some(include_bytes!("../assets/docs/samples/q3-budget-memo.docx")),
        ),
        file(
            "q3-budget.pptx",
            "finance/q3-budget.pptx",
            "",
            Some(include_bytes!("../assets/docs/samples/q3-budget.pptx")),
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

/// The name a scripted reply gives its chat; None keeps the prompt's words.
fn demo_title(prompt: &str) -> Option<&'static str> {
    let lower = prompt.to_lowercase();
    [
        ("agent", "Chat pipeline review"),
        ("approv", "Repo survey"),
        ("plan", "Settings migration plan"),
        ("chart", "Test times by crate"),
        ("ascii", "Window layout sketch"),
    ]
    .into_iter()
    .find(|(key, _)| lower.contains(key))
    .map(|(_, title)| title)
}

fn script(prompt: &str) -> Vec<Step> {
    let mut s = vec![];
    s.extend(words(
        &format!("The user asked: \"{}\". I should read the entry point first, change only what is needed, then run the tests to prove it.", prompt.lines().next().unwrap_or_default()),
        Step::Think,
    ));
    // Asked to draw something: the reply draws it, as a Mermaid diagram.
    let lower = prompt.to_lowercase();
    // Told to remember something: it says so, and writes it down.
    if let Some(rest) = lower.strip_prefix("remember") {
        let fact = prompt[prompt.len() - rest.len()..]
            .trim_start_matches([':', ' '])
            .trim_start_matches("that ")
            .trim();
        let mut fact = fact.to_string();
        if let Some(first) = fact.get(..1) {
            fact = format!("{}{}", first.to_uppercase(), &fact[1..]);
        }
        if !fact.ends_with('.') {
            fact.push('.');
        }
        s.extend(words("Noted. I'll keep that in this project's memory.", Step::Say));
        s.push(Step::Remember(fact));
        s.push(Step::End { seconds: 1.4 });
        return s;
    }
    if ["design", "deck", "slides", "course"].iter().any(|w| lower.contains(w)) {
        s.extend(demo_design());
        s.push(Step::End { seconds: 21.4 });
        return s;
    }
    if lower.contains("agent") {
        s.extend(demo_agents());
        s.push(Step::End { seconds: 14.2 });
        return s;
    }
    if lower.contains("approv") {
        s.extend(demo_approvals());
        s.push(Step::End { seconds: 4.8 });
        return s;
    }
    // Asked to rework the plan: it writes the plan again, changed, so the
    // panel has a second version to step to and compare.
    if lower.contains("rework") || lower.contains("as these comments ask") {
        s.extend(words(
            "I reworked the plan: the backup stays two releases, and you hear once that your settings moved.",
            Step::Say,
        ));
        s.push(Step::File(FileRef {
            name: "settings-migration-plan.md".into(),
            path: Some("docs/settings-migration-plan.md".into()),
            content: reworked_plan().into(),
            bytes: None,
        }));
        s.push(Step::End { seconds: 4.8 });
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
    // The change is known when the call starts, as ForgeCLI sends it: the
    // card shows it before you allow it.
    let change = diff(&[
        (DiffKind::Add, "pub fn greeting() -> &'static str {"),
        (DiffKind::Add, "    \"Hello, Forge\""),
        (DiffKind::Add, "}"),
        (DiffKind::Add, ""),
        (DiffKind::Same, "fn main() {"),
        (DiffKind::Del, "    println!(\"Hello, world\");"),
        (DiffKind::Add, "    println!(\"{}\", greeting());"),
        (DiffKind::Same, "}"),
    ]);
    s.push(Step::ToolStart(ToolKind::Edit, "src/main.rs".into(), None));
    s.push(Step::ToolDiff(change.clone()));
    s.push(Step::Ask(
        "Allow Forge to edit main.rs?".into(),
        "src/main.rs".into(),
    ));
    s.push(Step::ToolDone("The file was updated.".into(), change));
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

/// The projects: the folder Forge was started on (if it was), then the
/// ones you opened before (`projects.txt` in the config folder), then plain
/// chats' place, which is no project of yours.
/// The folder above a remote path (`~/work/api` → `~/work`); the top stays.
fn parent_dir(path: &str) -> String {
    let p = path.trim_end_matches('/');
    match p.rsplit_once('/') {
        Some(("", _)) => "/".into(),
        Some((up, _)) => up.to_string(),
        None if p.is_empty() => "/".into(),
        None => p.to_string(),
    }
}

fn initial_projects(launch: Option<&std::path::Path>, scratch: &std::path::Path) -> Vec<Project> {
    initial_projects_from(launch, scratch, &saved_projects())
}

fn initial_projects_from(
    launch: Option<&std::path::Path>,
    scratch: &std::path::Path,
    records: &[ProjectRecord],
) -> Vec<Project> {
    let mut out: Vec<Project> = vec![];
    // The folder Forge started on: a project this run, with its details
    // if it has some, on your list only if you saved it.
    if let Some(here) = launch {
        let mut p = Project::at(1, here.to_path_buf(), true);
        p.listed = false;
        if let Some(r) = records.iter().find(|r| r.path == here) {
            p.apply(r);
        }
        out.push(p);
    }
    for r in records.iter().filter(|r| r.listed) {
        let remote = crate::remote::Remote::parse(&r.path).is_some();
        if (remote || r.path.is_dir()) && !out.iter().any(|p| p.path == r.path) {
            let mut p = Project::at(out.len() + 1, r.path.clone(), true);
            p.opened_minutes_ago = 60;
            p.apply(r);
            out.push(p);
        }
    }
    out.push(Project {
        name: "Chats".into(),
        branch: "".into(),
        listed: false,
        scratch: true,
        ..Project::at(out.len() + 1, scratch.to_path_buf(), false)
    });
    out
}

/// Where a chat with no folder works: Forge's own folder, so a question or
/// a lesson never runs in (or reads) a folder of yours. Files it writes
/// there still come back as cards in the reply.
pub fn scratch_dir() -> std::path::PathBuf {
    crate::profile::dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("workspace")
}

/// The folder Forge was started on: `FORGE_PROJECT_DIR`, or a folder given
/// as its argument (`forge .`). Opened from the Start menu or a shortcut, it
/// was started on none, and opens on plain chats, as a chat app does.
fn launch_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("FORGE_PROJECT_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::args_os().nth(1).map(std::path::PathBuf::from))
        .filter(|p| p.is_dir())
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
}

/// A project's name: its folder's (a remote one's too).
fn project_name(path: &std::path::Path) -> String {
    if let Some(r) = crate::remote::Remote::parse(path) {
        return r.name();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn projects_file() -> Option<std::path::PathBuf> {
    crate::profile::dir().map(|d| d.join("projects.json"))
}

/// The list before `projects.json`: one folder per line.
fn old_projects_file() -> Option<std::path::PathBuf> {
    crate::profile::dir().map(|d| d.join("projects.txt"))
}

fn saved_projects() -> Vec<ProjectRecord> {
    let json = projects_file().and_then(|f| std::fs::read_to_string(f).ok());
    let old = old_projects_file().and_then(|f| std::fs::read_to_string(f).ok());
    parse_projects(json.as_deref(), old.as_deref())
}

/// Your projects from `projects.json`, or from the old list when there is
/// no JSON yet (each folder listed, nothing else known).
fn parse_projects(json: Option<&str>, old: Option<&str>) -> Vec<ProjectRecord> {
    if let Some(records) = json.and_then(|j| serde_json::from_str::<Vec<ProjectRecord>>(j).ok()) {
        return records;
    }
    old.unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| ProjectRecord {
            path: std::path::PathBuf::from(l.trim()),
            name: None,
            description: String::new(),
            starred: false,
            archived: false,
            created: 0,
            opened: 0,
            memory_on: true,
            listed: true,
            model: None,
            mode: None,
        })
        .collect()
}

/// A remote ForgeCLI's model settings with the secrets taken out: the route
/// the proxy keeps here (the real base URL, the key as the wire sends it,
/// custom headers), and the environment the remote gets instead (the key a
/// placeholder, no custom headers; the base URL is replaced by the caller).
fn proxied_env(env: &[(String, String)]) -> (crate::model_proxy::Route, Vec<(String, String)>) {
    let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let mut headers = vec![];
    let base = if let Some(b) = get("FORGE_BASE_URL") {
        if let Some(t) = get("FORGE_API_KEY") {
            headers.push(("x-api-key".to_string(), t.clone()));
            headers.push(("authorization".to_string(), format!("Bearer {t}")));
        }
        b
    } else {
        if let Some(t) = get("FORGE_OPENAI_API_KEY") {
            headers.push(("authorization".to_string(), format!("Bearer {t}")));
        }
        get("FORGE_OPENAI_BASE_URL").unwrap_or_default()
    };
    if let Some(lines) = get("FORGE_CUSTOM_HEADERS") {
        for l in lines.lines() {
            if let Some((k, v)) = l.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    let remote = env
        .iter()
        .filter(|(k, _)| k != "FORGE_CUSTOM_HEADERS")
        .map(|(k, v)| match k.as_str() {
            "FORGE_API_KEY" | "FORGE_AUTH_TOKEN" | "FORGE_OPENAI_API_KEY" => {
                (k.clone(), crate::model_proxy::PLACEHOLDER_KEY.to_string())
            }
            _ => (k.clone(), v.clone()),
        })
        .collect();
    (crate::model_proxy::Route { base, headers }, remote)
}

/// Keep your projects for next time (blocking: off the UI thread).
fn save_projects(records: &[ProjectRecord]) {
    let Some(f) = projects_file() else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(text) = serde_json::to_string_pretty(records) {
        let _ = std::fs::write(f, text);
    }
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
    #[test]
    fn projects_keep_their_details_and_the_old_list_still_loads() {
        // The old list: folders only, each on your list.
        let old = super::parse_projects(None, Some("/a\n\n/b\n"));
        assert_eq!(old.len(), 2);
        assert!(old.iter().all(|r| r.listed && r.memory_on && !r.starred));
        // A project's details round-trip through projects.json.
        let dir = std::env::temp_dir().join(format!("forge-proj-{}", std::process::id()));
        let here = dir.join("here");
        let other = dir.join("other");
        std::fs::create_dir_all(&here).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let mut p = super::Project::at(1, other.clone(), true);
        p.name = "Budget work".into();
        p.description = "Q3 numbers".into();
        p.pinned = true;
        let json = serde_json::to_string(&[p.record()]).unwrap();
        let back = super::parse_projects(Some(&json), Some("/ignored"));
        assert_eq!(back, vec![p.record()], "the JSON wins over the old list");
        // Loaded: named, described, starred; the launch folder joins as a
        // project for the run without being listed.
        let launch_rec = super::ProjectRecord {
            listed: false,
            ..super::Project::at(9, here.clone(), true).record()
        };
        let list = super::initial_projects_from(
            Some(&here),
            &dir.join("scratch"),
            &[back[0].clone(), launch_rec],
        );
        assert_eq!(list[0].path, here);
        assert!(list[0].saved && !list[0].listed);
        let o = list.iter().find(|x| x.path == other).unwrap();
        assert_eq!((o.name.as_ref(), o.description.as_ref(), o.pinned), ("Budget work", "Q3 numbers", true));
        assert!(list.last().unwrap().scratch);
        // A folder's own name isn't stored as a custom one.
        assert_eq!(super::Project::at(1, other.clone(), true).record().name, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn another_folder_starts_one_up() {
        assert_eq!(super::parent_dir("/root/work/api-server"), "/root/work");
        assert_eq!(super::parent_dir("/root/work/"), "/root");
        assert_eq!(super::parent_dir("/root"), "/");
        assert_eq!(super::parent_dir("/"), "/");
        assert_eq!(super::parent_dir("~/work"), "~");
        assert_eq!(super::parent_dir("~"), "~");
    }

    #[test]
    fn plain_chats_have_their_own_place_and_no_folder_of_yours() {
        let scratch = std::env::temp_dir().join("forge-scratch-test");
        let none = super::initial_projects(None, &scratch);
        let place: Vec<_> = none.iter().filter(|p| p.scratch).collect();
        assert_eq!(place.len(), 1);
        assert_eq!(place[0].path, scratch);
        assert!(!place[0].saved && place[0].branch.is_empty());
        // Started on a folder, that folder comes first and is a project.
        let here = std::env::temp_dir();
        let on = super::initial_projects(Some(&here), &scratch);
        assert_eq!(on[0].path, here);
        assert!(on[0].saved && !on[0].scratch);
        assert!(on.last().is_some_and(|p| p.scratch));
        let ids: std::collections::HashSet<_> = on.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), on.len());
    }

    #[test]
    fn always_allow_names_the_rule_it_adds() {
        let s = serde_json::json!([
            {"type": "setMode", "mode": "acceptEdits"},
            {"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "npm test:*"}]}
        ]);
        assert_eq!(super::rule_name(&s).as_deref(), Some("Bash(npm test:*)"));
        let s = serde_json::json!([{"type": "addRules", "rules": [{"toolName": "Edit"}]}]);
        assert_eq!(super::rule_name(&s).as_deref(), Some("Edit"));
        assert_eq!(super::rule_name(&serde_json::json!(null)), None);
        assert_eq!(
            super::rule_name(&serde_json::json!([{"type": "setMode"}])),
            None
        );
    }

    #[test]
    fn a_replayed_message_lands_the_queued_ones_it_carries() {
        use super::{take_landed, Queued};
        let q = |u: &str, t: &str| Queued { uuid: u.into(), text: t.to_string().into() };
        // Delivered mid-turn: by its own id.
        let mut list = vec![q("a", "Keep it short."), q("b", "Use lib.rs.")];
        assert_eq!(take_landed(&mut list, "b", "Use lib.rs."), vec![q("b", "Use lib.rs.")]);
        assert_eq!(list, vec![q("a", "Keep it short.")]);
        // Run as a turn of its own, joined, under ForgeCLI's id: by its words.
        let mut list = vec![q("a", "one"), q("b", "two"), q("c", "later")];
        let got = take_landed(&mut list, "cli-uuid", "one\n\ntwo");
        assert_eq!(got.len(), 2);
        assert_eq!(list, vec![q("c", "later")]);
        // Someone else's message: nothing of the queue.
        assert!(take_landed(&mut list, "x", "unrelated").is_empty());
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn a_shown_command_is_dressed_like_the_chat_terminal() {
        let feed = super::AgentFeed {
            title: "Check the branch".into(),
            command: "git branch --show-current && echo \"done\"".into(),
            output: "main\ndone".into(),
            place: None,
            failed: false,
        };
        let text = String::from_utf8(feed.bytes()).unwrap();
        assert!(text.starts_with("\x1b[2;3m# Check the branch"));
        assert!(text.contains("\x1b[1;35m$\x1b[0m \x1b[1;34mgit\x1b[0m"));
        assert!(text.contains("\x1b[36m--show-current\x1b[0m"));
        assert!(text.contains("main\r\ndone\r\n"));
        assert!(text.contains("\x1b[1;32mexit 0") && text.contains("2 lines"));
        let failed = super::AgentFeed { failed: true, ..feed };
        assert!(String::from_utf8(failed.bytes()).unwrap().contains("\x1b[1;31mexit 1"));
    }

    #[test]
    fn always_allow_says_what_it_allows_and_where() {
        use super::always_label as l;
        assert_eq!(l(Some("Bash(npm test:*)")), "Always allow npm test");
        assert_eq!(l(Some("Edit")), "Always allow edits");
        assert_eq!(l(Some("WebFetch(domain:docs.rs)")), "Always allow docs.rs");
        assert_eq!(l(Some("WebSearch")), "Always allow web searches");
        assert_eq!(l(None), "Always allow");
        assert!(l(Some("Bash(cargo build --release --target x86_64-pc-windows-gnu)")).ends_with('…'));
        let s = serde_json::json!([{"type": "addRules", "rules": [{"toolName": "Edit"}], "destination": "localSettings"}]);
        assert_eq!(super::rule_scope(&s), Some("in this project, for you"));
        assert_eq!(super::rule_scope(&serde_json::json!([])), None);
    }

    #[test]
    fn remote_sessions_get_no_secrets() {
        let env: Vec<(String, String)> = [
            ("FORGE_BASE_URL", "https://api.example.com"),
            ("FORGE_API_KEY", "sk-real"),
            ("FORGE_AUTH_TOKEN", "sk-real"),
            ("FORGE_CUSTOM_HEADERS", "x-team: blue\nx-org: 7"),
            ("FORGE_CONTEXT_WINDOW", "200000"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let (route, remote) = super::proxied_env(&env);
        assert_eq!(route.base, "https://api.example.com");
        assert!(route
            .headers
            .contains(&("x-api-key".into(), "sk-real".into())));
        assert!(route
            .headers
            .contains(&("authorization".into(), "Bearer sk-real".into())));
        assert!(route.headers.contains(&("x-team".into(), "blue".into())));
        let flat = format!("{remote:?}");
        assert!(
            !flat.contains("sk-real") && !flat.contains("x-team"),
            "{flat}"
        );
        assert!(remote.contains(&("FORGE_CONTEXT_WINDOW".into(), "200000".into())));
        // OpenAI-compatible: the bearer only.
        let env = vec![
            (
                "FORGE_OPENAI_BASE_URL".to_string(),
                "http://h/v1".to_string(),
            ),
            ("FORGE_OPENAI_API_KEY".to_string(), "k".to_string()),
        ];
        let (route, remote) = super::proxied_env(&env);
        assert_eq!(route.base, "http://h/v1");
        assert_eq!(
            route.headers,
            [("authorization".to_string(), "Bearer k".to_string())]
        );
        assert_eq!(remote[1].1, crate::model_proxy::PLACEHOLDER_KEY);
    }
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
