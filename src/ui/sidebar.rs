//! The left sidebar, a sheet on the window's cream like the session beside
//! it: the project (opening the switcher), New chat, search, the chats
//! grouped by date (Pinned first), and you with settings at the bottom.
//! Collapses with Ctrl/⌘B.

use gpui::component::input::{Input, InputEvent, InputState};
use std::time::Duration;

use gpui::{
    div, prelude::*, px, Animation, AnimationExt, Context, Entity, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::*;
use crate::state::{AppState, ChatStatus, Screen, TitleMenu};
use crate::theme;
use crate::theme::ActiveTheme;

pub struct Sidebar {
    state: Entity<AppState>,
    search: Entity<InputState>,
    /// The title being edited (F2, or Rename in a chat's menu).
    rename: Entity<InputState>,
    renaming: Option<usize>,
    /// The chat row under the pointer: its wash eases in (`motion::hover_wash`).
    row_hover: Option<usize>,
    /// The filter menu, kept while it leaves.
    filter_presence: super::motion::Presence<()>,
    /// Each chat row's place last time, and how often it moved up: a chat
    /// that rises to the top of its group rises into its new place.
    places: std::collections::HashMap<usize, usize>,
    bumps: std::collections::HashMap<usize, u32>,
    _subs: Vec<Subscription>,
}

/// The list's two groups, as Claude Code's: what you pinned, then the rest
/// by when it last changed.
const GROUPS: [&str; 2] = ["Pinned", "Recents"];

impl Sidebar {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search chats"));
        let rename = cx.new(|cx| InputState::new(window, cx).placeholder("Chat title"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                // Renaming starts: the title goes in the box, and the box takes the keyboard.
                let (now, search) = (state.read(cx).renaming, state.read(cx).search.clone());
                if now != this.renaming {
                    this.renaming = now;
                    if let Some(title) = now
                        .and_then(|id| state.read(cx).chats.iter().find(|c| c.id == id))
                        .map(|c| c.title.to_string())
                    {
                        this.rename.update(cx, |i, cx| {
                            i.set_value(title, window, cx);
                            i.focus(window, cx);
                        });
                    }
                }
                // The search box follows the state (a rename can clear it).
                if this.search.read(cx).value() != search {
                    this.search
                        .update(cx, |i, cx| i.set_value(search.to_string(), window, cx));
                }
                cx.notify();
            }),
            cx.subscribe_in(
                &rename,
                window,
                |this, input, ev: &InputEvent, window, cx| {
                    let Some(id) = this.renaming else { return };
                    match ev {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            let title = input.read(cx).value().to_string();
                            this.state.update(cx, |s, cx| s.rename_chat(id, &title, cx));
                            if matches!(ev, InputEvent::PressEnter { .. }) {
                                window.dispatch_action(Box::new(FocusComposer), cx);
                            }
                        }
                        _ => {}
                    }
                },
            ),
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
        Self {
            state,
            search,
            rename,
            renaming: None,
            row_hover: None,
            filter_presence: Default::default(),
            places: Default::default(),
            bumps: Default::default(),
            _subs: subs,
        }
    }
}

