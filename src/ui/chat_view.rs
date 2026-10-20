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
    shown_version: u64,
    /// Re-renders the working line's timer every second while a reply plays.
    ticker: Option<Task<()>>,
    /// The working line's step, and when it started (its timer restarts
    /// when the step changes).
    step: Option<(Activity, String, Instant)>,
    /// You scrolled up from the newest message: "Jump to latest" shows.
    away: bool,
    /// The find move last scrolled to.
    shown_find: u64,
    /// The jump (tasks, approvals) last scrolled to.
    shown_jump: u64,
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
            shown_version: 0,
            ticker: None,
            step: None,
            away: false,
            shown_find: 0,
            shown_jump: 0,
        };
        view.sync(cx);
        view
    }

    /// Bring the list in line with the state: a new chat resets it, new
    /// messages are spliced in, and any change re-measures (streaming grows
    /// the last item; opening a tool row grows another).
    fn sync(&mut self, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        let busy = s.busy();
        let (id, count, version) = match s.chat() {
            Some(c) => (Some(c.id), c.messages.len() + busy as usize, c.version),
            None => (None, 0, 0),
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
        if let Some((seq, ix)) = self.state.read(cx).jump {
            if seq != self.shown_jump && ix < count {
                self.list.scroll_to(gpui::ListOffset {
                    item_ix: ix,
                    offset_in_item: px(0.),
                });
                self.away = true;
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
            s.chat().and_then(|c| step_of(c, s.mode))
        } else {
            None
        };
        match (step, &self.step) {
            (Some((_, label)), Some((_, shown, _))) if label == *shown => {}
            (Some((activity, label)), _) => self.step = Some((activity, label, Instant::now())),
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
        // A step just ended, or a question was answered: the model reads
        // the result and picks what comes next.
        Some(_) => (Activity::Digesting, "Reading the result".into()),
        None => (Activity::Waiting, "Waiting for the model to answer".into()),
    })
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
                div()
                    .id("jump-latest")
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .h(t.size.control + px(4.))
                    .px(t.space.md)
                    .rounded(t.radius.button)
                    .bg(t.colors.surface)
                    .border_1()
                    .border_color(t.colors.border)
                    .shadow(t.elevation_popover())
                    .cursor_pointer()
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text)
                    .hover(move |d| d.bg(hover))
                    .child(icon_arrow(&t))
                    // While a reply plays, the button says there's more coming.
                    .child(if busy {
                        "New messages below"
                    } else {
                        "Jump to latest"
                    })
                    .child(super::kbd(&t, super::shortcut("J")))
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

impl Render for ChatView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let step = self.step.clone();
        let jump = self.away.then(|| self.jump_button(cx));
        let list = list(self.list.clone(), move |ix, window, cx| {
            let t = cx.theme().clone();
            let s = state.read(cx);
            let Some(chat) = s.chat() else {
                return div().into_any_element();
            };
            let body = match chat.messages.get(ix) {
                Some(m) => {
                    let after_user =
                        ix == 0 || matches!(chat.messages.get(ix - 1), Some(Message::User { .. }));
                    let ctx = ItemContext {
                        state: state.clone(),
                        place: s
                            .chat_remote(chat.id)
                            .map(|r| (r.target.label().into(), r.target.icon())),
                        chat: chat.id,
                        index: ix,
                        copied: s.copied.clone(),
                        busy: s.busy(),
                        last: ix + 1 == chat.messages.len(),
                        first_in_turn: after_user && matches!(m, Message::Assistant { .. }),
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
                        sources: s.diagram_code.clone(),
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
                None => match &step {
                    Some((activity, label, since)) => working_line(&t, *activity, label, *since),
                    None => div().into_any_element(),
                },
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
                // The task pill sits over the top of the transcript: the first
                // message starts below it.
                .when(ix == 0, |d| {
                    let pill = !super::tasks::tasks(&chat.messages).is_empty();
                    d.pt(if pill {
                        t.space.xl + t.size.control
                    } else {
                        t.space.xl
                    })
                })
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
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(list)
            .children(jump)
    }
}

/// `12s`, `1m 7s`, `1h 3m`.
fn elapsed(seconds: u64) -> String {
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
fn shimmer(label: &str, sweep: f32, t: &crate::theme::Theme) -> StyledText {
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

/// Forge's working line: the mark for the kind of work (`activity.rs`: a
/// page being read, a prompt's cursor, the quill for a plan), then the step
/// in words and for how long. A light sweeps across the words.
fn working_line(
    t: &crate::theme::Theme,
    activity: Activity,
    label: &str,
    since: Instant,
) -> gpui::AnyElement {
    let theme = t.clone();
    let text = label.to_string();
    div()
        .flex()
        .items_center()
        .gap(t.space.tight)
        .h(px(24.))
        .mt(px(4.))
        .child(super::activity::mark(
            activity,
            t,
            SharedString::from(format!("working-mark-{activity:?}")),
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
    fn elapsed_reads_short() {
        assert_eq!(elapsed(12), "12s");
        assert_eq!(elapsed(67), "1m 7s");
        assert_eq!(elapsed(3780), "1h 3m");
    }
}
