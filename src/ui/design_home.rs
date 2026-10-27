//! The design gallery: Open Design's library to start from (templates,
//! examples, design systems, skills), each a card with a picture of its
//! example page. A card's Use starts a design chat set up with it and its
//! ready-made request in the composer; Preview shows its page in the side
//! panel. The pictures are taken in the background by the export browser
//! (`design::export::thumbnail`) and cached, so the gallery opens at once
//! and fills in.

use std::path::PathBuf;

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, img, prelude::*, px, Context, Entity, FontWeight, IntoElement, ObjectFit, Render,
    SharedString, Subscription, Window,
};

use super::icon;
use crate::design::library::{Entry, Kind};
use crate::state::{AppState, GalleryTab, Thumb};
use crate::theme::{self, ActiveTheme, Theme};

/// Cards shown at first; Show more adds as many again.
const PAGE: usize = 48;
/// A card's least width; the columns share what is left. Pictures are
/// 16:10.
const CARD_MIN: f32 = 232.;

pub struct DesignHome {
    state: Entity<AppState>,
    search: Entity<InputState>,
    shown: usize,
    /// The previews last asked to be pictured, in order.
    asked: Vec<PathBuf>,
    /// The cards' area's width, as last laid out.
    width: std::rc::Rc<std::cell::Cell<f32>>,
    _subs: Vec<Subscription>,
}

impl DesignHome {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search: pitch deck, editorial, dashboard…")
        });
        let subs = vec![
            cx.subscribe(&search, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.shown = PAGE;
                    cx.notify();
                }
            }),
            cx.observe(&state, |_, _, cx| cx.notify()),
        ];
        Self {
            state,
            search,
            shown: PAGE,
            asked: vec![],
            width: Default::default(),
            _subs: subs,
        }
    }
}

