//! The conversation: a virtualized list (GPUI's `list`) that follows the
//! newest message while you are at the bottom and stays put when you scroll
//! up to read. While a reply plays, a working line sits at the end.

use std::time::Duration;

use std::time::Instant;

use gpui::{
    div, font, list, prelude::*, px, Animation, AnimationExt, Context, Entity, FollowMode,
    FontWeight, IntoElement, ListAlignment, ListState, Render, SharedString, StyledText, Task,
    TextRun, Window,
};

use super::activity::Activity;
use super::message::{self, ItemContext};
use crate::state::{AppState, Block, Chat, Message, ToolKind, ToolStatus, Verdict};
use crate::theme::ActiveTheme;

pub struct ChatView {
    state: Entity<AppState>,
    list: ListState,
    shown_chat: Option<usize>,
    shown_count: usize,
    /// The chat's version, and how many messages are queued.
    shown_version: (u64, usize),
    /// Re-renders the working line's timer every second while a reply plays.
    ticker: Option<Task<()>>,
    /// The working line: the step's mark, what the work is about (its
    /// headline), the step in words (its tooltip), and when the headline
    /// began (its timer restarts when the work moves on, not at each step).
    step: Option<(Activity, String, Option<String>, Instant)>,
    /// You scrolled up from the newest message: "Jump to latest" shows.
    away: bool,
    /// The find move last scrolled to.
    shown_find: u64,
    /// The jump (tasks, approvals) last scrolled to.
    shown_jump: u64,
    /// A jump to a block still finding it: its message, and where the
    /// block turned out to be drawn (`message::Reveal`).
    reveal: Option<(usize, std::rc::Rc<message::Reveal>)>,
    /// Frames the reveal has looked for its block; it gives up after a few.
    reveal_tries: u8,
    /// The waiting question card's "Other" field, made with the first frame
    /// (it needs the window), and the question it last showed.
    other: Option<gpui::Entity<gpui::component::input::InputState>>,
    other_for: Option<(usize, usize, usize, usize)>,
    _other_subs: Vec<gpui::Subscription>,
}

