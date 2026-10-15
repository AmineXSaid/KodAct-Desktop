//! The command palette (Ctrl/⌘K): every action, chat, model and mode, the
//! project's files and what was said in your chats, fuzzy-filtered as you
//! type. A first `>` keeps to commands, `#` to chats, `@` to files. Arrow
//! keys (and Page Up/Down, Home/End) move, Enter runs, Esc closes; with
//! nothing found, Enter asks Forge.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, font, prelude::*, px, Action, AnimationExt, Context, Entity, FontWeight, IntoElement,
    KeyDownEvent, Render, ScrollHandle, SharedString, StyledText, Subscription, TextRun, Window,
};

use super::*;
use crate::modes::ModeId;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

enum Target {
    Action(Box<dyn Action>),
    Chat(usize),
    Model(String),
    Mode(ModeId),
    /// A project file, by its path in the project.
    File(String),
    /// A chat where the query was said: it opens with the find bar on it.
    Message(usize, String),
    /// Nothing matched: a new chat asking this.
    Ask(String),
}

/// What a query looks through, from its first character.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Scope {
    All,
    Commands,
    Chats,
    Files,
}

fn scope(query: &str) -> (Scope, &str) {
    match query.chars().next() {
        Some('>') => (Scope::Commands, &query[1..]),
        Some('#') => (Scope::Chats, &query[1..]),
        Some('@') => (Scope::Files, &query[1..]),
        _ => (Scope::All, query),
    }
}

/// How many files and chats-said-it rows a query shows at most.
const MAX_FILES: usize = 20;
const MAX_SAID: usize = 8;

struct Command {
    label: SharedString,
    group: &'static str,
    icon: &'static str,
    hint: Option<SharedString>,
    /// A quieter second part: a file's folder, the words a chat said.
    detail: Option<SharedString>,
    target: Target,
}

pub struct CommandPalette {
    state: Entity<AppState>,
    query: Entity<InputState>,
    selected: usize,
    scroll: ScrollHandle,
    /// The selection moved by key: scroll it into view on the next frame.
    reveal: bool,
    _subs: Vec<Subscription>,
}

/// Fuzzy match: every query character in order. Higher is better: matches at
/// word starts and in runs score more. Returns the matched byte positions.
pub fn fuzzy(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, vec![]));
    }
    let lower: Vec<(usize, char)> = text
        .char_indices()
        .map(|(i, c)| (i, c.to_ascii_lowercase()))
        .collect();
    let mut positions = vec![];
    let mut score = 0;
    let mut at = 0;
    let mut prev: Option<usize> = None;
    for q in query
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| !c.is_whitespace())
    {
        let found = (at..lower.len()).find(|&k| lower[k].1 == q)?;
        let word_start = found == 0 || !lower[found - 1].1.is_alphanumeric();
        score += 1
            + if word_start { 8 } else { 0 }
            + if prev == Some(found.wrapping_sub(1)) {
                5
            } else {
                0
            };
        positions.push(lower[found].0);
        prev = Some(found);
        at = found + 1;
    }
    // Shorter labels win ties.
    Some((score * 10 - text.len() as i32 / 4, positions))
}

