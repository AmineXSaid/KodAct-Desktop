//! A Markdown file as the page editor sees it (`ui/md_page.rs`): a list of
//! blocks (headings, paragraphs, list items, checklist items, quotes, code,
//! tables, dividers, pictures), read from the text and written back as
//! Markdown. What the editor doesn't change comes back as it was written:
//! a paragraph keeps its line breaks, a code block its fence and language.

/// What a block is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Paragraph,
    /// `#` to `######`.
    Heading(u8),
    Bullet,
    Numbered,
    /// A checklist item, ticked or not.
    Check(bool),
    Quote,
    /// A fenced code block and its language.
    Code(String),
    /// A table: its rows of cells, the first the header.
    Table(Vec<Vec<String>>),
    Divider,
    /// A picture on a line of its own: its alt text and where it is.
    Image { alt: String, src: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: Kind,
    /// The words (empty for a divider, a table or a picture).
    pub text: String,
}

impl Block {
    pub fn new(kind: Kind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }

    /// A list item of any kind, which sits on the line under the one before.
    pub fn is_item(&self) -> bool {
        matches!(self.kind, Kind::Bullet | Kind::Numbered | Kind::Check(_))
    }

    /// Whether it holds words you type.
    pub fn is_text(&self) -> bool {
        !matches!(
            self.kind,
            Kind::Divider | Kind::Table(_) | Kind::Image { .. }
        )
    }
}

/// The blocks of `md`.
pub fn parse(md: &str) -> Vec<Block> {
    let lines: Vec<&str> = md.lines().collect();
    let mut out: Vec<Block> = vec![];
    let mut para: Vec<&str> = vec![];
    // A paragraph's lines are one run of words (Markdown wraps them); a
    // line ending in two spaces or a backslash keeps its break.
    let flush = |para: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            let mut text = String::new();
            for (k, l) in para.iter().enumerate() {
                if k > 0 {
                    let prev = para[k - 1];
                    text.push(if prev.ends_with("  ") || prev.ends_with('\\') { '\n' } else { ' ' });
                }
                text.push_str(if k + 1 < para.len() { l.trim_end_matches(' ') } else { l });
            }
            out.push(Block::new(Kind::Paragraph, text));
            para.clear();
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        // A fence runs to its closing fence (or the end).
        if let Some(lang) = trimmed.strip_prefix("```") {
            flush(&mut para, &mut out);
            let mut body = vec![];
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                body.push(lines[i]);
                i += 1;
            }
            out.push(Block::new(Kind::Code(lang.trim().to_string()), body.join("\n")));
            i += 1;
            continue;
        }
        if trimmed.starts_with('|') {
            flush(&mut para, &mut out);
            let mut rows = vec![];
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                let row = lines[i].trim();
                if !is_rule_row(row) {
                    rows.push(cells(row));
                }
                i += 1;
            }
            out.push(Block::new(Kind::Table(rows), ""));
            continue;
        }
        i += 1;
        if trimmed.is_empty() {
            flush(&mut para, &mut out);
            continue;
        }
        if let Some(b) = line_block(trimmed) {
            flush(&mut para, &mut out);
            out.push(b);
            continue;
        }
        para.push(line);
    }
    flush(&mut para, &mut out);
    out
}

/// A line that is a block of its own: a heading, an item, a quote, a rule,
/// a picture.
fn line_block(line: &str) -> Option<Block> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        return Some(Block::new(
            Kind::Heading(hashes as u8),
            line[hashes + 1..].trim(),
        ));
    }
    if matches!(line.trim_end(), "---" | "***" | "___") {
        return Some(Block::new(Kind::Divider, ""));
    }
    for (mark, done) in [
        ("- [ ] ", false),
        ("- [x] ", true),
        ("- [X] ", true),
        ("* [ ] ", false),
        ("* [x] ", true),
    ] {
        if let Some(rest) = line.strip_prefix(mark) {
            return Some(Block::new(Kind::Check(done), rest));
        }
    }
    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some(Block::new(Kind::Bullet, rest));
    }
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some(Block::new(Kind::Numbered, &line[digits + 2..]));
    }
    if let Some(rest) = line.strip_prefix("> ").or_else(|| line.strip_prefix('>')) {
        return Some(Block::new(Kind::Quote, rest));
    }
    if let Some((alt, src)) = image_line(line) {
        return Some(Block::new(Kind::Image { alt, src }, ""));
    }
    None
}

