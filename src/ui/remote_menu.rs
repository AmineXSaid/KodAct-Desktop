//! The remote indicator, bottom left as in VS Code: where this window's
//! project lives (this computer, a WSL distro, an SSH host, a container),
//! and its menu to work somewhere else: connect to WSL or a host, reopen the
//! folder in its dev container or attach to a running one, pick the remote
//! folder, trust a new host's key, or go back to this computer.

use std::time::Duration;

use gpui::component::input::{Input, InputState};
use gpui::{
    div, prelude::*, px, Animation, AnimationExt, AnyElement, Entity, FontWeight, SharedString,
    Transformation,
};

use super::icon;
use crate::remote::{Remote, Target};
use crate::state::{AppState, RemotePage};
use crate::theme::{self, Theme};

/// The chip at the foot of the sidebar.
pub fn indicator(s: &AppState, state: Entity<AppState>, t: &Theme) -> AnyElement {
    let remote = s.current_remote();
    let busy = s.remote_busy.clone();
    let (glyph, label): (&str, SharedString) = match &remote {
        Some(r) => (r.target.icon(), r.target.label().into()),
        None => ("icons/plug.svg", "Local".into()),
    };
    let connected = remote
        .as_ref()
        .is_some_and(|r| s.remote_connected(&r.target));
    let open = s.remote_menu.is_some();
    let hover = t.colors.hover;
    let status: AnyElement = if busy.is_some() {
        icon(
            "icons/loader-circle.svg",
            t.size.icon_xs,
            t.colors.text_muted,
        )
        .with_animation(
            "remote-busy",
            Animation::new(Duration::from_millis(900)).repeat(),
            |i, k| i.with_transformation(Transformation::rotate(gpui::percentage(k))),
        )
        .into_any_element()
    } else {
        let colour = match (&remote, connected) {
            (None, _) => None,
            (Some(_), true) => Some(t.extra.success),
            (Some(_), false) => Some(t.colors.text_subtle),
        };
        match colour {
            Some(c) => div()
                .size(px(7.))
                .rounded(t.radius.pill)
                .bg(c)
                .into_any_element(),
            None => div().into_any_element(),
        }
    };
    div()
        .id("remote-indicator")
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(t.space.md)
        .py(t.space.xs)
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |st| st.bg(hover))
        .when(open, |d| d.bg(hover))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .text_size(t.text.body_sm)
                .child(icon(glyph, t.size.icon_sm, t.colors.text_muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(t.colors.text)
                        .child(label),
                )
                .child(status),
        )
        .children(busy.map(|(_, msg)| {
            div()
                .pl(t.size.icon_sm + t.space.sm)
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(msg)
        }))
        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.toggle_remote_menu(cx)))
        .into_any_element()
}

/// A row of the menu: an icon, a title, an optional caption, a trailing mark.
fn row(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    title: impl Into<SharedString>,
    caption: Option<SharedString>,
    trailing: Option<&'static str>,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(t.space.sm)
        .min_h(t.size.row)
        .px(t.space.md)
        .py(px(4.))
        .mx(t.space.xs)
        .rounded(t.radius.small)
        .when(enabled, |d| {
            d.cursor_pointer().hover(move |st| st.bg(hover))
        })
        .child(icon(
            glyph,
            t.size.icon_sm,
            if enabled {
                t.colors.text_muted
            } else {
                t.colors.text_subtle
            },
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(if enabled {
                            t.colors.text
                        } else {
                            t.colors.text_muted
                        })
                        .child(title.into()),
                )
                .children(caption.map(|c| {
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(c)
                })),
        )
        .children(trailing.map(|g| icon(g, t.size.icon_xs, t.colors.text_subtle)))
}

fn heading(t: &Theme, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .px(t.space.md + t.space.xs)
        .pt(t.space.sm)
        .pb(t.space.xs)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .child(text.into())
}

fn note(t: &Theme, text: impl Into<SharedString>, error: bool) -> gpui::Div {
    div()
        .px(t.space.md + t.space.xs)
        .py(t.space.sm)
        .text_size(t.text.caption)
        .line_height(gpui::relative(1.4))
        .text_color(if error {
            t.extra.danger
        } else {
            t.colors.text_muted
        })
        .child(text.into())
}