impl CommandPalette {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search files, chats and commands…"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            // Enter is handled once, in the key capture below.
            cx.subscribe_in(&query, window, |this, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.selected = 0;
                    this.reveal = true;
                    cx.notify();
                }
            }),
        ];
        Self {
            state,
            query,
            selected: 0,
            scroll: ScrollHandle::new(),
            reveal: false,
            _subs: subs,
        }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = 0;
        self.reveal = true;
        // The files `@` completes from are the ones the palette finds.
        self.state.update(cx, |s, cx| s.ensure_completions(cx));
        self.query.update(cx, |q, cx| {
            q.set_value("", window, cx);
            q.focus(window, cx);
        });
        cx.notify();
    }

    fn commands(&self, cx: &gpui::App) -> Vec<Command> {
        let s = self.state.read(cx);
        let act = |label: &str,
                   icon: &'static str,
                   hint: Option<SharedString>,
                   action: Box<dyn Action>| Command {
            label: label.to_string().into(),
            group: "Actions",
            icon,
            hint,
            detail: None,
            target: Target::Action(action),
        };
        let mut out = vec![
            act(
                "New chat",
                "icons/plus.svg",
                Some(shortcut("N")),
                Box::new(NewChat),
            ),
            act(
                "Toggle sidebar",
                "icons/panel-left.svg",
                Some(shortcut("B")),
                Box::new(ToggleSidebar),
            ),
            act(
                if s.dark {
                    "Switch to light theme"
                } else {
                    "Switch to dark theme"
                },
                if s.dark {
                    "icons/sun.svg"
                } else {
                    "icons/moon.svg"
                },
                Some(shortcut("Shift L")),
                Box::new(ToggleTheme),
            ),
            act(
                if s.browser {
                    "Turn the web browser off"
                } else {
                    "Turn the web browser on for Forge's agents"
                },
                "icons/globe.svg",
                None,
                Box::new(ToggleBrowser),
            ),
            act(
                "Choose a model",
                "icons/cpu.svg",
                Some(shortcut("/")),
                Box::new(ToggleModelPicker),
            ),
            act(
                "Focus the composer",
                "icons/pencil.svg",
                Some(shortcut("L")),
                Box::new(FocusComposer),
            ),
            act(
                "Attach files and images",
                "icons/paperclip.svg",
                Some(shortcut("U")),
                Box::new(AttachFiles),
            ),
            act(
                "Next permission mode",
                "icons/shield-check.svg",
                Some("Shift Tab".into()),
                Box::new(CycleMode),
            ),
            act(
                "Copy the last reply",
                "icons/copy.svg",
                Some(shortcut("Shift C")),
                Box::new(CopyLastReply),
            ),
            act(
                "Projects",
                "icons/folder-open.svg",
                Some(shortcut("Shift P")),
                Box::new(OpenProjects),
            ),
            act(
                "The project map",
                "icons/map.svg",
                Some(shortcut("Shift M")),
                Box::new(ShowProjectMap),
            ),
            act(
                "Side panel: plan, files and browser",
                "icons/panel-right.svg",
                Some(shortcut("Shift E")),
                Box::new(TogglePanel),
            ),
            act(
                if s.doc_below {
                    "Put the side panel beside the chat"
                } else {
                    "Put the side panel below the chat"
                },
                "icons/split.svg",
                Some(shortcut("\\")),
                Box::new(ToggleSplit),
            ),
            act(
                "Rules and memory",
                "icons/list-todo.svg",
                Some(shortcut("Shift R")),
                Box::new(OpenRules),
            ),
            act(
                "Open settings",
                "icons/settings.svg",
                Some(shortcut(",")),
                Box::new(OpenSettings),
            ),
            act(
                "Open Slash commands…",
                "icons/square-slash.svg",
                None,
                Box::new(OpenSlashCommands),
            ),
            act("Open Agents…", "icons/bot.svg", None, Box::new(OpenAgents)),
            act(
                "Open Skills…",
                "icons/wand-sparkles.svg",
                None,
                Box::new(OpenSkills),
            ),
            act(
                "Open Hooks…",
                "icons/webhook.svg",
                None,
                Box::new(OpenHooks),
            ),
            act(
                "Open MCP servers…",
                "icons/plug.svg",
                None,
                Box::new(OpenMcpServers),
            ),
            act(
                "Open Permissions…",
                "icons/shield-check.svg",
                None,
                Box::new(OpenPermissions),
            ),
            act(
                "Open Profiles…",
                "icons/layers.svg",
                None,
                Box::new(OpenProfiles),
            ),
            act(
                "Open the Guide…",
                "icons/book-open.svg",
                None,
                Box::new(OpenGuide),
            ),
            act(
                "Set up an endpoint",
                "icons/route.svg",
                None,
                Box::new(EndpointGuide),
            ),
            act(
                "Open endpoints.json",
                "icons/file-pen.svg",
                None,
                Box::new(OpenEndpointsFile),
            ),
            act(
                "Change your name",
                "icons/pencil.svg",
                None,
                Box::new(ChangeName),
            ),
            act(
                "Change your avatar",
                "icons/sparkles.svg",
                None,
                Box::new(ChangeAvatar),
            ),
            act("Quit", "icons/x.svg", Some(shortcut("Q")), Box::new(Quit)),
        ];
        // Remote development: the same as the remote indicator's menu.
        out.extend([
            act(
                "Show or hide the terminal",
                "icons/terminal.svg",
                Some("Ctrl `".into()),
                Box::new(ToggleTerminal),
            ),
            act(
                "New terminal",
                "icons/terminal.svg",
                Some("Ctrl Shift `".into()),
                Box::new(NewTerminal),
            ),
            act(
                "Show the last command in the terminal",
                "icons/terminal.svg",
                None,
                Box::new(ShowAgentOutput),
            ),
            act(
                "Focus the terminal",
                "icons/terminal.svg",
                None,
                Box::new(FocusTerminal),
            ),
            act(
                "Clear the terminal",
                "icons/eraser.svg",
                None,
                Box::new(ClearTerminal),
            ),
            act(
                "Kill the terminal",
                "icons/circle-slash.svg",
                None,
                Box::new(KillTerminal),
            ),
            act(
                "Open Forge settings (settings.json)",
                "icons/file-pen.svg",
                None,
                Box::new(OpenAppSettings),
            ),
            act(
                "Show the remote menu",
                "icons/plug.svg",
                Some(shortcut("Alt O")),
                Box::new(OpenRemoteMenu),
            ),
            act(
                "Connect to WSL",
                "icons/terminal.svg",
                None,
                Box::new(ConnectWsl),
            ),
            act(
                "Connect to SSH host",
                "icons/server.svg",
                None,
                Box::new(ConnectHost),
            ),
            act(
                "Attach to running container",
                "icons/box.svg",
                None,
                Box::new(AttachContainer),
            ),
        ]);
        if s.has_devcontainer() {
            out.push(act(
                "Reopen in container",
                "icons/box.svg",
                None,
                Box::new(ReopenInContainer),
            ));
        }
        if s.in_devcontainer() || s.has_devcontainer() {
            out.push(act(
                "Rebuild container",
                "icons/rotate-ccw.svg",
                None,
                Box::new(RebuildContainer),
            ));
        }
        if s.in_devcontainer() {
            out.push(act(
                "Reopen folder locally",
                "icons/laptop.svg",
                None,
                Box::new(ReopenLocally),
            ));
        }
        if s.current_remote().is_some() {
            out.push(act(
                "Close remote connection",
                "icons/x.svg",
                None,
                Box::new(CloseRemote),
            ));
        }
        out.insert(
            1,
            act(
                "Keyboard shortcuts",
                "icons/keyboard.svg",
                Some("F1".into()),
                Box::new(ShowShortcuts),
            ),
        );
        if !s.pending_approvals().is_empty() {
            out.insert(
                0,
                act(
                    "Chats that need you",
                    "icons/hand.svg",
                    Some(shortcut("Shift A")),
                    Box::new(ShowApprovals),
                ),
            );
        }
        // The chat showing: what you can do with it.
        if let Some(c) = s.chat().filter(|c| !c.messages.is_empty()) {
            let mut chat = vec![
                act(
                    "Find in this chat",
                    "icons/search.svg",
                    Some(shortcut("F")),
                    Box::new(ToggleFind),
                ),
                act(
                    "Jump to the latest message",
                    "icons/arrow-down.svg",
                    Some(shortcut("J")),
                    Box::new(JumpToLatest),
                ),
                act(
                    "Rename this chat",
                    "icons/pencil.svg",
                    Some("F2".into()),
                    Box::new(RenameChat),
                ),
                act(
                    "Duplicate this chat",
                    "icons/copy-plus.svg",
                    None,
                    Box::new(DuplicateChat),
                ),
                act(
                    "Export this chat as Markdown",
                    "icons/archive.svg",
                    None,
                    Box::new(ExportChat),
                ),
                act(
                    "Export this chat as JSON",
                    "icons/archive.svg",
                    None,
                    Box::new(ExportChatJson),
                ),
                act(
                    "Share this chat (copy as JSON)",
                    "icons/copy.svg",
                    None,
                    Box::new(ShareChat),
                ),
                act(
                    "Show the context window",
                    "icons/cpu.svg",
                    None,
                    Box::new(ShowContext),
                ),
                act(
                    "Show this turn's tasks",
                    "icons/list-todo.svg",
                    Some(shortcut("Shift T")),
                    Box::new(ToggleTasks),
                ),
                act(
                    "Delete this chat",
                    "icons/trash.svg",
                    None,
                    Box::new(DeleteChat),
                ),
            ];
            if !s.busy()
                && c.messages
                    .iter()
                    .any(|m| matches!(m, crate::state::Message::User { .. }))
            {
                chat.insert(
                    2,
                    act(
                        "Regenerate the last reply",
                        "icons/rotate-ccw.svg",
                        None,
                        Box::new(RegenerateReply),
                    ),
                );
            }
            if s.can_continue() {
                chat.insert(
                    0,
                    act(
                        "Continue the stopped reply",
                        "icons/play.svg",
                        None,
                        Box::new(ContinueReply),
                    ),
                );
            }
            if s.busy() {
                chat.insert(
                    0,
                    act(
                        "Stop the reply",
                        "icons/square.svg",
                        Some("Esc".into()),
                        Box::new(StopReply),
                    ),
                );
            }
            // First when nothing is typed: they are about what you're looking at.
            let chat: Vec<Command> = chat
                .into_iter()
                .map(|cmd| Command {
                    group: "This chat",
                    ..cmd
                })
                .collect();
            out.splice(0..0, chat);
        }
        for m in ModeId::ALL
            .into_iter()
            .filter(|m| *m != ModeId::Bypass || s.bypass_allowed)
        {
            out.push(Command {
                label: format!("Mode: {}", m.label()).into(),
                group: "Modes",
                icon: m.icon(),
                hint: (m == s.mode).then(|| "current".into()),
                detail: None,
                target: Target::Mode(m),
            });
        }
        let current = s.active_profile().map(|p| p.name.clone());
        for row in super::model_menu::rows(s, "").0 {
            if row.disabled {
                continue;
            }
            let hint = (current.as_deref() == Some(row.name.as_str())).then(|| "current".into());
            out.push(Command {
                label: format!("Model: {} · {}", row.display, row.name).into(),
                group: "Models",
                icon: "icons/cpu.svg",
                hint,
                detail: None,
                target: Target::Model(row.name),
            });
        }
        for c in &s.chats {
            out.push(Command {
                label: c.title.clone(),
                group: "Chats",
                icon: "icons/message-square.svg",
                hint: Some(c.group().into()),
                detail: None,
                target: Target::Chat(c.id),
            });
        }
        out
    }

    /// The commands that match, best first, with their matched positions.
    /// While typing, each group stays together, the group with the best
    /// match first.
    fn matches(&self, cx: &gpui::App) -> Vec<(Command, Vec<usize>)> {
        let raw = self.query.read(cx).value().to_string();
        let (scope, q) = scope(&raw);
        let q = q.trim();
        let mut scored: Vec<(i32, Command, Vec<usize>)> = self
            .commands(cx)
            .into_iter()
            .filter(|c| match scope {
                Scope::All => true,
                Scope::Commands => c.group != "Chats",
                Scope::Chats => c.group == "Chats",
                Scope::Files => false,
            })
            .filter_map(|c| fuzzy(q, &c.label).map(|(score, pos)| (score, c, pos)))
            .collect();
        if !q.is_empty() {
            let s = self.state.read(cx);
            if matches!(scope, Scope::All | Scope::Files) {
                scored.extend(file_matches(s, q, cx.theme()));
            }
            if matches!(scope, Scope::All | Scope::Chats) && q.chars().count() >= 2 {
                scored.extend(said_matches(s, q));
            }
            // Groups together, best group first; within one, best first.
            let mut best: std::collections::HashMap<&'static str, i32> = Default::default();
            for (score, c, _) in &scored {
                let b = best.entry(c.group).or_insert(i32::MIN);
                *b = (*b).max(*score);
            }
            scored.sort_by(|a, b| {
                best[b.1.group]
                    .cmp(&best[a.1.group])
                    .then(a.1.group.cmp(b.1.group))
                    .then(b.0.cmp(&a.0))
            });
            if scored.is_empty() && scope != Scope::Files {
                scored.push((
                    0,
                    Command {
                        label: format!("Ask Forge: {q}").into(),
                        group: "Nothing matches",
                        icon: "icons/message-square.svg",
                        hint: Some("Enter".into()),
                        detail: Some("Starts a new chat".into()),
                        target: Target::Ask(q.to_string()),
                    },
                    vec![],
                ));
            }
        }
        scored
            .into_iter()
            .map(|(_, c, p)| (c, p))
            .take(80)
            .collect()
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.palette_open = false;
            cx.notify();
        });
    }

    fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut matches = self.matches(cx);
        if matches.is_empty() {
            return;
        }
        let (cmd, _) = matches.swap_remove(self.selected.min(matches.len() - 1));
        self.close(cx);
        window.dispatch_action(Box::new(FocusComposer), cx);
        match cmd.target {
            Target::Action(a) => window.dispatch_action(a, cx),
            Target::Chat(id) => self.state.update(cx, |s, cx| s.open_chat(id, cx)),
            Target::Model(name) => self.state.update(cx, |s, cx| s.select_profile(&name, cx)),
            Target::Mode(m) => self.state.update(cx, |s, cx| s.set_mode(m, cx)),
            Target::File(rel) => self.state.update(cx, |s, cx| s.open_project_file(rel, cx)),
            Target::Message(id, q) => {
                self.state.update(cx, |s, cx| {
                    s.open_chat(id, cx);
                    s.find_request = Some(q);
                });
                window.dispatch_action(Box::new(ToggleFind), cx);
            }
            Target::Ask(q) => self.state.update(cx, |s, cx| {
                s.new_chat(cx);
                s.send(q, cx);
            }),
        }
    }
}

