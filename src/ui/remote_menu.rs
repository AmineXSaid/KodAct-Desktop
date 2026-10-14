//! The remote quick pick, opened from the status bar's remote indicator
//! (`status_bar.rs`), Ctrl+Alt+O or the command palette, as in VS Code: a
//! filter box, then **Recent** remote folders (with a WSL, SSH or Container
//! badge) and the **Actions**: connect to an SSH host, connect to WSL, reopen
//! the folder in its dev container, attach to a running container, close the
//! remote connection. Each action's page lists what it can connect to; the
//! last step picks a folder there. Type to filter, ↑/↓ to move, Enter to
//! choose, Esc to go back or close. An SSH host seen for the first time shows
//! its key fingerprint and waits for "Trust".

use std::time::Duration;

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, Animation, AnimationExt, AnyElement, Context, Entity, FontWeight,
    KeyDownEvent, Render, SharedString, Subscription, Transformation, Window,
};

use super::icon;
use crate::remote::{Remote, Target};
use crate::state::{AppState, RemotePage};
use crate::theme::{self, ActiveTheme, Theme};

/// What choosing a row does.
#[derive(Clone)]
enum Act {
    Page(RemotePage),
    Project(usize),
    Reopen,
    Close,
    Browse(Target, String),
    Open(Remote),
}

/// One row of the list. `act: None` shows it disabled, with its caption
/// saying why.
#[derive(Clone)]
struct Row {
    section: Option<&'static str>,
    icon: &'static str,
    label: SharedString,
    caption: Option<SharedString>,
    badge: Option<&'static str>,
    trailing: Option<&'static str>,
    act: Option<Act>,
}

impl Row {
    fn new(icon: &'static str, label: impl Into<SharedString>, act: Act) -> Self {
        Row {
            section: None,
            icon,
            label: label.into(),
            caption: None,
            badge: None,
            trailing: None,
            act: Some(act),
        }
    }
    fn caption(mut self, c: impl Into<SharedString>) -> Self {
        self.caption = Some(c.into());
        self
    }
    fn section(mut self, s: &'static str) -> Self {
        self.section = Some(s);
        self
    }
    fn more(mut self) -> Self {
        self.trailing = Some("icons/chevron-right.svg");
        self
    }
}

fn badge(t: &Target) -> &'static str {
    match t {
        Target::Wsl(_) => "WSL",
        Target::Ssh(_) => "SSH",
        Target::Container { .. } => "Container",
    }
}

pub struct RemotePicker {
    state: Entity<AppState>,
    query: Entity<InputState>,
    selected: usize,
    /// The page (and folder) the query was typed on: a new page or folder
    /// starts with an empty box.
    page: Option<(RemotePage, Option<String>)>,
    _subs: Vec<Subscription>,
}

