//! New project modal: create or open a project from this computer or clone
//! from GitHub, GitLab, or a custom self-hosted Git host.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window,
};

use super::editor_kit as kit;
use super::*;
use crate::git_status::ForgeKind;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceTab {
    Local,
    Remote,
}

pub struct NewProject {
    state: Entity<AppState>,
    tab: SourceTab,
    host_kind: ForgeKind,
    custom_host: Entity<InputState>,
    url: Entity<InputState>,
    pat: Entity<InputState>,
    name: Entity<InputState>,
    template: &'static str,
    _subs: Vec<Subscription>,
}

impl NewProject {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let custom_host = cx.new(|cx| InputState::new(window, cx).placeholder("git.company.com"));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("owner/repo"));
        let pat = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Personal access token")
                .masked(true)
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
        let subs = vec![
            // The address's prefix and where it clones follow what's typed.
            cx.observe(&url, |_, _, cx| cx.notify()),
            cx.observe(&custom_host, |_, _, cx| cx.notify()),
            cx.subscribe(&url, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.clone_it(cx);
                }
            }),
            cx.subscribe(&pat, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.clone_it(cx);
                }
            }),
            cx.subscribe(&name, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.create(cx);
                }
            }),
        ];
        Self {
            state,
            tab: SourceTab::Local,
            host_kind: ForgeKind::GitHub,
            custom_host,
            url,
            pat,
            name,
            template: "code",
            _subs: subs,
        }
    }

    fn clone_it(&mut self, cx: &mut Context<Self>) {
        let raw_url = self.url.read(cx).value().to_string();
        let custom_host = self.custom_host.read(cx).value().to_string();
        let token = self.pat.read(cx).value().to_string();
        let Some(url) =
            crate::templates::normalize_git_clone_url(self.host_kind, Some(&custom_host), &raw_url)
        else {
            self.state.update(cx, |s, cx| {
                let msg = if self.host_kind == ForgeKind::Custom
                    && custom_host.trim().is_empty()
                    && !raw_url.contains("://")
                    && !raw_url.starts_with("git@")
                {
                    "Enter your custom Git host domain or paste a full clone URL."
                } else {
                    "Enter owner/repo or paste a valid Git repository URL."
                };
                s.show_toast(msg, cx);
            });
            return;
        };
        let pat = (!token.trim().is_empty()).then_some(token);
        self.state
            .update(cx, |s, cx| s.clone_project_with_auth(url, pat, cx));
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().to_string();
        let template = self.template;
        self.state
            .update(cx, |s, cx| s.create_project(name, template, cx));
    }
}

fn field_label(t: &Theme, label: &str) -> gpui::Div {
    div()
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .child(label.to_string())
}

impl Render for NewProject {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let picker = self.state.clone();
        let close_scrim = self.state.clone();
        let close_btn = self.state.clone();

        // The two sources, as every single choice is: the app's segmented
        // control (the provider below and Settings' Appearance use it too).
        let (to_local, to_remote) = (cx.entity(), cx.entity());
        let switcher = div().flex().child(
            super::button::Segmented::new("np-source", "Where the project comes from")
                .option(
                    "On this computer",
                    self.tab == SourceTab::Local,
                    move |_, _, cx| {
                        to_local.update(cx, |this, cx| {
                            this.tab = SourceTab::Local;
                            cx.notify();
                        })
                    },
                )
                .option(
                    "Clone from Git",
                    self.tab == SourceTab::Remote,
                    move |_, window, cx| {
                        to_remote.update(cx, |this, cx| {
                            this.tab = SourceTab::Remote;
                            this.url.update(cx, |i, cx| i.focus(window, cx));
                            cx.notify();
                        })
                    },
                ),
        );