/// The project's files that match, best first: on the file's name when it
/// can, else on its whole path.
fn file_matches(s: &AppState, q: &str, t: &Theme) -> Vec<(i32, Command, Vec<usize>)> {
    let Some(files) = s.project_file_index() else {
        return vec![];
    };
    let mut out: Vec<(i32, Command, Vec<usize>)> = files
        .iter()
        .filter(|f| !f.ends_with('/'))
        .filter_map(|f| {
            let (dir, name) = f.rsplit_once('/').map_or(("", f.as_str()), |(d, n)| (d, n));
            let lower = q.to_lowercase();
            let (score, pos) = match fuzzy(q, name).filter(|(_, pos)| runs(pos) <= 3) {
                // A hit on the name beats one on the path; the name holding
                // the query whole beats both.
                Some((score, pos)) => {
                    let whole = name.to_lowercase().contains(&lower);
                    (score + 40 + if whole { 60 } else { 0 }, pos)
                }
                // On the path, only the query whole: letters strewn over a
                // long path match almost anything.
                None if f.to_lowercase().contains(&lower) => (fuzzy(q, f)?.0, vec![]),
                None => return None,
            };
            Some((
                score,
                Command {
                    label: name.to_string().into(),
                    group: "Files",
                    icon: super::file_icon::for_name(name, false, t).0,
                    hint: None,
                    detail: (!dir.is_empty()).then(|| dir.to_string().into()),
                    target: Target::File(f.clone()),
                },
                pos,
            ))
        })
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.0));
    out.truncate(MAX_FILES);
    out
}

