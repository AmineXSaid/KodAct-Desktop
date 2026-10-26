//! What you write stays plain text, as in Claude's chat box: no Markdown is
//! styled as you type, and your sent message shows it as typed. Only the
//! mentions read as they do once sent: a `/skill` in its colour, a
//! `/command` or `@path` in the accent, both bold.

use std::ops::Range;

use gpui::{FontWeight, HighlightStyle};

use super::autocomplete::Mark;
use crate::theme::Theme;

/// The styled ranges of `text`: its mentions (`marks`), nothing else.
pub fn styles(
    text: &str,
    marks: &[(Range<usize>, Mark)],
    t: &Theme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut out: Vec<(Range<usize>, HighlightStyle)> = marks
        .iter()
        .map(|(r, mark)| (r.start.min(text.len())..r.end.min(text.len()), *mark))
        .filter(|(r, _)| {
            !r.is_empty() && text.is_char_boundary(r.start) && text.is_char_boundary(r.end)
        })
        .map(|(r, mark)| {
            let color = match mark {
                Mark::Skill => t.extra.skill,
                _ => t.colors.primary,
            };
            (
                r,
                HighlightStyle {
                    color: Some(color),
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            )
        })
        .collect();
    out.sort_by_key(|(r, _)| r.start);
    // Overlapping mentions: the first wins (the editor takes no overlaps).
    let mut end = 0;
    out.retain(|(r, _)| {
        let keep = r.start >= end;
        if keep {
            end = r.end;
        }
        keep
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_stays_plain_and_only_mentions_are_styled() {
        let t = Theme::light();
        let text = "**bold** `code` # head\n```js\nlet x\n```\n/review @src/a.rs";
        assert!(styles(text, &[], &t).is_empty(), "no Markdown styling");
        let at = text.find("/review").unwrap();
        let path = text.find("@src").unwrap();
        let marks = vec![(at..at + 7, Mark::Skill), (path..text.len(), Mark::File)];
        let out = styles(text, &marks, &t);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, at..at + 7);
        assert_eq!(out[0].1.color, Some(t.extra.skill));
        assert_eq!(out[1].1.color, Some(t.colors.primary));
    }
}