fn back(t: &Theme, state: &Entity<AppState>, title: &'static str, to: RemotePage) -> AnyElement {
    let st = state.clone();
    row(
        t,
        "remote-back",
        "icons/arrow-left.svg",
        title,
        None,
        None,
        true,
    )
    .font_weight(FontWeight::MEDIUM)
    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.remote_page(to, cx)))
    .into_any_element()
}

fn divider(t: &Theme) -> gpui::Div {
    div().my(t.space.xs).h(t.size.hairline).bg(t.colors.border)
}

/// The menu, above the chip. `host` is the box for typing a host.
pub fn menu(
    s: &AppState,
    state: Entity<AppState>,
    host: &Entity<InputState>,
    t: &Theme,
) -> AnyElement {
    let Some(m) = &s.remote_menu else {
        return div().into_any_element();
    };
    let mut body = div().flex().flex_col().py(t.space.xs);

    // An SSH host seen for the first time: show its key, ask before trusting.
    if let Some((_, h, fp)) = &m.host_key {
        let (trust, cancel) = (state.clone(), state.clone());
        body = body.child(
            div()
                .m(t.space.sm)
                .p(t.space.md)
                .rounded(t.radius.button)
                .border_1()
                .border_color(t.colors.border)
                .bg(t.extra.warning_tint)
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child(format!("Trust {h}?")),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child("Forge hasn't connected to this host before. Trust it only if this key fingerprint matches the server's:"),
                )
                .child(
                    div()
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption)
                        .text_color(t.colors.text)
                        .child(fp.clone()),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(t.space.sm)
                        .child(
                            theme::ghost_button(t, "Cancel").id("remote-untrust").on_click(move |_, _, cx| {
                                cancel.update(cx, |s, cx| {
                                    if let Some(m) = &mut s.remote_menu {
                                        m.host_key = None;
                                    }
                                    cx.notify();
                                })
                            }),
                        )
                        .child(
                            theme::primary_button(t, "Trust and connect").id("remote-trust")
                                .on_click(move |_, _, cx| trust.update(cx, |s, cx| s.trust_host(cx))),
                        ),
                ),
        );
    }

    match m.page {
        RemotePage::Root => {
            let (wsl, ssh, att) = (state.clone(), state.clone(), state.clone());
            body = body
                .child(heading(t, "Work in"))
                .child(
                    row(
                        t,
                        "remote-wsl",
                        "icons/terminal.svg",
                        "Connect to WSL…",
                        None,
                        Some("icons/chevron-right.svg"),
                        true,
                    )
                    .on_click(move |_, _, cx| {
                        wsl.update(cx, |s, cx| s.remote_page(RemotePage::Wsl, cx))
                    }),
                )
                .child(
                    row(
                        t,
                        "remote-ssh",
                        "icons/server.svg",
                        "Connect to Host…",
                        Some("SSH, with your keys or agent".into()),
                        Some("icons/chevron-right.svg"),
                        true,
                    )
                    .on_click(move |_, _, cx| {
                        ssh.update(cx, |s, cx| s.remote_page(RemotePage::Ssh, cx))
                    }),
                );
            if s.has_devcontainer() {
                let st = state.clone();
                body = body.child(
                    row(
                        t,
                        "remote-reopen",
                        "icons/box.svg",
                        "Reopen in Container",
                        Some("This folder's .devcontainer".into()),
                        None,
                        true,
                    )
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.reopen_in_container(cx))),
                );
            }
            body = body.child(
                row(
                    t,
                    "remote-attach",
                    "icons/box.svg",
                    "Attach to Running Container…",
                    None,
                    Some("icons/chevron-right.svg"),
                    true,
                )
                .on_click(move |_, _, cx| {
                    att.update(cx, |s, cx| s.remote_page(RemotePage::Containers, cx))
                }),
            );
            let recent: Vec<(usize, Remote, SharedString)> = s
                .projects
                .iter()
                .filter_map(|p| Remote::parse(&p.path).map(|r| (p.id, r, p.name.clone())))
                .filter(|(id, ..)| *id != s.current_project)
                .take(5)
                .collect();
            if !recent.is_empty() {
                body = body.child(divider(t)).child(heading(t, "Recent"));
                for (id, r, name) in recent {
                    let st = state.clone();
                    body = body.child(
                        row(
                            t,
                            ("remote-recent", id),
                            r.target.icon(),
                            name,
                            Some(format!("{} · {}", r.target.label(), r.path).into()),
                            None,
                            true,
                        )
                        .on_click(move |_, _, cx| {
                            st.update(cx, |s, cx| {
                                s.remote_menu = None;
                                s.show_project(id, cx);
                            })
                        }),
                    );
                }
            }
            if s.current_remote().is_some() {
                let st = state.clone();
                body = body.child(divider(t)).child(
                    row(
                        t,
                        "remote-close",
                        "icons/x.svg",
                        "Close remote connection",
                        Some("Back to this computer".into()),
                        None,
                        true,
                    )
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.close_remote(cx))),
                );
            }
        }
        RemotePage::Wsl => {
            body = body.child(back(t, &state, "WSL distros", RemotePage::Root));
            match &m.distros {
                None => body = body.child(note(t, "Looking for distros…", false)),
                Some(Err(e)) => body = body.child(note(t, e.clone(), true)),
                Some(Ok(d)) if d.is_empty() => {
                    body = body.child(note(
                        t,
                        "No distros yet. Install one with `wsl --install -d Ubuntu`.",
                        false,
                    ))
                }
                Some(Ok(list)) => {
                    for d in list {
                        let st = state.clone();
                        let name = d.name.clone();
                        let caption = format!(
                            "{}{} · WSL {}",
                            if d.default { "Default · " } else { "" },
                            if d.running {
                                "Running"
                            } else {
                                "Stopped (starts when you connect)"
                            },
                            d.version
                        );
                        body = body.child(
                            row(
                                t,
                                SharedString::from(format!("wsl-{name}")),
                                "icons/terminal.svg",
                                name.clone(),
                                Some(caption.into()),
                                Some("icons/chevron-right.svg"),
                                true,
                            )
                            .on_click(move |_, _, cx| {
                                st.update(cx, |s, cx| {
                                    s.browse_remote(Target::Wsl(name.clone()), "~".into(), cx)
                                })
                            }),
                        );
                    }
                }
            }
        }
        RemotePage::Ssh => {
            body = body
                .child(back(t, &state, "SSH hosts", RemotePage::Root))
                .child(div().px(t.space.md).py(t.space.xs).child(Input::new(host)))
                .child(note(
                    t,
                    "Type user@host (or a Host from ~/.ssh/config) and press Enter.",
                    false,
                ));
            if !m.hosts.is_empty() {
                body = body.child(heading(t, "From ~/.ssh/config"));
                for h in &m.hosts {
                    let st = state.clone();
                    let host = h.clone();
                    body = body.child(
                        row(
                            t,
                            SharedString::from(format!("ssh-{host}")),
                            "icons/server.svg",
                            host.clone(),
                            None,
                            Some("icons/chevron-right.svg"),
                            true,
                        )
                        .on_click(move |_, _, cx| {
                            st.update(cx, |s, cx| {
                                s.browse_remote(Target::Ssh(host.clone()), "~".into(), cx)
                            })
                        }),
                    );
                }
            }
        }
        RemotePage::Containers => {
            body = body.child(back(t, &state, "Running containers", RemotePage::Root));
            match &m.containers {
                None => body = body.child(note(t, "Looking for containers…", false)),
                Some(Err(e)) => body = body.child(note(t, e.clone(), true)),
                Some(Ok(c)) if c.is_empty() => {
                    body = body.child(note(t, "No running containers.", false))
                }
                Some(Ok(list)) => {
                    for c in list {
                        let st = state.clone();
                        let name = c.name.clone();
                        body = body.child(
                            row(
                                t,
                                SharedString::from(format!("ctr-{name}")),
                                "icons/box.svg",
                                name.clone(),
                                Some(format!("{} · {}", c.image, c.status).into()),
                                Some("icons/chevron-right.svg"),
                                true,
                            )
                            .on_click(move |_, _, cx| {
                                st.update(cx, |s, cx| {
                                    s.browse_remote(
                                        Target::Container {
                                            name: name.clone(),
                                            user: None,
                                        },
                                        "/".into(),
                                        cx,
                                    )
                                })
                            }),
                        );
                    }
                }
            }
        }
        RemotePage::Folder => {
            let Some(f) = &m.folder else {
                return div().into_any_element();
            };
            let back_to = match f.target {
                Target::Wsl(_) => RemotePage::Wsl,
                Target::Ssh(_) => RemotePage::Ssh,
                Target::Container { .. } => RemotePage::Containers,
            };
            body = body.child(back(t, &state, "Open a folder", back_to)).child(
                div()
                    .px(t.space.md + t.space.xs)
                    .pb(t.space.xs)
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(icon(f.target.icon(), t.size.icon_xs, t.colors.text_muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(format!("{} · {}", f.target.label(), f.path)),
                    ),
            );
            let open_here = Remote {
                target: f.target.clone(),
                path: f.path.clone(),
            };
            let st = state.clone();
            let ready = matches!(f.entries, Some(Ok(_)));
            body = body.child(
                div().px(t.space.md).pb(t.space.sm).child(
                    theme::primary_button(t, "Open this folder")
                        .id("remote-open-here")
                        .w_full()
                        .when(!ready, |b| b.opacity(0.5))
                        .on_click(move |_, _, cx| {
                            if ready {
                                st.update(cx, |s, cx| s.open_remote_folder(open_here.clone(), cx))
                            }
                        }),
                ),
            );
            let mut list = div()
                .id("remote-folder-list")
                .max_h(px(260.))
                .overflow_y_scroll()
                .flex()
                .flex_col();
            match &f.entries {
                None => list = list.child(note(t, "Reading the folder…", false)),
                // Waiting on "Trust" isn't a failure.
                Some(Err(e)) => list = list.child(note(t, e.clone(), m.host_key.is_none())),
                Some(Ok(entries)) => {
                    if f.path != "/" {
                        let (st, target) = (state.clone(), f.target.clone());
                        let up = match f.path.trim_end_matches('/').rsplit_once('/') {
                            Some(("", _)) | None => "/".to_string(),
                            Some((p, _)) => p.to_string(),
                        };
                        list = list.child(
                            row(t, "remote-up", "icons/arrow-up.svg", "..", None, None, true)
                                .on_click(move |_, _, cx| {
                                    st.update(cx, |s, cx| {
                                        s.browse_remote(target.clone(), up.clone(), cx)
                                    })
                                }),
                        );
                    }
                    for (i, e) in entries.iter().enumerate() {
                        if e.dir {
                            let (st, target) = (state.clone(), f.target.clone());
                            let into = format!("{}/{}", f.path.trim_end_matches('/'), e.name);
                            list = list.child(
                                row(
                                    t,
                                    ("remote-dir", i),
                                    "icons/folder.svg",
                                    e.name.clone(),
                                    None,
                                    None,
                                    true,
                                )
                                .on_click(move |_, _, cx| {
                                    st.update(cx, |s, cx| {
                                        s.browse_remote(target.clone(), into.clone(), cx)
                                    })
                                }),
                            );
                        } else {
                            list = list.child(row(
                                t,
                                ("remote-file", i),
                                "icons/file.svg",
                                e.name.clone(),
                                None,
                                None,
                                false,
                            ));
                        }
                    }
                }
            }
            body = body.child(list);
        }
    }
    if let Some(e) = &m.error {
        body = body.child(note(t, e.clone(), true));
    }

    let close = state.clone();
    let lift = f32::from(t.space.xs);
    theme::popover(t)
        .id("remote-menu")
        .absolute()
        .bottom_full()
        .left(t.space.sm)
        .mb(t.space.xs)
        .w(px(320.))
        .occlude()
        .text_size(t.text.body_sm)
        .on_mouse_down_out(move |_, _, cx| {
            close.update(cx, |s, cx| {
                if s.remote_menu.as_ref().is_some_and(|m| m.host_key.is_none()) {
                    s.remote_menu = None;
                    cx.notify();
                }
            })
        })
        .child(body)
        .with_animation("remote-menu-in", super::motion::pop_in(t), move |d, k| {
            d.opacity(k).mb(px(lift - 3. * (1. - k)))
        })
        .into_any_element()
}
