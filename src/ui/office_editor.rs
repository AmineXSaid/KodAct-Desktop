//! "Edit text" for a Word file or a deck: its words as fields you can change.
//! A Word file is an outline of its paragraphs (headings set larger, list
//! items with a bullet); a deck is each slide's text boxes and its speaker
//! notes. A changed field marks the file Edited (`AppState::doc_words`); Save
//! writes the new words into the file (`office_edit.rs`: the first run of a
//! paragraph keeps its formatting) and the pages are drawn again.

use std::collections::BTreeMap;

use gpui::component::input::{InputEvent, Textarea, TextareaState};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window,
};

use crate::office::Kind;
use crate::office_edit::Para;
use crate::state::AppState;
use crate::theme::ActiveTheme;

/// One thing on the page: a label, or a field with the words it started with.
enum Item {
    Label(String),
    Field {
        original: String,
        field: Entity<TextareaState>,
        heading: Option<u8>,
        list: bool,
        /// In a table: (table, row, column).
        cell: Option<(usize, usize, usize)>,
    },
}

pub struct OfficeEditor {
    state: Entity<AppState>,
    /// The file the fields were made for (`doc_version_key`).
    key: Option<SharedString>,
    items: Vec<Item>,
    error: Option<String>,
    _subs: Vec<Subscription>,
    field_subs: Vec<Subscription>,
}

impl OfficeEditor {
    pub fn new(state: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![cx.observe(&state, |_, _, cx| cx.notify())];
        Self {
            state,
            key: None,
            items: vec![],
            error: None,
            _subs: subs,
            field_subs: vec![],
        }
    }

    fn field(
        &mut self,
        key: (usize, bool, usize),
        p: &Para,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Item {
        let text = p.text.clone();
        let field = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 12)
                .default_value(text.clone())
        });
        let original = p.text.clone();
        let orig = original.clone();
        self.field_subs.push(
            cx.subscribe(&field, move |this, field, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    let now = field.read(cx).value().to_string();
                    let edit = (now != orig).then_some(now);
                    this.state.update(cx, |s, cx| s.set_doc_word(key, edit, cx));
                }
            }),
        );
        // Focus moves redraw the field's frame.
        let handle = gpui::Focusable::focus_handle(field.read(cx), cx);
        self.field_subs
            .push(cx.on_focus_in(&handle, window, |_, _, cx| cx.notify()));
        self.field_subs
            .push(cx.on_focus_out(&handle, window, |_, _, _, cx| cx.notify()));
        Item::Field {
            original,
            field,
            heading: p.heading,
            list: p.list,
            cell: p.table,
        }
    }

    /// The fields for `file`.
    fn open(&mut self, file: &crate::state::FileRef, window: &mut Window, cx: &mut Context<Self>) {
        self.items.clear();
        self.field_subs.clear();
        self.error = None;
        let Some(bytes) = file.bytes.clone() else {
            return;
        };
        match file.office() {
            Some(Kind::Docx) => match crate::office_edit::read_docx(&bytes) {
                Ok(paras) => {
                    for p in paras.iter().filter(|p| !p.text.trim().is_empty()) {
                        let item = self.field((0, false, p.index), p, window, cx);
                        self.items.push(item);
                    }
                }
                Err(e) => self.error = Some(e),
            },
            Some(Kind::Pptx) => match crate::office_edit::read_pptx(&bytes) {
                Ok(slides) => {
                    for (i, s) in slides.iter().enumerate() {
                        self.items.push(Item::Label(format!("Slide {}", i + 1)));
                        for p in &s.paras {
                            let item = self.field((i, false, p.index), p, window, cx);
                            self.items.push(item);
                        }
                        if !s.notes.is_empty() {
                            self.items.push(Item::Label("Speaker notes".into()));
                        }
                        for p in &s.notes {
                            let item = self.field((i, true, p.index), p, window, cx);
                            self.items.push(item);
                        }
                    }
                }
                Err(e) => self.error = Some(e),
            },
            _ => {}
        }
        self.key = Some(crate::state::doc_version_key(file));
    }
}

/// An edit map for a Word file, keyed by paragraph.
pub fn docx_edits(words: &BTreeMap<(usize, bool, usize), String>) -> BTreeMap<usize, String> {
    words.iter().map(|(k, v)| (k.2, v.clone())).collect()
}

/// An edit map for a deck.
pub fn pptx_edits(
    words: &BTreeMap<(usize, bool, usize), String>,
) -> BTreeMap<crate::office_edit::SlideKey, String> {
    words
        .iter()
        .map(|(k, v)| {
            (
                crate::office_edit::SlideKey {
                    slide: k.0,
                    notes: k.1,
                    para: k.2,
                },
                v.clone(),
            )
        })
        .collect()
}

