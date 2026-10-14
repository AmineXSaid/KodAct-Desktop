//! The command palette (Ctrl/⌘K): every action, every chat, every model and
//! mode, fuzzy-filtered as you type. Arrow keys move, Enter runs, Esc closes.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, font, prelude::*, px, Action, AnimationExt, Context, Entity, FontWeight, IntoElement,
    KeyDownEvent, Render, SharedString, StyledText, Subscription, TextRun, Window,
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
}

struct Command {
    label: SharedString,
    group: &'static str,
    icon: &'static str,
    hint: Option<SharedString>,
    target: Target,
}

pub struct CommandPalette {
    state: Entity<AppState>,
    query: Entity<InputState>,
    selected: usize,
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
        let query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search commands, chats, models…"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            // Enter is handled once, in the key capture below.
            cx.subscribe_in(&query, window, |this, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.selected = 0;
                    cx.notify();
                }
            }),
        ];
        Self {
            state,
            query,
            selected: 0,
            _subs: subs,
        }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = 0;
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
                "Connect to Host (SSH)",
                "icons/server.svg",
                None,
                Box::new(ConnectHost),
            ),
            act(
                "Attach to Running Container",
                "icons/box.svg",
                None,
                Box::new(AttachContainer),
            ),
        ]);
        if s.has_devcontainer() {
            out.push(act(
                "Reopen in Container",
                "icons/box.svg",
                None,
                Box::new(ReopenInContainer),
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
                target: Target::Model(row.name),
            });
        }
        for c in &s.chats {
            out.push(Command {
                label: c.title.clone(),
                group: "Chats",
                icon: "icons/message-square.svg",
                hint: Some(c.group().into()),
                target: Target::Chat(c.id),
            });
        }
        out
    }

    /// The commands that match, best first, with their matched positions.
    fn matches(&self, cx: &gpui::App) -> Vec<(Command, Vec<usize>)> {
        let q = self.query.read(cx).value().to_string();
        let mut scored: Vec<(i32, usize, Command, Vec<usize>)> = self
            .commands(cx)
            .into_iter()
            .enumerate()
            .filter_map(|(order, c)| fuzzy(&q, &c.label).map(|(score, pos)| (score, order, c, pos)))
            .collect();
        if !q.trim().is_empty() {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        scored
            .into_iter()
            .map(|(_, _, c, p)| (c, p))
            .take(50)
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
        }
    }
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
            .p(t.space.xs);
        let mut last_group = "";
        for (i, (cmd, pos)) in matches.iter().enumerate() {
            if cmd.group != last_group {
                last_group = cmd.group;
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
            list = list.child(
                div()
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
                            .child(highlighted(&cmd.label, pos, &t)),
                    )
                    .children(cmd.hint.clone().map(|h| kbd(&t, h)))
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.selected != i {
                            this.selected = i;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(|this, _, window, cx| this.run(window, cx))),
            );
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
                    "escape" => {
                        this.close(cx);
                        window.dispatch_action(Box::new(FocusComposer), cx);
                    }
                    "enter" => this.run(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                this.close(cx);
                window.dispatch_action(Box::new(FocusComposer), cx);
            }))
            .child(
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
                            .child(format!(
                                "{count} result{}",
                                if count == 1 { "" } else { "s" }
                            )),
                    )
                    .with_animation(
                        "palette-in",
                        gpui::Animation::new(fade).with_easing(gpui::ease_in_out),
                        |d, delta| d.opacity(delta),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

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
