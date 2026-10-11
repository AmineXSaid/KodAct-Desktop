//! The window's root view: title bar, sidebar, the conversation (or the
//! welcome screen), the composer, the command palette and the toast. Every
//! app-level action is handled here.

use gpui::component::TitleBar;
use gpui::{
    div, prelude::*, px, AnimationExt, ClipboardItem, Context, Entity, FocusHandle, FontWeight, IntoElement, Render,
    SharedString, Window,
};

use super::chat_view::ChatView;
use super::command_palette::CommandPalette;
use super::composer::Composer;
use super::onboarding::Onboarding;
use super::projects::ProjectsView;
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
    projects: Entity<ProjectsView>,
    focus: FocusHandle,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| {
            let mut s = AppState::new();
            s.load(cx);
            s
        });
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let sidebar = cx.new(|cx| Sidebar::new(state.clone(), window, cx));
        let chat = cx.new(|cx| ChatView::new(state.clone(), cx));
        let composer = cx.new(|cx| Composer::new(state.clone(), window, cx));
        let palette = cx.new(|cx| CommandPalette::new(state.clone(), window, cx));
        let onboarding = cx.new(|cx| Onboarding::new(state.clone(), window, cx));
        let rules = cx.new(|cx| RulesView::new(state.clone(), window, cx));
        let projects = cx.new(|cx| ProjectsView::new(state.clone(), composer.clone(), cx));
        let first_run = state.read(cx).onboarding;
        let ws = Self { state, sidebar, chat, composer, palette, onboarding, rules, projects, focus: cx.focus_handle() };
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
        // Deferred so it paints above the page under the title bar.
        let switcher = self.state.read(cx).switcher_open.then(|| gpui::deferred(self.switcher(cx)).with_priority(1));
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
                        .relative()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            if this.state.read(cx).switcher_open {
                                this.state.update(cx, |s, cx| {
                                    s.switcher_open = false;
                                    cx.notify();
                                });
                            }
                        }))
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
                                .when(s.switcher_open, |d| d.bg(hover))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                                .child(s.project.clone())
                                .child(icon(if s.switcher_open { "icons/chevron-up.svg" } else { "icons/chevron-down.svg" }, t.size.icon_sm, t.colors.text_subtle))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.update(cx, |s, cx| {
                                        s.switcher_open = !s.switcher_open;
                                        cx.notify();
                                    })
                                })),
                        )
                        .children(switcher),
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

    /// A new chat: one line greeting you, with your cube, and the composer.
    fn welcome(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let line = crate::greeting::pick(s.user_name.as_deref().unwrap_or_default(), crate::greeting::hour_now(), s.greeting_seed);
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
                    .gap(t.space.xl)
                    .child(
                        div()
                            .flex()
                            .justify_center()
                            .items_center()
                            .gap(t.space.md)
                            .font_family(t.font_serif.clone())
                            .text_size(t.size.greeting)
                            .text_color(t.colors.text_strong)
                            .child(super::cube_image(32., window))
                            .child(line),
                    )
                    .child(self.composer.clone()),
            )
    }

    /// The title bar's project switcher: every project, then All projects
    /// and Open folder….
    fn switcher(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let home = crate::rules::home_dir();
        let hover = t.colors.hover;
        let mut projects = s.projects.clone();
        projects.sort_by_key(|p| (p.id != s.current_project, !p.pinned, p.opened_minutes_ago));
        let row = |id: SharedString| div().id(id).flex().items_center().gap(t.space.sm).px(t.space.md).py(t.space.sm).rounded(t.radius.button).cursor_pointer().hover(move |st| st.bg(hover));
        let mut list = div().flex().flex_col().child(div().px(t.space.md).pt(t.space.sm).pb(t.space.xs).text_size(t.text.caption).font_weight(FontWeight::MEDIUM).text_color(t.colors.text_subtle).child("Projects"));
        for p in projects {
            let (state, id) = (self.state.clone(), p.id);
            let current = p.id == s.current_project;
            list = list.child(
                row(format!("switch-{}", p.id).into())
                    .child(icon("icons/folder-open.svg", t.size.icon_sm, if current { t.colors.primary } else { t.colors.text_muted }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_color(t.colors.text_strong).child(p.name.clone()))
                            .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().font_family(t.font_mono.clone()).text_size(t.text.caption).text_color(t.colors.text_subtle).child(crate::rules::display_path(&p.path, home.as_deref()))),
                    )
                    .child(div().w(t.size.icon).when(current, |d| d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.primary))))
                    .on_click(move |_, _, cx| {
                        state.update(cx, |s, cx| {
                            s.select_project(id, cx);
                            if !matches!(s.screen, Screen::Rules) {
                                s.screen = Screen::Chat;
                            }
                        })
                    }),
            );
        }
        let (all, open) = (self.state.clone(), self.state.clone());
        theme::popover(&t)
            .absolute()
            .top(t.size.control + t.space.xs)
            .left_0()
            .w(px(320.))
            .child(list)
            .child(div().my(t.space.xs).h(t.size.hairline).bg(t.colors.border))
            .child(row("switch-all".into()).child(icon("icons/list-todo.svg", t.size.icon_sm, t.colors.text_muted)).child("All projects").on_click(move |_, _, cx| {
                all.update(cx, |s, cx| {
                    s.switcher_open = false;
                    s.screen = Screen::Projects;
                    cx.notify();
                })
            }))
            .child(row("switch-open".into()).child(icon("icons/plus.svg", t.size.icon_sm, t.colors.text_muted)).child("Open folder…").on_click(move |_, _, cx| {
                open.update(cx, |s, cx| {
                    s.switcher_open = false;
                    cx.notify();
                });
                super::projects::open_folder(open.clone(), cx);
            }))
            .with_animation("switcher-in", gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms)).with_easing(gpui::ease_in_out), |d, delta| d.opacity(delta))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let (sidebar_open, palette_open, onboarding, screen, loading) = (s.sidebar_open, s.palette_open, s.onboarding, s.screen, s.loading);
        let empty = s.chat().is_none_or(|c| c.messages.is_empty());

        let main = div().flex_1().min_w_0().flex().flex_col().bg(t.extra.stage);
        let main = if screen == Screen::Rules {
            main.child(self.rules.clone())
        } else if matches!(screen, Screen::Projects | Screen::Project(_)) {
            main.child(self.projects.clone())
        } else if empty {
            main.child(self.welcome(window, cx))
        } else if loading {
            main.child(div().flex_1().min_h_0().child(super::skeleton::transcript(&t))).child(
                div()
                    .flex()
                    .justify_center()
                    .px(t.space.xl)
                    .pb(t.space.lg)
                    .child(div().w_full().max_w(t.size.column).child(self.composer.clone())),
            )
        } else {
            main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(self.chat.clone())
                    .with_animation("transcript-in", gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms)), |d, delta| d.opacity(delta)),
            )
            .child(
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
                    s.modes_open = false;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &CycleMode, _, cx| {
                this.state.update(cx, |s, cx| s.cycle_mode(cx))
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
            .on_action(cx.listener(|this, _: &OpenProjects, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.screen = Screen::Projects;
                    s.palette_open = false;
                    s.switcher_open = false;
                    cx.notify();
                })
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
