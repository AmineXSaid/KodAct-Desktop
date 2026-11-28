//! Markdown for replies: pulldown-cmark events turned into a few block kinds,
//! each drawn with theme tokens. Inline styles become text runs (bold,
//! italic, strikethrough, links, and inline code in the mono face on a tint).
//! Links (`[text](url)`, `<url>` and bare `http(s)://` addresses) open in
//! the system browser when clicked, and show their address on hover.
//! Code blocks get a header with the language and a Copy button.
//!
//! Partial input is fine: a reply is re-parsed as it streams, and an
//! unclosed fence simply runs to the end.
//!
//! Two looks: `Chat` for replies, and `Document` for a file opened in the
//! side panel (`doc_pane.rs`): a larger reading size, more leading, and the
//! title in the display serif. Both draw tables and task lists, and
//! GitHub's callouts (`> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`,
//! `[!CAUTION]`) as tinted boxes with their kind named.

use gpui::{
    canvas, div, fill, font, point, prelude::*, px, relative, size, AnyElement, App, Bounds,
    ClipboardItem, ElementId, Entity, FontStyle, FontWeight, InteractiveText, SharedString,
    StrikethroughStyle, StyledText, TextLayout, TextRun, UnderlineStyle,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::icon;
use crate::state::AppState;
use crate::theme::Theme;

#[derive(Clone, Default, Debug, PartialEq)]
struct Span {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    /// The address, when the text is a link.
    link: Option<String>,
}

#[derive(Debug, PartialEq)]
enum Block {
    Para(Vec<Span>),
    Heading(u8, Vec<Span>),
    /// A list item; `task` is its checkbox, when it has one.
    Item {
        number: Option<u64>,
        depth: usize,
        spans: Vec<Span>,
        task: Option<bool>,
    },
    Code {
        lang: String,
        code: String,
    },
    Quote(Vec<Span>),
    /// A paragraph of a callout; `first` carries its title row.
    Alert {
        kind: Alert,
        first: bool,
        spans: Vec<Span>,
    },
    Rule,
    Table {
        align: Vec<Alignment>,
        head: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
}

/// GitHub's callout kinds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Alert {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Alert {
    fn from_kind(k: pulldown_cmark::BlockQuoteKind) -> Alert {
        use pulldown_cmark::BlockQuoteKind as K;
        match k {
            K::Note => Alert::Note,
            K::Tip => Alert::Tip,
            K::Important => Alert::Important,
            K::Warning => Alert::Warning,
            K::Caution => Alert::Caution,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Alert::Note => "Note",
            Alert::Tip => "Tip",
            Alert::Important => "Important",
            Alert::Warning => "Warning",
            Alert::Caution => "Caution",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Alert::Note | Alert::Important => "icons/info.svg",
            Alert::Tip => "icons/lightbulb.svg",
            Alert::Warning | Alert::Caution => "icons/triangle-alert.svg",
        }
    }

    /// Its colour and tint, from the theme's tokens.
    fn colors(self, t: &Theme) -> (gpui::Hsla, gpui::Hsla) {
        match self {
            Alert::Note => (t.extra.thought, t.extra.thought.opacity(0.09)),
            Alert::Important => (t.extra.accent, t.extra.accent.opacity(0.08)),
            Alert::Tip => (t.extra.success, t.extra.success_tint),
            Alert::Warning => (t.extra.warning, t.extra.warning_tint),
            Alert::Caution => (t.extra.danger, t.extra.danger_tint),
        }
    }
}

/// How a piece of markdown is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Look {
    /// A reply in the transcript.
    Chat,
    /// A document in the side panel.
    Document,
    /// Text beside the reply (the model's thinking): smaller and muted.
    Aside,
}

#[derive(Default)]
struct TableBuild {
    align: Vec<Alignment>,
    head: Vec<Vec<Span>>,
    rows: Vec<Vec<Vec<Span>>>,
    in_head: bool,
}

/// Box-drawing and block characters (`U+2500..=U+259F`) plus diagram pointer
/// triangles (`▲▼◀▶`): prose arrows (`→`, `←`, `⇒`) are excluded so bullet
/// lists and navigation paths (`Security → Your devices`) stay Markdown prose.
fn drawing_char(c: char) -> bool {
    matches!(c, '\u{2500}'..='\u{259F}' | '▲' | '▼' | '◀' | '▶')
}

/// A line that only a drawing would have: box drawing, `+--+` corners or
/// multi-character ASCII diagram connectors.
fn drawing_line(l: &str) -> bool {
    l.chars().any(drawing_char)
        || [
            "+--", "--+", "-+-", "-->", "<--", "==>", "<==", "|--", "--|", "-.->",
        ]
        .iter()
        .any(|p| l.contains(p))
}

/// Width of a leading Markdown list marker (`- `, `* `, `+ `, `1. `, `1) `)
/// on an already left-trimmed line.
fn list_marker_width(trimmed: &str) -> Option<usize> {
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        return Some(2);
    }
    let digits = trimmed.bytes().take_while(|b| b.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && (trimmed[digits..].starts_with(". ") || trimmed[digits..].starts_with(") "))
    {
        return Some(digits + 2);
    }
    None
}

/// Whether a line carries Markdown list, heading, quote or inline formatting
/// syntax that should never be auto-wrapped as raw ASCII art.
fn markdown_prose_line(l: &str) -> bool {
    let t = l.trim_start();
    list_marker_width(t).is_some()
        || t.starts_with("• ")
        || t.starts_with('#')
        || t.starts_with("> ")
        || t == ">"
        || l.contains("**")
        || l.contains('`')
        || l.contains("](")
}

/// Leading indentation width in spaces (tabs count as 4 spaces).
fn indent_cols(l: &str) -> usize {
    let mut cols = 0;
    for b in l.bytes() {
        match b {
            b' ' => cols += 1,
            b'\t' => cols += 4,
            _ => break,
        }
    }
    cols
}

fn strip_indent_cols(l: &str, mut drop_cols: usize) -> String {
    let mut idx = 0;
    let mut extra_spaces = 0usize;
    for (i, ch) in l.char_indices() {
        if drop_cols == 0 {
            break;
        }
        match ch {
            ' ' => {
                drop_cols -= 1;
                idx = i + 1;
            }
            '\t' => {
                if drop_cols >= 4 {
                    drop_cols -= 4;
                } else {
                    extra_spaces = 4 - drop_cols;
                    drop_cols = 0;
                }
                idx = i + 1;
            }
            _ => break,
        }
    }
    if extra_spaces > 0 {
        format!("{}{}", " ".repeat(extra_spaces), &l[idx..])
    } else {
        l[idx..].to_string()
    }
}

/// Models often indent bullet or numbered lists by 4 spaces after a paragraph
/// (which CommonMark otherwise turns into an indented `text` code block).
/// Dedent such list blocks outside fenced code so they render as real lists.
fn dedent_indented_lists(src: &str) -> std::borrow::Cow<'_, str> {
    let lines: Vec<&str> = src.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = false;
    let mut fence: Option<&str> = None;
    let mut parent_list_col: Option<usize> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if let Some(f) = fence {
            if trimmed.starts_with(f) {
                fence = None;
            }
            out.push(line.to_string());
            i += 1;
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(&trimmed[..3]);
            parent_list_col = None;
            out.push(line.to_string());
            i += 1;
            continue;
        }
        if trimmed.is_empty() {
            out.push(line.to_string());
            i += 1;
            continue;
        }
        let ind = indent_cols(line);
        if ind < 4 {
            if let Some(mw) = list_marker_width(trimmed) {
                parent_list_col = Some(ind + mw);
            } else if ind == 0 {
                parent_list_col = None;
            }
            out.push(line.to_string());
            i += 1;
            continue;
        }
        // Under an active list item, CommonMark already treats `ind < parent_col + 4`
        // as a nested list or item continuation; only `parent_list_col.is_none()` or
        // `ind >= parent_col + 4` would trigger `CodeBlockKind::Indented`.
        let triggers_indented_code = match parent_list_col {
            None => true,
            Some(pcol) => ind >= pcol + 4,
        };
        if triggers_indented_code && list_marker_width(trimmed).is_some() && !drawing_line(line) {
            let base_ind = ind;
            let mut j = i + 1;
            while j < lines.len() {
                let lj = lines[j];
                let tj = lj.trim_start();
                if tj.starts_with("```") || tj.starts_with("~~~") {
                    break;
                }
                if tj.is_empty() || indent_cols(lj) >= 4 {
                    j += 1;
                } else {
                    break;
                }
            }
            // Trim trailing blank lines from the span so they stay outside.
            let mut end = j;
            while end > i && lines[end - 1].trim().is_empty() {
                end -= 1;
            }
            let block = &lines[i..end];
            let looks_like_list = block.iter().all(|l| {
                let t = l.trim_start();
                if t.is_empty() {
                    return true;
                }
                if drawing_line(l) {
                    return false;
                }
                let c = indent_cols(l);
                c > base_ind || list_marker_width(t).is_some()
            });
            if looks_like_list {
                let target = parent_list_col.unwrap_or(0);
                let shift = base_ind.saturating_sub(target);
                if shift > 0 {
                    changed = true;
                    for &bl in block {
                        if bl.trim().is_empty() {
                            out.push(bl.to_string());
                        } else {
                            out.push(strip_indent_cols(bl, shift));
                        }
                    }
                    if let Some(mw) = list_marker_width(trimmed) {
                        parent_list_col = Some(target + mw);
                    }
                    i = end;
                    continue;
                }
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    if changed {
        out.join("\n").into()
    } else {
        src.into()
    }
}

/// A line laid out in columns: a drawing's line, a run of three spaces
/// inside it, or nothing but connectors (`|`, `v`, `^`…).
fn laid_out_line(l: &str) -> bool {
    let t = l.trim();
    !t.is_empty()
        && (drawing_line(l)
            || t.contains("   ")
            || t.split_whitespace().all(|w| {
                matches!(
                    w,
                    "|" | "v" | "^" | "V" | "/" | "\\" | "+" | "*" | "<" | ">"
                )
            }))
}

/// A table's `|---|:-:|` row.
fn table_rule(l: &str) -> bool {
    let t = l.trim();
    t.contains('-') && t.contains('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// ASCII drawings a reply wrote without a fence are wrapped in one, so they
/// keep their columns in the mono face instead of running together as prose.
fn fence_ascii_art(src: &str) -> std::borrow::Cow<'_, str> {
    let lines: Vec<&str> = src.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = false;
    let mut fence: Option<&str> = None;
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            out.push(lines[i].to_string());
            i += 1;
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(&t[..3]);
            out.push(lines[i].to_string());
            i += 1;
            continue;
        }
        // A paragraph: the lines up to the next blank one or fence.
        let mut j = i;
        while j < lines.len() && !lines[j].trim().is_empty() && {
            let tj = lines[j].trim_start();
            j == i || !(tj.starts_with("```") || tj.starts_with("~~~"))
        } {
            j += 1;
        }
        let para = &lines[i..j.max(i + 1)];
        let laid = para.iter().filter(|l| laid_out_line(l)).count();
        let is_art = para.len() >= 2
            && laid >= 2
            && laid * 5 >= para.len() * 3
            && para.iter().any(|l| drawing_line(l))
            && !para.iter().any(|l| table_rule(l))
            && !para.iter().any(|l| markdown_prose_line(l))
            && !para
                .iter()
                .all(|l| l.starts_with("    ") || l.starts_with('\t'));
        if is_art {
            changed = true;
            out.push("```text".into());
            out.extend(para.iter().map(|l| l.to_string()));
            out.push("```".into());
        } else {
            out.extend(para.iter().map(|l| l.to_string()));
        }
        i = j.max(i + 1);
    }
    if changed {
        out.join("\n").into()
    } else {
        src.into()
    }
}

