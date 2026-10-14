//! Settings → Profiles: named sets of ForgeCLI settings, one in use
//! (`setting_profiles.rs`). Make one (it starts empty), use it, open its
//! file to fill it, or delete it. The one in use reaches each chat's
//! ForgeCLI as `--settings`, above your settings, the project's and this
//! PC's: what it sets wins, the rest falls back.

use gpui::component::input::InputState;
use gpui::{
    div, prelude::*, px, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};

use super::editor_kit as kit;
use super::icon;
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

pub struct ProfilesEditor {
    state: Entity<AppState>,
    name: Entity<InputState>,
    error: Option<SharedString>,
    confirm: Option<String>,
    _subs: Vec<Subscription>,
}

impl ProfilesEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("client-work"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &name,
                window,
                |this, _, ev: &gpui::component::input::InputEvent, window, cx| {
                    if matches!(ev, gpui::component::input::InputEvent::PressEnter { .. }) {
                        this.create(window, cx);
                    }
                },
            ),
        ];
        Self {
            state,
            name,
            error: None,
            confirm: None,
            _subs: subs,
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        if let Err(e) = crate::forge_files::check_name(&name, false) {
            self.error = Some(e.into());
            return cx.notify();
        }
        self.error = None;
        self.name.update(cx, |i, cx| i.set_value("", window, cx));
        let n = name.clone();
        let task = self.state.update(cx, |s, cx| {
            s.engine_write(
                format!("Made the {name} profile. It's empty until you fill it."),
                move |_| crate::setting_profiles::create(&n).map(|_| ()),
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            if let Err(e) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.error = Some(e.into());
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

impl Render for ProfilesEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t: Theme = cx.theme().clone();
        let s = self.state.read(cx);
        let (profiles, active, loaded) = (
            s.engine.profiles.clone(),
            s.settings_profile.clone(),
            s.engine.catalog.is_some(),
        );
        let mut out = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(kit::header(
                &t,
                "Profiles",
                "A profile is a set of ForgeCLI settings you can switch to, such as stricter permissions for client work. What it sets wins over your settings files; what it doesn't set falls back to them.",
                None,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().child(kit::line(&t, &self.name, true)))
                    .child(
                        theme::primary_button(&t, "New profile")
                            .id("new-profile")
                            .on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
                    ),
            )
            .children(kit::status(&t, &self.error));
        if !loaded {
            return out.child(kit::loading(&t));
        }
        // "None": just the settings files.
        let none_on = active.is_none();
        let st = self.state.clone();
        out = out.child(
            kit::card(&t)
                .when(none_on, |d| d.border_color(t.colors.primary.opacity(0.5)))
                .child(icon(
                    "icons/sliders-horizontal.svg",
                    t.size.icon,
                    t.colors.text_muted,
                ))
                .child(kit::card_text(
                    &t,
                    "No profile",
                    false,
                    Some("Your settings, the project's and this PC's, as they are".into()),
                    None,
                ))
                .child(if none_on {
                    theme::pill(&t, t.colors.primary_tint, t.colors.primary, "In use")
                        .into_any_element()
                } else {
                    kit::chip(&t, "use-no-profile", "Use", false)
                        .on_click(move |_, _, cx| {
                            st.update(cx, |s, cx| s.use_settings_profile(None, cx))
                        })
                        .into_any_element()
                }),
        );
        if profiles.is_empty() {
            out = out.child(kit::empty(
                &t,
                "No profiles yet. Make one, open its file, and put in the settings that differ: permissions, env, sandbox, hooks.",
            ));
        }
        for p in profiles {
            let on = active.as_deref() == Some(p.name.as_str());
            if self.confirm.as_deref() == Some(p.name.as_str()) {
                let name = p.name.clone();
                out = out.child(kit::confirm_row(
                    &t,
                    format!("Delete the {} profile?", p.name),
                    if on {
                        "It's in use: chats go back to no profile.".to_string()
                    } else {
                        "Its file goes; your settings files stay as they are.".to_string()
                    },
                    kit::cancel_button(&t, "cancel-delete").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.confirm = None;
                            cx.notify();
                        },
                    )),
                    theme::danger_button(&t, "Delete")
                        .id("confirm-delete")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.confirm = None;
                            let n = name.clone();
                            let task = this.state.update(cx, |s, cx| {
                                s.engine_write(
                                    format!("Deleted the {n} profile."),
                                    move |_| crate::setting_profiles::delete(&n),
                                    cx,
                                )
                            });
                            let was_on = on;
                            cx.spawn(async move |this, cx| {
                                if task.await.is_ok() && was_on {
                                    let _ = this.update(cx, |this, cx| {
                                        this.state
                                            .update(cx, |s, cx| s.use_settings_profile(None, cx))
                                    });
                                }
                            })
                            .detach();
                        })),
                ));
                continue;
            }
            let detail = if p.keys.is_empty() {
                "Empty: everything falls back to your settings files".to_string()
            } else {
                format!("Sets {}", p.keys.join(", "))
            };
            let note = p.problem.clone().map(|e| (t.extra.danger, e));
            let (path, use_name, del_name, st) = (
                p.path.clone(),
                p.name.clone(),
                p.name.clone(),
                self.state.clone(),
            );
            out = out.child(
                kit::card(&t)
                    .when(on, |d| d.border_color(t.colors.primary.opacity(0.5)))
                    .child(icon("icons/layers.svg", t.size.icon, t.colors.text_muted))
                    .child(kit::card_text(
                        &t,
                        p.name.clone(),
                        false,
                        Some(detail),
                        note,
                    ))
                    .child(if on {
                        theme::pill(&t, t.colors.primary_tint, t.colors.primary, "In use")
                            .into_any_element()
                    } else {
                        kit::chip(
                            &t,
                            SharedString::from(format!("use-profile-{}", p.name)),
                            "Use",
                            false,
                        )
                        .on_click(move |_, _, cx| {
                            st.update(cx, |s, cx| {
                                s.use_settings_profile(Some(use_name.clone()), cx)
                            })
                        })
                        .into_any_element()
                    })
                    .child(
                        kit::icon_button(
                            &t,
                            SharedString::from(format!("open-profile-{}", p.name)),
                            "icons/file-pen.svg",
                            "Open its file to edit it",
                            false,
                        )
                        .on_click(move |_, _, cx| cx.open_with_system(&path)),
                    )
                    .child(
                        kit::icon_button(
                            &t,
                            SharedString::from(format!("rm-profile-{}", p.name)),
                            "icons/trash.svg",
                            "Delete",
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.confirm = Some(del_name.clone());
                            cx.notify();
                        })),
                    ),
            );
        }
        out.child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .line_height(gpui::relative(1.4))
                .child("A profile file takes the same keys as settings.json. Switching restarts each idle chat's ForgeCLI on its next message, resuming its conversation."),
        )
    }
}
