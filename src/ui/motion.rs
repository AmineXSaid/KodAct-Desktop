//! Motion settings shared by the animated pieces.

/// Motion turned down: `FORGE_REDUCED_MOTION=1` shows still frames instead.
pub fn reduced_motion() -> bool {
    std::env::var("FORGE_REDUCED_MOTION").is_ok_and(|v| v == "1")
}

/// For looking at a transition frame by frame: `FORGE_SLOW_MOTION=8` plays
/// every one eight times slower.
fn slowed() -> f32 {
    std::env::var("FORGE_SLOW_MOTION")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|f| *f >= 1.)
        .unwrap_or(1.)
}

/// `ms` as played: 1ms under reduced motion, longer in slow motion.
pub fn played(ms: u64) -> u64 {
    if reduced_motion() {
        1
    } else {
        (ms as f32 * slowed()) as u64
    }
}

fn once(ms: u64) -> gpui::Animation {
    gpui::Animation::new(std::time::Duration::from_millis(played(ms)))
        .with_easing(gpui::ease_out_quint())
}

/// Leaving: slow at first, then gone (the reverse of `ease_out_quint`).
fn leave(ms: u64) -> gpui::Animation {
    gpui::Animation::new(std::time::Duration::from_millis(played(ms)))
        .with_easing(|k: f32| k * k)
}

/// A spring that runs a little past its rest and settles back.
fn spring(k: f32) -> f32 {
    let (c1, c3) = (1.4_f32, 2.4_f32);
    1. + c3 * (k - 1.).powi(3) + c1 * (k - 1.).powi(2)
}

/// What a window shows as it opens, rests and leaves. A window's flag goes
/// off the moment it closes, wherever that happens; `Presence` keeps what
/// it showed for the beat its exit takes, so the window can leave rather
/// than vanish. One per window, in the view that draws it.
pub struct Presence<K> {
    shown: Option<K>,
    left_at: Option<std::time::Instant>,
}

impl<K> Default for Presence<K> {
    fn default() -> Self {
        Self {
            shown: None,
            left_at: None,
        }
    }
}

impl<K: Clone> Presence<K> {
    /// What to draw: the open window's key (`false`), or the one just closed
    /// while it leaves for `out_ms` (`true`); None once it has gone.
    pub fn frame(
        &mut self,
        now: Option<K>,
        out_ms: u64,
        window: &mut gpui::Window,
    ) -> Option<(K, bool)> {
        if let Some(k) = now {
            self.shown = Some(k.clone());
            self.left_at = None;
            return Some((k, false));
        }
        let k = self.shown.clone()?;
        let left = *self.left_at.get_or_insert_with(std::time::Instant::now);
        if left.elapsed() >= std::time::Duration::from_millis(played(out_ms)) {
            self.shown = None;
            self.left_at = None;
            return None;
        }
        // Drawn again until it has gone, then once more without it.
        window.request_animation_frame();
        Some((k, true))
    }
}

/// A dialog (Settings, the shortcuts, the map, a confirmation): it rises
/// 14px into place as it fades in over its dimmed backdrop; leaving, it
/// sinks 6px and fades, slow at first. The steady, weighty one.
pub fn dialog<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    if leaving {
        el.with_animation(
            SharedString::from(format!("{id}-out")),
            leave(t.size.window_out_ms),
            |d, k| d.opacity(1. - k).mt(px(6. * k)),
        )
        .into_any_element()
    } else {
        el.with_animation(
            SharedString::from(format!("{id}-in")),
            once(t.size.window_in_ms),
            |d, k| d.opacity(k).mt(px(14. * (1. - k))),
        )
        .into_any_element()
    }
}

