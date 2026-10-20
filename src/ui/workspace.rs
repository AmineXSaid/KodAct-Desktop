//! The window's root view: title bar, sidebar, the conversation (or the
//! welcome screen), the composer, the command palette and the toast. Every
//! app-level action is handled here.

use gpui::component::TitleBar;
use gpui::{
    div, prelude::*, px, AnimationExt, ClipboardItem, Context, Entity, FocusHandle, FontWeight,
    IntoElement, Render, SharedString, Window,
};

use super::chat_view::ChatView;
use super::command_palette::CommandPalette;
use super::composer::Composer;
use super::confirm::ConfirmWindow;
use super::find_bar::FindBar;
use super::onboarding::Onboarding;
use super::projects::ProjectsView;
use super::rules_view::RulesView;
use super::settings::Settings;
use super::side_chat::SideChat;
use super::sidebar::Sidebar;
use super::*;
use crate::state::{AppState, Screen};
use crate::theme::Look;
use crate::theme::{self, ActiveTheme};

pub struct Workspace {
    state: Entity<AppState>,
    sidebar: Entity<Sidebar>,
    chat: Entity<ChatView>,
    composer: Entity<Composer>,
    palette: Entity<CommandPalette>,
    /// The remote quick pick, above the status bar's remote indicator.
    remote_picker: Entity<super::remote_menu::RemotePicker>,
    /// The terminal dock on the right; its shells outlive hiding it.
    terminal: Entity<super::terminal_dock::TerminalDock>,
    onboarding: Entity<Onboarding>,
    rules: Entity<RulesView>,
    projects: Entity<ProjectsView>,
    side: Entity<SideChat>,
    settings: Entity<Settings>,
    find: Entity<FindBar>,
    browser: Entity<super::browser_view::BrowserView>,
    confirm: Entity<ConfirmWindow>,
    /// SSH's password and passphrase prompts.
    askpass: Entity<super::askpass::AskpassDialog>,
    /// The home screen's field of dots, behind the greeting and the composer.
    dots: Entity<super::dot_field::DotField>,
    /// The film as Forge opens, while it plays (`opener.rs`).
    opener: Option<Entity<super::opener::Opener>>,
    _opener_sub: Option<gpui::Subscription>,
    focus: FocusHandle,
}

