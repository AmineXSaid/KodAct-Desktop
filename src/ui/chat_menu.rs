//! A chat's menu, from its sidebar row's ⋯ button or a right-click: Rename,
//! Pin, Duplicate, Export as Markdown or JSON, Share (the JSON to the
//! clipboard; links are `stubs::SHARE_LINK`), then Delete (asks first).

use gpui::{div, prelude::*, px, AnimationExt, AnyElement, Entity, SharedString};

use super::{icon, kbd};
use crate::state::AppState;
use crate::theme::{self, Theme};

pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    id: usize,
    pinned: bool,
    window: &gpui::Window,
) -> AnyElement {
    let (hover, danger_hover) = (t.colors.hover, t.extra.danger_tint);
    let row = |key: &'static str,
               path: &'static str,
               label: &'static str,
               hint: Option<SharedString>,
               danger: bool| {
        let fg = if danger {
            t.extra.danger
        } else {
            t.colors.text
        };
        div()
            .id(SharedString::from(format!("chat-menu-{key}")))
            .flex()
            .items_center()
            .gap(t.space.sm)
            .h(t.size.row)
            .px(t.space.md)
            .rounded(t.radius.button)
            .cursor_pointer()
            .text_color(fg)
            .hover(move |d| d.bg(if danger { danger_hover } else { hover }))
            .child(icon(
                path,
                t.size.icon_sm,
                if danger {
                    t.extra.danger
                } else {
                    t.colors.text_muted
                },
            ))
            .child(div().flex_1().child(label))
            .children(hint.map(|h| kbd(t, h)))
    };
    let act = |f: fn(&mut AppState, usize, &mut gpui::Context<AppState>)| {
        let state = state.clone();
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
            cx.stop_propagation();
            state.update(cx, |s, cx| f(s, id, cx));
        }
    };
    // Anchored and kept inside the window: at most the window's height.
    theme::fit_window(
        theme::popover(t).id(("chat-menu", id)),
        window,
        t,
        t.size.viewport_margin,
        None,
    )
    .occlude()
    .w(px(220.))
    .flex()
    .flex_col()
    .child(
        row(
            "rename",
            "icons/pencil.svg",
            "Rename",
            Some("F2".into()),
            false,
        )
        .on_click(act(|s, id, cx| s.start_rename(id, cx))),
    )
    .child(
        row(
            "pin",
            "icons/pin.svg",
            if pinned { "Unpin" } else { "Pin" },
            None,
            false,
        )
        .on_click(act(|s, id, cx| {
            s.chat_menu = None;
            s.toggle_pin(id, cx);
        })),
    )
    .child(
        row("duplicate", "icons/copy-plus.svg", "Duplicate", None, false)
            .on_click(act(|s, id, cx| s.duplicate_chat(id, cx))),
    )
    .child(
        row(
            "export",
            "icons/archive.svg",
            "Export as Markdown",
            None,
            false,
        )
        .on_click(act(|s, id, cx| s.export_chat(id, cx))),
    )
    .child(
        row(
            "export-json",
            "icons/archive.svg",
            "Export as JSON",
            None,
            false,
        )
        .on_click(act(|s, id, cx| s.export_chat_json(id, cx))),
    )
    .child(
        row(
            "share",
            "icons/copy.svg",
            "Share (copy as JSON)",
            None,
            false,
        )
        .on_click(act(|s, id, cx| s.share_chat(id, cx))),
    )
    .child(theme::rule(t).my(t.space.xs))
    .child(
        row("delete", "icons/trash.svg", "Delete…", None, true)
            .on_click(act(|s, id, cx| s.ask_delete(id, cx))),
    )
    .with_animation(
        ("chat-menu-in", id),
        gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms))
            .with_easing(gpui::ease_in_out),
        |d, delta| d.opacity(delta),
    )
    .into_any_element()
}
