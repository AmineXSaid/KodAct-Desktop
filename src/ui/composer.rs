//! The composer, one unit: a white card holding a multi-line input that
//! grows up to a limit, the outlined + (attach) and Send (Stop while a reply
//! plays), set in a recessed tray that carries the project, the context
//! meter, the model picker (a popover) and the Modes menu. Enter sends;
//! Shift+Enter adds a line.

use gpui::component::input::{
    InlineToken, InlineTokenContext, Input, InputEvent, InputState, Textarea, TextareaState,
};
use gpui::{
    div, prelude::*, px, relative, Context, Entity, IntoElement, Render, SharedString,
    Subscription, Window,
};

use super::*;
use crate::state::AppState;
use crate::theme::ActiveTheme;

/// The empty box's line: while a question waits, or a reply runs, what
/// typing does then; else a project's, or a plain chat's.
fn placeholder(in_folder: bool, asking: bool, busy: bool) -> &'static str {
    if asking {
        "Or tell Forge what to do instead…"
    } else if busy {
        "Add to the task, or steer Forge…"
    } else if in_folder {
        "What should we build today?"
    } else {
        "Ask Forge anything"
    }
}

pub struct Composer {
    state: Entity<AppState>,
    input: Entity<TextareaState>,
    /// The model window's search box, and its row under the keyboard.
    filter: Entity<InputState>,
    active: Option<String>,
    picker_was_open: bool,
    /// Where the press that just closed a menu happened: the click it ends
    /// in, on that menu's own button, must not open it again.
    dismissed_at: Option<gpui::Point<gpui::Pixels>>,
    /// The "+" menu is open.
    add_open: bool,
    /// Holds the keyboard while the model window is open (no search box),
    /// so the arrows move through it instead of the text.
    menu_focus: gpui::FocusHandle,
    /// The autocomplete row under the keyboard, and the word Esc closed it
    /// for (it stays closed until the word changes).
    ac_selected: usize,
    ac_closed_for: Option<String>,
    /// Enter just completed a word: the text box's own Enter that follows
    /// must not send the message.
    ac_took_enter: bool,
    /// Where the autocomplete's word starts (its byte offset), and the
    /// caret's place and line height there, in the window.
    ac_anchor: Option<(usize, gpui::Point<gpui::Pixels>, gpui::Pixels)>,
    /// The placeholder shown: (a project's, a question waits, a reply runs).
    placeholder_now: (bool, bool, bool),
    _subs: Vec<Subscription>,
}