/// How many separate runs the matched positions make: "readme" in
/// "README.md" is one, strewn over "forge-ui-upgrade.md" it is several.
fn runs(positions: &[usize]) -> usize {
    positions.windows(2).filter(|w| w[1] != w[0] + 1).count() + usize::from(!positions.is_empty())
}

/// Chats where `q` was said (your words or Forge's), newest chat first, with
/// the words around it.
fn said_matches(s: &AppState, q: &str) -> Vec<(i32, Command, Vec<usize>)> {
    let needle = q.to_lowercase();
    let mut out = vec![];
    for c in &s.chats {
        let said = c.messages.iter().rev().find_map(|m| match m {
            crate::state::Message::User { text, .. } => snippet(text, &needle),
            crate::state::Message::Assistant { blocks } => blocks.iter().find_map(|b| match b {
                crate::state::Block::Text(t) => snippet(t, &needle),
                _ => None,
            }),
            _ => None,
        });
        if let Some(said) = said {
            out.push((
                // Below a title that matches; the newest chat first.
                -1 - out.len() as i32,
                Command {
                    label: c.title.clone(),
                    group: "Said in chats",
                    icon: "icons/message-square.svg",
                    hint: None,
                    detail: Some(said.into()),
                    target: Target::Message(c.id, q.to_string()),
                },
                vec![],
            ));
            if out.len() == MAX_SAID {
                break;
            }
        }
    }
    out
}

