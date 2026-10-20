//! The composer's "+" menu, as Claude Desktop has one: what you can add to
//! a message and the tools a reply may use. Only rows Forge can serve:
//! files and photos, the web browser (a switch, so the menu stays open),
//! and ForgeCLI's extensions: connectors (MCP servers), skills, plugins
//! (`pluginDirs`) and hooks. Claude Desktop's screenshot,
//! research, style and project rows have no engine here, so they're left
//! out rather than shown dead.

use gpui::{div, prelude::*, px, Action, App, Entity, FontWeight, IntoElement, SharedString};

use super::icon;
use crate::state::AppState;
use crate::theme::Theme;

/// One row: an icon, a label, and what sits at its end.
fn row(t: &Theme, id: &'static str, glyph: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .mx(px(4.))
        .px(px(8.))
        .h(t.size.row)
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(4.))
        .cursor_pointer()
        .hover(move |d| d.bg(hover))
        .child(icon(glyph, t.size.icon_sm, t.colors.text_muted))
        .child(div().flex_1().min_w_0().text_color(t.colors.text_strong).child(label))
}

/// A trailing hint: a shortcut, or a chevron for a row that opens a page.
fn trailing(t: &Theme, text: Option<SharedString>) -> gpui::AnyElement {
    match text {
        Some(text) => div()
            .text_size(t.text.caption)
            .text_color(t.colors.text_muted)
            .child(text)
            .into_any_element(),
        None => icon("icons/chevron-right.svg", t.size.icon_sm, t.colors.text_muted).into_any_element(),
    }
}

/// The menu, above the "+" (its bottom-left corner at the button's top-left).
/// `close` shuts it; a press outside calls `on_out`.
pub fn menu(
    t: &Theme,
    state: &Entity<AppState>,
    cx: &App,
    close: impl Fn(&mut App) + Clone + 'static,
    on_out: impl Fn(gpui::Point<gpui::Pixels>, &mut App) + 'static,
) -> impl IntoElement {
    let browser = state.read(cx).browser;
    let pick = |id: &'static str, glyph, label, hint: Option<SharedString>, action: fn() -> Box<dyn Action>| {
        let close = close.clone();
        row(t, id, glyph, label)
            .child(trailing(t, hint))
            .on_click(move |_, window, cx| {
                close(cx);
                window.dispatch_action(action(), cx);
            })
    };
    let st = state.clone();
    let browser_row = row(t, "add-browser", "icons/globe.svg", "Web browser")
        .child(
            super::button::Switch::new("add-browser-switch", browser, "Web browser").on_toggle({
                let st = st.clone();
                move |_, _, cx| st.update(cx, |s, cx| s.toggle_browser(cx))
            }),
        )
        .on_click(move |_, _, cx| st.update(cx, |s, cx| s.toggle_browser(cx)));
    let divider = div().my(px(4.)).h(px(1.)).bg(t.colors.border);
    let card = div()
        .id("add-menu")
        .on_mouse_down_out(move |ev, _, cx| on_out(ev.position, cx))
        .w(px(260.))
        .py(px(4.))
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow_lg()
        .occlude()
        .text_size(t.text.body_sm)
        .font_weight(FontWeight::NORMAL)
        .child(pick(
            "add-files",
            "icons/paperclip.svg",
            "Add files or photos",
            Some(super::shortcut("U")),
            || Box::new(super::AttachFiles),
        ))
        .child(divider)
        .child(browser_row)
        .child(pick("add-connectors", "icons/plug.svg", "Connectors", None, || {
            Box::new(super::OpenMcpServers)
        }))
        .child(pick("add-skills", "icons/sparkles.svg", "Skills", None, || {
            Box::new(super::OpenSkills)
        }))
        .child(pick("add-plugins", "icons/puzzle.svg", "Plugins", None, || {
            Box::new(super::OpenPlugins)
        }))
        .child(pick("add-hooks", "icons/webhook.svg", "Hooks", None, || {
            Box::new(super::OpenHooks)
        }));
    div().absolute().top_0().left_0().child(
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::BottomLeft)
                .snap_to_window_with_margin(t.size.viewport_margin)
                .child(div().pb(px(8.)).child(card)),
        )
        .with_priority(2),
    )
}
