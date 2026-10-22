//! The browser in the side panel's Browser tab: a real Chromium page, live
//! (`web_view.rs`). Its tabs across the top, then back, forward, reload, the
//! address (type a URL or words to search) and "open in your browser"; a
//! thin bar while a page loads; then the page itself, which you click,
//! scroll and type into as in any browser. When Forge drives it (the
//! browser tools, `browser_mcp.rs`), a badge says so and you watch it work.
//! When Forge writes a page, the panel follows it and shows it forming
//! (`design::live`); a badge says which, with Stop following, or Show once
//! you have taken the browser yourself.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    canvas, div, img, percentage, prelude::*, px, relative, Animation, AnimationExt, Bounds,
    Context, CursorStyle, Entity, FocusHandle, Focusable, FontWeight, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Pixels, Render, ScrollWheelEvent,
    Subscription, Transformation, Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::{ActiveTheme, Theme};
use crate::web_view::{Cmd, PageDialog};

/// A GPUI mouse button as the page gets it.
fn page_button(b: MouseButton) -> Option<crate::web_view::MouseButton> {
    use crate::web_view::MouseButton as B;
    match b {
        MouseButton::Left => Some(B::Left),
        MouseButton::Right => Some(B::Right),
        MouseButton::Middle => Some(B::Middle),
        _ => None,
    }
}

/// The pointer for a CSS `cursor` value.
fn cursor_for(css: &str) -> CursorStyle {
    match css {
        "pointer" => CursorStyle::PointingHand,
        "text" => CursorStyle::IBeam,
        "vertical-text" => CursorStyle::IBeamCursorForVerticalLayout,
        "crosshair" => CursorStyle::Crosshair,
        "grab" => CursorStyle::OpenHand,
        "grabbing" | "move" => CursorStyle::ClosedHand,
        "not-allowed" | "no-drop" => CursorStyle::OperationNotAllowed,
        "ew-resize" | "e-resize" | "w-resize" => CursorStyle::ResizeLeftRight,
        "ns-resize" | "n-resize" | "s-resize" => CursorStyle::ResizeUpDown,
        "col-resize" => CursorStyle::ResizeColumn,
        "row-resize" => CursorStyle::ResizeRow,
        "nwse-resize" => CursorStyle::ResizeUpLeftDownRight,
        "nesw-resize" => CursorStyle::ResizeUpRightDownLeft,
        _ => CursorStyle::Arrow,
    }
}

pub struct BrowserView {
    state: Entity<AppState>,
    address: Entity<InputState>,
    /// The page has the keyboard: keys go to the page.
    focus: FocusHandle,
    /// Where the page is drawn, from the last frame, to map the mouse.
    page_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The size last sent to the browser: CSS width, height, scale.
    sent_size: Rc<Cell<(u32, u32, f32)>>,
    /// The address shown, so typing in it isn't overwritten as pages change.
    shown_url: String,
    /// The page's zoom as last drawn: the panel shows a CSS pixel this large.
    zoom: f64,
    /// The design bar's open menu.
    design_menu: Option<super::design_bar::Menu>,
    /// The comment tool's box, over the part picked.
    comment_box: Entity<super::design_comment::DesignCommentBox>,
    /// A comment was open when the app last changed.
    picked: bool,
    /// A mark being drawn with an Insert tool, in the page's CSS pixels.
    drawing: Option<crate::design::marks::Mark>,
    /// A label or note being typed, and where on the page it goes.
    typing: Option<(crate::design::marks::Shape, (f64, f64))>,
    mark_input: Entity<InputState>,
    /// The Library's search box.
    library_filter: Entity<InputState>,
    /// The Hand's last place, while it drags the page.
    hand_at: Option<gpui::Point<Pixels>>,
    _subs: Vec<Subscription>,
}