impl Sidebar {
    /// Which chats the list shows: this project's, or every project's.
    fn filter(
        &self,
        t: &crate::theme::Theme,
        // The menu: Some(leaving) while it shows or leaves.
        shown: Option<bool>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let s = self.state.read(cx);
        let (open, all, name, archived) = (
            s.filter_open,
            s.all_chats,
            s.project.clone(),
            s.show_archived,
        );
        let hover = t.colors.hover;
        let option = |id: &'static str,
                      label: SharedString,
                      on: bool,
                      value: bool,
                      state: Entity<AppState>| {
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
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .child(div().w(t.size.icon).when(on, |d| {
                    d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.primary))
                }))
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
            .child(
                div()
                    .px(t.space.md)
                    .pt(t.space.xs)
                    .pb(t.space.xs)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child("Show chats from"),
            )
            .child(option(
                "filter-all",
                "All chats".into(),
                all,
                true,
                self.state.clone(),
            ))
            .child(option(
                "filter-this",
                format!("Only {name}").into(),
                !all,
                false,
                self.state.clone(),
            ))
            .child(theme::rule(t).my(t.space.xs))
            // Archived chats (a chat menu's Archive) wait here.
            .child({
                let st = self.state.clone();
                div()
                    .id("filter-archived")
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text)
                    .hover(move |d| d.bg(hover))
                    .child(div().flex_1().min_w_0().child("Archived chats"))
                    .child(div().w(t.size.icon).when(archived, |d| {
                        d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.primary))
                    }))
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| {
                            s.show_archived = !s.show_archived;
                            s.filter_open = false;
                            cx.notify();
                        })
                    })
            });
        let (toggle, close) = (self.state.clone(), self.state.clone());
        div()
            .relative()
            .when(open, |d| {
                d.on_mouse_down_out(move |_, _, cx| {
                    if close.read(cx).filter_open {
                        close.update(cx, |s, cx| {
                            s.filter_open = false;
                            cx.notify();
                        });
                    }
                })
            })
            .child(
                div()
                    .id("chat-filter")
                    .p(px(3.))
                    .rounded(t.radius.small)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover))
                    .when(open, |d| d.bg(hover))
                    // All projects is the default; only a narrowed list is marked.
                    .child(icon(
                        "icons/sliders-horizontal.svg",
                        t.size.icon_sm,
                        if !all || archived {
                            t.colors.primary
                        } else {
                            t.colors.text_subtle
                        },
                    ))
                    .on_click(move |_, _, cx| {
                        toggle.update(cx, |s, cx| {
                            s.filter_open = !s.filter_open;
                            cx.notify();
                        })
                    }),
            )
            .children(shown.map(|leaving| {
                gpui::deferred(super::motion::unfold(
                    menu,
                    "chat-filter",
                    false,
                    leaving,
                    t,
                ))
                .with_priority(1)
            }))
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row_hover = self.row_hover;
        let t = cx.theme().clone();
        let filter_now = self.state.read(cx).filter_open.then_some(());
        let shown = self
            .filter_presence
            .frame(filter_now, t.size.menu_out_ms, window)
            .map(|f| f.1);
        let mut filter = Some(self.filter(&t, shown, cx).into_any_element());
        let s = self.state.read(cx);
        let q = s.search.to_lowercase();
        let name: SharedString = s.user_name.clone().unwrap_or_else(|| "Welcome".into());
        let avatar = s.avatar.clone();
        let hover = t.colors.hover;
        let rules_open = s.screen == Screen::Rules;
        let projects_open = matches!(s.screen, Screen::Projects | Screen::Project(_));
        let design_open = s.screen == Screen::Design;
        let loading = s.loading;

        let mut list = div()
            .id("chat-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(t.space.sm)
            .pb(t.space.md);
        // Chats of this project (or of every project, by the filter).
        let shown = |c: &&crate::state::Chat| {
            // One list: plain chats and every project's, newest first; the
            // filter can narrow it to the project you're in.
            let kind = s.all_chats || c.project == s.current_project;
            kind && (q.is_empty() || c.title.to_lowercase().contains(&q))
                // Archived chats only when the filter asks for them.
                && c.archived == s.show_archived
        };
        let pinned_projects: Vec<_> = s
            .projects
            .iter()
            .filter(|p| p.pinned && q.is_empty())
            .collect();
        let narrowed = !s.all_chats || s.show_archived;
        let mut any = false;
        let mut place = 0usize;
        for group in GROUPS {
            let rows: Vec<_> = s
                .chats
                .iter()
                .filter(|c| c.pinned == (group == "Pinned"))
                .filter(shown)
                .collect();
            // The chat you last wrote in, or that last answered, first;
            // pinned chats keep the order you gave them.
            let mut rows = rows;
            if group != "Pinned" {
                rows.sort_by(|a, b| b.updated.cmp(&a.updated));
            }
            let projects_here = if group == "Pinned" {
                pinned_projects.as_slice()
            } else {
                &[]
            };
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
                    .text_color(t.colors.text_muted)
                    .group("group-head")
                    // A quiet label, no rule beside it; the filter (shown on
                    // hover, or while it narrows the list), and on Recents a
                    // + for a new chat, as Claude Code's.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(group),
                    )
                    .when(first, |d| {
                        d.children(filter.take().map(|f| {
                            div()
                                .when(!narrowed, |d| {
                                    d.invisible().group_hover("group-head", |st| st.visible())
                                })
                                .child(f)
                        }))
                    })
                    .when(group == "Recents", |d| {
                        d.child(
                            div()
                                .id("recents-new")
                                .ml(px(2.))
                                .p(px(3.))
                                .rounded(t.radius.small)
                                .cursor_pointer()
                                .hover(move |st| st.bg(hover))
                                .tooltip(|w, cx| {
                                    gpui::component::tooltip::Tooltip::new("New chat (Ctrl+N)")
                                        .build(w, cx)
                                })
                                .child(icon("icons/plus.svg", t.size.icon_sm, t.colors.text_muted))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(NewChat), cx)
                                }),
                        )
                    }),
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
                        .text_color(if active {
                            t.colors.text_strong
                        } else {
                            t.colors.text_muted
                        })
                        .when(active, |d| crate::theme::selected(&t, d))
                        .when(!active, |d| d.hover(move |st| st.bg(hover)))
                        .child(
                            div()
                                .w(t.size.icon_sm)
                                .flex_none()
                                .flex()
                                .justify_center()
                                .child(icon(
                                    "icons/folder-open.svg",
                                    t.size.icon_xs,
                                    t.colors.text_muted,
                                )),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(p.name.clone()),
                        )
                        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.show_project(id, cx))),
                );
            }
            let mut age_was = None;
            for (local, c) in rows.into_iter().enumerate() {
                let id = c.id;
                // In Recents a quiet label where the age changes; the
                // newest rows sit under Recents itself.
                if group == "Recents" {
                    let age = age_of(c.updated);
                    if local > 0 && age_was != Some(age) {
                        list = list.child(
                            div()
                                .px(t.space.md)
                                .pt(t.space.sm)
                                .pb(px(2.))
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(age),
                        );
                    }
                    age_was = Some(age);
                }
                // Risen to the top of its group since last time: it rises
                // into place there.
                let was = self.places.insert(id, place);
                place += 1;
                if local == 0 && was.is_some_and(|w| w > place - 1) {
                    *self.bumps.entry(id).or_default() += 1;
                }
                let bump = self.bumps.get(&id).copied().unwrap_or(0);
                let selected = s.screen == Screen::Chat && s.current == Some(id);
                let pinned = c.pinned;
                let state = self.state.clone();
                let state_pin = self.state.clone();
                let status = s.status(c);
                let remote = s.chat_remote(id).map(|r| r.target.icon());
                let unread = status == ChatStatus::Unread;
                let editing = s.renaming == Some(id);
                let menu_open = s.chat_menu == Some(id);
                let folder: Option<SharedString> = s
                    .project(c.project)
                    .filter(|p| !p.scratch)
                    .map(|p| p.name.clone());
                let (state_menu, state_right) = (self.state.clone(), self.state.clone());
                let row = div()
                    .id(("chat", id))
                    .group("row")
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .h(t.size.row)
                    .px(t.space.md)
                    .text_size(t.text.body)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .text_color(if selected {
                        t.colors.text_strong
                    } else {
                        t.colors.text_body
                    })
                    .relative()
                    // The open chat: a soft fill, no coloured edge.
                    .when(selected, |d| crate::theme::selected(&t, d))
                    .when(!selected && row_hover == Some(id), |d| {
                        d.child(super::motion::hover_wash(("chat-wash", id), hover, &t))
                    })
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                        let next = match (*on, this.row_hover) {
                            (true, _) => Some(id),
                            (false, Some(r)) if r == id => None,
                            (false, other) => other,
                        };
                        if next != this.row_hover {
                            this.row_hover = next;
                            cx.notify();
                        }
                    }))
                    .map(|d| {
                        if editing {
                            // The title, editable: Enter or a click away keeps it, Esc puts it back.
                            let cancel = self.state.clone();
                            d.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .capture_key_down(move |ev: &gpui::KeyDownEvent, window, cx| {
                                        if ev.keystroke.key == "escape" {
                                            cx.stop_propagation();
                                            cancel.update(cx, |s, cx| s.cancel_rename(cx));
                                            window.dispatch_action(Box::new(FocusComposer), cx);
                                        }
                                    })
                                    .child(
                                        Input::new(&self.rename)
                                            .appearance(false)
                                            .text_size(t.text.body_sm),
                                    ),
                            )
                        } else {
                            d.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .when(unread, |d| {
                                        d.font_weight(FontWeight::SEMIBOLD)
                                            .text_color(t.colors.text_strong)
                                    })
                                    // Working, as Claude marks it: a band of
                                    // light runs along the title.
                                    .map(|d| {
                                        if status == ChatStatus::Working {
                                            let (title, theme) = (c.title.to_string(), t.clone());
                                            d.with_animation(
                                                ("chat-shine", id),
                                                Animation::new(Duration::from_millis(2200))
                                                    .repeat(),
                                                move |d, k| {
                                                    d.child(super::chat_view::shimmer(
                                                        &title,
                                                        -0.3 + k * 1.6,
                                                        &theme,
                                                    ))
                                                },
                                            )
                                            .into_any_element()
                                        } else {
                                            d.child(c.title.clone()).into_any_element()
                                        }
                                    }),
                            )
                            // At the row's end, only when there is something to
                            // say: working, waiting on you,
                            // unread, or an error. Hover shows the actions there.
                            .when(
                                !matches!(status, ChatStatus::Idle | ChatStatus::Working),
                                |d| {
                                    d.child(
                                        div()
                                            .flex_none()
                                            .group_hover("row", |st| st.invisible())
                                            .when(menu_open, |d| d.invisible())
                                            .child(status_mark(status, id, &t)),
                                    )
                                },
                            )
                            // At rest, a chat that works in a project shows a
                            // quiet folder (its project in the tooltip), so the
                            // one list still tells plain chats from project ones.
                            .when_some(
                                folder.filter(|_| remote.is_none() && status == ChatStatus::Idle),
                                |d, name| {
                                    d.child(
                                        div()
                                            .id(("chat-folder", id))
                                            .flex_none()
                                            .group_hover("row", |st| st.invisible())
                                            .when(menu_open, |d| d.invisible())
                                            .tooltip(move |w, cx| {
                                                gpui::component::tooltip::Tooltip::new(format!(
                                                    "In project {name}"
                                                ))
                                                .build(w, cx)
                                            })
                                            .child(icon(
                                                "icons/folder.svg",
                                                t.size.icon_xs,
                                                t.colors.text_subtle,
                                            )),
                                    )
                                },
                            )
                            // At rest, a chat on another machine shows where,
                            // as Claude Code marks a cloud session.
                            .when_some(remote.filter(|_| status == ChatStatus::Idle), |d, glyph| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .group_hover("row", |st| st.invisible())
                                        .when(menu_open, |d| d.invisible())
                                        .child(icon(glyph, t.size.icon_sm, t.colors.text_subtle)),
                                )
                            })
                            .child(
                                // On hover: pin, and the chat's menu.
                                div()
                                    .flex()
                                    .flex_none()
                                    .gap(px(2.))
                                    .absolute()
                                    .right(t.space.xs)
                                    .invisible()
                                    .group_hover("row", |st| st.visible())
                                    .when(menu_open, |d| d.visible())
                                    .child(
                                        div()
                                            .id(("pin", id))
                                            .p(px(3.))
                                            .rounded(t.radius.small)
                                            .hover(move |st| st.bg(hover))
                                            .child(icon(
                                                "icons/pin.svg",
                                                t.size.icon_sm,
                                                if pinned {
                                                    t.colors.primary
                                                } else {
                                                    t.colors.text_subtle
                                                },
                                            ))
                                            .on_click(move |_, _, cx| {
                                                cx.stop_propagation();
                                                state_pin.update(cx, |s, cx| s.toggle_pin(id, cx));
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id(("chat-more", id))
                                            .p(px(3.))
                                            .rounded(t.radius.small)
                                            .hover(move |st| st.bg(hover))
                                            .when(menu_open, |d| d.bg(hover))
                                            .child(icon(
                                                "icons/ellipsis.svg",
                                                t.size.icon_sm,
                                                t.colors.text_muted,
                                            ))
                                            .on_click(move |ev, _, cx| {
                                                cx.stop_propagation();
                                                let at = ev.position();
                                                state_menu.update(cx, |s, cx| {
                                                    s.chat_menu_from(id, at, cx)
                                                });
                                            }),
                                    ),
                            )
                        }
                    })
                    .relative()
                    .on_mouse_down(gpui::MouseButton::Right, move |ev, _, cx| {
                        cx.stop_propagation();
                        let at = ev.position;
                        state_right.update(cx, |s, cx| {
                            s.chat_menu_at = at;
                            s.open_chat_menu(Some(id), cx)
                        });
                    })
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| s.open_chat(id, cx)));
                list = list.child(if bump > 0 {
                    super::motion::row_in(
                        row,
                        SharedString::from(format!("chat-pop-{id}-{bump}")),
                        0,
                        &t,
                    )
                } else {
                    row.into_any_element()
                });
            }
        }
        if !any {
            // Headed like the groups that replace it, then what to do.
            let put_away = s
                .chats
                .iter()
                .any(|c| c.archived && (s.all_chats || c.project == s.current_project));
            let (title, hint): (&str, SharedString) = if !q.is_empty() {
                ("No chat matches", "Try fewer or other words.".into())
            } else if s.show_archived {
                (
                    "No archived chats",
                    "Archive a chat from its menu, or press A while the menu is open.".into(),
                )
            } else if put_away {
                (
                    "Every chat here is archived",
                    "The filter beside Chats shows them again.".into(),
                )
            } else {
                (
                    "No chats yet",
                    format!(
                        "Ask anything in the box, or press {} for a new chat.",
                        shortcut("N")
                    )
                    .into(),
                )
            };
            list = list
                .child(
                    div()
                        .flex()
                        .items_center()
                        .px(t.space.md)
                        .pt(t.space.lg)
                        .pb(t.space.xs)
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_muted)
                        .child(div().flex_1().min_w_0().child(if s.show_archived {
                            "Archived"
                        } else {
                            "Chats"
                        }))
                        // The filter rides the first header; here, this one.
                        .children(filter.take()),
                )
                .child(
                    div()
                        .mx(t.space.sm)
                        .mt(t.space.sm)
                        .p(t.space.md)
                        .flex()
                        .gap(t.space.sm)
                        .rounded(t.radius.button)
                        .border_1()
                        .border_dashed()
                        .border_color(t.colors.border)
                        .child(icon(
                            "icons/message-square.svg",
                            t.size.icon,
                            t.colors.text_muted,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(t.space.xs)
                                .child(
                                    div()
                                        .text_size(t.text.body_sm)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t.colors.text)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .child(hint),
                                ),
                        ),
                );
        }

        // A row of the nav: its icon in a fixed slot, its name, its key on hover.
        let nav = |key: &'static str,
                   path: &'static str,
                   label: &'static str,
                   on: bool,
                   keys: Option<SharedString>| {
            div()
                .id(SharedString::from(format!("nav-{key}")))
                .group("nav")
                .flex()
                .items_center()
                .gap(t.space.sm)
                .h(t.size.row)
                .px(t.space.sm)
                .rounded(t.radius.button)
                .cursor_pointer()
                .text_size(t.text.body)
                .text_color(if on {
                    t.colors.text_strong
                } else {
                    t.colors.text_body
                })
                .when(on, |d| crate::theme::selected(&t, d))
                .when(!on, |d| d.hover(move |st| st.bg(hover)))
                .child(
                    div()
                        .w(t.size.icon_lg)
                        .flex_none()
                        .flex()
                        .justify_center()
                        .child(icon(
                            path,
                            t.size.icon,
                            if on {
                                t.colors.text_strong
                            } else {
                                t.colors.text_muted
                            },
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .children(keys.map(|k| {
                    div()
                        .invisible()
                        .group_hover("nav", |st| st.visible())
                        .child(kbd(&t, k))
                }))
        };
        let documents_open = s.screen == Screen::Documents;
        let help_open = s.title_menu.is_some_and(|(m, _)| m == TitleMenu::Help);
        let account_open = s.title_menu.is_some_and(|(m, _)| m == TitleMenu::Account);
        let (help, account) = (self.state.clone(), self.state.clone());

        let sheet = div()
            .size_full()
            .flex()
            .flex_col()
            // Part of the window, not a card on it: the window's own
            // colour, flush to its edges; the chat is the raised sheet.
            .overflow_hidden()
            // KodAct's name is in the window's top row; the sidebar opens
            // with Chat | Code, across its width, then search.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px(t.space.sm)
                    .pt(t.space.xs)
                    .pb(t.space.sm)
                    // Search, as Claude Code's sidebar opens with it: a
                    // field that opens the palette over the window.
                    .child(
                        div()
                            .id("sidebar-search")
                            .role(gpui::accesskit::Role::Button)
                            .aria_label("Search files, chats and commands")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row + px(2.))
                            .px(t.space.sm)
                            .mb(t.space.sm)
                            .rounded(t.radius.field)
                            .border_1()
                            .border_color(t.colors.border)
                            .bg(t.extra.sunk)
                            .cursor_pointer()
                            .text_size(t.text.body)
                            .text_color(t.colors.text_muted)
                            .hover(move |d| d.border_color(t.extra.border_strong))
                            .child(
                                div()
                                    .w(t.size.icon_lg)
                                    .flex_none()
                                    .flex()
                                    .justify_center()
                                    .child(icon(
                                        "icons/search.svg",
                                        t.size.icon,
                                        t.colors.text_muted,
                                    )),
                            )
                            .child(div().flex_1().min_w_0().child("Search"))
                            .child(super::kbd(&t, super::shortcut("K")))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleCommandPalette), cx)
                            }),
                    )
                    // New: a plus in a ring, as Claude Code's, never a fill.
                    .child(
                        div()
                            .id("new-chat")
                            .group("nav")
                            .role(gpui::accesskit::Role::Button)
                            .aria_label("New chat")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row)
                            .px(t.space.sm)
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            .text_size(t.text.body)
                            .text_color(t.colors.text_body)
                            .hover(move |st| st.bg(hover))
                            .child(
                                div()
                                    .w(t.size.icon_lg)
                                    .flex_none()
                                    .flex()
                                    .justify_center()
                                    // A plain line icon, as every row here.
                                    .child(icon(
                                        "icons/plus.svg",
                                        t.size.icon_sm,
                                        t.colors.text_body,
                                    )),
                            )
                            .child(div().flex_1().min_w_0().child("New"))
                            .child(
                                div()
                                    .invisible()
                                    .group_hover("nav", |st| st.visible())
                                    .child(kbd(&t, shortcut("N"))),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(NewChat), cx)
                            }),
                    )
                    .child(
                        nav(
                            "projects",
                            "icons/folder-open.svg",
                            "Projects",
                            projects_open,
                            Some(shortcut("Shift P")),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(OpenProjects), cx)
                        }),
                    )
                    .child(
                        nav(
                            "documents",
                            "icons/file-text.svg",
                            "Documents",
                            documents_open,
                            None,
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(OpenDocuments), cx)
                        }),
                    )
                    .child(
                        nav(
                            "design",
                            "icons/pencil-ruler.svg",
                            "Design",
                            design_open,
                            Some(shortcut("Shift D")),
                        )
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenDesign), cx)),
                    )
                    .child(
                        nav(
                            "customize",
                            "icons/sliders-horizontal.svg",
                            "Rules and memory",
                            rules_open,
                            Some(shortcut("Shift R")),
                        )
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenRules), cx)),
                    )
                    .child(
                        nav("more", "icons/ellipsis.svg", "More", help_open, None).on_click(
                            move |ev, _, cx| {
                                let at = super::click_down(ev);
                                help.update(cx, |s, cx| {
                                    s.toggle_title_menu(TitleMenu::Help, at, cx)
                                })
                            },
                        ),
                    ),
            )
            .map(|d| {
                if loading {
                    d.child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .child(super::skeleton::chat_list(&t)),
                    )
                } else {
                    d.child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .child(list)
                            .with_animation(
                                "chats-in",
                                Animation::new(Duration::from_millis(t.motion_ms)),
                                |d, delta| d.opacity(delta),
                            ),
                    )
                }
            })
            // You, on one line at the foot: your picture and your name
            // (the model shows under the composer, not here); it opens
            // your menu. Settings on the right.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .p(t.space.sm)
                    .border_t_1()
                    .border_color(t.colors.border)
                    .child(
                        div()
                            .id("sidebar-account")
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row + px(2.))
                            .px(t.space.sm)
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            .hover(move |st| st.bg(hover))
                            .when(account_open, |d| d.bg(hover))
                            .child(super::avatar_image(&avatar, 24., window))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(t.text.body)
                                    .child(
                                        div().flex().gap(t.space.xs).child(
                                            div()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(t.colors.text_strong)
                                                .child(name),
                                        ),
                                    ),
                            )
                            .child(super::button::chevron(&t, account_open))
                            .child(div().flex_1())
                            .on_click(move |ev, _, cx| {
                                let at = super::click_down(ev);
                                account.update(cx, |s, cx| {
                                    s.toggle_title_menu(TitleMenu::Account, at, cx)
                                })
                            }),
                    )
                    .child(
                        div()
                            .id("sidebar-settings")
                            .size(t.size.control)
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            .hover(move |st| st.bg(hover))
                            .tooltip(|w, cx| {
                                gpui::component::tooltip::Tooltip::new("Settings (Ctrl+,)")
                                    .build(w, cx)
                            })
                            .child(icon("icons/settings.svg", t.size.icon, t.colors.text_muted))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenSettings), cx)
                            }),
                    ),
            );
        div().w(t.size.sidebar).flex_none().h_full().child(sheet)
    }
}