impl RemotePicker {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Type to filter"));
        let subs = vec![
            // Opening, or moving to another page: an empty box, focused.
            cx.observe_in(&state, window, |this, state, window, cx| {
                let page = state
                    .read(cx)
                    .remote_menu
                    .as_ref()
                    .map(|m| (m.page, m.folder.as_ref().map(|f| f.path.clone())));
                if page != this.page {
                    // Closing hands the keyboard back to the composer, or
                    // keys would go to a box no longer on screen.
                    if page.is_none() && this.page.is_some() {
                        window.dispatch_action(Box::new(super::FocusComposer), cx);
                    }
                    this.page = page.clone();
                    this.selected = 0;
                    if let Some((page, _)) = page {
                        let hint = match Some(page) {
                            Some(RemotePage::Ssh) => "Type a host from ~/.ssh/config, or user@host",
                            Some(RemotePage::Folder) => "Type to filter folders",
                            _ => "Type to filter",
                        };
                        this.query.update(cx, |q, cx| {
                            q.set_value("", window, cx);
                            q.set_placeholder(hint, window, cx);
                            q.focus(window, cx);
                        });
                    }
                }
                cx.notify();
            }),
            cx.subscribe_in(&query, window, |this, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.selected = 0;
                    cx.notify();
                }
            }),
        ];
        Self {
            state,
            query,
            selected: 0,
            page: None,
            _subs: subs,
        }
    }

    /// The rows for the page showing, filtered by the box.
    fn rows(&self, s: &AppState, q: &str) -> Vec<Row> {
        let Some(m) = &s.remote_menu else {
            return vec![];
        };
        let matches =
            |label: &str| q.is_empty() || super::command_palette::fuzzy(q, label).is_some();
        let mut rows = vec![];
        match m.page {
            RemotePage::Root => {
                let connected = s.current_remote().is_some();
                // Recent: every folder opened lately, here or remote, newest first.
                let mut recent: Vec<&crate::state::Project> = s
                    .projects
                    .iter()
                    .filter(|p| p.id != s.current_project)
                    .collect();
                recent.sort_by_key(|p| (p.opened_minutes_ago, std::cmp::Reverse(p.id)));
                for p in recent {
                    let remote = Remote::parse(&p.path);
                    let (glyph, caption, kind) = match &remote {
                        Some(r) => (
                            r.target.icon(),
                            format!("{} · {}", r.target.label(), r.path),
                            badge(&r.target),
                        ),
                        None => ("icons/folder.svg", tilde(&p.path), "Local"),
                    };
                    if (matches(&p.name) || matches(&caption)) && rows.len() < 8 {
                        let mut row = Row::new(glyph, p.name.clone(), Act::Project(p.id))
                            .caption(caption)
                            .section("Recent");
                        row.badge = Some(kind);
                        rows.push(row);
                    }
                }
                let mut actions = vec![
                    Row::new(
                        "icons/server.svg",
                        "Connect to SSH host…",
                        Act::Page(RemotePage::Ssh),
                    )
                    .more(),
                    Row::new(
                        "icons/terminal.svg",
                        "Connect to WSL…",
                        Act::Page(RemotePage::Wsl),
                    )
                    .more(),
                ];
                let mut reopen = Row::new("icons/box.svg", "Reopen in container", Act::Reopen);
                if !s.has_devcontainer() {
                    reopen.act = None;
                    reopen.caption = Some(if connected {
                        "Open the local folder first".into()
                    } else {
                        "This folder has no .devcontainer/devcontainer.json".into()
                    });
                }
                actions.push(reopen);
                actions.push(
                    Row::new(
                        "icons/box.svg",
                        "Attach to running container…",
                        Act::Page(RemotePage::Containers),
                    )
                    .more(),
                );
                // Closing has its own button at the foot; typing finds it too.
                if connected && !q.is_empty() {
                    actions.push(
                        Row::new("icons/unplug.svg", "Close remote connection", Act::Close)
                            .caption("Back to this computer"),
                    );
                }
                let mut first = true;
                for r in actions.into_iter().filter(|r| matches(&r.label)) {
                    rows.push(if std::mem::take(&mut first) {
                        r.section("Actions")
                    } else {
                        r
                    });
                }
            }
            RemotePage::Wsl => {
                if let Some(Ok(list)) = &m.distros {
                    for d in list.iter().filter(|d| !is_helper_distro(&d.name)) {
                        if matches(&d.name) {
                            let caption = format!(
                                "{}{} · WSL {}",
                                if d.default { "Default · " } else { "" },
                                if d.running {
                                    "Running"
                                } else {
                                    "Stopped, starts when you connect"
                                },
                                d.version
                            );
                            rows.push(
                                Row::new(
                                    "icons/terminal.svg",
                                    d.name.clone(),
                                    Act::Browse(Target::Wsl(d.name.clone()), "~".into()),
                                )
                                .caption(caption)
                                .more(),
                            );
                        }
                    }
                }
            }
            RemotePage::Ssh => {
                // Known hosts that match first; then the typed text as a new host.
                let typed = q.trim();
                for (n, h) in m.hosts.iter().filter(|h| matches(h)).enumerate() {
                    let row = Row::new(
                        "icons/server.svg",
                        h.clone(),
                        Act::Browse(Target::Ssh(h.clone()), "~".into()),
                    )
                    .more();
                    rows.push(if n == 0 {
                        row.section("From ~/.ssh/config")
                    } else {
                        row
                    });
                }
                if !typed.is_empty() && !m.hosts.iter().any(|h| h == typed) {
                    rows.push(
                        Row::new(
                            "icons/plus.svg",
                            format!("Connect to {typed}"),
                            Act::Browse(Target::Ssh(typed.into()), "~".into()),
                        )
                        .caption("A new host: user@host, or a Host from ~/.ssh/config")
                        .section("New host"),
                    );
                }
            }
            RemotePage::Containers => {
                if let Some(Ok(list)) = &m.containers {
                    for c in list.iter().filter(|c| matches(&c.name)) {
                        rows.push(
                            Row::new(
                                "icons/box.svg",
                                c.name.clone(),
                                Act::Browse(
                                    Target::Container {
                                        name: c.name.clone(),
                                        user: None,
                                    },
                                    "/".into(),
                                ),
                            )
                            .caption(format!("{} · {}", c.image, c.status))
                            .more(),
                        );
                    }
                }
            }
            RemotePage::Folder => {
                if let Some(f) = &m.folder {
                    if let Some(Ok(entries)) = &f.entries {
                        rows.push(
                            Row::new(
                                "icons/folder-open.svg",
                                "Open this folder",
                                Act::Open(Remote {
                                    target: f.target.clone(),
                                    path: f.path.clone(),
                                }),
                            )
                            .caption(f.path.clone()),
                        );
                        if f.path != "/" {
                            let up = match f.path.trim_end_matches('/').rsplit_once('/') {
                                Some(("", _)) | None => "/".to_string(),
                                Some((p, _)) => p.to_string(),
                            };
                            rows.push(Row::new(
                                "icons/arrow-up.svg",
                                "..",
                                Act::Browse(f.target.clone(), up),
                            ));
                        }
                        for e in entries.iter().filter(|e| matches(&e.name)) {
                            if e.dir {
                                let into = format!("{}/{}", f.path.trim_end_matches('/'), e.name);
                                rows.push(Row::new(
                                    "icons/folder.svg",
                                    e.name.clone(),
                                    Act::Browse(f.target.clone(), into),
                                ));
                            } else {
                                let mut r = Row::new("icons/file.svg", e.name.clone(), Act::Close);
                                r.act = None;
                                rows.push(r);
                            }
                        }
                    }
                }
            }
        }
        rows
    }

    fn run(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        self.state.update(cx, |s, cx| match act {
            Act::Page(p) => s.remote_page(p, cx),
            Act::Project(id) => {
                s.remote_menu = None;
                s.show_project(id, cx);
                // A recent remote folder connects at once (checking a
                // connection made earlier still answers).
                if let Some(r) = s.current_remote() {
                    s.connect(r.target, None, cx);
                }
            }
            Act::Reopen => s.reopen_in_container(cx),
            Act::Close => s.close_remote(cx),
            Act::Browse(t, p) => s.browse_remote(t, p, cx),
            Act::Open(r) => s.open_remote_folder(r, cx),
        });
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            let Some(m) = &s.remote_menu else { return };
            let to = match m.page {
                RemotePage::Root => None,
                RemotePage::Folder => Some(match m.folder.as_ref().map(|f| &f.target) {
                    Some(Target::Wsl(_)) => RemotePage::Wsl,
                    Some(Target::Ssh(_)) => RemotePage::Ssh,
                    _ => RemotePage::Containers,
                }),
                _ => Some(RemotePage::Root),
            };
            match to {
                Some(p) => s.remote_page(p, cx),
                None => {
                    s.remote_menu = None;
                    cx.notify();
                }
            }
        });
    }
}

