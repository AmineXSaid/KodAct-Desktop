//! The window's root view: title bar, sidebar, the conversation (or the
//! welcome screen), the composer, the command palette and the toast. Every
//! app-level action is handled here.

use gpui::component::TitleBar;
use gpui::{
    div, prelude::*, px, relative, ClipboardItem, Context, Entity, FocusHandle, FontWeight, IntoElement, Render,
    SharedString, Window,
};

use super::chat_view::ChatView;
use super::command_palette::CommandPalette;
use super::composer::Composer;
use super::onboarding::Onboarding;
use super::rules_view::RulesView;
use super::sidebar::Sidebar;
use super::*;
use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme};

pub struct Workspace {
    state: Entity<AppState>,
    sidebar: Entity<Sidebar>,
    chat: Entity<ChatView>,
    composer: Entity<Composer>,
    palette: Entity<CommandPalette>,
    onboarding: Entity<Onboarding>,
    rules: Entity<RulesView>,
    focus: FocusHandle,
}

const SUGGESTIONS: &[&str] = &[
    "Explain how this project is structured",
    "Find and fix a failing test",
    "Review my uncommitted changes",
    "Write a README for this project",
];

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| AppState::new());
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let sidebar = cx.new(|cx| Sidebar::new(state.clone(), window, cx));
        let chat = cx.new(|cx| ChatView::new(state.clone(), cx));
        let composer = cx.new(|cx| Composer::new(state.clone(), window, cx));
        let palette = cx.new(|cx| CommandPalette::new(state.clone(), window, cx));
        let onboarding = cx.new(|cx| Onboarding::new(state.clone(), window, cx));
        let rules = cx.new(|cx| RulesView::new(state.clone(), window, cx));
        let first_run = state.read(cx).onboarding;
        let ws = Self { state, sidebar, chat, composer, palette, onboarding, rules, focus: cx.focus_handle() };
        if first_run {
            ws.onboarding.update(cx, |o, cx| o.open(window, cx));
        } else {
            ws.composer.update(cx, |c, cx| c.focus(window, cx));
        }
        ws
    }

    fn focus_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| c.focus(window, cx));
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let hover = t.colors.hover;
        let button = |id: &'static str, path: &'static str| {
            div()
                .id(id)
                .size(t.size.control)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover(move |st| st.bg(hover))
                .child(icon(path, t.size.icon, t.colors.text_muted))
        };
        TitleBar::new().h(t.size.titlebar).child(
            div()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .w_full()
                .pr(t.space.sm)
                .child(button("toggle-sidebar", "icons/panel-left.svg").on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(ToggleSidebar), cx))))
                .child(
                    div()
                        .id("project")
                        .flex()
                        .items_center()
                        .gap(t.space.xs)
                        .px(t.space.sm)
                        .h(t.size.control)
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .hover(move |st| st.bg(hover))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(s.project.clone())
                        .child(icon("icons/chevron-down.svg", t.size.icon_sm, t.colors.text_subtle))
                        .on_click(cx.listener(|this, _, _, cx| this.state.update(cx, |s, cx| s.show_toast("Switching projects is a stub in this UI shell.", cx)))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.xs)
                        .px(t.space.sm)
                        .py(px(2.))
                        .rounded(t.radius.pill)
                        .bg(t.colors.hover)
                        .text_size(t.text.caption)
                        .font_family(t.font_mono.clone())
                        .text_color(t.colors.text_muted)
                        .child(icon("icons/git-branch.svg", t.size.icon_sm - px(2.), t.colors.text_muted))
                        .child(s.branch.clone()),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id("palette-button")
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .h(t.size.control)
                        .px(t.space.md)
                        .min_w(px(240.))
                        .rounded(t.radius.button)
                        .bg(t.colors.hover)
                        .cursor_pointer()
                        .text_color(t.colors.text_subtle)
                        .child(icon("icons/search.svg", t.size.icon_sm, t.colors.text_subtle))
                        .child(div().flex_1().child("Search or run a command"))
                        .child(kbd(&t, shortcut("K")))
                        .on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(ToggleCommandPalette), cx))),
                )
                .child(div().flex_1())
                .child(
                    button("toggle-theme", if t.is_dark { "icons/sun.svg" } else { "icons/moon.svg" })
                        .on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(ToggleTheme), cx))),
                )
                .child(button("settings", "icons/settings.svg").on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)))),
        )
    }

    fn welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let project = self.state.read(cx).project.clone();
        let where_ = crate::rules::display_path(&self.state.read(cx).project_dir, crate::rules::home_dir().as_deref());
        let name = self.state.read(cx).user_name.clone().unwrap_or_default();
        let composer = self.composer.clone();
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(t.space.xl)
            .pb(t.space.xxl * 3.)
            .child(
                div()
                    .w_full()
                    .max_w(t.size.column - t.space.xxl * 2.)
                    .flex()
                    .flex_col()
                    .gap(t.space.lg)
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .items_center()
                            .gap(t.space.sm)
                            .font_family(t.font_serif.clone())
                            .text_size(t.text.subheading - px(2.))
                            .text_color(t.colors.text_body)
                            .child(super::mark::mark(&t, px(32.)))
                            .child(format!("Welcome back, {name}")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .justify_center()
                            .items_center()
                            .gap(t.space.sm)
                            .text_size(t.size.display)
                            .line_height(relative(1.2))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.text_strong)
                            .child("What should we build in")
                            // The pill and the "?" wrap together.
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .child(div().px(t.space.md).rounded(t.radius.pill).bg(t.extra.highlight).child(project.clone()))
                                    .child("?"),
                            ),
                    )
                    .child(
                        div()
                            .text_center()
                            .font_family(t.font_serif.clone())
                            .text_size(t.text.subheading - px(2.))
                            .line_height(relative(1.56))
                            .text_color(t.colors.text_body)
                            .pb(t.space.md)
                            .child(format!("Forge works in {where_} and asks before it changes anything.")),
                    )
                    .child(self.composer.clone())
                    .child(
                        div().flex().flex_wrap().justify_center().gap(t.space.sm).children(SUGGESTIONS.iter().enumerate().map(|(i, s)| {
                            let hover = t.colors.text_subtle;
                            let composer = composer.clone();
                            div()
                                .id(("suggestion", i))
                                .px(t.space.md)
                                .py(t.space.xs + px(2.))
                                .rounded(t.radius.pill)
                                .bg(t.colors.surface)
                                .border_1()
                                .border_color(t.colors.border)
                                .text_color(t.colors.text_muted)
                                .text_size(t.text.body_sm - px(1.))
                                .cursor_pointer()
                                .hover(move |st| st.border_color(hover))
                                .child(*s)
                                .on_click(move |_, window, cx| composer.update(cx, |c, cx| c.fill(s, window, cx)))
                        })),
                    ),
            )
    }

    fn toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let t = cx.theme().clone();
        let text: SharedString = self.state.read(cx).toast.clone()?;
        Some(
            div().absolute().bottom(t.space.xl).left_0().right_0().flex().justify_center().child(
                theme::popover(&t)
                    .px(t.space.lg)
                    .py(t.space.sm)
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(icon("icons/info.svg", t.size.icon_sm, t.colors.text_muted))
                    .child(text),
            ),
        )
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let (sidebar_open, palette_open, onboarding, screen) = (s.sidebar_open, s.palette_open, s.onboarding, s.screen);
        let empty = s.chat().is_none_or(|c| c.messages.is_empty());

        let main = div().flex_1().min_w_0().flex().flex_col().bg(t.colors.canvas);
        let main = if screen == Screen::Rules {
            main.child(self.rules.clone())
        } else if empty {
            main.child(self.welcome(cx))
        } else {
            main.child(self.chat.clone()).child(
                div()
                    .flex()
                    .justify_center()
                    .px(t.space.xl)
                    .pb(t.space.lg)
                    .child(div().w_full().max_w(t.size.column).child(self.composer.clone())),
            )
        };

        theme::app_root(&t)
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .relative()
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &NewChat, window, cx| {
                this.state.update(cx, |s, cx| s.new_chat(cx));
                this.focus_composer(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.sidebar_open = !s.sidebar_open;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                let open = !this.state.read(cx).palette_open;
                this.state.update(cx, |s, cx| {
                    s.palette_open = open;
                    s.picker_open = false;
                    cx.notify();
                });
                if open {
                    this.palette.update(cx, |p, cx| p.open(window, cx));
                } else {
                    this.focus_composer(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _, cx| {
                let dark = !this.state.read(cx).dark;
                this.state.update(cx, |s, cx| {
                    s.dark = dark;
                    cx.notify();
                });
                set_dark(dark, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusComposer, window, cx| this.focus_composer(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleModelPicker, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.picker_open = !s.picker_open;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &CycleMode, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.mode = s.mode.next();
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &StopReply, window, cx| {
                let (palette, picker) = {
                    let s = this.state.read(cx);
                    (s.palette_open, s.picker_open)
                };
                this.state.update(cx, |s, cx| {
                    if palette || picker {
                        s.palette_open = false;
                        s.picker_open = false;
                        cx.notify();
                    } else {
                        s.stop(cx);
                    }
                });
                if palette {
                    this.focus_composer(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                this.state.update(cx, |s, cx| s.show_toast("Settings open in their own window in the full app; this shell stubs them.", cx))
            }))
            .on_action(cx.listener(|this, _: &CopyLastReply, _, cx| {
                let reply = this.state.read(cx).last_reply();
                this.state.update(cx, |s, cx| match reply {
                    Some(text) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        s.show_toast("Copied the last reply.", cx);
                    }
                    None => s.show_toast("No reply to copy yet.", cx),
                });
            }))
            .on_action(cx.listener(|this, _: &ChangeName, window, cx| {
                this.state.update(cx, |s, cx| {
                    s.onboarding = true;
                    s.palette_open = false;
                    cx.notify();
                });
                this.onboarding.update(cx, |o, cx| o.open(window, cx));
            }))
            .on_action(cx.listener(|this, _: &OpenRules, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.screen = Screen::Rules;
                    s.palette_open = false;
                    s.picker_open = false;
                    cx.notify();
                });
                this.rules.update(cx, |r, cx| r.reload(cx));
            }))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .child(self.title_bar(cx))
            .when(onboarding, |d| d.child(self.onboarding.clone()))
            .when(!onboarding, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .when(sidebar_open, |d| d.child(self.sidebar.clone()))
                        .child(main),
                )
            })
            .when(palette_open, |d| d.child(self.palette.clone()))
            .children(self.toast(cx))
    }
}
