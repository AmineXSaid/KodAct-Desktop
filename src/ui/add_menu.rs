//! The composer's "+" menu, as Claude Desktop has one: what you can add to
//! a message and the tools a reply may use. Only rows Forge can serve:
//! files and photos, the web browser (a switch, so the menu stays open),
//! and ForgeCLI's extensions: connectors (MCP servers), skills, plugins
//! (`pluginDirs`) and hooks. Claude Desktop's screenshot,
//! research, style and project rows have no engine here, so they're left
//! out rather than shown dead.
//!
//! The rows are one list (`ITEMS`); the theme's form (`theme::AddMenu`)
//! sets how they open: a plain menu, two groups with a line on each row, a
//! tray of chips inside the box, a list beside the chosen row's page, a
//! grid of tiles, or a list you type into.

use std::rc::Rc;

use gpui::component::input::{Input, InputState};
use gpui::{
    div, prelude::*, px, Action, AnyElement, App, Entity, FontWeight, IntoElement, Pixels, Point,
    SharedString, Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::{AddMenu, Theme};

/// What a row does.
#[derive(Clone, Copy)]
enum Does {
    /// Close the menu, then run this action.
    Act(fn() -> Box<dyn Action>),
    /// Flip the web browser; the menu stays open.
    Browser,
}

struct Item {
    key: &'static str,
    glyph: &'static str,
    label: &'static str,
    /// One line on what it does (the grouped and split menus show it).
    detail: &'static str,
    does: Does,
}

const ITEMS: [Item; 6] = [
    Item {
        key: "files",
        glyph: "icons/paperclip.svg",
        label: "Add files or photos",
        detail: "Or drop them anywhere on the box",
        does: Does::Act(|| Box::new(super::AttachFiles)),
    },
    Item {
        key: "browser",
        glyph: "icons/globe.svg",
        label: "Web browser",
        detail: "Forge drives it in the side panel, asking first",
        does: Does::Browser,
    },
    Item {
        key: "connectors",
        glyph: "icons/plug.svg",
        label: "Connectors",
        detail: "MCP servers and the tools they give",
        does: Does::Act(|| Box::new(super::OpenMcpServers)),
    },
    Item {
        key: "skills",
        glyph: "icons/sparkles.svg",
        label: "Skills",
        detail: "Know-how Forge pulls in when the task fits",
        does: Does::Act(|| Box::new(super::OpenSkills)),
    },
    Item {
        key: "plugins",
        glyph: "icons/puzzle.svg",
        label: "Plugins",
        detail: "Folders of commands, agents, skills and hooks",
        does: Does::Act(|| Box::new(super::OpenPlugins)),
    },
    Item {
        key: "hooks",
        glyph: "icons/webhook.svg",
        label: "Hooks",
        detail: "Commands that run around each tool",
        does: Does::Act(|| Box::new(super::OpenHooks)),
    },
];

/// What the composer gives the menu: how to close it, what a press outside
/// does, the split menu's chosen row, and the search menu's field.
#[derive(Clone)]
pub struct Hooks {
    pub close: Rc<dyn Fn(&mut App)>,
    pub on_out: Rc<dyn Fn(Point<Pixels>, &mut App)>,
    pub split_at: usize,
    pub on_split: Rc<dyn Fn(usize, &mut App)>,
    pub filter: Entity<InputState>,
}

/// The rows whose label or line holds `query`, in order.
pub fn matches(query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    (0..ITEMS.len())
        .filter(|&i| {
            q.is_empty()
                || ITEMS[i].label.to_lowercase().contains(&q)
                || ITEMS[i].detail.to_lowercase().contains(&q)
        })
        .collect()
}

/// Row `i`'s action; None for the browser, which flips in place.
pub fn action(i: usize) -> Option<Box<dyn Action>> {
    match ITEMS[i].does {
        Does::Act(action) => Some(action()),
        Does::Browser => None,
    }
}

/// Run row `i`: an action closes the menu first; the browser flips in place.
pub fn run(i: usize, hooks: &Hooks, state: &Entity<AppState>, window: &mut Window, cx: &mut App) {
    match ITEMS[i].does {
        Does::Act(action) => {
            (hooks.close)(cx);
            window.dispatch_action(action(), cx);
        }
        Does::Browser => state.update(cx, |s, cx| s.toggle_browser(cx)),
    }
}

/// What sits at a row's end: the shortcut, the browser's switch, or a
/// chevron for a row that opens a page.
fn trailing(t: &Theme, i: usize, browser: bool, state: &Entity<AppState>) -> AnyElement {
    match (ITEMS[i].key, ITEMS[i].does) {
        ("files", _) => div()
            .text_size(t.text.caption)
            .text_color(t.colors.text_muted)
            .child(super::shortcut("U"))
            .into_any_element(),
        (_, Does::Browser) => {
            let st = state.clone();
            super::button::Switch::new(("add-switch", i), browser, "Web browser")
                .on_toggle(move |_, _, cx| st.update(cx, |s, cx| s.toggle_browser(cx)))
                .into_any_element()
        }
        _ => icon("icons/chevron-right.svg", t.size.icon_sm, t.colors.text_muted)
            .into_any_element(),
    }
}

/// The menu for the theme's style, above the "+". None for the tray, which
/// opens inside the box instead (`tray`).
pub fn menu(
    t: &Theme,
    state: &Entity<AppState>,
    cx: &App,
    hooks: Hooks,
    leaving: bool,
) -> Option<AnyElement> {
    let browser = state.read(cx).browser;
    let card = match t.add_menu {
        AddMenu::Tray => return None,
        AddMenu::List => list(t, state, browser, &hooks),
        AddMenu::Grouped => grouped(t, state, browser, &hooks),
        AddMenu::Split => split(t, state, browser, &hooks),
        AddMenu::Tiles => tiles(t, state, browser, &hooks),
        AddMenu::Search => search(t, state, browser, &hooks, cx),
    };
    let out = hooks.on_out.clone();
    let card = card
        .on_mouse_down_out(move |ev, _, cx| out(ev.position, cx))
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow(t.elevation_popover())
        .occlude()
        .text_size(t.text.body_sm)
        .font_weight(FontWeight::NORMAL);
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(
                gpui::deferred(
                    gpui::anchored()
                        .anchor(gpui::Anchor::BottomLeft)
                        .snap_to_window_with_margin(t.size.viewport_margin)
                        .child(div().pb(px(8.)).child(super::motion::composer_menu(
                            card.relative(),
                            "add-menu",
                            0.,
                            leaving,
                            t,
                        ))),
                )
                .with_priority(2),
            )
            .into_any_element(),
    )
}

