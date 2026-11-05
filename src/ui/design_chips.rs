//! The composer's design chip: says whether the chat is a design chat and
//! what it is set up with (the kind of work, and the design system), and
//! opens the design menu above it. The menu's tabs set each part of the
//! setup (`design::prompt::Setup`): the kind of work, the look (a design
//! system), the template to start from, and the skill; a search filters the
//! library's lists. On a new chat's screen the setup is for the chat your
//! next message starts.

use gpui::{div, prelude::*, px, AnyElement, App, Entity, FontWeight, IntoElement, SharedString};

use super::icon;
use crate::design::library::{Entry, Index, Kind};
use crate::design::prompt::Surface;
use crate::state::{AppState, DesignTab};
use crate::theme::Theme;

/// The most rows a list shows; the search finds the rest.
const ROWS_MAX: usize = 80;

/// The footer chip. `skip` says a click is the one that just closed the
/// menu, so it doesn't reopen it.
pub fn trigger(
    t: &Theme,
    state: &Entity<AppState>,
    cx: &App,
    skip: impl Fn(&gpui::ClickEvent, &mut App) -> bool + 'static,
) -> Option<impl IntoElement> {
    crate::design::root()?;
    let s = state.read(cx);
    let open = s.design_menu.is_some();
    let setup = s.design_setup();
    let look = setup
        .and_then(|d| d.system.as_deref())
        .map(|id| title_of(s.design_index.as_deref(), Kind::DesignSystem, id));
    // An ordinary chat: the icon alone (its tooltip says what it does).
    let label: Option<SharedString> = match (setup, look) {
        (None, _) => None,
        (Some(d), None) => Some(d.surface.name().into()),
        (Some(d), Some(look)) => Some(format!("{} · {look}", d.surface.name()).into()),
    };
    let on = setup.is_some();
    let fg = if on {
        t.colors.primary
    } else {
        t.colors.text_muted
    };
    let tip: SharedString = if on {
        "A design chat: KodAct gets the design prompt and tools. Click to change what it makes and how it looks.".into()
    } else {
        "Make this a design chat: KodAct designs pages you watch in the side panel.".into()
    };
    let state = state.clone();
    Some(
        super::button::selector(t, "design-trigger", open)
            .aria_label("Design chat")
            .text_color(fg)
            .child(icon("icons/pencil-ruler.svg", t.size.icon_sm, fg))
            .children(label.map(|label| {
                div()
                    .max_w(px(140.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label)
            }))
            .child(super::button::chevron(t, open))
            // Not over its own open menu.
            .when(!open, |d| {
                d.tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
            })
            .on_click(move |ev, _, cx| {
                if skip(ev, cx) {
                    return;
                }
                state.update(cx, |s, cx| {
                    let to = match s.design_menu {
                        Some(_) => None,
                        None => Some(DesignTab::Work),
                    };
                    s.design_menu_to(to, cx)
                })
            }),
    )
}

/// The menu, above the chip. `search` is the filter's text box; `query`
/// its text. A press anywhere outside it calls `on_out`.
#[allow(clippy::too_many_arguments)]
pub fn menu(
    t: &Theme,
    state: &Entity<AppState>,
    tab: DesignTab,
    query: &str,
    search: AnyElement,
    window: &gpui::Window,
    cx: &App,
    on_out: impl Fn(gpui::Point<gpui::Pixels>, &mut App) + 'static,
    leaving: bool,
) -> impl IntoElement {
    let s = state.read(cx);
    let setup = s.design_setup().cloned();
    let surface = setup.as_ref().map(|d| d.surface);
    let hover = t.colors.hover;

    // The tabs: one choice on a track, as the app's other views.
    let mut seg = super::button::Segmented::new("design-tabs", "Design chat");
    for (tb, name) in [
        (DesignTab::Work, "Work"),
        (DesignTab::Look, "Look"),
        (DesignTab::Start, "Start from"),
        (DesignTab::Skill, "Skill"),
    ] {
        let st = state.clone();
        seg = seg.option(name, tb == tab, move |_, _, cx| {
            st.update(cx, |s, cx| s.design_menu_to(Some(tb), cx))
        });
    }
    let tabs = div().px(t.space.sm).pb(t.space.tight).flex().child(seg);

    let row = |id: SharedString, title: SharedString, sub: Option<SharedString>, picked: bool| {
        div()
            .id(id)
            .mx(t.space.xs)
            .px(t.space.sm)
            .py(t.space.xs)
            .flex()
            .items_center()
            .gap(t.space.snug)
            .rounded(t.radius.button)
            .cursor_pointer()
            .when(picked, |d| d.bg(hover))
            .hover(move |d| d.bg(hover))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_color(t.colors.text_strong)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(if picked {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(title),
                    )
                    .children(sub.map(|sub| {
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(sub)
                    })),
            )
            .child(div().w(t.size.icon_sm).flex_none().when(picked, |d| {
                d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.text))
            }))
    };

    // No gaps between rows: every point in the list picks something.
    let mut rows = div().flex().flex_col();
    let mut needs_search = false;
    match tab {
        DesignTab::Work => {
            for sf in Surface::ALL {
                let st = state.clone();
                rows = rows.child(
                    row(
                        format!("design-work-{}", sf.name()).into(),
                        sf.name().into(),
                        Some(sf.description().into()),
                        surface == Some(sf),
                    )
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.design_surface(sf, cx))),
                );
            }
        }
        tab => {
            needs_search = true;
            let (kind, current, none) = match tab {
                DesignTab::Look => (
                    Kind::DesignSystem,
                    setup.as_ref().and_then(|d| d.system.clone()),
                    "No design system: KodAct chooses the look",
                ),
                DesignTab::Start => (
                    Kind::Template,
                    setup.as_ref().and_then(|d| d.template.clone()),
                    "No template: start from a blank page",
                ),
                _ => (
                    Kind::Skill,
                    setup.as_ref().and_then(|d| d.skill.clone()),
                    "No skill: the design prompt alone",
                ),
            };
            let st = state.clone();
            rows = rows.child(
                row(
                    format!("design-none-{kind:?}").into(),
                    none.into(),
                    None,
                    current.is_none(),
                )
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.design_pick(tab, None, cx))),
            );
            match s.design_index.as_deref() {
                None => {
                    rows = rows.child(
                        div()
                            .px(t.space.md)
                            .py(t.space.tight)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child("Reading the design library…"),
                    )
                }
                Some(ix) => {
                    let list = entries(ix, kind, surface, query);
                    let total = list.len();
                    for e in list.into_iter().take(ROWS_MAX) {
                        let st = state.clone();
                        let id = e.id.clone();
                        let sub: Option<SharedString> = (!e.description.is_empty()).then(|| {
                            e.description
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                                .into()
                        });
                        rows = rows.child(
                            row(
                                format!("design-{kind:?}-{}", e.id).into(),
                                e.title.clone().into(),
                                sub,
                                current.as_deref() == Some(e.id.as_str()),
                            )
                            .on_click(move |_, _, cx| {
                                st.update(cx, |s, cx| s.design_pick(tab, Some(id.clone()), cx))
                            }),
                        );
                    }
                    let note = match total {
                        0 => Some("Nothing matches.".to_string()),
                        n if n > ROWS_MAX => {
                            Some(format!("{} more: search to find them.", n - ROWS_MAX))
                        }
                        _ => None,
                    };
                    rows = rows.children(note.map(|n| {
                        div()
                            .px(t.space.md)
                            .py(t.space.tight)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(n)
                    }));
                }
            }
        }
    }

    // Ending the design chat sits in the header, so picking something
    // never grows the menu: it stays put above its chip while you work in it.
    let end = setup.is_some().then(|| {
        let st = state.clone();
        div()
            .id("design-end")
            .flex_none()
            .flex()
            .items_center()
            .gap(t.space.xs)
            .px(t.space.sm)
            .h(t.size.control)
            .rounded(t.radius.compact)
            .cursor_pointer()
            .text_color(t.colors.text_muted)
            .hover(move |d| d.bg(hover))
            .child(icon("icons/x.svg", t.size.icon_xs, t.colors.text_muted))
            .child("End")
            .tooltip(|window, cx| {
                gpui::component::tooltip::Tooltip::new("End the design chat").build(window, cx)
            })
            .on_click(move |_, _, cx| st.update(cx, |s, cx| s.set_design(None, cx)))
    });

    let header = div()
        .pl(t.space.md)
        .pr(t.space.xs)
        .pb(t.space.xs)
        .flex()
        .items_center()
        .justify_between()
        .text_size(t.text.caption)
        .text_color(t.colors.text_subtle)
        .child(match (setup.is_some(), s.current.is_some()) {
            (true, _) => "Design chat",
            (false, true) => "Make this a design chat",
            (false, false) => "Make the next chat a design chat",
        })
        .children(end);

    // One height on every tab, the search included: the menu opens above
    // its chip, so a list that changed height would move the tabs under the
    // pointer between two clicks.
    let tall = t.size.design_menu_list.min(crate::theme::fit_height(
        window,
        t,
        t.size.viewport_margin + px(140.),
    ));
    let list = div()
        .id("design-rows")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .child(rows);
    let body = div()
        .h(tall)
        .flex()
        .flex_col()
        .when(needs_search, |d| d.child(search))
        .child(list);
    let menu = div()
        .id("design-menu")
        .on_mouse_down_out(move |ev, _, cx| on_out(ev.position, cx))
        .relative()
        .w(t.size.design_menu)
        .pt(t.space.tight)
        .pb(t.space.xs)
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow_lg()
        .occlude()
        .text_size(t.text.body_sm)
        .child(header)
        .child(tabs)
        .child(body);
    div().absolute().top_0().left_0().child(
        gpui::deferred(
            gpui::anchored()
                .anchor(gpui::Anchor::BottomLeft)
                .snap_to_window_with_margin(t.size.viewport_margin)
                .child(
                    div()
                        .pb(t.space.sm)
                        .child(super::motion::composer_menu(menu, "design", 0., leaving, t)),
                ),
        )
        .with_priority(2),
    )
}

