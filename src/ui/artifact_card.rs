//! An artifact as a card, as the welcome lists them under "Recent
//! artifacts": a small page on a stage tinted by its kind, the page showing
//! how the artifact really opens (its heading and first lines, a workbook's
//! first rows as a grid), cropped at the stage's foot; under it, its title
//! and kind. A binary file's opening is read once in the background
//! (`AppState::want_artifact_snips`); a text one's is read here.

use gpui::{div, prelude::*, px, AnyElement, FontWeight, Hsla, SharedString};

use crate::office::Kind;
use crate::state::FileRef;
use crate::theme::Theme;

/// Rows a page shows at most; the stage crops the rest.
const ROWS: usize = 9;
/// A workbook's columns shown.
const COLS: usize = 4;

/// The cache key of an artifact's opening (`AppState::artifact_snips`).
pub fn key(f: &FileRef) -> String {
    format!("{}|{}", f.name, f.path.as_deref().unwrap_or(""))
}

/// The opening of a Word, PowerPoint or PDF file as lines, or of a
/// workbook's first sheet as rows of cells. Empty when it can't be read.
pub fn office_snip(kind: Kind, bytes: &[u8]) -> Vec<Vec<String>> {
    match kind {
        Kind::Xlsx => crate::office::sheets_csv(bytes, 4_000)
            .ok()
            .and_then(|(sheets, _)| sheets.into_iter().next())
            .map(|(_, csv)| {
                csv.lines()
                    .map(csv_cells)
                    .filter(|r| r.iter().any(|c| !c.is_empty()))
                    .take(ROWS)
                    .map(|r| r.into_iter().take(COLS).collect())
                    .collect()
            })
            .unwrap_or_default(),
        _ => crate::office::text(kind, bytes)
            .map(|t| text_snip(&t))
            .unwrap_or_default(),
    }
}

/// The opening of a text artifact (Markdown, a page, plain text) as lines:
/// Markdown's marks and a page's tags and styles left out.
pub fn text_snip(text: &str) -> Vec<Vec<String>> {
    let html = text.trim_start().starts_with('<');
    let mut out = vec![];
    let mut in_style = false;
    for raw in text.lines() {
        let mut line = raw.trim().to_string();
        if html {
            let lower = line.to_ascii_lowercase();
            if lower.contains("<style") || lower.contains("<script") {
                in_style = true;
            }
            if in_style {
                if lower.contains("</style") || lower.contains("</script") {
                    in_style = false;
                }
                continue;
            }
            line = strip_tags(&line);
        }
        let line = line
            .trim_start_matches(['#', '>', '-', '*', '+'])
            .trim()
            .replace("**", "")
            .replace('`', "");
        if line.is_empty() || line.chars().all(|c| matches!(c, '-' | '|' | ':' | ' ')) {
            continue;
        }
        out.push(vec![line.chars().take(120).collect()]);
        if out.len() == ROWS {
            break;
        }
    }
    out
}

