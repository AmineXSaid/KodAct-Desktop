//! A file's versions in a chat, and what changed between two of them.
//!
//! Every time a reply writes a file, its card holds that version (the
//! file's bytes then). The chat's cards for one file, in order, are its
//! versions: nothing more is stored. The side panel steps between them and
//! shows what each changed, as Claude's version picker does.

use std::collections::HashMap;

use crate::state::{Block, Chat, FileRef, Message};
use crate::text_diff::Para;

/// What changed from the version before.
#[derive(Clone, Debug, PartialEq)]
pub enum Changes {
    /// A document's text, paragraph by paragraph.
    Text(Vec<Para>),
    /// A workbook's cells that changed, by sheet name: (row, column) to the
    /// value they had before.
    Cells(HashMap<String, HashMap<(usize, usize), String>>),
}

/// Two cards are the same file: the same path, or the same name when a
/// card has no path.
pub fn same_file(a: &FileRef, b: &FileRef) -> bool {
    match (&a.path, &b.path) {
        (Some(x), Some(y)) => x == y,
        _ => a.name == b.name,
    }
}

/// `file`'s versions in `chat`, oldest first (a card shown twice with the
/// same content is one version).
pub fn versions(chat: &Chat, file: &FileRef) -> Vec<FileRef> {
    let mut out: Vec<FileRef> = vec![];
    for m in &chat.messages {
        let Message::Assistant { blocks } = m else {
            continue;
        };
        for b in blocks {
            if let Block::File(f) = b {
                if same_file(f, file) && out.last().is_none_or(|last| !same_content(last, f)) {
                    out.push(f.clone());
                }
            }
        }
    }
    out
}

fn same_content(a: &FileRef, b: &FileRef) -> bool {
    a.content == b.content && a.bytes.as_deref() == b.bytes.as_deref()
}

/// What `new` changed from `old`; None when they cannot be compared.
/// Reading Office files blocks: run it off the UI thread.
pub fn changes(old: &FileRef, new: &FileRef) -> Option<Changes> {
    use crate::office::Kind;
    match (new.office(), old.bytes.as_deref(), new.bytes.as_deref()) {
        (Some(Kind::Xlsx), Some(a), Some(b)) => cells(a, b),
        (Some(kind), Some(a), Some(b)) => {
            let (a, b) = (
                crate::office::text(kind, a).ok()?,
                crate::office::text(kind, b).ok()?,
            );
            Some(Changes::Text(crate::text_diff::diff(&a, &b)))
        }
        (None, None, None) => Some(Changes::Text(crate::text_diff::diff(
            &old.content,
            &new.content,
        ))),
        _ => None,
    }
}

fn cells(old: &[u8], new: &[u8]) -> Option<Changes> {
    let preview = |b: &[u8]| match crate::office::preview("book.xlsx", b) {
        Ok(crate::office::Preview::Sheets(s)) => Some(s),
        _ => None,
    };
    let (a, b) = (preview(old)?, preview(new)?);
    let mut out = HashMap::new();
    for sheet in &b {
        let before = a.iter().find(|s| s.name == sheet.name);
        let mut changed = HashMap::new();
        for (r, row) in sheet.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let was = before
                    .and_then(|s| s.rows.get(r))
                    .and_then(|row| row.get(c))
                    .map(|x| x.text.clone())
                    .unwrap_or_default();
                if was != cell.text {
                    changed.insert((r, c), was);
                }
            }
        }
        out.insert(sheet.name.clone(), changed);
    }
    Some(Changes::Cells(out))
}

/// Where a comment points in a file, and the words or cells it quotes.
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    /// "the section “What changed”", "Slide 3", "Page 2", "Budget!B3:C5".
    pub place: String,
    /// The selected words, or a range's values; empty for a whole page.
    pub quote: String,
}

/// A comment on a file in the side panel, as Claude Docs keeps them.
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub id: usize,
    /// The file (its path, else its name) and the version commented on.
    pub file: String,
    pub version: usize,
    pub anchor: Anchor,
    pub note: String,
    /// Sent to KodAct: the versions the file had then. A later version
    /// answers it.
    pub sent: Option<usize>,
}

/// The file a comment belongs to.
pub fn file_id(f: &FileRef) -> String {
    f.path.clone().unwrap_or_else(|| f.name.clone()).to_string()
}

/// Where `quote` sits in a document's text: the slide, page or section
/// (its nearest heading above) it falls under.
pub fn place_of(text: &str, quote: &str) -> String {
    // Words compared without case, punctuation or Markdown marks.
    let norm = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };
    let words: Vec<String> = norm(quote).into_iter().take(6).collect();
    let lines: Vec<&str> = text.lines().collect();
    // Every word of the text with the line it is on; the quote's place is
    // the line its first word is on.
    let all: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .flat_map(|(i, l)| norm(l).into_iter().map(move |w| (i, w)))
        .collect();
    let found = (!words.is_empty())
        .then(|| {
            all.windows(words.len())
                .find(|w| w.iter().map(|(_, x)| x).eq(words.iter()))
                .map(|w| w[0].0)
        })
        .flatten();
    place_name(found.and_then(|i| heading_above(&lines, i)))
}