/// The sidebar collapsed (Ctrl+B): a narrow rail of the same actions as
/// icon buttons, each named in its tooltip, so nothing goes out of reach.
/// A click anywhere on it opens the sidebar again: on its bare strip that
/// alone, on a button along with what the button does.
pub fn rail(t: &crate::theme::Theme, screen: Screen, state: &Entity<AppState>) -> impl IntoElement {
    let open = state.clone();
    use super::button::Button;
    let nav = |id: &'static str, path: &'static str, tip: String, on: bool| {
        Button::icon(id, path, tip).selected(on)
    };
    let dispatch = |a: Box<dyn gpui::Action>| {
        move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut gpui::App| {
            window.dispatch_action(a.boxed_clone(), cx)
        }
    };
    div()
        .id("sidebar-rail")
        .on_click(move |_, _, cx| {
            open.update(cx, |s, cx| {
                s.sidebar_open = true;
                cx.notify();
            })
        })
        .w(t.size.rail)
        .flex_none()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(t.space.xs)
        .py(t.space.sm)
        .bg(t.extra.pane)
        .child(
            div()
                .id("rail-new-chat")
                .role(gpui::accesskit::Role::Button)
                .aria_label("New chat")
                .size(t.size.hit)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover({
                    let h = t.colors.hover;
                    move |st| st.bg(h)
                })
                .child(
                    div()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(t.extra.accent_fill)
                        .child(icon("icons/plus.svg", t.size.icon_sm, t.extra.accent_fg)),
                )
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new(format!("New chat ({})", shortcut("N")))
                        .build(window, cx)
                })
                .on_click(dispatch(Box::new(NewChat))),
        )
        .child(
            nav(
                "rail-search",
                "icons/search.svg",
                format!("Search files, chats and commands ({})", shortcut("K")),
                false,
            )
            .on_click(dispatch(Box::new(ToggleCommandPalette))),
        )
        .child(
            nav(
                "rail-projects",
                "icons/folder-open.svg",
                format!("Projects ({})", shortcut("Shift P")),
                matches!(screen, Screen::Projects | Screen::Project(_)),
            )
            .on_click(dispatch(Box::new(OpenProjects))),
        )
        .child(
            nav(
                "rail-design",
                "icons/pencil-ruler.svg",
                format!("Design ({})", shortcut("Shift D")),
                screen == Screen::Design,
            )
            .on_click(dispatch(Box::new(OpenDesign))),
        )
        .child(
            nav(
                "rail-rules",
                "icons/list-todo.svg",
                format!("Rules and memory ({})", shortcut("Shift R")),
                screen == Screen::Rules,
            )
            .on_click(dispatch(Box::new(OpenRules))),
        )
        .child(div().flex_1())
        .child(
            nav(
                "rail-settings",
                "icons/settings.svg",
                format!("Settings ({})", shortcut(",")),
                false,
            )
            .on_click(dispatch(Box::new(OpenSettings))),
        )
}