/// An entry's title by id, or the id when the library isn't read yet.
fn title_of(ix: Option<&Index>, kind: Kind, id: &str) -> String {
    ix.and_then(|ix| ix.get(kind, id))
        .map_or_else(|| id.to_string(), |e| e.title.clone())
}

/// The list for a tab: what fits the kind of work (any, with a search),
/// matching every word of `query`, by title.
pub fn entries<'a>(
    ix: &'a Index,
    kind: Kind,
    surface: Option<Surface>,
    query: &str,
) -> Vec<&'a Entry> {
    let words: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let modes = surface.and_then(Surface::modes);
    let mut out: Vec<&Entry> = ix
        .of(kind)
        .filter(|e| !e.hidden && !(e.catalogue && !e.upstream_shipped))
        .filter(|e| {
            if !words.is_empty() {
                let hay = format!(
                    "{} {} {} {}",
                    e.id,
                    e.title,
                    e.description,
                    e.tags.join(" ")
                )
                .to_lowercase();
                return words.iter().all(|w| hay.contains(w.as_str()));
            }
            // Design systems fit any work; skills and templates fit theirs.
            kind == Kind::DesignSystem
                || modes.is_none_or(|m| e.mode.as_deref().is_some_and(|mode| m.contains(&mode)))
        })
        .collect();
    // Titles that match the search first, then by title.
    let in_title = |e: &Entry| {
        let title = e.title.to_lowercase();
        !words.is_empty() && words.iter().all(|w| title.contains(w.as_str()))
    };
    out.sort_by_key(|e| (!in_title(e), e.title.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_fit_the_kind_of_work() {
        let Some(root) = crate::design::root() else {
            return;
        };
        let ix = Index::load(&root);
        let decks = entries(&ix, Kind::Template, Some(Surface::Deck), "");
        assert!(!decks.is_empty());
        assert!(decks.iter().all(|e| e.mode.as_deref() == Some("deck")));
        // A search looks through every kind of work.
        let found = entries(&ix, Kind::Template, Some(Surface::Deck), "dashboard");
        assert!(
            found.iter().any(|e| e.mode.as_deref() != Some("deck")),
            "{:?}",
            found.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
        // Every design system fits; links to skills that aren't shipped don't show.
        assert_eq!(
            entries(&ix, Kind::DesignSystem, Some(Surface::Deck), "").len(),
            ix.of(Kind::DesignSystem).filter(|e| !e.hidden).count()
        );
        assert!(entries(&ix, Kind::Skill, None, "")
            .iter()
            .all(|e| !e.catalogue || e.upstream_shipped));
        let apple = entries(&ix, Kind::DesignSystem, None, "apple");
        assert_eq!(apple.first().map(|e| e.id.as_str()), Some("apple"));
        let lin = entries(&ix, Kind::DesignSystem, None, "lin");
        assert!(
            lin[0].title.to_lowercase().contains("lin"),
            "{}",
            lin[0].title
        );
    }
}
