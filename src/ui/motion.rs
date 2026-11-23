//! Motion settings shared by the animated pieces.

/// Motion turned down: `KODACT_REDUCED_MOTION=1` shows still frames instead.
pub fn reduced_motion() -> bool {
    std::env::var("KODACT_REDUCED_MOTION").is_ok_and(|v| v == "1")
}

/// For looking at a transition frame by frame: `KODACT_SLOW_MOTION=8` plays
/// every one eight times slower.
fn slowed() -> f32 {
    std::env::var("KODACT_SLOW_MOTION")
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

/// Menu arrival: soft quartic deceleration so fade and glide track together smoothly.
fn menu_once(ms: u64) -> gpui::Animation {
    gpui::Animation::new(std::time::Duration::from_millis(played(ms)))
        .with_easing(|k: f32| 1. - (1. - k).powi(4))
}

/// Leaving: slow at first, then gone (the reverse of `ease_out_quint`).
fn leave(ms: u64) -> gpui::Animation {
    gpui::Animation::new(std::time::Duration::from_millis(played(ms))).with_easing(|k: f32| k * k)
}

/// Menu exit: immediate, silky deceleration toward the anchor.
fn menu_leave(ms: u64) -> gpui::Animation {
    gpui::Animation::new(std::time::Duration::from_millis(played(ms)))
        .with_easing(|k: f32| 1. - (1. - k).powi(3))
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
        if reduced_motion() {
            self.shown = None;
            self.left_at = None;
            return None;
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

/// Smooth bidirectional interpolation (`0.0` ↔ `1.0`) for a boolean state
/// (such as expanding a document pane into full-screen focus mode and
/// returning it to split view). Supports mid-flight reversal and respects
/// `KODACT_REDUCED_MOTION` and `KODACT_SLOW_MOTION`.
#[derive(Default)]
pub struct Tween {
    target: bool,
    from: f32,
    started: Option<std::time::Instant>,
    last_value: f32,
}

impl Tween {
    /// Current eased progress in `0.0..=1.0`, requesting another frame
    /// while the transition is still in flight.
    pub fn value(
        &mut self,
        target: bool,
        enter_ms: u64,
        exit_ms: u64,
        window: &mut gpui::Window,
    ) -> f32 {
        let goal = if target { 1.0 } else { 0.0 };
        if reduced_motion() {
            self.target = target;
            self.from = goal;
            self.started = None;
            self.last_value = goal;
            return goal;
        }
        if target != self.target {
            self.from = self.last_value;
            self.target = target;
            self.started = Some(std::time::Instant::now());
        }
        let Some(started) = self.started else {
            self.last_value = goal;
            return goal;
        };
        let dur_ms = played(if target { enter_ms } else { exit_ms }).max(1);
        let raw = (started.elapsed().as_secs_f32() / (dur_ms as f32 / 1000.0)).clamp(0.0, 1.0);
        // Entering: poised quintic ease-out; exiting: crisp quartic ease-out.
        let eased = if target {
            1.0 - (1.0 - raw).powi(5)
        } else {
            1.0 - (1.0 - raw).powi(4)
        };
        let val = self.from + (goal - self.from) * eased;
        self.last_value = val;
        if raw >= 1.0 {
            self.started = None;
            self.from = goal;
            self.last_value = goal;
            goal
        } else {
            window.request_animation_frame();
            val
        }
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
    if reduced_motion() && !leaving {
        return el.into_any_element();
    }
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
    if reduced_motion() && !leaving {
        return el.bg(color).into_any_element();
    }
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
/// spotlight does; leaving, it lifts back up and fades, smoothly.
pub fn spotlight<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    if reduced_motion() && !leaving {
        return el.into_any_element();
    }
    if leaving {
        el.with_animation(
            SharedString::from(format!("{id}-out")),
            menu_leave(t.size.menu_out_ms),
            |d, k| d.opacity((1. - k).powf(1.25)).mt(px(-8. * k)),
        )
        .into_any_element()
    } else {
        el.with_animation(
            SharedString::from(format!("{id}-in")),
            menu_once(t.size.pop_in_ms + 60),
            |d, k| d.opacity(k.powf(0.8)).mt(px(-10. * (1. - k))),
        )
        .into_any_element()
    }
}

/// A menu at its anchor (the title bar's, the sidebar's, a chat's): it
/// glides 8–10px smoothly out from its anchor while fading in without clipping
/// shadows; leaving, it settles 8px back toward its anchor while fading cleanly.
/// `up`: it opens upward (anchored by its foot, so bottom margin moves it).
pub fn unfold<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    up: bool,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    if reduced_motion() && !leaving {
        return if up { el.mb(px(8.)) } else { el }.into_any_element();
    }
    if leaving {
        el.with_animation(
            SharedString::from(format!("{id}-out")),
            menu_leave(t.size.menu_out_ms),
            move |d, k| {
                let d = d.opacity((1. - k).powf(1.25));
                if up {
                    d.mb(px(8. * (1. - k)))
                } else {
                    d.mt(px(-8. * k))
                }
            },
        )
        .into_any_element()
    } else {
        el.with_animation(
            SharedString::from(format!("{id}-in")),
            menu_once(t.size.pop_in_ms + 60),
            move |d, k| {
                let d = d.opacity(k.powf(0.8));
                if up {
                    d.mb(px(8. * k))
                } else {
                    d.mt(px(-10. * (1. - k)))
                }
            },
        )
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

/// A menu that hangs below what opened it (the terminal's shell list, a
/// right-click menu): it glides cleanly down into place and fades softly out
/// toward its anchor.
pub fn drop_menu(
    body: gpui::Stateful<gpui::Div>,
    id: &str,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    unfold(body, id, false, leaving, t)
}

/// A menu over the composer (the model, the mode, the design chips, the +):
/// it glides 10px upward into place with quartic easing on open and settles back
/// 8px toward its trigger on close. `lift` is its bottom margin at rest.
pub fn composer_menu(
    body: gpui::Stateful<gpui::Div>,
    id: &str,
    lift: f32,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    if reduced_motion() && !leaving {
        return body.mb(px(lift)).into_any_element();
    }
    if leaving {
        body.with_animation(
            SharedString::from(format!("{id}-out")),
            menu_leave(t.size.menu_out_ms),
            move |d, k| d.opacity((1. - k).powf(1.25)).mb(px(lift - 8. * k)),
        )
        .into_any_element()
    } else {
        body.with_animation(
            SharedString::from(format!("{id}-in")),
            menu_once(t.size.pop_in_ms + 60),
            move |d, k| d.opacity(k.powf(0.8)).mb(px(lift - 10. * (1. - k))),
        )
        .into_any_element()
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
        let anim =
            gpui::Animation::new(std::time::Duration::from_millis(played(t.size.toast_in_ms)));
        el.with_animation(SharedString::from(format!("{id}-in")), anim, |d, k| {
            d.opacity((k * 2.5).min(1.)).mb(px(-18. * (1. - spring(k))))
        })
        .into_any_element()
    }
}

/// How long a panel takes to close: its content fades first (fast, 120),
/// then its width closes (base, 200). The presences that keep a panel on
/// screen while it leaves wait this long.
pub fn close_ms(t: &crate::theme::Theme) -> u64 {
    t.size.pop_in_ms + t.size.panel_out_ms
}

/// Where a closing panel is at `elapsed` of `close_ms` (both as played):
/// the content's opacity, then the share of its width that is left. The
/// content is gone before the width starts to close.
fn close_at(elapsed: f32, fade_ms: f32, width_ms: f32) -> (f32, f32) {
    let fade = (elapsed / fade_ms.max(1.)).clamp(0., 1.);
    let shut = ((elapsed - fade_ms) / width_ms.max(1.)).clamp(0., 1.);
    (1. - fade, 1. - gpui::ease_in_out(shut))
}

/// Where an opening panel is at `elapsed` (as played): the share of its
/// width, over `width_ms`, and its content's progress, which starts `lag_ms`
/// after the width and runs `rise_ms`.
fn open_at(elapsed: f32, width_ms: f32, lag_ms: f32, rise_ms: f32) -> (f32, f32) {
    let width = (elapsed / width_ms.max(1.)).clamp(0., 1.);
    let rise = ((elapsed - lag_ms) / rise_ms.max(1.)).clamp(0., 1.);
    (gpui::ease_out_quint()(width), gpui::ease_out_quint()(rise))
}

/// The content starts this long after the width (fast, shorter than a hover).
const OPEN_LAG_MS: u64 = 40;
/// And rises this far as it fades in.
const OPEN_RISE_PX: f32 = 8.;

/// The reverse of `slide_in`: the side panel's content fades first (fast),
/// then its width closes (base) and the chat eases wider.
pub fn slide_out(
    pane: gpui::AnyElement,
    width: f32,
    id: &str,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _, SharedString};
    let (fade_ms, width_ms) = (
        played(t.size.pop_in_ms) as f32,
        played(t.size.panel_out_ms) as f32,
    );
    let total = fade_ms + width_ms;
    // Linear: each phase eases itself.
    let anim = |name: &str| {
        (
            SharedString::from(format!("{id}-{name}")),
            gpui::Animation::new(std::time::Duration::from_millis(total as u64)),
        )
    };
    let (wid, wanim) = anim("out");
    let (cid, canim) = anim("out-content");
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
                .child(pane)
                .with_animation(cid, canim, move |d, k| {
                    d.opacity(close_at(k * total, fade_ms, width_ms).0)
                }),
        )
        .with_animation(wid, wanim, move |d, k| {
            d.w(px(width * close_at(k * total, fade_ms, width_ms).1))
        })
        .into_any_element()
}

/// The side panel sliding in from the right edge as it opens: its width grows
/// from nothing to `width` (slow, 280) while the chat beside it eases
/// narrower; its content rises 8px and fades in, starting 40ms after the
/// width. Below the chat (a narrow window) it is left as it is
/// (`split_in` plays that one).
pub fn slide_in(
    pane: gpui::AnyElement,
    width: f32,
    below: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px, AnimationExt as _};
    if below || reduced_motion() {
        // Below the chat (or under reduced motion) the pane stays at its
        // resting size immediately.
        return pane;
    }
    let (width_ms, lag_ms) = (
        played(t.size.panel_in_ms) as f32,
        played(OPEN_LAG_MS) as f32,
    );
    let rise_ms = played(t.size.window_in_ms) as f32;
    let total = (lag_ms + rise_ms).max(width_ms);
    let timed = move || gpui::Animation::new(std::time::Duration::from_millis(total as u64));
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
                .child(pane)
                .with_animation("side-panel-in-content", timed(), move |d, k| {
                    let (_, rise) = open_at(k * total, width_ms, lag_ms, rise_ms);
                    d.opacity(rise).mt(px(OPEN_RISE_PX * (1. - rise)))
                }),
        )
        .with_animation("side-panel-in", timed(), move |d, k| {
            let (w, _) = open_at(k * total, width_ms, lag_ms, rise_ms);
            d.w(px(width * w))
        })
        .into_any_element()
}

/// The panel moving beside the chat to below it: it keeps the share of the
/// height it ends with (`share` of the stage), and its content slides up from
/// the foot of that share as it fades in, over `panel_in_ms`.
pub fn split_in(pane: gpui::AnyElement, share: f32, t: &crate::theme::Theme) -> gpui::AnyElement {
    use gpui::{div, prelude::*, relative, AnimationExt as _};
    let wrap = div()
        .relative()
        .flex_none()
        .w_full()
        .h(relative(share))
        .overflow_hidden();
    let inner = div()
        .absolute()
        .left_0()
        .w_full()
        .h_full()
        .flex()
        .child(pane);
    if reduced_motion() {
        return wrap.child(inner.top_0()).into_any_element();
    }
    wrap.child(
        inner.with_animation("side-panel-split", once(t.size.panel_in_ms), |d, k| {
            d.top(relative(1. - k)).opacity((k * 2.).min(1.))
        }),
    )
    .into_any_element()
}

/// How far the incoming view of a push travels, in px.
const PUSH_PX: f32 = 24.;

/// A detail view pushed over its list (a file opened from the Files tab) and
/// pulled back. `Push` keeps what the detail was, so it can leave as the list
/// returns, for `ms`.
pub struct Push<K> {
    /// The detail the last frame showed.
    shown: Option<K>,
    /// When the detail last came or went, and which way.
    moved: Option<(std::time::Instant, bool)>,
    /// The detail that last went, kept while it leaves.
    left: Option<K>,
}

impl<K> Default for Push<K> {
    fn default() -> Self {
        Self {
            shown: None,
            moved: None,
            left: None,
        }
    }
}

/// What a push draws.
#[derive(Debug, PartialEq)]
pub enum PushFrame<K> {
    /// The list, at rest.
    List,
    /// The detail, at rest.
    Detail,
    /// The detail arriving over the list; progress `0..1`, eased.
    Forward(f32),
    /// The detail (kept) leaving, the list returning; progress `0..1`, eased.
    Back(K, f32),
}

impl<K: Clone + PartialEq> Push<K> {
    /// The frame for `detail` (Some while a detail is open over the list),
    /// `elapsed` after `now` began it; `ms` is as played.
    fn step(&mut self, detail: Option<K>, now: std::time::Instant, ms: u64) -> PushFrame<K> {
        let was = self.shown.is_some();
        if detail.is_some() != was {
            self.moved = Some((now, detail.is_some()));
            if was {
                self.left = self.shown.clone();
            }
        }
        self.shown = detail.clone();
        if reduced_motion() {
            self.moved = None;
        }
        let at = self.moved.and_then(|(from, forward)| {
            let raw = now.duration_since(from).as_secs_f32() / (ms.max(1) as f32 / 1000.);
            (raw < 1.).then_some((raw, forward))
        });
        let Some((raw, forward)) = at else {
            self.moved = None;
            return if detail.is_some() {
                PushFrame::Detail
            } else {
                PushFrame::List
            };
        };
        let k = gpui::ease_out_quint()(raw);
        match (forward, self.left.clone()) {
            (true, _) => PushFrame::Forward(k),
            (false, Some(old)) => PushFrame::Back(old, k),
            (false, None) => PushFrame::List,
        }
    }

    /// `step`, asking for another frame while the push is in flight.
    pub fn frame(&mut self, detail: Option<K>, ms: u64, window: &mut gpui::Window) -> PushFrame<K> {
        let f = self.step(detail, std::time::Instant::now(), played(ms));
        if matches!(f, PushFrame::Forward(_) | PushFrame::Back(..)) {
            window.request_animation_frame();
        }
        f
    }
}

/// A push's two layers, at progress `k`: the list under (dimmed as the
/// detail covers it, `forward`; brought back as it leaves) and the detail
/// over it (24px in from the right while it fades in; going back, out to
/// the right as it fades). Both fill the same box.
pub fn push(
    list: gpui::AnyElement,
    detail: gpui::AnyElement,
    width: f32,
    forward: bool,
    k: f32,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px};
    let (list_a, detail_a, dx) = if forward {
        (1. - 0.6 * k, k, PUSH_PX * (1. - k))
    } else {
        (0.4 + 0.6 * k, 1. - k, PUSH_PX * k)
    };
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h_full()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .opacity(list_a)
                .child(list),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .size_full()
                .left(px(dx))
                .flex()
                .opacity(detail_a)
                .child(detail),
        )
        .into_any_element()
}

