//! KodAct's controls: one family of buttons (primary, secondary, ghost,
//! outlined, danger; icon-only or labelled; send and stop), the switch, the
//! segmented control and the selector trigger.
//!
//! Every control draws its colours, radii and sizes from the theme
//! (`theme.rs`), takes keyboard focus (Tab reaches it, Enter or Space presses
//! it, and a focus edge shows only for the keyboard), names itself to
//! assistive technology, and swallows clicks while disabled or loading.
//! Shapes are softly squared: 8px for buttons, 9px for outlined squares;
//! pills only for the switch.

use gpui::{
    accesskit, div, prelude::*, px, AnyElement, App, ClickEvent, ElementId, FocusHandle, Hsla,
    MouseButton, SharedString, Window,
};

use super::icon;
use crate::theme::{ActiveTheme, Theme};

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type OnToggle = Box<dyn Fn(bool, &mut Window, &mut App) + 'static>;

/// How loud a button is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    /// The one filled action on a surface: ink on paper (paper on charcoal).
    Primary,
    /// A bordered surface button: Cancel, Allow once, Save.
    Secondary,
    /// Quiet until hovered: toolbars, headers, menus.
    Ghost,
    /// A bordered square or chip on the surface: the composer's +.
    Outlined,
    /// A destructive confirm.
    Danger,
    /// The accent fill, kept for sending.
    Accent,
}

/// A button's height and padding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Compact,
    Regular,
}

/// The colours a variant paints at rest and hovered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint {
    pub bg: Hsla,
    pub hover_bg: Hsla,
    pub fg: Hsla,
    pub border: Option<Hsla>,
}

impl Variant {
    pub fn paint(self, t: &Theme) -> Paint {
        let clear = t.colors.surface.opacity(0.);
        match self {
            Variant::Primary => Paint {
                bg: t.extra.fill,
                hover_bg: t.extra.fill_hover,
                fg: t.extra.fill_fg,
                border: None,
            },
            Variant::Secondary => Paint {
                bg: t.colors.surface,
                hover_bg: over(t.colors.hover, t.colors.surface),
                fg: t.colors.text,
                border: Some(t.extra.border_strong),
            },
            Variant::Ghost => Paint {
                bg: clear,
                hover_bg: t.colors.hover,
                fg: t.colors.text,
                border: None,
            },
            Variant::Outlined => Paint {
                bg: t.colors.surface,
                hover_bg: over(t.colors.hover, t.colors.surface),
                fg: t.colors.text,
                border: Some(t.extra.border_strong),
            },
            Variant::Danger => Paint {
                bg: t.extra.fill_danger,
                hover_bg: t.extra.fill_danger.opacity(0.9),
                fg: t.colors.primary_fg,
                border: None,
            },
            Variant::Accent => Paint {
                bg: t.extra.accent,
                hover_bg: t.extra.accent.opacity(0.9),
                fg: t.extra.accent_fg,
                border: None,
            },
        }
    }
}

/// A translucent wash over an opaque colour, as one colour: a hover over a
/// surface stays opaque, so nothing behind the button shows through.
fn over(wash: Hsla, base: Hsla) -> Hsla {
    let (w, b) = (wash.to_rgb(), base.to_rgb());
    let a = w.a;
    gpui::Rgba {
        r: w.r * a + b.r * (1. - a),
        g: w.g * a + b.g * (1. - a),
        b: w.b * a + b.b * (1. - a),
        a: b.a,
    }
    .into()
}

/// The focus handle a control keeps across frames, by its id.
fn focus_handle(id: &ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    window
        .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone()
}

/// The keyboard focus edge every control shares: the accent's text shade.
fn focus_edge(t: &Theme) -> Hsla {
    t.colors.primary
}