/// A local path with the home folder shown as `~`.
fn tilde(path: &std::path::Path) -> String {
    match crate::rules::home_dir().and_then(|h| path.strip_prefix(h).ok().map(|r| r.to_path_buf()))
    {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Docker Desktop's own distros, which aren't for working in.
fn is_helper_distro(name: &str) -> bool {
    name.starts_with("docker-desktop") || name.starts_with("rancher-desktop")
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

impl Render for RemotePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Some(m) = s.remote_menu.clone() else {
            return div().into_any_element();
        };
        let q = self.query.read(cx).value().to_string();
        let rows = self.rows(s, &q);
        let enabled: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.act.is_some())
            .map(|(i, _)| i)
            .collect();
        let count = enabled.len();
        let selected = if count == 0 {
            usize::MAX
        } else {
            enabled[self.selected.min(count - 1)]
        };

        // What the page is waiting on, or why it's empty.
        let status: Option<(SharedString, bool)> = match m.page {
            RemotePage::Wsl => match &m.distros {
                None => Some(("Looking for WSL distros…".into(), false)),
                Some(Err(e)) => Some((e.clone().into(), true)),
                Some(Ok(_)) if rows.is_empty() && q.is_empty() => {
                    Some(("No distros yet. Install one with `wsl --install -d Ubuntu`.".into(), false))
                }
                _ => None,
            },
            RemotePage::Containers => match &m.containers {
                None => Some(("Looking for containers…".into(), false)),
                Some(Err(e)) => Some((e.clone().into(), true)),
                Some(Ok(_)) if rows.is_empty() && q.is_empty() => Some(("No running containers.".into(), false)),
                _ => None,
            },
            RemotePage::Ssh if m.hosts.is_empty() && q.is_empty() => Some((
                "No hosts in ~/.ssh/config. Type user@host to connect. Forge signs in with your SSH keys or agent.".into(),
                false,
            )),
            RemotePage::Folder => match m.folder.as_ref().and_then(|f| f.entries.as_ref()) {
                None => Some(("Reading the folder…".into(), false)),
                // Waiting on "Trust" isn't a failure.
                Some(Err(e)) => Some((e.clone().into(), m.host_key.is_none())),
                _ => None,
            },
            _ => None,
        };
        let title: Option<SharedString> = match m.page {
            RemotePage::Root => None,
            RemotePage::Wsl => Some("Connect to WSL".into()),
            RemotePage::Ssh => Some("Connect to SSH host".into()),
            RemotePage::Containers => Some("Attach to running container".into()),
            RemotePage::Folder => m
                .folder
                .as_ref()
                .map(|f| format!("Open a folder on {}", f.target.label()).into()),
        };

        let mut body = div().flex().flex_col().py(t.space.xs);

        if let Some((_, h, fp)) = &m.host_key {
            let (trust, cancel) = (self.state.clone(), self.state.clone());
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
                    .child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child(format!("Trust {h}?")))
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child("Forge hasn't connected to this host before. Trust it only if this key fingerprint matches the server's:"),
                    )
                    .child(div().font_family(t.font_mono.clone()).text_size(t.text.caption).text_color(t.colors.text).child(fp.clone()))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(t.space.sm)
                            .child(theme::ghost_button(&t, "Cancel").id("remote-untrust").on_click(move |_, _, cx| {
                                cancel.update(cx, |s, cx| {
                                    if let Some(m) = &mut s.remote_menu {
                                        m.host_key = None;
                                    }
                                    cx.notify();
                                })
                            }))
                            .child(
                                theme::primary_button(&t, "Trust and connect")
                                    .id("remote-trust")
                                    .on_click(move |_, _, cx| trust.update(cx, |s, cx| s.trust_host(cx))),
                            ),
                    ),
            );
        }

        if let Some(title) = title {
            let hover = t.colors.hover;
            body = body.child(
                div()
                    .id("remote-back")
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .mx(t.space.xs)
                    .px(t.space.md)
                    .h(t.size.row)
                    .rounded(t.radius.small)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_strong)
                    .child(icon(
                        "icons/arrow-left.svg",
                        t.size.icon_sm,
                        t.colors.text_muted,
                    ))
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
            );
        }

        body = body.child(
            div()
                .px(t.space.md)
                .py(t.space.xs)
                .child(Input::new(&self.query)),
        );

        let mut list = div()
            .id("remote-list")
            .max_h(px(340.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        let mut section_seen = None;
        let mut nth = 0usize;
        for (i, r) in rows.iter().enumerate() {
            if let Some(sec) = r.section {
                if section_seen != Some(sec) {
                    section_seen = Some(sec);
                    list = list.child(
                        div()
                            .px(t.space.md + t.space.xs)
                            .pt(t.space.sm)
                            .pb(t.space.xs)
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_muted)
                            .child(sec),
                    );
                }
            }
            let on = i == selected;
            let enabled_row = r.act.is_some();
            let my_index = if enabled_row { Some(nth) } else { None };
            if enabled_row {
                nth += 1;
            }
            let hover = t.colors.hover;
            let act = r.act.clone();
            list = list.child(
                div()
                    .id(("remote-row", i))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .min_h(t.size.row)
                    .px(t.space.md)
                    .py(px(4.))
                    .mx(t.space.xs)
                    .rounded(t.radius.small)
                    .when(on, |d| d.bg(t.colors.hover))
                    .when(enabled_row, |d| {
                        d.cursor_pointer().hover(move |st| st.bg(hover))
                    })
                    .child(icon(
                        r.icon,
                        t.size.icon_sm,
                        if enabled_row {
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
                                    .text_color(if enabled_row {
                                        t.colors.text
                                    } else {
                                        t.colors.text_muted
                                    })
                                    .child(r.label.clone()),
                            )
                            .children(r.caption.clone().map(|c| {
                                div()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(c)
                            })),
                    )
                    .children(r.badge.map(|b| {
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(b)
                    }))
                    .children(
                        r.trailing
                            .map(|g| icon(g, t.size.icon_xs, t.colors.text_subtle)),
                    )
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if let Some(n) = my_index {
                            if this.selected != n {
                                this.selected = n;
                                cx.notify();
                            }
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(a) = act.clone() {
                            this.run(a, window, cx);
                        }
                    })),
            );
        }
        if let Some((text, error)) = status {
            list = list.child(note(&t, text, error));
        } else if rows.is_empty() {
            list = list.child(note(&t, "Nothing matches.", false));
        }
        body = body.child(list);
        if m.page == RemotePage::Root {
            if let Some(r) = s.current_remote() {
                let close = self.state.clone();
                let connected = s.remote_connected(&r.target);
                body = body.child(
                    div()
                        .mt(t.space.xs)
                        .mx(t.space.xs)
                        .mb(t.space.xs)
                        .px(t.space.md)
                        .py(t.space.sm)
                        .rounded(t.radius.button)
                        .bg(t.extra.inset)
                        .border_1()
                        .border_color(t.colors.border)
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(div().size(px(7.)).flex_none().rounded(t.radius.pill).bg(
                            if connected {
                                t.extra.success
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
                                        .text_color(t.colors.text_strong)
                                        .font_weight(FontWeight::MEDIUM)
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(r.target.label()),
                                )
                                .child(
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(r.path.clone()),
                                ),
                        )
                        .child(
                            div()
                                .id("remote-close")
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(t.space.xs + px(2.))
                                .px(t.space.md)
                                .h(t.size.control + px(4.))
                                .rounded(t.radius.button)
                                .bg(t.colors.primary_tint)
                                .text_color(t.colors.primary)
                                .text_size(t.text.body_sm)
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .hover(|st| st.opacity(0.85))
                                .child(icon("icons/unplug.svg", t.size.icon_sm, t.colors.primary))
                                .child("Close connection")
                                .tooltip(|window, cx| {
                                    gpui::component::tooltip::Tooltip::new(
                                        "Close the remote connection and work on this computer",
                                    )
                                    .build(window, cx)
                                })
                                .on_click(move |_, _, cx| {
                                    close.update(cx, |s, cx| s.close_remote(cx))
                                }),
                        ),
                );
            }
        }

        let run_rows = rows.clone();
        let lift = f32::from(t.space.xs);
        theme::popover(&t)
            .id("remote-menu")
            .absolute()
            .bottom_full()
            .left(t.space.xs)
            .mb(t.space.xs)
            .w(px(360.))
            .occlude()
            .text_size(t.text.body_sm)
            .key_context("RemotePicker")
            .capture_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "down" if count > 0 => {
                        this.selected = (this.selected.min(count - 1) + 1) % count
                    }
                    "up" if count > 0 => {
                        this.selected = (this.selected.min(count - 1) + count - 1) % count
                    }
                    "escape" => this.back(cx),
                    "enter" if count > 0 => {
                        let i = enabled[this.selected.min(count - 1)];
                        if let Some(a) = run_rows[i].act.clone() {
                            this.run(a, window, cx);
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    if s.remote_menu.as_ref().is_some_and(|m| m.host_key.is_none()) {
                        s.remote_menu = None;
                        cx.notify();
                    }
                })
            }))
            .child(body)
            .with_animation("remote-menu-in", super::motion::pop_in(&t), move |d, k| {
                d.opacity(k).mb(px(lift - 3. * (1. - k)))
            })
            .into_any_element()
    }
}

