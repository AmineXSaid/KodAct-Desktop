//! Rules and memory: what Claude Code reads in this project, and when.
//!
//! The list groups every instruction file by when it loads (every session,
//! for matching files, in a folder, or as a skill when it fits), with a rough
//! token cost. The tester answers "what loads when Claude works on this
//! file?". A file opens on the right, rendered; Edit turns it into an editor,
//! and New rule writes `.claude/rules/<name>.md`. Scans and writes run off
//! the UI thread; Reload rescans.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui::{div, prelude::*, px, AnyElement, Context, Entity, FontWeight, IntoElement, Render, SharedString, Subscription, Task, Window};

use super::markdown::{self, CopyContext};
use super::*;
use crate::rules::{self, Kind, Scan, Source, When};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

pub struct RulesView {
    state: Entity<AppState>,
    scan: Option<Rc<Scan>>,
    scanning: bool,
    /// The open file, by path, so it stays open across rescans.
    selected: Option<PathBuf>,
    tester: Entity<InputState>,
    editor: Entity<TextareaState>,
    editing: bool,
    creating: bool,
    new_name: Entity<InputState>,
    new_paths: Entity<InputState>,
    error: Option<SharedString>,
    task: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

const SECTIONS: [(&str, &str); 4] = [
    ("Every session", "Read at the start of every session, so they cost context every time."),
    ("When Claude touches matching files", "Rules with path globs. Free until Claude works on a match."),
    ("When Claude works in a folder", "A CLAUDE.md inside a subfolder."),
    ("Skills, when Claude decides they fit", "Only the description is read up front."),
];

fn section(when: &When) -> usize {
    match when {
        When::Always => 0,
        When::Paths(_) => 1,
        When::Folder(_) => 2,
        When::OnDemand => 3,
    }
}

fn kind_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Memory => "icons/file-text.svg",
        Kind::Import => "icons/at-sign.svg",
        Kind::Rule => "icons/list-todo.svg",
        Kind::Folder => "icons/folder-open.svg",
        Kind::Skill => "icons/sparkles.svg",
    }
}

/// `312`, `1.2k`.
fn tokens(n: usize) -> String {
    if n >= 1000 { format!("{:.1}k", n as f32 / 1000.) } else { n.to_string() }
}