/// A labelled button (an icon before or after the label is optional), or an
/// icon-only square when it has no label.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    variant: Variant,
    size: Size,
    label: Option<SharedString>,
    icon: Option<&'static str>,
    /// What the button is called to assistive technology and in its tooltip,
    /// when the label alone doesn't say (always, for an icon-only button).
    describe: Option<SharedString>,
    tooltip: bool,
    disabled: bool,
    loading: bool,
    selected: bool,
    /// A key that does the same, after the label in its own colour, quieter.
    hint: Option<SharedString>,
    on_click: Option<OnClick>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, variant: Variant) -> Self {
        Self {
            id: id.into(),
            variant,
            size: Size::Regular,
            label: None,
            icon: None,
            describe: None,
            tooltip: false,
            disabled: false,
            loading: false,
            selected: false,
            hint: None,
            on_click: None,
        }
    }

    /// Show the key that presses this button (`Enter`, `Esc`) after its label.
    pub fn hint(mut self, key: impl Into<SharedString>) -> Self {
        self.hint = Some(key.into());
        self
    }

    pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Primary).label(label)
    }

    pub fn secondary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Secondary).label(label)
    }

    pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Ghost).label(label)
    }

    pub fn danger(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Danger).label(label)
    }

    /// The one decisive action, in the accent: Allow once, Send.
    pub fn accent(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self::new(id, Variant::Accent).label(label)
    }

    /// An icon-only button: transparent at rest, washed on hover, its name
    /// in a tooltip and to assistive technology.
    pub fn icon(
        id: impl Into<ElementId>,
        path: &'static str,
        describe: impl Into<SharedString>,
    ) -> Self {
        Self::new(id, Variant::Ghost)
            .with_icon(path)
            .describe(describe)
            .tooltip(true)
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_icon(mut self, path: &'static str) -> Self {
        self.icon = Some(path);
        self
    }

    pub fn describe(mut self, text: impl Into<SharedString>) -> Self {
        self.describe = Some(text.into());
        self
    }

    pub fn tooltip(mut self, on: bool) -> Self {
        self.tooltip = on;
        self
    }

    pub fn compact(mut self) -> Self {
        self.size = Size::Compact;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Busy: a turning loader replaces the icon and clicks are swallowed, so
    /// an action can't be sent twice.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Held open or chosen: a menu's trigger while its menu shows.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let focus = focus_handle(&self.id, window, cx);
        let paint = self.variant.paint(&t);
        let icon_only = self.label.is_none();
        let inert = self.disabled || self.loading;
        let (h, pad, icon_size, text) = match (self.size, icon_only) {
            (Size::Compact, true) => (t.size.hit_sm, px(0.), t.size.icon_sm, t.text.caption),
            (Size::Regular, true) => (t.size.hit, px(0.), t.size.icon, t.text.body_sm),
            (Size::Compact, false) => (
                t.size.button_sm,
                t.space.snug,
                t.size.icon_sm,
                t.text.caption,
            ),
            (Size::Regular, false) => (t.size.button_h, px(14.), t.size.icon, t.text.body_sm),
        };
        // Everything you press is a pill.
        let radius = match (icon_only, self.size) {
            (true, _) => t.radius.control,
            (_, Size::Compact) => t.radius.compact,
            _ => t.radius.pill,
        };
        let fg = if self.disabled {
            t.colors.text_subtle
        } else if self.variant == Variant::Ghost && icon_only && !self.selected {
            // An icon at rest is muted; it darkens with its wash on hover.
            t.colors.text_muted
        } else {
            paint.fg
        };
        let bg = if self.selected {
            paint.hover_bg
        } else if self.disabled
            && matches!(
                self.variant,
                Variant::Primary | Variant::Danger | Variant::Accent
            )
        {
            t.colors.hover
        } else {
            paint.bg
        };
        let hover_bg = paint.hover_bg;
        let lead: Option<AnyElement> = if self.loading {
            Some(spinner(icon_size, fg, self.id.clone()))
        } else {
            self.icon.map(|p| icon(p, icon_size, fg).into_any_element())
        };
        let describe = self.describe.clone().or_else(|| self.label.clone());
        let tooltip = self.tooltip.then(|| describe.clone()).flatten();
        div()
            .id(self.id.clone())
            .role(accesskit::Role::Button)
            .when_some(describe, |d, l| d.aria_label(l))
            .track_focus(&focus)
            .tab_index(0)
            .tab_stop(!self.disabled)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(t.space.tight)
            .h(h)
            .when(icon_only, |d| d.w(h))
            .when(!icon_only, |d| d.px(pad))
            .rounded(radius)
            .border_1()
            .border_color(paint.border.unwrap_or(t.colors.surface.opacity(0.)))
            .bg(bg)
            .text_size(text)
            .text_color(fg)
            // Labels on buttons are set in the heavier cut.
            .when(
                matches!(
                    self.variant,
                    Variant::Primary | Variant::Danger | Variant::Accent | Variant::Secondary
                ) || (self.variant == Variant::Ghost && self.size != Size::Compact),
                |d| d.font_weight(gpui::FontWeight::SEMIBOLD),
            )
            .focus_visible({
                let edge = focus_edge(&t);
                move |s| s.border_color(edge)
            })
            .when(!inert, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .active(|s| s.opacity(0.85))
            })
            .when(self.disabled, |d| d.cursor_default().opacity(0.7))
            .children(lead)
            .when_some(self.label, |d, l| {
                d.child(div().min_w_0().whitespace_nowrap().text_ellipsis().child(l))
            })
            .when_some(self.hint, |d, k| {
                d.child(
                    div()
                        .flex_none()
                        .ml(t.space.xs)
                        .font_family(t.font_mono.clone())
                        .font_weight(gpui::FontWeight::NORMAL)
                        .text_size(t.text.caption - px(1.))
                        .opacity(0.72)
                        .child(k),
                )
            })
            .when_some(tooltip, |d, tip| {
                d.tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
            })
            // A press never moves the keyboard focus here (the composer keeps it).
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .when_some(self.on_click.filter(|_| !inert), |d, f| d.on_click(f))
            .when(inert, |d| d.on_click(|_, _, cx| cx.stop_propagation()))
    }
}