/// Whether `e` matches every word of `query`.
fn matches(e: &Entry, query: &str) -> bool {
    let hay = format!(
        "{} {} {} {}",
        e.title,
        e.description,
        e.tags.join(" "),
        e.id
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

fn card(
    t: &Theme,
    width: f32,
    e: &Entry,
    pic: Option<&Thumb>,
    preview: Option<PathBuf>,
    state: &Entity<AppState>,
) -> impl IntoElement {
    let kind = e.kind.unwrap_or(Kind::Template);
    let id = e.id.clone();
    let hover = t.colors.text_subtle;
    let picture = div()
        .w_full()
        .h(px(width * 0.625))
        .flex_none()
        .overflow_hidden()
        .rounded_t(t.radius.card)
        .bg(t.colors.hover)
        .flex()
        .items_center()
        .justify_center()
        .child(match pic {
            Some(Thumb::Ready(p)) => img(p.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            Some(Thumb::Queued) => div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child("Drawing…")
                .into_any_element(),
            _ => icon(
                match kind {
                    Kind::DesignSystem => "icons/sliders-horizontal.svg",
                    Kind::Skill => "icons/wand-sparkles.svg",
                    _ => "icons/layers.svg",
                },
                t.size.icon_lg,
                t.colors.text_subtle,
            )
            .into_any_element(),
        });
    let use_it = {
        let (state, id) = (state.clone(), id.clone());
        theme::text_button(t, "Use")
            .id(SharedString::from(format!("use-{kind:?}-{id}")))
            .text_color(t.colors.primary)
            .on_click(move |_, _, cx| state.update(cx, |s, cx| s.design_start_from(kind, &id, cx)))
    };
    let look = preview.map(|p| {
        let state = state.clone();
        theme::text_button(t, "Preview")
            .id(SharedString::from(format!("look-{kind:?}-{id}")))
            .on_click(move |_, _, cx| state.update(cx, |s, cx| s.design_preview(&p, cx)))
    });
    div()
        .id(SharedString::from(format!("card-{kind:?}-{}", e.id)))
        .w(px(width))
        .flex_none()
        .flex()
        .flex_col()
        .rounded(t.radius.card)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .hover(move |d| d.border_color(hover))
        .child(picture)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .px(t.space.md)
                .pt(t.space.sm)
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text_strong)
                        .child(e.title.clone()),
                )
                .child(
                    div()
                        .h(px(36.))
                        .overflow_hidden()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(e.description.clone()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(t.space.xs)
                .px(t.space.sm)
                .py(t.space.xs)
                .children(look)
                .child(use_it),
        )
}

impl Render for DesignHome {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let state = self.state.clone();
        let query = self.search.read(cx).value().to_string();
        let s = state.read(cx);
        let tab = s.gallery_tab;
        let ix = s.design_index.clone();

        let header = div()
            .flex()
            .items_end()
            .gap(t.space.lg)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(t.space.xs)
                    .child(
                        div()
                            .font_family(t.font_display.clone())
                            .font_weight(t.display_weight)
                            .text_size(t.size.greeting)
                            .text_color(t.colors.text_strong)
                            .child("Design"),
                    )
                    .child(
                        div()
                            .font_family(t.font_serif.clone())
                            .text_size(t.text.subheading)
                            .text_color(t.colors.text_body)
                            .child("Start from a template, an example, a design system or a skill. Forge sets up the chat, and the page shows beside it as it's made."),
                    ),
            );

        let search = div()
            .w(px(280.))
            .flex_none()
            .px(t.space.sm)
            .rounded(t.radius.button)
            .border_1()
            .border_color(t.colors.border)
            .bg(t.colors.surface)
            .flex()
            .items_center()
            .gap(t.space.xs)
            .child(icon(
                "icons/search.svg",
                t.size.icon_sm,
                t.colors.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&self.search).appearance(false)),
            );

        let Some(ix) = ix else {
            return div()
                .size_full()
                .p(t.space.xxl)
                .flex()
                .flex_col()
                .gap(t.space.xl)
                .child(header)
                .child(
                    div()
                        .text_color(t.colors.text_muted)
                        .child(if crate::design::root().is_some() {
                            "Reading the design library…"
                        } else {
                            "Forge's design library isn't installed: put design-library beside Forge, or set FORGE_DESIGN_LIB."
                        }),
                )
                .into_any_element();
        };

        // The tab's entries: featured first, then by name.
        let mut entries: Vec<&Entry> = ix
            .of(tab.kind())
            .filter(|e| !e.hidden && matches(e, &query))
            .collect();
        entries.sort_by(|a, b| {
            let f = |e: &Entry| e.featured.unwrap_or(f64::MAX);
            f(a).total_cmp(&f(b))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        let total = entries.len();
        entries.truncate(self.shown);
        let previews: Vec<Option<PathBuf>> = entries
            .iter()
            .map(|e| {
                e.preview
                    .as_ref()
                    .filter(|p| p.extension().is_some_and(|x| x == "html"))
                    .map(|p| ix.root.join(p))
            })
            .collect();
        let pics: Vec<Option<Thumb>> = previews
            .iter()
            .map(|p| p.as_ref().and_then(|p| s.design_thumbs.get(p).cloned()))
            .collect();
        let counts: Vec<(GalleryTab, usize)> = GalleryTab::ALL
            .iter()
            .map(|g| (*g, ix.of(g.kind()).filter(|e| !e.hidden).count()))
            .collect();
        // Ask for the pictures of what shows, in order, when that changes.
        let wanted: Vec<PathBuf> = previews.iter().flatten().cloned().collect();
        if wanted != self.asked {
            self.asked = wanted.clone();
            let st = state.clone();
            cx.defer(move |cx| st.update(cx, |s, cx| s.design_want_thumbs(wanted, cx)));
        }

        let tabs = div()
            .flex()
            .items_center()
            .gap(t.space.xs)
            .children(counts.into_iter().map(|(g, n)| {
                let on = g == tab;
                let hover = t.colors.hover;
                let st = state.clone();
                div()
                    .id(SharedString::from(format!("gallery-{g:?}")))
                    .flex()
                    .items_center()
                    .gap(t.space.tight)
                    .h(t.size.button_sm)
                    .px(t.space.md)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .text_color(if on {
                        t.colors.text_strong
                    } else {
                        t.colors.text_muted
                    })
                    .when(on, |d| theme::selected(&t, d))
                    .when(!on, |d| d.hover(move |st| st.bg(hover)))
                    .child(g.label())
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(n.to_string()),
                    )
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.set_gallery_tab(g, cx)))
            }));

        // As many columns as fit, sharing the width.
        let gap = f32::from(t.space.lg);
        let room = self.width.get().max(CARD_MIN);
        let cols = ((room + gap) / (CARD_MIN + gap)).floor().max(1.);
        let card_w = ((room - gap * (cols - 1.)) / cols).floor();
        // A Figma file, opened as a page to keep designing.
        let from_figma = {
            let st = state.clone();
            theme::ghost_button(&t, "Open a Figma file…")
                .id("gallery-figma")
                .on_click(move |_, _, cx| {
                    let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: false,
                        prompt: Some("Open".into()),
                    });
                    let st = st.clone();
                    cx.spawn(async move |cx| {
                        if let Ok(Ok(Some(paths))) = picked.await {
                            if let Some(p) = paths.into_iter().next() {
                                st.update(cx, |s, cx| s.design_import_fig(p, cx));
                            }
                        }
                    })
                    .detach();
                })
        };
        // A design system of your own, from a website.
        let from_site = (tab == GalleryTab::Systems).then(|| {
            let st = state.clone();
            theme::ghost_button(&t, "From a website…")
                .id("gallery-from-site")
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.design_from_website(cx)))
        });
        let cards: Vec<_> = entries
            .iter()
            .zip(previews.into_iter().zip(pics))
            .map(|(e, (p, pic))| card(&t, card_w, e, pic.as_ref(), p, &state))
            .collect();
        let more = (total > self.shown).then(|| {
            theme::ghost_button(&t, format!("Show more ({} left)", total - self.shown))
                .id("gallery-more")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.shown += PAGE;
                    cx.notify();
                }))
        });
        let empty = (total == 0).then(|| {
            div()
                .text_color(t.colors.text_muted)
                .child(format!("Nothing in {} matches \"{query}\".", tab.label()))
        });

        div()
            .id("design-home")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .px(t.space.xxl)
                    .py(t.space.xl)
                    .flex()
                    .flex_col()
                    .gap(t.space.lg)
                    .child(header)
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(t.space.md)
                            .child(tabs)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(t.space.sm)
                                    .children(from_site)
                                    .child(from_figma)
                                    .child(search),
                            ),
                    )
                    .children(empty)
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_wrap()
                            .gap(t.space.lg)
                            .children(cards)
                            .child(measure(self.width.clone())),
                    )
                    .child(div().flex().justify_center().children(more)),
            )
            .into_any_element()
    }
}

/// An empty layer over the cards that notes their area's width, and draws
/// again when it changed (the window or the side panel was resized).
fn measure(width: std::rc::Rc<std::cell::Cell<f32>>) -> impl IntoElement {
    gpui::canvas(
        move |bounds, window, _| {
            let w = f32::from(bounds.size.width);
            if (w - width.get()).abs() > 0.5 {
                width.set(w);
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h(px(1.))
}
