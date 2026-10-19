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
use crate::state::{AppState, ChatStatus, Screen};
use crate::theme;
use crate::theme::ActiveTheme;

pub struct Sidebar {
    state: Entity<AppState>,
    search: Entity<InputState>,
    /// The title being edited (F2, or Rename in a chat's menu).
    rename: Entity<InputState>,
    renaming: Option<usize>,
    /// Where the open chat menu sits: under the ⋯ button, or at the click.
    menu_at: gpui::Point<gpui::Pixels>,
    _subs: Vec<Subscription>,
}

const GROUPS: [&str; 5] = ["Pinned", "Today", "Yesterday", "Previous 7 days", "Older"];

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
            menu_at: Default::default(),
            _subs: subs,
        }
    }
}

impl Sidebar {
    /// Which chats the list shows: this project's, or every project's.
    fn filter(&self, t: &crate::theme::Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let s = self.state.read(cx);
        let (open, all, name) = (s.filter_open, s.all_chats, s.project.clone());
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
                .child(div().flex_1().child(label))
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
                "filter-this",
                format!("This project ({name})").into(),
                !all,
                false,
                self.state.clone(),
            ))
            .child(option(
                "filter-all",
                "All projects".into(),
                all,
                true,
                self.state.clone(),
            ));
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
                    .child(icon(
                        "icons/sliders-horizontal.svg",
                        t.size.icon_sm,
                        if all {
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
        let avatar = s.avatar.clone();
        let hover = t.colors.hover;
        let rules_open = s.screen == Screen::Rules;
        let projects_open = matches!(s.screen, Screen::Projects | Screen::Project(_));
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
            (s.all_chats || c.project == s.current_project)
                && (q.is_empty() || c.title.to_lowercase().contains(&q))
        };
        let pinned_projects: Vec<_> = s
            .projects
            .iter()
            .filter(|p| p.pinned && q.is_empty())
            .collect();
        let mut any = false;
        for group in GROUPS {
            let rows: Vec<_> = s
                .chats
                .iter()
                .filter(|c| c.group() == group)
                .filter(shown)
                .collect();
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
                    .child(div().flex_none().child(group))
                    .child(div().flex_1().h(px(1.)).mx(t.space.sm).bg(t.colors.border))
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
                        .text_color(if active {
                            t.colors.text_strong
                        } else {
                            t.colors.text_muted
                        })
                        .when(active, |d| {
                            d.bg(t.extra.accent_soft.opacity(0.6))
                                .font_weight(FontWeight::MEDIUM)
                        })
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
                                    t.colors.primary,
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
            for c in rows {
                let id = c.id;
                let selected = s.screen == Screen::Chat && s.current == Some(id);
                let pinned = c.pinned;
                let state = self.state.clone();
                let state_pin = self.state.clone();
                let ago: SharedString = match (s.all_chats && c.project != s.current_project)
                    .then(|| s.project(c.project))
                    .flatten()
                {
                    Some(p) => format!("{} · {}", p.name, ago(c.minutes_ago)).into(),
                    None => ago(c.minutes_ago).into(),
                };
                let status = s.status(c);
                let unread = status == ChatStatus::Unread;
                let editing = s.renaming == Some(id);
                let menu_open = s.chat_menu == Some(id);
                let (state_menu, state_right) = (self.state.clone(), self.state.clone());
                let me = cx.entity().downgrade();
                let me_right = me.clone();
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
                        .text_color(if selected {
                            t.colors.text_strong
                        } else {
                            t.colors.text_muted
                        })
                        .when(selected, |d| {
                            d.bg(t.extra.accent_soft.opacity(0.6))
                                .font_weight(FontWeight::MEDIUM)
                        })
                        .when(!selected, |d| d.hover(move |st| st.bg(hover)))
                        .child(status_mark(status, id, &t))
                        .map(|d| {
                            if editing {
                                // The title, editable: Enter or a click away keeps it, Esc puts it back.
                                let cancel = self.state.clone();
                                d.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .capture_key_down(
                                            move |ev: &gpui::KeyDownEvent, window, cx| {
                                                if ev.keystroke.key == "escape" {
                                                    cx.stop_propagation();
                                                    cancel.update(cx, |s, cx| s.cancel_rename(cx));
                                                    window.dispatch_action(
                                                        Box::new(FocusComposer),
                                                        cx,
                                                    );
                                                }
                                            },
                                        )
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
                                        .child(c.title.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .group_hover("row", |st| st.invisible())
                                        .when(menu_open, |d| d.invisible())
                                        .child(ago),
                                )
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
                                                    state_pin
                                                        .update(cx, |s, cx| s.toggle_pin(id, cx));
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
                                                    let _ =
                                                        me.update(cx, |this, _| this.menu_at = at);
                                                    state_menu.update(cx, |s, cx| {
                                                        s.open_chat_menu(
                                                            if s.chat_menu == Some(id) {
                                                                None
                                                            } else {
                                                                Some(id)
                                                            },
                                                            cx,
                                                        )
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
                            let _ = me_right.update(cx, |this, _| this.menu_at = at);
                            state_right.update(cx, |s, cx| s.open_chat_menu(Some(id), cx));
                        })
                        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.open_chat(id, cx))),
                );
            }
        }
        if !any {
            list = list.child(
                div()
                    .px(t.space.md)
                    .py(t.space.lg)
                    .text_color(t.colors.text_muted)
                    .child(if q.is_empty() {
                        "Chats you start appear here."
                    } else {
                        "No chat matches."
                    }),
            );
        }

        // The open chat menu, anchored where it was asked for, over everything.
        let menu = s
            .chat_menu
            .and_then(|id| s.chats.iter().find(|c| c.id == id))
            .map(|c| {
                let close = self.state.clone();
                gpui::deferred(
                    gpui::anchored()
                        .position(self.menu_at)
                        .snap_to_window_with_margin(t.space.sm)
                        .child(
                            div()
                                .on_mouse_down_out(move |_, _, cx| {
                                    close.update(cx, |s, cx| s.open_chat_menu(None, cx))
                                })
                                .child(super::chat_menu::render(
                                    &t,
                                    &self.state,
                                    c.id,
                                    c.pinned,
                                    window,
                                )),
                        ),
                )
                .with_priority(2)
            });

        // The project, at the top of the sheet: its tile, its name and where
        // it stands (the branch, or where a remote one lives); a click opens
        // the switcher under it.
        let switcher_open = s.switcher_open;
        let switcher = switcher_open.then(|| {
            gpui::deferred(super::workspace::project_switcher(
                &self.state,
                window,
                cx,
                px(34.) + t.space.md * 2. + t.space.xs,
            ))
            .with_priority(1)
        });
        let place = s
            .current_remote()
            .map(|r| (r.target.icon(), SharedString::from(r.target.label()), false))
            .or_else(|| {
                (!s.branch.is_empty()).then(|| ("icons/git-branch.svg", s.branch.clone(), true))
            });
        let (closer, opener) = (self.state.clone(), self.state.clone());
        let header = div()
            .relative()
            .flex_none()
            .on_mouse_down_out(move |_, _, cx| {
                if closer.read(cx).switcher_open {
                    closer.update(cx, |s, cx| {
                        s.switcher_open = false;
                        cx.notify();
                    })
                }
            })
            .child(
                div()
                    .id("sidebar-project")
                    .flex()
                    .items_center()
                    .gap(t.space.md)
                    .p(t.space.md)
                    .border_b_1()
                    .border_color(t.colors.border)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover))
                    .when(switcher_open, |d| d.bg(hover))
                    .child(super::projects::project_tile(
                        &t,
                        &s.project,
                        &s.project_dir,
                        px(34.),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.colors.text_strong)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(s.project.clone()),
                            )
                            .children(place.map(|(glyph, label, mono)| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(t.space.xs)
                                    .min_w_0()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .when(mono, |d| d.font_family(t.font_mono.clone()))
                                    .child(icon(glyph, t.size.icon_xs, t.colors.text_muted))
                                    .child(
                                        div()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(label),
                                    )
                            })),
                    )
                    .child(icon(
                        "icons/chevrons-up-down.svg",
                        t.size.icon_sm,
                        t.colors.text_subtle,
                    ))
                    .on_click(move |_, _, cx| {
                        opener.update(cx, |s, cx| {
                            s.switcher_open = !s.switcher_open;
                            cx.notify();
                        })
                    }),
            )
            .children(switcher);

        let sheet =
            div()
                .size_full()
                .flex()
                .flex_col()
                .rounded(t.radius.card)
                .shadow(t.elevation_grain())
                .overflow_hidden()
                .bg(t.extra.stage)
                .child(header)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(t.space.xs)
                        .p(t.space.sm)
                        .child(
                            div()
                                .id("new-chat")
                                .role(gpui::accesskit::Role::Button)
                                .aria_label("New chat")
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .h(t.size.row)
                                .px(t.space.sm)
                                .rounded(t.radius.button)
                                .cursor_pointer()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                                .hover(move |st| st.bg(hover))
                                // The sidebar's one filled mark: the accent disc.
                                .child(
                                    div()
                                        .size(t.size.icon_lg)
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(t.radius.pill)
                                        .bg(t.extra.accent)
                                        .child(icon(
                                            "icons/plus.svg",
                                            t.size.icon_xs,
                                            t.extra.accent_fg,
                                        )),
                                )
                                .child(nav_label("New chat"))
                                .child(kbd(&t, shortcut("N")))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(NewChat), cx)
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.space.xs)
                                .h(t.size.row)
                                .pl(t.space.md)
                                .rounded(t.radius.button)
                                .child(icon(
                                    "icons/search.svg",
                                    t.size.icon_sm,
                                    t.colors.text_muted,
                                ))
                                .child(div().flex_1().child(
                                    Input::new(&self.search).appearance(false).cleanable(true),
                                )),
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
                                .text_color(if projects_open {
                                    t.colors.text_strong
                                } else {
                                    t.colors.text_muted
                                })
                                .when(projects_open, |d| {
                                    d.bg(t.extra.accent_soft.opacity(0.6))
                                        .font_weight(FontWeight::MEDIUM)
                                })
                                .when(!projects_open, |d| d.hover(move |st| st.bg(hover)))
                                .child(icon(
                                    "icons/folder-open.svg",
                                    t.size.icon_sm,
                                    if projects_open {
                                        t.colors.text_strong
                                    } else {
                                        t.colors.text_muted
                                    },
                                ))
                                .child(nav_label("Projects"))
                                .child(kbd(&t, shortcut("Shift P")))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(OpenProjects), cx)
                                }),
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
                                .text_color(if rules_open {
                                    t.colors.text_strong
                                } else {
                                    t.colors.text_muted
                                })
                                .when(rules_open, |d| {
                                    d.bg(t.extra.accent_soft.opacity(0.6))
                                        .font_weight(FontWeight::MEDIUM)
                                })
                                .when(!rules_open, |d| d.hover(move |st| st.bg(hover)))
                                .child(icon(
                                    "icons/list-todo.svg",
                                    t.size.icon_sm,
                                    if rules_open {
                                        t.colors.text_strong
                                    } else {
                                        t.colors.text_muted
                                    },
                                ))
                                .child(nav_label("Rules and memory"))
                                .child(kbd(&t, shortcut("Shift R")))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(OpenRules), cx)
                                }),
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
                .child(
                    div().relative().p(t.space.sm).child(
                        div()
                            .id("sidebar-settings")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .h(t.size.row + px(14.))
                            .px(t.space.sm)
                            .rounded(t.radius.button)
                            .cursor_pointer()
                            // You, in a recessed well at the foot of the sheet.
                            .bg(t.extra.sunk)
                            .hover(move |st| st.bg(hover))
                            .child(super::avatar_image(&avatar, 32., window))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(t.colors.text_strong)
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(name),
                                    )
                                    .child(
                                        div()
                                            .text_size(t.text.caption)
                                            .text_color(t.colors.text_muted)
                                            .child("Settings and providers"),
                                    ),
                            )
                            .child(icon(
                                "icons/settings.svg",
                                t.size.icon,
                                t.colors.text_subtle,
                            ))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenSettings), cx)
                            }),
                    ),
                );
        div()
            .w(t.size.sidebar)
            .flex_none()
            .h_full()
            .pl(t.space.sm)
            .pr(t.space.sm)
            .pb(t.space.xs)
            .child(sheet)
            .children(menu)
    }
}

