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
    /// The list's search shows (its icon was pressed, or it holds words).
    searching: bool,
    /// The sort menu is open.
    sort_open: bool,
    /// A project page's ⋯ menu is open.
    actions_open: bool,
    /// The two menus leave as they came (`motion::Presence`): this frame's
    /// state, `Some(leaving)` while drawn.
    sort_presence: super::motion::Presence<()>,
    actions_presence: super::motion::Presence<()>,
    sort_frame: Option<bool>,
    actions_frame: Option<bool>,
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
            searching: false,
            sort_open: false,
            actions_open: false,
            sort_presence: Default::default(),
            actions_presence: Default::default(),
            sort_frame: None,
            actions_frame: None,
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

/// A project's tile in the list: its folder (or where a remote one lives)
/// on a soft square, as a row's lead.
fn tile(t: &Theme, path: &std::path::Path) -> impl IntoElement {
    let glyph = crate::remote::Remote::parse(path)
        .map(|r| r.target.icon())
        .unwrap_or("icons/folder.svg");
    div()
        .flex_none()
        .size(t.size.hit)
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.card)
        .bg(t.colors.hover)
        .child(icon(glyph, t.size.icon_sm, t.colors.text_muted))
}

/// A quiet outlined label beside a name ("Open now").
fn badge(t: &Theme, text: &'static str) -> impl IntoElement {
    div()
        .flex_none()
        .px(t.space.tight)
        .rounded(t.radius.pill)
        .border_1()
        .border_color(t.colors.border)
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(text)
}

/// When a project was last opened, as a day: "Today", "Yesterday",
/// "Oct 12", or "Oct 12, 2024" in another year.
fn day(opened_at: u64) -> String {
    use chrono::{Datelike, Local, TimeZone};
    let Some(when) = Local.timestamp_opt(opened_at as i64, 0).single() else {
        return String::new();
    };
    let today = Local::now().date_naive();
    let d = when.date_naive();
    if d == today {
        "Today".into()
    } else if Some(d) == today.pred_opt() {
        "Yesterday".into()
    } else if d.year() == today.year() {
        when.format("%b %-d").to_string()
    } else {
        when.format("%b %-d, %Y").to_string()
    }
}

