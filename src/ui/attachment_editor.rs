//! The attachment editor: the pencil on a tile in the composer opens it.
//! A pasted text opens as text to edit (all of it erased removes the card).
//! A picture opens as a drawing pad: a pen in five colours and three
//! widths, Undo and Clear; Save burns the drawing into the picture
//! (`draw.rs`), so the model sees what you circled or crossed out. Esc
//! closes it without saving; Ctrl+Z undoes a stroke.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::component::input::{Textarea, TextareaState};
use gpui::{
    canvas, div, img, point, prelude::*, px, Bounds, Context, Entity, FocusHandle, Image,
    ImageFormat, IntoElement, KeyDownEvent, MouseButton, ObjectFit, PathBuilder, Pixels, Render,
    SharedString, Subscription, Window,
};

use super::button::Button;
use crate::attach::Kind;
use crate::draw::Stroke;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

/// The picture being drawn on.
struct Picture {
    id: u64,
    name: SharedString,
    bytes: Arc<[u8]>,
    image: Arc<Image>,
    /// Its size in its own pixels.
    w: f32,
    h: f32,
}

enum Open {
    Text { id: u64, name: SharedString },
    Picture(Picture),
}

/// The pad's area: the picture is fitted inside it.
const PAD_W: f32 = 720.;
const PAD_H: f32 = 420.;
/// Pen widths, in the pad's pixels.
const WIDTHS: [f32; 3] = [2., 4., 8.];

pub struct AttachmentEditor {
    state: Entity<AppState>,
    focus: FocusHandle,
    open: Option<Open>,
    text: Entity<TextareaState>,
    strokes: Vec<Stroke>,
    /// The stroke under the pointer, until it lifts.
    live: Option<Stroke>,
    color: usize,
    width: usize,
    /// Where the picture was painted last frame, for the pointer's position.
    shown: Rc<Cell<Option<Bounds<Pixels>>>>,
    saving: bool,
    error: Option<SharedString>,
    /// Closed, and sinking away: what it showed stays for that beat.
    leaving: bool,
    _subs: Vec<Subscription>,
}

