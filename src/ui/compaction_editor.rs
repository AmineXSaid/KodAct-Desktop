//! Settings → Compaction: how KodAct CLI summarizes a long conversation when
//! it fills the model's context window (`compaction` in its settings).
//!
//! - `compaction.model`: the model that writes the summaries, when not the
//!   chat's own. A reasoning model can spend a summary's whole reply
//!   reasoning and write nothing; one that answers directly can write them.
//!   A summary it can't write is written by the chat's model.
//! - `compaction.keepRecent`: keep the latest steps word for word after the
//!   summary, up to a tenth of the window.
//! - `compaction.background`: write the summary ahead of time, so nobody
//!   waits for it at the limit.
//!
//! What compaction did shows in the chat as it happens (`status_words.rs`).

use gpui::component::input::{InputEvent, InputState};
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};
use serde_json::{json, Value};

use super::editor_kit as kit;
use crate::forge_files::{self as ff, Layer};
use crate::state::{AppState, SettingsTab};
use crate::theme::{ActiveTheme, Theme};

pub struct CompactionEditor {
    state: Entity<AppState>,
    layer: Layer,
    values: Option<kit::KeyByLayer>,
    reading: bool,
    /// Another model ID, typed.
    other: Entity<InputState>,
    _subs: Vec<Subscription>,
}

impl CompactionEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let other =
            cx.new(|cx| InputState::new(window, cx).placeholder("Another model ID, then Enter"));
        let subs = vec![
            cx.observe(&state, |this, state, cx| {
                let s = state.read(cx);
                if s.settings_tab == SettingsTab::Compaction && !s.engine.loading {
                    this.read(cx);
                }
                cx.notify();
            }),
            cx.subscribe_in(
                &other,
                window,
                |this, input, ev: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        let id = input.read(cx).value().trim().to_string();
                        if !id.is_empty() {
                            this.write(move |v| v["model"] = json!(id), cx);
                            input.update(cx, |i, cx| i.set_value("", window, cx));
                        }
                    }
                },
            ),
        ];
        Self {
            state,
            layer: Layer::User,
            values: None,
            reading: false,
            other,
            _subs: subs,
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
                .background_spawn(async move { kit::read_key(&project, "compaction") })
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

    /// This file's `compaction` object.
    fn current(&self) -> Option<Value> {
        self.values
            .as_ref()?
            .iter()
            .find(|(l, _)| *l == self.layer)
            .and_then(|(_, v)| v.clone().ok().flatten())
    }

    fn write(&mut self, change: impl FnOnce(&mut Value) + Send + 'static, cx: &mut Context<Self>) {
        let layer = self.layer;
        let mut v = self.current().unwrap_or_else(|| json!({}));
        change(&mut v);
        let empty = v.as_object().is_some_and(|o| o.is_empty());
        self.state.update(cx, |s, cx| {
            s.engine_write(
                "Saved. New chats use it.",
                move |project| ff::set_setting(project, layer, "compaction", (!empty).then_some(v)),
                cx,
            )
            .detach()
        });
    }

    /// Unset `key` here, so another file (or the default) decides.
    fn unset(&mut self, key: &'static str, cx: &mut Context<Self>) {
        self.write(
            move |v| {
                if let Some(o) = v.as_object_mut() {
                    o.remove(key);
                }
            },
            cx,
        );
    }

    /// A switch for a `true`/`false` key, with "On" or "Off" beside it and
    /// a way back to "not set here".
    fn switch(
        &self,
        t: &Theme,
        key: &'static str,
        label: &'static str,
        cur: Option<bool>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let me = cx.entity().downgrade();
        div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .child(
                super::button::Switch::new(
                    SharedString::from(format!("compaction-{key}")),
                    cur.unwrap_or(false),
                    label,
                )
                .on_toggle(move |on, _, cx| {
                    let _ = me.update(cx, |this, cx| this.write(move |v| v[key] = json!(on), cx));
                }),
            )
            .child(
                div()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text)
                    .child(match cur {
                        Some(true) => "On",
                        Some(false) => "Off",
                        None => "Off (not set here)",
                    }),
            )
            .when(cur.is_some(), |d| {
                d.child(
                    kit::link(
                        t,
                        SharedString::from(format!("compaction-{key}-unset")),
                        "icons/circle-x.svg",
                        "Unset",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.unset(key, cx))),
                )
            })
            .into_any_element()
    }
}

