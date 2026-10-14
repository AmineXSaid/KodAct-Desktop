//! Settings → Environments: variables ForgeCLI sets for the commands Forge
//! runs (the `env` key of a settings file): a tool's path, a test
//! database URL, a flag your scripts read. One `KEY=value` a line, per file.

use gpui::component::input::TextareaState;
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use crate::forge_files::{self as ff, Layer};
use crate::state::{AppState, SettingsTab};
use crate::theme::{self, ActiveTheme, Theme};

pub struct EnvEditor {
    state: Entity<AppState>,
    layer: Layer,
    text: Entity<TextareaState>,
    values: Option<kit::KeyByLayer>,
    /// The box holds this layer's saved text (else it's being edited).
    shown: Option<(Layer, String)>,
    reading: bool,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl EnvEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("DATABASE_URL=postgres://localhost/test\nRUST_LOG=info")
        });
        let subs = vec![
            cx.observe(&state, |this, state, cx| {
                let s = state.read(cx);
                if s.settings_tab == SettingsTab::Environments && !s.engine.loading {
                    this.read(cx);
                }
                cx.notify();
            }),
            // Save lights up as soon as the text differs from the file.
            cx.subscribe(&text, |_, _, _: &gpui::component::input::InputEvent, cx| {
                cx.notify()
            }),
        ];
        Self {
            _subs: subs,
            text,
            state,
            layer: Layer::Local,
            values: None,
            shown: None,
            reading: false,
            error: None,
        }
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            return;
        }
        self.reading = true;
        let project = self.state.read(cx).project_dir.clone();
        cx.spawn(async move |this, cx| {
            let v = cx
                .background_spawn(async move { kit::read_key(&project, "env") })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.reading = false;
                if this.values.as_ref() != Some(&v) {
                    this.values = Some(v);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn saved(&self) -> Result<String, String> {
        match self
            .values
            .as_ref()
            .and_then(|v| v.iter().find(|(l, _)| *l == self.layer))
        {
            Some((_, Ok(v))) => Ok(ff::env_text(v.as_ref())),
            Some((_, Err(e))) => Err(e.clone()),
            None => Ok(String::new()),
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.text.read(cx).value().to_string();
        let value = match ff::env_value(&text) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e.into());
                return cx.notify();
            }
        };
        self.error = None;
        let layer = self.layer;
        self.state.update(cx, |s, cx| {
            s.engine_write(
                "Saved the variables. New chats get them.",
                move |project| ff::set_setting(project, layer, "env", value),
                cx,
            )
            .detach()
        });
    }
}

impl Render for EnvEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t: Theme = cx.theme().clone();
        if self.values.is_none() {
            self.read(cx);
        }
        let mut out = div().flex().flex_col().gap(px(14.)).child(kit::header(
            &t,
            "Environment variables",
            "Variables ForgeCLI sets for the commands Forge runs. Keep secrets in a file only this PC reads, or better, in your system's environment.",
            None,
        ));
        if self.values.is_none() {
            return out.child(kit::loading(&t));
        }
        let saved = self.saved();
        if let Err(e) = &saved {
            return out.children(kit::problems(&t, std::slice::from_ref(e)));
        }
        let saved = saved.unwrap_or_default();
        // Fill the box with what's saved when the file (or its text) changes.
        if self.shown.as_ref() != Some(&(self.layer, saved.clone())) {
            self.shown = Some((self.layer, saved.clone()));
            self.text
                .update(cx, |i, cx| i.set_value(saved.clone(), window, cx));
        }
        let dirty = self.text.read(cx).value().trim() != saved.trim();
        out = out
            .child(kit::field(
                &t,
                "Saved in",
                None,
                kit::layer_chips(&t, "env-layer", self.layer, cx, |this, l, cx| {
                    this.layer = l;
                    this.error = None;
                    cx.notify();
                })
                .into_any_element(),
            ))
            .child(kit::field(
                &t,
                "Variables",
                Some("One KEY=value a line."),
                kit::area(&t, &self.text, true),
            ))
            .children(kit::status(&t, &self.error));
        out.child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .when(dirty, |d| {
                    d.child(kit::cancel_button(&t, "env-revert").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.shown = None;
                            this.error = None;
                            cx.notify();
                        },
                    )))
                })
                .child(
                    theme::primary_button(&t, "Save")
                        .id("env-save")
                        .when(!dirty, |d| d.opacity(0.5))
                        .when(dirty, |d| {
                            d.on_click(cx.listener(|this, _, _, cx| this.save(cx)))
                        }),
                ),
        )
    }
}
