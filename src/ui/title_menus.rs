//! The title bar's two menus, drawn as the chat menu is (plain rows, the
//! key on the right): Share (copy or export the chat) and ⋮ (the terminal,
//! the theme, the shortcuts, settings).

use gpui::{div, prelude::*, AnimationExt, AnyElement, Entity, SharedString};

use crate::state::{AppState, TitleMenu};
use crate::theme::{self, Theme};

pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    which: TitleMenu,
    chat: Option<usize>,
    window: &gpui::Window,
) -> AnyElement {
    let hover = t.colors.hover;
    let row = |key: &str, label: SharedString, hint: Option<SharedString>| {
        div()
            .id(SharedString::from(format!("title-menu-{key}")))
            .flex()
            .items_center()
            .gap(t.space.md)
            .h(t.size.menu_row)
            .px(t.space.snug)
            .rounded(t.radius.small)
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(t.text.body_sm)
            .text_color(t.colors.text)
            .hover(move |d| d.bg(hover))
            .child(div().flex_1().child(label))
            .children(hint.map(|h| {
                div()
                    .flex_none()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(h)
            }))
    };
    // A row that runs an app action (the same as its key).
    let action = |key: &str, label: SharedString, hint: SharedString, a: fn() -> Box<dyn gpui::Action>| {
        let st = state.clone();
        row(key, label, Some(hint)).on_click(move |_, window, cx| {
            cx.stop_propagation();
            st.update(cx, |s, cx| {
                s.title_menu = None;
                cx.notify();
            });
            window.dispatch_action(a(), cx);
        })
    };
    // A row that acts on the open chat.
    let on_chat = |key: &str, label: &str, f: fn(&mut AppState, usize, &mut gpui::Context<AppState>)| {
        let st = state.clone();
        row(key, label.to_string().into(), None).on_click(move |_, _, cx| {
            cx.stop_propagation();
            st.update(cx, |s, cx| {
                s.title_menu = None;
                if let Some(id) = chat {
                    f(s, id, cx);
                }
                cx.notify();
            });
        })
    };
    let rows: Vec<AnyElement> = match which {
        TitleMenu::Share => vec![
            on_chat("copy-md", "Copy as Markdown", |s, id, cx| s.copy_chat_markdown(id, cx)).into_any_element(),
            on_chat("copy-json", "Copy as JSON", |s, id, cx| s.share_chat(id, cx)).into_any_element(),
            theme::rule(t).my(t.space.xs).mx(t.space.xs).into_any_element(),
            on_chat("export-md", "Export as Markdown", |s, id, cx| s.export_chat(id, cx)).into_any_element(),
            on_chat("export-json", "Export as JSON", |s, id, cx| s.export_chat_json(id, cx)).into_any_element(),
        ],
        TitleMenu::More => vec![
            action("terminal", "Terminal".into(), super::shortcut("`"), || Box::new(crate::ToggleTerminal)).into_any_element(),
            action(
                "theme",
                if t.is_dark { "Light theme" } else { "Dark theme" }.into(),
                super::shortcut("Shift L"),
                || Box::new(crate::ToggleTheme),
            )
            .into_any_element(),
            action("shortcuts", "Keyboard shortcuts".into(), "F1".into(), || Box::new(crate::ShowShortcuts)).into_any_element(),
            theme::rule(t).my(t.space.xs).mx(t.space.xs).into_any_element(),
            action("settings", "Settings".into(), super::shortcut(","), || Box::new(crate::OpenSettings)).into_any_element(),
        ],
    };
    theme::fit_window(theme::popover(t).id("title-menu"), window, t, t.size.viewport_margin, None)
        .occlude()
        .w(t.size.menu_width)
        .p(t.space.xs)
        .flex()
        .flex_col()
        .children(rows)
        .with_animation(
            "title-menu-in",
            gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms)).with_easing(gpui::ease_in_out),
            |d, delta| d.opacity(delta),
        )
        .into_any_element()
}