/// A file's YAML front matter (`---` lines around it, at the very top) as a
/// `yaml` code block: Markdown would read it as a rule and one big heading.
pub fn fence_front_matter(src: &str) -> std::borrow::Cow<'_, str> {
    let Some(rest) = src
        .strip_prefix("---\n")
        .or_else(|| src.strip_prefix("---\r\n"))
    else {
        return src.into();
    };
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.trim_end(), "---" | "...") {
            let (yaml, after) = (&rest[..at], &rest[at + line.len()..]);
            return format!("```yaml\n{yaml}```\n{after}").into();
        }
        at += line.len();
    }
    src.into()
}

fn parse(src: &str) -> Vec<Block> {
    let mut out = vec![];
    let mut spans: Vec<Span> = vec![];
    let (mut bold, mut italic, mut strike) = (false, false, false);
    let mut link: Option<String> = None;
    let mut lists: Vec<Option<u64>> = vec![];
    let mut item_number: Option<Option<u64>> = None;
    let mut quote = 0usize;
    // The callout being read, and whether its first paragraph is still to come.
    let mut alert: Option<(Alert, bool)> = None;
    let mut code: Option<(String, String)> = None;
    let mut heading: Option<u8> = None;
    let mut table: Option<TableBuild> = None;
    let mut task: Option<bool> = None;

    let push = |spans: &mut Vec<Span>, text: &str, s: Span| {
        if text.is_empty() {
            return;
        }
        match spans.last_mut() {
            Some(last)
                if Span {
                    text: String::new(),
                    ..last.clone()
                } == Span {
                    text: String::new(),
                    ..s.clone()
                } =>
            {
                last.text.push_str(text)
            }
            _ => spans.push(Span {
                text: text.to_string(),
                ..s
            }),
        }
    };

    let src = fence_front_matter(src);
    let src = dedent_indented_lists(&src);
    let src = fence_ascii_art(&src);
    for ev in Parser::new_ext(
        &src,
        Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_GFM,
    ) {
        let style = Span {
            text: String::new(),
            bold,
            italic,
            code: false,
            strike,
            link: link.clone(),
        };
        match ev {
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => {
                        l.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((lang, mut c)) = code.take() {
                    if c.ends_with('\n') {
                        c.pop();
                    }
                    out.push(Block::Code { lang, code: c });
                }
            }
            Event::Text(t) if code.is_some() => code.as_mut().unwrap().1.push_str(&t),
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    _ => 3,
                })
            }
            Event::End(TagEnd::Heading(_)) => out.push(Block::Heading(
                heading.take().unwrap_or(3),
                std::mem::take(&mut spans),
            )),
            Event::Start(Tag::List(start)) => lists.push(start),
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                let n = lists.last_mut().and_then(|l| {
                    let n = *l;
                    if let Some(v) = l.as_mut() {
                        *v += 1;
                    }
                    n
                });
                if !spans.is_empty() {
                    // A nested list starts: flush the parent item's text first.
                    if let Some(num) = item_number.take() {
                        out.push(Block::Item {
                            number: num,
                            depth: lists.len().saturating_sub(2),
                            spans: std::mem::take(&mut spans),
                            task: task.take(),
                        });
                    }
                }
                item_number = Some(n);
            }
            Event::End(TagEnd::Item) => {
                if let Some(num) = item_number.take() {
                    out.push(Block::Item {
                        number: num,
                        depth: lists.len().saturating_sub(1),
                        spans: std::mem::take(&mut spans),
                        task: task.take(),
                    });
                } else {
                    spans.clear();
                }
            }
            Event::TaskListMarker(done) => task = Some(done),
            Event::Start(Tag::Table(align)) => {
                table = Some(TableBuild {
                    align,
                    ..Default::default()
                })
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.in_head = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.in_head = false;
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(t) = table.as_mut() {
                    t.rows.push(vec![]);
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(t) = table.as_mut() {
                    let cell = std::mem::take(&mut spans);
                    if t.in_head {
                        t.head.push(cell);
                    } else if let Some(row) = t.rows.last_mut() {
                        row.push(cell);
                    }
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = table.take() {
                    out.push(Block::Table {
                        align: t.align,
                        head: t.head,
                        rows: t.rows,
                    });
                }
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                quote += 1;
                if let Some(k) = kind {
                    alert = Some((Alert::from_kind(k), true));
                }
            }
            Event::End(TagEnd::BlockQuote(kind)) => {
                quote = quote.saturating_sub(1);
                if kind.is_some() {
                    alert = None;
                }
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if item_number.is_some() {
                    push(&mut spans, " ", style);
                } else if let Some((kind, first)) = alert.as_mut() {
                    out.push(Block::Alert {
                        kind: *kind,
                        first: *first,
                        spans: std::mem::take(&mut spans),
                    });
                    *first = false;
                } else if quote > 0 {
                    out.push(Block::Quote(std::mem::take(&mut spans)));
                } else {
                    out.push(Block::Para(std::mem::take(&mut spans)));
                }
            }
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Strikethrough) => strike = true,
            Event::End(TagEnd::Strikethrough) => strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => link = None,
            // Bare addresses in plain text become links too.
            Event::Text(t) if link.is_none() => {
                for (piece, url) in split_urls(&t) {
                    push(
                        &mut spans,
                        piece,
                        Span {
                            link: url,
                            ..style.clone()
                        },
                    );
                }
            }
            Event::Text(t) => push(&mut spans, &t, style),
            Event::Code(t) => push(
                &mut spans,
                &t,
                Span {
                    code: true,
                    ..style
                },
            ),
            Event::SoftBreak => push(&mut spans, " ", style),
            Event::HardBreak => push(&mut spans, "\n", style),
            Event::Rule => out.push(Block::Rule),
            _ => {}
        }
    }
    if let Some((lang, c)) = code {
        out.push(Block::Code { lang, code: c });
    }
    if !spans.is_empty() {
        out.push(Block::Para(spans));
    }
    out
}

/// What a code block's Copy button needs.
pub struct CopyContext {
    pub state: Entity<AppState>,
    /// Unique per message: `<chat>-<message>-<block>`.
    pub key: String,
    pub copied: Option<SharedString>,
    /// Device pixels per logical pixel, for diagrams drawn to exact size.
    pub scale: f32,
    /// Diagrams showing their source instead of the drawing (`AppState::diagram_code`).
    pub sources: std::sync::Arc<std::collections::HashSet<SharedString>>,
    /// Diagrams zoomed in or out (`AppState::diagram_zoom`).
    pub zooms: std::sync::Arc<std::collections::HashMap<SharedString, f32>>,
    /// The reply is still arriving: its newest words fade in at the end.
    pub streaming: bool,
}

