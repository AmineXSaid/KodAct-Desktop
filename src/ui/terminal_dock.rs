//! The terminal dock on the right: tabs of shells (`term_session.rs`), each
//! drawn from its emulator's grid. On a remote project the shell runs there
//! (ssh, wsl.exe or docker exec), in the project's folder.
//!
//! Keys reach the shell before the app's shortcuts (Ctrl+L clears, Ctrl+R
//! searches history), except the few that work the dock and the app: Ctrl+`
//! and Ctrl+Shift+`, the command palette, the remote menu, and copy/paste
//! (Ctrl+Shift+C/V, or Ctrl+C while text is selected).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::{
    canvas, div, font, prelude::*, px, AnyElement, App, Bounds, ClipboardItem, Context, Entity,
    FocusHandle, FontWeight, Hsla, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels,
    Point, ScrollWheelEvent, SharedString, StyledText, Subscription, TextRun, Window,
};

use super::icon;
use crate::app_settings::CursorStyle;
use crate::state::AppState;
use crate::term_session::{self, Session, Shell, Size};
use crate::theme::{ActiveTheme as _, Theme};

struct Tab {
    id: usize,
    session: Arc<Session>,
    /// Where it runs, for the header's badge: the remote's label and icon,
    /// or None for this computer.
    place: Option<(String, &'static str)>,
}

pub struct TerminalDock {
    state: Entity<AppState>,
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
    focus: FocusHandle,
    /// The new-terminal menu (which shell) is open.
    picker: bool,
    /// Output arrived while the dock was hidden: the title-bar button shows
    /// a dot.
    pub unseen: bool,
    /// Why the last terminal didn't start.
    error: Option<SharedString>,
    /// The grid's place on screen, from the last paint.
    grid: Rc<Cell<Bounds<Pixels>>>,
    /// A drag selecting text is under way.
    selecting: bool,
    /// The right-click menu, open at this window position.
    menu_at: Option<Point<Pixels>>,
    /// Wheel movement not yet a whole line.
    wheel: f32,
    _intercept: Subscription,
}

/// The terminal's text size: `terminal.fontSize` when set, else the token.
fn text_size(t: &Theme) -> Pixels {
    let set = crate::app_settings::get().font_size;
    if (set - f32::from(t.size.terminal_text)).abs() < f32::EPSILON {
        t.size.terminal_text
    } else {
        px(set)
    }
}

/// One cell's width and height in the dock's monospace.
fn cell(window: &Window, t: &Theme) -> (Pixels, Pixels) {
    let size = text_size(t);
    let ts = window.text_system();
    let id = ts.resolve_font(&font(t.font_mono.clone()));
    let w = ts
        .advance(id, size, 'm')
        .map(|s| s.width)
        .unwrap_or(size * 0.6);
    (w, (size * 1.4).round())
}

impl TerminalDock {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let me = cx.entity().downgrade();
        // Ahead of the key bindings: while the terminal has the keyboard,
        // keys go to the shell.
        let intercept = cx.intercept_keystrokes(move |ev, window, cx| {
            if !ev.context_stack.iter().any(|c| c.contains("Terminal")) {
                return;
            }
            if reserved(&ev.keystroke) {
                return;
            }
            if let Some(dock) = me.upgrade() {
                let taken = dock.update(cx, |d, cx| d.key(&ev.keystroke, window, cx));
                if taken {
                    cx.stop_propagation();
                }
            }
        });
        let _ = window;
        Self {
            state,
            tabs: vec![],
            active: 0,
            next_id: 0,
            focus: cx.focus_handle(),
            picker: false,
            unseen: false,
            error: None,
            grid: Rc::new(Cell::new(Bounds::default())),
            selecting: false,
            menu_at: None,
            wheel: 0.,
            _intercept: intercept,
        }
    }