impl BrowserView {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search or enter an address"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                // A comment closed: the keyboard is the page's again, so
                // Esc puts the comment tool down.
                let picked = state.read(cx).design_pick.is_some();
                if this.picked && !picked {
                    window.focus(&this.focus, cx);
                }
                this.picked = picked;
                // The address follows the page, unless you are typing in it.
                let url = state.read(cx).web_page.url.clone();
                if url != this.shown_url {
                    this.shown_url = url.clone();
                    let editing = this.address.read(cx).focus_handle(cx).is_focused(window);
                    if !editing {
                        let shown = if url == "about:blank" {
                            String::new()
                        } else {
                            url
                        };
                        this.address
                            .update(cx, |i, cx| i.set_value(shown, window, cx));
                    }
                }
                cx.notify();
            }),
            cx.subscribe_in(
                &address,
                window,
                |this, input, ev: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        let typed = input.read(cx).value().to_string();
                        if !typed.trim().is_empty() {
                            let url = crate::web_view::address(&typed);
                            this.state
                                .update(cx, |s, cx| s.web_send(Cmd::Navigate(url), cx));
                            window.focus(&this.focus, cx);
                        }
                    }
                },
            ),
        ];
        let comment_box =
            cx.new(|cx| super::design_comment::DesignCommentBox::new(state.clone(), window, cx));
        let mark_input = cx.new(|cx| InputState::new(window, cx).placeholder("Type, then Enter"));
        let library_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search the design systems"));
        let mut subs = subs;
        subs.push(cx.subscribe(&library_filter, |_, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                cx.notify();
            }
        }));
        subs.push(cx.subscribe_in(&mark_input, window, |this, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.place_words(window, cx);
            }
        }));
        Self {
            state,
            address,
            focus: cx.focus_handle(),
            page_bounds: Rc::new(Cell::new(Bounds::default())),
            sent_size: Rc::new(Cell::new((0, 0, 0.))),
            shown_url: String::new(),
            zoom: 1.0,
            design_menu: None,
            comment_box,
            picked: false,
            drawing: None,
            typing: None,
            mark_input,
            library_filter,
            hand_at: None,
            _subs: subs,
        }
    }

    /// The comment tool is on over a design page: the mouse picks parts
    /// of the page instead of clicking them.
    fn selecting(&self, cx: &gpui::App) -> bool {
        let s = self.state.read(cx);
        s.design_select && !s.presenting && s.design_on_screen().is_some()
    }

    /// The Insert tool in hand, over a design page.
    fn tool(&self, cx: &gpui::App) -> Option<crate::state::InsertTool> {
        let s = self.state.read(cx);
        s.design_tool
            .filter(|_| !s.presenting && s.design_on_screen().is_some())
    }

    /// A point on the panel as a point of the page itself (its CSS pixels
    /// from its top left, not the window's).
    fn page_point(&self, p: gpui::Point<Pixels>, cx: &gpui::App) -> Option<(f64, f64)> {
        let (sx, sy) = self.state.read(cx).web_page.frame_info.scroll;
        self.to_page(p)
            .map(|(x, y)| (x + f64::from(sx), y + f64::from(sy)))
    }

    /// An Insert tool's press on the page: the Hand takes hold, a shape or
    /// stroke starts, a Pen point is set, a label or note opens for typing.
    fn tool_down(
        &mut self,
        tool: crate::state::InsertTool,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::design::marks::{Mark, Shape};
        let Some(p) = self.page_point(ev.position, cx) else { return };
        match tool {
            crate::state::InsertTool::Hand => self.hand_at = Some(ev.position),
            crate::state::InsertTool::Mark(shape) if shape.typed() => {
                self.typing = Some((shape, p));
                self.mark_input.update(cx, |i, cx| i.set_value("", window, cx));
                cx.on_next_frame(window, |this, window, cx| {
                    this.mark_input.update(cx, |i, cx| i.focus(window, cx));
                });
            }
            crate::state::InsertTool::Mark(Shape::Pen) => match &mut self.drawing {
                Some(m) if m.shape == Shape::Pen => {
                    if ev.click_count >= 2 {
                        self.finish_pen(cx);
                    } else {
                        m.points.push(p);
                    }
                }
                _ => {
                    self.drawing = Some(Mark {
                        anchor: None,
                        shape: Shape::Pen,
                        points: vec![p, p],
                        text: String::new(),
                    })
                }
            },
            crate::state::InsertTool::Mark(shape) => {
                self.drawing = Some(Mark {
                    anchor: None,
                    shape,
                    points: if shape == Shape::Pencil { vec![p] } else { vec![p, p] },
                    text: String::new(),
                })
            }
        }
        cx.notify();
    }

    /// The Pen's path is done: its points, less the one following the mouse.
    fn finish_pen(&mut self, cx: &mut Context<Self>) {
        if let Some(mut m) = self.drawing.take() {
            m.points.pop();
            m.points.dedup();
            if m.points.len() >= 2 {
                self.state.update(cx, |s, cx| s.design_add_mark(m, cx));
            }
        }
        cx.notify();
    }

    /// The label or note typed goes on the page.
    fn place_words(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((shape, at)) = self.typing.take() else { return };
        let text = self.mark_input.read(cx).value().trim().to_string();
        self.mark_input.update(cx, |i, cx| i.set_value("", window, cx));
        if !text.is_empty() {
            let mark = crate::design::marks::Mark {
                anchor: None,
                shape,
                points: vec![at],
                text,
            };
            self.state.update(cx, |s, cx| s.design_add_mark(mark, cx));
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The design bar's open menu.
    pub fn design_menu(&self) -> Option<super::design_bar::Menu> {
        self.design_menu
    }

    /// Open one of the design bar's menus, or close it.
    pub fn set_menu(&mut self, menu: Option<super::design_bar::Menu>, cx: &mut Context<Self>) {
        self.design_menu = menu;
        cx.notify();
    }

    fn send(&self, cmd: Cmd, cx: &mut Context<Self>) {
        // Taking the browser somewhere yourself stops it following Forge's design.
        let yours = matches!(
            cmd,
            Cmd::Show(_) | Cmd::NewTab(_) | Cmd::Navigate(_) | Cmd::Back | Cmd::Forward | Cmd::Close(_)
        );
        self.state.update(cx, |s, cx| {
            if yours && s.design_follow && s.designing().is_some() {
                s.design_set_follow(false, cx);
            }
            s.web_send(cmd, cx)
        });
    }

    /// A point in the window as a point on the page (CSS pixels): the
    /// page is laid out at the panel's size (over the zoom), so it is the
    /// offset in it, over the zoom.
    fn to_page(&self, p: gpui::Point<Pixels>) -> Option<(f64, f64)> {
        let b = self.page_bounds.get();
        b.contains(&p).then(|| {
            (
                f64::from(f32::from(p.x - b.origin.x)) / self.zoom,
                f64::from(f32::from(p.y - b.origin.y)) / self.zoom,
            )
        })
    }

    fn key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Keys are the page's only while it has the keyboard itself, not a
        // box over it (a comment being written).
        if !self.focus.is_focused(window) {
            return;
        }
        let k = &ev.keystroke;
        let m = k.modifiers;
        // Esc ends a presentation, closes a comment, or puts the comment
        // tool down, before the page gets it.
        if k.key == "escape" {
            let (presenting, picked) = {
                let s = self.state.read(cx);
                (s.presenting, s.design_pick.is_some())
            };
            if presenting {
                self.state.update(cx, |s, cx| s.set_presenting(false, cx));
                cx.stop_propagation();
                return;
            }
            if picked {
                self.state.update(cx, |s, cx| s.design_cancel_pick(cx));
                cx.stop_propagation();
                return;
            }
            // A Pen path ends; else the Insert tool goes down.
            if self.drawing.as_ref().is_some_and(|m| m.shape == crate::design::marks::Shape::Pen) {
                self.finish_pen(cx);
                cx.stop_propagation();
                return;
            }
            if self.tool(cx).is_some() {
                self.drawing = None;
                self.state.update(cx, |s, cx| s.design_set_tool(None, cx));
                cx.stop_propagation();
                return;
            }
            if self.selecting(cx) {
                self.state.update(cx, |s, cx| s.design_set_select(false, cx));
                cx.stop_propagation();
                return;
            }
        }
        if m.secondary() {
            match k.key.as_str() {
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        self.send(Cmd::Text(text), cx);
                    }
                }
                "l" => {
                    self.address.update(cx, |i, cx| i.focus(window, cx));
                }
                "c" => self.send(Cmd::Copy, cx),
                "a" => self.send(Cmd::SelectAll, cx),
                "r" => self.send(Cmd::Reload, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if k.key == "enter" && self.drawing.as_ref().is_some_and(|m| m.shape == crate::design::marks::Shape::Pen) {
            self.finish_pen(cx);
            cx.stop_propagation();
            return;
        }
        let named = matches!(
            k.key.as_str(),
            "enter"
                | "backspace"
                | "delete"
                | "tab"
                | "escape"
                | "left"
                | "right"
                | "up"
                | "down"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
        );
        if named {
            self.send(Cmd::Key(k.key.clone()), cx);
        } else if let Some(ch) = k.key_char.clone().filter(|c| !c.is_empty()) {
            self.send(Cmd::Text(ch), cx);
        } else {
            return;
        }
        cx.stop_propagation();
    }
}

fn bar_button(
    t: &Theme,
    id: &'static str,
    path: &'static str,
    tip: &'static str,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex_none()
        .size(t.size.control)
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.button)
        .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
        .child(icon(
            path,
            t.size.icon,
            if enabled {
                t.colors.text_muted
            } else {
                t.colors.text_subtle
            },
        ))
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
}

