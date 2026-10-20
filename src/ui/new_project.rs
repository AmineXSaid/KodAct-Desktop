//! New project: three ways to start, side by side on the Projects page.
//!
//! - **Open a folder** on this computer (the system's folder picker).
//! - **Clone from GitHub**: paste the repository's address; it is cloned
//!   into `Forge Projects` and opened.
//! - **Start fresh**: a name and a template (Blank, Code, Writing,
//!   Research, Data); Forge makes the folder in `Forge Projects`, writes the
//!   template's instructions into its FORGE.md, and opens it.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::*;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

pub struct NewProject {
    state: Entity<AppState>,
    url: Entity<InputState>,
    name: Entity<InputState>,
    template: &'static str,
    _subs: Vec<Subscription>,
}

impl NewProject {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/name"));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
        let subs = vec![
            cx.subscribe(&url, |this, _, ev: &InputEvent, cx| {
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
            url,
            name,
            template: "code",
            _subs: subs,
        }
    }

    fn clone_it(&mut self, cx: &mut Context<Self>) {
        let url = self.url.read(cx).value().to_string();
        if url.trim().is_empty() {
            return;
        }
        self.state.update(cx, |s, cx| s.clone_project(url, cx));
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().to_string();
        let template = self.template;
        self.state
            .update(cx, |s, cx| s.create_project(name, template, cx));
    }
}

/// One of the three ways: an icon, a title, a line on what it does, then
/// its controls.
fn way(t: &Theme, glyph: &'static str, title: &str, line: &str) -> gpui::Div {
    div()
        .flex_1()
        .min_w(px(220.))
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
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .bg(t.colors.primary_tint)
                .child(icon(glyph, t.size.icon, t.colors.primary)),
        )
        .child(
            div()
                .pt(t.space.xs)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.colors.text_strong)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .line_height(relative(1.4))
                .child(line.to_string()),
        )
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

impl Render for NewProject {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let home = crate::templates::projects_home()
            .map(|h| crate::rules::display_path(&h, crate::rules::home_dir().as_deref()))
            .unwrap_or_else(|| "Forge Projects".into());
        let picker = self.state.clone();
        let close = self.state.clone();

        let open = way(
            &t,
            "icons/folder-open.svg",
            "Open a folder",
            "A folder on this computer: Forge works in it, with its own chats, instructions and memory.",
        )
        .child(div().flex_1())
        .child(
            theme::ghost_button(&t, "Choose a folder…")
                .id("np-open")
                .on_click(move |_, _, cx| super::projects::open_folder(picker.clone(), cx)),
        );

        let clone = way(
            &t,
            "icons/git-branch.svg",
            "Clone from GitHub",
            "Paste the repository's address. It's cloned with your git sign-in and opened.",
        )
        .child(field(&t, &self.url))
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(format!("Into {home}")),
        )
        .child(
            theme::ghost_button(&t, "Clone")
                .id("np-clone")
                .on_click(cx.listener(|this, _, _, cx| this.clone_it(cx))),
        );

        let mut chips = div().flex().flex_wrap().gap(t.space.xs);
        for tpl in crate::templates::ALL {
            let on = self.template == tpl.key;
            chips = chips.child(
                kit::chip(&t, SharedString::from(format!("np-tpl-{}", tpl.key)), tpl.label, on).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.template = tpl.key;
                        cx.notify();
                    }),
                ),
            );
        }
        let chosen = crate::templates::by_key(self.template);
        let fresh = way(
            &t,
            "icons/sparkles.svg",
            "Start fresh",
            "A new folder for any kind of work. A template gives Forge its first instructions.",
        )
        .child(field(&t, &self.name))
        .child(chips)
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(if chosen.instructions.is_empty() {
                    format!("In {home}, empty.")
                } else {
                    format!("In {home}, with a FORGE.md for {}.", chosen.label.to_lowercase())
                }),
        )
        .child(
            theme::primary_button(&t, "Create")
                .id("np-create")
                .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
        );

        div()
            .flex()
            .flex_col()
            .gap(t.space.md)
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.text_strong)
                            .child("New project"),
                    )
                    .child(
                        super::button::Button::icon("np-close", "icons/x.svg", "Close")
                            .compact()
                            .on_click(move |_, _, cx| {
                                close.update(cx, |s, cx| {
                                    s.new_project_open = false;
                                    cx.notify();
                                })
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(t.space.md)
                    .child(open)
                    .child(clone)
                    .child(fresh),
            )
    }
}