impl ChatView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(800.));
        list.set_follow_mode(FollowMode::Tail);
        // Following the tail stops when you scroll up and starts again at
        // the bottom; the button shows in between.
        let me = cx.weak_entity();
        list.set_scroll_handler(move |ev, _, cx| {
            let away = !ev.is_following_tail;
            let _ = me.update(cx, |v: &mut ChatView, cx| {
                if v.away != away {
                    v.away = away;
                    cx.notify();
                }
            });
        });
        cx.observe(&state, |this, _, cx| {
            this.sync(cx);
            cx.notify();
        })
        .detach();
        let mut view = Self {
            state,
            list,
            shown_chat: None,
            shown_count: 0,
            shown_version: (0, 0),
            ticker: None,
            step: None,
            away: false,
            shown_find: 0,
            shown_jump: 0,
            reveal: None,
            reveal_tries: 0,
            other: None,
            other_for: None,
            _other_subs: vec![],
        };
        view.sync(cx);
        view
    }

    /// Bring the list in line with the state: a new chat resets it, new
    /// messages are spliced in, and any change re-measures (streaming grows
    /// the last item; opening a tool row grows another).
    fn sync(&mut self, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        // The last row holds the working line and what you queued.
        let busy = s.busy();
        let queued = s
            .chat()
            .and_then(|c| s.queued.get(&c.id))
            .map_or(0, Vec::len);
        let tail = busy || queued > 0;
        let (id, count, version) = match s.chat() {
            Some(c) => (
                Some(c.id),
                c.messages.len() + tail as usize,
                (c.version, queued),
            ),
            None => (None, 0, (0, 0)),
        };
        if id != self.shown_chat {
            self.list.reset(count);
            self.list.set_follow_mode(FollowMode::Tail);
            self.away = false;
        } else if count > self.shown_count {
            self.list
                .splice(self.shown_count..self.shown_count, count - self.shown_count);
            self.list.remeasure();
        } else if count < self.shown_count {
            self.list.reset(count);
        } else if version != self.shown_version {
            self.list.remeasure();
        }
        (self.shown_chat, self.shown_count, self.shown_version) = (id, count, version);

        // A find move: its match comes to the top of the view.
        let find = self
            .state
            .read(cx)
            .find
            .as_ref()
            .map(|f| (f.seq, f.current()));
        if let Some((seq, Some(ix))) = find {
            if seq != self.shown_find && ix < count {
                self.list.scroll_to(gpui::ListOffset {
                    item_ix: ix,
                    offset_in_item: px(0.),
                });
                self.away = true;
            }
        }
        self.shown_find = find.map_or(0, |(seq, _)| seq);

        // A jump from the task panel or the approvals list.
        // A block in it: the message comes to the top first, so it is laid
        // out, then the block's row is scrolled to once its place is known.
        if let Some((seq, ix, block)) = self.state.read(cx).jump {
            if seq != self.shown_jump && ix < count {
                self.list.scroll_to(gpui::ListOffset {
                    item_ix: ix,
                    offset_in_item: px(0.),
                });
                self.away = true;
                self.reveal_tries = 0;
                self.reveal = block.map(|block| {
                    (
                        ix,
                        std::rc::Rc::new(message::Reveal {
                            block,
                            root: Default::default(),
                            row: Default::default(),
                        }),
                    )
                });
            }
            self.shown_jump = seq;
        }

        if busy && self.ticker.is_none() {
            self.ticker = Some(cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }));
        } else if !busy {
            self.ticker = None;
        }

        // Follow the step, restarting its timer when it changes.
        let step = if busy {
            let s = self.state.read(cx);
            s.chat().and_then(|c| {
                let (activity, words) = step_of(c, s.mode)?;
                // The step itself shows in its own row above (Reading a
                // file, Running the tests): the line says what the work is
                // about instead, and keeps the step for its tooltip. States
                // with no row of their own, and a turn with no headline yet,
                // keep their words. Never your own message read back.
                let about = headline(c, s.headlines.get(&c.id).map(String::as_str));
                // A step with a row of its own, before the headline comes:
                // just "Working", not the row's words again.
                let own_row = !matches!(
                    activity,
                    Activity::Waiting | Activity::Thinking | Activity::Replying
                );
                // The thought being written already says so, pulsing.
                let thought = matches!(
                    c.messages.last(),
                    Some(Message::Assistant { blocks }) if matches!(blocks.last(), Some(Block::Thinking { .. }))
                );
                if thought {
                    return None;
                }
                Some(match (activity, about) {
                    (Activity::AwaitingYou | Activity::Compacting | Activity::Agents, _) => {
                        (activity, words, None)
                    }
                    (_, Some(about)) => (activity, about, Some(words)),
                    (_, None) if own_row => (activity, "Working".into(), Some(words)),
                    (_, None) => (activity, words, None),
                })
            })
        } else {
            None
        };
        // One timer for the turn, whatever the line says meanwhile.
        match (step, &mut self.step) {
            (Some((activity, label, detail)), Some((a, shown, d, _))) => {
                *a = activity;
                *shown = label;
                *d = detail;
            }
            (Some((activity, label, detail)), _) => {
                self.step = Some((activity, label, detail, Instant::now()))
            }
            (None, _) => self.step = None,
        }
    }
}

