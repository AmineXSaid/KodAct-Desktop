//! The composer, one surface as Claude's is: a lifted box holding a
//! multi-line input that grows up to a limit, and under it one row: the +
//! (attach and tools) and the project on the left, the context meter, the
//! model picker (a popover) and the Modes menu on the right, then Send
//! (Stop while a reply plays). Enter sends; Shift+Enter adds a line.

use gpui::component::input::{
    Editor, EditorState, Input, InputEvent, InputState, TextDecoration, TextDecorationCollection,
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
/// `asking`: a question card (`Some(true)`) or a permission card waits;
/// `in_chat`: the conversation has begun, so the box replies.
fn placeholder(in_folder: bool, asking: Option<bool>, busy: bool, in_chat: bool) -> &'static str {
    if asking == Some(true) {
        "Type your own answer…"
    } else if asking.is_some() {
        "Or tell KodAct what to do instead…"
    } else if busy {
        "Add to the task, or steer KodAct…"
    } else if in_chat {
        "Reply, or type / for commands"
    } else if in_folder {
        "What should we build today?"
    } else {
        "Ask KodAct anything"
    }
}

/// The completion list as last shown: its rows, the one selected, whether
/// more are loading, where it sits, and whether the rows are ranked.
type AcShown = (
    std::rc::Rc<Vec<super::autocomplete::Item>>,
    usize,
    bool,
    super::autocomplete::Place,
    bool,
);