/// The backdrop under a dialog, dimming as it opens and clearing as it
/// leaves, in step with it.
pub fn backdrop<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    color: gpui::Hsla,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, AnimationExt as _, SharedString};
    let (anim, key) = if leaving {
        (leave(t.size.window_out_ms), format!("{id}-out"))
    } else {
        (once(t.size.window_in_ms), format!("{id}-in"))
    };
    el.with_animation(SharedString::from(key), anim, move |d, k| {
        let k = if leaving { 1. - k } else { k };
        d.bg(gpui::Hsla {
            a: color.a * k,
            ..color
        })
    })
    .into_any_element()
}

/// The command palette: it drops 10px from above as it fades in, as a
/// spotlight does; leaving, it lifts back up and fades, quickly.
pub fn spotlight<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    if leaving {
        el.with_animation(
            SharedString::from(format!("{id}-out")),
            leave(t.size.menu_out_ms),
            |d, k| d.opacity(1. - k).mt(px(-8. * k)),
        )
        .into_any_element()
    } else {
        el.with_animation(
            SharedString::from(format!("{id}-in")),
            once(t.size.pop_in_ms + 40),
            |d, k| d.opacity(k).mt(px(-10. * (1. - k))),
        )
        .into_any_element()
    }
}

/// A menu at its anchor (the title bar's, a chat's, the composer's): it
/// unfolds from the anchor, its height opening while it fades in; leaving,
/// it fades and drifts 3px back toward the anchor. The quick, light one.
/// `up`: it opens upward (anchored by its foot).
pub fn unfold<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    up: bool,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    let toward = if up { 3. } else { -3. };
    if leaving {
        el.with_animation(
            SharedString::from(format!("{id}-out")),
            leave(t.size.menu_out_ms),
            move |d, k| d.opacity(1. - k).mt(px(toward * k)),
        )
        .into_any_element()
    } else {
        // On linear time: the reveal runs at an even 1.6px a millisecond
        // (so a short menu unfolds as visibly as a long one), the fade eased.
        let ms = t.size.pop_in_ms + 60;
        let anim = gpui::Animation::new(std::time::Duration::from_millis(played(ms)));
        let ease = gpui::ease_out_quint();
        el.with_animation(SharedString::from(format!("{id}-in")), anim, move |d, k| {
            // Clipped only while it opens, so its shadow shows at rest.
            d.when(k < 1., |d| {
                d.overflow_hidden().max_h(px(16. + 1.6 * ms as f32 * k))
            })
            .opacity(ease(k).min(1.))
        })
        .into_any_element()
    }
}

/// A question that needs an answer (delete, revert, a password): it opens
/// by drawing its edge in `accent` (`edge_draw`), and leaves as a dialog
/// does, sinking away.
pub fn alert(
    body: gpui::Stateful<gpui::Div>,
    id: &str,
    accent: gpui::Hsla,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    if leaving {
        dialog(body, id, true, t)
    } else {
        edge_draw(body, format!("{id}-in"), accent, 0., t)
    }
}

/// A menu over the composer (the model, the mode, the +): it opens by
/// drawing its jewel edge (`edge_draw`); leaving, it fades and drifts back
/// down toward the composer. `lift` is its bottom margin at rest.
pub fn composer_menu(
    body: gpui::Stateful<gpui::Div>,
    id: &str,
    lift: f32,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px};
    if leaving {
        unfold(body.mb(px(lift)), id, true, true, t)
    } else {
        edge_draw(body, format!("{id}-in"), t.colors.primary, lift, t)
    }
}

/// A notice at the foot of the window: it springs up 18px, running a hair
/// past its rest; leaving, it drops 10px and fades. The lively one.
pub fn notice<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    if leaving {
        el.with_animation(
            SharedString::from(format!("{id}-out")),
            leave(t.size.window_out_ms),
            |d, k| d.opacity(1. - k).mb(px(-10. * k)),
        )
        .into_any_element()
    } else {
        let anim = gpui::Animation::new(std::time::Duration::from_millis(played(
            t.size.toast_in_ms,
        )));
        el.with_animation(SharedString::from(format!("{id}-in")), anim, |d, k| {
            d.opacity((k * 2.5).min(1.)).mb(px(-18. * (1. - spring(k))))
        })
        .into_any_element()
    }
}

