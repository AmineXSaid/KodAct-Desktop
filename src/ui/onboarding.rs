//! First run, in two steps: Forge asks what to call you, then you pick your
//! avatar. The steps slide past each other (forward to the avatar, back to
//! the name). The name and avatar are kept (`profile.rs`) and shown in the
//! sidebar. Reopened from the command palette: "Change your name" starts at
//! the name, "Change your avatar" at the avatars.

use gpui::component::input::{Input, InputEvent, InputState};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, relative, Animation, AnimationExt, AnyElement, Context, Entity,
    FocusHandle, FontWeight, IntoElement, Render, SharedString, Subscription, Task, Window,
};

use super::icon;
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
            _subs: subs,
        }
    }

    /// Show it at the name: the current name filled in, the field focused.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.read(cx).user_name.clone().unwrap_or_default();
        self.error = None;
        self.step = Step::Name;
        self.leaving = None;
        self.selected = self.state.read(cx).avatar.clone();
        self.input.update(cx, |i, cx| {
            i.set_value(current, window, cx);
            i.focus(window, cx);
        });
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
        let name = self.input.read(cx).value().to_string();
        match self.state.update(cx, |s, cx| s.set_user_name(&name, cx)) {
            Ok(()) => self.go(Step::Avatar, window, cx),
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
        let returning = self.state.read(cx).user_name.is_some();
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
                div().flex_1().child(
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
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(super::cube_image(96., window))
            .child(heading(
                &t,
                if returning {
                    "What should we call you now?"
                } else {
                    "Hello. What should we call you?"
                },
            ))
            .child(div().mt(t.space.xl).w(px(420.)).child(field))
            .child(
                div()
                    .mt(t.space.sm)
                    .h(t.text.caption * 1.6)
                    .w(px(420.))
                    .pl(t.space.lg)
                    .text_size(t.text.caption)
                    .text_color(t.extra.danger)
                    .children(self.error),
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

fn heading(t: &Theme, text: &str) -> impl IntoElement {
    div()
        .mt(t.space.xl)
        .text_center()
        .font_family(t.font_display.clone())
        .text_size(t.size.greeting)
        .line_height(relative(1.2))
        .text_color(t.colors.text_strong)
        .font_weight(FontWeight::NORMAL)
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

        div()
            .id("onboarding")
            .key_context("Onboarding")
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| this.key(ev, window, cx)),
            )
            .flex_1()
            .min_h_0()
            // A window shorter than the steps scrolls them, never cuts them.
            .overflow_y_scroll()
            .bg(t.extra.stage)
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
            )
    }
}
