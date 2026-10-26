//! Selecting text in a reply or in your own message, to copy part of it:
//! press and drag across paragraphs as in a document, Ctrl+C copies, a
//! click anywhere else clears. Each paragraph (`markdown::linked`, a message
//! card) registers its text under a stable key and its place in reading
//! order this frame; the selection runs from an anchor (key, byte) to a
//! head, and every paragraph between paints its part highlighted.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

use gpui::{
    div, prelude::*, AnyElement, CursorStyle, Hsla, MouseButton, SharedString, TextLayout, TextRun,
};

#[derive(Default)]
struct Selection {
    anchor: Option<(SharedString, usize)>,
    head: Option<(SharedString, usize)>,
    dragging: bool,
    /// Each paragraph's place in reading order: in the frame being laid out,
    /// and in the last whole one (what a paragraph is highlighted by, as the
    /// ones after it aren't laid out yet when it is).
    order: HashMap<SharedString, u64>,
    last_order: HashMap<SharedString, u64>,
    texts: HashMap<SharedString, String>,
    seq: u64,
}

thread_local! {
    static SEL: RefCell<Selection> = RefCell::new(Selection::default());
}

/// A view laying out paragraphs from the top (the transcript) starts their
/// reading order afresh: only what is laid out now takes part, never a
/// paragraph of another chat, or one scrolled out of view.
pub fn begin_frame() {
    SEL.with(|c| {
        let mut s = c.borrow_mut();
        s.seq = 0;
        s.last_order = std::mem::take(&mut s.order);
        s.texts.clear();
    });
}

/// The selection's two ends in reading order: (start, end), each a place
/// and a byte. None when nothing is selected.
fn ends(s: &Selection, order: &HashMap<SharedString, u64>) -> Option<((u64, usize), (u64, usize))> {
    let (a, h) = (s.anchor.as_ref()?, s.head.as_ref()?);
    let at = |(k, i): &(SharedString, usize)| Some((*order.get(k)?, *i));
    let (a, h) = (at(a)?, at(h)?);
    if a == h {
        return None;
    }
    Some(if a < h { (a, h) } else { (h, a) })
}

/// The part of a paragraph (at reading place `me`, `len` bytes) selected.
fn part(
    s: &Selection,
    order: &HashMap<SharedString, u64>,
    me: u64,
    len: usize,
) -> Option<Range<usize>> {
    let ((so, si), (eo, ei)) = ends(s, order)?;
    if me < so || me > eo {
        return None;
    }
    let from = if me == so { si } else { 0 };
    let to = if me == eo { ei } else { len };
    (from < to).then(|| from.min(len)..to.min(len))
}

/// Register a paragraph and return its runs with the selected part
/// highlighted in `color`.
pub fn prepare(key: &SharedString, text: &str, runs: Vec<TextRun>, color: Hsla) -> Vec<TextRun> {
    SEL.with(|cell| {
        let mut s = cell.borrow_mut();
        let me = s.seq;
        s.seq += 1;
        s.order.insert(key.clone(), me);
        if s.texts.get(key).map(String::as_str) != Some(text) {
            s.texts.insert(key.clone(), text.to_string());
        }
        match part(&s, &s.last_order, me, text.len()) {
            Some(r) => highlight(runs, r, color),
            None => runs,
        }
    })
}

/// Split the runs at the range's ends and give the inside a background.
fn highlight(runs: Vec<TextRun>, r: Range<usize>, color: Hsla) -> Vec<TextRun> {
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut at = 0;
    for run in runs {
        let (start, end) = (at, at + run.len);
        at = end;
        let cuts = [
            start,
            r.start.clamp(start, end),
            r.end.clamp(start, end),
            end,
        ];
        for w in cuts.windows(2) {
            if w[1] > w[0] {
                let mut piece = run.clone();
                piece.len = w[1] - w[0];
                if w[0] >= r.start && w[1] <= r.end {
                    piece.background_color = Some(color);
                }
                out.push(piece);
            }
        }
    }
    out
}

