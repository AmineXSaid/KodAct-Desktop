//! The Share card on a project page's rail (`crate::project_share`): give
//! others the same KodAct in this project, its shared instructions and its
//! knowledge (and its memory, if you choose), never your own instructions.
//!
//! - With the repo: how many setup files aren't committed yet, and Commit.
//! - As a file: Export a `.kodactproject` to Downloads; Import one here.

use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, IntoElement,
    PathPromptOptions, Render, Subscription, Window,
};

use super::*;
use crate::state::{AppState, Screen};
use crate::theme::{self, ActiveTheme};

pub struct ProjectSharing {
    state: Entity<AppState>,
    /// The memory goes with the setup.
    memory: bool,
    _subs: Vec<Subscription>,
}

impl ProjectSharing {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            _subs: vec![cx.observe(&state, |_, _, cx| cx.notify())],
            state,
            memory: false,
        }
    }

    fn import(&self, id: usize, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import setup".into()),
        });
        let state = self.state.clone();
        cx.spawn(async move |_, cx| {
            let result = picked.await;
            state.update(cx, |s, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(p) = paths.into_iter().next() {
                        s.share_import(id, p, cx)
                    }
                }
                Ok(Ok(None)) => {}
                _ => s.show_toast("The file picker couldn't open on this system.", cx),
            });
        })
        .detach();
    }
}

impl Render for ProjectSharing {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let Screen::Project(id) = s.screen else {
            return div().into_any_element();
        };
        let Some((files, uncommitted)) = s
            .project_brief
            .as_ref()
            .filter(|(b, _)| *b == id)
            .map(|(_, b)| (b.setup_files, b.setup_uncommitted))
        else {
            return div().into_any_element();
        };
        let note = |text: String| {
            div()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_body)
                .line_height(relative(1.4))
                .child(text)
        };
        let label = |text: &str| {
            div()
                .pt(t.space.xs)
                .text_size(t.text.caption)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_muted)
                .child(text.to_string())
        };
        let memory = self.memory;
        let (commit, export) = (self.state.clone(), self.state.clone());
        let card = div()
            .flex()
            .flex_col()
            .gap(t.space.sm)
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
                    .child("SHARE"),
            );
        if files == 0 {
            return card
                .child(note("Nothing to share yet: write the project's instructions or add knowledge, and anyone you share it with gets the same KodAct here.".into()))
                .child(
                    theme::ghost_button(&t, "Import a setup…")
                        .id("share-import")
                        .on_click(cx.listener(move |this, _, _, cx| this.import(id, cx))),
                )
                .into_any_element();
        }
        let repo: AnyElement = match uncommitted {
            None => note("Not a git repository: share it as a file.".into()).into_any_element(),
            Some(0) => div()
                .flex()
                .items_start()
                .gap(t.space.sm)
                .child(div().pt(px(2.)).child(icon(
                    "icons/circle-check.svg",
                    t.size.icon_sm,
                    t.extra.success,
                )))
                .child(
                    note("Committed: everyone who clones the project gets it.".into())
                        .flex_1()
                        .min_w_0(),
                )
                .into_any_element(),
            Some(n) => div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(
                    note(format!(
                        "{n} setup file{} not committed yet.",
                        if n == 1 { "" } else { "s" }
                    ))
                    .flex_1(),
                )
                .child(
                    theme::ghost_button(&t, "Commit")
                        .id("share-commit")
                        .on_click(move |_, _, cx| {
                            commit.update(cx, |s, cx| s.share_commit(id, memory, cx))
                        }),
                )
                .into_any_element(),
        };
        card.child(note(format!(
            "{files} file{} to share: the project's instructions and its knowledge. Your own instructions stay yours.",
            if files == 1 { "" } else { "s" }
        )))
        .child(label("With the repository"))
        .child(repo)
        .child(label("As a file"))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .text_size(t.text.body_sm)
                .text_color(t.colors.text)
                .child(super::button::Switch::new("share-memory", memory, "Include the memory").on_toggle({
                let me = cx.entity();
                move |v, _, cx| {
                    me.update(cx, |this, cx| {
                        this.memory = v;
                        cx.notify();
                    })
                }
            }))
                .child("Include the memory"),
        )
        .child(
            div()
                .flex()
                .gap(t.space.sm)
                .child(
                    theme::ghost_button(&t, "Export…")
                        .id("share-export")
                        .on_click(move |_, _, cx| export.update(cx, |s, cx| s.share_export(id, memory, cx))),
                )
                .child(
                    theme::ghost_button(&t, "Import…")
                        .id("share-import")
                        .on_click(cx.listener(move |this, _, _, cx| this.import(id, cx))),
                ),
        )
        .into_any_element()
    }
}
