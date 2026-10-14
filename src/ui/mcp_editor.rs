//! Settings → MCP servers: tools from other programs, which ForgeCLI starts
//! for each chat. A server is a command (stdio) or a URL (HTTP or SSE),
//! saved where `forge mcp add` saves it: yours (`mcpServers` in your
//! settings), the project's `.mcp.json` (shared; each person trusts it), or
//! this PC's for the project. Add, edit, remove, trust, and a status row from
//! `forge mcp list`, which starts each server the way a chat would.

use gpui::component::input::{InputState, TextareaState};
use gpui::{
    div, prelude::*, px, Context, Entity, Hsla, IntoElement, Render, SharedString, Subscription,
    Window,
};

use super::editor_kit as kit;
use super::*;
use crate::forge_files::{self as ff, McpScope, Transport};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Stdio,
    Http,
    Sse,
}

pub struct McpEditor {
    state: Entity<AppState>,
    /// The form is open: editing (scope, name), or (None) a new server.
    form: Option<Option<(McpScope, String)>>,
    scope: McpScope,
    kind: Kind,
    name: Entity<InputState>,
    command: Entity<InputState>,
    args: Entity<InputState>,
    url: Entity<InputState>,
    /// `KEY=value` lines (stdio) or `Name: value` lines (HTTP).
    pairs: Entity<TextareaState>,
    error: Option<SharedString>,
    saving: bool,
    confirm: Option<(McpScope, String)>,
    _sub: Subscription,
}

