//! First run, in two steps: KodAct asks what to call you, then you pick your
//! avatar. The steps slide past each other (forward to the avatar, back to
//! the name). The name and avatar are kept (`profile.rs`) and shown in the
//! sidebar. Reopened from the command palette: "Change your name" starts at
//! the name, "Change your avatar" at the avatars.
//!
//! The name step opens on KodAct's mark: the tile rises in and plays its
//! beat once (the cursor bar, then the run triangle: code, then act), the
//! question arrives word by word, and Enter makes the triangle act once
//! more before the avatars slide in. Reduced motion: all of it at rest.

use gpui::component::input::{Input, InputEvent, InputState};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, relative, Animation, AnimationExt, AnyElement, App, Context,
    Entity, FocusHandle, IntoElement, Render, SharedString, Subscription, Task, Window,
};

use super::{icon, mark, motion};
use crate::avatars;
use crate::state::AppState;
use crate::theme::{ActiveTheme, Theme};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Name,
    Avatar,
}

/// How long one step takes to slide out and the next to slide in.
const SLIDE: Duration = Duration::from_millis(520);
/// How far the steps travel as they cross.
const TRAVEL: f32 = 56.;
/// The avatar picker: each choice, and the one you're looking at.
const CHOICE: f32 = 56.;
const PREVIEW: f32 = 104.;
/// The name step's hero: its box, and the mark in it (whole pixels of the
/// mark's 16-pixel grid).
const HERO: f32 = 112.;
const CUBE: f32 = 80.;
/// The opening, in milliseconds from the step opening: the mark rises in
/// by `LAND_MS` and plays its beat until `BEAT_END_MS`; the question starts
/// at `HEADING_MS` and the field rises at `FIELD_MS`; all of it is over by
/// `INTRO_MS`.
const LAND_MS: f32 = 260.;
const BEAT_END_MS: f32 = 1400.;
const HEADING_MS: f32 = 320.;
const FIELD_MS: f32 = 640.;
const INTRO_MS: f32 = 1400.;
/// Enter: the run triangle acts once more before the avatars come in.
const STRIKE: Duration = Duration::from_millis(520);

pub struct Onboarding {
    state: Entity<AppState>,
    input: Entity<InputState>,
    error: Option<&'static str>,
    step: Step,
    /// The step sliding out while `step` slides in, and which way they go.
    leaving: Option<Step>,
    forward: bool,
    /// Bumped on each change of step, so its slide plays afresh.
    turn: usize,
    settle: Option<Task<()>>,
    /// The avatar picked on the second step.
    selected: SharedString,
    focus: FocusHandle,
    /// The dot field behind both steps (its ripples are the strikes).
    /// Bumped each time the name step opens, so the forging plays afresh.
    intro: usize,
    /// Bumped on each strike (Enter on a good name), and the wait for it.
    strike: usize,
    striking: Option<Task<()>>,
    /// Whether you had a name when the step opened (the question asks for
    /// it "now"); kept, so the question doesn't change as you strike.
    returning: bool,
    /// Opened by the opener, which has already forged the cube: the hero
    /// rests while the question and the field arrive.
    forged: bool,
    _subs: Vec<Subscription>,
}

