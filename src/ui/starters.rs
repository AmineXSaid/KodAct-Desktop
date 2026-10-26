//! Under the new-chat composer: four starter chips, each filling the
//! composer with a prompt to send or change, and a row to pick up the most
//! recent chat in this project. A plain chat (no folder) gets everyday
//! starters; a project's, starters about its code.

use gpui::{div, prelude::*, AnyElement, Entity, FontWeight, SharedString};

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

/// A plain chat's starters: no folder to read, so nothing about code you
/// have not shown it.
pub const CHAT_STARTERS: [Starter; 4] = [
    Starter {
        icon: "icons/graduation-cap.svg",
        title: "Teach me something",
        hint: "Step by step, checking as we go",
        // The teaching skill: it finds your level, teaches by example and checks
        // you can do it yourself, and stays on for the conversation.
        prompt: "/forge:expert I want to learn how this works: ",
    },
    Starter {
        icon: "icons/pencil.svg",
        title: "Help me write",
        hint: "A draft in your voice, then edits",
        prompt: "Help me write something. Ask me who it is for and what it should do, then give me a first draft I can react to. It is: ",
    },
    Starter {
        icon: "icons/code-xml.svg",
        title: "Explain some code",
        hint: "Paste it, get it line by line",
        prompt: "Explain what this code does, line by line where it matters, and point out anything risky:\n\n",
    },
    Starter {
        icon: "icons/list-todo.svg",
        title: "Make a plan",
        hint: "From a goal to the first steps",
        prompt: "Help me turn a goal into a plan: the steps in order, what each needs, and what to do first today. The goal: ",
    },
];

/// The starter chips and, when there is one, the resume row.
pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    composer: &Entity<Composer>,
    cx: &gpui::App,
) -> AnyElement {
    let s = state.read(cx);
    let hover = t.colors.hover;
    // Outlined chips, softly squared; each names what it will ask in its
    // tooltip, and fills the composer when clicked.
    let chip = |i: usize, st: &Starter| {
        let composer = composer.clone();
        let (prompt, hint) = (st.prompt, st.hint);
        let wash = super::button::Variant::Outlined.paint(t).hover_bg;
        div()
            .id(("starter", i))
            .role(gpui::accesskit::Role::Button)
            .aria_label(st.title)
            .aria_description(hint)
            .flex()
            .flex_none()
            .items_center()
            .gap(t.space.tight)
            .h(t.size.button_sm)
            .px(t.space.snug)
            .rounded(t.radius.pill)
            .text_size(t.text.caption)
            .border_1()
            .border_color(t.extra.border_strong)
            .bg(t.colors.surface)
            .cursor_pointer()
            .text_color(t.colors.text)
            .hover(move |d| d.bg(wash))
            .child(icon(st.icon, t.size.icon_sm, t.colors.text_muted))
            .child(st.title)
            .tooltip(move |window, cx| {
                gpui::component::tooltip::Tooltip::new(hint).build(window, cx)
            })
            .on_click(move |_, window, app| {
                composer.update(app, |c, cx| c.fill(prompt, window, cx))
            })
    };
    // The chips rise into place one after another as the page opens.
    let mut chips = div()
        .w_full()
        .flex()
        .flex_wrap()
        .justify_center()
        .gap(t.space.sm);
    let starters = if s.in_folder() {
        &STARTERS
    } else {
        &CHAT_STARTERS
    };
    for (i, st) in starters.iter().enumerate() {
        chips = chips.child(super::motion::row_in(
            chip(i, st),
            SharedString::from(format!("starter-in-{i}")),
            i + 2,
            t,
        ));
    }

    // The most recent chat in this project, to pick up again.
    let recent = s
        .chats
        .iter()
        .filter(|c| c.project == s.current_project && !c.messages.is_empty())
        .min_by_key(|c| c.minutes_ago);
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
            .rounded(t.radius.pill)
            .cursor_pointer()
            .text_color(t.colors.text_muted)
            .hover(move |d| d.bg(hover))
            .child(icon(
                "icons/history.svg",
                t.size.icon_sm,
                t.colors.text_subtle,
            ))
            .child(div().flex_none().child("Resume"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text)
                    .child(title),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(super::sidebar::ago(c.minutes_ago)),
            )
            .child(icon(
                "icons/arrow-right.svg",
                t.size.icon_xs,
                t.colors.text_subtle,
            ))
            .on_click(move |_, _, app| state.update(app, |s, cx| s.open_chat(id, cx)))
    });

    div()
        .w_full()
        .max_w(t.size.composer_width + t.space.xxl)
        .flex()
        .flex_col()
        .items_center()
        .gap(t.space.md)
        .child(chips)
        .children(resume)
        .into_any_element()
}
