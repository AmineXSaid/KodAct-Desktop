//! Under the new-chat composer: four ways to begin, each filling the
//! composer with a prompt to send or change, and a row to pick up the most
//! recent chat in this project.

use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, SharedString};

use super::composer::Composer;
use super::icon;
use crate::state::AppState;
use crate::theme::Theme;

/// A starter: its icon, title, the line under it, and the prompt it writes.
pub struct Starter {
    pub icon: &'static str,
    pub title: &'static str,
    pub hint: &'static str,
    pub prompt: &'static str,
}

pub const STARTERS: [Starter; 4] = [
    Starter {
        icon: "icons/book-open.svg",
        title: "Explain this project",
        hint: "Entry points, modules, how they fit",
        prompt: "Read this project and explain how it is organised: the entry points, the main modules, and how they fit together. Keep it to one page.",
    },
    Starter {
        icon: "icons/bug.svg",
        title: "Find and fix a bug",
        hint: "Cause first, then a minimal fix",
        prompt: "Look for a real bug in this project. Explain its cause, then make the smallest fix and add a test that fails without it.",
    },
    Starter {
        icon: "icons/flask-conical.svg",
        title: "Write tests",
        hint: "For the least-tested code that matters",
        prompt: "Find the most important code in this project that has the fewest tests, and add focused unit tests for it. Run them.",
    },
    Starter {
        icon: "icons/wand-sparkles.svg",
        title: "Suggest improvements",
        hint: "Three refactors, smallest first",
        prompt: "Suggest the three most valuable refactors in this project, smallest first, with what each one changes. Then make the first one.",
    },
];

/// The starter cards (two by two) and, when there is one, the resume row.
pub fn render(t: &Theme, state: &Entity<AppState>, composer: &Entity<Composer>, cx: &gpui::App) -> AnyElement {
    let s = state.read(cx);
    let hover = t.colors.hover;
    let card = |i: usize, st: &Starter| {
        let composer = composer.clone();
        let prompt = st.prompt;
        div()
            .id(("starter", i))
            .flex_1()
            .min_w_0()
            .flex()
            .items_start()
            .gap(t.space.sm)
            .px(t.space.md)
            .py(t.space.sm)
            .rounded(t.radius.card)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.colors.surface)
            .cursor_pointer()
            .hover(move |d| d.bg(hover))
            .child(div().pt(px(2.)).child(icon(st.icon, t.size.icon_sm, t.colors.text_muted)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().font_weight(FontWeight::MEDIUM).text_color(t.colors.text_strong).child(st.title))
                    .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(t.text.caption).text_color(t.colors.text_subtle).child(st.hint)),
            )
            .on_click(move |_, window, app| composer.update(app, |c, cx| c.fill(prompt, window, cx)))
    };
    let row = |a: usize, b: usize| div().flex().gap(t.space.sm).child(card(a, &STARTERS[a])).child(card(b, &STARTERS[b]));

    // The most recent chat in this project, to pick up again.
    let recent = s.chats.iter().filter(|c| c.project == s.current_project && !c.messages.is_empty()).min_by_key(|c| c.minutes_ago);
    let resume = recent.map(|c| {
        let (state, id) = (state.clone(), c.id);
        let title: SharedString = c.title.clone();
        div()
            .id("resume")
            .flex()
            .items_center()
            .gap(t.space.sm)
            .px(t.space.md)
            .h(t.size.row)
            .rounded(t.radius.button)
            .cursor_pointer()
            .text_color(t.colors.text_muted)
            .hover(move |d| d.bg(hover))
            .child(icon("icons/history.svg", t.size.icon_sm, t.colors.text_subtle))
            .child(div().flex_none().child("Resume"))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().font_weight(FontWeight::MEDIUM).text_color(t.colors.text).child(title))
            .child(div().flex_none().text_size(t.text.caption).text_color(t.colors.text_subtle).child(super::sidebar::ago(c.minutes_ago)))
            .child(icon("icons/arrow-right.svg", t.size.icon_xs, t.colors.text_subtle))
            .on_click(move |_, _, app| state.update(app, |s, cx| s.open_chat(id, cx)))
    });

    div()
        .w_full()
        .max_w(t.size.composer_width)
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .child(row(0, 1))
        .child(row(2, 3))
        .children(resume)
        .into_any_element()
}
