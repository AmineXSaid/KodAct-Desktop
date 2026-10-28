//! Settings → Hooks: commands ForgeCLI runs at points in a chat (before a
//! tool runs, after it, when you send a message, when a reply ends…), kept
//! under `hooks` in a settings file: yours, the project's (shared), or this
//! PC's for the project. Each gets the event as JSON on stdin; exit 2
//! blocks what can be blocked. Create, edit and delete.

use gpui::component::input::InputState;
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::*;
use crate::forge_files::{self as ff, Layer};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

pub struct HooksEditor {
    state: Entity<AppState>,
    /// The form is open: editing the hook at (layer, its index there), or
    /// (None) a new one.
    form: Option<Option<(Layer, usize)>>,
    layer: Layer,
    event: &'static str,
    matcher: Entity<InputState>,
    command: Entity<InputState>,
    timeout: Entity<InputState>,
    error: Option<SharedString>,
    saving: bool,
    confirm: Option<(Layer, usize)>,
    _sub: Subscription,
}

impl HooksEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _sub: cx.observe(&state, |_, _, cx| cx.notify()),
            matcher: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Bash  or  Edit|Write  (empty: every tool)")
            }),
            command: cx.new(|cx| InputState::new(window, cx).placeholder("./scripts/check.sh")),
            timeout: cx.new(|cx| InputState::new(window, cx).placeholder("60")),
            state,
            form: None,
            layer: Layer::Project,
            event: "PreToolUse",
            error: None,
            saving: false,
            confirm: None,
        }
    }

    pub fn back(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.form.is_some() || self.confirm.is_some();
        self.form = None;
        self.confirm = None;
        cx.notify();
        open
    }

    fn open(
        &mut self,
        at: Option<(Layer, usize, ff::Hook)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let h = at.as_ref().map(|(_, _, h)| h);
        let set = |i: &Entity<InputState>, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            i.update(cx, |i, cx| i.set_value(v.to_string(), window, cx))
        };
        set(&self.matcher, h.map_or("", |h| &h.matcher), window, cx);
        set(&self.command, h.map_or("", |h| &h.command), window, cx);
        let timeout = h
            .and_then(|h| h.timeout)
            .map(|t| t.to_string())
            .unwrap_or_default();
        set(&self.timeout, &timeout, window, cx);
        self.event = h
            .and_then(|h| ff::HOOK_EVENTS.iter().find(|(e, _, _)| *e == h.event))
            .map_or("PreToolUse", |(e, _, _)| e);
        self.layer = h.map_or(Layer::Project, |h| h.layer);
        self.form = Some(at.map(|(l, i, _)| (l, i)));
        self.error = None;
        self.saving = false;
        self.command.update(cx, |i, cx| i.focus(window, cx));
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.form else { return };
        let timeout_text = self.timeout.read(cx).value().trim().to_string();
        let timeout = if timeout_text.is_empty() {
            None
        } else {
            match timeout_text.parse::<f64>() {
                Ok(v) => Some(v),
                Err(_) => {
                    self.error = Some("The timeout is a number of seconds.".into());
                    return cx.notify();
                }
            }
        };
        let matcher = if ff::event_takes_matcher(self.event) {
            self.matcher.read(cx).value().trim().to_string()
        } else {
            String::new()
        };
        let hook = ff::Hook {
            layer: self.layer,
            event: self.event.to_string(),
            matcher,
            command: self.command.read(cx).value().trim().to_string(),
            timeout,
        };
        if let Err(e) = ff::check_hook(&hook) {
            self.error = Some(e.into());
            return cx.notify();
        }
        self.saving = true;
        self.error = None;
        let event = hook.event.clone();
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Saved the {event} hook. New chats run it."),
                move |project| match at {
                    // Same file: replace it in place.
                    Some((l, i)) if l == hook.layer => {
                        ff::save_hook(project, l, Some(i), Some(hook))
                    }
                    // Moved to another file: add there, then remove here.
                    Some((l, i)) => {
                        ff::save_hook(project, hook.layer, None, Some(hook))?;
                        ff::save_hook(project, l, Some(i), None)
                    }
                    None => ff::save_hook(project, hook.layer, None, Some(hook)),
                },
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match r {
                    Ok(()) => this.form = None,
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let catalog = self.state.read(cx).engine.catalog.clone();
        let add = theme::primary_button(t, "New hook")
            .id("new-hook")
            .on_click(cx.listener(|this, _, window, cx| this.open(None, window, cx)))
            .into_any_element();
        let mut out = div().flex().flex_col().gap(px(10.)).child(kit::header(
            t,
            "Hooks",
            "A hook runs a command at a point in the chat: before or after a tool, when you send a message, when a reply ends. It gets the event as JSON; exit 2 blocks what can be blocked.",
            Some(add),
        ));
        let Some(catalog) = catalog else {
            return out.child(kit::loading(t)).into_any_element();
        };
        out = out.children(kit::problems(t, &catalog.problems));
        if catalog.hooks.is_empty() {
            out = out.child(kit::empty(
                t,
                "No hooks yet. A hook can run your formatter after every edit, block a command you never want run, or add context when a chat starts.",
            ));
        }
        for layer in [Layer::Project, Layer::Local, Layer::User] {
            let rows: Vec<&ff::Hook> = catalog.hooks.iter().filter(|h| h.layer == layer).collect();
            if rows.is_empty() {
                continue;
            }
            out = out.child(kit::group(t, layer.hint()));
            for (i, h) in rows.into_iter().enumerate() {
                out = out.child(self.row(t, layer, i, h, cx));
            }
        }
        out.into_any_element()
    }

    fn row(
        &self,
        t: &Theme,
        layer: Layer,
        i: usize,
        h: &ff::Hook,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = format!("{layer:?}-{i}");
        if self.confirm == Some((layer, i)) {
            return kit::confirm_row(
                t,
                format!("Delete this {} hook?", h.event),
                format!("It runs {}.", h.command),
                kit::cancel_button(t, "cancel-delete").on_click(cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                })),
                theme::danger_button(t, "Delete")
                    .id("confirm-delete")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = None;
                        this.state.update(cx, |s, cx| {
                            s.engine_write(
                                "Deleted the hook.",
                                move |project| ff::save_hook(project, layer, Some(i), None),
                                cx,
                            )
                            .detach()
                        });
                    })),
            )
            .into_any_element();
        }
        let when = if h.matcher.is_empty() || h.matcher == "*" {
            if ff::event_takes_matcher(&h.event) {
                format!("{} · every tool", h.event)
            } else {
                h.event.clone()
            }
        } else {
            format!("{} · {}", h.event, h.matcher)
        };
        let timeout = h.timeout.map(|t| format!(" · {t}s")).unwrap_or_default();
        let edit = (layer, i, h.clone());
        kit::card(t)
            .child(icon("icons/webhook.svg", t.size.icon, t.colors.text_muted))
            .child(kit::card_text(
                t,
                h.command.clone(),
                true,
                Some(format!("{when}{timeout}")),
                None,
            ))
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("edit-hook-{key}")),
                    "icons/pencil.svg",
                    "Edit",
                    false,
                )
                .on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.open(Some(edit.clone()), window, cx)
                    }),
                ),
            )
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("rm-hook-{key}")),
                    "icons/trash.svg",
                    "Delete",
                    true,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm = Some((layer, i));
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn form(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let editing = matches!(self.form, Some(Some(_)));
        let mut layers = div().flex().flex_wrap().gap(px(6.));
        for layer in [Layer::Project, Layer::Local, Layer::User] {
            let label = match layer {
                Layer::Project => "Project (shared)",
                Layer::Local => "Project, this PC only",
                Layer::User => "Yours, every project",
            };
            layers = layers.child(
                kit::chip(
                    t,
                    SharedString::from(format!("hook-layer-{layer:?}")),
                    label,
                    self.layer == layer,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.layer = layer;
                    cx.notify();
                })),
            );
        }
        let mut events = div().flex().flex_wrap().gap(px(6.));
        for (event, _, _) in ff::HOOK_EVENTS {
            events = events.child(
                kit::chip(
                    t,
                    SharedString::from(format!("hook-event-{event}")),
                    *event,
                    self.event == *event,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.event = event;
                    cx.notify();
                })),
            );
        }
        let what = ff::HOOK_EVENTS
            .iter()
            .find(|(e, _, _)| *e == self.event)
            .map_or("", |(_, d, _)| d);
        let takes_matcher = ff::event_takes_matcher(self.event);
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                kit::back(t, "hook-back", "Hooks").on_click(cx.listener(|this, _, _, cx| {
                    this.back(cx);
                })),
            )
            .child(kit::header(
                t,
                if editing { "Edit hook" } else { "New hook" },
                "The command runs in the project folder with the event as JSON on stdin.",
                None,
            ))
            .child(kit::field(t, "Saved in", None, layers.into_any_element()))
            .child(kit::field(t, "When", Some(what), events.into_any_element()))
            .when(takes_matcher, |d| {
                d.child(kit::field(
                    t,
                    "For tools",
                    Some("A tool name, or names joined with |. Empty runs it for every tool."),
                    kit::line(t, &self.matcher, true),
                ))
            })
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().flex_1().min_w_0().child(kit::field(
                        t,
                        "Command",
                        None,
                        kit::line(t, &self.command, true),
                    )))
                    .child(div().w(px(120.)).child(kit::field(
                        t,
                        "Timeout (s)",
                        None,
                        kit::line(t, &self.timeout, true),
                    ))),
            )
            .children(kit::status(t, &self.error))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(kit::cancel_button(t, "hook-cancel").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.back(cx);
                        },
                    )))
                    .child(
                        kit::save_button(t, "hook-save", self.saving).when(!self.saving, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                        }),
                    ),
            )
            .into_any_element()
    }
}

impl Render for HooksEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.form.is_some() {
            self.form(&t, cx)
        } else {
            self.list(&t, cx)
        }
    }
}
