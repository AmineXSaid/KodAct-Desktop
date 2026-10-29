//! Projects: the folders Forge works in.
//!
//! - The Projects page lists every project as a card (name, description,
//!   folder, branch, how many chats, when it was last opened), in tabs (All,
//!   Starred, Archived), searched and sorted; New project adds one.
//! - A project's page names it (click the name or description to edit),
//!   has a composer to start a chat there, its chats with their status, and
//!   its actions: star, archive, remove, map, rules.
//!
//! Chats, rules and memory belong to a project; switching project (here, in
//! the title bar, or by opening one of its chats) moves the whole window to
//! that folder.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, IntoElement, PathPromptOptions,
    Pixels, Render, SharedString, Subscription, Window,
};

use super::composer::Composer;
use super::sidebar::{ago, status_mark};
use super::*;
use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme, Theme};

pub struct ProjectsView {
    state: Entity<AppState>,
    composer: Entity<Composer>,
    /// The Projects page's search, tab and order.
    search: Entity<InputState>,
    tab: Tab,
    sort: Sort,
    /// A project's name or description being edited in place.
    edit: Entity<InputState>,
    editing: Option<(usize, Field)>,
    /// The page's right rail: what every chat gets, instructions.
    rail: Entity<super::project_rail::ProjectRail>,
    /// New project's three ways to start (`new_project.rs`).
    new_project: Entity<super::new_project::NewProject>,
    /// A project page's search over its chats.
    chat_search: Entity<InputState>,
    _subs: Vec<Subscription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    All,
    Starred,
    Archived,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Recent,
    Name,
    Created,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Description,
}

/// The projects a tab shows, searched (name, description, folder) and in order.
fn shown(
    projects: &[crate::state::Project],
    tab: Tab,
    sort: Sort,
    query: &str,
) -> Vec<crate::state::Project> {
    let q = query.trim().to_lowercase();
    let mut out: Vec<_> = projects
        .iter()
        .filter(|p| p.saved && !p.scratch)
        .filter(|p| match tab {
            Tab::All => !p.archived,
            Tab::Starred => p.pinned && !p.archived,
            Tab::Archived => p.archived,
        })
        .filter(|p| {
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.description.to_lowercase().contains(&q)
                || p.path.to_string_lossy().to_lowercase().contains(&q)
        })
        .cloned()
        .collect();
    match sort {
        Sort::Recent => out.sort_by_key(|p| (!p.pinned, p.opened_minutes_ago)),
        Sort::Name => out.sort_by_key(|p| (!p.pinned, p.name.to_lowercase())),
        Sort::Created => out.sort_by_key(|p| (!p.pinned, std::cmp::Reverse(p.created))),
    }
    out
}