impl Onboarding {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Your name"));
        let subs =
            vec![cx.subscribe_in(
                &input,
                window,
                |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    InputEvent::Change => {
                        this.error = None;
                        cx.notify();
                    }
                    _ => {}
                },
            )];
        let selected = state.read(cx).avatar.clone();
        Self {
            state,
            input,
            error: None,
            step: Step::Name,
            leaving: None,
            forward: true,
            turn: 0,
            settle: None,
            selected,
            focus: cx.focus_handle(),
            intro: 0,
            strike: 0,
            striking: None,
            returning: false,
            forged: false,
            _subs: subs,
        }
    }

    /// Play the opening: the mark rises in and plays its beat.
    fn play_intro(&mut self) {
        self.intro += 1;
        self.strike = 0;
        self.striking = None;
    }

    /// Show it at the name: the current name filled in, the field focused.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.read(cx).user_name.clone().unwrap_or_default();
        self.returning = self.state.read(cx).user_name.is_some();
        self.error = None;
        self.step = Step::Name;
        self.leaving = None;
        self.selected = self.state.read(cx).avatar.clone();
        self.input.update(cx, |i, cx| {
            i.set_value(current, window, cx);
            i.focus(window, cx);
        });
        self.forged = false;
        self.play_intro();
    }

    /// Show it at the name as the opener hands over: the mark it played
    /// rests in the hero; the question and the field arrive as usual.
    pub fn open_forged(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open(window, cx);
        self.forged = true;
    }

    /// Show it at the avatars.
    pub fn open_avatar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.step = Step::Avatar;
        self.leaving = None;
        self.selected = self.state.read(cx).avatar.clone();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn go(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        if step == self.step {
            return;
        }
        self.forward = step == Step::Avatar;
        self.leaving = Some(self.step);
        self.step = step;
        self.turn += 1;
        match step {
            Step::Avatar => window.focus(&self.focus, cx),
            Step::Name => self.input.update(cx, |i, cx| i.focus(window, cx)),
        }
        self.settle = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SLIDE).await;
            let _ = this.update(cx, |o, cx| {
                o.leaving = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.striking.is_some() || self.step != Step::Name {
            return;
        }
        let name = self.input.read(cx).value().to_string();
        match self.state.update(cx, |s, cx| s.set_user_name(&name, cx)) {
            Ok(()) if still(cx) => self.go(Step::Avatar, window, cx),
            // Strike the cube, a ring through the dots, then on to the avatars.
            Ok(()) => {
                self.strike += 1;
                self.striking = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(STRIKE).await;
                    let _ = this.update_in(cx, |o, window, cx| {
                        o.striking = None;
                        o.go(Step::Avatar, window, cx);
                    });
                }));
                cx.notify();
            }
            Err(e) => {
                self.error = Some(e);
                cx.notify();
            }
        }
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pick = self.selected.to_string();
        self.state.update(cx, |s, cx| {
            s.set_avatar(&pick, cx);
            s.finish_onboarding(cx);
        });
        window.dispatch_action(Box::new(super::FocusComposer), cx);
    }

    /// Arrows move through the avatars (left and right along a row, up and
    /// down between the rows), Enter keeps the one picked, Esc goes back.
    fn key(&mut self, ev: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.step != Step::Avatar {
            return;
        }
        let rows: [&[&str]; 2] = [&avatars::WOMEN, &avatars::MEN];
        let (r, c) = rows
            .iter()
            .enumerate()
            .find_map(|(r, row)| {
                row.iter()
                    .position(|a| *a == self.selected.as_ref())
                    .map(|c| (r, c))
            })
            .unwrap_or((0, 0));
        let pick = match ev.keystroke.key.as_str() {
            "right" => Some(rows[r][(c + 1) % rows[r].len()]),
            "left" => Some(rows[r][(c + rows[r].len() - 1) % rows[r].len()]),
            "down" | "up" => {
                let other = rows[1 - r];
                Some(other[c.min(other.len() - 1)])
            }
            "enter" => {
                self.finish(window, cx);
                None
            }
            "escape" => {
                self.go(Step::Name, window, cx);
                None
            }
            _ => return,
        };
        if let Some(p) = pick {
            self.selected = p.into();
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn name_step(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let charged = !still(cx)
            && self.error.is_none()
            && gpui::Focusable::focus_handle(self.input.read(cx), cx).is_focused(window);
        let returning = self.returning;
        let empty = self.input.read(cx).value().trim().is_empty();
        let hover = t.extra.fill_hover;
        let field = div()
            .w_full()
            .h(px(52.))
            .pl(t.space.lg)
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(t.space.sm)
            .rounded(t.radius.field)
            .bg(t.colors.surface)
            .border_1()
            .border_color(if self.error.is_some() {
                t.extra.danger
            } else {
                t.colors.border
            })
            .text_size(t.text.body)
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.input)
                        .appearance(false)
                        .text_size(t.text.body),
                ),
            )
            .child(
                div()
                    .id("onboarding-continue")
                    .size(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius.control)
                    .bg(if empty { t.colors.hover } else { t.extra.fill })
                    .when(!empty, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                    .child(icon(
                        "icons/arrow-right.svg",
                        t.size.icon,
                        if empty {
                            t.colors.text_subtle
                        } else {
                            t.extra.fill_fg
                        },
                    ))
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );
        // While it has the keyboard, the field's edge charges with the
        // accent: it brightens and dims, with a faint glow round it.
        let accent = t.colors.primary;
        let field = if charged {
            field
                .with_animation(
                    "onboarding-charge",
                    Animation::new(Duration::from_millis(2400)).repeat(),
                    move |d, k| {
                        let w = 0.5 + 0.5 * (k * std::f32::consts::TAU).sin();
                        d.border_color(accent.opacity(0.45 + 0.55 * w)).shadow(vec![
                            gpui::BoxShadow {
                                color: accent.opacity(0.08 + 0.1 * w),
                                offset: gpui::point(px(0.), px(0.)),
                                blur_radius: px(14.),
                                spread_radius: px(0.),
                                inset: false,
                            },
                        ])
                    },
                )
                .into_any_element()
        } else {
            field.into_any_element()
        };
        let below = div()
            .w(px(420.))
            .flex()
            .flex_col()
            .child(div().mt(t.space.xl).child(field))
            .child(
                div()
                    .mt(t.space.sm)
                    .h(t.text.caption * 1.6)
                    .pl(t.space.lg)
                    .text_size(t.text.caption)
                    .text_color(t.extra.danger)
                    .children(self.error),
            );
        // The field rises in once the question has arrived.
        let below = if still(cx) || self.intro == 0 {
            below.into_any_element()
        } else {
            below
                .with_animation(
                    SharedString::from(format!("onboarding-field-{}", self.intro)),
                    Animation::new(Duration::from_millis(INTRO_MS as u64)),
                    |d, k| {
                        let e = smooth((k * INTRO_MS - FIELD_MS) / 600.);
                        d.opacity(e).mt(px(10. * (1. - e)))
                    },
                )
                .into_any_element()
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(self.hero(cx))
            .child(arriving(
                &t,
                if returning {
                    "What should we call you now?"
                } else {
                    "Hello. What should we call you?"
                },
                if still(cx) { 0 } else { self.intro },
            ))
            .child(below)
            .into_any_element()
    }

    /// The mark: rising in and playing its beat as the step opens, its
    /// run triangle acting when you press Enter, at rest otherwise.
    fn hero(&self, cx: &App) -> AnyElement {
        let frame = div().relative().size(px(HERO)).flex_none();
        if still(cx) || (self.forged && self.strike == 0) {
            return frame.child(placed(mark::tile(CUBE), 0., 1.)).into_any_element();
        }
        if self.strike > 0 {
            // From the cursor bar to the run triangle and back: one act.
            return frame
                .child(div().absolute().inset_0().with_animation(
                    ("onboarding-strike", self.strike),
                    Animation::new(STRIKE),
                    move |d, k| d.child(placed(mark::tile_at_beat(CUBE, k), 0., 1.)),
                ))
                .into_any_element();
        }
        frame
            .with_animation(
                SharedString::from(format!("onboarding-mark-{}", self.intro)),
                Animation::new(Duration::from_millis(INTRO_MS as u64)),
                move |d, k| {
                    let ms = k * INTRO_MS;
                    let land = smooth(ms / LAND_MS);
                    let beat = (ms / BEAT_END_MS).clamp(0., 1.);
                    d.child(placed(
                        mark::tile_at_beat(CUBE, beat),
                        10. * (1. - land),
                        land,
                    ))
                },
            )
            .into_any_element()
    }

    fn avatar_step(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let name = self.state.read(cx).user_name.clone().unwrap_or_default();
        let selected = self.selected.clone();
        let row = |list: &'static [&'static str], cx: &mut Context<Self>| {
            div()
                .flex()
                .justify_center()
                .gap(t.space.md)
                .children(list.iter().map(|a| {
                    let picked = selected.as_ref() == *a;
                    let a = *a;
                    let (ring, hover) = (t.colors.primary, t.colors.border);
                    // A ring round the one picked; a faint one on hover.
                    div()
                        .id(SharedString::from(format!("avatar-{a}")))
                        .p(px(3.))
                        .rounded(t.radius.pill)
                        .border_2()
                        .border_color(if picked {
                            ring
                        } else {
                            t.colors.border.opacity(0.)
                        })
                        .cursor_pointer()
                        .when(!picked, |d| d.hover(move |st| st.border_color(hover)))
                        .child(super::avatar_image(a, CHOICE, window))
                        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                            this.selected = a.into();
                            // A double click keeps it straight away.
                            if ev.click_count() > 1 {
                                this.finish(window, cx);
                            }
                            cx.notify();
                        }))
                }))
        };
        let hover = t.colors.hover;
        let back = div()
            .id("onboarding-back")
            .px(t.space.md)
            .py(px(6.))
            .rounded(t.radius.button)
            .text_size(t.text.body_sm)
            .text_color(t.colors.text_muted)
            .cursor_pointer()
            .hover(move |st| st.bg(hover))
            .child("Back")
            .on_click(cx.listener(|this, _, window, cx| this.go(Step::Name, window, cx)));
        let keep = crate::theme::primary_button(&t, "Continue")
            .id("onboarding-keep")
            .on_click(cx.listener(|this, _, window, cx| this.finish(window, cx)));
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            // The one you're looking at, large; it settles in each time you pick.
            .child(
                div()
                    .size(px(PREVIEW))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(super::avatar_image(&self.selected, PREVIEW, window))
                    .with_animation(
                        SharedString::from(format!("preview-{}", self.selected)),
                        Animation::new(Duration::from_millis(260))
                            .with_easing(gpui::ease_out_quint()),
                        |d, k| d.opacity(0.4 + 0.6 * k).mt(px(6. * (1. - k))),
                    ),
            )
            .child(heading(&t, &format!("Pick your avatar, {name}")))
            .child(
                div()
                    .mt(t.space.xl)
                    .flex()
                    .flex_col()
                    .gap(t.space.md)
                    .child(row(&avatars::WOMEN, cx))
                    .child(row(&avatars::MEN, cx)),
            )
            .child(
                div()
                    .mt(t.space.xl)
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(back)
                    .child(keep),
            )
            .into_any_element()
    }
}

