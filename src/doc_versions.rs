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
    /// Sent to Forge: the versions the file had then. A later version
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
    let heading = found.and_then(|i| {
        lines[..=i].iter().rev().find_map(|l| {
            l.strip_prefix('#')
                .map(|h| h.trim_start_matches('#').trim().replace(['*', '`'], ""))
        })
    });
    match heading {
        Some(h) if h.starts_with("Slide ") || h.starts_with("Page ") => {
            h.split(':').next().unwrap_or(&h).to_string()
        }
        Some(h) => format!("the section “{h}”"),
        None => "the text".into(),
    }
}

/// The message that asks Forge to act on `comments` in `file`: each one's
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