/// A panel tab arriving: it fades in while it shifts 6px in the direction
/// of its tab (from the right for a tab further along, from the left for
/// one before). Keyed by `id`, so each switch plays it.
pub fn tab_in<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: impl Into<gpui::ElementId>,
    from_right: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _};
    if reduced_motion() {
        return el.into_any_element();
    }
    let sign = if from_right { 1. } else { -1. };
    el.with_animation(id, once(t.size.pop_in_ms), move |d, k| {
        d.opacity(k).relative().left(px(6. * sign * (1. - k)))
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
    let box_el = div().flex_none().child(row);
    if reduced_motion() {
        return box_el.into_any_element();
    }
    box_el
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
    let box_el = div().w_full().child(row);
    if reduced_motion() {
        return box_el.into_any_element();
    }
    box_el
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
    if reduced_motion() {
        return el.into_any_element();
    }
    el.with_animation(id, once(t.size.pop_in_ms), move |d, k| {
        d.opacity(k).mt(px(rise * (1. - k)))
    })
    .into_any_element()
}

/// A floating bar or badge (the browser's hints, a card at a pin): it rises
/// `rise` pixels into place as it fades in, as `rise_in`; leaving, it sinks
/// half of that back and fades, slow at first.
pub fn float<E: gpui::Styled + gpui::IntoElement + 'static>(
    el: E,
    id: &str,
    rise: f32,
    leaving: bool,
    t: &crate::theme::Theme,
) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _, SharedString};
    if leaving {
        el.with_animation(
            SharedString::from(format!("{id}-out")),
            leave(t.size.menu_out_ms),
            move |d, k| d.opacity(1. - k).mt(px(rise * 0.5 * k)),
        )
        .into_any_element()
    } else {
        rise_in(el, SharedString::from(format!("{id}-in")), rise, t)
    }
}