/// The remote indicator, leftmost in the status bar: icon only on this
/// computer; amber with a spinner and the step while connecting; the accent
/// with the target's name when connected; red when it failed.
pub fn indicator(s: &AppState, state: Entity<AppState>, t: &Theme) -> AnyElement {
    enum Look {
        Local,
        Busy(SharedString),
        Connected(String, &'static str),
        Waiting(String, &'static str),
        Failed(SharedString),
    }
    let remote = s.current_remote();
    let look = if let Some((_, msg)) = &s.remote_busy {
        Look::Busy(msg.clone())
    } else if let Some((_, why)) = &s.remote_failed {
        Look::Failed(why.clone())
    } else {
        match &remote {
            None => Look::Local,
            Some(r) if s.remote_connected(&r.target) => {
                Look::Connected(r.target.label(), r.target.icon())
            }
            Some(r) => Look::Waiting(r.target.label(), r.target.icon()),
        }
    };
    let (bg, fg, label, glyph, tip): (
        gpui::Hsla,
        gpui::Hsla,
        Option<SharedString>,
        &str,
        SharedString,
    ) = match &look {
        Look::Local => (
            gpui::transparent_black(),
            t.colors.text_muted,
            None,
            "icons/plug.svg",
            "Open a remote window: WSL, SSH host or container (Ctrl+Alt+O)".into(),
        ),
        Look::Busy(msg) => (
            t.extra.warning_tint,
            t.extra.warning,
            Some(msg.clone()),
            "icons/loader-circle.svg",
            msg.clone(),
        ),
        Look::Connected(l, g) => (
            t.extra.fill,
            gpui::white(),
            Some(l.clone().into()),
            g,
            format!("{l}: connected").into(),
        ),
        Look::Waiting(l, g) => (
            t.colors.primary_tint,
            t.colors.primary,
            Some(l.clone().into()),
            g,
            format!("{l}: connects with the next message").into(),
        ),
        Look::Failed(why) => (
            t.extra.danger_tint,
            t.extra.danger,
            Some("Connection failed".into()),
            "icons/triangle-alert.svg",
            why.clone(),
        ),
    };
    let busy = matches!(look, Look::Busy(_));
    let open = s.remote_menu.is_some();
    let glyph_el: AnyElement = if busy {
        icon(glyph, t.size.icon_xs, fg)
            .with_animation(
                "remote-busy",
                Animation::new(Duration::from_millis(900)).repeat(),
                |i, k| i.with_transformation(Transformation::rotate(gpui::percentage(k))),
            )
            .into_any_element()
    } else {
        icon(glyph, t.size.icon_xs, fg).into_any_element()
    };
    let hover = t.colors.hover;
    let local = matches!(look, Look::Local);
    div()
        .id("remote-indicator")
        .flex()
        .flex_none()
        .items_center()
        .gap(t.space.xs + px(2.))
        .h_full()
        .px(t.space.md)
        .max_w(px(300.))
        .bg(bg)
        .text_color(fg)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(local, move |d| d.hover(move |st| st.bg(hover)))
        .when(!local, |d| d.hover(|st| st.opacity(0.88)))
        .when(open && local, |d| d.bg(t.colors.hover))
        .child(glyph_el)
        .children(label.map(|l| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(l)
        }))
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
        })
        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.toggle_remote_menu(cx)))
        .into_any_element()
}