pub struct Composer {
    state: Entity<AppState>,
    input: Entity<EditorState>,
    /// The Markdown and mentions styled in the box (`compose_style`).
    styling: TextDecorationCollection,
    /// The tallest the box grows, in lines.
    max_rows: usize,
    /// The model window's search box, and its row under the keyboard.
    filter: Entity<InputState>,
    /// The design menu's search box, and the tab it was cleared for.
    design_filter: Entity<InputState>,
    design_tab_was: Option<crate::state::DesignTab>,
    active: Option<String>,
    picker_was_open: bool,
    /// The model, mode and + menus, kept while each leaves.
    model_presence: super::motion::Presence<()>,
    modes_presence: super::motion::Presence<()>,
    add_presence: super::motion::Presence<()>,
    design_presence: super::motion::Presence<crate::state::DesignTab>,
    /// Where the design chip's top sits in the window (measured each frame),
    /// so its menu fits the room above it and never covers the chip.
    design_chip_top: std::rc::Rc<std::cell::Cell<f32>>,
    project_presence: super::motion::Presence<()>,
    context_presence: super::motion::Presence<()>,
    ac_presence: super::motion::Presence<AcShown>,
    /// Where the press that just closed a menu happened: the click it ends
    /// in, on that menu's own button, must not open it again.
    dismissed_at: Option<(Menu, gpui::Point<gpui::Pixels>)>,
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
    placeholder_now: (bool, Option<bool>, bool, bool, Option<SharedString>),
    /// The chat column is narrow (a panel open beside it): the tray's
    /// folder and model show as icons, their names in tooltips. Set by the
    /// workspace from the column's width each frame.
    pub narrow: bool,
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
            // An editor, not a plain textarea: only an editor styles ranges
            // of its text, which Markdown as you type needs. No line numbers,
            // no brackets closed for you, wrapped like prose.
            EditorState::new(window, cx)
                .line_number(false)
                .folding(false)
                .auto_close(false)
                .soft_wrap(true)
                .placeholder(placeholder(in_folder, None, false, false))
                .submit_on_enter(true)
        });
        let styling = input.update(cx, |i, cx| i.create_decorations_collection(vec![], cx));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search models…"));
        let design_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search the design library…"));
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                // An edited message comes back here to be changed and sent.
                if let Some(text) = state.update(cx, |s, _| s.draft.take()) {
                    this.fill(&text, window, cx);
                }
                // A plain chat asks for anything; a project's, for work on it.
                // While a question waits, what you type answers it.
                let s = state.read(cx);
                let in_chat = s.chat().is_some_and(|c| !c.messages.is_empty());
                // On a project's page the box starts a chat there, and says so.
                let page = match s.screen {
                    crate::state::Screen::Project(id) => s.project(id).map(|p| p.name.clone()),
                    _ => None,
                };
                let now = (
                    s.in_folder(),
                    s.asking().then(|| s.asking_question()),
                    s.busy(),
                    in_chat,
                    page,
                );
                if now != this.placeholder_now {
                    let line: SharedString = match &now.4 {
                        Some(name) if !now.3 => format!("New session in {name}").into(),
                        _ => placeholder(now.0, now.1, now.2, now.3).into(),
                    };
                    this.placeholder_now = now;
                    this.input
                        .update(cx, |i, cx| i.set_placeholder(line, window, cx));
                }
                // The model window opening: an empty search, the current row active.
                let s = state.read(cx);
                let (open, current) = (s.picker_open, s.active_profile().map(|p| p.name.clone()));
                // As the menu decides whether it shows its search box.
                let rows = super::model_menu::rows(s, "").1;
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
                // The design menu on a new tab: an empty search.
                let tab = state.read(cx).design_menu;
                if tab != this.design_tab_was {
                    let was = std::mem::replace(&mut this.design_tab_was, tab);
                    this.design_filter
                        .update(cx, |f, cx| f.set_value("", window, cx));
                    // A list's search takes the keys; closing gives them back.
                    match tab {
                        Some(crate::state::DesignTab::Work) => {}
                        Some(_) => this.design_filter.update(cx, |f, cx| f.focus(window, cx)),
                        None if was.is_some() => this.input.update(cx, |i, cx| i.focus(window, cx)),
                        None => {}
                    }
                }
                cx.notify();
            }),
            cx.subscribe_in(&design_filter, window, |_, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
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
                        this.restyle(cx);
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
            design_filter,
            design_tab_was: None,
            active: None,
            picker_was_open: false,
            model_presence: Default::default(),
            ac_presence: Default::default(),
            modes_presence: Default::default(),
            add_presence: Default::default(),
            design_presence: Default::default(),
            design_chip_top: Default::default(),
            project_presence: Default::default(),
            context_presence: Default::default(),
            dismissed_at: None,
            add_open: false,
            styling,
            max_rows,
            menu_focus: cx.focus_handle(),
            ac_selected: 0,
            ac_closed_for: None,
            ac_took_enter: false,
            ac_anchor: None,
            narrow: false,
            placeholder_now: (in_folder, None, false, false, None),
            _subs: subs,
        }
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
        self.restyle(cx);
        cx.notify();
    }

    /// Style the box's Markdown and mentions as they stand now: a finished
    /// `/command` leading the message or `@path` naming a project file reads
    /// as it will once sent (a skill in its own colour), and **bold**,
    /// *italic*, `code`, headings, quotes, lists and links read as such.
    fn restyle(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        let marks = super::autocomplete::marks(&text, self.state.read(cx), false);
        let t = cx.theme().clone();
        let decorations = super::compose_style::styles(&text, &marks, &t)
            .into_iter()
            .map(|(r, style)| TextDecoration::new(r, style))
            .collect();
        self.styling.set(decorations, cx);
    }

    /// What the "+" menu needs from the composer: closing it, and a press
    /// outside.
    fn add_hooks(&self, cx: &mut Context<Self>) -> super::add_menu::Hooks {
        let me = cx.entity().downgrade();
        let (close, out) = (me.clone(), me);
        super::add_menu::Hooks {
            close: std::rc::Rc::new(move |cx| {
                let _ = close.update(cx, |c, cx| {
                    c.add_open = false;
                    cx.notify();
                });
            }),
            on_out: std::rc::Rc::new(move |pos, cx| {
                let _ = out.update(cx, |c, cx| {
                    c.dismissed_at = Some((Menu::Add, pos));
                    c.add_open = false;
                    cx.notify();
                });
            }),
        }
    }

    fn text(&self, cx: &gpui::App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.ac_took_enter) {
            return;
        }
        let text = self.text(cx);
        // A question waits: what you typed answers a question card, or
        // denies a permission card and says what to do instead.
        if self.state.read(cx).asking()
            && !text.trim().is_empty()
            && crate::state::parse_btw(&text).is_none()
        {
            self.state
                .update(cx, |s, cx| s.answer_in_words(text.trim().to_string(), cx));
            self.input.update(cx, |i, cx| i.set_value("", window, cx));
            return;
        }
        // While a reply runs, a message is queued for KodAct's next step
        // (`/btw` keeps its side card).
        if self.state.read(cx).busy() && crate::state::parse_btw(&text).is_none() {
            // Words, pictures or pasted texts: each is a message for the next step.
            if !text.trim().is_empty() || !self.state.read(cx).attachments.is_empty() {
                self.state
                    .update(cx, |s, cx| s.queue_message(text.trim().to_string(), cx));
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

    /// A paste too long for the box (over `attach::PASTE_CHARS` characters
    /// or `PASTE_LINES` lines: a log, a file, a transcript) becomes a Pasted
    /// text card above it, as in Claude's chat box. Shorter text goes in as
    /// it is. True when it took the paste.
    fn paste_long(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return false;
        };
        if !crate::attach::is_long_paste(&text) {
            return false;
        }
        self.state.update(cx, |s, cx| s.add_pasted_text(text, cx));
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

    /// Whether this click ends the press that just closed `menu` (and so
    /// should do nothing). Clears the record either way.
    fn just_dismissed(&mut self, menu: Menu, ev: &gpui::ClickEvent) -> bool {
        let at = self.dismissed_at.take();
        matches!((at, ev), (Some((m, pos)), gpui::ClickEvent::Mouse(ev)) if m == menu && ev.down.position == pos)
    }

    /// Esc in the box: close whichever menu floats over it. True when one was open.
    fn close_menus(&mut self, cx: &mut Context<Self>) -> bool {
        let s = self.state.read(cx);
        let open = self.add_open
            || s.modes_open
            || s.design_menu.is_some()
            || s.project_menu
            || s.context_open
            || s.picker_open;
        if open {
            self.add_open = false;
            self.state.update(cx, |s, cx| {
                s.modes_open = false;
                s.bypass_warning = false;
                s.design_menu = None;
                s.project_menu = false;
                s.context_open = false;
                s.picker_open = false;
                cx.notify();
            });
            cx.notify();
        }
        open
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
                        } else if let Some(r) = super::model_menu::rows(s, &query)
                            .0
                            .into_iter()
                            .find(|r| r.name == key && !r.disabled)
                        {
                            match r.pick {
                                Some((profile, id)) => s.pick_catalog_model(&profile, &id, cx),
                                None => s.select_profile(&key, cx),
                            }
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
        // The box grows with what you write, up to `max_rows` lines, then
        // scrolls. An editor takes the height it is given, so count the
        // lines as the text wraps at the box's width.
        let box_height = {
            let t = cx.theme().clone();
            let input = self.input.read(cx);
            let width = input.input_bounds().size.width;
            let text = input.value();
            let rows = if width > px(40.) && !text.is_empty() {
                let run = gpui::TextRun {
                    len: text.len(),
                    font: gpui::font(t.font_sans.clone()),
                    color: t.colors.text,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_text(text.clone(), t.text.body, &[run], Some(width), None)
                    .map(|lines| {
                        lines
                            .iter()
                            .map(|l| l.wrap_boundaries().len() + 1)
                            .sum::<usize>()
                    })
                    .unwrap_or(1)
            } else {
                text.split('\n').count()
            };
            // Two lines at rest on the home page, room to start writing;
            // one in a chat, where the reply is the page.
            let home = !self
                .state
                .read(cx)
                .chat()
                .is_some_and(|c| !c.messages.is_empty());
            let least = if home { 2 } else { 1 };
            // The editor's own inset above and below its lines.
            t.text.body * 1.5 * rows.clamp(least, self.max_rows.max(least)) as f32 + t.space.sm
        };
        let t = cx.theme().clone();
        let state = self.state.clone();
        let empty = self.text(cx).trim().is_empty();
        let hover = t.colors.hover;

        // A menu closes on a press outside it (each listens on itself: it
        // floats above the composer, outside the composer's own box).
        let me = cx.entity().downgrade();
        let dismiss = move |pos: gpui::Point<gpui::Pixels>, menu: Menu, cx: &mut gpui::App| {
            let _ = me.update(cx, |c, cx| {
                let is_open = {
                    let s = c.state.read(cx);
                    match menu {
                        Menu::Models => s.picker_open,
                        Menu::Modes => s.modes_open,
                        Menu::Design => s.design_menu.is_some(),
                        Menu::Project => s.project_menu,
                        Menu::Context => s.context_open,
                        Menu::Add => c.add_open,
                    }
                };
                if !is_open {
                    return;
                }
                c.dismissed_at = Some((menu, pos));
                if menu == Menu::Add {
                    c.add_open = false;
                }
                c.state.update(cx, |s, cx| {
                    match menu {
                        Menu::Models => s.picker_open = false,
                        Menu::Modes => {
                            s.modes_open = false;
                            s.bypass_warning = false;
                        }
                        Menu::Design => s.design_menu = None,
                        Menu::Project => s.project_menu = false,
                        Menu::Context => s.context_open = false,
                        Menu::Add => {}
                    }
                    cx.notify();
                });
            });
        };
        let (dismiss_models, dismiss_modes, dismiss_design, dismiss_project, dismiss_context) = (
            dismiss.clone(),
            dismiss.clone(),
            dismiss.clone(),
            dismiss.clone(),
            dismiss,
        );
        let skip_for = |menu: Menu| {
            let me = cx.entity().downgrade();
            move |ev: &gpui::ClickEvent, cx: &mut gpui::App| -> bool {
                me.update(cx, |c, _| {
                    let skipped = c.just_dismissed(menu, ev);
                    if !skipped {
                        c.add_open = false;
                    }
                    skipped
                })
                .unwrap_or(false)
            }
        };
        let skip_modes = skip_for(Menu::Modes);
        let skip_design = skip_for(Menu::Design);
        let skip_project = skip_for(Menu::Project);
        let skip_context = skip_for(Menu::Context);

        // The pieces that read the state are built first.
        let design_trigger = super::design_chips::trigger(&t, &state, cx, skip_design);
        let design_now = self.state.read(cx).design_menu;
        let design_out = t.size.menu_out_ms;
        let design_menu = self
            .design_presence
            .frame(design_now, design_out, window)
            .map(|(tab, leaving)| {
                let query = self.design_filter.read(cx).value().to_string();
                let search = super::search_field(&t, &self.design_filter)
                    .mx(t.space.sm)
                    .mb(t.space.xs)
                    .into_any_element();
                // The room above the chip, less the window's margin and the
                // gap the menu keeps from its chip.
                let room = gpui::px(
                    (self.design_chip_top.get()
                        - f32::from(t.size.viewport_margin)
                        - f32::from(t.space.sm))
                    .max(160.),
                );
                super::design_chips::menu(
                    &t,
                    &state,
                    tab,
                    &query,
                    search,
                    room,
                    window,
                    cx,
                    move |pos, cx| dismiss_design(pos, Menu::Design, cx),
                    leaving,
                )
            });
        let mode_trigger = super::mode_menu::trigger(&t, &state, cx, skip_modes);
        let out = t.size.menu_out_ms;
        let modes_now = self.state.read(cx).modes_open.then_some(());
        let modes_menu = self
            .modes_presence
            .frame(modes_now, out, window)
            .map(|(_, leaving)| {
                super::mode_menu::menu(
                    &t,
                    &state,
                    window,
                    cx,
                    move |pos, cx| dismiss_modes(pos, Menu::Modes, cx),
                    leaving,
                )
            });
        // The "+": a menu above it (`add_menu`).
        let add_hooks = self.add_hooks(cx);
        let add_menu = self
            .add_presence
            .frame(self.add_open.then_some(()), out, window)
            .and_then(|(_, leaving)| {
                super::add_menu::menu(&t, &state, cx, add_hooks.clone(), leaving)
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
            .rounded(t.radius.button)
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
        let model_frame =
            self.model_presence
                .frame(self.state.read(cx).picker_open.then_some(()), out, window);
        let s = self.state.read(cx);
        let (busy, mode, picker_open) = (s.busy(), s.mode, s.picker_open);
        let model_menu = model_frame.map(|(_, leaving)| {
            super::model_menu::render(
                &t,
                &state,
                s,
                &query,
                self.active.as_deref(),
                search,
                window,
                on_hover,
                move |pos, cx| dismiss_models(pos, Menu::Models, cx),
                leaving,
            )
        });
        let model_name: SharedString = match (&s.endpoints, s.active_profile()) {
            (None, _) => "Model".into(),
            // No endpoint yet: an invitation, not an alarm; the menu sets one up.
            (Some(_), None) => "Choose a model".into(),
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
        let queued_dock = {
            let s = self.state.read(cx);
            s.chat().and_then(|c| {
                let list = s.queued.get(&c.id).filter(|q| !q.is_empty())?;
                let collapsed = s.ui_toggles.contains("queued-collapsed");
                Some(super::queued::render(&t, &state, c.id, list, collapsed))
            })
        };
        let goal = {
            let s = self.state.read(cx);
            s.current
                .and_then(|c| s.goals.get(&c))
                .map(|g| super::goal_bar::render(&t, &state, g, s.busy()))
        };
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
        let ac_now = self.completion(cx).map(|(_, rows)| {
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
            let place = super::autocomplete::Place {
                caret,
                below: welcome,
            };
            (std::rc::Rc::new(rows), selected, loading, place, ranked)
        });
        // Kept as it was for the beat it takes to leave.
        let ac_frame = self.ac_presence.frame(ac_now, t.size.menu_out_ms, window);
        let completion = ac_frame.map(|((rows, selected, loading, place, ranked), leaving)| {
            let me = cx.entity().downgrade();
            let me2 = me.clone();
            super::autocomplete::render(
                &t,
                &rows,
                selected,
                loading,
                place,
                ranked,
                leaving,
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
        // Narrow (a panel open beside the chat), the names give way first.
        let model_button = quiet("model", picker_open)
            .min_w_0()
            .flex_shrink(1.)
            .aria_label(SharedString::from(format!("Model: {model_name}")))
            .child(icon("icons/model.svg", t.size.icon_sm, t.colors.text_muted))
            .when(!self.narrow, |d| {
                d.child(
                    div()
                        .min_w_0()
                        .max_w(px(220.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(t.colors.text)
                        .child(model_name.clone()),
                )
            })
            .when(self.narrow && !picker_open, |d| {
                let tip = model_name.clone();
                d.tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
            })
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
                if !this.just_dismissed(Menu::Models, ev) {
                    this.add_open = false;
                    window.dispatch_action(Box::new(ToggleModelPicker), cx);
                }
            }));
        // The project this chat works in: its menu lists the others, All
        // projects and New project…, above the chip (the title bar's
        // switcher, lifted).
        let project_now = self.state.read(cx).project_menu.then_some(());
        let project_menu = self
            .project_presence
            .frame(project_now, t.size.menu_out_ms, window)
            .map(|(_, leaving)| {
                let body = super::workspace::switcher_menu(&state, window, cx, px(140.)).when(
                    !leaving,
                    |d| {
                        d.on_mouse_down_out(move |ev, _, cx| {
                            dismiss_project(ev.position, Menu::Project, cx)
                        })
                    },
                );
                div().absolute().top_0().left_0().child(
                    gpui::deferred(
                        gpui::anchored()
                            .anchor(gpui::Anchor::BottomLeft)
                            .snap_to_window_with_margin(t.size.viewport_margin)
                            .child(div().pb(t.space.sm).child(super::motion::composer_menu(
                                body, "project", 0., leaving, &t,
                            ))),
                    )
                    .with_priority(2),
                )
            });
        let project_open = project_now.is_some();
        let project_name: SharedString = self
            .state
            .read(cx)
            .project(self.state.read(cx).current_project)
            .filter(|p| !p.scratch)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Work in a folder".into());
        let host = self
            .state
            .read(cx)
            .current_remote()
            .map(|r| r.target.label());
        let project_button = quiet("project", project_open)
            .min_w_0()
            .flex_shrink(1.)
            .aria_label(SharedString::from(format!("Project: {project_name}")))
            .text_color(t.colors.text_muted)
            .child(icon(
                "icons/folder.svg",
                t.size.icon_sm,
                t.colors.text_muted,
            ))
            .when(!self.narrow, |d| {
                d.child(
                    div()
                        .min_w_0()
                        .max_w(px(180.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(project_name.clone()),
                )
            })
            .child(super::button::chevron(&t, project_open))
            // No tooltip over its own open menu.
            .when(!project_open, |d| {
                let name = project_name.clone();
                let narrow = self.narrow;
                d.tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(match (&host, narrow) {
                        (_, true) => format!("{name}: choose the project"),
                        (Some(h), false) => format!("Projects, or another folder on {h}"),
                        (None, false) => "Choose the project".to_string(),
                    })
                    .build(window, cx)
                })
            })
            .on_click({
                let state = state.clone();
                move |ev, _, cx| {
                    if !skip_project(ev, cx) {
                        state.update(cx, |s, cx| {
                            s.project_menu = !s.project_menu;
                            if s.project_menu {
                                s.picker_open = false;
                                s.modes_open = false;
                                s.bypass_warning = false;
                                s.design_menu = None;
                                s.context_open = false;
                                s.title_menu = None;
                                s.title_menu_dismissed = None;
                            }
                            cx.notify();
                        })
                    }
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
            // The ring alone, as Claude Code's; its numbers in the tooltip.
            div()
                .id("context-meter")
                .flex()
                .items_center()
                .justify_center()
                .size(t.size.button_sm)
                .text_size(t.text.caption)
                .text_color(colour)
                .child(super::context_ring::ring(
                    px(14.),
                    px(2.),
                    f,
                    t.colors.border,
                    if f > 0.8 { colour } else { t.colors.primary },
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
                .on_click(move |ev, window, cx| {
                    if !skip_context(ev, cx) {
                        window.dispatch_action(Box::new(super::ShowContext), cx);
                    }
                })
        });
        let context_shown = self
            .context_presence
            .frame(self.state.read(cx).context_open.then_some(()), out, window)
            .map(|f| f.1);
        let context_popover =
            super::context_menu::render(&t, &state, window, context_shown, cx, move |pos, cx| {
                dismiss_context(pos, Menu::Context, cx)
            });

        // In a chat the box is as wide as the text above it, on the same
        // edges; on the new-chat screen it keeps its own width.
        let in_chat = self
            .state
            .read(cx)
            .chat()
            .is_some_and(|c| !c.messages.is_empty());
        let composer = div()
            .key_context("Composer")
            .track_focus(&self.menu_focus)
            .relative()
            .w_full()
            .max_w(if in_chat {
                t.size.column
            } else {
                t.size.composer_width
            })
            .flex()
            .flex_col()
            .gap(px(6.))
            // Ctrl+F finds in the chat (`find_bar.rs`), not in the text box:
            // the editor's own search bar would open inside the composer.
            .capture_action(
                cx.listener(|_, _: &gpui::component::input::Search, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(super::ToggleFind), cx);
                }),
            )
            // The text box's own Escape closes the autocomplete first, then
            // a menu open over the box (+, modes), keeping the focus.
            .capture_action(
                cx.listener(|this, _: &gpui::component::input::Escape, _, cx| {
                    if let Some((trig, _)) = this.completion(cx) {
                        this.ac_closed_for = Some(this.text(cx)[trig.start..].to_string());
                        cx.stop_propagation();
                        cx.notify();
                    } else if this.close_menus(cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            // Focus on a chip instead of the box: Esc arrives as StopReply,
            // and a menu open over the box closes before a reply stops.
            .capture_action(cx.listener(|this, _: &super::StopReply, _, cx| {
                if this.close_menus(cx) {
                    cx.stop_propagation();
                }
            }))
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
            // Shift+Tab cycles the mode, as in Claude Code. The text box binds
            // it to outdent, and its binding wins over ours: take it here.
            .capture_action(cx.listener(
                |_, _: &gpui::component::input::OutdentInline, window, cx| {
                    window.dispatch_action(Box::new(super::CycleMode), cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(cx.listener(
                |this, _: &gpui::component::input::IndentInline, window, cx| {
                    if this.completion_nav("tab", window, cx) {
                        cx.stop_propagation();
                    } else if this.input.read(cx).value().is_empty() {
                        // Nothing typed: Tab leaves the box for the next
                        // control, so rows, pills and buttons are reachable
                        // from the keyboard (the workspace walks on from there).
                        window.focus_next(cx);
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
                        return;
                    }
                    // Nothing written and a tool waits: Enter allows it,
                    // Shift Enter always allows it (the card's keys).
                    if !e.secondary
                        && this.text(cx).trim().is_empty()
                        && this.state.read(cx).attachments.is_empty()
                    {
                        let verdict = if e.shift {
                            crate::state::Verdict::AllowedAlways
                        } else {
                            crate::state::Verdict::Allowed
                        };
                        if this.state.update(cx, |s, cx| s.answer_waiting(verdict, cx)) {
                            cx.stop_propagation();
                        }
                    }
                }),
            )
            // The text box's own Paste: an image or files are attached instead.
            .capture_action(
                cx.listener(|this, _: &gpui::component::input::Paste, _, cx| {
                    if this.paste_files(cx) || this.paste_long(cx) {
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
            // One surface: what you write, then one row with everything
            // about it. It lifts off the page; the mode tints its edge, as it
            // tints the extension's focus ring, and the keyboard deepens it.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(goal)
                    .children(queued_dock.map(|q| div().mb(px(6.)).child(q)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .bg(t.colors.surface)
                            .border_1()
                            .border_color(tint.map(|c| c.opacity(0.55)).unwrap_or(if focused {
                                t.extra.border_strong
                            } else {
                                t.colors.border
                            }))
                            .rounded(t.size.composer_radius)
                            .shadow(t.elevation_grain())
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
                            // One card, as Claude's box: what you write, the
                            // full width; under it, inside the card, one row
                            // with everything about it: + and the mode, the
                            // folder and the design on the left; the model,
                            // the context and send on the right.
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .p(t.space.xs)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            // What you write, the full width of the card.
                                            .pt(t.space.sm)
                                            .px(t.space.sm)
                                            .min_h(t.size.composer_line)
                                            .text_size(t.text.body)
                                            .line_height(relative(1.5))
                                            .child(
                                                Editor::new(&self.input)
                                                    .appearance(false)
                                                    .font_family(t.font_sans.clone())
                                                    .text_size(t.text.body)
                                                    .line_height(relative(1.5))
                                                    .h(box_height),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(t.space.xs)
                                            .min_w_0()
                                            // "+": what to add, and the tools a reply may use.
                                            .child(
                                                div()
                                                    .relative()
                                                    .mb(px(2.))
                                                    .child(
                                                        super::button::Button::icon(
                                                            "attach",
                                                            "icons/plus.svg",
                                                            "Add files, connectors and more",
                                                        )
                                                        // Its name, not over its own open menu.
                                                        .tooltip(!self.add_open)
                                                        .on_click(cx.listener(|this, ev, _, cx| {
                                                            if this.just_dismissed(Menu::Add, ev) {
                                                                return;
                                                            }
                                                            this.add_open = !this.add_open;
                                                            if this.add_open {
                                                                this.state.update(cx, |s, cx| {
                                                                    s.picker_open = false;
                                                                    s.modes_open = false;
                                                                    s.bypass_warning = false;
                                                                    s.design_menu = None;
                                                                    s.project_menu = false;
                                                                    s.context_open = false;
                                                                    s.title_menu = None;
                                                                    s.title_menu_dismissed = None;
                                                                    cx.notify();
                                                                });
                                                            }
                                                            cx.notify();
                                                        })),
                                                    )
                                                    .children(add_menu),
                                            )
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .relative()
                                                    .child(mode_trigger)
                                                    .children(modes_menu),
                                            )
                                            .child(
                                                div()
                                                    .relative()
                                                    .min_w_0()
                                                    .flex_shrink(1.)
                                                    .child(project_button)
                                                    .children(project_menu),
                                            )
                                            // A design chat: the chip and its menu above it.
                                            .children(design_trigger.map(|d| {
                                                let top = self.design_chip_top.clone();
                                                div()
                                                    .relative()
                                                    .flex_none()
                                                    .child(d)
                                                    // Its top in the window, for the menu's room.
                                                    .child(
                                                        gpui::canvas(
                                                            move |bounds, _, _| {
                                                                top.set(f32::from(bounds.origin.y))
                                                            },
                                                            |_, _, _, _| {},
                                                        )
                                                        .absolute()
                                                        .size_full(),
                                                    )
                                                    .children(design_menu)
                                            }))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w(t.space.xs)
                                                    .relative()
                                                    .children(context_popover),
                                            )
                                            .child(model_button)
                                            .children(meter)
                                            .child(send),
                                    ),
                            ),
                    ),
            );
        // Never wider than its own measure, centred wherever it sits.
        div().w_full().flex().justify_center().child(composer)
    }
}

/// The composer's menus, for closing the one a press fell outside of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Models,
    Modes,
    Design,
    Project,
    Add,
    Context,
}