/// What the reply is doing right now, as the working line shows it: the
/// kind of work (its mark) and the step in words.
fn step_of(c: &Chat, mode: crate::modes::ModeId) -> Option<(Activity, String)> {
    let clip = |s: &str| {
        if s.chars().count() > 48 {
            format!("{}…", s.chars().take(47).collect::<String>())
        } else {
            s.to_string()
        }
    };
    let blocks = match c.messages.last() {
        Some(Message::Assistant { blocks }) => blocks.as_slice(),
        Some(Message::Notice { text, .. }) if text.as_ref() == crate::state::COMPACTING => {
            return Some((Activity::Compacting, "Compacting the conversation".into()));
        }
        _ => &[],
    };
    let agents = blocks
        .iter()
        .filter(|b| {
            matches!(b, Block::Tool(t) if t.status == ToolStatus::Running
                && crate::state::is_agent_tool(&t.target))
        })
        .count();
    if agents > 1 {
        return Some((
            Activity::Agents,
            format!("{agents} agents working side by side"),
        ));
    }
    let doc = writing_doc(c, mode);
    Some(match blocks.last() {
        Some(Block::Permission {
            verdict: Verdict::Pending,
            ..
        }) => (Activity::AwaitingYou, "Waiting for your answer".into()),
        Some(Block::Tool(t)) if t.status == ToolStatus::Running => {
            let target = clip(&t.target);
            match t.kind {
                ToolKind::Read => (Activity::Reading, format!("Reading {target}")),
                ToolKind::Edit if doc => (Activity::WritingDoc, format!("Writing {target}")),
                ToolKind::Edit => (Activity::Editing, format!("Editing {target}")),
                ToolKind::Write if doc => (Activity::WritingDoc, format!("Writing {target}")),
                ToolKind::Write => (Activity::WritingCode, format!("Writing {target}")),
                ToolKind::Bash => match &t.note {
                    Some(note) => (Activity::Running, clip(note)),
                    None => (Activity::Running, format!("Running {target}")),
                },
                ToolKind::Search => (
                    Activity::Searching,
                    format!("Searching the project for {target}"),
                ),
                ToolKind::WebSearch => (
                    Activity::SearchingWeb,
                    format!("Searching the web for {target}"),
                ),
                ToolKind::Web => (
                    Activity::Fetching,
                    format!(
                        "Fetching {}",
                        t.target
                            .split("://")
                            .nth(1)
                            .unwrap_or(&t.target)
                            .split('/')
                            .next()
                            .unwrap_or_default()
                    ),
                ),
                ToolKind::Other if crate::state::is_agent_tool(&t.target) => (
                    Activity::Agent,
                    match &t.note {
                        Some(note) => format!("An agent is working: {}", clip(note)),
                        None => "An agent is working".into(),
                    },
                ),
                ToolKind::Other if doc => (Activity::WritingDoc, "Writing the plan".into()),
                ToolKind::Other => match &t.note {
                    Some(note) => (Activity::Tool, clip(note)),
                    None => (Activity::Tool, format!("Using {}", tool_name(&t.target))),
                },
            }
        }
        Some(Block::Thinking { .. }) => (Activity::Thinking, "Thinking it through".into()),
        Some(Block::Text(_)) if doc => (Activity::WritingDoc, "Writing the plan".into()),
        Some(Block::Text(_)) => (Activity::Replying, "Writing the reply".into()),
        // A step just ended, or a question was answered: the model thinks
        // about the result and picks what comes next.
        Some(_) => (Activity::Thinking, "Thinking about the result".into()),
        None => (Activity::Waiting, "Waiting…".into()),
    })
}

/// What the work is about, for the working line: the task the model marked
/// in progress in its task list; else what the model said, in a few words,
/// it is doing about your latest message (`title::headline`). None until
/// then: the line shows the step, never your own message read back.
fn headline(c: &Chat, about: Option<&str>) -> Option<String> {
    let clip = |s: &str| {
        let s = s.trim();
        if s.chars().count() > 60 {
            format!("{}…", s.chars().take(59).collect::<String>().trim_end())
        } else {
            s.to_string()
        }
    };
    // The task in progress, from the reply's own list.
    let task = c.messages.iter().rev().find_map(|m| match m {
        Message::Assistant { blocks } => blocks.iter().rev().find_map(|b| match b {
            // Its rows (`backend::tool_call`): `[~]` marks the one in progress.
            Block::Tool(t) if t.target.as_ref() == "TodoWrite" => Some(
                t.diff
                    .iter()
                    .find_map(|l| l.text.trim().strip_prefix("[~] ").map(str::to_string)),
            ),
            _ => None,
        }),
        _ => None,
    });
    match (task, about) {
        (Some(Some(task)), _) => Some(clip(&task)),
        (_, Some(about)) if !about.trim().is_empty() => Some(clip(about)),
        _ => None,
    }
}

/// A tool's name as people say it: an MCP tool (`mcp__github__create_issue`)
/// as "create issue (github)".
fn tool_name(target: &str) -> String {
    match target
        .strip_prefix("mcp__")
        .and_then(|r| r.split_once("__"))
    {
        Some((server, tool)) => format!("{} ({server})", tool.replace('_', " ")),
        None => target.to_string(),
    }
}

/// Files Forge writes for people to read: plans, notes, docs.
const DOC_EXTENSIONS: &[&str] = &["md", "mdx", "markdown", "txt", "rst", "adoc", "org", "docx"];