/// The words around the first place `text` holds `needle` (lower-case), on
/// one line.
fn snippet(text: &str, needle: &str) -> Option<String> {
    let at = text.to_lowercase().find(needle)?;
    // Back to characters: lower-casing can change byte lengths.
    let lower = text.to_lowercase();
    let at = lower[..at].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let from = at.saturating_sub(24);
    let to = (at + needle.chars().count() + 48).min(chars.len());
    let mut out: String = chars[from..to]
        .iter()
        .map(|c| if c.is_whitespace() { ' ' } else { *c })
        .collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if from > 0 {
        out.insert(0, '…');
    }
    if to < chars.len() {
        out.push('…');
    }
    Some(out)
}

pub fn highlighted(label: &SharedString, positions: &[usize], t: &Theme) -> StyledText {
    let mut runs = vec![];
    let mut i = 0;
    for (start, ch) in label.char_indices() {
        if start < i {
            continue;
        }
        let hit = positions.contains(&start);
        let len = ch.len_utf8();
        let mut f = font(t.font_sans.clone());
        if hit {
            f.weight = FontWeight::SEMIBOLD;
        }
        runs.push(TextRun {
            len,
            font: f,
            color: if hit {
                t.colors.text_strong
            } else {
                t.colors.text
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        i = start + len;
    }
    StyledText::new(label.clone()).with_runs(runs)
}

impl Render for CommandPalette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let matches = self.matches(cx);
        let selected = self.selected.min(matches.len().saturating_sub(1));
        let count = matches.len();

        let mut list = div()
            .id("palette-list")
            .flex()
            .flex_col()
            .max_h(px(380.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(t.space.xs);
        let mut last_group = "";
        // The list's children: group labels as well as rows.
        let mut child = 0;
        let mut selected_child = 0;
        for (i, (cmd, pos)) in matches.iter().enumerate() {
            if cmd.group != last_group {
                last_group = cmd.group;
                child += 1;
                list = list.child(
                    div()
                        .px(t.space.md)
                        .pt(t.space.sm)
                        .pb(t.space.xs)
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_subtle)
                        .child(cmd.group),
                );
            }
            let on = i == selected;
            if on {
                selected_child = child;
            }
            child += 1;
            let row = div()
                .id(("cmd", i))
                .flex()
                .items_center()
                .gap(t.space.md)
                .h(t.size.row + px(2.))
                .px(t.space.md)
                .rounded(t.radius.button)
                .cursor_pointer()
                .when(on, |d| d.bg(t.colors.hover))
                .child(icon(
                    cmd.icon,
                    t.size.icon_sm,
                    if on {
                        t.colors.text
                    } else {
                        t.colors.text_muted
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(highlighted(&cmd.label, pos, &t))
                        .children(cmd.detail.clone().map(|d| {
                            div()
                                .ml(t.space.sm)
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(d)
                        }))
                        .flex()
                        .items_baseline(),
                )
                .children(cmd.hint.clone().map(|h| kbd(&t, h)))
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if this.selected != i {
                        this.selected = i;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(|this, _, window, cx| this.run(window, cx)));
            list = list.child(super::motion::row_in(row, ("cmd-in", i), i, &t));
        }
        if count == 0 {
            list = list.child(
                div()
                    .px(t.space.md)
                    .py(t.space.lg)
                    .text_color(t.colors.text_subtle)
                    .child("Nothing matches."),
            );
        }
        if std::mem::take(&mut self.reveal) {
            self.scroll.scroll_to_item(selected_child);
        }

        let fade = std::time::Duration::from_millis(t.motion_ms);
        div()
            .id("palette-scrim")
            .absolute()
            .inset_0()
            .bg(t.extra.scrim)
            .flex()
            .justify_center()
            .items_start()
            .pt(t.size.titlebar * 3.)
            .key_context("CommandPalette")
            .capture_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "down" if count > 0 => this.selected = (this.selected + 1) % count,
                    "up" if count > 0 => this.selected = (this.selected + count - 1) % count,
                    "pagedown" if count > 0 => this.selected = (this.selected + 8).min(count - 1),
                    "pageup" => this.selected = this.selected.saturating_sub(8),
                    "home" if ev.keystroke.modifiers.secondary() => this.selected = 0,
                    "end" if ev.keystroke.modifiers.secondary() && count > 0 => {
                        this.selected = count - 1
                    }
                    "escape" => {
                        this.close(cx);
                        window.dispatch_action(Box::new(FocusComposer), cx);
                    }
                    "enter" => this.run(window, cx),
                    _ => return,
                }
                this.reveal = true;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                this.close(cx);
                window.dispatch_action(Box::new(FocusComposer), cx);
            }))
            .child(super::motion::edge_draw(
                theme::popover(&t)
                    .id("palette")
                    .w(t.size.palette)
                    .h_auto()
                    .p_0()
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .px(t.space.lg)
                            .py(t.space.sm)
                            .border_b_1()
                            .border_color(t.colors.border)
                            .text_size(t.text.body)
                            .child(icon("icons/search.svg", t.size.icon, t.colors.text_subtle))
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(&self.query).appearance(false)),
                            ),
                    )
                    .child(list)
                    .child(
                        div()
                            .flex()
                            .gap(t.space.lg)
                            .px(t.space.lg)
                            .py(t.space.sm)
                            .border_t_1()
                            .border_color(t.colors.border)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child("↑↓ choose")
                            .child("Enter run")
                            .child("Esc close")
                            .child(div().flex_1())
                            .child("> commands  # chats  @ files")
                            .child(format!(
                                "{count} result{}",
                                if count == 1 { "" } else { "s" }
                            )),
                    )
                    .relative(),
                "palette-in",
                t.colors.primary,
                0.,
                &t,
            ))
            .with_animation(
                "palette-scrim-in",
                gpui::Animation::new(fade).with_easing(gpui::ease_in_out),
                |d, delta| d.opacity(delta),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn matched_letters_count_their_runs() {
        assert_eq!(super::runs(&[]), 0);
        assert_eq!(super::runs(&[0, 1, 2]), 1);
        assert_eq!(super::runs(&[0, 1, 5, 6, 9]), 3);
    }

    #[test]
    fn a_snippet_is_the_words_around_the_hit_on_one_line() {
        let text = "First line.\nThe chat pipeline starts in send_to_forge and ends in on_event, where every event the process sends lands, one by one, in order.";
        let s = super::snippet(text, "send_to_forge").unwrap();
        assert!(s.contains("send_to_forge") && !s.contains('\n'), "{s}");
        assert!(s.starts_with('…') && s.ends_with('…'), "{s}");
        assert_eq!(super::snippet(text, "absent"), None);
    }

    #[test]
    fn a_first_character_narrows_the_search() {
        use super::{scope, Scope};
        assert_eq!(scope(">theme"), (Scope::Commands, "theme"));
        assert_eq!(scope("#pipeline"), (Scope::Chats, "pipeline"));
        assert_eq!(scope("@main.rs"), (Scope::Files, "main.rs"));
        assert_eq!(scope("theme"), (Scope::All, "theme"));
    }

    #[test]
    fn fuzzy_matches_in_order_and_prefers_word_starts() {
        assert!(fuzzy("nc", "New chat").is_some());
        assert!(fuzzy("cn", "New chat").is_none());
        let (word, _) = fuzzy("tt", "Toggle theme").unwrap();
        let (mid, _) = fuzzy("tt", "Settings").unwrap();
        assert!(word > mid, "word starts win: {word} vs {mid}");
        assert_eq!(fuzzy("ch", "New chat").unwrap().1, vec![4, 5]);
        assert_eq!(fuzzy("", "anything").unwrap().1, Vec::<usize>::new());
    }
}
