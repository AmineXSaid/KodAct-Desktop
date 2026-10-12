//! Projects: the folders Forge works in.
//!
//! - The Projects page lists every project as a card (name, folder, branch,
//!   how many chats, when it was last opened); Open folder… adds one with
//!   the system's folder picker.
//! - A project's page has a composer to start a chat there, its chats with
//!   their status, and its actions: pin it to the sidebar, open its rules.
//!
//! Chats, rules and memory belong to a project; switching project (here, in
//! the title bar, or by opening one of its chats) moves the whole window to
//! that folder.

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, FontWeight, IntoElement, PathPromptOptions, Render, SharedString, Window};

use super::composer::Composer;
use super::sidebar::{ago, status_mark};
use super::*;
use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme, Theme};

pub struct ProjectsView {
    state: Entity<AppState>,
    composer: Entity<Composer>,
}

impl ProjectsView {
    pub fn new(state: Entity<AppState>, composer: Entity<Composer>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state, composer }
    }
}

/// Ask for a folder with the system picker and add it as a project.
pub fn open_folder(state: Entity<AppState>, cx: &mut gpui::App) {
    let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Open as a project".into()) });
    cx.spawn(async move |cx| {
        let result = picked.await;
        state.update(cx, |s, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        s.add_project(path, cx);
                    }
                }
                Ok(Ok(None)) => {}
                _ => s.show_toast("The folder picker couldn't open on this system.", cx),
            });
    })
    .detach();
}

fn folder_badge(t: &Theme, size: gpui::Pixels) -> impl IntoElement {
    div()
        .size(size)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .bg(t.colors.primary_tint)
        .child(icon("icons/folder-open.svg", size * 0.5, t.colors.primary))
}

fn branch_pill(t: &Theme, branch: SharedString) -> impl IntoElement {
    div()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .px(t.space.sm)
        .py(px(1.))
        .rounded(t.radius.pill)
        .bg(t.colors.hover)
        .font_family(t.font_mono.clone())
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(icon("icons/git-branch.svg", t.size.icon_xs, t.colors.text_muted))
        .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(branch))
}

