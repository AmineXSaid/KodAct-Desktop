//! The command palette (Ctrl/⌘K): every action, every chat, every model and
//! mode, fuzzy-filtered as you type. Arrow keys move, Enter runs, Esc closes.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, font, prelude::*, px, Action, AnimationExt, Context, Entity, FontWeight, IntoElement, KeyDownEvent, Render, SharedString,
    StyledText, Subscription, TextRun, Window,
};

use super::*;
use crate::state::{AppState, Mode, MODELS};
use crate::theme::{self, ActiveTheme, Theme};

enum Target {
    Action(Box<dyn Action>),
    Chat(usize),
    Model(usize),
    Mode(Mode),
}

struct Command {
    label: SharedString,
    group: &'static str,
    icon: &'static str,
    hint: Option<SharedString>,
    target: Target,
}

pub struct CommandPalette {
    state: Entity<AppState>,
    query: Entity<InputState>,
    selected: usize,
    _subs: Vec<Subscription>,
}

/// Fuzzy match: every query character in order. Higher is better: matches at
/// word starts and in runs score more. Returns the matched byte positions.
pub fn fuzzy(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, vec![]));
    }
    let lower: Vec<(usize, char)> = text.char_indices().map(|(i, c)| (i, c.to_ascii_lowercase())).collect();
    let mut positions = vec![];
    let mut score = 0;
    let mut at = 0;
    let mut prev: Option<usize> = None;
    for q in query.chars().map(|c| c.to_ascii_lowercase()).filter(|c| !c.is_whitespace()) {
        let found = (at..lower.len()).find(|&k| lower[k].1 == q)?;
        let word_start = found == 0 || !lower[found - 1].1.is_alphanumeric();
        score += 1 + if word_start { 8 } else { 0 } + if prev == Some(found.wrapping_sub(1)) { 5 } else { 0 };
        positions.push(lower[found].0);
        prev = Some(found);
        at = found + 1;
    }
    // Shorter labels win ties.
    Some((score * 10 - text.len() as i32 / 4, positions))
}

impl CommandPalette {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search commands, chats, models…"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&query, window, |this, _, ev: &InputEvent, _, cx| match ev {
                InputEvent::Change => {
                    this.selected = 0;
                    cx.notify();
                }
                // Enter is handled once, in the key capture below.
                _ => {}
            }),
        ];
        Self { state, query, selected: 0, _subs: subs }
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = 0;
        self.query.update(cx, |q, cx| {
            q.set_value("", window, cx);
            q.focus(window, cx);
        });
        cx.notify();
    }

    fn commands(&self, cx: &gpui::App) -> Vec<Command> {
        let s = self.state.read(cx);
        let act = |label: &str, icon: &'static str, hint: Option<SharedString>, action: Box<dyn Action>| Command {
            label: label.to_string().into(),
            group: "Actions",
            icon,
            hint,
            target: Target::Action(action),
        };
        let mut out = vec![
            act("New chat", "icons/plus.svg", Some(shortcut("N")), Box::new(NewChat)),
            act("Toggle sidebar", "icons/panel-left.svg", Some(shortcut("B")), Box::new(ToggleSidebar)),
            act(if s.dark { "Switch to light theme" } else { "Switch to dark theme" }, if s.dark { "icons/sun.svg" } else { "icons/moon.svg" }, Some(shortcut("Shift L")), Box::new(ToggleTheme)),
            act("Choose a model", "icons/cpu.svg", Some(shortcut("/")), Box::new(ToggleModelPicker)),
            act("Focus the composer", "icons/pencil.svg", Some(shortcut("L")), Box::new(FocusComposer)),
            act("Next permission mode", "icons/shield-check.svg", Some("Shift Tab".into()), Box::new(CycleMode)),
            act("Copy the last reply", "icons/copy.svg", Some(shortcut("Shift C")), Box::new(CopyLastReply)),
            act("Rules and memory", "icons/list-todo.svg", Some(shortcut("Shift R")), Box::new(OpenRules)),
            act("Open settings", "icons/settings.svg", Some(shortcut(",")), Box::new(OpenSettings)),
            act("Change your name", "icons/pencil.svg", None, Box::new(ChangeName)),
            act("Quit", "icons/x.svg", Some(shortcut("Q")), Box::new(Quit)),
        ];
        if s.busy() {
            out.insert(0, act("Stop the reply", "icons/square.svg", Some("Esc".into()), Box::new(StopReply)));
        }
        for m in Mode::ALL {
            out.push(Command { label: format!("Permission mode: {}", m.label()).into(), group: "Modes", icon: m.icon(), hint: (m == s.mode).then(|| "current".into()), target: Target::Mode(m) });
        }
        for (i, m) in MODELS.iter().enumerate() {
            out.push(Command { label: format!("Model: {} · {}", m.name, m.provider).into(), group: "Models", icon: "icons/cpu.svg", hint: (i == s.model).then(|| "current".into()), target: Target::Model(i) });
        }
        for c in &s.chats {
            out.push(Command { label: c.title.clone(), group: "Chats", icon: "icons/message-square.svg", hint: Some(c.group().into()), target: Target::Chat(c.id) });
        }
        out
    }

    /// The commands that match, best first, with their matched positions.
    fn matches(&self, cx: &gpui::App) -> Vec<(Command, Vec<usize>)> {
        let q = self.query.read(cx).value().to_string();
        let mut scored: Vec<(i32, usize, Command, Vec<usize>)> = self
            .commands(cx)
            .into_iter()
            .enumerate()
            .filter_map(|(order, c)| fuzzy(&q, &c.label).map(|(score, pos)| (score, order, c, pos)))
            .collect();
        if !q.trim().is_empty() {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        scored.into_iter().map(|(_, _, c, p)| (c, p)).take(50).collect()
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.palette_open = false;
            cx.notify();
        });
    }

    fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut matches = self.matches(cx);
        if matches.is_empty() {
            return;
        }
        let (cmd, _) = matches.swap_remove(self.selected.min(matches.len() - 1));
        self.close(cx);
        window.dispatch_action(Box::new(FocusComposer), cx);
        match cmd.target {
            Target::Action(a) => window.dispatch_action(a, cx),
            Target::Chat(id) => self.state.update(cx, |s, cx| s.open_chat(id, cx)),
            Target::Model(i) => self.state.update(cx, |s, cx| {
                s.model = i;
                cx.notify();
            }),
            Target::Mode(m) => self.state.update(cx, |s, cx| {
                s.mode = m;
                cx.notify();
            }),
        }
    }
}

