//! The left sidebar: New chat, search, the chats grouped by date (Pinned
//! first), and settings at the bottom. Collapses with Ctrl/⌘B.

use gpui::component::input::{Input, InputEvent, InputState};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, Animation, AnimationExt, Context, Entity, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window,
};

use super::*;
use crate::state::{AppState, ChatStatus, Screen};
use crate::theme;
use crate::theme::ActiveTheme;

pub struct Sidebar {
    state: Entity<AppState>,
    search: Entity<InputState>,
    _subs: Vec<Subscription>,
}

const GROUPS: [&str; 5] = ["Pinned", "Today", "Yesterday", "Previous 7 days", "Older"];

impl Sidebar {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search chats"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |this, input, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    let q = input.read(cx).value();
                    this.state.update(cx, |s, cx| {
                        s.search = q;
                        cx.notify();
                    });
                }
            }),
        ];
        Self { state, search, _subs: subs }
    }
}

impl Sidebar {
    /// Which chats the list shows: this project's, or every project's.
    fn filter(&self, t: &crate::theme::Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let s = self.state.read(cx);
        let (open, all, name) = (s.filter_open, s.all_chats, s.project.clone());
        let hover = t.colors.hover;
        let option = |id: &'static str, label: SharedString, on: bool, value: bool, state: Entity<AppState>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(t.space.sm)
                .px(t.space.md)
                .py(t.space.sm)
                .rounded(t.radius.button)
                .cursor_pointer()
                .font_weight(FontWeight::NORMAL)
                .text_size(t.text.body_sm)
                .text_color(t.colors.text)
                .hover(move |st| st.bg(hover))
                .child(div().flex_1().child(label))
                .child(div().w(t.size.icon).when(on, |d| d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.primary))))
                .on_click(move |_, _, cx| {
                    state.update(cx, |s, cx| {
                        s.all_chats = value;
                        s.filter_open = false;
                        cx.notify();
                    })
                })
        };
        let menu = theme::popover(t)
            .absolute()
            .top(t.size.icon + t.space.xs)
            .right_0()
            .w(px(220.))
            .child(div().px(t.space.md).pt(t.space.xs).pb(t.space.xs).text_size(t.text.caption).text_color(t.colors.text_subtle).child("Show chats from"))
            .child(option("filter-this", format!("This project ({name})").into(), !all, false, self.state.clone()))
            .child(option("filter-all", "All projects".into(), all, true, self.state.clone()));
        let (toggle, close) = (self.state.clone(), self.state.clone());
        div()
            .relative()
            .on_mouse_down_out(move |_, _, cx| {
                if close.read(cx).filter_open {
                    close.update(cx, |s, cx| {
                        s.filter_open = false;
                        cx.notify();
                    });
                }
            })
            .child(
                div()
                    .id("chat-filter")
                    .p(px(3.))
                    .rounded(t.radius.small)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover))
                    .when(open || all, |d| d.bg(hover))
                    .child(icon("icons/sliders-horizontal.svg", t.size.icon_sm, if all { t.colors.primary } else { t.colors.text_subtle }))
                    .on_click(move |_, _, cx| {
                        toggle.update(cx, |s, cx| {
                            s.filter_open = !s.filter_open;
                            cx.notify();
                        })
                    }),
            )
            .when(open, |d| d.child(gpui::deferred(menu).with_priority(1)))
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let mut filter = Some(self.filter(&t, cx).into_any_element());
        let s = self.state.read(cx);
        let q = s.search.to_lowercase();
        let name: SharedString = s.user_name.clone().unwrap_or_else(|| "Welcome".into());
        let hover = t.colors.hover;
        let rules_open = s.screen == Screen::Rules;
        let projects_open = matches!(s.screen, Screen::Projects | Screen::Project(_));
        let loading = s.loading;

        let mut list = div().id("chat-list").flex_1().min_h_0().overflow_y_scroll().px(t.space.sm).pb(t.space.md);
        // Chats of this project (or of every project, by the filter).
        let shown = |c: &&crate::state::Chat| (s.all_chats || c.project == s.current_project) && (q.is_empty() || c.title.to_lowercase().contains(&q));
        let pinned_projects: Vec<_> = s.projects.iter().filter(|p| p.pinned && q.is_empty()).collect();
        let mut any = false;
        for group in GROUPS {
            let rows: Vec<_> = s.chats.iter().filter(|c| c.group() == group).filter(shown).collect();
            let projects_here = if group == "Pinned" { pinned_projects.as_slice() } else { &[] };
            if rows.is_empty() && projects_here.is_empty() {
                continue;
            }
            let first = !any;
            any = true;
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .px(t.space.md)
                    .pt(t.space.lg)
                    .pb(t.space.xs)
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_subtle)
                    .child(div().flex_1().child(group))
                    .when(first, |d| d.children(filter.take())),
            );
            for p in projects_here {
                let (state, id) = (self.state.clone(), p.id);
                let active = s.screen == Screen::Project(p.id);
                list = list.child(
                    div()
                        .id(("pinned-project", p.id))
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .h(t.size.row)
                        .px(t.space.md)
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .text_color(if active { t.colors.text_strong } else { t.colors.text_muted })
                        .when(active, |d| d.bg(t.extra.selected).border_1().border_color(t.colors.border).font_weight(FontWeight::MEDIUM))
                        .when(!active, |d| d.hover(move |st| st.bg(hover)))
                        .child(div().w(t.size.icon_sm).flex_none().flex().justify_center().child(icon("icons/folder-open.svg", px(12.), t.colors.primary)))
                        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(p.name.clone()))
                        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.show_project(id, cx))),
                );
            }
            for c in rows {
                let id = c.id;
                let selected = s.screen == Screen::Chat && s.current == Some(id);
                let pinned = c.pinned;
                let state = self.state.clone();
                let state_pin = self.state.clone();
                let ago: SharedString = match (s.all_chats && c.project != s.current_project).then(|| s.project(c.project)).flatten() {
                    Some(p) => format!("{} · {}", p.name, ago(c.minutes_ago)).into(),
                    None => ago(c.minutes_ago).into(),
                };
                let status = s.status(c);
                let unread = status == ChatStatus::Unread;
                list = list.child(
                    div()
                        .id(("chat", id))
                        .group("row")
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .h(t.size.row)
                        .px(t.space.md)
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .text_color(if selected { t.colors.text_strong } else { t.colors.text_muted })
                        .when(selected, |d| d.bg(t.extra.selected).border_1().border_color(t.colors.border).font_weight(FontWeight::MEDIUM))
                        .when(!selected, |d| d.hover(move |st| st.bg(hover)))
                        .child(status_mark(status, id, &t))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .when(unread, |d| d.font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong))
                                .child(c.title.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .group_hover("row", |st| st.invisible())
                                .child(ago),
                        )
                        .child(
                            div()
                                .id(("pin", id))
                                .flex_none()
                                .absolute()
                                .right(t.space.sm)
                                .invisible()
                                .group_hover("row", |st| st.visible())
                                .p(px(3.))
                                .rounded(t.radius.small)
                                .hover(move |st| st.bg(hover))
                                .child(icon("icons/pin.svg", t.size.icon_sm, if pinned { t.colors.primary } else { t.colors.text_subtle }))
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    state_pin.update(cx, |s, cx| s.toggle_pin(id, cx));
                                }),
                        )
                        .relative()
                        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.open_chat(id, cx))),
                );
            }
        }
        if !any {
            list = list.child(div().px(t.space.md).py(t.space.lg).text_color(t.colors.text_subtle).child(if q.is_empty() { "Chats you start appear here." } else { "No chat matches." }));
        }

        div()
            .w(t.size.sidebar)
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.colors.sidebar)
            .border_r_1()
            .border_color(t.colors.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.xs)
                    .p(t.space.sm)
                    .child(
                        div()
                            .id("new-chat")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row + px(4.))
                            .px(t.space.md)
                            .rounded(t.radius.button)
                            .bg(t.colors.surface)
                            .border_1()
                            .border_color(t.colors.border)
                            .cursor_pointer()
                            .font_weight(FontWeight::MEDIUM)
                            .hover(move |st| st.bg(hover))
                            .child(icon("icons/plus.svg", t.size.icon, t.colors.text))
                            .child(div().flex_1().child("New chat"))
                            .child(kbd(&t, shortcut("N")))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(NewChat), cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .h(t.size.row)
                            .pl(t.space.md)
                            .rounded(t.radius.button)
                            .child(icon("icons/search.svg", t.size.icon_sm, t.colors.text_subtle))
                            .child(div().flex_1().child(Input::new(&self.search).appearance(false).cleanable(true))),
                    )
                    .child(
                        div()
                            .id("nav-projects")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row)
                            .px(t.space.md)
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            .text_color(if projects_open { t.colors.text_strong } else { t.colors.text_muted })
                            .when(projects_open, |d| d.bg(t.extra.selected).border_1().border_color(t.colors.border).font_weight(FontWeight::MEDIUM))
                            .when(!projects_open, |d| d.hover(move |st| st.bg(hover)))
                            .child(icon("icons/folder-open.svg", t.size.icon_sm, if projects_open { t.colors.text_strong } else { t.colors.text_subtle }))
                            .child(div().flex_1().child("Projects"))
                            .child(kbd(&t, shortcut("Shift P")))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenProjects), cx)),
                    )
                    .child(
                        div()
                            .id("nav-rules")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row)
                            .px(t.space.md)
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            .text_color(if rules_open { t.colors.text_strong } else { t.colors.text_muted })
                            .when(rules_open, |d| d.bg(t.extra.selected).border_1().border_color(t.colors.border).font_weight(FontWeight::MEDIUM))
                            .when(!rules_open, |d| d.hover(move |st| st.bg(hover)))
                            .child(icon("icons/list-todo.svg", t.size.icon_sm, if rules_open { t.colors.text_strong } else { t.colors.text_subtle }))
                            .child(div().flex_1().child("Rules and memory"))
                            .child(kbd(&t, shortcut("Shift R")))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenRules), cx)),
                    ),
            )
            .map(|d| {
                if loading {
                    d.child(div().flex_1().min_h_0().child(super::skeleton::chat_list(&t)))
                } else {
                    d.child(div().flex_1().min_h_0().flex().flex_col().child(list).with_animation(
                        "chats-in",
                        Animation::new(Duration::from_millis(t.motion_ms)),
                        |d, delta| d.opacity(delta),
                    ))
                }
            })
            .child(
                div()
                    .border_t_1()
                    .border_color(t.colors.border)
                    .p(t.space.sm)
                    .child(
                        div()
                            .id("sidebar-settings")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row + px(8.))
                            .px(t.space.md)
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            .hover(move |st| st.bg(hover))
                            .child(super::cube_image(32., window))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).overflow_hidden().whitespace_nowrap().text_ellipsis().child(name))
                                    .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child("Settings and providers")),
                            )
                            .child(icon("icons/settings.svg", t.size.icon, t.colors.text_subtle))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
                    ),
            )
    }
}

