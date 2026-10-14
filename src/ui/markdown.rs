//! Markdown for replies: pulldown-cmark events turned into a few block kinds,
//! each drawn with theme tokens. Inline styles become text runs (bold,
//! italic, strikethrough, links, and inline code in the mono face on a tint).
//! Code blocks get a header with the language and a Copy button.
//!
//! Partial input is fine: a reply is re-parsed as it streams, and an
//! unclosed fence simply runs to the end.
//!
//! Two looks: `Chat` for replies, and `Document` for a file opened in the
//! side panel (`doc_pane.rs`): a larger reading size, more leading, and the
//! title in the display serif. Both draw tables and task lists.

use gpui::{
    div, font, prelude::*, px, relative, AnyElement, App, ClipboardItem, Entity, FontStyle,
    FontWeight, SharedString, StrikethroughStyle, StyledText, TextRun, UnderlineStyle,
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
    link: bool,
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
    Rule,
    Table {
        align: Vec<Alignment>,
        head: Vec<Vec<Span>>,
        rows: Vec<Vec<Vec<Span>>>,
    },
}

/// How a piece of markdown is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Look {
    /// A reply in the transcript.
    Chat,
    /// A document in the side panel.
    Document,
}

#[derive(Default)]
struct TableBuild {
    align: Vec<Alignment>,
    head: Vec<Vec<Span>>,
    rows: Vec<Vec<Vec<Span>>>,
    in_head: bool,
}

/// Box-drawing, block, shape and arrow characters: only drawings use them.
fn drawing_char(c: char) -> bool {
    matches!(c, '\u{2190}'..='\u{21FF}' | '\u{2500}'..='\u{25FF}' | '\u{27F5}'..='\u{27FF}')
}

