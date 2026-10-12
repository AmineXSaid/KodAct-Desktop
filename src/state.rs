//! App state: chats, the transcript model, models and modes, and the stubs
//! that stand in for a backend. Nothing here talks to a network: `send`
//! plays a scripted reply on a timer so every part of the UI can be seen
//! working (streaming text, thinking, tool calls, a permission prompt).
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
    /// A file Forge made for you: a card in the reply, opened beside the chat.
    File(FileRef),
}

/// A file Forge made: shown as a card in the reply (`ui/file_card.rs`),
/// opened in the side panel (`ui/doc_pane.rs`), saved by Download.
#[derive(Clone, Debug, PartialEq)]
pub struct FileRef {
    /// Its file name: `settings-migration-plan.md`.
    pub name: SharedString,
    /// Where it was written in the project, if it was.
    pub path: Option<SharedString>,
    pub content: SharedString,
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
        self.name.rsplit_once('.').map_or(self.name.to_string(), |(stem, _)| stem.to_string()).into()
    }

    /// Its type, as the card labels it: `MD`, `PDF`, `DOCX`.
    pub fn kind(&self) -> SharedString {
        self.name.rsplit_once('.').map_or("FILE".to_string(), |(_, ext)| ext.to_ascii_uppercase()).into()
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

#[derive(Clone, Debug)]
pub enum Message {
    User { text: SharedString, sent: SystemTime },
    Assistant { blocks: Vec<Block> },
    /// The end of a turn: how long it took, and the usage the model
    /// reported for it (`context.rs`).
    TurnEnd { seconds: f32, usage: Option<crate::context::Usage> },
    /// Something the app says (interrupted, an error).
    Notice { text: SharedString, error: bool },
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
}

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
    /// A diagram in the panel fits its width (else it shows at actual size).
    pub doc_fit: bool,
    copied_reset: Option<Task<()>>,
    /// A short message at the bottom of the window (stubs say what they would do).
    pub toast: Option<SharedString>,
    toast_reset: Option<Task<()>>,
    /// The replies being played, by chat. Each keeps going while you look at
    /// another chat; dropping one stops it.
    replies: std::collections::HashMap<usize, Task<()>>,
    /// Text for the composer to take (an edited message).
    pub draft: Option<String>,
    /// Chats are still loading: screens show placeholders.
    pub loading: bool,
    load: Option<Task<()>>,
    next_id: usize,
}

impl AppState {
    pub fn new() -> Self {
        let chats = mock_chats();
        let next_id = chats.len() + 1;
        let user_name: Option<SharedString> = crate::profile::load_name().map(Into::into);
        let saved_avatar = crate::profile::load_avatar();
        let project_dir = std::env::var_os("FORGE_PROJECT_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let projects = mock_projects(&project_dir);
        Self {
            onboarding: user_name.is_none(),
            first_run: user_name.is_none(),
            user_name,
            avatar_saved: saved_avatar.is_some(),
            avatar: saved_avatar.unwrap_or_else(|| crate::avatars::random(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7)).to_string()).into(),
            project: projects[0].name.clone(),
            project_dir,
            branch: projects[0].branch.clone(),
            current_project: projects[0].id,
            projects,
            all_chats: false,
            switcher_open: false,
            filter_open: false,
            screen: Screen::Chat,
            current: Some(chats[0].id),
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
            greeting_seed: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            greeting_timer: None,
            mode: ModeId::Manual,
            kept_modes: Default::default(),
            default_mode: ModeId::Manual,
            bypass_allowed: false,
            modes_open: false,
            bypass_warning: false,
            dark: false,
            palette_open: false,
            picker_open: false,
            copied: None,
            doc: None,
            doc_fit: true,
            copied_reset: None,
            toast: None,
            toast_reset: None,
            replies: Default::default(),
            draft: None,
            loading: true,
            load: None,
            next_id,
        }
    }