/// Forge is writing a plan or a document right now: writing or editing a
/// document file, presenting its plan, or replying in plan mode (the reply
/// is the plan).
fn writing_doc(c: &Chat, mode: crate::modes::ModeId) -> bool {
    let Some(Message::Assistant { blocks }) = c.messages.last() else {
        return false;
    };
    match blocks.last() {
        Some(Block::Tool(t)) if t.status == ToolStatus::Running => match t.kind {
            ToolKind::Write | ToolKind::Edit => std::path::Path::new(t.target.as_ref())
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| DOC_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())),
            ToolKind::Other => t.target.to_ascii_lowercase().contains("plan"),
            _ => false,
        },
        Some(Block::Text(_)) => mode == crate::modes::ModeId::Plan,
        _ => false,
    }
}

impl ChatView {
    /// Back to the newest message, following it again.
    pub fn jump_to_latest(&mut self, cx: &mut Context<Self>) {
        self.list.set_follow_mode(FollowMode::Tail);
        self.away = false;
        cx.notify();
    }

    fn jump_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let hover = t.colors.hover;
        let busy = self.state.read(cx).busy();
        div()
            .absolute()
            .bottom(t.space.md)
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                // A round button, as Claude Code's: the arrow says it all;
                // the words and the key are in its tooltip.
                div()
                    .id("jump-latest")
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(t.size.button_h)
                    .rounded(t.radius.pill)
                    .bg(t.colors.surface)
                    .border_1()
                    .border_color(t.colors.border)
                    .shadow(t.elevation_popover())
                    .cursor_pointer()
                    .hover(move |d| d.bg(hover))
                    .child(icon_arrow(&t))
                    // While a reply plays, a dot says there's more coming.
                    .when(busy, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(1.))
                                .right(px(1.))
                                .size(px(7.))
                                .rounded(t.radius.pill)
                                .bg(t.extra.accent),
                        )
                    })
                    .tooltip(move |w, cx| {
                        let words = if busy {
                            "New messages below"
                        } else {
                            "Jump to latest"
                        };
                        gpui::component::tooltip::Tooltip::new(format!(
                            "{words} ({})",
                            super::shortcut("J")
                        ))
                        .build(w, cx)
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.jump_to_latest(cx)))
                    .with_animation(
                        "jump-in",
                        Animation::new(Duration::from_millis(t.motion_ms))
                            .with_easing(gpui::ease_in_out),
                        |d, delta| d.opacity(delta).mb(px(6.) * (1. - delta)),
                    ),
            )
    }
}

fn icon_arrow(t: &crate::theme::Theme) -> gpui::Svg {
    super::icon("icons/arrow-down.svg", t.size.icon_sm, t.colors.text_muted)
}

impl ChatView {
    /// A jump's block, once drawn: scroll so its row sits just under the
    /// top (clear of the task pill). Until then, look again next frame.
    fn finish_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((ix, r)) = &self.reveal else { return };
        match (r.root.get(), r.row.get()) {
            (Some(root), Some(row)) => {
                let t = cx.theme();
                let margin = t.space.lg;
                self.list.scroll_to(gpui::ListOffset {
                    item_ix: *ix,
                    offset_in_item: (row - root - margin).max(px(0.)),
                });
                self.reveal = None;
                cx.notify();
            }
            // Not drawn (yet): look again, a few frames at most.
            _ if self.reveal_tries < 8 => {
                self.reveal_tries += 1;
                window.request_animation_frame();
            }
            _ => self.reveal = None,
        }
    }
}