/// The loader the busy button shows: Lucide's circle, turning.
fn spinner(size: gpui::Pixels, color: Hsla, id: ElementId) -> AnyElement {
    use gpui::{Animation, AnimationExt as _, Transformation};
    let svg = icon("icons/loader-circle.svg", size, color);
    if super::motion::reduced_motion() {
        return svg.into_any_element();
    }
    svg.with_animation(
        ElementId::NamedChild(std::sync::Arc::new(id), "spin".into()),
        Animation::new(std::time::Duration::from_millis(900)).repeat(),
        |s, k| s.with_transformation(Transformation::rotate(gpui::percentage(k))),
    )
    .into_any_element()
}

/// What the send button can do now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SendState {
    /// Nothing written: pale, and it does nothing.
    Empty,
    /// Something to send: the accent, solid.
    Ready,
    /// A reply streaming: Stop, an ink ring round a square.
    Busy,
}

/// The composer's send / stop button, `hit_lg` square.
pub fn send_button(
    id: impl Into<ElementId>,
    state: SendState,
    t: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let size = t.size.hit_lg;
    let base = div()
        .id(id)
        .role(accesskit::Role::Button)
        .size(size)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        // The mark's tile: a square with its corners rounded a sixth.
        .rounded(size * (1. / 6.));
    match state {
        // Stop: a soft tile with the ink square, never louder than the
        // work it would stop.
        SendState::Busy => {
            let ink = t.colors.text;
            let (rest, hover) = (ink.opacity(0.08), ink.opacity(0.14));
            base.aria_label("Stop the reply")
                .bg(rest)
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(div().size(px(10.)).rounded(px(2.)).bg(ink))
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new("Stop (Esc)").build(window, cx)
                })
                .on_click(on_click)
        }
        // Send is the mark's tile holding its run triangle, so it takes the
        // opposite of the ground as the mark does: a lime tile on ink, an
        // ink tile with a lime triangle on paper.
        SendState::Ready => {
            let c = super::mark::Colors::of(t);
            base.aria_label("Send")
                .bg(c.tile)
                .cursor_pointer()
                .hover(|s| s.opacity(0.88))
                .active(|s| s.opacity(0.8))
                // The mark's run triangle: KodAct acts.
                .child(icon("kodact/run.svg", t.size.icon, c.ink))
                .on_click(on_click)
        }
        // Nothing to send: the tile faint, the triangle in it fainter.
        SendState::Empty => {
            let c = super::mark::Colors::of(t);
            base.aria_label("Send (write something first)")
                .bg(c.tile.opacity(0.12))
                .child(icon("kodact/run.svg", t.size.icon, c.tile.opacity(0.45)))
        }
    }
}

