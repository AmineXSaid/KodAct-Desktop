//! App state: chats, the transcript model, models and modes. `send` hands
//! your message to the chat's KodAct CLI process (`backend.rs`) and its events
//! fill the reply; with `KODACT_DEMO=1` it plays a scripted reply instead, so
//! every part of the UI can be seen working without a model. The chats the
//! sidebar starts with are demo data.
//!
//! The transcript follows KodAct's (the VS Code extension): a user turn, then
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
    /// You said no when KodAct asked, so it never ran.
    Declined,
    /// Stopped while it ran (Stop, or the app closed): it may or may not
    /// have taken effect, so nothing says it failed or worked.
    Interrupted,
}

/// What a call stopped mid-way says about itself.
pub const INTERRUPTED_CALL: &str =
    "Stopped before it finished. It may have taken effect: check before running it again.";

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
    /// The verb KodAct shows: present tense while running, past when done.
    pub fn verb(self, status: ToolStatus) -> &'static str {
        if status == ToolStatus::Declined {
            return "Skipped";
        }
        if status == ToolStatus::Interrupted {
            return "Stopped:";
        }
        // A call that failed changed nothing: it says so, not "Wrote".
        if status == ToolStatus::Failed {
            return match self {
                ToolKind::Read => "Couldn't read",
                ToolKind::Edit => "Couldn't edit",
                ToolKind::Write => "Couldn't write",
                ToolKind::Bash => "Failed:",
                ToolKind::Search => "Search failed:",
                ToolKind::Web => "Couldn't fetch",
                ToolKind::WebSearch => "Web search failed:",
                ToolKind::Other => "Failed:",
            };
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

/// A picture a call returned (an image it read, a screenshot a tool took),
/// shown as a thumbnail at the end of its line.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolImage {
    /// The file it came from, or what the tool called it.
    pub name: SharedString,
    /// Where the file is, so a saved chat finds the picture again.
    pub path: Option<SharedString>,
    pub image: std::sync::Arc<gpui::Image>,
}

impl ToolImage {
    /// The picture in a file, if it is one KodAct can show and not too big
    /// to keep in a chat.
    pub fn from_file(path: &str) -> Option<Self> {
        use gpui::ImageFormat;
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        let format = match ext.as_str() {
            "png" => ImageFormat::Png,
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "gif" => ImageFormat::Gif,
            "webp" => ImageFormat::Webp,
            _ => return None,
        };
        if std::fs::metadata(path).ok()?.len() > 5 * 1024 * 1024 {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        Some(Self {
            name: path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(path)
                .to_string()
                .into(),
            path: Some(path.to_string().into()),
            image: std::sync::Arc::new(gpui::Image::from_bytes(format, bytes)),
        })
    }
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
    /// The pictures it returned.
    pub images: Vec<ToolImage>,
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
    /// Stop cut it off: no exit code, "stopped" in its footer.
    pub stopped: bool,
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
            stopped: c.status == ToolStatus::Interrupted,
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
        let (chip, tone) = match (self.stopped, self.failed) {
            (true, _) => ("stopped", "33"),
            (_, true) => ("exit 1", "31"),
            _ => ("exit 0", "32"),
        };
        out.push_str(&format!(
            "\r\n\x1b[1;{tone}m{chip}\x1b[0m \x1b[2m· {lines} line{}\x1b[0m\r\n",
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
            crate::backend::QUESTION_TOOL => "questions".into(),
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
    /// A file KodAct made for you: a card in the reply, opened beside the chat.
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

/// A chat's `/goal`, as KodAct CLI last reported it: shown above the box
/// while it lasts (`ui/goal_bar.rs`).
#[derive(Clone, Debug, PartialEq)]
pub struct Goal {
    pub condition: String,
    pub status: GoalStatus,
    /// What the last check said: what is missing, or why it is met.
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalStatus {
    /// KodAct keeps working toward it.
    Working,
    /// Stopped by a guard or an interrupt; a message picks it up again.
    Paused,
    Achieved,
    /// The check judged it can't be met from here.
    Failed,
    Cleared,
}

impl Goal {
    /// A `system/goal` line: `{condition, status, reason, paused}`.
    pub fn from_wire(v: &serde_json::Value) -> Option<Goal> {
        let condition = v["condition"].as_str()?.to_string();
        let status = match (v["status"].as_str()?, v["paused"].is_string()) {
            ("active", true) => GoalStatus::Paused,
            ("active", false) => GoalStatus::Working,
            ("achieved", _) => GoalStatus::Achieved,
            ("failed", _) => GoalStatus::Failed,
            ("cleared", _) => GoalStatus::Cleared,
            _ => return None,
        };
        let reason = v["reason"]
            .as_str()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_string);
        Some(Goal {
            condition,
            status,
            reason,
        })
    }
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
    /// What you told KodAct to do instead, with a denial.
    pub note: Option<SharedString>,
    /// AskUserQuestion's questions, with what you chose: the card is a
    /// question card, not a permission prompt.
    pub questions: Vec<Question>,
    /// The question the card shows.
    pub step: usize,
    /// KodAct CLI asks about this tool every time: no "Always allow".
    pub every_time: bool,
}

/// One question AskUserQuestion puts to you, and what you chose.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Question {
    /// A short label for its tab ("Stack").
    pub header: String,
    pub text: String,
    pub options: Vec<QOption>,
    #[serde(default)]
    pub multi: bool,
    /// The options picked, by index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub picked: Vec<usize>,
    /// Your own words instead of an option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

impl Question {
    /// The answer as KodAct hears it: your words, else the labels picked.
    pub fn answer(&self) -> Option<String> {
        if let Some(own) = self.own.as_deref().map(str::trim).filter(|o| !o.is_empty()) {
            return Some(own.to_string());
        }
        let labels: Vec<&str> = self
            .picked
            .iter()
            .filter_map(|&i| self.options.get(i).map(|o| o.label.as_str()))
            .collect();
        (!labels.is_empty()).then(|| labels.join(", "))
    }

    pub fn answered(&self) -> bool {
        self.answer().is_some()
    }
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

/// Pick option `opt` of question `q` (see `AppState::pick_option`).
pub fn pick(qs: &mut [Question], step: &mut usize, q: usize, opt: usize) {
    let Some(question) = qs.get_mut(q) else {
        return;
    };
    if opt >= question.options.len() {
        return;
    }
    question.own = None;
    if question.multi {
        match question.picked.iter().position(|&p| p == opt) {
            Some(i) => {
                question.picked.remove(i);
            }
            None => {
                question.picked.push(opt);
                question.picked.sort_unstable();
            }
        }
        return;
    }
    question.picked = vec![opt];
    if let Some(next) = next_unanswered(qs, q) {
        *step = next;
    }
}

/// Your own words as the answer to the question shown; true once every
/// question has its answer.
pub fn own_answer(qs: &mut [Question], step: &mut usize, text: String) -> bool {
    let q = (*step).min(qs.len().saturating_sub(1));
    if let Some(question) = qs.get_mut(q) {
        question.own = Some(text);
        question.picked.clear();
    }
    match next_unanswered(qs, q) {
        Some(next) => {
            *step = next;
            false
        }
        None => qs.iter().all(Question::answered),
    }
}

/// The first question after `from` (wrapping round) with no answer yet.
fn next_unanswered(qs: &[Question], from: usize) -> Option<usize> {
    (1..=qs.len())
        .map(|d| (from + d) % qs.len())
        .find(|&i| !qs[i].answered())
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
        // An MCP tool, said as its server and its name: KodAct's browser's
        // `mcp__kodact-browser__click` is "browser click".
        (t, "") if t.starts_with("mcp__") => {
            let mut parts = t.trim_start_matches("mcp__").splitn(2, "__");
            // KodAct's own servers read without the prefix (Forge's too, in
            // chats from before the rename).
            let server = parts.next().unwrap_or("");
            let server = server
                .strip_prefix("kodact-")
                .or_else(|| server.strip_prefix("forge-"))
                .unwrap_or(server);
            let name = parts.next().unwrap_or("").replace('_', " ");
            format!("{server} {name}").trim().to_string()
        }
        (_, "") => tool.to_string(),
        (_, c) if c.chars().count() > 28 => {
            format!("{}…", c.chars().take(27).collect::<String>().trim_end())
        }
        (_, c) => c.to_string(),
    };
    format!("Always allow {what}")
}

/// Programs used through a subcommand: their rule keeps it (`git push`,
/// `npm test`), as allowing all of git or npm is not what the click meant.
const SUBCOMMANDED: &[&str] = &[
    "git",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "cargo",
    "docker",
    "kubectl",
    "go",
    "pip",
    "pip3",
    "uv",
    "poetry",
    "dotnet",
    "gh",
    "brew",
    "apt",
    "apt-get",
    "systemctl",
    "terraform",
];

/// A command rule widened to the command itself: `rm perm_a.txt` allows
/// `rm`, `git push origin main` allows `git push`, never one set of options
/// or files (`Bash(rm:*)`, `Bash(git push:*)`).
fn command_of(rule: &str) -> String {
    let first = rule
        .trim_end_matches(":*")
        .split(['&', '|', ';'])
        .next()
        .unwrap_or("")
        .trim();
    let mut words = first
        .split_whitespace()
        .skip_while(|w| w.contains('=') && !w.starts_with('-') && !w.starts_with('='));
    let Some(program) = words.next() else {
        return rule.trim_end_matches(":*").to_string();
    };
    let program = program.rsplit(['/', '\\']).next().unwrap_or(program);
    match words.next() {
        Some(sub)
            if SUBCOMMANDED.contains(&program)
                && !sub.starts_with('-')
                && !sub.contains(['/', '\\', '.']) =>
        {
            format!("{program} {sub}")
        }
        _ => program.to_string(),
    }
}

/// A question's suggested rules with each command rule widened to its
/// command (`command_of`), so "Always allow" names and saves `rm`, not
/// `rm perm_a.txt`.
fn command_rules(mut suggestions: serde_json::Value) -> serde_json::Value {
    for s in suggestions.as_array_mut().into_iter().flatten() {
        if s["type"] != "addRules" {
            continue;
        }
        for r in s["rules"].as_array_mut().into_iter().flatten() {
            let shell = matches!(r["toolName"].as_str(), Some("Bash" | "PowerShell"));
            if let (true, Some(c)) = (shell, r["ruleContent"].as_str().filter(|c| !c.is_empty())) {
                r["ruleContent"] = format!("{}:*", command_of(c)).into();
            }
        }
    }
    suggestions
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

/// One entry in a sub-agent's chronological activity stream: prose or a tool step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEntry {
    Text(String),
    Step(usize),
}

/// A sub-agent KodAct started (a `Task` / `Agent` tool call), as the
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
    /// When it last did anything (started, a step, a step's end, words):
    /// long quiet while it runs, it looks stuck.
    pub heard: std::time::Instant,
    pub failed: bool,
    pub steps: Vec<AgentStep>,
    /// Chronological activity stream: interleaved prose and tool step indices.
    pub entries: Vec<AgentEntry>,
    /// Its latest words; once it ends, its answer.
    pub said: String,
    /// You asked KodAct to stop it, and KodAct hasn't said it has yet.
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
    /// Full target (command, file path, search pattern) for expanding the step.
    pub detail: String,
    /// Output returned by this step's tool call.
    pub output: String,
    pub started: std::time::Instant,
    pub ended: Option<std::time::Instant>,
    pub status: ToolStatus,
    pub open: bool,
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

/// How KodAct CLI's result for a sub-agent you stopped begins (its
/// `kodact_tools::AGENT_STOPPED`).
pub const AGENT_STOPPED: &str = "Stopped by user";

/// What a KodAct CLI too old for `stop_task` is told when asked to stop one agent.
pub const CANT_STOP_AGENT: &str =
    "This KodAct can't stop one agent; update it, or press Esc to stop the whole reply.";

/// The tools that start a sub-agent.
pub fn is_agent_tool(name: &str) -> bool {
    matches!(name, "Task" | "Agent")
}

/// The panel's browser as the UI shows it.
#[derive(Default)]
pub struct WebPage {
    /// The latest picture of the page.
    pub frame: Option<std::sync::Arc<gpui::RenderImage>>,
    /// What that picture shows: its area and the page's scroll, in CSS
    /// pixels (0 when the browser didn't say).
    pub frame_info: crate::web_view::FrameInfo,
    pub url: String,
    pub title: String,
    pub loading: bool,
    /// A deck: the slide in view and how many (`Update::Deck`).
    pub deck: Option<(usize, usize)>,
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

/// A page KodAct is designing, shown live in the browser.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveDesign {
    pub chat: usize,
    /// The page on disk.
    pub path: std::path::PathBuf,
    /// KodAct is writing it now: its draft streaming, or an edit not done.
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

/// A comment on part of a design page (the design bar's comment tool).
#[derive(Clone, Debug, PartialEq)]
pub struct DesignComment {
    /// Its number on its page, as its pin shows it.
    pub n: usize,
    pub chat: usize,
    pub page: std::path::PathBuf,
    pub part: crate::web_view::PagePart,
    pub text: String,
    pub sent: CommentSent,
    /// Done with: its pin is hidden unless resolved ones are shown.
    pub resolved: bool,
}

/// A tool of the design bar's Insert menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertTool {
    /// Drag to move round the page.
    Hand,
    /// Draw this kind of mark.
    Mark(crate::design::marks::Shape),
}

/// A comment on marks, sent once their picture is taken.
struct MarksOut {
    chat: usize,
    page: std::path::PathBuf,
    message: String,
    /// Its place among the comments, to mark it sent.
    n: usize,
}

/// A design system of the library, as the design bar's Library lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemCard {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Its folder, with its `DESIGN.md`.
    pub dir: std::path::PathBuf,
}

/// The artboards the Insert menu offers: a name and a width.
pub const ARTBOARDS: [(&str, u32); 4] = [
    ("Desktop", 1440),
    ("Laptop", 1280),
    ("Tablet", 834),
    ("Phone", 390),
];

/// A theme colour as CSS, for what KodAct draws on a page.
fn css(c: gpui::Hsla) -> String {
    format!(
        "hsla({:.0}, {:.0}%, {:.0}%, {:.2})",
        c.h * 360.0,
        c.s * 100.0,
        c.l * 100.0,
        c.a
    )
}

/// Where a design comment went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentSent {
    /// Kept as a note for yourself (no @KodAct).
    Note,
    /// Waiting for KodAct's reply to end, then sent.
    Waiting,
    /// Sent to KodAct.
    Sent,
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

/// A file KodAct made: shown as a card in the reply (`ui/file_card.rs`),
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

    /// A web page that is not a diagram: it previews in the browser.
    pub fn is_html(&self) -> bool {
        matches!(self.kind().as_ref(), "HTML" | "HTM") && !self.is_diagram()
    }

    /// A picture the panel shows (`ui/image_view.rs`).
    pub fn is_image(&self) -> bool {
        self.bytes.is_some() && crate::ui::image_view::format_of(&self.name).is_some()
    }

    pub fn is_markdown(&self) -> bool {
        matches!(self.kind().as_ref(), "MD" | "MARKDOWN")
    }

    /// A diagram KodAct draws itself (`diagram.rs`): an SVG, or a Diagram
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
    /// Your message: its text, the files you attached, and the id KodAct CLI
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
    /// Its KodAct CLI conversation's id (`--session-id`, then `--resume`) and
    /// its file name in the history (`history.rs`).
    pub session: String,
    /// Stable working folder id under `workspace/chats/` for plain chats, kept
    /// across `new_conversation()`.
    pub folder: Option<String>,
    /// KodAct CLI has written this conversation: the next process resumes it.
    pub resumable: bool,
    /// When it last changed, for the sidebar's dates and the history's order.
    pub updated: SystemTime,
    /// Earlier versions of the conversation, replaced by an edit or a retry.
    pub versions: Vec<Version>,
    /// A design chat: what its design prompt is built from
    /// (`design::prompt`). None for an ordinary chat.
    pub design: Option<crate::design::prompt::Setup>,
    /// Put away: out of the sidebar's list, until its filter shows archived chats.
    pub archived: bool,
    /// The transcript in detail: every run of work and every thought opened.
    pub detailed: bool,
}

/// The width a web page previews at in the file view: the panel's own, a
/// tablet's or a phone's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Device {
    #[default]
    Desktop,
    Tablet,
    Phone,
}

impl Device {
    /// The page's width in CSS pixels; None is the room the panel gives it.
    pub fn width(self) -> Option<u32> {
        match self {
            Device::Desktop => None,
            Device::Tablet => Some(768),
            Device::Phone => Some(390),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Device::Desktop => "Desktop",
            Device::Tablet => "Tablet",
            Device::Phone => "Phone",
        }
    }
}

/// A menu open in the file view's toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocMenu {
    Zoom,
    More,
    /// A Markdown file's title: Rename, Versions.
    Title,
    /// Its headings, to jump to.
    Outline,
    /// Editing, Viewing or Markdown.
    MdMode,
    /// The page toolbar's text style.
    TextStyle,
    /// The page toolbar's "…": bold, italic, link, code.
    Format,
}

/// A row of the Files tab's explorer, right-clicked.
#[derive(Clone, Debug, PartialEq)]
pub enum ExplorerItem {
    /// A file KodAct wrote in this chat.
    Chat(FileRef),
    /// A file or folder of the project, by its project-relative path.
    Project { rel: String, dir: bool },
}

/// What the explorer's right-click menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplorerAction {
    Open,
    Download,
    Attach,
    CopyPath,
    CopyRelative,
    Reveal,
}

/// A change to the deck in the panel (`AppState::deck_edit`).
#[derive(Clone, Debug, PartialEq)]
pub enum DeckEdit {
    /// A shape on slide `slide` moved, resized, filled, added…
    Shape {
        slide: usize,
        op: crate::pptx_shapes::ShapeOp,
    },
    Format {
        slide: usize,
        para: usize,
        format: crate::pptx_edit::Format,
    },
    Slides(crate::pptx_edit::SlideOp),
}

/// A shape being dragged on a slide: what, how, and where it is now.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeDrag {
    pub slide: usize,
    pub id: u32,
    /// None to move it; a corner (0 top left, 1 top right, 2 bottom left,
    /// 3 bottom right) to resize it.
    pub corner: Option<u8>,
    pub start: gpui::Point<gpui::Pixels>,
    /// Pixels per EMU on screen.
    pub k: f32,
    pub from: (i64, i64, i64, i64),
    pub now: (i64, i64, i64, i64),
}

impl ShapeDrag {
    /// The shape's place for the pointer at `at`.
    pub fn at(&self, at: gpui::Point<gpui::Pixels>) -> (i64, i64, i64, i64) {
        let dx = (f32::from(at.x - self.start.x) / self.k) as i64;
        let dy = (f32::from(at.y - self.start.y) / self.k) as i64;
        let (x, y, w, h) = self.from;
        let min = 91_440; // a tenth of an inch
        match self.corner {
            None => (x + dx, y + dy, w, h),
            Some(0) => (x + dx.min(w - min), y + dy.min(h - min), (w - dx).max(min), (h - dy).max(min)),
            Some(1) => (x, y + dy.min(h - min), (w + dx).max(min), (h - dy).max(min)),
            Some(2) => (x + dx.min(w - min), y, (w - dx).max(min), (h + dy).max(min)),
            _ => (x, y, (w + dx).max(min), (h + dy).max(min)),
        }
    }
}

/// How the side panel shows a Markdown file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdMode {
    /// A page you type on, saved as Markdown as you go (`ui/md_page.rs`).
    Editing,
    /// The page as it reads, to select and comment on.
    Viewing,
    /// Its Markdown source in the code editor.
    Markdown,
}

/// The title bar's menus, and the sidebar's (its More row, your name).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleMenu {
    Share,
    More,
    /// The sidebar's More: shortcuts, the guide, settings.
    Help,
    /// Your name at the sidebar's foot: settings, models, the theme.
    Account,
}

/// The chat menu's submenus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatSub {
    OpenIn,
    View,
    Move,
    Export,
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

/// The composer's design menu's tabs: what a design chat is set up with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignTab {
    /// The kind of work (`design::prompt::Surface`).
    Work,
    /// The design system.
    Look,
    /// The template.
    Start,
    Skill,
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
/// it was, and the KodAct CLI conversation it had (restoring it resumes that).
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
    /// Brings `file`'s card in the reply being written up to date; the file
    /// back when it needs a card of its own (`AppState::place_card`).
    fn update_card(&mut self, file: FileRef, edit: bool) -> Option<FileRef> {
        if let Some(Message::Assistant { blocks }) = self.messages.last_mut() {
            let shown = blocks.iter_mut().find_map(|b| match b {
                Block::File(f) if crate::doc_versions::same_file(f, &file) => Some(f),
                _ => None,
            });
            if let Some(f) = shown {
                if *f != file {
                    *f = file;
                    self.version += 1;
                }
                return None;
            }
        }
        (!edit || file.is_diagram()).then_some(file)
    }

    /// A new, empty chat with a fresh conversation id.
    pub fn new(id: usize, project: usize, title: SharedString) -> Self {
        let session = crate::history::new_id();
        Self {
            id,
            project,
            title,
            minutes_ago: 0,
            pinned: false,
            messages: vec![],
            version: 0,
            unread: false,
            folder: Some(session.clone()),
            session,
            resumable: false,
            updated: SystemTime::now(),
            versions: vec![],
            design: None,
            archived: false,
            detailed: false,
        }
    }

    /// Start a new KodAct CLI conversation for this chat (its old one can't
    /// follow: the transcript was cut back or couldn't be resumed), keeping
    /// its plain-chat working folder stable.
    pub fn new_conversation(&mut self) {
        if self.folder.is_none() {
            self.folder = Some(self.session.clone());
        }
        self.session = crate::history::new_id();
        self.resumable = false;
    }

    /// Record the end of a turn; only a turn that succeeded without an error
    /// guarantees KodAct CLI wrote a resumable session file.
    pub fn record_turn_end(
        &mut self,
        seconds: f32,
        usage: Option<crate::context::Usage>,
        succeeded: bool,
    ) {
        self.messages.push(Message::TurnEnd { seconds, usage });
        self.version += 1;
        if succeeded {
            self.resumable = true;
        }
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

/// Where you are, for back and forward: the screen, and on a chat which.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Place {
    pub screen: Screen,
    pub chat: Option<usize>,
}

/// How many places back remembers.
const NAV_DEPTH: usize = 50;

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
    /// Every file KodAct made, across chats (`ui/documents.rs`).
    Documents,
    /// The design library to start from: templates, examples, design
    /// systems and skills (`ui/design_home.rs`).
    Design,
}

/// The design gallery's lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalleryTab {
    Templates,
    Examples,
    Systems,
    Skills,
}

impl GalleryTab {
    pub const ALL: [GalleryTab; 4] = [
        GalleryTab::Templates,
        GalleryTab::Examples,
        GalleryTab::Systems,
        GalleryTab::Skills,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GalleryTab::Templates => crate::i18n::tr("Templates"),
            GalleryTab::Examples => crate::i18n::tr("Examples"),
            GalleryTab::Systems => crate::i18n::tr("Design systems"),
            GalleryTab::Skills => crate::i18n::tr("Skills"),
        }
    }

    pub fn kind(self) -> crate::design::library::Kind {
        use crate::design::library::Kind;
        match self {
            GalleryTab::Templates => Kind::Template,
            GalleryTab::Examples => Kind::Example,
            GalleryTab::Systems => Kind::DesignSystem,
            GalleryTab::Skills => Kind::Skill,
        }
    }
}

/// A gallery card's picture.
#[derive(Clone, Debug, PartialEq)]
pub enum Thumb {
    Queued,
    Ready(std::path::PathBuf),
    Failed,
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
    Compaction,
    /// Not listed until KodAct CLI reads a proxy and extra certificates
    /// (`planned::NETWORK`): a page there would change nothing yet.
    #[allow(dead_code)]
    Network,
    Environments,
    Memory,
    Plugins,
    Guide,
    About,
}

impl SettingsTab {
    /// The editors backed by KodAct CLI's files (`forge_files.rs`): open, they
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
                | SettingsTab::Compaction
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

/// What the Settings editors show: KodAct CLI's files for this project.
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
    /// Each MCP server's state as `kodact mcp list` last said it.
    pub mcp_status: std::collections::BTreeMap<String, String>,
    pub mcp_checking: bool,
    /// What went wrong with the last check (no KodAct CLI, it failed…).
    pub mcp_error: Option<String>,
    /// The settings profiles (`setting_profiles.rs`), read with the catalog.
    pub profiles: Vec<crate::setting_profiles::SettingsProfile>,
}

/// A folder KodAct works in. Its chats, rules and memory belong to it.
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
    /// What it is for, in your words (KodAct doesn't read it).
    pub description: SharedString,
    /// Archived: off the lists, its chats and files kept.
    pub archived: bool,
    /// When it was made and last opened (seconds since 1970).
    pub created: u64,
    pub opened_at: u64,
    /// KodAct keeps a memory for it (`.forge/memory.md`).
    pub memory_on: bool,
    /// On your Projects page. The folder KodAct was started on is a
    /// project for that run without joining the list, unless you saved it.
    pub listed: bool,
    /// The model (a profile's name) opening the project picks, and the
    /// mode its new chats start in; None: yours.
    pub model: Option<String>,
    pub default_mode: Option<ModeId>,
    /// You made it a project (Open folder…, Save as project). A remote
    /// folder you connected to is worked in without becoming one.
    pub saved: bool,
    /// Plain chats' place: no folder of yours, KodAct's own (`scratch_dir`).
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

/// One project in `projects.json`: everything KodAct keeps about it besides
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

/// A chat's KodAct CLI process, the task applying its events, and the
/// profile it was started on.
struct Live {
    session: crate::backend::Session,
    _pump: Task<()>,
    profile: crate::endpoints::Profile,
    /// Effective `--effort` passed when this process started (`None` = Auto).
    effort: Option<crate::endpoints::Effort>,
    /// `AppState::behaviour_version` when it started: the nudges and
    /// reasoning carry-over it read from settings.json.
    behaviour: u64,
    /// Which process this is: events from one that was replaced are dropped.
    generation: u64,
    /// Started as a design chat with this setup (its prompt and tools are
    /// fixed for a process).
    design: Option<crate::design::prompt::Setup>,
    /// Relays carrying the remote's ports back here (WSL, containers):
    /// they end with the session.
    _relays: Vec<std::sync::Arc<crate::relay::Relay>>,
}

/// A permission question KodAct is waiting on, and where its card is.
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

/// The side-question card's copy, as KodAct for VS Code has it.
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

/// A chat's side questions: answered by KodAct CLI from the conversation,
/// without tools, while the conversation goes on untouched.
#[derive(Clone, Debug, Default)]
pub struct Side {
    pub visible: bool,
    /// Folded to its title bar, still asking and answering.
    pub minimized: bool,
    /// Grown, for a long answer: wider, with a taller thread.
    pub expanded: bool,
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
pub const INTERRUPTED: &str = "Interrupted. Tell KodAct what to do instead.";

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

/// Said while KodAct CLI summarizes the conversation, and when it has.
pub const COMPACTING: &str = "Compacting the conversation…";
pub const COMPACTED: &str = "Compacted: earlier messages were summarized to free context. KodAct still knows what was decided.";

/// Said under a reply that was still running when KodAct closed.
pub const CLOSED_MID_REPLY: &str =
    "KodAct was closed before this reply finished. Send a message to pick it up.";

/// A `rewind_files` request for KodAct CLI's turn `turn`.
fn json_rewind(turn: &str, dry_run: bool) -> serde_json::Value {
    serde_json::json!({ "subtype": "rewind_files", "user_message_id": turn, "dry_run": dry_run })
}

/// Whether KodAct CLI's absolute path `full` is the tool row's `target`
/// (absolute, or relative to the project).
fn same_file(full: &str, target: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/");
    let (f, t) = (norm(full), norm(target));
    f == t || f.ends_with(&format!("/{}", t.trim_start_matches("./")))
}

/// What Continue sends: KodAct picks up where Stop cut it off.
const CONTINUE_PROMPT: &str = "Continue from where you stopped.";

/// The largest file the Files pane's viewer shows, in bytes.
const FILES_VIEW_MAX: usize = 1 << 20;

/// `KODACT_DEMO=1`: replies are the scripted demo instead of KodAct CLI.
pub(crate) fn demo_mode() -> bool {
    std::env::var("KODACT_DEMO").is_ok_and(|v| v == "1")
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
    /// Allow Bypass permissions (it runs every tool without asking)?
    Bypass,
}

/// A command `/` offers, and the group it is listed under.
#[derive(Clone, Debug)]
pub struct CompletionCommand {
    pub name: String,
    pub args: String,
    pub description: String,
    pub group: &'static str,
    pub kind: crate::commands::CommandKind,
}

/// Ephemeral modal showing the output of an out-of-band slash command (`/status`, `/doctor`…).
#[derive(Clone, Debug, PartialEq)]
pub struct CommandModal {
    pub command: SharedString,
    pub output: SharedString,
    pub pending: bool,
    pub is_error: bool,
}

#[derive(Clone, Debug)]
enum ImmediateSink {
    Side,
    Modal(SharedString),
}

#[derive(Clone, Debug)]
struct EphemeralTurn {
    command: SharedString,
    output: String,
    is_error: bool,
    toast_only: bool,
}

/// A control request sent to a chat's KodAct CLI, and what its answer is for.
#[derive(Clone, Debug)]
pub enum Request {
    /// "Which files would reverting to before this message change?"
    RevertPlan { chat: usize, msg: usize },
    /// Revert them.
    Revert { chat: usize, msg: usize },
    /// Stop these sub-agents (every running one when you chose "Stop all").
    StopAgents { chat: usize, ids: Vec<String> },
    /// Each MCP server's state, for the Session column.
    McpStatus { chat: usize },
    /// An MCP server started again.
    McpReconnect { chat: usize, server: String },
    /// Take back a queued message; `edit` puts it back in the box.
    CancelQueued {
        chat: usize,
        uuid: String,
        edit: bool,
    },
    /// Cancel queueing in CLI and send that queued message right away.
    SendQueuedNow { chat: usize, uuid: String },
    /// What fills the chat's context, for the Session column.
    ContextUsage { chat: usize },
    /// A mode switch: `chat` asked to go from `from` to `mode`. Refused, KodAct CLI stays in
    /// `from`, and the chip goes back to it; the toast says why.
    SetMode {
        chat: usize,
        from: ModeId,
        mode: ModeId,
    },
}

/// The queued messages a replayed message of yours carries, taken out of
/// `list`: the one with its id (delivered mid-turn), else those its words
/// hold (they ran as a turn of their own, alone or joined, under KodAct CLI's id).
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

/// A message you sent while a reply ran, waiting for KodAct's next step.
#[derive(Clone, Debug, PartialEq)]
pub struct Queued {
    pub uuid: String,
    pub text: SharedString,
    /// What it carries: pictures, pasted texts, files.
    pub attachments: Vec<crate::attach::Attachment>,
}

// ---------------------------------------------------------------- the state

pub struct AppState {
    /// What KodAct calls the person; None until the first-run screen sets it.
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
    /// The sidebar lists archived chats instead of the others.
    pub show_archived: bool,
    /// The Session column beside the chat (`ui/session_panel.rs`) is open.
    pub session_open: bool,
    /// Its cards folded shut, by name ("Environment", "Context", "Changes").
    pub session_folded: Vec<&'static str>,
    /// What fills each chat's context, as KodAct CLI last counted it.
    pub context_usage: std::collections::HashMap<usize, crate::context::ContextUsage>,
    /// The title bar's project switcher, and the sidebar's chat filter, are open.
    pub switcher_open: bool,
    /// The composer's project menu (the same list as the switcher, above its chip).
    pub project_menu: bool,
    pub filter_open: bool,
    /// The current project's name, shown in the title bar.
    pub project: SharedString,
    /// The current project's folder; plain chats' place when it is none.
    pub project_dir: std::path::PathBuf,
    /// The folder KodAct was started on, if any (`launch_dir`).
    pub launch_dir: Option<std::path::PathBuf>,
    /// Remote targets KodAct CLI was put on this run (`remote.rs`).
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
    /// The design tools' HTTP servers for remote projects, by target and
    /// folder (`ensure_design_http`).
    design_http: std::collections::HashMap<String, (u16, String)>,
    pub screen: Screen,
    pub branch: SharedString,
    pub chats: Vec<Chat>,
    pub current: Option<usize>,
    /// Back and forward, as Claude Desktop has them: the places you were
    /// (a chat, a project, Projects, Rules), the one you're at, and whether
    /// the next change of place is a step through them (`go_back`).
    pub nav_back: Vec<Place>,
    pub nav_forward: Vec<Place>,
    nav_at: Option<Place>,
    nav_stepping: bool,
    pub sidebar_open: bool,
    /// LibreOffice is being installed (`install_libreoffice`).
    pub office_installing: bool,
    /// LibreOffice is on this computer, as last looked (off the UI thread).
    pub libreoffice: bool,
    pub search: SharedString,
    /// Endpoint profiles (each one endpoint and its model); None while
    /// they load. No model is built in.
    pub endpoints: Option<crate::endpoints::Endpoints>,
    /// Each profile's last check (`health.rs`).
    pub health: crate::health::Store,
    /// Each gateway profile's model list (`capabilities.catalog`), as it
    /// last answered, for the model menu.
    pub catalogs: std::collections::HashMap<String, Vec<crate::endpoints::ModelInfo>>,
    /// Gateway models put to the test, by address (`health::ModelTests`).
    pub model_tests: crate::health::ModelTests,
    /// A test running, by address: models done, and how many it tries.
    pub tests_running: std::collections::HashMap<String, (usize, usize)>,
    /// Models that failed a test are listed (greyed, with why) rather than hidden.
    pub show_failed_models: bool,
    /// The effort level, when the model takes one; None is Auto.
    pub effort: Option<crate::endpoints::Effort>,
    /// The most tokens the model may think for (`set_max_thinking_tokens`):
    /// None is the model's default, `Some(0)` no thinking. Sent to each
    /// chat's KodAct CLI as it starts, and to the live one when changed.
    pub thinking_budget: Option<u32>,
    /// Each chat's MCP servers, as its KodAct CLI last reported them.
    pub mcp_servers: std::collections::HashMap<usize, Vec<crate::backend::McpServer>>,
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
    /// The current chat's mode (Manual, Edit automatically, Plan, Bypass).
    pub mode: ModeId,
    /// Modes chats keep when reopened (see `ModeId::kept`), by chat.
    kept_modes: std::collections::HashMap<usize, ModeId>,
    /// The mode new chats start in (Settings > General).
    pub default_mode: ModeId,
    /// Bypass permissions has been allowed, behind its warning.
    pub bypass_allowed: bool,
    /// KodAct CLI's nudges and reasoning carry-over, as settings.json has
    /// them (Settings > General, `behaviour.rs`).
    pub behaviour: Vec<(&'static str, bool)>,
    /// Bumped by each change to them: a chat's process started before one
    /// restarts on its next message.
    behaviour_version: u64,
    /// The web browser tools (`browser_mcp.rs`) are given to new KodAct CLI sessions.
    pub browser: bool,
    /// The Modes menu is open; its Bypass warning is showing.
    pub modes_open: bool,
    /// The Reasoning chip's menu is open (`ui::reasoning_menu`).
    pub reasoning_open: bool,
    /// Profiles whose reasoning is being checked (`detect_reasoning`).
    pub reasoning_checking: std::collections::HashSet<String>,
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
    /// The page KodAct is designing: the browser shows it as it is written
    /// (`design::live`).
    pub design_live: Option<LiveDesign>,
    /// Whether the panel follows KodAct's design. Off once you take the
    /// browser yourself, or stop it; on again at the next reply.
    pub design_follow: bool,
    /// The browser's zoom (1 is the page's own size), as a browser zooms.
    pub web_zoom: f64,
    /// The panel's room for the page, last told the browser (CSS pixels).
    pub web_room: (u32, u32),
    /// The page on screen fills the window, as a presentation: no chat,
    /// no browser bars (Esc ends it).
    pub presenting: bool,
    /// The comment tool is on: pointing at the page outlines a part, and a
    /// click opens a comment on it.
    pub design_select: bool,
    /// The part under the pointer, and the one a comment is being written on.
    pub design_hover: Option<crate::web_view::PagePart>,
    pub design_pick: Option<crate::web_view::PagePart>,
    /// The comments on the chats' design pages, oldest first.
    pub design_comments: Vec<DesignComment>,
    /// Show the comments' pins on the page (and the resolved ones too).
    pub design_pins: bool,
    pub design_resolved: bool,
    /// Comments waiting for a chat's reply to end, by chat.
    pub design_held:
        std::collections::HashMap<usize, Vec<(String, Vec<crate::attach::Attachment>)>>,
    /// The Insert tool in hand: drawing marks, or moving the page.
    pub design_tool: Option<InsertTool>,
    /// Marks drawn on the chats' design pages: (chat, page, mark).
    pub design_marks: Vec<(usize, std::path::PathBuf, crate::design::marks::Mark)>,
    /// The comment open is on the page's marks, not on a part.
    pub design_pick_marks: bool,
    /// A comment with marks waiting for their picture.
    design_marks_out: Option<MarksOut>,
    /// The artboard the page is shown in: its name and width (the zoom
    /// follows the panel to keep that width in view).
    pub design_artboard: Option<(&'static str, u32)>,
    /// The order you dragged the chats' pages into, by chat.
    pub design_order: std::collections::HashMap<usize, Vec<std::path::PathBuf>>,
    /// The library's design systems, read once (None until then; empty
    /// when there is no library).
    pub design_systems: Option<std::sync::Arc<Vec<SystemCard>>>,
    design_systems_asked: bool,
    /// The whole design library's index, for design chats' prompts: read
    /// once, off the main thread, when the first design chat needs it.
    pub design_index: Option<std::sync::Arc<crate::design::library::Index>>,
    design_index_asked: bool,
    /// The pet beside the chat (`design::pets`), by id; None for none.
    pub pet: Option<String>,
    /// Requests KodAct runs on a schedule (`design::routines`).
    pub routines: Vec<crate::design::routines::Routine>,
    /// The design gallery's list on show.
    pub gallery_tab: GalleryTab,
    /// Gallery cards' pictures, by preview page (`design::export::thumbnail`),
    /// and the pages waiting for one, in the order they show.
    pub design_thumbs: std::collections::HashMap<std::path::PathBuf, Thumb>,
    design_thumb_queue: std::collections::VecDeque<std::path::PathBuf>,
    design_thumbs_running: bool,
    /// The Agents tab's pill strip: its scroll, and the pill it last
    /// brought into view (`agent_strip.rs`).
    pub agent_strip: gpui::ScrollHandle,
    pub agent_strip_at: std::cell::Cell<Option<usize>>,
    /// The jumper row to give the keyboard focus on the next frame: the
    /// agent's index, or `usize::MAX` for the current one (as the list
    /// opens), so ↑ ↓ Enter work at once (`agent_jumper.rs`).
    pub agent_jumper_focus: std::cell::Cell<Option<usize>>,
    /// Chosen before the chat exists (on a new chat's screen): the chat
    /// your next message starts is a design chat set up with this.
    pub design_next: Option<crate::design::prompt::Setup>,
    /// The composer's design menu (`ui/design_chips.rs`): open, and on
    /// which tab.
    pub design_menu: Option<DesignTab>,
    /// Where the parts the page's comments and marks are on are now, by
    /// selector (`Update::Anchors`); the selectors last given the browser;
    /// new marks waiting to hear their part, by request.
    pub design_anchors: std::collections::HashMap<String, (f64, f64, f64, f64)>,
    design_anchors_sent: Vec<String>,
    design_anchor_wait: std::collections::HashMap<u64, (std::path::PathBuf, Vec<(f64, f64)>)>,
    design_anchor_next: u64,
    /// Layout guides: every box on the page outlined.
    pub design_guides: bool,
    /// The page's outline beside it, and the headings it lists.
    pub design_outline: bool,
    pub design_headings: Vec<crate::web_view::Heading>,
    /// Each design page's versions (its text, before KodAct's first change
    /// and after each), and which one is on disk: undo and redo.
    pub design_versions:
        std::collections::HashMap<std::path::PathBuf, (Vec<std::sync::Arc<str>>, usize)>,
    /// A deck's slide pictures for the panel's rail, by page and version
    /// (`design::export::slide_thumbs`); asked once each.
    pub design_slides:
        std::collections::HashMap<(std::path::PathBuf, usize), Vec<std::path::PathBuf>>,
    design_slides_asked: std::collections::HashSet<(std::path::PathBuf, usize)>,
    /// Pages made beside a chat's own (duplicates), by chat.
    pub design_extra: Vec<(usize, std::path::PathBuf)>,
    /// Each chat's sub-agents, in the order they started (`agents_view.rs`).
    pub agents: std::collections::HashMap<usize, Vec<AgentRun>>,
    /// Sub-agent prompt cards opened to show their full prompt, by agent id.
    pub agents_open: std::collections::HashSet<String>,
    /// Which sub-agent is focused in the Agents panel, by chat id.
    pub selected_agent: std::collections::HashMap<usize, String>,
    /// The Agents panel is expanded across the main session area (Full View).
    pub panel_full: bool,
    /// A diagram in the panel fits its width (else it shows at actual size).
    pub doc_fit: bool,
    /// The side panel sits below the chat instead of beside it.
    pub doc_below: bool,
    /// The document pane is expanded across the full main stage.
    pub doc_full: bool,
    /// An Office file or PDF in the panel, read: by file name and content
    /// (`name#key`), the preview or why it couldn't be read. None while it
    /// is being read.
    /// Each file card's first page, by its bytes' key (`office_render::key`):
    /// `None` while it is drawn, or when it cannot be (no LibreOffice, a
    /// file it cannot read); the card then shows the app's mark.
    pub card_thumbs:
        std::sync::Arc<std::collections::HashMap<u64, Option<std::sync::Arc<gpui::Image>>>>,
    /// Each Office file's or PDF's page (slide) count, by the same key, once
    /// its first page is drawn: a card says "12 slides".
    pub card_pages: std::sync::Arc<std::collections::HashMap<u64, usize>>,
    /// The cards whose first page has been drawn or could not be: until
    /// then a card shows a placeholder the page's size.
    pub card_drawn: std::sync::Arc<std::collections::HashSet<u64>>,
    /// Each file card's strip (`strip.rs`), by `strip::key`; None when the
    /// file can't be shown (the card then shows its type). Absent while built.
    pub card_strips:
        std::sync::Arc<std::collections::HashMap<u64, Option<std::sync::Arc<crate::strip::Strip>>>>,
    /// A page or slide to bring up once the panel's file has drawn its pages
    /// (a strip's frame was clicked before they were ready).
    doc_jump: Option<usize>,
    pub doc_preview: Option<(
        SharedString,
        Result<std::sync::Arc<crate::office::Preview>, String>,
    )>,
    /// The sheet shown, for a workbook.
    pub doc_sheet: usize,
    /// A document in the panel shows its text, not its pages.
    pub doc_text: bool,
    /// A workbook in the panel shows its pages as they print (charts, bars and
    /// all), not its cells.
    pub sheet_print: bool,
    /// Those pages, for the version shown (`doc_version_key`), once drawn.
    pub sheet_pages: Option<(SharedString, Result<std::sync::Arc<crate::office::Doc>, String>)>,
    /// The panel's file or plan shows its source, not its preview.
    pub source_view: bool,
    /// A Markdown file shows as it reads (else as a page to type on, or,
    /// with `source_view`, its source).
    pub md_viewing: bool,
    /// A passage the page editor picked to comment on, for the comment box.
    pub doc_comment_hold: Option<crate::doc_versions::Anchor>,
    /// When the page editor last wrote the file.
    pub md_saved_at: Option<std::time::Instant>,
    /// The page editor scrolls to this block (the outline's pick).
    pub md_jump: Option<usize>,
    /// The explorer's right-click menu: the row, and where the click was.
    pub explorer_menu: Option<(ExplorerItem, gpui::Point<gpui::Pixels>)>,
    /// The slide the deck editor shows after a change to the deck.
    pub deck_focus: Option<usize>,
    /// The deck's shapes, slide by slide, for its version, and its size.
    pub deck_shapes: Option<(
        SharedString,
        std::sync::Arc<Vec<Vec<crate::pptx_shapes::Shape>>>,
        (i64, i64),
    )>,
    /// The shape picked on a slide: (slide, id).
    pub deck_sel: Option<(usize, u32)>,
    pub deck_drag: Option<ShapeDrag>,
    /// The shape bar's fill menu is open.
    pub deck_fill_menu: bool,
    /// The engine new chats run (`engine.rs`): its path, its version, and
    /// whether it is one you put in (else the bundled one).
    pub engine_info: Option<(std::path::PathBuf, Result<String, String>, bool)>,
    /// The plan version the Plan tab shows (from 0); None, the latest.
    pub plan_version: Option<usize>,
    /// The cells picked in a workbook in the panel (anchor, other end).
    pub doc_pick: crate::ui::sheet_view::Pick,
    /// The panel shows what its version changed from the one before.
    pub doc_show_changes: bool,
    /// Comments on files in the panel, as Claude Docs keeps them: each
    /// on a passage, a page or cells, until it is sent and answered.
    pub doc_comments: Vec<crate::doc_versions::Comment>,
    /// Files you attached and opened in the panel, as they came (by chat):
    /// each is its file's first version, before KodAct edits the copy.
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
    /// The chat menu's submenu that is open (Open in, Transcript view…).
    pub chat_sub: Option<ChatSub>,
    /// The title bar's Share or ⋮ menu, and where it opens.
    pub title_menu: Option<(TitleMenu, gpui::Point<gpui::Pixels>)>,
    /// Where the chat menu opens: under the title, or where it was asked for.
    pub chat_menu_at: gpui::Point<gpui::Pixels>,
    /// The panel's find bar is open (`ui/doc_find.rs`).
    pub doc_finding: bool,
    /// The panel's body: its pages are its items, so the keyboard and a
    /// search result can bring one to the top.
    pub doc_scroll: gpui::ScrollHandle,
    /// Each text file a reply is writing, by chat and path: its content as
    /// far as it has streamed (`Event::Drafting`), for the live view.
    pub drafts: std::collections::HashMap<(usize, String), SharedString>,
    /// The file the side panel follows while KodAct writes it: its chat and
    /// path. Its draft replaces the open file's text as it streams, and the
    /// panel keeps to the newest line. Ends when the write ends.
    pub doc_live: Option<(usize, String)>,
    next_comment: usize,
    /// A page (or slide) of the panel's document picked to comment on, from 0.
    pub doc_page_pick: Option<usize>,
    /// How each file in the panel is scaled, by file (`doc_versions::file_id`).
    pub doc_zoom: std::collections::HashMap<String, crate::doc_zoom::Zoom>,
    /// Which files show their thumbnail rail (a deck or PDF does by default).
    pub doc_thumbs: std::collections::HashMap<String, bool>,
    /// The file view's open menu: the zoom list, or the tools that did not fit.
    pub doc_menu: Option<DocMenu>,
    /// The text file in the panel has edits not saved: its file id and the
    /// text in the editor (`ui/text_editor.rs`).
    pub doc_edit: Option<(String, SharedString)>,
    /// Words changed in a Word file or deck in the panel, by (slide, notes,
    /// paragraph): the new text (`ui/office_editor.rs`).
    pub doc_words: std::collections::BTreeMap<(usize, bool, usize), String>,
    /// The width a web page in the panel previews at, and the version of the
    /// page the browser last loaded.
    pub web_device: Device,
    html_previewed: Option<SharedString>,
    /// Where the last mouse move of a drag to pan a picture was.
    pub doc_drag: std::rc::Rc<std::cell::Cell<Option<gpui::Point<gpui::Pixels>>>>,
    /// The percentage the open file's pages are drawn at when fitted, for
    /// the toolbar and for stepping from (set by the view as it draws).
    pub doc_fitted: std::sync::Arc<std::sync::atomic::AtomicU16>,
    /// Sent comments on a Word file are also written into it as Word comments.
    pub doc_word_comments: bool,
    /// Those changes, for the version they belong to (`doc_version_key`);
    /// None inside while they could not be worked out.
    pub doc_changes: Option<(SharedString, Option<crate::doc_versions::Changes>)>,
    copied_reset: Option<Task<()>>,
    /// A short message at the bottom of the window (stubs say what they would do).
    pub toast: Option<SharedString>,
    /// The file the toast is about (a download): it offers Open and Show in folder.
    pub toast_file: Option<std::path::PathBuf>,
    toast_reset: Option<Task<()>>,
    /// The replies being played, by chat. Each keeps going while you look at
    /// another chat; dropping one stops it.
    replies: std::collections::HashMap<usize, Task<()>>,
    /// Each chat's KodAct CLI process (`backend.rs`), kept between turns, and
    /// the task that applies its events.
    sessions: std::collections::HashMap<usize, Live>,
    /// Turn results still to come from interrupted turns, by chat: until
    /// they arrive, what the process says belongs to the stopped turn.
    stale_turns: std::collections::HashMap<usize, u32>,
    /// Where each tool call's row is (message, block), by chat and call id.
    tool_rows: std::collections::HashMap<(usize, String), (usize, usize)>,
    /// When each command started: a document it leaves behind, changed
    /// since, gets its card (`documents_made`).
    command_started: std::collections::HashMap<(usize, String), SystemTime>,
    /// The permission question each chat is waiting on.
    asks: std::collections::HashMap<usize, Ask>,
    /// What KodAct is doing about your latest message in each chat, in a few
    /// words the model wrote (`title::headline`), for the working line.
    pub headlines: std::collections::HashMap<usize, String>,
    /// Messages sent while each chat's reply runs, oldest first, until they
    /// reach KodAct (`Event::UserAccepted`).
    pub queued: std::collections::HashMap<usize, Vec<Queued>>,
    /// Each chat's `/goal` while it lasts.
    pub goals: std::collections::HashMap<usize, Goal>,
    /// Chats kept busy between a goal's turns, and not by a turn of their own.
    goal_waiting: std::collections::HashSet<usize>,
    /// When each running turn started.
    turn_started: std::collections::HashMap<usize, std::time::Instant>,
    /// Chats whose next text starts a new block rather than running on.
    fresh_block: std::collections::HashSet<usize>,
    /// The Settings window is open, on this section.
    pub settings_open: bool,
    pub settings_tab: SettingsTab,
    /// Settings should open the Add endpoint form (taken by the view).
    pub settings_add: bool,
    /// Settings > Skills shows its Install panel (`ui/skill_install.rs`).
    pub skills_install: bool,
    /// Settings > Plugins shows its marketplaces (`ui/marketplaces.rs`).
    pub plugins_market: bool,
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
    /// User-resized side panel share of the space after dock (`None`: default width).
    pub panel_share: Option<f32>,
    /// The side panel's left edge is being dragged.
    pub panel_resizing: bool,
    /// Pending unsaved cell edits for the workbook open in the side panel.
    pub sheet_edits: crate::office::CellEdits,
    /// Extra blank rows appended per sheet index while editing.
    pub sheet_extra_rows: std::collections::HashMap<usize, usize>,
    /// Extra blank columns appended per sheet index while editing.
    pub sheet_extra_cols: std::collections::HashMap<usize, usize>,
    /// Incremented when double-clicking a cell or pressing Enter so SheetBar grabs focus.
    pub sheet_focus_seq: u64,
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
    /// Each diagram's zoom, when not fitted to the column (1.0 = fitted).
    pub diagram_zoom: std::sync::Arc<std::collections::HashMap<SharedString, f32>>,
    /// Chats are still loading: screens show placeholders.
    pub loading: bool,
    load: Option<Task<()>>,
    /// Where chats are kept between launches (`history.rs`); None keeps them in memory only.
    history: Option<std::path::PathBuf>,
    /// Chats changed since they were last written, and the pending write.
    dirty: std::collections::HashSet<usize>,
    save_task: Option<Task<()>>,
    /// Documents each chat is making this turn, watched so their cards and
    /// the side panel redraw as they change (`live_docs.rs`).
    pub live: std::collections::HashMap<usize, crate::live_docs::Watch>,
    live_task: Option<Task<()>>,
    /// Chats whose first document of the turn already opened the panel.
    live_opened: std::collections::HashSet<usize>,
    /// Chats whose process is resuming a conversation and hasn't spoken yet:
    /// if it ends first, the conversation couldn't be reopened.
    resuming: std::collections::HashSet<usize>,
    /// Chats whose KodAct CLI process was started afresh mid-conversation and
    /// still needs `<conversation_history>` prepended on the next non-command turn.
    history_unseeded: std::collections::HashSet<usize>,
    /// Keeps the sidebar's "5m" / "2h" true as time passes.
    clock: Option<Task<()>>,
    /// Counts KodAct CLI processes started, to tell their events apart.
    generations: u64,
    /// Files waiting in the composer to go with the next message.
    pub attachments: Vec<crate::attach::Attachment>,
    /// The last attachment id given (a chip is matched to its result by it).
    attachment_seq: u64,
    /// Pasted images so far, to number their names.
    pasted: usize,
    /// Pasted texts so far, likewise.
    pasted_texts: usize,
    /// The attachment open in the editor (`attachment_editor.rs`): a pasted
    /// text to edit, or a picture to draw on. By its id.
    pub editing_attachment: Option<u64>,
    /// Answers KodAct CLI gives at once, in the order asked, by chat: modal
    /// output for an out-of-band command (`/status`), or the `/btw` side card.
    immediate_queue: std::collections::HashMap<usize, std::collections::VecDeque<ImmediateSink>>,
    /// Out-of-band turn-based CLI slash commands (`/doctor`, `/reload-plugins`…)
    /// running without entering `c.messages`.
    ephemeral_turns: std::collections::HashMap<usize, EphemeralTurn>,
    /// Ephemeral output sheet for out-of-band inspection slash commands.
    pub command_modal: Option<CommandModal>,
    /// Control requests waiting on KodAct CLI's answer, by request id.
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
    /// The Files pane (`ui/files_pane.rs`) is open: the project's tree
    /// beside a viewer, under any file open in the side panel.
    pub files_open: bool,
    /// The file in its viewer (project-relative) and its text once read, or
    /// why it can't show.
    pub files_view: Option<(String, Option<Result<SharedString, String>>)>,
    /// Replies whose "Edited N files" card shows every file, by (chat, message).
    pub edited_open: Vec<(usize, usize)>,
    /// A jump to show: its number, the message, and the block in it to
    /// bring into view (None: the message's top).
    pub jump: Option<(u64, usize, Option<usize>)>,
    jump_seq: u64,
    /// Runs of calls opened to their steps: (chat, message, first block).
    pub open_groups: std::collections::HashSet<(usize, usize, usize)>,
    /// Transient presentation-only UI toggles (show more lines, sub-groups, collapsed intermediate notes).
    pub ui_toggles: std::collections::HashSet<String>,
    /// Last outside-press positions for transient UI toggles so clicking the trigger closes cleanly.
    pub ui_dismissed: std::collections::HashMap<String, gpui::Point<gpui::Pixels>>,
    /// Last title/footer menu dismissed by clicking outside, with its press position,
    /// so clicking the trigger button while open closes cleanly instead of reopening.
    pub title_menu_dismissed: Option<(TitleMenu, gpui::Point<gpui::Pixels>)>,
    /// Last remote indicator press that dismissed its popup, for click-open / click-close.
    pub remote_dismissed_at: Option<gpui::Point<gpui::Pixels>>,
    /// The row a jump landed on, marked for a moment: (chat, message, block).
    pub flash: Option<(usize, usize, usize)>,
    /// When each thought began and ended: (chat, message, block).
    thought: std::collections::HashMap<
        (usize, usize, usize),
        (std::time::Instant, Option<std::time::Instant>),
    >,
    /// The task panel and the approvals list are open.
    pub tasks_open: bool,
    pub approvals_open: bool,
    pub tasks_dismissed_at: Option<gpui::Point<gpui::Pixels>>,
    pub approvals_dismissed_at: Option<gpui::Point<gpui::Pixels>>,
    /// Chats whose running turn is a compaction.
    compacting: std::collections::HashSet<usize>,
    /// KodAct CLI's own files, for the Settings editors.
    pub engine: Engine,
    /// Settings → Models → Diagnose: the endpoint checked and its steps.
    pub diagnosis: Option<Diagnosis>,
    /// Counts diagnoses, so a stale run's steps never land on a new one.
    diagnosis_seq: u64,
    /// The project map is open, and its last read (None while reading).
    pub map_open: bool,
    pub map: Option<ProjectMap>,
    /// The settings profile each chat's KodAct CLI gets (`--settings`).
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
        // Made at start, so KodAct CLI has a folder to run in; a failure shows
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
            design_http: Default::default(),
            branch: first.branch.clone(),
            current_project: first.id,
            projects,
            all_chats: true,
            show_archived: false,
            session_open: true,
            session_folded: vec![],
            context_usage: Default::default(),
            switcher_open: false,
            project_menu: false,
            filter_open: false,
            screen: Screen::Chat,
            current: None,
            nav_back: vec![],
            nav_forward: vec![],
            nav_at: None,
            nav_stepping: false,
            chats,
            sidebar_open: true,
            office_installing: false,
            libreoffice: false,
            search: "".into(),
            endpoints: None,
            health: Default::default(),
            catalogs: Default::default(),
            model_tests: Default::default(),
            tests_running: Default::default(),
            show_failed_models: false,
            effort: None,
            thinking_budget: None,
            mcp_servers: Default::default(),
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
            bypass_allowed: crate::app_settings::load_window().bypass_allowed,
            behaviour: crate::behaviour::load(),
            behaviour_version: 0,
            browser: crate::browser::enabled(),
            modes_open: false,
            reasoning_checking: Default::default(),
            reasoning_open: false,
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
            web_zoom: 1.0,
            web_room: (0, 0),
            presenting: false,
            design_select: false,
            design_hover: None,
            design_pick: None,
            design_comments: Vec::new(),
            design_pins: true,
            design_resolved: false,
            design_held: Default::default(),
            design_tool: None,
            design_marks: Vec::new(),
            design_pick_marks: false,
            design_marks_out: None,
            design_artboard: None,
            design_order: Default::default(),
            design_systems: None,
            design_systems_asked: false,
            design_index: None,
            design_index_asked: false,
            pet: crate::design::pets::chosen(),
            routines: crate::design::routines::load(),
            gallery_tab: GalleryTab::Templates,
            design_thumbs: Default::default(),
            design_thumb_queue: Default::default(),
            design_thumbs_running: false,
            agent_strip: Default::default(),
            agent_strip_at: Default::default(),
            agent_jumper_focus: Default::default(),
            design_next: None,
            design_anchors: Default::default(),
            design_anchors_sent: vec![],
            design_anchor_wait: Default::default(),
            design_anchor_next: 0,
            design_menu: None,
            design_guides: false,
            design_outline: false,
            design_headings: Vec::new(),
            design_versions: Default::default(),
            design_slides: Default::default(),
            design_slides_asked: Default::default(),
            design_extra: Vec::new(),
            agents: std::collections::HashMap::new(),
            agents_open: std::collections::HashSet::new(),
            selected_agent: std::collections::HashMap::new(),
            panel_full: false,
            doc_fit: true,
            doc_below: false,
            doc_full: false,
            doc_preview: None,
            card_thumbs: Default::default(),
            card_pages: Default::default(),
            card_drawn: Default::default(),
            card_strips: Default::default(),
            doc_jump: None,
            doc_text: false,
            sheet_print: false,
            sheet_pages: None,
            source_view: false,
            md_viewing: false,
            doc_comment_hold: None,
            md_saved_at: None,
            md_jump: None,
            explorer_menu: None,
            deck_focus: None,
            deck_shapes: None,
            deck_sel: None,
            deck_drag: None,
            deck_fill_menu: false,
            engine_info: None,
            plan_version: None,
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
            chat_sub: None,
            title_menu: None,
            chat_menu_at: Default::default(),
            doc_scroll: gpui::ScrollHandle::new(),
            drafts: Default::default(),
            doc_live: None,
            next_comment: 1,
            doc_page_pick: None,
            doc_zoom: Default::default(),
            doc_thumbs: Default::default(),
            doc_menu: None,
            doc_drag: Default::default(),
            doc_edit: None,
            doc_words: Default::default(),
            web_device: Device::Desktop,
            html_previewed: None,
            doc_fitted: std::sync::Arc::new(std::sync::atomic::AtomicU16::new(100)),
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
            command_started: Default::default(),
            asks: Default::default(),
            headlines: Default::default(),
            queued: Default::default(),
            goals: Default::default(),
            goal_waiting: Default::default(),
            turn_started: Default::default(),
            fresh_block: Default::default(),
            settings_open: false,
            settings_tab: SettingsTab::General,
            settings_add: false,
            skills_install: false,
            plugins_market: false,
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
            panel_share: crate::app_settings::load_window().panel_share,
            panel_resizing: false,
            sheet_edits: Default::default(),
            sheet_extra_rows: Default::default(),
            sheet_extra_cols: Default::default(),
            sheet_focus_seq: 0,
            agent_feed: None,
            askpass: Default::default(),
            model_routes: Default::default(),
            relay_errors: async_channel::unbounded().0,
            diagram_code: Default::default(),
            diagram_zoom: Default::default(),
            loading: true,
            load: None,
            history: crate::history::dir(),
            dirty: Default::default(),
            save_task: None,
            live: Default::default(),
            toast_file: None,
            live_task: None,
            live_opened: Default::default(),
            resuming: Default::default(),
            history_unseeded: Default::default(),
            clock: None,
            generations: 0,
            attachments: vec![],
            attachment_seq: 0,
            pasted: 0,
            pasted_texts: 0,
            editing_attachment: None,
            immediate_queue: Default::default(),
            ephemeral_turns: Default::default(),
            command_modal: None,
            requests: Default::default(),
            file_index: None,
            find_request: None,
            custom_commands: None,
            skill_commands: vec![],
            completions_loading: false,
            context_open: false,
            versions_open: None,
            edited_open: vec![],
            files_open: false,
            files_view: None,
            jump: None,
            jump_seq: 0,
            open_groups: Default::default(),
            ui_toggles: Default::default(),
            ui_dismissed: Default::default(),
            title_menu_dismissed: None,
            remote_dismissed_at: None,
            flash: None,
            thought: Default::default(),
            tasks_open: false,
            approvals_open: false,
            tasks_dismissed_at: None,
            approvals_dismissed_at: None,
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
        // A sub-agent that goes quiet shows as stalled even when nothing
        // else in its chat changes: every few seconds, a chat with a stalled
        // agent is drawn again (its "No word for …" counting on).
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(10))
                .await;
            if this.update(cx, |s, cx| s.redraw_stalled(cx)).is_err() {
                break;
            }
        })
        .detach();
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
            let (endpoints, health, tests) = cx
                .background_spawn(async move {
                    let e = crate::endpoints::load();
                    let h = crate::health::load(&e.profiles);
                    (e, h, crate::health::load_tests())
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                let minutes = endpoints.sync_interval_minutes;
                s.endpoints = Some(endpoints);
                s.health = health;
                // Results that came in while the file was read win.
                for (url, models) in tests {
                    let kept = s.model_tests.entry(url).or_default();
                    for (id, verdict) in models {
                        kept.entry(id).or_insert(verdict);
                    }
                }
                s.load_catalogs(cx);
                s.detect_reasoning_if_unknown(cx);
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

    /// Read every gateway's model list again, off the UI thread; each
    /// lands in `catalogs` as it answers (one that fails keeps its last).
    pub fn load_catalogs(&mut self, cx: &mut Context<Self>) {
        let Some(e) = &self.endpoints else { return };
        let gateways: Vec<_> = e
            .profiles
            .iter()
            .filter(|p| p.capabilities.catalog)
            .cloned()
            .collect();
        self.catalogs
            .retain(|name, _| gateways.iter().any(|p| &p.name == name));
        for p in gateways {
            cx.spawn(async move |this, cx| {
                let name = p.name.clone();
                let listed = cx
                    .background_spawn(async move {
                        let key = crate::health::token(&p.auth).ok().flatten();
                        crate::endpoints::list_catalog(
                            &p.base_url,
                            p.wire,
                            key.as_deref(),
                            &p.headers,
                        )
                    })
                    .await;
                if let Ok(models) = listed {
                    let _ = this.update(cx, |s, cx| {
                        s.catalogs.insert(name, models);
                        cx.notify();
                    });
                }
            })
            .detach();
        }
    }

    /// Switch gateway profile `name` to model `id` from its list (its name
    /// and context window with it), save it, and use it.
    pub fn pick_catalog_model(&mut self, name: &str, id: &str, cx: &mut Context<Self>) {
        let info = self
            .catalogs
            .get(name)
            .and_then(|c| c.iter().find(|m| m.id == id))
            .cloned()
            .unwrap_or_else(|| crate::endpoints::ModelInfo {
                id: id.to_string(),
                ..Default::default()
            });
        let Some(e) = self.endpoints.as_mut() else {
            return;
        };
        let Some(p) = e.profiles.iter_mut().find(|p| p.name == name) else {
            return;
        };
        p.model = info.id.clone();
        p.display_name = info.name.clone();
        p.capabilities.context_window = info.context;
        let saved = p.clone();
        cx.background_spawn(async move {
            let _ = crate::endpoints::save_profile(&saved, Some(&saved.name));
        })
        .detach();
        self.select_profile(name, cx);
        // The new model is checked as the others are.
        self.sync_all(cx);
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
        self.detect_reasoning_if_unknown(cx);
        cx.notify();
    }

    /// The reasoning level the next message asks for (`None`: Auto, the
    /// model decides). A chat's KodAct CLI restarts on its next message with
    /// it (`send_to_forge`).
    pub fn set_effort(&mut self, effort: Option<crate::endpoints::Effort>, cx: &mut Context<Self>) {
        self.effort = effort;
        self.reasoning_open = false;
        cx.notify();
    }

    /// The model in use, checked for reasoning once: the first time it is
    /// used, so the Reasoning chip offers what it really takes.
    pub fn detect_reasoning_if_unknown(&mut self, cx: &mut Context<Self>) {
        if self
            .active_profile()
            .is_some_and(|p| p.capabilities.reasoning.is_none())
        {
            self.detect_reasoning(cx);
        }
    }

    /// Ask the model in use whether it reasons and which levels it takes
    /// (`reasoning_detect`), in the background; the verdict sets the
    /// profile's effort levels and is saved with it. The Reasoning chip's
    /// refresh runs it again.
    pub fn detect_reasoning(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.active_profile().cloned() else {
            return;
        };
        if !self.reasoning_checking.insert(profile.name.clone()) {
            return;
        }
        cx.notify();
        let probe = profile.clone();
        let task = cx.background_spawn(async move {
            crate::reasoning_detect::detect(&probe, Duration::from_secs(120))
        });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            let _ = this.update(cx, |s, cx| {
                s.reasoning_checking.remove(&profile.name);
                match found {
                    Ok(r) => s.apply_reasoning(&profile.name, r, cx),
                    Err(e) => s.show_toast(
                        format!("Couldn't check {}'s reasoning: {e}", profile.model),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A verdict, into the profile: the levels it takes (none when it
    /// doesn't reason or its level is fixed), saved; a level chosen that it
    /// doesn't take goes back to Auto.
    fn apply_reasoning(
        &mut self,
        name: &str,
        found: crate::reasoning_detect::Reasoning,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = self
            .endpoints
            .as_mut()
            .and_then(|e| e.profiles.iter_mut().find(|p| p.name == name))
        else {
            return;
        };
        let levels = found.levels();
        p.capabilities.effort = !levels.is_empty();
        p.capabilities.effort_levels = (!levels.is_empty()).then(|| levels.clone());
        p.capabilities.reasoning = Some(found.clone());
        let saved = p.clone();
        let model = saved.model.clone();
        cx.background_spawn(async move {
            let _ = crate::endpoints::save_profile(&saved, Some(&saved.name));
        })
        .detach();
        if self.active_profile().is_some_and(|a| a.name == name)
            && self.effort.is_some_and(|e| !levels.contains(&e))
        {
            self.effort = None;
        }
        self.show_toast(format!("{model}: {}", found.summary()), cx);
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

    /// Try each of `ids` on the gateway `base` describes (its address,
    /// format and headers; the model is set per try) with one short
    /// request, a few at a time, and keep the verdicts. A model already
    /// being tried is not tried twice.
    pub fn test_models(
        &mut self,
        base: crate::endpoints::Profile,
        key: Option<String>,
        ids: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let url = crate::health::tests_key(&base.base_url);
        if ids.is_empty() || self.tests_running.contains_key(&url) {
            return;
        }
        self.tests_running.insert(url.clone(), (0, ids.len()));
        let queue = std::sync::Arc::new(std::sync::Mutex::new(
            ids.into_iter().collect::<std::collections::VecDeque<_>>(),
        ));
        let (tx, rx) = async_channel::unbounded::<(String, crate::health::ModelHealth)>();
        for _ in 0..crate::health::TEST_PARALLELISM {
            let (queue, tx, base, key) = (queue.clone(), tx.clone(), base.clone(), key.clone());
            cx.background_spawn(async move {
                loop {
                    let Some(id) = queue.lock().ok().and_then(|mut q| q.pop_front()) else {
                        break;
                    };
                    let profile = crate::endpoints::Profile {
                        model: id.clone(),
                        ..base.clone()
                    };
                    let verdict = crate::health::probe_with(
                        &profile,
                        key.clone(),
                        crate::health::TEST_TIMEOUT,
                    )
                    .unwrap_or_else(|e| crate::health::ModelHealth {
                        id: id.clone(),
                        servable: false,
                        ms: 0,
                        detail: Some(e),
                        checked_at: crate::health::now_ms(),
                    });
                    if tx.send_blocking((id, verdict)).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        drop(tx);
        cx.spawn(async move |this, cx| {
            while let Ok((id, verdict)) = rx.recv().await {
                let url = url.clone();
                let alive = this.update(cx, |s, cx| {
                    s.model_tests
                        .entry(url.clone())
                        .or_default()
                        .insert(id, verdict);
                    if let Some((done, _)) = s.tests_running.get_mut(&url) {
                        *done += 1;
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            let _ = this.update(cx, |s, cx| {
                s.tests_running.remove(&url);
                let tests = s.model_tests.clone();
                cx.background_spawn(async move { crate::health::save_tests(&tests) })
                    .detach();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn toggle_failed_models(&mut self, cx: &mut Context<Self>) {
        self.show_failed_models = !self.show_failed_models;
        cx.notify();
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
                archived: c.archived,
                detailed: c.detailed,
                messages,
                folder: c.folder,
                session: c.session,
                resumable: c.resumable,
                updated,
                versions,
                design: c.design,
                ..Chat::new(id, project, c.title.into())
            });
            if let Some(d) = c.design_pages {
                self.design_restore(id, d);
            }
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
                let mut saved = crate::history::save_form(c, &dir);
                saved.design_pages = Some(self.design_saved(c.id)).filter(|d| !d.is_empty());
                saved
            })
            .collect()
    }

    /// A chat's design pages' comments, marks, order and added pages, as
    /// written with it.
    fn design_saved(&self, chat: usize) -> crate::design::store::SavedDesign {
        use crate::design::store::{SavedComment, SavedDesign};
        SavedDesign {
            comments: self
                .design_comments
                .iter()
                .filter(|c| c.chat == chat)
                .map(|c| SavedComment {
                    n: c.n,
                    page: c.page.clone(),
                    part: c.part.clone(),
                    text: c.text.clone(),
                    sent: match c.sent {
                        CommentSent::Note => "note",
                        CommentSent::Waiting => "waiting",
                        CommentSent::Sent => "sent",
                    }
                    .into(),
                    resolved: c.resolved,
                })
                .collect(),
            marks: self
                .design_marks
                .iter()
                .filter(|(c, _, _)| *c == chat)
                .map(|(_, p, m)| (p.clone(), m.clone()))
                .collect(),
            order: self.design_order.get(&chat).cloned().unwrap_or_default(),
            extra: self
                .design_extra
                .iter()
                .filter(|(c, _)| *c == chat)
                .map(|(_, p)| p.clone())
                .collect(),
        }
    }

    /// A chat's design state back from its file.
    fn design_restore(&mut self, chat: usize, d: crate::design::store::SavedDesign) {
        for c in d.comments {
            self.design_comments.push(DesignComment {
                n: c.n,
                chat,
                page: c.page,
                part: c.part,
                text: c.text,
                sent: if c.sent == "sent" {
                    CommentSent::Sent
                } else {
                    CommentSent::Note
                },
                resolved: c.resolved,
            });
        }
        self.design_marks
            .extend(d.marks.into_iter().map(|(p, m)| (chat, p, m)));
        if !d.order.is_empty() {
            self.design_order.insert(chat, d.order);
        }
        self.design_extra
            .extend(d.extra.into_iter().map(|p| (chat, p)));
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
    /// Clear the chat's goal (`/goal clear`). While a reply runs it is
    /// stopped first: KodAct CLI pauses the goal, and Clear then ends it.
    pub fn clear_goal(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        match self.goals.get(&chat).map(|g| g.status) {
            Some(GoalStatus::Working | GoalStatus::Paused) if self.busy() => self.stop(cx),
            Some(GoalStatus::Working | GoalStatus::Paused) => self.send("/goal clear".into(), cx),
            // A goal met or given up is over: only its strip goes.
            _ => {
                self.goals.remove(&chat);
                cx.notify();
            }
        }
    }

    /// Between a goal's turns: the chat stays busy while KodAct CLI checks
    /// the goal and starts the next turn (`system/goal` ends the wait).
    fn goal_keeps_working(&mut self, chat: usize) {
        if !self.replies.contains_key(&chat) {
            self.replies.insert(chat, Task::ready(()));
            self.turn_started.insert(chat, std::time::Instant::now());
            self.goal_waiting.insert(chat);
        }
    }

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
        self.project_menu = false;
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
    fn place(&self) -> Place {
        Place {
            screen: self.screen,
            chat: (self.screen == Screen::Chat)
                .then_some(self.current)
                .flatten(),
        }
    }

    /// Note where you are now (the workspace calls it after each change):
    /// a new place pushes the last one on back and clears forward, unless
    /// it came from stepping back or forward.
    pub fn track_place(&mut self) {
        let here = self.place();
        if self.nav_at != Some(here) {
            if let Some(was) = self.nav_at.filter(|_| !self.nav_stepping) {
                self.nav_back.push(was);
                if self.nav_back.len() > NAV_DEPTH {
                    self.nav_back.remove(0);
                }
                self.nav_forward.clear();
            }
            self.nav_at = Some(here);
        }
        self.nav_stepping = false;
    }

    /// A place that is still there: its chat or project not deleted since.
    fn reachable(&self, p: &Place) -> bool {
        match p.screen {
            Screen::Chat => p
                .chat
                .is_none_or(|id| self.chats.iter().any(|c| c.id == id)),
            Screen::Project(id) => self.project(id).is_some(),
            Screen::Rules | Screen::Projects | Screen::Documents | Screen::Design => true,
        }
    }

    /// Back (`forward` false) or forward: to the nearest place still there.
    pub fn step_place(&mut self, forward: bool, cx: &mut Context<Self>) {
        loop {
            let next = if forward {
                self.nav_forward.pop()
            } else {
                self.nav_back.pop()
            };
            let Some(next) = next else {
                return;
            };
            if !self.reachable(&next) {
                continue;
            }
            if let Some(at) = self.nav_at {
                if forward {
                    self.nav_back.push(at);
                } else {
                    self.nav_forward.push(at);
                }
            }
            self.nav_stepping = true;
            match (next.screen, next.chat) {
                (Screen::Chat, Some(id)) => self.open_chat(id, cx),
                (Screen::Project(id), _) => self.show_project(id, cx),
                (screen, _) => {
                    self.screen = screen;
                    if screen == Screen::Chat {
                        self.current = None;
                    }
                    cx.notify();
                }
            }
            return;
        }
    }

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
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
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

    /// Export project `id`'s setup as a `.kodactproject` in Downloads.
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
                        f.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    )
                })
            },
            cx,
        );
    }

    /// Import a `.kodactproject` into project `id`.
    pub fn share_import(&mut self, id: usize, file: std::path::PathBuf, cx: &mut Context<Self>) {
        self.share_step(
            id,
            move |path| {
                crate::project_share::import(&file, path).map(|r| {
                    match (r.added.len(), r.kept.len()) {
                        (0, 0) => "That setup was empty.".to_string(),
                        (a, 0) => format!("Imported {a} file{}.", if a == 1 { "" } else { "s" }),
                        (a, k) => format!(
                            "Imported {a} file{}; kept {k} this project already had ({}).",
                            if a == 1 { "" } else { "s" },
                            r.kept.join(", ")
                        ),
                    }
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

    /// New project › Start fresh: a folder in `KodAct Projects` made from a
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

    /// Clone a repository (GitHub, GitLab, or custom self-hosted host) with an
    /// optional Personal Access Token. The token reaches git only through an
    /// environment variable read by a credential helper given on this one
    /// command (`clone_command`): never on the command line, in a file, in
    /// `.git/config`, or in the user's own credential store.
    pub fn clone_project_with_auth(
        &mut self,
        url: String,
        token: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let url = url.trim().to_string();
        let token = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let Some(dir) = crate::templates::projects_home()
            .and_then(|h| crate::templates::clone_target(&h, &url))
        else {
            return self.show_toast(
                "Paste a repository's address, like https://github.com/owner/name.",
                cx,
            );
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
                    let out = crate::templates::clone_command(&url, &d, token.as_deref())
                        .output()
                        .map_err(|e| match e.kind() {
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
        self.chat_sub = None;
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

    /// Start a chat in project `id` asking KodAct about something (an issue,
    /// a pull request, a failing run).
    pub fn ask_forge(&mut self, id: usize, text: String, cx: &mut Context<Self>) {
        self.select_project(id, cx);
        self.new_chat(cx);
        self.send(text, cx);
    }

    /// The local folder chat `id` works in (None for a remote one).
    /// A chat with no folder of yours: it works in KodAct's scratch place.
    fn chat_is_plain(&self, id: usize) -> bool {
        self.chats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| self.project(c.project))
            .is_some_and(|p| p.scratch)
    }

    fn chat_project_dir(&self, id: usize) -> Option<std::path::PathBuf> {
        let chat = self.chats.iter().find(|c| c.id == id)?;
        let p = self
            .project(chat.project)
            .filter(|p| crate::remote::Remote::parse(&p.path).is_none())?;
        Some(if p.scratch {
            let folder_id = chat.folder.as_deref().unwrap_or(&chat.session);
            chat_folder(&p.path, folder_id, chat.resumable && chat.folder.is_none())
        } else {
            p.path.clone()
        })
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
    pub fn knowledge_add(
        &mut self,
        id: usize,
        paths: Vec<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.knowledge_change(
            id,
            move |p| {
                crate::knowledge::add(p, &paths).map(|n| match n {
                    0 => "Those files are in the knowledge already.".into(),
                    1 => {
                        "Added to the knowledge, On demand: KodAct reads it when it matters.".into()
                    }
                    n => format!("Added {n} files to the knowledge, On demand."),
                })
            },
            cx,
        );
    }

    /// Add the picked files and folders of a GitHub repository's `tree` to
    /// project `id`'s knowledge (`github_knowledge.rs`).
    pub fn knowledge_github_add(
        &mut self,
        id: usize,
        tree: crate::github_knowledge::Tree,
        picks: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let repo = tree.repo.clone();
        self.knowledge_change(
            id,
            move |p| {
                crate::github_knowledge::add(p, &tree, &picks).map(|n| match n {
                    1 => format!("Added 1 file from {repo} to the knowledge."),
                    n => format!("Added {n} files from {repo} to the knowledge."),
                })
            },
            cx,
        );
    }

    /// Bring a GitHub repository's files in project `id`'s knowledge up to
    /// its branch's latest.
    pub fn knowledge_github_sync(&mut self, id: usize, repo: String, cx: &mut Context<Self>) {
        self.knowledge_change(
            id,
            move |p| {
                crate::github_knowledge::sync(p, &repo).map(|d| {
                    if d.unchanged {
                        format!("{repo} is up to date.")
                    } else if d.removed > 0 {
                        format!(
                            "Synced {repo}: {} files, {} gone upstream taken out.",
                            d.updated, d.removed
                        )
                    } else {
                        format!("Synced {repo}: {} files.", d.updated)
                    }
                })
            },
            cx,
        );
    }

    /// Take a GitHub repository, and its files, out of project `id`'s knowledge.
    pub fn knowledge_github_remove(&mut self, id: usize, repo: String, cx: &mut Context<Self>) {
        self.knowledge_change(
            id,
            move |p| {
                crate::github_knowledge::remove(p, &repo)
                    .map(|_| format!("Took {repo} out of the knowledge."))
            },
            cx,
        );
    }

    /// Add a note you wrote to project `id`'s knowledge.
    pub fn knowledge_add_text(
        &mut self,
        id: usize,
        title: String,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.knowledge_change(
            id,
            move |p| crate::knowledge::add_text(p, &title, &text).map(|_| String::new()),
            cx,
        );
    }

    pub fn knowledge_set_always(
        &mut self,
        id: usize,
        rel: String,
        always: bool,
        cx: &mut Context<Self>,
    ) {
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

    /// Save project `id`'s instructions: shared (its KODACT.md) or yours
    /// only (KODACT.local.md). Empty removes the file.
    pub fn save_instructions(
        &mut self,
        id: usize,
        mine: bool,
        text: String,
        cx: &mut Context<Self>,
    ) {
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
    /// under All projects). The folder KodAct started in always stays.
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
    /// KodAct started on (its details, without listing it). Plain chats'
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
        self.follow_chat_folder(id);
        if self.session_open {
            self.ask_context_usage(id);
            self.ask_mcp_status(id);
        }
        cx.notify();
    }

    /// A plain chat's own folder is where the app looks for its files (the
    /// Files tab, `@`, a file's place on disk) while it is open.
    fn follow_chat_folder(&mut self, id: usize) {
        let plain = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| self.project(c.project))
            .is_some_and(|p| p.scratch);
        if let Some(dir) = self.chat_project_dir(id).filter(|_| plain) {
            if self.project_dir != dir {
                self.project_dir = dir;
                self.file_index = None;
            }
        }
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

    /// A chat's mode as KodAct CLI now runs it (it changed it itself, or a
    /// plan's answer did): the mode chip follows, and the mode is kept for
    /// the chat's next session.
    fn mode_reported(&mut self, chat: usize, mode: ModeId) {
        if self.current == Some(chat) {
            self.mode = mode;
            self.keep_mode();
        } else if mode.kept(self.bypass_allowed) {
            self.kept_modes.insert(chat, mode);
        } else {
            self.kept_modes.remove(&chat);
        }
    }

    /// The mode a chat's KodAct CLI runs in: the showing chat's, or what
    /// another chat kept.
    fn chat_mode(&self, chat: usize) -> ModeId {
        if self.current == Some(chat) {
            self.mode
        } else {
            self.kept_modes.get(&chat).copied().unwrap_or(self.mode)
        }
    }

    /// Whether KodAct answers a permission request itself (`approve`): a
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
            let from = self.mode;
            self.mode = mode;
            self.modes_open = false;
            self.bypass_warning = false;
            self.keep_mode();
            if let Some(chat) = self.current {
                if let Some(Ok(id)) = self.sessions.get(&chat).map(|l| l.session.set_mode(mode)) {
                    self.requests
                        .insert(id, Request::SetMode { chat, from, mode });
                }
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
        // Auto lets edits through as Edit automatically does; a command
        // KodAct CLI still asks about in Auto is one it judged risky.
        let allowed = match mode.permission().cli_name() {
            "bypassPermissions" => crate::modes::bypasses(mode, &ask.tool_name),
            "acceptEdits" | "auto" if edit => true,
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
        self.save_window();
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
                "Your name could not be saved; KodAct will ask again next time.",
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
    /// back to that message, and KodAct answers it afresh.
    pub fn retry(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        let Some(text) = self.take_from(chat, msg, cx) else {
            return;
        };
        self.send(text, cx);
    }

    /// Edit one of your messages: the conversation goes back to before it,
    /// and its text waits in the composer to be changed and sent.
    pub fn edit(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        // Its pasted texts and pictures go back in the box with its words.
        let kept: Vec<crate::attach::Attachment> = match self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| c.messages.get(msg))
        {
            Some(Message::User { attachments, .. }) => attachments
                .iter()
                .filter(|a| {
                    a.kind == crate::attach::Kind::Pasted
                        || (matches!(a.kind, crate::attach::Kind::Image(_)) && a.bytes.is_some())
                })
                .cloned()
                .collect(),
            _ => vec![],
        };
        if let Some(text) = self.take_from(chat, msg, cx) {
            self.draft = Some(text);
            for mut a in kept {
                self.attachment_seq += 1;
                a.id = self.attachment_seq;
                self.attachments.push(a);
            }
            cx.notify();
        }
    }

    /// Cut the chat back to before message `msg` (one of yours), returning
    /// its text and the superseded session id.
    fn truncate_chat_from(&mut self, chat: usize, msg: usize) -> Option<(String, String)> {
        self.asks.remove(&chat);
        self.stale_turns.remove(&chat);
        self.turn_started.remove(&chat);
        self.tool_rows.retain(|(c, _), _| *c != chat);
        // Ratings of the replies being cut go with them.
        self.ratings.retain(|(c, m), _| *c != chat || *m < msg);
        let scratch_root = self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| self.project(c.project))
            .filter(|p| p.scratch)
            .map(|p| p.path.clone());
        let c = self.chat_mut(chat)?;
        let Some(Message::User { text, .. }) = c.messages.get(msg) else {
            return None;
        };
        let text = text.to_string();
        let old_session = c.session.clone();
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
        if let Some(root) = scratch_root {
            if c.folder.is_none() && chat_folder(&root, &c.session, c.resumable) == root {
                c.folder = Some(String::new());
            }
        }
        c.new_conversation();
        let has_prior = prior_conversation_block(&c.messages).is_some();
        if has_prior {
            self.history_unseeded.insert(chat);
        } else {
            self.history_unseeded.remove(&chat);
        }
        Some((text, old_session))
    }

    /// Cut the chat back to before message `msg` (one of yours), returning
    /// its text. Stops a reply in progress first.
    fn take_from(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) -> Option<String> {
        self.replies.remove(&chat);
        // KodAct's conversation can't be cut back: the next message starts a new one.
        self.sessions.remove(&chat);
        while self.side.get(&chat).is_some_and(|s| !s.pending.is_empty()) {
            self.answer_side(
                chat,
                "The conversation was restarted before it answered.".into(),
                true,
            );
        }
        let (text, old_session) = self.truncate_chat_from(chat, msg)?;
        if let Some(dir) = self.history.clone() {
            cx.background_spawn(async move { crate::history::remove(&dir, &old_session) })
                .detach();
        }
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

    /// Show a Settings section; an editor of KodAct CLI's files reads them
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

    /// Read KodAct CLI's files again (off the UI thread).
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

    /// While Settings shows an editor of KodAct CLI's files, look at them every
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

    /// Ask KodAct CLI which MCP servers start (`kodact mcp list`, in the project).
    pub fn check_mcp(&mut self, cx: &mut Context<Self>) {
        let Some(cli) = crate::backend::find_cli() else {
            self.engine.mcp_error = Some(
                "KodAct CLI isn't installed beside KodAct, so the servers can't be checked.".into(),
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
                                    .unwrap_or("kodact mcp list failed")
                                    .to_string(),
                            );
                        }
                    }
                    Err(e) => s.engine.mcp_error = Some(format!("Couldn't run KodAct CLI: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Run a change to KodAct CLI's files off the UI thread, then read them
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

    /// Settings > Skills, on its Install panel.
    pub fn install_skill(&mut self, cx: &mut Context<Self>) {
        self.skills_install = true;
        self.open_settings(SettingsTab::Skills, cx);
    }

    /// Settings > Plugins, on its marketplaces.
    pub fn open_marketplaces(&mut self, cx: &mut Context<Self>) {
        self.plugins_market = true;
        self.open_settings(SettingsTab::Plugins, cx);
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
        // A file KodAct is still writing: open it live, following its draft.
        if let (Some(path), Some(chat)) = (self.drafting(&file), self.current) {
            if self
                .doc
                .as_ref()
                .is_some_and(|d| crate::doc_versions::same_file(d, &file))
            {
                self.close_doc(cx);
                return;
            }
            return self.watch_draft(chat, &path, cx);
        }
        if self.doc.as_ref() == Some(&file) {
            self.doc = None;
            cx.notify();
            return;
        }
        self.show_doc(file, cx);
    }

    /// Open a Word, Excel, PowerPoint or PDF file you attached in the side
    /// panel, as one KodAct wrote: the same preview, comments and versions.
    /// It points at the project's copy (what KodAct edits); the file you
    /// picked is read for its first version. Anything else opens in its app.
    pub fn open_attachment(&mut self, a: crate::attach::Attachment, cx: &mut Context<Self>) {
        use crate::attach::Kind;
        let ext = a
            .name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        // A pasted text opens beside the chat, to read whole.
        if a.kind == Kind::Pasted {
            let content = a.text.as_deref().unwrap_or_default().to_string();
            return self.open_doc(
                FileRef {
                    name: format!("{}.txt", a.name).into(),
                    path: None,
                    content: content.into(),
                    bytes: None,
                },
                cx,
            );
        }
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
            // The copy now: the same file, or KodAct's edit of it.
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

    /// Send the panel's file's open comments to KodAct as one message: each
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

    /// One comment, sent at once: "Ask KodAct to change this".
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
        self.doc_words.clear();
        self.sheet_edits.clear();
        self.sheet_extra_rows.clear();
        self.sheet_extra_cols.clear();
        if let (Some(_), Some(bytes)) = (file.office(), file.bytes.clone()) {
            let key: SharedString =
                format!("{}#{:x}", file.name, crate::office_render::key(&bytes)).into();
            let ready = self.doc_preview.as_ref().is_some_and(|(k, _)| *k == key);
            if !ready {
                // A new version of the file on show (a deck growing slide by
                // slide): its last drawn pages stay up, marked as updating,
                // until the new ones are drawn, instead of the panel falling
                // back to text for the seconds the drawing takes.
                let stale_pages = self.doc_preview.as_ref().and_then(|(k, r)| {
                    let same_file = k.split_once('#').map(|(n, _)| n) == Some(file.name.as_ref());
                    match r {
                        Ok(p) if same_file => match p.as_ref() {
                            crate::office::Preview::Doc(d) if !d.pages.is_empty() => {
                                Some((d.pages.clone(), d.total))
                            }
                            _ => None,
                        },
                        _ => None,
                    }
                });
                if stale_pages.is_none() {
                    self.doc_preview = None;
                }
                let name = file.name.clone();
                cx.spawn(async move |this, cx| {
                    let (n, b) = (name.to_string(), bytes.clone());
                    let read = cx
                        .background_spawn(async move { crate::office::preview(&n, &b) })
                        .await;
                    let drawing = matches!(&read, Ok(crate::office::Preview::Doc(d)) if d.drawing);
                    let read = read.map(|mut p| {
                        if let (crate::office::Preview::Doc(d), Some((pages, total))) = (&mut p, stale_pages) {
                            if d.drawing && d.pages.is_empty() {
                                (d.pages, d.total) = (pages, total);
                            }
                        }
                        p
                    });
                    let _ = this.update(cx, |s, cx| {
                        s.doc_preview = Some((key.clone(), read.map(std::sync::Arc::new)));
                        s.apply_doc_jump(cx);
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
                            s.apply_doc_jump(cx);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }
        self.doc = Some(file);
        self.source_view = false;
        self.ensure_sheet_pages(cx);
        if self.doc_show_changes {
            self.load_doc_changes(cx);
        }
        self.preview_html(cx);
        cx.notify();
    }

    /// A web page in the panel, shown (not its code): load it into the
    /// panel's browser, once for each version of it.
    pub fn preview_html(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.doc.clone().filter(|f| f.is_html()) else {
            return;
        };
        if self.source_view {
            return;
        }
        let key = doc_version_key(&file);
        if self.html_previewed.as_ref() == Some(&key) && self.web.is_some() {
            return;
        }
        self.html_previewed = Some(key);
        let dir = std::env::temp_dir().join("forge-files");
        let name = crate::downloads::clean_name(&file.name);
        let content = file.content.clone();
        cx.spawn(async move |this, cx| {
            let path = dir.join(name);
            let wrote = cx
                .background_spawn({
                    let path = path.clone();
                    async move {
                        std::fs::create_dir_all(&dir)?;
                        std::fs::write(&path, content.as_bytes())
                    }
                })
                .await;
            let _ = this.update(cx, |s, cx| match wrote {
                Ok(()) => s.web_send(
                    crate::web_view::Cmd::Navigate(crate::office_render::url(&path)),
                    cx,
                ),
                Err(e) => s.show_toast(format!("Could not show the page: {e}"), cx),
            });
        })
        .detach();
    }

    pub fn set_web_device(&mut self, d: Device, cx: &mut Context<Self>) {
        self.web_device = d;
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

    /// The zoom of the panel's file.
    pub fn doc_zoom_here(&self) -> crate::doc_zoom::Zoom {
        self.doc
            .as_ref()
            .and_then(|f| self.doc_zoom.get(&crate::doc_versions::file_id(f)))
            .copied()
            .unwrap_or_default()
    }

    pub fn set_doc_zoom(&mut self, z: crate::doc_zoom::Zoom, cx: &mut Context<Self>) {
        if let Some(f) = &self.doc {
            self.doc_zoom.insert(crate::doc_versions::file_id(f), z);
        }
        self.doc_menu = None;
        cx.notify();
    }

    /// One step in or out, from the scale the pages are drawn at now.
    pub fn zoom_doc(&mut self, up: bool, cx: &mut Context<Self>) {
        let now = match self.doc_zoom_here() {
            crate::doc_zoom::Zoom::Percent(p) => p,
            _ => self.doc_fitted.load(std::sync::atomic::Ordering::Relaxed),
        };
        self.set_doc_zoom(crate::doc_zoom::Zoom::step(now, up), cx);
    }

    /// Whether the panel's file shows its thumbnail rail.
    pub fn doc_thumbs_here(&self) -> bool {
        let Some(f) = &self.doc else { return false };
        let default = matches!(
            f.office(),
            Some(crate::office::Kind::Pptx | crate::office::Kind::Pdf)
        );
        self.doc_thumbs
            .get(&crate::doc_versions::file_id(f))
            .copied()
            .unwrap_or(default)
    }

    pub fn toggle_doc_thumbs(&mut self, cx: &mut Context<Self>) {
        if let Some(f) = &self.doc {
            let on = !self.doc_thumbs_here();
            self.doc_thumbs.insert(crate::doc_versions::file_id(f), on);
        }
        cx.notify();
    }

    /// Open `menu`, or close it when it is the one open.
    pub fn toggle_doc_menu(&mut self, menu: DocMenu, cx: &mut Context<Self>) {
        self.doc_menu = (self.doc_menu != Some(menu)).then_some(menu);
        cx.notify();
    }

    /// The page `by` away from the one at the top (-1 back, +1 on).
    pub fn step_doc_page(&mut self, by: isize, cx: &mut Context<Self>) {
        // No document open, and the browser shows a deck: its slides.
        if self.doc.is_none() && self.web_page.deck.is_some() {
            return self.web_send(crate::web_view::Cmd::Slide(by.signum() as i32), cx);
        }
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
            None => {
                return self.show_toast("Only Word, Excel and PowerPoint files export to PDF.", cx)
            }
        }
        if !crate::office_render::can_draw() {
            self.show_toast(
                "Exporting to PDF needs Microsoft Office or LibreOffice. Install one, then try again.",
                cx,
            );
            return;
        }
        let Some(dir) = crate::downloads::dir() else {
            self.show_toast("Could not find your Downloads folder.", cx);
            return;
        };
        let (name, bytes) = (file.name.to_string(), file.data());
        let pdf_name = format!(
            "{}.pdf",
            name.rsplit_once('.')
                .map_or(name.as_str(), |(stem, _)| stem)
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
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    s.show_file_toast(format!("Saved to Downloads as {name}"), path, cx)
                }
                Err(e) => s.show_toast(format!("Could not export it: {e}"), cx),
            });
        })
        .detach();
    }

    /// Show the workbook in the panel as it prints, or as its cells.
    pub fn set_sheet_print(&mut self, on: bool, cx: &mut Context<Self>) {
        self.sheet_print = on;
        self.ensure_sheet_pages(cx);
        cx.notify();
    }

    /// Draw the open workbook's pages (Excel or LibreOffice, off the UI
    /// thread), once per version, while its print view is on.
    fn ensure_sheet_pages(&mut self, cx: &mut Context<Self>) {
        if !self.sheet_print {
            return;
        }
        let Some(file) = self.doc.clone() else { return };
        if file.office() != Some(crate::office::Kind::Xlsx) {
            return;
        }
        let Some(bytes) = file.bytes.clone() else { return };
        let key = doc_version_key(&file);
        if self.sheet_pages.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        // The last version's pages stay up until the new ones are drawn.
        let name = file.name.to_string();
        cx.spawn(async move |this, cx| {
            let drawn = cx
                .background_spawn(async move { crate::office::draw(&name, &bytes) })
                .await;
            let _ = this.update(cx, |s, cx| {
                let still = s.doc.as_ref().is_some_and(|d| doc_version_key(d) == key);
                if still {
                    s.sheet_pages = Some((
                        key,
                        drawn.map(|(pages, total)| {
                            std::sync::Arc::new(crate::office::Doc { pages, total, ..Default::default() })
                        }),
                    ));
                    cx.notify();
                }
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

    pub fn edit_sheet_cell(&mut self, at: (usize, usize), cx: &mut Context<Self>) {
        self.doc_pick = Some((at, at));
        self.sheet_focus_seq = self.sheet_focus_seq.wrapping_add(1);
        cx.notify();
    }

    pub fn set_sheet_cell(
        &mut self,
        sheet: usize,
        row: usize,
        col: usize,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.sheet_edits.insert((sheet, row, col), value);
        self.doc_pick = Some(((row, col), (row, col)));
        cx.notify();
    }

    pub fn add_sheet_row(&mut self, sheet: usize, cx: &mut Context<Self>) {
        let base_rows = self
            .doc_preview
            .as_ref()
            .and_then(|(_, r)| r.as_ref().ok())
            .and_then(|p| match p.as_ref() {
                crate::office::Preview::Sheets(sheets) => sheets.get(sheet).map(|s| s.rows.len()),
                _ => None,
            })
            .unwrap_or(0);
        let extra = self.sheet_extra_rows.entry(sheet).or_insert(0);
        let next_row = base_rows + *extra;
        *extra += 1;
        self.doc_pick = Some(((next_row, 0), (next_row, 0)));
        self.sheet_focus_seq = self.sheet_focus_seq.wrapping_add(1);
        cx.notify();
    }

    pub fn add_sheet_col(&mut self, sheet: usize, cx: &mut Context<Self>) {
        let base_cols = self
            .doc_preview
            .as_ref()
            .and_then(|(_, r)| r.as_ref().ok())
            .and_then(|p| match p.as_ref() {
                crate::office::Preview::Sheets(sheets) => sheets
                    .get(sheet)
                    .and_then(|s| s.rows.first().map(|r| r.len())),
                _ => None,
            })
            .unwrap_or(0);
        let extra = self.sheet_extra_cols.entry(sheet).or_insert(0);
        let next_col = base_cols + *extra;
        *extra += 1;
        self.doc_pick = Some(((0, next_col), (0, next_col)));
        self.sheet_focus_seq = self.sheet_focus_seq.wrapping_add(1);
        cx.notify();
    }

    pub fn discard_sheet_edits(&mut self, cx: &mut Context<Self>) {
        if self.sheet_edits.is_empty()
            && self.sheet_extra_rows.is_empty()
            && self.sheet_extra_cols.is_empty()
        {
            return;
        }
        self.sheet_edits.clear();
        self.sheet_extra_rows.clear();
        self.sheet_extra_cols.clear();
        cx.notify();
    }

    pub fn save_sheet_edits(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.doc.clone() else {
            return;
        };
        if self.sheet_edits.is_empty() {
            return;
        }
        let edits = self.sheet_edits.clone();
        let orig_bytes = file.data();
        let name = file.name.clone();
        let disk_path = file.path.clone().and_then(|p| {
            let pb = std::path::PathBuf::from(p.as_ref());
            pb.is_absolute().then_some(pb)
        });
        cx.spawn(async move |this, cx| {
            let name_bg = name.to_string();
            let result = cx
                .background_spawn(async move {
                    let updated = crate::office::write_xlsx_edits(&orig_bytes, &edits)?;
                    if let Some(p) = &disk_path {
                        if let Some(parent) = p.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(p, &updated).map_err(|e| e.to_string())?;
                    }
                    let preview = crate::office::preview(&name_bg, &updated)?;
                    Ok::<(Vec<u8>, crate::office::Preview), String>((updated, preview))
                })
                .await;
            let _ = this.update(cx, |s, cx| match result {
                Ok((updated, preview)) => {
                    let bytes_arc: std::sync::Arc<[u8]> = std::sync::Arc::from(updated);
                    let key: SharedString =
                        format!("{}#{:x}", name, crate::office_render::key(&bytes_arc)).into();
                    if let Some(d) = s.doc.as_mut() {
                        if d.name == name {
                            d.bytes = Some(bytes_arc);
                        }
                    }
                    s.doc_preview = Some((key, Ok(std::sync::Arc::new(preview))));
                    s.sheet_edits.clear();
                    s.sheet_extra_rows.clear();
                    s.sheet_extra_cols.clear();
                    s.show_toast(format!("Saved {name}"), cx);
                    cx.notify();
                }
                Err(e) => s.show_toast(format!("Could not save {name}: {e}"), cx),
            });
        })
        .detach();
    }

    /// The editor's text changed (or went back to the saved one: None).
    pub fn set_doc_edit(&mut self, edit: Option<(String, SharedString)>, cx: &mut Context<Self>) {
        self.doc_edit = edit;
        cx.notify();
    }

    /// A field of a Word file or deck changed (or went back: None).
    pub fn set_doc_word(
        &mut self,
        key: (usize, bool, usize),
        text: Option<String>,
        cx: &mut Context<Self>,
    ) {
        match text {
            Some(t) => self.doc_words.insert(key, t),
            None => self.doc_words.remove(&key),
        };
        cx.notify();
    }

    /// The panel's file has changes to save: a text edit or cell edits.
    pub fn doc_dirty(&self) -> bool {
        !self.sheet_edits.is_empty()
            || !self.doc_words.is_empty()
            || self.doc.as_ref().is_some_and(|f| {
                self.doc_edit
                    .as_ref()
                    .is_some_and(|(id, _)| *id == crate::doc_versions::file_id(f))
            })
    }

    /// Where the panel's file lives on this computer: its own path, or its
    /// project-relative one under the project. None for a remote chat.
    fn doc_disk_path(&self, f: &FileRef) -> Option<std::path::PathBuf> {
        if self.current_remote().is_some() {
            return None;
        }
        let p = std::path::PathBuf::from(f.path.as_ref()?.as_ref());
        Some(if p.is_absolute() {
            p
        } else {
            self.project_dir.join(p)
        })
    }

    /// How the panel shows a Markdown file.
    pub fn md_mode(&self) -> MdMode {
        if self.source_view {
            MdMode::Markdown
        } else if self.md_viewing || self.doc_is_live() {
            MdMode::Viewing
        } else {
            MdMode::Editing
        }
    }

    pub fn set_md_mode(&mut self, mode: MdMode, cx: &mut Context<Self>) {
        self.source_view = mode == MdMode::Markdown;
        self.md_viewing = mode == MdMode::Viewing;
        self.doc_menu = None;
        cx.notify();
    }

    /// The page editor's text, written to the file as you type: on disk
    /// (when it has a place there) and in its card in the chat, which keeps
    /// its version rather than adding one per keystroke.
    pub fn autosave_doc(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(file) = self.doc.clone() else { return };
        if file.content.as_ref() == text {
            return;
        }
        let new = FileRef {
            content: text.clone().into(),
            ..file.clone()
        };
        if let Some(chat) = self.current {
            if let Some(c) = self.chats.iter_mut().find(|c| c.id == chat) {
                let card = c.messages.iter_mut().rev().find_map(|m| match m {
                    Message::Assistant { blocks } => blocks.iter_mut().rev().find_map(|b| match b {
                        Block::File(f) if crate::doc_versions::same_file(f, &file) => Some(f),
                        _ => None,
                    }),
                    _ => None,
                });
                if let Some(f) = card {
                    *f = new.clone();
                }
            }
        }
        self.doc = Some(new);
        self.md_saved_at = Some(std::time::Instant::now());
        if let Some(p) = self.doc_disk_path(&file) {
            cx.background_spawn(async move {
                if let Some(parent) = p.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&p, text.as_bytes());
            })
            .detach();
        }
        cx.notify();
    }

    /// Give the panel's file a new name (its extension kept when left off):
    /// on disk, and on its cards in the chat.
    pub fn rename_doc(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(file) = self.doc.clone() else { return };
        let mut name = name.trim().replace(['/', '\\'], "-");
        if name.is_empty() {
            return;
        }
        let ext = file.name.rsplit_once('.').map(|(_, e)| e.to_string());
        if let Some(ext) = ext {
            if !name.to_ascii_lowercase().ends_with(&format!(".{}", ext.to_ascii_lowercase())) {
                name = format!("{name}.{ext}");
            }
        }
        if name == file.name.as_ref() {
            self.doc_menu = None;
            return cx.notify();
        }
        let path = file.path.as_ref().map(|p| {
            let p = p.as_ref();
            match p.rfind(['/', '\\']) {
                Some(i) => format!("{}{name}", &p[..=i]),
                None => name.clone(),
            }
        });
        if let (Some(from), Some(to)) = (
            self.doc_disk_path(&file),
            path.as_ref().and_then(|p| {
                self.doc_disk_path(&FileRef {
                    path: Some(p.clone().into()),
                    ..file.clone()
                })
            }),
        ) {
            if to.exists() {
                return self.show_toast(format!("{name} already exists."), cx);
            }
            if from.exists() {
                if let Err(e) = std::fs::rename(&from, &to) {
                    return self.show_toast(format!("Could not rename: {e}"), cx);
                }
            }
        }
        let renamed = FileRef {
            name: name.clone().into(),
            path: path.map(Into::into),
            ..file.clone()
        };
        if let Some(chat) = self.current {
            if let Some(c) = self.chats.iter_mut().find(|c| c.id == chat) {
                for m in c.messages.iter_mut() {
                    if let Message::Assistant { blocks } = m {
                        for b in blocks.iter_mut() {
                            if let Block::File(f) = b {
                                if crate::doc_versions::same_file(f, &file) {
                                    f.name = renamed.name.clone();
                                    f.path = renamed.path.clone();
                                }
                            }
                        }
                    }
                }
            }
        }
        self.doc = Some(renamed);
        self.doc_menu = None;
        self.show_toast(format!("Renamed to {name}"), cx);
    }

    pub fn open_explorer_menu(
        &mut self,
        item: ExplorerItem,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.explorer_menu = Some((item, at));
        cx.notify();
    }

    pub fn close_explorer_menu(&mut self, cx: &mut Context<Self>) {
        if self.explorer_menu.take().is_some() {
            cx.notify();
        }
    }

    /// Where an explorer row is on this computer, and its path in the project.
    fn explorer_paths(&self, item: &ExplorerItem) -> (Option<std::path::PathBuf>, String) {
        match item {
            ExplorerItem::Chat(f) => {
                let disk = self.doc_disk_path(f);
                let rel = disk
                    .as_ref()
                    .and_then(|p| p.strip_prefix(&self.project_dir).ok())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .or_else(|| f.path.as_ref().map(|p| p.to_string()))
                    .unwrap_or_else(|| f.name.to_string());
                (disk, rel)
            }
            ExplorerItem::Project { rel, .. } => (
                (self.current_remote().is_none()).then(|| self.project_dir.join(rel)),
                rel.clone(),
            ),
        }
    }

    /// Do `act` to the right-clicked row.
    pub fn explorer_action(&mut self, act: ExplorerAction, cx: &mut Context<Self>) {
        let Some((item, _)) = self.explorer_menu.take() else { return };
        let (disk, rel) = self.explorer_paths(&item);
        match act {
            ExplorerAction::Open => match &item {
                ExplorerItem::Chat(f) => self.open_doc(f.clone(), cx),
                ExplorerItem::Project { rel, dir: false } => self.open_project_file(rel.clone(), cx),
                ExplorerItem::Project { .. } => {}
            },
            ExplorerAction::Download => match &item {
                ExplorerItem::Chat(f) => self.download(f, cx),
                ExplorerItem::Project { dir: false, .. } => {
                    let Some(p) = disk else {
                        return self.show_toast("This file is on another computer.", cx);
                    };
                    let Some(dir) = crate::downloads::dir() else {
                        return self.show_toast("Could not find your Downloads folder.", cx);
                    };
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    cx.spawn(async move |this, cx| {
                        let saved = cx
                            .background_spawn(async move {
                                let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
                                crate::downloads::save(&dir, &name, &bytes).map_err(|e| e.to_string())
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
                            Err(e) => s.show_toast(format!("Could not save it: {e}"), cx),
                        });
                    })
                    .detach();
                }
                ExplorerItem::Project { .. } => {}
            },
            ExplorerAction::Attach => match (&item, disk) {
                (_, Some(p)) if p.exists() => self.add_attachment_paths(vec![p], cx),
                (ExplorerItem::Chat(f), _) if f.bytes.is_none() => {
                    self.attach_text(f.name.to_string(), f.content.to_string(), cx)
                }
                _ => self.show_toast("This file can't be attached from here.", cx),
            },
            ExplorerAction::CopyPath => {
                let text = disk.map(|p| p.display().to_string()).unwrap_or(rel);
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                self.show_toast("Path copied.", cx);
            }
            ExplorerAction::CopyRelative => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(rel));
                self.show_toast("Relative path copied.", cx);
            }
            ExplorerAction::Reveal => match disk {
                Some(p) => {
                    let folder = if p.is_dir() {
                        p
                    } else {
                        p.parent().map(|d| d.to_path_buf()).unwrap_or(p)
                    };
                    cx.open_with_system(&folder);
                }
                None => self.show_toast("This file is on another computer.", cx),
            },
        }
        cx.notify();
    }

    /// Change the deck in the panel: a paragraph's format, or its slides.
    /// Words typed and not saved go in first, so nothing is lost; the deck
    /// is written to its file and its card (keeping its version), and the
    /// pages are drawn again, showing the slide the change is on.
    pub fn deck_edit(&mut self, edit: DeckEdit, cx: &mut Context<Self>) {
        let Some(file) = self.doc.clone() else { return };
        let Some(bytes) = file.bytes.clone() else { return };
        if file.office() != Some(crate::office::Kind::Pptx) {
            return;
        }
        let words = std::mem::take(&mut self.doc_words);
        let disk = self.doc_disk_path(&file);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut picked: Option<(usize, Option<u32>)> = None;
                    let typed = crate::office_edit::write_pptx_text(
                        &bytes,
                        &crate::ui::office_editor::pptx_edits(&words),
                    )?;
                    let (updated, show) = match edit {
                        DeckEdit::Format { slide, para, format } => (
                            crate::pptx_edit::format_para(&typed, slide, para, format)?,
                            slide,
                        ),
                        DeckEdit::Slides(op) => crate::pptx_edit::slides(&typed, op)?,
                        DeckEdit::Shape { slide, op } => {
                            let out = crate::pptx_shapes::edit(&typed, slide, &op)?;
                            let sel = crate::pptx_shapes::selected_after(&typed, &out, slide, &op);
                            picked = Some((slide, sel));
                            (out, slide)
                        }
                    };
                    if let Some(p) = disk {
                        if let Some(parent) = p.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(&p, &updated).map_err(|e| e.to_string())?;
                    }
                    Ok::<_, String>((updated, show, picked))
                })
                .await;
            let _ = this.update(cx, |s, cx| match result {
                Ok((updated, show, picked)) => {
                    if let Some((slide, sel)) = picked {
                        s.deck_sel = sel.map(|id| (slide, id));
                    }
                    let new = FileRef {
                        bytes: Some(std::sync::Arc::from(updated)),
                        ..file.clone()
                    };
                    if let Some(chat) = s.current {
                        if let Some(c) = s.chats.iter_mut().find(|c| c.id == chat) {
                            let card = c.messages.iter_mut().rev().find_map(|m| match m {
                                Message::Assistant { blocks } => {
                                    blocks.iter_mut().rev().find_map(|b| match b {
                                        Block::File(f) if crate::doc_versions::same_file(f, &file) => Some(f),
                                        _ => None,
                                    })
                                }
                                _ => None,
                            });
                            if let Some(f) = card {
                                *f = new.clone();
                            }
                        }
                    }
                    s.doc_preview = None;
                    s.deck_focus = Some(show);
                    s.show_doc(new, cx);
                    cx.notify();
                }
                Err(e) => s.show_toast(format!("Could not change the deck: {e}"), cx),
            });
        })
        .detach();
    }

    /// Read the deck's shapes for the version in the panel, once.
    pub fn ensure_deck_shapes(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.doc.clone() else { return };
        let Some(bytes) = file.bytes.clone() else { return };
        if file.office() != Some(crate::office::Kind::Pptx) {
            return;
        }
        let key = doc_version_key(&file);
        if self.deck_shapes.as_ref().is_some_and(|(k, ..)| *k == key) {
            return;
        }
        self.deck_shapes = Some((key.clone(), Default::default(), (12_192_000, 6_858_000)));
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    (crate::pptx_shapes::read(&bytes), crate::pptx_shapes::slide_size(&bytes))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if let (Ok(shapes), size) = read {
                    if s.deck_shapes.as_ref().is_some_and(|(k, ..)| *k == key) {
                        s.deck_shapes = Some((key, std::sync::Arc::new(shapes), size));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Pick a shape on a slide (None: nothing).
    pub fn pick_shape(&mut self, pick: Option<(usize, u32)>, cx: &mut Context<Self>) {
        self.deck_sel = pick;
        self.deck_fill_menu = false;
        cx.notify();
    }

    /// The pointer moved while dragging a shape.
    pub fn drag_shape(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if let Some(d) = self.deck_drag.as_mut() {
            let now = d.at(at);
            if now != d.now {
                d.now = now;
                cx.notify();
            }
        }
    }

    /// The drag ended: the shape goes where it was dropped.
    pub fn drop_shape(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.deck_drag.take() else { return };
        cx.notify();
        if d.now != d.from {
            let (x, y, w, h) = d.now;
            self.deck_edit(
                DeckEdit::Shape {
                    slide: d.slide,
                    op: crate::pptx_shapes::ShapeOp::Place { id: d.id, x, y, w, h },
                },
                cx,
            );
        }
    }

    /// Read which engine new chats run, and its version (off the UI thread).
    pub fn refresh_engine(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let info = cx
                .background_spawn(async move {
                    let path = crate::backend::find_cli()?;
                    let mine = crate::engine::user_engine().is_some_and(|u| u == path);
                    let v = crate::engine::version(&path);
                    Some((path, v, mine))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.engine_info = info;
                cx.notify();
            });
        })
        .detach();
    }

    /// Replace the engine with a `kodact.exe` you pick: new chats run it; a
    /// chat already running keeps its own until it starts again.
    pub fn replace_engine(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Use this KodAct CLI".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            let Some(p) = paths.into_iter().next() else { return };
            let done = cx.background_spawn(async move { crate::engine::install(&p) }).await;
            let _ = this.update(cx, |s, cx| {
                match done {
                    Ok(v) => s.show_toast(format!("New chats now run {v}. Running chats keep theirs until they restart."), cx),
                    Err(e) => s.show_toast(format!("Not used: {e}."), cx),
                }
                s.refresh_engine(cx);
            });
        })
        .detach();
    }

    /// Go back to the engine the installer put beside the app.
    pub fn reset_engine(&mut self, cx: &mut Context<Self>) {
        match crate::engine::reset() {
            Ok(()) => self.show_toast("New chats run the bundled engine again.", cx),
            Err(e) => self.show_toast(format!("Couldn't go back: {e}"), cx),
        }
        self.refresh_engine(cx);
    }

    /// Hold `anchor` for the comment box: the page editor's Comment.
    pub fn hold_doc_comment(&mut self, anchor: crate::doc_versions::Anchor, cx: &mut Context<Self>) {
        self.doc_comment_hold = Some(anchor);
        cx.notify();
    }

    /// Save what is edited in the panel: cells of a workbook, or the text of
    /// a text file, written to its project path and kept as a new version in
    /// the chat (Changes and the version stepper work as for KodAct's own).
    pub fn save_doc_edits(&mut self, cx: &mut Context<Self>) {
        if !self.sheet_edits.is_empty() {
            return self.save_sheet_edits(cx);
        }
        if !self.doc_words.is_empty() {
            return self.save_office_text(cx);
        }
        let (Some(file), Some((id, text))) = (self.doc.clone(), self.doc_edit.clone()) else {
            return;
        };
        if id != crate::doc_versions::file_id(&file) {
            return;
        }
        let new = FileRef {
            name: file.name.clone(),
            path: file.path.clone(),
            content: text.clone(),
            bytes: None,
        };
        let disk = self.doc_disk_path(&file);
        let name = file.name.clone();
        cx.spawn(async move |this, cx| {
            let wrote = cx
                .background_spawn(async move {
                    if let Some(p) = disk {
                        if let Some(parent) = p.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(&p, text.as_bytes()).map_err(|e| e.to_string())?;
                    }
                    Ok::<(), String>(())
                })
                .await;
            let _ = this.update(cx, |s, cx| match wrote {
                Ok(()) => {
                    if let Some(chat) = s.current {
                        s.push_block(chat, Block::File(new.clone()));
                    }
                    s.doc_edit = None;
                    s.show_doc(new, cx);
                    s.show_toast(format!("Saved {name}"), cx);
                }
                Err(e) => s.show_toast(format!("Could not save {name}: {e}"), cx),
            });
        })
        .detach();
    }

    /// Save the words changed in a Word file or deck: written into the file
    /// (`office_edit.rs`), to its project path, kept as a new version; the
    /// pages are drawn again.
    fn save_office_text(&mut self, cx: &mut Context<Self>) {
        let (Some(file), Some(bytes)) = (
            self.doc.clone(),
            self.doc.as_ref().and_then(|f| f.bytes.clone()),
        ) else {
            return;
        };
        let words = self.doc_words.clone();
        let kind = file.office();
        let disk = self.doc_disk_path(&file);
        let (name, path) = (file.name.clone(), file.path.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let updated = match kind {
                        Some(crate::office::Kind::Docx) => crate::office_edit::write_docx_text(
                            &bytes,
                            &crate::ui::office_editor::docx_edits(&words),
                        )?,
                        Some(crate::office::Kind::Pptx) => crate::office_edit::write_pptx_text(
                            &bytes,
                            &crate::ui::office_editor::pptx_edits(&words),
                        )?,
                        _ => return Err("only Word files and decks have words to edit".to_string()),
                    };
                    if let Some(p) = disk {
                        if let Some(parent) = p.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::write(&p, &updated).map_err(|e| e.to_string())?;
                    }
                    Ok(updated)
                })
                .await;
            let _ = this.update(cx, |s, cx| match result {
                Ok(updated) => {
                    let new = FileRef {
                        name: name.clone(),
                        path,
                        content: SharedString::default(),
                        bytes: Some(std::sync::Arc::from(updated)),
                    };
                    if let Some(chat) = s.current {
                        s.push_block(chat, Block::File(new.clone()));
                    }
                    // The pages are drawn again from the new file.
                    s.doc_preview = None;
                    s.show_doc(new, cx);
                    s.show_toast(format!("Saved {name}"), cx);
                }
                Err(e) => s.show_toast(format!("Could not save {name}: {e}"), cx),
            });
        })
        .detach();
    }

    /// Drop what is edited in the panel: the file goes back as it was saved.
    pub fn discard_doc_edits(&mut self, cx: &mut Context<Self>) {
        self.doc_edit = None;
        self.doc_words.clear();
        self.discard_sheet_edits(cx);
        cx.notify();
    }

    pub fn toggle_doc_fit(&mut self, cx: &mut Context<Self>) {
        self.doc_fit = !self.doc_fit;
        cx.notify();
    }

    /// Open a file in the app the system has for it: a diagram's page in the
    /// browser (where it is interactive), a workbook in Excel. Written to
    /// KodAct's temp folder off the UI thread, then handed to the system.
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

    /// KodAct starts on a page: the design this chat shows live. A design
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
                // The page as it was, for undo, before KodAct changes it.
                self.design_keep_version(&path);
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
        self.design_follow
            && self.current == Some(chat)
            && self.design_live.as_ref().is_some_and(|l| l.chat == chat)
    }

    /// The browser comes to the panel, unless a document you opened is there.
    fn design_panel(&mut self) {
        if self.doc.is_none() {
            self.panel = Some(PanelTab::Browser);
        }
    }

    /// A text file streaming in: kept as far as it has got, and shown in
    /// the side panel when it is the file you are watching.
    fn text_draft(&mut self, chat: usize, path: &str, content: &str, cx: &mut Context<Self>) {
        let content: SharedString = content.to_string().into();
        let first = !self.drafts.contains_key(&(chat, path.to_string()));
        self.drafts.insert((chat, path.to_string()), content.clone());
        // The file's card, as soon as its writing starts, as if it were
        // done: a click opens it beside the chat, filling in live. The
        // finished file then takes the same card's place (`update_card`).
        if first && Self::gets_card(path) {
            let file = FileRef {
                name: std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string())
                    .into(),
                path: Some(self.chat_path(chat, path).into()),
                content: content.clone(),
                bytes: None,
            };
            if let Some(file) = self.chat_mut(chat).and_then(|c| c.update_card(file, false)) {
                self.push_block(chat, Block::File(file));
            }
        }
        if self.doc_live.as_ref() == Some(&(chat, path.to_string())) {
            if let Some(doc) = self.doc.as_mut() {
                doc.content = content;
                // Keep to the newest line, as a terminal follows its output.
                self.doc_scroll.scroll_to_bottom();
            }
        }
        cx.notify();
    }

    /// Text files that get a card in the reply when written (`written_file`).
    fn gets_card(path: &str) -> bool {
        let ext = path
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        matches!(ext.as_str(), "md" | "markdown" | "svg")
    }

    /// The path, as its Write named it, of a file the current chat is
    /// writing now that is `file`, if it is one.
    fn drafting(&self, file: &FileRef) -> Option<String> {
        let chat = self.current?;
        let want = file.path.as_deref()?;
        self.drafts
            .keys()
            .find(|(c, p)| *c == chat && self.chat_path(chat, p) == want)
            .map(|(_, p)| p.clone())
    }

    /// Open the file a reply is writing in the side panel and follow it:
    /// what has streamed so far now, the rest as it comes.
    pub fn watch_draft(&mut self, chat: usize, path: &str, cx: &mut Context<Self>) {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        let content = self
            .drafts
            .get(&(chat, path.to_string()))
            .cloned()
            .unwrap_or_default();
        self.show_doc(
            FileRef {
                name: name.into(),
                path: Some(self.chat_path(chat, path).into()),
                content,
                bytes: None,
            },
            cx,
        );
        self.doc_live = Some((chat, path.to_string()));
        self.doc_scroll.scroll_to_bottom();
        cx.notify();
    }

    /// The write of `path` ended: the live view stops following; the file's
    /// card then opens what was written.
    fn end_draft(&mut self, chat: usize, path: &str, cx: &mut Context<Self>) {
        let key = (chat, path.to_string());
        let finished = self.drafts.remove(&key);
        if self.doc_live.as_ref() == Some(&key) {
            self.doc_live = None;
            if let (Some(doc), Some(text)) = (self.doc.as_mut(), finished) {
                doc.content = text;
            }
            cx.notify();
        }
    }

    /// Whether the side panel is following a file KodAct is writing now.
    pub fn doc_is_live(&self) -> bool {
        self.doc_live.is_some() && self.busy()
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
    fn design_call(
        &mut self,
        chat: usize,
        name: &str,
        input: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
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
        if let Some(l) = self
            .design_live
            .as_mut()
            .filter(|l| l.chat == chat && l.path == full)
        {
            l.writing = false;
            l.unseen = unseen;
        }
        self.design_keep_version(&full);
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

    /// Follow KodAct's design again (the badge's Show), or stop: you take
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
    /// replies and KodAct writes it.
    pub fn designing(&self) -> Option<&LiveDesign> {
        self.design_live.as_ref().filter(|l| {
            l.writing && self.current == Some(l.chat) && self.replies.contains_key(&l.chat)
        })
    }

    /// The pages this chat has written or changed, in the order it first
    /// did: the design's pages, for the Pages menu.
    pub fn design_pages(&self) -> Vec<std::path::PathBuf> {
        let Some(chat) = self.chat() else {
            return Vec::new();
        };
        let mut pages: Vec<std::path::PathBuf> = Vec::new();
        for m in &chat.messages {
            let Message::Assistant { blocks } = m else {
                continue;
            };
            for b in blocks {
                if let Block::Tool(t) = b {
                    if matches!(t.kind, ToolKind::Write | ToolKind::Edit) {
                        if let Some(p) = self.design_path(chat.id, &t.target) {
                            if !pages.contains(&p) {
                                pages.push(p);
                            }
                        }
                    }
                }
            }
        }
        // Then the copies made beside them.
        for (c, p) in &self.design_extra {
            if *c == chat.id && !pages.contains(p) {
                pages.push(p.clone());
            }
        }
        // In the order you dragged them into; new ones after.
        if let Some(order) = self.design_order.get(&chat.id) {
            pages.sort_by_key(|p| order.iter().position(|o| o == p).unwrap_or(usize::MAX));
        }
        pages
    }

    /// The design page the browser shows now, if it shows one of this
    /// chat's pages.
    pub fn design_on_screen(&self) -> Option<std::path::PathBuf> {
        let url = crate::design::live::page_key(&self.web_page.url);
        self.design_pages()
            .into_iter()
            .find(|p| crate::design::live::file_url(p) == url)
    }

    /// Show one of the design's pages in its tab.
    pub fn design_show(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if !path.is_file() {
            self.show_toast("That page isn't on disk any more.", cx);
            return;
        }
        self.design_load_versions(path);
        self.design_panel();
        self.web_send(
            crate::web_view::Cmd::Design {
                key: crate::design::live::file_url(path),
                draft: None,
            },
            cx,
        );
        cx.notify();
    }

    /// Ask KodAct to review the page on screen: Open Design's five-part
    /// design review (craft, the brief, brand, accessibility, copy), with
    /// the fixes made, not only listed. Sent now, or queued while KodAct
    /// replies.
    pub fn design_critique(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.design_on_screen() else {
            self.show_toast("Open a design page to review it.", cx);
            return;
        };
        if self.current.is_none() {
            self.show_toast("Review a page from the chat that made it.", cx);
            return;
        }
        let review = crate::design::root()
            .map(|r| r.join("plugins/_official/atoms/critique-theater/SKILL.md"))
            .filter(|p| p.is_file())
            .map(|p| format!(" Follow the review in {}.", p.display()))
            .unwrap_or_default();
        let text = format!(
            "Review {} as a senior designer would, on five counts: craft (layout, hierarchy, spacing, alignment), \
whether it does what I asked, the design system (its tokens, type and voice), accessibility (contrast, targets, focus, labels, \
heading order) and copy (specific, no filler). Render it and lint it first.{review} Score each count out of 10 with the reason, \
fix everything under 8, render it again to check, then tell me the scores before and after.",
            page.display()
        );
        if self.busy() {
            self.queue_message(text, cx);
        } else {
            self.send(text, cx);
        }
    }

    /// Ask for another page: the composer starts the request.
    pub fn design_new_page(&mut self, cx: &mut Context<Self>) {
        let like = self
            .design_on_screen()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        self.draft = Some(match like {
            Some(name) => format!("Make a new page in the style of {name}: "),
            None => "Make a new page: ".to_string(),
        });
        cx.notify();
    }

    /// Save the page on screen as a PDF beside it (or in Downloads when it
    /// isn't a file), never over another file.
    pub fn design_export_pdf(&mut self, cx: &mut Context<Self>) {
        let (dir, stem) = match self.design_on_screen() {
            Some(p) => (
                p.parent().map(std::path::Path::to_path_buf),
                p.file_stem().map(|s| s.to_string_lossy().into_owned()),
            ),
            None => (crate::downloads::dir(), None),
        };
        let Some(dir) = dir.filter(|d| std::fs::create_dir_all(d).is_ok()) else {
            self.show_toast("There is nowhere to save the PDF.", cx);
            return;
        };
        let stem = stem
            .or_else(|| {
                (!self.web_page.title.trim().is_empty()).then(|| self.web_page.title.clone())
            })
            .unwrap_or_else(|| "page".to_string());
        let name = crate::downloads::clean_name(&format!("{stem}.pdf"));
        let to = crate::downloads::free_path(&dir, &name);
        self.web_send(crate::web_view::Cmd::Pdf(to), cx);
    }

    /// Export the design page on screen (`design::export`): beside it, in a
    /// browser of its own, so the panel doesn't change. Says where it went.
    pub fn design_export(&mut self, format: crate::design::export::Format, cx: &mut Context<Self>) {
        let Some(page) = self.design_on_screen() else {
            self.show_toast("Open a design page to export it.", cx);
            return;
        };
        if !page.is_file() {
            self.show_toast("Only a page saved on this computer can be exported.", cx);
            return;
        }
        let to = crate::design::export::target(&page, format);
        self.show_toast(format!("Exporting as {}…", format.label()), cx);
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_spawn(async move { crate::design::export::export(&page, format, &to) })
                .await;
            let _ = this.update(cx, |s, cx| {
                let say = match r {
                    Ok(d) => {
                        let name = d
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let what = if d.path.is_dir() {
                            format!("{} slides in {name}", d.count)
                        } else if d.count > 1 {
                            format!("{name} ({} slides)", d.count)
                        } else {
                            name
                        };
                        let missing = match d.notes.len() {
                            0 => String::new(),
                            n => format!(" {n} linked files weren't found and stay links."),
                        };
                        let figma = if format == crate::design::export::Format::Figma {
                            " Import it in Figma with the OD Figma Import plugin (design-library/figma-plugin)."
                        } else {
                            ""
                        };
                        format!(
                            "{} {what} {} {}.{missing}{figma}",
                            crate::i18n::tr("Saved"),
                            crate::i18n::tr("beside the page in"),
                            crate::design::export::took(d.took)
                        )
                    }
                    Err(e) => format!("The {} export failed: {e}", format.label()),
                };
                s.show_toast(say, cx);
            });
        })
        .detach();
    }

    /// Zoom the browser's page: 1 is its own size.
    pub fn web_zoom_to(&mut self, zoom: f64, cx: &mut Context<Self>) {
        let zoom = zoom.clamp(0.25, 2.0);
        if (zoom - self.web_zoom).abs() < 0.001 {
            return;
        }
        self.web_zoom = zoom;
        self.web_send(crate::web_view::Cmd::Zoom(zoom), cx);
        cx.notify();
    }

    /// Zoom in (1) or out (-1) a step, as a browser does.
    pub fn web_zoom_step(&mut self, step: i32, cx: &mut Context<Self>) {
        self.web_zoom_to(crate::web_view::zoom_step(self.web_zoom, step), cx);
    }

    /// The zoom at which a desktop-wide page (1280 px) fits the panel: its
    /// layout as on a laptop, all of its width in view. Never larger than 1.
    pub fn web_zoom_fit(&mut self, cx: &mut Context<Self>) {
        let room = f64::from(self.web_room.0.max(1));
        self.web_zoom_to((room / 1280.0).min(1.0), cx);
    }

    /// The page fills the window, or goes back to the panel.
    pub fn set_presenting(&mut self, on: bool, cx: &mut Context<Self>) {
        if on && self.web.is_none() {
            self.show_toast("Open a page in the browser to present it.", cx);
            return;
        }
        self.presenting = on;
        if on {
            self.panel = Some(PanelTab::Browser);
            self.doc = None;
        }
        cx.notify();
    }

    /// Keep the page's text as a version, unless it is the one on disk
    /// already. A new version after an undo drops the undone ones, as an
    /// editor does.
    fn design_keep_version(&mut self, path: &std::path::Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        self.design_load_versions(path);
        let (list, at) = self.design_versions.entry(path.to_path_buf()).or_default();
        if list.get(*at).is_some_and(|v| **v == *text) {
            return;
        }
        if !list.is_empty() {
            list.truncate(*at + 1);
        }
        list.push(text.into());
        *at = list.len() - 1;
        let kept = crate::design::store::save_versions(path, list, *at);
        self.design_versions.insert(path.to_path_buf(), kept);
    }

    /// A page's versions from its file, the first time the page is used
    /// (none kept leaves it empty, so it isn't read again).
    fn design_load_versions(&mut self, path: &std::path::Path) {
        if !self.design_versions.contains_key(path) {
            let kept = crate::design::store::load_versions(path).unwrap_or_default();
            self.design_versions.insert(path.to_path_buf(), kept);
        }
    }

    /// Whether the page on screen has a version before (`step` -1) or
    /// after (1) the one on disk.
    pub fn design_can_step(&self, step: i32) -> bool {
        self.design_on_screen()
            .and_then(|p| self.design_versions.get(&p))
            .is_some_and(|(list, at)| {
                let to = *at as i64 + i64::from(step);
                to >= 0 && (to as usize) < list.len()
            })
    }

    /// Undo (-1) or redo (1) a change to the page on screen: that version
    /// is written back, and the page reloads.
    pub fn design_step(&mut self, step: i32, cx: &mut Context<Self>) {
        let Some(page) = self.design_on_screen() else {
            return;
        };
        if self.designing().is_some_and(|l| l.path == page) {
            return self.show_toast("KodAct is changing this page; undo once it's done.", cx);
        }
        if !self.design_can_step(step) {
            return self.show_toast(
                if step < 0 {
                    "Nothing to undo on this page."
                } else {
                    "Nothing to redo on this page."
                },
                cx,
            );
        }
        let Some((list, at)) = self.design_versions.get_mut(&page) else {
            return;
        };
        let to = (*at as i64 + i64::from(step)) as usize;
        if let Err(e) = std::fs::write(&page, list[to].as_bytes()) {
            return self.show_toast(format!("Couldn't change the page: {e}"), cx);
        }
        *at = to;
        let list = list.clone();
        crate::design::store::save_versions(&page, &list, to);
        self.design_show(&page, cx);
    }

    /// A copy of the page on screen beside it, shown, and listed with the
    /// chat's pages.
    pub fn design_duplicate(&mut self, cx: &mut Context<Self>) {
        let (Some(page), Some(chat)) = (self.design_on_screen(), self.current) else {
            return;
        };
        let (Some(dir), Some(stem)) = (page.parent(), page.file_stem()) else {
            return;
        };
        let ext = page
            .extension()
            .map_or("html".to_string(), |e| e.to_string_lossy().into_owned());
        let to =
            crate::downloads::free_path(dir, &format!("{} copy.{ext}", stem.to_string_lossy()));
        if let Err(e) = std::fs::copy(&page, &to) {
            return self.show_toast(format!("Couldn't copy the page: {e}"), cx);
        }
        self.design_extra.push((chat, to.clone()));
        self.mark_dirty(chat, cx);
        self.design_show(&to, cx);
    }

    /// Ask KodAct to rename the page on screen, so it knows the new name:
    /// the composer starts the request.
    pub fn design_rename(&mut self, cx: &mut Context<Self>) {
        if let Some(name) = self
            .design_on_screen()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        {
            self.draft = Some(format!("Rename {name} to "));
            cx.notify();
        }
    }

    /// Layout guides on or off: every box on the page outlined, in the
    /// theme's accent.
    pub fn design_set_guides(&mut self, on: bool, cx: &mut Context<Self>) {
        use crate::theme::ActiveTheme;
        self.design_guides = on;
        let colour = css(cx.theme().colors.primary.opacity(0.55));
        self.web_send(crate::web_view::Cmd::Guides(on.then_some(colour)), cx);
        cx.notify();
    }

    /// The page's outline beside it, or not.
    pub fn design_set_outline(&mut self, on: bool, cx: &mut Context<Self>) {
        self.design_outline = on;
        if !on {
            self.design_headings.clear();
        }
        self.web_send(crate::web_view::Cmd::Outline(on), cx);
        cx.notify();
    }

    /// Turn the comment tool on or off.
    pub fn design_set_select(&mut self, on: bool, cx: &mut Context<Self>) {
        self.design_select = on;
        self.design_hover = None;
        if on {
            self.design_tool = None;
        } else {
            self.design_pick = None;
            self.design_pick_marks = false;
        }
        cx.notify();
    }

    /// Take an Insert tool, or put it down. It and the comment tool are
    /// one at a time.
    pub fn design_set_tool(&mut self, tool: Option<InsertTool>, cx: &mut Context<Self>) {
        self.design_tool = tool;
        if tool.is_some() {
            self.design_select = false;
            self.design_hover = None;
            self.design_pick = None;
            self.design_pick_marks = false;
        }
        cx.notify();
    }

    /// The marks on the page on screen.
    /// The marks on the page on screen, where their parts are now.
    pub fn design_marks_here(&self) -> Vec<crate::design::marks::Mark> {
        let Some(page) = self.design_on_screen() else {
            return Vec::new();
        };
        self.design_marks
            .iter()
            .filter(|(_, p, _)| *p == page)
            .map(|(_, _, m)| {
                let now = m
                    .anchor
                    .as_ref()
                    .and_then(|a| self.design_anchors.get(&a.selector))
                    .copied();
                m.followed(now)
            })
            .collect()
    }

    /// A comment's part where it is now (as picked when the page doesn't
    /// say otherwise).
    pub fn design_part_now(&self, part: &crate::web_view::PagePart) -> crate::web_view::PagePart {
        match self.design_anchors.get(&part.selector) {
            Some(&(x, y, w, h)) => crate::web_view::PagePart {
                x,
                y,
                w,
                h,
                ..part.clone()
            },
            None => part.clone(),
        }
    }

    /// Take a page's marks off, in every chat that had them, each chat
    /// written again so they don't come back.
    fn design_marks_off(&mut self, page: &std::path::Path, cx: &mut Context<Self>) {
        let chats: std::collections::BTreeSet<usize> = self
            .design_marks
            .iter()
            .filter(|(_, p, _)| p == page)
            .map(|(c, _, _)| *c)
            .collect();
        self.design_marks.retain(|(_, p, _)| p != page);
        for chat in chats {
            self.mark_dirty(chat, cx);
        }
    }

    /// Tell the browser which parts the page on screen has comments and
    /// marks on, when that changes, so they follow them.
    fn design_sync_anchors(&mut self, cx: &mut Context<Self>) {
        let mut list: Vec<String> = self
            .design_comments_here()
            .iter()
            .map(|c| c.part.selector.clone())
            // The page as a whole stays where it was clicked.
            .filter(|s| !s.is_empty() && s != "body")
            .collect();
        if let Some(page) = self.design_on_screen() {
            list.extend(
                self.design_marks
                    .iter()
                    .filter(|(_, p, _)| *p == page)
                    .filter_map(|(_, _, m)| m.anchor.as_ref().map(|a| a.selector.clone())),
            );
        }
        list.sort();
        list.dedup();
        if list != self.design_anchors_sent {
            self.design_anchors.clear();
            self.design_anchors_sent = list.clone();
            self.web_send(crate::web_view::Cmd::Anchors(list), cx);
        }
    }

    /// Keep a mark drawn on the page on screen (one too small to mean
    /// anything is dropped).
    pub fn design_add_mark(&mut self, mark: crate::design::marks::Mark, cx: &mut Context<Self>) {
        let (Some(page), Some(chat)) = (self.design_on_screen(), self.current) else {
            return;
        };
        if mark.worth_keeping() {
            // Ask which part it is on, to follow that part later.
            if let Some((x, y)) = mark.anchor_point() {
                self.design_anchor_next += 1;
                let id = self.design_anchor_next;
                self.design_anchor_wait
                    .insert(id, (page.clone(), mark.points.clone()));
                self.web_send(crate::web_view::Cmd::AnchorAt { id, x, y }, cx);
            }
            self.design_marks.push((chat, page, mark));
            self.mark_dirty(chat, cx);
            cx.notify();
        }
    }

    /// Take the page's marks off.
    pub fn design_clear_marks(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = self.design_on_screen() {
            self.design_marks_off(&page, cx);
            self.design_pick_marks = false;
            if self.design_pick.as_ref().is_some_and(|p| p.tag == "marks") {
                self.design_pick = None;
            }
            cx.notify();
        }
    }

    /// Open the comment box on the page's marks: what you say goes to KodAct
    /// with a picture of them.
    pub fn design_comment_on_marks(&mut self, cx: &mut Context<Self>) {
        let marks: Vec<_> = self.design_marks_here();
        let Some((x, y, w, h)) = crate::design::marks::bounds(&marks, 0.0) else {
            return;
        };
        self.design_tool = None;
        self.design_select = true;
        self.design_hover = None;
        self.design_pick = Some(crate::web_view::PagePart {
            tag: "marks".into(),
            text: match marks.len() {
                1 => "1 mark".into(),
                n => format!("{n} marks"),
            },
            selector: String::new(),
            x,
            y,
            w,
            h,
        });
        self.design_pick_marks = true;
        cx.notify();
    }

    /// A comment on the page's marks: a note keeps them; for KodAct, a
    /// picture of the page with them on it is taken (`Update::Picture`),
    /// then they go with it, and come off the page.
    fn design_comment_marks(
        &mut self,
        chat: usize,
        page: std::path::PathBuf,
        part: crate::web_view::PagePart,
        text: String,
        to_forge: bool,
        cx: &mut Context<Self>,
    ) {
        use crate::theme::ActiveTheme;
        let marks: Vec<_> = self.design_marks_here();
        if marks.is_empty() {
            return;
        }
        let name = page
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let n = self
            .design_comments
            .iter()
            .filter(|c| c.page == page)
            .count()
            + 1;
        let said = if text.is_empty() {
            format!("{} on the page", part.text)
        } else {
            text.clone()
        };
        let sent = if to_forge {
            let (x, y, w, h) = crate::design::marks::bounds(&marks, 24.0).unwrap_or_default();
            let t = cx.theme();
            let (ink, paper) = (css(t.extra.page_ink), css(t.extra.page_halo));
            self.web_send(
                crate::web_view::Cmd::Picture {
                    before: crate::design::marks::overlay_js(&marks, &ink, &paper),
                    clip: (x, y, w.min(3000.0), h.min(3000.0)),
                    after: crate::design::marks::REMOVE_OVERLAY_JS.into(),
                },
                cx,
            );
            self.design_marks_out = Some(MarksOut {
                chat,
                page: page.clone(),
                message: crate::design::marks::message(&name, &marks, &text),
                n,
            });
            CommentSent::Waiting
        } else {
            CommentSent::Note
        };
        self.design_comments.push(DesignComment {
            n,
            chat,
            page,
            part,
            text: said,
            sent,
            resolved: false,
        });
        self.mark_dirty(chat, cx);
        cx.notify();
    }

    /// The marks' picture is taken: the comment goes with it now, or once
    /// the reply running ends; the marks come off the page.
    fn design_marks_pictured(&mut self, png: Result<Vec<u8>, String>, cx: &mut Context<Self>) {
        let Some(out) = self.design_marks_out.take() else {
            return;
        };
        let mut attachments = Vec::new();
        match png {
            Ok(bytes) => {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                let id = 1_000_000 + NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut a) = crate::attach::pasted_image(bytes, 1, &self.project_dir, id) {
                    let stem = out
                        .page
                        .file_stem()
                        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                    a.name = format!("{stem} marks.png").into();
                    attachments.push(a);
                }
            }
            Err(e) => self.show_toast(
                format!("The marks' picture didn't work ({e}); they go as words."),
                cx,
            ),
        }
        self.design_marks_off(&out.page.clone(), cx);
        let sent = if self.replies.contains_key(&out.chat) {
            self.design_held
                .entry(out.chat)
                .or_default()
                .push((out.message, attachments));
            CommentSent::Waiting
        } else {
            self.send_in(out.chat, out.message, attachments, cx);
            CommentSent::Sent
        };
        if let Some(c) = self
            .design_comments
            .iter_mut()
            .find(|c| c.page == out.page && c.n == out.n)
        {
            c.sent = sent;
        }
        self.mark_dirty(out.chat, cx);
        cx.notify();
    }

    /// Show the page in an artboard of this width (None: the panel's own),
    /// zoomed to keep all of it in view as the panel changes.
    pub fn design_set_artboard(
        &mut self,
        board: Option<(&'static str, u32)>,
        cx: &mut Context<Self>,
    ) {
        self.design_artboard = board;
        self.design_fit_artboard(cx);
        if board.is_none() {
            self.web_zoom_to(1.0, cx);
        }
        cx.notify();
    }

    /// The zoom that shows the artboard's width in the panel.
    fn design_fit_artboard(&mut self, cx: &mut Context<Self>) {
        if let Some((_, width)) = self.design_artboard {
            let room = f64::from(self.web_room.0.max(1));
            self.web_zoom_to(room / f64::from(width), cx);
        }
    }

    /// Read the library's design systems, once, off the main thread.
    pub fn design_load_systems(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.design_systems_asked, true) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let cards = cx
                .background_spawn(async move {
                    use crate::design::library::{Index, Kind};
                    let Some(root) = crate::design::root() else {
                        return Vec::new();
                    };
                    let ix = Index::load(&root);
                    let mut cards: Vec<SystemCard> = ix
                        .of(Kind::DesignSystem)
                        .map(|e| SystemCard {
                            id: e.id.clone(),
                            title: e.title.clone(),
                            description: e.description.clone(),
                            dir: root.join(&e.dir),
                        })
                        .collect();
                    cards.sort_by_key(|c| c.title.to_lowercase());
                    cards
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.design_systems = Some(std::sync::Arc::new(cards));
                cx.notify();
            });
        })
        .detach();
    }

    /// Ask KodAct to restyle the page on screen with a design system: the
    /// composer starts the request, naming the system's `DESIGN.md`.
    pub fn design_apply_system(&mut self, card: &SystemCard, cx: &mut Context<Self>) {
        let Some(name) = self
            .design_on_screen()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        else {
            return;
        };
        self.draft = Some(format!(
            "Restyle {name} with the {} design system: read {} first, and keep the page's content as it is.",
            card.title,
            card.dir.join("DESIGN.md").display()
        ));
        // A design chat keeps the look for what it makes next.
        if let Some(mut setup) = self.chat().and_then(|c| c.design.clone()) {
            setup.system = Some(card.id.clone());
            self.set_design(Some(setup), cx);
        }
        cx.notify();
    }

    /// Make the open chat a design chat set up with `setup`, or an
    /// ordinary chat again (None). Its next message starts KodAct CLI again
    /// (resuming the conversation) with the design prompt and tools.
    pub fn set_design(
        &mut self,
        setup: Option<crate::design::prompt::Setup>,
        cx: &mut Context<Self>,
    ) {
        if setup.is_some() && crate::design::root().is_none() {
            return self.show_toast(
                "KodAct's design library isn't installed: put design-library beside KodAct, or set KODACT_DESIGN_LIB.",
                cx,
            );
        }
        // A new chat's screen: the chat your next message starts.
        let Some(id) = self.current else {
            let toast = match &setup {
                Some(s) => format!("Your next chat is a design chat: {}.", s.surface.label()),
                None => "Your next chat is an ordinary chat.".into(),
            };
            self.design_next = setup;
            if self.design_next.is_some() {
                self.design_load_index(cx);
            }
            self.show_toast(toast, cx);
            return cx.notify();
        };
        let Some(chat) = self.chat_mut(id) else {
            return;
        };
        if chat.design == setup {
            return;
        }
        let was = std::mem::replace(&mut chat.design, setup.clone());
        let toast = match (&setup, was) {
            (Some(s), None) => format!(
                "Design chat: {}. KodAct gets the design prompt and tools from your next message.",
                s.surface.label()
            ),
            (Some(s), Some(_)) => format!("Design chat: {}.", s.surface.label()),
            (None, _) => {
                "An ordinary chat again: no design prompt or tools from the next message.".into()
            }
        };
        // Its KodAct CLI starts again on the next message, unless a reply is
        // running (that one keeps what it started with).
        if self
            .chats
            .iter()
            .find(|c| c.id == id)
            .is_some_and(|c| self.status(c) != ChatStatus::Working)
        {
            self.sessions.remove(&id);
        }
        self.mark_dirty(id, cx);
        if setup.is_some() {
            self.design_load_index(cx);
        }
        self.show_toast(toast, cx);
        cx.notify();
    }

    /// Make the open chat a design chat for `surface`, keeping the design
    /// system, template and skill it already has.
    pub fn design_surface(
        &mut self,
        surface: crate::design::prompt::Surface,
        cx: &mut Context<Self>,
    ) {
        let mut setup = self.design_setup().cloned().unwrap_or_default();
        setup.surface = surface;
        self.set_design(Some(setup), cx);
    }

    /// The open chat's design setup, or the next chat's on a new chat's
    /// screen.
    pub fn design_setup(&self) -> Option<&crate::design::prompt::Setup> {
        match self.current {
            Some(_) => self.chat().and_then(|c| c.design.as_ref()),
            None => self.design_next.as_ref(),
        }
    }

    /// Open the composer's design menu on `tab` (None closes it).
    pub fn design_menu_to(&mut self, tab: Option<DesignTab>, cx: &mut Context<Self>) {
        self.design_menu = tab;
        if tab.is_some() {
            self.picker_open = false;
            self.modes_open = false;
            self.project_menu = false;
            self.context_open = false;
            self.title_menu = None;
            self.title_menu_dismissed = None;
            self.design_load_index(cx);
        }
        cx.notify();
    }

    /// Set one part of the design setup from the composer's menu: the
    /// design system, template or skill (by library id; None clears it).
    /// Picking one makes the chat a design chat.
    pub fn design_pick(&mut self, tab: DesignTab, id: Option<String>, cx: &mut Context<Self>) {
        let mut setup = self.design_setup().cloned().unwrap_or_default();
        match tab {
            DesignTab::Work => return,
            DesignTab::Look => setup.system = id,
            DesignTab::Start => setup.template = id,
            DesignTab::Skill => setup.skill = id,
        }
        self.set_design(Some(setup), cx);
    }

    /// The design gallery: the library to start a design from.
    pub fn open_design_home(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Design;
        self.palette_open = false;
        self.picker_open = false;
        self.design_load_index(cx);
        // The pictures come from the export browser: start it now.
        crate::design::export::warm_up();
        cx.notify();
    }

    pub fn set_gallery_tab(&mut self, tab: GalleryTab, cx: &mut Context<Self>) {
        self.gallery_tab = tab;
        cx.notify();
    }

    /// The gallery shows these previews: picture each that has none yet,
    /// in this order. Pages waiting that no longer show wait no more.
    pub fn design_want_thumbs(&mut self, pages: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let wanted: std::collections::HashSet<&std::path::PathBuf> = pages.iter().collect();
        let dropped: Vec<std::path::PathBuf> = self
            .design_thumb_queue
            .iter()
            .filter(|p| !wanted.contains(p))
            .cloned()
            .collect();
        for p in dropped {
            self.design_thumbs.remove(&p);
        }
        self.design_thumb_queue.retain(|p| wanted.contains(p));
        for p in pages {
            if !self.design_thumbs.contains_key(&p) {
                self.design_thumbs.insert(p.clone(), Thumb::Queued);
                self.design_thumb_queue.push_back(p);
            }
        }
        if self.design_thumbs_running || self.design_thumb_queue.is_empty() {
            return;
        }
        self.design_thumbs_running = true;
        cx.spawn(async move |this, cx| loop {
            let next = this
                .update(cx, |s, _| {
                    let next = s.design_thumb_queue.pop_front();
                    if next.is_none() {
                        s.design_thumbs_running = false;
                    }
                    next
                })
                .ok()
                .flatten();
            let Some(page) = next else { break };
            let at = page.clone();
            let got = cx
                .background_spawn(async move { crate::design::export::thumbnail(&at) })
                .await;
            let alive = this.update(cx, |s, cx| {
                // Still wanted (not dropped meanwhile)?
                if let Some(t) = s.design_thumbs.get_mut(&page) {
                    *t = match got {
                        Ok(p) => Thumb::Ready(p),
                        Err(_) => Thumb::Failed,
                    };
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();
    }

    /// Start a design from a gallery card: a new chat, set up with it (the
    /// template, design system or skill, and the kind of work it is for),
    /// and its ready-made request in the composer.
    pub fn design_start_from(
        &mut self,
        kind: crate::design::library::Kind,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        use crate::design::library::Kind;
        use crate::design::prompt::{Setup, Surface};
        let Some(ix) = self.design_index.clone() else {
            return;
        };
        let Some(e) = ix.get(kind, id) else { return };
        let surface = match e.mode.as_deref() {
            Some("deck") => Surface::Deck,
            Some("prototype") => Surface::Prototype,
            Some("image") => Surface::Marketing,
            Some("video") => Surface::Video,
            _ => Surface::Any,
        };
        let mut setup = Setup {
            surface,
            ..Setup::default()
        };
        // An example starts from what it was made with.
        let (kind, id) = match (kind, &e.twin, &e.parent) {
            (Kind::Example, Some((k, i)), _) => (*k, i.clone()),
            (Kind::Example, None, Some(parent)) => (Kind::Template, parent.clone()),
            _ => (kind, e.id.clone()),
        };
        match kind {
            Kind::Template => setup.template = Some(id),
            Kind::Skill => setup.skill = Some(id),
            Kind::DesignSystem => setup.system = Some(id),
            _ => {}
        }
        let ask = match (&e.query, e.kind) {
            (_, Some(Kind::DesignSystem)) => {
                format!("Make a landing page in the {} design system.", e.title)
            }
            (Some(q), _) => q.clone(),
            _ if !e.example_prompt.trim().is_empty() => e.example_prompt.clone(),
            _ => format!("Make something like {}.", e.title),
        };
        self.new_chat(cx);
        self.set_design(Some(setup), cx);
        self.draft = Some(ask);
        cx.notify();
    }

    /// A design system from a website: a design chat whose request (in the
    /// composer, for you to finish with the address) has KodAct capture the
    /// site's brand with `design_capture`, then design in it.
    pub fn design_from_website(&mut self, cx: &mut Context<Self>) {
        self.new_chat(cx);
        self.set_design(Some(crate::design::prompt::Setup::default()), cx);
        self.draft = Some(
            "Capture the brand of https:// with design_capture, then make a landing page in that look.".into(),
        );
        cx.notify();
    }

    /// Open a Figma file (`.fig`) as a design page (`design::fig`): saved
    /// as `designs/<name>/index.html` in the project, shown in the panel,
    /// and a design chat started to work on it.
    pub fn design_import_fig(&mut self, fig: std::path::PathBuf, cx: &mut Context<Self>) {
        if self.current_remote().is_some() {
            return self.show_toast("Figma files open into a project on this computer.", cx);
        }
        let project = self
            .project(self.current_project)
            .map(|p| p.path.clone())
            .unwrap_or_else(|| self.project_dir.clone());
        let stem: String = fig
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "figma".into())
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let to = project.join("designs").join(&stem).join("index.html");
        self.show_toast("Opening the Figma file…", cx);
        cx.spawn(async move |this, cx| {
            let at = to.clone();
            let started = std::time::Instant::now();
            let r = cx
                .background_spawn(async move { crate::design::fig::import(&fig, &at) })
                .await;
            let _ = this.update(cx, |s, cx| match r {
                Ok(pages) => {
                    s.new_chat(cx);
                    s.set_design(Some(crate::design::prompt::Setup::default()), cx);
                    let rel = to.strip_prefix(&project).unwrap_or(&to).display().to_string();
                    s.draft = Some(format!(
                        "{rel} is a Figma file I imported. Make it a real, responsive page from it: keep its look, use real HTML structure (headings, sections, buttons) instead of fixed boxes, then render it to check. "
                    ));
                    s.design_show(&to, cx);
                    s.show_toast(
                        format!(
                            "Opened {pages} page{} from Figma in {} as {rel}.",
                            if pages == 1 { "" } else { "s" },
                            crate::design::export::took(started.elapsed())
                        ),
                        cx,
                    );
                }
                Err(e) => s.show_toast(format!("The Figma file couldn't be opened: {e}"), cx),
            });
        })
        .detach();
    }

    /// The page's version, for its slide pictures: it changes with each of
    /// KodAct's edits.
    pub fn design_version_of(&self, page: &std::path::Path) -> usize {
        self.design_versions
            .get(page)
            .map_or(0, |(list, at)| list.len() * 1000 + at)
    }

    /// The rail's pictures of the deck on screen, taken in the background
    /// the first time they're wanted for this version of it.
    pub fn design_want_slides(&mut self, page: std::path::PathBuf, cx: &mut Context<Self>) {
        let key = (page.clone(), self.design_version_of(&page));
        if !self.design_slides_asked.insert(key.clone()) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_spawn(async move { crate::design::export::slide_thumbs(&page) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if let Ok(list) = got {
                    s.design_slides.insert(key, list);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Show the next pet (after none, the first): kept for next time.
    pub fn next_pet(&mut self, cx: &mut Context<Self>) {
        let all = crate::design::pets::all();
        if all.is_empty() {
            return self.show_toast("No pets: they come with KodAct's design library.", cx);
        }
        let at = self
            .pet
            .as_ref()
            .and_then(|id| all.iter().position(|p| &p.id == id));
        let next = &all[at.map_or(0, |i| (i + 1) % all.len())];
        self.pet = Some(next.id.clone());
        let _ = crate::design::pets::choose(Some(&next.id));
        let about = if next.description.is_empty() {
            String::new()
        } else {
            format!(": {}", next.description)
        };
        self.show_toast(format!("{}{about}", next.name), cx);
        cx.notify();
    }

    /// No pet.
    pub fn hide_pet(&mut self, cx: &mut Context<Self>) {
        self.pet = None;
        let _ = crate::design::pets::choose(None);
        cx.notify();
    }

    /// Make a routine from `<when>: <request>` ("weekly on monday at 9:
    /// refresh the metrics deck"), in this project, with this chat's design
    /// setup. It waits for its first time.
    pub fn add_routine(&mut self, spec: &str, cx: &mut Context<Self>) {
        use crate::design::routines::Every;
        // The schedule ends at a colon; "at 9:30: …" has one in its time,
        // so the last colon that leaves a schedule before it.
        let found = spec.match_indices(':').rev().find_map(|(i, _)| {
            Every::parse(&spec[..i]).map(|e| (e, spec[i + 1..].trim().to_string()))
        });
        let Some((when, prompt)) = found else {
            return self.show_toast(
                "A routine is /routine <when>: <request>, e.g. /routine weekly on monday at 9:00: refresh the metrics deck. When: hourly at 15, daily at 9:00, or weekly on a day at a time.",
                cx,
            );
        };
        if prompt.is_empty() {
            return self.show_toast("Say what the routine asks for, after the colon.", cx);
        }
        self.make_routine(when, prompt, cx);
    }

    /// Make a routine that asks `prompt` on schedule `when`, in this
    /// project, with this chat's design setup (the Routines form, and
    /// `/routine` once read). It waits for its first time.
    pub fn make_routine(
        &mut self,
        when: crate::design::routines::Every,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        use crate::design::routines::Routine;
        let now = chrono::Local::now().timestamp();
        let project = self
            .project(self.current_project)
            .filter(|p| !p.scratch)
            .map(|p| p.path.clone());
        let r = Routine {
            id: format!("r{now}{}", self.routines.len()),
            name: prompt.chars().take(48).collect(),
            prompt,
            every: when.clone(),
            project,
            design: self.design_setup().cloned(),
            enabled: true,
            last_run: Some(now),
        };
        self.routines.push(r);
        let _ = crate::design::routines::save(&self.routines);
        self.show_toast(
            format!(
                "Routine made: it runs {}, while KodAct is open. Manage it in Design › Routines.",
                when.say()
            ),
            cx,
        );
        cx.notify();
    }

    /// Run a routine now: a new chat in its project, set up as it was.
    pub fn run_routine(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(i) = self.routines.iter().position(|r| r.id == id) else {
            return;
        };
        let r = self.routines[i].clone();
        self.routines[i].last_run = Some(chrono::Local::now().timestamp());
        let _ = crate::design::routines::save(&self.routines);
        if let Some(path) = &r.project {
            if let Some(p) = self.projects.iter().find(|p| &p.path == path).map(|p| p.id) {
                self.select_project(p, cx);
            }
        }
        self.new_chat(cx);
        if r.design.is_some() {
            self.set_design(r.design.clone(), cx);
        }
        self.send(r.prompt.clone(), cx);
        self.show_toast(format!("Routine: {}", r.name), cx);
    }

    pub fn toggle_routine(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(r) = self.routines.iter_mut().find(|r| r.id == id) {
            r.enabled = !r.enabled;
            if r.enabled {
                // Back on: from now, not for the times it was off.
                r.last_run = Some(chrono::Local::now().timestamp());
            }
        }
        let _ = crate::design::routines::save(&self.routines);
        cx.notify();
    }

    pub fn delete_routine(&mut self, id: &str, cx: &mut Context<Self>) {
        self.routines.retain(|r| r.id != id);
        let _ = crate::design::routines::save(&self.routines);
        cx.notify();
    }

    /// Check the routines every half minute, and run those due (one at a
    /// time: the next waits for the next check).
    pub fn start_routines(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            let alive = this.update(cx, |s, cx| {
                let now = chrono::Local::now();
                let due = s.routines.iter().find(|r| r.due(now)).map(|r| r.id.clone());
                // Not into a chat that is replying, or a screen you're on
                // mid-answer: a routine waits until KodAct is free.
                if let (Some(id), false) = (due, s.busy()) {
                    s.run_routine(&id, cx);
                }
            });
            if alive.is_err() {
                break;
            }
            cx.background_executor()
                .timer(std::time::Duration::from_secs(30))
                .await;
        })
        .detach();
    }

    /// A chat's latest reply, as Langfuse traces it (`design::trace`).
    fn trace_turn(&self, chat: usize) -> Option<crate::design::trace::Turn> {
        let c = self.chats.iter().find(|c| c.id == chat)?;
        let start = c
            .messages
            .iter()
            .rposition(|m| matches!(m, Message::User { .. }))?;
        let Message::User { text, .. } = &c.messages[start] else {
            return None;
        };
        let mut t = crate::design::trace::Turn {
            chat: c.title.to_string(),
            session: c.session.clone(),
            input: text.to_string(),
            model: self.active_profile().map(|p| p.model.clone()),
            design: c.design.as_ref().and_then(|d| serde_json::to_value(d).ok()),
            ..Default::default()
        };
        for m in &c.messages[start + 1..] {
            match m {
                Message::Assistant { blocks } => {
                    for b in blocks {
                        match b {
                            Block::Text(x) => t.output.push_str(x),
                            Block::Tool(call) => {
                                t.tools.push(format!("{:?} {}", call.kind, call.target))
                            }
                            _ => {}
                        }
                    }
                }
                Message::TurnEnd { seconds, usage } => {
                    t.seconds = Some(*seconds);
                    t.tokens = usage
                        .as_ref()
                        .map(|u| (u.input + u.cache_read + u.cache_creation, u.output));
                }
                Message::Notice { error: true, .. } => t.failed = true,
                _ => {}
            }
        }
        Some(t)
    }

    /// Show a gallery card's example page beside the gallery.
    pub fn design_preview(&mut self, page: &std::path::Path, cx: &mut Context<Self>) {
        self.design_show(page, cx);
    }

    /// Read the library's index for design chats, once, off the main thread.
    fn design_load_index(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.design_index_asked, true) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let ix = cx
                .background_spawn(async move {
                    crate::design::root()
                        .map(|r| std::sync::Arc::new(crate::design::library::Index::load(&r)))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.design_index.is_none() {
                    s.design_index = ix;
                }
                // The gallery and the composer's menu show it.
                cx.notify();
            });
        })
        .detach();
    }

    /// Put a page of the chat's before another in its list (dragged in
    /// the Pages menu).
    pub fn design_move_page(
        &mut self,
        from: &std::path::Path,
        before: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = self.current else { return };
        let mut order = self.design_pages();
        let Some(at) = order.iter().position(|p| p == from) else {
            return;
        };
        let moved = order.remove(at);
        let to = order
            .iter()
            .position(|p| p == before)
            .unwrap_or(order.len());
        order.insert(to, moved);
        self.design_order.insert(chat, order);
        self.mark_dirty(chat, cx);
        cx.notify();
    }

    /// Close the comment being written, keeping the tool on.
    pub fn design_cancel_pick(&mut self, cx: &mut Context<Self>) {
        self.design_pick = None;
        self.design_pick_marks = false;
        cx.notify();
    }

    /// The comments on the page on screen, in order.
    pub fn design_comments_here(&self) -> Vec<&DesignComment> {
        let Some(page) = self.design_on_screen() else {
            return Vec::new();
        };
        self.design_comments
            .iter()
            .filter(|c| c.page == page)
            .collect()
    }

    /// Comment on the part picked: a note for yourself, or (`to_forge`) a
    /// request KodAct acts on; while KodAct replies it waits for the reply to
    /// end (`queue`), or reaches KodAct at its next step.
    pub fn design_comment(
        &mut self,
        text: String,
        to_forge: bool,
        queue: bool,
        cx: &mut Context<Self>,
    ) {
        let text = text.trim().to_string();
        let (Some(page), Some(chat)) = (self.design_on_screen(), self.current) else {
            return;
        };
        let Some(part) = self
            .design_pick
            .take()
            .filter(|_| !text.is_empty() || self.design_pick_marks)
        else {
            return;
        };
        if std::mem::take(&mut self.design_pick_marks) {
            return self.design_comment_marks(chat, page, part, text, to_forge, cx);
        }
        let name = page
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let message = crate::design::live::comment_message(&name, &part, &text);
        let sent = if !to_forge {
            CommentSent::Note
        } else if !self.replies.contains_key(&chat) {
            self.send_in(chat, message, Vec::new(), cx);
            CommentSent::Sent
        } else if queue {
            self.design_held
                .entry(chat)
                .or_default()
                .push((message, Vec::new()));
            CommentSent::Waiting
        } else {
            self.queue_message(message, cx);
            CommentSent::Sent
        };
        let n = self
            .design_comments
            .iter()
            .filter(|c| c.page == page)
            .count()
            + 1;
        self.design_comments.push(DesignComment {
            n,
            chat,
            page,
            part,
            text,
            sent,
            resolved: false,
        });
        self.mark_dirty(chat, cx);
        self.design_sync_anchors(cx);
        cx.notify();
    }

    /// Mark a comment done, or open it again.
    pub fn design_resolve(&mut self, page: &std::path::Path, n: usize, cx: &mut Context<Self>) {
        let mut chat = None;
        if let Some(c) = self
            .design_comments
            .iter_mut()
            .find(|c| c.page == page && c.n == n)
        {
            c.resolved = !c.resolved;
            chat = Some(c.chat);
        }
        if let Some(chat) = chat {
            self.mark_dirty(chat, cx);
        }
        cx.notify();
    }

    /// Scroll the page to a comment's part.
    pub fn design_go_to(&mut self, page: &std::path::Path, n: usize, cx: &mut Context<Self>) {
        let at = self
            .design_comments
            .iter()
            .find(|c| c.page == page && c.n == n)
            .map(|c| (c.part.x, c.part.y));
        if let Some((x, y)) = at {
            self.web_send(crate::web_view::Cmd::ScrollTo(x, y), cx);
        }
    }

    /// Send `text` in chat `id`, which isn't replying, as if typed there.
    fn send_in(
        &mut self,
        id: usize,
        text: String,
        attachments: Vec<crate::attach::Attachment>,
        cx: &mut Context<Self>,
    ) {
        if self.replies.contains_key(&id) {
            return;
        }
        self.headlines.remove(&id);
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

    /// The chat on screen's design, written while you weren't following:
    /// the badge offers to show it.
    pub fn design_ready(&self) -> Option<&LiveDesign> {
        self.design_live
            .as_ref()
            .filter(|l| !l.writing && l.unseen && self.current == Some(l.chat))
    }

    /// Open or close a sub-agent's prompt disclosure ("Message from parent agent").
    pub fn toggle_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.agents_open.remove(id) {
            self.agents_open.insert(id.to_string());
        }
        cx.notify();
    }

    /// Which sub-agent is currently focused in the Agents panel for the active chat.
    pub fn selected_agent_id(&self) -> Option<&str> {
        let chat = self.current?;
        let runs = self.agents.get(&chat)?;
        if let Some(id) = self.selected_agent.get(&chat) {
            if runs.iter().any(|r| &r.id == id) {
                return Some(id.as_str());
            }
        }
        runs.iter()
            .find(|r| r.ended.is_none())
            .or_else(|| runs.first())
            .map(|r| r.id.as_str())
    }

    /// Pick an agent in the Agents panel's jumper: it becomes current and
    /// the jumper closes (`agent_roster::jump`).
    pub fn jump_to_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut picked = None;
        let mut open = self.ui_toggles.contains(crate::ui::AGENT_JUMPER);
        crate::ui::agent_roster::jump(&mut picked, &mut open, id);
        if !open {
            self.ui_toggles.remove(crate::ui::AGENT_JUMPER);
        }
        if let Some(id) = picked {
            self.select_agent(&id, cx);
        }
    }

    /// Focus a specific sub-agent in the right-side Agents inspector.
    pub fn select_agent(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(chat) = self.current {
            self.selected_agent.insert(chat, id.to_string());
        }
        self.doc = None;
        self.panel = Some(PanelTab::Agents);
        cx.notify();
    }

    /// Move to the previous or next sub-agent in `chat` (`delta` = -1 or +1).
    fn cycle_agent_in(&mut self, chat: usize, delta: isize) -> bool {
        let Some(runs) = self.agents.get(&chat) else {
            return false;
        };
        if runs.is_empty() {
            return false;
        }
        let cur_idx = self
            .selected_agent
            .get(&chat)
            .and_then(|id| runs.iter().position(|r| r.id == *id))
            .or_else(|| runs.iter().position(|r| r.ended.is_none()))
            .unwrap_or(0) as isize;
        let len = runs.len() as isize;
        let next = (cur_idx + delta).rem_euclid(len) as usize;
        let id = runs[next].id.clone();
        self.selected_agent.insert(chat, id);
        true
    }

    /// Move to the previous or next sub-agent in the current chat (`delta` = -1 or +1).
    pub fn cycle_agent(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        if self.cycle_agent_in(chat, delta) {
            cx.notify();
        }
    }

    /// Toggle a single tool step's open state for `(chat, agent_id, step_idx)`.
    fn toggle_step_in(&mut self, chat: usize, agent_id: &str, step_idx: usize) -> bool {
        if let Some(step) = self
            .agents
            .get_mut(&chat)
            .and_then(|runs| runs.iter_mut().find(|r| r.id == agent_id))
            .and_then(|r| r.steps.get_mut(step_idx))
        {
            step.open = !step.open;
            true
        } else {
            false
        }
    }

    /// Expand or collapse a single tool step inside a sub-agent's activity feed.
    pub fn toggle_agent_step(&mut self, agent_id: &str, step_idx: usize, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        if self.toggle_step_in(chat, agent_id, step_idx) {
            cx.notify();
        }
    }

    /// Toggle Full View (expand the Agents inspector across the main session area).
    pub fn toggle_panel_full(&mut self, cx: &mut Context<Self>) {
        self.panel_full = !self.panel_full;
        cx.notify();
    }

    /// Copy a sub-agent's output (final answer, or step outputs if no summary yet).
    pub fn copy_agent_output(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(run) = self
            .current
            .and_then(|c| self.agents.get(&c))
            .and_then(|runs| runs.iter().find(|r| r.id == id))
        else {
            return;
        };
        let text = if !run.said.trim().is_empty() {
            run.said.clone()
        } else {
            run.steps
                .iter()
                .filter(|s| !s.output.trim().is_empty())
                .map(|s| format!("{}\n{}", s.label(), s.output.trim_end()))
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        if text.trim().is_empty() {
            self.show_toast("This agent has no output to copy yet.", cx);
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.mark_copied(format!("agent-{id}").into(), cx);
    }

    /// Map `(msg, block)` in `chat` to its `AgentRun` (synthesizing historical runs when absent)
    /// and update `selected_agent`.
    fn focus_agent_for_block(&mut self, chat: usize, msg: usize, block: usize) -> bool {
        let Some(c) = self.chats.iter().find(|c| c.id == chat) else {
            return false;
        };
        let mut all_agent_blocks = vec![];
        let mut target_nth = None;
        for (mi, m) in c.messages.iter().enumerate() {
            if let Message::Assistant { blocks } = m {
                for (bi, b) in blocks.iter().enumerate() {
                    if let Block::Tool(t) = b {
                        if is_agent_tool(&t.target) {
                            if mi == msg && bi == block {
                                target_nth = Some(all_agent_blocks.len());
                            }
                            all_agent_blocks.push((mi, bi, t.clone()));
                        }
                    }
                }
            }
        }
        let Some(nth) = target_nth else {
            return false;
        };
        let runs = self.agents.entry(chat).or_default();
        // Historical chats reloaded from disk have blocks without live `AgentRun`s:
        // synthesize read-only runs in block order so clicking still inspects them.
        if runs.len() <= nth {
            let now = std::time::Instant::now();
            for (mi, bi, call) in all_agent_blocks.into_iter().skip(runs.len()) {
                runs.push(AgentRun {
                    id: format!("hist-{chat}-{mi}-{bi}"),
                    kind: "agent".into(),
                    task: call
                        .note
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("Sub-agent task")
                        .to_string(),
                    prompt: String::new(),
                    started: now,
                    ended: (call.status != ToolStatus::Running).then_some(now),
                    heard: now,
                    failed: call.status == ToolStatus::Failed,
                    steps: vec![],
                    entries: vec![],
                    said: call.output.to_string(),
                    stopping: false,
                    stopped: call.status == ToolStatus::Declined,
                });
            }
        }
        if let Some(id) = runs.get(nth).map(|r| r.id.clone()) {
            self.selected_agent.insert(chat, id);
            true
        } else {
            false
        }
    }

    /// Open and focus the sub-agent belonging to the `(msg, block)` row in `chat`.
    pub fn open_agent_for_block(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_agent_for_block(chat, msg, block) {
            return;
        }
        self.doc = None;
        self.panel = Some(PanelTab::Agents);
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
                    heard: now,
                    failed: false,
                    steps: vec![],
                    entries: vec![],
                    said: String::new(),
                    stopping: false,
                    stopped: false,
                });
                let has_selected = self
                    .selected_agent
                    .get(&chat)
                    .is_some_and(|sel| runs.iter().any(|r| &r.id == sel));
                if !has_selected {
                    self.selected_agent.insert(chat, id.clone());
                }
                // The Agents panel opens with the first agent; each also has
                // its row in the conversation.
                if self.doc.is_none() && self.current == Some(chat) {
                    self.panel = Some(PanelTab::Agents);
                }
            }
            Event::ToolResult {
                id,
                output,
                is_error,
                ..
            } => {
                // A run you stopped here (no engine, in the demo) has already ended.
                if let Some(r) = runs.iter_mut().find(|r| &r.id == id && !r.stopped) {
                    r.ended = Some(now);
                    r.heard = now;
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
                        s.ended.get_or_insert(now);
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
                    r.heard = now;
                    let idx = r.steps.len();
                    r.steps.push(AgentStep {
                        id: id.clone(),
                        kind: call.kind,
                        what,
                        described,
                        detail: call.target.to_string(),
                        output: String::new(),
                        started: now,
                        ended: None,
                        status: ToolStatus::Running,
                        open: false,
                    });
                    r.entries.push(AgentEntry::Step(idx));
                }
            }
            Event::SubResult {
                parent,
                id,
                output,
                is_error,
            } => {
                if let Some(r) = runs.iter_mut().find(|r| &r.id == parent) {
                    r.heard = now;
                    if let Some(s) = r.steps.iter_mut().find(|s| &s.id == id) {
                        s.status = if *is_error {
                            ToolStatus::Failed
                        } else {
                            ToolStatus::Done
                        };
                        s.ended = Some(now);
                        s.output = output.clone();
                    }
                }
            }
            Event::SubText { parent, text } => {
                if let Some(r) = runs
                    .iter_mut()
                    .find(|r| &r.id == parent && r.ended.is_none())
                {
                    r.heard = now;
                    if !text.trim().is_empty() {
                        let dup = matches!(
                            r.entries.last(),
                            Some(AgentEntry::Text(prev)) if prev.trim() == text.trim()
                        );
                        if !dup {
                            r.entries.push(AgentEntry::Text(text.clone()));
                        }
                    }
                    r.said = text.clone();
                }
            }
            // A turn that ends with agents still going stopped them.
            Event::TurnEnd { .. } | Event::Exited { .. } => {
                for r in runs.iter_mut().filter(|r| r.ended.is_none()) {
                    r.ended = Some(now);
                    r.stopped = r.stopping;
                    r.failed = !r.stopping;
                    let step_end = if r.stopping {
                        ToolStatus::Declined
                    } else {
                        ToolStatus::Failed
                    };
                    for s in r
                        .steps
                        .iter_mut()
                        .filter(|s| s.status == ToolStatus::Running)
                    {
                        s.status = step_end;
                        s.ended.get_or_insert(now);
                    }
                    r.stopping = false;
                }
            }
            _ => {}
        }
    }

    /// Show the side panel on `tab`, closing any file open in it. The
    /// browser starts when its tab first shows.
    /// Preview or Source, in the panel.
    pub fn set_source_view(&mut self, on: bool, cx: &mut Context<Self>) {
        self.source_view = on;
        self.preview_html(cx);
        cx.notify();
    }

    /// Show plan version `at` of `total` in the Plan tab; the last is live.
    pub fn show_plan_version(&mut self, at: usize, total: usize, cx: &mut Context<Self>) {
        self.plan_version = (at + 1 < total).then_some(at);
        cx.notify();
    }

    /// Open or close the Agents panel's jumper (the list of every agent),
    /// opening the panel on the Agents tab first if needed.
    pub fn toggle_agent_jumper(&mut self, cx: &mut Context<Self>) {
        if self.panel != Some(PanelTab::Agents) || self.doc.is_some() {
            self.show_panel(PanelTab::Agents, cx);
            self.ui_toggles.insert(crate::ui::AGENT_JUMPER.to_string());
            self.agent_jumper_focus.set(Some(usize::MAX));
        } else if !self.ui_toggles.remove(crate::ui::AGENT_JUMPER) {
            self.ui_toggles.insert(crate::ui::AGENT_JUMPER.to_string());
            self.agent_jumper_focus.set(Some(usize::MAX));
        }
        cx.notify();
    }

    pub fn show_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        self.doc = None;
        self.panel = Some(tab);
        if tab != PanelTab::Agents {
            self.panel_full = false;
        }
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
                "KodAct needs Chromium, Chrome or Edge for the browser. Install one, or run: npx playwright install chromium"
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
                // A browser started again keeps the zoom you chose.
                if self.web_zoom != 1.0 {
                    view.send(crate::web_view::Cmd::Zoom(self.web_zoom));
                }
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
            Update::Frame(img, info) => {
                self.web_page.frame_info = info;
                // The last picture leaves the GPU's atlas as the next arrives.
                if let Some(old) = self.web_page.frame.replace(img) {
                    cx.drop_image(old, None);
                }
            }
            Update::Page { url, title } => {
                let (url, title) = self.design_named(url, title);
                self.web_page.url = url;
                self.web_page.title = title;
                // A design page on screen: its undo history from its file,
                // and the parts its comments and marks are on, to follow.
                if let Some(p) = self.design_on_screen() {
                    self.design_load_versions(&p);
                }
                self.design_sync_anchors(cx);
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
            Update::Picture(png) => self.design_marks_pictured(png, cx),
            Update::Deck(at) => {
                self.web_page.deck = at;
            }
            Update::Anchors(now) => {
                self.design_anchors = now.into_iter().collect();
            }
            Update::AnchoredAt(id, part) => {
                let (Some((page, points)), Some(part)) =
                    (self.design_anchor_wait.remove(&id), part)
                else {
                    return;
                };
                let found = self
                    .design_marks
                    .iter_mut()
                    .find(|(_, p, m)| *p == page && m.points == points && m.anchor.is_none());
                if let Some((chat, _, m)) = found {
                    m.anchor = Some(crate::design::marks::Anchor {
                        selector: part.selector,
                        at: (part.x, part.y, part.w, part.h),
                    });
                    let chat = *chat;
                    self.mark_dirty(chat, cx);
                    self.design_sync_anchors(cx);
                }
            }
            Update::Outline(heads) => {
                self.design_headings = heads;
                cx.notify();
            }
            Update::Part { part, pick } => {
                if !self.design_select {
                    return;
                }
                if pick {
                    if part.is_some() {
                        self.design_pick = part;
                    }
                } else {
                    self.design_hover = part;
                }
                cx.notify();
            }
            Update::Printed(Ok(path)) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.show_toast(format!("Saved {name} beside the page"), cx);
            }
            Update::Printed(Err(why)) => {
                self.show_toast(format!("The page could not be saved as a PDF: {why}"), cx);
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
        let resized = if let crate::web_view::Cmd::Resize { width, height, .. } = cmd {
            self.web_room = (width, height);
            true
        } else {
            false
        };
        if self.web.is_none() {
            self.web_page.error = None;
            self.ensure_web(cx);
        }
        if let Some(w) = &self.web {
            w.send(cmd);
        }
        // An artboard keeps its width in view as the panel changes.
        if resized {
            self.design_fit_artboard(cx);
        }
    }

    /// Show the side panel on `tab`, or close it when that tab is showing.
    pub fn toggle_panel_tab(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.doc.is_none() && self.panel == Some(tab) {
            self.panel = None;
            self.panel_full = false;
            cx.notify();
        } else {
            self.show_panel(tab, cx);
        }
    }

    /// Open or close the side panel (on the plan when it opens).
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        if self.doc.is_some() || self.panel.is_some() {
            self.doc = None;
            self.panel = None;
            self.panel_full = false;
        } else {
            self.panel = Some(PanelTab::Plan);
        }
        cx.notify();
    }

    pub fn close_panel(&mut self, cx: &mut Context<Self>) {
        self.doc = None;
        self.panel = None;
        self.panel_full = false;
        self.doc_full = false;
        cx.notify();
    }

    pub fn close_doc(&mut self, cx: &mut Context<Self>) {
        self.doc_live = None;
        self.doc_finding = false;
        self.doc_full = false;
        self.sheet_edits.clear();
        self.sheet_extra_rows.clear();
        self.sheet_extra_cols.clear();
        if self.doc.take().is_some() {
            cx.notify();
        }
    }

    /// Toggle full-screen focus mode for the open document pane.
    pub fn toggle_doc_full(&mut self, cx: &mut Context<Self>) {
        if self.doc.is_none() {
            self.doc_full = false;
            return;
        }
        self.doc_full = !self.doc_full;
        cx.notify();
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
                        // A remote project's files and skills come over the
                        // connection: KodAct CLI there reads its own folders,
                        // not this computer's.
                        Some((extra, r)) => (
                            crate::remote::project_files(&r.target, &r.path, 20_000, &extra)
                                .map(crate::rules::entries_of)
                                .unwrap_or_default(),
                            vec![],
                            crate::commands::remote_skills(
                                &crate::remote::skill_heads(&r.target, &r.path, &extra)
                                    .unwrap_or_default(),
                            ),
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

    /// Open the file a reply's file chip names (`verify.rs`,
    /// `src/ui/markdown.rs`), when this chat's folder has it; else say so.
    pub fn open_named_file(&mut self, name: &str, cx: &mut Context<Self>) {
        let index = self.project_file_index();
        let local = self.current_remote().is_none();
        match resolve_named_file(name, &self.project_dir, index.as_deref().map(|v| v.as_slice()), local) {
            Some(rel) => self.open_project_file(rel, cx),
            None => self.show_toast(format!("{name} isn't in this chat's folder"), cx),
        }
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
                    match doc.then(|| file_card(name.clone(), ext.clone(), full.clone(), bytes.clone())) {
                        Some(Some(file)) => {
                            s.doc = None;
                            s.open_doc(file, cx);
                        }
                        // A picture, or a text or code file, reads in the panel.
                        _ if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") => {
                            s.doc = None;
                            s.open_doc(
                                FileRef {
                                    name: name.clone().into(),
                                    path: Some(full.clone().into()),
                                    content: "".into(),
                                    bytes: Some(std::sync::Arc::from(bytes.into_boxed_slice())),
                                },
                                cx,
                            );
                        }
                        _ if bytes.len() <= crate::ui::text_editor::MAX_BYTES
                            && !bytes.contains(&0)
                            && std::str::from_utf8(&bytes).is_ok() =>
                        {
                            s.doc = None;
                            s.open_doc(
                                FileRef {
                                    name: name.clone().into(),
                                    path: Some(full.clone().into()),
                                    content: String::from_utf8_lossy(&bytes).to_string().into(),
                                    bytes: None,
                                },
                                cx,
                            );
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

    /// The commands `/` offers: the project's own, then KodAct CLI's.
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
                kind: c.kind,
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
                        kind: c.kind,
                    }),
            )
            .chain(
                crate::commands::builtin()
                    .into_iter()
                    .map(|c| CompletionCommand {
                        name: c.name,
                        args: c.args,
                        description: c.description,
                        group: "KodAct CLI",
                        kind: c.kind,
                    }),
            )
            // KodAct's own: a side question (`parse_btw`), answered in a
            // card over the chat, which leaves the conversation as it is.
            .chain([CompletionCommand {
                name: "btw".into(),
                args: "<question>".into(),
                description: "Ask a side question; the answer floats over the chat".into(),
                group: "KodAct",
                kind: crate::commands::CommandKind::Action,
            }])
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

    /// "Run again" on a command: KodAct is asked to run it again, as a
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
        self.jump = Some((self.jump_seq, msg, None));
        cx.notify();
    }

    /// Open or close a run of calls (`work_group`), by its first block.
    pub fn toggle_group(&mut self, chat: usize, msg: usize, start: usize, cx: &mut Context<Self>) {
        let key = (chat, msg, start);
        if !self.open_groups.remove(&key) {
            self.open_groups.insert(key);
        }
        if let Some(c) = self.chat_mut(chat) {
            c.version += 1;
        }
        cx.notify();
    }

    /// Show block `block` of message `msg` in the chat showing: the run of
    /// calls holding it opens, the call opens (a failure shows its error),
    /// the list scrolls to it and it is marked for a moment.
    pub fn jump_to_block(&mut self, msg: usize, block: usize, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        let streaming = self.busy();
        let mut start = None;
        if let Some(c) = self.chat_mut(chat) {
            let last = msg + 1 == c.messages.len();
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                start = crate::ui::work_group::run_of(blocks, block, streaming && last)
                    .map(|r| r.start);
                if let Some(Block::Tool(t)) = blocks.get_mut(block) {
                    t.open = true;
                }
            }
            c.version += 1;
        }
        if let Some(start) = start {
            self.open_groups.insert((chat, msg, start));
        }
        self.tasks_open = false;
        self.flash = Some((chat, msg, block));
        self.jump_seq += 1;
        self.jump = Some((self.jump_seq, msg, Some(block)));
        // The mark fades by itself; then it goes.
        let seq = self.jump_seq;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1800))
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.jump_seq == seq && s.flash.take().is_some() {
                    if let Some(c) = s.chat_mut(chat) {
                        c.version += 1;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub fn dismiss_tasks(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if self.tasks_open {
            self.tasks_dismissed_at = Some(at);
            self.tasks_open = false;
            cx.notify();
        }
    }

    pub fn toggle_tasks_at(
        &mut self,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let dismissed = self.tasks_dismissed_at.take();
        if let (Some(d), Some(p)) = (dismissed, at) {
            if d == p {
                cx.notify();
                return;
            }
        }
        self.toggle_tasks(cx);
    }

    pub fn toggle_tasks(&mut self, cx: &mut Context<Self>) {
        self.tasks_open = !self.tasks_open;
        cx.notify();
    }

    pub fn dismiss_approvals(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if self.approvals_open {
            self.approvals_dismissed_at = Some(at);
            self.approvals_open = false;
            cx.notify();
        }
    }

    pub fn toggle_approvals_at(
        &mut self,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let dismissed = self.approvals_dismissed_at.take();
        if let (Some(d), Some(p)) = (dismissed, at) {
            if d == p {
                cx.notify();
                return;
            }
        }
        self.toggle_approvals(cx);
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
        match self.pending_question(chat) {
            Some((m, b)) => self.jump_to_block(m, b, cx),
            None => self.jump_to(msg, cx),
        }
    }

    // ------------------------------------------------------------ JSON export and share

    /// The chat as JSON: its whole transcript, tool calls included (the
    /// history's format, so KodAct can read it back).
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

    /// Copy as JSON: the conversation goes to the clipboard.
    pub fn share_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some((_, json)) = self.chat_json(id) else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(json));
        self.show_toast("Copied the conversation as JSON.", cx);
    }

    /// Bring back version `v`: the conversation from its message on becomes
    /// what it was then (the one showing now is kept as a version in turn),
    /// and the next message resumes that version's KodAct CLI conversation.
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
        let prev_session = c.session.clone();
        c.messages.truncate(at);
        c.messages.extend(old.tail);
        c.session = old.session;
        c.resumable = old.resumable;
        c.version += 1;
        let seed_needed = !c.resumable && prior_conversation_block(&c.messages).is_some();
        let same_session = prev_session == c.session;
        if seed_needed {
            self.history_unseeded.insert(chat);
        } else {
            self.history_unseeded.remove(&chat);
        }
        if !same_session {
            if let Some(dir) = self.history.clone() {
                cx.background_spawn(async move { crate::history::remove(&dir, &prev_session) })
                    .detach();
            }
        }
        self.touch(chat, cx);
        self.show_toast("Restored that version.", cx);
        cx.notify();
    }

    // ------------------------------------------------------------ context

    /// Summarize the conversation to free context: KodAct CLI's `/compact`,
    /// shown as a notice rather than a message of yours.
    pub fn compact(&mut self, cx: &mut Context<Self>) {
        self.compact_with(None, cx);
    }

    pub fn compact_with(&mut self, instructions: Option<&str>, cx: &mut Context<Self>) {
        self.context_open = false;
        let Some(id) = self.current else {
            return self.show_toast("Start or open a conversation first before compacting.", cx);
        };
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
                return self.show_toast(format!("Couldn't start KodAct to compact: {e}"), cx);
            }
        }
        let cmd = match instructions.map(str::trim).filter(|s| !s.is_empty()) {
            Some(extra) => format!("/compact {extra}"),
            None => "/compact".to_string(),
        };
        match self.sessions.get(&id).map(|l| l.session.send(&cmd)) {
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
        self.project_menu = false;
        self.design_menu = None;
        if self.context_open {
            self.title_menu = None;
            self.title_menu_dismissed = None;
        }
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

    /// Text you pasted that was too long for the box: a Pasted text card.
    pub fn add_pasted_text(&mut self, text: String, cx: &mut Context<Self>) {
        if self.attachments.len() >= crate::attach::MAX_FILES {
            return self.show_toast(
                format!("A message takes up to {} files.", crate::attach::MAX_FILES),
                cx,
            );
        }
        self.pasted_texts += 1;
        self.attachment_seq += 1;
        let a = crate::attach::pasted_text(&text, self.pasted_texts, self.attachment_seq);
        self.attachments.push(a);
        cx.notify();
    }

    /// A message from the chat, yours or KodAct's, attached to the next one
    /// as a text card (`name` says whose), so the model reads it whole and
    /// can be pointed at it ("shorten the attached reply").
    pub fn attach_text(&mut self, name: String, text: String, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            return self.show_toast("This message has no text to attach.", cx);
        }
        if self.attachments.len() >= crate::attach::MAX_FILES {
            return self.show_toast(
                format!("A message takes up to {} files.", crate::attach::MAX_FILES),
                cx,
            );
        }
        self.attachment_seq += 1;
        let mut a = crate::attach::pasted_text(&text, 0, self.attachment_seq);
        a.name = name.into();
        self.attachments.push(a);
        cx.notify();
    }

    /// The reply ending at `end`, attached to the next message.
    pub fn attach_turn(&mut self, chat: usize, end: usize, cx: &mut Context<Self>) {
        let Some(c) = self.chats.iter().find(|c| c.id == chat) else {
            return;
        };
        let text = turn_text(&c.messages, end);
        self.attach_text("KodAct's reply".into(), text, cx);
    }

    /// Open an attachment in the box in its editor: a pasted text, or a
    /// picture to draw on.
    pub fn edit_attachment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.editing_attachment = self.attachments.iter().any(|a| a.id == id).then_some(id);
        cx.notify();
    }

    pub fn close_attachment_editor(&mut self, cx: &mut Context<Self>) {
        self.editing_attachment = None;
        cx.notify();
    }

    /// A pasted text, edited: its new words (all of it gone removes the card).
    pub fn set_pasted_text(&mut self, id: u64, text: String, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            self.attachments.retain(|a| a.id != id);
        } else if let Some(a) = self.attachments.iter_mut().find(|a| a.id == id) {
            a.size = text.len() as u64;
            a.text = Some(std::sync::Arc::from(text.as_str()));
        }
        self.editing_attachment = None;
        cx.notify();
    }

    /// A picture, edited: the rendered result (a PNG) is what travels, the
    /// untouched original and the edits are kept beside it for Reset and
    /// for reopening. No edits at all puts the original back.
    pub fn set_attachment_edits(
        &mut self,
        id: u64,
        png: Vec<u8>,
        edits: Vec<crate::draw::EditOp>,
        cx: &mut Context<Self>,
    ) {
        if let Some(a) = self.attachments.iter_mut().find(|a| a.id == id) {
            let original = a.original.clone().or_else(|| a.bytes.clone());
            if edits.is_empty() {
                if let Some(orig) = original {
                    let mime = match image::guess_format(&orig) {
                        Ok(image::ImageFormat::Jpeg) => "image/jpeg",
                        Ok(image::ImageFormat::Gif) => "image/gif",
                        Ok(image::ImageFormat::WebP) => "image/webp",
                        _ => "image/png",
                    };
                    a.kind = crate::attach::Kind::Image(mime);
                    a.size = orig.len() as u64;
                    a.bytes = Some(orig);
                }
                a.original = None;
                a.edits.clear();
                if a.note.as_deref() == Some("edited by the user") {
                    a.note = None;
                }
            } else {
                a.kind = crate::attach::Kind::Image("image/png");
                a.size = png.len() as u64;
                a.bytes = Some(std::sync::Arc::from(png.into_boxed_slice()));
                a.original = original;
                a.edits = edits;
                a.note = Some("edited by the user".into());
            }
        }
        self.editing_attachment = None;
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
    /// their KodAct CLI on their next message (resuming the conversation), so
    /// the change reaches them; a running reply keeps what it started with.
    /// Idle chats' KodAct CLI processes end, so each starts again (resuming its
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

    /// At launch: find the Python the office-documents skill runs on, and
    /// set its libraries up when a Python is here without them
    /// (`doc_tools::prepare`). Not in the demo, which has no KodAct CLI, nor
    /// with `KODACT_NO_DOC_SETUP=1`.
    pub fn prepare_doc_tools(&mut self, cx: &mut Context<Self>) {
        let off = std::env::var("KODACT_NO_DOC_SETUP").is_ok_and(|v| v == "1");
        if demo_mode() || off {
            // Still worth knowing what is here: Settings says so.
            cx.spawn(async move |this, cx| {
                let office = cx
                    .background_spawn(async {
                        crate::doc_tools::find();
                        crate::doc_tools::libreoffice()
                    })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    s.libreoffice = office;
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        cx.spawn(async move |this, cx| {
            let (before, office) = cx
                .background_spawn(async {
                    (crate::doc_tools::find(), crate::doc_tools::libreoffice())
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.libreoffice = office;
                cx.notify();
            });
            let now = cx
                .background_spawn(async { crate::doc_tools::prepare() })
                .await;
            let _ = this.update(cx, |s, cx| {
                // Only an install is worth a word: finding one says nothing.
                if before != now {
                    s.doc_tools_said(&now, cx);
                    s.restart_idle_engines();
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Settings and the palette: set the document libraries up now.
    pub fn set_up_doc_tools(&mut self, cx: &mut Context<Self>) {
        use crate::doc_tools::Python;
        if crate::doc_tools::python() == Python::Installing {
            return self.show_toast("The document tools are being set up.", cx);
        }
        self.show_toast("Setting up the document tools…", cx);
        cx.spawn(async move |this, cx| {
            let now = cx
                .background_spawn(async {
                    match crate::doc_tools::find() {
                        Python::Ready(p) => Python::Ready(p),
                        _ => crate::doc_tools::install(),
                    }
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.doc_tools_said(&now, cx);
                s.restart_idle_engines();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn doc_tools_said(&mut self, now: &crate::doc_tools::Python, cx: &mut Context<Self>) {
        use crate::doc_tools::Python;
        match now {
            Python::Ready(_) => self.show_toast(
                "The document tools are ready: KodAct can edit Word, Excel, PowerPoint and PDF files.",
                cx,
            ),
            Python::Missing(why) => self.show_toast(
                format!("Couldn't set up the document tools. {why} Try again from Settings."),
                cx,
            ),
            Python::NoPython => self.show_toast(
                "The document tools need Python, and this computer has none. Install Python 3, then try again from Settings.",
                cx,
            ),
            Python::Installing | Python::Unknown => {}
        }
    }

    /// Settings and the palette: install LibreOffice (winget on Windows),
    /// or open its download page where that can't be done from here.
    pub fn install_libreoffice(&mut self, cx: &mut Context<Self>) {
        if self.libreoffice {
            return self.show_toast("LibreOffice is already installed.", cx);
        }
        if self.office_installing {
            return self.show_toast("LibreOffice is being installed.", cx);
        }
        if !cfg!(windows) {
            cx.open_url(crate::doc_tools::LIBREOFFICE_URL);
            return self.show_toast("Opened LibreOffice's download page.", cx);
        }
        self.office_installing = true;
        self.show_toast("Installing LibreOffice. This takes a few minutes.", cx);
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async { crate::doc_tools::install_libreoffice() })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.office_installing = false;
                s.libreoffice = done.is_ok();
                match done {
                    Ok(()) => s.show_toast("LibreOffice is installed.", cx),
                    Err(why) => {
                        cx.open_url(crate::doc_tools::LIBREOFFICE_URL);
                        s.show_toast(why, cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn toggle_browser(&mut self, cx: &mut Context<Self>) {
        let on = !self.browser;
        if on && crate::web_view::find_chromium().is_none() {
            return self.show_toast(
                "KodAct's browser needs Chromium, Chrome or Edge. Install one (or run: npx playwright install chromium), then turn it on again.",
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
                "Web browser on: KodAct's agents drive the browser in the side panel (open, click, type, read, run scripts, screenshots) while you watch. Each step asks first."
            } else {
                "Web browser off."
            },
            cx,
        );
        cx.notify();
    }

    /// Show a diagram's source, or its drawing again.
    /// Zoom a diagram by `factor` (between a quarter and four times its
    /// fitted size); `None` fits it to the column again.
    pub fn zoom_diagram(&mut self, key: SharedString, factor: Option<f32>, cx: &mut Context<Self>) {
        let map = std::sync::Arc::make_mut(&mut self.diagram_zoom);
        match factor {
            Some(f) => {
                let z = (map.get(&key).copied().unwrap_or(1.) * f).clamp(0.25, 4.);
                if (z - 1.).abs() < 0.01 {
                    map.remove(&key);
                } else {
                    map.insert(key, z);
                }
            }
            None => {
                map.remove(&key);
            }
        }
        if let Some(id) = self.current {
            if let Some(c) = self.chat_mut(id) {
                c.version += 1;
            }
        }
        cx.notify();
    }

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

    /// Revert the changes made since your message before `msg`: KodAct CLI's
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
            // A message from before KodAct learnt turn ids (or one never accepted).
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
            self.show_toast(format!("Couldn't ask KodAct what to revert: {e}"), cx);
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

    /// Send a control request to a chat's KodAct CLI (starting it, resumed,
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
            .ok_or("KodAct isn't running")?
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
        if let Request::SetMode { chat, from, mode } = req {
            if let Err(e) = result {
                // KodAct CLI stayed in `from`: the chip says so too, rather than show a mode
                // that isn't running (Bypass on the chip while every command still asks).
                // Unless the chat has moved on to another mode since.
                if self.chat_mode(chat) == mode {
                    self.mode_reported(chat, from);
                }
                self.show_toast(
                    format!(
                        "Couldn't switch to {}; still {}: {e}",
                        mode.label(),
                        from.label()
                    ),
                    cx,
                );
                cx.notify();
            }
            return;
        }
        if let Request::StopAgents { chat, ids } = req {
            return self.on_stop_reply(chat, &ids, result, cx);
        }
        if let Request::McpStatus { chat } = req {
            if let Ok(v) = result {
                self.mcp_servers
                    .insert(chat, crate::backend::McpServer::list(&v));
                cx.notify();
            }
            return;
        }
        if let Request::McpReconnect { chat, server } = req {
            if let Err(e) = result {
                self.show_toast(format!("Couldn't restart {server}: {e}"), cx);
            }
            self.ask_mcp_status(chat);
            return;
        }
        if let Request::ContextUsage { chat } = req {
            // An older KodAct CLI doesn't know the request: the card keeps
            // the composer's measure.
            if let Some(u) = result
                .ok()
                .as_ref()
                .and_then(crate::context::ContextUsage::from_json)
            {
                self.context_usage.insert(chat, u);
                cx.notify();
            }
            return;
        }
        if let Request::CancelQueued { chat, uuid, edit } = req {
            match result {
                Ok(v) if v["cancelled"] == true => self.unqueue(chat, &uuid, edit, cx),
                Ok(_) => self.show_toast(
                    "Already with KodAct: it reached the model before it could be taken back.",
                    cx,
                ),
                Err(e) => self.show_toast(format!("Couldn't take it back: {e}"), cx),
            }
            return;
        }
        if let Request::SendQueuedNow { chat, uuid } = req {
            match result {
                Ok(v) if v["cancelled"] == true => self.dispatch_queued_now(chat, &uuid, cx),
                Ok(_) => self.show_toast("Already with KodAct: it is already being processed.", cx),
                Err(e) => self.show_toast(format!("Couldn't send now: {e}"), cx),
            }
            return;
        }
        let files = |v: &serde_json::Value| -> Result<Vec<String>, String> {
            if v["canRewind"] == false {
                return Err(v["error"]
                    .as_str()
                    .unwrap_or("KodAct has no checkpoint for this message")
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
            (
                Request::StopAgents { .. }
                | Request::McpStatus { .. }
                | Request::McpReconnect { .. }
                | Request::CancelQueued { .. }
                | Request::SendQueuedNow { .. }
                | Request::ContextUsage { .. }
                | Request::SetMode { .. },
                _,
            ) => {}
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

    /// Pick a stopped reply back up. The chat's KodAct CLI process still holds
    /// the conversation after Stop, so KodAct is asked to go on; when the
    /// process is gone the conversation can't be resumed (a stub until P2).
    pub fn continue_reply(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.current else { return };
        if self.busy() {
            return;
        }
        // The live process still holds the conversation, and one KodAct CLI
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
        self.chat_sub = None;
        cx.notify();
    }

    /// Open chat `id`'s menu at `at` (a toggle when it's open already).
    pub fn chat_menu_from(
        &mut self,
        id: usize,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.chat_menu_at = at;
        let open = self.chat_menu != Some(id);
        self.open_chat_menu(open.then_some(id), cx);
    }

    /// Ask chat `chat`'s KodAct CLI what fills its context; the answer lands
    /// in `context_usage`. Nothing to ask without a running session.
    /// Ask a chat's KodAct CLI how its MCP servers are.
    pub fn ask_mcp_status(&mut self, chat: usize) {
        if let Some(Ok(id)) = self.sessions.get(&chat).map(|l| l.session.mcp_status()) {
            self.requests.insert(id, Request::McpStatus { chat });
        }
    }

    /// Start one of the current chat's MCP servers again.
    pub fn mcp_reconnect(&mut self, server: String, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        match self
            .sessions
            .get(&chat)
            .map(|l| l.session.mcp_reconnect(&server))
        {
            Some(Ok(id)) => {
                self.requests
                    .insert(id, Request::McpReconnect { chat, server });
            }
            Some(Err(e)) => self.show_toast(format!("Couldn't restart {server}: {e}"), cx),
            None => {}
        }
        cx.notify();
    }

    /// Set how long the model may think: every running chat's KodAct CLI
    /// uses it from its next request, and chats started later get it too.
    pub fn set_thinking_budget(&mut self, tokens: Option<u32>, cx: &mut Context<Self>) {
        self.thinking_budget = tokens;
        for live in self.sessions.values() {
            let _ = live.session.set_thinking_budget(tokens);
        }
        cx.notify();
    }

    pub fn ask_context_usage(&mut self, chat: usize) {
        if let Some(Ok(id)) = self.sessions.get(&chat).map(|l| l.session.context_usage()) {
            self.requests.insert(id, Request::ContextUsage { chat });
        }
    }

    /// Show or hide the Session column; opening it asks for fresh numbers.
    pub fn toggle_session(&mut self, cx: &mut Context<Self>) {
        self.session_open = !self.session_open;
        if self.session_open {
            if let Some(chat) = self.current {
                self.ask_context_usage(chat);
                self.ask_mcp_status(chat);
            }
        }
        cx.notify();
    }

    /// Open or close the Files pane; opening reads the project's tree.
    pub fn toggle_files(&mut self, cx: &mut Context<Self>) {
        self.files_open = !self.files_open;
        if self.files_open {
            self.ensure_completions(cx);
        }
        cx.notify();
    }

    /// Read the project's tree again, for the Files pane.
    pub fn refresh_files(&mut self, cx: &mut Context<Self>) {
        self.file_index = None;
        self.ensure_completions(cx);
        if let Some((rel, _)) = self.files_view.clone() {
            self.open_in_files(rel, cx);
        }
        cx.notify();
    }

    /// Open a project file from the Files pane: a document in the side
    /// panel above it, text in the pane's own viewer, anything else in the
    /// app the system opens it with. Read off the UI thread.
    pub fn open_in_files(&mut self, rel: String, cx: &mut Context<Self>) {
        let ext = rel
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(
            ext.as_str(),
            "md" | "markdown" | "svg" | "html" | "htm" | "docx" | "xlsx" | "pptx" | "pdf"
        ) {
            return self.open_project_file(rel, cx);
        }
        let remote = self
            .current_remote()
            .map(|r| (self.remote_extra(&r.target), r));
        let local = self.project_dir.join(&rel);
        self.files_view = Some((rel.clone(), None));
        cx.notify();
        let read = cx.background_spawn(async move {
            match remote {
                Some((extra, r)) => {
                    let full = format!("{}/{rel}", r.path.trim_end_matches('/'));
                    crate::remote::read_file(&r.target, &full, FILES_VIEW_MAX as u64, &extra)
                        .map_err(|e| e.to_string())
                }
                None => match std::fs::metadata(&local) {
                    Ok(m) if m.len() > FILES_VIEW_MAX as u64 => Err(format!(
                        "This file is {} KB; the viewer shows files up to {} KB.",
                        m.len() / 1024,
                        FILES_VIEW_MAX / 1024
                    )),
                    _ => std::fs::read(&local).map_err(|e| e.to_string()),
                },
            }
        });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            let _ = this.update(cx, |s, cx| {
                let Some((rel, text)) = s.files_view.as_mut() else {
                    return;
                };
                match read.map(String::from_utf8) {
                    Ok(Ok(t)) => *text = Some(Ok(t.into())),
                    // Not text: the system's app for it.
                    Ok(Err(_)) => {
                        let rel = rel.clone();
                        s.files_view = None;
                        s.open_project_file(rel, cx);
                    }
                    Err(e) => *text = Some(Err(e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Close the file in the Files pane's viewer.
    pub fn close_files_view(&mut self, cx: &mut Context<Self>) {
        self.files_view = None;
        cx.notify();
    }

    /// Show every file in a reply's "Edited N files" card, or five again.
    pub fn toggle_edited(&mut self, chat: usize, msg: usize, cx: &mut Context<Self>) {
        match self.edited_open.iter().position(|k| *k == (chat, msg)) {
            Some(i) => {
                self.edited_open.remove(i);
            }
            None => self.edited_open.push((chat, msg)),
        }
        cx.notify();
    }

    /// Fold or unfold one of the Session column's cards.
    pub fn fold_session_card(&mut self, card: &'static str, cx: &mut Context<Self>) {
        match self.session_folded.iter().position(|c| *c == card) {
            Some(i) => {
                self.session_folded.remove(i);
            }
            None => self.session_folded.push(card),
        }
        cx.notify();
    }

    /// Record that a title/footer menu closed on mouse-down outside at `at`.
    pub fn dismiss_title_menu(
        &mut self,
        which: TitleMenu,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.title_menu.is_some_and(|(m, _)| m == which) {
            self.title_menu_dismissed = Some((which, at));
            self.title_menu = None;
            cx.notify();
        }
    }

    /// Open or close one of the title bar's or sidebar's menus, at `at`.
    /// Clicking its own trigger while open closes it cleanly instead of immediately reopening.
    pub fn toggle_title_menu(
        &mut self,
        which: TitleMenu,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        if let Some((dismissed, pos)) = self.title_menu_dismissed.take() {
            if dismissed == which && pos == at {
                cx.notify();
                return;
            }
        }
        self.title_menu = match self.title_menu {
            Some((open, _)) if open == which => None,
            _ => {
                self.picker_open = false;
                self.modes_open = false;
                self.bypass_warning = false;
                self.design_menu = None;
                self.project_menu = false;
                self.context_open = false;
                Some((which, at))
            }
        };
        self.chat_menu = None;
        cx.notify();
    }

    /// Toggle a transient presentation-only UI key (e.g., show more lines, sub-group expand).
    pub fn toggle_ui(&mut self, key: String, chat: Option<usize>, cx: &mut Context<Self>) {
        self.toggle_ui_at(key, chat, None, cx);
    }

    /// Dismiss a transient UI key from an outside click, remembering `at` so
    /// clicking the trigger button itself does not immediately reopen it.
    pub fn dismiss_ui(&mut self, key: &str, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if self.ui_toggles.remove(key) {
            self.ui_dismissed.insert(key.to_string(), at);
            cx.notify();
        }
    }

    /// Toggle a transient UI key from a click at `at`, suppressing immediate
    /// reopen if `dismiss_ui` just closed it at the same pointer location.
    pub fn toggle_ui_at(
        &mut self,
        key: String,
        chat: Option<usize>,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let just_dismissed = self
            .ui_dismissed
            .remove(&key)
            .zip(at)
            .is_some_and(|(d, p)| {
                (d.x - p.x).abs() <= gpui::px(2.) && (d.y - p.y).abs() <= gpui::px(2.)
            });
        if just_dismissed {
            cx.notify();
            return;
        }
        if !self.ui_toggles.remove(&key) {
            self.ui_toggles.insert(key);
        }
        if let Some(id) = chat {
            if let Some(c) = self.chat_mut(id) {
                c.version += 1;
            }
        }
        cx.notify();
    }

    /// Show one of the chat menu's submenus (another closes).
    pub fn show_chat_sub(&mut self, sub: Option<ChatSub>, cx: &mut Context<Self>) {
        if self.chat_sub != sub {
            self.chat_sub = sub;
            cx.notify();
        }
    }

    /// Where chat `chat` can move: your other projects, and plain chats.
    pub fn move_targets(&self, chat: usize) -> Vec<(usize, SharedString)> {
        let here = self.chats.iter().find(|c| c.id == chat).map(|c| c.project);
        self.projects
            .iter()
            .filter(|p| Some(p.id) != here && !p.archived && (p.saved || p.scratch))
            .map(|p| {
                let name = if p.scratch {
                    "Plain chats".into()
                } else {
                    p.name.clone()
                };
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
                Err(e) => return eprintln!("kodact: chat {id} keeps its name: {e}"),
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

    /// Ask the model, apart from the chat, what KodAct is doing about
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
                Err(e) => eprintln!("kodact: no headline for chat {id}: {e}"),
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
    /// starts its own KodAct CLI conversation on its first message.
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

    /// Put a chat away from the sidebar's list, or bring it back. It stays
    /// open if it is; the sidebar's filter shows archived chats.
    pub fn toggle_archive(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some(c) = self.chat_mut(id) else { return };
        c.archived = !c.archived;
        let (archived, title) = (c.archived, c.title.clone());
        self.mark_dirty(id, cx);
        self.show_toast(
            if archived {
                format!("Archived “{title}”. The chat filter shows archived chats.")
            } else {
                format!("“{title}” is back in the list.")
            },
            cx,
        );
    }

    /// Show a chat's transcript in detail (every run and thought opened),
    /// or as usual.
    pub fn set_detailed(&mut self, id: usize, detailed: bool, cx: &mut Context<Self>) {
        self.chat_menu = None;
        if let Some(c) = self.chat_mut(id) {
            c.detailed = detailed;
            c.version += 1;
        }
        self.mark_dirty(id, cx);
        cx.notify();
    }

    /// The whole chat as Markdown, on the clipboard.
    pub fn copy_chat_markdown(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some(c) = self.chats.iter().find(|c| c.id == id) else {
            return;
        };
        let project = self
            .project(c.project)
            .map(|p| p.name.to_string())
            .unwrap_or_default();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(crate::export::markdown(
            c, &project,
        )));
        self.show_toast("Copied the chat as Markdown.", cx);
    }

    /// A new chat in the same project that picks up where this one got to:
    /// its last reply, in the box, ready for you to add to and send.
    pub fn continue_in_new_chat(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some(c) = self.chats.iter().find(|c| c.id == id) else {
            return;
        };
        let last = c
            .messages
            .iter()
            .rev()
            .find_map(|m| match m {
                Message::Assistant { blocks, .. } => {
                    let text: Vec<String> = blocks
                        .iter()
                        .filter_map(|b| match b {
                            Block::Text(t) => Some(t.to_string()),
                            _ => None,
                        })
                        .collect();
                    (!text.is_empty()).then(|| text.join("\n\n"))
                }
                _ => None,
            })
            .unwrap_or_default();
        let (project, title) = (c.project, c.title.clone());
        self.select_project(project, cx);
        self.new_chat(cx);
        let mut draft = format!("Continuing from the chat “{title}”.");
        if !last.trim().is_empty() {
            draft.push_str("\n\nWhere it got to:\n\n");
            draft.push_str(last.trim());
        }
        self.draft = Some(draft);
        cx.notify();
    }

    /// The folder a chat works in, opened in the system's file manager.
    pub fn open_chat_folder(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        match self.chat_project_dir(id) {
            Some(dir) => cx.open_with_system(&dir),
            None => self.show_toast(
                "This chat works on another machine: open its folder there.",
                cx,
            ),
        }
    }

    /// The folder a chat works in, opened in VS Code (`code` on the PATH).
    pub fn open_chat_in_vscode(&mut self, id: usize, cx: &mut Context<Self>) {
        self.chat_menu = None;
        let Some(dir) = self.chat_project_dir(id) else {
            return self.show_toast(
                "This chat works on another machine: open its folder there.",
                cx,
            );
        };
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_spawn(async move {
                    let mut cmd =
                        std::process::Command::new(if cfg!(windows) { "code.cmd" } else { "code" });
                    cmd.arg(&dir);
                    cmd.spawn().map(|_| ())
                })
                .await;
            if started.is_err() {
                let _ = this.update(cx, |s, cx| {
                    s.show_toast(
                        "VS Code isn't on this computer's PATH (the `code` command).",
                        cx,
                    )
                });
            }
        })
        .detach();
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
        self.history_unseeded.remove(&id);
        self.ratings.retain(|(c, _), _| *c != id);
        let title = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.clone());
        // Its file goes too (KodAct CLI's own log of the conversation stays, as it does for the CLI).
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
        // A plain chat's own folder goes with it (never the shared one, or a
        // folder of yours).
        if let Some(dir) = self.chat_project_dir(id).filter(|d| d.starts_with(scratch_dir().join("chats"))) {
            cx.background_spawn(async move {
                let _ = std::fs::remove_dir_all(dir);
            })
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
        self.toast_file = None;
        self.toast_for(text.into(), Duration::from_millis(2600), cx);
    }

    /// A toast about a file just saved, with Open and Show in folder: up
    /// longer, so there is time to reach for them.
    pub fn show_file_toast(
        &mut self,
        text: impl Into<SharedString>,
        path: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.toast_file = Some(path);
        self.toast_for(text.into(), Duration::from_millis(7000), cx);
    }

    /// Open the file the toast is about, in its own app (PowerPoint, Word…).
    pub fn toast_open(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.toast_file.clone() {
            cx.open_with_system(&p);
        }
    }

    /// Show the file the toast is about in Explorer, selected.
    pub fn toast_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.toast_file.clone() else { return };
        self.reveal_file(&p, cx);
    }

    /// Open a file KodAct made in its own app (PowerPoint, Word, Excel, the
    /// PDF reader), from a card.
    pub fn open_in_app(&mut self, file: &FileRef, cx: &mut Context<Self>) {
        match file
            .path
            .as_ref()
            .map(|p| std::path::PathBuf::from(p.as_ref()))
        {
            Some(p) => cx.open_with_system(&p),
            None => self.show_toast("This file isn't on disk; use Download to save it.", cx),
        }
    }

    /// Show a file in Explorer (Finder, the file manager), selected where the
    /// system can select it, else its folder.
    pub fn reveal_file(&mut self, p: &std::path::Path, cx: &mut Context<Self>) {
        if cfg!(windows) {
            let mut arg = std::ffi::OsString::from("/select,");
            arg.push(p.as_os_str());
            if std::process::Command::new("explorer")
                .arg(arg)
                .spawn()
                .is_ok()
            {
                return;
            }
        }
        if let Some(dir) = p.parent() {
            cx.open_with_system(dir);
        }
    }

    fn toast_for(&mut self, text: SharedString, how_long: Duration, cx: &mut Context<Self>) {
        self.toast = Some(text);
        self.toast_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(how_long).await;
            let _ = this.update(cx, |s, cx| {
                s.toast = None;
                s.toast_file = None;
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

    /// The tool the open chat waits on you to allow (not a question or a
    /// plan): its card's place, and whether it offers Always allow.
    pub fn waiting_tool(&self) -> Option<(usize, usize, usize, bool)> {
        let chat = self.current?;
        let (msg, block) = self.pending_question(chat)?;
        let c = self.chats.iter().find(|c| c.id == chat)?;
        match c.messages.get(msg) {
            Some(Message::Assistant { blocks }) => match blocks.get(block) {
                Some(Block::Permission {
                    title,
                    info,
                    verdict: Verdict::Pending,
                    ..
                }) if info.questions.is_empty()
                    && title.as_ref() != crate::backend::PLAN_ASK_TITLE =>
                {
                    Some((chat, msg, block, !info.every_time))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Answer the tool the open chat waits on (its card's keys: Enter, Shift
    /// Enter, Esc). False when nothing waits, or Always isn't offered.
    pub fn answer_waiting(&mut self, verdict: Verdict, cx: &mut Context<Self>) -> bool {
        match self.waiting_tool() {
            Some((_, _, _, false)) if verdict == Verdict::AllowedAlways => false,
            Some((chat, msg, block, _)) => {
                self.answer(chat, msg, block, verdict, cx);
                true
            }
            None => false,
        }
    }

    /// Deny the question `chat` waits on, telling KodAct what to do instead
    /// (you typed it in the composer while the card waited).
    pub fn deny_with_note(&mut self, chat: usize, note: String, cx: &mut Context<Self>) {
        if let Some((msg, block)) = self.pending_question(chat) {
            self.answer_with(chat, msg, block, Verdict::Denied, Some(note), cx);
        }
    }

    /// The questions on the card at (msg, block), as answered so far.
    fn questions_at(&self, chat: usize, msg: usize, block: usize) -> Vec<Question> {
        match self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| c.messages.get(msg))
        {
            Some(Message::Assistant { blocks }) => match blocks.get(block) {
                Some(Block::Permission { info, .. }) => info.questions.clone(),
                _ => vec![],
            },
            _ => vec![],
        }
    }

    /// Change a waiting question card's questions and the one it shows.
    fn edit_questions(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        f: impl FnOnce(&mut Vec<Question>, &mut usize),
    ) {
        if let Some(c) = self.chat_mut(chat) {
            if let Some(Message::Assistant { blocks }) = c.messages.get_mut(msg) {
                if let Some(Block::Permission {
                    verdict: Verdict::Pending,
                    info,
                    ..
                }) = blocks.get_mut(block)
                {
                    f(&mut info.questions, &mut info.step);
                    c.version += 1;
                }
            }
        }
    }

    /// Pick option `opt` of question `q`: one choice moves on to the next
    /// question not yet answered; with several allowed it toggles.
    pub fn pick_option(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        q: usize,
        opt: usize,
        cx: &mut Context<Self>,
    ) {
        self.edit_questions(chat, msg, block, |qs, step| pick(qs, step, q, opt));
        cx.notify();
    }

    /// Choose "Other" on question `q`: your own words replace the options
    /// picked, typed in the card's field (`set_own_text`).
    pub fn choose_other(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        q: usize,
        cx: &mut Context<Self>,
    ) {
        self.edit_questions(chat, msg, block, |qs, _| {
            if let Some(question) = qs.get_mut(q) {
                question.picked.clear();
                question.own.get_or_insert_with(String::new);
            }
        });
        cx.notify();
    }

    /// What you are typing as "Other" on the question the waiting card shows.
    pub fn set_own_text(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        let Some((msg, block)) = self.pending_question(chat) else {
            return;
        };
        self.edit_questions(chat, msg, block, |qs, step| {
            if let Some(question) = qs.get_mut(*step) {
                if question.own.is_some() {
                    question.own = Some(text);
                }
            }
        });
        cx.notify();
    }

    /// The question card the chat you're looking at waits on: (chat,
    /// message, block, the question it shows, what you typed there).
    pub fn waiting_question(&self) -> Option<(usize, usize, usize, usize, Option<String>)> {
        let chat = self.current?;
        let (msg, block) = self.pending_question(chat)?;
        let qs = self.questions_at(chat, msg, block);
        let step = match self
            .chats
            .iter()
            .find(|c| c.id == chat)?
            .messages
            .get(msg)?
        {
            Message::Assistant { blocks } => match blocks.get(block)? {
                Block::Permission { info, .. } => info.step,
                _ => 0,
            },
            _ => 0,
        };
        let own = qs.get(step)?.own.clone();
        Some((chat, msg, block, step, own))
    }

    /// Show question `q` on its card.
    pub fn question_tab(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        q: usize,
        cx: &mut Context<Self>,
    ) {
        self.edit_questions(chat, msg, block, |qs, step| {
            if q < qs.len() {
                *step = q;
            }
        });
        cx.notify();
    }

    /// Send your answers, once every question has one.
    pub fn submit_answers(
        &mut self,
        chat: usize,
        msg: usize,
        block: usize,
        cx: &mut Context<Self>,
    ) {
        if self
            .questions_at(chat, msg, block)
            .iter()
            .all(Question::answered)
        {
            self.answer(chat, msg, block, Verdict::Allowed, cx);
        }
    }

    /// What you typed while a question card waits: your own answer to the
    /// question it shows. The card moves on, and the last answer sends them
    /// all. A permission card is denied with it, as before.
    pub fn answer_in_words(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        let Some((msg, block)) = self.pending_question(chat) else {
            return;
        };
        if self.questions_at(chat, msg, block).is_empty() {
            return self.deny_with_note(chat, text, cx);
        }
        let mut done = false;
        self.edit_questions(chat, msg, block, |qs, step| {
            done = own_answer(qs, step, text);
        });
        if done {
            self.answer(chat, msg, block, Verdict::Allowed, cx);
        }
        cx.notify();
    }

    /// The chat you're looking at waits on your answer to a question.
    pub fn asking(&self) -> bool {
        self.busy()
            && self
                .current
                .and_then(|c| self.pending_question(c))
                .is_some()
    }

    /// What the chat you're looking at waits on is a question card.
    pub fn asking_question(&self) -> bool {
        self.current
            .and_then(|c| Some((c, self.pending_question(c)?)))
            .is_some_and(|(c, (m, b))| !self.questions_at(c, m, b).is_empty())
    }

    // ------------------------------------------------------------ messages sent while KodAct works

    /// Send `text` while the reply runs: shown as queued, and given to the
    /// model at its next step (between tool calls), or run next when the
    /// reply ends first (`backend::Session::queue`).
    pub fn queue_message(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(chat) = self.current else { return };
        // What is attached goes with it, as with a message sent between replies.
        if self.attachments.iter().any(|a| a.pending) {
            return self.show_toast("Wait a moment: an attachment is still being prepared.", cx);
        }
        let attachments = std::mem::take(&mut self.attachments);
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
                .ok_or_else(|| "KodAct isn't running".to_string())
                .and_then(|l| {
                    let documents = l.profile.wire != crate::endpoints::Wire::Openai;
                    let content = crate::attach::content_blocks(&text, &attachments, documents);
                    l.session.queue_content(content, &uuid)
                });
            if let Err(e) = sent {
                // Not sent: what was attached stays in the box.
                self.attachments = attachments;
                return self.show_toast(format!("Couldn't send it: {e}"), cx);
            }
        }
        self.queued.entry(chat).or_default().push(Queued {
            uuid,
            text: text.into(),
            attachments: attachments.iter().map(|a| a.for_history()).collect(),
        });
        cx.notify();
    }

    /// Take a queued message back (`edit`: into the box, to change it).
    /// Only once KodAct CLI confirms it hadn't reached the model yet.
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
                self.requests
                    .insert(id, Request::CancelQueued { chat, uuid, edit });
            }
            Some(Err(e)) => self.show_toast(format!("Couldn't take it back: {e}"), cx),
            None => {}
        }
    }

    /// Interrupt the running turn if needed and send this queued message immediately.
    pub fn send_queued_now(&mut self, chat: usize, uuid: String, cx: &mut Context<Self>) {
        if demo_mode() || !self.sessions.contains_key(&chat) {
            return self.dispatch_queued_now(chat, &uuid, cx);
        }
        let sent = self
            .sessions
            .get(&chat)
            .map(|l| l.session.cancel_queued(&uuid));
        match sent {
            Some(Ok(id)) => {
                self.requests
                    .insert(id, Request::SendQueuedNow { chat, uuid });
            }
            Some(Err(e)) => self.show_toast(format!("Couldn't send now: {e}"), cx),
            None => self.dispatch_queued_now(chat, &uuid, cx),
        }
    }

    fn pop_queued(&mut self, chat: usize, uuid: &str) -> Option<Queued> {
        self.queued.get_mut(&chat).and_then(|list| {
            list.iter()
                .position(|q| q.uuid == uuid)
                .map(|i| list.remove(i))
        })
    }

    fn dispatch_queued_now(&mut self, chat: usize, uuid: &str, cx: &mut Context<Self>) {
        let Some(q) = self.pop_queued(chat, uuid) else {
            return;
        };
        if self.replies.remove(&chat).is_some() {
            self.turn_started.remove(&chat);
            if let Some(live) = self.sessions.get(&chat) {
                if live.session.interrupt().is_ok() {
                    *self.stale_turns.entry(chat).or_default() += 1;
                } else {
                    self.sessions.remove(&chat);
                }
            }
            if let Some(ask) = self.asks.remove(&chat) {
                self.answer(chat, ask.msg, ask.block, Verdict::Denied, cx);
                self.set_decider(chat, ask.msg, ask.block, Decider::Stopped);
            }
            if let Some(c) = self.chat_mut(chat) {
                for m in c.messages.iter_mut() {
                    if let Message::Assistant { blocks } = m {
                        for b in blocks.iter_mut() {
                            if let Block::Tool(t) = b {
                                if t.status == ToolStatus::Running {
                                    t.status = ToolStatus::Interrupted;
                                    t.output = INTERRUPTED_CALL.into();
                                }
                            }
                        }
                    }
                }
                c.version += 1;
            }
            self.end_open_agent_runs(chat);
        }
        self.current = Some(chat);
        self.screen = Screen::Chat;
        let text = q.text.to_string();
        if let Some(c) = self.chat_mut(chat) {
            c.messages.push(Message::User {
                text: text.clone().into(),
                sent: SystemTime::now(),
                attachments: q.attachments.clone(),
                turn: None,
            });
            c.version += 1;
        }
        self.touch(chat, cx);
        if demo_mode() {
            self.play_script(chat, text, cx);
        } else {
            self.send_to_forge(chat, text, q.attachments, cx);
        }
        cx.notify();
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
        let Some(list) = self.queued.get_mut(&chat) else {
            return;
        };
        let Some(i) = list.iter().position(|q| q.uuid == uuid) else {
            return;
        };
        let q = list.remove(i);
        if edit {
            self.draft = Some(q.text.to_string());
            self.attachments.extend(q.attachments);
        }
        cx.notify();
    }

    /// Stopped, or KodAct ended: what was still queued goes back in the box,
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
            let joined = list
                .iter()
                .map(|q| q.text.to_string())
                .collect::<Vec<_>>()
                .join("\n\n");
            self.draft = Some(joined);
            for q in list {
                self.attachments.extend(q.attachments);
            }
        }
        cx.notify();
    }

    /// A replayed message of yours: the queued ones it carries reach the
    /// transcript here, as your messages, where the model read them. Returns
    /// whether it was one of them.
    fn land_queued(&mut self, chat: usize, uuid: &str, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(list) = self.queued.get_mut(&chat) else {
            return false;
        };
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
                    attachments: q.attachments,
                    turn: Some(uuid.to_string().into()),
                });
            }
            c.version += 1;
        }
        self.mark_dirty(chat, cx);
        cx.notify();
        true
    }

    /// The demo's next step: its queued messages land, as KodAct CLI's would.
    fn land_queued_demo(&mut self, chat: usize, cx: &mut Context<Self>) -> bool {
        let Some(list) = self.queued.get(&chat).filter(|l| !l.is_empty()) else {
            return false;
        };
        let text = list
            .iter()
            .map(|q| q.text.to_string())
            .collect::<Vec<_>>()
            .join("\n\n");
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
                .rposition(|b| {
                    matches!(
                        b,
                        Block::Permission {
                            verdict: Verdict::Pending,
                            ..
                        }
                    )
                })
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
        // A real question: KodAct waits on this answer.
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
            // Questions: your answers ride in the call's input; skipping
            // (or your own words typed as a denial) says so.
            if ask.tool_name == crate::backend::QUESTION_TOOL {
                let questions = self.questions_at(chat, msg, block);
                if !allow {
                    if let Some(&(m, b)) = self.tool_rows.get(&(chat, ask.tool_use_id.clone())) {
                        self.update_tool(chat, m, b, |t| {
                            t.status = ToolStatus::Declined;
                            t.output = "You skipped the questions.".into();
                        });
                    }
                }
                let sent = self
                    .sessions
                    .get(&chat)
                    .map(|l| match (allow, note.as_deref()) {
                        (true, _) => {
                            l.session
                                .answer_questions(&ask.request_id, &ask.input, &questions)
                        }
                        (false, Some(n)) => l.session.answer(
                            &ask.request_id,
                            false,
                            false,
                            &ask.input,
                            &ask.suggestions,
                            Some(n),
                        ),
                        (false, None) => l.session.skip_questions(&ask.request_id),
                    });
                if let Some(Err(e)) = sent {
                    self.notice(chat, e, true);
                    self.finish_turn(chat, cx);
                }
                cx.notify();
                return;
            }
            if !allow {
                if let Some(&(m, b)) = self.tool_rows.get(&(chat, ask.tool_use_id.clone())) {
                    let said = note.as_ref().map_or("You declined this.".to_string(), |n| {
                        format!("You declined this: {n}")
                    });
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
    /// Ctrl+Shift+Escape); KodAct carries on with what they found.
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

    /// Draw again each chat that has a stalled sub-agent
    /// (`agent_roster::is_stalled`).
    fn redraw_stalled(&mut self, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        let chats: Vec<usize> = self
            .agents
            .iter()
            .filter(|(_, runs)| {
                runs.iter()
                    .any(|r| crate::ui::agent_roster::is_stalled(r, now))
            })
            .map(|(&c, _)| c)
            .collect();
        if chats.is_empty() {
            return;
        }
        for chat in chats {
            if let Some(c) = self.chat_mut(chat) {
                c.version += 1;
            }
        }
        cx.notify();
    }

    /// The reply stopped: its sub-agents still open stop with it, so the
    /// Agents panel counts them as the transcript does (stopped, not running).
    fn end_open_agent_runs(&mut self, chat: usize) {
        let now = std::time::Instant::now();
        for r in self.agents.entry(chat).or_default() {
            if r.ended.is_none() {
                r.ended = Some(now);
                r.stopping = false;
                r.stopped = true;
                for s in r
                    .steps
                    .iter_mut()
                    .filter(|s| s.status == ToolStatus::Running)
                {
                    s.status = ToolStatus::Interrupted;
                    s.ended = Some(now);
                }
            }
        }
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
                    s.ended = Some(now);
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

    /// KodAct CLI's answer to a stop: the agents' own results follow (and mark
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
            // What KodAct says until the stopped turn's result belongs to that turn.
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
        // Design comments waiting for the reply stay on the page as notes:
        // KodAct didn't get them.
        if self.design_held.remove(&id).is_some() {
            for c in self.design_comments.iter_mut().filter(|c| c.chat == id) {
                if c.sent == CommentSent::Waiting {
                    c.sent = CommentSent::Note;
                }
            }
        }
        {
            if let Some(c) = self.chat_mut(id) {
                for m in c.messages.iter_mut() {
                    if let Message::Assistant { blocks } = m {
                        for b in blocks.iter_mut() {
                            if let Block::Tool(t) = b {
                                if t.status == ToolStatus::Running {
                                    t.status = ToolStatus::Interrupted;
                                    t.output = INTERRUPTED_CALL.into();
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
            self.end_open_agent_runs(id);
        }
        self.touch(id, cx);
        cx.notify();
    }

    /// Complete an out-of-band slash command: brief 1-line confirmations show
    /// as a toast, while multi-line inspection reports open in `command_modal`.
    fn finish_ephemeral_command(
        &mut self,
        command: SharedString,
        output: String,
        is_error: bool,
        toast_only: bool,
        cx: &mut Context<Self>,
    ) {
        let trimmed = output.trim();
        let single_line = !trimmed.contains('\n') && trimmed.chars().count() <= 140;
        if toast_only || (!is_error && single_line) {
            self.command_modal = None;
            let msg = if trimmed.is_empty() {
                format!("Ran {command}.")
            } else {
                trimmed.to_string()
            };
            self.show_toast(msg, cx);
            return;
        }
        let body = if trimmed.is_empty() {
            if is_error {
                "KodAct CLI reported an error for this command.".to_string()
            } else {
                "(no output)".to_string()
            }
        } else {
            trimmed.to_string()
        };
        self.command_modal = Some(CommandModal {
            command,
            output: body.into(),
            pending: false,
            is_error,
        });
        cx.notify();
    }

    /// Execute an engine slash command (`/status`, `/doctor`, `/reload-plugins`,
    /// `/diff`, `/fast`…) out-of-band without creating a user or assistant
    /// message in `c.messages`.
    fn run_ephemeral_cli_command(&mut self, cli_line: String, toast_only: bool, cx: &mut Context<Self>) {
        let label: SharedString = cli_line.clone().into();
        if demo_mode() {
            let demo_out = format!(
                "### `{cli_line}` (Demo)\n\nRan out-of-band without entering the conversation transcript."
            );
            return self.finish_ephemeral_command(label, demo_out, false, toast_only, cx);
        }
        let immediate = crate::commands::is_immediate(&cli_line);
        // Use the active chat's running KodAct CLI session when available.
        if let Some(id) = self.current.filter(|id| self.sessions.contains_key(id)) {
            if immediate {
                if !toast_only {
                    self.command_modal = Some(CommandModal {
                        command: label.clone(),
                        output: SharedString::default(),
                        pending: true,
                        is_error: false,
                    });
                }
                if let Some(live) = self.sessions.get(&id) {
                    match live.session.send(&cli_line) {
                        Ok(()) => {
                            self.immediate_queue
                                .entry(id)
                                .or_default()
                                .push_back(ImmediateSink::Modal(label));
                        }
                        Err(e) => self.finish_ephemeral_command(label, e, true, toast_only, cx),
                    }
                }
                cx.notify();
                return;
            }
            if self.busy() {
                return self.show_toast(
                    format!("Wait for the current reply to finish before running {cli_line}."),
                    cx,
                );
            }
            if !toast_only {
                self.command_modal = Some(CommandModal {
                    command: label.clone(),
                    output: SharedString::default(),
                    pending: true,
                    is_error: false,
                });
            }
            if let Some(live) = self.sessions.get(&id) {
                match live.session.send(&cli_line) {
                    Ok(()) => {
                        self.ephemeral_turns.insert(
                            id,
                            EphemeralTurn {
                                command: label,
                                output: String::new(),
                                is_error: false,
                                toast_only,
                            },
                        );
                        self.replies.insert(id, Task::ready(()));
                    }
                    Err(e) => self.finish_ephemeral_command(label, e, true, toast_only, cx),
                }
            }
            cx.notify();
            return;
        }

        // No live session running yet (e.g. on the `+ New` screen or an idle chat):
        // run a short-lived `--no-session-persistence` CLI process off the UI thread
        // so zero dummy chats or session files are created.
        let Some(exe) = crate::backend::find_cli() else {
            return self.show_toast(
                "KodAct CLI wasn't found. Check Settings → About or KODACT_CLI.",
                cx,
            );
        };
        let cwd = self
            .current
            .and_then(|id| self.chat_project_dir(id))
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| self.project_dir.clone());
        let base_launch = self.active_profile().and_then(|profile| {
            let eff = self.effort.filter(|e| profile.effort_levels().contains(e));
            let sp = self
                .settings_profile
                .as_deref()
                .and_then(crate::setting_profiles::path_of);
            crate::backend::launch(
                profile,
                eff,
                self.mode,
                self.bypass_allowed,
                crate::backend::Conversation::New("ephemeral"),
                Some(&cwd),
                sp.as_deref(),
            )
            .ok()
        });
        if !toast_only {
            self.command_modal = Some(CommandModal {
                command: label.clone(),
                output: SharedString::default(),
                pending: true,
                is_error: false,
            });
            cx.notify();
        }
        let line_for_bg = cli_line.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    crate::backend::run_oneshot_command(
                        &exe,
                        base_launch.as_ref(),
                        &cwd,
                        &line_for_bg,
                    )
                })
                .await;
            let _ = this.update(cx, |s, cx| match res {
                Ok((out, is_err)) => {
                    s.finish_ephemeral_command(label, out, is_err, toast_only, cx);
                }
                Err(e) => s.finish_ephemeral_command(label, e, true, toast_only, cx),
            });
        })
        .detach();
    }

    /// Intercept UI, settings, mode, and out-of-band engine slash commands so
    /// they execute directly without adding a `Message::User` bubble to
    /// `c.messages` or creating a dummy chat from `+ New`. Returns `true` when
    /// handled; returns `false` for conversational skills and prompt commands.
    pub fn try_slash_action(
        &mut self,
        raw: &str,
        window: Option<&mut gpui::Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        let trimmed = raw.trim();
        if let Some(spec) = trimmed.strip_prefix("/routine ") {
            self.add_routine(spec, cx);
            return true;
        }
        if trimmed.eq_ignore_ascii_case("/routine") {
            self.show_toast("Usage: /routine <when>: <request>", cx);
            return true;
        }
        if let Some(question) = parse_btw(trimmed) {
            self.ask_side(question, cx);
            return true;
        }
        if trimmed.eq_ignore_ascii_case("/btw") {
            self.show_toast("Usage: /btw <question>", cx);
            return true;
        }
        let Some((cmd_raw, args)) = crate::commands::parse_slash(trimmed) else {
            return false;
        };
        // Custom project `.md` commands and user/plugin skills with non-core names
        // stay conversational prompts.
        let exact_lower = cmd_raw.to_ascii_lowercase();
        let is_user_or_skill = self.completion_commands().iter().any(|c| {
            (c.group == "Skills" || c.group == "This project")
                && c.name.eq_ignore_ascii_case(&exact_lower)
        });
        if is_user_or_skill && crate::commands::normalize_action(cmd_raw).is_none() {
            return false;
        }
        if crate::commands::classify(trimmed) != Some(crate::commands::CommandKind::Action) {
            return false;
        }
        let key = crate::commands::normalize_action(cmd_raw)
            .map(str::to_string)
            .unwrap_or_else(|| exact_lower.replace('_', "-"));

        match key.as_str() {
            "plugins" => {
                if args.is_empty() || args.eq_ignore_ascii_case("list") {
                    self.open_settings(SettingsTab::Plugins, cx);
                } else {
                    self.run_ephemeral_cli_command(format!("/plugin {args}"), false, cx);
                }
            }
            "skills" => self.open_settings(SettingsTab::Skills, cx),
            "commands" => self.open_settings(SettingsTab::Commands, cx),
            "environments" => self.open_settings(SettingsTab::Environments, cx),
            "memory" => self.open_settings(SettingsTab::Memory, cx),
            "hooks" => {
                if args.is_empty() {
                    self.open_settings(SettingsTab::Hooks, cx);
                } else {
                    self.run_ephemeral_cli_command(format!("/hooks {args}"), false, cx);
                }
            }
            "agents" => {
                if args.is_empty() || args.eq_ignore_ascii_case("list") {
                    self.open_settings(SettingsTab::Agents, cx);
                } else {
                    self.run_ephemeral_cli_command(format!("/agents {args}"), false, cx);
                }
            }
            "mcp" => {
                if args.is_empty() {
                    self.open_settings(SettingsTab::Mcp, cx);
                } else {
                    let mut parts = args.split_whitespace();
                    let sub = parts.next().unwrap_or_default().to_ascii_lowercase();
                    let server = parts.next().unwrap_or("all");
                    match sub.as_str() {
                        "reconnect" if self.current.is_some_and(|id| self.sessions.contains_key(&id)) => {
                            self.mcp_reconnect(server.to_string(), cx);
                            self.show_toast(format!("Reconnecting MCP server ({server})…"), cx);
                        }
                        "enable" | "disable"
                            if self.current.is_some_and(|id| self.sessions.contains_key(&id)) =>
                        {
                            let enable = sub == "enable";
                            if let Some(chat) = self.current {
                                if let Some(live) = self.sessions.get(&chat) {
                                    let _ = live.session.mcp_toggle(server, enable);
                                }
                                self.ask_mcp_status(chat);
                            }
                            self.show_toast(
                                format!(
                                    "{} MCP server {server}.",
                                    if enable { "Enabled" } else { "Disabled" }
                                ),
                                cx,
                            );
                        }
                        _ => self.open_settings(SettingsTab::Mcp, cx),
                    }
                }
            }
            "permissions" => {
                if args.is_empty() {
                    self.open_settings(SettingsTab::Permissions, cx);
                } else {
                    self.run_ephemeral_cli_command(format!("/permissions {args}"), false, cx);
                }
            }
            "sandbox" => {
                if args.is_empty() {
                    self.open_settings(SettingsTab::Sandbox, cx);
                } else {
                    self.run_ephemeral_cli_command(format!("/sandbox {args}"), false, cx);
                }
            }
            "autocompact" => {
                if args.is_empty() {
                    self.open_settings(SettingsTab::Compaction, cx);
                } else {
                    self.run_ephemeral_cli_command(format!("/autocompact {args}"), false, cx);
                }
            }
            "config" => {
                if args.is_empty() {
                    self.open_settings(SettingsTab::General, cx);
                } else {
                    self.run_ephemeral_cli_command(format!("/config {args}"), false, cx);
                }
            }
            "settings" => self.open_settings(SettingsTab::General, cx),
            "help" => self.open_settings(SettingsTab::Guide, cx),
            "about" => self.open_settings(SettingsTab::About, cx),
            "model" => {
                if args.is_empty() {
                    self.picker_open = !self.picker_open;
                    self.modes_open = false;
                    self.context_open = false;
                    cx.notify();
                } else {
                    let q = args.to_ascii_lowercase();
                    let profiles = self
                        .endpoints
                        .as_ref()
                        .map(|e| e.profiles.as_slice())
                        .unwrap_or(&[]);
                    let found: Option<String> = profiles
                        .iter()
                        .find(|p| {
                            p.name.eq_ignore_ascii_case(args)
                                || p.model.eq_ignore_ascii_case(args)
                                || p.display_name
                                    .as_deref()
                                    .is_some_and(|d| d.eq_ignore_ascii_case(args))
                        })
                        .or_else(|| {
                            profiles.iter().find(|p| {
                                p.name.to_ascii_lowercase().contains(&q)
                                    || p.model.to_ascii_lowercase().contains(&q)
                            })
                        })
                        .map(|p| p.name.clone());
                    if let Some(name) = found {
                        self.select_profile(&name, cx);
                        self.show_toast(format!("Switched model profile to {name}."), cx);
                    } else {
                        self.picker_open = true;
                        self.show_toast(format!("No profile matched \"{args}\" — choose one here."), cx);
                    }
                }
            }
            "effort" => {
                if args.is_empty() {
                    self.picker_open = true;
                    cx.notify();
                } else {
                    let a = args.to_ascii_lowercase();
                    if matches!(a.as_str(), "auto" | "none" | "off" | "default") {
                        self.effort = None;
                        self.show_toast("Reasoning effort: Auto", cx);
                    } else {
                        let parsed = match a.as_str() {
                            "low" => Some(crate::endpoints::Effort::Low),
                            "medium" | "med" => Some(crate::endpoints::Effort::Medium),
                            "high" => Some(crate::endpoints::Effort::High),
                            "xhigh" | "extra-high" => Some(crate::endpoints::Effort::Xhigh),
                            "max" => Some(crate::endpoints::Effort::Max),
                            _ => None,
                        };
                        let levels = self
                            .active_profile()
                            .map(|p| p.effort_levels())
                            .unwrap_or_default();
                        match parsed {
                            Some(lvl) if levels.contains(&lvl) => {
                                self.effort = Some(lvl);
                                self.show_toast(format!("Reasoning effort: {}", lvl.label()), cx);
                            }
                            Some(lvl) if levels.is_empty() => {
                                self.effort = None;
                                self.show_toast(
                                    format!("Current model uses automatic effort ({} not offered).", lvl.label()),
                                    cx,
                                );
                            }
                            _ => {
                                let allowed: Vec<&str> =
                                    levels.iter().map(|l| l.cli_name()).collect();
                                self.show_toast(
                                    format!(
                                        "Supported effort levels: auto{}{}",
                                        if allowed.is_empty() { "" } else { ", " },
                                        allowed.join(", ")
                                    ),
                                    cx,
                                );
                            }
                        }
                    }
                }
            }
            "context" | "context-window" => {
                if key == "context" && !args.is_empty() {
                    self.run_ephemeral_cli_command(format!("/context {args}"), false, cx);
                } else if let Some(id) = self.current {
                    self.session_open = true;
                    self.ask_context_usage(id);
                    self.toggle_context(cx);
                } else {
                    self.show_toast("Start or open a conversation first to inspect its context.", cx);
                }
            }
            "tasks" => {
                if let Some(rest) = args.strip_prefix("stop") {
                    let target = rest.trim();
                    if target.is_empty() || target.eq_ignore_ascii_case("all") {
                        self.stop_agents(cx);
                        self.show_toast("Stopping background subagents…", cx);
                    } else {
                        self.stop_agent(target, cx);
                        self.show_toast(format!("Stopping subagent {target}…"), cx);
                    }
                } else if !args.is_empty() {
                    self.run_ephemeral_cli_command(format!("/tasks {args}"), false, cx);
                } else {
                    self.toggle_tasks(cx);
                }
            }
            "compact" => {
                self.compact_with((!args.is_empty()).then_some(args), cx);
            }
            "reload-plugins" | "reload-skills" => {
                self.custom_commands = None;
                self.file_index = None;
                self.ensure_completions(cx);
                let cli_cmd = format!("/{key}");
                if self
                    .current
                    .is_some_and(|id| self.sessions.contains_key(&id))
                    && !self.busy()
                {
                    self.run_ephemeral_cli_command(cli_cmd, true, cx);
                } else {
                    self.show_toast("Reloaded plugins, skills and commands.", cx);
                }
            }
            "plan" => {
                self.set_mode(ModeId::Plan, cx);
                self.show_toast("Switched to Plan mode.", cx);
            }
            "manual" => {
                self.set_mode(ModeId::Manual, cx);
                self.show_toast("Switched to Manual mode.", cx);
            }
            "edit-automatically" => {
                self.set_mode(ModeId::AcceptEdits, cx);
                self.show_toast("Switched to Edit automatically mode.", cx);
            }
            "auto" => {
                self.set_mode(ModeId::Auto, cx);
                self.show_toast("Switched to Auto mode.", cx);
            }
            "bypass-permissions" => {
                if self.bypass_allowed {
                    self.set_mode(ModeId::Bypass, cx);
                    self.show_toast("Switched to Bypass permissions mode.", cx);
                } else {
                    self.bypass_warning = true;
                    cx.notify();
                }
            }
            "goal" => {
                if args.eq_ignore_ascii_case("clear") {
                    if let Some(id) = self.current {
                        self.goals.remove(&id);
                        if self.sessions.contains_key(&id) && !self.busy() {
                            self.run_ephemeral_cli_command("/goal clear".into(), true, cx);
                            return true;
                        }
                    }
                    self.show_toast("Cleared active goal.", cx);
                } else if let Some(g) = self.current.and_then(|id| self.goals.get(&id)) {
                    self.show_toast(format!("Active goal: {}", g.condition), cx);
                } else {
                    self.show_toast("Usage: /goal <condition> (or /goal clear)", cx);
                }
            }
            "subtask" => self.show_toast("Usage: /subtask <task>", cx),
            "loop" => self.show_toast("Usage: /loop [interval] <prompt>", cx),
            "new" => self.new_chat(cx),
            "session" => self.toggle_session(cx),
            "files" => self.toggle_files(cx),
            "browser" => {
                let want = match args.to_ascii_lowercase().as_str() {
                    "on" | "true" | "1" => Some(true),
                    "off" | "false" | "0" => Some(false),
                    _ => None,
                };
                if want.is_none() || want != Some(self.browser) {
                    self.toggle_browser(cx);
                }
                self.show_toast(
                    format!(
                        "Browser tools: {}",
                        if self.browser { "on" } else { "off" }
                    ),
                    cx,
                );
            }
            "shortcuts" => {
                self.shortcuts_open = !self.shortcuts_open;
                cx.notify();
            }
            "terminal" => {
                if let Some(w) = window {
                    w.dispatch_action(Box::new(crate::ui::ToggleTerminal), cx);
                } else {
                    self.terminal_open = !self.terminal_open;
                    cx.notify();
                }
            }
            "theme" => {
                if let Some(w) = window {
                    w.dispatch_action(Box::new(crate::ui::ToggleTheme), cx);
                } else {
                    self.dark = !self.dark;
                    cx.notify();
                }
            }
            "find" => {
                if let Some(w) = window {
                    w.dispatch_action(Box::new(crate::ui::ToggleFind), cx);
                }
            }
            "attach" => {
                if let Some(w) = window {
                    w.dispatch_action(Box::new(crate::ui::AttachFiles), cx);
                }
            }
            "export" => {
                if let Some(id) = self.current {
                    self.export_chat(id, cx);
                } else {
                    self.show_toast("No active chat to export.", cx);
                }
            }
            "export-json" => {
                if let Some(id) = self.current {
                    self.export_chat_json(id, cx);
                } else {
                    self.show_toast("No active chat to export.", cx);
                }
            }
            "status" | "usage" | "doctor" | "diff" | "fast" | "advisor" | "output-style"
            | "add-dir" | "feedback" | "import" | "release-notes" => {
                let cli_line = if args.is_empty() {
                    format!("/{key}")
                } else {
                    format!("/{key} {args}")
                };
                self.run_ephemeral_cli_command(cli_line, false, cx);
            }
            _ => return false,
        }
        true
    }

    /// Add your message, and have KodAct answer it (`send_to_forge`).
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        // UI, settings and out-of-band engine commands never enter `c.messages`.
        if self.try_slash_action(&text, None, cx) {
            return;
        }
        if (text.is_empty() && self.attachments.is_empty()) || self.busy() {
            return;
        }
        // `/plan <description>` switches into Plan mode as it sends your prompt.
        if crate::commands::parse_slash(&text)
            .is_some_and(|(cmd, args)| cmd.eq_ignore_ascii_case("plan") && !args.is_empty())
        {
            self.set_mode(ModeId::Plan, cx);
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
        // KodAct CLI reads a command (`/simplify`) only from a message that is
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
                self.chats.insert(
                    0,
                    Chat {
                        design: self.design_next.take(),
                        ..Chat::new(id, self.current_project, title.clone().into())
                    },
                );
                self.current = Some(id);
                self.keep_mode();
                if let Some(profile) = profile.filter(|_| !demo_mode() && !text.trim().is_empty()) {
                    self.name_chat(id, profile, text.clone(), title, cx);
                }
                id
            }
        };
        self.screen = Screen::Chat;
        // The working line says what KodAct is doing about this message once
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

    /// `KODACT_DEMO=1`: replies are the scripted demo, no KodAct CLI needed.
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
        // When the turn began, as for a real reply (the live design keeps
        // the page as it was before a new turn's first change).
        self.turn_started.insert(id, std::time::Instant::now());
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
                    // KodAct CLI, and the demo says it heard you.
                    if matches!(step, Step::ToolStart(..) | Step::End { .. }) {
                        let _ = this.update(cx, |s, cx| {
                            if s.land_queued_demo(id, cx) {
                                s.apply(
                                    id,
                                    Step::Say("Noted, I'll take that into account. ".into()),
                                    cx,
                                );
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
                    s.send_design_held(id, cx);
                    cx.notify();
                });
            }),
        );
        cx.notify();
    }

    // ------------------------------------------------------------ the engine

    /// Send your message to the chat's KodAct CLI process, starting it first
    /// when the chat has none (or the endpoint changed under it).
    fn send_to_forge(
        &mut self,
        id: usize,
        text: String,
        attachments: Vec<crate::attach::Attachment>,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.active_profile().cloned() else {
            return self.notice(id, "Set up an endpoint first: KodAct needs a model to answer. Open the model menu to add one.", true);
        };
        let effective_effort = self
            .effort
            .filter(|e| profile.effort_levels().contains(e));
        if let Some(live) = self.sessions.get(&id) {
            // A nudge or the reasoning carry-over changed in Settings:
            // KodAct CLI reads them as it starts, so it restarts and resumes.
            if live.effort != effective_effort || live.behaviour != self.behaviour_version {
                self.sessions.remove(&id);
            } else if live.profile != profile {
                let same_endpoint = live.profile.base_url == profile.base_url
                    && live.profile.wire == profile.wire
                    && live.profile.auth == profile.auth
                    && live.profile.headers == profile.headers;
                if same_endpoint && live.session.set_model(&profile.model).is_ok() {
                    self.sessions.get_mut(&id).expect("checked above").profile = profile.clone();
                } else {
                    // Another endpoint: KodAct restarts on it and resumes this
                    // conversation, so the new model reads every message above.
                    self.sessions.remove(&id);
                }
            }
        }
        // A design chat's setup is part of the process's system prompt
        // (changed, perhaps, while a reply ran): a change restarts the
        // chat's KodAct CLI, resuming the conversation.
        let resumable = self.chats.iter().any(|c| c.id == id && c.resumable);
        let design = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.design.clone());
        if resumable && self.sessions.get(&id).is_some_and(|l| l.design != design) {
            self.sessions.remove(&id);
        }
        // A remote project's first message connects (and installs KodAct CLI
        // there if needed) in the background, then sends.
        if !self.sessions.contains_key(&id) {
            if let Some(remote) = self.chat_remote(id) {
                if !self.remote_ready.contains_key(&remote.target) {
                    return self.connect_then_send(id, remote.target, text, attachments, cx);
                }
            }
        }
        let had_live_session = self.sessions.contains_key(&id);
        if !had_live_session {
            if let Err(e) = self.start_session(id, profile, cx) {
                return self.notice(id, e, true);
            }
        }
        // A new conversation's first message carries the project's knowledge
        // ahead of your words (`knowledge.rs`); a command doesn't.
        let resumable = self.chats.iter().any(|c| c.id == id && c.resumable);
        let dir = self.chat_project_dir(id);
        let memory_on = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| self.project(c.project))
            .is_some_and(|p| p.memory_on);
        // A plain chat (no folder) carries none: the scratch place's memory
        // is no project's, and one chat's notes there would reach the next.
        let knowledge =
            (!resumable && !text.trim_start().starts_with('/') && !self.chat_is_plain(id))
                .then_some(dir.as_ref())
                .flatten()
                .and_then(|dir| {
                    let mut block = crate::knowledge::block(dir).unwrap_or_default();
                    if memory_on {
                        block.push_str(&crate::project_memory::prompt(
                            &crate::project_memory::read(dir),
                            &crate::project_memory::file(dir),
                        ));
                    }
                    (!block.trim().is_empty()).then_some(block)
                });
        let fresh_unresumed = !had_live_session && !resumable;
        let history_block = self.take_outgoing_history(id, &text, fresh_unresumed);
        self.snapshot_memory(id);
        let sent = self
            .sessions
            .get(&id)
            .map(|l| {
                if knowledge.is_some() || history_block.is_some() {
                    let mut content = crate::attach::content_blocks(
                        &text,
                        &attachments,
                        l.profile.wire != crate::endpoints::Wire::Openai,
                    );
                    if let Some(blocks) = content.as_array_mut() {
                        let mut at = 0;
                        if let Some(k) = &knowledge {
                            blocks.insert(
                                at,
                                serde_json::json!({ "type": "text", "text": context_block(k) }),
                            );
                            at += 1;
                        }
                        if let Some(h) = &history_block {
                            blocks.insert(
                                at,
                                serde_json::json!({ "type": "text", "text": h }),
                            );
                        }
                    }
                    l.session.send_content(content)
                } else if attachments.is_empty() {
                    l.session.send(&text)
                } else {
                    l.session.send_content(crate::attach::content_blocks(
                        &text,
                        &attachments,
                        l.profile.wire != crate::endpoints::Wire::Openai,
                    ))
                }
            })
            .unwrap_or(Err("KodAct isn't running.".into()));
        if let Err(e) = sent {
            if history_block.is_some() {
                self.history_unseeded.insert(id);
            }
            self.sessions.remove(&id);
            return self.notice(id, e, true);
        }
        // A command KodAct CLI answers at once (`/status`) starts no turn: its
        // answer comes back as an immediate result.
        if attachments.is_empty() && crate::commands::is_immediate(&text) {
            self.immediate_queue
                .entry(id)
                .or_default()
                .push_back(ImmediateSink::Modal(text.clone().into()));
            return;
        }
        self.replies.insert(id, Task::ready(()));
        self.turn_started.insert(id, std::time::Instant::now());
    }

    /// Take `<conversation_history>` to prepend on `id`'s outgoing message when
    /// its KodAct CLI process started fresh mid-conversation. Slash commands
    /// (`/reload-plugins`) go out bare while keeping `history_unseeded` armed
    /// for the next normal message.
    fn take_outgoing_history(
        &mut self,
        id: usize,
        text: &str,
        fresh_unresumed: bool,
    ) -> Option<String> {
        let needs_seed = fresh_unresumed || self.history_unseeded.contains(&id);
        if !needs_seed {
            return None;
        }
        let prior = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| prior_conversation_block(prior_messages(&c.messages)));
        let Some(block) = prior else {
            self.history_unseeded.remove(&id);
            return None;
        };
        if text.trim_start().starts_with('/') {
            self.history_unseeded.insert(id);
            None
        } else {
            self.history_unseeded.remove(&id);
            Some(block)
        }
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
        let effective_effort = self
            .effort
            .filter(|e| profile.effort_levels().contains(e));
        let local_project_dir = self.chat_project_dir(id);
        let profile_path = self
            .settings_profile
            .as_deref()
            .and_then(crate::setting_profiles::path_of);
        let mut launch = crate::backend::launch(
            &profile,
            effective_effort,
            self.mode,
            self.bypass_allowed,
            conversation,
            local_project_dir.as_deref(),
            profile_path.as_deref(),
        )?;
        // The web browser: KodAct's own browser tools (this executable, as
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
        // A design chat: its prompt, built from the library, and the design
        // tools on the panel's browser (started for it even when the
        // browser setting is off). A remote KodAct CLI gets the prompt through
        // its boot script (`remote::HOST_FILE`) and the tools over HTTP,
        // forwarded back here; its pages are read from the remote.
        let design = self
            .chats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.design.clone());
        if let (Some(setup), Some(root)) = (design, crate::design::root()) {
            // Read in the background when the chat became a design chat;
            // if that hasn't finished, read now.
            let ix = self
                .design_index
                .clone()
                .unwrap_or_else(|| std::sync::Arc::new(crate::design::library::Index::load(&root)));
            let text = crate::design::prompt::compose(&ix, &setup);
            self.design_index = Some(ix);
            let cdp = self.ensure_web(cx).unwrap_or_default();
            match &remote {
                None => {
                    let file = crate::profile::dir()
                        .unwrap_or_else(std::env::temp_dir)
                        .join("design-prompts")
                        .join(format!("{session_id}.md"));
                    let written = file
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|_| std::fs::write(&file, text));
                    if written.is_ok() {
                        launch.args.extend([
                            "--append-system-prompt-file".to_string(),
                            file.to_string_lossy().into_owned(),
                        ]);
                    }
                    if let Ok(exe) = std::env::current_exe() {
                        launch.args.extend([
                            "--mcp-config".to_string(),
                            crate::design_mcp::mcp_config(
                                &exe,
                                &cdp,
                                &crate::browser::focus_file(),
                            ),
                        ]);
                    }
                }
                Some(remote) => {
                    let base = crate::app_settings::get().server_install_dir;
                    let file = format!(
                        "{}/design-prompts/{session_id}.md",
                        base.trim().trim_end_matches('/')
                    );
                    launch
                        .env
                        .push((crate::remote::HOST_FILE.into(), file.clone()));
                    launch.env.push((crate::remote::HOST_TEXT.into(), text));
                    launch
                        .args
                        .extend(["--append-system-prompt-file".to_string(), file]);
                    if let Some((port, token)) = self.ensure_design_http(remote, &cdp) {
                        let there =
                            crate::remote::forward_port(&format!("{session_id}-design"), port);
                        launch.args.extend([
                            "--mcp-config".to_string(),
                            crate::design_mcp::http_mcp_config(there, &token),
                        ]);
                        backs.push((there, port));
                    }
                }
            }
        }
        // The settings profile, layered above the other settings files.
        if let Some(mut flag) = crate::modes::flag_settings(profile_path.as_deref()) {
            // A remote KodAct CLI can't read this machine's file: send its settings.
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
            // KodAct's plugin, as put on the remote.
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
            // KodAct CLI's relay beside the session.
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
            let exe = crate::backend::find_cli().ok_or("KodAct CLI wasn't found. Reinstall KodAct (it puts kodact.exe in the bin folder beside the app), or set KODACT_CLI to its path.")?;
            // A plain chat works in a folder of its own (made now, on its
            // first message); a project's chat in the project.
            let dir = self
                .chat_project_dir(id)
                .inspect(|d| {
                    let _ = std::fs::create_dir_all(d);
                })
                .filter(|p| p.is_dir())
                .unwrap_or_else(|| self.project_dir.clone());
            if self.current == Some(id) {
                self.follow_chat_folder(id);
            }
            crate::backend::Session::spawn(&exe, &launch, &dir)?
        };
        let events = session.events.clone();
        // The thinking budget you set reaches each chat's KodAct CLI.
        if self.thinking_budget.is_some() {
            let _ = session.set_thinking_budget(self.thinking_budget);
        }
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
                effort: effective_effort,
                behaviour: self.behaviour_version,
                generation,
                design: self
                    .chats
                    .iter()
                    .find(|c| c.id == id)
                    .and_then(|c| c.design.clone()),
                _relays: relays,
            },
        );
        if self.session_open {
            self.ask_context_usage(id);
            self.ask_mcp_status(id);
        }
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
            // the conversation couldn't be reopened on disk. Start a fresh
            // CLI conversation seeded with the chat's earlier transcript,
            // keeping its working folder intact.
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
            let scratch_root = self
                .chats
                .iter()
                .find(|c| c.id == chat)
                .and_then(|c| self.project(c.project))
                .filter(|p| p.scratch)
                .map(|p| p.path.clone());
            let mut old_session = None;
            let mut has_prior = false;
            if let Some(c) = self.chat_mut(chat) {
                has_prior = prior_conversation_block(prior_messages(&c.messages)).is_some();
                if let Some(root) = scratch_root {
                    if c.folder.is_none() && chat_folder(&root, &c.session, c.resumable) == root {
                        c.folder = Some(String::new());
                    }
                }
                old_session = Some(c.session.clone());
                c.new_conversation();
            }
            if let (Some(dir), Some(old)) = (self.history.clone(), old_session) {
                cx.background_spawn(async move { crate::history::remove(&dir, &old) })
                    .detach();
            }
            if has_prior {
                self.history_unseeded.insert(chat);
                self.notice(
                    chat,
                    "Reopened this conversation from your chat history.",
                    false,
                );
            }
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
                        let _ =
                            l.session
                                .answer(&request_id, false, false, &input, &suggestions, None);
                    }
                    return;
                }
                Event::Exited { .. } | Event::Immediate { .. } => {}
                _ => return,
            }
        }
        // Out-of-band slash commands (`/doctor`, `/reload-plugins`, `/diff`…)
        // accumulate into a toast or `command_modal`, never into `c.messages`.
        if self.ephemeral_turns.contains_key(&chat) {
            match &ev {
                Event::Immediate { .. } | Event::ControlReply { .. } | Event::ModeChanged(_) => {}
                Event::UserAccepted { .. } | Event::Thinking(_) => return,
                Event::Break => {
                    if let Some(ep) = self.ephemeral_turns.get_mut(&chat) {
                        if !ep.output.is_empty() && !ep.output.ends_with('\n') {
                            ep.output.push_str("\n\n");
                        }
                    }
                    return;
                }
                Event::Text(t) | Event::Status(t) => {
                    if let Some(ep) = self.ephemeral_turns.get_mut(&chat) {
                        ep.output.push_str(t);
                        if !ep.toast_only {
                            self.command_modal = Some(CommandModal {
                                command: ep.command.clone(),
                                output: ep.output.clone().into(),
                                pending: true,
                                is_error: ep.is_error,
                            });
                        }
                    }
                    return cx.notify();
                }
                Event::Error(e) => {
                    if let Some(ep) = self.ephemeral_turns.remove(&chat) {
                        let out = if ep.output.trim().is_empty() {
                            e.clone()
                        } else {
                            format!("{}\n{e}", ep.output.trim())
                        };
                        self.finish_ephemeral_command(ep.command, out, true, ep.toast_only, cx);
                    }
                    self.finish_turn(chat, cx);
                    return;
                }
                Event::TurnEnd { error, .. } => {
                    if let Some(ep) = self.ephemeral_turns.remove(&chat) {
                        let is_err = ep.is_error || error.is_some();
                        let out = match error {
                            Some(e) if ep.output.trim().is_empty() => e.clone(),
                            Some(e) => format!("{}\n{e}", ep.output.trim()),
                            None => ep.output,
                        };
                        self.finish_ephemeral_command(ep.command, out, is_err, ep.toast_only, cx);
                    }
                    self.finish_turn(chat, cx);
                    return;
                }
                Event::Exited { code, detail } => {
                    if let Some(ep) = self.ephemeral_turns.remove(&chat) {
                        let out = if !ep.output.trim().is_empty() {
                            ep.output
                        } else if !detail.is_empty() {
                            detail.clone()
                        } else {
                            format!("KodAct stopped (exit code {}).", code.unwrap_or(-1))
                        };
                        self.finish_ephemeral_command(
                            ep.command,
                            out,
                            code.unwrap_or(0) != 0,
                            ep.toast_only,
                            cx,
                        );
                    }
                    self.sessions.remove(&chat);
                    self.finish_turn(chat, cx);
                    return;
                }
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
            Event::Goal(goal) => {
                let checked = self.goals.contains_key(&chat);
                match (goal.status, goal.reason.as_deref()) {
                    // A check found it not met: another turn starts.
                    (GoalStatus::Working, Some(why)) if checked => {
                        self.notice(
                            chat,
                            format!("Goal not met yet: {why} KodAct keeps going."),
                            false,
                        );
                        self.goal_keeps_working(chat);
                    }
                    (GoalStatus::Achieved, why) => {
                        self.notice(
                            chat,
                            format!("Goal achieved: {}", why.unwrap_or(&goal.condition)),
                            false,
                        );
                    }
                    (GoalStatus::Failed, why) => {
                        self.notice(
                            chat,
                            format!("Goal can't be met: {}", why.unwrap_or(&goal.condition)),
                            true,
                        );
                    }
                    (GoalStatus::Paused, Some(why)) => self.notice(chat, why.to_string(), false),
                    _ => {}
                }
                // The loop stopped: nothing more comes until you write.
                if goal.status != GoalStatus::Working
                    && self.replies.contains_key(&chat)
                    && !self.asks.contains_key(&chat)
                    && self.goal_waiting.remove(&chat)
                {
                    self.replies.remove(&chat);
                    self.turn_started.remove(&chat);
                }
                if goal.status == GoalStatus::Cleared {
                    self.goals.remove(&chat);
                } else {
                    self.goals.insert(chat, goal);
                }
                cx.notify();
            }
            // What KodAct CLI did on its own (a check, a summary, a nudge):
            // a quiet line in the chat, where it happened.
            Event::Status(line) => {
                self.notice(chat, line, false);
                self.fresh_block.insert(chat);
                cx.notify();
            }
            Event::ModeChanged(m) => {
                if let Some(mode) = ModeId::from_cli(&m) {
                    self.mode_reported(chat, mode);
                    cx.notify();
                }
            }
            Event::Text(t) => self.apply(chat, Step::Say(t), cx),
            Event::Thinking(t) => self.apply(chat, Step::Think(t), cx),
            Event::ToolUse { id, name, input } => {
                self.panel_follows(chat, &name);
                if name == "Bash" || name == "PowerShell" {
                    self.command_started
                        .insert((chat, id.clone()), SystemTime::now());
                }
                self.design_call(chat, &name, &input, cx);
                self.watch_documents(chat, &name, &input, cx);
                if let Some(at) =
                    self.push_block(chat, Block::Tool(crate::backend::tool_call(&name, &input)))
                {
                    self.tool_rows.insert((chat, id), at);
                }
            }
            Event::Drafting { path, content, .. } => {
                if crate::design::live::is_page(&path) {
                    self.design_draft(chat, &path, &content, cx)
                } else {
                    self.text_draft(chat, &path, &content, cx)
                }
            }
            Event::ToolResult {
                id,
                output,
                is_error,
                images,
            } => {
                let started = self.command_started.remove(&(chat, id.clone()));
                if let Some(&(m, b)) = self.tool_rows.get(&(chat, id)) {
                    // A document a command made (a script writing a .docx,
                    // a PDF export): its card too, as for one Write made.
                    if let (Some(since), false) = (started, is_error) {
                        let command = self
                            .tool_at(chat, m, b)
                            .map(|t| t.target.to_string())
                            .unwrap_or_default();
                        let found = Self::document_paths(&command, &output);
                        if !found.is_empty() {
                            self.documents_made(chat, found, since, cx);
                        }
                        // And the ones it made without naming them (`python
                        // build_deck.py`): a bounded look at the chat's folder,
                        // off this thread. A deck already shown is updated, not
                        // shown twice.
                        let command_call = self
                            .tool_at(chat, m, b)
                            .is_some_and(|t| t.kind == ToolKind::Bash);
                        if let Some(root) = self.chat_project_dir(chat).filter(|_| command_call) {
                            let after = since - std::time::Duration::from_secs(2);
                            let look = cx.background_spawn(async move {
                                crate::live_docs::recent_documents(&root, after)
                            });
                            cx.spawn(async move |this, cx| {
                                let fresh = look.await;
                                if fresh.is_empty() {
                                    return;
                                }
                                let _ = this.update(cx, |s, cx| {
                                    let paths =
                                        fresh.iter().map(|p| p.display().to_string()).collect();
                                    s.documents_made(chat, paths, since, cx);
                                });
                            })
                            .detach();
                        }
                    }
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
                            t.images = images;
                            if !is_error && matches!(t.kind, ToolKind::Write | ToolKind::Edit) {
                                written = Some((t.target.to_string(), t.kind == ToolKind::Edit));
                            }
                        }
                    });
                    // A page it wrote (or tried to): the live view shows the file.
                    if let Some(p) = page {
                        self.design_done(chat, &p, cx);
                    }
                    // A document KodAct wrote gets its card in the reply; one
                    // it edited, the card brought up to date.
                    if let Some((p, edit)) = written {
                        self.end_draft(chat, &p, cx);
                        if let Some(remote) = self.chat_remote(chat) {
                            self.remote_written_file(chat, remote, p, edit, cx);
                        } else if let Some(file) = self.written_file(chat, &p) {
                            self.place_card(chat, file, edit, cx);
                        }
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
                // "Always allow" allows the command, not this one use of it.
                let suggestions = command_rules(suggestions);
                // The chip says Bypass: no card, whatever mode the engine
                // thinks it is in. It is told again, so it stops asking.
                if crate::modes::bypasses(self.chat_mode(chat), &tool_name) {
                    if let Some(l) = self.sessions.get(&chat) {
                        let _ =
                            l.session
                                .answer(&request_id, true, false, &input, &suggestions, None);
                        let _ = l.session.set_mode(ModeId::Bypass);
                    }
                    return;
                }
                // A harmless command, for the mode the chat is in, runs
                // without a card: its tool row already shows it.
                if !crate::backend::always_asks(&tool_name)
                    && self.auto_approves(chat, &tool_name, &input)
                {
                    if let Some(l) = self.sessions.get(&chat) {
                        let _ =
                            l.session
                                .answer(&request_id, true, false, &input, &suggestions, None);
                    }
                    return;
                }
                let (title, detail) = crate::backend::ask_text(&tool_name, &input);
                let row = self.tool_rows.get(&(chat, tool_use_id.clone())).copied();
                // A tool KodAct CLI always asks about offers no rule.
                let offers = !crate::backend::always_asks(&tool_name);
                let info = PermInfo {
                    kind: Some(crate::backend::tool_call(&tool_name, &input).kind),
                    offer: rule_name(&suggestions).filter(|_| offers).map(Into::into),
                    scope: rule_scope(&suggestions).filter(|_| offers),
                    every_time: !offers,
                    questions: if tool_name == crate::backend::QUESTION_TOOL {
                        crate::backend::questions(&input)
                    } else {
                        vec![]
                    },
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
                // Out-of-band inspection commands (`/status`, `/usage`…) open in
                // `command_modal` or toast; `/btw` answers go to the side card.
                let sink = self
                    .immediate_queue
                    .get_mut(&chat)
                    .and_then(|q| q.pop_front())
                    .unwrap_or(ImmediateSink::Side);
                match sink {
                    ImmediateSink::Modal(command) => {
                        self.finish_ephemeral_command(command, text, is_error, false, cx);
                    }
                    ImmediateSink::Side => {
                        self.answer_side(chat, text, is_error);
                    }
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
                    let ok = error.is_none();
                    if let Some(c) = self.chat_mut(chat) {
                        c.messages.retain(|m| !matches!(m, Message::Notice { text, .. } if text.as_ref() == COMPACTING));
                        if ok {
                            c.resumable = true;
                        }
                    }
                    match error {
                        Some(e) => self.notice(chat, e, true),
                        None => self.notice(chat, COMPACTED, false),
                    }
                    return self.finish_turn(chat, cx);
                }
                let ok = error.is_none();
                if let Some(e) = error {
                    self.notice(chat, e, true);
                }
                // The turn changed what fills the context: ask again.
                self.ask_context_usage(chat);
                let seconds = if ms > 0 {
                    ms as f32 / 1000.
                } else {
                    self.turn_started
                        .get(&chat)
                        .map(|t| t.elapsed().as_secs_f32())
                        .unwrap_or(0.)
                };
                if let Some(c) = self.chat_mut(chat) {
                    c.record_turn_end(seconds, usage, ok);
                }
                self.finish_turn(chat, cx);
                // A goal at work: KodAct CLI checks it now and goes on until it
                // holds, so the chat keeps working until the goal says.
                if self.goals.get(&chat).map(|g| g.status) == Some(GoalStatus::Working) {
                    self.goal_keeps_working(chat);
                }
            }
            Event::Error(e) => {
                self.notice(chat, e, true);
                self.finish_turn(chat, cx);
            }
            Event::Exited { code, detail } => {
                self.sessions.remove(&chat);
                while self.side.get(&chat).is_some_and(|s| !s.pending.is_empty()) {
                    self.answer_side(chat, "KodAct stopped before it answered.".into(), true);
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
                        (_, Some(c)) => format!("KodAct stopped (exit code {c})."),
                        (_, None) => "KodAct stopped.".to_string(),
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

    /// Open a web page KodAct wrote in the side panel's browser.
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

    /// A document KodAct wrote, read back for its card: Markdown, a diagram,
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

    /// Where `path` (as a Write names it) is on disk for `chat`: itself
    /// when absolute, else under the chat's project.
    fn chat_path(&self, chat: usize, path: &str) -> String {
        let p = std::path::Path::new(path);
        if p.is_absolute() {
            return path.to_string();
        }
        let project = self
            .chats
            .iter()
            .find(|c| c.id == chat)
            .and_then(|c| self.project(c.project))
            .map(|p| p.path.clone())
            .unwrap_or_else(|| self.project_dir.clone());
        project.join(p).display().to_string()
    }

    /// Documents a command may have made: the `.docx`, `.xlsx`, `.pptx` and
    /// `.pdf` paths in its command line and what it printed, and the `.html` and
    /// `.svg` ones (a diagram copied from its template; at most eight).
    pub fn document_paths(command: &str, output: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for word in command.split_whitespace().chain(output.split_whitespace()) {
            let w = word
                .trim_matches(|c: char| {
                    matches!(c, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | ',' | ';')
                })
                .trim_end_matches(['.', ':']);
            let w = w.rsplit_once('=').map_or(w, |(_, v)| v);
            let lower = w.to_ascii_lowercase();
            let doc = [".docx", ".xlsx", ".pptx", ".pdf", ".html", ".htm", ".svg"]
                .iter()
                .any(|e| lower.ends_with(e));
            if doc && w.len() > 5 && !out.iter().any(|o| o == w) {
                out.push(w.to_string());
            }
            if out.len() == 8 {
                break;
            }
        }
        out
    }

    /// A document KodAct wrote on a remote, read back over the connection
    /// (off this thread); its card joins the reply when it arrives.
    fn remote_written_file(
        &mut self,
        chat: usize,
        remote: crate::remote::Remote,
        path: String,
        edit: bool,
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
                    s.place_card(chat, file, edit, cx);
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
            panel_share: self.panel_share,
            dark: self.dark,
            theme: self.look.id().into(),
            bypass_allowed: self.bypass_allowed,
        });
    }

    /// Turn a nudge or the reasoning carry-over on or off (Settings >
    /// General): written to settings.json at once, and each chat's KodAct CLI
    /// restarts on its next message to read it, resuming its conversation.
    pub fn set_behaviour(&mut self, key: &str, on: bool, cx: &mut Context<Self>) {
        if let Err(e) = crate::behaviour::save(key, on) {
            return self.show_toast(format!("Couldn't save the setting: {e}"), cx);
        }
        self.behaviour = crate::behaviour::load();
        self.behaviour_version += 1;
        self.show_toast("Saved. Every chat uses it from its next message.", cx);
        cx.notify();
    }

    /// Open KodAct's settings.json in your editor, writing one with every key
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
                None => s.show_toast("Couldn't write settings.json in KodAct's config folder.", cx),
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

    /// KodAct CLI is on this target and answered: chats there start at once.
    pub fn remote_connected(&self, target: &crate::remote::Target) -> bool {
        self.remote_ready.contains_key(target)
    }

    pub(crate) fn remote_extra(&self, target: &crate::remote::Target) -> crate::remote::Extra {
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

    /// Put this app's KodAct CLI on a target (off this thread; what it's doing
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

    /// Record that the remote menu closed on mouse-down outside at `at`.
    pub fn dismiss_remote_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if self
            .remote_menu
            .as_ref()
            .is_some_and(|m| m.host_key.is_none())
        {
            self.remote_dismissed_at = Some(at);
            self.remote_menu = None;
            cx.notify();
        }
    }

    /// Click-open / click-close handler for the remote indicator button.
    pub fn toggle_remote_menu_at(
        &mut self,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let dismissed = self.remote_dismissed_at.take();
        if let (Some(d), Some(p)) = (dismissed, at) {
            if d == p {
                cx.notify();
                return;
            }
        }
        self.toggle_remote_menu(cx);
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
    /// KodAct CLI is put there in the background.
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
                && crate::remote::Remote::parse(&p.path).is_some_and(|r| r.target == remote.target)
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
            None => self
                .project_dir
                .is_dir()
                .then(|| DevSource::Local(self.project_dir.clone())),
            Some(r) => match r.target {
                Target::Wsl(distro) => Some(DevSource::Wsl {
                    distro,
                    path: r.path,
                }),
                _ => None,
            },
        }
    }

    /// In a WSL folder: find out, off the UI thread, whether it has a dev
    /// container definition, for the menu's Reopen in container.
    fn check_devcontainer(&mut self, cx: &mut Context<Self>) {
        let Some(crate::remote::DevSource::Wsl { distro, path }) = self.devcontainer_folder()
        else {
            return;
        };
        let key = self.project_dir.clone();
        if self.wsl_devcontainers.contains_key(&key) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let has = cx
                .background_spawn(
                    async move { crate::remote::wsl_has_devcontainer(&distro, &path) },
                )
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

    /// The current remote is a dev container KodAct started from a folder.
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
        } else if let Some(src) = self
            .devcontainer_folder()
            .filter(|_| self.has_devcontainer())
        {
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
    fn start_devcontainer(
        &mut self,
        src: crate::remote::DevSource,
        fresh: bool,
        cx: &mut Context<Self>,
    ) {
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
                dir.join(".devcontainer")
                    .join("devcontainer.json")
                    .is_file()
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

    /// Back to this machine, where KodAct started: the folder it was
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
                                // Ready again (KodAct CLI is still there: no upload).
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
    /// The design tools' HTTP server for a remote project's chats, started
    /// once per project: (port, token).
    fn ensure_design_http(
        &mut self,
        remote: &crate::remote::Remote,
        cdp: &str,
    ) -> Option<(u16, String)> {
        // Per browser too: one started before the browser was, or on a
        // browser since closed, would render nothing.
        let key = format!("{}\u{0}{}\u{0}{cdp}", remote.target.label(), remote.path);
        if let Some(h) = self.design_http.get(&key) {
            return Some(h.clone());
        }
        let token = crate::history::new_id().replace('-', "");
        let port = crate::design_mcp::serve_http(
            cdp,
            &crate::browser::focus_file(),
            &token,
            remote.clone(),
        )
        .ok()?;
        self.design_http.insert(key, (port, token.clone()));
        Some((port, token))
    }

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
        self.time_thinking(chat);
        Some((m, b))
    }

    /// When each thought of the newest reply began and ended (it ends when
    /// anything follows it), for its "Thought for 12s".
    fn time_thinking(&mut self, chat: usize) {
        let Some(c) = self.chats.iter().find(|c| c.id == chat) else {
            return;
        };
        let m = c.messages.len().saturating_sub(1);
        let Some(Message::Assistant { blocks }) = c.messages.get(m) else {
            return;
        };
        let now = std::time::Instant::now();
        for (b, blk) in blocks.iter().enumerate() {
            if !matches!(blk, Block::Thinking { .. }) {
                continue;
            }
            let span = self.thought.entry((chat, m, b)).or_insert((now, None));
            if span.1.is_none() && b + 1 < blocks.len() {
                span.1 = Some(now);
            }
        }
    }

    /// How long thought `block` of message `msg` took, once it is over.
    pub fn thought_secs(&self, chat: usize, msg: usize, block: usize) -> Option<u64> {
        let (start, end) = self.thought.get(&(chat, msg, block))?;
        Some(end.as_ref()?.duration_since(*start).as_secs())
    }

    fn tool_at(&self, chat: usize, msg: usize, block: usize) -> Option<&ToolCall> {
        let c = self.chats.iter().find(|c| c.id == chat)?;
        match c.messages.get(msg)? {
            Message::Assistant { blocks } => match blocks.get(block)? {
                Block::Tool(t) => Some(t),
                _ => None,
            },
            _ => None,
        }
    }

    /// A tool call that names a document (`pptx_tool.py init deck.pptx`, a
    /// Write of `report.pdf`): watch it for the rest of the turn. Its card
    /// joins the chat as soon as the file exists, and it redraws each time
    /// the file settles, whatever command changed it.
    fn watch_documents(
        &mut self,
        chat: usize,
        name: &str,
        input: &serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if self.chat_remote(chat).is_some() {
            return; // A remote's files come back over the connection (`documents_made`).
        }
        let named = match name {
            "Bash" | "PowerShell" => input["command"].as_str().unwrap_or_default().to_string(),
            "Write" | "Edit" | "MultiEdit" => {
                input["file_path"].as_str().unwrap_or_default().to_string()
            }
            _ => return,
        };
        let mut added = false;
        for p in crate::live_docs::named(&named) {
            let full = std::path::PathBuf::from(self.chat_path(chat, &p));
            added |= self.live.entry(chat).or_default().add(full);
        }
        if added {
            self.ensure_live_loop(cx);
        }
    }

    /// The poll that drives every chat's watch, while any chat has one.
    fn ensure_live_loop(&mut self, cx: &mut Context<Self>) {
        if self.live_task.is_some() {
            return;
        }
        self.live_task = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(crate::live_docs::EVERY)
                .await;
            let go_on = this
                .update(cx, |s, cx| {
                    let chats: Vec<usize> = s.live.keys().copied().collect();
                    for chat in chats {
                        s.poll_live(chat, false, cx);
                    }
                    if s.live.values().all(|w| w.is_empty()) {
                        s.live_task = None;
                        return false;
                    }
                    true
                })
                .unwrap_or(false);
            if !go_on {
                break;
            }
        }));
    }

    /// Show the watched documents of `chat` that changed and settled; with
    /// `last`, also the ones that changed since the last poll at all.
    fn poll_live(&mut self, chat: usize, last: bool, cx: &mut Context<Self>) {
        let Some(w) = self.live.get_mut(&chat) else {
            return;
        };
        let mut ready = w.poll(crate::live_docs::Stamp::of);
        if last {
            // The turn is over: nothing more is being written.
            ready.extend(w.poll(crate::live_docs::Stamp::of));
        }
        for path in ready {
            if let Some(file) = self.written_file(chat, &path.to_string_lossy()) {
                self.place_card(chat, file, false, cx);
            }
        }
        if !self.live.get(&chat).is_none_or(|w| w.is_empty()) {
            cx.notify();
        }
    }

    /// The paths `chat` is making now, for the "Writing…" on their cards.
    pub fn writing(&self, chat: usize) -> Vec<String> {
        if !self.turn_started.contains_key(&chat) {
            return vec![];
        }
        self.live
            .get(&chat)
            .map(|w| w.paths().map(|p| p.display().to_string()).collect())
            .unwrap_or_default()
    }

    /// The documents among `paths` that changed since `since` (a command
    /// made or rewrote them), each read and shown as a card in the reply:
    /// from disk here, over the connection on a remote, so a document made
    /// in WSL or a container previews and downloads on this computer too.
    fn documents_made(
        &mut self,
        chat: usize,
        paths: Vec<String>,
        since: SystemTime,
        cx: &mut Context<Self>,
    ) {
        // A little slack: file times and this clock can differ by a beat.
        let since = since - std::time::Duration::from_secs(2);
        if let Some(remote) = self.chat_remote(chat) {
            let extra = self.remote_extra(&remote.target);
            let secs = since
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let task = cx.background_spawn(async move {
                crate::remote::changed_since(&remote.target, &remote.path, &paths, secs, &extra)
            });
            cx.spawn(async move |this, cx| {
                let fresh = task.await;
                let _ = this.update(cx, |s, cx| {
                    for full in fresh {
                        if let Some(r) = s.chat_remote(chat) {
                            s.remote_written_file(chat, r, full, false, cx);
                        }
                    }
                });
            })
            .detach();
            return;
        }
        for p in paths {
            let Some(file) = self.written_file(chat, &p) else {
                continue;
            };
            let Some(path) = file.path.as_ref().map(|p| p.to_string()) else {
                continue;
            };
            let fresh = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .is_ok_and(|t| t >= since);
            if fresh {
                self.place_card(chat, file, false, cx);
            }
        }
    }

    /// A file KodAct made or changed, as a card in the reply. A file the
    /// reply already shows has that card brought up to date, so it ends on
    /// the final version. An edit adds a card only for a diagram (Diagram
    /// Design copies its template, then edits it); editing a README doesn't.
    fn place_card(&mut self, chat: usize, file: FileRef, edit: bool, cx: &mut Context<Self>) {
        if let Some(path) = file
            .path
            .as_ref()
            .map(|p| std::path::PathBuf::from(p.as_ref()))
        {
            if let Some(w) = self.live.get_mut(&chat) {
                w.shown(&path, crate::live_docs::Stamp::of(&path));
            }
        }
        // The turn's first Word, Excel, PowerPoint or PDF file opens beside
        // the chat, so the person watches it being made; one they closed stays
        // closed. (Markdown and pages still redraw live; a note the model jots
        // doesn't take the panel.)
        if file.office().is_some()
            && self.current == Some(chat)
            && self.doc.is_none()
            && self.turn_started.contains_key(&chat)
            && self.live_opened.insert(chat)
        {
            self.open_doc(file.clone(), cx);
        }
        self.follow_doc(chat, &file, cx);
        let Some(c) = self.chat_mut(chat) else { return };
        if let Some(file) = c.update_card(file, edit) {
            self.push_block(chat, Block::File(file));
        }
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
        // A last look, so the cards end on the final version, then stop watching.
        self.poll_live(chat, true, cx);
        self.live.remove(&chat);
        self.live_opened.remove(&chat);
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
        // Your Langfuse, when set up: the reply as a trace.
        if crate::design::trace::target().is_some() {
            if let Some(turn) = self.trace_turn(chat) {
                cx.background_spawn(async move {
                    let _ = crate::design::trace::send(&turn);
                })
                .detach();
            }
        }
        self.send_design_held(chat, cx);
        cx.notify();
    }

    /// Comments on the design that waited for the chat's reply go once it
    /// is over, as one message.
    fn send_design_held(&mut self, chat: usize, cx: &mut Context<Self>) {
        let Some(held) = self.design_held.remove(&chat).filter(|h| !h.is_empty()) else {
            return;
        };
        for c in self.design_comments.iter_mut().filter(|c| c.chat == chat) {
            if c.sent == CommentSent::Waiting {
                c.sent = CommentSent::Sent;
            }
        }
        let (texts, attachments): (Vec<String>, Vec<Vec<crate::attach::Attachment>>) =
            held.into_iter().unzip();
        self.send_in(chat, texts.join("\n\n---\n\n"), attachments.concat(), cx);
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
                    Ok(()) => {
                        s.show_toast("Undone: the memory is as it was before that reply.", cx)
                    }
                    Err(e) => s.show_toast(format!("Couldn't undo it: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// KodAct keeps a memory for project `id`, or doesn't.
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

    // ------------------------------------------------------------ file cards

    /// Build the strip of each of `files` not built yet, off the UI thread
    /// (`strip::build` reads, and renders a document's first pages).
    pub fn want_strips(&mut self, files: Vec<FileRef>, cx: &mut Context<Self>) {
        for file in files {
            let key = crate::strip::key(&file);
            if self.card_strips.contains_key(&key) {
                continue;
            }
            std::sync::Arc::make_mut(&mut self.card_strips).insert(key, None);
            let task = cx.background_spawn(async move { crate::strip::build(&file) });
            cx.spawn(async move |this, cx| {
                let strip = task.await.map(std::sync::Arc::new);
                let _ = this.update(cx, |s, cx| {
                    std::sync::Arc::make_mut(&mut s.card_strips).insert(key, strip);
                    std::sync::Arc::make_mut(&mut s.card_drawn).insert(key);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Open a file beside the chat at one of its parts, a strip's frame: a
    /// sheet at once, a page or slide when its pages are drawn. Already open,
    /// it stays open and moves there.
    pub fn open_doc_at(&mut self, file: FileRef, at: usize, cx: &mut Context<Self>) {
        let open = self
            .doc
            .as_ref()
            .is_some_and(|d| crate::doc_versions::same_file(d, &file));
        if !open {
            self.show_doc(file.clone(), cx);
        }
        match file.office() {
            Some(crate::office::Kind::Xlsx) => self.doc_sheet = at,
            Some(_) => {
                self.doc_jump = Some(at);
                self.apply_doc_jump(cx);
            }
            None => {}
        }
        cx.notify();
    }

    /// Bring up the page a strip's frame asked for, once the panel has pages.
    fn apply_doc_jump(&mut self, cx: &mut Context<Self>) {
        if let Some(at) = self.doc_jump {
            if self.doc_page_count() > at {
                self.doc_jump = None;
                self.show_doc_page(at, cx);
            }
        }
    }

    /// Draw the first page of each of `files` not drawn yet, off the UI
    /// thread, for its card in the reply: a picture is itself; a PDF, Word,
    /// PowerPoint or Excel file its first page (`office::first_page`).
    pub fn want_thumbs(&mut self, files: Vec<FileRef>, cx: &mut Context<Self>) {
        for file in files {
            let Some(bytes) = file.bytes.clone() else {
                continue;
            };
            let key = crate::office_render::key(&bytes);
            if self.card_thumbs.contains_key(&key) {
                continue;
            }
            std::sync::Arc::make_mut(&mut self.card_thumbs).insert(key, None);
            let name = file.name.to_string();
            cx.spawn(async move |this, cx| {
                let (thumb, pages) = cx
                    .background_spawn(async move {
                        if let Some(format) = crate::ui::image_view::format_of(&name) {
                            return (
                                Some(std::sync::Arc::new(gpui::Image::from_bytes(
                                    format,
                                    bytes.to_vec(),
                                ))),
                                None,
                            );
                        }
                        match crate::office::first_page_counted(&name, &bytes) {
                            Ok((p, n)) => (Some(p.image), Some(n).filter(|n| *n > 0)),
                            Err(_) => (None, None),
                        }
                    })
                    .await;
                let _ = this.update(cx, |s, cx| {
                    if let Some(n) = pages {
                        std::sync::Arc::make_mut(&mut s.card_pages).insert(key, n);
                    }
                    if thumb.is_some() {
                        std::sync::Arc::make_mut(&mut s.card_thumbs).insert(key, thumb);
                    }
                    std::sync::Arc::make_mut(&mut s.card_drawn).insert(key);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    // ------------------------------------------------------------ side questions

    /// The side card of the chat showing, if it is open.
    pub fn side_shown(&self) -> Option<&Side> {
        // A chat's own: on its screen only, not over Projects or Design.
        self.current
            .filter(|_| self.screen == Screen::Chat)
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
                self.answer_side(id, format!("(Demo) A side answer to \"{question}\": KodAct CLI answers these from the conversation, without tools."), false);
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
        let live = self.sessions.get(&id).ok_or("KodAct isn't running.")?;
        live.session.send(&format!("/btw {question}"))?;
        self.immediate_queue
            .entry(id)
            .or_default()
            .push_back(ImmediateSink::Side);
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
                "KodAct didn't return an answer for this side question.".to_string()
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

    /// Grow the side card, or bring it back to its size; a folded card opens.
    pub fn toggle_side_expand(&mut self, cx: &mut Context<Self>) {
        if let Some(side) = self.current.and_then(|id| self.side.get_mut(&id)) {
            side.expanded = !side.expanded || side.minimized;
            side.minimized = false;
            self.side_focus = true;
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
            if crate::design::live::is_page(path) {
                self.design_draft(chat, path, content, cx);
            } else {
                self.text_draft(chat, path, content, cx);
            }
            return;
        }
        if let Step::Wrote(path, content) = &step {
            let made = std::path::Path::new(path)
                .parent()
                .map(std::fs::create_dir_all)
                .is_some_and(|r| r.is_ok())
                && std::fs::write(path, content).is_ok();
            if let Some(Message::Assistant { blocks }) =
                self.chat_mut(chat).and_then(|c| c.messages.last_mut())
            {
                if let Some(Block::Tool(t)) = blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, Block::Tool(_)))
                {
                    t.status = if made {
                        ToolStatus::Done
                    } else {
                        ToolStatus::Failed
                    };
                }
            }
            self.design_done(chat, path, cx);
            self.end_draft(chat, path, cx);
            // The card dropped when the writing started takes the file.
            if let Some(file) = self.written_file(chat, path) {
                if let Some(file) = self.chat_mut(chat).and_then(|c| c.update_card(file, false)) {
                    self.push_block(chat, Block::File(file));
                }
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
                crate::backend::Event::ToolResult {
                    id,
                    output,
                    is_error,
                    ..
                } => {
                    // A run you stopped already shows so.
                    let agent_pos = self
                        .agents
                        .get(&chat)
                        .and_then(|runs| runs.iter().position(|r| &r.id == id));
                    let stopped = agent_pos.is_some_and(|i| self.agents[&chat][i].stopped);
                    if stopped {
                    } else if let Some(Message::Assistant { blocks }) =
                        self.chat_mut(chat).and_then(|c| c.messages.last_mut())
                    {
                        let idx = agent_pos
                            .and_then(|n| {
                                blocks
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, b)| matches!(b, Block::Tool(t) if is_agent_tool(&t.target)))
                                    .nth(n)
                                    .map(|(i, _)| i)
                            })
                            .or_else(|| {
                                blocks.iter().position(|b| {
                                    matches!(b, Block::Tool(t) if is_agent_tool(&t.target) && t.status == ToolStatus::Running)
                                })
                            });
                        if let Some(Block::Tool(t)) = idx.and_then(|i| blocks.get_mut(i)) {
                            t.status = if *is_error {
                                ToolStatus::Failed
                            } else {
                                ToolStatus::Done
                            };
                            if !output.trim().is_empty() {
                                t.output = output.clone().into();
                            }
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
                    open: false,
                    full: false,
                    review: Review::Open,
                    images: vec![],
                }))
            }
            (Step::ToolDiff(diff), Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, Block::Tool(_)))
                {
                    t.diff = diff;
                }
            }
            (Step::ToolImages(images), Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, Block::Tool(_)))
                {
                    t.images = images;
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
            (Step::Failed(output), Some(blocks)) => {
                if let Some(Block::Tool(t)) = blocks
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, Block::Tool(_)))
                {
                    t.status = ToolStatus::Failed;
                    t.output = output.into();
                }
            }
            (Step::Question(input), Some(blocks)) => {
                let mut t = crate::backend::tool_call(crate::backend::QUESTION_TOOL, &input);
                t.status = ToolStatus::Done;
                blocks.push(Block::Tool(t));
                let row = blocks.len() - 1;
                let (title, detail) =
                    crate::backend::ask_text(crate::backend::QUESTION_TOOL, &input);
                blocks.push(Block::Permission {
                    title: title.into(),
                    detail: detail.into(),
                    verdict: Verdict::Pending,
                    info: PermInfo {
                        kind: Some(ToolKind::Other),
                        row: Some(row),
                        every_time: true,
                        questions: crate::backend::questions(&input),
                        ..Default::default()
                    },
                });
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
                self.context_usage
                    .insert(chat, crate::context::ContextUsage::demo(usage.total()));
                self.touch(chat, cx);
                self.check_memory(chat, cx);
            }
            (Step::Remember(fact), _) => {
                // As KodAct would: the fact written into the project's memory.
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
        self.time_thinking(chat);
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
    /// A tool call as KodAct CLI sends it (name and input), done with its output.
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
    /// The pictures the last call returned.
    ToolImages(Vec<ToolImage>),
    /// The running call failed, with this error.
    Failed(String),
    /// AskUserQuestion with this input: its card waits on your answers.
    Question(serde_json::Value),
}

impl Step {
    fn delay(&self) -> u64 {
        match self {
            Step::Think(_) | Step::Say(_) => 45,
            Step::ToolStart(..) => 350,
            Step::ToolDiff(_) | Step::ToolImages(_) => 0,
            Step::ToolDone(..) => 900,
            Step::Ask(..) | Step::Question(_) => 300,
            Step::Failed(_) => 600,
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

/// A reply that changes nine files in one run, so it ends on the "Edited 9
/// files" card: each file with its lines added and removed.
fn demo_refactor() -> Vec<Step> {
    let mut s = words(
        "I'll move the sync settings into their own module and wire the installer to it.",
        Step::Say,
    );
    let files: [(&str, usize, usize); 9] = [
        ("config/sync.toml", 5, 0),
        ("scripts/sync_open_design.py", 12, 0),
        ("installer/kodact.nsi", 3, 0),
        ("installer/build.sh", 4, 1),
        ("installer/README.md", 4, 2),
        ("src/sync.rs", 16, 1),
        ("src/main.rs", 2, 0),
        ("README.md", 4, 1),
        ("docs/endpoints.md", 9, 1),
    ];
    for (path, adds, dels) in files {
        let mut change: Vec<DiffLine> = (0..dels)
            .map(|i| DiffLine {
                kind: DiffKind::Del,
                text: format!("old sync line {}", i + 1).into(),
            })
            .collect();
        change.extend((0..adds).map(|i| DiffLine {
            kind: DiffKind::Add,
            text: format!("sync setting {}", i + 1).into(),
        }));
        s.push(Step::ToolStart(ToolKind::Edit, path.into(), None));
        s.push(Step::ToolDone("The file was updated.".into(), change));
    }
    s.extend(words(
        "Done: the sync settings live in `config/sync.toml`, and the installer reads them.",
        Step::Say,
    ));
    s
}

/// The demo's drawn answer: a flowchart and a sequence, in Mermaid.
const DEMO_DIAGRAM: &str = "Here is how a message travels through KodAct.\n\n```mermaid\nflowchart TD\n  A[You type a message] --> B{Endpoint set up?}\n  B -->|no| C[Settings › Models]\n  C --> D\n  B -->|yes| D[KodAct CLI process]\n  D -.->|stream-json| E([Reply in the chat])\n  D --> F[(Session log)]\n```\n\nAnd the turn itself, step by step:\n\n```mermaid\nsequenceDiagram\n  autonumber\n  participant U as You\n  participant D as KodAct\n  participant C as KodAct CLI\n  U->>D: Send\n  D->>C: user turn\n  loop each tool call\n    C-->>D: tool_use / tool_result\n  end\n  C-->>D: result\n  D-->>U: Reply\n```";

/// The demo's charts: a pie and an xychart, drawn natively.
const DEMO_CHARTS: &str = "Where the time went:\n\n```mermaid\npie title Time per task\n  \"Reading\" : 42\n  \"Writing\" : 30\n  \"Tests\" : 12\n```\n\nAnd per month:\n\n```mermaid\nxychart-beta\n  title \"Replies\"\n  x-axis [jan, feb, mar, apr]\n  y-axis \"Replies\" 0 --> 100\n  bar [20, 45, 60, 80]\n  line [25, 40, 70, 90]\n```\n";

/// The demo's ASCII answer, for checking that box drawing keeps its columns.
const DEMO_ASCII: &str = "Here it is in ASCII.\n\n```text\n┌──────────────┐     ┌──────────────┐\n│  KodAct app   │ ──▶ │   KodAct CLI   │\n└──────┬───────┘     └──────┬───────┘\n       │ stream-json        │ HTTPS\n       ▼                    ▼\n┌──────────────┐     ┌──────────────┐\n│  Chat view   │ ◀── │    Model     │\n└──────────────┘     └──────────────┘\n```\n\nAnd the same without a fence:\n\n+--------+      +---------+\n| Client | ---> | Server  |\n+--------+      +---------+\n     |               |\n     v               v\n  [ UI ]         [ DB ]\n";

/// The demo's notes: a Markdown file written a few lines at a time, so its
/// Watch live view (`AppState::watch_draft`) shows it filling in.
fn demo_notes() -> Vec<Step> {
    const NOTES: &str = include_str!("../assets/docs/settings-migration-plan.md");
    let path = std::env::temp_dir()
        .join("kodact-demo")
        .join("migration-notes.md")
        .display()
        .to_string();
    let mut s = words(
        "I'll write the migration notes as a Markdown file you can keep in the repo.",
        Step::Say,
    );
    s.push(Step::ToolStart(ToolKind::Write, path.clone(), None));
    let lines: Vec<&str> = NOTES.split_inclusive('\n').collect();
    let mut so_far = String::new();
    for chunk in lines.chunks(3) {
        so_far.extend(chunk.iter().copied());
        s.push(Step::Draft(path.clone(), so_far.clone()));
    }
    s.push(Step::Wrote(path, NOTES.to_string()));
    s.extend(words("\n\nThe notes are written.", Step::Say));
    s
}

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
        Step::Asked("Bash", bash("file target/release/kodact", "Check the binary"), "", Verdict::Allowed, Decider::You, false),
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

/// A command that takes screenshots, then the two pictures read back: the
/// run folds to one line with their thumbnails at its end.
fn demo_screenshots() -> Vec<Step> {
    let shot = |name: &str, bytes: &'static [u8]| ToolImage {
        name: name.to_string().into(),
        path: None,
        image: std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Jpeg,
            bytes.to_vec(),
        )),
    };
    let mut s = words("I'll take both themes and look at them.", Step::Say);
    s.extend([
        Step::ToolStart(
            ToolKind::Bash,
            "scripts/screenshots.sh home".into(),
            Some("Capture the home screen in light and dark".into()),
        ),
        Step::ToolDone(
            "wrote shots/home-light.jpg\nwrote shots/home-dark.jpg".into(),
            vec![],
        ),
        Step::ToolStart(ToolKind::Read, "shots/home-light.jpg".into(), None),
        Step::ToolDone(String::new(), vec![]),
        Step::ToolImages(vec![shot(
            "home-light.jpg",
            include_bytes!("../assets/docs/samples/forge-light.jpg"),
        )]),
        Step::ToolStart(ToolKind::Read, "shots/home-dark.jpg".into(), None),
        Step::ToolDone(String::new(), vec![]),
        Step::ToolImages(vec![shot(
            "home-dark.jpg",
            include_bytes!("../assets/docs/samples/forge-dark.jpg"),
        )]),
    ]);
    s.extend(words(
        "Both read well. In dark, the Deny button now sits back as it should; in light, the sidebar's filter no longer looks switched on.",
        Step::Say,
    ));
    s
}

/// A deck checked as the pptx skill checks one: its slides rendered, then
/// looked at one by one (they fold into one row with their thumbnails), and
/// the deck's card, with its first slide and slide count.
fn demo_deck_check() -> Vec<Step> {
    let slide = |n: usize, bytes: &'static [u8]| ToolImage {
        name: format!("slide-{n}.jpg").into(),
        path: None,
        image: std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Jpeg,
            bytes.to_vec(),
        )),
    };
    let light: &'static [u8] = include_bytes!("../assets/docs/samples/forge-light.jpg");
    let dark: &'static [u8] = include_bytes!("../assets/docs/samples/forge-dark.jpg");
    let mut s = words("I'll render the deck and look at each slide.", Step::Say);
    s.extend([
        Step::ToolStart(
            ToolKind::Bash,
            "python scripts/render_preview.py finance/q3-budget.pptx --out-dir preview --grid"
                .into(),
            Some("Render the slides".into()),
        ),
        Step::ToolDone("Rendered 4 slide(s)".into(), vec![]),
    ]);
    for n in 1..=4 {
        s.extend([
            Step::ToolStart(ToolKind::Read, format!("preview/slide-{n}.png"), None),
            Step::ToolDone(String::new(), vec![]),
            Step::ToolImages(vec![slide(n, if n % 2 == 1 { light } else { dark })]),
        ]);
    }
    s.extend(words(
        "Every slide reads well: nothing overflows, and the numbers match the budget.",
        Step::Say,
    ));
    s.push(Step::File(FileRef {
        name: "q3-budget.pptx".into(),
        path: Some("finance/q3-budget.pptx".into()),
        content: "".into(),
        bytes: Some(
            include_bytes!("../assets/docs/samples/q3-budget.pptx")
                .as_slice()
                .into(),
        ),
    }));
    s
}

/// One file of every kind, each with its card: a Word memo, a one-page PDF,
/// a workbook, a Markdown guide, a Python script and a deck of one slide.
fn demo_every_kind() -> Vec<Step> {
    let doc = |name: &str, path: &str, bytes: &'static [u8]| {
        Step::File(FileRef {
            name: name.to_string().into(),
            path: Some(path.to_string().into()),
            content: "".into(),
            bytes: Some(bytes.into()),
        })
    };
    let text = |name: &str, content: &str| {
        Step::File(FileRef {
            name: name.to_string().into(),
            path: Some(name.to_string().into()),
            content: content.to_string().into(),
            bytes: None,
        })
    };
    let mut s = words("Here is the budget pack, in every form you asked for.", Step::Say);
    s.push(doc(
        "q3-budget-memo.docx",
        "finance/q3-budget-memo.docx",
        include_bytes!("../assets/docs/samples/q3-budget-memo.docx"),
    ));
    s.push(doc(
        "q3-budget.pdf",
        "finance/q3-budget.pdf",
        include_bytes!("../assets/docs/samples/q3-budget.pdf"),
    ));
    s.push(doc(
        "q3-budget.xlsx",
        "finance/q3-budget.xlsx",
        include_bytes!("../assets/docs/samples/q3-budget.xlsx"),
    ));
    s.extend(words("And the notes, the script that builds the numbers, and the title slide:", Step::Say));
    s.push(text(
        "BUDGET.md",
        "# Q3 budget\n\nHow the quarter's budget is built and checked.\n\n## Sources\n\nActuals come from the ledger export; forecasts from each team's plan.\n\n## Method\n\nEach line is the plan, less what was spent, rounded to the nearest hundred.\n\n## Checks\n\nTotals must match the ledger to the cent, and no team may exceed its cap.\n\n## Open questions\n\nWhether hiring moves to Q4, and how travel is split.\n",
    ));
    s.push(text(
        "build_budget.py",
        "import csv\n\n\ndef load(path):\n    with open(path) as f:\n        return list(csv.DictReader(f))\n\n\ndef remaining(plan, spent):\n    return round(plan - spent, -2)\n\n\nclass Report:\n    def __init__(self, rows):\n        self.rows = rows\n\n    def total(self):\n        return sum(float(r[\"plan\"]) for r in self.rows)\n",
    ));
    s.push(doc(
        "q3-title.pptx",
        "finance/q3-title.pptx",
        include_bytes!("../assets/docs/samples/q3-title.pptx"),
    ));
    s
}

/// Questions first (a card with two), then a burst of work: a write that
/// fails as KodAct CLI's does on a file not read yet, then edits and commands
/// with thinking between them, as a real build-out goes.
fn demo_questions() -> Vec<Step> {
    use serde_json::json;
    let mut s = words(
        "Before I scaffold anything, two choices change the whole layout:",
        Step::Say,
    );
    s.push(Step::Question(json!({ "questions": [
        { "question": "Which stack should the app use?", "header": "Stack", "multiSelect": false, "options": [
            { "label": "Tauri v2 hybrid (Recommended)", "description": "React and TypeScript for the UI, a Rust core for files and search." },
            { "label": "Leptos", "description": "All Rust, UI included; a smaller ecosystem." },
            { "label": "Electron", "description": "The widest ecosystem; a much larger install." } ] },
        { "question": "Where should sheets be stored?", "header": "Storage", "multiSelect": false, "options": [
            { "label": "JSON files", "description": "Easy to back up and to read by hand." },
            { "label": "SQLite", "description": "One file, fast queries, harder to diff." } ] } ] })));
    s.extend(words(
        "Going with your answers. The Rust core first, then the frontend on top.",
        Step::Say,
    ));
    s.push(Step::ToolStart(
        ToolKind::Write,
        "folio/Cargo.toml".into(),
        None,
    ));
    s.push(Step::ToolDiff(diff(&[
        (DiffKind::Add, "[package]"),
        (DiffKind::Add, "name = \"folio-core\""),
        (DiffKind::Add, "version = \"0.1.0\""),
        (DiffKind::Add, "edition = \"2021\""),
    ])));
    s.push(Step::Failed(
        "File has not been read yet. Read it first before writing to it.".into(),
    ));
    s.extend(words(
        "cargo init already made it; read it, then edit.",
        Step::Think,
    ));
    s.push(Step::Use(
        "Read",
        json!({ "file_path": "folio/Cargo.toml" }),
        "[package]\nname = \"folio\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".into(),
    ));
    let edit = |file: &str, old: &str, new: &str| {
        Step::Use(
            "Edit",
            json!({ "file_path": file, "old_string": old, "new_string": new }),
            "The file was updated.".into(),
        )
    };
    s.push(edit(
        "folio/Cargo.toml",
        "name = \"folio\"",
        "name = \"folio-core\"",
    ));
    s.push(edit(
        "folio/src/model.rs",
        "pub struct Sheet;",
        "pub struct Sheet {\n    pub id: Uuid,\n    pub stickers: Vec<Sticker>,\n}",
    ));
    s.push(edit(
        "folio/src/lib.rs",
        "",
        "pub mod model;\npub mod store;\npub mod assets;",
    ));
    s.push(Step::Use(
        "Bash",
        json!({ "command": "cargo build", "description": "Build the Rust core" }),
        "error[E0277]: the trait `From<ZipError>` is not implemented for `Error`".into(),
    ));
    s.push(Step::Failed(
        "error[E0277]: the trait `From<ZipError>` is not implemented for `Error`\nexit 101".into(),
    ));
    s.extend(words("The zip error needs its own variant.", Step::Think));
    for (file, old, new) in [
        (
            "folio/src/lib.rs",
            "Io(std::io::Error),",
            "Io(std::io::Error),\n    Zip(zip::result::ZipError),",
        ),
        ("folio/src/assets.rs", "fn import(", "pub fn import("),
        (
            "folio/src/assets.rs",
            "let img = decode(bytes)?;",
            "let img = decode(bytes).map_err(Error::Image)?;",
        ),
        ("folio/src/store.rs", "fn save(", "pub fn save("),
        (
            "folio/src/store.rs",
            "std::fs::write(path, json)?;",
            "write_atomic(path, json.as_bytes())?;",
        ),
        (
            "folio/src/workspace.rs",
            "sheets: vec![],",
            "sheets: Vec::new(),",
        ),
        ("folio/src/backup.rs", "zip.finish();", "zip.finish()?;"),
    ] {
        s.push(edit(file, old, new));
    }
    s.push(Step::Use(
        "Bash",
        json!({ "command": "cargo build", "description": "Rebuild the Rust core" }),
        "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.12s".into(),
    ));
    s.extend(words(
        "The core builds, and the layout follows your two answers.\n\n**What is in place**\n- `folio-core`: the document model (`Sheet`, `Sticker`, `Stroke`) with stable ids.\n- `store.rs`: each sheet is one JSON file, written to a temporary file and renamed, so a crash never leaves half a sheet.\n- `backup.rs`: a zip of the workspace on demand.\n\nThe first write failed because `cargo init` had already made `Cargo.toml`; I read it and edited it instead. The build then stopped on a missing `From<ZipError>`, which now has its own error variant.\n\nNext I'll put the Tauri shell on top and wire the board's drag and drop to `store::save`.",
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
    let done = |parent: &str, id: &str, out: &str| {
        Step::Agent(Event::SubResult {
            parent: parent.into(),
            id: id.into(),
            output: out.into(),
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
            images: vec![],
        })
    };
    let mut s = words(
        "I'll split the review across three agents: one per area, working at the same time.",
        Step::Say,
    );
    s.extend([
        start("a1", "Explore", "Map the chat pipeline", "Find how a message travels from the composer to KodAct CLI and back. List the files and the functions on the path."),
        start("a2", "code-reviewer", "Review the side panel", "Review src/ui/workbench.rs and src/ui/doc_pane.rs for bugs and dead code. Report findings with file:line."),
        start("a3", "test-runner", "Check the tests", "Run the test suite, and list any test that is slow or flaky."),
        say("a1", "Starting by searching for `fn send` across `src/` to locate the outbound dispatch path."),
        say("a2", "I'll inspect `workbench.rs` and `doc_pane.rs` first to trace how tabs and documents switch."),
        tool("a1", "a1t1", "Grep", json!({ "pattern": "fn send", "path": "src" })),
        tool("a2", "a2t1", "Read", json!({ "file_path": "src/ui/workbench.rs" })),
        tool("a3", "a3t1", "Bash", json!({ "command": "cargo test", "description": "Run the tests" })),
        done("a1", "a1t1", "src/backend.rs:380: pub fn send(&self, text: &str) -> Result<(), String>\nsrc/state.rs:4712: pub fn send(&mut self, text: &str, cx: &mut Context<Self>)"),
        say("a1", "Found `AppState::send` and `Session::send`. Reading `src/state.rs` and `src/backend.rs` to trace the reply stream back to the transcript."),
        tool("a1", "a1t2", "Read", json!({ "file_path": "src/state.rs" })),
        done("a2", "a2t1", "529 lines read from src/ui/workbench.rs"),
        tool("a2", "a2t2", "Read", json!({ "file_path": "src/ui/doc_pane.rs" })),
        done("a1", "a1t2", "AppState::on_event dispatches Event::Text, Event::ToolUse, and Event::TurnEnd into Chat::messages."),
        tool("a1", "a1t3", "Read", json!({ "file_path": "src/backend.rs" })),
        say("a2", "`workbench.rs` looks sound; checking how `doc_pane.rs` leads back to the file list."),
        done("a2", "a2t2", "431 lines read from src/ui/doc_pane.rs"),
        tool("a2", "a2t3", "Grep", json!({ "pattern": "from_panel", "path": "src/ui" })),
        done("a1", "a1t3", "Parser::line parses stream-json events from KodAct CLI stdout."),
        say("a1", "The path: `composer.rs` → `AppState::send` → `backend::Session::send` (`stream-json`) → `Parser::line` → `AppState::on_event` → transcript."),
        end("a1", "The path: composer.rs → AppState::send → backend::Session::send (stream-json) → Parser::line → AppState::on_event → transcript."),
        done("a2", "a2t3", "src/ui/doc_pane.rs:56: pub from_panel: bool\nsrc/ui/doc_pane.rs:92: let from_panel = fit.from_panel;"),
        done("a3", "a3t1", "test result: ok. 235 passed; 0 failed; 1 ignored; finished in 2.41s"),
        say("a3", "All unit tests passed quickly. Timing individual tests now to identify outliers."),
        tool("a3", "a3t2", "Bash", json!({ "command": "cargo test -- --report-time", "description": "Time each test" })),
        end("a2", "No bugs found. One note: the panel's Files list dedupes by name, so two files with the same name in different folders show once."),
        done("a3", "a3t2", "All 235 unit tests < 15ms; web_view::tests::real_browser_roundtrip (~20s, #[ignore])."),
        end("a3", "235 tests pass. The slowest is the real-browser test (about 20 s, ignored by default); none is flaky."),
    ]);
    s.extend(words("All three are done: the chat path is mapped, the side panel review found no bugs (one note on duplicate names), and the 235 tests pass.", Step::Say));
    s
}

/// Six agents at once ("6 agents" in the demo): different kinds, steps
/// landing out of order, one that fails, for the agents' own UI.
fn demo_six_agents() -> Vec<Step> {
    use crate::backend::Event;
    use serde_json::json;
    let start = |id: &str, kind: &str, task: &str| {
        Step::Agent(Event::ToolUse {
            id: id.into(),
            name: "Task".into(),
            input: json!({ "subagent_type": kind, "description": task, "prompt": task }),
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
            output: String::new(),
            is_error: false,
        })
    };
    let end = |id: &str, out: &str, failed: bool| {
        Step::Agent(Event::ToolResult {
            id: id.into(),
            output: out.into(),
            is_error: failed,
            images: vec![],
        })
    };
    let mut s = words(
        "I'll survey the repository with six agents working side by side.",
        Step::Say,
    );
    s.extend([
        start("b1", "Explore", "Find Rust files"),
        start("b2", "Explore", "Count Python files"),
        start("b3", "Explore", "Find README files"),
        start("b4", "general-purpose", "List directories"),
        start("b5", "code-reviewer", "Check sticker HTML"),
        start("b6", "Explore", "Find Cargo projects"),
        tool("b1", "b1a", "Glob", json!({ "pattern": "**/*.rs" })),
        tool("b2", "b2a", "Glob", json!({ "pattern": "**/*.py" })),
        tool("b3", "b3a", "Glob", json!({ "pattern": "**/README*" })),
        tool("b4", "b4a", "Bash", json!({ "command": "ls -la", "description": "List the top folder" })),
        tool("b5", "b5a", "Read", json!({ "file_path": "web/sticker.html" })),
        tool("b6", "b6a", "Glob", json!({ "pattern": "**/Cargo.toml" })),
        done("b1", "b1a"),
        tool("b1", "b1b", "Grep", json!({ "pattern": "fn main", "path": "src" })),
        done("b4", "b4a"),
        tool("b4", "b4b", "Bash", json!({ "command": "ls services", "description": "List the services" })),
        done("b2", "b2a"),
        tool("b2", "b2b", "Bash", json!({ "command": "wc -l $(git ls-files '*.py')", "description": "Count Python lines" })),
        done("b3", "b3a"),
        tool("b3", "b3b", "Read", json!({ "file_path": "docs/README.md" })),
        done("b6", "b6a"),
        tool("b6", "b6b", "Read", json!({ "file_path": "Cargo.toml" })),
        done("b1", "b1b"),
        tool("b1", "b1c", "Read", json!({ "file_path": "src/main.rs" })),
        done("b5", "b5a"),
        tool("b5", "b5b", "Grep", json!({ "pattern": "<script", "path": "web" })),
        done("b2", "b2b"),
        end("b2", "12 Python files, 1,840 lines, all under scripts/.", false),
        done("b4", "b4b"),
        tool("b4", "b4c", "Bash", json!({ "command": "ls -la services/api", "description": "Look inside services/api" })),
        done("b3", "b3b"),
        tool("b3", "b3c", "Read", json!({ "file_path": "README.md" })),
        done("b6", "b6b"),
        tool("b6", "b6c", "Grep", json!({ "pattern": "\\[workspace\\]", "path": "." })),
        done("b5", "b5b"),
        end("b5", "web/sticker.html could not be read: the file is not in this checkout.", true),
        done("b1", "b1c"),
        end("b1", "23 .rs files in 4 crates; the entry point is src/main.rs.", false),
        done("b3", "b3c"),
        end("b3", "Two READMEs: the root one and docs/README.md.", false),
        done("b4", "b4c"),
        end("b4", "Top level: src, docs, scripts, services (api, worker), web.", false),
        done("b6", "b6c"),
        end("b6", "One Cargo workspace with 4 members.", false),
    ]);
    s.extend(words("Five of six agents finished: 23 Rust files in 4 crates, 12 Python files, two READMEs, one Cargo workspace. The sticker HTML isn't in this checkout.", Step::Say));
    s
}

/// Two sub-agents, one of which goes quiet and never reports back ("stall"
/// in the demo): after `agent_roster::STALLED_AFTER` its row shows it.
fn demo_stalled_agent() -> Vec<Step> {
    use crate::backend::Event;
    use serde_json::json;
    let mut s = words("Sending two sub-agents to look around.", Step::Say);
    for (id, task) in [
        ("s1", "Map the settings code"),
        ("s2", "Trace the sync worker"),
    ] {
        s.push(Step::Agent(Event::ToolUse {
            id: id.into(),
            name: "Task".into(),
            input: json!({ "subagent_type": "Explore", "description": task, "prompt": task }),
        }));
    }
    s.push(Step::Agent(Event::SubTool {
        parent: "s2".into(),
        id: "s2a".into(),
        name: "Read".into(),
        input: json!({ "file_path": "src/sync/worker.rs" }),
    }));
    s.push(Step::Agent(Event::ToolResult {
        id: "s1".into(),
        output: "Settings live in src/settings; nothing unusual.".into(),
        is_error: false,
        images: vec![],
    }));
    // s2 never answers.
    s.extend(words(
        "One agent is still reading the sync worker.",
        Step::Say,
    ));
    s
}

/// Ten parallel sub-agents ("10 agents" in the demo) to stress-test the jumper
/// popover, status filters, and scale behavior when >= 10 agents run together.
fn demo_ten_agents() -> Vec<Step> {
    use crate::backend::Event;
    use serde_json::json;
    let start = |id: &str, kind: &str, task: &str| {
        Step::Agent(Event::ToolUse {
            id: id.into(),
            name: "Task".into(),
            input: json!({ "subagent_type": kind, "description": task, "prompt": task }),
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
            output: String::new(),
            is_error: false,
        })
    };
    let end = |id: &str, out: &str, failed: bool| {
        Step::Agent(Event::ToolResult {
            id: id.into(),
            output: out.into(),
            is_error: failed,
            images: vec![],
        })
    };
    let mut s = words(
        "Spinning up ten specialized sub-agents across the repository.",
        Step::Say,
    );
    let specs = [
        ("t1", "Explore", "Audit UI components", "src/ui"),
        ("t2", "Explore", "Check state reducers", "src/state"),
        (
            "t3",
            "code-reviewer",
            "Review the Office parser's XLSX formula checks",
            "src/office.rs",
        ),
        (
            "t4",
            "Explore",
            "Inspect Git integration",
            "src/git_status.rs",
        ),
        ("t5", "Explore", "Verify SVG icons bundle", "assets/icons"),
        (
            "t6",
            "general-purpose",
            "Check theme palette",
            "src/theme.rs",
        ),
        ("t7", "code-reviewer", "Scan IPC backend", "src/backend.rs"),
        ("t8", "Explore", "Check i18n coverage", "src/i18n.rs"),
        (
            "t9",
            "general-purpose",
            "Validate project config",
            "src/projects",
        ),
        ("t10", "Explore", "Profile startup path", "src/main.rs"),
    ];
    for (id, kind, task, _) in specs {
        s.push(start(id, kind, task));
    }
    for (id, _, _, path) in specs {
        let tid = format!("{id}a");
        s.push(tool(id, &tid, "Read", json!({ "file_path": path })));
    }
    for (i, (id, _, task, _)) in specs.iter().enumerate() {
        let tid = format!("{id}a");
        s.push(done(id, &tid));
        if i == 2 {
            s.push(end(
                id,
                "Office XLSX formula validation found a schema warning.",
                true,
            ));
        } else {
            s.push(end(id, &format!("{task} completed cleanly."), false));
        }
    }
    s.extend(words(
        "Nine of ten sub-agents passed; one flagged an Office XLSX formula warning.",
        Step::Say,
    ));
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
            "mcp__kodact-browser__navigate",
            json!({"url": "https://vendor.example/pricing"}),
            "Navigated to: https://vendor.example/pricing".into(),
        ),
        Step::Use(
            "mcp__kodact-browser__state",
            json!({}),
            r#"{"url": "https://vendor.example/pricing#teams", "title": "Pricing"}"#.into(),
        ),
        Step::Use(
            "mcp__kodact-browser__click",
            json!({"index": 7}),
            "Clicked element 7".into(),
        ),
        Step::Use(
            "mcp__kodact-browser__text",
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
            "release-notes.html",
            "docs/release-notes.html",
            include_str!("../assets/docs/release-notes.html"),
            None,
        ),
        file(
            "greeting.rs",
            "src/greeting.rs",
            "//! The greeting on the home screen.\n\n/// \"Good morning, Sam\", by the hour.\npub fn greeting(name: &str, hour: u32) -> String {\n    let part = match hour {\n        5..=11 => \"Good morning\",\n        12..=17 => \"Good afternoon\",\n        _ => \"Good evening\",\n    };\n    format!(\"{part}, {name}\")\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn the_hour_picks_the_words() {\n        assert_eq!(greeting(\"Sam\", 9), \"Good morning, Sam\");\n        assert_eq!(greeting(\"Sam\", 20), \"Good evening, Sam\");\n    }\n}\n",
            None,
        ),
        file(
            "badge.png",
            "docs/badge.png",
            "",
            Some(include_bytes!("../assets/docs/samples/mark-transparent.png")),
        ),
        file(
            "forge-home.jpg",
            "docs/forge-home.jpg",
            "",
            Some(include_bytes!("../assets/docs/samples/forge-light.jpg")),
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
        ("10 agents", "Ten-agent audit"),
        ("ten agents", "Ten-agent audit"),
        ("6 agents", "Repository survey"),
        ("agent", "Chat pipeline review"),
        ("approv", "Repo survey"),
        ("plan", "Settings migration plan"),
        ("chart", "Test times by crate"),
        ("ascii", "Window layout sketch"),
        ("refactor", "Refactor the sync settings"),
    ]
    .into_iter()
    .find(|(key, _)| lower.contains(key))
    .map(|(_, title)| title)
}

/// The project file `name` means, as a reply names it: its path in the
/// project (`/`-separated), an absolute path inside it, or a bare file name
/// found anywhere in it (the shallowest when several share it). `index` is
/// the project's file list when known; without it, only a path that
/// exists on this computer (`local`) is found.
pub fn resolve_named_file(
    name: &str,
    root: &std::path::Path,
    index: Option<&[String]>,
    local: bool,
) -> Option<String> {
    let mut n = name.trim().replace('\\', "/");
    let base = root.to_string_lossy().replace('\\', "/");
    let base = base.trim_end_matches('/');
    if let Some(rest) = n.strip_prefix(base).and_then(|r| r.strip_prefix('/')) {
        n = rest.to_string();
    }
    let n = n.trim_start_matches("./").to_string();
    if n.is_empty() || n.starts_with('/') || n.contains("..") || n.ends_with('/') {
        return None;
    }
    match index {
        Some(files) => {
            let files = files.iter().filter(|f| !f.ends_with('/'));
            let tail = format!("/{n}");
            let mut best: Option<&String> = None;
            for f in files {
                if *f == n {
                    return Some(n);
                }
                if f.ends_with(&tail)
                    && best.is_none_or(|b| f.matches('/').count() < b.matches('/').count())
                {
                    best = Some(f);
                }
            }
            best.cloned()
        }
        None => (local && root.join(&n).is_file()).then_some(n),
    }
}

/// The demo reply that shows every reading style (`script`).
const DEMO_STYLES: &str = "## What changed\n\nThe check in `verify.rs` now counts a batch-mode editor as a check. The skill's `self_check.py` passes, and `cargo test` is green. See [the harness notes](https://example.com/notes) for the details.\n\n> [!IMPORTANT]\n> Two verifier failures were infrastructure, not the agent: `report.py` misread a missing binary.\n\n| Metric | Raw | Adjusted |\n|---|---|---|\n| Pass@1 | 84.3% | 86.2% |\n| Exceptions | 0 | 0 |\n| Output tokens | 2.32M | — |\n\n### Next\n\n- Bound `finish_check_note` in `src/engine.rs` the way `summary_notes` is.\n- Scale the reasoning limit with the time left.\n\n> [!NOTE]\n> The demo replies are scripted; no model is called.";

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
        s.extend(words(
            "Noted. I'll keep that in this project's memory.",
            Step::Say,
        ));
        s.push(Step::Remember(fact));
        s.push(Step::End { seconds: 1.4 });
        return s;
    }
    // The reply's reading styles at once (headings, links, file chips, a
    // callout, a table), to look at them: "show the reading styles".
    if lower.contains("reading styles") {
        s.extend(words(DEMO_STYLES, Step::Say));
        s.push(Step::End { seconds: 2.1 });
        return s;
    }
    // Open Design's discovery form, before a design: a few questions.
    if lower.contains("ask me") {
        s.extend(words(
            "Before I design it, three quick questions.",
            Step::Say,
        ));
        s.push(Step::Question(serde_json::json!({ "questions": [
            { "header": "Audience", "question": "Who is it for?", "multiSelect": false, "options": [
                { "label": "Designers", "description": "People who make interfaces" },
                { "label": "Engineers", "description": "People who build them" },
                { "label": "Leaders", "description": "People who fund the work" } ] },
            { "header": "Feel", "question": "What should it feel like?", "multiSelect": true, "options": [
                { "label": "Editorial", "description": "Serif headlines, generous space" },
                { "label": "Technical", "description": "Mono details, grids" },
                { "label": "Warm", "description": "Soft colours, rounded" },
                { "label": "Bold", "description": "Big type, strong contrast" } ] },
            { "header": "Length", "question": "How many slides?", "multiSelect": false, "options": [
                { "label": "Six", "description": "A short introduction" },
                { "label": "Twelve", "description": "A full session" } ] } ] })));
        s.extend(words(
            "Thanks, that's what I needed. I'll start on it now.",
            Step::Say,
        ));
        s.push(Step::End { seconds: 3.1 });
        return s;
    }
    if lower.contains("notes") || lower.contains("write a doc") {
        s.extend(demo_notes());
        s.push(Step::End { seconds: 12.0 });
        return s;
    }
    if lower.contains("every kind") {
        s.extend(demo_every_kind());
        s.push(Step::End { seconds: 6.2 });
        return s;
    }
    if lower.contains("render") {
        s.extend(demo_deck_check());
        s.push(Step::End { seconds: 9.4 });
        return s;
    }
    if ["design", "deck", "slides", "course"]
        .iter()
        .any(|w| lower.contains(w))
    {
        s.extend(demo_design());
        s.push(Step::End { seconds: 21.4 });
        return s;
    }
    if lower.contains("stall") {
        s.extend(demo_stalled_agent());
        s.push(Step::End { seconds: 6.0 });
        return s;
    }
    if lower.contains("10 agents") || lower.contains("ten agents") {
        s.extend(demo_ten_agents());
        s.push(Step::End { seconds: 18.0 });
        return s;
    }
    if lower.contains("6 agents") || lower.contains("six agents") {
        s.extend(demo_six_agents());
        s.push(Step::End { seconds: 31.4 });
        return s;
    }
    if lower.contains("agent") {
        s.extend(demo_agents());
        s.push(Step::End { seconds: 14.2 });
        return s;
    }
    if lower.contains("question") {
        s.extend(demo_questions());
        s.push(Step::End { seconds: 38.6 });
        return s;
    }
    if lower.contains("summar") {
        s.extend(words(DEMO_SUMMARY, Step::Say));
        s.push(Step::End { seconds: 4.2 });
        return s;
    }
    if lower.contains("screenshot") {
        s.extend(demo_screenshots());
        s.push(Step::End { seconds: 6.1 });
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
    if lower.contains("refactor") {
        s.extend(demo_refactor());
        s.push(Step::End { seconds: 12.7 });
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
    // The change is known when the call starts, as KodAct CLI sends it: the
    // card shows it before you allow it.
    let change = diff(&[
        (DiffKind::Add, "pub fn greeting() -> &'static str {"),
        (DiffKind::Add, "    \"Hello, KodAct\""),
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
        "Allow KodAct to edit main.rs?".into(),
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
        "Done. The greeting now lives in `greeting()` and `main` prints it.\n\n**What changed**\n- `src/main.rs`: a new `greeting()` function, used by `main`.\n- A unit test, `greets_by_name`, checks the text.\n\n```rust\npub fn greeting() -> &'static str {\n    \"Hello, KodAct\"\n}\n```\n\nAll tests pass.",
        Step::Say,
    ));
    s.push(Step::End { seconds: 14.2 });
    s
}

/// A reply in the shape of a written plan: numbered points with bold
/// lead-ins, bullets three deep, and names in inline code, one long enough
/// to wrap.
const DEMO_SUMMARY: &str = "I've written the plan to `/root/.claude/plans/scalable-forging-stearns.md`. Nothing is changed yet.\n\n**Where KodAct loses**\n\n1. **No chat title.** The open chat's title goes in the title bar, and clicking it opens the chat's actions (Rename, Pin, Move, Export, Delete).\n   - The search pill leaves the title bar: `Search` becomes the sidebar's first row.\n2. **The message box looks like a form.** It becomes one line: `+`, the text, then send.\n   - Mode, model and the context ring move under the box as quiet text.\n     - The ring keeps its numbers in the tooltip.\n3. **You can't read the session at a glance.** A Session column shows the place, the branch in `git_status::Repo`, context use and changes.\n\nThe tokens live in `src/theme.rs`; run `UPDATE_SYSTEM_MD=1 cargo test system_md` after changing one.";

// ---------------------------------------------------------------- mock chats

/// The projects: the folder KodAct was started on (if it was), then the
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
    // The folder KodAct started on: a project this run, with its details
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

/// The project's knowledge and memory, as they ride ahead of a first
/// message: a `<system-reminder>`, so KodAct CLI reads them as context and
/// not as your request. Its checks of what a request names (files to make,
/// commands to run) read only your words; as plain text, a file name in the
/// memory (`NN_topic.m`) read as a file you asked for, and a "hi" got a
/// reply about it.
fn context_block(k: &str) -> String {
    format!("<system-reminder>\n{}\n</system-reminder>", k.trim_end())
}

/// Where a chat with no folder works: KodAct's own folder, so a question or
/// a lesson never runs in (or reads) a folder of yours. Files it writes
/// there still come back as cards in the reply.
pub fn scratch_dir() -> std::path::PathBuf {
    crate::profile::dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("workspace")
}

/// A plain chat's folder, under plain chats' place (`root`): its own,
/// `chats/<session>`, for a new chat or one that has it; the shared place
/// for a chat KodAct CLI already wrote there (it files conversations by the
/// folder they ran in, so moving one would lose it).
pub fn chat_folder(root: &std::path::Path, session: &str, resumable: bool) -> std::path::PathBuf {
    if session.is_empty() {
        return root.to_path_buf();
    }
    let own = root.join("chats").join(session);
    if own.is_dir() || !resumable {
        own
    } else {
        root.to_path_buf()
    }
}

const MAX_PRIOR_CONVERSATION_BYTES: usize = 48 * 1024;

/// Messages in `messages` preceding the newest `Message::User` (the turn now
/// going out).
pub fn prior_messages(messages: &[Message]) -> &[Message] {
    match messages
        .iter()
        .rposition(|m| matches!(m, Message::User { .. }))
    {
        Some(at) => &messages[..at],
        None => messages,
    }
}

/// Format earlier turns of a conversation as a `<conversation_history>` block
/// when a fresh KodAct CLI process starts mid-chat (after Edit/Retry or when
/// `--resume` could not reopen the session file).
pub fn prior_conversation_block(messages: &[Message]) -> Option<String> {
    let mut entries: Vec<String> = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        match &messages[i] {
            Message::User {
                text, attachments, ..
            } => {
                let next_user = messages[i + 1..]
                    .iter()
                    .position(|m| matches!(m, Message::User { .. }))
                    .map(|d| i + 1 + d)
                    .unwrap_or(messages.len());
                let between = &messages[i + 1..next_user];
                let has_reply = between.iter().any(|m| match m {
                    Message::Assistant { blocks } => blocks.iter().any(|b| match b {
                        Block::Text(t) => !t.trim().is_empty(),
                        Block::Tool(_) | Block::File(_) => true,
                        _ => false,
                    }),
                    _ => false,
                });
                let trimmed = text.trim();
                // Skip bare slash commands that produced no assistant reply (e.g.
                // an unknown `/command` error or `/reload-plugins`).
                if trimmed.starts_with('/') && !has_reply {
                    i += 1;
                    continue;
                }
                let mut line = String::from("User: ");
                line.push_str(trimmed);
                if !attachments.is_empty() {
                    let names: Vec<&str> = attachments.iter().map(|a| a.name.as_ref()).collect();
                    line.push_str(&format!("\n[Attached: {}]", names.join(", ")));
                }
                if !trimmed.is_empty() || !attachments.is_empty() {
                    entries.push(line);
                }
            }
            Message::Assistant { blocks } => {
                let mut parts: Vec<String> = Vec::new();
                for b in blocks {
                    match b {
                        Block::Text(t) if !t.trim().is_empty() => {
                            parts.push(t.trim().to_string());
                        }
                        Block::Tool(tc) => {
                            let status = match tc.status {
                                ToolStatus::Done => "done",
                                ToolStatus::Failed => "failed",
                                ToolStatus::Declined => "declined",
                                ToolStatus::Running | ToolStatus::Interrupted => "interrupted",
                            };
                            let target: String = tc.target.chars().take(160).collect();
                            parts.push(format!("[Tool {:?}: {} ({status})]", tc.kind, target));
                        }
                        Block::File(f) => {
                            parts.push(format!("[File: {}]", f.name));
                        }
                        _ => {}
                    }
                }
                if !parts.is_empty() {
                    entries.push(format!("Assistant:\n{}", parts.join("\n")));
                }
            }
            Message::TurnEnd { .. } | Message::Notice { .. } => {}
        }
        i += 1;
    }
    if entries.is_empty() {
        return None;
    }
    let mut total: usize = entries.iter().map(|e| e.len() + 2).sum();
    let mut dropped = false;
    while entries.len() > 1 && total > MAX_PRIOR_CONVERSATION_BYTES {
        let removed = entries.remove(0);
        total = total.saturating_sub(removed.len() + 2);
        dropped = true;
    }
    let mut body = entries.join("\n\n");
    if body.len() > MAX_PRIOR_CONVERSATION_BYTES {
        let cut = body.len() - MAX_PRIOR_CONVERSATION_BYTES;
        let cut = (cut..body.len())
            .find(|&ix| body.is_char_boundary(ix))
            .unwrap_or(0);
        body = body[cut..].to_string();
        dropped = true;
    }
    if dropped {
        body = format!("[…earlier messages omitted…]\n\n{body}");
    }
    Some(format!(
        "<conversation_history>\nEarlier messages in this conversation (treat this as the active conversation context):\n\n{body}\n</conversation_history>"
    ))
}

/// The folder KodAct was started on: `KODACT_PROJECT_DIR`, or a folder given
/// as its argument (`kodact .`). Opened from the Start menu or a shortcut, it
/// was started on none, and opens on plain chats, as a chat app does.
fn launch_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("KODACT_PROJECT_DIR")
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

/// A remote KodAct CLI's model settings with the secrets taken out: the route
/// the proxy keeps here (the real base URL, the key as the wire sends it,
/// custom headers), and the environment the remote gets instead (the key a
/// placeholder, no custom headers; the base URL is replaced by the caller).
fn proxied_env(env: &[(String, String)]) -> (crate::model_proxy::Route, Vec<(String, String)>) {
    let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let mut headers = vec![];
    let base = if let Some(b) = get("KODACT_BASE_URL") {
        if let Some(t) = get("KODACT_API_KEY") {
            headers.push(("x-api-key".to_string(), t.clone()));
            headers.push(("authorization".to_string(), format!("Bearer {t}")));
        }
        b
    } else if let Some(b) = get("KODACT_GEMINI_BASE_URL") {
        // As the real host takes it (the remote, on 127.0.0.1, sends both).
        if let Some(t) = get("KODACT_GEMINI_API_KEY") {
            for (h, v) in crate::endpoints::auth_headers(crate::endpoints::Wire::Gemini, &b, &t) {
                headers.push((h.to_string(), v));
            }
        }
        b
    } else {
        if let Some(t) = get("KODACT_OPENAI_API_KEY") {
            headers.push(("authorization".to_string(), format!("Bearer {t}")));
        }
        get("KODACT_OPENAI_BASE_URL").unwrap_or_default()
    };
    if let Some(lines) = get("KODACT_CUSTOM_HEADERS") {
        for l in lines.lines() {
            if let Some((k, v)) = l.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    let remote = env
        .iter()
        .filter(|(k, _)| k != "KODACT_CUSTOM_HEADERS")
        .map(|(k, v)| match k.as_str() {
            "KODACT_API_KEY"
            | "KODACT_AUTH_TOKEN"
            | "KODACT_OPENAI_API_KEY"
            | "KODACT_GEMINI_API_KEY" => {
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
        Message::user("Fix the greeting in src/main.rs so it says Hello, KodAct, and add a test for it.".into(), minutes_ago(9)),
        Message::Assistant {
            blocks: vec![
                Block::Thinking {
                    text: "The greeting is printed directly in `main`. To test it I should move the string into a function that `main` calls, then assert on that function.".into(),
                    open: false,
                },
                Block::Text("I'll move the greeting into a small function so it can be tested, then update `main`.".into()),
                Block::Tool(ToolCall { kind: ToolKind::Read, target: "src/main.rs".into(), note: None, status: ToolStatus::Done, output: "fn main() {\n    println!(\"Hello, world\");\n}\n".into(), diff: vec![], open: false, full: false, review: Review::Open, images: vec![] }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Edit,
                    target: "src/main.rs".into(),
                    note: None,
                    status: ToolStatus::Done,
                    output: "".into(),
                    diff: diff(&[
                        (DiffKind::Add, "pub fn greeting() -> &'static str {"),
                        (DiffKind::Add, "    \"Hello, KodAct\""),
                        (DiffKind::Add, "}"),
                        (DiffKind::Add, ""),
                        (DiffKind::Same, "fn main() {"),
                        (DiffKind::Del, "    println!(\"Hello, world\");"),
                        (DiffKind::Add, "    println!(\"{}\", greeting());"),
                        (DiffKind::Same, "}"),
                    ]),
                    open: true,
                    full: false, review: Review::Open, images: vec![],
                }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Bash,
                    target: "cargo test".into(),
                    note: Some("Run the tests".into()),
                    status: ToolStatus::Done,
                    output: "   \x1b[1;32mCompiling\x1b[0m hello v0.1.0 (/home/amine/dev/kryptonite)\n    \x1b[1;32mFinished\x1b[0m `test` profile [unoptimized + debuginfo] target(s) in 1.84s\n     \x1b[1;32mRunning\x1b[0m unittests src/main.rs (target/debug/deps/hello-3f2a9c1e)\n\nrunning 3 tests\ntest tests::greets_by_name ... \x1b[32mok\x1b[0m\ntest tests::greeting_is_static ... \x1b[32mok\x1b[0m\ntest tests::main_prints_the_greeting ... \x1b[32mok\x1b[0m\n\ntest result: \x1b[32mok\x1b[0m. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n   \x1b[1;32mDoc-tests\x1b[0m hello\n\nrunning 0 tests\n\ntest result: \x1b[32mok\x1b[0m. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s".into(),
                    diff: vec![],
                    open: true,
                    full: false, review: Review::Open, images: vec![],
                }),
            ],
        },
        Message::Assistant {
            blocks: vec![Block::Text(
                "Done. The greeting now lives in `greeting()` and `main` prints it.\n\n## What changed\n\n- `src/main.rs`: new **`greeting()`** function, used by `main`.\n- A unit test, *greets_by_name*, checks the text.\n\n```rust\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn greets_by_name() {\n        assert_eq!(super::greeting(), \"Hello, KodAct\");\n    }\n}\n```\n\n> Run `cargo test` any time to check it again.\n\n1. Build with `cargo build`.\n2. Run it with `cargo run`.".into(),
            )],
        },
        Message::TurnEnd { seconds: 18.4, usage: None },
        Message::user("Can you also check the hermes review pipeline for unused imports?".into(), minutes_ago(4)),
        Message::Assistant {
            blocks: vec![
                Block::Tool(ToolCall { kind: ToolKind::Web, target: "https://eslint.org/docs/latest/rules/no-unused-vars".into(), note: None, status: ToolStatus::Done, output: "no-unused-vars: disallow unused variables, including imports.".into(), diff: vec![], open: false, full: false, review: Review::Open, images: vec![] }),
                Block::Tool(ToolCall { kind: ToolKind::Write, target: "docs/lint-notes.md".into(), note: None, status: ToolStatus::Done, output: "".into(), diff: diff(&[(DiffKind::Add, "# Lint notes"), (DiffKind::Add, ""), (DiffKind::Add, "Run `pnpm lint --filter hermes` before pushing.")]), open: false, full: false, review: Review::Open, images: vec![] }),
                Block::Tool(ToolCall { kind: ToolKind::Search, target: "^use .*;$ in src/hermes".into(), note: None, status: ToolStatus::Done, output: "src/hermes/review.ts:3\nsrc/hermes/store.ts:1".into(), diff: vec![], open: false, full: false, review: Review::Open, images: vec![] }),
                Block::Tool(ToolCall { kind: ToolKind::Bash, target: "pnpm lint --filter hermes".into(), note: Some("Lint the package".into()), status: ToolStatus::Failed, output: "\x1b[4msrc/hermes/review.ts\x1b[24m\n  \x1b[2m3:10\x1b[22m  \x1b[31merror\x1b[39m  'diffLines' is defined but never used  \x1b[2m@typescript-eslint/no-unused-vars\x1b[22m\n\n\x1b[31m\x1b[1m✖ 1 problem (1 error, 0 warnings)\x1b[22m\x1b[39m\n\x1b[2m ELIFECYCLE \x1b[22m Command failed with exit code 1.".into(), diff: vec![], open: true, full: false, review: Review::Open, images: vec![] }),
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
                done(crate::backend::tool_call("mcp__kodact-browser__navigate", &serde_json::json!({"url": "https://serde.rs/field-attrs.html"}))),
                done(crate::backend::tool_call("mcp__kodact-browser__state", &serde_json::json!({}))),
                done(crate::backend::tool_call("mcp__kodact-browser__click", &serde_json::json!({"index": 12}))),
                done(crate::backend::tool_call("mcp__kodact-browser__text", &serde_json::json!({"query": "how #[serde(alias)] works"}))),
                Block::Tool(ToolCall { kind: ToolKind::Read, target: "src/settings.rs".into(), note: None, status: ToolStatus::Done, output: "pub struct Settings { pub theme: String, pub model: String, pub permissions: Vec<String> }".into(), diff: vec![], open: false, full: false, review: Review::Open, images: vec![] }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Write,
                    target: "docs/settings-migration-plan.md".into(),
                    note: None,
                    status: ToolStatus::Done,
                    output: "".into(),
                    diff: diff(&PLAN.lines().take(6).map(|l| (DiffKind::Add, l)).collect::<Vec<_>>()),
                    open: false,
                    full: false, review: Review::Open, images: vec![],
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
        Message::user("Draw how a chat in KodAct reaches the model, and where the API key gets added.".into(), minutes_ago(13)),
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
                    full: false, review: Review::Open, images: vec![],
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
            images: vec![],
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
mod chat_folder_tests {
    use super::chat_folder;
    use std::path::Path;

    #[test]
    fn a_new_plain_chat_gets_its_own_folder_and_an_old_one_keeps_the_shared() {
        let root = Path::new("/ws");
        assert_eq!(chat_folder(root, "abc", false), Path::new("/ws/chats/abc"));
        // Already written by KodAct CLI in the shared place: it stays there.
        assert_eq!(chat_folder(root, "old-chat-xyz", true), Path::new("/ws"));
        assert_eq!(chat_folder(root, "", false), Path::new("/ws"));
    }
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
        assert_eq!(
            (o.name.as_ref(), o.description.as_ref(), o.pinned),
            ("Budget work", "Q3 numbers", true)
        );
        assert!(list.last().unwrap().scratch);
        // A folder's own name isn't stored as a custom one.
        assert_eq!(
            super::Project::at(1, other.clone(), true).record().name,
            None
        );
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
    fn knowledge_rides_as_context_not_as_the_request() {
        let b = super::context_block(
            "<project_memory file=\"M.md\">\n- NN_topic.m\n</project_memory>\n",
        );
        // KodAct CLI leaves out of "what you asked" any text block that starts so.
        assert!(b.starts_with("<system-reminder>\n<project_memory"));
        assert!(b.ends_with("</project_memory>\n</system-reminder>"));
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
        let q = |u: &str, t: &str| Queued {
            attachments: vec![],
            uuid: u.into(),
            text: t.to_string().into(),
        };
        // Delivered mid-turn: by its own id.
        let mut list = vec![q("a", "Keep it short."), q("b", "Use lib.rs.")];
        assert_eq!(
            take_landed(&mut list, "b", "Use lib.rs."),
            vec![q("b", "Use lib.rs.")]
        );
        assert_eq!(list, vec![q("a", "Keep it short.")]);
        // Run as a turn of its own, joined, under KodAct CLI's id: by its words.
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
            stopped: false,
        };
        let text = String::from_utf8(feed.bytes()).unwrap();
        assert!(text.starts_with("\x1b[2;3m# Check the branch"));
        assert!(text.contains("\x1b[1;35m$\x1b[0m \x1b[1;34mgit\x1b[0m"));
        assert!(text.contains("\x1b[36m--show-current\x1b[0m"));
        assert!(text.contains("main\r\ndone\r\n"));
        assert!(text.contains("\x1b[1;32mexit 0") && text.contains("2 lines"));
        let failed = super::AgentFeed {
            failed: true,
            ..feed.clone()
        };
        let stopped = super::AgentFeed {
            stopped: true,
            ..feed.clone()
        };
        assert!(String::from_utf8_lossy(&stopped.bytes()).contains("stopped"));
        assert!(String::from_utf8(failed.bytes())
            .unwrap()
            .contains("\x1b[1;31mexit 1"));
    }

    #[test]
    fn a_reply_shows_a_diagram_s_final_version_once() {
        use super::{Block, Chat, FileRef, Message};
        let card = |name: &str, content: &str| FileRef {
            name: name.to_string().into(),
            path: Some(format!("docs/{name}").into()),
            content: content.to_string().into(),
            bytes: None,
        };
        let page = |h1: &str| {
            format!("<html><h1>{h1}</h1><svg viewBox=\"0 0 10 10\"><rect/></svg></html>")
        };
        let mut chat = Chat::new(1, 1, "t".into());
        chat.messages = vec![Message::Assistant { blocks: vec![] }];
        // Copied from the template, then edited: the edit brings its card.
        let v1 = card("tls.html", &page("Template"));
        assert_eq!(chat.update_card(v1.clone(), true), Some(v1.clone()));
        chat.messages = vec![Message::Assistant {
            blocks: vec![Block::File(v1)],
        }];
        // Edited again: the same card, now showing the last version.
        let v2 = card("tls.html", &page("TLS 1.3 handshake"));
        assert_eq!(chat.update_card(v2.clone(), true), None);
        let Message::Assistant { blocks } = &chat.messages[0] else {
            panic!()
        };
        assert!(matches!(&blocks[..], [Block::File(f)] if f == &v2));
        // Editing a README brings no card; writing one still does.
        let readme = card("README.md", "# KodAct");
        assert_eq!(chat.update_card(readme.clone(), true), None);
        assert_eq!(chat.update_card(readme.clone(), false), Some(readme));
    }

    #[test]
    fn a_command_s_documents_are_found_in_its_line_and_output() {
        let found = super::AppState::document_paths(
            "python scripts/docx_tool.py create memo.md out/Q3-memo.docx && echo done",
            "Wrote out/Q3-memo.docx (2 pages).\nAlso saved \"deck.pptx\", budget.XLSX; see report.pdf.",
        );
        assert_eq!(
            found,
            ["out/Q3-memo.docx", "deck.pptx", "budget.XLSX", "report.pdf"]
        );
        assert!(super::AppState::document_paths("ls -la", "a.txt b.md").is_empty());
        // Diagram Design starts a diagram by copying its template.
        let found = super::AppState::document_paths(
            "cp ~/.forge/plugin/forge/skills/diagram-design/assets/template.html docs/diagrams/tls.html",
            "",
        );
        assert_eq!(
            found.last().map(String::as_str),
            Some("docs/diagrams/tls.html")
        );
    }

    #[test]
    fn always_allow_allows_the_command_not_one_use_of_it() {
        let rule = |c: &str| {
            let v = super::command_rules(
                serde_json::json!([{"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": c}], "destination": "localSettings"}]),
            );
            super::rule_name(&v).unwrap()
        };
        assert_eq!(rule("rm perm_a.txt"), "Bash(rm:*)");
        assert_eq!(rule("rm -rf build"), "Bash(rm:*)");
        assert_eq!(rule("python3 -c \"print(1)\""), "Bash(python3:*)");
        assert_eq!(rule("/usr/bin/curl -s https://x"), "Bash(curl:*)");
        assert_eq!(rule("git push origin main"), "Bash(git push:*)");
        assert_eq!(rule("npm test:*"), "Bash(npm test:*)");
        assert_eq!(rule("git -C repo status"), "Bash(git:*)");
        assert_eq!(rule("FOO=1 make build && make test"), "Bash(make:*)");
        assert_eq!(
            super::always_label(Some(&rule("rm perm_a.txt"))),
            "Always allow rm"
        );
        // Other tools' rules are left as they are.
        let web = super::command_rules(
            serde_json::json!([{"type": "addRules", "rules": [{"toolName": "WebFetch", "ruleContent": "domain:docs.rs"}]}]),
        );
        assert_eq!(super::rule_name(&web).unwrap(), "WebFetch(domain:docs.rs)");
    }

    #[test]
    fn always_allow_says_what_it_allows_and_where() {
        use super::always_label as l;
        assert_eq!(l(Some("Bash(npm test:*)")), "Always allow npm test");
        assert_eq!(l(Some("Edit")), "Always allow edits");
        assert_eq!(l(Some("WebFetch(domain:docs.rs)")), "Always allow docs.rs");
        assert_eq!(l(Some("WebSearch")), "Always allow web searches");
        assert_eq!(
            l(Some("mcp__kodact-browser__click")),
            "Always allow browser click"
        );
        assert_eq!(
            l(Some("mcp__github__create_issue")),
            "Always allow github create issue"
        );
        assert_eq!(
            l(Some("mcp__kodact-browser__click")),
            "Always allow browser click"
        );
        assert_eq!(
            l(Some("mcp__github__create_issue")),
            "Always allow github create issue"
        );
        assert_eq!(l(None), "Always allow");
        assert!(l(Some(
            "Bash(cargo build --release --target x86_64-pc-windows-gnu)"
        ))
        .ends_with('…'));
        let s = serde_json::json!([{"type": "addRules", "rules": [{"toolName": "Edit"}], "destination": "localSettings"}]);
        assert_eq!(super::rule_scope(&s), Some("in this project, for you"));
        assert_eq!(super::rule_scope(&serde_json::json!([])), None);
    }

    #[test]
    fn remote_sessions_get_no_secrets() {
        let env: Vec<(String, String)> = [
            ("KODACT_BASE_URL", "https://api.example.com"),
            ("KODACT_API_KEY", "sk-real"),
            ("KODACT_AUTH_TOKEN", "sk-real"),
            ("KODACT_CUSTOM_HEADERS", "x-team: blue\nx-org: 7"),
            ("KODACT_CONTEXT_WINDOW", "200000"),
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
        assert!(remote.contains(&("KODACT_CONTEXT_WINDOW".into(), "200000".into())));
        // OpenAI-compatible: the bearer only.
        let env = vec![
            (
                "KODACT_OPENAI_BASE_URL".to_string(),
                "http://h/v1".to_string(),
            ),
            ("KODACT_OPENAI_API_KEY".to_string(), "k".to_string()),
        ];
        let (route, remote) = super::proxied_env(&env);
        assert_eq!(route.base, "http://h/v1");
        assert_eq!(
            route.headers,
            [("authorization".to_string(), "Bearer k".to_string())]
        );
        assert_eq!(remote[1].1, crate::model_proxy::PLACEHOLDER_KEY);
        // Gemini: the key as Google AI Studio takes it; the remote gets a placeholder.
        let env = vec![
            (
                "KODACT_GEMINI_BASE_URL".to_string(),
                "https://generativelanguage.googleapis.com/v1beta".to_string(),
            ),
            ("KODACT_GEMINI_API_KEY".to_string(), "AIza-real".to_string()),
        ];
        let (route, remote) = super::proxied_env(&env);
        assert_eq!(
            route.base,
            "https://generativelanguage.googleapis.com/v1beta"
        );
        assert_eq!(
            route.headers,
            [("x-goog-api-key".to_string(), "AIza-real".to_string())]
        );
        assert_eq!(remote[1].1, crate::model_proxy::PLACEHOLDER_KEY);
    }
    use super::*;

    fn question(multi: bool, labels: &[&str]) -> Question {
        Question {
            header: "H".into(),
            text: format!("{labels:?}?"),
            options: labels
                .iter()
                .map(|l| QOption {
                    label: l.to_string(),
                    description: String::new(),
                })
                .collect(),
            multi,
            ..Default::default()
        }
    }

    #[test]
    fn picking_answers_a_question_and_moves_on() {
        let mut qs = vec![
            question(false, &["a", "b"]),
            question(true, &["x", "y", "z"]),
        ];
        let mut step = 0;
        pick(&mut qs, &mut step, 0, 1);
        assert_eq!((qs[0].answer().as_deref(), step), (Some("b"), 1));
        // Several allowed: picks toggle, and the card stays.
        pick(&mut qs, &mut step, 1, 2);
        pick(&mut qs, &mut step, 1, 0);
        pick(&mut qs, &mut step, 1, 2);
        assert_eq!((qs[1].answer().as_deref(), step), (Some("x"), 1));
        // Your own words replace the picks; the last answer completes them all.
        assert!(own_answer(&mut qs, &mut step, "both".into()));
        assert_eq!(qs[1].answer().as_deref(), Some("both"));
        // Typed on the first of two unanswered: it moves to the second.
        let mut qs = vec![question(false, &["a", "b"]), question(false, &["c", "d"])];
        let mut step = 0;
        assert!(!own_answer(&mut qs, &mut step, "mine".into()));
        assert_eq!(step, 1);
        // An option out of range does nothing.
        pick(&mut qs, &mut step, 1, 9);
        assert!(!qs[1].answered());
    }

    #[test]
    fn find_looks_through_every_kind_of_message() {
        let chats = mock_chats();
        let rich = &chats.iter().find(|c| c.id == 1).unwrap().messages;
        // Your message, a tool's target and a reply's prose.
        assert_eq!(find_hits(rich, "HELLO, KODACT"), vec![0, 1, 2]);
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

    #[test]
    fn on_agent_event_interleaves_entries_dedups_text_and_captures_step_output() {
        use crate::backend::Event;
        let mut s = AppState::new();
        let chat = 42;

        s.on_agent_event(
            chat,
            &Event::ToolUse {
                id: "ag-1".into(),
                name: "Agent".into(),
                input: serde_json::json!({
                    "subagent_type": "Explore",
                    "description": "Audit state module",
                    "prompt": "Check state transitions"
                }),
            },
        );
        assert_eq!(
            s.selected_agent.get(&chat).map(String::as_str),
            Some("ag-1")
        );

        s.on_agent_event(
            chat,
            &Event::SubText {
                parent: "ag-1".into(),
                text: "Looking at `src/state.rs` first.".into(),
            },
        );
        // Duplicate consecutive text with same trimmed content is ignored.
        s.on_agent_event(
            chat,
            &Event::SubText {
                parent: "ag-1".into(),
                text: "  Looking at `src/state.rs` first.  ".into(),
            },
        );
        s.on_agent_event(
            chat,
            &Event::SubTool {
                parent: "ag-1".into(),
                id: "step-1".into(),
                name: "Read".into(),
                input: serde_json::json!({"file_path": "src/state.rs"}),
            },
        );
        s.on_agent_event(
            chat,
            &Event::SubResult {
                parent: "ag-1".into(),
                id: "step-1".into(),
                output: "pub struct AppState { ... }".into(),
                is_error: false,
            },
        );
        s.on_agent_event(
            chat,
            &Event::SubText {
                parent: "ag-1".into(),
                text: "Found AppState definition.".into(),
            },
        );
        s.on_agent_event(
            chat,
            &Event::ToolResult {
                id: "ag-1".into(),
                output: "Audit complete.".into(),
                is_error: false,
                images: vec![],
            },
        );

        let runs = &s.agents[&chat];
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.entries.len(), 3);
        assert!(matches!(&r.entries[0], AgentEntry::Text(t) if t.contains("Looking at")));
        assert!(matches!(r.entries[1], AgentEntry::Step(0)));
        assert!(matches!(&r.entries[2], AgentEntry::Text(t) if t == "Found AppState definition."));
        assert_eq!(r.steps.len(), 1);
        assert_eq!(r.steps[0].status, ToolStatus::Done);
        assert_eq!(r.steps[0].output, "pub struct AppState { ... }");
        assert!(r.steps[0].ended.is_some());
        assert_eq!(r.said, "Audit complete.");
    }

    #[test]
    fn open_agent_for_block_maps_live_and_synthesizes_historical_runs() {
        let mut s = AppState::new();
        let chat = 77;
        let mut chat_obj = Chat::new(chat, 1, "Review".into());
        chat_obj.messages = vec![Message::Assistant {
            blocks: vec![
                Block::Text("Dispatching two agents.".into()),
                Block::Tool(ToolCall {
                    kind: ToolKind::Other,
                    target: "Agent".into(),
                    note: Some("Inspect parser".into()),
                    status: ToolStatus::Done,
                    output: "Parser looks clean.".into(),
                    diff: vec![],
                    open: false,
                    full: false,
                    review: Review::Open,
                    images: vec![],
                }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Other,
                    target: "Task".into(),
                    note: Some("Inspect UI".into()),
                    status: ToolStatus::Failed,
                    output: "Found contrast issue.".into(),
                    diff: vec![],
                    open: false,
                    full: false,
                    review: Review::Open,
                    images: vec![],
                }),
            ],
        }];
        s.chats.push(chat_obj);
        s.current = Some(chat);

        // Historical chat has no live entries in s.agents yet: clicking the 2nd block synthesizes both.
        assert!(s.focus_agent_for_block(chat, 0, 2));
        let runs = &s.agents[&chat];
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].task, "Inspect parser");
        assert_eq!(runs[1].task, "Inspect UI");
        assert!(runs[1].failed);
        assert_eq!(runs[1].said, "Found contrast issue.");
        assert_eq!(s.selected_agent_id(), Some(runs[1].id.as_str()));

        // Clicking the 1st block focuses the first run without duplicating runs.
        assert!(s.focus_agent_for_block(chat, 0, 1));
        assert_eq!(s.agents[&chat].len(), 2);
        assert_eq!(s.selected_agent_id(), Some(s.agents[&chat][0].id.as_str()));
    }

    #[test]
    fn toggle_step_and_cycle_agent_work_with_wraparound() {
        let mut s = AppState::new();
        let chat = 99;
        let now = std::time::Instant::now();
        s.current = Some(chat);
        s.agents.insert(
            chat,
            vec![
                AgentRun {
                    id: "a1".into(),
                    kind: "Explore".into(),
                    task: "First".into(),
                    prompt: "".into(),
                    started: now,
                    ended: Some(now),
                    heard: now,
                    failed: false,
                    steps: vec![AgentStep {
                        id: "s1".into(),
                        kind: ToolKind::Read,
                        what: "src/lib.rs".into(),
                        described: false,
                        detail: "src/lib.rs".into(),
                        output: "fn main() {}".into(),
                        started: now,
                        ended: Some(now),
                        status: ToolStatus::Done,
                        open: false,
                    }],
                    entries: vec![AgentEntry::Step(0)],
                    said: "Done 1".into(),
                    stopping: false,
                    stopped: false,
                },
                AgentRun {
                    id: "a2".into(),
                    kind: "Plan".into(),
                    task: "Second".into(),
                    prompt: "".into(),
                    started: now,
                    ended: None,
                    heard: now,
                    failed: false,
                    steps: vec![],
                    entries: vec![],
                    said: String::new(),
                    stopping: false,
                    stopped: false,
                },
            ],
        );

        // No explicit selection defaults to first active agent ("a2").
        assert_eq!(s.selected_agent_id(), Some("a2"));
        // Cycle forward wraps to "a1".
        assert!(s.cycle_agent_in(chat, 1));
        assert_eq!(s.selected_agent_id(), Some("a1"));
        // Cycle backward wraps back to "a2".
        assert!(s.cycle_agent_in(chat, -1));
        assert_eq!(s.selected_agent_id(), Some("a2"));

        // Step toggle opens and closes step 0 on "a1".
        assert!(!s.agents[&chat][0].steps[0].open);
        assert!(s.toggle_step_in(chat, "a1", 0));
        assert!(s.agents[&chat][0].steps[0].open);
        assert!(s.toggle_step_in(chat, "a1", 0));
        assert!(!s.agents[&chat][0].steps[0].open);
    }

    #[test]
    fn click_outside_same_position_does_not_reopen_menu_on_button_click() {
        let s_point = gpui::point(gpui::px(100.), gpui::px(40.));
        let other_point = gpui::point(gpui::px(220.), gpui::px(40.));

        // Simulate AppState::toggle_title_menu / dismiss_title_menu logic directly:
        let mut open = Some(TitleMenu::Account);
        let mut dismissed: Option<(TitleMenu, gpui::Point<gpui::Pixels>)> = None;

        // First: outside mouse down dismisses menu at s_point.
        if let Some(was) = open.take() {
            dismissed = Some((was, s_point));
        }
        assert!(open.is_none());

        // Click landing at the same coordinates must NOT reopen Account.
        let just_dismissed = dismissed
            .take()
            .filter(|(m, pos)| {
                *m == TitleMenu::Account
                    && (pos.x - s_point.x).abs() <= gpui::px(2.)
                    && (pos.y - s_point.y).abs() <= gpui::px(2.)
            })
            .is_some();
        assert!(just_dismissed);

        // Next click (no prior dismiss) opens it cleanly.
        let just_dismissed_next = dismissed
            .take()
            .filter(|(m, pos)| *m == TitleMenu::Account && *pos == other_point)
            .is_some();
        assert!(!just_dismissed_next);
    }

    #[test]
    fn pop_queued_preserves_remaining_items_and_demo_ten_agents_spawns_ten() {
        let mut s = AppState::new();
        let chat = 7;
        s.queued.insert(
            chat,
            vec![
                Queued {
                    uuid: "q1".into(),
                    text: "first".into(),
                    attachments: vec![],
                },
                Queued {
                    uuid: "q2".into(),
                    text: "second".into(),
                    attachments: vec![],
                },
                Queued {
                    uuid: "q3".into(),
                    text: "third".into(),
                    attachments: vec![],
                },
            ],
        );

        let popped = s.pop_queued(chat, "q2").expect("middle item");
        assert_eq!(popped.text, "second");
        let left: Vec<&str> = s.queued[&chat].iter().map(|q| q.uuid.as_ref()).collect();
        assert_eq!(left, ["q1", "q3"]);
        assert!(s.pop_queued(chat, "missing").is_none());

        let steps = demo_ten_agents();
        let started = steps
            .iter()
            .filter(|st| {
                matches!(
                    st,
                    Step::Agent(crate::backend::Event::ToolUse { name, .. }) if name == "Task"
                )
            })
            .count();
        assert_eq!(started, 10);
    }

    #[test]
    fn session_panel_defaults_open_while_new_chat_has_no_active_conversation() {
        let s = AppState::new();
        assert!(s.session_open);
        assert_eq!(s.current, None);
        assert_eq!(s.session_open.then_some(s.current).flatten(), None);
    }

    #[test]
    fn failed_first_turn_does_not_mark_chat_resumable() {
        let mut c = Chat::new(1, 0, "Greeting".into());
        assert!(!c.resumable);
        // Turn 1 fails (e.g. Unknown command: /i-have-adhd): session file not written.
        c.record_turn_end(1.6, None, false);
        assert!(!c.resumable);
        // Once a real turn succeeds, it becomes resumable and stays resumable on a later error.
        c.record_turn_end(2.4, None, true);
        assert!(c.resumable);
        c.record_turn_end(0.5, None, false);
        assert!(c.resumable);
    }

    #[test]
    fn edit_and_new_conversation_preserve_plain_chat_folder_and_prior_context() {
        let mut s = AppState::new();
        let scratch_proj = s.scratch_project().expect("scratch project");
        let mut c = Chat::new(42, scratch_proj, "Context test".into());
        let initial_folder_id = c.folder.clone().expect("initial folder");
        c.messages = vec![
            Message::user("My project codename is Aurora.".into(), SystemTime::now()),
            Message::Assistant {
                blocks: vec![
                    Block::Tool(ToolCall {
                        kind: ToolKind::Read,
                        target: "notes.md".into(),
                        note: None,
                        status: ToolStatus::Done,
                        output: "ok".into(),
                        diff: vec![],
                        open: false,
                        full: false,
                        review: Review::Open,
                        images: vec![],
                    }),
                    Block::Text("Got it — Aurora noted.".into()),
                ],
            },
            Message::TurnEnd {
                seconds: 1.2,
                usage: None,
            },
            Message::user("Draft the summary".into(), SystemTime::now()),
        ];
        c.resumable = true;
        s.chats.push(c);

        let dir_before = s.chat_project_dir(42).expect("plain chat dir");
        assert!(dir_before.ends_with(&initial_folder_id));

        // Cut back at message index 3 (editing/retrying Turn 2).
        let (cut_text, _old_session) = s.truncate_chat_from(42, 3).expect("truncated");
        assert_eq!(cut_text, "Draft the summary");
        assert!(s.history_unseeded.contains(&42));

        // Plain-chat folder remains identical even though `session` rotated!
        let dir_after = s.chat_project_dir(42).expect("plain chat dir after cut");
        assert_eq!(dir_after, dir_before);
        assert_ne!(s.chats[0].session, initial_folder_id);
        assert!(!s.chats[0].resumable);

        // Next outgoing turn is `/reload-plugins`: goes bare, keeps history_unseeded armed!
        s.chats[0]
            .messages
            .push(Message::user("/reload-plugins".into(), SystemTime::now()));
        assert!(s
            .take_outgoing_history(42, "/reload-plugins", true)
            .is_none());
        assert!(s.history_unseeded.contains(&42));
        // `/reload-plugins` finishes without assistant text, marking resumable = true.
        s.chats[0].record_turn_end(1.8, None, true);
        assert!(s.chats[0].resumable);

        // Next normal user message receives `<conversation_history>` with Aurora and clears flag!
        s.chats[0].messages.push(Message::user(
            "what is this conversation context?".into(),
            SystemTime::now(),
        ));
        let seeded = s
            .take_outgoing_history(42, "what is this conversation context?", false)
            .expect("seeded history");
        assert!(!s.history_unseeded.contains(&42));
        assert!(seeded.starts_with("<conversation_history>"));
        assert!(seeded.contains("Aurora"));
        assert!(seeded.contains("Got it — Aurora noted."));
        assert!(seeded.contains("[Tool Read: notes.md (done)]"));
        assert!(!seeded.contains("/reload-plugins"));
        // Subsequent turns do not re-send `<conversation_history>`.
        s.chats[0]
            .messages
            .push(Message::user("and next?".into(), SystemTime::now()));
        assert!(s.take_outgoing_history(42, "and next?", false).is_none());
    }

    #[test]
    fn slash_actions_and_prompts_separate_cleanly_in_autocomplete() {
        let mut s = AppState::new();
        s.skill_commands = crate::commands::plugin_skills()
            .chain(crate::commands::bundled_skills())
            .collect();
        let trig = crate::ui::autocomplete::trigger("/").expect("slash trigger");
        let rows = crate::ui::autocomplete::items(&trig, &s);
        let plugins_row = rows.iter().find(|r| r.label.as_ref() == "/plugins").expect("/plugins");
        assert!(matches!(
            &plugins_row.pick,
            crate::ui::autocomplete::Pick::SlashAction(run) if run == "/plugins"
        ));
        let reload_row = rows
            .iter()
            .find(|r| r.label.as_ref() == "/reload-plugins")
            .expect("/reload-plugins");
        assert!(matches!(
            &reload_row.pick,
            crate::ui::autocomplete::Pick::SlashAction(run) if run == "/reload-plugins"
        ));
        let review_row = rows.iter().find(|r| r.label.as_ref() == "/review").expect("/review");
        assert!(review_row.skill);
        assert!(matches!(
            &review_row.pick,
            crate::ui::autocomplete::Pick::Text(t) if t == "/review "
        ));
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;

    #[test]
    fn a_page_previews_at_the_width_of_its_device() {
        assert_eq!(Device::Desktop.width(), None);
        assert_eq!(Device::Tablet.width(), Some(768));
        assert_eq!(Device::Phone.width(), Some(390));
        let page = |name: &str, content: &str| FileRef {
            name: name.into(),
            path: None,
            content: content.into(),
            bytes: None,
        };
        assert!(page("a.html", "<p>hi</p>").is_html());
        assert!(!page("a.html", "<svg></svg>").is_html());
        assert!(!page("a.md", "# hi").is_html());
    }

    #[test]
    fn a_file_chip_finds_its_file() {
        let root = std::path::Path::new("/work/app");
        let files: Vec<String> = ["Cargo.toml", "src/", "src/engine.rs", "src/ui/verify.rs", "tests/verify.rs", "a/b/c/verify.rs"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let find = |n: &str| super::resolve_named_file(n, root, Some(&files), false);
        assert_eq!(find("src/engine.rs").as_deref(), Some("src/engine.rs"));
        assert_eq!(find("./Cargo.toml").as_deref(), Some("Cargo.toml"));
        assert_eq!(find("/work/app/src/engine.rs").as_deref(), Some("src/engine.rs"));
        assert_eq!(find("src\\engine.rs").as_deref(), Some("src/engine.rs"));
        // A bare name: the shallowest of those that share it.
        assert_eq!(find("verify.rs").as_deref(), Some("tests/verify.rs"));
        assert_eq!(find("engine.rs").as_deref(), Some("src/engine.rs"));
        assert_eq!(find("missing.rs"), None);
        assert_eq!(find("../etc/passwd"), None);
        assert_eq!(find("/etc/hosts"), None);
        // Without a list, nothing is guessed on a remote project.
        assert_eq!(super::resolve_named_file("Cargo.toml", root, None, false), None);
    }
}
