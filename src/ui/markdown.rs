//! Markdown for replies: pulldown-cmark events turned into a few block kinds,
//! each drawn with theme tokens. Inline styles become text runs (bold,
//! italic, strikethrough, links, and inline code in the mono face on a tint).
//! Code blocks get a header with the language and a Copy button.
//!
//! Partial input is fine: a reply is re-parsed as it streams, and an
//! unclosed fence simply runs to the end.

use gpui::{
    div, font, prelude::*, px, relative, AnyElement, App, ClipboardItem, Entity, FontStyle, FontWeight, SharedString,
    StrikethroughStyle, StyledText, TextRun, UnderlineStyle,
};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

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
    Item { number: Option<u64>, depth: usize, spans: Vec<Span> },
    Code { lang: String, code: String },
    Quote(Vec<Span>),
    Rule,
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

    let push = |spans: &mut Vec<Span>, text: &str, s: Span| {
        if text.is_empty() {
            return;
        }
        match spans.last_mut() {
            Some(last) if Span { text: String::new(), ..last.clone() } == Span { text: String::new(), ..s.clone() } => last.text.push_str(text),
            _ => spans.push(Span { text: text.to_string(), ..s }),
        }
    };

    for ev in Parser::new_ext(src, Options::ENABLE_STRIKETHROUGH) {
        let style = Span { text: String::new(), bold, italic, code: false, strike, link };
        match ev {
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.split_whitespace().next().unwrap_or("").to_string(),
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
            Event::End(TagEnd::Heading(_)) => out.push(Block::Heading(heading.take().unwrap_or(3), std::mem::take(&mut spans))),
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
                        out.push(Block::Item { number: num, depth: lists.len().saturating_sub(2), spans: std::mem::take(&mut spans) });
                    }
                }
                item_number = Some(n);
            }
            Event::End(TagEnd::Item) => {
                if let Some(num) = item_number.take() {
                    out.push(Block::Item { number: num, depth: lists.len().saturating_sub(1), spans: std::mem::take(&mut spans) });
                } else {
                    spans.clear();
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
            Event::Code(t) => push(&mut spans, &t, Span { code: true, ..style }),
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
}

fn runs(spans: &[Span], t: &Theme, base_weight: FontWeight, color: gpui::Hsla) -> StyledText {
    let mut text = String::new();
    let mut runs = vec![];
    for s in spans {
        if s.text.is_empty() {
            continue;
        }
        text.push_str(&s.text);
        let mut f = font(if s.code { t.font_mono.clone() } else { t.font_sans.clone() });
        f.weight = if s.bold { FontWeight::SEMIBOLD } else { base_weight };
        if s.italic {
            f.style = FontStyle::Italic;
        }
        runs.push(TextRun {
            len: s.text.len(),
            font: f,
            color: if s.link { t.colors.primary } else { color },
            background_color: s.code.then_some(t.extra.inset),
            underline: s.link.then(|| UnderlineStyle { thickness: px(1.), color: Some(t.colors.primary), wavy: false }),
            strikethrough: s.strike.then(|| StrikethroughStyle { thickness: px(1.), color: Some(color) }),
        });
    }
    StyledText::new(text).with_runs(runs)
}

pub fn render(src: &str, t: &Theme, copy: &CopyContext) -> AnyElement {
    let blocks = parse(src);
    let mut col = div().flex().flex_col().w_full().text_size(t.size.chat_text).line_height(relative(t.size.line_height_chat));
    let mut prev: Option<&'static str> = None;
    for (n, b) in blocks.into_iter().enumerate() {
        // Space above a block depends on what it follows: list items sit
        // close together, headings and code get room above them.
        let kind = match &b {
            Block::Item { .. } => "item",
            Block::Heading(..) => "heading",
            Block::Code { .. } => "code",
            _ => "text",
        };
        let gap = match (prev, kind) {
            (None, _) => px(0.),
            (Some("item"), "item") => px(3.),
            (_, "heading") => t.space.md + px(2.),
            (Some("heading"), _) => t.space.xs,
            (_, "code") | (Some("code"), _) => t.space.sm + px(2.),
            _ => t.space.sm,
        };
        prev = Some(kind);
        let el: AnyElement = match b {
            Block::Para(spans) => div().child(runs(&spans, t, FontWeight::NORMAL, t.colors.text)).into_any_element(),
            Block::Heading(level, spans) => div()
                .text_size(if level == 1 { t.text.body + px(1.) } else if level == 2 { t.text.body_sm + px(1.) } else { t.size.chat_text })
                .line_height(relative(1.35))
                .child(runs(&spans, t, FontWeight::SEMIBOLD, t.colors.text_strong))
                .into_any_element(),
            Block::Item { number, depth, spans } => div()
                .flex()
                .gap(t.space.sm)
                .pl(t.space.xs + t.space.lg * depth as f32)
                .child(
                    div()
                        .flex_none()
                        .min_w(t.space.lg)
                        .text_color(t.colors.text_subtle)
                        .child(match number {
                            Some(n) => format!("{n}."),
                            None => "•".to_string(),
                        }),
                )
                .child(div().flex_1().min_w_0().child(runs(&spans, t, FontWeight::NORMAL, t.colors.text)))
                .into_any_element(),
            Block::Quote(spans) => div()
                .border_l_2()
                .border_color(t.colors.border)
                .pl(t.space.md)
                .child(runs(&spans, t, FontWeight::NORMAL, t.colors.text_body))
                .into_any_element(),
            Block::Rule => div().h(t.size.hairline).bg(t.colors.border).into_any_element(),
            Block::Code { lang, code } => code_block(&lang, code, &format!("{}-{n}", copy.key), t, copy),
        };
        col = col.child(div().mt(gap).child(el));
    }
    col.into_any_element()
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
                .text_color(t.colors.text_subtle)
                .child(div().font_family(t.font_mono.clone()).child(if lang.is_empty() { "text".to_string() } else { lang.to_string() }))
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
                        .text_color(if copied { t.extra.success } else { t.colors.text_muted })
                        .child(icon(if copied { "icons/check.svg" } else { "icons/copy.svg" }, t.size.icon_sm, if copied { t.extra.success } else { t.colors.text_muted }))
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
                .py(t.space.sm + px(2.))
                .bg(t.extra.inset)
                .font_family(t.font_mono.clone())
                .text_size(t.text.caption + px(0.5))
                .line_height(relative(1.55))
                .text_color(t.colors.text)
                .whitespace_nowrap()
                .child(shown),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_and_inline_styles() {
        let b = parse("# Title\n\nSome **bold** and `code`.\n\n- one\n- two\n\n1. first\n2. second\n\n```rust\nfn x() {}\n```\n\n> quoted\n\n---");
        assert_eq!(b[0], Block::Heading(1, vec![Span { text: "Title".into(), ..Default::default() }]));
        let Block::Para(p) = &b[1] else { panic!("{:?}", b[1]) };
        assert!(p.iter().any(|s| s.bold && s.text == "bold"));
        assert!(p.iter().any(|s| s.code && s.text == "code"));
        assert!(matches!(&b[2], Block::Item { number: None, .. }));
        assert!(matches!(&b[4], Block::Item { number: Some(1), .. }));
        assert!(matches!(&b[5], Block::Item { number: Some(2), .. }));
        assert_eq!(b[6], Block::Code { lang: "rust".into(), code: "fn x() {}".into() });
        assert!(matches!(&b[7], Block::Quote(_)));
        assert_eq!(b[8], Block::Rule);
    }

    #[test]
    fn an_unclosed_fence_while_streaming_is_still_code() {
        let b = parse("Here:\n\n```sh\ncargo te");
        assert_eq!(b.last(), Some(&Block::Code { lang: "sh".into(), code: "cargo te".into() }));
    }
}