impl ChatView {
    /// The "Other" field of the question card waiting on you: made once,
    /// and showing what you typed for the question the card shows.
    fn other_field(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<gpui::component::input::InputState>> {
        use gpui::component::input::{InputEvent, InputState};
        let Some((chat, msg, block, step, own)) = self.state.read(cx).waiting_question() else {
            self.other_for = None;
            return None;
        };
        let input = match &self.other {
            Some(i) => i.clone(),
            None => {
                let i = cx.new(|cx| InputState::new(window, cx).placeholder("Type your answer"));
                let state = self.state.clone();
                self._other_subs.push(cx.subscribe_in(
                    &i,
                    window,
                    move |_, i, ev: &InputEvent, _, cx| {
                        let text = i.read(cx).value().to_string();
                        match ev {
                            InputEvent::Change => {
                                state.update(cx, |s, cx| s.set_own_text(text, cx))
                            }
                            // Enter answers this question with it and moves on.
                            InputEvent::PressEnter { .. } if !text.trim().is_empty() => state
                                .update(cx, |s, cx| s.answer_in_words(text.trim().to_string(), cx)),
                            _ => {}
                        }
                    },
                ));
                self.other = Some(i.clone());
                i
            }
        };
        let key = (chat, msg, block, step);
        if self.other_for != Some(key) {
            self.other_for = Some(key);
            let text = own.unwrap_or_default();
            input.update(cx, |i, cx| i.set_value(text, window, cx));
        }
        Some(input)
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let other = self.other_field(window, cx);
        let other_msg = self
            .other_for
            .map(|(_, m, _, _)| m)
            .filter(|_| other.is_some());
        // Paragraphs take their reading order afresh, top to bottom.
        super::select::begin_frame();
        self.finish_reveal(window, cx);
        let reveal = self.reveal.clone();
        let state = self.state.clone();
        let step = self.step.clone();
        // Only when there is something below to jump to: a short chat, or
        // one that stopped following while nothing more came, has none.
        let below = self.list.is_scrolled_to_end() == Some(false);
        let jump = (self.away && below).then(|| self.jump_button(cx));
        let list = list(self.list.clone(), move |ix, window, cx| {
            let t = cx.theme().clone();
            let s = state.read(cx);
            let Some(chat) = s.chat() else {
                return div().into_any_element();
            };
            let agents = std::rc::Rc::new(s.agents.get(&chat.id).cloned().unwrap_or_default());
            let body = match chat.messages.get(ix) {
                Some(m) => {
                    let ctx = ItemContext {
                        state: state.clone(),
                        marks: match m {
                            Message::User { text, .. } => super::autocomplete::marks(text, s, true),
                            _ => vec![],
                        },
                        place: s
                            .chat_remote(chat.id)
                            .map(|r| (r.target.label().into(), r.target.icon())),
                        chat: chat.id,
                        index: ix,
                        copied: s.copied.clone(),
                        busy: s.busy(),
                        last: ix + 1 == chat.messages.len(),
                        open_doc: s.doc.clone(),
                        scale: window.scale_factor(),
                        rating: s.ratings.get(&(chat.id, ix)).copied(),
                        newest_turn: !s.busy()
                            && chat.messages[ix + 1..].iter().all(|m| {
                                !matches!(m, Message::TurnEnd { .. } | Message::User { .. })
                            }),
                        can_continue: s.can_continue(),
                        memory: matches!(m, Message::TurnEnd { .. })
                            .then(|| s.memory_changes.get(&chat.id).cloned())
                            .flatten()
                            .filter(|_| {
                                !chat.messages[ix + 1..].iter().any(|m| {
                                    matches!(m, Message::TurnEnd { .. } | Message::User { .. })
                                })
                            }),
                        after_error: ix > 0
                            && matches!(
                                chat.messages.get(ix - 1),
                                Some(Message::Notice { error: true, .. })
                            ),
                        in_last_turn: !chat.messages[ix + 1..]
                            .iter()
                            .any(|m| matches!(m, Message::User { .. })),
                        versions: if matches!(m, Message::User { .. }) {
                            chat.versions
                                .iter()
                                .enumerate()
                                .filter(|(_, v)| v.at == ix)
                                .map(|(index, v)| super::versions::Row {
                                    index,
                                    when: v.when,
                                    asked: v.asked(),
                                    preview: v.preview(),
                                })
                                .collect()
                        } else {
                            vec![]
                        },
                        versions_open: s.versions_open == Some((chat.id, ix)),
                        edited_open: s.edited_open.contains(&(chat.id, ix)),
                        agents: agents.clone(),
                        sources: s.diagram_code.clone(),
                        zooms: s.diagram_zoom.clone(),
                        detailed: chat.detailed,
                        open_groups: s
                            .open_groups
                            .iter()
                            .filter(|(c, m, _)| *c == chat.id && *m == ix)
                            .map(|(_, _, start)| *start)
                            .collect(),
                        flash: s
                            .flash
                            .filter(|(c, m, _)| *c == chat.id && *m == ix)
                            .map(|(_, _, b)| b),
                        other: other.clone().filter(|_| other_msg == Some(ix)),
                        thoughts: match m {
                            Message::Assistant { blocks } => blocks
                                .iter()
                                .enumerate()
                                .filter(|(_, b)| matches!(b, Block::Thinking { .. }))
                                .filter_map(|(b, _)| Some((b, s.thought_secs(chat.id, ix, b)?)))
                                .collect(),
                            _ => vec![],
                        },
                        reveal: reveal
                            .as_ref()
                            .filter(|(m, _)| *m == ix)
                            .map(|(_, r)| r.clone()),
                    };
                    // A message rises into place as it arrives (and as it
                    // scrolls back into view), as the reference's sections do.
                    super::motion::row_in_full(
                        message::render(m, &ctx, &t),
                        SharedString::from(format!("msg-in-{}-{ix}", chat.id)),
                        0,
                        &t,
                    )
                }
                None => {
                    let line = match &step {
                        // The card says it waits on you: no second line.
                        Some((Activity::AwaitingYou, ..)) => div().into_any_element(),
                        Some((activity, label, detail, since)) => working_line(
                            &t,
                            *activity,
                            label,
                            detail.clone(),
                            *since,
                            written(chat.messages.last()),
                        ),
                        None => div().into_any_element(),
                    };
                    let queued = s
                        .queued
                        .get(&chat.id)
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    div()
                        .flex()
                        .flex_col()
                        .child(line)
                        .child(super::queued::render(&t, &state, chat.id, queued))
                        .into_any_element()
                }
            };
            // Find: every match gets a faint bar at its left, the one shown a strong one.
            let mark = s
                .find
                .as_ref()
                .filter(|f| !f.query.trim().is_empty())
                .and_then(|f| {
                    if f.current() == Some(ix) {
                        Some(t.colors.primary)
                    } else {
                        f.hits.contains(&ix).then(|| t.colors.primary.opacity(0.3))
                    }
                });
            div()
                .w_full()
                .flex()
                .justify_center()
                .px(t.space.xl)
                .when(ix == 0, |d| d.pt(t.space.xl))
                .child(
                    div()
                        .relative()
                        .w_full()
                        .max_w(t.size.column)
                        .pb(t.space.md)
                        .child(body)
                        .children(mark.map(|c| {
                            div()
                                .absolute()
                                .top_0()
                                .bottom(t.space.md)
                                .left(-t.space.md)
                                .w(px(3.))
                                .rounded(t.radius.line)
                                .bg(c)
                        })),
                )
                .into_any_element()
        })
        .size_full()
        .flex_1();
        // Scrolled off its top, the transcript fades out under the head, as
        // Claude Code's does, rather than ending on a cut line.
        let top = self.list.logical_scroll_top();
        let scrolled = top.item_ix > 0 || top.offset_in_item > px(0.);
        let t = cx.theme().clone();
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(list)
            .when(scrolled, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(t.size.edge_fade)
                        .bg(gpui::linear_gradient(
                            180.,
                            gpui::linear_color_stop(t.extra.stage, 0.),
                            gpui::linear_color_stop(t.extra.stage.opacity(0.), 1.),
                        )),
                )
            })
            // Scrolled up, the words fade out where the composer starts, so
            // text running under it reads as "there's more". At the end
            // nothing covers the newest lines.
            .when(self.away, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(t.size.edge_fade)
                        .bg(gpui::linear_gradient(
                            180.,
                            gpui::linear_color_stop(t.extra.stage.opacity(0.), 0.),
                            gpui::linear_color_stop(t.extra.stage, 1.),
                        )),
                )
            })
            // The transcript's scrollbar, over its right edge.
            .child(div().absolute().inset_0().child(
                gpui::component::scroll::Scrollbar::vertical(&self.list).id("chat-scrollbar"),
            ))
            .children(jump)
    }
}

