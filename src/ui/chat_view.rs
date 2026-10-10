//! The conversation: a virtualized list (GPUI's `list`) that follows the
//! newest message while you are at the bottom and stays put when you scroll
//! up to read. While a reply plays, a working line sits at the end.

use std::time::Duration;

use std::time::Instant;

use gpui::{
    div, font, list, prelude::*, px, Animation, AnimationExt, Context, Entity, FollowMode, FontWeight, IntoElement,
    ListAlignment, ListState, Render, SharedString, StyledText, Task, TextRun, Window,
};

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
    step: Option<(String, Instant)>,
}

impl ChatView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(800.));
        list.set_follow_mode(FollowMode::Tail);
        cx.observe(&state, |this, _, cx| {
            this.sync(cx);
            cx.notify();
        })
        .detach();
        let mut view = Self { state, list, shown_chat: None, shown_count: 0, shown_version: 0, ticker: None, step: None };
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
        } else if count > self.shown_count {
            self.list.splice(self.shown_count..self.shown_count, count - self.shown_count);
            self.list.remeasure();
        } else if count < self.shown_count {
            self.list.reset(count);
        } else if version != self.shown_version {
            self.list.remeasure();
        }
        (self.shown_chat, self.shown_count, self.shown_version) = (id, count, version);

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
        let step = if busy { self.state.read(cx).chat().and_then(|c| step_label(c, s_turn_seed(c))) } else { None };
        match (&step, &self.step) {
            (Some(label), Some((shown, _))) if label == shown => {}
            (Some(label), _) => self.step = Some((label.clone(), Instant::now())),
            (None, _) => self.step = None,
        }
    }
}

/// A seed that stays the same for one turn, so its idle verb changes only
/// with time.
fn s_turn_seed(c: &Chat) -> usize {
    c.messages.iter().filter(|m| matches!(m, Message::User { .. })).count()
}

/// Forge's idle verbs, between steps.
const VERBS: &[&str] = &["Forging", "Tempering", "Smelting", "Hammering", "Annealing", "Quenching", "Shaping", "Polishing", "Mining", "Casting"];

/// What the reply is doing right now, as the working line says it; `None`
/// while it waits on you (the permission card says that).
fn step_label(c: &Chat, seed: usize) -> Option<String> {
    let blocks = match c.messages.last() {
        Some(Message::Assistant { blocks }) => blocks.as_slice(),
        _ => &[],
    };
    let clip = |s: &str| if s.chars().count() > 48 { format!("{}…", s.chars().take(47).collect::<String>()) } else { s.to_string() };
    let label = match blocks.last() {
        Some(Block::Permission { verdict: Verdict::Pending, .. }) => return None,
        Some(Block::Tool(t)) if t.status == ToolStatus::Running => match t.kind {
            ToolKind::Read => format!("Reading {}", clip(&t.target)),
            ToolKind::Edit => format!("Editing {}", clip(&t.target)),
            ToolKind::Write => format!("Writing {}", clip(&t.target)),
            ToolKind::Bash => match &t.note {
                Some(note) => clip(note),
                None => format!("Running {}", clip(&t.target)),
            },
            ToolKind::Search => format!("Searching for {}", clip(&t.target)),
            ToolKind::Web => format!("Fetching {}", t.target.split("://").nth(1).unwrap_or(&t.target).split('/').next().unwrap_or_default()),
        },
        Some(Block::Thinking { .. }) => "Thinking".into(),
        _ => VERBS[seed % VERBS.len()].to_string(),
    };
    Some(label)
}

impl Render for ChatView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
        let step = self.step.clone();
        list(self.list.clone(), move |ix, _window, cx| {
            let t = cx.theme().clone();
            let s = state.read(cx);
            let Some(chat) = s.chat() else { return div().into_any_element() };
            let body = match chat.messages.get(ix) {
                Some(m) => {
                    let after_user = ix == 0 || matches!(chat.messages.get(ix - 1), Some(Message::User { .. }));
                    let last_user = chat.messages.iter().rposition(|m| matches!(m, Message::User { .. }));
                    let ctx = ItemContext {
                        state: state.clone(),
                        chat: chat.id,
                        index: ix,
                        copied: s.copied.clone(),
                        busy: s.busy(),
                        last: ix + 1 == chat.messages.len(),
                        first_in_turn: after_user && matches!(m, Message::Assistant { .. }),
                        live_turn: s.busy() && last_user.is_none_or(|u| ix > u),
                    };
                    message::render(m, &ctx, &t)
                }
                None => match &step {
                    Some((label, since)) => working_line(&t, label, since.elapsed().as_secs()),
                    None => div().into_any_element(),
                },
            };
            div()
                .w_full()
                .flex()
                .justify_center()
                .px(t.space.xl)
                .when(ix == 0, |d| d.pt(t.space.xl))
                .child(div().w_full().max_w(t.size.column).pb(t.space.lg).child(body))
                .into_any_element()
        })
        .size_full()
        .flex_1()
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

/// The step's words with a light sweeping across them: each letter lifts
/// from muted to full ink as the sweep passes.
fn shimmer(label: &str, sweep: f32, t: &crate::theme::Theme) -> StyledText {
    let n = label.chars().count().max(1) as f32;
    let mut runs = vec![];
    for (i, ch) in label.chars().enumerate() {
        let x = (i as f32 + 0.5) / n;
        let lift = (-((x - sweep) / 0.16).powi(2)).exp();
        let mut f = font(t.font_sans.clone());
        f.weight = FontWeight::MEDIUM;
        runs.push(TextRun { len: ch.len_utf8(), font: f, color: t.colors.text_strong.opacity(0.5 + 0.5 * lift), background_color: None, underline: None, strikethrough: None });
    }
    StyledText::new(label.to_string()).with_runs(runs)
}

/// Forge's working line: the cube at work, what the reply is doing (with a
/// light sweeping across it), and how long that has taken.
fn working_line(t: &crate::theme::Theme, label: &str, seconds: u64) -> gpui::AnyElement {
    let theme = t.clone();
    let text = label.to_string();
    div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .child(
            div()
                .relative()
                .size(t.size.icon)
                .child(
                    div()
                        .absolute()
                        .child(super::mark::mark(t, t.size.icon))
                        .with_animation("working-cube", Animation::new(Duration::from_millis(650)).repeat(), |d, delta| {
                            // A strike: lift, then drop back quickly.
                            let lift = if delta < 0.7 { delta / 0.7 } else { 1. - (delta - 0.7) / 0.3 };
                            d.top(px(-2.5 * lift))
                        }),
                ),
        )
        .child(
            div().with_animation(SharedString::from(format!("working-{label}")), Animation::new(Duration::from_millis(2200)).repeat(), move |d, delta| {
                d.child(shimmer(&text, -0.3 + delta * 1.6, &theme))
            }),
        )
        .child(div().text_color(t.colors.text_subtle).child(elapsed(seconds)))
        .child(div().text_color(t.colors.text_subtle).opacity(0.7).child("· Esc to stop"))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::elapsed;

    #[test]
    fn elapsed_reads_short() {
        assert_eq!(elapsed(12), "12s");
        assert_eq!(elapsed(67), "1m 7s");
        assert_eq!(elapsed(3780), "1h 3m");
    }
}
