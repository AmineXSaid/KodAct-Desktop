//! Settings → Permissions: the rules KodAct CLI checks before a tool runs,
//! under `permissions` in a settings file. `allow` runs without asking,
//! `ask` always asks, `deny` never runs; a rule is a tool name, or a tool
//! with what it applies to: `Bash(git status)`, `Read(./.env)`,
//! `WebFetch(domain:example.com)`. Rules from every file add up; a deny
//! wins. Add and remove rules per file.

use gpui::component::input::InputState;
use gpui::{
    div, prelude::*, px, Context, Entity, Hsla, IntoElement, Render, SharedString, Subscription,
    Window,
};

use super::editor_kit as kit;
use crate::forge_files::{self as ff, Layer};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

const KINDS: [(&str, &str); 3] = [
    ("allow", "Allow without asking"),
    ("ask", "Always ask"),
    ("deny", "Never allow"),
];

pub struct PermissionsEditor {
    state: Entity<AppState>,
    layer: Layer,
    kind: &'static str,
    rule: Entity<InputState>,
    /// Each file's rules, read with the catalog (None while reading).
    rules: Option<Vec<(Layer, Result<ff::Permissions, String>)>>,
    error: Option<SharedString>,
    /// A read is under way (render asks for the first one).
    reading: bool,
    _subs: Vec<Subscription>,
}

impl PermissionsEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rule = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Bash(npm test)  or  Read(./secrets/**)")
        });
        let subs = vec![
            // A new read of KodAct CLI's files: read the rules again with it.
            cx.observe(&state, |this, state, cx| {
                let s = state.read(cx);
                if s.settings_tab == crate::state::SettingsTab::Permissions && !s.engine.loading {
                    this.read(cx);
                }
                cx.notify();
            }),
            cx.subscribe_in(
                &rule,
                window,
                |this, _, ev: &gpui::component::input::InputEvent, window, cx| {
                    if matches!(ev, gpui::component::input::InputEvent::PressEnter { .. }) {
                        this.add(window, cx);
                    }
                },
            ),
        ];
        Self {
            state,
            layer: Layer::Local,
            kind: "allow",
            rule,
            rules: None,
            error: None,
            reading: false,
            _subs: subs,
        }
    }

    /// Read the three files' rules off the UI thread.
    fn read(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            return;
        }
        self.reading = true;
        let project = self.state.read(cx).project_dir.clone();
        cx.spawn(async move |this, cx| {
            let rules = cx
                .background_spawn(async move {
                    Layer::ALL
                        .iter()
                        .map(|l| (*l, ff::permissions(&project, *l)))
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.reading = false;
                if this.rules.as_ref() != Some(&rules) {
                    this.rules = Some(rules);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rule = self.rule.read(cx).value().trim().to_string();
        if let Err(e) = ff::check_rule(&rule) {
            self.error = Some(e.into());
            return cx.notify();
        }
        self.error = None;
        self.rule.update(cx, |i, cx| i.set_value("", window, cx));
        let (layer, kind) = (self.layer, self.kind);
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Added {rule}. New chats follow it."),
                move |project| ff::set_rule(project, layer, kind, &rule, true),
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Err(e) = r {
                    this.error = Some(e.into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn rule_row(
        &self,
        t: &Theme,
        layer: Layer,
        kind: &'static str,
        rule: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (fg, bg): (Hsla, Hsla) = match kind {
            "allow" => (t.extra.success, t.extra.success_tint),
            "deny" => (t.extra.danger, t.extra.danger_tint),
            _ => (t.extra.warning, t.extra.warning_tint),
        };
        let r = rule.to_string();
        div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .px(px(10.))
            .py(px(4.))
            .rounded(t.radius.button)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .child(theme::pill(t, bg, fg, kind).flex_none())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(t.text.body_sm)
                    .font_family(t.font_mono.clone())
                    .text_color(t.colors.text)
                    .child(rule.to_string()),
            )
            .child(
                kit::icon_button(
                    t,
                    SharedString::from(format!("rm-rule-{layer:?}-{kind}-{rule}")),
                    "icons/x.svg",
                    "Remove this rule",
                    true,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    let r = r.clone();
                    this.state.update(cx, |s, cx| {
                        s.engine_write(
                            format!("Removed {r}."),
                            move |project| ff::set_rule(project, layer, kind, &r, false),
                            cx,
                        )
                        .detach()
                    });
                })),
            )
            .into_any_element()
    }
}

impl Render for PermissionsEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        if self.rules.is_none() {
            self.read(cx);
        }
        // Which file: one of three, as everywhere in Settings.
        let layers = kit::layer_chips(&t, "perm-layer", self.layer, cx, |this, l, cx| {
            this.layer = l;
            cx.notify();
        });
        let mut kinds = div().flex().gap(px(6.));
        for (kind, label) in KINDS {
            kinds = kinds.child(
                kit::chip(
                    &t,
                    SharedString::from(format!("perm-kind-{kind}")),
                    label,
                    self.kind == kind,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.kind = kind;
                    cx.notify();
                })),
            );
        }
        let add_row = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(kit::line(&t, &self.rule, true)),
            )
            .child(
                theme::primary_button(&t, "Add rule")
                    .id("add-rule")
                    .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
            );
        let mut out = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(kit::header(
                &t,
                "Permissions",
                "Rules decide which tool calls run without asking, which always ask, and which never run. Rules from every file add up, and a deny always wins.",
                None,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(t.space.md)
                    .rounded(t.radius.card)
                    .border_1()
                    .border_color(t.colors.border)
                    .child(kit::field(&t, "Saved in", None, layers.into_any_element()))
                    .child(kit::field(&t, "The rule", None, kinds.into_any_element()))
                    .child(add_row)
                    .children(kit::status(&t, &self.error)),
            );
        match &self.rules {
            None => out = out.child(kit::loading(&t)),
            Some(all) => {
                let mut any = false;
                for (layer, rules) in all.clone() {
                    match rules {
                        Err(e) => out = out.children(kit::problems(&t, &[e])),
                        Ok(p) => {
                            let list: Vec<(&'static str, String)> = KINDS
                                .iter()
                                .flat_map(|(k, _)| {
                                    let rules = match *k {
                                        "allow" => &p.allow,
                                        "ask" => &p.ask,
                                        _ => &p.deny,
                                    };
                                    rules.iter().map(move |r| (*k, r.clone()))
                                })
                                .collect();
                            if list.is_empty() && p.default_mode.is_none() {
                                continue;
                            }
                            any = true;
                            out = out.child(kit::group(&t, layer.hint()));
                            if let Some(mode) = &p.default_mode {
                                out = out.child(
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_muted)
                                        .child(format!("Chats here start in the {mode} mode (defaultMode); KodAct's mode menu changes it per chat.")),
                                );
                            }
                            for (kind, rule) in list {
                                out = out.child(self.rule_row(&t, layer, kind, &rule, cx));
                            }
                        }
                    }
                }
                if !any {
                    out = out.child(kit::empty(
                        &t,
                        "No rules yet, so KodAct asks before each tool that changes something, as the chat's mode says. Allow what you trust here, such as Bash(npm test), to stop being asked.",
                    ));
                }
            }
        }
        out
    }
}