/// `12s`, `1m 7s`, `1h 3m`.
pub(super) fn elapsed(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, seconds / 60 % 60),
    }
}

/// The step's words with a light passing over them, as on Claude's own
/// working line: the words rest in the muted ink, and a narrow band of
/// light runs left to right, each letter catching it as it passes: white
/// at night, a pale shine on paper.
pub(super) fn shimmer(label: &str, sweep: f32, t: &crate::theme::Theme) -> StyledText {
    let n = label.chars().count().max(1) as f32;
    let base = t.colors.text_muted;
    let strength = if t.is_dark { 1. } else { 0.6 };
    let mut runs = vec![];
    for (i, ch) in label.chars().enumerate() {
        let x = (i as f32 + 0.5) / n;
        let lift = (-((x - sweep) / 0.09).powi(2)).exp() * strength;
        let mut f = font(t.font_sans.clone());
        f.weight = FontWeight::MEDIUM;
        runs.push(TextRun {
            len: ch.len_utf8(),
            font: f,
            color: mix(base, gpui::white(), lift),
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    StyledText::new(label.to_string()).with_runs(runs)
}

/// `a` moved `k` (0..1) of the way to `b`, in RGB.
fn mix(a: gpui::Hsla, b: gpui::Hsla, k: f32) -> gpui::Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let l = |x: f32, y: f32| x + (y - x) * k;
    gpui::Rgba {
        r: l(a.r, b.r),
        g: l(a.g, b.g),
        b: l(a.b, b.b),
        a: l(a.a, b.a),
    }
    .into()
}

/// About how many tokens the reply in progress has written: its words,
/// thoughts and what its calls returned, at four characters a token (the
/// engine reports the true count when the turn ends).
fn written(last: Option<&Message>) -> usize {
    let Some(Message::Assistant { blocks }) = last else {
        return 0;
    };
    let chars: usize = blocks
        .iter()
        .map(|b| match b {
            Block::Text(t) => t.len(),
            Block::Thinking { text, .. } => text.len(),
            Block::Tool(c) => c.output.len() + c.target.len(),
            _ => 0,
        })
        .sum();
    chars / 4
}

/// `11.5k`, `840`.
fn tokens(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f32 / 1000.)
    } else {
        n.to_string()
    }
}

