//! The side panel's view switcher, "Lens": Plan, Agents, Files and Browser
//! in one flat track. The open view is a raised capsule of one fixed width
//! with its icon, its name and one readout ("2/6"); the others are icons,
//! named in their tooltip with their shortcut. A blue dot marks agents at
//! work wherever you are. Switching slides the capsule across.

use std::cell::Cell;
use std::time::Duration;

use gpui::{
    div, prelude::*, px, Animation, AnimationExt, App, ElementId, Entity, FontWeight, Hsla,
    SharedString, Window,
};

use super::icon;
use crate::state::{AppState, PanelTab};
use crate::theme::{ActiveTheme, Theme};

/// The open view's capsule and a closed view's square, and the gap.
const OPEN_W: f32 = 132.;
const CLOSED_W: f32 = 32.;
const GAP: f32 = 2.;
const H: f32 = 28.;

pub struct Item {
    pub which: PanelTab,
    pub label: &'static str,
    pub icon: &'static str,
    /// Its one figure ("2/4", "3"), shown while it is open.
    pub readout: Option<String>,
    /// Work going on in it now (agents running): a blue dot.
    pub live: bool,
    /// The digit of its shortcut (Ctrl+1 for the first).
    pub key: &'static str,
}

#[derive(IntoElement)]
pub struct Lens {
    items: Vec<Item>,
    open: usize,
    state: Entity<AppState>,
}

impl Lens {
    pub fn new(items: Vec<Item>, open: PanelTab, state: Entity<AppState>) -> Self {
        let open = items.iter().position(|i| i.which == open).unwrap_or(0);
        Self { items, open, state }
    }
}

thread_local! {
    /// The capsule's last two places (from, to): the slide runs between them.
    static SLIDE: Cell<(usize, usize)> = const { Cell::new((usize::MAX, usize::MAX)) };
}

fn dot(color: Hsla, t: &Theme) -> gpui::Div {
    div().size(px(6.)).rounded(t.radius.pill).bg(color)
}

impl RenderOnce for Lens {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let open = self.open;
        let (from, to) = SLIDE.with(|s| {
            let (a, b) = s.get();
            let next = if b == open { (a, b) } else { (if b == usize::MAX { open } else { b }, open) };
            s.set(next);
            next
        });
        let x = |k: usize| GAP + k as f32 * (CLOSED_W + GAP);
        let (x0, x1) = (x(from), x(to));
        let ms = if super::motion::reduced_motion() { 1 } else { t.size.pop_in_ms.max(180) };
        // The capsule itself, under the tabs: it slides from the last open
        // view to this one.
        let capsule = div()
            .absolute()
            .top(px(GAP))
            .w(px(OPEN_W))
            .h(px(H))
            .rounded(t.radius.button)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .shadow(t.elevation_grain())
            .with_animation(
                SharedString::from(format!("lens-{from}-{to}")),
                Animation::new(Duration::from_millis(ms)).with_easing(gpui::ease_out_quint()),
                move |d, k| d.left(px(x0 + (x1 - x0) * k)),
            );
        let hover = t.colors.surface;
        let edge = t.colors.primary;
        let tabs = self.items.into_iter().enumerate().map(|(i, item)| {
            let on = i == open;
            let id = ElementId::Name(format!("lens-{:?}", item.which).into());
            let focus = window
                .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone();
            let (st, key_st, which) = (self.state.clone(), self.state.clone(), item.which);
            let tip: SharedString = format!("{}   {}", item.label, super::shortcut(item.key)).into();
            div()
                .id(id)
                .relative()
                .flex_none()
                .h(px(H))
                .w(px(if on { OPEN_W } else { CLOSED_W }))
                .flex()
                .items_center()
                .rounded(t.radius.button)
                .border_1()
                .border_color(gpui::transparent_black())
                .track_focus(&focus)
                .tab_index(0)
                .focus_visible(move |s| s.border_color(edge))
                .cursor_pointer()
                .map(|d| {
                    if on {
                        // The name and readout fade in behind the sliding capsule.
                        d.px(t.space.sm + px(2.)).child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .text_size(t.text.body_sm)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                                .child(icon(item.icon, t.size.icon_sm, t.colors.text_strong))
                                .child(item.label)
                                .child(
                                    div()
                                        .ml_auto()
                                        .flex()
                                        .items_center()
                                        .gap(t.space.xs + px(2.))
                                        .text_size(t.text.caption)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(t.colors.text_muted)
                                        .children(item.readout.clone())
                                        .when(item.live, |d| d.child(dot(t.extra.live, &t))),
                                )
                                .with_animation(
                                    SharedString::from(format!("lens-label-{from}-{to}")),
                                    Animation::new(Duration::from_millis(ms))
                                        .with_easing(gpui::ease_out_quint()),
                                    |d, k| d.opacity(0.35 + 0.65 * k),
                                ),
                        )
                    } else {
                        d.justify_center()
                            .hover(move |s| s.bg(hover))
                            .child(icon(item.icon, t.size.icon, t.colors.text_subtle))
                            .when(item.live, |d| {
                                d.child(div().absolute().top(px(5.)).right(px(6.)).child(dot(t.extra.live, &t)))
                            })
                            .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(tip.clone()).build(w, cx))
                    }
                })
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.show_panel(which, cx)))
                .on_key_down(move |ev, _, cx| {
                    if matches!(ev.keystroke.key.as_str(), "enter" | "space") {
                        key_st.update(cx, |s, cx| s.show_panel(which, cx));
                        cx.stop_propagation();
                    }
                })
        });
        div()
            .relative()
            .flex()
            .items_center()
            .gap(px(GAP))
            .p(px(GAP))
            .rounded(t.radius.button + px(2.))
            .bg(t.extra.sunk)
            .border_1()
            .border_color(t.colors.border)
            .child(capsule)
            .children(tabs)
    }
}