impl Render for BrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let dialog = self.state.read(cx).web_page.dialog.clone();
        let dialog = dialog.map(|d| dialog_card(&d, &t, cx));
        let s = self.state.read(cx);
        let page = &s.web_page;
        let running = s.web.is_some();
        let agent = s
            .chat()
            .is_some_and(|c| super::workbench::agent_browsing(&c.messages));
        // The page Forge is writing (true), or wrote while you weren't
        // following (false), and whether the panel follows it.
        let design = s
            .designing()
            .map(|l| (l.name(), true))
            .or_else(|| s.design_ready().map(|l| (l.name(), false)));
        let following = s.design_follow;
        let blank = page.url.is_empty() || page.url == "about:blank";
        let secure = page.url.starts_with("https://");
        let (tabs, active) = (page.tabs.clone(), page.active.clone());
        let (frame, loading, error) = (page.frame.clone(), page.loading, page.error.clone());
        let frame_info = page.frame_info;
        let zoom = s.web_zoom;
        self.zoom = zoom;
        let presenting = s.presenting;
        // A page this chat designed gets the design bar, and its comments.
        let here = s.design_comments_here();
        let design_bar = s.design_on_screen().filter(|_| !presenting).map(|page| {
            super::design_bar::Bar {
                page,
                pages: s.design_pages(),
                zoom,
                menu: self.design_menu,
                select: s.design_select,
                comments: here
                    .iter()
                    .map(|c| super::design_bar::Note {
                        n: c.n,
                        label: c.part.label(),
                        text: c.text.clone(),
                        sent: c.sent,
                        resolved: c.resolved,
                    })
                    .collect(),
                pins: s.design_pins,
                resolved: s.design_resolved,
                outline: s.design_outline,
                guides: s.design_guides,
                deck: s.web_page.deck,
                undo: s.design_can_step(-1),
                redo: s.design_can_step(1),
                tool: s.design_tool,
                artboard: s.design_artboard.map(|(name, _)| name),
                systems: s.design_systems.clone(),
                filter: self.library_filter.clone(),
                filter_text: self.library_filter.read(cx).value().to_string(),
            }
        });
        let tool_now = s.design_tool.filter(|_| design_bar.is_some());
        // The page's marks, and the one being drawn.
        let mut marks: Vec<crate::design::marks::Mark> = if design_bar.is_some() {
            s.design_marks_here()
        } else {
            Vec::new()
        };
        let marks_count = marks.len();
        marks.extend(self.drawing.clone().filter(|_| tool_now.is_some()));
        let marks_place = super::design_marks::Place {
            scroll: page.frame_info.scroll,
            zoom: zoom as f32,
        };
        let typing_at = self
            .typing
            .filter(|_| tool_now.is_some())
            .map(|(shape, p)| (shape, marks_place.at(p)));
        // The page's outline: its headings, the one at the top marked.
        let outline = (s.design_outline && design_bar.is_some()).then(|| {
            let top = f64::from(page.frame_info.scroll.1) + 80.0;
            let at = s.design_headings.iter().rposition(|h| h.y <= top);
            (s.design_headings.clone(), at)
        });
        let guides_on = s.design_guides;
        let selecting = s.design_select && design_bar.is_some();
        // Where a part of the page is in the panel: its CSS box, less the
        // page's scroll, times the zoom.
        let scroll = page.frame_info.scroll;
        let place = move |p: &crate::web_view::PagePart| {
            let z = zoom as f32;
            (
                (p.x as f32 - scroll.0) * z,
                (p.y as f32 - scroll.1) * z,
                p.w as f32 * z,
                p.h as f32 * z,
            )
        };
        let hover_box = s
            .design_hover
            .as_ref()
            .filter(|_| selecting && s.design_pick.is_none())
            .map(|p| (place(p), p.tag.clone()));
        let pick_box = s.design_pick.as_ref().filter(|_| selecting).map(&place);
        let drawing_tool = s.design_tool.is_some();
        let pins: Vec<_> = here
            .iter()
            .filter(|c| design_bar.is_some() && s.design_pins && (s.design_resolved || !c.resolved))
            .map(|c| (c.n, c.sent, c.text.clone(), place(&s.design_part_now(&c.part))))
            .collect();
        let room = self.page_bounds.get().size;
        let last_download = page.last_download.clone();
        let url = page.url.clone();
        let hover = t.colors.hover;

        // The tabs, as a browser has them: the one on screen raised.
        let tab_strip = div()
            .flex_none()
            .flex()
            .items_end()
            .gap(px(2.))
            .px(t.space.sm)
            .pt(t.space.xs)
            .bg(t.extra.inset)
            .children(tabs.iter().enumerate().map(|(i, tab)| {
                let on = tab.id == active;
                let (show, close) = (tab.id.clone(), tab.id.clone());
                let name = if tab.title.is_empty() || tab.title == "about:blank" {
                    if tab.url == "about:blank" {
                        "New tab".to_string()
                    } else {
                        tab.url.clone()
                    }
                } else {
                    tab.title.clone()
                };
                div()
                    .id(("web-tab", i))
                    .flex_1()
                    .min_w_0()
                    .max_w(px(200.))
                    .h(t.size.control + px(4.))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .pl(t.space.md)
                    .pr(t.space.xs)
                    .rounded_t(t.radius.button)
                    .cursor_pointer()
                    .when(on, |d| d.bg(t.colors.surface))
                    .when(!on, |d| d.hover(move |s| s.bg(hover)))
                    .child(icon(
                        "icons/globe.svg",
                        t.size.icon_xs,
                        t.colors.text_subtle,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.caption)
                            .text_color(if on {
                                t.colors.text_strong
                            } else {
                                t.colors.text_muted
                            })
                            .child(name),
                    )
                    .child(
                        div()
                            .id(("web-tab-close", i))
                            .flex_none()
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.small)
                            .hover(move |s| s.bg(hover))
                            .child(icon("icons/x.svg", t.size.icon_xs, t.colors.text_subtle))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.send(Cmd::Close(close.clone()), cx);
                            })),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.send(Cmd::Show(show.clone()), cx)),
                    )
            }))
            .child(
                bar_button(&t, "web-new-tab", "icons/plus.svg", "New tab", true)
                    .mb(px(2.))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.send(Cmd::NewTab("about:blank".into()), cx);
                        this.address.update(cx, |i, cx| {
                            i.set_value("", window, cx);
                            i.focus(window, cx);
                        });
                    })),
            );

        let toolbar = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(t.space.xs)
            .px(t.space.sm)
            .py(t.space.xs)
            .bg(t.colors.surface)
            .border_b_1()
            .border_color(t.colors.border)
            .child(
                bar_button(&t, "web-back", "icons/arrow-left.svg", "Back", running)
                    .on_click(cx.listener(|this, _, _, cx| this.send(Cmd::Back, cx))),
            )
            .child(
                bar_button(
                    &t,
                    "web-forward",
                    "icons/arrow-right.svg",
                    "Forward",
                    running,
                )
                .on_click(cx.listener(|this, _, _, cx| this.send(Cmd::Forward, cx))),
            )
            .child(
                bar_button(&t, "web-reload", "icons/rotate-ccw.svg", "Reload", running)
                    .on_click(cx.listener(|this, _, _, cx| this.send(Cmd::Reload, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(t.size.control + px(4.))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .px(t.space.md)
                    .rounded(t.radius.button)
                    .bg(t.extra.inset)
                    .child(icon(
                        if blank {
                            "icons/search.svg"
                        } else if secure {
                            "icons/shield-check.svg"
                        } else {
                            "icons/globe.svg"
                        },
                        t.size.icon_sm,
                        if secure {
                            t.extra.success
                        } else {
                            t.colors.text_subtle
                        },
                    ))
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.address)
                                .appearance(false)
                                .text_size(t.text.body_sm),
                        ),
                    ),
            )
            // Layout guides, while on: a click takes them off.
            .when(guides_on, |d| {
                d.child(
                    crate::theme::text_button(&t, "Guides")
                        .id("web-guides-off")
                        .flex_none()
                        .tooltip(|window, cx| {
                            gpui::component::tooltip::Tooltip::new("Take the layout guides off").build(window, cx)
                        })
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(super::LayoutGuides), cx)),
                )
            })
            // The zoom, when it isn't 100%: a click puts it back.
            .when((zoom - 1.0).abs() > 0.001, |d| {
                d.child(
                    crate::theme::text_button(&t, super::design_bar::percent(zoom))
                        .id("web-zoom-reset")
                        .flex_none()
                        .tooltip(|window, cx| {
                            gpui::component::tooltip::Tooltip::new("Back to 100%").build(window, cx)
                        })
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(super::ZoomReset), cx)),
                )
            })
            .child(
                bar_button(
                    &t,
                    "web-open",
                    "icons/external-link.svg",
                    "Open in your browser",
                    !blank,
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    if !url.is_empty() && url != "about:blank" {
                        cx.open_url(&url);
                    }
                })),
            );

        // A thin bar sweeping under the toolbar while the page loads.
        let progress = div()
            .flex_none()
            .h(px(2.))
            .w_full()
            .relative()
            .overflow_hidden()
            .when(loading && running, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .h_full()
                        .w(relative(0.3))
                        .rounded(t.radius.line)
                        .bg(t.colors.primary)
                        .with_animation(
                            "web-loading",
                            Animation::new(Duration::from_millis(1100)).repeat(),
                            |d, k| d.left(relative(-0.3 + 1.3 * k)),
                        ),
                )
            });

        // The page, laid out at the panel's own size (see `canvas` below).
        let bounds = self.page_bounds.clone();
        let sent = self.sent_size.clone();
        let state = self.state.clone();
        let viewport = div()
            .id("web-page")
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .bg(t.colors.surface)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| this.key(ev, window, cx)))
            // The page's picture at its true size, from the top left, where
            // the mouse mapping (`to_page`) expects it: as large as the area
            // the browser says it shows, which may be more than the page.
            .children(frame.clone().map(|f| {
                let (w, h) = match frame_info.area {
                    (w, h) if w > 0.0 && h > 0.0 => (w * zoom as f32, h * zoom as f32),
                    _ => {
                        let size = f.size(0);
                        let scale = window.scale_factor();
                        (size.width.0 as f32 / scale, size.height.0 as f32 / scale)
                    }
                };
                img(f)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(w))
                    .h(px(h))
                    .object_fit(ObjectFit::Fill)
            }))
            .child(
                // Measures the page's room each frame; a new size is sent
                // to the browser, which lays the page out again to fit.
                canvas(
                    move |b, window, cx| {
                        bounds.set(b);
                        let size = (
                            f32::from(b.size.width).round() as u32,
                            f32::from(b.size.height).round() as u32,
                            window.scale_factor(),
                        );
                        if size.0 > 0 && size.1 > 0 && sent.get() != size {
                            sent.set(size);
                            cx.defer(move |cx| {
                                state.update(cx, |s, cx| {
                                    s.web_send(
                                        Cmd::Resize { width: size.0, height: size.1, scale: f64::from(size.2) },
                                        cx,
                                    )
                                })
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .cursor(match tool_now {
                Some(crate::state::InsertTool::Hand) if self.hand_at.is_some() => CursorStyle::ClosedHand,
                Some(crate::state::InsertTool::Hand) => CursorStyle::OpenHand,
                Some(crate::state::InsertTool::Mark(m)) if m.typed() => CursorStyle::IBeam,
                Some(_) => CursorStyle::Crosshair,
                None if selecting => CursorStyle::Crosshair,
                None => cursor_for(&page.cursor),
            })
            // Down and up apart, so a drag selects text or moves a slider.
            // With the comment tool, a click picks the part instead.
            .on_any_mouse_down(cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                window.focus(&this.focus, cx);
                let Some((x, y)) = this.to_page(ev.position) else { return };
                if let Some(tool) = this.tool(cx) {
                    if ev.button == MouseButton::Left {
                        this.tool_down(tool, ev, window, cx);
                    }
                    return;
                }
                if this.selecting(cx) {
                    if ev.button == MouseButton::Left {
                        this.send(Cmd::Inspect { x, y, pick: true }, cx);
                    }
                } else if let Some(button) = page_button(ev.button) {
                    this.send(Cmd::Press { x, y, button, count: ev.click_count.max(1) as u32 }, cx);
                }
            }))
            .capture_any_mouse_up(cx.listener(|this, ev: &MouseUpEvent, _, cx| {
                if this.tool(cx).is_some() {
                    this.hand_at = None;
                    // A dragged shape or a stroke is done (a Pen path goes on).
                    if let Some(m) = this
                        .drawing
                        .take_if(|m| m.shape != crate::design::marks::Shape::Pen)
                    {
                        this.state.update(cx, |s, cx| s.design_add_mark(m, cx));
                    }
                    cx.notify();
                    return;
                }
                if this.selecting(cx) {
                    return;
                }
                if let (Some((x, y)), Some(button)) = (this.to_page(ev.position), page_button(ev.button)) {
                    this.send(Cmd::Release { x, y, button, count: ev.click_count.max(1) as u32 }, cx);
                }
            }))
            // Let go over a pin, or outside the page: a shape being drawn
            // is done all the same (else it would hang there, unsaved).
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| {
                if this.tool(cx).is_some() {
                    this.hand_at = None;
                    if let Some(m) = this
                        .drawing
                        .take_if(|m| m.shape != crate::design::marks::Shape::Pen)
                    {
                        this.state.update(cx, |s, cx| s.design_add_mark(m, cx));
                    }
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if let Some(tool) = this.tool(cx) {
                    use crate::design::marks::Shape;
                    let held = ev.pressed_button == Some(MouseButton::Left);
                    if tool == crate::state::InsertTool::Hand {
                        // The page follows the hand.
                        if let (Some(last), true, Some((x, y))) = (this.hand_at, held, this.to_page(ev.position)) {
                            let d = ev.position - last;
                            this.hand_at = Some(ev.position);
                            let z = this.zoom;
                            this.send(
                                Cmd::Wheel {
                                    x,
                                    y,
                                    dx: -f64::from(f32::from(d.x)) / z,
                                    dy: -f64::from(f32::from(d.y)) / z,
                                },
                                cx,
                            );
                        }
                        return;
                    }
                    let Some(p) = this.page_point(ev.position, cx) else { return };
                    let step = 2.0 / this.zoom;
                    if let Some(m) = &mut this.drawing {
                        match m.shape {
                            Shape::Pencil if held => {
                                let last = m.points[m.points.len() - 1];
                                if (last.0 - p.0).hypot(last.1 - p.1) >= step {
                                    m.points.push(p);
                                }
                            }
                            Shape::Pen => {
                                let n = m.points.len();
                                m.points[n - 1] = p;
                            }
                            _ if held && m.points.len() >= 2 => m.points[1] = p,
                            _ => {}
                        }
                        cx.notify();
                    }
                    return;
                }
                if let Some((x, y)) = this.to_page(ev.position) {
                    if this.selecting(cx) {
                        this.send(Cmd::Inspect { x, y, pick: false }, cx);
                    } else {
                        let held = ev.pressed_button.and_then(page_button);
                        this.send(Cmd::Move { x, y, held }, cx);
                    }
                }
            }))
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _, cx| {
                if let Some((x, y)) = this.to_page(ev.position) {
                    // The page moves as far as the wheel did on screen.
                    let d = ev.delta.pixel_delta(px(40.));
                    let z = this.zoom;
                    this.send(
                        Cmd::Wheel {
                            x,
                            y,
                            dx: -f64::from(f32::from(d.x)) / z,
                            dy: -f64::from(f32::from(d.y)) / z,
                        },
                        cx,
                    );
                }
            }))
            // A start page over a blank tab.
            .when(blank && error.is_none(), |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(t.space.sm)
                        .bg(t.colors.surface)
                        .child(icon("icons/globe.svg", t.size.icon_lg, t.colors.text_subtle))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.colors.text_strong)
                                .child(if running { "Where to?" } else { "Starting the browser…" }),
                        )
                        .child(
                            div()
                                .max_w(px(300.))
                                .text_center()
                                .text_size(t.text.body_sm)
                                .line_height(relative(1.5))
                                .text_color(t.colors.text_muted)
                                .child("Type an address or a search above. When Forge browses for you, you watch it here, and can take over at any time."),
                        ),
                )
            })
            .children(error.map(|e| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(t.space.md)
                    .p(t.space.xxl)
                    .bg(t.colors.surface)
                    .child(icon("icons/triangle-alert.svg", t.size.icon_lg, t.extra.warning))
                    .child(
                        div()
                            .max_w(px(340.))
                            .text_center()
                            .text_size(t.text.body_sm)
                            .line_height(relative(1.5))
                            .text_color(t.colors.text)
                            .child(e),
                    )
                    .child(
                        crate::theme::ghost_button(&t, "Try again")
                            .id("web-retry")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |s, cx| {
                                    s.web_page.error = None;
                                    s.ensure_web(cx);
                                })
                            })),
                    )
            }))
            // The comment tool: the part under the pointer, the one picked,
            // the comments' pins, and the box for the next one.
            .children(hover_box.map(|((x, y, w, h), tag)| {
                part_outline(&t, x, y, w, h, false).child(
                    // At the right corner: a pin sits at the left one.
                    div()
                        .absolute()
                        .top(px(-20.))
                        .right(px(-2.))
                        .px(t.space.xs)
                        .rounded(t.radius.small)
                        .bg(t.colors.primary)
                        .text_color(t.colors.primary_fg)
                        .text_size(t.text.caption)
                        .child(tag),
                )
            }))
            .children(pick_box.map(|(x, y, w, h)| part_outline(&t, x, y, w, h, true)))
            .children(pins.into_iter().map(|(n, sent, text, (x, y, _, _))| {
                div()
                    .id(("design-pin", n))
                    .absolute()
                    .left(px(x - 10.))
                    .top(px(y - 10.))
                    .child(super::design_bar::pin_mark(&t, n, sent).shadow(t.elevation_popover()))
                    // With a tool in hand the pins let the mouse through, so
                    // a stroke can pass over them.
                    .when(!drawing_tool, |d| {
                        d.occlude().tooltip(move |window, cx| {
                            gpui::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
                        })
                    })
            }))
            // The Insert tools' marks, over the page.
            .when(!marks.is_empty(), |d| d.child(super::design_marks::render(marks, marks_place, &t)))
            // A label or note being typed, where it goes.
            .children(typing_at.map(|(shape, (x, y))| {
                crate::theme::popover(&t)
                    .id("design-mark-words")
                    .occlude()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(220.))
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                        if ev.keystroke.key == "escape" {
                            cx.stop_propagation();
                            this.typing = None;
                            window.focus(&this.focus, cx);
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .px(t.space.xs)
                            .child(if shape == crate::design::marks::Shape::Note { "Note" } else { "Text" }),
                    )
                    .child(Input::new(&self.mark_input).appearance(false).text_size(t.text.body_sm))
            }))
            // Marks made: comment on them for Forge, or clear them.
            .when(marks_count > 0 && s.design_pick.is_none() && !selecting, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(t.space.md)
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            crate::theme::popover(&t)
                                .occlude()
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .pl(t.space.md)
                                .text_size(t.text.caption)
                                .text_color(t.colors.text)
                                .child(match marks_count {
                                    1 => "1 mark".to_string(),
                                    n => format!("{n} marks"),
                                })
                                .child(
                                    crate::theme::ghost_button(&t, "Comment…")
                                        .id("design-marks-comment")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.drawing = None;
                                            this.state.update(cx, |s, cx| s.design_comment_on_marks(cx))
                                        })),
                                )
                                .child(
                                    crate::theme::text_button(&t, "Clear")
                                        .id("design-marks-clear")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.state.update(cx, |s, cx| s.design_clear_marks(cx))
                                        })),
                                ),
                        ),
                )
            })
            .when(selecting && s.design_pick.is_none(), |d| {
                d.child(
                    div()
                        .absolute()
                        .top(t.space.md)
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            crate::theme::popover(&t)
                                .px(t.space.md)
                                .py(t.space.xs)
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .text_size(t.text.caption)
                                .text_color(t.colors.text)
                                .child(icon("icons/mouse-pointer-2.svg", t.size.icon_xs, t.colors.primary))
                                .child("Point at any part and click to comment. Esc to stop."),
                        ),
                )
            })
            .children(pick_box.map(|(x, y, _, h)| {
                // Under the part when there is room, else over it; inside the panel.
                let (w_box, h_box, edge) = (300., 190., 8.);
                let (room_w, room_h) = (f32::from(room.width), f32::from(room.height));
                let left = x.min(room_w - w_box - edge).max(edge);
                let below = y + h + edge;
                let top = if below + h_box <= room_h {
                    below
                } else if y - h_box - edge >= edge {
                    y - h_box - edge
                } else {
                    (room_h - h_box - edge).max(edge)
                };
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .child(self.comment_box.clone())
            }))
            // The page's dialog (alert, confirm, prompt), answered here.
            .children(dialog)
            // Forge writing a page: which, and whether the panel follows it.
            .children(design.clone().map(|(name, writing)| {
                let following = following && writing;
                let state = self.state.clone();
                // Opaque, as it floats over the page, and its clicks are its
                // own, not the page's under it.
                crate::theme::popover(&t)
                    .occlude()
                    .absolute()
                    .bottom(t.space.md)
                    .left(t.space.md)
                    .max_w(relative(0.9))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .pl(t.space.md)
                    .pr(t.space.xs)
                    .py(t.space.xs)
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text)
                    .child(if !writing {
                        icon("icons/circle-check.svg", t.size.icon_xs, t.colors.primary).into_any_element()
                    } else if following {
                        icon("icons/loader-circle.svg", t.size.icon_xs, t.colors.primary)
                            .with_animation(
                                "web-design",
                                Animation::new(Duration::from_millis(900)).repeat(),
                                |svg, d| svg.with_transformation(Transformation::rotate(percentage(d))),
                            )
                            .into_any_element()
                    } else {
                        icon("icons/sparkles.svg", t.size.icon_xs, t.colors.primary).into_any_element()
                    })
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(if writing {
                                format!("Forge is designing {name}")
                            } else {
                                format!("{name} is ready")
                            }),
                    )
                    .child(
                        // Show leads somewhere, so it is the stronger of the two.
                        if following {
                            crate::theme::text_button(&t, "Stop following")
                        } else {
                            crate::theme::ghost_button(&t, "Show")
                        }
                        .id("web-design-follow")
                            .flex_none()
                            .on_click(move |_, _, cx| {
                                state.update(cx, |s, cx| s.design_set_follow(!following, cx))
                            }),
                    )
            }))
            // Forge at the wheel: a badge over the page.
            .when(agent && design.is_none(), |d| {
                d.child(
                    crate::theme::popover(&t)
                        .occlude()
                        .absolute()
                        .bottom(t.space.md)
                        .left(t.space.md)
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .px(t.space.md)
                        .py(t.space.xs)
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.text)
                        .child(
                            icon("icons/loader-circle.svg", t.size.icon_xs, t.colors.primary).with_animation(
                                "web-agent",
                                Animation::new(Duration::from_millis(900)).repeat(),
                                |svg, d| svg.with_transformation(Transformation::rotate(percentage(d))),
                            ),
                        )
                        .child("Forge is using the browser"),
                )
            });

        // A page this chat designed has the design bar for its toolbar.
        let bar = match design_bar {
            Some(b) => super::design_bar::render(b, &t, &self.state, cx).into_any_element(),
            None => toolbar.into_any_element(),
        };
        // Presenting, the page alone, with a quiet way back and through.
        let present_controls = presenting.then(|| present_controls(&t, cx));
        // The outline beside the page: a heading's click scrolls to it.
        let outline_rail = outline.map(|(heads, at)| {
            let hover = t.colors.hover;
            let empty = heads.is_empty();
            div()
                .id("design-outline")
                .flex_none()
                .w(px(200.))
                .h_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(2.))
                .p(t.space.sm)
                .bg(t.colors.surface)
                .border_r_1()
                .border_color(t.colors.border)
                .child(
                    div()
                        .px(t.space.sm)
                        .pb(t.space.xs)
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child("Outline"),
                )
                .when(empty, |d| {
                    d.child(
                        div()
                            .px(t.space.sm)
                            .text_size(t.text.caption)
                            .line_height(relative(1.4))
                            .text_color(t.colors.text_subtle)
                            .child("No headings on this page yet."),
                    )
                })
                .children(heads.into_iter().enumerate().map(|(i, h)| {
                    let state = self.state.clone();
                    div()
                        .id(("design-heading", i))
                        .flex_none()
                        .h(t.size.row)
                        .flex()
                        .items_center()
                        .pl(t.space.sm + t.space.md * f32::from(h.level.saturating_sub(1)))
                        .pr(t.space.sm)
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .text_size(t.text.body_sm)
                        .text_color(if h.level == 1 { t.colors.text_strong } else { t.colors.text })
                        .map(|d| if at == Some(i) { crate::theme::selected(&t, d) } else { d.hover(move |s| s.bg(hover)) })
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(h.text),
                        )
                        .on_click(move |_, _, cx| {
                            state.update(cx, |s, cx| s.web_send(Cmd::ScrollToHeading(i), cx))
                        })
                }))
        });

        div()
            .key_context("Browser")
            .size_full()
            .flex()
            .flex_col()
            .when(!presenting, |d| d.child(tab_strip).child(bar).child(progress))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .children(outline_rail)
                    .child(viewport.min_w_0().children(present_controls)),
            )
            .when(!presenting, |d| d.child(
                // The agent's access, said once at the foot.
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .px(t.space.md)
                    .py(t.space.xs)
                    .border_t_1()
                    .border_color(t.colors.border)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .children(last_download.map(|path| {
                        // The last download, named, and its folder a click away.
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let folder = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                        div()
                            .id("web-download")
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .max_w(px(220.))
                            .cursor_pointer()
                            .text_color(t.colors.text_muted)
                            .child(icon(
                                "icons/download.svg",
                                t.size.icon_xs,
                                t.colors.text_subtle,
                            ))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(format!("Downloaded {name}")),
                            )
                            .tooltip(move |window, cx| {
                                gpui::component::tooltip::Tooltip::new("Show in folder")
                                    .build(window, cx)
                            })
                            .on_click(move |_, _, cx| cx.open_with_system(&folder))
                    }))
                    .child(icon("icons/bot.svg", t.size.icon_xs, t.colors.text_subtle))
                    .child(div().flex_1().child(if s.browser {
                        "Forge can use this browser. Each step asks first."
                    } else {
                        "Only you use this browser. Turn it on for Forge in Settings."
                    }))
                    .when(!s.browser, |d| {
                        d.child(
                            div()
                                .id("web-agent-on")
                                .cursor_pointer()
                                .text_color(t.colors.primary)
                                .font_weight(FontWeight::MEDIUM)
                                .child("Turn on")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(super::ToggleBrowser), cx)
                                }),
                        )
                    }),
            ))
            .into_any_element()
    }
}