/// A clickable row's frame, with its hover.
fn frame(t: &Theme, id: (&'static str, usize)) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex()
        .items_center()
        .rounded(t.radius.button.min(px(10.)))
        .cursor_pointer()
        .hover(move |d| d.bg(hover))
}

fn on_run(
    row: gpui::Stateful<gpui::Div>,
    i: usize,
    hooks: &Hooks,
    state: &Entity<AppState>,
) -> gpui::Stateful<gpui::Div> {
    let (hooks, state) = (hooks.clone(), state.clone());
    row.on_click(move |_, window, cx| run(i, &hooks, &state, window, cx))
}

/// The plain menu: icon, label, and its end.
fn list(t: &Theme, state: &Entity<AppState>, browser: bool, hooks: &Hooks) -> gpui::Stateful<gpui::Div> {
    let mut card = div().id("add-menu").w(px(260.)).py(px(4.)).flex().flex_col();
    for (i, item) in ITEMS.iter().enumerate() {
        if i == 1 {
            card = card.child(div().my(px(4.)).h(px(1.)).bg(t.colors.border));
        }
        let row = frame(t, ("add-row", i))
            .mx(px(4.))
            .px(px(8.))
            .h(t.size.row)
            .gap(px(10.))
            .child(icon(item.glyph, t.size.icon_sm, t.colors.text_muted))
            .child(div().flex_1().min_w_0().text_color(t.colors.text_strong).child(item.label))
            .child(trailing(t, i, browser, state));
        card = card.child(on_run(row, i, hooks, state));
    }
    card
}

/// Two groups, each row an icon tile over a line on what it does.
fn grouped(t: &Theme, state: &Entity<AppState>, browser: bool, hooks: &Hooks) -> gpui::Stateful<gpui::Div> {
    let label = |text: &'static str| {
        div()
            .px(px(10.))
            .pt(px(8.))
            .pb(px(4.))
            .text_size(t.text.caption)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.colors.text_muted)
            .child(text)
    };
    let mut card = div()
        .id("add-menu")
        .w(px(340.))
        .p(px(6.))
        .flex()
        .flex_col()
        .child(label("Add to this message"));
    for (i, item) in ITEMS.iter().enumerate() {
        if i == 1 {
            card = card
                .child(div().mx(px(8.)).my(px(6.)).h(px(1.)).bg(t.colors.border))
                .child(label("Tools for this chat"));
        }
        let first = i == 0;
        let row = frame(t, ("add-row", i))
            .gap(px(12.))
            .px(px(10.))
            .py(px(7.))
            .child(
                div()
                    .size(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius.button.min(px(8.)))
                    .bg(if first { t.extra.accent_soft } else { t.extra.sunk })
                    .child(icon(
                        item.glyph,
                        t.size.icon_sm,
                        if first { t.extra.accent } else { t.colors.text_muted },
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.text_strong)
                            .child(item.label),
                    )
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(item.detail),
                    ),
            )
            .child(trailing(t, i, browser, state));
        card = card.child(on_run(row, i, hooks, state));
    }
    card
}