/// The sidebar collapsed (Ctrl+B): a narrow rail of the same actions as
/// icon buttons, each named in its tooltip, so nothing goes out of reach.
pub fn rail(t: &crate::theme::Theme, screen: Screen) -> impl IntoElement {
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
        .w(t.size.rail)
        .flex_none()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(t.space.xs)
        .py(t.space.sm)
        .bg(t.extra.pane)
        .border_r_1()
        .border_color(t.colors.border)
        .child(
            nav(
                "rail-expand",
                "icons/panel-left.svg",
                format!("Show the sidebar ({})", shortcut("B")),
                false,
            )
            .on_click(dispatch(Box::new(ToggleSidebar))),
        )
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
                        .bg(t.extra.accent)
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

/// A chat's status, in a fixed slot before its title: a voxel stepping
/// round while it works, a breathing amber dot when it needs you, a warning when it
/// ended on an error, a blue dot when a reply is unread, a faint ring at rest.
pub fn status_mark(status: ChatStatus, id: usize, t: &crate::theme::Theme) -> gpui::AnyElement {
    let dot = || div().size(px(7.)).rounded(t.radius.pill);
    let inner: gpui::AnyElement = match status {
        ChatStatus::Working => super::voxels::running(t.colors.sky, ("chat-working", id)),
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

/// A nav row's words: they give way (with an ellipsis) to the shortcut
/// beside them in a narrow sidebar.
fn nav_label(text: &'static str) -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(text)
}