fn runs(
    id: ElementId,
    spans: &[Span],
    t: &Theme,
    base_weight: FontWeight,
    color: gpui::Hsla,
) -> AnyElement {
    runs_in(id, spans, t, &t.font_reading, base_weight, color)
}

/// Text runs in `family` (code spans stay in the mono face).
fn runs_in(
    id: ElementId,
    spans: &[Span],
    t: &Theme,
    family: &SharedString,
    base_weight: FontWeight,
    color: gpui::Hsla,
) -> AnyElement {
    let (text, runs) = run_list(spans, t, family, base_weight, color);
    linked(id, spans, text, runs, t)
}

/// Your message, selectable to copy part of it, styled as it was in the
/// box (`compose_style`): its Markdown, and its mentions bold, a skill in the
/// skills' colour, a command or file in the accent.
pub fn selectable_plain(
    key: SharedString,
    text: &SharedString,
    marks: &[(std::ops::Range<usize>, super::autocomplete::Mark)],
    t: &Theme,
) -> AnyElement {
    let run = |len: usize, h: Option<gpui::HighlightStyle>| {
        let h = h.unwrap_or_default();
        let mut font = gpui::font(t.font_sans.clone());
        if let Some(w) = h.font_weight {
            font.weight = w;
        }
        if let Some(st) = h.font_style {
            font.style = st;
        }
        TextRun {
            len,
            font,
            color: h.color.unwrap_or(t.colors.text),
            background_color: h.background_color,
            underline: h.underline,
            strikethrough: h.strikethrough,
        }
    };
    let mut runs = vec![];
    let mut at = 0;
    for (r, h) in super::compose_style::styles(text, marks, t) {
        if r.start > at {
            runs.push(run(r.start - at, None));
        }
        runs.push(run(r.len(), Some(h)));
        at = r.end;
    }
    if at < text.len() || runs.is_empty() {
        runs.push(run(text.len() - at, None));
    }
    let runs = super::select::prepare(&key, text, runs, t.colors.primary_tint);
    let styled = StyledText::new(text.clone()).with_runs(runs);
    let layout = styled.layout().clone();
    super::select::wrap(key, layout, styled.into_any_element())
}

/// The text, with its links clickable (opened in the system browser) and
/// their address shown on hover; plain text when it has none. Either way
/// it can be selected to copy part of it (`select.rs`).
fn linked(
    id: ElementId,
    spans: &[Span],
    text: String,
    runs: Vec<TextRun>,
    t: &Theme,
) -> AnyElement {
    let key = SharedString::from(format!("{id:?}"));
    let runs = super::select::prepare(&key, &text, runs, t.colors.primary_tint);
    let text = StyledText::new(text).with_runs(runs);
    let layout = text.layout().clone();
    let code = code_ranges(spans);
    let files = file_chips(spans);
    let el = linked_text(id, spans, text);
    let el = if code.is_empty() {
        el
    } else {
        // The chips paint first, so the text sits on them; a file's icon
        // last, in the room kept before its name.
        div()
            .relative()
            .child(chips(layout.clone(), code, t))
            .child(el)
            .when(!files.is_empty(), |d| {
                d.child(file_icons(layout.clone(), files, chip_icon(t)))
            })
            .into_any_element()
    };
    super::select::wrap(key, layout, el)
}

/// How far an inline code chip reaches past its first and last glyph. No
/// padding characters: the wrapper breaks at any space (a no-break one
/// too), so padding made stray gaps before punctuation and empty rows.
const CODE_REACH: f32 = 3.0;

/// The room a file chip keeps for its icon: one em space before the name.
/// The wrapper may break before it (not a word character) but never
/// between it and the name, so the icon always travels with its file.
/// Copying drops it (`select::selected_text`).
pub const ICON_GAP: char = '\u{2003}';

/// A file chip's icon size, from the icon scale.
fn chip_icon(t: &Theme) -> gpui::Pixels {
    t.size.icon_xs
}

/// Whether inline code names a file: one word, with a name or extension
/// the file icons know (`verify.rs`, `src/ui/markdown.rs`, `Cargo.toml`).
fn file_like(code: &str) -> bool {
    use super::file_icon_map::{EXTENSIONS, NAMES};
    if code.is_empty()
        || code.len() > 160
        || code.contains(char::is_whitespace)
        || code.contains("://")
        || code.starts_with('-')
    {
        return false;
    }
    let base = code
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(code)
        .to_lowercase();
    let known = |table: &[(&str, &str)], key: &str| {
        table.binary_search_by(|(k, _)| (*k).cmp(key)).is_ok()
    };
    if known(NAMES, &base) {
        return true;
    }
    match base.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty()
                && !ext.is_empty()
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && !ext.chars().all(|c| c.is_ascii_digit())
                && known(EXTENSIONS, ext)
        }
        None => false,
    }
}

/// A span's text as it is laid out: exactly its text, a file name with
/// room for its icon before it.
fn shown(s: &Span) -> std::borrow::Cow<'_, str> {
    if s.code && file_like(&s.text) {
        format!("{ICON_GAP}{}", s.text).into()
    } else {
        s.text.as_str().into()
    }
}

/// The file chips in the laid-out text: where each sits, and its name.
fn file_chips(spans: &[Span]) -> Vec<(std::ops::Range<usize>, String)> {
    let mut out = vec![];
    let mut at = 0;
    for s in spans.iter().filter(|s| !s.text.is_empty()) {
        let len = shown(s).len();
        if s.code && file_like(&s.text) {
            out.push((at..at + len, s.text.clone()));
        }
        at += len;
    }
    out
}

/// Where the code spans sit in the laid-out text.
fn code_ranges(spans: &[Span]) -> Vec<std::ops::Range<usize>> {
    let mut out = vec![];
    let mut at = 0;
    for s in spans.iter().filter(|s| !s.text.is_empty()) {
        let len = shown(s).len();
        if s.code {
            out.push(at..at + len);
        }
        at += len;
    }
    out
}

/// Rounded grounds behind inline code, one per row a span wraps onto: text
/// runs can only fill square, so these are painted from the laid-out
/// glyph positions, under the text.
fn chips(layout: TextLayout, ranges: Vec<std::ops::Range<usize>>, t: &Theme) -> impl IntoElement {
    let (bg, radius) = (t.extra.code_inline_bg, t.radius.small);
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let line = layout.line_height();
            let text = layout.text();
            for r in &ranges {
                for b in rows(&layout, &text, r.clone(), line) {
                    // A soft pill wash, no edge, reaching slightly past the glyphs.
                    let b = Bounds::new(
                        point(b.origin.x - px(CODE_REACH), b.origin.y),
                        size(b.size.width + px(CODE_REACH * 2.), b.size.height),
                    );
                    window.paint_quad(fill(b, bg).corner_radii(radius));
                }
            }
        },
    )
    .absolute()
    .size_full()
}

/// The file chips' icons (the file's own, full colour), each placed in the
/// room its chip keeps before the name, on whichever row the name landed.
/// Laid out once the text is (this sits after it), so the glyphs' places
/// are known.
fn file_icons(
    layout: TextLayout,
    files: Vec<(std::ops::Range<usize>, String)>,
    side: gpui::Pixels,
) -> impl IntoElement {
    canvas(
        move |_, window, cx| {
            let line = layout.line_height();
            let text = layout.text();
            let mut out = vec![];
            for (r, name) in &files {
                let Some(first) = rows(&layout, &text, r.clone(), line).into_iter().next() else {
                    continue;
                };
                let at = point(
                    first.origin.x,
                    first.origin.y + (first.size.height - side) / 2.,
                );
                let mut el = gpui::img(super::file_icon::asset(name, false, name))
                    .size(side)
                    .into_any_element();
                el.prepaint_as_root(at, size(side, side).map(Into::into), window, cx);
                out.push(el);
            }
            out
        },
        |_, mut icons, window, cx| {
            for el in &mut icons {
                el.paint(window, cx);
            }
        },
    )
    .absolute()
    .size_full()
}

/// Group per-character `(ch, start_pos, end_pos)` pairs into `(x0, y, x1)` ink
/// spans per visual line.
///
/// In GPUI's `WrappedLineLayout::position_for_index`, a soft wrap right before
/// `char_start` returns `p0` at the end of the previous row (`p0.y`) while
/// `char_end` advances to the new row (`p1.y`, where `char_start` starts at
/// `origin_x`). Comparing `p0.y` and `p1.y` places wrapped glyphs cleanly on
/// `p1.y` from `origin_x..p1.x` and never paints a phantom box on `p0.y`.
///
/// A span on more than one row keeps only rows with real ink: a sliver
/// narrower than `min_w` (a glyph's edge left at a row's end, as Windows'
/// layout reports when the span wraps whole) would paint as an empty chip.
fn glyph_rows(
    origin_x: f32,
    min_w: f32,
    glyphs: &[(char, (f32, f32), (f32, f32))],
) -> Vec<(f32, f32, f32)> {
    let mut out: Vec<(f32, f32, f32)> = Vec::new();
    let mut cur: Option<(f32, f32, f32)> = None;
    for &(ch, p0, p1) in glyphs {
        let (y, x0, x1) = if (p0.1 - p1.1).abs() < 0.5 {
            (p0.1, p0.0, p1.0)
        } else {
            (p1.1, origin_x.min(p1.0), p1.0)
        };
        if ch.is_whitespace() {
            if let Some((cy, rx0, rx1)) = cur {
                if (cy - y).abs() >= 0.5 {
                    if rx1 > rx0 {
                        out.push((rx0, cy, rx1));
                    }
                    cur = None;
                }
            }
            continue;
        }
        if x1 <= x0 {
            continue;
        }
        match cur {
            Some((cy, rx0, rx1)) if (cy - y).abs() < 0.5 => {
                cur = Some((cy, rx0.min(x0), rx1.max(x1)));
            }
            Some((cy, rx0, rx1)) => {
                if rx1 > rx0 {
                    out.push((rx0, cy, rx1));
                }
                cur = Some((y, x0, x1));
            }
            None => {
                cur = Some((y, x0, x1));
            }
        }
    }
    if let Some((cy, rx0, rx1)) = cur {
        if rx1 > rx0 {
            out.push((rx0, cy, rx1));
        }
    }
    if out.len() > 1 {
        out.retain(|&(x0, _, x1)| x1 - x0 >= min_w);
    }
    out
}