impl RulesView {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tester = cx.new(|cx| InputState::new(window, cx).placeholder("Test a file path, e.g. src/ui/sidebar.rs"));
        let editor = cx.new(|cx| TextareaState::new(window, cx));
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. UI rules"));
        let new_paths = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. src/ui/**, *.toml"));
        let changed = |this: &mut Self, _: &Entity<InputState>, ev: &InputEvent, window: &mut Window, cx: &mut Context<Self>| match ev {
            InputEvent::Change => {
                this.error = None;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => this.create(window, cx),
            _ => {}
        };
        let subs = vec![
            cx.subscribe(&tester, |_, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&new_name, window, changed),
            cx.subscribe_in(&new_paths, window, changed),
        ];
        Self { state, scan: None, scanning: false, selected: None, tester, editor, editing: false, creating: false, new_name, new_paths, error: None, task: None, _subs: subs }
    }

    /// Scan the project again (on open, after a write, and from Reload).
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let dir = self.state.read(cx).project_dir.clone();
        self.scanning = true;
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            let scan = cx.background_executor().spawn(async move { rules::scan(&dir, rules::home_dir().as_deref()) }).await;
            this.update(cx, |v, cx| {
                if v.selected.as_ref().is_none_or(|p| !scan.sources.iter().any(|s| &s.path == p)) {
                    v.selected = scan.sources.first().map(|s| s.path.clone());
                }
                v.scan = Some(Rc::new(scan));
                v.scanning = false;
                cx.notify();
            })
            .ok();
        }));
    }

    fn source(&self) -> Option<&Source> {
        let path = self.selected.as_ref()?;
        self.scan.as_ref()?.sources.iter().find(|s| &s.path == path)
    }

    /// A path as the screen shows it: project-relative inside the project.
    fn shown(&self, path: &std::path::Path, cx: &gpui::App) -> String {
        rules::short_name(path, &self.state.read(cx).project_dir, rules::home_dir().as_deref())
    }

    fn select(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.selected = Some(path);
        self.editing = false;
        self.creating = false;
        self.error = None;
        cx.notify();
    }

    fn start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.source().map(|s| s.text.clone()) else { return };
        self.editor.update(cx, |e, cx| {
            e.set_value(text, window, cx);
            e.focus(window, cx);
        });
        self.editing = true;
        self.error = None;
        cx.notify();
    }

    /// Write the editor back to the open file (only a file the scan found).
    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.source().map(|s| s.path.clone()) else { return };
        let text = self.editor.read(cx).value().to_string();
        let shown = self.shown(&path, cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn({
                let path = path.clone();
                async move { std::fs::write(&path, text) }
            });
            let result = result.await;
            this.update(cx, |v, cx| match result {
                Ok(()) => {
                    v.editing = false;
                    v.state.update(cx, |s, cx| s.show_toast(format!("Saved {shown}."), cx));
                    v.reload(cx);
                }
                Err(e) => {
                    v.error = Some(format!("Couldn't save: {e}").into());
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    pub fn start_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.creating = true;
        self.editing = false;
        self.error = None;
        self.new_name.update(cx, |i, cx| {
            i.set_value("", window, cx);
            i.focus(window, cx);
        });
        self.new_paths.update(cx, |i, cx| i.set_value("", window, cx));
        cx.notify();
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.creating {
            return;
        }
        let name = self.new_name.read(cx).value().to_string();
        let globs = rules::parse_globs(&self.new_paths.read(cx).value());
        let project = self.state.read(cx).project_dir.clone();
        match rules::create_rule(&project, &name, &globs) {
            Ok(path) => {
                self.creating = false;
                self.selected = Some(path.clone());
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                // Start typing on the empty bullet the template ends with.
                let end = text.trim_end_matches(['\n', '\r']).len();
                self.editor.update(cx, |e, cx| {
                    e.set_value(text, window, cx);
                    e.set_selected_range(end..end, cx);
                    e.focus(window, cx);
                });
                self.editing = true;
                self.reload(cx);
            }
            Err(e) => {
                self.error = Some(e.into());
                cx.notify();
            }
        }
    }

    // ---------------------------------------------------------------- views

    fn header(&self, t: &Theme, where_: String, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let hover = t.colors.hover;
        div()
            .flex()
            .items_start()
            .gap(t.space.lg)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(t.space.xs)
                    .child(div().text_size(t.text.heading_sm).font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child("Rules and memory"))
                    .child(
                        div()
                            .font_family(t.font_serif.clone())
                            .text_size(t.text.body)
                            .text_color(t.colors.text_body)
                            .child(format!("What Claude Code reads in {where_}, and when. Files read every session cost context every time; the rest load only when they apply.")),
                    ),
            )
            .child(
                div()
                    .id("rules-reload")
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .h(t.size.control + px(4.))
                    .px(t.space.md)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .text_color(t.colors.text_muted)
                    .hover(move |st| st.bg(hover))
                    .child(icon("icons/refresh-cw.svg", t.size.icon_sm, t.colors.text_muted))
                    .child(if self.scanning { "Reading…" } else { "Reload" })
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
            .child(
                theme::ghost_button(t, "New rule")
                    .id("rules-new")
                    .on_click(cx.listener(|this, _, window, cx| this.start_create(window, cx))),
            )
    }

    fn summary(&self, t: &Theme, scan: &Scan, test: &str) -> impl IntoElement + use<> {
        let mut line = div().flex().flex_wrap().items_center().gap(t.space.sm).text_size(t.text.body_sm).text_color(t.colors.text_muted);
        if test.is_empty() {
            let counts = [
                (scan.count(|w| *w == When::Always), "always read", "always read"),
                (scan.count(|w| matches!(w, When::Paths(_))), "path rule", "path rules"),
                (scan.count(|w| matches!(w, When::Folder(_))), "folder file", "folder files"),
                (scan.count(|w| *w == When::OnDemand), "skill", "skills"),
            ];
            line = line.child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child(format!("≈{} tokens in every session", tokens(scan.always_tokens()))));
            for (n, one, many) in counts.into_iter().filter(|(n, _, _)| *n > 0) {
                line = line.child(format!("· {n} {}", if n == 1 { one } else { many }));
            }
        } else {
            let loads: Vec<&Source> = scan.sources.iter().filter(|s| s.loads_for(test)).collect();
            let cost: usize = loads.iter().map(|s| s.tokens()).sum();
            line = line
                .child("Working on")
                .child(div().font_family(t.font_mono.clone()).text_color(t.colors.text_strong).child(test.to_string()))
                .child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child(format!("loads {} file{} · ≈{} tokens", loads.len(), if loads.len() == 1 { "" } else { "s" }, tokens(cost))));
        }
        line
    }

    fn list(&self, t: &Theme, scan: &Scan, test: &str, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let hover = t.colors.hover;
        let home = rules::home_dir();
        let mut list = div().id("rules-list").w(t.size.rules_list).flex_none().h_full().overflow_y_scroll().pr(t.space.sm).flex().flex_col();
        for (n, (title, note)) in SECTIONS.iter().enumerate() {
            let items: Vec<(usize, &Source)> = scan.sources.iter().enumerate().filter(|(_, s)| section(&s.when) == n).collect();
            if items.is_empty() {
                continue;
            }
            list = list.child(
                div()
                    .pt(t.space.lg)
                    .pb(t.space.xs)
                    .px(t.space.sm)
                    .flex()
                    .flex_col()
                    .child(div().text_size(t.text.caption).font_weight(FontWeight::MEDIUM).text_color(t.colors.text_subtle).child(*title))
                    .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(*note)),
            );
            for (i, s) in items {
                let selected = self.selected.as_ref() == Some(&s.path);
                let dim = !test.is_empty() && !s.loads_for(test);
                let sub: String = match (&s.when, s.kind) {
                    (_, Kind::Import) => format!("Imported by {}", s.imported_by.as_deref().map(|p| rules::display_path(p, home.as_deref())).unwrap_or_default()),
                    (When::Always, _) => format!("{} · every session", s.scope.label()),
                    (When::Paths(globs), _) => format!("{} · {}", s.scope.label(), globs.join(", ")),
                    (When::Folder(dir), _) => format!("{} · in {dir}/", s.scope.label()),
                    (When::OnDemand, _) => s.description.clone().unwrap_or_else(|| "No description".into()),
                };
                let path = s.path.clone();
                list = list.child(
                    div()
                        .id(("rule", i))
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .px(t.space.sm)
                        .py(t.space.xs + px(2.))
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .when(dim, |d| d.opacity(0.45))
                        .when(selected, |d| d.bg(t.extra.selected).border_1().border_color(t.colors.border))
                        .when(!selected, |d| d.border_1().border_color(t.colors.canvas.opacity(0.)).hover(move |st| st.bg(hover)))
                        .child(icon(kind_icon(s.kind), t.size.icon_sm, if s.problem.is_some() { t.extra.danger } else { t.colors.text_muted }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child(s.name.clone()))
                                .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(t.text.caption).text_color(t.colors.text_subtle).child(sub)),
                        )
                        .child(div().flex_none().text_size(t.text.caption).font_family(t.font_mono.clone()).text_color(t.colors.text_subtle).child(format!("≈{}", tokens(s.tokens()))))
                        .on_click(cx.listener(move |this, _, _, cx| this.select(path.clone(), cx))),
                );
            }
        }
        list
    }

    fn detail(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        if self.creating {
            return self.create_form(t, cx).into_any_element();
        }
        let Some(s) = self.source() else {
            return div().p(t.space.xl).text_color(t.colors.text_subtle).child("Choose a file on the left.").into_any_element();
        };
        let hover = t.colors.hover;
        let (when_bg, when_fg, when_label) = match &s.when {
            When::Always => (t.colors.primary_tint, t.colors.primary, "Every session"),
            When::Paths(_) => (t.extra.success_tint, t.extra.success, "Matching files"),
            When::Folder(_) => (t.extra.success_tint, t.extra.success, "In its folder"),
            When::OnDemand => (t.colors.hover, t.colors.text_muted, "When it fits"),
        };

        let explain: AnyElement = match &s.when {
            When::Always => div().child(format!("Claude reads this at the start of every session: about {} tokens each time.", tokens(s.tokens()))).into_any_element(),
            When::Paths(globs) => div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(t.space.xs)
                .child("Claude reads this when it works on a file matching")
                .children(globs.iter().map(|g| {
                    div().px(t.space.xs + px(2.)).rounded(t.radius.small).bg(t.extra.inset).font_family(t.font_mono.clone()).text_size(t.text.caption).text_color(t.colors.text_strong).child(g.clone())
                }))
                .into_any_element(),
            When::Folder(dir) => div().child(format!("Claude reads this when it works on files in {dir}/.")).into_any_element(),
            When::OnDemand => div()
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .child("Claude reads this description every session and loads the rest when it decides the skill fits:")
                .child(div().text_color(t.colors.text_strong).child(s.description.clone().unwrap_or_else(|| "(no description, so Claude can't tell when it fits)".into())))
                .into_any_element(),
        };

        let buttons = if self.editing {
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(theme::text_button(t, "Cancel").id("rule-cancel").on_click(cx.listener(|this, _, _, cx| {
                    this.editing = false;
                    this.error = None;
                    cx.notify();
                })))
                .child(theme::primary_button(t, "Save").id("rule-save").on_click(cx.listener(|this, _, _, cx| this.save(cx))))
        } else {
            div().child(theme::ghost_button(t, "Edit").id("rule-edit").on_click(cx.listener(|this, _, window, cx| this.start_edit(window, cx))))
        };

        let copied = self.state.read(cx).copied.clone();
        let content: AnyElement = if self.editing {
            div()
                .border_1()
                .border_color(t.colors.border)
                .rounded(t.radius.button)
                .bg(t.extra.stage)
                .p(t.space.sm)
                .font_family(t.font_mono.clone())
                .text_size(t.text.body_sm - px(1.))
                .child(Textarea::new(&self.editor).appearance(false).h(t.size.editor))
                .into_any_element()
        } else if s.body.trim().is_empty() {
            div().text_color(t.colors.text_subtle).child("This file is empty.").into_any_element()
        } else {
            markdown::render(&s.body, t, &CopyContext { state: self.state.clone(), key: format!("rules-{}", s.path.display()), copied })
        };

        theme::card(t)
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(t.space.md)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(t.space.md)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(t.space.xs)
                            .child(div().text_size(t.text.subheading).font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child(s.name.clone()))
                            .child(div().font_family(t.font_mono.clone()).text_size(t.text.caption).text_color(t.colors.text_subtle).child(self.shown(&s.path, cx)))
                            .child(
                                div()
                                    .flex()
                                    .gap(t.space.xs)
                                    .pt(t.space.xs)
                                    .child(theme::pill(t, hover, t.colors.text_muted, s.scope.label()))
                                    .child(theme::pill(t, when_bg, when_fg, when_label))
                                    .when_some(s.imported_by.as_ref(), |d, from| {
                                        d.child(theme::pill(t, hover, t.colors.text_muted, format!("Imported by {}", from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())))
                                    }),
                            ),
                    )
                    .child(buttons),
            )
            .child(div().text_size(t.text.body_sm).text_color(t.colors.text_muted).child(explain))
            .when_some(s.problem.clone(), |d, p| d.child(div().px(t.space.md).py(t.space.sm).rounded(t.radius.button).bg(t.extra.danger_tint).text_color(t.extra.danger).text_size(t.text.body_sm).child(p)))
            .when_some(self.error.clone(), |d, e| d.child(div().text_color(t.extra.danger).text_size(t.text.body_sm).child(e)))
            .child(div().border_t_1().border_color(t.colors.border))
            .child(div().id("rule-body").flex_1().min_h_0().overflow_y_scroll().child(content))
            .into_any_element()
    }

    fn create_form(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let name = self.new_name.read(cx).value();
        let project = self.state.read(cx).project_dir.clone();
        let slug = rules::slug(&name);
        let file: SharedString = match &slug {
            Some(slug) => format!(".claude/rules/{slug}.md").into(),
            None => ".claude/rules/<name>.md".into(),
        };
        // Checked against the last scan, so typing does no disk I/O.
        let taken = slug.as_ref().is_some_and(|slug| {
            let path = project.join(".claude").join("rules").join(format!("{slug}.md"));
            self.scan.as_ref().is_some_and(|sc| sc.sources.iter().any(|s| s.path == path))
        });
        let ready = slug.is_some() && !taken;
        let field = |label: &'static str, hint: Option<&'static str>, input: &Entity<InputState>| {
            div()
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child(label))
                .child(
                    div()
                        .h(t.size.row + px(4.))
                        .px(t.space.md)
                        .flex()
                        .items_center()
                        .rounded(t.radius.button)
                        .bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.border)
                        .child(Input::new(input).appearance(false)),
                )
                .when_some(hint, |d, h| d.child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(h)))
        };
        theme::card(t)
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .gap(t.space.lg)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.xs)
                    .child(div().text_size(t.text.subheading).font_weight(FontWeight::SEMIBOLD).text_color(t.colors.text_strong).child("New rule"))
                    .child(div().text_color(t.colors.text_muted).flex().gap(t.space.xs).child("Saved in this project as").child(div().font_family(t.font_mono.clone()).text_color(t.colors.text_strong).child(file.clone()))),
            )
            .child(field("Name", None, &self.new_name))
            .when(taken, |d| d.child(div().mt(-t.space.md).text_size(t.text.caption).text_color(t.extra.danger).child(format!("{file} already exists. Open it from the list, or pick another name."))))
            .child(field(
                "Load when Claude touches",
                Some("Globs from the project root, separated by commas. Leave it empty to load the rule in every session."),
                &self.new_paths,
            ))
            .when_some(self.error.clone(), |d, e| d.child(div().text_color(t.extra.danger).text_size(t.text.body_sm).child(e)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(t.space.sm)
                    .child(theme::text_button(t, "Cancel").id("rule-new-cancel").on_click(cx.listener(|this, _, _, cx| {
                        this.creating = false;
                        this.error = None;
                        cx.notify();
                    })))
                    .child(
                        theme::primary_button(t, "Create")
                            .id("rule-create")
                            .when(!ready, |d| d.opacity(0.5).cursor_default())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if ready {
                                    this.create(window, cx)
                                }
                            })),
                    ),
            )
    }
}

impl Render for RulesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let where_ = rules::display_path(&s.project_dir, rules::home_dir().as_deref());
        let scan = self.scan.clone();
        let test = scan.as_ref().map(|sc| sc.relative(&self.tester.read(cx).value())).unwrap_or_default();

        let mut root = div().flex_1().min_h_0().flex().flex_col().gap(t.space.md).px(t.space.xl).pt(t.space.xl).pb(t.space.lg).bg(t.extra.stage);
        root = root.child(self.header(&t, where_.clone(), cx));

        let Some(scan) = scan else {
            return root.child(div().w(t.size.rules_list).child(super::skeleton::file_list(&t)));
        };

        root = root.child(
            div()
                .flex()
                .items_center()
                .gap(t.space.lg)
                .child(
                    div()
                        .w(t.size.rules_list)
                        .flex_none()
                        .h(t.size.row + px(4.))
                        .px(t.space.md)
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .rounded(t.radius.button)
                        .bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.border)
                        .child(icon("icons/search.svg", t.size.icon_sm, t.colors.text_subtle))
                        .child(div().flex_1().child(Input::new(&self.tester).appearance(false).cleanable(true))),
                )
                .child(self.summary(&t, &scan, &test)),
        );

        if scan.sources.is_empty() && !self.creating {
            return root.child(
                theme::card(&t)
                    .flex()
                    .flex_col()
                    .gap(t.space.sm)
                    .child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child("Claude Code has no rules for this project yet."))
                    .child(div().text_color(t.colors.text_muted).child(format!(
                        "Add a CLAUDE.md to {where_} for rules every session needs, or a rule that loads only for the files it is about."
                    ))),
            );
        }

        root.child(div().flex_1().min_h_0().flex().gap(t.space.lg).child(self.list(&t, &scan, &test, cx)).child(self.detail(&t, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::tokens;

    #[test]
    fn token_counts_read_short() {
        assert_eq!(tokens(312), "312");
        assert_eq!(tokens(1234), "1.2k");
    }
}