/// A switch: a pill track (the toggle blue when on) and a round thumb that
/// slides across when it changes. It always shows `on`, the real state.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    on: bool,
    label: SharedString,
    on_toggle: Option<OnToggle>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>, on: bool, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            on,
            label: label.into(),
            on_toggle: None,
        }
    }

    /// Called with the new state when the switch is flipped.
    pub fn on_toggle(mut self, f: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        use gpui::{Animation, AnimationExt as _};
        let t = cx.theme().clone();
        let focus = focus_handle(&self.id, window, cx);
        let (w, h, thumb) = (t.size.switch_w, t.size.switch_h, t.size.switch_thumb);
        let inset = (h - thumb) / 2.;
        let travel = f32::from(w - thumb - inset * 2.);
        let on = self.on;
        let track = if on {
            t.extra.toggle_on
        } else {
            t.extra.border_strong
        };
        let ms = if super::motion::reduced_motion() {
            1
        } else {
            t.size.pop_in_ms
        };
        // Keyed by the state, so each change slides the thumb from the old side.
        let anim_id = ElementId::NamedChild(
            std::sync::Arc::new(self.id.clone()),
            if on { "on".into() } else { "off".into() },
        );
        let knob = div()
            .absolute()
            .top(inset)
            .size(thumb)
            .rounded_full()
            .bg(t.colors.primary_fg)
            .with_animation(
                anim_id,
                Animation::new(std::time::Duration::from_millis(ms))
                    .with_easing(gpui::ease_out_quint()),
                move |d, k| {
                    let at = if on { k } else { 1. - k };
                    d.left(inset + px(travel * at))
                },
            );
        let toggle = self.on_toggle;
        div()
            .id(self.id)
            .role(accesskit::Role::Switch)
            .aria_label(self.label)
            .aria_toggled(if on {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            })
            .track_focus(&focus)
            .tab_index(0)
            .relative()
            .flex_none()
            .w(w)
            .h(h)
            .rounded(t.radius.pill)
            .bg(track)
            .border_1()
            .border_color(track)
            .focus_visible({
                let edge = focus_edge(&t);
                move |s| s.border_color(edge)
            })
            .cursor_pointer()
            .child(knob)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .when_some(toggle, |d, f| {
                d.on_click(move |_, window, cx| f(!on, window, cx))
            })
    }
}

/// A row of mutually exclusive choices on a recessed track; the chosen one
/// is raised onto the surface.
#[derive(IntoElement)]
pub struct Segmented {
    id: ElementId,
    label: SharedString,
    options: Vec<(SharedString, bool, OnClick)>,
}

impl Segmented {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            options: Vec::new(),
        }
    }

    pub fn option(
        mut self,
        label: impl Into<SharedString>,
        selected: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.options
            .push((label.into(), selected, Box::new(on_click)));
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = cx.theme().clone();
        let mut row = div()
            .id(self.id.clone())
            .role(accesskit::Role::RadioGroup)
            .aria_label(self.label)
            .flex()
            .flex_none()
            .items_center()
            .p(px(3.))
            .gap(px(2.))
            .rounded(t.radius.pill)
            .bg(t.extra.sunk)
            .border_1()
            .border_color(t.colors.border);
        for (i, (label, selected, on_click)) in self.options.into_iter().enumerate() {
            let id = ElementId::NamedChild(
                std::sync::Arc::new(self.id.clone()),
                SharedString::from(format!("seg-{i}")),
            );
            let focus = focus_handle(&id, window, cx);
            let (muted, text) = (t.colors.text_muted, t.colors.text);
            row = row.child(
                div()
                    .id(id)
                    .role(accesskit::Role::RadioButton)
                    .aria_label(label.clone())
                    .aria_toggled(if selected {
                        accesskit::Toggled::True
                    } else {
                        accesskit::Toggled::False
                    })
                    .track_focus(&focus)
                    .tab_index(0)
                    .h(px(24.))
                    .px(t.space.snug)
                    .flex()
                    .items_center()
                    .rounded(t.radius.segment)
                    .border_1()
                    .text_size(t.text.caption)
                    .map(|d| {
                        if selected {
                            d.bg(t.colors.surface)
                                .border_color(t.colors.border)
                                .text_color(text)
                                .font_weight(gpui::FontWeight::MEDIUM)
                        } else {
                            d.border_color(t.colors.surface.opacity(0.))
                                .text_color(muted)
                                .cursor_pointer()
                                .hover(move |s| s.text_color(text))
                        }
                    })
                    .focus_visible({
                        let edge = focus_edge(&t);
                        move |s| s.border_color(edge)
                    })
                    .child(label)
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .when(!selected, |d| d.on_click(on_click)),
            );
        }
        row
    }
}