impl ProjectsView {
    pub fn new(
        state: Entity<AppState>,
        composer: Entity<Composer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects"));
        let edit = cx.new(|cx| InputState::new(window, cx));
        let rail = cx.new(|cx| super::project_rail::ProjectRail::new(state.clone(), window, cx));
        let new_project =
            cx.new(|cx| super::new_project::NewProject::new(state.clone(), window, cx));
        let chat_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search these chats"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&chat_search, |_, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            }),
            // A name or description is kept on Enter, or when you click away.
            cx.subscribe_in(&edit, window, |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    this.commit(window, cx);
                }
            }),
        ];
        Self {
            state,
            composer,
            search,
            tab: Tab::All,
            sort: Sort::Recent,
            edit,
            editing: None,
            rail,
            new_project,
            chat_search,
            _subs: subs,
        }
    }

    /// Edit a project's name or description in place.
    fn start_edit(&mut self, id: usize, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.state.read(cx).project(id) else {
            return;
        };
        let (value, hint) = match field {
            Field::Name => (p.name.to_string(), "Name"),
            Field::Description => (p.description.to_string(), "What is this project for?"),
        };
        self.editing = Some((id, field));
        self.edit.update(cx, |i, cx| {
            i.set_placeholder(hint, window, cx);
            i.set_value(value, window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, field)) = self.editing.take() else {
            return;
        };
        let value = self.edit.read(cx).value().to_string();
        self.edit.update(cx, |i, cx| i.set_value("", window, cx));
        self.state.update(cx, |s, cx| match field {
            Field::Name => s.rename_project(id, &value, cx),
            Field::Description => s.describe_project(id, &value, cx),
        });
        cx.notify();
    }

    /// The Projects page's tabs (with counts), search and order.
    fn toolbar(
        &self,
        t: &Theme,
        counts: (usize, usize, usize),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let hover = t.colors.hover;
        let tab = |label: &'static str, n: usize, which: Tab, cx: &mut Context<Self>| {
            let on = self.tab == which;
            div()
                .id(SharedString::from(format!("projects-tab-{label}")))
                .flex()
                .items_center()
                .gap(t.space.xs)
                .h(t.size.control)
                .px(t.space.md)
                .rounded(t.radius.compact)
                .cursor_pointer()
                .text_size(t.text.body_sm)
                .when(on, |d| {
                    crate::theme::selected(t, d).text_color(t.colors.text_strong)
                })
                .when(!on, |d| {
                    d.text_color(t.colors.text_muted)
                        .hover(move |st| st.bg(hover))
                })
                .child(label)
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(n.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = which;
                    cx.notify();
                }))
        };
        let order = |label: &'static str, which: Sort, cx: &mut Context<Self>| {
            let on = self.sort == which;
            div()
                .id(SharedString::from(format!("projects-sort-{label}")))
                .px(t.space.sm)
                .h(t.size.control)
                .flex()
                .items_center()
                .rounded(t.radius.compact)
                .cursor_pointer()
                .text_size(t.text.caption)
                .text_color(if on {
                    t.colors.text_strong
                } else {
                    t.colors.text_subtle
                })
                .when(on, |d| d.font_weight(FontWeight::SEMIBOLD))
                .when(!on, |d| d.hover(move |st| st.bg(hover)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sort = which;
                    cx.notify();
                }))
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.space.sm)
            .child(tab("All", counts.0, Tab::All, cx))
            .child(tab("Starred", counts.1, Tab::Starred, cx))
            .child(tab("Archived", counts.2, Tab::Archived, cx))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .pr(t.space.xs)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child("Sort"),
                    )
                    .child(order("Recent", Sort::Recent, cx))
                    .child(order("Name", Sort::Name, cx))
                    .child(order("Created", Sort::Created, cx)),
            )
            .child(
                div()
                    .w(px(220.))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .px(t.space.sm)
                    .rounded(t.radius.field)
                    .border_1()
                    .border_color(t.colors.border)
                    .bg(t.colors.surface)
                    .child(icon(
                        "icons/search.svg",
                        t.size.icon_sm,
                        t.colors.text_subtle,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.search).appearance(false)),
                    ),
            )
    }
}

/// Ask for a folder with the system picker and add it as a project.
pub fn open_folder(state: Entity<AppState>, cx: &mut gpui::App) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open as a project".into()),
    });
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

/// A project's mark: its folder, as an outline icon.
pub fn project_tile(t: &Theme, name: &str, path: &std::path::Path, size: Pixels) -> AnyElement {
    let _ = name;
    // A plain outline, as the rest of the chrome draws its icons: a folder,
    // or for a remote one the place it lives (WSL, an SSH host, a
    // container). No tinted tile, no colour coding.
    let glyph = if size >= px(40.) {
        t.size.icon_lg
    } else if size >= px(28.) {
        t.size.icon
    } else {
        t.size.icon_sm
    };
    let path_icon = crate::remote::Remote::parse(path)
        .map(|r| r.target.icon())
        .unwrap_or("icons/folder.svg");
    // As wide as the icon, so it lines up with the text beside or under it.
    div()
        .flex_none()
        .h(size)
        .flex()
        .items_center()
        .child(icon(path_icon, glyph, t.colors.text_muted))
        .into_any_element()
}