    pub fn has_tabs(&self) -> bool {
        !self.tabs.is_empty()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    fn session(&self) -> Option<&Arc<Session>> {
        self.tabs.get(self.active).map(|t| &t.session)
    }

    /// The shells on offer for the current project.
    fn shells(&self, cx: &App) -> Vec<Shell> {
        let (remote, dir, trusted) = self.state.read(cx).terminal_place();
        term_session::shells(remote.as_ref(), &dir, trusted)
    }

    /// Start a terminal running `shell`, or the first one on offer.
    pub fn open(&mut self, shell: Option<Shell>, window: &mut Window, cx: &mut Context<Self>) {
        self.picker = false;
        let Some(shell) = shell.or_else(|| self.shells(cx).into_iter().next()) else {
            self.error = Some("No shell found on this computer.".into());
            cx.notify();
            return;
        };
        let t = cx.theme().clone();
        let (cw, ch) = cell(window, &t);
        let b = self.grid.get();
        // The grid's last size, or a fair guess before the first paint.
        let size = if b.size.width > px(0.) {
            Size {
                cols: (b.size.width / cw).floor().max(2.) as u16,
                rows: (b.size.height / ch).floor().max(1.) as u16,
            }
        } else {
            Size { cols: 80, rows: 24 }
        };
        let (tx, rx) = async_channel::bounded::<()>(1);
        let place = self
            .state
            .read(cx)
            .current_remote()
            .map(|r| (r.target.label(), r.target.icon()));
        let scrollback = crate::app_settings::get().scrollback;
        match Session::start(shell, size, scrollback, move || {
            let _ = tx.try_send(());
        }) {
            Ok(session) => {
                self.error = None;
                self.tabs.push(Tab {
                    id: self.next_id,
                    session: Arc::new(session),
                    place,
                });
                self.next_id += 1;
                self.active = self.tabs.len() - 1;
                cx.spawn(async move |this, cx| {
                    while rx.recv().await.is_ok() {
                        let gone = this
                            .update(cx, |d, cx| {
                                if !d.state.read(cx).terminal_open {
                                    d.unseen = true;
                                    d.state.update(cx, |_, cx| cx.notify());
                                }
                                cx.notify();
                            })
                            .is_err();
                        if gone {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(e) => self.error = Some(e.into()),
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// "Show in terminal": an Agent tab with a command and what it printed,
    /// read-only, badged with where it ran.
    pub fn show_feed(
        &mut self,
        feed: crate::state::AgentFeed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let t = cx.theme().clone();
        let (cw, ch) = cell(window, &t);
        let b = self.grid.get();
        let size = if b.size.width > px(0.) {
            Size {
                cols: (b.size.width / cw).floor().max(2.) as u16,
                rows: (b.size.height / ch).floor().max(1.) as u16,
            }
        } else {
            Size { cols: 80, rows: 24 }
        };
        let shell = Shell {
            label: format!("Agent · {}", feed.title),
            program: String::new(),
            args: vec![],
            cwd: None,
        };
        let session = Session::replay(
            shell,
            &feed.bytes(),
            size,
            crate::app_settings::get().scrollback,
        );
        self.tabs.push(Tab {
            id: self.next_id,
            session: Arc::new(session),
            place: feed.place,
        });
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
        self.error = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Terminal: Kill. The active shell's process is ended; its tab stays,
    /// saying so, until a key closes it.
    pub fn kill(&mut self, cx: &mut Context<Self>) {
        self.menu_at = None;
        if let Some(s) = self.session() {
            s.kill();
        }
        cx.notify();
    }

    /// Terminal: Clear. The scrollback goes, and the shell redraws its
    /// prompt on a clean screen (Ctrl+L), as `clear` would.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        use alacritty_terminal::vte::ansi::{ClearMode, Handler as _};
        self.menu_at = None;
        if let Some(s) = self.session().cloned() {
            {
                let mut term = s.term.lock();
                term.clear_screen(ClearMode::Saved);
                term.scroll_display(Scroll::Bottom);
            }
            s.input(b"\x0c");
        }
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        self.menu_at = None;
        if let Some(s) = self.session() {
            let text = s
                .term
                .lock()
                .selection_to_string()
                .filter(|t| !t.is_empty());
            if let Some(text) = text {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                s.term.lock().selection = None;
            }
        }
        cx.notify();
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        self.menu_at = None;
        if let (Some(s), Some(text)) = (
            self.session().cloned(),
            cx.read_from_clipboard().and_then(|c| c.text()),
        ) {
            self.paste(&s, &text);
        }
        cx.notify();
    }

    /// The right-click menu: copy, paste, clear, kill.
    fn context_menu(&self, at: Point<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let hover = t.colors.hover;
        let has_selection = self.session().is_some_and(|s| {
            s.term
                .lock()
                .selection_to_string()
                .is_some_and(|x| !x.is_empty())
        });
        let item = |id: &'static str,
                    glyph: &'static str,
                    label: &'static str,
                    keys: &'static str,
                    on: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(t.space.sm)
                .h(t.size.row)
                .px(t.space.md)
                .rounded(t.radius.small)
                .when(on, |d| d.cursor_pointer().hover(move |st| st.bg(hover)))
                .text_color(if on {
                    t.colors.text
                } else {
                    t.colors.text_subtle
                })
                .child(icon(
                    glyph,
                    t.size.icon_sm,
                    if on {
                        t.colors.text_muted
                    } else {
                        t.colors.text_subtle
                    },
                ))
                .child(div().flex_1().child(label))
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(keys),
                )
        };
        let menu = crate::theme::popover(&t)
            .id("terminal-context-menu")
            .relative()
            .w(px(220.))
            .occlude()
            .text_size(t.text.body_sm)
            .child(
                item(
                    "term-copy",
                    "icons/copy.svg",
                    "Copy",
                    "Ctrl+Shift+C",
                    has_selection,
                )
                .when(has_selection, |d| {
                    d.on_click(cx.listener(|this, _, _, cx| this.copy(cx)))
                }),
            )
            .child(
                item(
                    "term-paste",
                    "icons/clipboard-paste.svg",
                    "Paste",
                    "Ctrl+Shift+V",
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| this.paste_clipboard(cx))),
            )
            .child(crate::theme::rule(&t).my(t.space.xs))
            .child(
                item("term-clear", "icons/eraser.svg", "Clear", "", true)
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            )
            .child(
                item("term-kill", "icons/circle-slash.svg", "Kill", "", true)
                    .on_click(cx.listener(|this, _, _, cx| this.kill(cx))),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu_at = None;
                cx.notify();
            }));
        gpui::deferred(
            gpui::anchored()
                .position(at)
                .snap_to_window_with_margin(t.space.sm)
                .child(super::motion::edge_draw(
                    menu,
                    "terminal-context-menu-in",
                    t.colors.primary,
                    0.,
                    &t,
                )),
        )
        .with_priority(2)
        .into_any_element()
    }

    /// Close tab `i`; the last one closing hides the dock.
    fn close(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        if self.tabs.is_empty() {
            self.state.update(cx, |s, cx| {
                s.terminal_open = false;
                s.save_window();
                cx.notify();
            });
            window.dispatch_action(Box::new(super::FocusComposer), cx);
        }
        cx.notify();
    }

    /// A key while the terminal has the keyboard. Returns whether it was
    /// taken (else it goes on to the app's bindings).
    fn key(&mut self, k: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(s) = self.session().cloned() else {
            return false;
        };
        let m = &k.modifiers;
        let key = k.key.as_str();
        // Esc closes the shell menu before it reaches the shell.
        if (self.picker || self.menu_at.is_some()) && key == "escape" {
            self.picker = false;
            self.menu_at = None;
            cx.notify();
            return true;
        }
        let selected = s
            .term
            .lock()
            .selection_to_string()
            .filter(|t| !t.is_empty());
        // Copy: Ctrl+Shift+C, Cmd+C, or Ctrl+C over a selection.
        if key == "c" && ((m.control && m.shift) || m.platform || (m.control && selected.is_some()))
        {
            if let Some(text) = selected {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                s.term.lock().selection = None;
                cx.notify();
            }
            return true;
        }
        if key == "v" && ((m.control && m.shift) || m.platform) {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                self.paste(&s, &text);
            }
            return true;
        }
        if m.platform {
            return false;
        }
        // Output only: keys don't reach anything (copy above still works).
        if s.is_replay() {
            return k.key_char.is_some() || key == "enter" || key == "backspace";
        }
        // A shell that ended: any key closes its tab.
        if s.exited.load(Ordering::SeqCst) {
            self.close(self.active, window, cx);
            return true;
        }
        let app_cursor = s.term.lock().mode().contains(TermMode::APP_CURSOR);
        let Some(bytes) = key_bytes(k, app_cursor) else {
            return false;
        };
        {
            let mut term = s.term.lock();
            term.selection = None;
            term.scroll_display(Scroll::Bottom);
        }
        s.input(&bytes);
        cx.notify();
        true
    }

    fn paste(&self, s: &Session, text: &str) {
        let bracketed = s.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        // Line ends as a terminal sends them; no escape may end the paste early.
        let body = text.replace("\r\n", "\r").replace('\n', "\r");
        if bracketed {
            let body = body.replace("\x1b[201~", "");
            s.input(format!("\x1b[200~{body}\x1b[201~").as_bytes());
        } else {
            s.input(body.as_bytes());
        }
    }

    /// The grid cell under `pos`, and which half of it.
    fn point_at(&self, pos: Point<Pixels>, window: &Window, cx: &App) -> Option<(GridPoint, Side)> {
        let s = self.session()?;
        let t = cx.theme();
        let (cw, ch) = cell(window, t);
        let b = self.grid.get();
        let x = (pos.x - b.origin.x).max(px(0.));
        let y = (pos.y - b.origin.y).max(px(0.));
        let term = s.term.lock();
        let cols = term.columns();
        let rows = term.screen_lines();
        let col = ((x / cw) as usize).min(cols.saturating_sub(1));
        let row = ((y / ch) as usize).min(rows.saturating_sub(1));
        let side = if (x / cw).fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        let offset = term.grid().display_offset() as i32;
        Some((GridPoint::new(Line(row as i32 - offset), Column(col)), side))
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.picker = false;
        let Some((point, side)) = self.point_at(ev.position, window, cx) else {
            return;
        };
        let Some(s) = self.session() else { return };
        let ty = match ev.click_count {
            2 => SelectionType::Semantic,
            n if n >= 3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        s.term.lock().selection = Some(Selection::new(ty, point, side));
        self.selecting = true;
        cx.notify();
    }

    fn mouse_move(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || ev.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some((point, side)) = self.point_at(ev.position, window, cx) else {
            return;
        };
        if let Some(s) = self.session() {
            if let Some(sel) = s.term.lock().selection.as_mut() {
                sel.update(point, side);
            }
        }
        cx.notify();
    }

    fn mouse_up(&mut self, cx: &mut Context<Self>) {
        self.selecting = false;
        if let Some(s) = self.session() {
            let mut term = s.term.lock();
            let empty = term.selection.as_ref().is_some_and(|sel| sel.is_empty());
            if empty {
                term.selection = None;
            }
        }
        cx.notify();
    }

    fn scroll(&mut self, ev: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.session().cloned() else {
            return;
        };
        let (_, ch) = cell(window, cx.theme());
        self.wheel += f32::from(ev.delta.pixel_delta(ch).y) / f32::from(ch);
        let lines = self.wheel.trunc() as i32;
        if lines == 0 {
            return;
        }
        self.wheel -= lines as f32;
        let alt = s.term.lock().mode().contains(TermMode::ALT_SCREEN);
        if alt {
            // A full-screen program scrolls itself: send it arrow keys.
            let app = s.term.lock().mode().contains(TermMode::APP_CURSOR);
            let key = match (lines > 0, app) {
                (true, true) => "\x1bOA",
                (true, false) => "\x1b[A",
                (false, true) => "\x1bOB",
                (false, false) => "\x1b[B",
            };
            s.input(key.repeat(lines.unsigned_abs() as usize).as_bytes());
        } else {
            s.term.lock().scroll_display(Scroll::Delta(lines));
        }
        cx.notify();
    }

    fn tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let hover = t.colors.hover;
        let mut bar = div()
            .id("terminal-tabs")
            .flex()
            .items_center()
            .gap(t.space.xs)
            .h(t.size.row + px(4.))
            .px(t.space.sm)
            .border_b_1()
            .border_color(t.colors.border)
            .overflow_x_scroll();
        for (i, tab) in self.tabs.iter().enumerate() {
            let on = i == self.active;
            let exited = tab.session.exited.load(Ordering::SeqCst);
            let title = tab.session.title();
            bar = bar.child(
                div()
                    .id(("terminal-tab", tab.id))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(t.space.xs)
                    .h(t.size.control)
                    .pl(t.space.sm)
                    .pr(t.space.xs)
                    .max_w(px(180.))
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(t.colors.surface)
                            .border_1()
                            .border_color(t.colors.border)
                    })
                    .when(!on, |d| d.hover(move |st| st.bg(hover)))
                    .child(if exited {
                        icon(
                            "icons/circle-slash.svg",
                            t.size.icon_xs,
                            t.colors.text_subtle,
                        )
                    } else if tab.session.is_replay() {
                        icon("icons/sparkles.svg", t.size.icon_xs, t.colors.primary)
                    } else {
                        icon("icons/terminal.svg", t.size.icon_xs, t.colors.text_muted)
                    })
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.caption)
                            .text_color(if on {
                                t.colors.text
                            } else {
                                t.colors.text_muted
                            })
                            .when(exited, |d| d.text_color(t.colors.text_subtle))
                            .child(if exited {
                                format!("Ended · {title}")
                            } else {
                                title
                            }),
                    )
                    .child(
                        div()
                            .id(("terminal-tab-close", tab.id))
                            .flex_none()
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.small)
                            .hover(move |st| st.bg(hover))
                            .child(icon("icons/x.svg", t.size.icon_xs, t.colors.text_muted))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close(i, window, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.active = i;
                        window.focus(&this.focus, cx);
                        cx.notify();
                    })),
            );
        }
        let button = |id: &'static str, path: &'static str| {
            div()
                .id(id)
                .flex_none()
                .size(t.size.control)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover(move |st| st.bg(hover))
                .child(icon(path, t.size.icon_sm, t.colors.text_muted))
        };
        bar.child(
            button("terminal-new", "icons/plus.svg")
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new("New terminal (Ctrl+Shift+`)")
                        .build(window, cx)
                })
                .on_click(cx.listener(|this, _, window, cx| this.open(None, window, cx))),
        )
        .child(
            button("terminal-shells", "icons/chevron-down.svg")
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new("Choose a shell").build(window, cx)
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.picker = !this.picker;
                    cx.notify();
                })),
        )
        .child(div().flex_1())
        .children(self.tabs.get(self.active).map(|tab| {
            // Where the active terminal runs.
            let (label, glyph) = tab
                .place
                .clone()
                .unwrap_or_else(|| ("Local".into(), "icons/laptop.svg"));
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.space.xs)
                .h(px(20.))
                .px(t.space.sm)
                .rounded(t.radius.pill)
                .bg(if tab.place.is_some() {
                    t.colors.primary_tint
                } else {
                    t.extra.inset
                })
                .text_size(t.text.caption)
                .text_color(if tab.place.is_some() {
                    t.colors.primary
                } else {
                    t.colors.text_muted
                })
                .max_w(px(160.))
                .child(icon(
                    glyph,
                    t.size.icon_xs,
                    if tab.place.is_some() {
                        t.colors.primary
                    } else {
                        t.colors.text_muted
                    },
                ))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
        }))
        .child(
            button("terminal-hide", "icons/minus.svg")
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new("Hide the terminal (Ctrl+`)")
                        .build(window, cx)
                })
                .on_click(cx.listener(|_, _, window, cx| {
                    window.dispatch_action(Box::new(super::ToggleTerminal), cx)
                })),
        )
    }

    fn shell_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let hover = t.colors.hover;
        let mut list = div().flex().flex_col();
        for (i, sh) in self.shells(cx).into_iter().enumerate() {
            let label = sh.label.clone();
            list = list.child(
                div()
                    .id(("terminal-shell", i))
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .h(t.size.row)
                    .px(t.space.md)
                    .rounded(t.radius.small)
                    .cursor_pointer()
                    .hover(move |st| st.bg(hover))
                    .child(icon(
                        "icons/terminal.svg",
                        t.size.icon_sm,
                        t.colors.text_muted,
                    ))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open(Some(sh.clone()), window, cx)
                    })),
            );
        }
        let menu = crate::theme::popover(&t)
            .id("terminal-shell-menu")
            .absolute()
            .top(t.size.row + px(4.))
            .left(t.space.sm)
            .w(px(240.))
            .occlude()
            .text_size(t.text.body_sm)
            .child(list);
        super::motion::edge_draw(menu, "terminal-shell-menu-in", t.colors.primary, 0., &t)
    }

    /// The active terminal's screen.
    fn screen(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = cx.theme().clone();
        let (cw, ch) = cell(window, &t);
        let focused = self.focus.is_focused(window);
        let Some(s) = self.session().cloned() else {
            return div()
                .flex_1()
                .p(t.space.lg)
                .text_color(t.colors.text_muted)
                .child(
                    self.error
                        .clone()
                        .unwrap_or_else(|| "No terminal open.".into()),
                )
                .into_any_element();
        };
        // Forge for VS Code's terminal: ink, its foreground, a purple cursor.
        let k = t.terminal;
        let fg = k.foreground;
        let bg = k.background;
        let sel_bg = k.prompt.opacity(0.3);
        let mono = t.font_mono.clone();
        let mut rows: Vec<AnyElement> = vec![];
        let (cursor, cursor_char, cursor_row) = {
            let term = s.term.lock();
            let content = term.renderable_content();
            let offset = content.display_offset as i32;
            let lines = term.screen_lines();
            let cols = term.columns();
            let mut text: Vec<String> = vec![String::with_capacity(cols); lines];
            let mut runs: Vec<Vec<TextRun>> = vec![vec![]; lines];
            let mut cursor_char = ' ';
            let cursor = content.cursor.point;
            for c in content.display_iter {
                let row = (c.point.line.0 + offset) as usize;
                if row >= lines {
                    continue;
                }
                let flags = c.cell.flags;
                if flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                if c.point == cursor {
                    cursor_char = c.cell.c;
                }
                let mut f = resolve(c.cell.fg, content.colors, &t, fg, bg);
                let mut b = resolve(c.cell.bg, content.colors, &t, fg, bg);
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut f, &mut b);
                }
                if flags.contains(Flags::DIM) {
                    f = f.opacity(0.7);
                }
                if flags.contains(Flags::HIDDEN) {
                    f = b;
                }
                if content
                    .selection
                    .is_some_and(|r| r.contains_cell(&c, cursor, content.cursor.shape))
                {
                    b = sel_bg;
                }
                let ch_ = if c.cell.c == '\0' { ' ' } else { c.cell.c };
                let len = ch_.len_utf8();
                text[row].push(ch_);
                let mut fnt = font(mono.clone());
                if flags.contains(Flags::BOLD) {
                    fnt.weight = FontWeight::BOLD;
                }
                if flags.contains(Flags::ITALIC) {
                    fnt.style = gpui::FontStyle::Italic;
                }
                let back = (b != bg).then_some(b);
                let underline =
                    flags
                        .intersects(Flags::ALL_UNDERLINES)
                        .then(|| gpui::UnderlineStyle {
                            thickness: px(1.),
                            color: Some(f),
                            wavy: flags.contains(Flags::UNDERCURL),
                        });
                let strike = flags
                    .contains(Flags::STRIKEOUT)
                    .then(|| gpui::StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(f),
                    });
                match runs[row].last_mut() {
                    Some(r)
                        if r.color == f
                            && r.background_color == back
                            && r.font == fnt
                            && r.underline == underline
                            && r.strikethrough == strike =>
                    {
                        r.len += len
                    }
                    _ => runs[row].push(TextRun {
                        len,
                        font: fnt,
                        color: f,
                        background_color: back,
                        underline,
                        strikethrough: strike,
                    }),
                }
            }
            for (line, (text, runs)) in text.into_iter().zip(runs).enumerate() {
                rows.push(
                    div()
                        .id(("terminal-line", line))
                        .h(ch)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(StyledText::new(text).with_runs(runs))
                        .into_any_element(),
                );
            }
            let shown = content.mode.contains(TermMode::SHOW_CURSOR);
            let row = cursor.line.0 + offset;
            let visible = shown && row >= 0 && (row as usize) < lines;
            (
                visible.then_some((cursor.column.0, row as usize)),
                cursor_char,
                row.clamp(0, lines as i32 - 1),
            )
        };
        let grid = self.grid.clone();
        let session = s.clone();
        let exited = s.exited.load(Ordering::SeqCst);
        // An ended shell takes no input: no cursor, a note where it was.
        let replay = s.is_replay();
        let cursor = cursor.filter(|_| !exited && !replay);
        div()
            .id("terminal-screen")
            .relative()
            .bg(bg)
            .flex_1()
            .min_h_0()
            .flex()
            .px(t.space.sm)
            .pt(t.space.xs)
            .font_family(mono.clone())
            .text_size(text_size(&t))
            .line_height(ch)
            .text_color(fg)
            .cursor(gpui::CursorStyle::IBeam)
            .child(
                // The grid, inside the padding: its size sets the shell's
                // columns and rows, and cells are placed from its corner.
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                grid.set(bounds);
                                let size = Size {
                                    cols: (bounds.size.width / cw).floor().max(2.) as u16,
                                    rows: (bounds.size.height / ch).floor().max(1.) as u16,
                                };
                                if session.size() != size {
                                    session.resize(size);
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .children(rows)
                    .children(cursor.map(|(col, row)| {
                        let at = div()
                            .absolute()
                            .left(cw * col as f32)
                            .top(ch * row as f32)
                            .w(cw)
                            .h(ch);
                        match (focused, crate::app_settings::get().cursor_style) {
                            (false, _) => {
                                at.border_1().rounded(t.radius.line).border_color(k.prompt)
                            }
                            (true, CursorStyle::Block) => at
                                .rounded(t.radius.line)
                                .bg(k.prompt)
                                .text_color(k.background)
                                .child(cursor_char.to_string()),
                            (true, CursorStyle::Bar) => at.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .bottom_0()
                                    .w(px(2.))
                                    .rounded(t.radius.line)
                                    .bg(k.prompt),
                            ),
                            (true, CursorStyle::Underline) => at.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .bottom_0()
                                    .h(px(2.))
                                    .rounded(t.radius.line)
                                    .bg(k.prompt),
                            ),
                        }
                    }))
                    .when(exited, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left_0()
                                .top(ch * cursor_row as f32)
                                .flex()
                                .items_center()
                                .gap(t.space.xs)
                                .px(t.space.sm)
                                .h(ch)
                                .rounded(t.radius.small)
                                .bg(t.extra.inset)
                                .font_family(t.font_sans.clone())
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(icon(
                                    "icons/circle-slash.svg",
                                    t.size.icon_xs,
                                    t.colors.text_subtle,
                                ))
                                .child("The shell ended. Press any key to close this tab."),
                        )
                    }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev, window, cx| this.mouse_down(ev, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.menu_at = Some(ev.position);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev, window, cx| this.mouse_move(ev, window, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.mouse_up(cx)),
            )
            .on_scroll_wheel(cx.listener(|this, ev, window, cx| this.scroll(ev, window, cx)))
            .into_any_element()
    }
}