/// `![alt](src)` alone on its line.
fn image_line(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("![")?;
    let (alt, rest) = rest.split_once("](")?;
    let src = rest.strip_suffix(')')?;
    (!src.contains(' ') || src.starts_with('<')).then(|| (alt.to_string(), src.to_string()))
}

/// `|---|:--:|`: the line under a table's header.
fn is_rule_row(row: &str) -> bool {
    row.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) && row.contains('-')
}

fn cells(row: &str) -> Vec<String> {
    let inner = row.trim().trim_start_matches('|').trim_end_matches('|');
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

/// The blocks as Markdown: a blank line between blocks, list items of one
/// list on consecutive lines, numbers counted afresh in each list.
pub fn write(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut number = 0;
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            let prev = &blocks[i - 1];
            let same_list = prev.is_item()
                && b.is_item()
                && std::mem::discriminant(&prev.kind) == std::mem::discriminant(&b.kind);
            out.push_str(if same_list { "\n" } else { "\n\n" });
        }
        if matches!(b.kind, Kind::Numbered) {
            let continues = i > 0 && matches!(blocks[i - 1].kind, Kind::Numbered);
            number = if continues { number + 1 } else { 1 };
        }
        match &b.kind {
            Kind::Paragraph => out.push_str(&b.text),
            Kind::Heading(n) => {
                out.push_str(&"#".repeat(*n as usize));
                out.push(' ');
                out.push_str(&b.text);
            }
            Kind::Bullet => {
                out.push_str("- ");
                out.push_str(&b.text);
            }
            Kind::Numbered => out.push_str(&format!("{number}. {}", b.text)),
            Kind::Check(done) => {
                out.push_str(if *done { "- [x] " } else { "- [ ] " });
                out.push_str(&b.text);
            }
            Kind::Quote => {
                let quoted: Vec<String> = b.text.lines().map(|l| format!("> {l}")).collect();
                out.push_str(&if quoted.is_empty() {
                    "> ".to_string()
                } else {
                    quoted.join("\n")
                });
            }
            Kind::Code(lang) => out.push_str(&format!("```{lang}\n{}\n```", b.text)),
            Kind::Table(rows) => out.push_str(&table(rows)),
            Kind::Divider => out.push_str("---"),
            Kind::Image { alt, src } => out.push_str(&format!("![{alt}]({src})")),
        }
    }
    out.push('\n');
    out
}

/// A table's rows as a pipe table, the first row its header.
pub fn table(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let line = |r: &Vec<String>| {
        let mut cells: Vec<String> = r.clone();
        cells.resize(cols, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![];
    for (i, r) in rows.iter().enumerate() {
        out.push(line(r));
        if i == 0 {
            out.push(format!("|{}|", vec!["---"; cols].join("|")));
        }
    }
    out.join("\n")
}

/// What typing a Markdown mark at the start of a paragraph turns it into:
/// `# ` a heading, `- ` a bullet, `1. ` a numbered item, `[] ` a checklist
/// item, `> ` a quote, `---` a divider. The rest of the words stay.
pub fn shortcut(text: &str) -> Option<(Kind, String)> {
    if text == "---" {
        return Some((Kind::Divider, String::new()));
    }
    for (mark, kind) in [
        ("### ", Kind::Heading(3)),
        ("## ", Kind::Heading(2)),
        ("# ", Kind::Heading(1)),
        ("[] ", Kind::Check(false)),
        ("[ ] ", Kind::Check(false)),
        ("- ", Kind::Bullet),
        ("* ", Kind::Bullet),
        ("1. ", Kind::Numbered),
        ("> ", Kind::Quote),
    ] {
        if let Some(rest) = text.strip_prefix(mark) {
            return Some((kind, rest.to_string()));
        }
    }
    None
}

/// The headings, for the outline: (block index, level, words).
pub fn outline(blocks: &[Block]) -> Vec<(usize, u8, String)> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match b.kind {
            Kind::Heading(n) => Some((i, n, b.text.clone())),
            _ => None,
        })
        .collect()
}

/// Wrap `range` of `text` in `mark` (`**` bold, `*` italic, `` ` `` code),
/// or take the marks off when they are already there. Returns the new text
/// and the range the words now sit in.
pub fn toggle_mark(
    text: &str,
    range: std::ops::Range<usize>,
    mark: &str,
) -> (String, std::ops::Range<usize>) {
    let (a, b) = (range.start.min(text.len()), range.end.min(text.len()));
    let m = mark.len();
    let wrapped = a >= m && text.get(a - m..a) == Some(mark) && text.get(b..b + m) == Some(mark);
    if wrapped {
        let s = format!("{}{}{}", &text[..a - m], &text[a..b], &text[b + m..]);
        return (s, a - m..b - m);
    }
    let s = format!("{}{mark}{}{mark}{}", &text[..a], &text[a..b], &text[b..]);
    (s, a + m..b + m)
}