/// The reverse of `slide_in`: the side panel slides back out to the right
/// edge, its width closing while it fades, and the chat eases wider.
pub fn slide_out(
    pane: gpui::AnyElement,
    width: f32,
    id: &str,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _, SharedString};
    div()
        .relative()
        .flex_none()
        .h_full()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(width))
                .flex()
                .child(pane),
        )
        // Eased both ways, so it is seen leaving rather than snatched away.
        .with_animation(
            SharedString::from(format!("{id}-out")),
            gpui::Animation::new(std::time::Duration::from_millis(played(t.size.panel_out_ms)))
                .with_easing(gpui::ease_in_out),
            move |d, k| d.w(px(width * (1. - k))).opacity(1. - k * k),
        )
        .into_any_element()
}

/// The side panel sliding in from the right edge as it opens: its width grows
/// from nothing to `width` while the chat beside it eases narrower, and the
/// panel fades in. Below the chat (a narrow window) it is left as it is.
pub fn slide_in(
    pane: gpui::AnyElement,
    width: f32,
    below: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _};
    let anim = once(t.size.panel_in_ms);
    if below {
        // Below the chat the pane's height is a share of its parent's, and
        // it fades in on its own.
        return pane;
    }
    // The panel keeps its full width inside, pinned to the wrapper's left
    // edge, so it moves in with that edge rather than being uncovered.
    div()
        .relative()
        .flex_none()
        .h_full()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(width))
                .flex()
                .child(pane),
        )
        .with_animation("side-panel-in", anim, move |d, k| {
            d.w(px(width * k)).opacity((k * 1.6).min(1.))
        })
        .into_any_element()
}

/// A list opening row by row: row `index` starts a beat after the one above
/// it and rises into place over `pop_in_ms`. Rows past the eighth share its
/// beat, so a long list never keeps you waiting.
pub fn stagger(t: &crate::theme::Theme, index: usize) -> gpui::Animation {
    if reduced_motion() {
        return once(1);
    }
    let rise = t.size.pop_in_ms as f32;
    let delay = (t.size.stagger_ms * index.min(8) as u64) as f32;
    let total = rise + delay;
    let ease = gpui::ease_out_quint();
    gpui::Animation::new(std::time::Duration::from_millis(total as u64))
        .with_easing(move |k| ease(((k * total - delay) / rise).clamp(0., 1.)))
}

/// A staggered row: fades in from a few pixels lower, a beat after the row
/// above it. `id` names the opening, so a new page of rows plays it again.
pub fn row_in(
    row: impl gpui::IntoElement + 'static,
    id: impl Into<gpui::ElementId>,
    index: usize,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _};
    div()
        .flex_none()
        .child(row)
        .with_animation(id, stagger(t, index), |d, k| {
            d.opacity(k).mt(px(4. * (1. - k))).mb(px(-4. * (1. - k)))
        })
        .into_any_element()
}

/// `row_in` for a row that fills its parent's width (a card in a grid).
pub fn row_in_full(
    row: impl gpui::IntoElement + 'static,
    id: impl Into<gpui::ElementId>,
    index: usize,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _};
    div()
        .w_full()
        .child(row)
        .with_animation(id, stagger(t, index), |d, k| {
            d.opacity(k).mt(px(4. * (1. - k))).mb(px(-4. * (1. - k)))
        })
        .into_any_element()
}