impl Composer {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let t = cx.theme().clone();
        // The tallest the input grows, in lines of chat text.
        let max_rows = (f32::from(t.size.composer_max) / (f32::from(t.text.body) * 1.5))
            .floor()
            .max(2.) as usize;
        let in_folder = state.read(cx).in_folder();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, max_rows)
                .placeholder(placeholder(in_folder, false, false))
                .submit_on_enter(true)
        });
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search models…"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                // An edited message comes back here to be changed and sent.
                if let Some(text) = state.update(cx, |s, _| s.draft.take()) {
                    this.fill(&text, window, cx);
                }
                // A plain chat asks for anything; a project's, for work on it.
                // While a question waits, what you type answers it.
                let s = state.read(cx);
                let now = (s.in_folder(), s.asking(), s.busy());
                if now != this.placeholder_now {
                    this.placeholder_now = now;
                    this.input.update(cx, |i, cx| {
                        i.set_placeholder(placeholder(now.0, now.1, now.2), window, cx)
                    });
                }
                // The model window opening: an empty search, the current row active.
                let s = state.read(cx);
                let (open, current) = (s.picker_open, s.active_profile().map(|p| p.name.clone()));
                let rows = s.endpoints.as_ref().map(|e| e.profiles.len()).unwrap_or(0);
                if open && !this.picker_was_open {
                    this.active = current;
                    this.filter.update(cx, |f, cx| f.set_value("", window, cx));
                    if rows >= super::model_menu::FILTER_THRESHOLD {
                        this.filter.update(cx, |f, cx| f.focus(window, cx));
                    } else {
                        window.focus(&this.menu_focus, cx);
                    }
                }
                if !open
                    && this.picker_was_open
                    && (this.menu_focus.is_focused(window)
                        || gpui::Focusable::focus_handle(this.filter.read(cx), cx)
                            .is_focused(window))
                {
                    this.input.update(cx, |i, cx| i.focus(window, cx));
                }
                this.picker_was_open = open;
                cx.notify();
            }),
            cx.subscribe_in(&filter, window, |_, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(
                &input,
                window,
                |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::PressEnter { shift: false, .. } => this.submit(window, cx),
                    InputEvent::Change => {
                        this.ac_selected = 0;
                        this.tokenize(window, cx);
                        if super::autocomplete::trigger(&this.text(cx)).is_some() {
                            this.state.update(cx, |s, cx| s.ensure_completions(cx));
                        }
                        cx.notify()
                    }
                    _ => {}
                },
            ),
        ];
        Self {
            state,
            input,
            filter,
            active: None,
            picker_was_open: false,
            dismissed_at: None,
            add_open: false,
            menu_focus: cx.focus_handle(),
            ac_selected: 0,
            ac_closed_for: None,
            ac_took_enter: false,
            ac_anchor: None,
            placeholder_now: (in_folder, false, false),
            _subs: subs,
        }
    }

    /// The text box's focus: the home screen's dot field brightens on it.
    pub fn focus_handle(&self, cx: &gpui::App) -> gpui::FocusHandle {
        gpui::Focusable::focus_handle(self.input.read(cx), cx)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| i.focus(window, cx));
    }

    /// Put text in the box (a suggestion chip) and focus it.
    pub fn fill(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| {
            i.set_value(text.to_string(), window, cx);
            // The caret goes to the end, where you go on typing.
            let end = gpui::component::RopeExt::offset_to_position(i.text(), text.len());
            i.set_cursor_position(end, window, cx);
        });
        self.tokenize(window, cx);
        cx.notify();
    }

    /// A finished `/command` leading the message, or `@path` naming a
    /// project file, becomes one unit in the box: bold, a skill in its own
    /// colour (`mention_token`). Finished means a space follows it, so a
    /// word being typed stays text. The message sent is the same text.
    fn tokenize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (text, taken) = {
            let i = self.input.read(cx);
            let taken: Vec<std::ops::Range<usize>> =
                i.tokens().iter().map(|t| t.range()).collect();
            (i.value().to_string(), taken)
        };
        let found = mentions(&text, self.state.read(cx));
        let fresh: Vec<_> = found
            .into_iter()
            .filter(|(r, _)| !taken.iter().any(|t| t.start < r.end && r.start < t.end))
            .collect();
        if fresh.is_empty() {
            return;
        }
        self.input.update(cx, |i, cx| {
            // Replacing a range leaves the caret at its end, before the space
            // just typed; the text is unchanged, so put the caret back.
            let caret = i.cursor();
            // From the end, so the earlier ranges still hold.
            for (range, token) in fresh.into_iter().rev() {
                let _ = i.replace_range_with_token(range, token, window, cx);
            }
            let at = gpui::component::RopeExt::offset_to_position(i.text(), caret);
            i.set_cursor_position(at, window, cx);
        });
    }

    fn text(&self, cx: &gpui::App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.ac_took_enter) {
            return;
        }
        let text = self.text(cx);
        // A question waits: what you typed denies it and says what to do
        // instead.
        if self.state.read(cx).asking() && !text.trim().is_empty() && crate::state::parse_btw(&text).is_none() {
            self.state.update(cx, |s, cx| s.deny_current_with_note(text.trim().to_string(), cx));
            self.input.update(cx, |i, cx| i.set_value("", window, cx));
            return;
        }
        // While a reply runs, a message is queued for Forge's next step
        // (`/btw` keeps its side card).
        if self.state.read(cx).busy() && crate::state::parse_btw(&text).is_none() {
            if !text.trim().is_empty() {
                self.state.update(cx, |s, cx| s.queue_message(text.trim().to_string(), cx));
                self.input.update(cx, |i, cx| i.set_value("", window, cx));
            }
            return;
        }
        if text.trim().is_empty() && self.state.read(cx).attachments.is_empty() {
            return;
        }
        self.state.update(cx, |s, cx| s.send(text, cx));
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
    }

    /// The autocomplete's word and rows, when it is open.
    fn completion(
        &self,
        cx: &gpui::App,
    ) -> Option<(super::autocomplete::Trigger, Vec<super::autocomplete::Item>)> {
        let text = self.text(cx);
        let trig = super::autocomplete::trigger(&text)?;
        if self.ac_closed_for.as_deref() == Some(&text[trig.start..]) {
            return None;
        }
        let rows = super::autocomplete::items(&trig, self.state.read(cx));
        Some((trig, rows))
    }

    /// Complete with row `i`.
    fn pick(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((trig, mut rows)) = self.completion(cx) else {
            return;
        };
        if i >= rows.len() {
            return;
        }
        let text = self.text(cx);
        let before = text[..trig.start].to_string();
        match rows.swap_remove(i).pick {
            super::autocomplete::Pick::Text(t) => self.fill(&format!("{before}{t}"), window, cx),
            super::autocomplete::Pick::Action(a) => {
                self.fill(&before, window, cx);
                window.dispatch_action(a, cx);
            }
            super::autocomplete::Pick::Mode(m) => {
                self.fill(&before, window, cx);
                self.state.update(cx, |s, cx| s.set_mode(m, cx));
            }
        }
        self.ac_selected = 0;
    }

    /// The autocomplete's keys, while it is open. True when it took the key.
    fn completion_key(
        &mut self,
        ev: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let k = &ev.keystroke;
        if k.modifiers.secondary() || k.modifiers.alt || k.modifiers.shift {
            return false;
        }
        self.completion_nav(k.key.as_str(), window, cx)
    }

    /// Move through the list, pick from it or close it. True when it took `key`.
    fn completion_nav(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((trig, rows)) = self.completion(cx) else {
            return false;
        };
        let n = rows.len();
        match key {
            "down" if n > 0 => self.ac_selected = (self.ac_selected + 1) % n,
            "up" if n > 0 => self.ac_selected = (self.ac_selected + n - 1) % n,
            "enter" | "tab" if n > 0 => {
                let i = self.ac_selected.min(n - 1);
                self.ac_took_enter = key == "enter";
                self.pick(i, window, cx);
            }
            "escape" => self.ac_closed_for = Some(self.text(cx)[trig.start..].to_string()),
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Paste with an image or copied files on the clipboard attaches them
    /// (text pastes into the box as usual). True when it took the paste.
    fn paste_files(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        let mut taken = false;
        for entry in item.entries() {
            match entry {
                gpui::ClipboardEntry::Image(image) => {
                    let ext = format!("{:?}", image.format).to_lowercase();
                    let bytes = image.bytes.clone();
                    self.state
                        .update(cx, |s, cx| s.add_pasted_image(bytes, &ext, cx));
                    taken = true;
                }
                gpui::ClipboardEntry::ExternalPaths(paths) => {
                    let paths = paths.paths().to_vec();
                    self.state
                        .update(cx, |s, cx| s.add_attachment_paths(paths, cx));
                    taken = true;
                }
                // Text that is only file paths (a "Copy path", a file:// list) attaches those files.
                gpui::ClipboardEntry::String(text) => {
                    let paths = crate::attach::paths_in(text.text());
                    if !paths.is_empty() {
                        self.state
                            .update(cx, |s, cx| s.add_attachment_paths(paths, cx));
                        taken = true;
                    }
                }
            }
        }
        taken
    }

    /// Whether this click ends the press that just closed a menu (and so
    /// should do nothing). Clears the record either way.
    fn just_dismissed(&mut self, ev: &gpui::ClickEvent) -> bool {
        let at = self.dismissed_at.take();
        matches!(ev, gpui::ClickEvent::Mouse(m) if Some(m.down.position) == at)
    }

    /// The model window's keys: arrows move (wrapping, through the effort
    /// row), Enter picks, Esc clears the search, then closes.
    fn menu_key(&mut self, ev: &gpui::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.read(cx).picker_open {
            return;
        }
        let query = self.filter.read(cx).value().to_string();
        let keys = super::model_menu::keys(self.state.read(cx), &query);
        let at = self
            .active
            .as_ref()
            .and_then(|a| keys.iter().position(|k| k == a));
        match ev.keystroke.key.as_str() {
            "down" | "up" if !keys.is_empty() => {
                let n = keys.len();
                let i = match (at, ev.keystroke.key.as_str()) {
                    (None, _) => 0,
                    (Some(i), "down") => (i + 1) % n,
                    (Some(i), _) => (i + n - 1) % n,
                };
                self.active = Some(keys[i].clone());
            }
            "enter" => {
                if let Some(key) = self.active.clone() {
                    self.state.update(cx, |s, cx| {
                        if key == super::model_menu::EFFORT_ROW {
                            let levels = s
                                .active_profile()
                                .map(|p| p.effort_levels())
                                .unwrap_or_default();
                            s.effort = super::model_menu::next_effort(&levels, s.effort);
                            cx.notify();
                        } else if super::model_menu::rows(s, &query)
                            .0
                            .iter()
                            .any(|r| r.name == key && !r.disabled)
                        {
                            s.select_profile(&key, cx);
                        }
                    });
                }
            }
            "escape" => {
                if query.is_empty() {
                    self.state.update(cx, |s, cx| {
                        s.picker_open = false;
                        cx.notify();
                    });
                    self.focus(window, cx);
                } else {
                    self.filter.update(cx, |f, cx| f.set_value("", window, cx));
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let state = self.state.clone();
        let empty = self.text(cx).trim().is_empty();
        let hover = t.colors.hover;

        // A menu closes on a press outside it (each listens on itself: it
        // floats above the composer, outside the composer's own box).
        let me = cx.entity().downgrade();
        let dismiss = move |pos: gpui::Point<gpui::Pixels>, models: bool, cx: &mut gpui::App| {
            let _ = me.update(cx, |c, cx| {
                c.dismissed_at = Some(pos);
                c.state.update(cx, |s, cx| {
                    if models {
                        s.picker_open = false;
                    } else {
                        s.modes_open = false;
                        s.bypass_warning = false;
                    }
                    cx.notify();
                });
            });
        };
        let (dismiss_models, dismiss_modes) = (dismiss.clone(), dismiss);
        let me = cx.entity().downgrade();
        let skip = move |ev: &gpui::ClickEvent, cx: &mut gpui::App| -> bool {
            me.update(cx, |c, _| c.just_dismissed(ev)).unwrap_or(false)
        };

        // The pieces that read the state are built first.
        let mode_trigger = super::mode_menu::trigger(&t, &state, cx, skip);
        let modes_menu = self.state.read(cx).modes_open.then(|| {
            super::mode_menu::menu(&t, &state, window, cx, move |pos, cx| {
                dismiss_modes(pos, false, cx)
            })
        });
        let me = cx.entity().downgrade();
        let add_menu = self.add_open.then(|| {
            let (close, out) = (me.clone(), me.clone());
            super::add_menu::menu(
                &t,
                &state,
                cx,
                move |cx| {
                    let _ = close.update(cx, |c, cx| {
                        c.add_open = false;
                        cx.notify();
                    });
                },
                move |pos, cx| {
                    let _ = out.update(cx, |c, cx| {
                        c.dismissed_at = Some(pos);
                        c.add_open = false;
                        cx.notify();
                    });
                },
            )
        });
        let query = self.filter.read(cx).value().to_string();
        let search = div()
            .m(px(4.))
            .child(
                Input::new(&self.filter)
                    .appearance(false)
                    .text_size(t.text.body_sm),
            )
            .px(px(8.))
            .py(px(4.))
            .rounded(px(4.))
            .bg(t.colors.hover)
            .into_any_element();
        let this = cx.entity().downgrade();
        let on_hover = move |key: String, cx: &mut gpui::App| {
            let _ = this.update(cx, |c, cx| {
                if c.active.as_deref() != Some(key.as_str()) {
                    c.active = Some(key);
                    cx.notify();
                }
            });
        };
        let s = self.state.read(cx);
        let (busy, mode, picker_open) = (s.busy(), s.mode, s.picker_open);
        let model_menu = picker_open.then(|| {
            super::model_menu::render(
                &t,
                &state,
                s,
                &query,
                self.active.as_deref(),
                search,
                window,
                on_hover,
                move |pos, cx| dismiss_models(pos, true, cx),
            )
        });
        let model_name: SharedString = match (&s.endpoints, s.active_profile()) {
            (None, _) => "Model".into(),
            (Some(_), None) => "Set up an endpoint first".into(),
            (Some(_), Some(p)) => p.display().to_string().into(),
        };
        let pill_effort = super::model_menu::pill_effort(&t, s);
        let tint = super::mode_menu::tint(&t, mode);
        // The context meter: this conversation against the window of the
        // model picked now. Hidden when either isn't known.
        let profile = s.active_profile();
        let meter = crate::context::meter(
            s.chat()
                .and_then(|c| crate::context::last_usage(&c.messages)),
            profile.and_then(|p| p.context_window(s.health.get(&p.name))),
        )
        .map(|m| {
            (
                m,
                profile.map(|p| p.display().to_string()).unwrap_or_default(),
            )
        });

        let pending = super::attachments::pending(&t, &state, cx);
        // The list opens at the word being completed: where the caret was
        // when its sign was typed (the input's last layout), kept while the
        // word is typed on.
        let trig_start = super::autocomplete::trigger(&self.text(cx)).map(|t| t.start);
        if trig_start.is_none() {
            self.ac_anchor = None;
        } else if self.ac_anchor.map(|a| Some(a.0)) != Some(trig_start) {
            let input = self.input.read(cx);
            self.ac_anchor = input.cursor_layout().map(|(b, line)| {
                let scroll = input.scroll_offset();
                (trig_start.unwrap_or(0), b.origin + scroll, line)
            });
        }
        let caret = self.ac_anchor.map(|(_, at, line)| (at, line));
        let completion = self.completion(cx).map(|(_, rows)| {
            let me = cx.entity().downgrade();
            let me2 = me.clone();
            let selected = self.ac_selected.min(rows.len().saturating_sub(1));
            let (loading, welcome) = {
                let s = self.state.read(cx);
                (
                    s.completions_loading,
                    s.chat().is_none_or(|c| c.messages.is_empty()),
                )
            };
            let ranked =
                super::autocomplete::trigger(&self.text(cx)).is_some_and(|t| !t.query.is_empty());
            super::autocomplete::render(
                &t,
                &rows,
                selected,
                loading,
                super::autocomplete::Place {
                    caret,
                    below: welcome,
                },
                ranked,
                move |i, window, cx| {
                    let _ = me.update(cx, |c, cx| c.pick(i, window, cx));
                },
                move |i, cx| {
                    let _ = me2.update(cx, |c, cx| {
                        if c.ac_selected != i {
                            c.ac_selected = i;
                            cx.notify();
                        }
                    });
                },
            )
        });
        let has_attachments = pending.is_some();
        let send_ready = busy || !empty || has_attachments;
        // While a reply runs and you typed: the button sends it (queued, or
        // as the answer to a waiting question); Stop when the box is empty.
        let busy = busy && empty;
        // Send is the accent, pale until there is something to send; Stop
        // replaces it while a reply plays.
        let send = super::button::send_button(
            "send",
            if busy {
                super::button::SendState::Busy
            } else if send_ready {
                super::button::SendState::Ready
            } else {
                super::button::SendState::Empty
            },
            &t,
            cx.listener(move |this, _, window, cx| {
                let typed = !this.text(cx).trim().is_empty();
                if this.state.read(cx).busy() && !typed {
                    this.state.update(cx, |s, cx| s.stop(cx));
                } else {
                    this.submit(window, cx);
                }
            }),
        );

        // In the tray: quiet selectors, washed while open or hovered.
        let quiet = |id: &'static str, open: bool| super::button::selector(&t, id, open);
        let model_button = quiet("model", picker_open)
            .aria_label("Model")
            .child(icon("icons/cpu.svg", t.size.icon_sm, t.colors.text_muted))
            .child(
                div()
                    .max_w(px(220.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(t.colors.text)
                    .child(model_name),
            )
            .children(pill_effort.map(|(label, color)| {
                div()
                    .text_color(if color == t.colors.text_subtle {
                        t.colors.text_muted
                    } else {
                        color
                    })
                    .child(label)
            }))
            .child(super::button::chevron(&t, picker_open))
            .on_click(cx.listener(|this, ev: &gpui::ClickEvent, window, cx| {
                if !this.just_dismissed(ev) {
                    window.dispatch_action(Box::new(ToggleModelPicker), cx);
                }
            }));
        // The project this chat works in: opens Projects to choose another.
        let project_name: SharedString = self
            .state
            .read(cx)
            .project(self.state.read(cx).current_project)
            .filter(|p| !p.scratch)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Work in a folder".into());
        let host = self.state.read(cx).current_remote().map(|r| r.target.label());
        let remote = host.is_some();
        let project_button = quiet("project", false)
            .aria_label("Project")
            .text_color(t.colors.text_muted)
            .child(icon(
                "icons/folder.svg",
                t.size.icon_sm,
                t.colors.text_muted,
            ))
            .child(
                div()
                    .max_w(px(180.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(project_name),
            )
            .child(super::button::chevron(&t, false))
            .tooltip(move |window, cx| {
                gpui::component::tooltip::Tooltip::new(match &host {
                    Some(h) => format!("Open another folder on {h}"),
                    None => format!("Choose the project ({})", super::shortcut("Shift P")),
                })
                .build(window, cx)
            })
            // On a host, the folder's own browser there (the connection
            // stays); here, Projects.
            .on_click(move |_, window, cx| {
                if remote {
                    window.dispatch_action(Box::new(ChangeRemoteFolder), cx)
                } else {
                    window.dispatch_action(Box::new(OpenProjects), cx)
                }
            });
        let focused = gpui::Focusable::focus_handle(self.input.read(cx), cx).is_focused(window);

        let meter = meter.map(|(m, model)| {
            let f = m.fraction();
            let colour = if f > 1. {
                t.extra.danger
            } else if f > 0.8 {
                t.extra.warning
            } else {
                t.colors.text_subtle
            };
            let tip: SharedString = format!(
                "{}{} / {} context used · {model}{}",
                if m.estimated { "About " } else { "" },
                crate::context::short(m.used),
                crate::context::short(m.window),
                if f > 1. {
                    ". More than this model's window: the conversation will be compacted"
                } else {
                    ""
                },
            )
            .into();
            div()
                .id("context-meter")
                .flex()
                .items_center()
                .gap(px(5.))
                .h(t.size.button_sm)
                .px(px(6.))
                .text_size(t.text.caption)
                .text_color(colour)
                .child(super::context_ring::ring(
                    px(14.),
                    px(2.),
                    f,
                    t.colors.border,
                    if f > 0.8 { colour } else { t.colors.primary },
                ))
                // How much of the model's window the chat fills, in tokens.
                .child(format!(
                    "{}{} of {}",
                    if m.estimated { "~" } else { "" },
                    crate::context::short(m.used),
                    crate::context::short(m.window)
                ))
                .rounded(t.radius.compact)
                .cursor_pointer()
                .hover(move |st| st.bg(hover))
                .tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(format!(
                        "{tip} · click for the breakdown"
                    ))
                    .build(window, cx)
                })
                .on_click(|_, window, cx| window.dispatch_action(Box::new(super::ShowContext), cx))
        });
        let context_popover = super::context_menu::render(&t, &state, window, cx);

        let composer = div()
            .key_context("Composer")
            .track_focus(&self.menu_focus)
            .relative()
            .w_full()
            .max_w(t.size.composer_width)
            .flex()
            .flex_col()
            .gap(px(6.))
            // The text box's own Escape closes the autocomplete first, keeping the focus.
            .capture_action(
                cx.listener(|this, _: &gpui::component::input::Escape, _, cx| {
                    if let Some((trig, _)) = this.completion(cx) {
                        this.ac_closed_for = Some(this.text(cx)[trig.start..].to_string());
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            // The text box binds ↑ ↓ Tab and Enter to its own actions, which
            // run before any key listener: while the list is open they move
            // through it and pick from it instead.
            .capture_action(
                cx.listener(|this, _: &gpui::component::input::MoveUp, window, cx| {
                    if this.completion_nav("up", window, cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui::component::input::MoveDown, window, cx| {
                    if this.completion_nav("down", window, cx) {
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_action(cx.listener(
                |this, _: &gpui::component::input::IndentInline, window, cx| {
                    if this.completion_nav("tab", window, cx) {
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_action(
                cx.listener(|this, e: &gpui::component::input::Enter, window, cx| {
                    if !e.secondary && !e.shift && this.completion_nav("enter", window, cx) {
                        // Stopped here, the box never sends: nothing to swallow later.
                        this.ac_took_enter = false;
                        cx.stop_propagation();
                    }
                }),
            )
            // The text box's own Paste: an image or files are attached instead.
            .capture_action(
                cx.listener(|this, _: &gpui::component::input::Paste, _, cx| {
                    if this.paste_files(cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| {
                if this.completion_key(ev, window, cx) {
                    return cx.stop_propagation();
                }
                this.menu_key(ev, window, cx)
            }))
            .children(model_menu)
            // Painted last, over the cards under the box on the new-chat screen.
            .children(completion.map(|c| gpui::deferred(c).with_priority(2)))
            // One unit: the white box (what you write, + on the left, send on
            // the right) resting in a recessed tray that holds the selectors.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(t.size.composer_radius)
                    .bg(t.extra.sunk)
                    .border_1()
                    .border_color(t.colors.border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .bg(t.colors.surface)
                            .border_1()
                            // The mode tints the box's edge, as it tints the extension's
                            // focus ring; the keyboard deepens it.
                            .border_color(tint.map(|c| c.opacity(0.55)).unwrap_or(if focused {
                                t.extra.border_strong
                            } else {
                                t.colors.border
                            }))
                            .rounded(t.size.composer_radius)
                            // The tray's edge meets the box's: no double hairline.
                            .m(px(-1.))
                            // Files dropped anywhere on the box are attached.
                            .drag_over::<gpui::ExternalPaths>({
                                let (edge, wash) = (t.colors.primary, t.colors.primary_tint);
                                move |d, _, _, _| d.border_color(edge).bg(wash)
                            })
                            .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                                let paths = paths.paths().to_vec();
                                this.state
                                    .update(cx, |s, cx| s.add_attachment_paths(paths, cx));
                            }))
                            .children(pending)
                            .child(
                                // The text starts where the + under it starts
                                // (the textarea adds 10px of its own).
                                div()
                                    .px(px(4.))
                                    .pt(t.space.md)
                                    .pb(t.space.xs)
                                    .text_size(t.text.body)
                                    .line_height(relative(1.5))
                                    .child(
                                        Textarea::new(&self.input)
                                            .appearance(false)
                                            .text_size(t.text.body)
                                            .token({
                                                let t = t.clone();
                                                move |token, _, _| mention_token(&t, token)
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(t.space.sm)
                                    // Equal insets right and below, so send sits
                                    // square in the box's corner.
                                    .p(t.space.snug)
                                    // "+": what to add, and the tools a reply may use.
                                    .child(
                                        div()
                                            .relative()
                                            .child(
                                                super::button::Button::outlined_icon(
                                                    "attach",
                                                    "icons/plus.svg",
                                                    "Add files, connectors and more",
                                                )
                                                .large()
                                                .on_click(cx.listener(|this, ev, _, cx| {
                                                    if this.just_dismissed(ev) {
                                                        return;
                                                    }
                                                    this.add_open = !this.add_open;
                                                    cx.notify();
                                                })),
                                            )
                                            .children(add_menu),
                                    )
                                    .child(div().flex_1())
                                    .child(send),
                            ),
                    )
                    // The tray: the project, the context meter, the model, the mode.
                    .child(
                        div()
                            .relative()
                            .children(context_popover)
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .px(t.space.tight)
                            .py(t.space.xs)
                            .child(project_button)
                            .children(meter)
                            .child(div().flex_1())
                            .child(model_button)
                            // Modes: the button and its menu above it.
                            .child(div().relative().child(mode_trigger).children(modes_menu)),
                    ),
            );
        // Never wider than its own measure, centred wherever it sits.
        div().w_full().flex().justify_center().child(composer)
    }
}

/// The finished mentions in `text`, by byte range: a `/name` ForgeCLI runs,
/// leading the message, and `@path` naming one of the project's files or
/// folders. Each token's id says its kind (`skill:`, `command:`, `file:`).
fn mentions(text: &str, s: &AppState) -> Vec<(std::ops::Range<usize>, InlineToken)> {
    let mut out = vec![];
    let finished = |end: usize| text[end..].starts_with(char::is_whitespace);
    if let Some(rest) = text.strip_prefix('/') {
        let end = 1 + rest.find(char::is_whitespace).unwrap_or(rest.len());
        let name = &text[1..end];
        if finished(end) {
            if let Some(c) = s.completion_commands().into_iter().find(|c| c.name == name) {
                let kind = if c.group == "Skills" { "skill" } else { "command" };
                out.push((0..end, InlineToken::new(format!("{kind}:{name}"), &text[..end])));
            }
        }
    }
    let files = s.completion_files();
    let mut at = 0;
    for word in text.split_inclusive(char::is_whitespace) {
        let start = at;
        at += word.len();
        let word = word.trim_end();
        let Some(path) = word.strip_prefix('@') else {
            continue;
        };
        let end = start + word.len();
        if path.is_empty() || !finished(end) {
            continue;
        }
        let known = files
            .iter()
            .any(|f| f == path || f.trim_end_matches('/') == path.trim_end_matches('/'));
        if known {
            out.push((start..end, InlineToken::new(format!("file:{path}"), word)));
        }
    }
    out
}

/// How a mention shows in the box: its text, bold; a skill in the skills'
/// colour, a command or a file in the accent.
fn mention_token(t: &crate::theme::Theme, token: &InlineTokenContext) -> gpui::Div {
    let skill = token.token().id().starts_with("skill:");
    div()
        .flex()
        .items_center()
        .h(token.line_height())
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(if skill { t.extra.skill } else { t.colors.primary })
        .when(token.is_selected(), |d| {
            d.bg(t.colors.primary_tint).rounded(t.radius.small)
        })
        .child(token.token().label().clone())
}