/// Under reduced motion, nothing moves.
fn still(cx: &App) -> bool {
    motion::reduced_motion() || cx.reduce_motion()
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}

/// The mark in the hero's box, centred, `rise` px low and at `alpha`.
fn placed(tile: mark::Tile, rise: f32, alpha: f32) -> impl IntoElement {
    div()
        .absolute()
        .left(px((HERO - CUBE) / 2.))
        .top(px((HERO - CUBE) / 2. + rise))
        .opacity(alpha)
        .child(tile)
}

/// The question, arriving word by word as the cube cools (`intro` 0: at once).
fn arriving(t: &Theme, text: &str, intro: usize) -> AnyElement {
    if intro == 0 {
        return heading(t, text).into_any_element();
    }
    div()
        .mt(t.space.xl)
        .flex()
        .flex_wrap()
        .justify_center()
        .gap_x(t.size.greeting * 0.26)
        .font_family(t.font_display.clone())
        .font_weight(t.display_weight)
        .text_size(t.size.greeting)
        .line_height(relative(1.2))
        .text_color(t.colors.text_strong)
        .children(text.split(' ').enumerate().map(|(i, word)| {
            div().child(word.to_string()).with_animation(
                SharedString::from(format!("onboarding-word-{intro}-{i}")),
                Animation::new(Duration::from_millis(INTRO_MS as u64)),
                move |d, k| {
                    let e = smooth((k * INTRO_MS - HEADING_MS - i as f32 * 70.) / 420.);
                    d.opacity(e).mt(px(8. * (1. - e)))
                },
            )
        }))
        .into_any_element()
}

