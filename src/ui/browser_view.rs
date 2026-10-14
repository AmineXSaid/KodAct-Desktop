//! The browser in the side panel's Browser tab: a real Chromium page, live
//! (`web_view.rs`). Its tabs across the top, then back, forward, reload, the
//! address (type a URL or words to search) and "open in your browser"; a
//! thin bar while a page loads; then the page itself, which you click,
//! scroll and type into as in any browser. When Forge drives it (the
//! browser tools, `browser_mcp.rs`), a badge says so and you watch it work.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    canvas, div, img, percentage, prelude::*, px, relative, Animation, AnimationExt, Bounds,
    Context, Entity, FocusHandle, Focusable, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, ObjectFit, Pixels, Render, ScrollWheelEvent, Subscription, Transformation,
    Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::{ActiveTheme, Theme};
use crate::web_view::Cmd;

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
    _subs: Vec<Subscription>,
}

impl BrowserView {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search or enter an address"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
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
        Self {
            state,
            address,
            focus: cx.focus_handle(),
            page_bounds: Rc::new(Cell::new(Bounds::default())),
            sent_size: Rc::new(Cell::new((0, 0, 0.))),
            shown_url: String::new(),
            _subs: subs,
        }
    }

    fn send(&self, cmd: Cmd, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| s.web_send(cmd, cx));
    }

    /// A point in the window as a point on the page (CSS pixels): the
    /// page is laid out at the panel's size, so it is the offset in it.
    fn to_page(&self, p: gpui::Point<Pixels>) -> Option<(f64, f64)> {
        let b = self.page_bounds.get();
        b.contains(&p).then(|| {
            (
                f64::from(f32::from(p.x - b.origin.x)),
                f64::from(f32::from(p.y - b.origin.y)),
            )
        })
    }

    fn key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = k.modifiers;
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
                "r" => self.send(Cmd::Reload, cx),
                _ => return,
            }
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
        let s = self.state.read(cx);
        let page = &s.web_page;
        let running = s.web.is_some();
        let agent = s
            .chat()
            .is_some_and(|c| super::workbench::agent_browsing(&c.messages));
        let blank = page.url.is_empty() || page.url == "about:blank";
        let secure = page.url.starts_with("https://");
        let (tabs, active) = (page.tabs.clone(), page.active.clone());
        let (frame, loading, error) = (page.frame.clone(), page.loading, page.error.clone());
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
                    .rounded(t.radius.pill)
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
            // the mouse mapping (`to_page`) expects it.
            .children(frame.clone().map(|f| {
                let size = f.size(0);
                let scale = window.scale_factor();
                img(f)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(size.width.0 as f32 / scale))
                    .h(px(size.height.0 as f32 / scale))
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    if let Some((x, y)) = this.to_page(ev.position) {
                        this.send(Cmd::Click { x, y, count: ev.click_count.max(1) as u32 }, cx);
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if let Some((x, y)) = this.to_page(ev.position) {
                    this.send(Cmd::Move { x, y }, cx);
                }
            }))
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _, cx| {
                if let Some((x, y)) = this.to_page(ev.position) {
                    let d = ev.delta.pixel_delta(px(40.));
                    this.send(
                        Cmd::Wheel { x, y, dx: -f64::from(f32::from(d.x)), dy: -f64::from(f32::from(d.y)) },
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
            // Forge at the wheel: a badge over the page.
            .when(agent, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom(t.space.md)
                        .left(t.space.md)
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .px(t.space.md)
                        .py(t.space.xs)
                        .rounded(t.radius.pill)
                        .bg(t.colors.primary_tint)
                        .border_1()
                        .border_color(t.colors.primary.opacity(0.3))
                        .text_size(t.text.caption)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.colors.primary)
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

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(tab_strip)
            .child(toolbar)
            .child(progress)
            .child(viewport)
            .child(
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
            )
            .into_any_element()
    }
}