impl ProjectsView {
    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.state.read(cx);
        let home = crate::rules::home_dir();
        let hover = t.colors.text_subtle;
        let mut projects = s.projects.clone();
        projects.sort_by_key(|p| (!p.pinned, p.opened_minutes_ago));
        let cards = projects.into_iter().map(|p| {
            let chats = s.chats.iter().filter(|c| c.project == p.id).count();
            let current = p.id == s.current_project;
            let state = self.state.clone();
            let id = p.id;
            div()
                .id(("project-card", p.id))
                .w(px(300.))
                .flex()
                .flex_col()
                .gap(t.space.md)
                .p(t.space.lg)
                .bg(t.colors.surface)
                .border_1()
                .border_color(t.colors.border)
                .rounded(t.radius.card)
                .cursor_pointer()
                .hover(move |st| st.border_color(hover))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.md)
                        .child(folder_badge(t, px(36.)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child(p.name.clone()))
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .font_family(t.font_mono.clone())
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_subtle)
                                        .child(crate::rules::display_path(&p.path, home.as_deref())),
                                ),
                        )
                        .when(p.pinned, |d| d.child(icon("icons/pin.svg", t.size.icon_sm, t.colors.primary))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(branch_pill(t, p.branch.clone()))
                        .child(div().flex_1())
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .child(format!(
                                    "{chats} chat{} · {}",
                                    if chats == 1 { "" } else { "s" },
                                    if current { "open now".to_string() } else { format!("opened {} ago", ago(p.opened_minutes_ago)) }
                                )),
                        ),
                )
                .on_click(move |_, _, cx| state.update(cx, |s, cx| s.show_project(id, cx)))
        });
        let state = self.state.clone();
        div()
            .id("projects-page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(t.space.xxl)
            .pt(t.space.xl)
            .pb(t.space.xl)
            .flex()
            .flex_col()
            .gap(t.space.xl)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(t.space.lg)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(t.space.xs)
                            .child(div().text_size(t.text.heading_sm).font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child("Projects"))
                            .child(
                                div()
                                    .font_family(t.font_serif.clone())
                                    .text_size(t.text.body)
                                    .text_color(t.colors.text_body)
                                    .child("Each project is a folder Forge works in. Its chats, rules and memory stay with it."),
                            ),
                    )
                    .child(
                        theme::ghost_button(t, "Open folder…")
                            .id("open-folder")
                            .on_click(move |_, _, cx| open_folder(state.clone(), cx)),
                    ),
            )
            .child(div().flex().flex_wrap().gap(t.space.lg).children(cards))
            .into_any_element()
    }

    fn project(&self, id: usize, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.state.read(cx);
        let Some(p) = s.project(id).cloned() else { return div().into_any_element() };
        let home = crate::rules::home_dir();
        let hover = t.colors.hover;
        let mut chats: Vec<_> = s.chats.iter().filter(|c| c.project == id).collect();
        chats.sort_by_key(|c| c.minutes_ago);
        let rows: Vec<AnyElement> = chats
            .iter()
            .map(|c| {
                let state = self.state.clone();
                let chat = c.id;
                div()
                    .id(("project-chat", c.id))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .h(t.size.row + px(8.))
                    .px(t.space.md)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover))
                    .child(status_mark(s.status(c), c.id, t))
                    .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.colors.text).child(c.title.clone()))
                    .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(ago(c.minutes_ago)))
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| s.open_chat(chat, cx)))
                    .into_any_element()
            })
            .collect();
        let (pin_state, back_state) = (self.state.clone(), self.state.clone());
        let pinned = p.pinned;
        div()
            .id("project-page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .px(t.space.xl)
            .pt(t.space.xl)
            .pb(t.space.xl)
            .child(
                div()
                    .w_full()
                    .max_w(t.size.column)
                    .flex()
                    .flex_col()
                    .gap(t.space.lg)
                    .child(
                        div()
                            .id("back-to-projects")
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .cursor_pointer()
                            .hover(|st| st.underline())
                            .child("Projects")
                            .child(icon("icons/chevron-right.svg", t.size.icon_xs, t.colors.text_subtle))
                            .child(div().text_color(t.colors.text_muted).child(p.name.clone()))
                            .on_click(move |_, _, cx| {
                                back_state.update(cx, |s, cx| {
                                    s.screen = Screen::Projects;
                                    cx.notify();
                                })
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.md)
                            .child(folder_badge(t, px(44.)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(div().text_size(t.text.heading_sm).font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child(p.name.clone()))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(t.space.sm)
                                            .child(div().font_family(t.font_mono.clone()).text_size(t.text.caption).text_color(t.colors.text_subtle).child(crate::rules::display_path(&p.path, home.as_deref())))
                                            .child(branch_pill(t, p.branch.clone())),
                                    ),
                            )
                            .child(
                                div()
                                    .id("project-pin")
                                    .flex()
                                    .items_center()
                                    .gap(t.space.xs)
                                    .h(t.size.control + px(4.))
                                    .px(t.space.md)
                                    .rounded(t.radius.button)
                                    .cursor_pointer()
                                    .text_color(t.colors.text_muted)
                                    .hover(move |st| st.bg(hover))
                                    .child(icon("icons/pin.svg", t.size.icon_sm, if pinned { t.colors.primary } else { t.colors.text_muted }))
                                    .child(if pinned { "Unpin" } else { "Pin" })
                                    .on_click(move |_, _, cx| pin_state.update(cx, |s, cx| s.toggle_project_pin(id, cx))),
                            )
                            .child(
                                theme::ghost_button(t, "Rules and memory")
                                    .id("project-rules")
                                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenRules), cx)),
                            ),
                    )
                    .child(self.composer.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .px(t.space.md)
                                    .pb(t.space.xs)
                                    .text_size(t.text.caption)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t.colors.text_subtle)
                                    .child(format!("Chats in {}", p.name)),
                            )
                            .when(rows.is_empty(), |d| {
                                d.child(div().px(t.space.md).py(t.space.md).text_color(t.colors.text_subtle).child("No chats here yet. Ask something above to start one."))
                            })
                            .children(rows),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ProjectsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        match self.state.read(cx).screen {
            Screen::Project(id) => self.project(id, &t, cx),
            _ => self.list(&t, cx),
        }
    }
}