/// A window drawing itself as it opens: a jewel bar runs down its left edge
/// while the window rises a few pixels into place and fades in, then the bar
/// fades away, leaving the window as it rests. The caller positions `body`
/// and makes it `relative`; `lift` is its bottom margin at rest, for one
/// that opens upward from what anchors it.
pub fn edge_draw(
    body: gpui::Stateful<gpui::Div>,
    id: impl Into<gpui::ElementId>,
    accent: gpui::Hsla,
    lift: f32,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, relative, AnimationExt as _};
    let inset = f32::from(t.radius.card);
    let line = t.radius.line;
    let bar = f32::from(t.size.edge_bar);
    let draw_ms = t.size.edge_draw_ms as f32;
    let ms = t.size.pop_in_ms + t.size.edge_draw_ms;
    // Linear time: the bar and the fade keep their own pace inside it.
    let anim = if reduced_motion() {
        gpui::Animation::new(std::time::Duration::from_millis(1))
    } else {
        gpui::Animation::new(std::time::Duration::from_millis(ms))
    };
    let split = draw_ms / ms as f32;
    let ease = gpui::ease_out_quint();
    body.with_animation(id, anim, move |d, k| {
        let draw = ease((k / split).min(1.));
        let rise = ease((k / (1. - split)).min(1.));
        let fade = ((k - split) / (1. - split)).clamp(0., 1.);
        d.opacity(rise)
            .mb(px(lift - 3. * (1. - rise)))
            .when(k < 1., |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(inset))
                        .bottom(px(inset))
                        .w(px(bar))
                        .child(
                            div()
                                .w_full()
                                .h(relative(draw))
                                .rounded(line)
                                .bg(accent.opacity(1. - fade)),
                        ),
                )
            })
    })
    .into_any_element()
}

/// A window or a page arriving: it fades in while it rises `rise` pixels into
/// place. Keyed by `id`, so a new page (another Settings section) plays it
/// again while the old one is simply gone: the content swaps, the frame stays.
pub fn rise_in<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: impl Into<gpui::ElementId>,
    rise: f32,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _};
    el.with_animation(id, once(t.size.pop_in_ms), move |d, k| {
        d.opacity(k).mt(px(rise * (1. - k)))
    })
    .into_any_element()
}

/// The sidebar opening out of its rail, or folding back into it: the width
/// eases from `from` to `to` while what's inside keeps its own width, pinned
/// to the left, so it slides out with the edge rather than being squeezed.
/// Keyed by `id`: each open (or close) plays it again.
pub fn sidebar_slide(
    pane: gpui::AnyElement,
    from: gpui::Pixels,
    to: gpui::Pixels,
    id: &'static str,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, AnimationExt as _};
    let full = if from > to { from } else { to };
    let opening = to > from;
    div()
        .relative()
        .flex_none()
        .h_full()
        .overflow_hidden()
        .child(div().absolute().top_0().bottom_0().left_0().w(full).flex().child(pane))
        .with_animation(id, once(t.size.panel_in_ms), move |d, k| {
            let d = d.w(from + (to - from) * k);
            if opening {
                d.opacity((0.4 + k).min(1.))
            } else {
                d
            }
        })
        .into_any_element()
}

/// A row's hover wash easing in under its content, where `.hover()` would
/// snap. Make it the row's first child; the row is `relative`.
pub fn hover_wash(
    id: impl Into<gpui::ElementId>,
    color: gpui::Hsla,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, AnimationExt as _};
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .rounded(t.radius.button)
        .bg(color)
        .with_animation(id, once(t.motion_ms), |d, k| d.opacity(k))
        .into_any_element()
}

/// The sidebar's shadow falling across the session's left edge (put it last
/// in the session, which is `relative` and clips to its corners). Nothing in
/// a flat theme.
pub fn side_shade(t: &crate::theme::Theme) -> Option<gpui::AnyElement> {
    use gpui::{div, prelude::*};
    let edge = t.side_shade()?;
    Some(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(t.size.side_shade)
            .bg(gpui::linear_gradient(
                90.,
                gpui::linear_color_stop(edge, 0.),
                gpui::linear_color_stop(edge.opacity(0.), 1.),
            ))
            .into_any_element(),
    )
}

