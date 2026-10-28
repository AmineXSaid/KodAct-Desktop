//! Find in the file beside the chat: a field under the panel's header and
//! the matches under it, each with its place (slide, page, section or cell)
//! and the words around it. Clicking one brings its page to the top, or
//! picks its cell; Enter goes to the next. Escape (`workspace.rs`) or ×
//! closes the bar.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, FontWeight, IntoElement, Render, SharedString,
    StyledText, Subscription, TextRun, Window,
};

use super::icon;
use crate::doc_versions::Hit;
use crate::state::AppState;
use crate::theme::{ActiveTheme, Theme};

/// The most matches listed; the count still says how many there are.
const SHOWN: usize = 50;

pub struct DocFind {
    state: Entity<AppState>,
    input: Entity<InputState>,
    /// The match Enter went to last.
    at: Option<usize>,
    /// The bar was closed: focus the field when it opens again.
    was_open: bool,
    _subs: Vec<Subscription>,
}

impl DocFind {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in this file"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => {
                    this.at = None;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    let hits = this.hits(cx).0;
                    if !hits.is_empty() {
                        let next = this.at.map_or(0, |i| (i + 1) % hits.len());
                        this.at = Some(next);
                        this.go(&hits[next], cx);
                    }
                }
                _ => {}
            }),
        ];
        Self {
            state,
            input,
            at: None,
            was_open: false,
            _subs: subs,
        }
    }

    /// The matches in the panel's file, and how many there are in all.
    fn hits(&self, cx: &gpui::App) -> (Vec<Hit>, usize) {
        let query = self.input.read(cx).value().to_string();
        let s = self.state.read(cx);
        let Some(file) = s.doc.as_ref() else {
            return (vec![], 0);
        };
        if query.trim().is_empty() {
            return (vec![], 0);
        }
        match s.doc_preview.as_ref().map(|(_, r)| r) {
            Some(Ok(p)) if file.office().is_some() => match p.as_ref() {
                crate::office::Preview::Sheets(sheets) => {
                    crate::doc_versions::find_cells(sheets, &query, SHOWN)
                }
                crate::office::Preview::Doc(d) => {
                    crate::doc_versions::find_text(&d.text, &query, SHOWN)
                }
            },
            _ if file.office().is_none() => {
                crate::doc_versions::find_text(&file.content, &query, SHOWN)
            }
            _ => (vec![], 0),
        }
    }

    /// Show a match: its page at the top, its cell picked, or the text.
    fn go(&self, hit: &Hit, cx: &mut gpui::App) {
        let hit = hit.clone();
        self.state.update(cx, |s, cx| {
            if let Some((sheet, cell)) = hit.cell {
                s.select_sheet(sheet, cx);
                s.pick_cell(cell, false, cx);
            } else if let (Some(page), true) = (hit.page, s.doc_page_count() > 0) {
                s.show_doc_page(page, cx);
            } else if s.doc_page_count() > 0 && !s.doc_text {
                // A Word file's pages don't say where a section falls: its
                // text does.
                s.toggle_doc_text(cx);
            }
        });
    }
}

/// A match in its words, the match itself on the selection wash.
fn snippet(hit: &Hit, t: &Theme) -> StyledText {
    let font = gpui::font(t.font_sans.clone());
    let run = |len: usize, strong: bool| TextRun {
        len,
        font: if strong {
            gpui::Font {
                weight: FontWeight::SEMIBOLD,
                ..font.clone()
            }
        } else {
            font.clone()
        },
        color: if strong {
            t.colors.text_strong
        } else {
            t.colors.text_body
        },
        // A highlighter's wash, as find-in-page marks a match: it stays
        // visible on a selected row, whatever the theme selects with.
        background_color: strong.then_some(t.extra.warning_tint),
        underline: None,
        strikethrough: None,
    };
    let text = format!("{}{}{}", hit.before, hit.hit, hit.after);
    StyledText::new(text).with_runs(vec![
        run(hit.before.len(), false),
        run(hit.hit.len(), true),
        run(hit.after.len(), false),
    ])
}

impl Render for DocFind {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let open = self.state.read(cx).doc_finding;
        if !open {
            self.was_open = false;
            return div().into_any_element();
        }
        if !self.was_open {
            self.was_open = true;
            self.input.update(cx, |i, cx| i.focus(window, cx));
        }
        let (hits, total) = self.hits(cx);
        let query = self.input.read(cx).value().to_string();
        let hover = t.colors.hover;
        let count = if query.trim().is_empty() {
            String::new()
        } else if total == 0 {
            "No matches".into()
        } else if total == 1 {
            "1 match".into()
        } else {
            format!("{total} matches")
        };
        let closer = self.state.clone();
        let mut list = div()
            .id("doc-find-hits")
            .max_h(px(220.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for (i, hit) in hits.iter().enumerate() {
            let on = self.at == Some(i);
            let h = hit.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("doc-find-{i}")))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px(t.space.md)
                    .py(t.space.xs + px(2.))
                    .cursor_pointer()
                    .when(on, |d| crate::theme::selected(&t, d))
                    .when(!on, |d| d.hover(move |d| d.bg(hover)))
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(hit.place.clone()),
                    )
                    .child(
                        div()
                            .text_size(t.text.body_sm)
                            .line_height(relative(1.4))
                            .overflow_hidden()
                            .child(snippet(hit, &t)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.at = Some(i);
                        this.go(&h, cx);
                        cx.notify();
                    })),
            );
        }
        div()
            .flex_none()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(t.colors.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .child(icon(
                        "icons/search.svg",
                        t.size.icon_sm,
                        t.colors.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.input).appearance(false)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(count),
                    )
                    .child(
                        super::button::Button::icon("doc-find-close", "icons/x.svg", "Close find")
                            .compact()
                            .on_click(move |_, _, cx| {
                                closer.update(cx, |s, cx| s.toggle_doc_find(cx))
                            }),
                    ),
            )
            .when(!hits.is_empty(), |d| d.child(list.pb(t.space.xs)))
            .when(total > hits.len(), |d| {
                d.child(
                    div()
                        .px(t.space.md)
                        .pb(t.space.sm)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(format!("The first {} are listed.", hits.len())),
                )
            })
            .into_any_element()
    }
}
