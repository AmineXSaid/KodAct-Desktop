//! Settings → Sandbox: KodAct CLI's OS sandbox for the shell commands KodAct
//! runs (`sandbox.mode`: off, read-only or workspace-write;
//! `sandbox.network`). A sandboxed command runs without asking, since it
//! can't reach beyond the sandbox. KodAct CLI confines commands with
//! bubblewrap on Linux and sandbox-exec on macOS; where it has no sandbox
//! (Windows today), a mode set here makes it ask before each command
//! instead, and this screen says so.

use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};
use serde_json::{json, Value};

use super::editor_kit as kit;
use crate::forge_files::{self as ff, Layer};
use crate::state::{AppState, SettingsTab};
use crate::theme::{ActiveTheme, Theme};

pub struct SandboxEditor {
    state: Entity<AppState>,
    layer: Layer,
    values: Option<kit::KeyByLayer>,
    reading: bool,
    _sub: Subscription,
}

impl SandboxEditor {
    pub fn new(state: Entity<AppState>, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            _sub: cx.observe(&state, |this, state, cx| {
                let s = state.read(cx);
                if s.settings_tab == SettingsTab::Sandbox && !s.engine.loading {
                    this.read(cx);
                }
                cx.notify();
            }),
            state,
            layer: Layer::Local,
            values: None,
            reading: false,
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
                .background_spawn(async move { kit::read_key(&project, "sandbox") })
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

    /// This file's `sandbox` object.
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
                move |project| ff::set_setting(project, layer, "sandbox", (!empty).then_some(v)),
                cx,
            )
            .detach()
        });
    }
}

impl Render for SandboxEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t: Theme = cx.theme().clone();
        if self.values.is_none() {
            self.read(cx);
        }
        let platform = if cfg!(windows) {
            "KodAct CLI has no sandbox on Windows yet. With a mode set, it asks before each command instead, so nothing runs unconfined without you."
        } else if cfg!(target_os = "macos") {
            "On this Mac, KodAct CLI confines commands with sandbox-exec."
        } else {
            "On Linux, KodAct CLI confines commands with bubblewrap (bwrap). Without it, KodAct CLI asks before each command instead."
        };
        let mut out = div().flex().flex_col().gap(px(14.)).child(kit::header(
            &t,
            "Sandbox",
            "Run the shell commands KodAct starts in a sandbox. A sandboxed command runs without asking, because it can't reach past the sandbox.",
            None,
        ));
        out = out.child(
            div()
                .p(t.space.md)
                .rounded(t.radius.card)
                .bg(if cfg!(windows) {
                    t.extra.warning_tint
                } else {
                    t.colors.hover
                })
                .text_size(t.text.caption)
                .text_color(t.colors.text)
                .child(platform),
        );
        let Some(values) = self.values.clone() else {
            return out.child(kit::loading(&t));
        };
        if let Some((_, Err(e))) = values.iter().find(|(l, v)| *l == self.layer && v.is_err()) {
            return out.children(kit::problems(&t, std::slice::from_ref(e)));
        }
        let cur = self.current();
        let mode = cur
            .as_ref()
            .and_then(|v| v.get("mode"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let network = cur
            .as_ref()
            .and_then(|v| v.get("network"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut modes = div().flex().flex_col().gap(px(6.)).child(
            kit::option_row(
                &t,
                "sandbox-unset",
                "Not set here: the other places decide, and with none, no sandbox",
                mode.is_none(),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.write(
                    |v| {
                        if let Some(o) = v.as_object_mut() {
                            o.remove("mode");
                        }
                    },
                    cx,
                )
            })),
        );
        for (m, label) in ff::SANDBOX_MODES {
            modes =
                modes.child(
                    kit::option_row(
                        &t,
                        SharedString::from(format!("sandbox-{m}")),
                        label,
                        mode.as_deref() == Some(m),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.write(move |v| v["mode"] = json!(m), cx)
                    })),
                );
        }
        // Which file wins: the latest layer (local, then project, then
        // yours) that sets a mode.
        let winner = values.iter().rev().find_map(|(l, v)| {
            let m = v
                .as_ref()
                .ok()?
                .as_ref()?
                .get("mode")?
                .as_str()?
                .to_string();
            Some((*l, m))
        });
        out.child(kit::field(
            &t,
            "Saved in",
            None,
            kit::layer_chips(&t, "sandbox-layer", self.layer, cx, |this, l, cx| {
                this.layer = l;
                cx.notify();
            })
            .into_any_element(),
        ))
        .child(kit::field(&t, "Mode", None, modes.into_any_element()))
        .child(kit::field(
            &t,
            "Network",
            Some("Off keeps sandboxed commands offline; on lets them reach the network."),
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(super::button::Switch::new("sandbox-network", network, "Network for sandboxed commands").on_toggle({
                    let me = cx.entity().downgrade();
                    move |on, _, cx| {
                        let _ = me.update(cx, |this, cx| this.write(move |v| v["network"] = json!(on), cx));
                    }
                }))
                .child(
                    div()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text)
                        .child(if network { "On" } else { "Off" }),
                )
                .into_any_element(),
        ))
        .child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child(match winner {
                    Some((l, m)) => format!("New chats here use {m}, from {}.", l.hint()),
                    None => "No file sets a mode, so commands aren't sandboxed: KodAct asks before each one that changes something.".to_string(),
                }),
        )
    }
}
