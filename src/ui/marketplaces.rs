//! Settings → Plugins → Marketplaces: Claude Code plugin marketplaces (a
//! repository with `.claude-plugin/marketplace.json`), as `/plugin
//! marketplace add` and `/plugin install` work there. Add one by
//! `owner/repo`, a git URL or a folder; install a plugin from it, and Forge
//! copies it under ForgeCLI's config folder (`plugins/<marketplace>/<plugin>`)
//! and lists it in your `pluginDirs`, so every new chat loads it.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::component::input::InputState;
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::icon;
use crate::skill_install::{self as si, MarketPlugin, Marketplace};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

/// A marketplace read: its list, and the folder it was fetched into (for its
/// own plugins' folders).
#[derive(Clone)]
struct Read {
    market: Marketplace,
    root: PathBuf,
}

pub struct Marketplaces {
    state: Entity<AppState>,
    source: Entity<InputState>,
    /// Each added marketplace by name: read, failed, or still being read (absent).
    read: HashMap<String, Result<Read, String>>,
    reading: bool,
    busy: Option<String>,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl Marketplaces {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let source = cx.new(|cx| {
            InputState::new(window, cx).placeholder("owner/repo, a git URL, or a folder")
        });
        Self {
            _subs: vec![
                cx.observe(&state, |_, _, cx| cx.notify()),
                cx.subscribe_in(
                    &source,
                    window,
                    |this, _, ev: &gpui::component::input::InputEvent, window, cx| {
                        if matches!(ev, gpui::component::input::InputEvent::PressEnter { .. }) {
                            this.add(window, cx);
                        }
                    },
                ),
            ],
            source,
            state,
            read: HashMap::new(),
            reading: false,
            busy: None,
            error: None,
        }
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.state.update(cx, |s, cx| {
            s.plugins_market = false;
            cx.notify();
        });
    }

    /// Fetch every added marketplace's list (they change: read each time the panel opens).
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            return;
        }
        self.reading = true;
        let cwd = self.state.read(cx).project_dir.clone();
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_spawn(async move {
                    si::known_marketplaces()
                        .into_iter()
                        .map(|(name, source)| {
                            let r = si::parse_source(&source, &cwd).and_then(|src| {
                                let root = si::fetch(&src, &si::work_dir())?;
                                Ok(Read {
                                    market: si::read_marketplace(&root)?,
                                    root,
                                })
                            });
                            (name, r)
                        })
                        .collect::<HashMap<_, _>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.read = got;
                cx.notify();
            });
        })
        .detach();
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.source.read(cx).value().trim().to_string();
        let cwd = self.state.read(cx).project_dir.clone();
        let src = match si::parse_source(&text, &cwd) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e.into());
                return cx.notify();
            }
        };
        self.busy = Some("Adding…".into());
        self.error = None;
        cx.spawn_in(window, async move |this, cx| {
            let r = cx
                .background_spawn(async move {
                    let root = si::fetch(&src, &si::work_dir())?;
                    let market = si::read_marketplace(&root)
                        .map_err(|_| "That isn't a plugin marketplace: it has no .claude-plugin/marketplace.json.".to_string())?;
                    si::remember_marketplace(&market.name, Some(&src.label()))?;
                    Ok::<_, String>(Read { market, root })
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = None;
                match r {
                    Ok(read) => {
                        this.source.update(cx, |i, cx| i.set_value("", window, cx));
                        let name = read.market.name.clone();
                        this.read.insert(name.clone(), Ok(read));
                        this.state.update(cx, |s, cx| s.show_toast(format!("Added the {name} marketplace."), cx));
                    }
                    Err(e) => this.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn forget(&mut self, name: String, cx: &mut Context<Self>) {
        match si::remember_marketplace(&name, None) {
            Ok(()) => {
                self.read.remove(&name);
                self.state.update(cx, |s, cx| {
                    s.show_toast(
                        format!("Removed the {name} marketplace. Plugins installed from it stay."),
                        cx,
                    )
                });
            }
            Err(e) => self.error = Some(e.into()),
        }
        cx.notify();
    }

    fn install(&mut self, read: &Read, plugin: &MarketPlugin, cx: &mut Context<Self>) {
        let Some(root) = si::plugins_root() else {
            return;
        };
        let source = si::known_marketplaces()
            .into_iter()
            .find(|(n, _)| *n == read.market.name)
            .map(|(_, s)| s)
            .unwrap_or_default();
        let (read, plugin) = (read.clone(), plugin.clone());
        let name = plugin.name.clone();
        self.busy = Some(name.clone());
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Installed the {name} plugin. New chats load it."),
                move |project| {
                    let work = si::work_dir();
                    let r = si::install_plugin(
                        &plugin,
                        &read.market,
                        &read.root,
                        &source,
                        &root,
                        &work,
                    )
                    .and_then(|dir| si::add_plugin_dir(project, &dir));
                    let _ = std::fs::remove_dir_all(&work);
                    r
                },
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            let r = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(e) = r {
                    this.error = Some(e.into());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn remove(&mut self, dir: PathBuf, name: String, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Removed the {name} plugin."),
                move |project| {
                    si::remove_plugin_dir(project, &dir)?;
                    si::uninstall(&dir)
                },
                cx,
            )
            .detach()
        });
    }

    fn plugin_card(
        &self,
        t: &Theme,
        read: &Read,
        p: &MarketPlugin,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let dir = si::plugins_root().map(|r| {
            r.join(crate::downloads::clean_name(&read.market.name))
                .join(crate::downloads::clean_name(&p.name))
        });
        let installed = dir.as_ref().is_some_and(|d| d.is_dir());
        let key = format!("{}-{}", read.market.name, p.name);
        let busy = self.busy.as_deref() == Some(p.name.as_str());
        let title = match &p.version {
            Some(v) => format!("{} {v}", p.name),
            None => p.name.clone(),
        };
        let (r1, p1, r2, p2) = (read.clone(), p.clone(), read.clone(), p.clone());
        let mut card = kit::card(t)
            .child(icon("icons/puzzle.svg", t.size.icon, t.colors.text_muted))
            .child(kit::card_text(
                t,
                title,
                true,
                Some(p.description.clone()),
                None,
            ));
        if installed {
            let (dir, name) = (dir.clone().unwrap_or_default(), p.name.clone());
            card =
                card.child(kit::scope_pill(t, "Installed"))
                    .child(
                        kit::icon_button(
                            t,
                            SharedString::from(format!("update-plugin-{key}")),
                            "icons/refresh-cw.svg",
                            "Update",
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.install(&r1, &p1, cx))),
                    )
                    .child(
                        kit::icon_button(
                            t,
                            SharedString::from(format!("rm-plugin-{key}")),
                            "icons/trash.svg",
                            "Remove",
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove(dir.clone(), name.clone(), cx)
                        })),
                    );
        } else {
            card = card.child(
                theme::ghost_button(t, if busy { "Installing…" } else { "Install" })
                    .id(SharedString::from(format!("install-plugin-{key}")))
                    .when(busy, |d| d.opacity(0.6))
                    .when(!busy, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| this.install(&r2, &p2, cx)))
                    }),
            );
        }
        card.into_any_element()
    }
}

