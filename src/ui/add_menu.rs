//! The composer's "+" menu, as Claude Desktop has one: what you can add to
//! a message and the tools a reply may use. Only rows KodAct can serve:
//! files and photos, the web browser (a switch, so the menu stays open),
//! and KodAct CLI's extensions: connectors (MCP servers), skills, plugins
//! (`pluginDirs`) and hooks. Claude Desktop's screenshot,
//! research, style and project rows have no engine here, so they're left
//! out rather than shown dead.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, Action, AnyElement, App, Entity, FontWeight, IntoElement, Pixels, Point,
    Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::Theme;

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
    does: Does,
}

const ITEMS: [Item; 6] = [
    Item {
        key: "files",
        glyph: "icons/paperclip.svg",
        label: "Add files or photos",
        does: Does::Act(|| Box::new(super::AttachFiles)),
    },
    Item {
        key: "browser",
        glyph: "icons/globe.svg",
        label: "Web browser",
        does: Does::Browser,
    },
    Item {
        key: "connectors",
        glyph: "icons/plug.svg",
        label: "Connectors",
        does: Does::Act(|| Box::new(super::OpenMcpServers)),
    },
    Item {
        key: "skills",
        glyph: "icons/scroll-text.svg",
        label: "Skills",
        does: Does::Act(|| Box::new(super::OpenSkills)),
    },
    Item {
        key: "plugins",
        glyph: "icons/puzzle.svg",
        label: "Plugins",
        does: Does::Act(|| Box::new(super::OpenPlugins)),
    },
    Item {
        key: "hooks",
        glyph: "icons/webhook.svg",
        label: "Hooks",
        does: Does::Act(|| Box::new(super::OpenHooks)),
    },
];

/// What the composer gives the menu: how to close it, and what a press
/// outside does.
#[derive(Clone)]
pub struct Hooks {
    pub close: Rc<dyn Fn(&mut App)>,
    pub on_out: Rc<dyn Fn(Point<Pixels>, &mut App)>,
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
        _ => icon(
            "icons/chevron-right.svg",
            t.size.icon_sm,
            t.colors.text_muted,
        )
        .into_any_element(),
    }
}

/// The menu, above the "+".
pub fn menu(
    t: &Theme,
    state: &Entity<AppState>,
    cx: &App,
    hooks: Hooks,
    leaving: bool,
) -> Option<AnyElement> {
    let browser = state.read(cx).browser;
    let card = list(t, state, browser, &hooks);
    let out = hooks.on_out.clone();
    let card = card
        .when(!leaving, |d| {
            d.on_mouse_down_out(move |ev, _, cx| out(ev.position, cx))
        })
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
fn list(
    t: &Theme,
    state: &Entity<AppState>,
    browser: bool,
    hooks: &Hooks,
) -> gpui::Stateful<gpui::Div> {
    let mut card = div()
        .id("add-menu")
        .w(px(260.))
        .py(px(4.))
        .flex()
        .flex_col();
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
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(t.colors.text_strong)
                    .child(item.label),
            )
            .child(trailing(t, i, browser, state));
        // One by one as the menu opens.
        card = card.child(super::motion::row_in_full(
            on_run(row, i, hooks, state),
            ("add-in", i),
            i,
            t,
        ));
    }
    card
}
