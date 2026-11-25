//! The attachment editor: the pencil on a tile in the composer opens it.
//! A pasted text opens as text to edit (all of it erased removes the card).
//! A picture opens as a small studio: a tool rail (pen, highlighter, arrow,
//! box, ellipse, text, redact, crop, pan), five colours, three line
//! weights, crop shapes, a quarter turn, zoom, Undo / Redo and Reset.
//! Every edit is kept as an op beside the untouched original (`draw.rs`),
//! so nothing loses quality and reopening carries on where you left off.
//! Esc closes it without saving; Ctrl+Z undoes, Ctrl+Shift+Z / Ctrl+Y redo.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui::{
    canvas, div, img, point, prelude::*, px, Bounds, Context, Entity, FocusHandle, Image,
    ImageFormat, IntoElement, KeyDownEvent, MouseButton, ObjectFit, PathBuilder, Pixels, Render,
    SharedString, Subscription, Window,
};

use super::button::Button;
use crate::attach::Kind;
use crate::draw::{EditOp, Stroke};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

/// The picture being edited.
struct Picture {
    id: u64,
    name: SharedString,
    /// The untouched original every edit is replayed over.
    original: Arc<[u8]>,
    /// The original's size, in its own pixels.
    ow: u32,
    oh: u32,
    /// The committed edits, rendered: what the pad shows.
    shown: Arc<Image>,
    /// The rendered picture's size, after crops and turns.
    w: f32,
    h: f32,
}

enum Open {
    Text { id: u64, name: SharedString },
    Picture(Picture),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Pen,
    Highlight,
    Arrow,
    Rect,
    Ellipse,
    Text,
    Redact,
    Crop,
    Pan,
}

/// The rail, in order: the tool, its icon, its name.
const TOOLS: [(Tool, &str, &str); 9] = [
    (Tool::Pen, "icons/pencil.svg", "Pen"),
    (Tool::Highlight, "icons/pen-tool.svg", "Highlighter"),
    (Tool::Arrow, "icons/arrow-up-right.svg", "Arrow"),
    (Tool::Rect, "icons/square.svg", "Box"),
    (Tool::Ellipse, "icons/circle.svg", "Ellipse"),
    (Tool::Text, "icons/type.svg", "Text"),
    (Tool::Redact, "icons/layout-grid.svg", "Redact"),
    (Tool::Crop, "icons/frame.svg", "Crop"),
    (Tool::Pan, "icons/hand.svg", "Pan"),
];

/// Crop shapes: a name and width / height (None: free).
const ASPECTS: [(&str, Option<f32>); 4] = [
    ("Free", None),
    ("1:1", Some(1.)),
    ("4:3", Some(4. / 3.)),
    ("16:9", Some(16. / 9.)),
];

/// What the pointer is drawing, in the rendered picture's pixels.
#[derive(Clone)]
enum Live {
    Points(Vec<(f32, f32)>),
    Drag((f32, f32), (f32, f32)),
    /// Panning: where the pointer was, in window pixels.
    Pan(gpui::Point<Pixels>),
}

/// The pad's area: the picture is fitted inside it.
const PAD_W: f32 = 720.;
const PAD_H: f32 = 420.;
/// Line weights, in the pad's pixels.
const WIDTHS: [f32; 3] = [2., 4., 8.];
/// Text cap heights, in the pad's pixels, for each weight.
const TEXT_SIZES: [f32; 3] = [14., 21., 28.];
const ZOOMS: [f32; 6] = [0.5, 0.75, 1., 1.5, 2., 3.];