impl Render for Marketplaces {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let known = si::known_marketplaces();
        if !self.reading && !known.is_empty() && self.read.is_empty() {
            self.refresh(cx);
        }
        let adding = self.busy.as_deref() == Some("Adding…");
        let mut out = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(kit::back(&t, "market-back", "Plugins").on_click(cx.listener(|this, _, _, cx| this.close(cx))))
            .child(kit::header(
                &t,
                "Marketplaces",
                "A marketplace is a repository listing plugins (.claude-plugin/marketplace.json), the same ones Claude Code installs with /plugin. Install one here and every new chat loads it.",
                None,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(kit::line(&t, &self.source, true)))
                    .child(
                        theme::primary_button(&t, if adding { "Adding…" } else { "Add marketplace" })
                            .id("add-market")
                            .when(adding, |d| d.opacity(0.6))
                            .when(!adding, |d| d.on_click(cx.listener(|this, _, window, cx| this.add(window, cx)))),
                    ),
            )
            .children(kit::status(&t, &self.error));
        if known.is_empty() {
            return out.child(kit::empty(
                &t,
                "No marketplaces yet. Add one by its repository (owner/repo) to browse and install its plugins.",
            ));
        }
        let project = self.state.read(cx).project_dir.clone();
        for (name, source) in known {
            let forget = name.clone();
            // `github.com/owner/repo`, or a folder as people read it.
            let from = match source.split_once("://") {
                Some((_, rest)) => rest.to_string(),
                None => kit::short(std::path::Path::new(&source), &project),
            };
            out = out.child(
                div()
                    .flex()
                    .items_end()
                    .gap(t.space.sm)
                    .child(kit::group(&t, name.clone()).flex_none())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(from),
                    )
                    .child(
                        kit::icon_button(
                            &t,
                            SharedString::from(format!("forget-market-{name}")),
                            "icons/x.svg",
                            "Remove this marketplace (installed plugins stay)",
                            true,
                        )
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.forget(forget.clone(), cx)),
                        ),
                    ),
            );
            match self.read.get(&name).cloned() {
                None => out = out.child(kit::loading(&t)),
                Some(Err(e)) => {
                    out = out.child(
                        div()
                            .text_size(t.text.caption)
                            .text_color(t.extra.danger)
                            .child(format!("Couldn't read it: {e}")),
                    )
                }
                Some(Ok(read)) => {
                    if read.market.plugins.is_empty() {
                        out = out.child(
                            div()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child("It lists no plugins."),
                        );
                    }
                    let cards: Vec<gpui::AnyElement> = read
                        .market
                        .plugins
                        .iter()
                        .map(|p| self.plugin_card(&t, &read, p, cx))
                        .collect();
                    out = out.children(cards);
                }
            }
        }
        out
    }
}
