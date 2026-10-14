//! Settings: a window over the app (dimmed backdrop, Esc or a click outside
//! closes it) with General, Models and About.
//!
//! Models is where endpoints are set up, the way Forge for VS Code's guided
//! "Add endpoint" flow does it, as one form instead of a row of prompts:
//! start from a preset (local runtimes found on this PC are marked), then
//! name, URL, protocol and key, then the model list loaded *with* that key
//! (embedding models dropped), then one real check of the pair before it is
//! saved and selected. The key goes to Windows Credential Manager; the
//! profile in endpoints.json only names it (`${secret:…}`).

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, Hsla, IntoElement,
    Render, SharedString, Subscription, Window,
};

use super::*;
use crate::endpoints::{Profile, Wire, PRESETS};
use crate::state::{AppState, SettingsTab};
use crate::theme::{self, ActiveTheme, Theme};

/// Where a form's check stands.
#[derive(Clone, PartialEq)]
enum Status {
    Idle,
    Working(SharedString),
    Ok(SharedString),
    Failed(SharedString),
}

pub struct Settings {
    state: Entity<AppState>,
    name: Entity<InputState>,
    // The endpoint form; open while `form` is set.
    form: bool,
    /// Editing this profile (its name before the edit), else adding.
    editing: Option<String>,
    preset: Option<&'static str>,
    ep_name: Entity<InputState>,
    ep_url: Entity<InputState>,
    ep_key: Entity<InputState>,
    ep_model: Entity<InputState>,
    wire: Wire,
    models: Vec<String>,
    status: Status,
    /// The check failed: offer to save anyway.
    save_anyway: bool,
    detected: Vec<&'static str>,
    commands: Entity<super::slash_commands::SlashCommands>,
    agents: Entity<super::agents_editor::AgentsEditor>,
    skills: Entity<super::skills_editor::SkillsEditor>,
    hooks: Entity<super::hooks_editor::HooksEditor>,
    mcp: Entity<super::mcp_editor::McpEditor>,
    permissions: Entity<super::permissions_editor::PermissionsEditor>,
    sandbox: Entity<super::sandbox_editor::SandboxEditor>,
    env: Entity<super::env_editor::EnvEditor>,
    plugins: Entity<super::plugins_editor::PluginsEditor>,
    memory: Entity<super::memory_view::MemoryView>,
    _subs: Vec<Subscription>,
}

/// The Settings window's corner radius.
const PANEL_RADIUS: gpui::Pixels = px(14.);

/// The sections, in groups, with each one's icon and name.
const SECTIONS: &[(&str, &[(SettingsTab, &str, &str)])] = &[
    (
        "Forge",
        &[
            (
                SettingsTab::General,
                "icons/sliders-horizontal.svg",
                "General",
            ),
            (SettingsTab::Models, "icons/cpu.svg", "Models"),
            (SettingsTab::Profiles, "icons/layers.svg", "Profiles"),
        ],
    ),
    (
        "Engine",
        &[
            (
                SettingsTab::Commands,
                "icons/square-slash.svg",
                "Slash commands",
            ),
            (SettingsTab::Agents, "icons/bot.svg", "Agents"),
            (SettingsTab::Skills, "icons/wand-sparkles.svg", "Skills"),
            (SettingsTab::Hooks, "icons/webhook.svg", "Hooks"),
            (SettingsTab::Mcp, "icons/plug.svg", "MCP servers"),
            (
                SettingsTab::Permissions,
                "icons/shield-check.svg",
                "Permissions",
            ),
        ],
    ),
    (
        "Workspace",
        &[
            (SettingsTab::Sandbox, "icons/box.svg", "Sandbox"),
            (SettingsTab::Network, "icons/network.svg", "Network"),
            (
                SettingsTab::Environments,
                "icons/server.svg",
                "Environments",
            ),
            (SettingsTab::Memory, "icons/brain.svg", "Memory and rules"),
            (SettingsTab::Plugins, "icons/puzzle.svg", "Plugins"),
        ],
    ),
    (
        "Help",
        &[
            (SettingsTab::Guide, "icons/book-open.svg", "Guide"),
            (SettingsTab::About, "icons/info.svg", "About"),
        ],
    ),
];

