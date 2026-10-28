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
    /// The routines list shows, above the cards.
    routines_open: bool,
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
            routines_open: false,
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
                .child(crate::i18n::tr("Drawing…"))
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
        theme::text_button(t, crate::i18n::tr("Use"))
            .id(SharedString::from(format!("use-{kind:?}-{id}")))
            .text_color(t.colors.primary)
            .on_click(move |_, _, cx| state.update(cx, |s, cx| s.design_start_from(kind, &id, cx)))
    };
    let look = preview.map(|p| {
        let state = state.clone();
        theme::text_button(t, crate::i18n::tr("Preview"))
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
        let routines = s.routines.clone();
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
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(t.text.heading_sm)
                            .text_color(t.colors.text_strong)
                            .child(crate::i18n::tr("Design")),
                    )
                    .child(
                        div()
                            .text_size(t.text.body)
                            .text_color(t.colors.text_muted)
                            .child(crate::i18n::tr("Start from a template, an example, a design system or a skill. Forge sets up the chat, and the page shows beside it as it's made.")),
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
                            crate::i18n::tr("Reading the design library…")
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

        // The four lists: one choice on a track, as the app's other views.
        let tabs = counts.into_iter().fold(
            super::button::Segmented::new("gallery-tabs", crate::i18n::tr("Library")),
            |seg, (g, n)| {
                let st = state.clone();
                seg.option(format!("{} {n}", g.label()), g == tab, move |_, _, cx| {
                    st.update(cx, |s, cx| s.set_gallery_tab(g, cx))
                })
            },
        );

        // As many columns as fit, sharing the width.
        let gap = f32::from(t.space.lg);
        let room = self.width.get().max(CARD_MIN);
        let cols = ((room + gap) / (CARD_MIN + gap)).floor().max(1.);
        let card_w = ((room - gap * (cols - 1.)) / cols).floor();
        // A Figma file, opened as a page to keep designing.
        let from_figma = {
            let st = state.clone();
            theme::ghost_button(&t, crate::i18n::tr("Open a Figma file…"))
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
        // Routines: requests Forge runs on a schedule.
        let routines_button = theme::ghost_button(
            &t,
            format!("{} ({})", crate::i18n::tr("Routines"), routines.len()),
        )
        .id("gallery-routines")
        .on_click(cx.listener(|this, _, _, cx| {
            this.routines_open = !this.routines_open;
            cx.notify();
        }));
        let routines_list = self.routines_open.then(|| {
            let hover = t.colors.hover;
            let rows = routines.iter().map(|r| {
                let (run, flip, del) = (state.clone(), state.clone(), state.clone());
                let (a, b, c) = (r.id.clone(), r.id.clone(), r.id.clone());
                div()
                    .id(SharedString::from(format!("routine-{}", r.id)))
                    .flex()
                    .items_center()
                    .gap(t.space.md)
                    .px(t.space.md)
                    .py(t.space.sm)
                    .rounded(t.radius.button)
                    .hover(move |d| d.bg(hover))
                    .child(icon("icons/clock.svg", t.size.icon_sm, if r.enabled { t.colors.primary } else { t.colors.text_subtle }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.colors.text_strong).child(r.name.clone()))
                            .child(
                                div()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .child(format!(
                                        "{}{}",
                                        r.every.say(),
                                        r.project
                                            .as_ref()
                                            .and_then(|p| p.file_name())
                                            .map(|n| format!(" · {}", n.to_string_lossy()))
                                            .unwrap_or_default()
                                    )),
                            ),
                    )
                    .child(theme::text_button(&t, crate::i18n::tr("Run now")).id(SharedString::from(format!("run-{a}"))).on_click(move |_, _, cx| run.update(cx, |s, cx| s.run_routine(&a, cx))))
                    .child(theme::text_button(&t, if r.enabled { crate::i18n::tr("Turn off") } else { crate::i18n::tr("Turn on") }).id(SharedString::from(format!("flip-{b}"))).on_click(move |_, _, cx| flip.update(cx, |s, cx| s.toggle_routine(&b, cx))))
                    .child(theme::text_button(&t, crate::i18n::tr("Delete")).id(SharedString::from(format!("del-{c}"))).on_click(move |_, _, cx| del.update(cx, |s, cx| s.delete_routine(&c, cx))))
            });
            let st = state.clone();
            div()
                .flex()
                .flex_col()
                .gap(t.space.xs)
                .p(t.space.md)
                .rounded(t.radius.card)
                .border_1()
                .border_color(t.colors.border)
                .bg(t.colors.surface)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(t.space.md)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(crate::i18n::tr("Requests Forge runs on a schedule, while it's open, as a new chat in their project. Make one in the composer: /routine weekly on monday at 9:00: refresh the metrics deck")),
                        )
                        .child(
                            theme::ghost_button(&t, crate::i18n::tr("New routine…"))
                                .flex_none()
                                .id("routine-new")
                                .on_click(move |_, _, cx| {
                                    st.update(cx, |s, cx| {
                                        s.screen = crate::state::Screen::Chat;
                                        s.draft = Some("/routine weekly on monday at 9:00: ".into());
                                        cx.notify();
                                    })
                                }),
                        ),
                )
                .children(rows)
        });
        // A design system of your own, from a website.
        let from_site = (tab == GalleryTab::Systems).then(|| {
            let st = state.clone();
            theme::ghost_button(&t, crate::i18n::tr("From a website…"))
                .id("gallery-from-site")
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.design_from_website(cx)))
        });
        let cards: Vec<_> = entries
            .iter()
            .zip(previews.into_iter().zip(pics))
            .map(|(e, (p, pic))| card(&t, card_w, e, pic.as_ref(), p, &state))
            .collect();
        let more = (total > self.shown).then(|| {
            theme::ghost_button(
                &t,
                format!(
                    "{} ({} {})",
                    crate::i18n::tr("Show more"),
                    total - self.shown,
                    crate::i18n::tr("left")
                ),
            )
            .id("gallery-more")
            .on_click(cx.listener(|this, _, _, cx| {
                this.shown += PAGE;
                cx.notify();
            }))
        });
        let empty = (total == 0).then(|| {
            div().text_color(t.colors.text_muted).child(format!(
                "{}: {} · \"{query}\"",
                crate::i18n::tr("Nothing matches"),
                tab.label()
            ))
        });

        div()
            .id("design-home")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .px(t.space.xl)
                    .py(t.space.xxl)
                    .flex()
                    .flex_col()
                    .gap(t.space.lg)
                    .child(header)
                    // The lists and the search on one row, what to start
                    // from under it: neither moves when the list changes.
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(t.space.md)
                            .child(tabs)
                            .child(search),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(t.space.sm)
                            .child(routines_button)
                            .child(from_figma)
                            .children(from_site),
                    )
                    .children(routines_list)
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
