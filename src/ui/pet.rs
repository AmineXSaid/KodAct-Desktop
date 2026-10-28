//! The pet beside the chat (`design::pets`): the one you picked, in the
//! mood of what Forge is doing. Running while it works, reviewing while it
//! reviews a design, waiting when it asks you something, a jump when a
//! reply ends, failed after an error, idle otherwise; a wave when it first
//! appears. Its frames are cut from the sheet off the window's thread.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{div, img, prelude::*, px, Context, Entity, IntoElement, Render, RenderImage, Window};

use crate::design::pets::{self, Mood};
use crate::state::{AppState, ChatStatus, Message};

/// The pet's width on screen; its height follows the cell (192 by 208).
const WIDTH: u32 = 72;

type Moods = Arc<Vec<(Mood, Vec<Arc<RenderImage>>)>>;

pub struct PetView {
    state: Entity<AppState>,
    /// The pet shown, and its frames once cut.
    pet: Option<String>,
    frames: Option<Moods>,
    mood: Mood,
    frame: usize,
    /// A mood shown for a moment (a wave, a jump), then back to the chat's.
    passing: Option<(Mood, Instant)>,
    was: Option<ChatStatus>,
    _tick: Option<gpui::Task<()>>,
}

fn to_render(frames: Vec<(Mood, pets::Frames)>) -> Moods {
    Arc::new(
        frames
            .into_iter()
            .map(|(m, list)| {
                let list = list
                    .into_iter()
                    .map(|mut rgba| {
                        // GPUI draws BGRA.
                        for p in rgba.pixels_mut() {
                            p.0.swap(0, 2);
                        }
                        Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]))
                    })
                    .collect();
                (m, list)
            })
            .collect(),
    )
}

impl PetView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this: &mut PetView, _, cx| this.sync(cx))
            .detach();
        let mut v = Self {
            state,
            pet: None,
            frames: None,
            mood: Mood::Idle,
            frame: 0,
            passing: None,
            was: None,
            _tick: None,
        };
        v.sync(cx);
        v
    }

    /// Follow the chosen pet and the chat's state.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        let want = s.pet.clone();
        let status = s.chat().map(|c| s.status(c));
        let reviewing = s.chat().is_some_and(|c| {
            c.messages.iter().rev().find_map(|m| match m {
                Message::User { text, .. } => Some(text.starts_with("Review ")),
                _ => None,
            }) == Some(true)
        });
        if want != self.pet {
            self.pet = want.clone();
            self.frames = None;
            self._tick = None;
            if let Some(id) = want {
                let sheet = pets::all()
                    .into_iter()
                    .find(|p| p.id == id)
                    .map(|p| p.sheet);
                if let Some(sheet) = sheet {
                    let task = cx.spawn(async move |this, cx| {
                        let cut = cx
                            .background_spawn(async move { pets::frames(&sheet, WIDTH * 2) })
                            .await;
                        let _ = this.update(cx, |v, cx| {
                            if let Ok(f) = cut {
                                v.frames = Some(to_render(f));
                                v.passing = Some((Mood::Waving, Instant::now()));
                                v.start(cx);
                            }
                        });
                    });
                    self._tick = Some(task);
                }
            }
        }
        // A reply that just ended: a jump, unless it failed.
        if matches!(self.was, Some(ChatStatus::Working))
            && !matches!(
                status,
                Some(ChatStatus::Working | ChatStatus::Error | ChatStatus::NeedsYou)
            )
        {
            self.passing = Some((Mood::Jumping, Instant::now()));
        }
        self.was = status;
        let mood = match status {
            Some(ChatStatus::Working) if reviewing => Mood::Review,
            Some(ChatStatus::Working) => Mood::Running,
            Some(ChatStatus::NeedsYou) => Mood::Waiting,
            Some(ChatStatus::Error) => Mood::Failed,
            _ => Mood::Idle,
        };
        if mood != self.mood {
            self.mood = mood;
            self.frame = 0;
        }
        cx.notify();
    }

    /// The mood showing now: a passing one while it lasts.
    fn now(&mut self) -> Mood {
        if let Some((m, at)) = self.passing {
            let lasts: u64 = m.timings().iter().sum::<u64>() * 2;
            if at.elapsed() < Duration::from_millis(lasts) {
                return m;
            }
            self.passing = None;
            self.frame = 0;
        }
        self.mood
    }

    /// Step through the frames, each for its time.
    fn start(&mut self, cx: &mut Context<Self>) {
        self._tick = Some(cx.spawn(async move |this, cx| loop {
            let wait = this
                .update(cx, |v, cx| {
                    let mood = v.now();
                    let times = mood.timings();
                    let count = v.count(mood).max(1);
                    v.frame = (v.frame + 1) % count;
                    cx.notify();
                    times.get(v.frame).copied().unwrap_or(150)
                })
                .ok();
            let Some(wait) = wait else { break };
            cx.background_executor()
                .timer(Duration::from_millis(wait))
                .await;
        }));
    }

    fn count(&self, mood: Mood) -> usize {
        self.frames
            .as_ref()
            .and_then(|f| f.iter().find(|(m, _)| *m == mood))
            .map_or(0, |(_, l)| l.len())
    }
}

impl Render for PetView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // In a chat it perches just above the composer's left edge; on the
        // start screen, in the corner.
        let in_chat = self.state.read(cx).current.is_some();
        let mood = self.now();
        let pic = self.frames.as_ref().and_then(|f| {
            let list = f
                .iter()
                .find(|(m, _)| *m == mood)
                .or_else(|| f.iter().find(|(m, _)| *m == Mood::Idle))
                .map(|(_, l)| l)?;
            list.get(self.frame % list.len().max(1)).cloned()
        });
        let h = WIDTH * pets::CELL.1 / pets::CELL.0;
        div()
            .absolute()
            .left(px(24.))
            // Just above the composer's one line and its tray.
            .bottom(px(if in_chat { 112. } else { 24. }))
            .w(px(WIDTH as f32))
            .h(px(h as f32))
            .children(pic.map(|p| img(p).size_full()))
    }
}