/// Make `range` of `text` a link to `url`: `[words](url)`.
pub fn link(text: &str, range: std::ops::Range<usize>, url: &str) -> (String, usize) {
    let (a, b) = (range.start.min(text.len()), range.end.min(text.len()));
    let words = if a == b { "link" } else { &text[a..b] };
    let s = format!("{}[{words}]({url}){}", &text[..a], &text[b..]);
    let end = a + words.len() + url.len() + 4;
    (s, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Settings migration\n\nWhy we move. It is time.\n\n## Steps\n\n- Read the sheet\n- Check prices\n\n1. First\n2. Second\n\n- [ ] Write the memo\n- [x] Ask finance\n\n> Keep it short.\n\n```rust\nfn main() {}\n```\n\n| Item | Cost |\n|---|---|\n| Licences | 40 |\n\n---\n\n![Logo](logo.png)\n";

    #[test]
    fn a_document_reads_as_blocks() {
        let b = parse(DOC);
        let kinds: Vec<&Kind> = b.iter().map(|b| &b.kind).collect();
        assert_eq!(kinds[0], &Kind::Heading(1));
        assert_eq!(b[1], Block::new(Kind::Paragraph, "Why we move. It is time."));
        assert_eq!(kinds[2], &Kind::Heading(2));
        assert_eq!(b[3], Block::new(Kind::Bullet, "Read the sheet"));
        assert_eq!(kinds[5], &Kind::Numbered);
        assert_eq!(b[7], Block::new(Kind::Check(false), "Write the memo"));
        assert_eq!(kinds[8], &Kind::Check(true));
        assert_eq!(b[9], Block::new(Kind::Quote, "Keep it short."));
        assert_eq!(b[10], Block::new(Kind::Code("rust".into()), "fn main() {}"));
        assert_eq!(
            kinds[11],
            &Kind::Table(vec![
                vec!["Item".into(), "Cost".into()],
                vec!["Licences".into(), "40".into()]
            ])
        );
        assert_eq!(kinds[12], &Kind::Divider);
        assert_eq!(
            kinds[13],
            &Kind::Image {
                alt: "Logo".into(),
                src: "logo.png".into()
            }
        );
    }

    #[test]
    fn a_document_comes_back_as_written() {
        assert_eq!(write(&parse(DOC)), DOC);
    }

    #[test]
    fn a_paragraph_wrapped_over_lines_reads_as_one() {
        let b = parse("one\ntwo\nthree  \nfour\n");
        assert_eq!(b, vec![Block::new(Kind::Paragraph, "one two three\nfour")]);
    }

    #[test]
    fn numbers_count_afresh_in_each_list() {
        let b = vec![
            Block::new(Kind::Numbered, "a"),
            Block::new(Kind::Numbered, "b"),
            Block::new(Kind::Paragraph, "x"),
            Block::new(Kind::Numbered, "c"),
        ];
        assert_eq!(write(&b), "1. a\n2. b\n\nx\n\n1. c\n");
    }

    #[test]
    fn marks_typed_at_the_start_change_the_block() {
        assert_eq!(shortcut("# Title"), Some((Kind::Heading(1), "Title".into())));
        assert_eq!(shortcut("[] task"), Some((Kind::Check(false), "task".into())));
        assert_eq!(shortcut("- "), Some((Kind::Bullet, String::new())));
        assert_eq!(shortcut("---"), Some((Kind::Divider, String::new())));
        assert_eq!(shortcut("plain"), None);
    }

    #[test]
    fn bold_wraps_and_unwraps_the_selection() {
        let (s, r) = toggle_mark("make this bold", 5..9, "**");
        assert_eq!((s.as_str(), r.clone()), ("make **this** bold", 7..11));
        let (s, r) = toggle_mark(&s, r, "**");
        assert_eq!((s.as_str(), r), ("make this bold", 5..9));
        assert_eq!(link("see docs", 4..8, "https://x.y").0, "see [docs](https://x.y)");
    }

    #[test]
    fn the_outline_lists_the_headings() {
        let o = outline(&parse(DOC));
        assert_eq!(o, vec![(0, 1, "Settings migration".into()), (2, 2, "Steps".into())]);
    }
}
