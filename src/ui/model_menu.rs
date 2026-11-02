//! The model window, as Forge for VS Code has it: a menu above the
//! composer, as wide as it. Above eight rows it has a search box. Then
//! "Select a model" with a refresh button that turns while models are being
//! checked, and one row per endpoint: the model's name with its chips (Max,
//! Ultracode, Fast) and a coloured ping, the profile and host underneath
//! (and why, when it isn't answering), a check on the one in use. When the
//! model takes effort, an Effort row with a slider ends the menu.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, relative, Animation, AnimationExt, AnyElement, Entity, FontWeight, Hsla,
    IntoElement, SharedString, Transformation,
};

use super::icon;
use crate::endpoints::{self, Effort};
use crate::health::{self, Check, Tone};
use crate::state::AppState;
use crate::theme::Theme;

/// More rows than this and the menu offers a search box.
pub const FILTER_THRESHOLD: usize = 8;
/// The effort row's key among the rows, for the arrow keys.
pub const EFFORT_ROW: &str = "\u{0}effort";

pub struct Row {
    pub name: String,
    pub display: String,
    pub description: String,
    pub chips: Vec<&'static str>,
    /// Text, tone and tooltip of the ping chip, when it answered.
    pub ping: Option<(String, Tone, String)>,
    /// Failed or unreachable: shown greyed (only when it's the one in use).
    pub disabled: bool,
    pub current: bool,
    /// A model from a gateway's list: its profile and the model's id.
    /// Picking it switches that profile to the model.
    pub pick: Option<(String, String)>,
    /// The first of a gateway's models: the heading above them.
    pub group: Option<Group>,
    /// Failed its test (listed only while failed models are shown): why.
    pub failed: Option<String>,
}

/// The heading above a gateway's models.
pub struct Group {
    pub title: String,
    /// How many of its models failed a test (behind the Show switch).
    pub failed: usize,
}

/// The most of a gateway's models listed before a search narrows them.
const CATALOG_SHOWN: usize = 300;

/// A context window as people say it: "200K", "1M".
fn window_text(tokens: u64) -> String {
    match tokens {
        t if t >= 1_000_000 && t % 1_000_000 == 0 => format!("{}M", t / 1_000_000),
        t if t >= 1_000_000 => format!("{:.1}M", t as f64 / 1_000_000.),
        t => format!("{}K", (t as f64 / 1000.).round() as u64),
    }
}

