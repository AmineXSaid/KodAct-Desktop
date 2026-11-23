//! "Edit text" for a Word file or a deck: its words as fields you can change.
//! A Word file is an outline of its paragraphs (headings set larger, list
//! items with a bullet); a deck is each slide's text boxes and its speaker
//! notes. A changed field marks the file Edited (`AppState::doc_words`); Save
//! writes the new words into the file (`office_edit.rs`: the first run of a
//! paragraph keeps its formatting) and the pages are drawn again.

use std::collections::BTreeMap;

use gpui::component::input::{InputEvent, Textarea, TextareaState};
use gpui::{
    div, prelude::*, px, Context, Entity, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window,
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
        Item::Field {
            original,
            field,
            heading: p.heading,
            list: p.list,
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
        for item in &self.items {
            col = match item {
                Item::Label(text) => col.child(
                    div()
                        .pt(t.space.lg)
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_muted)
                        .child(text.clone()),
                ),
                Item::Field {
                    field,
                    heading,
                    list,
                    ..
                } => {
                    let size = match heading {
                        Some(1) => t.text.subheading,
                        Some(2) => t.text.body + px(2.),
                        Some(_) => t.text.body,
                        None => t.text.body_sm,
                    };
                    col.child(
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
                                    .pt(px(6.))
                                    .w(t.space.md)
                                    .text_color(t.colors.text_muted)
                                    .child("•")
                            }))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(size)
                                    .font_weight(if heading.is_some() {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(Textarea::new(field).appearance(false)),
                            ),
                    )
                }
            };
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
