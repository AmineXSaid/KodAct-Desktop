//! Settings → Guide: a short walkthrough of KodAct, to read top to bottom.
//! No engine needed. Its key table is the shortcuts sheet's (`shortcuts.rs`),
//! which a test keeps equal to the bindings `main.rs` makes, so the Guide
//! can't list a key that doesn't work.

use gpui::{div, prelude::*, px, relative, FontWeight, IntoElement};

use super::shortcuts;
use crate::theme::Theme;

/// (title, paragraphs).
const SECTIONS: &[(&str, &[&str])] = &[
    (
        "What KodAct is",
        &[
            "KodAct is a desktop home for KodAct CLI, an agent that reads, writes and runs code. Each chat runs its own KodAct CLI in the project you picked: it can read files, edit them and run commands, and it asks before anything risky.",
        ],
    ),
    (
        "Endpoints and models",
        &[
            "KodAct talks to a model through an endpoint: a cloud API with your key, or a model server on this PC (Ollama, LM Studio, vLLM…). Add one in Settings → Models; KodAct lists its models with your key and checks the one you pick with a short request before saving it.",
            "Keys stay in Windows Credential Manager (on other systems, a private file), never in endpoints.json. Diagnose on an endpoint checks DNS, the connection, TLS and the key, step by step.",
        ],
    ),
    (
        "Modes",
        &[
            "The mode says how much KodAct may do without asking. Manual asks before each edit; Edit automatically makes edits without asking; Plan explores and presents a plan before editing. To be taught as you go, in any mode, type /kodact:expert and what you want to learn. Shift+Tab cycles the mode inside a chat; Settings → General picks where new chats start.",
        ],
    ),
    (
        "Projects, memory and rules",
        &[
            "A project is a folder, with its own chats. KodAct reads its KODACT.md (or AGENTS.md) at the start of every chat: put how to build and test, and the conventions to keep. Settings → Memory and rules lists every file it reads.",
        ],
    ),
    (
        "In a chat",
        &[
            "Attach files with the paperclip, by pasting, or by dropping them on the box: images, PDFs, Office files and archives all go. Type / for commands and skills, @ for a file or folder.",
            "The ring by the box shows how full the context is; open it to see what fills it, and compact when it's nearly full. Each edit KodAct makes has Keep and Revert. Editing or retrying a message keeps the old conversation as a version you can restore.",
            "The task pill lists this turn's tool calls and sub-agents; the title bar shows chats waiting on your answer. Find, export and copy are in the ⋯ menu and the palette.",
        ],
    ),
    (
        "Making KodAct yours",
        &[
            "Settings → Engine edits KodAct CLI's own files, for you or for this project: slash commands (saved prompts), agents (helpers with their own tools), skills (know-how loaded when it fits), hooks (commands run at points in a chat), MCP servers (more tools) and permissions (what runs without asking). A new chat picks up a change.",
            "Profiles keep different settings side by side, such as stricter permissions for client work, and switch between them.",
        ],
    ),
    (
        "The project map and split view",
        &[
            "The map (in the project's header and the palette) shows the project's folders at a glance, with the files this chat changed. A file opened from a chat shows in the side panel; the split button puts it beside the chat or below it.",
        ],
    ),
];

pub fn render(t: &Theme) -> impl IntoElement {
    let para = |text: &str| {
        div()
            .text_size(t.text.body_sm)
            .text_color(t.colors.text)
            .line_height(relative(1.55))
            .child(text.to_string())
    };
    let mut out = div().flex().flex_col().gap(px(22.)).child(
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(t.text.subheading)
                    .font_family(t.font_display.clone())
                    .font_weight(t.display_weight)
                    .text_color(t.colors.text_strong)
                    .child("A short guide to KodAct"),
            )
            .child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child("Two minutes, top to bottom. F1 shows the keys any time."),
            ),
    );
    for (title, paras) in SECTIONS {
        let mut sec = div().flex().flex_col().gap(px(8.)).child(
            div()
                .text_size(t.text.body_sm)
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text_strong)
                .child(*title),
        );
        for p in *paras {
            sec = sec.child(para(p));
        }
        out = out.child(sec);
    }
    // The keys, from the shortcuts sheet (and so from main.rs).
    let mut keys = div().flex().flex_col().gap(px(14.)).child(
        div()
            .text_size(t.text.body_sm)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.text_strong)
            .child("Keys"),
    );
    for group in shortcuts::GROUPS {
        let mut rows = div().flex().flex_col().gap(px(6.)).child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(group),
        );
        for (binding, what, g) in shortcuts::SHORTCUTS {
            if *g != group {
                continue;
            }
            let mut caps = div().flex().flex_none().gap(px(4.));
            for k in shortcuts::keys(binding) {
                caps = caps.child(shortcuts::keycap(t, k));
            }
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(t.text.body_sm)
                            .text_color(t.colors.text)
                            .child(*what),
                    )
                    .child(caps),
            );
        }
        keys = keys.child(rows);
    }
    out.child(keys)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_guide_draws_its_keys_from_the_sheet() {
        // The sheet is tested against main.rs; the Guide must use it, not a copy.
        let src = include_str!("guide.rs");
        assert!(src.contains("shortcuts::SHORTCUTS"));
        assert!(!src.contains(concat!("\"secondary", "-")));
    }
}
