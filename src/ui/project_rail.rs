//! A project page's right rail: what every chat in the project gets, and
//! the cards that shape it.
//!
//! - **What every chat gets**: one bar of the instructions that load
//!   (`project_ctx.rs`: yours, the project's, your local ones), each part in
//!   its own colour, against the window of the model picked now; you see the
//!   cost before you type.
//! - **Instructions**: the project's own, Shared (its `FORGE.md`, kept with
//!   the folder) or Only me (`FORGE.local.md`), read and edited in place.
//!   Rules (CLAUDE.md and `.claude/rules`) stay on their own screen, a click
//!   away.
//! - **Knowledge** (`project_knowledge.rs`): reference files, Always or On
//!   demand.
//! - **Memory** (`project_memory.rs`): what Forge has learned there.
//! - **GitHub** (`project_github.rs`): the repository, live.

use gpui::component::input::TextareaState;
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, Hsla, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::*;
use crate::project_ctx::Brief;
use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme, Theme};

pub struct ProjectRail {
    state: Entity<AppState>,
    editor: Entity<TextareaState>,
    /// Which instructions the card shows: yours only, or the project's.
    mine: bool,
    /// The instructions are being edited (the editor holds them).
    editing: bool,
    /// The Knowledge and Memory cards (`project_knowledge.rs`,
    /// `project_memory.rs`).
    knowledge: Entity<super::project_knowledge::ProjectKnowledge>,
    memory: Entity<super::project_memory::ProjectMemory>,
    /// The repository card (`project_github.rs`).
    github: Entity<super::project_github::ProjectGithub>,
    _subs: Vec<Subscription>,
}

impl ProjectRail {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(
                "How Forge should work here: what the project is, its conventions, what to avoid…",
            )
        });
        let knowledge =
            cx.new(|cx| super::project_knowledge::ProjectKnowledge::new(state.clone(), window, cx));
        let memory = cx.new(|cx| super::project_memory::ProjectMemory::new(state.clone(), window, cx));
        let github = cx.new(|cx| super::project_github::ProjectGithub::new(state.clone(), window, cx));
        Self {
            _subs: vec![cx.observe(&state, |_, _, cx| cx.notify())],
            state,
            editor,
            knowledge,
            memory,
            github,
            mine: false,
            editing: false,
        }
    }

    fn edit(&mut self, brief: &Brief, window: &mut Window, cx: &mut Context<Self>) {
        let text = if self.mine { &brief.mine } else { &brief.shared };
        let text = text.trim_end().to_string();
        self.editor.update(cx, |e, cx| {
            e.set_value(text, window, cx);
            e.focus(window, cx);
        });
        self.editing = true;
        cx.notify();
    }

    fn save(&mut self, id: usize, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).value().to_string();
        let mine = self.mine;
        self.editing = false;
        self.state
            .update(cx, |s, cx| s.save_instructions(id, mine, text, cx));
        cx.notify();
    }
}

/// A card on the rail: a titled block on the surface.
fn card(t: &Theme, title: &str) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(t.space.md)
        .p(t.space.lg)
        .rounded(t.radius.card)
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .child(
            div()
                .text_size(t.text.caption)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_muted)
                .child(title.to_uppercase()),
        )
}

/// The colour a part of the brief takes in the bar and its legend.
fn part_color(kind: &str, t: &Theme) -> Hsla {
    match kind {
        "You" => t.colors.sky,
        "Local" => t.colors.marigold,
        "Knowledge" => t.extra.success,
        "Memory" => t.colors.coral,
        _ => t.colors.primary,
    }
}

/// What every chat gets: the bar, then each part with its cost.
fn budget(t: &Theme, brief: &Brief, project: &std::path::Path, window: Option<u64>) -> gpui::Div {
    let total = brief.total();
    // Against the model's window when it is known; else the parts share the bar.
    let scale = window.unwrap_or(total.max(1)).max(1) as f32;
    let mut bar = div()
        .flex()
        .w_full()
        .h(px(6.))
        .rounded(t.radius.pill)
        .overflow_hidden()
        .bg(t.colors.hover);
    for p in &brief.parts {
        // Always visible, however small next to a large window.
        let share = (p.tokens as f32 / scale).max(0.012);
        bar = bar.child(
            div()
                .h_full()
                .w(relative(share.min(1.)))
                .bg(part_color(p.kind, t)),
        );
    }
    let home = crate::rules::home_dir();
    let legend = brief.parts.iter().map(|p| {
        let name = match p.path.strip_prefix(project) {
            Ok(rel) => rel.display().to_string(),
            Err(_) => crate::rules::display_path(&p.path, home.as_deref()),
        };
        let what = match p.kind {
            "You" => "Yours, in every project",
            "Local" => "Only you, this computer",
            "Knowledge" => "Knowledge set to Always",
            "Memory" => "What Forge remembers here",
            _ => "The project's",
        };
        div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .text_size(t.text.caption)
            .child(div().size(px(8.)).rounded(t.radius.pill).bg(part_color(p.kind, t)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_color(t.colors.text).child(what))
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(t.font_mono.clone())
                            .text_color(t.colors.text_subtle)
                            .child(name),
                    ),
            )
            .child(
                div()
                    .text_color(t.colors.text_muted)
                    .child(crate::context::short(p.tokens)),
            )
    });
    let summary = match (total, window) {
        (0, _) => "Nothing yet: chats here start from Forge's own instructions.".to_string(),
        (n, Some(w)) => format!(
            "{} of {} tokens before you type ({:.1}%)",
            crate::context::short(n),
            crate::context::short(w),
            n as f32 * 100. / w as f32
        ),
        (n, None) => format!("{} tokens before you type", crate::context::short(n)),
    };
    // Without the model's window there is nothing to measure against:
    // the numbers alone.
    card(t, "What every chat gets")
        .when(window.is_some() && total > 0, |d| d.child(bar))
        .child(
            div()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_body)
                .child(summary),
        )
        .children(legend)
}