/// The nearest heading at or above line `i`, without its Markdown marks.
fn heading_above(lines: &[&str], i: usize) -> Option<String> {
    lines[..=i].iter().rev().find_map(|l| {
        l.strip_prefix('#')
            .map(|h| h.trim_start_matches('#').trim().replace(['*', '`'], ""))
    })
}

/// "Slide 3", "Page 2", the section “…”, or "the text".
fn place_name(heading: Option<String>) -> String {
    match heading {
        Some(h) if h.starts_with("Slide ") || h.starts_with("Page ") => {
            h.split(':').next().unwrap_or(&h).to_string()
        }
        Some(h) => format!("the section “{h}”"),
        None => "the text".into(),
    }
}

/// A match of a search in the panel's file.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Where it is: "Slide 3", the section “…”, "Budget!B3".
    pub place: String,
    /// The page or slide it is on, from 0, when the text says.
    pub page: Option<usize>,
    /// A workbook's sheet and cell (row, column).
    pub cell: Option<(usize, (usize, usize))>,
    /// The words around it, and the match itself.
    pub before: String,
    pub hit: String,
    pub after: String,
}

/// Where `needle` occurs in `hay`, ignoring case: byte ranges.
fn find_ci(hay: &str, needle: &str) -> Vec<(usize, usize)> {
    let n: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if n.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    let mut from = 0;
    for (i, _) in hay.char_indices() {
        if i < from {
            continue;
        }
        let mut chars = hay[i..]
            .char_indices()
            .flat_map(|(j, c)| c.to_lowercase().map(move |l| (j, l)));
        let mut end = None;
        let mut ok = true;
        for want in &n {
            match chars.next() {
                Some((j, got)) if got == *want => end = Some(j),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if let (true, Some(j)) = (ok, end) {
            let j = i + j;
            let stop = j + hay[j..].chars().next().map_or(0, char::len_utf8);
            out.push((i, stop));
            from = stop;
        }
    }
    out
}

/// Up to `chars` characters at the end (or start) of `s`.
fn tail(s: &str, chars: usize) -> String {
    let n = s.chars().count();
    let t: String = s.chars().skip(n.saturating_sub(chars)).collect();
    if n > chars {
        format!("…{}", t.trim_start())
    } else {
        t
    }
}

fn head(s: &str, chars: usize) -> String {
    let t: String = s.chars().take(chars).collect();
    if s.chars().count() > chars {
        format!("{}…", t.trim_end())
    } else {
        t
    }
}

fn plain(s: &str) -> String {
    s.replace(['*', '`'], "")
}

/// Every match of `query` in a document's text (Markdown): its place and
/// the words around it. At most `limit`; the second number is how many
/// there are in all.
pub fn find_text(text: &str, query: &str, limit: usize) -> (Vec<Hit>, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let (mut out, mut total) = (vec![], 0);
    for (i, line) in lines.iter().enumerate() {
        for (a, b) in find_ci(line, query.trim()) {
            total += 1;
            if out.len() >= limit {
                continue;
            }
            let place = place_name(heading_above(&lines, i));
            let page = place
                .strip_prefix("Slide ")
                .or_else(|| place.strip_prefix("Page "))
                .and_then(|n| n.trim().parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));
            let before = plain(line[..a].trim_start_matches(['#', '-', '>', '|', ' ']));
            out.push(Hit {
                place,
                page,
                cell: None,
                before: tail(&before, 40),
                hit: line[a..b].to_string(),
                after: head(&plain(&line[b..]), 60),
            });
        }
    }
    (out, total)
}

/// Every cell of a workbook whose text has `query`, as `Sheet!B3`.
pub fn find_cells(sheets: &[crate::office::Sheet], query: &str, limit: usize) -> (Vec<Hit>, usize) {
    let (mut out, mut total) = (vec![], 0);
    for (si, sheet) in sheets.iter().enumerate() {
        for (r, row) in sheet.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let Some(&(a, b)) = find_ci(&cell.text, query.trim()).first() else {
                    continue;
                };
                total += 1;
                if out.len() < limit {
                    out.push(Hit {
                        place: format!(
                            "{}!{}{}",
                            sheet.name,
                            crate::office::column_letters(c),
                            r + 1
                        ),
                        page: None,
                        cell: Some((si, (r, c))),
                        before: tail(&cell.text[..a], 30),
                        hit: cell.text[a..b].to_string(),
                        after: head(&cell.text[b..], 40),
                    });
                }
            }
        }
    }
    (out, total)
}