impl Render for OfficeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let (file, words) = {
            let s = self.state.read(cx);
            (s.doc.clone(), s.doc_words.is_empty())
        };
        let Some(file) = file.filter(|f| matches!(f.office(), Some(Kind::Docx | Kind::Pptx)))
        else {
            return div().into_any_element();
        };
        let key = crate::state::doc_version_key(&file);
        if self.key.as_ref() != Some(&key) {
            self.open(&file, window, cx);
        }
        // Discard: the state dropped the edits, so the words go back.
        if words {
            for item in &self.items {
                if let Item::Field {
                    original, field, ..
                } = item
                {
                    if field.read(cx).value().as_ref() != original.as_str() {
                        let original = original.clone();
                        field.update(cx, |f, cx| f.set_value(original, window, cx));
                    }
                }
            }
        }
        if let Some(e) = &self.error {
            return div()
                .p(t.space.xl)
                .text_size(t.text.body_sm)
                .text_color(t.colors.text_muted)
                .child(e.clone())
                .into_any_element();
        }
        let mut col = div()
            .w_full()
            .max_w(t.size.doc_measure)
            .flex()
            .flex_col()
            .gap(t.space.xs);
        // One field: set in the reading face at the document's size, framed
        // on hover and ringed while it is typed in.
        let field_view = |field: &Entity<TextareaState>,
                          heading: Option<u8>,
                          list: bool,
                          window: &mut Window,
                          cx: &mut Context<Self>| {
            let size = match heading {
                Some(1) => t.text.subheading,
                Some(2) => t.text.body + px(2.),
                _ => t.size.doc_text - px(1.),
            };
            let focused = gpui::Focusable::focus_handle(field.read(cx), cx).is_focused(window);
            let hover = t.colors.border;
            div()
                .flex()
                .items_start()
                .gap(t.space.xs)
                .pt(if heading.is_some() {
                    t.space.md
                } else {
                    px(0.)
                })
                .children(list.then(|| {
                    div()
                        .flex_none()
                        .pt(px(7.))
                        .w(t.space.md)
                        .text_color(t.colors.text_muted)
                        .child("•")
                }))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "office-field-{}",
                            field.entity_id()
                        )))
                        .flex_1()
                        .min_w_0()
                        .px(t.space.xs)
                        .rounded(t.radius.small)
                        .border_1()
                        .border_color(if focused {
                            t.colors.primary
                        } else {
                            gpui::transparent_black()
                        })
                        .when(!focused, |d| d.hover(move |s| s.border_color(hover)))
                        .font_family(t.font_reading.clone())
                        .text_size(size)
                        .line_height(relative(t.size.line_height_doc))
                        .text_color(t.colors.text_body)
                        .font_weight(if heading.is_some() {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .child(
                            Textarea::new(field)
                                .appearance(false)
                                .font_family(t.font_reading.clone())
                                .text_size(size)
                                .line_height(relative(t.size.line_height_doc)),
                        ),
                )
                .into_any_element()
        };
        let items: Vec<(usize, &Item)> = self.items.iter().enumerate().collect();
        let mut i = 0;
        while i < items.len() {
            match items[i].1 {
                Item::Label(text) => {
                    col = col.child(
                        div()
                            .pt(t.space.lg)
                            .text_size(t.text.caption)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_muted)
                            .child(text.clone()),
                    );
                    i += 1;
                }
                Item::Field {
                    cell: Some((tbl, ..)),
                    ..
                } => {
                    // A table: its cells as a grid of fields, row by row.
                    let tbl = *tbl;
                    let mut cells: BTreeMap<(usize, usize), Vec<AnyElement>> = BTreeMap::new();
                    while let Some(Item::Field {
                        cell: Some((t2, r, c)),
                        field,
                        heading,
                        list,
                        ..
                    }) = items.get(i).map(|x| x.1)
                    {
                        if *t2 != tbl {
                            break;
                        }
                        cells
                            .entry((*r, *c))
                            .or_default()
                            .push(field_view(field, *heading, *list, window, cx));
                        i += 1;
                    }
                    let rows = cells.keys().map(|k| k.0).max().map_or(0, |m| m + 1);
                    let mut grid = div()
                        .flex()
                        .flex_col()
                        .my(t.space.sm)
                        .border_1()
                        .border_color(t.colors.border)
                        .rounded(t.radius.small);
                    for r in 0..rows {
                        let mut row = div().flex().border_t_1().border_color(t.colors.border);
                        if r == 0 {
                            row = row.border_t_0();
                        }
                        let in_row: Vec<_> =
                            cells.keys().filter(|k| k.0 == r).map(|k| k.1).collect();
                        for c in in_row {
                            let content = cells.remove(&(r, c)).unwrap_or_default();
                            row = row.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .p(t.space.xs)
                                    .border_l_1()
                                    .border_color(t.colors.border)
                                    .flex()
                                    .flex_col()
                                    .children(content),
                            );
                        }
                        grid = grid.child(row);
                    }
                    col = col.child(grid);
                }
                Item::Field {
                    field,
                    heading,
                    list,
                    ..
                } => {
                    col = col.child(field_view(field, *heading, *list, window, cx));
                    i += 1;
                }
            }
        }
        div()
            .flex()
            .justify_center()
            .px(t.space.xxl)
            .pt(t.space.xl)
            .pb(t.space.xxl * 2.)
            .child(col)
            .into_any_element()
    }
}