    /// Load the chats. In the full app this reads the session index off the
    /// UI thread; this shell's chats are built in, so it only stands in for
    /// that read's time, so the loading placeholders can be seen.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        const MOCK_INDEX_READ: Duration = Duration::from_millis(700);
        self.load = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(MOCK_INDEX_READ).await;
            let _ = this.update(cx, |s, cx| {
                s.loading = false;
                cx.notify();
            });
        }));
        self.load_endpoints(cx);
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
    pub fn load_endpoints(&mut self, cx: &mut Context<Self>) {
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
                s.sync_due(cx);
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
        let Some(e) = self.endpoints.as_mut() else { return };
        if !e.profiles.iter().any(|p| p.name == name) {
            return;
        }
        e.selected = Some(name.to_string());
        self.picker_open = false;
        // An effort the new model doesn't take is dropped.
        let levels = self.active_profile().map(|p| p.effort_levels()).unwrap_or_default();
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
        let due: Vec<String> = e.profiles.iter().filter(|p| crate::health::is_due(self.health.get(&p.name).and_then(|h| h.last_synced_at), now, e.sync_interval_minutes)).map(|p| p.name.clone()).collect();
        self.check(due, true, cx);
    }

    /// The model window's refresh: check them all now.
    pub fn sync_all(&mut self, cx: &mut Context<Self>) {
        let names = self.endpoints.as_ref().map(|e| e.profiles.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
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
            let fingerprint = self.endpoints.as_ref().and_then(|e| e.profiles.iter().find(|p| p.name == name)).map(|p| p.fingerprint()).unwrap_or_default();
            let entry = self.health.entry(name.clone()).or_default();
            if entry.fingerprint != fingerprint {
                *entry = crate::health::EndpointHealth { fingerprint, ..Default::default() };
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
            let Some((name, background)) = self.check_queue.pop_front() else { break };
            let Some(profile) = self.endpoints.as_ref().and_then(|e| e.profiles.iter().find(|p| p.name == name)).cloned() else {
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
                let (outcome, listed) = cx.background_spawn(async move { (crate::health::probe(&probed, timeout), crate::health::listed_context(&probed, timeout)) }).await;
                let _ = this.update(cx, |s, cx| {
                    s.checks_running -= 1;
                    if s.check_gen.get(&profile.name) == Some(&generation) {
                        crate::health::apply(&mut s.health, &profile, outcome, listed);
                        let store = s.health.clone();
                        cx.background_spawn(async move { crate::health::save(&store) }).detach();
                    }
                    s.pump_checks(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub fn chat(&self) -> Option<&Chat> {
        self.current.and_then(|id| self.chats.iter().find(|c| c.id == id))
    }

    fn chat_mut(&mut self, id: usize) -> Option<&mut Chat> {
        self.chats.iter_mut().find(|c| c.id == id)
    }

    /// A reply is playing in the chat you're looking at.
    pub fn busy(&self) -> bool {
        self.current.is_some_and(|id| self.replies.contains_key(&id))
    }

    /// Where a chat stands, for its row in the sidebar.
    pub fn status(&self, chat: &Chat) -> ChatStatus {
        if self.replies.contains_key(&chat.id) {
            return if self.waiting_on_permission(chat.id) { ChatStatus::NeedsYou } else { ChatStatus::Working };
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
        let Some(p) = self.projects.iter_mut().find(|p| p.id == id) else { return };
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
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        self.projects.push(Project { id, name: name.into(), path, branch: "main".into(), pinned: false, opened_minutes_ago: 0 });
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
        self.greeting_seed = self.greeting_seed.wrapping_add(1);
        self.sync_mode();
        cx.notify();
    }

    /// The mode for the chat now showing: what it kept, else the setting's.
    fn sync_mode(&mut self) {
        self.mode = self.current.and_then(|id| self.kept_modes.get(&id).copied()).unwrap_or_else(|| crate::modes::initial(self.default_mode, self.bypass_allowed));
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
        let Some(text) = self.take_from(chat, msg, cx) else { return };
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
        let c = self.chat_mut(chat)?;
        let Some(Message::User { text, .. }) = c.messages.get(msg) else { return None };
        let text = text.to_string();
        c.messages.truncate(msg);
        c.version += 1;
        cx.notify();
        Some(text)
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

    /// Open a file in the side panel (the same file again closes it).
    pub fn open_doc(&mut self, file: FileRef, cx: &mut Context<Self>) {
        self.doc = if self.doc.as_ref() == Some(&file) { None } else { Some(file) };
        cx.notify();
    }

    pub fn toggle_doc_fit(&mut self, cx: &mut Context<Self>) {
        self.doc_fit = !self.doc_fit;
        cx.notify();
    }

    /// Open a diagram's page in the browser, where it is interactive: write
    /// it to Forge's temp folder off the UI thread, then hand it to the system.
    pub fn open_in_browser(&mut self, file: &FileRef, cx: &mut Context<Self>) {
        let dir = std::env::temp_dir().join("forge-diagrams");
        let (name, bytes) = (crate::downloads::clean_name(&file.name), file.content.as_bytes().to_vec());
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
        let (name, bytes) = (file.name.to_string(), file.content.as_bytes().to_vec());
        cx.spawn(async move |this, cx| {
            let saved = cx.background_spawn(async move { crate::downloads::save(&dir, &name, &bytes) }).await;
            let _ = this.update(cx, |s, cx| match saved {
                Ok(path) => s.show_toast(format!("Saved to Downloads as {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), cx),
                Err(e) => s.show_toast(format!("Could not save it: {e}"), cx),
            });
        })
        .detach();
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
        let Some(id) = self.current else { return };
        if self.replies.remove(&id).is_none() {
            return;
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
                self.chats.insert(0, Chat { id, project: self.current_project, title: title.into(), minutes_ago: 0, pinned: false, messages: vec![], version: 0, unread: false });
                self.current = Some(id);
                self.keep_mode();
                id
            }
        };
        self.screen = Screen::Chat;
        if let Some(c) = self.chat_mut(id) {
            c.messages.push(Message::User { text: text.clone().into(), sent: SystemTime::now() });
            c.minutes_ago = 0;
            c.version += 1;
        }
        self.replies.insert(id, cx.spawn(async move |this, cx| {
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
                s.replies.remove(&id);
                // Finished while you were elsewhere: it waits as unread.
                if s.current != Some(id) {
                    if let Some(c) = s.chat_mut(id) {
                        c.unread = true;
                    }
                }
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
            (Step::End { seconds }, _) => {
                // A mock reply has no model to report usage: count it from the text.
                let usage = crate::context::estimate(&c.messages);
                c.messages.push(Message::TurnEnd { seconds, usage: Some(usage) });
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
    End { seconds: f32 },
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
    s.push(Step::End { seconds: 14.2 });
    s
}

// ---------------------------------------------------------------- mock chats

/// The folder Forge started in, and a few others to switch between.
fn mock_projects(here: &std::path::Path) -> Vec<Project> {
    let home = crate::rules::home_dir().unwrap_or_default();
    let name = here.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
    let p = |id, name: &str, branch: &str, pinned, opened| Project {
        id,
        name: name.to_string().into(),
        path: home.join("dev").join(name),
        branch: branch.to_string().into(),
        pinned,
        opened_minutes_ago: opened,
    };
    vec![
        Project { id: 1, name: name.into(), path: here.to_path_buf(), branch: "fix/hermes_upgrade".into(), pinned: false, opened_minutes_ago: 0 },
        p(2, "genesis", "main", true, 600),
        p(3, "forge-cli", "feat/model-evals", false, 1500),
        p(4, "hermes", "main", false, 3000),
    ]
}

fn minutes_ago(minutes: u32) -> SystemTime {
    SystemTime::now() - Duration::from_secs(minutes as u64 * 60)
}

fn mock_chats() -> Vec<Chat> {
    let rich = vec![
        Message::User { text: "Fix the greeting in src/main.rs so it says Hello, Forge, and add a test for it.".into(), sent: minutes_ago(9) },
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
        Message::TurnEnd { seconds: 18.4, usage: None },
        Message::User { text: "Can you also check the hermes review pipeline for unused imports?".into(), sent: minutes_ago(4) },
        Message::Assistant {
            blocks: vec![
                Block::Tool(ToolCall { kind: ToolKind::Web, target: "https://eslint.org/docs/latest/rules/no-unused-vars".into(), note: None, status: ToolStatus::Done, output: "no-unused-vars: disallow unused variables, including imports.".into(), diff: vec![], open: false, full: false }),
                Block::Tool(ToolCall { kind: ToolKind::Write, target: "docs/lint-notes.md".into(), note: None, status: ToolStatus::Done, output: "".into(), diff: diff(&[(DiffKind::Add, "# Lint notes"), (DiffKind::Add, ""), (DiffKind::Add, "Run `pnpm lint --filter hermes` before pushing.")]), open: false, full: false }),
                Block::Tool(ToolCall { kind: ToolKind::Search, target: "^use .*;$ in src/hermes".into(), note: None, status: ToolStatus::Done, output: "src/hermes/review.ts:3\nsrc/hermes/store.ts:1".into(), diff: vec![], open: false, full: false }),
                Block::Tool(ToolCall { kind: ToolKind::Bash, target: "pnpm lint --filter hermes".into(), note: Some("Lint the package".into()), status: ToolStatus::Failed, output: "\x1b[4msrc/hermes/review.ts\x1b[24m\n  \x1b[2m3:10\x1b[22m  \x1b[31merror\x1b[39m  'diffLines' is defined but never used  \x1b[2m@typescript-eslint/no-unused-vars\x1b[22m\n\n\x1b[31m\x1b[1m✖ 1 problem (1 error, 0 warnings)\x1b[22m\x1b[39m\n\x1b[2m ELIFECYCLE \x1b[22m Command failed with exit code 1.".into(), diff: vec![], open: true, full: false }),
                Block::Text("One unused import: `diffLines` in `src/hermes/review.ts`. Want me to remove it?".into()),
            ],
        },
        Message::TurnEnd { seconds: 6.1, usage: None },
    ];
    let simple = |title: &str, minutes: u32| {
        vec![
            Message::User { text: title.to_string().into(), sent: minutes_ago(minutes) },
            Message::Assistant { blocks: vec![Block::Text(format!("This is an older conversation about *{}*.", title.to_lowercase()).into())] },
            Message::TurnEnd { seconds: 3.2, usage: None },
        ]
    };
    // A reply that wrote a plan: the write, a short note, and the file's card.
    const PLAN: &str = include_str!("../assets/docs/settings-migration-plan.md");
    let plan = vec![
        Message::User { text: "Plan the migration of our settings to the new schema. Write it up so I can review it.".into(), sent: minutes_ago(41) },
        Message::Assistant {
            blocks: vec![
                Block::Tool(ToolCall { kind: ToolKind::Read, target: "src/settings.rs".into(), note: None, status: ToolStatus::Done, output: "pub struct Settings { pub theme: String, pub model: String, pub permissions: Vec<String> }".into(), diff: vec![], open: false, full: false }),
                Block::Tool(ToolCall {
                    kind: ToolKind::Write,
                    target: "docs/settings-migration-plan.md".into(),
                    note: None,
                    status: ToolStatus::Done,
                    output: "".into(),
                    diff: diff(&PLAN.lines().take(6).map(|l| (DiffKind::Add, l)).collect::<Vec<_>>()),
                    open: false,
                    full: false,
                }),
                Block::Text("Here is the plan. It moves every setting to the v2 schema without losing anything, and keeps a backup for one release. Two checklist items are still open: the round-trip test and a Windows run with a read-only file.".into()),
                Block::File(FileRef { name: "settings-migration-plan.md".into(), path: Some("docs/settings-migration-plan.md".into()), content: PLAN.into() }),
            ],
        },
        Message::TurnEnd { seconds: 24.7, usage: None },
    ];
    // A reply that drew a diagram with the bundled Diagram Design skill.
    const CHAT_PATH: &str = include_str!("../assets/docs/forge-chat-path.html");
    let drawn = vec![
        Message::User { text: "Draw how a chat in Forge reaches the model, and where the API key gets added.".into(), sent: minutes_ago(13) },
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
                    full: false,
                }),
                Block::Text("Here it is. The gateway is the one accent: it is the only piece that ever holds a key, and it reads it from Credential Manager on the way out.".into()),
                Block::File(FileRef { name: "chat-path.html".into(), path: Some("docs/chat-path.html".into()), content: CHAT_PATH.into() }),
            ],
        },
        Message::TurnEnd { seconds: 31.2, usage: None },
    ];
    let with_error = |mut m: Vec<Message>| {
        m.push(Message::Notice { text: "The agent stopped unexpectedly (exit code 1). Send a message to try again.".into(), error: true });
        m
    };
    let mut chats = vec![
        Chat { id: 1, project: 1, title: "Fix the greeting and add a test".into(), minutes_ago: 4, pinned: true, messages: rich, version: 0, unread: false },
        Chat { id: 2, project: 4, title: "Explain the hermes review pipeline".into(), minutes_ago: 120, pinned: false, messages: simple("Explain the hermes review pipeline", 120), version: 0, unread: true },
        Chat { id: 3, project: 1, title: "Speed up the session store".into(), minutes_ago: 300, pinned: false, messages: simple("Speed up the session store", 300), version: 0, unread: false },
        Chat { id: 4, project: 3, title: "Write a README for the CLI".into(), minutes_ago: 1500, pinned: false, messages: simple("Write a README for the CLI", 1500), version: 0, unread: false },
        Chat { id: 5, project: 1, title: "Why does cargo test hang on Windows?".into(), minutes_ago: 1800, pinned: false, messages: with_error(simple("Why does cargo test hang on Windows?", 1800)), version: 0, unread: false },
        Chat { id: 6, project: 1, title: "Migrate settings to the new schema".into(), minutes_ago: 40, pinned: false, messages: plan, version: 0, unread: false },
        Chat { id: 8, project: 1, title: "Draw how a chat reaches the model".into(), minutes_ago: 12, pinned: false, messages: drawn, version: 0, unread: false },
        Chat { id: 7, project: 2, title: "Genesis redesign: landing page".into(), minutes_ago: 20000, pinned: false, messages: simple("Genesis redesign: landing page", 20000), version: 0, unread: false },
    ];
    // The mock replies' usage, counted from their text as each turn ended.
    for c in &mut chats {
        for i in 0..c.messages.len() {
            if let Message::TurnEnd { usage: None, seconds } = c.messages[i] {
                c.messages[i] = Message::TurnEnd { seconds, usage: Some(crate::context::estimate(&c.messages[..i])) };
            }
        }
    }
    chats.sort_by_key(|c| (!c.pinned, c.minutes_ago));
    chats
}
