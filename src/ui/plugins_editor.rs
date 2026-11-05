//! Settings → Plugins: folders bundling commands, agents, skills, hooks and
//! MCP servers, which KodAct CLI loads from the `pluginDirs` setting (relative
//! paths are from the project). KodAct's own plugin rides along with every
//! chat. Add a folder, or remove one; Marketplaces (`marketplaces.rs`)
//! installs plugins from Claude Code marketplaces.

use gpui::component::input::InputState;
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};
use serde_json::{json, Value};

use super::editor_kit as kit;
use super::icon;
use crate::forge_files::{self as ff, Layer};
use crate::state::{AppState, SettingsTab};
use crate::theme::{self, ActiveTheme, Theme};

pub struct PluginsEditor {
    state: Entity<AppState>,
    layer: Layer,
    path: Entity<InputState>,
    values: Option<kit::KeyByLayer>,
    /// Each listed folder's manifest (None: no such folder), read with them.
    found: std::collections::HashMap<String, Option<(String, String)>>,
    reading: bool,
    error: Option<SharedString>,
    /// The marketplaces panel (`marketplaces.rs`), shown while `AppState::plugins_market`.
    market: Entity<super::marketplaces::Marketplaces>,
    _subs: Vec<Subscription>,
}

/// A listed folder as KodAct CLI resolves it: relative to the project.
fn full_path(project: &std::path::Path, d: &str) -> std::path::PathBuf {
    if std::path::Path::new(d).is_absolute() {
        std::path::PathBuf::from(d)
    } else {
        project.join(d)
    }
}

/// A plugin folder's name and description, from its manifest
/// (`.kodact-plugin/plugin.json` or `plugin.json`), else the folder's name.
fn manifest(dir: &std::path::Path) -> Option<(String, String)> {
    if !dir.is_dir() {
        return None;
    }
    let read = |p: std::path::PathBuf| -> Option<Value> {
        serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
    };
    let m = read(dir.join(".kodact-plugin").join("plugin.json"))
        .or_else(|| read(dir.join("plugin.json")))
        .or_else(|| read(dir.join(".claude-plugin").join("plugin.json")));
    let name = m
        .as_ref()
        .and_then(|m| m.get("name")?.as_str().map(str::to_string))
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))?;
    let description = m
        .as_ref()
        .and_then(|m| m.get("description")?.as_str().map(str::to_string))
        .unwrap_or_default();
    Some((name, description))
}