/// The menu's rows: every pair that answers or hasn't been measured, plus
/// the one in use when it doesn't answer. Then filtered by `query`.
pub fn rows(s: &AppState, query: &str) -> (Vec<Row>, usize) {
    let Some(e) = &s.endpoints else {
        return (vec![], 0);
    };
    let current = e.active().map(|p| p.name.clone());
    let now = health::now_ms();
    let mut offered = vec![];
    let mut unavailable = vec![];
    for p in &e.profiles {
        let (check, _) = health::pair_check(&p.model, s.health.get(&p.name));
        let status = health::status_text(&check);
        let description = [p.name.clone(), endpoints::host_of(&p.base_url), status]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let ping = match check {
            Check::Answered { ms, checked_at } => Some((
                health::ping_text(ms),
                health::ping_tone(ms),
                format!(
                    "Answered a check in {} {}",
                    health::ping_text(ms),
                    health::ago_text(checked_at, now)
                ),
            )),
            _ => None,
        };
        let row = Row {
            name: p.name.clone(),
            display: p.display().to_string(),
            description,
            chips: p.chips(),
            ping,
            disabled: !health::is_offered(&check),
            current: current.as_deref() == Some(&p.name),
            pick: None,
            group: None,
            failed: None,
        };
        if row.disabled {
            unavailable.push(row);
        } else {
            offered.push(row);
        }
    }
    offered.extend(unavailable.into_iter().filter(|r| r.current));
    let q = query.trim().to_lowercase();
    if !q.is_empty() {
        offered.retain(|r| {
            [&r.name, &r.display, &r.description]
                .iter()
                .any(|f| f.to_lowercase().contains(&q))
        });
    }
    // Then each gateway's own models, as Claude Code's `/model` lists a
    // gateway's: best match first while searching, else by id.
    let mut total = offered.len();
    for p in e.profiles.iter().filter(|p| p.capabilities.catalog) {
        let Some(models) = s.catalogs.get(&p.name) else {
            continue;
        };
        total += models.len();
        let tests = s.model_tests.get(&health::tests_key(&p.base_url));
        let verdict = |id: &str| tests.and_then(|t| t.get(id));
        let failed = models
            .iter()
            .filter(|m| verdict(&m.id).is_some_and(|v| !v.servable))
            .count();
        let title = match failed {
            0 => format!("{} · {} models", p.name, models.len()),
            n => format!(
                "{} · {} working / {} models",
                p.name,
                models.len() - n,
                models.len()
            ),
        };
        let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
        let found = endpoints::find_models(&ids, query);
        let matched = !found.is_empty();
        let mut group = Some(Group { title, failed });
        for id in found.into_iter().take(CATALOG_SHOWN) {
            let Some(m) = models.iter().find(|m| &m.id == id) else {
                continue;
            };
            let in_use = current.as_deref() == Some(&p.name) && p.model == m.id;
            let tested = verdict(&m.id);
            let why = tested
                .filter(|v| !v.servable)
                .map(|v| v.detail.clone().unwrap_or_else(|| "no reply".into()));
            // A model that failed its test stays out, unless asked for or in use.
            if why.is_some() && !s.show_failed_models && !in_use {
                continue;
            }
            let description = [
                Some(m.id.clone()),
                m.context.map(|c| format!("{} context", window_text(c))),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            offered.push(Row {
                name: format!("{}\u{1}{}", p.name, m.id),
                display: m.name.clone().unwrap_or_else(|| m.id.clone()),
                description,
                chips: if m.reasoning { vec!["Thinks"] } else { vec![] },
                ping: tested.filter(|v| v.servable).map(|v| {
                    (
                        health::ping_text(v.ms),
                        health::ping_tone(v.ms),
                        format!(
                            "Answered a test in {} {}",
                            health::ping_text(v.ms),
                            health::ago_text(v.checked_at, now)
                        ),
                    )
                }),
                disabled: false,
                current: in_use,
                pick: Some((p.name.clone(), m.id.clone())),
                group: group.take(),
                failed: why,
            });
        }
        // Every match failed its test: the heading still offers to show them.
        if let Some(g) = group.filter(|g| g.failed > 0 && matched) {
            offered.push(Row {
                name: format!("{}\u{1}", p.name),
                display: "Every match failed its test".into(),
                description: String::new(),
                chips: vec![],
                ping: None,
                disabled: true,
                current: false,
                pick: None,
                group: Some(g),
                failed: None,
            });
        }
    }
    (offered, total)
}

/// The keys the arrows move through: the rows, then the effort row.
pub fn keys(s: &AppState, query: &str) -> Vec<String> {
    let mut keys: Vec<String> = rows(s, query).0.into_iter().map(|r| r.name).collect();
    if s.active_profile()
        .is_some_and(|p| !p.effort_levels().is_empty())
    {
        keys.push(EFFORT_ROW.into());
    }
    keys
}

/// The next effort level, wrapping through Auto (unset).
pub fn next_effort(levels: &[Effort], current: Option<Effort>) -> Option<Effort> {
    match current.and_then(|c| levels.iter().position(|l| *l == c)) {
        None => levels.first().copied(),
        Some(i) if i + 1 < levels.len() => Some(levels[i + 1]),
        Some(_) => None,
    }
}

fn effort_tone(t: &Theme, e: Option<Effort>) -> Hsla {
    match e {
        Some(Effort::Xhigh) => t.extra.warning,
        Some(Effort::Max) => t.extra.danger,
        _ => t.colors.text_subtle,
    }
}

/// The pill's effort suffix: the level, when the model takes effort and one is set.
pub fn pill_effort(t: &Theme, s: &AppState) -> Option<(SharedString, Hsla)> {
    let levels = s.active_profile()?.effort_levels();
    let e = s.effort.filter(|e| levels.contains(e))?;
    Some((e.label().into(), effort_tone(t, Some(e))))
}

fn chip(
    t: &Theme,
    text: impl Into<SharedString>,
    bg: Hsla,
    fg: Hsla,
    border: Option<Hsla>,
) -> gpui::Div {
    div()
        .px(px(5.))
        .rounded(t.radius.button)
        .bg(bg)
        .when_some(border, |d, b| d.border_1().border_color(b))
        .text_color(fg)
        .text_size(t.text.caption - px(1.5))
        .font_weight(FontWeight::MEDIUM)
        .line_height(relative(1.2))
        .child(text.into())
}

/// The menu. `active` is the row under the keyboard or pointer; `search`
/// the search box, shown above eight rows; `on_out` runs on a press
/// anywhere outside the menu.
#[allow(clippy::too_many_arguments)]
pub fn render(
    t: &Theme,
    state: &Entity<AppState>,
    s: &AppState,
    query: &str,
    active: Option<&str>,
    search: AnyElement,
    window: &gpui::Window,
    on_hover: impl Fn(String, &mut gpui::App) + Clone + 'static,
    on_out: impl Fn(gpui::Point<gpui::Pixels>, &mut gpui::App) + 'static,
    leaving: bool,
) -> impl IntoElement {
    let (rows, total) = rows(s, query);
    let checking = s.checking();
    let loading = s.endpoints.is_none();
    let none = s.endpoints.as_ref().is_some_and(|e| e.profiles.is_empty());
    let brand = t.colors.primary;
    let active_bg = t.colors.hover;

    let refresh = {
        let st = state.clone();
        let spin = icon(
            "icons/refresh-cw.svg",
            t.size.icon_sm,
            if checking {
                brand
            } else {
                t.colors.text_subtle
            },
        );
        div()
            .id("models-refresh")
            .flex_none()
            .size(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(t.radius.button)
            .cursor_pointer()
            .hover(move |d| d.bg(active_bg))
            .tooltip(move |window, cx| {
                gpui::component::tooltip::Tooltip::new(if checking {
                    "Checking which models answer…"
                } else {
                    "Read endpoints.json again and check every model"
                })
                .build(window, cx)
            })
            .child(if checking {
                spin.with_animation(
                    "models-refresh-spin",
                    Animation::new(Duration::from_millis(900)).repeat(),
                    |svg, d| svg.with_transformation(Transformation::rotate(gpui::percentage(d))),
                )
                .into_any_element()
            } else {
                spin.into_any_element()
            })
            // Refresh reads endpoints.json again, so an edit shows at once.
            .on_click(move |_, _, cx| st.update(cx, |s, cx| s.load_endpoints(true, cx)))
    };

    let heading = div()
        .px(px(12.))
        .py(px(4.))
        .flex()
        .items_center()
        .justify_between()
        .text_size(t.text.caption)
        .text_color(t.colors.text_subtle)
        .child(if none {
            "No models yet: set up an endpoint below".to_string()
        } else if rows.is_empty() && checking {
            "Checking which models answer…".into()
        } else if rows.is_empty() {
            "No models available".into()
        } else {
            "Select a model".into()
        })
        .child(refresh);

    let mut list = div().flex().flex_col().gap(px(2.)).pb(px(8.));
    if loading {
        list = list.child(
            div()
                .pt(px(8.))
                .px(px(8.))
                .text_center()
                .text_size(t.text.caption)
                .text_color(t.colors.text_subtle)
                .child("Loading models…"),
        );
    } else {
        list = list.child(heading);
        for row in rows {
            if let Some(g) = &row.group {
                let toggle = (g.failed > 0).then(|| {
                    let st = state.clone();
                    div()
                        .id(SharedString::from(format!("failed-{}", row.name)))
                        .flex_none()
                        .px(px(4.))
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .text_color(t.colors.text_muted)
                        .hover(move |d| d.bg(active_bg))
                        .child(if s.show_failed_models {
                            format!("Hide {} that failed", g.failed)
                        } else {
                            format!("Show {} that failed", g.failed)
                        })
                        .on_click(move |_, _, cx| st.update(cx, |s, cx| s.toggle_failed_models(cx)))
                });
                list = list.child(
                    div()
                        .px(px(12.))
                        .pt(px(8.))
                        .pb(px(2.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.))
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(g.title.clone()),
                        )
                        .children(toggle),
                );
            }
            let pick = row.pick.clone();
            let key = row.name.clone();
            let is_active = active == Some(key.as_str());
            let (st, hover) = (state.clone(), on_hover.clone());
            let hover_key = key.clone();
            let ping = row.ping.map(|(text, tone, title)| {
                let (bg, fg) = match tone {
                    Tone::Fast => (t.extra.ping_fast_bg, t.extra.ping_fast_fg),
                    Tone::Fair => (t.extra.ping_fair_bg, t.extra.ping_fair_fg),
                    Tone::Slow => (t.extra.ping_slow_bg, t.extra.ping_slow_fg),
                };
                chip(t, text, bg, fg, None)
                    .id(SharedString::from(format!("ping-{key}")))
                    .tooltip(move |window, cx| {
                        gpui::component::tooltip::Tooltip::new(title.clone()).build(window, cx)
                    })
            });
            list = list.child(
                div()
                    .id(SharedString::from(format!("model-{key}")))
                    .mx(px(4.))
                    .px(px(8.))
                    .py(px(4.))
                    .flex()
                    .items_center()
                    .rounded(t.radius.button)
                    .when(row.disabled || row.failed.is_some(), |d| d.opacity(0.5))
                    .when(!row.disabled, |d| d.cursor_pointer())
                    .when(is_active, |d| d.bg(active_bg))
                    .when(row.current, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(4.))
                                .bottom(px(4.))
                                .w(px(2.))
                                .rounded(t.radius.line)
                                .bg(brand),
                        )
                    })
                    .relative()
                    .on_mouse_move(move |_, _, cx| hover(hover_key.clone(), cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .line_height(relative(1.3))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .text_color(t.colors.text_strong)
                                            .when(row.current, |d| {
                                                d.font_weight(FontWeight::SEMIBOLD)
                                            })
                                            .child(row.display),
                                    )
                                    .child(div().ml(px(2.)).flex().gap(px(4.)).children(
                                        row.chips.into_iter().map(|c| {
                                            let fg = if c == "Fast" {
                                                t.extra.mode_plan
                                            } else {
                                                brand
                                            };
                                            chip(t, c, fg.opacity(0.1), fg, Some(fg.opacity(0.4)))
                                        }),
                                    ))
                                    .children(ping),
                            )
                            .child(
                                div()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_subtle)
                                    .when(row.disabled, |d| d.text_color(t.colors.text))
                                    .when(row.failed.is_some(), |d| d.text_color(t.extra.danger))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(match &row.failed {
                                        Some(why) => format!("{} · {why}", row.description),
                                        None => row.description,
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .w(px(24.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .when(row.current, |d| {
                                d.child(icon("icons/check.svg", t.size.icon, t.colors.text))
                            }),
                    )
                    .when(!row.disabled, |d| {
                        let name = key.clone();
                        d.on_click(move |_, _, cx| {
                            st.update(cx, |s, cx| match &pick {
                                Some((profile, id)) => s.pick_catalog_model(profile, id, cx),
                                None => s.select_profile(&name, cx),
                            })
                        })
                    }),
            );
        }
    }

    // Effort: a divider, then the level and a slider, when the model takes it.
    let effort = s
        .active_profile()
        .map(|p| p.effort_levels())
        .filter(|l| !l.is_empty())
        .map(|levels| {
            let current = s.effort.filter(|e| levels.contains(e));
            let is_active = active == Some(EFFORT_ROW);
            let (st, hover) = (state.clone(), on_hover.clone());
            let cycle_levels = levels.clone();
            let slider = slider(t, state, &levels, current);
            div()
                .flex()
                .flex_col()
                .pb(px(4.))
                .child(crate::theme::rule(t).mb(px(4.)))
                .child(
                    div()
                        .id("effort-row")
                        .mx(px(4.))
                        .px(px(8.))
                        .py(px(4.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .when(is_active, |d| d.bg(active_bg))
                        .on_mouse_move(move |_, _, cx| hover(EFFORT_ROW.to_string(), cx))
                        .child(icon("icons/brain.svg", t.size.icon_sm, t.colors.text_muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .gap(px(4.))
                                .text_color(t.colors.text)
                                .child("Effort")
                                .child(div().text_color(effort_tone(t, current)).child(format!(
                                    "({})",
                                    current.map(|e| e.label()).unwrap_or("Auto")
                                ))),
                        )
                        .child(slider)
                        .on_click(move |_, _, cx| {
                            st.update(cx, |s, cx| {
                                s.effort = next_effort(&cycle_levels, s.effort);
                                cx.notify();
                            })
                        }),
                )
        });

    let menu = div()
        .id("models-menu")
        .on_mouse_down_out(move |ev, _, cx| on_out(ev.position, cx))
        .absolute()
        .bottom_full()
        .left_0()
        .right_0()
        .mb(px(8.))
        .flex()
        .flex_col()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .shadow_lg()
        .occlude()
        .overflow_hidden()
        .text_size(t.text.body_sm)
        .child(if total >= FILTER_THRESHOLD {
            search
        } else {
            div().h(px(4.)).into_any_element()
        })
        .child(
            div()
                .id("models-list")
                // Above the composer, under the search, over the effort and links.
                .max_h(px(300.).min(crate::theme::fit_height(
                    window,
                    t,
                    t.size.titlebar + t.size.composer_clearance * 2.,
                )))
                .overflow_y_scroll()
                .child(list),
        )
        .children(effort)
        .child(setup_links(t));
    super::motion::composer_menu(menu, "models", 8., leaving, t)
}

/// Under the list: the setup guide and the file, one click each.
fn setup_links(t: &Theme) -> impl IntoElement {
    let hover = t.colors.hover;
    let link = |id: &'static str, path: &'static str, label: &'static str| {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(6.))
            .h(t.size.control)
            .px(px(8.))
            .rounded(t.radius.button)
            .cursor_pointer()
            .text_size(t.text.caption)
            .text_color(t.colors.text_muted)
            .hover(move |st| st.bg(hover))
            .child(icon(path, t.size.icon_sm, t.colors.text_muted))
            .child(label)
    };
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .px(px(4.))
        .py(px(4.))
        .border_t_1()
        .border_color(t.colors.border)
        .child(
            link("models-guide", "icons/route.svg", "Set up an endpoint").on_click(
                |_, window, cx| window.dispatch_action(Box::new(super::EndpointGuide), cx),
            ),
        )
        .child(
            link("models-file", "icons/file-pen.svg", "Open endpoints.json").on_click(
                |_, window, cx| window.dispatch_action(Box::new(super::OpenEndpointsFile), cx),
            ),
        )
}

/// A 76×18 track with a notch per level and a thumb on the current one
/// (none for Auto). Clicking a notch picks that level.
fn slider(
    t: &Theme,
    state: &Entity<AppState>,
    levels: &[Effort],
    current: Option<Effort>,
) -> impl IntoElement {
    let n = levels.len().max(2);
    let (w, inset, thumb) = (76., 2., 14.);
    let step = (w - 2. * inset - thumb) / (n - 1) as f32;
    let mut track = div()
        .relative()
        .w(px(w))
        .h(px(18.))
        .flex_none()
        .rounded(t.radius.pill)
        .bg(t.colors.hover)
        .border_1()
        .border_color(t.colors.border);
    for (i, level) in levels.iter().enumerate() {
        let st = state.clone();
        let level = *level;
        let x = inset + step * i as f32 + thumb / 2. - 2.;
        track = track.child(
            div()
                .id(SharedString::from(format!("effort-{}", level.label())))
                .absolute()
                .left(px(x - 3.))
                .top(px(1.))
                .size(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(
                    div()
                        .size(px(4.))
                        .rounded_full()
                        .bg(t.colors.text_subtle.opacity(0.6)),
                )
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    st.update(cx, |s, cx| {
                        s.effort = Some(level);
                        cx.notify();
                    })
                }),
        );
    }
    if let Some(i) = current.and_then(|c| levels.iter().position(|l| *l == c)) {
        track = track.child(
            div()
                .absolute()
                .left(px(inset + step * i as f32 - 1.))
                .top(px(1.))
                .size(px(thumb))
                .rounded_full()
                .bg(effort_tone(t, current).opacity(0.9))
                .border_1()
                .border_color(t.colors.surface),
        );
    }
    track
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_cycles_through_auto() {
        let levels = [Effort::Low, Effort::Medium, Effort::High];
        assert_eq!(next_effort(&levels, None), Some(Effort::Low));
        assert_eq!(
            next_effort(&levels, Some(Effort::Medium)),
            Some(Effort::High)
        );
        assert_eq!(next_effort(&levels, Some(Effort::High)), None);
        assert_eq!(next_effort(&levels, Some(Effort::Max)), Some(Effort::Low));
    }
}
