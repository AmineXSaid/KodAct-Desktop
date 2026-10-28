//! The command palette (Ctrl/⌘K): every action, chat, model and mode, the
//! project's files and what was said in your chats, fuzzy-filtered as you
//! type. You type in the title bar's search field; the results drop below
//! it, under tabs for each scope with how many each finds. A first `>` keeps
//! to commands, `#` to chats, `@` to files, `%` to what was said, and Tab
//! moves between them. Arrow keys (and Page Up/Down, Home/End) move, Enter
//! runs, Esc closes; with nothing found, Enter asks Forge.

use gpui::component::input::{self, Input, InputEvent, InputState};
use gpui::component::Sizable as _;
use gpui::{
    div, font, prelude::*, px, Action, AnimationExt, AnyElement, Context, CursorStyle, Div, Entity,
    FontWeight, IntoElement, Render, ScrollHandle, SharedString, StyledText,
    Subscription, TextRun, Window,
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

/// What a query looks through, from its first character: the tabs.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Scope {
    All,
    Commands,
    Chats,
    Files,
    Said,
}

impl Scope {
    const TABS: [Scope; 5] = [
        Scope::All,
        Scope::Commands,
        Scope::Chats,
        Scope::Files,
        Scope::Said,
    ];

    fn prefix(self) -> &'static str {
        match self {
            Scope::All => "",
            Scope::Commands => ">",
            Scope::Chats => "#",
            Scope::Files => "@",
            Scope::Said => "%",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Scope::All => "All",
            Scope::Commands => "Commands",
            Scope::Chats => "Chats",
            Scope::Files => "Files",
            Scope::Said => "Said in chats",
        }
    }

    fn icon(self) -> Option<&'static str> {
        match self {
            Scope::All => None,
            Scope::Commands => Some("icons/terminal.svg"),
            Scope::Chats => Some("icons/hash.svg"),
            Scope::Files => Some("icons/at-sign.svg"),
            Scope::Said => Some("icons/text-quote.svg"),
        }
    }

    /// The tab a row's group belongs to.
    fn of(group: &str) -> Scope {
        match group {
            "Chats" => Scope::Chats,
            "Files" => Scope::Files,
            "Said in chats" => Scope::Said,
            "Nothing matches" => Scope::All,
            _ => Scope::Commands,
        }
    }

    fn index(self) -> usize {
        Scope::TABS.iter().position(|s| *s == self).unwrap_or(0)
    }
}

fn scope(query: &str) -> (Scope, &str) {
    match query.chars().next() {
        Some('>') => (Scope::Commands, &query[1..]),
        Some('#') => (Scope::Chats, &query[1..]),
        Some('@') => (Scope::Files, &query[1..]),
        Some('%') => (Scope::Said, &query[1..]),
        _ => (Scope::All, query),
    }
}

/// How many files and chats-said-it rows a query shows at most.
const MAX_FILES: usize = 20;
const MAX_SAID: usize = 8;
/// In All, each group shows this many rows; its tab shows the rest.
const PER_GROUP: usize = 3;
/// How many recently run rows the empty palette starts with.
const MAX_RECENT: usize = 5;