fn highlighted(label: &SharedString, positions: &[usize], t: &Theme) -> StyledText {
    let mut runs = vec![];
    let mut i = 0;
    for (start, ch) in label.char_indices() {
        if start < i {
            continue;
        }
        let hit = positions.contains(&start);
        let len = ch.len_utf8();
        let mut f = font(t.font_sans.clone());
        if hit {
            f.weight = FontWeight::SEMIBOLD;
        }
        runs.push(TextRun { len, font: f, color: if hit { t.colors.text_strong } else { t.colors.text }, background_color: None, underline: None, strikethrough: None });
        i = start + len;
    }
    StyledText::new(label.clone()).with_runs(runs)
}

impl Render for CommandPalette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let matches = self.matches(cx);
        let selected = self.selected.min(matches.len().saturating_sub(1));
        let count = matches.len();

        let mut list = div().id("palette-list").flex().flex_col().max_h(px(380.)).overflow_y_scroll().p(t.space.xs);
        let mut last_group = "";
        for (i, (cmd, pos)) in matches.iter().enumerate() {
            if cmd.group != last_group {
                last_group = cmd.group;
                list = list.child(div().px(t.space.md).pt(t.space.sm).pb(t.space.xs).text_size(t.text.caption).font_weight(FontWeight::MEDIUM).text_color(t.colors.text_subtle).child(cmd.group));
            }
            let on = i == selected;
            list = list.child(
                div()
                    .id(("cmd", i))
                    .flex()
                    .items_center()
                    .gap(t.space.md)
                    .h(t.size.row + px(2.))
                    .px(t.space.md)
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .when(on, |d| d.bg(t.colors.hover))
                    .child(icon(cmd.icon, t.size.icon_sm, if on { t.colors.text } else { t.colors.text_muted }))
                    .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(highlighted(&cmd.label, pos, &t)))
                    .children(cmd.hint.clone().map(|h| kbd(&t, h)))
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.selected != i {
                            this.selected = i;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(|this, _, window, cx| this.run(window, cx))),
            );
        }
        if count == 0 {
            list = list.child(div().px(t.space.md).py(t.space.lg).text_color(t.colors.text_subtle).child("Nothing matches."));
        }

        let fade = std::time::Duration::from_millis(t.motion_ms);
        div()
            .id("palette-scrim")
            .absolute()
            .inset_0()
            .bg(t.extra.scrim)
            .flex()
            .justify_center()
            .items_start()
            .pt(t.size.titlebar * 3.)
            .key_context("CommandPalette")
            .capture_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                match ev.keystroke.key.as_str() {
                    "down" if count > 0 => this.selected = (this.selected + 1) % count,
                    "up" if count > 0 => this.selected = (this.selected + count - 1) % count,
                    "escape" => {
                        this.close(cx);
                        window.dispatch_action(Box::new(FocusComposer), cx);
                    }
                    "enter" => this.run(window, cx),
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                this.close(cx);
                window.dispatch_action(Box::new(FocusComposer), cx);
            }))
            .child(
                theme::popover(&t)
                    .id("palette")
                    .w(t.size.palette)
                    .h_auto()
                    .p_0()
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .px(t.space.lg)
                            .py(t.space.sm)
                            .border_b_1()
                            .border_color(t.colors.border)
                            .text_size(t.text.body)
                            .child(icon("icons/search.svg", t.size.icon, t.colors.text_subtle))
                            .child(div().flex_1().child(Input::new(&self.query).appearance(false))),
                    )
                    .child(list)
                    .child(
                        div()
                            .flex()
                            .gap(t.space.lg)
                            .px(t.space.lg)
                            .py(t.space.sm)
                            .border_t_1()
                            .border_color(t.colors.border)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child("↑↓ choose")
                            .child("Enter run")
                            .child("Esc close")
                            .child(div().flex_1())
                            .child(format!("{count} result{}", if count == 1 { "" } else { "s" })),
                    )
                    .with_animation("palette-in", gpui::Animation::new(fade).with_easing(gpui::ease_in_out), |d, delta| d.opacity(delta)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn fuzzy_matches_in_order_and_prefers_word_starts() {
        assert!(fuzzy("nc", "New chat").is_some());
        assert!(fuzzy("cn", "New chat").is_none());
        let (word, _) = fuzzy("tt", "Toggle theme").unwrap();
        let (mid, _) = fuzzy("tt", "Settings").unwrap();
        assert!(word > mid, "word starts win: {word} vs {mid}");
        assert_eq!(fuzzy("ch", "New chat").unwrap().1, vec![4, 5]);
        assert_eq!(fuzzy("", "anything").unwrap().1, Vec::<usize>::new());
    }
}