/// A chat's status, in a fixed slot at its row's end: the strike mark
/// turning while it works, a breathing accent dot when it needs you, a
/// warning when it ended on an error, the link colour's dot when a reply is
/// unread, a faint ring at rest.
pub fn status_mark(status: ChatStatus, id: usize, t: &crate::theme::Theme) -> gpui::AnyElement {
    let dot = || div().size(px(7.)).rounded(t.radius.pill);
    let inner: gpui::AnyElement = match status {
        ChatStatus::Working => super::turning_logo(14., ("chat-working", id)),
        ChatStatus::NeedsYou => dot()
            .bg(t.extra.accent)
            .with_animation(
                ("chat-needs", id),
                Animation::new(Duration::from_millis(1600))
                    .repeat()
                    .with_easing(gpui::ease_in_out),
                |d, delta| d.opacity(0.45 + 0.55 * (1. - (delta * 2. - 1.).abs())),
            )
            .into_any_element(),
        ChatStatus::Error => {
            icon("icons/triangle-alert.svg", t.size.icon_xs, t.extra.danger).into_any_element()
        }
        ChatStatus::Unread => dot().bg(t.colors.primary).into_any_element(),
        ChatStatus::Idle => dot()
            .border_1()
            .border_color(t.colors.text_subtle.opacity(0.45))
            .into_any_element(),
    };
    div()
        .w(t.size.icon_sm)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(inner)
        .into_any_element()
}