/// The paragraph `el` (laid out by `layout`) made selectable: press to
/// start, drag across it and its neighbours to extend.
pub fn wrap(key: SharedString, layout: TextLayout, el: AnyElement) -> AnyElement {
    let (down, moved) = (layout.clone(), layout);
    let (k_down, k_move) = (key.clone(), key.clone());
    div()
        .id(key)
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, move |ev, window, _| {
            let ix = down.index_for_position(ev.position).unwrap_or_else(|i| i);
            SEL.with(|c| {
                let mut s = c.borrow_mut();
                s.anchor = Some((k_down.clone(), ix));
                s.head = Some((k_down.clone(), ix));
                s.dragging = true;
            });
            window.refresh();
        })
        .on_mouse_move(move |ev, window, _| {
            let pressed = ev.pressed_button == Some(MouseButton::Left);
            let moved_head = SEL.with(|c| {
                let mut s = c.borrow_mut();
                if !s.dragging {
                    return false;
                }
                if !pressed {
                    s.dragging = false;
                    return false;
                }
                let ix = moved.index_for_position(ev.position).unwrap_or_else(|i| i);
                let head = Some((k_move.clone(), ix));
                let changed = s.head != head;
                s.head = head;
                changed
            });
            if moved_head {
                window.refresh();
            }
        })
        .child(el)
        .into_any_element()
}

/// The selected text, paragraphs joined by line breaks.
pub fn selected_text() -> Option<String> {
    SEL.with(|c| {
        let s = c.borrow();
        let ((so, _), (eo, _)) = ends(&s, &s.order)?;
        let mut keys: Vec<(&SharedString, u64)> = s
            .order
            .iter()
            .filter(|(_, o)| **o >= so && **o <= eo)
            .map(|(k, o)| (k, *o))
            .collect();
        keys.sort_by_key(|(_, o)| *o);
        let mut parts = vec![];
        for (k, o) in keys {
            let Some(text) = s.texts.get(k) else {
                continue;
            };
            if let Some(r) = part(&s, &s.order, o, text.len()) {
                let r = floor(text, r.start)..floor(text, r.end);
                parts.push(text[r].to_string());
            }
        }
        (!parts.is_empty()).then(|| parts.join("\n"))
    })
}

/// A byte offset moved back to a character boundary.
fn floor(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Clear the selection (a press anywhere). True when there was one.
pub fn clear() -> bool {
    SEL.with(|c| {
        let mut s = c.borrow_mut();
        let had = ends(&s, &s.order).is_some() || ends(&s, &s.last_order).is_some();
        s.anchor = None;
        s.head = None;
        had
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: gpui::font("x"),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn a_selection_runs_across_paragraphs_in_reading_order() {
        begin_frame();
        let (a, b, c): (SharedString, SharedString, SharedString) =
            ("a".into(), "b".into(), "c".into());
        let color = gpui::black();
        prepare(&a, "first para", vec![run(10)], color);
        prepare(&b, "second", vec![run(6)], color);
        prepare(&c, "third one", vec![run(9)], color);
        // Another chat's paragraph, laid out earlier, takes no part.
        begin_frame();
        prepare(
            &"other".into(),
            "Explain this project",
            vec![run(20)],
            color,
        );
        begin_frame();
        prepare(&a, "first para", vec![run(10)], color);
        prepare(&b, "second", vec![run(6)], color);
        prepare(&c, "third one", vec![run(9)], color);
        // Dragged from inside the third back into the first.
        SEL.with(|s| {
            let mut s = s.borrow_mut();
            s.anchor = Some((c.clone(), 5));
            s.head = Some((a.clone(), 6));
        });
        assert_eq!(selected_text().as_deref(), Some("para\nsecond\nthird"));
        // The middle paragraph is highlighted whole, the first in part.
        begin_frame();
        let first = prepare(&a, "first para", vec![run(10)], color);
        assert_eq!(
            first
                .iter()
                .map(|r| (r.len, r.background_color.is_some()))
                .collect::<Vec<_>>(),
            [(6, false), (4, true)]
        );
        assert!(clear());
        assert_eq!(selected_text(), None);
    }
}
