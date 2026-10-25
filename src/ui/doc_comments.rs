//! Commenting on a file in the side panel, as in Claude Docs and Claude's
//! artifacts: select words (or cells, or a page), say what should change,
//! and either keep it as a comment or ask Forge to change it at once.
//!
//! - What you picked is held while you type, so clicking into the box does
//!   not lose it.
//! - Comments gather under the file, each with its place and quote. Send
//!   sends them all as one message (`doc_versions::comments_message`):
//!   change only these parts, keep the rest, write the file back.
//! - The reply's new version of the file answers them: each says so, with
//!   a link to that version.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window,
};

use super::icon;
use crate::doc_versions::Anchor;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme};

pub struct DocComments {
    state: Entity<AppState>,
    input: Entity<InputState>,
    /// What the next comment is on: kept from the selection as you type.
    held: Option<Anchor>,
    _subs: Vec<Subscription>,
}

impl DocComments {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What should change here?"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, .. } = ev {
                    // Enter keeps a comment; Ctrl/⌘ Enter asks Forge at once.
                    this.submit(*secondary, window, cx);
                }
            }),
        ];
        Self {
            state,
            input,
            held: None,
            _subs: subs,
        }
    }

    /// What is picked in the panel right now: selected words, cells, or a page.
    fn picked(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Anchor> {
        let selected = gpui::base::TextSelection::selected_text(window, cx);
        let s = self.state.read(cx);
        let file = s.doc.as_ref()?;
        if !selected.trim().is_empty() {
            let text = match s.doc_preview.as_ref().map(|(_, r)| r) {
                Some(Ok(p)) => match p.as_ref() {
                    crate::office::Preview::Doc(d) => d.text.clone(),
                    _ => String::new(),
                },
                _ => file.content.to_string(),
            };
            return Some(Anchor {
                place: crate::doc_versions::place_of(&text, &selected),
                quote: selected,
            });
        }
        if let (Some(pick), Some(Ok(p))) = (s.doc_pick, s.doc_preview.as_ref().map(|(_, r)| r)) {
            if let crate::office::Preview::Sheets(sheets) = p.as_ref() {
                let sheet = sheets.get(s.doc_sheet)?;
                let ((r0, c0), (r1, c1)) = pick;
                let values: Vec<String> = (r0.min(r1)..=r0.max(r1))
                    .filter_map(|r| sheet.rows.get(r))
                    .map(|row| {
                        (c0.min(c1)..=c0.max(c1))
                            .filter_map(|c| row.get(c))
                            .map(|cell| cell.text.clone())
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .take(12)
                    .collect();
                let name = if sheet.name.contains(' ') {
                    format!("'{}'", sheet.name)
                } else {
                    sheet.name.clone()
                };
                return Some(Anchor {
                    place: format!("{name}!{}", super::sheet_view::address(pick)),
                    quote: values.join(" / "),
                });
            }
        }
        let page = s.doc_page_pick?;
        let slides = matches!(
            s.doc_preview.as_ref().map(|(_, r)| r),
            Some(Ok(p)) if matches!(p.as_ref(), crate::office::Preview::Doc(d) if d.slides)
        );
        Some(Anchor {
            place: format!("{} {}", if slides { "Slide" } else { "Page" }, page + 1),
            quote: String::new(),
        })
    }

    fn submit(&mut self, ask_now: bool, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.input.read(cx).value().to_string();
        let Some(anchor) = self.held.take() else {
            return;
        };
        if note.trim().is_empty() {
            self.held = Some(anchor);
            return;
        }
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
        gpui::base::TextSelection::clear(window, cx);
        self.state.update(cx, |s, cx| {
            if ask_now {
                s.ask_doc_change(anchor, note, cx)
            } else {
                s.add_doc_comment(anchor, note, cx)
            }
        });
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.held = None;
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
        gpui::base::TextSelection::clear(window, cx);
        self.state.update(cx, |s, cx| {
            s.doc_pick = None;
            s.doc_page_pick = None;
            cx.notify();
        });
    }
}

/// A quote, cut to a line or two.
fn short(quote: &str, max: usize) -> String {
    let flat: String = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max {
        format!("{}…", flat.chars().take(max - 1).collect::<String>())
    } else {
        flat
    }
}

impl Render for DocComments {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if let Some(now) = self.picked(window, cx) {
            self.held = Some(now);
        }
        let s = self.state.read(cx);
        let comments = s.doc_comments_here();
        let (versions, _) = s.doc_versions();
        let word = s.doc.as_ref().and_then(|f| f.office()) == Some(crate::office::Kind::Docx);
        let word_on = s.doc_word_comments;
        let busy = s.busy();
        let open = comments.iter().filter(|c| c.sent.is_none()).count();
        if self.held.is_none() && comments.is_empty() {
            return div().into_any_element();
        }
        let hover = t.colors.hover;

        // What you picked, and the box to say what should change.
        let composer =
            self.held.clone().map(|a| {
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.sm)
                    .p(t.space.md)
                    .rounded(t.radius.card)
                    .bg(t.colors.surface)
                    .border_1()
                    .border_color(t.colors.primary.opacity(0.5))
                    .shadow(t.elevation_popover())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(icon(
                                "icons/message-square.svg",
                                t.size.icon_xs,
                                t.colors.primary,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(format!("Comment on {}", a.place)),
                            )
                            .child(
                                div()
                                    .id("comment-cancel")
                                    .cursor_pointer()
                                    .rounded(t.radius.small)
                                    .hover(move |st| st.bg(hover))
                                    .child(icon("icons/x.svg", t.size.icon_sm, t.colors.text_muted))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                                    ),
                            ),
                    )
                    .when(!a.quote.trim().is_empty(), |d| {
                        d.child(
                            div()
                                .pl(t.space.sm)
                                .border_l_2()
                                .border_color(t.colors.primary)
                                .text_size(t.text.body_sm)
                                .italic()
                                .text_color(t.colors.text_body)
                                .line_height(relative(1.45))
                                .child(short(&a.quote, 160)),
                        )
                    })
                    .child(
                        div()
                            .rounded(t.radius.field)
                            .border_1()
                            .border_color(t.colors.border)
                            .px(t.space.sm)
                            .child(
                                Input::new(&self.input)
                                    .appearance(false)
                                    .text_size(t.text.body_sm),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(t.space.sm)
                            .child(
                                theme::ghost_button(&t, "Comment")
                                    .id("comment-add")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit(false, window, cx)
                                    })),
                            )
                            .child(
                                theme::primary_button(&t, "Ask Forge to change this")
                                    .id("comment-ask")
                                    .when(busy, |d| d.opacity(0.5))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit(true, window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(format!(
                                "Enter keeps it as a comment · {} asks Forge now",
                                super::shortcut("Enter")
                            )),
                    )
            });

        // The file's comments, and Send.
        let list = (!comments.is_empty()).then(|| {
            let mut list = div().flex().flex_col().gap(t.space.xs);
            for c in &comments {
                let id = c.id;
                let st = self.state.clone();
                let status: Option<(String, gpui::Hsla)> = match c.sent {
                    None => None,
                    Some(n) if versions.len() > n => {
                        Some((format!("Answered in version {}", n + 1), t.extra.success))
                    }
                    Some(_) => Some((
                        "Sent · waiting for the new version".into(),
                        t.colors.text_muted,
                    )),
                };
                list = list.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(t.space.sm)
                        .px(t.space.sm)
                        .py(t.space.xs + px(2.))
                        .rounded(t.radius.button)
                        .bg(t.colors.surface)
                        .border_1()
                        .border_color(t.colors.border)
                        .child(div().pt(px(2.)).child(icon(
                            if status.as_ref().is_some_and(|(_, c)| *c == t.extra.success) {
                                "icons/circle-check.svg"
                            } else {
                                "icons/message-square.svg"
                            },
                            t.size.icon_xs,
                            t.colors.text_muted,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .child(if c.anchor.quote.trim().is_empty() {
                                            c.anchor.place.clone()
                                        } else {
                                            format!(
                                                "{} · “{}”",
                                                c.anchor.place,
                                                short(&c.anchor.quote, 60)
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(t.text.body_sm)
                                        .text_color(t.colors.text)
                                        .child(c.note.clone()),
                                )
                                .children(status.map(|(text, color)| {
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(color)
                                        .child(text)
                                })),
                        )
                        .when(c.sent.is_none(), |d| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("comment-remove-{id}")))
                                    .cursor_pointer()
                                    .rounded(t.radius.small)
                                    .hover(move |st| st.bg(hover))
                                    .child(icon("icons/x.svg", t.size.icon_xs, t.colors.text_muted))
                                    .on_click(move |_, _, cx| {
                                        st.update(cx, |s, cx| s.remove_doc_comment(id, cx))
                                    }),
                            )
                        }),
                );
            }
            let st = self.state.clone();
            let toggler = self.state.clone();
            list.when(open > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .pt(t.space.xs)
                        .when(word, |d| {
                            d.child(
                                div()
                                    .id("word-comments")
                                    .flex()
                                    .items_center()
                                    .gap(t.space.xs)
                                    .cursor_pointer()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .child(icon(
                                        if word_on {
                                            "icons/square-check.svg"
                                        } else {
                                            "icons/square.svg"
                                        },
                                        t.size.icon_sm,
                                        if word_on {
                                            t.colors.primary
                                        } else {
                                            t.colors.text_muted
                                        },
                                    ))
                                    .child("Also put them in the file as Word comments")
                                    .on_click(move |_, _, cx| {
                                        toggler.update(cx, |s, cx| s.toggle_word_comments(cx))
                                    }),
                            )
                        })
                        .child(div().flex_1())
                        .child(
                            theme::primary_button(
                                &t,
                                if open == 1 {
                                    "Send 1 comment".to_string()
                                } else {
                                    format!("Send {open} comments")
                                },
                            )
                            .id("comments-send")
                            .when(busy, |d| d.opacity(0.5))
                            .on_click(move |_, _, cx| {
                                st.update(cx, |s, cx| s.send_doc_comments(cx))
                            }),
                        ),
                )
            })
        });

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(t.space.sm)
            .p(t.space.sm)
            .border_t_1()
            .border_color(t.colors.border)
            .bg(t.extra.inset)
            .max_h(px(360.))
            .id("doc-comments")
            .overflow_y_scroll()
            .children(composer)
            .when(!comments.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_muted)
                        .child(format!("Comments · {}", comments.len())),
                )
            })
            .children(list)
            .into_any_element()
    }
}