/// What the palette finds for the query.
struct Found {
    scope: Scope,
    /// The query has words (beyond its scope prefix).
    typed: bool,
    rows: Vec<(Command, Vec<usize>)>,
    /// Per tab, how many rows the query finds there.
    counts: [usize; 5],
    /// The groups All shows cut short, with how many each has.
    cut: Vec<(&'static str, usize)>,
}

/// Something run from the palette, to offer again first.
#[derive(Clone, PartialEq)]
enum Recent {
    /// An action, chat, model or mode, by its label.
    Label(SharedString),
    File(String),
}

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
    /// Newest first.
    recent: Vec<Recent>,
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
            recent: vec![],
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
        ];
        // One row per theme; the current one is ticked.
        let current = cx.theme().look;
        for look in crate::theme::Look::ALL {
            let action: Box<dyn Action> = match look {
                crate::theme::Look::Calm => Box::new(ThemeCalm),
                crate::theme::Look::Terracotta => Box::new(ThemeTerracotta),
                crate::theme::Look::Patina => Box::new(ThemePatina),
                crate::theme::Look::Ink => Box::new(ThemeInk),
                crate::theme::Look::Slate => Box::new(ThemeSlate),
                crate::theme::Look::Plum => Box::new(ThemePlum),
            };
            out.push(act(
                &format!("Theme: {} · {}", look.name(), look.blurb()),
                if look == current {
                    "icons/check.svg"
                } else {
                    "icons/sparkles.svg"
                },
                None,
                action,
            ));
        }
        out.extend([
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
                &format!(
                    "{}: {} → {}",
                    crate::i18n::tr("Language"),
                    crate::i18n::lang().name(),
                    next_lang().name()
                ),
                "icons/globe.svg",
                None,
                Box::new(NextLanguage),
            ),
            act(
                if s.pet.is_some() { "Pet: the next one" } else { "Pet: a companion beside the chat" },
                "icons/sparkles.svg",
                None,
                Box::new(NextPet),
            ),
            act(
                "Design: templates, design systems and skills",
                "icons/pen-tool.svg",
                Some(shortcut("Shift D")),
                Box::new(OpenDesign),
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
        ]);
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
        // Connected to a host: another folder there, still connected.
        if let Some(r) = s.current_remote() {
            out.push(act(
                &format!("Open another folder on {}…", r.target.label()),
                "icons/folder-open.svg",
                None,
                Box::new(ChangeRemoteFolder),
            ));
        }
        // The file beside the chat: find in it, page through it, export it.
        if let Some(f) = s.doc.as_ref() {
            out.push(act(
                "Find in the open file",
                "icons/search.svg",
                Some(shortcut("Shift F")),
                Box::new(FindInFile),
            ));
            if s.doc_page_count() > 1 {
                out.push(act(
                    "Next page or slide",
                    "icons/chevron-down.svg",
                    Some("Alt ↓".into()),
                    Box::new(NextPage),
                ));
                out.push(act(
                    "Previous page or slide",
                    "icons/chevron-up.svg",
                    Some("Alt ↑".into()),
                    Box::new(PreviousPage),
                ));
            }
            if matches!(
                f.office(),
                Some(crate::office::Kind::Docx | crate::office::Kind::Pptx | crate::office::Kind::Xlsx)
            ) {
                out.push(act(
                    "Export the open file to PDF",
                    "icons/file-down.svg",
                    None,
                    Box::new(ExportPdf),
                ));
            }
        }
        // Design chats: the design prompt and tools (`design::prompt`).
        if crate::design::root().is_some() {
            use crate::design::prompt::Surface;
            let now = s.design_setup().map(|d| d.surface);
            let kinds: [(Surface, &str, Box<dyn Action>); 4] = [
                (Surface::Any, "Design chat: any design", Box::new(DesignChat)),
                (Surface::Deck, "Design chat: a deck", Box::new(DesignDeck)),
                (Surface::Prototype, "Design chat: a prototype", Box::new(DesignPrototype)),
                (Surface::Document, "Design chat: a document", Box::new(DesignDocument)),
            ];
            for (surface, label, action) in kinds {
                if now != Some(surface) {
                    out.push(act(label, "icons/sparkles.svg", None, action));
                }
            }
            if now.is_some() {
                out.push(act("End the design chat", "icons/x.svg", None, Box::new(EndDesignChat)));
            }
        }
        // A deck in the browser, with no document open: its slides.
        if s.doc.is_none() && s.web_page.deck.is_some() {
            out.push(act("Next slide", "icons/chevron-down.svg", Some("Alt ↓".into()), Box::new(NextPage)));
            out.push(act("Previous slide", "icons/chevron-up.svg", Some("Alt ↑".into()), Box::new(PreviousPage)));
        }
        // The browser's page: zoom it, present it, save it; and the design's.
        if s.web.is_some() {
            out.extend([
                act("Zoom the page in", "icons/zoom-in.svg", Some(shortcut("=")), Box::new(ZoomIn)),
                act("Zoom the page out", "icons/zoom-out.svg", Some(shortcut("-")), Box::new(ZoomOut)),
                act(
                    "The page at its own size (100%)",
                    "icons/search.svg",
                    Some(shortcut("0")),
                    Box::new(ZoomReset),
                ),
                act("Zoom the page to a desktop width", "icons/scan.svg", None, Box::new(ZoomFit)),
                act(
                    "Present the page: it fills the window",
                    "icons/play.svg",
                    Some("F11".into()),
                    Box::new(Present),
                ),
                act("Save the page as PDF", "icons/file-down.svg", None, Box::new(SavePagePdf)),
                act("Save the page as PNG", "icons/image.svg", None, Box::new(SavePagePng)),
                act("Save the deck as PowerPoint", "icons/presentation.svg", None, Box::new(SavePagePptx)),
                act("Save the deck as editable PowerPoint", "icons/file-pen.svg", None, Box::new(SavePagePptxEditable)),
                act("Save the page as one HTML file", "icons/file-code.svg", None, Box::new(SavePageHtml)),
                act("Save the page for Figma (editable layers)", "icons/layers.svg", None, Box::new(SavePageFigma)),
            ]);
        }
        if s.pet.is_some() {
            out.push(act("Pet: hide it", "icons/x.svg", None, Box::new(HidePet)));
        }
        if s.design_on_screen().is_some() {
            out.push(act(
                "Review the page: craft, brief, brand, accessibility, copy",
                "icons/stethoscope.svg",
                None,
                Box::new(CritiquePage),
            ));
            out.push(act("Ask for a new page", "icons/plus.svg", None, Box::new(NewDesignPage)));
            out.push(act(
                if s.design_select {
                    "Put the comment tool down"
                } else {
                    "Comment on part of the page"
                },
                "icons/mouse-pointer-2.svg",
                None,
                Box::new(CommentTool),
            ));
            out.extend([
                act(
                    if s.design_outline {
                        "Hide the page's outline"
                    } else {
                        "The page's outline: its headings"
                    },
                    "icons/panel-left.svg",
                    None,
                    Box::new(PageOutline),
                ),
                act("Undo the page's last change", "icons/undo-2.svg", None, Box::new(UndoPage)),
                act("Redo the page's change", "icons/redo-2.svg", None, Box::new(RedoPage)),
                act("Duplicate the page", "icons/copy.svg", None, Box::new(DuplicatePage)),
            ]);
        }
        if s.web.is_some() {
            out.push(act(
                if s.design_guides {
                    "Take the layout guides off the page"
                } else {
                    "Layout guides: outline every box on the page"
                },
                "icons/layout-grid.svg",
                None,
                Box::new(LayoutGuides),
            ));
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
                    "icons/file-text.svg",
                    None,
                    Box::new(ExportChat),
                ),
                act(
                    "Export this chat as JSON",
                    "icons/file-json.svg",
                    None,
                    Box::new(ExportChatJson),
                ),
                act(
                    "Share this chat (copy as JSON)",
                    "icons/share-2.svg",
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
            if s.agents_running() {
                chat.insert(
                    0,
                    act(
                        "Stop all agents",
                        "icons/square.svg",
                        Some(
                            crate::ui::shortcuts::keys("secondary-shift-x")
                                .join(" ")
                                .into(),
                        ),
                        Box::new(StopAgents),
                    ),
                );
            }
            // Messages sent while Forge works, not yet with it.
            if s.current.and_then(|c| s.queued.get(&c)).is_some_and(|q| !q.is_empty()) {
                chat.insert(
                    0,
                    act(
                        "Remove the queued messages",
                        "icons/x.svg",
                        None,
                        Box::new(RemoveQueued),
                    ),
                );
                chat.insert(
                    0,
                    act(
                        "Edit the last queued message",
                        "icons/pencil.svg",
                        None,
                        Box::new(EditQueued),
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
    fn search(&self, cx: &gpui::App) -> Found {
        let raw = self.query.read(cx).value().to_string();
        let (scope, q) = scope(&raw);
        let q = q.trim();
        let s = self.state.read(cx);
        let t = cx.theme();
        // Everything the query finds, in every scope: the tabs count it.
        let mut scored: Vec<(i32, Command, Vec<usize>)> = self
            .commands(cx)
            .into_iter()
            .filter_map(|c| fuzzy(q, &c.label).map(|(score, pos)| (score, c, pos)))
            .collect();
        // With nothing typed, Files lists the project's first files.
        if !q.is_empty() || scope == Scope::Files {
            scored.extend(file_matches(s, q, t));
        }
        if q.chars().count() >= 2 {
            scored.extend(said_matches(s, q));
        }
        let mut counts = [0; 5];
        for (_, c, _) in &scored {
            counts[Scope::of(c.group).index()] += 1;
        }
        counts[0] = counts.iter().sum();
        scored.retain(|(_, c, _)| scope == Scope::All || Scope::of(c.group) == scope);

        if !q.is_empty() {
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
        } else if scope == Scope::All && !self.recent.is_empty() {
            // Nothing typed: what you ran lately comes first.
            let mut recent: Vec<(i32, Command, Vec<usize>)> = vec![];
            for r in &self.recent {
                let found = match r {
                    Recent::Label(label) => scored
                        .iter()
                        .position(|(_, c, _)| &c.label == label)
                        .map(|i| scored.remove(i)),
                    Recent::File(path) => Some((0, file_command(path, t), vec![])),
                };
                if let Some((score, mut c, pos)) = found {
                    c.group = "Recently used";
                    recent.push((score, c, pos));
                }
            }
            recent.append(&mut scored);
            scored = recent;
        }

        // All shows a few rows a group; the group's tab has the rest.
        let mut cut: Vec<(&'static str, usize)> = vec![];
        if scope == Scope::All {
            let mut seen: Vec<(&'static str, usize)> = vec![];
            scored.retain(|(_, c, _)| {
                if matches!(c.group, "Recently used" | "Nothing matches") {
                    return true;
                }
                match seen.iter_mut().find(|(g, _)| *g == c.group) {
                    Some((_, n)) => {
                        *n += 1;
                        *n <= PER_GROUP
                    }
                    None => {
                        seen.push((c.group, 1));
                        true
                    }
                }
            });
            cut = seen.into_iter().filter(|(_, n)| *n > PER_GROUP).collect();
        }

        if scored.is_empty() && !q.is_empty() && scope != Scope::Files {
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
        Found {
            scope,
            typed: !q.is_empty(),
            rows: scored
                .into_iter()
                .map(|(_, c, p)| (c, p))
                .take(80)
                .collect(),
            counts,
            cut,
        }
    }

    /// Switch tab: the query keeps its words under the new scope's prefix.
    fn set_scope(&mut self, to: Scope, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.query.read(cx).value().to_string();
        let (_, words) = scope(&raw);
        let value = format!("{}{}", to.prefix(), words);
        self.query.update(cx, |q, cx| {
            q.set_value(value, window, cx);
            q.focus(window, cx);
        });
        self.selected = 0;
        self.reveal = true;
        cx.notify();
    }

    /// A key while the field has focus, by name. Returns whether it was the
    /// palette's.
    fn key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let count = self.search(cx).rows.len();
        match key {
            "down" if count > 0 => self.selected = (self.selected + 1) % count,
            "up" if count > 0 => self.selected = (self.selected + count - 1) % count,
            "pagedown" if count > 0 => self.selected = (self.selected + 8).min(count - 1),
            "pageup" => self.selected = self.selected.saturating_sub(8),
            "start" => self.selected = 0,
            "end" if count > 0 => self.selected = count - 1,
            "tab" | "shift-tab" => {
                let raw = self.query.read(cx).value().to_string();
                let at = scope(&raw).0.index();
                let n = Scope::TABS.len();
                let to = if key == "shift-tab" {
                    (at + n - 1) % n
                } else {
                    (at + 1) % n
                };
                self.set_scope(Scope::TABS[to], window, cx);
            }
            "escape" => {
                self.close(cx);
                window.dispatch_action(Box::new(FocusComposer), cx);
            }
            "enter" => self.run(window, cx),
            _ => return false,
        }
        self.reveal = true;
        cx.notify();
        true
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.palette_open = false;
            cx.notify();
        });
    }

    fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut matches = self.search(cx).rows;
        if matches.is_empty() {
            return;
        }
        let (cmd, _) = matches.swap_remove(self.selected.min(matches.len() - 1));
        let recent = match &cmd.target {
            Target::File(path) => Some(Recent::File(path.clone())),
            Target::Message(..) | Target::Ask(_) => None,
            _ => Some(Recent::Label(cmd.label.clone())),
        };
        if let Some(r) = recent {
            self.recent.retain(|x| *x != r);
            self.recent.insert(0, r);
            self.recent.truncate(MAX_RECENT);
        }
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
            let name = f.rsplit_once('/').map_or(f.as_str(), |(_, n)| n);
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
            Some((score, file_command(f, t), pos))
        })
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.0));
    out.truncate(MAX_FILES);
    out
}