/// The models offered for summaries: those of your other models on the
/// same endpoint as the one in use (a summary model is asked there).
fn same_endpoint_models(s: &AppState) -> Vec<String> {
    let Some(e) = s.endpoints.as_ref() else {
        return vec![];
    };
    let Some(active) = e.active() else {
        return vec![];
    };
    let mut out: Vec<String> = e
        .profiles
        .iter()
        .filter(|p| p.base_url == active.base_url && p.model != active.model)
        .map(|p| p.model.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

impl Render for CompactionEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t: Theme = cx.theme().clone();
        if self.values.is_none() {
            self.read(cx);
        }
        let out = div().flex().flex_col().gap(px(14.)).child(kit::header(
            &t,
            "Compaction",
            "When a conversation fills the model's context window, KodAct summarizes its earlier part and goes on from the summary. The chat says when it does.",
            None,
        ));
        let Some(values) = self.values.clone() else {
            return out.child(kit::loading(&t));
        };
        if let Some((_, Err(e))) = values.iter().find(|(l, v)| *l == self.layer && v.is_err()) {
            return out.children(kit::problems(&t, std::slice::from_ref(e)));
        }
        let cur = self.current();
        let get = |k: &str| cur.as_ref().and_then(|v| v.get(k)).cloned();
        let model = get("model").and_then(|v| v.as_str().map(str::to_string));
        let keep = get("keepRecent").and_then(|v| v.as_bool());
        let ahead = get("background").and_then(|v| v.as_bool());

        let s = self.state.read(cx);
        let chat_model = s
            .endpoints
            .as_ref()
            .and_then(|e| e.active())
            .map(|p| p.model.clone());
        let mut offered = same_endpoint_models(s);
        // A model set here that isn't one of yours is still listed, picked.
        if let Some(m) = model.as_ref().filter(|m| !offered.contains(m)) {
            offered.insert(0, m.clone());
        }
        let mut models = div().flex().flex_col().gap(px(6.)).child(
            kit::option_row(
                &t,
                "compaction-model-chat",
                &match &chat_model {
                    Some(m) => format!("The chat's own model ({m})"),
                    None => "The chat's own model".to_string(),
                },
                model.is_none(),
            )
            .on_click(cx.listener(|this, _, _, cx| this.unset("model", cx))),
        );
        for (i, m) in offered.into_iter().enumerate() {
            let picked = model.as_deref() == Some(m.as_str());
            let id = m.clone();
            models = models.child(
                kit::option_row(
                    &t,
                    SharedString::from(format!("compaction-model-{i}")),
                    &m,
                    picked,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let id = id.clone();
                    this.write(move |v| v["model"] = json!(id), cx)
                })),
            );
        }
        models = models.child(kit::line(&t, &self.other, true));

        // Which file decides the model: the latest layer that sets one.
        let winner = values.iter().rev().find_map(|(l, v)| {
            let m = v
                .as_ref()
                .ok()?
                .as_ref()?
                .get("model")?
                .as_str()?
                .to_string();
            Some((*l, m))
        });

        out.child(kit::field(
            &t,
            "Saved in",
            None,
            kit::layer_chips(&t, "compaction-layer", self.layer, cx, |this, l, cx| {
                this.layer = l;
                cx.notify();
            })
            .into_any_element(),
        ))
        .child(kit::field(
            &t,
            "Summary model",
            Some("A reasoning model can spend a summary's whole reply reasoning and write nothing. A model on the same endpoint that answers directly can write the summaries instead; when it can't, the chat's own model does."),
            models.into_any_element(),
        ))
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(match winner {
                    Some((l, m)) => format!("New chats here summarize with {m}, from {}.", l.hint()),
                    None => "No file sets a summary model: each chat's own model writes its summaries.".to_string(),
                }),
        )
        .child(kit::field(
            &t,
            "Keep the latest steps",
            Some("After a summary, keep the most recent steps word for word (up to a tenth of the window), so KodAct still sees exactly what it just did."),
            self.switch(&t, "keepRecent", "Keep the latest steps after a summary", keep, cx),
        ))
        .child(kit::field(
            &t,
            "Summarize ahead",
            Some("Write the summary in the background as the conversation nears the limit, so nobody waits for it when it's due."),
            self.switch(&t, "background", "Write the summary ahead of time", ahead, cx),
        ))
    }
}