/// A screen arriving on the session's sheet (Projects, Documents, Design,
/// Rules, back to the chat): it fades in while it rises 10px, over
/// `panel_in_ms`, as a chat's transcript does. Keyed by the screen, so each
/// switch plays it; the sheet under it stays.
pub fn screen_in(el: gpui::Div, screen: usize, t: &crate::theme::Theme) -> gpui::AnyElement {
    use gpui::{prelude::*, px, AnimationExt as _};
    if reduced_motion() {
        return el.into_any_element();
    }
    el.with_animation(("screen-in", screen), once(t.size.panel_in_ms), |d, k| {
        d.opacity(k).mt(px(10. * (1. - k)))
    })
    .into_any_element()
}

/// The pace of a card changing size (the side question card growing or
/// folding): `panel_in_ms`, fast then settling.
pub fn resize(t: &crate::theme::Theme) -> gpui::Animation {
    once(t.size.panel_in_ms)
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
    if reduced_motion() {
        return div()
            .flex_none()
            .h_full()
            .w(to)
            .flex()
            .child(pane)
            .into_any_element();
    }
    let full = if from > to { from } else { to };
    let opening = to > from;
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
                .w(full)
                .flex()
                .child(pane),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_closing_panel_fades_before_its_width_closes() {
        // 120ms of fade, then 200ms of width.
        let (op, w) = close_at(0., 120., 200.);
        assert_eq!((op, w), (1., 1.));
        let (op, w) = close_at(120., 120., 200.);
        assert_eq!(op, 0.);
        assert_eq!(w, 1.);
        let (op, w) = close_at(60., 120., 200.);
        assert!(op > 0. && op < 1. && w == 1.);
        let (op, w) = close_at(220., 120., 200.);
        assert_eq!(op, 0.);
        assert!(w > 0. && w < 1.);
        assert_eq!(close_at(320., 120., 200.), (0., 0.));
    }

    #[test]
    fn an_opening_panel_lands_open_with_its_content_a_beat_behind() {
        let (w, r) = open_at(0., 280., 40., 200.);
        assert_eq!((w, r), (0., 0.));
        // 30ms in: the width has started, the content has not.
        let (w, r) = open_at(30., 280., 40., 200.);
        assert!(w > 0. && r == 0.);
        let (w, r) = open_at(240., 280., 40., 200.);
        assert!(w > 0.9 && r == 1.);
        assert_eq!(open_at(280., 280., 40., 200.), (1., 1.));
    }

    #[test]
    fn a_push_runs_forward_and_back_and_lands_at_rest() {
        let mut p: Push<&str> = Push::default();
        let t0 = Instant::now();
        let ms = 200;
        // At rest on the list.
        assert_eq!(p.step(None, t0, ms), PushFrame::List);
        // A file opens: it arrives, then rests.
        let f = p.step(Some("a.md"), t0, ms);
        assert!(matches!(f, PushFrame::Forward(k) if k < 0.01));
        let f = p.step(Some("a.md"), t0 + Duration::from_millis(100), ms);
        assert!(matches!(f, PushFrame::Forward(k) if k > 0. && k < 1.));
        assert_eq!(
            p.step(Some("a.md"), t0 + Duration::from_millis(250), ms),
            PushFrame::Detail
        );
        // Back: the file leaves as the list returns, then the list rests.
        let t1 = t0 + Duration::from_millis(300);
        let f = p.step(None, t1, ms);
        assert!(matches!(f, PushFrame::Back("a.md", k) if k < 0.01));
        let f = p.step(None, t1 + Duration::from_millis(100), ms);
        assert!(matches!(f, PushFrame::Back("a.md", k) if k > 0. && k < 1.));
        assert_eq!(
            p.step(None, t1 + Duration::from_millis(250), ms),
            PushFrame::List
        );
        // Another file over the open one is no push.
        let mut p: Push<&str> = Push::default();
        p.step(Some("a"), t0, ms);
        assert_eq!(
            p.step(Some("a"), t0 + Duration::from_millis(500), ms),
            PushFrame::Detail
        );
        assert_eq!(
            p.step(Some("b"), t0 + Duration::from_millis(600), ms),
            PushFrame::Detail
        );
    }
}