/// A project file's row: its name, then its folder, quieter.
fn file_command(path: &str, t: &Theme) -> Command {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    Command {
        label: name.to_string().into(),
        group: "Files",
        icon: super::file_icon::for_name(name, false, t).0,
        hint: None,
        detail: (!dir.is_empty()).then(|| dir.to_string().into()),
        target: Target::File(path.to_string()),
    }
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
                t.colors.primary
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

impl CommandPalette {
    /// The title bar's search field. Closed, it shows what it searches and
    /// opens on a click; open, you type in it and the results drop below.
    pub fn field(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let open = self.state.read(cx).palette_open;
        // Every control in the title bar occludes it (see `title_bar`).
        let field = div()
            .id("palette-field")
            .occlude()
            .relative()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .h(t.size.control)
            .px(t.space.md)
            .min_w(px(240.))
            .rounded(t.radius.field)
            .border_1()
            .text_size(t.text.body)
            .child(icon(
                "icons/search.svg",
                t.size.icon_sm,
                if open {
                    t.colors.text_muted
                } else {
                    t.colors.text_subtle
                },
            ));
        if !open {
            let (edge, ink, fill) = (t.colors.border, t.colors.text_muted, t.colors.surface);
            return field
                .bg(t.colors.hover)
                .border_color(t.colors.hover)
                .cursor(CursorStyle::IBeam)
                .text_color(t.colors.text_subtle)
                .hover(move |st| st.bg(fill).border_color(edge).text_color(ink))
                .child(
                    div()
                        .flex_1()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .child("Search files, chats and commands"),
                )
                .child(keys(&t, &shortcut("K")))
                .on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(ToggleCommandPalette), cx)
                })
                .into_any_element();
        }
        let dropdown = self.dropdown(window, cx);
        field
            .w(t.size.palette)
            .bg(t.colors.surface)
            .border_color(t.colors.primary)
            .key_context("CommandPalette")
            // The text box binds these keys to its own actions, which run
            // before any key listener: here they move through the results.
            .capture_action(nav::<input::MoveUp>("up", cx))
            .capture_action(nav::<input::MoveDown>("down", cx))
            .capture_action(nav::<input::MovePageUp>("pageup", cx))
            .capture_action(nav::<input::MovePageDown>("pagedown", cx))
            .capture_action(nav::<input::MoveToStart>("start", cx))
            .capture_action(nav::<input::MoveToEnd>("end", cx))
            .capture_action(nav::<input::IndentInline>("tab", cx))
            .capture_action(nav::<input::OutdentInline>("shift-tab", cx))
            .capture_action(nav::<input::Escape>("escape", cx))
            .capture_action(
                cx.listener(|this, e: &input::Enter, window, cx| {
                    if !e.secondary && !e.shift && this.key("enter", window, cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.query).appearance(false).small()),
            )
            // Above the scrim and the page under the title bar.
            .child(gpui::deferred(dropdown).with_priority(2))
            .into_any_element()
    }

    /// The tabs, the results and the keys, hung below the field.
    fn dropdown(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let found = self.search(cx);
        let count = found.rows.len();
        let selected = self.selected.min(count.saturating_sub(1));

        let mut tabs = div()
            .flex()
            .items_end()
            .gap(px(2.))
            .px(t.space.sm)
            .pt(t.space.xs)
            .border_b_1()
            .border_color(t.colors.border);
        for (i, tab) in Scope::TABS.into_iter().enumerate() {
            let on = tab == found.scope;
            tabs = tabs.child(
                div()
                    .id(("palette-tab", i))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .px(t.space.md)
                    .pt(t.space.sm)
                    .pb(t.space.sm)
                    .border_b_2()
                    .border_color(if on {
                        t.colors.primary
                    } else {
                        gpui::transparent_black()
                    })
                    .text_size(t.text.caption)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                        } else {
                            let ink = t.colors.text;
                            d.text_color(t.colors.text_muted)
                                .hover(move |st| st.text_color(ink))
                        }
                    })
                    .children(tab.icon().map(|p| {
                        icon(
                            p,
                            t.size.icon_xs,
                            if on {
                                t.colors.primary
                            } else {
                                t.colors.text_subtle
                            },
                        )
                    }))
                    .child(tab.label())
                    .when(found.typed, |d| {
                        d.child(
                            div()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(if on {
                                    t.colors.primary
                                } else {
                                    t.colors.text_subtle
                                })
                                .child(found.counts[i].to_string()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_scope(tab, window, cx)
                    })),
            );
        }

        // Over the chat: the list keeps the wheel while it has more to show.
        let mut list = super::hold_scroll(div().id("palette-list"), &self.scroll)
            .flex()
            .flex_col()
            .max_h(px(380.).min(theme::fit_height(window, &t, t.size.titlebar * 4.)))
            .p(t.space.xs);
        let mut last_group = "";
        // The list's children: group labels as well as rows.
        let mut child = 0;
        let mut selected_child = 0;
        for (i, (cmd, pos)) in found.rows.iter().enumerate() {
            if cmd.group != last_group {
                last_group = cmd.group;
                child += 1;
                let more = found.cut.iter().find(|(g, _)| *g == cmd.group).map(|(_, n)| *n);
                let to = Scope::of(cmd.group);
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(t.space.md)
                        .pt(t.space.sm)
                        .pb(t.space.xs)
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_subtle)
                        .child(cmd.group)
                        .children(more.map(|n| {
                            let ink = t.colors.text_strong;
                            div()
                                .id(("palette-more", i))
                                .cursor_pointer()
                                .text_color(t.colors.primary)
                                .hover(move |st| st.text_color(ink))
                                .child(format!("All {n} ›"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_scope(to, window, cx)
                                }))
                        })),
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
                .h(t.size.row)
                .px(t.space.md)
                .rounded(t.radius.button)
                .cursor_pointer()
                .map(|d| {
                    if on {
                        theme::selected(&t, d)
                    } else {
                        let hover = t.colors.hover;
                        d.hover(move |st| st.bg(hover))
                    }
                })
                .child(icon(
                    cmd.icon,
                    t.size.icon_sm,
                    if on {
                        t.colors.primary
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
                .children(cmd.hint.as_ref().map(|h| keys(&t, h)))
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
            let words = scope(self.query.read(cx).value().as_ref()).1.trim().chars().count();
            list = list.child(
                div()
                    .px(t.space.md)
                    .py(t.space.lg)
                    .text_color(t.colors.text_subtle)
                    .child(match found.scope {
                        Scope::Said if words < 2 => {
                            "Type two letters or more to search what was said in your chats."
                        }
                        Scope::Files if !found.typed => "This project has no files to show.",
                        _ => "Nothing matches.",
                    }),
            );
        }
        if std::mem::take(&mut self.reveal) {
            self.scroll.scroll_to_item(selected_child);
        }

        let hint = |k: Div, what: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .child(k)
                .child(what)
        };
        let footer = div()
            .flex()
            .items_center()
            .gap(t.space.lg)
            .px(t.space.md)
            .py(t.space.sm)
            .border_t_1()
            .border_color(t.colors.border)
            .text_size(t.text.caption)
            .text_color(t.colors.text_subtle)
            .child(hint(keys(&t, "↑ ↓"), "move"))
            .child(hint(keys(&t, "Enter"), "open"))
            .child(hint(keys(&t, "Esc"), "close"))
            .child(div().flex_1())
            .child(hint(keys(&t, "Tab"), "next scope"));

        div()
            .absolute()
            .top(t.size.control + t.space.xs)
            .left_0()
            .right_0()
            .child(super::motion::edge_draw(
                theme::popover(&t)
                    .id("palette")
                    .occlude()
                    .p_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .cursor(CursorStyle::Arrow)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(tabs)
                    .child(list)
                    .child(footer)
                    .relative(),
                "palette-in",
                t.colors.primary,
                0.,
                &t,
            ))
            .into_any_element()
    }
}