pub fn ago(minutes: u32) -> String {
    match minutes {
        0 => "now".into(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// How long ago a chat last moved, as Recents groups it.
fn age_of(updated: std::time::SystemTime) -> &'static str {
    let secs = std::time::SystemTime::now()
        .duration_since(updated)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    age_label(secs, seconds_into_today())
}

/// Seconds since local midnight.
fn seconds_into_today() -> u64 {
    use chrono::Timelike;
    chrono::Local::now().num_seconds_from_midnight() as u64
}

fn age_label(ago: u64, into_today: u64) -> &'static str {
    const DAY: u64 = 86_400;
    if ago <= into_today {
        "Today"
    } else if ago <= into_today + DAY {
        "Yesterday"
    } else if ago <= into_today + 7 * DAY {
        "Previous 7 days"
    } else {
        "Earlier"
    }
}

#[cfg(test)]
mod age_tests {
    use super::age_label;

    #[test]
    fn recents_group_by_calendar_day() {
        let noon = 12 * 3600;
        assert_eq!(age_label(3600, noon), "Today");
        assert_eq!(age_label(noon + 3600, noon), "Yesterday");
        assert_eq!(age_label(noon + 3 * 86_400, noon), "Previous 7 days");
        assert_eq!(age_label(noon + 30 * 86_400, noon), "Earlier");
    }
}