/// "3 chats · opened 2 h ago", or "open now" for the one you're in.
pub fn project_meta(chats: usize, current: bool, opened_minutes_ago: u32) -> String {
    format!(
        "{chats} chat{} · {}",
        if chats == 1 { "" } else { "s" },
        if current {
            "open now".to_string()
        } else {
            match opened_minutes_ago {
                0 => "opened just now".to_string(),
                m => format!("opened {} ago", ago(m)),
            }
        }
    )
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
        .child(icon(
            "icons/git-branch.svg",
            t.size.icon_xs,
            t.colors.text_muted,
        ))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(branch),
        )
}

impl ProjectsView {
    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let counts = {
            let all = &self.state.read(cx).projects;
            let n = |tab| shown(all, tab, self.sort, "").len();
            (n(Tab::All), n(Tab::Starred), n(Tab::Archived))
        };
        let toolbar = self.toolbar(t, counts, cx);
        let s = self.state.read(cx);
        let home = crate::rules::home_dir();
        let (hover_border, hover_bg) = (t.colors.text_subtle, t.colors.hover);
        let query = self.search.read(cx).value().to_string();
        let projects = shown(&s.projects, self.tab, self.sort, &query);
        let tab = self.tab;
        let connected: Vec<_> = s
            .projects
            .iter()
            .filter(|p| !p.saved && !p.scratch)
            .cloned()
            .collect();
        let none_shown = projects.is_empty();
        let cards = projects.into_iter().enumerate().map(|(i, p)| {
            let chats = s.chats.iter().filter(|c| c.project == p.id).count();
            let current = p.id == s.current_project;
            let state = self.state.clone();
            let (star, restore) = (self.state.clone(), self.state.clone());
            let id = p.id;
            let starred = p.pinned;
            let path = match crate::remote::Remote::parse(&p.path) {
                Some(r) => format!("{} · {}", r.target.label(), r.path),
                None => crate::rules::display_path(&p.path, home.as_deref()),
            };
            let card = div()
                .id(("project-card", p.id))
                .w(px(280.))
                .min_h(px(148.))
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .p(t.space.lg)
                .bg(t.colors.surface)
                .border_1()
                .border_color(if current {
                    t.colors.primary.opacity(0.45)
                } else {
                    t.colors.border
                })
                .rounded(t.radius.card)
                .cursor_pointer()
                .hover(move |st| st.border_color(hover_border))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(t.space.md)
                        .child(project_tile(t, &p.name, &p.path, px(36.)))
                        .child(div().flex_1())
                        .when(tab != Tab::Archived, |d| {
                            d.child(
                                super::button::Button::icon(
                                    SharedString::from(format!("project-star-{id}")),
                                    if starred {
                                        "icons/star-filled.svg"
                                    } else {
                                        "icons/star.svg"
                                    },
                                    if starred { "Unstar" } else { "Star" },
                                )
                                .compact()
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    star.update(cx, |s, cx| s.toggle_project_pin(id, cx))
                                }),
                            )
                        })
                        .when(tab == Tab::Archived, |d| {
                            d.child(
                                theme::text_button(t, "Restore")
                                    .id(("project-restore", id))
                                    .text_size(t.text.caption)
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        restore.update(cx, |s, cx| s.archive_project(id, false, cx))
                                    }),
                            )
                        }),
                )
                .child(
                    div()
                        .flex_none()
                        .pt(t.space.xs)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.body)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(p.name.clone()),
                )
                .when(!p.description.is_empty(), |d| {
                    d.child(
                        div()
                            .flex_none()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.body_sm)
                            .text_color(t.colors.text_body)
                            .child(p.description.clone()),
                    )
                })
                .child(
                    div()
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(path),
                )
                .child(div().flex_1())
                .child(
                    // The branch drops to its own line rather than be cut.
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(t.space.sm)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(div().flex_none().whitespace_nowrap().child(project_meta(
                            chats,
                            current,
                            p.opened_minutes_ago,
                        )))
                        .when(!p.branch.is_empty(), |d| {
                            d.child(branch_pill(t, p.branch.clone()))
                        }),
                )
                .on_click(move |_, _, cx| state.update(cx, |s, cx| s.show_project(id, cx)));
            // The cards rise onto the page one after another, like Notion's.
            super::motion::row_in(card, ("project-card-in", id), i, t)
        });
        // Folders you connected to but didn't make projects: one line each.
        let connected_rows = connected.into_iter().map(|p| {
            let (open, save) = (self.state.clone(), self.state.clone());
            let id = p.id;
            let place = crate::remote::Remote::parse(&p.path)
                .map(|r| r.target.label())
                .unwrap_or_else(|| crate::rules::display_path(&p.path, home.as_deref()));
            div()
                .id(("connected-row", p.id))
                .flex()
                .items_center()
                .gap(t.space.md)
                .px(t.space.md)
                .py(t.space.sm)
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover(move |st| st.bg(hover_bg))
                .child(project_tile(t, &p.name, &p.path, px(28.)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().text_color(t.colors.text_strong).child(p.name.clone()))
                        .child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(place),
                        ),
                )
                .child(
                    theme::text_button(t, "Save as project")
                        .id(("connected-save", p.id))
                        .text_size(t.text.caption)
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            save.update(cx, |s, cx| s.save_project(id, cx))
                        }),
                )
                .on_click(move |_, _, cx| {
                    open.update(cx, |s, cx| {
                        s.select_project(id, cx);
                        s.screen = Screen::Chat;
                        cx.notify();
                    })
                })
        });
        let has_connected = s.projects.iter().any(|p| !p.saved && !p.scratch);
        let sheet_open = s.new_project_open;
        let state = self.state.clone();
        div()
            .id("projects-page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .px(t.space.xxl)
            .pt(t.space.xxl)
            .pb(t.space.xxl)
            .child(
                div()
                    .w_full()
                    .max_w(px(900.))
                    .flex()
                    .flex_col()
                    .gap(t.space.xl)
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(t.space.lg)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(t.space.xs)
                                    .child(
                                        div()
                                            .font_family(t.font_display.clone())
                                            .font_weight(t.display_weight)
                                            .text_size(t.text.heading_sm)
                                            .text_color(t.colors.text_strong)
                                            .child("Projects"),
                                    )
                                    .child(
                                        // The page's lede, as every page's: body size, muted.
                                        div()
                                            .text_size(t.text.body)
                                            .text_color(t.colors.text_muted)
                                            .child("A folder with its own chats, rules and memory. Open one to pick up where you left off."),
                                    ),
                            )
                            .child(
                                theme::primary_button(t, "New project")
                                    .id("open-folder")
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(t.space.xs)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |s, cx| {
                                            s.new_project_open = !s.new_project_open;
                                            cx.notify();
                                        })
                                    }),
                            ),
                    )
                    .when(sheet_open, |d| d.child(self.new_project.clone()))
                    .child(toolbar)
                    .when(none_shown, |d| {
                        d.child(
                            div()
                                .py(t.space.xl)
                                .text_color(t.colors.text_subtle)
                                .child(if !query.trim().is_empty() {
                                    format!("No project matches “{}”.", query.trim())
                                } else {
                                    match tab {
                                        Tab::All => "No projects yet. New project opens a folder as one.".into(),
                                        Tab::Starred => "Star a project to keep it at the top.".into(),
                                        Tab::Archived => "Nothing archived.".into(),
                                    }
                                }),
                        )
                    })
                    .child(div().flex().flex_wrap().gap(t.space.lg).children(cards))
                    .when(has_connected, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(t.space.xs)
                                .child(
                                    div()
                                        .px(t.space.md)
                                        .text_size(t.text.caption)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t.colors.text_subtle)
                                        .child("Connected, not saved as projects"),
                                )
                                .children(connected_rows),
                        )
                    }),
            )
            .into_any_element()
    }

    fn project(&self, id: usize, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.state.read(cx);
        let Some(p) = s.project(id).cloned() else {
            return div().into_any_element();
        };
        let home = crate::rules::home_dir();
        let hover = t.colors.hover;
        let all_chats = s.chats.iter().filter(|c| c.project == id).count();
        let q = self.chat_search.read(cx).value().trim().to_lowercase();
        let mut chats: Vec<_> = s
            .chats
            .iter()
            .filter(|c| c.project == id)
            .filter(|c| {
                q.is_empty()
                    || c.title.to_lowercase().contains(&q)
                    || c.messages.iter().any(|m| match m {
                        crate::state::Message::User { text, .. } => {
                            text.to_lowercase().contains(&q)
                        }
                        _ => false,
                    })
            })
            .collect();
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(t.colors.text)
                            .child(c.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(ago(c.minutes_ago)),
                    )
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| s.open_chat(chat, cx)))
                    .into_any_element()
            })
            .collect();
        let (pin_state, back_state) = (self.state.clone(), self.state.clone());
        let (save_state, forget_state) = (self.state.clone(), self.state.clone());
        let archive_state = self.state.clone();
        let pinned = p.pinned;
        let editing = self.editing.filter(|(e, _)| *e == id).map(|(_, f)| f);
        let field = |t: &Theme| {
            div()
                .w_full()
                .px(t.space.sm)
                .rounded(t.radius.field)
                .border_1()
                .border_color(t.colors.primary)
                .bg(t.colors.surface)
                .child(Input::new(&self.edit).appearance(false))
        };
        let name: AnyElement = if editing == Some(Field::Name) {
            field(t).text_size(t.text.heading_sm).into_any_element()
        } else {
            div()
                .id("project-name")
                .group("project-name")
                .flex()
                .items_center()
                .gap(t.space.xs)
                .cursor_text()
                .text_size(t.text.heading_sm)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_strong)
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(p.name.clone()),
                )
                .child(
                    div()
                        .invisible()
                        .group_hover("project-name", |d| d.visible())
                        .child(icon(
                            "icons/pencil.svg",
                            t.size.icon_xs,
                            t.colors.text_subtle,
                        )),
                )
                .tooltip(|w, cx| gpui::component::tooltip::Tooltip::new("Rename").build(w, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_edit(id, Field::Name, window, cx)
                }))
                .into_any_element()
        };
        let description: AnyElement = if editing == Some(Field::Description) {
            field(t).text_size(t.text.body).into_any_element()
        } else {
            let empty = p.description.is_empty();
            div()
                .id("project-description")
                .cursor_text()
                .text_size(t.text.body)
                .text_color(if empty {
                    t.colors.text_subtle
                } else {
                    t.colors.text_body
                })
                .hover(move |st| st.bg(hover))
                .rounded(t.radius.compact)
                .child(if empty {
                    SharedString::from("Add a description")
                } else {
                    p.description.clone()
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_edit(id, Field::Description, window, cx)
                }))
                .into_any_element()
        };
        // The folder Forge started in is always here; others can leave the list.
        let (saved, removable) = (p.saved, Some(&p.path) != s.launch_dir.as_ref());
        let page = div()
            .id("project-page")
            .flex_1()
            .min_w_0()
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
                            .child(icon(
                                "icons/chevron-right.svg",
                                t.size.icon_xs,
                                t.colors.text_subtle,
                            ))
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
                            .child(project_tile(t, &p.name, &p.path, px(44.)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(name)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(t.space.sm)
                                            .child(
                                                div()
                                                    .font_family(t.font_mono.clone())
                                                    .text_size(t.text.caption)
                                                    .text_color(t.colors.text_subtle)
                                                    .child(crate::rules::display_path(
                                                        &p.path,
                                                        home.as_deref(),
                                                    )),
                                            )
                                            .when(!p.branch.is_empty(), |d| {
                                                d.child(branch_pill(t, p.branch.clone()))
                                            }),
                                    ),
                            )
                            .when(!saved, |d| {
                                d.child(
                                    theme::ghost_button(t, "Save as project")
                                        .id("project-save")
                                        .on_click(move |_, _, cx| {
                                            save_state.update(cx, |s, cx| s.save_project(id, cx))
                                        }),
                                )
                            })
                            .child(
                                super::button::Button::icon(
                                    "project-pin",
                                    if pinned {
                                        "icons/star-filled.svg"
                                    } else {
                                        "icons/star.svg"
                                    },
                                    if pinned {
                                        "Unstar"
                                    } else {
                                        "Star: first in your lists"
                                    },
                                )
                                .on_click(move |_, _, cx| {
                                    pin_state.update(cx, |s, cx| s.toggle_project_pin(id, cx))
                                }),
                            )
                            .when(saved && removable, |d| {
                                d.child(
                                    super::button::Button::icon(
                                        "project-archive",
                                        "icons/archive.svg",
                                        "Archive: off your lists, its chats and files kept",
                                    )
                                    .on_click(
                                        move |_, _, cx| {
                                            archive_state
                                                .update(cx, |s, cx| s.archive_project(id, true, cx))
                                        },
                                    ),
                                )
                            })
                            .child(
                                super::button::Button::icon(
                                    "project-map",
                                    "icons/map.svg",
                                    "Map: its folders, and what this chat changed",
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(ShowProjectMap), cx)
                                }),
                            )
                            .when(saved && removable, |d| {
                                d.child(
                                    super::button::Button::icon(
                                        "project-forget",
                                        "icons/x.svg",
                                        "Remove from projects: its chats and files stay",
                                    )
                                    .on_click(
                                        move |_, _, cx| {
                                            forget_state
                                                .update(cx, |s, cx| s.forget_project(id, cx))
                                        },
                                    ),
                                )
                            }),
                    )
                    .child(description)
                    .child(self.composer.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(t.space.md)
                                    .px(t.space.md)
                                    .pb(t.space.xs)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_size(t.text.caption)
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(t.colors.text_subtle)
                                            .child(format!("Chats in {}", p.name)),
                                    )
                                    .when(all_chats > 3, |d| {
                                        d.child(
                                            div()
                                                .w(px(200.))
                                                .flex()
                                                .items_center()
                                                .gap(t.space.xs)
                                                .px(t.space.sm)
                                                .rounded(t.radius.field)
                                                .border_1()
                                                .border_color(t.colors.border)
                                                .bg(t.colors.surface)
                                                .text_size(t.text.caption)
                                                .child(icon(
                                                    "icons/search.svg",
                                                    t.size.icon_xs,
                                                    t.colors.text_subtle,
                                                ))
                                                .child(div().flex_1().min_w_0().child(
                                                    Input::new(&self.chat_search).appearance(false),
                                                )),
                                        )
                                    }),
                            )
                            .when(rows.is_empty() && !q.is_empty(), |d| {
                                d.child(
                                    div()
                                        .px(t.space.md)
                                        .py(t.space.md)
                                        .text_color(t.colors.text_subtle)
                                        .child(format!("No chat here mentions “{q}”.")),
                                )
                            })
                            .when(rows.is_empty() && q.is_empty(), |d| {
                                d.child(
                                    div()
                                        .px(t.space.md)
                                        .py(t.space.md)
                                        .text_color(t.colors.text_subtle)
                                        .child(
                                            "No chats here yet. Ask something above to start one.",
                                        ),
                                )
                            })
                            .children(rows),
                    ),
            );
        // The page, and its rail on the right; a file opened from the rail
        // takes the rail's place in the side panel until it is closed.
        let doc_open = self.state.read(cx).doc.is_some();
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .child(page)
            .when(!doc_open, |d| d.child(self.rail.clone()))
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
