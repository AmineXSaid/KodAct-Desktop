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

use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, IntoElement, PathPromptOptions,
    Pixels, Render, SharedString, Window,
};

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
    pub fn new(
        state: Entity<AppState>,
        composer: Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state, composer }
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

/// A project's mark: a folder on a tile in a colour of its own (the same
/// each time), with a corner badge when the folder is remote (WSL, SSH or a
/// container).
pub fn project_tile(t: &Theme, name: &str, path: &std::path::Path, size: Pixels) -> AnyElement {
    let colors = [
        t.colors.primary,
        t.colors.marigold,
        t.extra.success,
        t.colors.sky,
        t.colors.coral,
    ];
    let h = name
        .bytes()
        .fold(7usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    let color = colors[h % colors.len()];
    // A folder, in the project's own hue, on a soft wash of it: the shape
    // says what it is, the colour tells projects apart at a glance. No
    // outline, so a small tile reads as a mark rather than a checkbox.
    let glyph = if size >= px(40.) {
        t.size.icon_lg
    } else if size >= px(28.) {
        t.size.icon
    } else {
        t.size.icon_sm
    };
    let remote = crate::remote::Remote::parse(path);
    div()
        .relative()
        .flex_none()
        .size(size)
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .bg(color.opacity(if t.is_dark { 0.22 } else { 0.15 }))
                .child(icon("icons/folder.svg", glyph, color)),
        )
        // Too small a tile for a badge: the title bar names the remote beside it.
        .children(remote.filter(|_| size >= px(32.)).map(|r| {
            div()
                .absolute()
                .right(px(-3.))
                .bottom(px(-3.))
                .size(t.size.icon + px(2.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.pill)
                .bg(t.colors.surface)
                .border_1()
                .border_color(t.colors.border)
                .child(icon(r.target.icon(), t.size.icon_xs, t.colors.text_muted))
        }))
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
            format!("opened {} ago", ago(opened_minutes_ago))
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
        let s = self.state.read(cx);
        let home = crate::rules::home_dir();
        let (hover_border, hover_bg) = (t.colors.text_subtle, t.colors.hover);
        let mut projects: Vec<_> = s.projects.iter().filter(|p| p.saved).cloned().collect();
        projects.sort_by_key(|p| (!p.pinned, p.opened_minutes_ago));
        let connected: Vec<_> = s
            .projects
            .iter()
            .filter(|p| !p.saved && !p.scratch)
            .cloned()
            .collect();
        let cards = projects.into_iter().enumerate().map(|(i, p)| {
            let chats = s.chats.iter().filter(|c| c.project == p.id).count();
            let current = p.id == s.current_project;
            let state = self.state.clone();
            let id = p.id;
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
                        .when(p.pinned, |d| {
                            d.child(icon("icons/pin.svg", t.size.icon_sm, t.colors.text_subtle))
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
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(div().flex_none().whitespace_nowrap().child(project_meta(
                            chats,
                            current,
                            p.opened_minutes_ago,
                        )))
                        .child(div().flex_1())
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
                                            .text_size(t.size.greeting)
                                            .text_color(t.colors.text_strong)
                                            .child("Projects"),
                                    )
                                    .child(
                                        // The section's subhead, in the editorial serif.
                                        div()
                                            .font_family(t.font_serif.clone())
                                            .text_size(t.text.subheading)
                                            .text_color(t.colors.text_body)
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
                                    .on_click(move |_, _, cx| open_folder(state.clone(), cx)),
                            ),
                    )
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
        let pinned = p.pinned;
        // The folder Forge started in is always here; others can leave the list.
        let (saved, removable) = (p.saved, Some(&p.path) != s.launch_dir.as_ref());
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
                                    .child(
                                        div()
                                            .text_size(t.text.heading_sm)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(t.colors.text_strong)
                                            .child(p.name.clone()),
                                    )
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
                            .when(saved && removable, |d| {
                                d.child(
                                    div()
                                        .id("project-forget")
                                        .flex()
                                        .items_center()
                                        .gap(t.space.xs)
                                        .h(t.size.control + px(4.))
                                        .px(t.space.md)
                                        .rounded(t.radius.button)
                                        .cursor_pointer()
                                        .text_color(t.colors.text_muted)
                                        .hover(move |st| st.bg(hover))
                                        .tooltip(|w, cx| {
                                            gpui::component::tooltip::Tooltip::new(
                                                "Take it off your projects; its chats and files stay",
                                            )
                                            .build(w, cx)
                                        })
                                        .child(icon("icons/x.svg", t.size.icon_sm, t.colors.text_muted))
                                        .child("Remove")
                                        .on_click(move |_, _, cx| {
                                            forget_state.update(cx, |s, cx| s.forget_project(id, cx))
                                        }),
                                )
                            })
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
                                    .child(icon(
                                        "icons/pin.svg",
                                        t.size.icon_sm,
                                        if pinned {
                                            t.colors.primary
                                        } else {
                                            t.colors.text_muted
                                        },
                                    ))
                                    .child(if pinned { "Unpin" } else { "Pin" })
                                    .on_click(move |_, _, cx| {
                                        pin_state.update(cx, |s, cx| s.toggle_project_pin(id, cx))
                                    }),
                            )
                            .child(
                                div()
                                    .id("project-map")
                                    .flex()
                                    .items_center()
                                    .gap(t.space.xs)
                                    .h(t.size.control + px(4.))
                                    .px(t.space.md)
                                    .rounded(t.radius.button)
                                    .cursor_pointer()
                                    .text_color(t.colors.text_muted)
                                    .hover(move |st| st.bg(hover))
                                    .child(icon(
                                        "icons/map.svg",
                                        t.size.icon_sm,
                                        t.colors.text_muted,
                                    ))
                                    .child("Map")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(ShowProjectMap), cx)
                                    }),
                            )
                            .child(
                                theme::ghost_button(t, "Rules and memory")
                                    .id("project-rules")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(OpenRules), cx)
                                    }),
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