/// A listener for one of the text box's actions that runs `key` instead.
fn nav<A: Action>(
    key: &'static str,
    cx: &mut Context<CommandPalette>,
) -> impl Fn(&A, &mut Window, &mut gpui::App) + 'static {
    let this = cx.entity().downgrade();
    move |_, window, cx| {
        let _ = this.update(cx, |p, cx| {
            if p.key(key, window, cx) {
                cx.stop_propagation();
            }
        });
    }
}

/// A shortcut as key caps: "Ctrl Shift E" is three.
fn keys(t: &Theme, text: &str) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(3.))
        .children(text.split_whitespace().map(|k| {
            div()
                .flex()
                .items_center()
                .h(px(18.))
                .px(px(5.))
                .rounded(t.radius.small)
                .border_1()
                .border_b_2()
                .border_color(t.colors.border)
                .bg(t.extra.stage)
                .text_size(t.text.caption)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_muted)
                .child(k.to_string())
        }))
}

/// Behind the open palette: the window under the title bar, dimmed. A click
/// on it closes the palette.
impl Render for CommandPalette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let fade = std::time::Duration::from_millis(t.motion_ms);
        div()
            .id("palette-scrim")
            .occlude()
            .absolute()
            .top(t.size.titlebar)
            .left_0()
            .right_0()
            .bottom_0()
            .bg(t.extra.scrim)
            .on_click(cx.listener(|this, _, window, cx| {
                this.close(cx);
                window.dispatch_action(Box::new(FocusComposer), cx);
            }))
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
        assert_eq!(scope("%send_to"), (Scope::Said, "send_to"));
        assert_eq!(scope("theme"), (Scope::All, "theme"));
    }

    #[test]
    fn every_tab_is_reached_by_its_prefix_and_owns_its_rows() {
        use super::{scope, Scope};
        for tab in Scope::TABS {
            assert_eq!(scope(&format!("{}words", tab.prefix())), (tab, "words"));
        }
        assert_eq!(Scope::of("Files"), Scope::Files);
        assert_eq!(Scope::of("Said in chats"), Scope::Said);
        assert_eq!(Scope::of("This chat"), Scope::Commands);
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

/// The language after the one in use.
fn next_lang() -> crate::i18n::Lang {
    use crate::i18n::Lang;
    let now = crate::i18n::lang();
    let i = Lang::ALL.iter().position(|l| *l == now).unwrap_or(0);
    Lang::ALL[(i + 1) % Lang::ALL.len()]
}

/// Use the next language (from the palette).
pub fn use_next_language(cx: &mut gpui::App) -> crate::i18n::Lang {
    let next = next_lang();
    let _ = crate::i18n::choose(next);
    cx.refresh_windows();
    next
}
