//! A chat's menu, as Claude Code draws its session menu: plain rows, each
//! with its letter key while the menu is open, in three groups, and flyouts
//! for the rows marked ›. Opened from the title bar's chat name, a sidebar
//! row's ⋯ or a right-click.
//!
//! - Open in ›, Rename R, Copy as Markdown C, Continue in a new chat
//! - Transcript view ›, Pin P, Move to project ›, Duplicate, Export ›,
//!   Edit remote connection (a remote chat)
//! - Archive A, Delete… D, in red
//!
//! The letters are read by the workspace (`letter`), so they work wherever
//! the focus is while the menu shows.

use gpui::{div, prelude::*, px, AnimationExt, AnyElement, Entity, SharedString};

use super::icon;
use crate::state::{AppState, ChatSub};
use crate::theme::{self, Theme};

/// What the menu needs to know about its chat.
pub struct MenuChat {
    pub id: usize,
    pub pinned: bool,
    pub archived: bool,
    pub detailed: bool,
    /// It works on another machine (WSL, SSH, a container).
    pub remote: bool,
}

/// The action a letter runs while the menu is open, if it names one.
pub fn letter(key: &str) -> Option<fn(&mut AppState, usize, &mut gpui::Context<AppState>)> {
    Some(match key {
        "r" => |s, id, cx| s.start_rename(id, cx),
        "c" => |s, id, cx| s.copy_chat_markdown(id, cx),
        "p" => |s, id, cx| {
            s.chat_menu = None;
            s.toggle_pin(id, cx)
        },
        "a" => |s, id, cx| s.toggle_archive(id, cx),
        "d" => |s, id, cx| s.ask_delete(id, cx),
        _ => return None,
    })
}

pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    c: MenuChat,
    sub: Option<ChatSub>,
    targets: Vec<(usize, SharedString)>,
    window: &gpui::Window,
) -> AnyElement {
    let id = c.id;
    let (hover, danger_hover) = (t.colors.hover, t.extra.danger_tint);
    // One row: its words, then its letter or its ›.
    let row =
        |key: &str, label: SharedString, letter: Option<&'static str>, more: bool, danger: bool| {
            div()
                .id(SharedString::from(format!("chat-menu-{key}")))
                .relative()
                .flex()
                .items_center()
                .gap(t.space.md)
                .h(t.size.menu_row)
                .px(t.space.snug)
                .rounded(t.radius.button)
                .cursor_pointer()
                .text_size(t.text.body_sm)
                .text_color(if danger {
                    t.extra.danger
                } else {
                    t.colors.text
                })
                .hover(move |d| d.bg(if danger { danger_hover } else { hover }))
                .child(div().flex_1().whitespace_nowrap().child(label))
                .children(letter.map(|l| {
                    div()
                        .flex_none()
                        .text_size(t.text.caption)
                        .text_color(if danger {
                            t.extra.danger
                        } else {
                            t.colors.text_subtle
                        })
                        .child(l.to_uppercase())
                }))
                .when(more, |d| {
                    d.child(icon(
                        "icons/chevron-right.svg",
                        t.size.icon_xs,
                        t.colors.text_subtle,
                    ))
                })
        };
    let act = |f: fn(&mut AppState, usize, &mut gpui::Context<AppState>)| {
        let state = state.clone();
        move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
            cx.stop_propagation();
            state.update(cx, |s, cx| f(s, id, cx));
        }
    };
    // A row with a flyout: hovering (or clicking) it opens its list beside it.
    let opener = |key: &str, label: &str, which: ChatSub| {
        let (on_hover, on_click) = (state.clone(), state.clone());
        row(key, label.to_string().into(), None, true, false)
            .when(sub == Some(which), |d| d.bg(hover))
            .on_hover(move |hovered, _, cx| {
                if *hovered {
                    on_hover.update(cx, |s, cx| s.show_chat_sub(Some(which), cx));
                }
            })
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                on_click.update(cx, |s, cx| s.show_chat_sub(Some(which), cx));
            })
    };
    // Any other row closes an open flyout when hovered.
    let plain = |el: gpui::Stateful<gpui::Div>| {
        let st = state.clone();
        el.on_hover(move |hovered, _, cx| {
            if *hovered {
                st.update(cx, |s, cx| s.show_chat_sub(None, cx));
            }
        })
    };
    // A flyout sits beside the menu, outside its clip (the menu scrolls
    // inside the window), level with its row: `rows` rows and `rules`
    // separators down.
    let flyout = |rows: f32, rules: f32, items: Vec<AnyElement>| {
        let rule_h = px(1.) + t.space.xs * 2.;
        div()
            .absolute()
            .left(t.size.menu_width + t.space.xs)
            .top(t.size.menu_row * rows + rule_h * rules)
            .child(
                theme::popover(t)
                    .occlude()
                    .min_w(t.size.menu_width)
                    .p(t.space.xs)
                    .flex()
                    .flex_col()
                    .children(items),
            )
    };
    let check = |on: bool| {
        div().w(t.size.icon_sm).flex_none().when(on, |d| {
            d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.text))
        })
    };
    let item = |key: String, label: SharedString| {
        div()
            .id(SharedString::from(format!("chat-sub-{key}")))
            .flex()
            .items_center()
            .gap(t.space.sm)
            .h(t.size.menu_row)
            .px(t.space.snug)
            .rounded(t.radius.button)
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(t.text.body_sm)
            .text_color(t.colors.text)
            .hover(move |d| d.bg(hover))
            .child(label)
    };

    let open_in = {
        let mut items = vec![];
        if !c.remote {
            items.push(
                item("folder".into(), "File manager".into())
                    .on_click(act(|s, id, cx| s.open_chat_folder(id, cx)))
                    .into_any_element(),
            );
        }
        let st = state.clone();
        items.push(
            item("terminal".into(), "Terminal".into())
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    let closed = !st.read(cx).terminal_open;
                    st.update(cx, |s, cx| s.open_chat_menu(None, cx));
                    if closed {
                        window.dispatch_action(Box::new(crate::ToggleTerminal), cx);
                    }
                })
                .into_any_element(),
        );
        if !c.remote {
            items.push(
                item("vscode".into(), "VS Code".into())
                    .on_click(act(|s, id, cx| s.open_chat_in_vscode(id, cx)))
                    .into_any_element(),
            );
        }
        items
    };
    let view = [("Normal", false), ("Detailed", true)]
        .into_iter()
        .map(|(label, detailed)| {
            let st = state.clone();
            item(label.to_lowercase(), "".into())
                .child(check(c.detailed == detailed))
                .child(label)
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    st.update(cx, |s, cx| s.set_detailed(id, detailed, cx));
                })
                .into_any_element()
        })
        .collect::<Vec<_>>();
    let moves = if targets.is_empty() {
        vec![div()
            .px(t.space.snug)
            .py(t.space.xs)
            .text_size(t.text.caption)
            .text_color(t.colors.text_subtle)
            .child("No other project yet.")
            .into_any_element()]
    } else {
        targets
            .into_iter()
            .map(|(to, name)| {
                let st = state.clone();
                item(format!("move-{to}"), name)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        st.update(cx, |s, cx| s.move_chat(id, to, cx));
                    })
                    .into_any_element()
            })
            .collect()
    };
    let export = vec![
        item("md".into(), "Markdown file".into())
            .on_click(act(|s, id, cx| s.export_chat(id, cx)))
            .into_any_element(),
        item("json".into(), "JSON file".into())
            .on_click(act(|s, id, cx| s.export_chat_json(id, cx)))
            .into_any_element(),
        item("copy-json".into(), "Copy as JSON".into())
            .on_click(act(|s, id, cx| s.share_chat(id, cx)))
            .into_any_element(),
    ];
    let rule = || theme::rule(t).my(t.space.xs).mx(t.space.xs);

    // Anchored and kept inside the window: at most the window's height.
    let menu = theme::fit_window(
        theme::popover(t).id(("chat-menu", id)),
        window,
        t,
        t.size.viewport_margin,
        None,
    )
    .occlude()
    .w(t.size.menu_width)
    .p(t.space.xs)
    .flex()
    .flex_col()
    .child(opener("open-in", "Open in", ChatSub::OpenIn))
    .child(
        plain(row("rename", "Rename".into(), Some("r"), false, false))
            .on_click(act(|s, id, cx| s.start_rename(id, cx))),
    )
    .child(
        plain(row(
            "copy",
            "Copy as Markdown".into(),
            Some("c"),
            false,
            false,
        ))
        .on_click(act(|s, id, cx| s.copy_chat_markdown(id, cx))),
    )
    .child(
        plain(row(
            "continue",
            "Continue in a new chat".into(),
            None,
            false,
            false,
        ))
        .on_click(act(|s, id, cx| s.continue_in_new_chat(id, cx))),
    )
    .child(rule())
    .child(opener("view", "Transcript view", ChatSub::View))
    .child(
        plain(row(
            "pin",
            if c.pinned { "Unpin" } else { "Pin" }.into(),
            Some("p"),
            false,
            false,
        ))
        .on_click(act(|s, id, cx| {
            s.chat_menu = None;
            s.toggle_pin(id, cx);
        })),
    )
    .child(opener("move", "Move to project", ChatSub::Move))
    .child(
        plain(row("duplicate", "Duplicate".into(), None, false, false))
            .on_click(act(|s, id, cx| s.duplicate_chat(id, cx))),
    )
    .child(opener("export", "Export", ChatSub::Export))
    .when(c.remote, |d| {
        d.child(
            plain(row(
                "remote",
                "Edit remote connection".into(),
                None,
                false,
                false,
            ))
            .on_click(act(|s, _, cx| {
                s.open_chat_menu(None, cx);
                s.toggle_remote_menu(cx);
            })),
        )
    })
    .child(rule())
    .child(
        plain(row(
            "archive",
            if c.archived { "Unarchive" } else { "Archive" }.into(),
            Some("a"),
            false,
            false,
        ))
        .on_click(act(|s, id, cx| s.toggle_archive(id, cx))),
    )
    .child(
        plain(row("delete", "Delete…".into(), Some("d"), false, true))
            .on_click(act(|s, id, cx| s.ask_delete(id, cx))),
    );
    let open = match sub {
        Some(ChatSub::OpenIn) => Some(flyout(0., 0., open_in)),
        Some(ChatSub::View) => Some(flyout(4., 1., view)),
        Some(ChatSub::Move) => Some(flyout(6., 1., moves)),
        Some(ChatSub::Export) => Some(flyout(8., 1., export)),
        None => None,
    };
    div()
        .relative()
        .child(menu)
        .children(open)
        .with_animation(
            ("chat-menu-in", id),
            gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms))
                .with_easing(gpui::ease_in_out),
            |d, delta| d.opacity(delta),
        )
        .into_any_element()
}