impl Settings {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let name = input("Your name", window, cx);
        let ep_name = input("kgpt", window, cx);
        let ep_url = input("https://api.example.com/v1", window, cx);
        let ep_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Paste the key")
                .masked(true)
        });
        let ep_model = input("Type a model id, or load the list", window, cx);
        let subs = vec![
            cx.observe_in(&state, window, |this, state, window, cx| {
                let (open, add) = (state.read(cx).settings_open, state.read(cx).settings_add);
                if add {
                    state.update(cx, |s, _| s.settings_add = false);
                    this.open_form(None, window, cx);
                }
                if open && this.name.read(cx).value().is_empty() {
                    let n = state.read(cx).user_name.clone().unwrap_or_default();
                    this.name
                        .update(cx, |i, cx| i.set_value(n.to_string(), window, cx));
                }
                cx.notify();
            }),
            cx.subscribe_in(&ep_url, window, |this, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.models.clear();
                    cx.notify();
                }
            }),
            cx.subscribe_in(&ep_model, window, |_, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        let commands =
            cx.new(|cx| super::slash_commands::SlashCommands::new(state.clone(), window, cx));
        let agents =
            cx.new(|cx| super::agents_editor::AgentsEditor::new(state.clone(), window, cx));
        let skills =
            cx.new(|cx| super::skills_editor::SkillsEditor::new(state.clone(), window, cx));
        let hooks = cx.new(|cx| super::hooks_editor::HooksEditor::new(state.clone(), window, cx));
        let mcp = cx.new(|cx| super::mcp_editor::McpEditor::new(state.clone(), window, cx));
        let permissions = cx
            .new(|cx| super::permissions_editor::PermissionsEditor::new(state.clone(), window, cx));
        let sandbox =
            cx.new(|cx| super::sandbox_editor::SandboxEditor::new(state.clone(), window, cx));
        let env = cx.new(|cx| super::env_editor::EnvEditor::new(state.clone(), window, cx));
        let plugins =
            cx.new(|cx| super::plugins_editor::PluginsEditor::new(state.clone(), window, cx));
        let memory = cx.new(|cx| super::memory_view::MemoryView::new(state.clone(), window, cx));
        Self {
            state,
            name,
            form: false,
            editing: None,
            preset: None,
            ep_name,
            ep_url,
            ep_key,
            ep_model,
            wire: Wire::Openai,
            models: vec![],
            status: Status::Idle,
            save_anyway: false,
            detected: vec![],
            commands,
            agents,
            skills,
            hooks,
            mcp,
            permissions,
            sandbox,
            env,
            plugins,
            memory,
            _subs: subs,
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.settings_open = false;
            cx.notify();
        });
    }

    fn set(input: &Entity<InputState>, v: &str, window: &mut Window, cx: &mut Context<Self>) {
        input.update(cx, |i, cx| i.set_value(v.to_string(), window, cx));
    }

    fn val(input: &Entity<InputState>, cx: &Context<Self>) -> String {
        input.read(cx).value().trim().to_string()
    }

    /// Open the form: empty to add, or filled from a profile to edit it.
    fn open_form(&mut self, profile: Option<Profile>, window: &mut Window, cx: &mut Context<Self>) {
        self.form = true;
        self.models.clear();
        self.status = Status::Idle;
        self.save_anyway = false;
        self.preset = None;
        self.editing = profile.as_ref().map(|p| p.name.clone());
        let p = profile.unwrap_or_else(|| Profile {
            name: String::new(),
            base_url: String::new(),
            wire: Wire::Openai,
            model: String::new(),
            display_name: None,
            chat_path: None,
            headers: Default::default(),
            auth: Default::default(),
            capabilities: Default::default(),
        });
        self.wire = p.wire;
        Self::set(&self.ep_name, &p.name, window, cx);
        Self::set(&self.ep_url, &p.base_url, window, cx);
        Self::set(&self.ep_model, &p.model, window, cx);
        Self::set(&self.ep_key, "", window, cx);
        self.ep_key.update(cx, |i, cx| {
            i.set_placeholder(
                if self.editing.is_some() {
                    "Leave empty to keep the saved key"
                } else {
                    "Paste the key"
                },
                window,
                cx,
            );
        });
        // Which local runtimes answer right now.
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async { crate::endpoints::detect_local() })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.detected = found;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn pick_preset(&mut self, id: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = PRESETS.iter().find(|p| p.id == id) else {
            return;
        };
        self.preset = Some(id);
        self.wire = p.wire;
        self.models.clear();
        self.status = Status::Idle;
        Self::set(&self.ep_url, p.base_url, window, cx);
        if Self::val(&self.ep_name, cx).is_empty() || self.editing.is_none() {
            Self::set(&self.ep_name, p.id, window, cx);
        }
        // A local server names its own models: list them straight away.
        if !p.key {
            self.load_models(cx);
        }
        cx.notify();
    }

    /// The key typed, else (editing) the one saved.
    fn key(&self, cx: &Context<Self>) -> Option<String> {
        let typed = Self::val(&self.ep_key, cx);
        if !typed.is_empty() {
            return Some(typed);
        }
        let old = self.editing.as_ref()?;
        crate::secrets::get(&crate::secrets::key_for(old)).or_else(|| {
            let s = self.state.read(cx);
            let p = s
                .endpoints
                .as_ref()?
                .profiles
                .iter()
                .find(|p| &p.name == old)?;
            crate::health::token(&p.auth).ok().flatten()
        })
    }

    fn load_models(&mut self, cx: &mut Context<Self>) {
        let url = Self::val(&self.ep_url, cx);
        if let Err(e) = crate::endpoints::validate_url(&url) {
            self.status = Status::Failed(e.into());
            return cx.notify();
        }
        let (wire, key) = (self.wire, self.key(cx));
        self.status = Status::Working("Loading the models…".into());
        cx.spawn(async move |this, cx| {
            let listed = cx
                .background_spawn(async move {
                    crate::endpoints::list_models(&url, wire, key.as_deref(), &Default::default())
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                match listed {
                    Ok(m) if m.is_empty() => {
                        s.status = Status::Failed(
                            "The endpoint listed no chat models: type the model id.".into(),
                        )
                    }
                    Ok(m) => {
                        s.status = Status::Ok(
                            format!(
                                "{} {}. Pick one.",
                                m.len(),
                                if m.len() == 1 { "model" } else { "models" }
                            )
                            .into(),
                        );
                        s.models = m;
                    }
                    Err(e) => {
                        s.status = Status::Failed(
                            format!("Couldn't list models: {e}. You can still type the model id.")
                                .into(),
                        )
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Check the pair with one real request, then save and select it.
    fn check_and_save(&mut self, skip_check: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (name, url, model) = (
            Self::val(&self.ep_name, cx),
            Self::val(&self.ep_url, cx),
            Self::val(&self.ep_model, cx),
        );
        let taken: Vec<String> = self
            .state
            .read(cx)
            .endpoints
            .as_ref()
            .map(|e| {
                e.profiles
                    .iter()
                    .map(|p| p.name.clone())
                    .filter(|n| Some(n) != self.editing.as_ref())
                    .collect()
            })
            .unwrap_or_default();
        let invalid = crate::endpoints::validate_name(&name, &taken)
            .err()
            .or_else(|| crate::endpoints::validate_url(&url).err())
            .or_else(|| {
                model
                    .is_empty()
                    .then(|| "Pick or type a model.".to_string())
            });
        if let Some(e) = invalid {
            self.status = Status::Failed(e.into());
            return cx.notify();
        }
        let key = self.key(cx);
        let needs_key = self.wire == Wire::Anthropic
            || PRESETS.iter().any(|p| Some(p.id) == self.preset && p.key);
        if key.is_none() && needs_key {
            self.status = Status::Failed("This endpoint needs a key.".into());
            return cx.notify();
        }
        let old = self.editing.clone().and_then(|n| {
            self.state
                .read(cx)
                .endpoints
                .as_ref()
                .and_then(|e| e.profiles.iter().find(|p| p.name == n).cloned())
        });
        let secret = crate::secrets::key_for(&name);
        let profile = Profile {
            name: name.clone(),
            base_url: url,
            wire: self.wire,
            model,
            display_name: old
                .as_ref()
                .and_then(|p| p.display_name.clone())
                .filter(|_| {
                    old.as_ref()
                        .is_some_and(|o| o.model == Self::val(&self.ep_model, cx))
                }),
            chat_path: old.as_ref().and_then(|p| p.chat_path.clone()),
            headers: old.as_ref().map(|p| p.headers.clone()).unwrap_or_default(),
            auth: if key.is_some() {
                crate::endpoints::Auth::Bearer {
                    token: crate::secrets::reference(&secret),
                }
            } else {
                crate::endpoints::Auth::None
            },
            capabilities: old
                .as_ref()
                .map(|p| p.capabilities.clone())
                .unwrap_or_default(),
        };
        let replaces = self.editing.clone();
        self.status = Status::Working(if skip_check {
            "Saving…".into()
        } else {
            "Checking the model with one short request…".into()
        });
        self.save_anyway = false;
        let _ = window;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    if let Some(k) = &key {
                        crate::secrets::set(&secret, k)?;
                    }
                    if !skip_check {
                        let h = crate::health::probe(&profile, std::time::Duration::from_secs(30))?;
                        if !h.servable {
                            return Err(format!(
                                "the model didn't answer: {}",
                                h.detail.unwrap_or_default()
                            ));
                        }
                    }
                    crate::endpoints::save_profile(&profile, replaces.as_deref())
                        .map_err(|e| format!("endpoints.json could not be written: {e}"))?;
                    // A rename leaves the old name's key behind: drop it.
                    if let Some(old) = replaces.filter(|o| *o != profile.name) {
                        crate::secrets::delete(&crate::secrets::key_for(&old));
                    }
                    Ok(profile.name.clone())
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                match result {
                    Ok(name) => {
                        s.form = false;
                        s.status = Status::Idle;
                        s.state.update(cx, |st, cx| {
                            st.load_endpoints(true, cx);
                            st.show_toast(format!("{name} is saved and in use."), cx);
                        });
                    }
                    Err(e) => {
                        s.status = Status::Failed(e.into());
                        s.save_anyway = !skip_check;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn remove(&mut self, name: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let n = name.clone();
            let r = cx
                .background_spawn(async move {
                    crate::secrets::delete(&crate::secrets::key_for(&n));
                    crate::endpoints::remove_profile(&n)
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.state.update(cx, |st, cx| {
                    st.load_endpoints(false, cx);
                    st.show_toast(
                        if r.is_ok() {
                            format!("Removed {name}.")
                        } else {
                            format!("{name} could not be removed.")
                        },
                        cx,
                    );
                });
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ pieces

    fn section(t: &Theme, title: &str, hint: Option<&str>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(t.text.body_sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_strong)
                    .child(title.to_string()),
            )
            .children(hint.map(|h| {
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .line_height(relative(1.4))
                    .child(h.to_string())
            }))
    }

    fn field(t: &Theme, label: &str, input: &Entity<InputState>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .child(
                div()
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_muted)
                    .child(label.to_string()),
            )
            .child(Self::boxed(
                t,
                Input::new(input)
                    .appearance(false)
                    .text_size(t.text.body_sm)
                    .into_any_element(),
            ))
    }

    fn boxed(t: &Theme, child: AnyElement) -> gpui::Div {
        div()
            .h(px(32.))
            .px(px(2.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .bg(t.colors.canvas)
            .border_1()
            .border_color(t.colors.border)
            .child(div().flex_1().min_w_0().child(child))
    }

    fn chip(
        t: &Theme,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        on: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = t.colors.hover;
        let label: SharedString = label.into();
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(5.))
            .h(px(26.))
            .px(px(10.))
            .rounded(t.radius.pill)
            .border_1()
            .border_color(if on {
                t.colors.primary
            } else {
                t.colors.border
            })
            .bg(if on {
                t.colors.primary_tint
            } else {
                t.colors.surface
            })
            .text_size(t.text.caption)
            .text_color(if on { t.colors.primary } else { t.colors.text })
            .cursor_pointer()
            .when(!on, |d| d.hover(move |s| s.bg(hover)))
            .when(!label.is_empty(), |d| d.child(label))
    }

    fn general(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.state.read(cx);
        let (dark, default_mode, bypass, browser) =
            (s.dark, s.default_mode, s.bypass_allowed, s.browser);
        let name_row = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div().w(px(260.)).child(Self::boxed(
                    t,
                    Input::new(&self.name)
                        .appearance(false)
                        .text_size(t.text.body_sm)
                        .into_any_element(),
                )),
            )
            .child(
                theme::ghost_button(t, "Save")
                    .id("save-name")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let n = this.name.read(cx).value().to_string();
                        this.state
                            .update(cx, |s, cx| match s.set_user_name(&n, cx) {
                                Ok(()) => s.show_toast("Saved your name.", cx),
                                Err(e) => s.show_toast(e, cx),
                            });
                    })),
            );
        let appearance = div()
            .flex()
            .gap(px(6.))
            .child(
                Self::chip(t, "light", "Light", !dark).on_click(move |_, window, cx| {
                    if dark {
                        window.dispatch_action(Box::new(ToggleTheme), cx)
                    }
                }),
            )
            .child(
                Self::chip(t, "dark", "Dark", dark).on_click(move |_, window, cx| {
                    if !dark {
                        window.dispatch_action(Box::new(ToggleTheme), cx)
                    }
                }),
            );
        let mut modes = div().flex().flex_wrap().gap(px(6.));
        for m in crate::modes::ModeId::ALL {
            if m == crate::modes::ModeId::Bypass && !bypass {
                continue;
            }
            let state = self.state.clone();
            modes = modes.child(
                Self::chip(
                    t,
                    SharedString::from(format!("mode-{}", m.label())),
                    "",
                    m == default_mode,
                )
                .child(icon(
                    m.icon(),
                    t.size.icon_xs,
                    if m == default_mode {
                        t.colors.primary
                    } else {
                        t.colors.text_muted
                    },
                ))
                .child(m.label())
                .on_click(move |_, _, cx| {
                    state.update(cx, |s, cx| {
                        s.default_mode = m;
                        cx.notify();
                    })
                }),
            );
        }
        let state = self.state.clone();
        let bypass_row = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().flex_1().text_size(t.text.caption).text_color(t.colors.text_subtle).line_height(relative(1.4)).child("Lets Forge run any tool without asking. Only for throwaway folders or a sandbox."))
            .child(if bypass {
                Self::chip(t, "bypass", "Allowed · turn off", false).on_click(move |_, _, cx| state.update(cx, |s, cx| {
                    s.bypass_allowed = false;
                    if s.default_mode == crate::modes::ModeId::Bypass {
                        s.default_mode = crate::modes::ModeId::Manual;
                    }
                    cx.notify();
                }))
            } else {
                Self::chip(t, "bypass", "Allow", false).on_click(move |_, _, cx| state.update(cx, |s, cx| {
                    s.bypass_allowed = true;
                    cx.notify();
                }))
            });
        let browser_row = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().flex_1().min_w_0().text_size(t.text.caption).text_color(t.colors.text_subtle).line_height(relative(1.4)).child("Lets Forge open web pages, click, type and take screenshots, through browser-use (run by uv). Each step asks first."))
            .child(
                Self::chip(t, "browser", if browser { "On · turn off" } else { "Turn on" }, browser)
                    .flex_none()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(super::ToggleBrowser), cx)),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(Self::section(
                        t,
                        "Your name",
                        Some("What Forge calls you on the new-chat screen."),
                    ))
                    .child(name_row),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(Self::section(t, "Appearance", None))
                    .child(appearance),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(Self::section(
                        t,
                        "New chats start in",
                        Some("Shift+Tab changes the mode inside a chat."),
                    ))
                    .child(modes),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(Self::section(t, "Bypass permissions", None))
                    .child(bypass_row),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(Self::section(t, "Web browser", None))
                    .child(browser_row),
            )
            .into_any_element()
    }

    fn models_list(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.state.read(cx);
        let profiles = s
            .endpoints
            .as_ref()
            .map(|e| e.profiles.clone())
            .unwrap_or_default();
        let active = s.active_profile().map(|p| p.name.clone());
        let mut list = div().flex().flex_col().gap(px(8.));
        if profiles.is_empty() {
            list = list.child(
                div()
                    .p(px(18.))
                    .rounded(t.radius.card)
                    .border_1()
                    .border_color(t.colors.border)
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_muted)
                    .child("No endpoints yet. Add one: a cloud API with its key, or a model server running on this PC."),
            );
        }
        for p in profiles {
            let (check, syncing) = crate::health::pair_check(&p.model, s.health.get(&p.name));
            let (dot, text): (Hsla, String) = match &check {
                crate::health::Check::Answered { ms, .. } => (
                    t.extra.success,
                    format!("answered in {}", crate::health::ping_text(*ms)),
                ),
                crate::health::Check::Failed { detail }
                | crate::health::Check::Unreachable { detail } => (t.extra.danger, detail.clone()),
                _ if syncing => (t.colors.text_subtle, "checking…".into()),
                _ => (t.colors.text_subtle, "not checked yet".into()),
            };
            let in_use = active.as_deref() == Some(p.name.as_str());
            let (use_state, edit_p, rm_name) = (self.state.clone(), p.clone(), p.name.clone());
            let name = p.name.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(14.))
                    .py(px(11.))
                    .rounded(t.radius.card)
                    .border_1()
                    .border_color(if in_use {
                        t.colors.primary.opacity(0.5)
                    } else {
                        t.colors.border
                    })
                    .bg(t.colors.surface)
                    .child(icon(
                        if p.wire == Wire::Anthropic {
                            "icons/sparkles.svg"
                        } else {
                            "icons/cpu.svg"
                        },
                        t.size.icon,
                        t.colors.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(t.text.body_sm)
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(t.colors.text_strong)
                                            .child(p.display().to_string()),
                                    )
                                    .when(in_use, |d| {
                                        d.child(theme::pill(
                                            t,
                                            t.colors.primary_tint,
                                            t.colors.primary,
                                            "In use",
                                        ))
                                    }),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_subtle)
                                    .child(format!(
                                        "{} · {} · {}",
                                        p.name,
                                        crate::endpoints::host_of(&p.base_url),
                                        if p.wire == Wire::Anthropic {
                                            "Anthropic Messages"
                                        } else {
                                            "OpenAI-compatible"
                                        }
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(div().size(px(6.)).rounded(t.radius.pill).bg(dot))
                                    .child(
                                        div()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_size(t.text.caption)
                                            .text_color(t.colors.text_muted)
                                            .child(text),
                                    ),
                            ),
                    )
                    .when(!in_use, |d| {
                        d.child(
                            Self::chip(t, SharedString::from(format!("use-{name}")), "Use", false)
                                .on_click(move |_, _, cx| {
                                    use_state.update(cx, |s, cx| s.select_profile(&name, cx))
                                }),
                        )
                    })
                    .child(
                        Self::chip(
                            t,
                            SharedString::from(format!("edit-{}", p.name)),
                            "Edit",
                            false,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.open_form(Some(edit_p.clone()), window, cx)
                            },
                        )),
                    )
                    .child({
                        let (state, name) = (self.state.clone(), p.name.clone());
                        super::editor_kit::icon_button(
                            t,
                            SharedString::from(format!("diagnose-{}", p.name)),
                            "icons/stethoscope.svg",
                            "Diagnose: DNS, connection, TLS and the key, step by step",
                            false,
                        )
                        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.diagnose(&name, cx)))
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("rm-{}", p.name)))
                            .size(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(t.radius.pill)
                            .cursor_pointer()
                            .hover({
                                let h = t.extra.danger_tint;
                                move |s| s.bg(h)
                            })
                            .child(icon("icons/trash.svg", t.size.icon_sm, t.colors.text_muted))
                            .tooltip(|window, cx| {
                                gpui::component::tooltip::Tooltip::new(
                                    "Remove this endpoint and its saved key",
                                )
                                .build(window, cx)
                            })
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.remove(rm_name.clone(), cx)),
                            ),
                    ),
            );
        }
        let state = self.state.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .child(div().flex_1().min_w_0().child(Self::section(t, "Endpoints", Some("Each endpoint is a URL and the model it runs. Forge checks each one with a tiny request; the one in use answers your chats."))))
                    .child(theme::primary_button(t, "Add endpoint").id("add-endpoint").on_click(cx.listener(|this, _, window, cx| this.open_form(None, window, cx)))),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .gap(px(14.))
                    .child(div().id("check-all").flex().items_center().gap(px(5.)).cursor_pointer().text_size(t.text.caption).text_color(t.colors.primary).child(icon("icons/refresh-cw.svg", t.size.icon_xs, t.colors.primary)).child("Check all again").on_click({
                        let state = state.clone();
                        move |_, _, cx| state.update(cx, |s, cx| s.sync_all(cx))
                    }))
                    .child(div().id("open-json").flex().items_center().gap(px(5.)).cursor_pointer().text_size(t.text.caption).text_color(t.colors.primary).child(icon("icons/file-text.svg", t.size.icon_xs, t.colors.primary)).child("Open endpoints.json").on_click({
                        let state = state.clone();
                        move |_, _, cx| state.update(cx, |s, cx| s.open_endpoints_file(cx))
                    }))
                    .child(div().id("guide").flex().items_center().gap(px(5.)).cursor_pointer().text_size(t.text.caption).text_color(t.colors.primary).child(icon("icons/route.svg", t.size.icon_xs, t.colors.primary)).child("Setup guide").on_click(move |_, _, cx| state.update(cx, |s, cx| {
                        s.settings_open = false;
                        s.show_endpoint_guide(cx);
                    }))),
            )
            .into_any_element()
    }

    fn models_form(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut presets = div().flex().flex_wrap().gap(px(6.));
        for p in PRESETS.iter() {
            let found = self.detected.contains(&p.id);
            let id = p.id;
            presets = presets.child(
                Self::chip(
                    t,
                    SharedString::from(format!("preset-{id}")),
                    p.label,
                    self.preset == Some(id),
                )
                .when(found, |d| {
                    d.child(
                        div()
                            .size(px(6.))
                            .rounded(t.radius.pill)
                            .bg(t.extra.success),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| this.pick_preset(id, window, cx))),
            );
        }
        let found_note = (!self.detected.is_empty()).then(|| {
            let names: Vec<&str> = PRESETS
                .iter()
                .filter(|p| self.detected.contains(&p.id))
                .map(|p| p.label)
                .collect();
            format!("Running on this PC: {}.", names.join(", "))
        });
        let wire = self.wire;
        let wire_row = div()
            .flex()
            .gap(px(6.))
            .child(
                Self::chip(t, "wire-openai", "OpenAI-compatible", wire == Wire::Openai).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.wire = Wire::Openai;
                        this.models.clear();
                        cx.notify();
                    }),
                ),
            )
            .child(
                Self::chip(
                    t,
                    "wire-anthropic",
                    "Anthropic Messages",
                    wire == Wire::Anthropic,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wire = Wire::Anthropic;
                    this.models.clear();
                    cx.notify();
                })),
            );
        let picked = Self::val(&self.ep_model, cx);
        let model_row = div()
            .flex()
            .items_end()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .child(Self::field(t, "Model", &self.ep_model)),
            )
            .child(
                theme::ghost_button(t, "Load models")
                    .id("load-models")
                    .on_click(cx.listener(|this, _, _, cx| this.load_models(cx))),
            );
        let mut list = div()
            .id("model-list")
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .max_h(px(132.))
            .overflow_y_scroll();
        for m in self.models.iter().take(300) {
            let id = m.clone();
            list = list.child(
                Self::chip(
                    t,
                    SharedString::from(format!("m-{m}")),
                    m.clone(),
                    picked == *m,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    Self::set(&this.ep_model, &id, window, cx);
                    cx.notify();
                })),
            );
        }
        let status = match &self.status {
            Status::Idle => None,
            Status::Working(s) => Some((t.colors.text_muted, "icons/loader-circle.svg", s.clone())),
            Status::Ok(s) => Some((t.extra.success, "icons/circle-check.svg", s.clone())),
            Status::Failed(s) => Some((t.extra.danger, "icons/circle-x.svg", s.clone())),
        };
        let working = matches!(self.status, Status::Working(_));
        let title = if self.editing.is_some() {
            "Edit endpoint"
        } else {
            "Add an endpoint"
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(div().id("back").flex().items_center().gap(px(4.)).cursor_pointer().text_size(t.text.caption).text_color(t.colors.primary).child("← Endpoints").on_click(cx.listener(|this, _, _, cx| {
                this.form = false;
                cx.notify();
            })))
            .child(Self::section(t, title, Some("Start from a preset or fill it in. Forge lists the models with your key, checks the one you pick with a short request, then saves it and uses it.")))
            .child(div().flex().flex_col().gap(px(6.)).child(div().text_size(t.text.caption).font_weight(FontWeight::MEDIUM).text_color(t.colors.text_muted).child("Start from")).child(presets).children(found_note.map(|n| div().text_size(t.text.caption).text_color(t.extra.success).child(n))))
            .child(div().flex().gap(px(10.)).child(div().w(px(170.)).child(Self::field(t, "Name", &self.ep_name))).child(div().flex_1().child(Self::field(t, "Base URL", &self.ep_url))))
            .child(div().flex().flex_col().gap(px(6.)).child(div().text_size(t.text.caption).font_weight(FontWeight::MEDIUM).text_color(t.colors.text_muted).child("Protocol")).child(wire_row))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(Self::field(t, "API key", &self.ep_key))
                    .child(div().text_size(t.text.caption).text_color(t.colors.text_subtle).child(if cfg!(windows) { "Kept in Windows Credential Manager, never in a file. Local servers need none." } else { "Kept in a private file in Forge's config folder, never in endpoints.json. Local servers need none." })),
            )
            .child(div().flex().flex_col().gap(px(8.)).child(model_row).when(!self.models.is_empty(), |d| d.child(list)))
            .children(status.map(|(c, i, s)| div().flex().items_start().gap(px(6.)).text_size(t.text.caption).text_color(c).line_height(relative(1.4)).child(div().pt(px(1.)).child(icon(i, t.size.icon_xs, c))).child(div().flex_1().child(s))))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(theme::text_button(t, "Cancel").id("cancel-form").on_click(cx.listener(|this, _, _, cx| {
                        this.form = false;
                        cx.notify();
                    })))
                    .when(self.save_anyway && !working, |d| d.child(theme::ghost_button(t, "Save anyway").id("save-anyway").on_click(cx.listener(|this, _, window, cx| this.check_and_save(true, window, cx)))))
                    .child(theme::primary_button(t, if working { "Checking…" } else { "Check and save" }).id("check-save").when(!working, |d| d.on_click(cx.listener(|this, _, window, cx| this.check_and_save(false, window, cx))))),
            )
            .into_any_element()
    }

    fn about(&self, t: &Theme) -> AnyElement {
        let cli = crate::backend::find_cli();
        let row = |k: &str, v: String| {
            div()
                .flex()
                .gap(px(12.))
                .child(
                    div()
                        .w(px(130.))
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(k.to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(t.text.caption)
                        .font_family(t.font_mono.clone())
                        .text_color(t.colors.text)
                        .child(v),
                )
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(super::forge_cube::finished(t, px(40.), t.colors.primary))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(t.text.subheading)
                                    .font_family(t.font_display.clone())
                                    .text_color(t.colors.text_strong)
                                    .child("Forge"),
                            )
                            .child(
                                div()
                                    .text_size(t.text.caption)
                                    .text_color(t.colors.text_subtle)
                                    .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                            ),
                    ),
            )
            .child(row(
                "Engine",
                cli.map(|p| p.display().to_string()).unwrap_or_else(|| {
                    "ForgeCLI not found: put forge.exe beside ForgeDesktop.exe".into()
                }),
            ))
            .child(row(
                "Config folder",
                crate::profile::dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ))
            .child(row(
                "Endpoints",
                crate::endpoints::path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ))
            .into_any_element()
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let (open, tab) = {
            let s = self.state.read(cx);
            (s.settings_open, s.settings_tab)
        };
        if !open {
            return div().into_any_element();
        }
        let hover = t.colors.hover;
        let mut side = div()
            .id("settings-nav")
            .w(t.size.settings_nav)
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(12.))
            .overflow_y_scroll()
            // GPUI clips children to a rectangle, not the panel's rounded
            // corners: the pane rounds its own, inside the panel's border.
            .rounded_tl(PANEL_RADIUS - t.size.hairline)
            .rounded_bl(PANEL_RADIUS - t.size.hairline)
            .bg(t.extra.pane)
            .border_r_1()
            .border_color(t.colors.border)
            .child(
                div()
                    .px(px(10.))
                    .pt(px(4.))
                    .pb(px(8.))
                    .text_size(t.text.subheading)
                    .font_family(t.font_display.clone())
                    .text_color(t.colors.text_strong)
                    .child("Settings"),
            );
        for (group, items) in SECTIONS {
            side = side.child(
                div()
                    .px(px(10.))
                    .pt(px(10.))
                    .pb(px(2.))
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(*group),
            );
            for (this_tab, path, label) in items.iter().copied() {
                let on = tab == this_tab;
                let state = self.state.clone();
                side = side.child(
                    div()
                        .id(SharedString::from(format!("tab-{label}")))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(8.))
                        .h(t.size.row)
                        .px(px(10.))
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .text_size(t.text.body_sm)
                        .text_color(if on {
                            t.colors.text_strong
                        } else {
                            t.colors.text_muted
                        })
                        .when(on, |d| {
                            d.bg(t.extra.selected).font_weight(FontWeight::MEDIUM)
                        })
                        .when(!on, |d| d.hover(move |s| s.bg(hover)))
                        .child(icon(
                            path,
                            t.size.icon_sm,
                            if on {
                                t.colors.primary
                            } else {
                                t.colors.text_muted
                            },
                        ))
                        .child(label)
                        .on_click(move |_, _, cx| {
                            state.update(cx, |s, cx| s.settings_tab(this_tab, cx))
                        }),
                );
            }
        }
        let body =
            match tab {
                SettingsTab::General => self.general(&t, cx),
                SettingsTab::Models if self.form => self.models_form(&t, cx),
                SettingsTab::Models if self.state.read(cx).diagnosis.is_some() => {
                    let d =
                        self.state.read(cx).diagnosis.as_ref().map(|d| {
                            super::diagnostics::render(&t, d, &self.state).into_any_element()
                        });
                    d.unwrap_or_else(|| div().into_any_element())
                }
                SettingsTab::Models => self.models_list(&t, cx),
                SettingsTab::Commands => self.commands.clone().into_any_element(),
                SettingsTab::Agents => self.agents.clone().into_any_element(),
                SettingsTab::Skills => self.skills.clone().into_any_element(),
                SettingsTab::Hooks => self.hooks.clone().into_any_element(),
                SettingsTab::Mcp => self.mcp.clone().into_any_element(),
                SettingsTab::Permissions => self.permissions.clone().into_any_element(),
                SettingsTab::Sandbox => self.sandbox.clone().into_any_element(),
                SettingsTab::Network => {
                    super::planned::render(&t, &super::planned::NETWORK).into_any_element()
                }
                SettingsTab::Environments => self.env.clone().into_any_element(),
                SettingsTab::Memory => self.memory.clone().into_any_element(),
                SettingsTab::Plugins => self.plugins.clone().into_any_element(),
                SettingsTab::Guide => super::guide::render(&t).into_any_element(),
                SettingsTab::About => self.about(&t),
                _ => div().into_any_element(),
            };
        let close = div()
            .id("settings-close")
            .absolute()
            .top(px(12.))
            .right(px(12.))
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(t.radius.pill)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(icon("icons/x.svg", t.size.icon, t.colors.text_muted))
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)));
        let panel = div()
            .id("settings-panel")
            .occlude()
            .relative()
            .w(px(820.))
            .max_w(relative(0.94))
            .h(px(600.))
            .max_h(relative(0.9))
            .flex()
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .rounded(PANEL_RADIUS)
            .shadow(t.elevation_popover())
            .overflow_hidden()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(side)
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .min_w_0()
                    .overflow_y_scroll()
                    .pl(px(28.))
                    .pr(px(52.))
                    .py(px(24.))
                    .child(body),
            )
            .child(close);
        div()
            .id("settings-scrim")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(t.extra.scrim)
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    let closed_inner = match this.state.read(cx).settings_tab {
                        SettingsTab::Commands => this.commands.update(cx, |e, cx| e.back(cx)),
                        SettingsTab::Agents => this.agents.update(cx, |e, cx| e.back(cx)),
                        SettingsTab::Skills => this.skills.update(cx, |e, cx| e.back(cx)),
                        SettingsTab::Hooks => this.hooks.update(cx, |e, cx| e.back(cx)),
                        SettingsTab::Mcp => this.mcp.update(cx, |e, cx| e.back(cx)),
                        _ => false,
                    };
                    if closed_inner {
                    } else if this.form {
                        this.form = false;
                        cx.notify();
                    } else {
                        this.close(cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close(cx)),
            )
            .child(panel.on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()))
            .into_any_element()
    }
}