impl ProjectsView {
    /// The Projects page, after Claude's: the title with its few tools
    /// (search, sort, New project) on one line, then the projects in two
    /// columns of quiet rows: a tile, the name, when you last opened it, and
    /// the pin. Pinned ones come first and keep their pin in view.
    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let archived = shown(&self.state.read(cx).projects, Tab::Archived, self.sort, "").len();
        let s = self.state.read(cx);
        let home = crate::rules::home_dir();
        let (hover_bg, row_bg) = (t.colors.hover, t.colors.surface);
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
        let rows: Vec<AnyElement> = projects
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let current = p.id == s.current_project;
                let (state, pin, restore) =
                    (self.state.clone(), self.state.clone(), self.state.clone());
                let id = p.id;
                let pinned = p.pinned;
                let group: SharedString = format!("project-row-{id}").into();
                let tip: SharedString = match crate::remote::Remote::parse(&p.path) {
                    Some(r) => format!("{} · {}", r.target.label(), r.path),
                    None => crate::rules::display_path(&p.path, home.as_deref()),
                }
                .into();
                let row = div()
                    .id(("project-card", p.id))
                    .group(group.clone())
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(t.space.md)
                    .px(t.space.md)
                    .py(t.space.sm + px(2.))
                    .rounded(t.radius.card)
                    .bg(row_bg)
                    .border_1()
                    .border_color(t.colors.border)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover_bg))
                    .tooltip(move |w, cx| {
                        gpui::component::tooltip::Tooltip::new(tip.clone()).build(w, cx)
                    })
                    .child(tile(t, &p.path))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(t.space.sm)
                                    .min_w_0()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(t.colors.text_strong)
                                            .child(p.name.clone()),
                                    )
                                    .when(current, |d| d.child(badge(t, "Open now"))),
                            )
                            .child(
                                div()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_subtle)
                                    .child(day(p.opened_at)),
                            ),
                    )
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
                    })
                    // The pin: always there on a pinned project, on hover
                    // on the others.
                    .when(tab != Tab::Archived, |d| {
                        d.child(
                            div()
                                .when(!pinned, |d| {
                                    d.invisible().group_hover(group.clone(), |st| st.visible())
                                })
                                .child(
                                    super::button::Button::icon(
                                        SharedString::from(format!("project-pin-{id}")),
                                        "icons/pin.svg",
                                        if pinned { "Unpin" } else { "Pin to the top" },
                                    )
                                    .compact()
                                    .on_click(
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            pin.update(cx, |s, cx| s.toggle_project_pin(id, cx))
                                        },
                                    ),
                                ),
                        )
                    })
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| s.show_project(id, cx)));
                super::motion::row_in(row, ("project-card-in", id), i, t).into_any_element()
            })
            .collect();
        // Two columns: rows of two, the last one alone if odd.
        let mut grid = div().flex().flex_col().gap(t.space.sm);
        let mut rows = rows.into_iter();
        while let Some(a) = rows.next() {
            let b = rows.next();
            grid = grid.child(
                div()
                    .flex()
                    .gap(t.space.sm)
                    .child(div().flex_1().min_w_0().flex().flex_col().child(a))
                    .child(div().flex_1().min_w_0().flex().flex_col().children(b)),
            );
        }
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
                .child(tile(t, &p.path))
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

        // The tools beside the title: search (it opens in place), sort (a
        // small menu, which also holds the archive), New project.
        let searching = self.searching || !query.trim().is_empty();
        let search = searching.then(|| {
            div()
                .w(px(220.))
                .flex()
                .items_center()
                .gap(t.space.xs)
                .h(t.size.control)
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
                )
        });
        let search_button = (!searching).then(|| {
            super::button::Button::icon("projects-search", "icons/search.svg", "Search projects")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.searching = true;
                    this.search.update(cx, |i, cx| i.focus(window, cx));
                    cx.notify();
                }))
        });
        let sort_item = |label: &'static str, on: bool, id: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(t.space.sm)
                .h(t.size.row)
                .px(t.space.sm)
                .rounded(t.radius.button)
                .cursor_pointer()
                .text_color(t.colors.text)
                .hover(move |st| st.bg(hover_bg))
                .child(div().flex_1().child(label))
                .child(div().w(t.size.icon_sm).when(on, |d| {
                    d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.text))
                }))
        };
        let sort_menu = self.sort_frame.is_some().then(|| {
            let pick = |sort: Sort| {
                cx.listener(move |this: &mut Self, _: &gpui::ClickEvent, _, cx| {
                    this.sort = sort;
                    this.sort_open = false;
                    cx.notify();
                })
            };
            theme::popover(t)
                .id("projects-sort-menu")
                .absolute()
                .top(t.size.control + t.space.xs)
                .right_0()
                .w(px(200.))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.sort_open = false;
                    cx.notify();
                }))
                .child(
                    div()
                        .px(t.space.sm)
                        .py(t.space.xs)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child("Sort by"),
                )
                .child(
                    sort_item("Last opened", self.sort == Sort::Recent, "sort-recent")
                        .on_click(pick(Sort::Recent)),
                )
                .child(
                    sort_item("Name", self.sort == Sort::Name, "sort-name")
                        .on_click(pick(Sort::Name)),
                )
                .child(
                    sort_item("Date created", self.sort == Sort::Created, "sort-created")
                        .on_click(pick(Sort::Created)),
                )
                .child(theme::rule(t).my(t.space.xs))
                .child(
                    sort_item(
                        if tab == Tab::Archived {
                            "Back to projects"
                        } else {
                            "Archived"
                        },
                        false,
                        "sort-archived",
                    )
                    .when(tab != Tab::Archived, |d| {
                        d.child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(archived.to_string()),
                        )
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.tab = if this.tab == Tab::Archived {
                            Tab::All
                        } else {
                            Tab::Archived
                        };
                        this.sort_open = false;
                        cx.notify();
                    })),
                )
        });
        let sort_button = div()
            .relative()
            .child(
                super::button::Button::icon(
                    "projects-sort",
                    "icons/arrow-up-down.svg",
                    "Sort, or show archived",
                )
                .tooltip(!self.sort_open)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.sort_open = !this.sort_open;
                    cx.notify();
                })),
            )
            // Deferred: it paints over the rows below the title.
            .children(sort_menu.map(|m| {
                let leaving = self.sort_frame == Some(true);
                gpui::deferred(super::motion::unfold(
                    m,
                    "projects-sort-menu",
                    false,
                    leaving,
                    t,
                ))
                .with_priority(1)
            }));

        div()
            .id("projects-page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .px(t.space.xxl)
            .pt(t.space.xxl + t.space.lg)
            .pb(t.space.xxl)
            .child(
                div()
                    .w_full()
                    .max_w(t.size.column)
                    .flex()
                    .flex_col()
                    .gap(t.space.xl)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(t.font_display.clone())
                                    .font_weight(t.display_weight)
                                    .text_size(t.text.heading_sm + px(4.))
                                    .text_color(t.colors.text_strong)
                                    .child(if tab == Tab::Archived {
                                        "Archived projects"
                                    } else {
                                        "Projects"
                                    }),
                            )
                            .children(search)
                            .children(search_button)
                            .child(sort_button)
                            .child(
                                theme::primary_button(t, "New project")
                                    .id("open-folder")
                                    .flex_none()
                                    .ml(t.space.xs)
                                    .on_click(move |_, _, cx| {
                                        state.update(cx, |s, cx| {
                                            s.new_project_open = !s.new_project_open;
                                            cx.notify();
                                        })
                                    }),
                            ),
                    )
                    .when(sheet_open, |d| d.child(self.new_project.clone()))
                    .when(none_shown, |d| {
                        d.child(div().py(t.space.xl).text_color(t.colors.text_subtle).child(
                            if !query.trim().is_empty() {
                                format!("No project matches “{}”.", query.trim())
                            } else {
                                match tab {
                                    Tab::All => {
                                        "No projects yet. New project opens a folder as one.".into()
                                    }
                                    Tab::Archived => "Nothing archived.".into(),
                                }
                            },
                        ))
                    })
                    .child(grid)
                    .when(has_connected && tab == Tab::All, |d| {
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
                .font_family(t.font_display.clone())
                .font_weight(t.display_weight)
                .text_size(t.text.heading_sm + px(4.))
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
                    .max_w(t.size.composer_width)
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
                            .gap(t.space.xs)
                            .child(div().flex_1().min_w_0().child(name))
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
                                    "icons/pin.svg",
                                    if pinned { "Unpin" } else { "Pin: first in your lists" },
                                )
                                .on_click(move |_, _, cx| {
                                    pin_state.update(cx, |s, cx| s.toggle_project_pin(id, cx))
                                }),
                            )
                            .child(
                                div()
                                    .relative()
                                    .child(
                                        super::button::Button::icon(
                                            "project-more",
                                            "icons/ellipsis.svg",
                                            "More: map, archive, remove",
                                        )
                                        .tooltip(!self.actions_open)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.actions_open = !this.actions_open;
                                            cx.notify();
                                        })),
                                    )
                                    .children(self.actions_frame.is_some().then(|| gpui::deferred(super::motion::unfold({
                                        let item = |id: &'static str, glyph: &'static str, label: &'static str| {
                                            div()
                                                .id(id)
                                                .flex()
                                                .items_center()
                                                .gap(t.space.sm)
                                                .h(t.size.row)
                                                .px(t.space.sm)
                                                .rounded(t.radius.button)
                                                .cursor_pointer()
                                                .text_color(t.colors.text)
                                                .hover(move |st| st.bg(hover))
                                                .child(icon(glyph, t.size.icon_sm, t.colors.text_muted))
                                                .child(label)
                                        };
                                        theme::popover(t)
                                            .id("project-more-menu")
                                            .absolute()
                                            .top(t.size.control + t.space.xs)
                                            .right_0()
                                            .w(px(220.))
                                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                                this.actions_open = false;
                                                cx.notify();
                                            }))
                                            .child(item("project-map", "icons/map.svg", "Map of its folders").on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.actions_open = false;
                                                    window.dispatch_action(Box::new(ShowProjectMap), cx)
                                                }),
                                            ))
                                            .when(saved && removable, |d| {
                                                d.child(item("project-archive", "icons/archive.svg", "Archive").on_click(
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.actions_open = false;
                                                        archive_state.update(cx, |s, cx| s.archive_project(id, true, cx))
                                                    }),
                                                ))
                                                .child(item("project-forget", "icons/x.svg", "Remove from projects").on_click(
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.actions_open = false;
                                                        forget_state.update(cx, |s, cx| s.forget_project(id, cx))
                                                    }),
                                                ))
                                            })
                                    }, "project-more-menu", false, self.actions_frame == Some(true), t)).with_priority(1))),
                            ),
                    )
                    .child(description)
                    .child(self.composer.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .when(all_chats > 0, |d| d.child(
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
                                            .child("Chats"),
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
                            ))
                            .when(rows.is_empty() && !q.is_empty(), |d| {
                                d.child(
                                    div()
                                        .px(t.space.md)
                                        .py(t.space.md)
                                        .text_color(t.colors.text_subtle)
                                        .child(format!("No chat here mentions “{q}”.")),
                                )
                            })
                            // No chats yet: what a project is for, centred and quiet.
                            .when(rows.is_empty() && q.is_empty(), |d| {
                                d.child(
                                    div()
                                        .pt(t.space.xxl + t.space.lg)
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(t.space.md)
                                        .text_color(t.colors.text_muted)
                                        .child(icon("icons/message-square.svg", t.size.icon_lg, t.colors.text_muted))
                                        .child(
                                            "Forge reads the same instructions, files and memory every time you chat in this project.",
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let out = t.size.menu_out_ms;
        self.sort_frame = self
            .sort_presence
            .frame(self.sort_open.then_some(()), out, window)
            .map(|f| f.1);
        self.actions_frame = self
            .actions_presence
            .frame(self.actions_open.then_some(()), out, window)
            .map(|f| f.1);
        match self.state.read(cx).screen {
            Screen::Project(id) => self.project(id, &t, cx),
            _ => self.list(&t, cx),
        }
    }
}