fn heading(t: &Theme, text: &str) -> impl IntoElement {
    div()
        .mt(t.space.xl)
        .text_center()
        .font_family(t.font_display.clone())
        .font_weight(t.display_weight)
        .text_size(t.size.greeting)
        .line_height(relative(1.2))
        .text_color(t.colors.text_strong)
        .child(text.to_string())
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        // The step coming in slides from one side as the one leaving slides
        // to the other, both fading: forward to the avatars, back to the name.
        let dir = if self.forward { 1. } else { -1. };
        let step = |this: &Self, step: Step, window: &Window, cx: &mut Context<Self>| match step {
            Step::Name => this.name_step(window, cx),
            Step::Avatar => this.avatar_step(window, cx),
        };
        let first = self.turn == 0;
        let entering = div()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .child(step(self, self.step, window, cx))
            .with_animation(
                SharedString::from(format!("onboarding-in-{}", self.turn)),
                Animation::new(if first {
                    Duration::from_millis(t.motion_ms * 2)
                } else {
                    SLIDE
                })
                .with_easing(gpui::ease_out_quint()),
                move |d, k| {
                    if first {
                        d.opacity(k).mt(px(10. * (1. - k)))
                    } else {
                        d.opacity(k).ml(px(TRAVEL * dir * (1. - k)))
                    }
                },
            );
        let leaving = self.leaving.map(|s| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .child(step(self, s, window, cx))
                .with_animation(
                    SharedString::from(format!("onboarding-out-{}", self.turn)),
                    Animation::new(SLIDE).with_easing(gpui::ease_out_quint()),
                    move |d, k| d.opacity((1. - 1.6 * k).max(0.)).ml(px(-TRAVEL * dir * k)),
                )
        });

        div().flex_1().min_h_0().relative().bg(t.extra.stage).child(
            div()
                .id("onboarding")
                .key_context("Onboarding")
                .track_focus(&self.focus)
                .on_key_down(
                    cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| {
                        this.key(ev, window, cx)
                    }),
                )
                .absolute()
                .inset_0()
                // A window shorter than the steps scrolls them, never cuts them.
                .overflow_y_scroll()
                .child(
                    div()
                        .min_h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .pt(t.space.xl)
                        .pb(t.space.xxl * 2.)
                        .child(
                            div()
                                .flex_none()
                                .relative()
                                .w(px(560.))
                                .h(px(480.))
                                .children(leaving)
                                .child(entering),
                        ),
                ),
        )
    }
}