pub struct AttachmentEditor {
    state: Entity<AppState>,
    focus: FocusHandle,
    open: Option<Open>,
    text: Entity<TextareaState>,
    /// The words for a text label being placed.
    label: Entity<InputState>,
    /// Where the label goes, once the picture is clicked with Text.
    label_at: Option<(f32, f32)>,
    ops: Vec<EditOp>,
    redo: Vec<EditOp>,
    live: Option<Live>,
    tool: Tool,
    color: usize,
    width: usize,
    aspect: usize,
    zoom: usize,
    pan: (f32, f32),
    /// Bumped on every render request, so a slow one never wins late.
    generation: u64,
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
        let label = cx.new(|cx| InputState::new(window, cx).placeholder("Label text, then Enter"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
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
                                if this.leaving && this.state.read(cx).editing_attachment.is_none()
                                {
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
            }),
            cx.subscribe_in(&label, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.place_label(window, cx);
                }
            }),
        ];
        Self {
            state,
            focus: cx.focus_handle(),
            open: None,
            text,
            label,
            label_at: None,
            ops: vec![],
            redo: vec![],
            live: None,
            tool: Tool::Pen,
            color: 0,
            width: 1,
            aspect: 0,
            zoom: 2,
            pan: (0., 0.),
            generation: 0,
            shown: Rc::new(Cell::new(None)),
            saving: false,
            error: None,
            leaving: false,
            _subs: subs,
        }
    }

    fn load(&mut self, id: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        self.ops.clear();
        self.redo.clear();
        self.live = None;
        self.label_at = None;
        self.error = None;
        self.saving = false;
        self.zoom = 2;
        self.pan = (0., 0.);
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
            Some(a) => match (a.kind, a.original.clone().or(a.bytes.clone())) {
                (Kind::Image(_), Some(original)) => {
                    let (ow, oh) = image::load_from_memory(&original)
                        .map(|i| (i.width(), i.height()))
                        .unwrap_or((1, 1));
                    // The pad shows the committed edits; until they render,
                    // the picture as attached (already the edited one).
                    let bytes = a.bytes.clone().unwrap_or_else(|| original.clone());
                    let format = match image::guess_format(&bytes) {
                        Ok(image::ImageFormat::Jpeg) => ImageFormat::Jpeg,
                        Ok(image::ImageFormat::Gif) => ImageFormat::Gif,
                        Ok(image::ImageFormat::WebP) => ImageFormat::Webp,
                        _ => ImageFormat::Png,
                    };
                    let (w, h) = crate::draw::size_after(ow, oh, &a.edits);
                    self.ops = a.edits.clone();
                    window.focus(&self.focus, cx);
                    Some(Open::Picture(Picture {
                        id: a.id,
                        name: a.name.clone(),
                        original,
                        ow,
                        oh,
                        shown: Arc::new(Image::from_bytes(format, bytes.to_vec())),
                        w: w as f32,
                        h: h as f32,
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
            Some(Open::Picture(p)) => {
                let (id, original, ops) = (p.id, p.original.clone(), self.ops.clone());
                self.saving = true;
                cx.notify();
                cx.spawn_in(window, async move |this, cx| {
                    let rendered = cx
                        .background_spawn(async move {
                            if ops.is_empty() {
                                Ok((Vec::new(), ops))
                            } else {
                                crate::draw::render_png(&original, &ops).map(|png| (png, ops))
                            }
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match rendered {
                        Ok((png, ops)) => {
                            this.state
                                .update(cx, |s, cx| s.set_attachment_edits(id, png, ops, cx));
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

    /// Commit an edit: it joins the list, Redo forgets, the pad re-renders.
    fn push(&mut self, op: EditOp, cx: &mut Context<Self>) {
        self.ops.push(op);
        self.redo.clear();
        self.rerender(cx);
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(op) = self.ops.pop() {
            self.redo.push(op);
            self.rerender(cx);
        }
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        if let Some(op) = self.redo.pop() {
            self.ops.push(op);
            self.rerender(cx);
        }
    }

    /// Back to the picture as first attached. Undo brings the edits back.
    fn reset(&mut self, cx: &mut Context<Self>) {
        if self.ops.is_empty() {
            return;
        }
        let mut gone = std::mem::take(&mut self.ops);
        gone.reverse();
        self.redo = gone;
        self.rerender(cx);
    }

    /// Render the committed edits in the background and show them.
    fn rerender(&mut self, cx: &mut Context<Self>) {
        let Some(Open::Picture(p)) = &mut self.open else {
            return;
        };
        let (w, h) = crate::draw::size_after(p.ow, p.oh, &self.ops);
        if (w as f32, h as f32) != (p.w, p.h) {
            self.pan = (0., 0.);
        }
        (p.w, p.h) = (w as f32, h as f32);
        self.generation += 1;
        let (gen, original, ops) = (self.generation, p.original.clone(), self.ops.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let png = cx
                .background_spawn(async move { crate::draw::render_png(&original, &ops) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != gen {
                    return;
                }
                match png {
                    Ok(png) => {
                        if let Some(Open::Picture(p)) = &mut this.open {
                            p.shown = Arc::new(Image::from_bytes(ImageFormat::Png, png));
                        }
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn place_label(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let words = self.label.read(cx).value().to_string();
        let Some(at) = self.label_at.take() else {
            return;
        };
        if !words.trim().is_empty() {
            let color = rgba(palette(cx.theme())[self.color]);
            let size = TEXT_SIZES[self.width] / self.scale();
            self.push(
                EditOp::Text {
                    at,
                    text: words,
                    color,
                    size,
                },
                cx,
            );
        }
        self.label.update(cx, |i, cx| i.set_value("", window, cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The pointer's place on the picture, in the rendered picture's pixels.
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

    /// Screen pixels per picture pixel: fitted to the pad, then zoomed.
    fn scale(&self) -> f32 {
        match &self.open {
            Some(Open::Picture(p)) => {
                (PAD_W / p.w).min(PAD_H / p.h).min(1.) * ZOOMS[self.zoom]
            }
            _ => 1.,
        }
    }

    /// A drag's far corner, held to the crop shape when cropping.
    fn constrain(&self, a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
        let Some(ratio) = ASPECTS[self.aspect].1.filter(|_| self.tool == Tool::Crop) else {
            return b;
        };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let w = dx.abs().max(dy.abs() * ratio);
        let h = w / ratio;
        (a.0 + w.copysign(dx), a.1 + h.copysign(dy))
    }

    fn pointer_down(
        &mut self,
        e: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tool == Tool::Pan || e.button == MouseButton::Middle {
            self.live = Some(Live::Pan(e.position));
            return;
        }
        let Some(at) = self.on_picture(e.position) else {
            return;
        };
        self.live = match self.tool {
            Tool::Pen | Tool::Highlight => Some(Live::Points(vec![at])),
            Tool::Text => {
                self.label_at = Some(at);
                window.focus(&gpui::Focusable::focus_handle(self.label.read(cx), cx), cx);
                None
            }
            _ => Some(Live::Drag(at, at)),
        };
        cx.notify();
    }

    fn pointer_move(&mut self, e: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if e.pressed_button.is_none() {
            return;
        }
        let at = self.on_picture(e.position);
        match &mut self.live {
            Some(Live::Pan(last)) => {
                let d = e.position - *last;
                *last = e.position;
                self.pan.0 += f32::from(d.x);
                self.pan.1 += f32::from(d.y);
            }
            Some(Live::Points(pts)) => pts.extend(at),
            Some(Live::Drag(a, _)) => {
                let a = *a;
                if let Some(b) = at {
                    let b = self.constrain(a, b);
                    self.live = Some(Live::Drag(a, b));
                }
            }
            None => return,
        }
        cx.notify();
    }

    fn pointer_up(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.take() else {
            return;
        };
        let color = rgba(palette(cx.theme())[self.color]);
        let width = WIDTHS[self.width] / self.scale();
        let op = match (self.tool, live) {
            (Tool::Pen, Live::Points(points)) => Some(EditOp::Pen(Stroke {
                color,
                width,
                points,
            })),
            (Tool::Highlight, Live::Points(points)) => Some(EditOp::Highlight(Stroke {
                color,
                width: width * 3.,
                points,
            })),
            (tool, Live::Drag(a, b)) if (a.0 - b.0).abs() + (a.1 - b.1).abs() >= 2. => match tool {
                Tool::Arrow => Some(EditOp::Arrow {
                    from: a,
                    to: b,
                    color,
                    width,
                }),
                Tool::Rect => Some(EditOp::Rect { a, b, color, width }),
                Tool::Ellipse => Some(EditOp::Ellipse { a, b, color, width }),
                Tool::Redact => Some(EditOp::Redact { a, b }),
                Tool::Crop => Some(EditOp::Crop { a, b }),
                _ => None,
            },
            _ => None,
        };
        match op {
            Some(op) => self.push(op, cx),
            None => cx.notify(),
        }
    }
}

/// A theme colour as the RGBA an edit is drawn in.
fn rgba(c: gpui::Hsla) -> [u8; 4] {
    let r = c.to_rgb();
    [
        (r.r * 255.).round() as u8,
        (r.g * 255.).round() as u8,
        (r.b * 255.).round() as u8,
        255,
    ]
}

/// Seven crisp annotation colours (Coral, Marigold, Sky, Sage, Burgundy,
/// White, Ink) so marks read clearly over dark and light screenshots alike.
fn palette(t: &Theme) -> [gpui::Hsla; 7] {
    [
        t.extra.danger,
        t.colors.marigold,
        t.colors.sky,
        t.extra.success,
        t.extra.mark_top,
        gpui::rgb(0xffffff).into(),
        gpui::rgb(0x1e1c1a).into(),
    ]
}

fn color_of(c: [u8; 4], alpha: f32) -> gpui::Rgba {
    gpui::Rgba {
        r: f32::from(c[0]) / 255.,
        g: f32::from(c[1]) / 255.,
        b: f32::from(c[2]) / 255.,
        a: alpha,
    }
}

/// A thin vertical rule between groups in a tool row.
fn rule(t: &Theme) -> gpui::Div {
    div().w(px(1.)).h(px(18.)).mx(t.space.xs).bg(t.colors.border)
}

impl AttachmentEditor {
    /// The left rail: one square button per tool, the chosen one washed.
    fn rail(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut rail = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(4.))
            .rounded(t.radius.field)
            .bg(t.extra.inset);
        for (i, &(tool, path, name)) in TOOLS.iter().enumerate() {
            let on = tool == self.tool;
            let hover = t.colors.hover;
            rail = rail.child(
                div()
                    .id(("edit-tool", i))
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .when(on, |d| d.bg(t.colors.primary_tint))
                    .when(!on, move |d| d.hover(move |d| d.bg(hover)))
                    .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(name).build(w, cx))
                    .child(super::icon(
                        path,
                        t.size.icon_sm,
                        if on { t.colors.primary } else { t.colors.text_muted },
                    ))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.tool = tool;
                        this.label_at = None;
                        window.focus(&this.focus, cx);
                        cx.notify();
                    })),
            );
        }
        rail
    }

    /// The row above the pad: colours, weights (or crop shapes), then turn,
    /// zoom, Undo / Redo, Reset.
    fn toolbar(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut row = div().flex().items_center().gap(t.space.xs);
        let cropping = self.tool == Tool::Crop;
        let colors_matter = !matches!(self.tool, Tool::Crop | Tool::Redact | Tool::Pan);
        if colors_matter {
            for (i, c) in palette(t).iter().enumerate() {
                let on = i == self.color;
                // Crisp 1px selection ring + subtle inner hairline so white/ink read cleanly.
                row = row.child(
                    div()
                        .id(("pen-color", i))
                        .size(px(24.))
                        .p(px(2.))
                        .rounded_full()
                        .border_1()
                        .border_color(if on {
                            t.colors.text_strong
                        } else {
                            gpui::transparent_black()
                        })
                        .cursor_pointer()
                        .child(
                            div()
                                .size_full()
                                .rounded_full()
                                .border_1()
                                .border_color(t.colors.border)
                                .bg(*c),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.color = i;
                            cx.notify();
                        })),
                );
            }
            row = row.child(rule(t));
            for (i, wpx) in WIDTHS.iter().enumerate() {
                let on = i == self.width;
                let hover = t.colors.hover;
                // A bar as thick as the line it draws.
                row = row.child(
                    div()
                        .id(("pen-width", i))
                        .w(px(30.))
                        .h(px(26.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.radius.button)
                        .when(on, |d| d.bg(t.colors.primary_tint))
                        .cursor_pointer()
                        .hover(move |d| d.bg(hover))
                        .child(div().w(px(16.)).h(px(*wpx)).rounded_full().bg(if on {
                            t.colors.primary
                        } else {
                            t.colors.text
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.width = i;
                            cx.notify();
                        })),
                );
            }
        }
        if cropping {
            for (i, (name, _)) in ASPECTS.iter().enumerate() {
                row = row.child(
                    Button::ghost(("crop-aspect", i), *name)
                        .compact()
                        .selected(i == self.aspect)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.aspect = i;
                            cx.notify();
                        })),
                );
            }
        }
        row.child(div().flex_1())
            .child(
                Button::icon("edit-rotate", "icons/rotate-ccw.svg", "Turn a quarter to the left")
                    .on_click(cx.listener(|this, _, _, cx| this.push(EditOp::Rotate, cx))),
            )
            .child(rule(t))
            .child(
                Button::icon("edit-zoom-out", "icons/zoom-out.svg", "Zoom out")
                    .disabled(self.zoom == 0)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.zoom = this.zoom.saturating_sub(1);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .min_w(px(40.))
                    .text_center()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child(format!("{}%", (ZOOMS[self.zoom] * 100.).round())),
            )
            .child(
                Button::icon("edit-zoom-in", "icons/zoom-in.svg", "Zoom in")
                    .disabled(self.zoom + 1 == ZOOMS.len())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.zoom = (this.zoom + 1).min(ZOOMS.len() - 1);
                        cx.notify();
                    })),
            )
            .child(rule(t))
            .child(
                Button::icon("edit-undo", "icons/undo-2.svg", "Undo")
                    .disabled(self.ops.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.undo(cx))),
            )
            .child(
                Button::icon("edit-redo", "icons/redo-2.svg", "Redo")
                    .disabled(self.redo.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.redo(cx))),
            )
            .child(
                Button::ghost("edit-reset", "Reset")
                    .compact()
                    .disabled(self.ops.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
            )
    }
}

impl Render for AttachmentEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let Some(open) = &self.open else {
            return div().into_any_element();
        };
        let (title, name, is_picture) = match open {
            Open::Text { name, .. } => ("Edit the pasted text", name.clone(), false),
            Open::Picture(p) => ("Edit the picture", p.name.clone(), true),
        };
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
                let scale = self.scale();
                let (w, h) = (p.w * scale, p.h * scale);
                let (left, top) = ((PAD_W - w) / 2. + self.pan.0, (PAD_H - h) / 2. + self.pan.1);
                let image = p.shown.clone();
                let toolbar = self.toolbar(&t, cx);
                let rail = self.rail(&t, cx);

                // The edit under the pointer, drawn over the picture.
                let live = self.live.clone();
                let tool = self.tool;
                let color = rgba(palette(&t)[self.color]);
                let line = WIDTHS[self.width];
                let marker = t.colors.primary;
                let label_at = self.label_at;
                let shown = self.shown.clone();
                let overlay = canvas(
                    move |bounds, _, _| shown.set(Some(bounds)),
                    move |bounds, _, window, _| {
                        let at = |p: (f32, f32)| {
                            point(bounds.origin.x + px(p.0 * scale), bounds.origin.y + px(p.1 * scale))
                        };
                        let stroke = |pts: &[(f32, f32)], width: f32, c: gpui::Rgba, window: &mut Window| {
                            let Some(first) = pts.first() else { return };
                            let mut b = PathBuilder::stroke(px(width.max(1.)));
                            b.move_to(at(*first));
                            if pts.len() == 1 {
                                b.line_to(at((first.0 + 0.5 / scale, first.1 + 0.5 / scale)));
                            }
                            for p in &pts[1..] {
                                b.line_to(at(*p));
                            }
                            if let Ok(path) = b.build() {
                                window.paint_path(path, c);
                            }
                        };
                        if let Some((x, y)) = label_at {
                            let c = gpui::Rgba::from(marker);
                            stroke(&[(x, y), (x, y + 18. / scale)], 2., c, window);
                        }
                        match &live {
                            Some(Live::Points(pts)) => match tool {
                                Tool::Highlight => stroke(
                                    pts,
                                    line * 3.,
                                    color_of(color, crate::draw::HIGHLIGHT_ALPHA),
                                    window,
                                ),
                                _ => stroke(pts, line, color_of(color, 1.), window),
                            },
                            Some(Live::Drag(a, b)) => {
                                let c = color_of(color, 1.);
                                let boxed = [*a, (b.0, a.1), *b, (a.0, b.1), *a];
                                match tool {
                                    Tool::Arrow => stroke(&[*a, *b], line, c, window),
                                    Tool::Rect => stroke(&boxed, line, c, window),
                                    Tool::Ellipse => {
                                        let (cx_, cy_) = ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
                                        let (rx, ry) = ((a.0 - b.0).abs() / 2., (a.1 - b.1).abs() / 2.);
                                        let pts: Vec<_> = (0..=96)
                                            .map(|i| {
                                                let k = i as f32 / 96. * std::f32::consts::TAU;
                                                (cx_ + rx * k.cos(), cy_ + ry * k.sin())
                                            })
                                            .collect();
                                        stroke(&pts, line, c, window);
                                    }
                                    _ => stroke(&boxed, 1.5, gpui::Rgba::from(marker), window),
                                }
                            }
                            _ => {}
                        }
                    },
                )
                .absolute()
                .inset_0();
                let cursor_pan = self.tool == Tool::Pan;
                let pad = div()
                    .id("draw-pad")
                    .relative()
                    .w(px(PAD_W))
                    .h(px(PAD_H))
                    .overflow_hidden()
                    .rounded(t.radius.field)
                    .bg(t.extra.inset)
                    .when(cursor_pan, |d| d.cursor_grab())
                    .when(!cursor_pan, |d| d.cursor_crosshair())
                    .child(
                        div()
                            .absolute()
                            .left(px(left))
                            .top(px(top))
                            .w(px(w))
                            .h(px(h))
                            .shadow_sm()
                            .child(img(image).size_full().object_fit(ObjectFit::Fill))
                            .child(overlay),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                            this.pointer_down(e, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, e: &gpui::MouseDownEvent, window, cx| {
                            this.pointer_down(e, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, _, cx| {
                        this.pointer_move(e, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.pointer_up(cx)),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, _, _, cx| this.pointer_up(cx)),
                    )
                    .on_scroll_wheel(cx.listener(|this, ev: &gpui::ScrollWheelEvent, window, cx| {
                        let dy = f32::from(ev.delta.pixel_delta(window.rem_size()).y);
                        if dy > 2. {
                            this.zoom = (this.zoom + 1).min(ZOOMS.len() - 1);
                            cx.notify();
                        } else if dy < -2. {
                            this.zoom = this.zoom.saturating_sub(1);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }));
                let label_row = self.label_at.map(|_| {
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(
                            div()
                                .flex_1()
                                .px(t.space.sm)
                                .rounded(t.radius.field)
                                .border_1()
                                .border_color(t.extra.border_strong)
                                .bg(t.colors.surface)
                                .child(Input::new(&self.label).appearance(false)),
                        )
                        .child(
                            Button::secondary("edit-label-add", "Add label")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.place_label(window, cx)
                                })),
                        )
                });
                let hint = match self.tool {
                    Tool::Text if self.label_at.is_none() => "Click where the label should go",
                    Tool::Crop => "Drag the area to keep",
                    Tool::Redact => "Drag over what to hide",
                    Tool::Pan => "Drag to move around the picture",
                    _ => "",
                };
                div()
                    .flex()
                    .gap(t.space.sm)
                    .child(rail)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(t.space.sm)
                            .child(toolbar)
                            .child(pad)
                            .children(label_row)
                            .child(
                                div()
                                    .h(px(16.))
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_muted)
                                    .child(format!(
                                        "{hint}{}{} × {} px",
                                        if hint.is_empty() { "" } else { " · " },
                                        p.w as u32,
                                        p.h as u32
                                    )),
                            ),
                    )
                    .into_any_element()
            }
        };
        let error = self.error.clone().map(|e| {
            div()
                .text_size(t.text.caption)
                .text_color(t.extra.danger)
                .child(e)
        });
        let footer = vec![
            Button::ghost("attachment-editor-cancel", "Cancel")
                .on_click(cx.listener(|this, _, window, cx| this.close(window, cx)))
                .into_any_element(),
            Button::accent("attachment-editor-save", if is_picture { "Save picture" } else { "Save" })
                .loading(self.saving)
                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx)))
                .into_any_element(),
        ];
        let width = if is_picture { PAD_W + 48. + 48. } else { PAD_W + 48. };
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
                        if this.label_at.take().is_some() {
                            window.focus(&this.focus, cx);
                            cx.notify();
                        } else {
                            this.close(window, cx);
                        }
                        cx.stop_propagation();
                    },
                ))
                .capture_action(cx.listener(|this, _: &super::StopReply, window, cx| {
                    this.close(window, cx);
                    cx.stop_propagation();
                }))
                .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    let k = &ev.keystroke;
                    let picture = matches!(this.open, Some(Open::Picture(_)));
                    if k.key == "escape" {
                        if this.label_at.take().is_some() {
                            window.focus(&this.focus, cx);
                            cx.notify();
                        } else {
                            this.close(window, cx);
                        }
                    } else if picture
                        && k.modifiers.secondary()
                        && (k.key == "y" || (k.key == "z" && k.modifiers.shift))
                    {
                        this.redo(cx);
                    } else if picture && k.key == "z" && k.modifiers.secondary() {
                        this.undo(cx);
                    } else if picture
                        && this.label_at.is_none()
                        && !k.modifiers.control
                        && !k.modifiers.alt
                        && !k.modifiers.platform
                    {
                        let next = match k.key.as_str() {
                            "p" => Some(Tool::Pen),
                            "h" => Some(Tool::Highlight),
                            "a" => Some(Tool::Arrow),
                            "r" => Some(Tool::Rect),
                            "c" | "e" => Some(Tool::Ellipse),
                            "t" => Some(Tool::Text),
                            "b" => Some(Tool::Redact),
                            "x" => Some(Tool::Crop),
                            "v" => Some(Tool::Pan),
                            _ => None,
                        };
                        if let Some(tool) = next {
                            this.tool = tool;
                            cx.notify();
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                    cx.stop_propagation();
                }))
                .child(super::motion::alert(
                    super::panel_chrome::window(
                        &t,
                        theme::popover(&t)
                            .id("attachment-editor")
                            .occlude()
                            .w(px(width))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                        "attachment-editor",
                        title,
                        None,
                        cx.listener(|this, _, window, cx| this.close(window, cx)),
                        div()
                            .flex()
                            .flex_col()
                            .gap(t.space.md)
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_color(t.colors.text_muted)
                                    .child(name),
                            )
                            .child(body)
                            .children(error),
                        footer,
                    ),
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