/// A line that only a drawing would have: box drawing, `+--+` corners or
/// ASCII arrows.
fn drawing_line(l: &str) -> bool {
    l.chars().any(drawing_char)
        || [
            "+--", "--+", "-+-", "-->", "<--", "==>", "<==", "|--", "--|", "-.->",
        ]
        .iter()
        .any(|p| l.contains(p))
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

fn parse(src: &str) -> Vec<Block> {
    let mut out = vec![];
    let mut spans: Vec<Span> = vec![];
    let (mut bold, mut italic, mut strike, mut link) = (false, false, false, false);
    let mut lists: Vec<Option<u64>> = vec![];
    let mut item_number: Option<Option<u64>> = None;
    let mut quote = 0usize;
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

    let src = fence_ascii_art(src);
    for ev in Parser::new_ext(
        &src,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
    ) {
        let style = Span {
            text: String::new(),
            bold,
            italic,
            code: false,
            strike,
            link,
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
            Event::Start(Tag::BlockQuote(_)) => quote += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote = quote.saturating_sub(1),
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if item_number.is_some() {
                    push(&mut spans, " ", style);
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
            Event::Start(Tag::Link { .. }) => link = true,
            Event::End(TagEnd::Link) => link = false,
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
}

fn runs(spans: &[Span], t: &Theme, base_weight: FontWeight, color: gpui::Hsla) -> StyledText {
    runs_in(spans, t, &t.font_sans, base_weight, color)
}

/// Text runs in `family` (code spans stay in the mono face).
fn runs_in(
    spans: &[Span],
    t: &Theme,
    family: &SharedString,
    base_weight: FontWeight,
    color: gpui::Hsla,
) -> StyledText {
    let mut text = String::new();
    let mut runs = vec![];
    for s in spans {
        if s.text.is_empty() {
            continue;
        }
        text.push_str(&s.text);
        let mut f = font(if s.code {
            t.font_mono.clone()
        } else {
            family.clone()
        });
        f.weight = if s.bold {
            FontWeight::SEMIBOLD
        } else {
            base_weight
        };
        if s.italic {
            f.style = FontStyle::Italic;
        }
        runs.push(TextRun {
            len: s.text.len(),
            font: f,
            color: if s.link { t.colors.primary } else { color },
            background_color: s.code.then_some(t.extra.inset),
            underline: s.link.then(|| UnderlineStyle {
                thickness: px(1.),
                color: Some(t.colors.primary),
                wavy: false,
            }),
            strikethrough: s.strike.then(|| StrikethroughStyle {
                thickness: px(1.),
                color: Some(color),
            }),
        });
    }
    StyledText::new(text).with_runs(runs)
}

pub fn render(src: &str, t: &Theme, copy: &CopyContext) -> AnyElement {
    render_as(src, t, copy, Look::Chat)
}

pub fn render_as(src: &str, t: &Theme, copy: &CopyContext, look: Look) -> AnyElement {
    let doc = look == Look::Document;
    let blocks = parse(src);
    let (size, leading) = if doc {
        (t.size.doc_text, t.size.line_height_doc)
    } else {
        (t.size.chat_text, t.size.line_height_chat)
    };
    let mut col = div()
        .flex()
        .flex_col()
        .w_full()
        .text_size(size)
        .line_height(relative(leading));
    let mut prev: Option<&'static str> = None;
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
            (Some("item"), "item", false) => px(2.),
            (Some("item"), "item", true) => px(6.),
            (_, "heading", false) => t.space.md,
            (_, "heading", true) => t.space.xl + t.space.xs,
            (Some("heading"), _, false) => t.space.xs,
            (Some("heading"), _, true) => t.space.sm + px(2.),
            (_, "code", false) | (Some("code"), _, false) => t.space.sm,
            (_, "code", true) | (Some("code"), _, true) => t.space.lg,
            (_, _, false) => t.space.sm - px(2.),
            (_, _, true) => t.space.md,
        };
        prev = Some(kind);
        let el: AnyElement = match b {
            Block::Para(spans) => div()
                .child(runs(&spans, t, FontWeight::NORMAL, t.colors.text))
                .into_any_element(),
            // A document's title is set in the display serif, as the greeting is.
            Block::Heading(1, spans) if doc => div()
                .text_size(t.size.doc_title)
                .line_height(relative(1.2))
                .child(runs_in(
                    &spans,
                    t,
                    &t.font_display,
                    FontWeight::NORMAL,
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
                .child(runs(&spans, t, FontWeight::SEMIBOLD, t.colors.text_strong))
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
                    None => div()
                        .flex_none()
                        .min_w(t.space.lg)
                        .text_color(t.colors.text_muted)
                        .child(match number {
                            Some(n) => format!("{n}."),
                            None => "•".to_string(),
                        })
                        .into_any_element(),
                })
                .child(div().flex_1().min_w_0().child(runs(
                    &spans,
                    t,
                    FontWeight::NORMAL,
                    if task == Some(true) {
                        t.colors.text_muted
                    } else {
                        t.colors.text
                    },
                )))
                .into_any_element(),
            Block::Quote(spans) => div()
                .border_l_2()
                .border_color(t.colors.border)
                .pl(t.space.md)
                .child(runs(&spans, t, FontWeight::NORMAL, t.colors.text))
                .into_any_element(),
            Block::Rule => div()
                .h(t.size.hairline)
                .bg(t.colors.border)
                .into_any_element(),
            Block::Code { lang, code } => {
                let key = format!("{}-{n}", copy.key);
                // A diagram (Mermaid, an SVG) is drawn, unless you asked for its source.
                let source_shown = copy.sources.contains(key.as_str());
                match super::diagram_view::fence(&lang, &code, t) {
                    Some(svg) => diagram_block(&lang, code, svg, &key, source_shown, t, copy),
                    None => code_block(&lang, code, &key, t, copy),
                }
            }
            Block::Table { align, head, rows } => table(&align, &head, &rows, t),
        };
        col = col.child(div().mt(gap).child(el));
    }
    col.into_any_element()
}

/// A table: equal columns, a tinted header, hairlines between rows.
fn table(
    align: &[Alignment],
    head: &[Vec<Span>],
    rows: &[Vec<Vec<Span>>],
    t: &Theme,
) -> AnyElement {
    let columns = head
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0))
        .max(1);
    let row = |cells: &[Vec<Span>], header: bool| {
        let mut r = div().flex().w_full();
        for i in 0..columns {
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
        out = out.child(row(head, true).bg(t.extra.inset));
    }
    for r in rows {
        out = out.child(row(r, false).border_t_1().border_color(t.colors.border));
    }
    out.into_any_element()
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
        .child(
            div()
                .id(SharedString::from(format!("code-{key}")))
                .overflow_x_scroll()
                .px(t.space.md)
                .py(t.space.sm)
                .bg(t.extra.inset)
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption)
                .line_height(relative(1.5))
                .text_color(t.colors.text)
                .whitespace_nowrap()
                .child(code_text(&shown, lang, t)),
        )
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
                .gap(t.space.xs)
                .px(t.space.md)
                .py(t.space.xs)
                .border_b_1()
                .border_color(t.colors.border)
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child(icon(
                    "icons/route.svg",
                    t.size.icon_xs,
                    t.colors.text_subtle,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(format!("Diagram · {kind}")),
                )
                .child(
                    header_button(
                        &format!("source-{key}"),
                        "icons/code-xml.svg",
                        "Code",
                        false,
                        t,
                    )
                    .on_click(move |_, _, cx: &mut App| {
                        toggler.update(cx, |s, cx| s.toggle_diagram_code(toggle_key.clone(), cx))
                    }),
                )
                .child(
                    header_button(
                        &format!("open-{key}"),
                        "icons/maximize-2.svg",
                        "Open",
                        false,
                        t,
                    )
                    .on_click(move |_, _, cx: &mut App| {
                        opener.update(cx, |s, cx| s.open_doc(to_open.clone(), cx))
                    }),
                )
                .child(
                    header_button(
                        &format!("copy-{key}"),
                        if copied {
                            "icons/check.svg"
                        } else {
                            "icons/copy.svg"
                        },
                        if copied { "Copied" } else { "Copy" },
                        copied,
                        t,
                    )
                    .on_click(move |_, _, cx: &mut App| {
                        cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                        let key = key_owned.clone();
                        copier.update(cx, |s, cx| s.mark_copied(key, cx));
                    }),
                ),
        )
        // The drawing, centred, at its own size or the column's, whichever is smaller.
        .child(
            div()
                .flex()
                .justify_center()
                .p(t.space.md)
                .children(drawing),
        )
        .into_any_element()
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

/// Code in `lang`, coloured with Forge's code palette (comments italic).
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
    use super::*;

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
        assert_eq!(b[8], Block::Rule);
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
        let boxed = "┌──────┐\n│ Forge │\n└──┬───┘\n   ▼";
        assert!(matches!(&parse(boxed)[0], Block::Code { lang, .. } if lang == "text"));
        let fenced = format!("```\n{art}\n```");
        assert_eq!(fence_ascii_art(&fenced), fenced);
        // Prose, lists and tables are not drawings.
        for md in [
            "Run it, then --> check the log.\nIt should pass.",
            "- one --> two\n- three",
            "| a   | b |\n|---|---|\n| 1 --> 2 | 3 |",
        ] {
            assert_eq!(fence_ascii_art(md), md);
        }
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
}