/// A part of the page outlined: the one under the pointer, or (`picked`)
/// the one a comment is on, a little filled.
fn part_outline(t: &Theme, x: f32, y: f32, w: f32, h: f32, picked: bool) -> gpui::Div {
    div()
        .absolute()
        .left(px(x - 2.))
        .top(px(y - 2.))
        .w(px(w + 4.))
        .h(px(h + 4.))
        .rounded(t.radius.small)
        .border_2()
        .border_color(t.colors.primary)
        .when(picked, |d| d.bg(t.colors.primary_tint))
}

/// While presenting: back a screen, on a screen, and the way out, in a
/// small bar at the page's foot. Its clicks are its own.
fn present_controls(t: &Theme, cx: &Context<BrowserView>) -> impl IntoElement {
    let step = |id: &'static str, path: &'static str, tip: &'static str, key: &'static str| {
        bar_button(t, id, path, tip, true).on_click(cx.listener(move |this, _, window, cx| {
            window.focus(&this.focus, cx);
            this.send(Cmd::Key(key.into()), cx);
        }))
    };
    div()
        .absolute()
        .bottom(t.space.lg)
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            crate::theme::popover(t)
                .occlude()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .child(step("present-back", "icons/arrow-left.svg", "Back a screen (Page Up)", "pageup"))
                .child(step("present-on", "icons/arrow-right.svg", "On a screen (Page Down)", "pagedown"))
                .child(
                    crate::theme::text_button(t, "Exit · Esc")
                        .id("present-exit")
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(super::Present), cx)),
                ),
        )
}

