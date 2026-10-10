//! The conversation: a virtualized list (GPUI's `list`) that follows the
//! newest message while you are at the bottom and stays put when you scroll
//! up to read. While a reply plays, a working line sits at the end.

use std::time::Duration;

use gpui::{
    div, list, percentage, prelude::*, px, Animation, AnimationExt, Context, Entity, FollowMode, IntoElement, ListAlignment,
    ListState, Render, Task, Transformation, Window,
};

use super::message::{self, ItemContext};
use super::*;
use crate::state::{AppState, Message};
use crate::theme::ActiveTheme;

pub struct ChatView {
    state: Entity<AppState>,
    list: ListState,
    shown_chat: Option<usize>,
    shown_count: usize,
    shown_version: u64,
    /// Re-renders the working line's timer every second while a reply plays.
    ticker: Option<Task<()>>,
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
        let mut view = Self { state, list, shown_chat: None, shown_count: 0, shown_version: 0, ticker: None };
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
    }
}

impl Render for ChatView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.clone();
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
                None => working_line(&t, s.turn_started.map(|i| i.elapsed().as_secs()).unwrap_or(0)),
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

/// Forge's working indicator: a turning mark, a verb, the elapsed time.
fn working_line(t: &crate::theme::Theme, seconds: u64) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .pl(t.space.xs)
        .text_color(t.colors.text_muted)
        .child(
            icon("icons/loader-circle.svg", t.size.icon, t.colors.primary)
                .with_animation("working", Animation::new(Duration::from_millis(900)).repeat(), |svg, delta| {
                    svg.with_transformation(Transformation::rotate(percentage(delta)))
                }),
        )
        .child("Working")
        .child(div().text_color(t.colors.text_subtle).child(format!("{seconds}s · Esc to stop")))
        .into_any_element()
}