/// The message that asks KodAct to act on `comments` in `file`: each one's
/// place, quote and request, and how to edit (only those parts, the rest
/// kept exactly, written back as a new version, then read back).
/// `word_comments`: also add each as a real comment in the Word file.
pub fn comments_message(file: &FileRef, comments: &[Comment], word_comments: bool) -> String {
    let path = file.path.as_deref().unwrap_or(file.name.as_ref());
    let mut out = format!(
        "Change `{path}` as these comments ask. Change only the parts they point at and keep everything else exactly as it is (wording, formatting, layout), then write it back to the same file.\n"
    );
    for (i, c) in comments.iter().enumerate() {
        out.push_str(&format!("\n{}. In {}", i + 1, c.anchor.place));
        if !c.anchor.quote.trim().is_empty() {
            let q: String = c
                .anchor
                .quote
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!(", where it says: “{q}”"));
        }
        out.push_str(&format!("\n   {}\n", c.note.trim()));
    }
    if word_comments {
        out.push_str("\nAlso add each of these as a Word comment on the text it points at (`docx_tool.py comment`), so they travel with the file.\n");
    }
    out.push_str("\nThen read the file back and tell me, comment by comment, what you changed.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(name: &str, path: &str, content: &str) -> FileRef {
        FileRef {
            name: name.into(),
            path: Some(path.into()),
            content: content.into(),
            bytes: None,
        }
    }

    #[test]
    fn a_quote_is_placed_under_its_heading() {
        let text = "# Q3 budget\n\nThe forecast is **5,500 EUR**.\n\n## What changed\n\n- Licences fall to 700.";
        assert_eq!(
            place_of(text, "Licences fall to 700"),
            "the section “What changed”"
        );
        assert_eq!(
            place_of(text, "forecast is 5,500"),
            "the section “Q3 budget”"
        );
        let deck = "## Slide 3: What changed\n\n- Licences: 700";
        assert_eq!(place_of(deck, "Licences: 700"), "Slide 3");
        assert_eq!(place_of(text, "not in it"), "the text");
    }

    #[test]
    fn a_search_finds_each_match_with_its_place() {
        let deck = "## Slide 1: Q3 budget\n\n- Forecast\n\n## Slide 3: What changed\n\n- **Licences**: 700, licences renegotiated";
        let (hits, total) = find_text(deck, "licences", 10);
        assert_eq!(total, 2);
        assert_eq!(hits[0].place, "Slide 3");
        assert_eq!(hits[0].page, Some(2));
        assert_eq!(
            (hits[0].before.as_str(), hits[0].hit.as_str()),
            ("", "Licences")
        );
        assert_eq!(hits[0].after, ": 700, licences renegotiated");
        assert_eq!(hits[1].hit, "licences");
        let memo = "# Q3 memo\n\nTravel rises to 520 EUR.";
        let (hits, _) = find_text(memo, "RISES", 10);
        assert_eq!(
            (hits[0].place.as_str(), hits[0].page),
            ("the section “Q3 memo”", None)
        );
        assert_eq!(
            find_text(memo, "  ", 10).1,
            0,
            "a blank search finds nothing"
        );
        assert_eq!(find_ci("Straße straße", "STRASSE").len(), 0);
        assert_eq!(find_ci("Éclair éclair", "éclair"), vec![(0, 7), (8, 15)]);
    }

    #[test]
    fn comments_become_one_precise_request() {
        let file = md("q3-budget-memo.docx", "finance/q3-budget-memo.docx", "");
        let c = |place: &str, quote: &str, note: &str| Comment {
            id: 0,
            file: file_id(&file),
            version: 0,
            anchor: Anchor {
                place: place.into(),
                quote: quote.into(),
            },
            note: note.into(),
            sent: None,
        };
        let m = comments_message(
            &file,
            &[
                c(
                    "the section “What changed”",
                    "Licences fall to\n700 EUR",
                    "Say why: the renewal was renegotiated.",
                ),
                c("Page 2", "", "Drop this page."),
            ],
            true,
        );
        assert!(m.starts_with("Change `finance/q3-budget-memo.docx` as these comments ask."));
        assert!(m.contains("1. In the section “What changed”, where it says: “Licences fall to 700 EUR”\n   Say why"));
        assert!(m.contains("2. In Page 2\n   Drop this page."));
        assert!(m.contains("docx_tool.py comment"));
        assert!(m.ends_with("what you changed."));
    }

    #[test]
    fn a_rewritten_file_has_versions_in_order() {
        let mut chat = Chat::new(1, 1, "t".into());
        let v1 = md(
            "plan.md",
            "docs/plan.md",
            "# Plan\n\nKeep a backup for a release.",
        );
        let v2 = md(
            "plan.md",
            "docs/plan.md",
            "# Plan\n\nKeep a backup for two releases.",
        );
        let other = md("notes.md", "docs/notes.md", "x");
        chat.messages = vec![
            Message::Assistant {
                blocks: vec![Block::File(v1.clone()), Block::File(other)],
            },
            Message::Assistant {
                blocks: vec![Block::File(v2.clone())],
            },
            // Shown again unchanged: still two versions.
            Message::Assistant {
                blocks: vec![Block::File(v2.clone())],
            },
        ];
        assert_eq!(versions(&chat, &v2), vec![v1.clone(), v2.clone()]);
        let Some(Changes::Text(paras)) = changes(&v1, &v2) else {
            panic!()
        };
        assert_eq!(crate::text_diff::counts(&paras), (1, 0, 0));
    }
}
