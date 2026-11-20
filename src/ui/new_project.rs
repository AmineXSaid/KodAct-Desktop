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
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("owner/repo or repository URL"));
        let pat = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Optional Personal Access Token")
                .masked(true)
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
        let subs = vec![
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

fn field(t: &Theme, input: &Entity<InputState>) -> gpui::Div {
    div()
        .px(t.space.sm)
        .rounded(t.radius.field)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.canvas)
        .child(Input::new(input).appearance(false))
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
        let hover = t.colors.hover;
        let home = crate::templates::projects_home()
            .map(|h| crate::rules::display_path(&h, crate::rules::home_dir().as_deref()))
            .unwrap_or_else(|| "KodAct Projects".into());
        let picker = self.state.clone();
        let close_scrim = self.state.clone();
        let close_btn = self.state.clone();

        let seg_tab = |id: &'static str, label: &'static str, glyph: &'static str, on: bool| {
            div()
                .id(id)
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .gap(t.space.xs)
                .h(px(30.))
                .px(t.space.md)
                .rounded(t.radius.pill)
                .cursor_pointer()
                .text_size(t.text.body_sm)
                .font_weight(if on {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .when(on, |d| {
                    d.bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.border)
                })
                .text_color(if on {
                    t.colors.text_strong
                } else {
                    t.colors.text_muted
                })
                .when(!on, |d| d.hover(move |st| st.bg(hover)))
                .child(icon(
                    glyph,
                    t.size.icon_xs,
                    if on {
                        t.colors.primary
                    } else {
                        t.colors.text_subtle
                    },
                ))
                .child(label)
        };

        let switcher = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .p(px(3.))
            .rounded(t.radius.pill)
            .bg(t.extra.inset)
            .border_1()
            .border_color(t.colors.border)
            .child(
                seg_tab(
                    "np-tab-local",
                    "This Computer",
                    "icons/folder-open.svg",
                    self.tab == SourceTab::Local,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.tab = SourceTab::Local;
                    cx.notify();
                })),
            )
            .child(
                seg_tab(
                    "np-tab-remote",
                    "Remote",
                    "icons/git-branch.svg",
                    self.tab == SourceTab::Remote,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.tab = SourceTab::Remote;
                    this.url.update(cx, |i, cx| i.focus(window, cx));
                    cx.notify();
                })),
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
                        .p(t.space.md)
                        .rounded(t.radius.card)
                        .bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.border)
                        .child(
                            div()
                                .size(px(34.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(t.radius.button)
                                .bg(t.colors.primary_tint)
                                .child(icon("icons/folder-open.svg", t.size.icon, t.colors.primary)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(t.text.body_sm)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(t.colors.text_strong)
                                        .child("Open an existing folder"),
                                )
                                .child(
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .child("Keep existing files, git history, and add project instructions."),
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
                        .p(t.space.md)
                        .rounded(t.radius.card)
                        .bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.border)
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
                        .child(field_label(&t, "Project name"))
                        .child(field(&t, &self.name))
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
                                            format!("Creates an empty folder in {home}.")
                                        } else {
                                            format!(
                                                "Creates folder in {home} with KODACT.md for {}.",
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
            let hosts = div()
                .flex()
                .flex_wrap()
                .gap(t.space.xs)
                .child(
                    kit::chip(&t, "np-host-github", "GitHub", self.host_kind == ForgeKind::GitHub)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.host_kind = ForgeKind::GitHub;
                            cx.notify();
                        })),
                )
                .child(
                    kit::chip(&t, "np-host-gitlab", "GitLab", self.host_kind == ForgeKind::GitLab)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.host_kind = ForgeKind::GitLab;
                            cx.notify();
                        })),
                )
                .child(
                    kit::chip(
                        &t,
                        "np-host-custom",
                        "Custom Git Host",
                        self.host_kind == ForgeKind::Custom,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.host_kind = ForgeKind::Custom;
                        this.custom_host.update(cx, |i, cx| i.focus(window, cx));
                        cx.notify();
                    })),
                );

            let example = match self.host_kind {
                ForgeKind::GitHub => "e.g. owner/repo or https://github.com/owner/repo.git",
                ForgeKind::GitLab => {
                    "e.g. group/sub/repo, https://gitlab.com/group/repo, or self-hosted URL"
                }
                ForgeKind::Custom => {
                    "e.g. org/repo (with host domain above) or ssh://git@git.company.com/org/repo.git"
                }
            };

            div()
                .flex()
                .flex_col()
                .gap(t.space.md)
                .p(t.space.md)
                .rounded(t.radius.card)
                .bg(t.colors.surface)
                .border_1()
                .border_color(t.colors.border)
                .child(field_label(&t, "Git Provider"))
                .child(hosts)
                .when(self.host_kind != ForgeKind::GitHub, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(field_label(
                                &t,
                                if self.host_kind == ForgeKind::GitLab {
                                    "Host domain (leave blank for gitlab.com)"
                                } else {
                                    "Custom host domain (company Git server)"
                                },
                            ))
                            .child(field(&t, &self.custom_host)),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(field_label(&t, "Repository URL or path"))
                        .child(field(&t, &self.url))
                        .child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(example),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(field_label(&t, "Access Token / PAT (optional, never saved)"))
                        .child(field(&t, &self.pat)),
                )
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
                                .child(format!("Clones into {home}")),
                        )
                        .child(
                            theme::primary_button(&t, "Clone & Open")
                                .id("np-clone")
                                .flex_none()
                                .on_click(cx.listener(|this, _, _, cx| this.clone_it(cx))),
                        ),
                )
        };

        let modal = theme::popover(&t)
            .id("new-project-modal")
            .occlude()
            .w(px(580.))
            .max_w(relative(0.92))
            .max_h(relative(0.88))
            .p(t.space.xl)
            .flex()
            .flex_col()
            .gap(t.space.lg)
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(t.font_display.clone())
                            .font_weight(t.display_weight)
                            .text_size(t.text.heading_sm)
                            .text_color(t.colors.text_strong)
                            .child("Create project"),
                    )
                    .child(
                        super::button::Button::icon("np-close", "icons/x.svg", "Close")
                            .compact()
                            .on_click(move |_, _, cx| {
                                close_btn.update(cx, |s, cx| {
                                    s.new_project_open = false;
                                    cx.notify();
                                })
                            }),
                    ),
            )
            .child(switcher)
            .child(
                div()
                    .id("np-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(match self.tab {
                        SourceTab::Local => local_body,
                        SourceTab::Remote => remote_body,
                    }),
            );

        super::motion::backdrop(
            div()
                .id("np-scrim")
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p(t.space.xl)
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