/// A dialog the page opened, as a card over the page: what it says, and
/// OK (with Cancel when it asks something). A prompt is answered with its
/// suggested text.
fn dialog_card(d: &PageDialog, t: &Theme, cx: &mut Context<BrowserView>) -> impl IntoElement {
    let question = matches!(d.kind.as_str(), "confirm" | "prompt" | "beforeunload");
    let heading = match d.kind.as_str() {
        "confirm" => "The page asks",
        "prompt" => "The page asks for an answer",
        "beforeunload" => "Leave this page?",
        _ => "The page says",
    };
    let message = if d.kind == "beforeunload" && d.message.is_empty() {
        "Changes you made may not be saved.".to_string()
    } else {
        d.message.clone()
    };
    let default = (d.kind == "prompt").then(|| d.default.clone());
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_start()
        .justify_center()
        .pt(t.space.xxl)
        .bg(t.extra.scrim)
        // The page under the card waits for the answer: no clicks reach it.
        .occlude()
        .child(
            div()
                .id("web-dialog")
                .w(px(360.))
                .flex()
                .flex_col()
                .gap(t.space.md)
                .p(t.space.lg)
                .bg(t.colors.surface)
                .border_1()
                .border_color(t.colors.border)
                .rounded(t.radius.card)
                .shadow_lg()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_strong)
                        .child(heading),
                )
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .line_height(relative(1.5))
                        .text_color(t.colors.text)
                        .child(message),
                )
                .children(default.clone().filter(|v| !v.is_empty()).map(|v| {
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(format!("OK answers \u{201c}{v}\u{201d}."))
                }))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(t.space.sm)
                        .when(question, |d| {
                            d.child(
                                crate::theme::ghost_button(t, "Cancel")
                                    .id("web-dialog-cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.send(
                                            Cmd::Dialog {
                                                accept: false,
                                                text: None,
                                            },
                                            cx,
                                        )
                                    })),
                            )
                        })
                        .child(
                            crate::theme::primary_button(
                                t,
                                if d.kind == "beforeunload" {
                                    "Leave"
                                } else {
                                    "OK"
                                },
                            )
                            .id("web-dialog-ok")
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.send(
                                        Cmd::Dialog {
                                            accept: true,
                                            text: default.clone(),
                                        },
                                        cx,
                                    )
                                },
                            )),
                        ),
                ),
        )
}