/// A chat's status, in a fixed slot before its title: a voxel stepping
/// round while it works, a breathing amber dot when it needs you, a warning when it
/// ended on an error, a blue dot when a reply is unread, a faint ring at rest.
pub fn status_mark(status: ChatStatus, id: usize, t: &crate::theme::Theme) -> gpui::AnyElement {
    let dot = || div().size(px(7.)).rounded(t.radius.pill);
    let inner: gpui::AnyElement = match status {
        ChatStatus::Working => super::voxels::running(t.colors.sky, ("chat-working", id)),
        ChatStatus::NeedsYou => dot()
            .bg(t.colors.marigold)
            .with_animation(("chat-needs", id), Animation::new(Duration::from_millis(1600)).repeat().with_easing(gpui::ease_in_out), |d, delta| {
                d.opacity(0.45 + 0.55 * (1. - (delta * 2. - 1.).abs()))
            })
            .into_any_element(),
        ChatStatus::Error => icon("icons/triangle-alert.svg", px(12.), t.extra.danger).into_any_element(),
        ChatStatus::Unread => dot().bg(t.colors.primary).into_any_element(),
        ChatStatus::Idle => dot().border_1().border_color(t.colors.text_subtle.opacity(0.45)).into_any_element(),
    };
    div().w(t.size.icon_sm).flex_none().flex().items_center().justify_center().child(inner).into_any_element()
}

pub fn ago(minutes: u32) -> String {
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}