impl McpEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _sub: cx.observe(&state, |_, _, cx| cx.notify()),
            name: cx.new(|cx| InputState::new(window, cx).placeholder("github")),
            command: cx.new(|cx| InputState::new(window, cx).placeholder("npx")),
            args: cx.new(|cx| {
                InputState::new(window, cx).placeholder("-y @modelcontextprotocol/server-github")
            }),
            url: cx
                .new(|cx| InputState::new(window, cx).placeholder("https://mcp.example.com/mcp")),
            pairs: cx.new(|cx| TextareaState::new(window, cx)),
            state,
            form: None,
            scope: McpScope::User,
            kind: Kind::Stdio,
            error: None,
            saving: false,
            confirm: None,
        }
    }

    pub fn back(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.form.is_some() || self.confirm.is_some();
        self.form = None;
        self.confirm = None;
        cx.notify();
        open
    }

    fn pairs_placeholder(&self, window: &mut Window, cx: &mut Context<Self>) {
        let p = if self.kind == Kind::Stdio {
            "GITHUB_TOKEN=${GITHUB_TOKEN}"
        } else {
            "Authorization: Bearer ${MY_TOKEN}"
        };
        self.pairs
            .update(cx, |i, cx| i.set_placeholder(p, window, cx));
    }

    fn open(&mut self, server: Option<ff::Server>, window: &mut Window, cx: &mut Context<Self>) {
        let set = |i: &Entity<InputState>, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            i.update(cx, |i, cx| i.set_value(v.to_string(), window, cx))
        };
        let (mut command, mut args, mut url, mut pairs) =
            (String::new(), String::new(), String::new(), String::new());
        self.kind = Kind::Stdio;
        if let Some(s) = &server {
            match &s.transport {
                Transport::Stdio {
                    command: c,
                    args: a,
                    env,
                } => {
                    command = c.clone();
                    args = a
                        .iter()
                        .map(|a| {
                            if a.contains(' ') {
                                format!("\"{a}\"")
                            } else {
                                a.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    pairs = ff::pairs_text(env, "=");
                }
                Transport::Http {
                    url: u,
                    headers,
                    sse,
                } => {
                    self.kind = if *sse { Kind::Sse } else { Kind::Http };
                    url = u.clone();
                    pairs = ff::pairs_text(headers, ": ");
                }
            }
        }
        set(
            &self.name,
            server.as_ref().map_or("", |s| &s.name),
            window,
            cx,
        );
        set(&self.command, &command, window, cx);
        set(&self.args, &args, window, cx);
        set(&self.url, &url, window, cx);
        self.pairs
            .update(cx, |i, cx| i.set_value(pairs, window, cx));
        self.pairs_placeholder(window, cx);
        self.scope = server.as_ref().map_or(McpScope::User, |s| s.scope);
        self.form = Some(server.map(|s| (s.scope, s.name)));
        self.error = None;
        self.saving = false;
        self.name.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(old) = self.form.clone() else { return };
        let val =
            |i: &Entity<InputState>, cx: &Context<Self>| i.read(cx).value().trim().to_string();
        let name = val(&self.name, cx);
        let pairs_text = self.pairs.read(cx).value().to_string();
        let transport = match self.kind {
            Kind::Stdio => ff::split_args(&val(&self.args, cx)).and_then(|args| {
                Ok(Transport::Stdio {
                    command: val(&self.command, cx),
                    args,
                    env: ff::parse_pairs(&pairs_text, '=')?,
                })
            }),
            Kind::Http | Kind::Sse => {
                ff::parse_pairs(&pairs_text, ':').map(|headers| Transport::Http {
                    url: val(&self.url, cx),
                    headers,
                    sse: self.kind == Kind::Sse,
                })
            }
        };
        let transport = match transport {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(e.into());
                return cx.notify();
            }
        };
        if let Err(e) = ff::check_name(&name, false) {
            self.error = Some(e.into());
            return cx.notify();
        }
        let scope = self.scope;
        self.saving = true;
        self.error = None;
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Saved {name}. New chats start it."),
                move |project| {
                    ff::save_server(
                        project,
                        old.as_ref().map(|(s, n)| (*s, n.as_str())),
                        scope,
                        &name,
                        &transport,
                    )
                },
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match r {
                    Ok(()) => this.form = None,
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let s = self.state.read(cx);
        let catalog = s.engine.catalog.clone();
        let (checking, error, browser) =
            (s.engine.mcp_checking, s.engine.mcp_error.clone(), s.browser);
        let add = theme::primary_button(t, "Add server")
            .id("add-server")
            .on_click(cx.listener(|this, _, window, cx| this.open(None, window, cx)))
            .into_any_element();
        let mut out = div().flex().flex_col().gap(px(10.)).child(kit::header(
            t,
            "MCP servers",
            "An MCP server gives Forge more tools: a database, an issue tracker, a browser. ForgeCLI starts each one for every chat, and its tools ask permission like any other.",
            Some(add),
        ));
        let Some(catalog) = catalog else {
            return out.child(kit::loading(t)).into_any_element();
        };
        out = out.children(kit::problems(t, &catalog.problems));
        if catalog.servers.is_empty() {
            out = out.child(kit::empty(
                t,
                "No MCP servers yet. Add one with the command that starts it (npx, uvx, a program) or the URL it serves on.",
            ));
        }
        for scope in [McpScope::Project, McpScope::Local, McpScope::User] {
            let rows: Vec<&ff::Server> = catalog
                .servers
                .iter()
                .filter(|s| s.scope == scope)
                .collect();
            if rows.is_empty() {
                continue;
            }
            out = out.child(kit::group(
                t,
                match scope {
                    McpScope::Project => "This project, shared (.mcp.json)",
                    McpScope::Local => "This project, this PC only",
                    McpScope::User => "Yours, in every project",
                },
            ));
            for server in rows {
                out = out.child(self.row(t, server, cx));
            }
        }
        if browser {
            out = out.child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child("The web browser (Settings → General) adds Forge's browser tools for each chat too; it isn't saved in these files."),
            );
        }
        let status = if checking {
            Some((
                t.colors.text_muted,
                "Starting each server, as a chat would…".to_string(),
            ))
        } else {
            error.map(|e| (t.extra.danger, e))
        };
        // Nothing to check without a server.
        if catalog.servers.is_empty() {
            return out.into_any_element();
        }
        out.child(
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .pt(px(4.))
                .child(
                    kit::link(
                        t,
                        "check-servers",
                        "icons/refresh-cw.svg",
                        "Check each server",
                    )
                    .when(!checking, |d| {
                        d.on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |s, cx| s.check_mcp(cx))
                        }))
                    }),
                )
                .children(
                    status
                        .map(|(c, text)| div().text_size(t.text.caption).text_color(c).child(text)),
                ),
        )
        .into_any_element()
    }

    fn row(&self, t: &Theme, server: &ff::Server, cx: &mut Context<Self>) -> gpui::AnyElement {
        let key = format!("{:?}-{}", server.scope, server.name);
        let this_one = (server.scope, server.name.clone());
        if self.confirm.as_ref() == Some(&this_one) {
            let (scope, name) = this_one.clone();
            return kit::confirm_row(
                t,
                format!("Remove {}?", server.name),
                format!(
                    "It comes out of {}.",
                    match server.scope {
                        McpScope::User => "your settings.json",
                        McpScope::Project => "the project's .mcp.json, for everyone",
                        McpScope::Local => ".forge/settings.local.json",
                    }
                ),
                kit::cancel_button(t, "cancel-delete").on_click(cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                })),
                theme::danger_button(t, "Remove")
                    .id("confirm-delete")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = None;
                        let name = name.clone();
                        this.state.update(cx, |s, cx| {
                            s.engine_write(
                                format!("Removed {name}."),
                                move |project| ff::remove_server(project, scope, &name),
                                cx,
                            )
                            .detach()
                        });
                    })),
            )
            .into_any_element();
        }
        let checked = self
            .state
            .read(cx)
            .engine
            .mcp_status
            .get(&server.name)
            .cloned();
        let (dot, line): (Hsla, String) = if !server.trusted {
            (
                t.extra.warning,
                "Not trusted on this PC, so ForgeCLI won't start it.".into(),
            )
        } else if server.disabled {
            (
                t.colors.text_subtle,
                "Turned off (disabledMcpjsonServers).".into(),
            )
        } else {
            match checked {
                Some(s) if s.starts_with("connected") => (t.extra.success, s),
                Some(s) if s.starts_with("failed") => (t.extra.danger, s),
                Some(s) => (t.colors.text_subtle, s),
                None => (t.colors.text_subtle, "Not checked yet".into()),
            }
        };
        let auth = match &server.transport {
            Transport::Stdio { env, .. } if !env.is_empty() => {
                format!(
                    " · {} variable{}",
                    env.len(),
                    if env.len() == 1 { "" } else { "s" }
                )
            }
            Transport::Http { headers, .. } if !headers.is_empty() => {
                format!(
                    " · {} header{}",
                    headers.len(),
                    if headers.len() == 1 { "" } else { "s" }
                )
            }
            _ => String::new(),
        };
        let edit = server.clone();
        let (trust_name, needs_trust) = (server.name.clone(), !server.trusted || server.disabled);
        kit::card(t)
            .child(icon("icons/plug.svg", t.size.icon, t.colors.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(kit::card_text(
                        t,
                        server.name.clone(),
                        true,
                        Some(format!("{}{auth}", server.summary())),
                        None,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .size(px(6.))
                                    .flex_none()
                                    .rounded(t.radius.pill)
                                    .bg(dot),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .child(line),
                            ),
                    ),
            )
            .when(needs_trust && server.scope == McpScope::Project, |d| {
                d.child(
                    kit::chip(
                        t,
                        SharedString::from(format!("trust-{key}")),
                        "Trust",
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let name = trust_name.clone();
                        this.state.update(cx, |s, cx| {
                            s.engine_write(
                                format!("Trusting {name} on this PC."),
                                move |project| ff::trust_server(project, &name, true),
                                cx,
                            )
                            .detach()
                        });
                    })),
                )
            })
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("edit-mcp-{key}")),
                    "icons/pencil.svg",
                    "Edit",
                    false,
                )
                .on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.open(Some(edit.clone()), window, cx)
                    }),
                ),
            )
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("rm-mcp-{key}")),
                    "icons/trash.svg",
                    "Remove",
                    true,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm = Some(this_one.clone());
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn form(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let editing = matches!(self.form, Some(Some(_)));
        let mut scopes = div().flex().flex_wrap().gap(px(6.));
        for (scope, label) in [
            (McpScope::User, "Yours, every project"),
            (McpScope::Project, "Project, shared (.mcp.json)"),
            (McpScope::Local, "Project, this PC only"),
        ] {
            scopes = scopes.child(
                kit::chip(
                    t,
                    SharedString::from(format!("mcp-scope-{scope:?}")),
                    label,
                    self.scope == scope,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.scope = scope;
                    cx.notify();
                })),
            );
        }
        let mut kinds = div().flex().gap(px(6.));
        for (kind, label) in [
            (Kind::Stdio, "Command (stdio)"),
            (Kind::Http, "URL (HTTP)"),
            (Kind::Sse, "URL (SSE)"),
        ] {
            kinds = kinds.child(
                kit::chip(
                    t,
                    SharedString::from(format!("mcp-kind-{label}")),
                    label,
                    self.kind == kind,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.kind = kind;
                    this.pairs_placeholder(window, cx);
                    cx.notify();
                })),
            );
        }
        let stdio = self.kind == Kind::Stdio;
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(kit::back(t, "mcp-back", "MCP servers").on_click(cx.listener(|this, _, _, cx| {
                this.back(cx);
            })))
            .child(kit::header(
                t,
                if editing { "Edit server" } else { "Add a server" },
                "Keep keys out of the file: write ${NAME} and ForgeCLI fills it from the environment when it starts the server.",
                None,
            ))
            .child(kit::field(t, "Saved in", None, scopes.into_any_element()))
            .child(div().w(px(260.)).child(kit::field(
                t,
                "Name",
                Some("Its tools are named mcp__name__tool."),
                kit::line(t, &self.name, true),
            )))
            .child(kit::field(t, "Runs as", None, kinds.into_any_element()))
            .when(stdio, |d| {
                d.child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .child(div().w(px(180.)).child(kit::field(t, "Command", None, kit::line(t, &self.command, true))))
                        .child(div().flex_1().child(kit::field(t, "Arguments", None, kit::line(t, &self.args, true)))),
                )
            })
            .when(!stdio, |d| d.child(kit::field(t, "URL", None, kit::line(t, &self.url, true))))
            .child(kit::field(
                t,
                if stdio { "Environment (auth)" } else { "Headers (auth)" },
                Some(if stdio {
                    "One KEY=value a line."
                } else {
                    "One Name: value a line."
                }),
                kit::area(t, &self.pairs, true),
            ))
            .children(kit::status(t, &self.error))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(kit::cancel_button(t, "mcp-cancel").on_click(cx.listener(|this, _, _, cx| {
                        this.back(cx);
                    })))
                    .child(kit::save_button(t, "mcp-save", self.saving).when(!self.saving, |d| {
                        d.on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                    })),
            )
            .into_any_element()
    }
}

impl Render for McpEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.form.is_some() {
            self.form(&t, cx)
        } else {
            self.list(&t, cx)
        }
    }
}