/// The bounds a range covers on each row it wraps onto, trimmed to sit
/// inside the line's height.
fn rows(
    layout: &TextLayout,
    text: &str,
    r: std::ops::Range<usize>,
    line: gpui::Pixels,
) -> Vec<Bounds<gpui::Pixels>> {
    let Some(slice) = text.get(r.clone()) else {
        return vec![];
    };
    let origin_x = f32::from(layout.bounds().origin.x);
    let mut glyphs: Vec<(char, (f32, f32), (f32, f32))> = Vec::new();
    for (rel, ch) in slice.char_indices() {
        let start = r.start + rel;
        let end = start + ch.len_utf8();
        let (Some(p0), Some(p1)) = (
            layout.position_for_index(start),
            layout.position_for_index(end),
        ) else {
            continue;
        };
        // A file chip's icon room is ink, not a space to skip.
        let ch = if ch == ICON_GAP { 'I' } else { ch };
        glyphs.push((
            ch,
            (f32::from(p0.x), f32::from(p0.y)),
            (f32::from(p1.x), f32::from(p1.y)),
        ));
    }
    let inset = line * 0.13;
    // The narrowest real glyph is a fifth of the line or more (a full stop).
    let min_w = f32::from(line) * 0.18;
    glyph_rows(origin_x, min_w, &glyphs)
        .into_iter()
        .map(|(x0, y, x1)| {
            Bounds::new(
                point(px(x0), px(y) + inset),
                size(px(x1 - x0), line - inset * 2.),
            )
        })
        .collect()
}

/// What a clickable stretch of a reply opens.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    /// A link: its address, opened in the system browser.
    Link(String),
    /// A file chip: the file's name or path, opened from the chat's folder.
    File(String),
}

/// The clickable stretches of laid-out text: its links (neighbouring spans
/// of one link joined) and its file chips.
fn targets(spans: &[Span]) -> Vec<(std::ops::Range<usize>, Target)> {
    let mut out: Vec<(std::ops::Range<usize>, Target)> = vec![];
    let mut at = 0;
    for s in spans.iter().filter(|s| !s.text.is_empty()) {
        let len = shown(s).len();
        if let Some(url) = &s.link {
            // Neighbouring spans of one link (bold inside it) join up.
            match out.last_mut() {
                Some((r, Target::Link(u))) if u == url && r.end == at => r.end = at + len,
                _ => out.push((at..at + len, Target::Link(url.clone()))),
            }
        } else if s.code && file_like(&s.text) {
            out.push((at..at + len, Target::File(s.text.clone())));
        }
        at += len;
    }
    out
}

fn linked_text(id: ElementId, spans: &[Span], text: StyledText) -> AnyElement {
    let found = targets(spans);
    if found.is_empty() {
        return text.into_any_element();
    }
    let ranges: Vec<_> = found.iter().map(|(r, _)| r.clone()).collect();
    let found = std::rc::Rc::new(found);
    let (open, tip) = (found.clone(), found);
    InteractiveText::new(id, text)
        .on_click(ranges, move |i, window, cx| match &open[i].1 {
            Target::Link(url) => open_link(url, cx),
            Target::File(name) => window.dispatch_action(
                Box::new(super::OpenFileChip {
                    name: name.clone().into(),
                }),
                cx,
            ),
        })
        .tooltip(move |ix, window, cx| {
            let (_, target) = tip.iter().find(|(r, _)| r.contains(&ix))?;
            let words = match target {
                Target::Link(url) => url.clone(),
                Target::File(name) => format!("Open {name}"),
            };
            Some(gpui::component::tooltip::Tooltip::new(words).build(window, cx))
        })
        .into_any_element()
}

/// Open a link from a reply: web and mail addresses go to the system's
/// handler; anything else (a relative path, `javascript:`) is left alone.
fn open_link(url: &str, cx: &mut App) {
    if openable(url) {
        cx.open_url(url);
    }
}

