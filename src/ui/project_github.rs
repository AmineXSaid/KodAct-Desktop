//! The GitHub card on a project page's rail (`git_status.rs`): the
//! repository live, not a synced copy.
//!
//! - Where it stands: the branch (switch or make one), ahead and behind
//!   its upstream with Pull, what has changed.
//! - On GitHub: the latest workflow run, open pull requests with their
//!   checks, open issues. Each opens on GitHub, and **Ask Forge** starts a
//!   chat in the project about it: review this PR, work on this issue, fix
//!   this failing run.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Hsla, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::*;
use crate::git_status::{Checks, Repo};
use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme, Theme};

pub struct ProjectGithub {
    state: Entity<AppState>,
    /// The branch list is open, its branches, and the new branch's name.
    branching: bool,
    branches: Vec<String>,
    new_branch: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl ProjectGithub {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_branch = cx.new(|cx| InputState::new(window, cx).placeholder("New branch name"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&new_branch, window, |this, input, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    let name = input.read(cx).value().trim().to_string();
                    if name.is_empty() {
                        return;
                    }
                    input.update(cx, |i, cx| i.set_value("", window, cx));
                    this.branching = false;
                    if let Screen::Project(id) = this.state.read(cx).screen {
                        this.state.update(cx, |s, cx| s.repo_switch(id, name, true, cx));
                    }
                    cx.notify();
                }
            }),
        ];
        Self {
            state,
            branching: false,
            branches: vec![],
            new_branch,
            _subs: subs,
        }
    }

    fn open_branches(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        if self.branching {
            self.branching = false;
            return cx.notify();
        }
        self.branching = true;
        cx.spawn(async move |this, cx| {
            let list = cx
                .background_spawn(async move { crate::git_status::branches(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.branches = list;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn checks_color(c: Checks, t: &Theme) -> Hsla {
    match c {
        Checks::Passing => t.extra.success,
        Checks::Failing => t.extra.danger,
        Checks::Pending => t.extra.warning,
    }
}

fn section(t: &Theme, label: String) -> gpui::Div {
    div()
        .pt(t.space.xs)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .child(label)
}

/// One pull request or issue: its mark, number and title, a second line,
/// and on hover Open and Ask Forge.
#[allow(clippy::too_many_arguments)]
fn item(
    t: &Theme,
    key: String,
    glyph: &'static str,
    color: Hsla,
    title: String,
    detail: String,
    url: String,
    ask: String,
    project: usize,
    state: Entity<AppState>,
) -> AnyElement {
    let hover = t.colors.hover;
    let group: SharedString = format!("gh-{key}").into();
    let open = url.clone();
    div()
        .id(SharedString::from(format!("gh-item-{key}")))
        .group(group.clone())
        .relative()
        .flex()
        .items_start()
        .gap(t.space.sm)
        .px(t.space.xs)
        .py(t.space.xs)
        .rounded(t.radius.compact)
        .hover(move |st| st.bg(hover))
        .child(div().pt(px(2.)).child(icon(glyph, t.size.icon_sm, color)))
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
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text)
                        .child(title),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(detail),
                ),
        )
        // Over the row's end on hover, so titles keep the whole width.
        .child(
            div()
                .absolute()
                .top(px(2.))
                .right(px(2.))
                .flex()
                .rounded(t.radius.compact)
                .bg(t.colors.surface)
                .invisible()
                .group_hover(group, |d| d.visible())
                .child(
                    super::button::Button::icon(
                        SharedString::from(format!("gh-ask-{key}")),
                        "icons/sparkles.svg",
                        "Ask Forge about this",
                    )
                    .compact()
                    .on_click(move |_, _, cx| {
                        let text = ask.clone();
                        state.update(cx, |s, cx| s.ask_forge(project, text, cx))
                    }),
                )
                .child(
                    super::button::Button::icon(
                        SharedString::from(format!("gh-open-{key}")),
                        "icons/external-link.svg",
                        "Open on GitHub",
                    )
                    .compact()
                    .on_click(move |_, _, cx| cx.open_url(&open)),
                ),
        )
        .into_any_element()
}

impl Render for ProjectGithub {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Screen::Project(id) = s.screen else {
            return div().into_any_element();
        };
        let Some(project) = s.project(id).cloned() else {
            return div().into_any_element();
        };
        if crate::remote::Remote::parse(&project.path).is_some() {
            return div().into_any_element();
        }
        let card = div()
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .p(t.space.lg)
            .rounded(t.radius.card)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border);
        let title = |label: &str| {
            let reload = self.state.clone();
            div()
                .flex()
                .items_center()
                .child(crate::theme::section_label(&t, &label).flex_1())
                .child(
                    super::button::Button::icon("gh-refresh", "icons/refresh-cw.svg", "Read it again")
                        .compact()
                        .on_click(move |_, _, cx| reload.update(cx, |s, cx| s.load_repo(id, cx))),
                )
                .into_any_element()
        };
        let Some((_, repo)) = s.repo.as_ref().filter(|(r, _)| *r == id) else {
            return card
                .child(title("Repository"))
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_subtle)
                        .child("Reading the repository…"),
                )
                .into_any_element();
        };
        let Some(repo): Option<&Repo> = repo.as_ref().as_ref() else {
            let ask = self.state.clone();
            return card
                .child(title("Repository"))
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_subtle)
                        .child("Not a git repository yet, so there is no history to go back to."),
                )
                .child(
                    theme::ghost_button(&t, "Ask Forge to set up git")
                        .id("gh-init")
                        .on_click(move |_, _, cx| {
                            ask.update(cx, |s, cx| {
                                s.ask_forge(id, "Set this folder up as a git repository: git init, a .gitignore that fits what's here, and a first commit. Tell me what you left out and why.".into(), cx)
                            })
                        }),
                )
                .into_any_element();
        };
        let repo = repo.clone();
        let hover = t.colors.hover;

        // Where it stands.
        let changes = match (repo.changed, repo.untracked) {
            (0, 0) => "Clean".to_string(),
            (c, 0) => format!("{c} changed"),
            (0, u) => format!("{u} new"),
            (c, u) => format!("{c} changed · {u} new"),
        };
        let path = project.path.clone();
        let branch_row = div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .child(
                div()
                    .id("gh-branch")
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .min_w_0()
                    .px(t.space.sm)
                    .py(px(2.))
                    .rounded(t.radius.pill)
                    .bg(t.colors.hover)
                    .cursor_pointer()
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.caption)
                    .text_color(t.colors.text)
                    .child(icon("icons/git-branch.svg", t.size.icon_xs, t.colors.text_muted))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(repo.branch.clone()),
                    )
                    .child(icon("icons/chevron-down.svg", t.size.icon_xs, t.colors.text_subtle))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_branches(path.clone(), cx))),
            )
            .when(repo.ahead > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(icon("icons/arrow-up.svg", t.size.icon_xs, t.colors.text_muted))
                        .child(repo.ahead.to_string()),
                )
            })
            .when(repo.behind > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .text_size(t.text.caption)
                        .text_color(t.extra.warning)
                        .child(icon("icons/arrow-down.svg", t.size.icon_xs, t.extra.warning))
                        .child(repo.behind.to_string()),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(changes),
            );
        let pull = self.state.clone();
        let actions = div()
            .flex()
            .gap(t.space.sm)
            .when(repo.upstream.is_some(), |d| {
                d.child(
                    theme::ghost_button(&t, "Pull")
                        .id("gh-pull")
                        .on_click(move |_, _, cx| pull.update(cx, |s, cx| s.repo_pull(id, cx))),
                )
            })
            .when_some(repo.github.clone(), |d, (owner, name)| {
                let url = format!("https://github.com/{owner}/{name}");
                d.child(
                    div()
                        .id("gh-repo")
                        .flex()
                        .items_center()
                        .gap(t.space.xs)
                        .min_w_0()
                        .cursor_pointer()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .hover(|st| st.underline())
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(format!("{owner}/{name}")),
                        )
                        .child(icon("icons/external-link.svg", t.size.icon_xs, t.colors.text_subtle))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            });

        // The branch list: switch, or make one from here.
        let branches = self.branching.then(|| {
            let mut list = div()
                .flex()
                .flex_col()
                .gap(px(1.))
                .p(t.space.xs)
                .rounded(t.radius.field)
                .bg(t.extra.inset);
            for (i, b) in self.branches.iter().enumerate() {
                let on = *b == repo.branch;
                let (st, name) = (self.state.clone(), b.clone());
                list = list.child(
                    div()
                        .id(("gh-branch-row", i))
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .px(t.space.sm)
                        .py(px(3.))
                        .rounded(t.radius.compact)
                        .cursor_pointer()
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption)
                        .text_color(if on { t.colors.text_strong } else { t.colors.text })
                        .hover(move |s| s.bg(hover))
                        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(b.clone()))
                        .when(on, |d| d.child(icon("icons/check.svg", t.size.icon_xs, t.colors.primary)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.branching = false;
                            if !on {
                                let name = name.clone();
                                st.update(cx, |s, cx| s.repo_switch(id, name, false, cx));
                            }
                            cx.notify();
                        })),
                );
            }
            list.child(
                div()
                    .mt(t.space.xs)
                    .px(t.space.sm)
                    .rounded(t.radius.compact)
                    .border_1()
                    .border_color(t.colors.border)
                    .bg(t.colors.surface)
                    .text_size(t.text.caption)
                    .child(Input::new(&self.new_branch).appearance(false)),
            )
        });

        let mut out = card
            .child(title(if repo.github.is_some() { "GitHub" } else { "Repository" }))
            .child(branch_row)
            .children(branches)
            .child(actions);

        match &repo.hub {
            None => {}
            Some(Err(e)) => {
                out = out.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(e.clone()),
                );
            }
            Some(Ok(hub)) => {
                let (owner, name) = repo.github.clone().unwrap_or_default();
                if let Some(run) = &hub.run {
                    let c = run.checks();
                    let label = match c {
                        Checks::Passing => "passed",
                        Checks::Failing => "failed",
                        Checks::Pending => "running",
                    };
                    let (ask, url) = (self.state.clone(), run.url.clone());
                    let fix = format!(
                        "The latest CI run “{}” on {} failed ({}). Find out why (gh run view --log-failed if you can), then fix it and tell me what was wrong.",
                        run.name, run.branch, run.url
                    );
                    out = out.child(
                        div()
                            .id("gh-run")
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .text_size(t.text.caption)
                            .child(div().size(px(8.)).rounded(t.radius.pill).bg(checks_color(c, &t)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_color(t.colors.text_muted)
                                    .child(format!("{} on {} {label}", run.name, run.branch)),
                            )
                            .when(c == Checks::Failing, |d| {
                                d.child(
                                    theme::text_button(&t, "Ask Forge to fix")
                                        .id("gh-run-fix")
                                        .text_size(t.text.caption)
                                        .on_click(move |_, _, cx| {
                                            let text = fix.clone();
                                            ask.update(cx, |s, cx| s.ask_forge(id, text, cx))
                                        }),
                                )
                            })
                            .cursor_pointer()
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    );
                }
                out = out.child(section(&t, format!("Pull requests · {}", hub.prs.len())));
                if hub.prs.is_empty() {
                    out = out.child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child("None open."));
                }
                for p in &hub.prs {
                    let color = p.checks.map_or(t.colors.text_muted, |c| checks_color(c, &t));
                    let detail = format!(
                        "#{} · {}{} · {}",
                        p.number,
                        if p.draft { "draft · " } else { "" },
                        p.author,
                        p.branch
                    );
                    let ask = format!(
                        "Review pull request #{} “{}” in {owner}/{name} ({}): read it (gh pr view {} and gh pr diff {} if you can), summarise what it changes, and point out bugs or risks with file:line. Don't change anything yet.",
                        p.number, p.title, p.url, p.number, p.number
                    );
                    out = out.child(item(
                        &t,
                        format!("pr-{}", p.number),
                        "icons/git-pull-request.svg",
                        color,
                        p.title.clone(),
                        detail,
                        p.url.clone(),
                        ask,
                        id,
                        self.state.clone(),
                    ));
                }
                out = out.child(section(&t, format!("Issues · {}", hub.issues.len())));
                if hub.issues.is_empty() {
                    out = out.child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child("None open."));
                }
                for i in &hub.issues {
                    let ask = format!(
                        "Work on issue #{} “{}” in {owner}/{name} ({}): read it (gh issue view {} if you can), find the code it's about, and propose a fix before changing anything.",
                        i.number, i.title, i.url, i.number
                    );
                    out = out.child(item(
                        &t,
                        format!("issue-{}", i.number),
                        "icons/circle-dot.svg",
                        t.extra.success,
                        i.title.clone(),
                        format!("#{}", i.number),
                        i.url.clone(),
                        ask,
                        id,
                        self.state.clone(),
                    ));
                }
                out = out.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(format!("Live from {}; read again every minute while this page is open.", hub.via)),
                );
            }
        }
        out.into_any_element()
    }
}