fn strip_tags(line: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in line.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// One CSV line's cells, quotes honoured.
fn csv_cells(line: &str) -> Vec<String> {
    let mut cells = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => cells.push(std::mem::take(&mut cell)),
            c => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}

/// The stage's hue for a kind: Word's blue, a workbook's green, a deck's
/// coral, a PDF's red; the accent for everything else.
fn hue(f: &FileRef, t: &Theme) -> Hsla {
    match f.office() {
        Some(Kind::Docx) => t.colors.sky,
        Some(Kind::Xlsx) => t.extra.success,
        Some(Kind::Pptx) => t.colors.coral,
        Some(Kind::Pdf) => t.extra.danger,
        None => t.extra.accent,
    }
}

/// The card. `snip` is a binary artifact's opening once read (`None` or
/// empty while it's being read); a text artifact reads its own.
pub fn render(
    f: &FileRef,
    snip: Option<&[Vec<String>]>,
    id: usize,
    t: &Theme,
    scale: f32,
    on_click: impl Fn(&mut gpui::App) + 'static,
) -> AnyElement {
    let text_rows;
    let rows: &[Vec<String>] = if f.bytes.is_some() {
        snip.unwrap_or(&[])
    } else {
        text_rows = text_snip(&f.content);
        &text_rows
    };
    let sheet = f.office() == Some(Kind::Xlsx);
    let hue = hue(f, t);
    let border = t.colors.border;
    let border_hover = t.colors.text_subtle;

    let page = div()
        .absolute()
        .top(t.space.md)
        .left(t.space.lg)
        .right(t.space.lg)
        .h(px(120.))
        .rounded_t(t.radius.card)
        .border_1()
        .border_color(border)
        .bg(t.colors.surface)
        .overflow_hidden()
        .px(t.space.sm)
        .py(t.space.sm - px(2.))
        .flex()
        .flex_col()
        .gap(px(3.));
    let page = if rows.is_empty() {
        // Nothing to show yet (still being read, or unreadable): the
        // kind's own mark, centred.
        page.items_center()
            .justify_center()
            .child(mark(f, t, scale))
    } else if sheet {
        page.gap_0()
            .px_0()
            .py_0()
            .children(rows.iter().enumerate().map(|(r, cells)| {
                div()
                    .flex()
                    .border_b_1()
                    .border_color(border)
                    .children((0..COLS).map(|c| {
                        let text = cells.get(c).cloned().unwrap_or_default();
                        let numeric = text.trim().parse::<f64>().is_ok();
                        div()
                            .flex_1()
                            .min_w_0()
                            .px(px(3.))
                            .py(px(1.))
                            .when(c > 0, |d| d.border_l_1().border_color(border))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(7.))
                            .line_height(px(9.))
                            .when(numeric, |d| d.flex().justify_end())
                            .text_color(if r == 0 {
                                t.colors.text_strong
                            } else {
                                t.colors.text_body
                            })
                            .when(r == 0, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .child(text)
                    }))
            }))
    } else {
        page.children(rows.iter().enumerate().map(|(i, line)| {
            let text = line.first().cloned().unwrap_or_default();
            div()
                .w_full()
                .overflow_hidden()
                .when(i == 0, |d| {
                    d.text_size(px(9.))
                        .line_height(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .whitespace_nowrap()
                        .text_ellipsis()
                })
                .when(i > 0, |d| {
                    d.text_size(px(7.))
                        .line_height(px(9.))
                        .text_color(t.colors.text_muted)
                })
                .child(text)
        }))
    };

    let title: SharedString = f.title();
    div()
        .id(("artifact-card", id))
        .flex_1()
        .min_w_0()
        .rounded(t.radius.card)
        .border_1()
        .border_color(border)
        .bg(t.colors.surface)
        .overflow_hidden()
        .cursor_pointer()
        .hover(move |st| st.border_color(border_hover))
        .on_click(move |_, _, cx| on_click(cx))
        .child(
            // The stage: the kind's hue, washed; the page rises from it.
            div()
                .relative()
                .h(px(92.))
                .w_full()
                .overflow_hidden()
                .bg(t.extra.inset)
                .child(div().absolute().inset_0().bg(hue.opacity(0.10)))
                .child(page),
        )
        .child(
            div()
                .border_t_1()
                .border_color(border)
                .px(t.space.snug)
                .py(t.space.sm)
                .flex()
                .items_center()
                .gap(t.space.tight)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.body_sm)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child(title),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(f.kind()),
                ),
        )
        .into_any_element()
}

fn mark(f: &FileRef, t: &Theme, scale: f32) -> AnyElement {
    match f.office() {
        Some(kind) => super::file_mark::render(kind, 26., scale),
        None => gpui::img(super::file_icon::asset(
            &f.name,
            false,
            f.path.as_deref().unwrap_or(&f.name),
        ))
        .flex_none()
        .size(t.size.icon_lg)
        .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_opens_with_its_heading_and_lines_without_marks() {
        let rows =
            text_snip("# Q3 plan\n\n- **Read** the sheet\n| a | b |\n|---|---|\n`code` here\n");
        let lines: Vec<&str> = rows.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(
            lines,
            ["Q3 plan", "Read the sheet", "| a | b |", "code here"]
        );
    }

    #[test]
    fn a_page_shows_its_words_not_its_tags_or_styles() {
        let rows = text_snip(
            "<!doctype html>\n<style>\n*, *::before { box-sizing: border-box }\n</style>\n<h1>How a chat works</h1>\n<p>It starts here.</p>",
        );
        let lines: Vec<&str> = rows.iter().map(|r| r[0].as_str()).collect();
        assert_eq!(lines, ["How a chat works", "It starts here."]);
    }

    #[test]
    fn csv_cells_honour_quotes() {
        assert_eq!(
            csv_cells(r#"a,"b, c","say ""hi""",4"#),
            ["a", "b, c", r#"say "hi""#, "4"]
        );
    }

    #[test]
    fn a_workbook_opens_with_its_first_rows() {
        let xlsx = include_bytes!("../../tests/fixtures/office/hard.xlsx");
        let rows = office_snip(Kind::Xlsx, xlsx);
        assert!(!rows.is_empty() && rows.len() <= ROWS);
        assert!(rows.iter().all(|r| r.len() <= COLS));
        assert_eq!(rows[0][0], "Q3 sales summary");
    }
}