impl AttachmentEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextareaState::new(window, cx));
        let subs = vec![cx.observe_in(&state, window, |this, state, window, cx| {
            let want = state.read(cx).editing_attachment;
            let have = this.open.as_ref().map(|o| match o {
                Open::Text { id, .. } => *id,
                Open::Picture(p) => p.id,
            });
            if want == have {
                this.leaving = false;
            } else if want.is_none() && !super::motion::reduced_motion() {
                if !this.leaving {
                    this.leaving = true;
                    let out = super::motion::played(cx.theme().size.window_out_ms);
                    cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(out))
                            .await;
                        let _ = this.update_in(cx, |this, window, cx| {
                            if this.leaving && this.state.read(cx).editing_attachment.is_none() {
                                this.leaving = false;
                                this.load(None, window, cx);
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                    cx.notify();
                }
            } else {
                this.leaving = false;
                this.load(want, window, cx);
            }
        })];
        Self {
            state,
            focus: cx.focus_handle(),
            open: None,
            text,
            strokes: vec![],
            live: None,
            color: 0,
            width: 1,
            shown: Rc::new(Cell::new(None)),
            saving: false,
            error: None,
            leaving: false,
            _subs: subs,
        }
    }

    fn load(&mut self, id: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        self.strokes.clear();
        self.live = None;
        self.error = None;
        self.saving = false;
        let a = id.and_then(|id| {
            self.state
                .read(cx)
                .attachments
                .iter()
                .find(|a| a.id == id)
                .cloned()
        });
        self.open = match a {
            Some(a) if a.kind == Kind::Pasted => {
                let value = a.text.as_deref().unwrap_or_default().to_string();
                self.text.update(cx, |i, cx| i.set_value(value, window, cx));
                window.focus(&gpui::Focusable::focus_handle(self.text.read(cx), cx), cx);
                Some(Open::Text {
                    id: a.id,
                    name: a.name.clone(),
                })
            }
            Some(a) => match (a.kind, a.bytes.clone()) {
                (Kind::Image(mime), Some(bytes)) => {
                    let format = match mime {
                        "image/jpeg" => ImageFormat::Jpeg,
                        "image/gif" => ImageFormat::Gif,
                        "image/webp" => ImageFormat::Webp,
                        _ => ImageFormat::Png,
                    };
                    let (w, h) = image::load_from_memory(&bytes)
                        .map(|i| (i.width() as f32, i.height() as f32))
                        .unwrap_or((1., 1.));
                    window.focus(&self.focus, cx);
                    Some(Open::Picture(Picture {
                        id: a.id,
                        name: a.name.clone(),
                        image: Arc::new(Image::from_bytes(format, bytes.to_vec())),
                        bytes,
                        w,
                        h,
                    }))
                }
                _ => None,
            },
            None => None,
        };
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| s.close_attachment_editor(cx));
        window.dispatch_action(Box::new(super::FocusComposer), cx);
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.open {
            Some(Open::Text { id, .. }) => {
                let (id, value) = (*id, self.text.read(cx).value().to_string());
                self.state
                    .update(cx, |s, cx| s.set_pasted_text(id, value, cx));
                window.dispatch_action(Box::new(super::FocusComposer), cx);
            }
            Some(Open::Picture(p)) if self.strokes.is_empty() => {
                let _ = p;
                self.close(window, cx);
            }
            Some(Open::Picture(p)) => {
                let (id, bytes, strokes) = (p.id, p.bytes.clone(), self.strokes.clone());
                self.saving = true;
                cx.notify();
                cx.spawn_in(window, async move |this, cx| {
                    let burnt = cx
                        .background_spawn(async move { crate::draw::burn(&bytes, &strokes) })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match burnt {
                        Ok(png) => {
                            this.state
                                .update(cx, |s, cx| s.set_attachment_image(id, png, cx));
                            window.dispatch_action(Box::new(super::FocusComposer), cx);
                        }
                        Err(e) => {
                            this.saving = false;
                            this.error = Some(e.into());
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            None => {}
        }
    }

    /// The pointer's place on the picture, in the picture's own pixels.
    fn on_picture(&self, at: gpui::Point<Pixels>) -> Option<(f32, f32)> {
        let Some(Open::Picture(p)) = &self.open else {
            return None;
        };
        let b = self.shown.get()?;
        let scale = f32::from(b.size.width) / p.w;
        let x = (f32::from(at.x - b.origin.x) / scale).clamp(0., p.w);
        let y = (f32::from(at.y - b.origin.y) / scale).clamp(0., p.h);
        Some((x, y))
    }

    /// The pad's scale (screen pixels per picture pixel) for a picture.
    fn fit(p: &Picture) -> f32 {
        (PAD_W / p.w).min(PAD_H / p.h).min(1.)
    }
}

/// A theme colour as the RGBA a stroke is burnt in.
fn rgba(c: gpui::Hsla) -> [u8; 4] {
    let r = c.to_rgb();
    [
        (r.r * 255.).round() as u8,
        (r.g * 255.).round() as u8,
        (r.b * 255.).round() as u8,
        255,
    ]
}

/// The pen's colours, five that tell apart on any picture: red, yellow,
/// blue, green, and ink.
fn palette(t: &Theme) -> [gpui::Hsla; 5] {
    [
        t.extra.danger,
        t.colors.marigold,
        t.colors.sky,
        t.extra.success,
        t.colors.text_strong,
    ]
}

impl Render for AttachmentEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let Some(open) = &self.open else {
            return div().into_any_element();
        };
        let (title, name, is_picture) = match open {
            Open::Text { name, .. } => ("Edit the pasted text", name.clone(), false),
            Open::Picture(p) => ("Draw on the picture", p.name.clone(), true),
        };
        let header = div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .child(
                div()
                    .text_size(t.text.subheading)
                    .font_family(t.font_display.clone())
                    .font_weight(t.display_weight)
                    .text_color(t.colors.text_strong)
                    .child(title),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(t.colors.text_muted)
                    .child(name),
            )
            .child(div().flex_1())
            .child(
                Button::icon(
                    "attachment-editor-close",
                    "icons/x.svg",
                    "Close without saving (Esc)",
                )
                .tooltip(true)
                .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            );
        let body: gpui::AnyElement = match open {
            Open::Text { .. } => {
                let lines = self.text.read(cx).value().lines().count();
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.xs)
                    .child(
                        div()
                            .rounded(t.radius.field)
                            .bg(t.colors.surface)
                            .border_1()
                            .border_color(t.extra.border_strong)
                            .p(t.space.sm)
                            .font_family(t.font_mono.clone())
                            .text_size(t.text.body_sm)
                            .child(super::hold_wheel(
                                div().child(Textarea::new(&self.text).appearance(false).h(px(PAD_H))),
                            )),
                    )
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(format!(
                                "{lines} {} · sent whole with your message; erase it all to remove it",
                                if lines == 1 { "line" } else { "lines" }
                            )),
                    )
                    .into_any_element()
            }
            Open::Picture(p) => {
                let scale = Self::fit(p);
                let (w, h) = (p.w * scale, p.h * scale);
                let colors = palette(&t);
                let mut tools = div().flex().items_center().gap(t.space.xs);
                for (i, c) in colors.iter().enumerate() {
                    let on = i == self.color;
                    tools = tools.child(
                        div()
                            .id(("pen-color", i))
                            .size(px(22.))
                            .rounded_full()
                            .border_2()
                            .border_color(if on {
                                t.colors.text_strong
                            } else {
                                t.colors.border
                            })
                            .cursor_pointer()
                            .child(
                                div()
                                    .size_full()
                                    .rounded_full()
                                    .border_2()
                                    .border_color(t.colors.surface)
                                    .bg(*c),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.color = i;
                                cx.notify();
                            })),
                    );
                }
                tools = tools.child(
                    div()
                        .w(px(1.))
                        .h(px(18.))
                        .mx(t.space.xs)
                        .bg(t.colors.border),
                );
                for (i, wpx) in WIDTHS.iter().enumerate() {
                    let on = i == self.width;
                    let hover = t.colors.hover;
                    tools = tools.child(
                        div()
                            .id(("pen-width", i))
                            .size(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.pill)
                            .when(on, |d| d.bg(t.colors.primary_tint))
                            .cursor_pointer()
                            .hover(move |d| d.bg(hover))
                            .child(div().size(px(wpx + 2.)).rounded_full().bg(t.colors.text))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.width = i;
                                cx.notify();
                            })),
                    );
                }
                tools = tools
                    .child(div().flex_1())
                    .child(
                        Button::ghost("pen-undo", "Undo")
                            .with_icon("icons/rotate-ccw.svg")
                            .disabled(self.strokes.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.strokes.pop();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::ghost("pen-clear", "Clear")
                            .disabled(self.strokes.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.strokes.clear();
                                cx.notify();
                            })),
                    );
                // The strokes, painted over the picture at the pad's scale.
                let mut all = self.strokes.clone();
                all.extend(self.live.clone());
                let shown = self.shown.clone();
                let ink = canvas(
                    move |bounds, _, _| shown.set(Some(bounds)),
                    move |bounds, _, window, _| {
                        let s = f32::from(bounds.size.width) / (w / scale).max(1.);
                        for st in &all {
                            let at = |p: (f32, f32)| {
                                point(bounds.origin.x + px(p.0 * s), bounds.origin.y + px(p.1 * s))
                            };
                            let mut b = PathBuilder::stroke(px((st.width * s).max(1.)));
                            let Some(first) = st.points.first() else {
                                continue;
                            };
                            b.move_to(at(*first));
                            if st.points.len() == 1 {
                                b.line_to(at((first.0 + 0.5, first.1 + 0.5)));
                            }
                            for p in &st.points[1..] {
                                b.line_to(at(*p));
                            }
                            if let Ok(path) = b.build() {
                                let c = st.color;
                                window.paint_path(
                                    path,
                                    gpui::rgb(u32::from_be_bytes([0, c[0], c[1], c[2]])),
                                );
                            }
                        }
                    },
                )
                .absolute()
                .inset_0();
                let pad = div()
                    .id("draw-pad")
                    .relative()
                    .w(px(w))
                    .h(px(h))
                    .cursor_crosshair()
                    .child(img(p.image.clone()).size_full().object_fit(ObjectFit::Fill))
                    .child(ink)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                            let Some(at) = this.on_picture(e.position) else {
                                return;
                            };
                            let color = rgba(palette(cx.theme())[this.color]);
                            // The width is drawn in the pad's pixels: in the picture's, it scales.
                            this.live = Some(Stroke {
                                color,
                                width: WIDTHS[this.width] / scale,
                                points: vec![at],
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
                        if e.pressed_button != Some(MouseButton::Left) || this.live.is_none() {
                            return;
                        }
                        if let Some(at) = this.on_picture(e.position) {
                            if let Some(s) = this.live.as_mut() {
                                s.points.push(at);
                            }
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if let Some(s) = this.live.take() {
                                this.strokes.push(s);
                            }
                            cx.notify();
                        }),
                    );
                div()
                    .flex()
                    .flex_col()
                    .gap(t.space.sm)
                    .child(tools)
                    .child(
                        div()
                            .w(px(PAD_W))
                            .h(px(PAD_H))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.field)
                            .bg(t.extra.inset)
                            .child(pad),
                    )
                    .into_any_element()
            }
        };
        let footer = div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .children(self.error.clone().map(|e| {
                div()
                    .text_size(t.text.caption)
                    .text_color(t.extra.danger)
                    .child(e)
            }))
            .child(div().flex_1())
            .child(
                Button::ghost("attachment-editor-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .child(
                Button::primary(
                    "attachment-editor-save",
                    if is_picture {
                        "Save the drawing"
                    } else {
                        "Save"
                    },
                )
                .loading(self.saving)
                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            );
        super::motion::backdrop(
            div()
                .id("attachment-editor-scrim")
                .track_focus(&self.focus)
                .key_context("AttachmentEditor")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                // Esc is an action, not a key, here: the text area's own
                // (`input::Escape`), else the workspace's Stop. Both close it,
                // and a running reply is never stopped by it.
                .capture_action(cx.listener(
                    |this, _: &gpui::component::input::Escape, window, cx| {
                        this.close(window, cx);
                        cx.stop_propagation();
                    },
                ))
                .capture_action(cx.listener(|this, _: &super::StopReply, window, cx| {
                    this.close(window, cx);
                    cx.stop_propagation();
                }))
                .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    let k = &ev.keystroke;
                    if k.key == "escape" {
                        this.close(window, cx);
                    } else if k.key == "z"
                        && k.modifiers.secondary()
                        && matches!(this.open, Some(Open::Picture(_)))
                    {
                        this.strokes.pop();
                        cx.notify();
                    } else {
                        return;
                    }
                    cx.stop_propagation();
                }))
                .child(super::motion::alert(
                    theme::popover(&t)
                        .id("attachment-editor")
                        .occlude()
                        .w(px(PAD_W + 48.))
                        .p(t.space.xl)
                        .flex()
                        .flex_col()
                        .gap(t.space.md)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(header)
                        .child(body)
                        .child(footer),
                    "attachment-editor",
                    t.colors.primary,
                    self.leaving,
                    &t,
                )),
            "attachment-editor",
            t.extra.scrim,
            self.leaving,
            &t,
        )
    }
}