impl PluginsEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("e.g. tools/kodact-plugin  or  C:\\plugins\\review-kit")
        });
        let subs = vec![
            cx.observe(&state, |this, state, cx| {
                let s = state.read(cx);
                if s.settings_tab == SettingsTab::Plugins && !s.engine.loading {
                    this.read(cx);
                }
                cx.notify();
            }),
            cx.subscribe_in(
                &path,
                window,
                |this, _, ev: &gpui::component::input::InputEvent, window, cx| {
                    if matches!(ev, gpui::component::input::InputEvent::PressEnter { .. }) {
                        this.add(window, cx);
                    }
                },
            ),
        ];
        Self {
            market: cx.new(|cx| super::marketplaces::Marketplaces::new(state.clone(), window, cx)),
            state,
            layer: Layer::Local,
            path,
            values: None,
            found: Default::default(),
            reading: false,
            error: None,
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
            let (v, found) = cx
                .background_spawn(async move {
                    let v = kit::read_key(&project, "pluginDirs");
                    let mut found = std::collections::HashMap::new();
                    for (_, dirs) in &v {
                        let list = dirs
                            .as_ref()
                            .ok()
                            .and_then(|d| d.as_ref()?.as_array().cloned());
                        for d in list.into_iter().flatten() {
                            if let Some(d) = d.as_str() {
                                found.insert(d.to_string(), manifest(&full_path(&project, d)));
                            }
                        }
                    }
                    (v, found)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.reading = false;
                if this.values.as_ref() != Some(&v) || this.found != found {
                    this.values = Some(v);
                    this.found = found;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn dirs(&self, layer: Layer) -> Vec<String> {
        self.values
            .as_ref()
            .and_then(|v| v.iter().find(|(l, _)| *l == layer))
            .and_then(|(_, v)| v.as_ref().ok()?.as_ref()?.as_array().cloned())
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn set(&mut self, layer: Layer, dirs: Vec<String>, done: String, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.engine_write(
                done,
                move |project| {
                    ff::set_setting(
                        project,
                        layer,
                        "pluginDirs",
                        (!dirs.is_empty()).then(|| json!(dirs)),
                    )
                },
                cx,
            )
            .detach()
        });
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.path.read(cx).value().trim().to_string();
        if path.is_empty() {
            self.error = Some("Type the plugin folder's path.".into());
            return cx.notify();
        }
        let project = self.state.read(cx).project_dir.clone();
        let full = full_path(&project, &path);
        if !full.is_dir() {
            self.error = Some(format!("{} isn't a folder.", full.display()).into());
            return cx.notify();
        }
        let mut dirs = self.dirs(self.layer);
        if dirs.contains(&path) {
            self.error = Some("That folder is already listed here.".into());
            return cx.notify();
        }
        dirs.push(path.clone());
        self.error = None;
        self.path.update(cx, |i, cx| i.set_value("", window, cx));
        self.set(
            self.layer,
            dirs,
            format!("Added {path}. New chats load it."),
            cx,
        );
    }
}

impl Render for PluginsEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t: Theme = cx.theme().clone();
        if self.state.read(cx).plugins_market {
            return self.market.clone().into_any_element();
        }
        if self.values.is_none() {
            self.read(cx);
        }
        let browse =
            theme::ghost_button(&t, "Marketplaces")
                .id("open-marketplaces")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.update(cx, |s, cx| s.open_marketplaces(cx))
                }))
                .into_any_element();
        let mut out = div().flex().flex_col().gap(px(10.)).child(kit::header(
            &t,
            "Plugins",
            "A plugin is a folder bundling commands, agents, skills, hooks and MCP servers, Claude Code's plugins included. KodAct CLI loads the folders listed here into every chat.",
            Some(browse),
        ));
        out = out.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .p(t.space.md)
                .rounded(t.radius.card)
                .border_1()
                .border_color(t.colors.border)
                .child(kit::field(
                    &t,
                    "Saved in",
                    None,
                    kit::layer_chips(&t, "plugin-layer", self.layer, cx, |this, l, cx| {
                        this.layer = l;
                        cx.notify();
                    })
                    .into_any_element(),
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(kit::line(&t, &self.path, true)),
                        )
                        .child(
                            theme::primary_button(&t, "Add folder")
                                .id("add-plugin")
                                .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
                        ),
                )
                .children(kit::status(&t, &self.error)),
        );
        let Some(values) = self.values.clone() else {
            return out.child(kit::loading(&t)).into_any_element();
        };
        let problems: Vec<String> = values.iter().filter_map(|(_, v)| v.clone().err()).collect();
        out = out.children(kit::problems(&t, &problems));
        let mut any = false;
        for layer in [Layer::Local, Layer::Project, Layer::User] {
            let dirs = self.dirs(layer);
            if dirs.is_empty() {
                continue;
            }
            any = true;
            out = out.child(kit::group(&t, layer.hint()));
            for d in dirs {
                let (title, detail, note) = match self.found.get(&d).cloned().flatten() {
                    Some((name, desc)) => (name, format!("{d} · {desc}"), None),
                    None => (
                        d.clone(),
                        String::new(),
                        Some((
                            t.extra.danger,
                            "This folder doesn't exist, so KodAct CLI skips it.".to_string(),
                        )),
                    ),
                };
                let remove = d.clone();
                out = out.child(
                    kit::card(&t)
                        .child(icon("icons/puzzle.svg", t.size.icon, t.colors.text_muted))
                        .child(kit::card_text(&t, title, false, Some(detail), note))
                        .child(
                            kit::icon_button(
                                &t,
                                SharedString::from(format!("rm-plugin-{layer:?}-{d}")),
                                "icons/x.svg",
                                "Stop loading this folder (it stays on disk)",
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let dirs: Vec<String> = this
                                        .dirs(layer)
                                        .into_iter()
                                        .filter(|x| *x != remove)
                                        .collect();
                                    this.set(
                                        layer,
                                        dirs,
                                        format!("Removed {remove} from the list."),
                                        cx,
                                    );
                                },
                            )),
                        ),
                );
            }
        }
        if !any {
            out = out.child(kit::empty(
                &t,
                "No plugin folders yet. Add one to load its commands, agents, skills, hooks and MCP servers into every chat.",
            ));
        }
        out = out.child(kit::group(&t, "Built into KodAct")).child(
            kit::card(&t)
                .child(icon("icons/puzzle.svg", t.size.icon, t.colors.text_muted))
                .child(kit::card_text(
                    &t,
                    "kodact",
                    false,
                    Some("Diagram Design and Office documents, given to every chat".into()),
                    None,
                ))
                .child(kit::scope_pill(&t, "Built in")),
        );
        out.into_any_element()
    }
}