impl Render for ProjectRail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Screen::Project(id) = s.screen else {
            return div().into_any_element();
        };
        let Some(project) = s.project(id).cloned() else {
            return div().into_any_element();
        };
        let rail = div()
            .id("project-rail")
            .flex_none()
            .w(t.size.project_rail)
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(t.space.lg)
            .p(t.space.lg)
            .border_l_1()
            .border_color(t.colors.border);
        if crate::remote::Remote::parse(&project.path).is_some() {
            return rail
                .child(
                    card(&t, "Instructions").child(
                        div()
                            .text_size(t.text.body_sm)
                            .text_color(t.colors.text_body)
                            .child("This project's files are on the host: its FORGE.md is read there by Forge as each chat starts. Edit it from a chat, or in the host's editor."),
                    ),
                )
                .into_any_element();
        }
        let Some(brief) = s
            .project_brief
            .as_ref()
            .filter(|(b, _)| *b == id)
            .map(|(_, b)| b.clone())
        else {
            return rail
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_subtle)
                        .child("Reading the project…"),
                )
                .into_any_element();
        };
        let window = s
            .active_profile()
            .and_then(|p| p.context_window(s.health.get(&p.name)));
        let rules = brief.rules;

        // Instructions: Shared or Only me, shown, or edited in place.
        let mine = self.mine;
        let text = if mine { &brief.mine } else { &brief.shared };
        let file = if mine { &brief.mine_path } else { &brief.shared_path };
        let file_name = file
            .strip_prefix(&project.path)
            .map(|r| r.display().to_string())
            .unwrap_or_else(|_| file.display().to_string());
        let tab = |label: &'static str, on: bool, to_mine: bool, cx: &mut Context<Self>| {
            kit::chip(&t, SharedString::from(format!("instr-{label}")), label, on).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.mine = to_mine;
                    this.editing = false;
                    cx.notify();
                }),
            )
        };
        let tabs = div()
            .flex()
            .gap(t.space.xs)
            .child(tab("Shared", !mine, false, cx))
            .child(tab("Only me", mine, true, cx));
        let hint = if mine {
            "Only you, on this computer: FORGE.local.md stays out of the project's files you share."
        } else {
            "Kept with the project's files, so anyone with the folder or the repo gets them."
        };
        let body: AnyElement = if self.editing {
            div()
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(kit::area(&t, &self.editor, false))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(
                            div()
                                .flex_1()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(format!(
                                    "{} tokens",
                                    crate::context::short(crate::project_ctx::tokens(
                                        &self.editor.read(cx).value()
                                    ))
                                )),
                        )
                        .child(
                            theme::text_button(&t, "Cancel").id("instr-cancel").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.editing = false;
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            theme::primary_button(&t, "Save")
                                .id("instr-save")
                                .on_click(cx.listener(move |this, _, _, cx| this.save(id, cx))),
                        ),
                )
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(file_name.clone()),
                )
                .into_any_element()
        } else if text.trim().is_empty() {
            let b = brief.clone();
            div()
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_subtle)
                        .child(if mine {
                            "Nothing of yours yet. Say how you like to work here: your review style, what to ask before doing."
                        } else {
                            "No instructions yet. Tell Forge what this project is, its conventions, and what to avoid."
                        }),
                )
                .child(
                    theme::ghost_button(&t, "Write instructions")
                        .id("instr-write")
                        .on_click(cx.listener(move |this, _, window, cx| this.edit(&b, window, cx))),
                )
                .into_any_element()
        } else {
            let b = brief.clone();
            let preview: String = text.lines().take(12).collect::<Vec<_>>().join("\n");
            let more = text.lines().count() > 12;
            div()
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(
                    div()
                        .id("instr-preview")
                        .p(t.space.md)
                        .rounded(t.radius.field)
                        .bg(t.extra.inset)
                        .cursor_text()
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption)
                        .line_height(relative(1.5))
                        .text_color(t.colors.text_body)
                        .child(if more { format!("{preview}\n…") } else { preview })
                        .on_click({
                            let b = b.clone();
                            cx.listener(move |this, _, window, cx| this.edit(&b, window, cx))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(format!(
                                    "{file_name} · {} tokens",
                                    crate::context::short(crate::project_ctx::tokens(text))
                                )),
                        )
                        .child(
                            theme::ghost_button(&t, "Edit")
                                .id("instr-edit")
                                .on_click(cx.listener(move |this, _, window, cx| this.edit(&b, window, cx))),
                        ),
                )
                .into_any_element()
        };
        let instructions = card(&t, "Instructions")
            .child(tabs)
            .child(body)
            .child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .line_height(relative(1.4))
                    .child(hint),
            )
            .child(theme::rule(&t))
            .child(
                div()
                    .id("instr-rules")
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .cursor_pointer()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text)
                    .hover(|st| st.underline())
                    .child(icon("icons/list-todo.svg", t.size.icon_sm, t.colors.text_muted))
                    .child(div().flex_1().child("Rules and memory files"))
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(format!("{rules} file{}", if rules == 1 { "" } else { "s" })),
                    )
                    .child(icon("icons/chevron-right.svg", t.size.icon_xs, t.colors.text_subtle))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenRules), cx)),
            );
        rail.child(budget(&t, &brief, &project.path, window))
            .child(instructions)
            .child(self.knowledge.clone())
            .child(self.memory.clone())
            .child(self.github.clone())
            .into_any_element()
    }
}