impl Workspace {
    /// Switch to `look` (the command palette's "Theme: …"), keeping light
    /// or dark, and keep it for the next launch.
    fn pick_look(&mut self, look: Look, cx: &mut Context<Self>) {
        let dark = self.state.read(cx).dark;
        self.state.update(cx, |s, cx| {
            s.look = look;
            s.save_window();
            cx.notify();
        });
        set_theme(look, dark, cx);
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| {
            let mut s = AppState::new();
            s.load(cx);
            s
        });
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        // The theme and mode you left it in.
        {
            let s = state.read(cx);
            let (look, dark) = (s.look, s.dark);
            set_theme(look, dark, cx);
        }
        let sidebar = cx.new(|cx| Sidebar::new(state.clone(), window, cx));
        let chat = cx.new(|cx| ChatView::new(state.clone(), cx));
        let composer = cx.new(|cx| Composer::new(state.clone(), window, cx));
        let palette = cx.new(|cx| CommandPalette::new(state.clone(), window, cx));
        let remote_picker =
            cx.new(|cx| super::remote_menu::RemotePicker::new(state.clone(), window, cx));
        let terminal =
            cx.new(|cx| super::terminal_dock::TerminalDock::new(state.clone(), window, cx));
        let onboarding = cx.new(|cx| Onboarding::new(state.clone(), window, cx));
        let rules = cx.new(|cx| RulesView::new(state.clone(), window, cx));
        let projects = cx.new(|cx| ProjectsView::new(state.clone(), composer.clone(), cx));
        let side = cx.new(|cx| SideChat::new(state.clone(), window, cx));
        let settings = cx.new(|cx| Settings::new(state.clone(), window, cx));
        let find = cx.new(|cx| FindBar::new(state.clone(), window, cx));
        let browser = cx.new(|cx| super::browser_view::BrowserView::new(state.clone(), window, cx));
        let confirm = cx.new(|cx| ConfirmWindow::new(state.clone(), window, cx));
        let askpass = cx.new(|cx| super::askpass::AskpassDialog::new(state.clone(), window, cx));
        let composer_focus = composer.read(cx).focus_handle(cx);
        let dots = cx.new(|cx| super::dot_field::DotField::new(Some(composer_focus), window, cx));
        let first_run = state.read(cx).onboarding;
        // The opener: the full film on a first run, ending on the naming
        // screen; a short one after, ending in the app.
        let (opener, opener_sub) = if super::opener::Opener::plays() {
            use super::opener::{Cut, Opener, OpenerEvent};
            let cut = if first_run { Cut::First } else { Cut::Again };
            let o = cx.new(|cx| Opener::new(cut, window, cx));
            let sub = cx.subscribe_in(
                &o,
                window,
                move |ws: &mut Self, _, ev, window, cx| match ev {
                    OpenerEvent::Handoff if cut == Cut::First => {
                        ws.onboarding.update(cx, |o, cx| o.open_forged(window, cx))
                    }
                    OpenerEvent::Handoff => {}
                    OpenerEvent::Done => {
                        ws.opener = None;
                        ws._opener_sub = None;
                        if cut == Cut::Again {
                            ws.composer.update(cx, |c, cx| c.focus(window, cx));
                        }
                        cx.notify();
                    }
                },
            );
            (Some(o), Some(sub))
        } else {
            (None, None)
        };
        let playing = opener.is_some();
        let ws = Self {
            state,
            sidebar,
            chat,
            composer,
            palette,
            remote_picker,
            terminal,
            onboarding,
            rules,
            projects,
            side,
            settings,
            find,
            browser,
            confirm,
            askpass,
            dots,
            opener,
            _opener_sub: opener_sub,
            focus: cx.focus_handle(),
        };
        if playing {
            // The opener has the keyboard; it hands over when it's done.
        } else if first_run {
            ws.onboarding.update(cx, |o, cx| o.open(window, cx));
        } else {
            ws.composer.update(cx, |c, cx| c.focus(window, cx));
        }
        // The dock was left open: it comes back with a shell in it, and the
        // keyboard stays with the composer.
        if !first_run && ws.state.read(cx).terminal_open {
            ws.terminal.update(cx, |d, cx| d.open(None, window, cx));
            match &ws.opener {
                // The opener keeps the keyboard, so any key skips it.
                Some(o) => o.update(cx, |o, cx| o.take_focus(window, cx)),
                None => ws.composer.update(cx, |c, cx| c.focus(window, cx)),
            }
        }
        ws
    }

    fn focus_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| c.focus(window, cx));
    }

    /// Ctrl+`: show the dock (starting a shell when it has none) and give
    /// it the keyboard, or hide it and hand the keyboard back.
    fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.state.read(cx).terminal_open;
        self.state.update(cx, |s, cx| {
            s.terminal_open = !open;
            s.save_window();
            cx.notify();
        });
        if open {
            self.focus_composer(window, cx);
        } else {
            self.terminal.update(cx, |d, cx| {
                if d.has_tabs() {
                    d.focus(window, cx);
                } else {
                    d.open(None, window, cx);
                }
            });
        }
    }

    /// Ctrl+Shift+`: a new terminal tab, showing the dock.
    fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.terminal_open = true;
            s.save_window();
            cx.notify();
        });
        self.terminal.update(cx, |d, cx| d.open(None, window, cx));
    }

    /// The width the dock asks for: its share of the window, held between
    /// its narrowest and three fifths.
    fn dock_want(&self, window: &Window, cx: &Context<Self>) -> f32 {
        let t = cx.theme();
        let vw = f32::from(window.viewport_size().width);
        (vw * self.state.read(cx).terminal_share)
            .min(vw * 0.6)
            .max(f32::from(t.size.terminal_min))
    }

    /// The dock beside the session, `width` wide, with a grip on its left edge.
    fn dock(&self, width: f32, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = cx.theme().clone();
        let grip = self.state.clone();
        let line = t.colors.primary;
        let pane = div()
            .relative()
            .size_full()
            .flex()
            .border_l_1()
            .border_color(t.colors.border)
            .child(self.terminal.clone())
            .child(
                div()
                    .id("terminal-grip")
                    .absolute()
                    .left(px(-3.))
                    .top_0()
                    .bottom_0()
                    .w(px(6.))
                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                    .hover(move |st| st.bg(line.opacity(0.35)))
                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                        grip.update(cx, |s, cx| {
                            s.terminal_resizing = true;
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
            )
            .into_any_element();
        super::motion::slide_in(pane, width, false, &t)
    }

    fn title_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        // Deferred so it paints above the page under the title bar.
        let switcher = self.state.read(cx).switcher_open.then(|| {
            gpui::deferred(project_switcher(
                &self.state,
                window,
                cx,
                t.size.control + t.space.xs,
            ))
            .with_priority(1)
        });
        let s = self.state.read(cx);
        let hover = t.colors.hover;
        // Icon-only: each says what it does, with its key.
        // Every control in the bar occludes it: on Windows the bar is the
        // window's drag area, and a click on anything not occluding it is
        // taken by the OS as the start of a window move, never a click.
        let button = |id: &'static str, path: &'static str, tip: &'static str| {
            div()
                .id(id)
                .occlude()
                .tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tip).build(window, cx)
                })
                .size(t.size.control)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.pill)
                .cursor_pointer()
                .hover(move |st| st.bg(hover))
                .child(icon(path, t.size.icon, t.colors.text_muted))
        };
        // A flat chrome: the component's own background is a gradient.
        TitleBar::new().h(t.size.titlebar).bg(t.extra.chrome).child(
            div()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .w_full()
                .pr(t.space.sm)
                .child(
                    button(
                        "toggle-sidebar",
                        "icons/panel-left.svg",
                        "Show or hide the sidebar (Ctrl+B)",
                    )
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(ToggleSidebar), cx)
                    })),
                )
                // The project and its branch: here while the sidebar is
                // hidden, at the top of the sidebar's sheet while it shows.
                // A plain chat has no folder to show.
                .when(!s.sidebar_open && s.in_folder(), |d| {
                    d.child(
                        div()
                            .relative()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                if this.state.read(cx).switcher_open {
                                    this.state.update(cx, |s, cx| {
                                        s.switcher_open = false;
                                        cx.notify();
                                    });
                                }
                            }))
                            .child(
                                div()
                                    .id("project")
                                    .occlude()
                                    .flex()
                                    .items_center()
                                    .gap(t.space.xs)
                                    .px(t.space.sm)
                                    .h(t.size.control)
                                    .rounded(t.radius.pill)
                                    .cursor_pointer()
                                    .hover(move |st| st.bg(hover))
                                    .when(s.switcher_open, |d| d.bg(hover))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.colors.text_strong)
                                    .child(super::projects::project_tile(
                                        &t,
                                        &s.project,
                                        &s.project_dir,
                                        px(22.),
                                    ))
                                    .child(s.project.clone())
                                    // A remote project says where it lives.
                                    .children(s.current_remote().map(|r| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.))
                                            .font_weight(FontWeight::NORMAL)
                                            .text_size(t.text.caption)
                                            .text_color(t.colors.text_muted)
                                            .child(icon(
                                                r.target.icon(),
                                                t.size.icon_xs,
                                                t.colors.text_muted,
                                            ))
                                            .child(r.target.label())
                                    }))
                                    .child(icon(
                                        if s.switcher_open {
                                            "icons/chevron-up.svg"
                                        } else {
                                            "icons/chevron-down.svg"
                                        },
                                        t.size.icon_sm,
                                        t.colors.text_subtle,
                                    ))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |s, cx| {
                                            s.switcher_open = !s.switcher_open;
                                            cx.notify();
                                        })
                                    })),
                            )
                            .children(switcher),
                    )
                })
                // The branch, when the folder is a repository.
                .when(!s.branch.is_empty() && !s.sidebar_open, |d| {
                    let branch = s.branch.clone();
                    let copier = self.state.clone();
                    let strong = t.colors.text;
                    d.child(
                        div()
                            .id("branch")
                            .occlude()
                            .flex()
                            .items_center()
                            .gap(t.space.xs)
                            .px(t.space.sm)
                            .py(px(2.))
                            .rounded(t.radius.pill)
                            .bg(t.colors.hover)
                            .cursor_pointer()
                            .hover(move |st| st.text_color(strong))
                            .tooltip(|window, cx| {
                                gpui::component::tooltip::Tooltip::new("Copy the branch name")
                                    .build(window, cx)
                            })
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    branch.to_string(),
                                ));
                                copier.update(cx, |s, cx| {
                                    s.show_toast(format!("Copied {branch}"), cx)
                                });
                            })
                            .text_size(t.text.caption)
                            .font_family(t.font_mono.clone())
                            .text_color(t.colors.text_muted)
                            .child(icon(
                                "icons/git-branch.svg",
                                t.size.icon_xs,
                                t.colors.text_muted,
                            ))
                            .child(s.branch.clone()),
                    )
                })
                .child(div().flex_1())
                .child(
                    div()
                        .id("palette-button")
                        .occlude()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .h(t.size.control)
                        .px(t.space.md)
                        .min_w(px(240.))
                        .rounded(t.radius.pill)
                        .bg(t.colors.hover)
                        .border_1()
                        .border_color(t.colors.hover)
                        .cursor_pointer()
                        .text_color(t.colors.text_subtle)
                        .hover({
                            let (edge, ink, fill) =
                                (t.colors.border, t.colors.text_muted, t.colors.surface);
                            move |st| st.bg(fill).border_color(edge).text_color(ink)
                        })
                        .when(s.palette_open, |d| {
                            d.border_color(t.colors.primary)
                                .text_color(t.colors.text_muted)
                        })
                        .tooltip(|window, cx| {
                            gpui::component::tooltip::Tooltip::new(
                                "Files, chats, commands, models. Start with > # or @ to narrow it",
                            )
                            .build(window, cx)
                        })
                        .child(icon(
                            "icons/search.svg",
                            t.size.icon_sm,
                            t.colors.text_subtle,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .whitespace_nowrap()
                                .child("Search files, chats and commands"),
                        )
                        .child(kbd(&t, shortcut("K")))
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(ToggleCommandPalette), cx)
                        })),
                )
                .child(div().flex_1())
                .children(super::approvals::render(&t, &self.state, window, cx))
                .child({
                    let open = s.terminal_open;
                    let unseen = !open && self.terminal.read(cx).unseen;
                    div()
                        .id("toggle-terminal")
                        .occlude()
                        .relative()
                        .size(t.size.control)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.radius.pill)
                        .cursor_pointer()
                        .when(open, |d| d.bg(t.colors.primary_tint))
                        .when(!open, |d| d.hover(move |st| st.bg(hover)))
                        .child(icon(
                            "icons/terminal.svg",
                            t.size.icon,
                            if open {
                                t.colors.primary
                            } else {
                                t.colors.text_muted
                            },
                        ))
                        .when(unseen, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(4.))
                                    .right(px(4.))
                                    .size(px(6.))
                                    .rounded(t.radius.pill)
                                    .bg(t.colors.primary),
                            )
                        })
                        .tooltip(move |window, cx| {
                            gpui::component::tooltip::Tooltip::new(if unseen {
                                "Terminal: new output (Ctrl+`)"
                            } else {
                                "Terminal (Ctrl+`)"
                            })
                            .build(window, cx)
                        })
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(ToggleTerminal), cx)
                        }))
                })
                .child(
                    button(
                        "toggle-panel",
                        "icons/panel-right.svg",
                        "Side panel: plan, files, browser (Ctrl+Shift+E)",
                    )
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(TogglePanel), cx)
                    })),
                )
                .child(
                    button(
                        "toggle-theme",
                        if t.is_dark {
                            "icons/sun.svg"
                        } else {
                            "icons/moon.svg"
                        },
                        if t.is_dark {
                            "Light theme (Ctrl+Shift+L)"
                        } else {
                            "Dark theme (Ctrl+Shift+L)"
                        },
                    )
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(ToggleTheme), cx)
                    })),
                )
                .child(
                    button("settings", "icons/settings.svg", "Settings (Ctrl+,)").on_click(
                        cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(OpenSettings), cx)
                        }),
                    ),
                ),
        )
    }

    /// A new chat: the greeting in the display serif beside Forge's mark, a
    /// quiet line under it, the composer and the starters, over the field of
    /// dots. The field fills the session behind them and never scrolls.
    fn welcome(&self, _window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let name = s.user_name.clone().unwrap_or_default();
        let name = name.trim();
        let line = crate::greeting::pick(name, crate::greeting::hour_now(), s.greeting_seed);
        let project = s
            .project(s.current_project)
            .map(|p| p.name.to_string())
            .unwrap_or_default();
        let sub = if !s.in_folder() {
            "Ask anything, or pick a folder below to work on code. Forge asks before it changes anything."
                .to_string()
        } else if project.is_empty() {
            "Ask, plan or build. Forge works in your project and asks before it changes anything."
                .to_string()
        } else {
            format!("Working in {project}. Forge asks before it changes anything.")
        };
        div()
            .flex_1()
            .min_h_0()
            .relative()
            .flex()
            .flex_col()
            .child(self.dots.clone())
            .child(
                // A short window scrolls it rather than cutting off the starters.
                div()
                    .id("welcome")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .min_h_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px(t.space.xl)
                            .py(t.space.xxxl)
                            // A little above the middle, where the eye lands.
                            .pb(t.space.xxl * 2.)
                            .child(
                                div()
                                    .w_full()
                                    .max_w(t.size.composer_width + t.space.xxl)
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(t.space.xl)
                                    // The page rises in piece by piece as it opens.
                                    .child(super::motion::row_in_full(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(t.space.snug)
                                            .child(greeting_line(&line, &t))
                                            .child(
                                                div()
                                                    .text_size(t.text.body_sm)
                                                    .text_color(t.colors.text_muted)
                                                    .text_center()
                                                    .child(sub),
                                            ),
                                        "welcome-greeting",
                                        0,
                                        &t,
                                    ))
                                    .child(self.composer.clone())
                                    .child(super::starters::render(
                                        &t,
                                        &self.state,
                                        &self.composer,
                                        cx,
                                    ))
                                    // The dot field arranges itself around this column.
                                    .child(self.dots.read(cx).content_probe()),
                            ),
                    ),
            )
    }

    /// The chat an action from the palette or a key acts on: the one showing.
    fn current_chat(&self, cx: &Context<Self>) -> Option<usize> {
        let s = self.state.read(cx);
        s.chat().map(|c| c.id).filter(|_| s.screen == Screen::Chat)
    }

    /// The title bar's project switcher: every project, then All projects
    /// and Open folder….
    fn toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let t = cx.theme().clone();
        let text: SharedString = self.state.read(cx).toast.clone()?;
        Some(
            div()
                .absolute()
                .bottom(t.size.composer_clearance)
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    theme::popover(&t)
                        .px(t.space.lg)
                        .py(t.space.sm)
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(icon("icons/info.svg", t.size.icon_sm, t.colors.text_muted))
                        .child(text),
                ),
        )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let (sidebar_open, palette_open, onboarding, screen, loading) = (
            s.sidebar_open,
            s.palette_open,
            s.onboarding,
            s.screen,
            s.loading,
        );
        let (find_open, shortcuts_open) = (s.find.is_some(), s.shortcuts_open);
        let empty = s.chat().is_none_or(|c| c.messages.is_empty());
        self.dots.read(cx).set_backdrop(!empty, s.busy());
        let below = s.doc_below;
        let terminal_open = s.terminal_open;
        let side_open = s.doc.is_some() || s.panel.is_some();
        // What is left of the window beside the sidebar is shared by the
        // chat, the side panel and the dock; the chat keeps its narrowest.
        let beside = f32::from(window.viewport_size().width)
            - if sidebar_open {
                f32::from(t.size.sidebar)
            } else {
                f32::from(t.size.rail)
            };
        let dock_want = self.dock_want(window, cx);
        let wide = s
            .doc
            .as_ref()
            .is_some_and(|f| f.is_diagram() || f.office() == Some(crate::office::Kind::Xlsx));
        let (panel_w, dock_w) = share_out(
            beside,
            (side_open && !below).then(|| {
                let after_dock = beside - if terminal_open { dock_want } else { 0. };
                (
                    super::doc_pane::width(after_dock, wide, &t),
                    f32::from(t.size.doc_pane),
                )
            }),
            terminal_open.then(|| (dock_want, f32::from(t.size.terminal_min))),
            f32::from(t.size.chat_min),
        );
        let dock = self.dock(dock_w, cx);

        // The session is a sheet on the cream: 4px corners, paper grain,
        // a gutter to its right and below.
        let main = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .mr(t.space.sm)
            .mb(t.space.xs)
            .rounded(t.radius.card)
            .shadow(t.elevation_grain())
            .overflow_hidden()
            .bg(t.extra.stage);
        // A file opened from a reply, or the setup guide, sits beside the session.
        let pane = {
            let s = self.state.read(cx);
            let main_width = beside - if terminal_open { dock_w } else { 0. };
            let fit = super::doc_pane::Fit {
                width: if s.doc_below { main_width } else { panel_w },
                scale: window.scale_factor(),
                fit: s.doc_fit,
                below: s.doc_below,
                from_panel: s.panel.is_some(),
                text: s.doc_text,
            };
            let (pane_width, pane_below) = (fit.width, fit.below);
            let read = s
                .doc_preview
                .as_ref()
                .filter(|(k, _)| {
                    s.doc
                        .as_ref()
                        .is_some_and(|f| k.split_once('#').map(|(n, _)| n) == Some(f.name.as_ref()))
                })
                .map(|(_, r)| r);
            let panel = s.panel.map(|tab| {
                super::workbench::render(
                    super::workbench::View {
                        tab,
                        messages: s.chat().map_or(&[][..], |c| c.messages.as_slice()),
                        browser: self.browser.clone().into(),
                        agents: s
                            .current
                            .and_then(|c| s.agents.get(&c))
                            .map_or(&[][..], |r| r.as_slice()),
                        agents_open: &s.agents_open,
                        // The logo turns while an agent runs or the reply is still going.
                        logo: if s.busy()
                            || s.current
                                .and_then(|c| s.agents.get(&c))
                                .is_some_and(|r| r.iter().any(|run| run.ended.is_none()))
                        {
                            super::turning_logo(32., "agents-logo")
                        } else {
                            super::logo(32.).into_any_element()
                        },
                        working: s.busy(),
                        copied: s.copied.clone(),
                        width: fit.width,
                        below: s.doc_below,
                        scale: fit.scale,
                    },
                    self.state.clone(),
                    &t,
                )
            });
            s.doc
                .as_ref()
                .map(|f| {
                    super::doc_pane::render(
                        f,
                        self.state.clone(),
                        s.copied.clone(),
                        fit,
                        read,
                        s.doc_sheet,
                        s.doc_pick,
                        &t,
                    )
                })
                .or(panel)
                .map(|p| super::motion::slide_in(p, pane_width, pane_below, &t))
        };
        let main = if screen == Screen::Rules {
            main.child(self.rules.clone())
        } else if matches!(screen, Screen::Projects | Screen::Project(_)) {
            main.child(self.projects.clone())
        } else if empty {
            main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(below, |d| d.flex_col())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .child(self.welcome(window, cx)),
                    )
                    .children(pane),
            )
        } else if loading {
            main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(super::skeleton::transcript(&t)),
            )
            .child(
                div()
                    .flex()
                    .justify_center()
                    .px(t.space.xl)
                    .pb(t.space.lg)
                    .child(
                        div()
                            .w_full()
                            .max_w(t.size.column)
                            .child(self.composer.clone()),
                    ),
            )
        } else {
            let conversation = div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .when(find_open, |d| d.child(self.find.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .relative()
                        // The home screen's field stays behind the chat, faint,
                        // and a slow wave runs through it while Forge replies.
                        .child(self.dots.clone())
                        .child(self.chat.clone())
                        .children(super::tasks::render(&t, &self.state, cx))
                        .with_animation(
                            "transcript-in",
                            gpui::Animation::new(std::time::Duration::from_millis(t.motion_ms)),
                            |d, delta| d.opacity(delta),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_center()
                        .px(t.space.xl)
                        .pb(t.space.lg)
                        .child(
                            div()
                                .w_full()
                                .max_w(t.size.column)
                                .child(self.composer.clone()),
                        ),
                );
            main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(below, |d| d.flex_col())
                    .child(conversation)
                    .children(pane),
            )
        };

        theme::app_root(&t)
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .relative()
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &NewChat, window, cx| {
                this.state.update(cx, |s, cx| s.new_plain_chat(cx));
                this.focus_composer(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowProjectMap, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_map(cx))
            }))
            .on_action(cx.listener(|this, _: &TogglePanel, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_panel(cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleSplit, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.doc_below = !s.doc_below;
                    if s.doc.is_none() {
                        s.show_toast(
                            if s.doc_below {
                                "Files you open from a chat will show below it."
                            } else {
                                "Files you open from a chat will show beside it."
                            },
                            cx,
                        );
                    }
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.sidebar_open = !s.sidebar_open;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &ToggleCommandPalette, window, cx| {
                let open = !this.state.read(cx).palette_open;
                this.state.update(cx, |s, cx| {
                    s.palette_open = open;
                    s.picker_open = false;
                    cx.notify();
                });
                if open {
                    this.palette.update(cx, |p, cx| p.open(window, cx));
                } else {
                    this.focus_composer(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleBrowser, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_browser(cx))
            }))
            // Remote development (`ui/remote_menu.rs`): the sidebar opens so
            // its indicator's menu shows.
            .on_action(cx.listener(|this, _: &OpenRemoteMenu, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.sidebar_open = true;
                    s.toggle_remote_menu(cx)
                })
            }))
            .on_action(cx.listener(|this, _: &ConnectWsl, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_remote_page(crate::state::RemotePage::Wsl, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &ConnectHost, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_remote_page(crate::state::RemotePage::Ssh, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &AttachContainer, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_remote_page(crate::state::RemotePage::Containers, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &ReopenInContainer, _, cx| {
                this.state.update(cx, |s, cx| s.reopen_in_container(cx))
            }))
            .on_action(cx.listener(|this, _: &RebuildContainer, _, cx| {
                this.state.update(cx, |s, cx| s.rebuild_container(cx))
            }))
            .on_action(cx.listener(|this, _: &ReopenLocally, _, cx| {
                this.state.update(cx, |s, cx| s.reopen_locally(cx))
            }))
            .on_action(cx.listener(|this, _: &CloseRemote, _, cx| {
                this.state.update(cx, |s, cx| s.close_remote(cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _, cx| {
                let (dark, look) = (!this.state.read(cx).dark, this.state.read(cx).look);
                this.state.update(cx, |s, cx| {
                    s.dark = dark;
                    s.save_window();
                    cx.notify();
                });
                set_theme(look, dark, cx);
            }))
            .on_action(cx.listener(|this, _: &ThemeCalm, _, cx| this.pick_look(Look::Calm, cx)))
            .on_action(
                cx.listener(|this, _: &ThemeTerracotta, _, cx| {
                    this.pick_look(Look::Terracotta, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &ThemePatina, _, cx| this.pick_look(Look::Patina, cx)))
            .on_action(cx.listener(|this, _: &ThemeInk, _, cx| this.pick_look(Look::Ink, cx)))
            .on_action(cx.listener(|this, _: &ThemeSlate, _, cx| this.pick_look(Look::Slate, cx)))
            .on_action(cx.listener(|this, _: &ThemePlum, _, cx| this.pick_look(Look::Plum, cx)))
            .on_action(
                cx.listener(|this, _: &FocusComposer, window, cx| this.focus_composer(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleModelPicker, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.picker_open = !s.picker_open;
                    s.modes_open = false;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &CycleMode, _, cx| {
                this.state.update(cx, |s, cx| s.cycle_mode(cx))
            }))
            .on_action(cx.listener(|this, _: &StopAgents, _, cx| {
                this.state.update(cx, |s, cx| s.stop_agents(cx))
            }))
            .on_action(cx.listener(|this, _: &StopReply, window, cx| {
                let (palette, picker) = {
                    let s = this.state.read(cx);
                    (s.palette_open, s.picker_open)
                };
                this.state.update(cx, |s, cx| {
                    // Esc closes what is open, the newest first, before it stops a reply.
                    if s.map_open {
                        s.toggle_map(cx);
                    } else if s.shortcuts_open
                        || s.chat_menu.is_some()
                        || s.approvals_open
                        || s.context_open
                        || s.tasks_open
                        || s.versions_open.is_some()
                    {
                        s.shortcuts_open = false;
                        s.chat_menu = None;
                        s.approvals_open = false;
                        s.context_open = false;
                        s.tasks_open = false;
                        if let Some((chat, msg)) = s.versions_open {
                            s.toggle_versions(chat, msg, cx);
                        }
                        cx.notify();
                    } else if s.find.is_some() && !s.busy() {
                        s.close_find(cx);
                    } else if palette || picker {
                        s.palette_open = false;
                        s.picker_open = false;
                        cx.notify();
                    } else if s.doc.is_some() && !s.busy() {
                        s.close_doc(cx);
                    } else {
                        s.stop(cx);
                    }
                });
                if palette {
                    this.focus_composer(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::General, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenSlashCommands, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Commands, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenAgents, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Agents, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenSkills, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Skills, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenHooks, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Hooks, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenMcpServers, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Mcp, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenPermissions, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Permissions, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenProfiles, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Profiles, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenGuide, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Guide, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &CopyLastReply, _, cx| {
                let reply = this.state.read(cx).last_reply();
                this.state.update(cx, |s, cx| match reply {
                    Some(text) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        s.show_toast("Copied the last reply.", cx);
                    }
                    None => s.show_toast("No reply to copy yet.", cx),
                });
            }))
            .on_action(cx.listener(|this, _: &ChangeName, window, cx| {
                this.state.update(cx, |s, cx| {
                    s.onboarding = true;
                    s.palette_open = false;
                    cx.notify();
                });
                this.onboarding.update(cx, |o, cx| o.open(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ChangeAvatar, window, cx| {
                this.state.update(cx, |s, cx| {
                    s.onboarding = true;
                    s.palette_open = false;
                    cx.notify();
                });
                this.onboarding
                    .update(cx, |o, cx| o.open_avatar(window, cx));
            }))
            .on_action(cx.listener(|this, _: &OpenProjects, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.screen = Screen::Projects;
                    s.palette_open = false;
                    s.switcher_open = false;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &OpenRules, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.screen = Screen::Rules;
                    s.palette_open = false;
                    s.picker_open = false;
                    cx.notify();
                });
                this.rules.update(cx, |r, cx| r.reload(cx));
            }))
            .on_action(cx.listener(|this, _: &EndpointGuide, _, cx| {
                this.state.update(cx, |s, cx| s.add_endpoint(cx))
            }))
            .on_action(cx.listener(|this, _: &OpenEndpointsFile, _, cx| {
                this.state.update(cx, |s, cx| s.open_endpoints_file(cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleFind, window, cx| {
                if this.current_chat(cx).is_some() {
                    this.find.update(cx, |f, cx| f.open(window, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleTasks, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_tasks(cx))
            }))
            .on_action(cx.listener(|this, _: &ShowApprovals, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_approvals(cx))
            }))
            .on_action(cx.listener(|this, _: &ExportChatJson, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.export_chat_json(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ShareChat, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.share_chat(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ShowContext, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_context(cx))
            }))
            .on_action(cx.listener(|this, _: &AttachFiles, _, cx| {
                super::attachments::pick(this.state.clone(), cx)
            }))
            .on_action(cx.listener(|this, _: &JumpToLatest, _, cx| {
                this.chat.update(cx, |c, cx| c.jump_to_latest(cx))
            }))
            .on_action(cx.listener(|this, _: &ShowShortcuts, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.shortcuts_open = !s.shortcuts_open;
                    s.palette_open = false;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &RenameChat, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.start_rename(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &DuplicateChat, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.duplicate_chat(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ExportChat, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.export_chat(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &DeleteChat, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.ask_delete(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &RegenerateReply, _, cx| {
                this.state.update(cx, |s, cx| s.regenerate_last(cx))
            }))
            .on_action(cx.listener(|this, _: &ContinueReply, _, cx| {
                this.state.update(cx, |s, cx| s.continue_reply(cx))
            }))
            .on_action(
                cx.listener(|this, _: &ToggleTerminal, window, cx| {
                    this.toggle_terminal(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &NewTerminal, window, cx| this.new_terminal(window, cx)),
            )
            .on_action(cx.listener(|this, _: &FocusTerminal, window, cx| {
                if this.state.read(cx).terminal_open && this.terminal.read(cx).has_tabs() {
                    this.terminal.update(cx, |d, cx| d.focus(window, cx));
                } else {
                    this.state.update(cx, |s, _| s.terminal_open = false);
                    this.toggle_terminal(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ShowAgentOutput, window, cx| {
                let feed = this.state.update(cx, |s, _| s.take_agent_feed());
                let Some(feed) = feed else {
                    this.state
                        .update(cx, |s, cx| s.show_toast("No command in this chat yet.", cx));
                    return;
                };
                this.state.update(cx, |s, cx| {
                    s.terminal_open = true;
                    s.save_window();
                    cx.notify();
                });
                this.terminal
                    .update(cx, |d, cx| d.show_feed(feed, window, cx));
            }))
            .on_action(cx.listener(|this, _: &ClearTerminal, _, cx| {
                this.terminal.update(cx, |d, cx| d.clear(cx))
            }))
            .on_action(cx.listener(|this, _: &KillTerminal, _, cx| {
                this.terminal.update(cx, |d, cx| d.kill(cx))
            }))
            .on_action(cx.listener(|this, _: &OpenAppSettings, _, cx| {
                this.state.update(cx, |s, cx| s.open_app_settings(cx))
            }))
            .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, window, cx| {
                if !this.state.read(cx).terminal_resizing {
                    return;
                }
                let vw = f32::from(window.viewport_size().width);
                let share = ((vw - f32::from(ev.position.x)) / vw).clamp(0.05, 0.6);
                this.state.update(cx, |s, cx| {
                    s.terminal_share = share;
                    cx.notify();
                });
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        if s.terminal_resizing {
                            s.terminal_resizing = false;
                            s.save_window();
                            cx.notify();
                        }
                    })
                }),
            )
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .child(self.title_bar(window, cx))
            .children(super::status_bar::banner(
                self.state.read(cx),
                self.state.clone(),
                &t,
            ))
            .when(onboarding, |d| d.child(self.onboarding.clone()))
            .when(!onboarding, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .bg(t.extra.pane)
                        .map(|d| {
                            if sidebar_open {
                                d.child(self.sidebar.clone())
                            } else {
                                d.child(super::sidebar::rail(&t, screen))
                            }
                        })
                        .child(main)
                        .when(terminal_open, |d| d.child(dock)),
                )
                .child(super::status_bar::render(
                    self.state.read(cx),
                    self.state.clone(),
                    self.remote_picker.clone(),
                    &t,
                ))
            })
            .when(palette_open, |d| d.child(self.palette.clone()))
            .child(self.side.clone())
            .child(self.settings.clone())
            .when(self.state.read(cx).map_open, |d| {
                d.child(super::project_map::render(&t, &self.state, cx))
            })
            .when(shortcuts_open, |d| {
                d.child(super::shortcuts::render(&t, &self.state))
            })
            .child(self.confirm.clone())
            .child(self.askpass.clone())
            .children(self.toast(cx))
            .children(self.opener.clone())
    }
}

/// The widths of the side panel and the dock, each given as (wanted,
/// narrowest) when it is open, in `avail` beside the sidebar. Each gets what
/// it wants while the chat keeps `chat_min`; past that the panel gives way
/// first, then the dock, neither below its narrowest.
fn share_out(
    avail: f32,
    panel: Option<(f32, f32)>,
    dock: Option<(f32, f32)>,
    chat_min: f32,
) -> (f32, f32) {
    let (mut p, p_min) = panel.unwrap_or((0., 0.));
    let (mut d, d_min) = dock.unwrap_or((0., 0.));
    let over = p + d + chat_min - avail;
    if over > 0. {
        let from_panel = over.min((p - p_min).max(0.));
        p -= from_panel;
        d -= (over - from_panel).min((d - d_min).max(0.));
    }
    (p, d)
}

/// The greeting, in the display serif, after Forge's mark: an editorial
/// line, not a banner.
fn greeting_line(line: &str, t: &crate::theme::Theme) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .justify_center()
        .items_center()
        .gap(t.space.md)
        // Centred on the capitals rather than the line box, whose descender
        // room would otherwise leave the mark sitting high.
        .child(
            div()
                .flex_none()
                .mt(px(3.))
                .child(super::logo(f32::from(t.size.mark_home))),
        )
        .child(
            div()
                .min_w_0()
                .font_family(t.font_display.clone())
                .font_weight(t.display_weight)
                .text_size(t.size.home_heading)
                .text_color(t.colors.text_strong)
                .line_height(gpui::relative(1.1))
                .child(line.to_string()),
        )
}

/// The project switcher's menu: every project, then All projects and New
/// project…; placed `top` below its opener's container. The title bar opens
/// it, or the sidebar's project header when the sidebar shows.
pub fn project_switcher(
    state: &Entity<AppState>,
    window: &Window,
    cx: &App,
    top: Pixels,
) -> impl IntoElement + use<> {
    let t = cx.theme().clone();
    let s = state.read(cx);
    let home = crate::rules::home_dir();
    let hover = t.colors.hover;
    let mut projects: Vec<_> = s.projects.iter().filter(|p| p.saved).cloned().collect();
    projects.sort_by_key(|p| (p.id != s.current_project, !p.pinned, p.opened_minutes_ago));
    let connected: Vec<_> = s
        .projects
        .iter()
        .filter(|p| !p.saved && !p.scratch)
        .cloned()
        .collect();
    let row = |id: SharedString, current: bool| {
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(t.space.md)
            .px(t.space.sm)
            .py(t.space.xs + px(2.))
            .rounded(t.radius.button)
            .cursor_pointer()
            .when(current, |d| d.bg(t.extra.selected))
            .hover(move |st| st.bg(hover))
    };
    let label = |text: &'static str| {
        div()
            .px(t.space.sm)
            .pt(t.space.sm)
            .pb(t.space.xs)
            .text_size(t.text.caption)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.text_subtle)
            .child(text)
    };
    let entry = |p: &crate::state::Project, meta: String, current: bool| {
        let (state, id) = (state.clone(), p.id);
        let tip: SharedString = crate::rules::display_path(&p.path, home.as_deref()).into();
        row(format!("switch-{}", p.id).into(), current)
            .tooltip(move |w, cx| gpui::component::tooltip::Tooltip::new(tip.clone()).build(w, cx))
            .child(super::projects::project_tile(&t, &p.name, &p.path, px(28.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.colors.text_strong)
                            .child(p.name.clone()),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(meta),
                    ),
            )
            .child(div().w(t.size.icon).when(current, |d| {
                d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.primary))
            }))
            .on_click(move |_, _, cx| {
                state.update(cx, |s, cx| {
                    s.select_project(id, cx);
                    if !matches!(s.screen, Screen::Rules) {
                        s.screen = Screen::Chat;
                    }
                })
            })
    };
    let mut list = div().flex().flex_col().gap(px(2.)).child(label("Projects"));
    // Rows rise in one after another as the menu opens.
    let mut i = 0;
    for p in &projects {
        let chats = s.chats.iter().filter(|c| c.project == p.id).count();
        let current = p.id == s.current_project;
        let e = entry(
            p,
            super::projects::project_meta(chats, current, p.opened_minutes_ago),
            current,
        );
        list = list.child(super::motion::row_in(e, ("switch-in", i), i, &t));
        i += 1;
    }
    // Folders you're connected to that aren't projects: here to come back to.
    if !connected.is_empty() {
        list = list.child(label("Connected"));
        for p in &connected {
            let place = crate::remote::Remote::parse(&p.path)
                .map(|r| r.target.label())
                .unwrap_or_default();
            let current = p.id == s.current_project;
            let e = entry(p, place, current);
            list = list.child(super::motion::row_in(e, ("switch-in", i), i, &t));
            i += 1;
        }
    }
    let (all, open) = (state.clone(), state.clone());
    let menu = theme::fit_window(
        theme::popover(&t).id("switcher"),
        window,
        &t,
        t.size.titlebar,
        None,
    )
    .absolute()
    .top(top)
    .left_0()
    .w(px(340.))
    .p(t.space.xs)
    .child(list)
    .child(theme::rule(&t).my(t.space.xs))
    .child(
        row("switch-all".into(), false)
            .child(
                div()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        "icons/layout-grid.svg",
                        t.size.icon_sm,
                        t.colors.text_muted,
                    )),
            )
            .child(div().text_color(t.colors.text).child("All projects"))
            .on_click(move |_, _, cx| {
                all.update(cx, |s, cx| {
                    s.switcher_open = false;
                    s.screen = Screen::Projects;
                    cx.notify();
                })
            }),
    )
    .child(
        row("switch-open".into(), false)
            .child(
                div()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("icons/plus.svg", t.size.icon_sm, t.colors.text_muted)),
            )
            .child(div().text_color(t.colors.text).child("New project…"))
            .on_click(move |_, _, cx| {
                open.update(cx, |s, cx| {
                    s.switcher_open = false;
                    cx.notify();
                });
                super::projects::open_folder(open.clone(), cx);
            }),
    );
    super::motion::edge_draw(menu, "switcher-in", t.colors.primary, 0., &t)
}

#[cfg(test)]
mod tests {
    use super::share_out;

    #[test]
    fn the_chat_keeps_its_narrowest_and_the_panel_gives_way_first() {
        // Room for all: each gets what it wants.
        assert_eq!(
            share_out(2000., Some((600., 460.)), Some((500., 280.)), 400.),
            (600., 500.)
        );
        // Tight: the panel shrinks to its narrowest before the dock gives way.
        assert_eq!(
            share_out(1300., Some((600., 460.)), Some((500., 280.)), 400.),
            (460., 440.)
        );
        // Tighter still: both at their narrowest, the chat takes what's left.
        assert_eq!(
            share_out(900., Some((600., 460.)), Some((500., 280.)), 400.),
            (460., 280.)
        );
        // Only the dock.
        assert_eq!(share_out(1000., None, Some((700., 280.)), 400.), (0., 600.));
    }
}