fn openable(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// Text cut into plain pieces and bare `http(s)://` addresses, each address
/// with itself as its link. Closing punctuation after an address stays text.
fn split_urls(text: &str) -> Vec<(&str, Option<String>)> {
    let mut out = vec![];
    let mut rest = text;
    loop {
        let start = ["http://", "https://"]
            .iter()
            .filter_map(|p| rest.find(p))
            .filter(|&i| i == 0 || !rest[..i].ends_with(|c: char| c.is_alphanumeric()))
            .min();
        let Some(start) = start else { break };
        let tail = &rest[start..];
        let mut end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`'))
            .unwrap_or(tail.len());
        // A sentence's full stop, a closing bracket nobody opened.
        while end > 0 {
            let c = tail[..end].chars().next_back().unwrap_or(' ');
            let open = |o: char| tail[..end].matches(o).count();
            let unbalanced =
                (c == ')' && open('(') < open(')')) || (c == ']' && open('[') < open(']'));
            if matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*') || unbalanced {
                end -= c.len_utf8();
            } else {
                break;
            }
        }
        let url = &tail[..end];
        if url.len() <= "https://".len() {
            // "http://" on its own: plain text.
            out.push((&rest[..start + end.max(1)], None));
            rest = &rest[start + end.max(1)..];
            continue;
        }
        if start > 0 {
            out.push((&rest[..start], None));
        }
        out.push((url, Some(url.to_string())));
        rest = &tail[end..];
    }
    if !rest.is_empty() {
        out.push((rest, None));
    }
    out
}

/// The text of `spans` and its styled runs.
fn run_list(
    spans: &[Span],
    t: &Theme,
    family: &SharedString,
    base_weight: FontWeight,
    color: gpui::Hsla,
) -> (String, Vec<TextRun>) {
    let mut text = String::new();
    let mut runs = vec![];
    for s in spans {
        if s.text.is_empty() {
            continue;
        }
        let shown = shown(s);
        text.push_str(&shown);
        let mut f = font(if s.code {
            t.font_code_inline.clone()
        } else {
            family.clone()
        });
        // Code keeps the mono face's regular weight, in bold text too: the
        // mono faces ship Regular and Bold only, and bold mono shouts.
        f.weight = if s.code {
            FontWeight::NORMAL
        } else if s.bold {
            FontWeight::SEMIBOLD
        } else {
            base_weight
        };
        if s.italic {
            f.style = FontStyle::Italic;
        }
        runs.push(TextRun {
            len: shown.len(),
            font: f,
            // A link in burgundy ink; inline code (a file, a command) in
            // its sky ink on a soft pill (painted by `chips`): a name set
            // apart at a glance.
            color: if s.link.is_some() {
                t.extra.reply_link
            } else if s.code {
                t.extra.code_inline_fg
            } else {
                color
            },
            background_color: None,
            underline: s.link.is_some().then(|| UnderlineStyle {
                thickness: px(1.),
                color: Some(t.extra.reply_link.opacity(0.45)),
                wavy: false,
            }),
            strikethrough: s.strike.then(|| StrikethroughStyle {
                thickness: px(1.),
                color: Some(color),
            }),
        });
    }
    (text, runs)
}

/// How many characters at the end of a reply still arriving fade in.
const TAIL_CHARS: usize = 24;

/// A paragraph still arriving: its last few words fade toward the end, so
/// new text eases in rather than popping onto the line.
fn runs_streaming(id: ElementId, spans: &[Span], t: &Theme, color: gpui::Hsla) -> AnyElement {
    let (text, runs) = run_list(spans, t, &t.font_reading, FontWeight::NORMAL, color);
    let runs = fade_tail(&text, runs, TAIL_CHARS);
    linked(id, spans, text, runs, t)
}

/// Splits `runs` so the last `n` characters of `text` step down in opacity
/// toward the end: whole near the start of the tail, about a third at its
/// end, still legible.
fn fade_tail(text: &str, runs: Vec<TextRun>, n: usize) -> Vec<TextRun> {
    let chars = text.chars().count();
    if chars == 0 || n == 0 {
        return runs;
    }
    let start_char = chars.saturating_sub(n);
    // Byte offset of every character from the tail's start, plus the end.
    let mut cuts: Vec<usize> = text
        .char_indices()
        .skip(start_char)
        .map(|(i, _)| i)
        .collect();
    cuts.push(text.len());
    let tail = cuts.len() - 1;
    let tail_start = cuts[0];
    let mut out = vec![];
    let mut pos = 0;
    for run in runs {
        let (lo, hi) = (pos, pos + run.len);
        pos = hi;
        if hi <= tail_start {
            out.push(run);
            continue;
        }
        if lo < tail_start {
            out.push(TextRun {
                len: tail_start - lo,
                ..run.clone()
            });
        }
        // One run per character in the tail, each a little fainter.
        for (k, w) in cuts.windows(2).enumerate() {
            let (a, b) = (w[0].max(lo), w[1].min(hi));
            if a >= b {
                continue;
            }
            let fade = 1. - 0.65 * (k + 1) as f32 / tail as f32;
            let mut r = run.clone();
            r.len = b - a;
            r.color = r.color.opacity(fade);
            out.push(r);
        }
    }
    out
}

pub fn render(src: &str, t: &Theme, copy: &CopyContext) -> AnyElement {
    render_as(src, t, copy, Look::Chat)
}

pub fn render_as(src: &str, t: &Theme, copy: &CopyContext, look: Look) -> AnyElement {
    let doc = look == Look::Document;
    let aside = look == Look::Aside;
    let blocks = parse(src);
    let (size, leading) = match look {
        Look::Document => (t.size.doc_text, t.size.line_height_doc),
        Look::Aside => (t.text.body_sm, t.size.line_height_chat),
        Look::Chat => (t.size.chat_text, t.size.line_height_chat),
    };
    // Prose in the warm body ink, a shade under headings; an aside quieter.
    let prose = if aside {
        t.colors.text_muted
    } else {
        t.colors.text_body
    };
    let mut col = div()
        .flex()
        .flex_col()
        .w_full()
        .text_size(size)
        .line_height(relative(leading));
    let mut prev: Option<&'static str> = None;
    let last = blocks.len().saturating_sub(1);
    for (n, b) in blocks.into_iter().enumerate() {
        // Space above a block depends on what it follows: list items sit
        // close together, headings and code get room above them.
        let kind = match &b {
            Block::Item { .. } => "item",
            Block::Heading(..) => "heading",
            Block::Code { .. } | Block::Table { .. } => "code",
            _ => "text",
        };
        let gap = match (prev, kind, doc) {
            (None, _, _) => px(0.),
            (Some("item"), "item", false) => t.space.xs,
            (Some("item"), "item", true) => px(6.),
            (_, "heading", false) => t.space.lg + t.space.xs,
            (_, "heading", true) => t.space.xl + t.space.xs,
            (Some("heading"), _, false) => t.space.sm,
            (Some("heading"), _, true) => t.space.sm + px(2.),
            (_, "code", false) | (Some("code"), _, false) => t.space.md,
            (_, "code", true) | (Some("code"), _, true) => t.space.lg,
            // Paragraphs breathe apart, as a page's do.
            (_, _, false) => t.space.md,
            (_, _, true) => t.space.md,
        };
        prev = Some(kind);
        let id = ElementId::Name(format!("md-{}-{n}", copy.key).into());
        let el: AnyElement = match b {
            Block::Para(spans) if copy.streaming && n == last => div()
                .child(runs_streaming(id, &spans, t, prose))
                .into_any_element(),
            Block::Para(spans) => div()
                .child(runs(id, &spans, t, FontWeight::NORMAL, prose))
                .into_any_element(),
            // A document's title is set in the display serif, as the greeting is.
            Block::Heading(1, spans) if doc => div()
                .text_size(t.size.doc_title)
                .line_height(relative(1.2))
                .child(runs_in(
                    id,
                    &spans,
                    t,
                    &t.font_display,
                    t.display_weight,
                    t.colors.text_strong,
                ))
                .into_any_element(),
            Block::Heading(level, spans) => div()
                .text_size(match (level, doc) {
                    // A step above the 14px body each, on whole pixels.
                    (1, false) => t.size.chat_text + px(3.),
                    (2, false) => t.size.chat_text + px(2.),
                    (_, false) => t.size.chat_text + px(1.),
                    (2, true) => t.text.subheading,
                    (_, true) => t.text.body,
                })
                .line_height(relative(1.35))
                .child(runs(
                    id,
                    &spans,
                    t,
                    // The top two levels a weight heavier: one clear step.
                    if level <= 2 {
                        FontWeight::BOLD
                    } else {
                        FontWeight::SEMIBOLD
                    },
                    t.colors.text_strong,
                ))
                // A section of a reply opens on a short burgundy rule.
                .when(level <= 2 && !doc && !aside, |d| {
                    d.child(
                        div()
                            .mt(t.space.xs)
                            .w(t.space.xl)
                            .h(px(2.))
                            .rounded(t.radius.line)
                            .bg(t.extra.accent),
                    )
                })
                .into_any_element(),
            Block::Item {
                number,
                depth,
                spans,
                task,
            } => div()
                .flex()
                .gap(t.space.sm)
                .pl(t.space.xs + t.space.lg * depth as f32)
                .child(match task {
                    // A checkbox, centred on the first line.
                    Some(done) => div()
                        .flex_none()
                        .h(relative(1.))
                        .min_w(t.space.lg)
                        .pt((size * leading - t.size.icon_sm) / 2.)
                        .child(icon(
                            if done {
                                "icons/square-check.svg"
                            } else {
                                "icons/square.svg"
                            },
                            t.size.icon_sm,
                            if done {
                                t.colors.primary
                            } else {
                                t.colors.text_subtle
                            },
                        ))
                        .into_any_element(),
                    // Step numbers carry meaning, so they read at 4.5:1.
                    None => match number {
                        Some(n) => div()
                            .flex_none()
                            .min_w(t.space.lg)
                            .text_color(t.colors.text_muted)
                            .child(format!("{n}."))
                            .into_any_element(),
                        // A bullet per level, as a written outline: a dot, a
                        // ring, a dash. Drawn, not typed, so every face
                        // shows it round (some draw • square).
                        None => {
                            let ink = t.colors.text_muted;
                            let mark = match depth.min(2) {
                                0 => div().size(px(5.)).rounded(px(2.5)).bg(ink),
                                1 => div()
                                    .size(px(6.))
                                    .rounded(px(3.))
                                    .border_1()
                                    .border_color(ink),
                                _ => div().w(px(6.)).h(px(1.5)).rounded(px(1.)).bg(ink),
                            };
                            div()
                                .flex_none()
                                .w(t.space.lg)
                                .h(px(f32::from(size) * leading))
                                .flex()
                                .items_center()
                                .child(mark)
                                .into_any_element()
                        }
                    },
                })
                .child(div().flex_1().min_w_0().child(runs(
                    id,
                    &spans,
                    t,
                    FontWeight::NORMAL,
                    if task == Some(true) {
                        t.colors.text_muted
                    } else {
                        prose
                    },
                )))
                .into_any_element(),
            Block::Quote(spans) => div()
                .border_l_2()
                .border_color(t.colors.border)
                .pl(t.space.md)
                .child(runs(id, &spans, t, FontWeight::NORMAL, prose))
                .into_any_element(),
            Block::Alert { kind, first, spans } => {
                let (color, tint) = kind.colors(t);
                let mut b = div()
                    .flex()
                    .flex_col()
                    .gap(t.space.xs)
                    .border_l(px(3.))
                    .border_color(color)
                    .bg(tint)
                    .rounded_r(t.radius.button)
                    .px(t.space.md)
                    .py(t.space.sm);
                if first {
                    b = b.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .child(icon(kind.icon(), t.size.icon_sm, color))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color)
                                    .child(kind.title()),
                            ),
                    );
                }
                b.child(runs(id, &spans, t, FontWeight::NORMAL, prose))
                    .into_any_element()
            }
            Block::Rule => crate::theme::rule(t).mx_0().into_any_element(),
            Block::Code { lang, code } => {
                let key = format!("{}-{n}", copy.key);
                // A diagram (Mermaid, an SVG) is drawn, unless you asked for its source.
                let source_shown = copy.sources.contains(key.as_str());
                match super::diagram_view::fence(&lang, &code, t) {
                    Some(svg) => diagram_block(&lang, code, svg, &key, source_shown, t, copy),
                    None => code_block(&lang, code, &key, t, copy),
                }
            }
            Block::Table { align, head, rows } => table(&id.to_string(), &align, &head, &rows, t),
        };
        col = col.child(div().mt(gap).child(el));
    }
    col.into_any_element()
}

/// A table: equal columns, a tinted header, hairlines between rows.
fn table(
    key: &str,
    align: &[Alignment],
    head: &[Vec<Span>],
    rows: &[Vec<Vec<Span>>],
    t: &Theme,
) -> AnyElement {
    let columns = head
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0))
        .max(1);
    // A column of numbers with no alignment of its own sits right, so its
    // digits line up.
    let align: Vec<Alignment> = (0..columns)
        .map(|i| match align.get(i).copied().unwrap_or(Alignment::None) {
            Alignment::None if numeric_column(rows, i) => Alignment::Right,
            a => a,
        })
        .collect();
    let align = align.as_slice();
    let row = |cells: &[Vec<Span>], header: bool, y: usize| {
        let mut r = div().flex().w_full();
        for i in 0..columns {
            let id = ElementId::Name(format!("{key}-{y}-{i}").into());
            let spans = cells.get(i).map(|c| c.as_slice()).unwrap_or(&[]);
            // A block, not a flex row, so long cells wrap inside their column.
            let cell = div()
                .flex_1()
                .min_w_0()
                .px(t.space.md)
                .py(t.space.sm)
                .map(|d| match align.get(i) {
                    Some(Alignment::Right) => d.text_right(),
                    Some(Alignment::Center) => d.text_center(),
                    _ => d,
                })
                .child(runs(
                    id,
                    spans,
                    t,
                    if header {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    },
                    if header {
                        t.colors.text_strong
                    } else {
                        t.colors.text
                    },
                ));
            r = r.child(cell);
        }
        r
    };
    let mut out = div()
        .w_full()
        .flex()
        .flex_col()
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.button)
        .overflow_hidden()
        .text_size(t.text.body_sm)
        .line_height(relative(1.45));
    if !head.is_empty() {
        out = out.child(row(head, true, 0).bg(t.extra.code_inline_bg));
    }
    for (y, r) in rows.iter().enumerate() {
        out = out.child(
            row(r, false, y + 1)
                .border_t_1()
                .border_color(t.colors.border),
        );
    }
    out.into_any_element()
}

/// Whether every filled cell of column `i` is a number (`42`, `-3.5`,
/// `84.3%`, `$1,200`, `2.32M`, `96.9% (89.3M / 92.2M in)` is not).
fn numeric_column(rows: &[Vec<Vec<Span>>], i: usize) -> bool {
    let mut any = false;
    for r in rows {
        let text: String = r
            .get(i)
            .map(|c| c.iter().map(|s| s.text.as_str()).collect())
            .unwrap_or_default();
        let s = text.trim();
        if s.is_empty() || matches!(s, "-" | "—" | "–") {
            continue;
        }
        if !number(s) {
            return false;
        }
        any = true;
    }
    any
}

fn number(s: &str) -> bool {
    let s = s.trim_start_matches(['+', '-', '−', '~', '$', '€', '£', '≈']);
    let s = s.trim_end_matches(['%', 'x', '×', 'k', 'K', 'M', 'B', 's']);
    !s.is_empty()
        && s.starts_with(|c: char| c.is_ascii_digit())
        && s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',')
}

fn code_block(lang: &str, code: String, key: &str, t: &Theme, copy: &CopyContext) -> AnyElement {
    let copied = copy.copied.as_deref() == Some(key);
    let shown: SharedString = code.clone().into();
    let state = copy.state.clone();
    let key_owned: SharedString = key.to_string().into();
    let hover = t.colors.hover;
    div()
        .w_full()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(t.space.md)
                .py(t.space.xs)
                .border_b_1()
                .border_color(t.colors.border)
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(
                    div()
                        .font_family(t.font_mono.clone())
                        .child(if lang.is_empty() {
                            "text".to_string()
                        } else {
                            lang.to_string()
                        }),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("copy-{key}")))
                        .flex()
                        .items_center()
                        .gap(t.space.xs)
                        .px(t.space.sm)
                        .py(px(2.))
                        .rounded(t.radius.small)
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .text_color(if copied {
                            t.extra.success
                        } else {
                            t.colors.text_muted
                        })
                        .child(icon(
                            if copied {
                                "icons/check.svg"
                            } else {
                                "icons/copy.svg"
                            },
                            t.size.icon_sm,
                            if copied {
                                t.extra.success
                            } else {
                                t.colors.text_muted
                            },
                        ))
                        .child(if copied { "Copied" } else { "Copy" })
                        .on_click(move |_, _, cx: &mut App| {
                            cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                            let key = key_owned.clone();
                            state.update(cx, |s, cx| s.mark_copied(key, cx));
                        }),
                ),
        )
        .child({
            let wrap_plain =
                matches!(lang.to_ascii_lowercase().as_str(), "" | "text" | "plaintext")
                    && !shown.lines().any(drawing_line);
            div()
                .id(SharedString::from(format!("code-{key}")))
                .when(!wrap_plain, |d| d.overflow_x_scroll().whitespace_nowrap())
                .px(t.space.md)
                .py(t.space.sm)
                .bg(t.extra.inset)
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption)
                .line_height(relative(1.5))
                .text_color(t.colors.text)
                .child(code_text(&shown, lang, t))
        })
        .into_any_element()
}

/// A diagram a reply wrote: drawn in a card, with its kind, Code (show the
/// source instead), Open (the side panel, to zoom and download) and Copy.
fn diagram_block(
    lang: &str,
    code: String,
    svg: std::sync::Arc<str>,
    key: &str,
    source_shown: bool,
    t: &Theme,
    copy: &CopyContext,
) -> AnyElement {
    if source_shown {
        // The source, with a way back to the drawing.
        return div()
            .relative()
            .child(code_block(lang, code, key, t, copy))
            .child(
                div().absolute().top(px(3.)).right(px(84.)).child(
                    header_button(&format!("draw-{key}"), "icons/eye.svg", "Diagram", false, t)
                        .on_click({
                            let (state, key) =
                                (copy.state.clone(), SharedString::from(key.to_string()));
                            move |_, _, cx: &mut App| {
                                state.update(cx, |s, cx| s.toggle_diagram_code(key.clone(), cx))
                            }
                        }),
                ),
            )
            .into_any_element();
    }
    let copied = copy.copied.as_deref() == Some(key);
    let (copier, toggler, opener) = (copy.state.clone(), copy.state.clone(), copy.state.clone());
    let key_owned: SharedString = key.to_string().into();
    let toggle_key = key_owned.clone();
    let kind = match lang.to_ascii_lowercase().as_str() {
        "mermaid" | "mmd" => "Mermaid",
        _ => "SVG",
    };
    let natural = super::diagram_view::natural_width(&svg).unwrap_or(f32::from(t.size.column));
    let inner = (f32::from(t.size.column) - 2. * f32::from(t.space.md) - 2.).max(80.);
    let drawing = super::diagram_view::fluid(&svg, natural.min(inner), copy.scale);
    // Named for what it is, so the panel's title and a download read well.
    let first = code
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let stem = match first.as_str() {
        "sequencediagram" => "sequence-diagram",
        s if s.starts_with("statediagram") => "state-diagram",
        "graph" | "flowchart" => "flowchart",
        _ => "diagram",
    };
    let to_open = crate::state::FileRef {
        name: format!("{stem}.svg").into(),
        path: None,
        content: svg.to_string().into(),
        bytes: None,
    };
    // Zoomed: drawn at that width, scrolling inside the block when wider
    // than it. Fitted: at its own size or the column's, whichever is less.
    let zoom = copy.zooms.get(&key_owned).copied().unwrap_or(1.);
    let fit = natural.min(inner);
    let drawing = if (zoom - 1.).abs() < 0.01 {
        drawing
    } else {
        super::diagram_view::image(&svg, fit * zoom, copy.scale)
    };
    let zoomer = |id: &str, glyph: &'static str, tip: &'static str, by: Option<f32>| {
        let (state, key) = (copy.state.clone(), key_owned.clone());
        tool_button(&format!("{id}-{key_owned}"), glyph, tip, t).on_click(
            move |_, _, cx: &mut App| state.update(cx, |s, cx| s.zoom_diagram(key.clone(), by, cx)),
        )
    };
    let group: SharedString = format!("diagram-{key}").into();
    let fit_state = (copy.state.clone(), key_owned.clone());
    // The tools float over the drawing's corner, shown only while the
    // pointer is on the diagram, so at rest it is the drawing alone.
    let tools = div()
        .absolute()
        .top(t.space.sm)
        .right(t.space.sm)
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(3.))
        .rounded(t.radius.button)
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .shadow(t.elevation_popover())
        .invisible()
        .group_hover(group.clone(), |d| d.visible())
        .child(zoomer(
            "zoom-out",
            "icons/minus.svg",
            "Zoom out",
            Some(1. / 1.25),
        ))
        .child(
            div()
                .id(SharedString::from(format!("zoom-fit-{key}")))
                .min_w(px(40.))
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .cursor_pointer()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .hover({
                    let h = t.colors.hover;
                    move |d| d.bg(h)
                })
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new("Fit to the column").build(window, cx)
                })
                .child(format!("{}%", (zoom * 100.).round()))
                .on_click(move |_, _, cx: &mut App| {
                    fit_state
                        .0
                        .update(cx, |s, cx| s.zoom_diagram(fit_state.1.clone(), None, cx))
                }),
        )
        .child(zoomer("zoom-in", "icons/plus.svg", "Zoom in", Some(1.25)))
        .child(div().w(px(1.)).h(px(16.)).mx(px(2.)).bg(t.colors.border))
        .child(
            tool_button(
                &format!("source-{key}"),
                "icons/code-xml.svg",
                "Show the code",
                t,
            )
            .on_click(move |_, _, cx: &mut App| {
                toggler.update(cx, |s, cx| s.toggle_diagram_code(toggle_key.clone(), cx))
            }),
        )
        .child(
            tool_button(
                &format!("open-{key}"),
                "icons/maximize-2.svg",
                "Open beside the chat",
                t,
            )
            .on_click(move |_, _, cx: &mut App| {
                opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx))
            }),
        )
        .child(
            tool_button(
                &format!("copy-{key}"),
                if copied {
                    "icons/check.svg"
                } else {
                    "icons/copy.svg"
                },
                if copied { "Copied" } else { "Copy the code" },
                t,
            )
            .on_click(move |_, _, cx: &mut App| {
                cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                let key = key_owned.clone();
                copier.update(cx, |s, cx| s.mark_copied(key, cx));
            }),
        );
    let _ = kind;
    div()
        .id(SharedString::from(format!("diagram-block-{key}")))
        .group(group)
        .relative()
        .w_full()
        .bg(t.extra.sunk)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .overflow_hidden()
        .child(
            div()
                .id(SharedString::from(format!("diagram-scroll-{key}")))
                .w_full()
                .when(zoom > 1.01, |d| d.overflow_x_scroll())
                .child(
                    div()
                        .flex()
                        .when(zoom <= 1.01, |d| d.justify_center())
                        .px(t.space.lg)
                        .py(t.space.lg)
                        // The drawing fades in once it is ready, never a
                        // blank card snapping full.
                        .children(drawing.map(|d| {
                            // Fitted, the drawing takes its width from this
                            // wrapper; zoomed, it has its own.
                            let wrap = if zoom <= 1.01 {
                                div().w_full().flex().justify_center().child(d)
                            } else {
                                div().flex_none().child(d)
                            };
                            super::motion::rise_in(
                                wrap,
                                SharedString::from(format!("diagram-in-{key}")),
                                0.,
                                t,
                            )
                        })),
                ),
        )
        .child(tools)
        .into_any_element()
}

/// An icon button in a diagram's floating tools, its name as a tooltip.
fn tool_button(
    id: &str,
    path: &'static str,
    tip: &'static str,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(SharedString::from(id.to_string()))
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .cursor_pointer()
        .hover(move |d| d.bg(hover))
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
        .child(icon(path, t.size.icon_sm, t.colors.text_muted))
}

/// A small text button in a block's header.
fn header_button(
    id: &str,
    path: &'static str,
    label: &'static str,
    done: bool,
    t: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    let color = if done {
        t.extra.success
    } else {
        t.colors.text_muted
    };
    div()
        .id(SharedString::from(id.to_string()))
        .flex()
        .items_center()
        .gap(t.space.xs)
        .px(t.space.sm)
        .py(px(2.))
        .rounded(t.radius.small)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .text_size(t.text.caption)
        .text_color(color)
        .child(icon(path, t.size.icon_xs, color))
        .child(label)
}

/// Code in `lang`, coloured with KodAct's code palette (comments italic).
pub fn code_text(code: &str, lang: &str, t: &Theme) -> StyledText {
    use crate::highlight::{highlight, Kind};
    let k = &t.code;
    let runs = highlight(code, lang)
        .iter()
        .map(|(range, kind)| {
            let mut f = font(t.font_mono.clone());
            if *kind == Kind::Comment {
                f.style = FontStyle::Italic;
            }
            let color = match kind {
                Kind::Plain => t.colors.text,
                Kind::Keyword => k.keyword,
                Kind::Str => k.string,
                Kind::Number => k.number,
                Kind::Comment => k.comment,
                Kind::Function => k.function,
                Kind::Type => k.type_,
                Kind::Variable => k.variable,
                Kind::Operator => k.operator,
                Kind::Meta => k.meta,
                Kind::Added => k.added,
                Kind::Removed => k.removed,
            };
            TextRun {
                len: range.len(),
                font: f,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        })
        .collect::<Vec<_>>();
    StyledText::new(code.to_string()).with_runs(runs)
}

/// The language of a file, by its extension (for colouring its contents).
pub fn lang_of(path: &str) -> &str {
    path.rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|e| !e.contains('/'))
        .unwrap_or("")
}

#[cfg(test)]
mod tests {

    #[test]
    fn front_matter_reads_as_yaml_not_a_heading() {
        let out = super::fence_front_matter("---\nname: KodAct\ncolors: x\n---\n# Title\n");
        assert_eq!(out, "```yaml\nname: KodAct\ncolors: x\n```\n# Title\n");
        // A rule later on, or an unclosed block, stays as written.
        assert_eq!(super::fence_front_matter("Hi\n---\n"), "Hi\n---\n");
        assert_eq!(super::fence_front_matter("---\nno end"), "---\nno end");
    }

    use super::*;

    #[test]
    fn a_url_in_code_keeps_its_text() {
        let b = parse("- Use the OpenAI portal (`https://chat.openai.com`) instead.");
        let Block::Item { spans, .. } = &b[0] else {
            panic!("{:?}", b[0])
        };
        let all: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            all,
            "Use the OpenAI portal (https://chat.openai.com) instead."
        );
    }

    #[test]
    fn blocks_and_inline_styles() {
        let b = parse("# Title\n\nSome **bold** and `code`.\n\n- one\n- two\n\n1. first\n2. second\n\n```rust\nfn x() {}\n```\n\n> quoted\n\n---");
        assert_eq!(
            b[0],
            Block::Heading(
                1,
                vec![Span {
                    text: "Title".into(),
                    ..Default::default()
                }]
            )
        );
        let Block::Para(p) = &b[1] else {
            panic!("{:?}", b[1])
        };
        assert!(p.iter().any(|s| s.bold && s.text == "bold"));
        assert!(p.iter().any(|s| s.code && s.text == "code"));
        assert!(matches!(&b[2], Block::Item { number: None, .. }));
        assert!(matches!(
            &b[4],
            Block::Item {
                number: Some(1),
                ..
            }
        ));
        assert!(matches!(
            &b[5],
            Block::Item {
                number: Some(2),
                ..
            }
        ));
        assert_eq!(
            b[6],
            Block::Code {
                lang: "rust".into(),
                code: "fn x() {}".into()
            }
        );
        assert!(matches!(&b[7], Block::Quote(_)));
        // GitHub's callouts become their own blocks; a plain quote stays a quote.
        let a = parse("> [!WARNING]\n> Back up first.\n>\n> Then run it.\n\n> Just a quote.");
        assert!(matches!(&a[0], Block::Alert { kind: Alert::Warning, first: true, .. }));
        assert!(matches!(&a[1], Block::Alert { kind: Alert::Warning, first: false, .. }));
        assert!(matches!(&a[2], Block::Quote(_)));
        assert_eq!(b[8], Block::Rule);
    }

    #[test]
    fn inline_code_is_laid_out_as_itself() {
        let spans = vec![
            Span {
                text: "see ".into(),
                ..Default::default()
            },
            Span {
                text: "a.rs".into(),
                code: true,
                ..Default::default()
            },
            Span {
                text: ".".into(),
                ..Default::default()
            },
        ];
        let laid: String = spans.iter().map(|s| shown(s).into_owned()).collect();
        // A file keeps room for its icon before it, none after.
        assert_eq!(laid, "see \u{2003}a.rs.", "no padding before the full stop");
        assert_eq!(code_ranges(&spans), vec![4..11]);
        let word = Span {
            text: "cargo test".into(),
            code: true,
            ..Default::default()
        };
        assert_eq!(shown(&word), "cargo test", "a command is laid out as itself");
    }

    #[test]
    fn file_chips_and_links_are_both_clickable() {
        let spans = vec![
            Span {
                text: "see ".into(),
                ..Default::default()
            },
            Span {
                text: "verify.rs".into(),
                code: true,
                ..Default::default()
            },
            Span {
                text: " and ".into(),
                ..Default::default()
            },
            Span {
                text: "the notes".into(),
                link: Some("https://example.com".into()),
                ..Default::default()
            },
            Span {
                text: " via ".into(),
                ..Default::default()
            },
            Span {
                text: "cargo test".into(),
                code: true,
                ..Default::default()
            },
        ];
        assert_eq!(
            targets(&spans),
            vec![
                (4..16, Target::File("verify.rs".into())),
                (21..30, Target::Link("https://example.com".into())),
            ]
        );
    }

    #[test]
    fn number_columns_are_found() {
        let cell = |s: &str| {
            vec![Span {
                text: s.into(),
                ..Default::default()
            }]
        };
        let rows = vec![
            vec![cell("Pass@1"), cell("84.3%"), cell("75")],
            vec![cell("Tokens"), cell("2.32M"), cell("—")],
        ];
        assert!(!numeric_column(&rows, 0));
        assert!(numeric_column(&rows, 1));
        assert!(numeric_column(&rows, 2));
        assert!(!number("v1.2"));
        assert!(number("$1,200"));
    }

    #[test]
    fn file_names_are_told_from_other_code() {
        for f in ["verify.rs", "src/ui/markdown.rs", "Cargo.toml", "self_check.py", "report.py"] {
            assert!(file_like(f), "{f} is a file");
        }
        for c in ["cargo test", "3.14", "x.y", "https://a.com/b.rs", "--flag.rs", "fn", "a.b()"] {
            assert!(!file_like(c), "{c} is not a file");
        }
    }

    #[test]
    fn tables_and_task_lists() {
        let b = parse("| Field | Old | New |\n|:--|:-:|--:|\n| `theme` | string | enum |\n| fonts | – | list |\n\n- [x] Read the old file\n- [ ] Write the new one");
        let Block::Table { align, head, rows } = &b[0] else {
            panic!("{:?}", b[0])
        };
        assert_eq!(
            align,
            &vec![Alignment::Left, Alignment::Center, Alignment::Right]
        );
        assert_eq!(head.len(), 3);
        assert_eq!(head[0][0].text, "Field");
        assert_eq!(rows.len(), 2);
        assert!(rows[0][0][0].code && rows[0][0][0].text == "theme");
        assert_eq!(rows[1][2][0].text, "list");
        assert!(matches!(
            &b[1],
            Block::Item {
                task: Some(true),
                ..
            }
        ));
        assert!(matches!(
            &b[2],
            Block::Item {
                task: Some(false),
                ..
            }
        ));
    }

    #[test]
    fn ascii_drawings_without_a_fence_keep_their_columns() {
        let art = "+--------+      +---------+\n| Client | ---> | Server  |\n+--------+      +---------+\n     |               |\n     v               v";
        let b = parse(&format!("Here:\n\n{art}\n\nDone."));
        assert_eq!(
            b[1],
            Block::Code {
                lang: "text".into(),
                code: art.into()
            }
        );
        // Box drawing too, and a fenced drawing is left alone.
        let boxed = "┌──────┐\n│ KodAct │\n└──┬───┘\n   ▼";
        assert!(matches!(&parse(boxed)[0], Block::Code { lang, .. } if lang == "text"));
        let fenced = format!("```\n{art}\n```");
        assert_eq!(fence_ascii_art(&fenced), fenced);
        // Prose, lists with Unicode arrows, and tables are not drawings.
        for md in [
            "Run it, then --> check the log.\nIt should pass.",
            "- one --> two\n- three",
            "- Go to **myaccount.google.com → Security → Your devices** and sign out.\n- Check **Security → Recent activity** for anything unfamiliar.",
            "| a   | b |\n|---|---|\n| 1 --> 2 | 3 |",
        ] {
            assert_eq!(fence_ascii_art(md), md);
        }
    }

    #[test]
    fn indented_lists_and_arrow_bullets_render_as_items_not_code_blocks() {
        let md = "Since they've now been pasted, treat them as compromised:\n\n    - Go to **myaccount.google.com → Security → Your devices** and sign out of all sessions (this invalidates the `SID` family).\n    - Change the account password, which revokes the rest.\n    - Check **Security → Recent activity** for anything you don't recognise.\n\nWhat I can help with instead:\n\n  - **You're building an in-product assistant** → I can show you the supported path (`grpc-web`).\n  - **You're debugging a client** → tell me the error and I'll trace it.";
        let blocks = parse(md);
        assert!(
            !blocks.iter().any(|b| matches!(b, Block::Code { .. })),
            "expected native Markdown items, got: {blocks:?}"
        );
        let items: Vec<&Vec<Span>> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Item { spans, .. } => Some(spans),
                _ => None,
            })
            .collect();
        assert_eq!(items.len(), 5);
        assert!(items[0]
            .iter()
            .any(|s| s.bold && s.text.contains("myaccount.google.com → Security")));
        assert!(items[0].iter().any(|s| s.code && s.text == "SID"));
        assert!(items[3]
            .iter()
            .any(|s| s.code && s.text == "grpc-web"));

        // A real 4-space indented code block stays a code block.
        let code_md = "Example:\n\n    fn main() {\n        println!(\"hi\");\n    }";
        let code_blocks = parse(code_md);
        assert!(matches!(&code_blocks[1], Block::Code { code, .. } if code.contains("fn main()")));
    }

    #[test]
    fn wrapped_inline_code_never_emits_phantom_box_on_previous_line() {
        // Simulate GPUI's `WrappedLineLayout::position_for_index` when
        // `x-sec-ch-ua` wraps right before `x`: `p0` for `x` is at `(845, 100)`
        // on the previous line, while `p1` is at `(328, 124)` on the new line.
        let origin_x = 320.0;
        let glyphs = vec![
            ('x', (845.0, 100.0), (328.0, 124.0)),
            ('-', (328.0, 124.0), (336.0, 124.0)),
            ('s', (336.0, 124.0), (344.0, 124.0)),
        ];
        assert_eq!(glyph_rows(origin_x, 4., &glyphs), vec![(320.0, 124.0, 344.0)]);

        // Mid-span wrap after `a-` (`a-` on y=100, `b` wrapping to y=124), plus
        // trailing space before wrap ignored.
        let mid_wrap = vec![
            ('a', (820.0, 100.0), (828.0, 100.0)),
            ('-', (828.0, 100.0), (836.0, 100.0)),
            (' ', (836.0, 100.0), (842.0, 100.0)),
            ('b', (842.0, 100.0), (328.0, 124.0)),
            ('c', (328.0, 124.0), (336.0, 124.0)),
        ];
        assert_eq!(
            glyph_rows(origin_x, 4., &mid_wrap),
            vec![(820.0, 100.0, 836.0), (320.0, 124.0, 336.0)]
        );
    }

    #[test]
    fn a_sliver_left_at_a_rows_end_is_not_a_chip() {
        // `KODACT_NUDGES` wrapping whole after "or ": the layout leaves its
        // first glyph's edge as a 0.4px sliver at the end of the first row.
        let origin_x = 320.0;
        let glyphs = vec![
            ('K', (860.0, 100.0), (860.4, 100.0)),
            ('O', (860.4, 100.0), (338.0, 124.0)),
            ('D', (338.0, 124.0), (348.0, 124.0)),
        ];
        assert_eq!(glyph_rows(origin_x, 4., &glyphs), vec![(320.0, 124.0, 348.0)]);
        // A one-character span on one row is kept, however narrow.
        let dot = vec![('.', (400.0, 100.0), (403.0, 100.0))];
        assert_eq!(glyph_rows(origin_x, 4., &dot), vec![(400.0, 100.0, 403.0)]);
    }

    #[test]
    fn an_unclosed_fence_while_streaming_is_still_code() {
        let b = parse("Here:\n\n```sh\ncargo te");
        assert_eq!(
            b.last(),
            Some(&Block::Code {
                lang: "sh".into(),
                code: "cargo te".into()
            })
        );
    }

    #[test]
    fn links_keep_their_address() {
        let b = parse("See [the docs](https://example.com/docs), <https://a.test/x> and http://127.0.0.1:8765/p.html.");
        let Block::Para(p) = &b[0] else {
            panic!("{:?}", b[0])
        };
        let links: Vec<(&str, &str)> = p
            .iter()
            .filter_map(|s| s.link.as_deref().map(|l| (s.text.as_str(), l)))
            .collect();
        assert_eq!(
            links,
            [
                ("the docs", "https://example.com/docs"),
                ("https://a.test/x", "https://a.test/x"),
                (
                    "http://127.0.0.1:8765/p.html",
                    "http://127.0.0.1:8765/p.html"
                ),
            ]
        );
        // The full stop after the bare address stays text.
        assert_eq!(
            p.last().map(|s| (s.text.as_str(), s.link.is_none())),
            Some((".", true))
        );
        // Inside code, an address is not a link.
        let b = parse("`curl https://x.test`");
        let Block::Para(p) = &b[0] else { panic!() };
        assert!(p.iter().all(|s| s.link.is_none()));
    }

    #[test]
    fn bare_addresses_are_cut_cleanly() {
        let urls = |t: &str| {
            split_urls(t)
                .into_iter()
                .filter_map(|(_, u)| u)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            urls("(see https://en.wikipedia.org/wiki/Rust_(language))"),
            ["https://en.wikipedia.org/wiki/Rust_(language)"]
        );
        assert_eq!(
            urls("at https://a.test/b, then http://c.test!"),
            ["https://a.test/b", "http://c.test"]
        );
        assert_eq!(
            urls("xhttps://no.test and http:// alone"),
            Vec::<String>::new()
        );
        // Every piece put back together is the text.
        let t = "a https://a.test/b. c http:// d";
        assert_eq!(split_urls(t).iter().map(|(p, _)| *p).collect::<String>(), t);
    }

    #[test]
    fn only_web_and_mail_links_open() {
        assert!(openable("https://example.com"));
        assert!(openable("HTTP://x.test"));
        assert!(openable("mailto:a@b.test"));
        assert!(!openable("javascript:alert(1)"));
        assert!(!openable("file:///etc/passwd"));
        assert!(!openable("docs/readme.md"));
    }

    #[test]
    fn a_streaming_tail_fades_toward_the_end() {
        let t = crate::theme::Theme::light();
        let text = "Hello wörld, the newest words";
        let spans = [Span {
            text: text.into(),
            ..Default::default()
        }];
        let (text, runs) =
            super::run_list(&spans, &t, &t.font_sans, FontWeight::NORMAL, t.colors.text);
        let faded = super::fade_tail(&text, runs, 8);
        // The runs still cover the text exactly, on character boundaries.
        assert_eq!(faded.iter().map(|r| r.len).sum::<usize>(), text.len());
        assert_eq!(faded.len(), 1 + 8);
        // Whole before the tail, fainter step by step inside it.
        assert_eq!(faded[0].color, t.colors.text);
        let alphas: Vec<f32> = faded.iter().map(|r| r.color.a).collect();
        assert!(alphas.windows(2).all(|w| w[1] < w[0]));
        // Shorter than the tail: every character fades.
        let one = TextRun {
            len: "ö".len(),
            ..faded[0].clone()
        };
        let short = super::fade_tail("ö", vec![one], 8);
        assert_eq!(short.len(), 1);
        assert!(short[0].color.a < t.colors.text.a);
    }
}