/// A list beside the chosen row's page: its name, its line, and the one
/// thing it does. Pointing at a row chooses it; a click runs it.
fn split(t: &Theme, state: &Entity<AppState>, browser: bool, hooks: &Hooks) -> gpui::Stateful<gpui::Div> {
    let at = hooks.split_at.min(ITEMS.len() - 1);
    let mut left = div()
        .w(px(196.))
        .flex_none()
        .p(px(6.))
        .flex()
        .flex_col()
        .gap(px(1.))
        .border_r_1()
        .border_color(t.colors.border);
    for (i, item) in ITEMS.iter().enumerate() {
        let on = i == at;
        let pick = hooks.on_split.clone();
        let row = frame(t, ("add-row", i))
            .h(t.size.row)
            .px(px(10.))
            .gap(px(10.))
            .when(on, |d| d.bg(t.extra.accent_soft))
            .on_hover(move |hovered, _, cx| {
                if *hovered {
                    pick(i, cx)
                }
            })
            .child(icon(
                item.glyph,
                t.size.icon_sm,
                if on { t.extra.accent } else { t.colors.text_muted },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(t.colors.text_strong)
                    .when(on, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .child(match item.key {
                        "files" => "Files or photos",
                        _ => item.label,
                    }),
            )
            .when(on, |d| {
                d.child(icon("icons/chevron-right.svg", t.size.icon_sm, t.extra.accent))
            });
        left = left.child(on_run(row, i, hooks, state));
    }
    let item = &ITEMS[at];
    let action_label: SharedString = match item.does {
        Does::Browser if browser => "Turn it off".into(),
        Does::Browser => "Turn it on".into(),
        Does::Act(_) if item.key == "files" => "Choose files…".into(),
        Does::Act(_) => format!("Open {}", item.label).into(),
    };
    let go = on_run(
        div()
            .id("add-split-go")
            .h(t.size.button_sm)
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(t.radius.button)
            .cursor_pointer()
            .bg(t.extra.accent)
            .text_color(t.extra.accent_fg)
            .font_weight(FontWeight::SEMIBOLD)
            .child(action_label),
        at,
        hooks,
        state,
    );
    let page = div()
        .flex_1()
        .min_w_0()
        .p(px(14.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(icon(item.glyph, t.size.icon, t.extra.accent))
                .child(
                    div()
                        .font_family(t.font_display.clone())
                        .font_weight(t.display_weight)
                        .text_size(t.text.body)
                        .text_color(t.colors.text_strong)
                        .child(item.label),
                ),
        )
        .child(div().text_color(t.colors.text_muted).child(item.detail))
        .child(div().flex_1().min_h(px(16.)))
        .child(div().flex().child(go));
    div()
        .id("add-menu")
        .w(px(520.))
        .h(px(6. * 30. + 24.))
        .flex()
        .overflow_hidden()
        .child(left)
        .child(page)
}

/// A grid of large tiles; the browser's tile says whether it is on.
fn tiles(t: &Theme, state: &Entity<AppState>, browser: bool, hooks: &Hooks) -> gpui::Stateful<gpui::Div> {
    let mut grid = div()
        .grid()
        .grid_cols(3)
        .gap(px(8.));
    for (i, item) in ITEMS.iter().enumerate() {
        let first = i == 0;
        let label: SharedString = match (item.key, item.does) {
            ("files", _) => "Files".into(),
            (_, Does::Browser) if browser => "Browser · on".into(),
            (_, Does::Browser) => "Browser · off".into(),
            _ => item.label.into(),
        };
        let border = t.colors.border;
        let tile = frame(t, ("add-row", i))
            .relative()
            .h(px(84.))
            .flex_col()
            .justify_center()
            .gap(px(8.))
            .rounded(t.radius.card.min(px(14.)))
            .map(|d| {
                if first {
                    d.bg(t.extra.accent_soft)
                } else {
                    d.bg(t.extra.sunk).border_1().border_color(border)
                }
            })
            .when(matches!(item.does, Does::Browser) && browser, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(8.))
                        .right(px(8.))
                        .size(px(8.))
                        .rounded_full()
                        .bg(t.extra.success),
                )
            })
            .child(icon(item.glyph, t.size.icon_lg, t.extra.accent))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.colors.text_strong)
                    .child(label),
            );
        grid = grid.child(on_run(tile, i, hooks, state));
    }
    div()
        .id("add-menu")
        .w(px(360.))
        .p(px(10.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(grid)
        .child(
            div()
                .px(px(4.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(icon("icons/image.svg", t.size.icon_xs, t.colors.text_muted))
                .child("Drop files or paste an image into the box"),
        )
}

/// A list you type into: the matching rows, the first marked; Enter runs it.
fn search(
    t: &Theme,
    state: &Entity<AppState>,
    browser: bool,
    hooks: &Hooks,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let query = hooks.filter.read(cx).value().to_string();
    let found = matches(&query);
    let mut rows = div().p(px(6.)).flex().flex_col();
    for (n, &i) in found.iter().enumerate() {
        let item = &ITEMS[i];
        let first = n == 0;
        let row = frame(t, ("add-row", i))
            .h(t.size.row)
            .px(px(10.))
            .gap(px(10.))
            .when(first, |d| d.bg(t.extra.accent_soft))
            .child(icon(
                item.glyph,
                t.size.icon_sm,
                if first { t.extra.accent } else { t.colors.text_muted },
            ))
            .child(div().flex_1().min_w_0().text_color(t.colors.text_strong).child(item.label))
            .map(|d| {
                if first {
                    d.child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.extra.accent)
                            .child("Enter"),
                    )
                } else {
                    d.child(trailing(t, i, browser, state))
                }
            });
        rows = rows.child(on_run(row, i, hooks, state));
    }
    if found.is_empty() {
        rows = rows.child(
            div()
                .px(px(10.))
                .py(px(8.))
                .text_color(t.colors.text_muted)
                .child("Nothing matches"),
        );
    }
    let close = hooks.close.clone();
    div()
        .id("add-menu")
        // Esc closes it: the field turns Esc into its own action, so take
        // that on the way down.
        .capture_action(move |_: &gpui::component::input::Escape, _, cx| {
            cx.stop_propagation();
            close(cx);
        })
        .w(px(340.))
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .h(px(42.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_b_1()
                .border_color(t.colors.border)
                .child(icon("icons/search.svg", t.size.icon_sm, t.colors.text_muted))
                .child(div().flex_1().child(Input::new(&hooks.filter).appearance(false))),
        )
        .child(rows)
        .child(
            div()
                .flex()
                .gap(px(14.))
                .px(px(12.))
                .py(px(6.))
                .border_t_1()
                .border_color(t.colors.border)
                .bg(t.extra.sunk)
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child("Enter opens the first")
                .child("Esc or + closes"),
        )
}

/// The tray: a row of chips inside the box, above what you write. Nothing
/// floats over the chat; the box grows by a row while it is open.
pub fn tray(t: &Theme, state: &Entity<AppState>, cx: &App, hooks: &Hooks) -> AnyElement {
    let browser = state.read(cx).browser;
    let mut row = div()
        .flex()
        .flex_wrap()
        .gap(px(6.))
        .px(px(10.))
        .pt(px(10.))
        .pb(px(8.))
        .border_b_1()
        .border_color(t.colors.border)
        .bg(t.extra.sunk)
        .rounded_t(t.size.composer_radius);
    for (i, item) in ITEMS.iter().enumerate() {
        let on = matches!(item.does, Does::Browser) && browser;
        let label: SharedString = match (item.key, item.does) {
            ("files", _) => format!("Files or photos  {}", super::shortcut("U")).into(),
            (_, Does::Browser) if browser => "Web browser · on".into(),
            _ => item.label.into(),
        };
        let hover = t.colors.hover;
        let chip = div()
            .id(("add-chip", i))
            .h(px(30.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(t.radius.compact)
            .border_1()
            .cursor_pointer()
            .map(|d| {
                if on {
                    d.border_color(t.extra.accent)
                        .bg(t.extra.accent_soft)
                        .font_weight(FontWeight::SEMIBOLD)
                } else {
                    d.border_color(t.colors.border)
                        .bg(t.colors.surface)
                        .hover(move |st| st.bg(hover))
                }
            })
            .child(icon(item.glyph, t.size.icon_sm, t.extra.accent))
            .child(div().text_color(t.colors.text_strong).child(label));
        row = row.child(on_run(chip, i, hooks, state));
    }
    row.into_any_element()
}