        let local_body = {
            let mut chips = div().flex().flex_wrap().gap(t.space.xs);
            for tpl in crate::templates::ALL {
                let on = self.template == tpl.key;
                chips = chips.child(
                    kit::chip(
                        &t,
                        SharedString::from(format!("np-tpl-{}", tpl.key)),
                        tpl.label,
                        on,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.template = tpl.key;
                        cx.notify();
                    })),
                );
            }
            let chosen = crate::templates::by_key(self.template);
            div()
                .flex()
                .flex_col()
                .gap(t.space.md)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.md)
                        .pb(t.space.lg)
                        .border_b_1()
                        .border_color(t.colors.border)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(t.space.sm)
                                        .child(icon(
                                            "icons/folder-open.svg",
                                            t.size.icon_sm,
                                            t.colors.primary,
                                        ))
                                        .child(
                                            div()
                                                .text_size(t.text.body_sm)
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(t.colors.text_strong)
                                                .child("Open an existing folder"),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .pl(t.size.icon_sm + t.space.sm)
                                        .child("Keep its files and history; KodAct adds its instructions."),
                                ),
                        )
                        .child(
                            theme::ghost_button(&t, "Choose folder…")
                                .id("np-open")
                                .flex_none()
                                .on_click(move |_, _, cx| {
                                    super::projects::open_folder(picker.clone(), cx)
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(t.space.sm)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .child(icon("icons/folder-plus.svg", t.size.icon_sm, t.colors.primary))
                                .child(
                                    div()
                                        .text_size(t.text.body_sm)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(t.colors.text_strong)
                                        .child("Start a new project"),
                                ),
                        )
                        .child(field_label(&t, "Name"))
                        .child(kit::line(&t, &self.name, false))
                        .child(field_label(&t, "Template"))
                        .child(chips)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .pt(t.space.xs)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_subtle)
                                        .child(if chosen.instructions.is_empty() {
                                            "An empty folder in KodAct Projects.".to_string()
                                        } else {
                                            format!(
                                                "Saved in KodAct Projects, set up for {}.",
                                                chosen.label.to_lowercase()
                                            )
                                        }),
                                )
                                .child(
                                    theme::primary_button(&t, "Create project")
                                        .id("np-create")
                                        .flex_none()
                                        .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                                ),
                        ),
                )
        };

        let remote_body = {
            let (gh, gl, other) = (cx.entity(), cx.entity(), cx.entity());
            let provider = super::button::Segmented::new("np-host", "Git host")
                .option(
                    "GitHub",
                    self.host_kind == ForgeKind::GitHub,
                    move |_, _, cx| {
                        gh.update(cx, |this, cx| {
                            this.host_kind = ForgeKind::GitHub;
                            cx.notify();
                        })
                    },
                )
                .option(
                    "GitLab",
                    self.host_kind == ForgeKind::GitLab,
                    move |_, _, cx| {
                        gl.update(cx, |this, cx| {
                            this.host_kind = ForgeKind::GitLab;
                            cx.notify();
                        })
                    },
                )
                .option(
                    "Other server",
                    self.host_kind == ForgeKind::Custom,
                    move |_, window, cx| {
                        other.update(cx, |this, cx| {
                            this.host_kind = ForgeKind::Custom;
                            this.custom_host.update(cx, |i, cx| i.focus(window, cx));
                            cx.notify();
                        })
                    },
                );

            let raw = self.url.read(cx).value().to_string();
            let server = self.custom_host.read(cx).value().trim().to_string();
            let pasted_url = raw.contains("://") || raw.trim_start().starts_with("git@");
            // What to type after the host, shown before the field: the host
            // itself, so "owner/repo" reads as the rest of the address.
            let prefix = (!pasted_url).then(|| match self.host_kind {
                ForgeKind::GitHub => "github.com/".to_string(),
                ForgeKind::GitLab if server.is_empty() => "gitlab.com/".to_string(),
                _ if server.is_empty() => "server/".to_string(),
                _ => format!(
                    "{}/",
                    server
                        .trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .trim_end_matches('/')
                ),
            });
            let target =
                crate::templates::normalize_git_clone_url(self.host_kind, Some(&server), &raw)
                    .and_then(|u| {
                        crate::templates::projects_home()
                            .and_then(|h| crate::templates::clone_target(&h, &u))
                    })
                    .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()));
            let repo_box = div()
                .h(t.size.field_h)
                .pl(t.space.sm)
                .pr(px(4.))
                .flex()
                .items_center()
                .rounded(t.radius.field)
                .bg(t.colors.surface)
                .border_1()
                .border_color(t.extra.border_strong)
                // The host, fixed: an addon set off by a hairline, so it
                // reads as part of the address and not as a hint.
                .when_some(prefix, |d, p| {
                    d.child(
                        div()
                            .flex_none()
                            .h_full()
                            .flex()
                            .items_center()
                            .pr(t.space.sm)
                            .mr(t.space.xs)
                            .border_r_1()
                            .border_color(t.colors.border)
                            .text_size(t.text.body_sm)
                            .text_color(t.colors.text_muted)
                            .child(p),
                    )
                })
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&self.url)
                            .appearance(false)
                            .text_size(t.text.body_sm),
                    ),
                );
            let optional = |label: &str| {
                div()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_muted)
                            .child(label.to_string()),
                    )
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child("Optional"),
                    )
            };

            div()
                .flex()
                .flex_col()
                .gap(t.space.lg)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(t.space.md)
                        .child(
                            div()
                                .text_size(t.text.body_sm)
                                .text_color(t.colors.text_muted)
                                .child("Where the repository lives"),
                        )
                        .child(provider),
                )
                .when(self.host_kind != ForgeKind::GitHub, |d| {
                    let gitlab = self.host_kind == ForgeKind::GitLab;
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(if gitlab {
                                optional("Server").into_any_element()
                            } else {
                                field_label(&t, "Server").into_any_element()
                            })
                            .child(kit::line(&t, &self.custom_host, false))
                            .child(
                                div()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .child(if gitlab {
                                        "Only for a self-hosted GitLab; gitlab.com otherwise."
                                    } else {
                                        "Your company's Git server, like git.company.com."
                                    }),
                            ),
                    )
                })
                .child(kit::field(
                    &t,
                    "Repository",
                    Some(match self.host_kind {
                        ForgeKind::GitLab => "group/project, or paste the full address.",
                        _ => "owner/name, or paste the full address (HTTPS or SSH).",
                    }),
                    repo_box.into_any_element(),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(optional("Access token"))
                        .child(kit::line(&t, &self.pat, false))
                        .child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child("Only for a private repository. Used for this clone, never saved."),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.md)
                        .pt(t.space.md)
                        .border_t_1()
                        .border_color(t.colors.border)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(match target {
                                    Some(d) => format!("Saved as {d} in KodAct Projects"),
                                    None => "Saved in KodAct Projects".to_string(),
                                }),
                        )
                        .child(
                            theme::primary_button(&t, "Clone and open")
                                .id("np-clone")
                                .flex_none()
                                .on_click(cx.listener(|this, _, _, cx| this.clone_it(cx))),
                        ),
                )
        };

        let modal = super::panel_chrome::window(
            &t,
            theme::popover(&t)
                .id("new-project-modal")
                .occlude()
                .w(px(580.))
                .max_w(relative(0.92))
                .max_h(relative(0.88))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            "np",
            "Create project",
            None,
            move |_, _, cx| {
                close_btn.update(cx, |s, cx| {
                    s.new_project_open = false;
                    cx.notify();
                })
            },
            div()
                .flex()
                .flex_col()
                .gap(t.space.lg)
                .child(switcher)
                .child(match self.tab {
                    SourceTab::Local => local_body,
                    SourceTab::Remote => remote_body,
                }),
            vec![],
        );

        super::motion::backdrop(
            div()
                .id("np-scrim")
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                // Sized to its content and hung from a fixed line near the
                // top, so switching tabs grows it downward without a jump.
                .items_start()
                .justify_center()
                .p(t.space.xl)
                .pt(relative(0.14))
                .bg(t.extra.scrim)
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    close_scrim.update(cx, |s, cx| {
                        s.new_project_open = false;
                        cx.notify();
                    })
                })
                .child(super::motion::dialog(
                    modal,
                    "new-project-dialog",
                    false,
                    &t,
                )),
            "np-scrim",
            t.extra.scrim,
            false,
            &t,
        )
    }
}
