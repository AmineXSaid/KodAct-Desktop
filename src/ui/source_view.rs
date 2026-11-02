//! A file's source, as the panel's Source view shows it (the other side of
//! Preview): highlighted, with line numbers in a gutter, lines kept whole
//! and scrolled sideways when long, as a code editor shows them. Plain text
//! (a pasted text, a note, a log) is read rather than edited, so its lines
//! wrap at the panel's edge instead, each number beside its own line.

use gpui::{div, prelude::*, px, relative, AnyElement, SharedString};

use crate::theme::Theme;

/// `lang` is the highlighter's name for it (`md`, `html`, `rs`…).
pub fn render(id: impl Into<SharedString>, text: &str, lang: &str, t: &Theme) -> AnyElement {
    let text = text.trim_end_matches('\n');
    if wraps(lang) {
        return wrapped(id, text, t);
    }
    let lines = text.lines().count().max(1);
    let line = relative(1.6);
    let numbers = (1..=lines)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    div()
        .flex()
        .items_start()
        .font_family(t.font_mono.clone())
        .text_size(t.text.body_sm)
        .line_height(line)
        .child(
            div()
                .flex_none()
                .pr(t.space.md)
                .mr(t.space.md)
                .border_r_1()
                .border_color(t.colors.border)
                .text_color(t.colors.text_subtle)
                .text_right()
                .min_w(px(28.))
                .child(numbers),
        )
        .child(
            div()
                .id(id.into())
                .flex_1()
                .min_w_0()
                .overflow_x_scroll()
                .child(
                    div()
                        .whitespace_nowrap()
                        .child(super::markdown::code_text(text, lang, t)),
                ),
        )
        .into_any_element()
}

/// Text that is read as prose: a `.txt` (pasted texts are named so), a log,
/// or a file with no extension.
pub fn wraps(lang: &str) -> bool {
    matches!(
        lang.to_ascii_lowercase().as_str(),
        "" | "txt" | "text" | "log"
    )
}

/// One row per line: its number, then the line wrapping in what is left.
fn wrapped(id: impl Into<SharedString>, text: &str, t: &Theme) -> AnyElement {
    let digits = text.lines().count().max(1).to_string().len() as f32;
    let gutter = px(28.).max(px(9. * digits));
    div()
        .id(id.into())
        .flex()
        .flex_col()
        .text_size(t.text.body_sm)
        .line_height(relative(1.6))
        .children(text.split('\n').enumerate().map(|(i, l)| {
            div()
                .flex()
                .items_start()
                .child(
                    div()
                        .flex_none()
                        .w(gutter)
                        .pr(t.space.md)
                        .mr(t.space.md)
                        .border_r_1()
                        .border_color(t.colors.border)
                        .font_family(t.font_mono.clone())
                        .text_color(t.colors.text_subtle)
                        .text_right()
                        .child((i + 1).to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(t.colors.text_body)
                        .child(if l.is_empty() {
                            " ".to_string()
                        } else {
                            l.trim_end_matches('\r').to_string()
                        }),
                )
        }))
        .into_any_element()
}

/// The highlighter's language for a file name: Markdown and pages by their
/// usual names, everything else by its extension.
pub fn lang_for(name: &str) -> &str {
    match super::markdown::lang_of(name).to_ascii_lowercase().as_str() {
        "md" | "markdown" => "markdown",
        "htm" | "html" => "html",
        "svg" => "xml",
        _ => super::markdown::lang_of(name),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn files_are_highlighted_by_what_they_are() {
        assert_eq!(super::lang_for("plan.md"), "markdown");
        assert_eq!(super::lang_for("flow.HTML"), "html");
        assert_eq!(super::lang_for("a.svg"), "xml");
        assert_eq!(super::lang_for("main.rs"), "rs");
    }

    #[test]
    fn plain_text_wraps_and_code_does_not() {
        for name in ["Pasted text 1.txt", "build.log", "NOTES"] {
            assert!(super::wraps(super::lang_for(name)), "{name}");
        }
        for name in ["plan.md", "main.rs", "flow.html"] {
            assert!(!super::wraps(super::lang_for(name)), "{name}");
        }
    }
}