/// A selector's trigger as a plain row the caller fills (an icon, a label,
/// a chevron) and clicks: a compact ghost control, washed on hover and
/// raised onto the surface while its menu is open. The composer's project,
/// mode and model use it.
pub fn selector(t: &Theme, id: impl Into<ElementId>, open: bool) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    let (raised, edge) = (t.colors.surface, t.colors.border);
    div()
        .id(id)
        .role(accesskit::Role::ComboBox)
        .flex()
        .flex_none()
        .items_center()
        .gap(t.space.tight)
        .h(t.size.button_sm)
        .px(t.space.sm)
        .rounded(t.radius.compact)
        // A clear edge at rest, so opening it doesn't shift the label.
        .border_1()
        .border_color(t.colors.border.opacity(0.))
        .cursor_pointer()
        .text_size(t.text.caption)
        .text_color(t.colors.text)
        .map(|d| {
            if open {
                d.bg(raised).border_color(edge)
            } else {
                d.hover(move |s| s.bg(hover))
            }
        })
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

/// The chevron a selector ends with: down, or up while its menu (which
/// opens above) is showing.
pub fn chevron(t: &Theme, open: bool) -> gpui::Svg {
    icon(
        if open {
            "icons/chevron-up.svg"
        } else {
            "icons/chevron-down.svg"
        },
        t.size.icon_xs,
        t.colors.text_muted,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette_check::contrast;

    /// Every variant's label reads on its own fill, at rest and hovered, in
    /// light and dark: AA for text.
    #[test]
    fn every_variant_reads_on_its_fill() {
        for t in crate::theme::Look::ALL
            .into_iter()
            .flat_map(|l| [Theme::of(l, false), Theme::of(l, true)])
        {
            for v in [
                Variant::Primary,
                Variant::Secondary,
                Variant::Ghost,
                Variant::Outlined,
                Variant::Danger,
            ] {
                let p = v.paint(&t);
                for bg in [p.bg, p.hover_bg] {
                    // A clear fill shows the surface it sits on.
                    let ground = if bg.a < 0.01 {
                        t.colors.surface
                    } else {
                        over(bg, t.colors.surface)
                    };
                    let c = contrast(p.fg, ground);
                    assert!(
                        c >= 4.5,
                        "{v:?} ({}): {c:.2}:1",
                        if t.is_dark { "dark" } else { "light" }
                    );
                }
            }
            // The send arrow is an icon on the accent: a UI part, 3:1.
            let a = Variant::Accent.paint(&t);
            assert!(contrast(a.fg, a.bg) >= 3.0);
            // A switch that is on stands off the card it sits on.
            assert!(contrast(t.extra.toggle_on, t.colors.surface) >= 3.0);
        }
    }

    /// KodAct's look has two shapes: what you press is a pill, what holds
    /// content is 4px. Every theme keeps the order: what you press is at
    /// least as round as a row, and the composer at least as round as a card.
    #[test]
    fn pills_to_press_four_pixels_to_hold() {
        for dark in [false, true] {
            let r = Theme::of(crate::theme::Look::KodAct, dark).radius;
            let z = Theme::of(crate::theme::Look::KodAct, dark).size;
            for v in [r.compact, r.control, r.segment, r.pill] {
                assert!(f32::from(v) >= 999., "{}px", f32::from(v));
            }
            for v in [r.button, r.card, r.field, z.composer_radius] {
                assert_eq!(f32::from(v), 4.);
            }
        }
        for t in crate::theme::Look::ALL
            .into_iter()
            .flat_map(|l| [Theme::of(l, false), Theme::of(l, true)])
        {
            let (r, name) = (t.radius, t.look.name());
            assert!(r.compact >= r.button && r.control >= r.button, "{name}");
            assert!(t.size.composer_radius >= r.card, "{name}");
            assert!(f32::from(r.pill) >= 999., "{name}");
        }
    }
}
