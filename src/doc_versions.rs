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