/// Forge's working line, as Claude Code's: the ember mark (`activity.rs`),
/// the step in words with a light sweeping across them, then for how long
/// and about how much it has written.
fn working_line(
    t: &crate::theme::Theme,
    activity: Activity,
    label: &str,
    detail: Option<String>,
    since: Instant,
    written: usize,
) -> gpui::AnyElement {
    let theme = t.clone();
    let text = label.to_string();
    div()
        .id("working-line")
        .flex()
        .items_center()
        .gap(t.space.tight)
        .h(px(24.))
        .mt(px(4.))
        // Forge's cube turning in the theme's live colour, as on agent rows;
        // the words say what kind of step it is.
        .child(super::turning_logo_in(
            f32::from(t.size.icon),
            SharedString::from(format!("working-mark-{activity:?}")),
            t.extra.live,
        ))
        .child(div().font_weight(FontWeight::MEDIUM).with_animation(
            SharedString::from(format!("working-{label}")),
            Animation::new(Duration::from_millis(2200)).repeat(),
            move |d, delta| d.child(shimmer(&text, -0.3 + delta * 1.6, &theme)),
        ))
        .child(
            div()
                .ml(px(4.))
                .text_color(t.colors.text_subtle)
                .child(elapsed(since.elapsed().as_secs())),
        )
        .when(written > 0, |d| {
            d.child(
                div()
                    .text_color(t.colors.text_subtle)
                    .child(format!("· ~{} tokens", tokens(written))),
            )
        })
        .when_some(detail, |d, step| {
            let step: SharedString = step.into();
            d.tooltip(move |w, cx| {
                gpui::component::tooltip::Tooltip::new(step.clone()).build(w, cx)
            })
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::elapsed;

    #[test]
    fn mcp_tools_read_as_people_say_them() {
        assert_eq!(
            super::tool_name("mcp__github__create_issue"),
            "create issue (github)"
        );
        assert_eq!(super::tool_name("TodoWrite"), "TodoWrite");
    }

    #[test]
    fn token_counts_read_short() {
        assert_eq!(super::tokens(840), "840");
        assert_eq!(super::tokens(11_540), "11.5k");
    }

    #[test]
    fn elapsed_reads_short() {
        assert_eq!(elapsed(12), "12s");
        assert_eq!(elapsed(67), "1m 7s");
        assert_eq!(elapsed(3780), "1h 3m");
    }
}