impl Render for TerminalDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.state.read(cx).terminal_open {
            self.unseen = false;
        }
        let picker = self.picker.then(|| self.shell_menu(cx));
        let menu = self.menu_at.map(|at| self.context_menu(at, cx));
        div()
            .id("terminal-dock")
            .key_context("Terminal")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.extra.stage)
            .child(self.tab_bar(cx))
            .child(self.screen(window, cx))
            .children(picker)
            .children(menu)
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.picker {
                    this.picker = false;
                    cx.notify();
                }
            }))
    }
}

/// Keys that keep their app meaning inside the terminal.
fn reserved(k: &Keystroke) -> bool {
    let m = &k.modifiers;
    let key = k.key.as_str();
    // Ctrl+Shift+` arrives as Ctrl+~ (Shift folds into the symbol).
    (m.control && (key == "`" || key == "~"))
        || (m.control && !m.shift && !m.alt && key == "k")
        || (m.control && m.alt && key == "o")
}

/// The bytes a key sends to a shell, as an xterm sends them.
pub fn key_bytes(k: &Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let m = &k.modifiers;
    // xterm's modifier parameter: 1 + shift + 2·alt + 4·ctrl.
    let param = 1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.control as u8;
    let csi = |end: &str| -> Vec<u8> {
        if param > 1 {
            format!("\x1b[1;{param}{end}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{end}").into_bytes()
        } else {
            format!("\x1b[{end}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if param > 1 {
            format!("\x1b[{n};{param}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let alt = |b: &[u8]| -> Vec<u8> {
        if m.alt {
            [b"\x1b", b].concat()
        } else {
            b.to_vec()
        }
    };
    let out = match k.key.as_str() {
        "enter" => alt(b"\r"),
        "backspace" if m.control => alt(b"\x08"),
        "backspace" => alt(b"\x7f"),
        "tab" if m.shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "space" if m.control => vec![0],
        "up" => csi("A"),
        "down" => csi("B"),
        "right" => csi("C"),
        "left" => csi("D"),
        "home" => csi("H"),
        "end" => csi("F"),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" => csi("P"),
        "f2" => csi("Q"),
        "f3" => csi("R"),
        "f4" => csi("S"),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        key => {
            // Ctrl with Alt is AltGr on Windows keyboards: the typed character wins.
            if m.control && m.alt {
                if let Some(c) = &k.key_char {
                    return Some(c.as_bytes().to_vec());
                }
            }
            if m.control {
                let c = key.chars().next().filter(|_| key.chars().count() == 1)?;
                let b = match c.to_ascii_lowercase() {
                    c @ 'a'..='z' => c as u8 & 0x1f,
                    '@' | '2' => 0,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '^' | '6' => 0x1e,
                    '_' | '-' | '/' | '7' => 0x1f,
                    '8' => 0x7f,
                    _ => return None,
                };
                return Some(alt(&[b]));
            }
            let text = k.key_char.clone().or_else(|| {
                (key.chars().count() == 1).then(|| {
                    if m.shift {
                        key.to_uppercase()
                    } else {
                        key.to_string()
                    }
                })
            })?;
            alt(text.as_bytes())
        }
    };
    Some(out)
}

/// A cell's colour in the theme: the 16 named colours from the dock's
/// tokens (unless a program set its own), the 256-colour cube and greys,
/// and exact colours as given.
fn resolve(
    c: Color,
    set: &alacritty_terminal::term::color::Colors,
    t: &Theme,
    fg: Hsla,
    bg: Hsla,
) -> Hsla {
    let rgb = |r: u8, g: u8, b: u8| -> Hsla {
        gpui::Rgba {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a: 1.,
        }
        .into()
    };
    let indexed = |i: usize| -> Hsla {
        if let Some(c) = set[i] {
            return rgb(c.r, c.g, c.b);
        }
        match i {
            0..=15 => t.extra.ansi[i],
            16..=231 => {
                let i = i - 16;
                let level = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
                rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
            }
            _ => {
                let v = (8 + (i.min(255) - 232) * 10) as u8;
                rgb(v, v, v)
            }
        }
    };
    match c {
        Color::Spec(c) => rgb(c.r, c.g, c.b),
        Color::Indexed(i) => indexed(i as usize),
        Color::Named(n) => {
            let i = n as usize;
            match n {
                NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => {
                    set[i].map_or(fg, |c| rgb(c.r, c.g, c.b))
                }
                NamedColor::Background => set[i].map_or(bg, |c| rgb(c.r, c.g, c.b)),
                NamedColor::DimForeground => fg.opacity(0.7),
                _ if i < 16 => indexed(i),
                // The dim eight follow the cursor colours in the list.
                _ => {
                    let base = i.saturating_sub(NamedColor::DimBlack as usize).min(7);
                    indexed(base).opacity(0.7)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn k(key: &str, ctrl: bool, shift: bool, alt: bool, ch: Option<&str>) -> Keystroke {
        Keystroke {
            key: key.into(),
            key_char: ch.map(Into::into),
            modifiers: Modifiers {
                control: ctrl,
                shift,
                alt,
                ..Default::default()
            },
        }
    }

    #[test]
    fn keys_reach_the_shell_as_xterm_sends_them() {
        let b = |ks: Keystroke, app: bool| key_bytes(&ks, app).unwrap();
        assert_eq!(b(k("a", false, false, false, Some("a")), false), b"a");
        assert_eq!(b(k("a", false, true, false, Some("A")), false), b"A");
        assert_eq!(b(k("c", true, false, false, None), false), [3]);
        assert_eq!(b(k("l", true, false, false, None), false), [12]);
        assert_eq!(b(k("enter", false, false, false, None), false), b"\r");
        assert_eq!(b(k("backspace", false, false, false, None), false), b"\x7f");
        assert_eq!(b(k("up", false, false, false, None), false), b"\x1b[A");
        assert_eq!(b(k("up", false, false, false, None), true), b"\x1bOA");
        assert_eq!(b(k("right", true, false, false, None), false), b"\x1b[1;5C");
        assert_eq!(b(k("delete", false, false, false, None), false), b"\x1b[3~");
        assert_eq!(b(k("f5", false, false, false, None), false), b"\x1b[15~");
        assert_eq!(b(k("b", false, false, true, Some("b")), false), b"\x1bb");
        assert_eq!(b(k("tab", false, true, false, None), false), b"\x1b[Z");
        // AltGr on a Windows keyboard: the character typed, not Ctrl+Alt.
        assert_eq!(b(k("q", true, false, true, Some("@")), false), b"@");
    }

    #[test]
    fn app_keys_stay_with_the_app() {
        assert!(reserved(&k("`", true, false, false, None)));
        assert!(reserved(&k("`", true, true, false, None)));
        assert!(reserved(&k("~", true, false, false, None)));
        assert!(reserved(&k("k", true, false, false, None)));
        assert!(reserved(&k("o", true, false, true, None)));
        assert!(!reserved(&k("l", true, false, false, None)));
        assert!(!reserved(&k("r", true, false, false, None)));
    }

    #[test]
    fn colours_come_from_the_theme() {
        let t = Theme::light();
        let set = alacritty_terminal::term::color::Colors::default();
        let (fg, bg) = (t.terminal.foreground, t.terminal.background);
        assert_eq!(
            resolve(Color::Named(NamedColor::Red), &set, &t, fg, bg),
            t.extra.ansi[1]
        );
        assert_eq!(
            resolve(Color::Indexed(12), &set, &t, fg, bg),
            t.extra.ansi[12]
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::Foreground), &set, &t, fg, bg),
            fg
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::Background), &set, &t, fg, bg),
            bg
        );
        // The cube's corners: black and white.
        let white = resolve(Color::Indexed(231), &set, &t, fg, bg);
        assert!((white.l - 1.).abs() < 0.01);
    }
}
