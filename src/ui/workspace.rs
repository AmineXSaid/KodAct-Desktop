//! The window's root view: title bar, sidebar, the conversation (or the
//! welcome screen), the composer, the command palette and the toast. Every
//! app-level action is handled here.

use gpui::component::TitleBar;
use gpui::{
    div, prelude::*, px, AnimationExt, ClipboardItem, Context, Entity, FocusHandle, FontWeight,
    IntoElement, Render, SharedString, Window,
};

use super::chat_view::ChatView;

/// The Files pane's share of the side column, under a file open above it.
const FILES_SHARE: f32 = 0.5;
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
use crate::state::{AppState, PanelTab, Screen, TitleMenu};
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
    design_home: Entity<super::design_home::DesignHome>,
    pet: Entity<super::pet::PetView>,
    projects: Entity<ProjectsView>,
    side: Entity<SideChat>,
    settings: Entity<Settings>,
    find: Entity<FindBar>,
    browser: Entity<super::browser_view::BrowserView>,
    confirm: Entity<ConfirmWindow>,
    attachment_editor: Entity<super::attachment_editor::AttachmentEditor>,
    /// SSH's password and passphrase prompts.
    askpass: Entity<super::askpass::AskpassDialog>,
    /// Comments on the file in the side panel (`doc_comments.rs`).
    doc_comments: Entity<super::doc_comments::DocComments>,
    /// Find in the file in the side panel (`doc_find.rs`).
    doc_find: Entity<super::doc_find::DocFind>,
    /// The file view's page field (`page_field.rs`).
    page_field: Entity<super::page_field::PageField>,
    /// The file view's text editor (`text_editor.rs`).
    text_editor: Entity<super::text_editor::TextEditor>,
    /// A Word file's or deck's words as fields (`office_editor.rs`).
    office_editor: Entity<super::office_editor::OfficeEditor>,
    /// Interactive spreadsheet editor bar (`sheet_view.rs`).
    sheet_editor: Entity<super::sheet_view::SheetEditor>,
    /// The project's tree and a viewer, under the side panel (`files_pane.rs`).
    files: Entity<super::files_pane::FilesPane>,
    /// The windows drawn here, kept for the beat each takes to leave.
    presence: Presences,
    focus: FocusHandle,
}

impl Workspace {
    /// Switch to `look` (the command palette's "Theme: …"), keeping light
    /// or dark, and keep it for the next launch.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| {
            let mut s = AppState::new();
            s.load(cx);
            s.prepare_doc_tools(cx);
            s.start_routines(cx);
            s
        });
        cx.observe(&state, |_, state, cx| {
            // Each change of place goes on back and forward's list.
            state.update(cx, |s, _| s.track_place());
            cx.notify()
        })
        .detach();
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
        // The palette draws its field in the title bar, which this view renders.
        cx.observe(&palette, |_, _, cx| cx.notify()).detach();
        let remote_picker =
            cx.new(|cx| super::remote_menu::RemotePicker::new(state.clone(), window, cx));
        let terminal =
            cx.new(|cx| super::terminal_dock::TerminalDock::new(state.clone(), window, cx));
        let onboarding = cx.new(|cx| Onboarding::new(state.clone(), window, cx));
        let rules = cx.new(|cx| RulesView::new(state.clone(), window, cx));
        let design_home = cx.new(|cx| super::design_home::DesignHome::new(state.clone(), window, cx));
        let pet = cx.new(|cx| super::pet::PetView::new(state.clone(), cx));
        let projects = cx.new(|cx| ProjectsView::new(state.clone(), composer.clone(), window, cx));
        let side = cx.new(|cx| SideChat::new(state.clone(), window, cx));
        let settings = cx.new(|cx| Settings::new(state.clone(), window, cx));
        let find = cx.new(|cx| FindBar::new(state.clone(), window, cx));
        let browser = cx.new(|cx| super::browser_view::BrowserView::new(state.clone(), window, cx));
        let confirm = cx.new(|cx| ConfirmWindow::new(state.clone(), window, cx));
        let attachment_editor =
            cx.new(|cx| super::attachment_editor::AttachmentEditor::new(state.clone(), window, cx));
        let askpass = cx.new(|cx| super::askpass::AskpassDialog::new(state.clone(), window, cx));
        let doc_comments =
            cx.new(|cx| super::doc_comments::DocComments::new(state.clone(), window, cx));
        let doc_find = cx.new(|cx| super::doc_find::DocFind::new(state.clone(), window, cx));
        let page_field =
            cx.new(|cx| super::page_field::PageField::new(state.clone(), window, cx));
        let text_editor =
            cx.new(|cx| super::text_editor::TextEditor::new(state.clone(), window, cx));
        let office_editor =
            cx.new(|cx| super::office_editor::OfficeEditor::new(state.clone(), window, cx));
        let sheet_editor =
            cx.new(|cx| super::sheet_view::SheetEditor::new(state.clone(), window, cx));
        let files = cx.new(|cx| super::files_pane::FilesPane::new(state.clone(), window, cx));
        let first_run = state.read(cx).onboarding;
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
            design_home,
            pet,
            projects,
            side,
            settings,
            find,
            browser,
            confirm,
            attachment_editor,
            askpass,
            doc_comments,
            doc_find,
            page_field,
            text_editor,
            office_editor,
            sheet_editor,
            files,
            presence: Presences::default(),
            focus: cx.focus_handle(),
        };
        if first_run {
            ws.onboarding.update(cx, |o, cx| o.open(window, cx));
        } else {
            ws.composer.update(cx, |c, cx| c.focus(window, cx));
        }
        // The dock was left open: it comes back with a shell in it, and the
        // keyboard stays with the composer.
        if !first_run && ws.state.read(cx).terminal_open {
            ws.terminal.update(cx, |d, cx| d.open(None, window, cx));
            ws.composer.update(cx, |c, cx| c.focus(window, cx));
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
    fn dock(&self, width: f32, leaving: bool, cx: &mut Context<Self>) -> gpui::AnyElement {
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
        match leaving {
            true => super::motion::slide_out(pane, width, "terminal-dock", &t),
            false => super::motion::slide_in(pane, width, false, &t),
        }
    }

    /// The open chat's name, heading the conversation under the title bar
    /// (the bar keeps only the app and the window's controls); a click
    /// opens its menu under it.
    fn chat_header(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let c = (s.screen == Screen::Chat)
            .then(|| s.current.and_then(|id| s.chats.iter().find(|c| c.id == id)))
            .flatten()
            .filter(|c| !c.messages.is_empty())?;
        let (id, hover) = (c.id, t.colors.hover);
        let menu_open = s.chat_menu == Some(id);
        let state = self.state.clone();
        let title = div()
            .id("chat-title")
            .flex()
            .items_center()
            .gap(t.space.xs)
            .min_w_0()
            .h(t.size.control)
            .pl(t.space.sm)
            .pr(t.space.xs)
            .ml(-t.space.sm)
            .rounded(t.radius.button)
            .cursor_pointer()
            .text_size(t.text.body)
            .font_weight(FontWeight::MEDIUM)
            .text_color(t.colors.text_strong)
            .hover(move |st| st.bg(hover))
            .when(menu_open, |d| d.bg(hover))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(c.title.clone()),
            )
            .child(icon(
                "icons/chevron-down.svg",
                t.size.icon_sm,
                t.colors.text_muted,
            ))
            .on_click(move |ev, _, cx| {
                cx.stop_propagation();
                // Under the title, its left edge.
                let at = gpui::point(
                    ev.position().x - t.space.lg,
                    ev.position().y + t.size.control / 2. + t.space.xs,
                );
                state.update(cx, |s, cx| s.chat_menu_from(id, at, cx));
            });
        Some(
            // From the chat's left edge, one control tall: the title, not
            // a band of empty room.
            div()
                .flex_none()
                .flex()
                .items_center()
                .h(t.size.control + t.space.sm)
                .px(t.space.lg)
                .border_b_1()
                .border_color(t.colors.border)
                .child(title)
                .into_any_element(),
        )
    }

    fn title_bar(
        &self,
        frames: BarFrames,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = cx.theme().clone();
        // Deferred so it paints above the page under the title bar.
        let switcher = frames.switcher.map(|leaving| {
            gpui::deferred(super::motion::unfold(
                div().child(project_switcher(
                    &self.state,
                    window,
                    cx,
                    t.size.control + t.space.xs,
                )),
                "project-switcher",
                false,
                leaving,
                &t,
            ))
            .with_priority(1)
        });
        let unseen = !self.state.read(cx).terminal_open && self.terminal.read(cx).unseen;
        let s = self.state.read(cx);
        let hover = t.colors.hover;
        let (tint, ink) = (t.colors.primary_tint, t.colors.primary);
        let side = s.doc.is_none().then_some(s.panel).flatten();
        let (plan_on, files_on) = (side == Some(PanelTab::Plan), s.files_open);
        let menu = s.title_menu.map(|(m, _)| m);
        let (share_on, more_on) = (
            menu == Some(TitleMenu::Share),
            menu == Some(TitleMenu::More),
        );
        let session_on = s.session_open;
        // A pane's button shows it is open: the accent's wash and ink.
        let toggle = |id: &'static str, path: &'static str, tip: &'static str, on: bool| {
            div()
                .id(id)
                .occlude()
                // No tooltip over its own open menu or pane.
                .when(!on, |d| {
                    d.tooltip(move |window, cx| {
                        gpui::component::tooltip::Tooltip::new(tip).build(window, cx)
                    })
                })
                .size(t.size.control)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.radius.small)
                .cursor_pointer()
                .when(on, |d| d.bg(tint))
                .when(!on, |d| d.hover(move |st| st.bg(hover)))
                .child(icon(
                    path,
                    t.size.icon,
                    if on { ink } else { t.colors.text_muted },
                ))
        };
        let chat = (s.screen == Screen::Chat)
            .then(|| s.current.and_then(|id| s.chats.iter().find(|c| c.id == id)))
            .flatten()
            .filter(|c| !c.messages.is_empty());
        let chat_open = chat.is_some();
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
        let sidebar_open = s.sidebar_open;
        TitleBar::new().h(t.size.titlebar).bg(t.extra.chrome).child(
            div()
                .relative()
                .flex()
                .items_center()
                .gap(t.space.xs)
                .w_full()
                .h_full()
                .pr(t.space.sm)
                .map(|d| {
                    let toggle = button(
                        "toggle-sidebar",
                        "icons/panel-left.svg",
                        "Show or hide the sidebar (Ctrl+B)",
                    )
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(ToggleSidebar), cx)
                    }));
                    if sidebar_open {
                        // Over the sidebar, its head: the toggle and KodAct's
                        // name, without the mark (the welcome carries it).
                        d.child(
                            // The title bar insets its row by the sidebar's
                            // own padding, so the switch ends where the
                            // sidebar's search field does.
                            div()
                                .flex_none()
                                .w(t.size.sidebar - t.space.md)
                                .flex()
                                .items_center()
                                .gap(t.space.sm)
                                .pr(t.space.sm)
                                .child(toggle)
                                .child(super::kodact_mark::name(15., &t)),
                        )
                    } else {
                        d.child(toggle)
                    }
                })
                // Back and forward through the places you were, dimmed when
                // there's nowhere to go.
                .child(nav_button(
                    "go-back",
                    "icons/arrow-left.svg",
                    "Back (Alt+Left)",
                    !s.nav_back.is_empty(),
                    Box::new(GoBack),
                    &t,
                ))
                .child(nav_button(
                    "go-forward",
                    "icons/arrow-right.svg",
                    "Forward (Alt+Right)",
                    !s.nav_forward.is_empty(),
                    Box::new(GoForward),
                    &t,
                ))
                // The project and its branch: here while the sidebar is
                // hidden, at the top of the sidebar's sheet while it shows.
                // A plain chat has no folder to show.
                .when(!s.sidebar_open && s.in_folder(), |d| {
                    d.child(
                        div()
                            .relative()
                            .when(s.switcher_open, |d| {
                                d.on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                    if this.state.read(cx).switcher_open {
                                        this.state.update(cx, |s, cx| {
                                            s.switcher_open = false;
                                            cx.notify();
                                        });
                                    }
                                }))
                            })
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
                .child(div().flex_1())
                .children(super::tasks::render(
                    &t,
                    &self.state,
                    window,
                    frames.tasks,
                    cx,
                ))
                .children(super::approvals::render(
                    &t,
                    &self.state,
                    window,
                    frames.approvals,
                    cx,
                ))
                // The Session column: where it runs, its context, its changes.
                .when(chat_open, |d| {
                    d.child(
                        toggle(
                            "show-session",
                            "icons/panel-right.svg",
                            "Session",
                            session_on,
                        )
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(ToggleSession), cx)
                        })),
                    )
                })
                .child(
                    toggle(
                        "show-plan",
                        "icons/list-todo.svg",
                        "Plan (Ctrl+Shift+E)",
                        plan_on,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state
                            .update(cx, |s, cx| s.toggle_panel_tab(PanelTab::Plan, cx))
                    })),
                )
                .child(
                    toggle("show-files", "icons/folder.svg", "Files", files_on).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |s, cx| s.toggle_files(cx))
                        }),
                    ),
                )
                .when(chat_open, |d| {
                    d.child(
                        toggle("share-chat", "icons/share.svg", "Share", share_on).on_click(
                            cx.listener(|this, ev: &gpui::ClickEvent, _, cx| {
                                let at = super::click_down(ev);
                                this.state.update(cx, |s, cx| {
                                    s.toggle_title_menu(TitleMenu::Share, at, cx)
                                })
                            }),
                        ),
                    )
                })
                .child(
                    toggle("more", "icons/ellipsis-vertical.svg", "More", more_on)
                        // New output in a closed terminal: a dot, as before.
                        .when(unseen, |d| {
                            d.relative().child(
                                div()
                                    .absolute()
                                    .top(px(4.))
                                    .right(px(4.))
                                    .size(px(6.))
                                    .rounded(t.radius.pill)
                                    .bg(t.colors.primary),
                            )
                        })
                        .on_click(cx.listener(|this, ev: &gpui::ClickEvent, _, cx| {
                            let at = super::click_down(ev);
                            this.state
                                .update(cx, |s, cx| s.toggle_title_menu(TitleMenu::More, at, cx))
                        })),
                ),
        )
    }

    /// A new chat: the greeting in the display serif beside KodAct's mark, a
    /// quiet line under it, the composer and the starters, on a plain
    /// ground.
    fn welcome(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        // A binary artifact's card shows its opening, read once off the
        // render path.
        let unread: Vec<crate::state::FileRef> = {
            let s = self.state.read(cx);
            crate::ui::documents::all(s)
                .into_iter()
                .take(3)
                .map(|(f, ..)| f)
                .filter(|f| {
                    f.bytes.is_some()
                        && !s.artifact_snips.contains_key(&super::artifact_card::key(f))
                })
                .collect()
        };
        if !unread.is_empty() {
            let st = self.state.clone();
            cx.defer(move |cx| st.update(cx, |s, cx| s.want_artifact_snips(unread, cx)));
        }
        let s = self.state.read(cx);
        let name = s.user_name.clone().unwrap_or_default();
        let name = name.trim();
        let line = crate::greeting::pick(name, crate::greeting::hour_now(), s.greeting_seed);
        // Three cards a row; two when a panel narrows the column, so each
        // stays readable.
        let per_row = if self.composer.read(cx).narrow { 2 } else { 3 };
        let recent: Vec<_> = crate::ui::documents::all(s).into_iter().take(per_row).collect();
        let recent_section = (!recent.is_empty()).then(|| {
            let scale = window.scale_factor();
            let cards: Vec<gpui::AnyElement> = recent
                .into_iter()
                .enumerate()
                .map(|(i, (f, _chat, _chat_title))| {
                    let snip = s.artifact_snips.get(&super::artifact_card::key(&f)).cloned();
                    let open_state = self.state.clone();
                    let to_open = f.clone();
                    super::artifact_card::render(
                        &f,
                        snip.as_deref().map(|v| v.as_slice()),
                        i,
                        &t,
                        scale,
                        move |cx| {
                            let file = to_open.clone();
                            open_state.update(cx, |s, cx| s.open_doc(file, cx));
                        },
                    )
                })
                .collect();

            let view_all = self.state.clone();
            let text_hover = t.colors.text;
            div()
                .w_full()
                .max_w(t.size.composer_width)
                .pt(t.space.xs)
                .flex()
                .flex_col()
                .gap(t.space.sm)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(t.text.caption)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.colors.text_muted)
                                .child("Recent artifacts"),
                        )
                        .child(
                            div()
                                .id("welcome-view-artifacts")
                                .text_size(t.text.caption)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.colors.text_muted)
                                .cursor_pointer()
                                .hover(move |st| st.text_color(text_hover).underline())
                                .child("View all")
                                .on_click(move |_, _, cx| {
                                    view_all.update(cx, |s, cx| {
                                        s.screen = Screen::Documents;
                                        cx.notify();
                                    });
                                }),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_stretch()
                        .gap(t.space.sm)
                        .children(cards),
                )
        });

        div().flex_1().min_h_0().relative().flex().flex_col().child(
            // `my_auto()` centers when tall and scrolls smoothly without clipping when short.
            div()
                .id("welcome")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .child(
                    div()
                        .my_auto()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .px(t.space.xl)
                        .py(t.space.xxl)
                        .child(
                            div()
                                .w_full()
                                .max_w(t.size.composer_width + t.space.lg)
                                .relative()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(t.space.lg + px(4.))
                                // The page rises in piece by piece as it opens.
                                .child(super::motion::row_in_full(
                                    div()
                                        .w_full()
                                        .max_w(t.size.composer_width)
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(t.space.xs + px(2.))
                                        .child(greeting_line(
                                            &line,
                                            s.panel.is_some() || s.doc.is_some(),
                                            &t,
                                        )),
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
                                .children(recent_section),
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
    fn toast(
        &self,
        frame: Frame<SharedString>,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let t = cx.theme().clone();
        let (text, leaving) = frame?;
        let id = format!("toast-{}", text);
        Some(
            div()
                .absolute()
                .bottom(t.size.composer_clearance)
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(super::motion::notice(
                    theme::popover(&t)
                        .px(t.space.lg)
                        .py(t.space.sm)
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(icon("icons/info.svg", t.size.icon_sm, t.colors.text_muted))
                        .child(text),
                    &id,
                    leaving,
                    &t,
                )),
        )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        // Each window that just closed leaves over a beat of its own.
        let (palette_now, shortcuts_now, map_now, title_now, chat_now, toast_now) = {
            let s = self.state.read(cx);
            (
                s.palette_open.then_some(()),
                s.shortcuts_open.then_some(()),
                s.map_open.then_some(()),
                s.title_menu,
                s.chat_menu,
                s.toast.clone(),
            )
        };
        let p = &mut self.presence;
        let palette_f = p.palette.frame(palette_now, t.size.menu_out_ms, window);
        let shortcuts_f = p
            .shortcuts
            .frame(shortcuts_now, t.size.window_out_ms, window);
        let map_f = p.map.frame(map_now, t.size.window_out_ms, window);
        let title_f = p.title.frame(title_now, t.size.menu_out_ms, window);
        let chat_f = p.chat.frame(chat_now, t.size.menu_out_ms, window);
        let toast_f = p.toast.frame(toast_now, t.size.window_out_ms, window);
        let bar = {
            let s = self.state.read(cx);
            let (sw, ta, ap) = (s.switcher_open, s.tasks_open, s.approvals_open);
            let p = &mut self.presence;
            let out = t.size.menu_out_ms;
            BarFrames {
                switcher: p.switcher.frame(sw.then_some(()), out, window).map(|f| f.1),
                tasks: p.tasks.frame(ta.then_some(()), out, window).map(|f| f.1),
                approvals: p
                    .approvals
                    .frame(ap.then_some(()), out, window)
                    .map(|f| f.1),
            }
        };
        let s = self.state.read(cx);
        let (sidebar_open, onboarding, screen, loading) =
            (s.sidebar_open, s.onboarding, s.screen, s.loading);
        // Which chat shows: its transcript animates in when it changes.
        let chat_key = s.chat().map_or(usize::MAX, |c| c.id);
        let presenting = s.presenting;
        let empty = s.chat().is_none_or(|c| c.messages.is_empty());
        // The side column: the file or tab in it and the Files pane, kept
        // while it slides back out after it closes.
        let side_now = (s.doc.is_some() || s.panel.is_some() || s.files_open)
            .then(|| (s.doc.clone(), s.panel, s.files_open));
        let side_f = self
            .presence
            .side
            .frame(side_now, super::motion::close_ms(&t), window);
        let side_leaving = side_f.as_ref().is_some_and(|(_, leaving)| *leaving);
        let side_open = side_f.is_some();
        let (side_doc, side_panel, files_open) =
            side_f.map(|(k, _)| k).unwrap_or((None, None, false));
        let panel_full = !loading
            && !empty
            && screen == Screen::Chat
            && side_doc.is_none()
            && side_panel == Some(crate::state::PanelTab::Agents)
            && s.panel_full
            && !files_open;
        let doc_full_want = !loading && side_doc.is_some() && s.doc_full && !files_open;
        let doc_full_t = self
            .presence
            .doc_full
            .value(doc_full_want, 320, 260, window);
        let doc_full_settled = doc_full_t >= 0.999;
        let doc_full_active = doc_full_t > 0.001;
        // With Files open or full-screen active, the side column stands beside the chat.
        let below = s.doc_below && !files_open && !panel_full && !doc_full_active;
        // The terminal dock, kept while it slides back out.
        let dock_f = self.presence.dock.frame(
            s.terminal_open.then_some(()),
            super::motion::close_ms(&t),
            window,
        );
        let dock_leaving = dock_f.is_some_and(|(_, leaving)| leaving);
        let terminal_open = dock_f.is_some();
        // What is left of the window beside the sidebar is shared by the
        // chat, the side panel and the dock; the chat keeps its narrowest.
        let beside = f32::from(window.viewport_size().width)
            - if sidebar_open {
                f32::from(t.size.sidebar)
            } else {
                f32::from(t.size.rail)
            };
        let dock_want = self.dock_want(window, cx);
        let wide = side_doc
            .as_ref()
            .is_some_and(|f| f.is_diagram() || f.office() == Some(crate::office::Kind::Xlsx));
        let panel_share = self.state.read(cx).panel_share;
        let (panel_w, dock_w) = share_out(
            beside,
            (side_open && !below && !panel_full).then(|| {
                let after_dock = beside - if terminal_open { dock_want } else { 0. };
                let panel_min = 320.0_f32.min(f32::from(t.size.doc_pane));
                let want = match panel_share {
                    Some(share) => (after_dock * share).max(panel_min),
                    None => super::doc_pane::width(after_dock, wide, &t),
                };
                (want, panel_min)
            }),
            terminal_open.then(|| (dock_want, f32::from(t.size.terminal_min))),
            if panel_full {
                0.
            } else {
                f32::from(t.size.chat_min)
            },
        );
        let dock = self.dock(dock_w, dock_leaving, cx);
        // The chat column's width: under `NARROW_CHAT`, the composer's tray
        // shows its folder and model as icons, so nothing is cut mid-word.
        let chat_w = beside
            - if side_open && !below && !panel_full { panel_w } else { 0. }
            - if terminal_open { dock_w } else { 0. };
        let narrow = chat_w < NARROW_CHAT;
        if self.composer.read(cx).narrow != narrow {
            self.composer.update(cx, |c, _| c.narrow = narrow);
        }

        // The session is a sheet on the cream: 4px corners, paper grain,
        // a gutter to its right and below.
        let sheet = div()
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
        // What's on the sheet: each screen arrives on it (`screen_in`),
        // while the sheet itself stays.
        let main = div().flex_1().min_w_0().min_h_0().flex().flex_col();
        let screen_key = match screen {
            Screen::Chat => 0,
            Screen::Rules => 1,
            Screen::Projects => 2,
            Screen::Documents => 3,
            Screen::Design => 4,
            Screen::Project(id) => 5 + id,
        };
        // A file opened from the Files tab is pushed over its list (and
        // pulled back); a file opened from a reply just arrives.
        let push_frame = self.presence.push.frame(
            side_doc
                .clone()
                .filter(|_| side_panel.is_some() && !files_open && !below),
            t.size.window_in_ms,
            window,
        );
        // A file opened from a reply, or the setup guide, sits beside the session.
        let pane = {
            let s = self.state.read(cx);
            let main_width = beside - if terminal_open { dock_w } else { 0. };
            let effective_doc_w = if doc_full_active {
                panel_w + (main_width - panel_w) * doc_full_t
            } else {
                panel_w
            };
            let fit = super::doc_pane::Fit {
                width: if below { main_width } else { effective_doc_w },
                scale: window.scale_factor(),
                fit: s.doc_fit,
                below,
                full: doc_full_want,
                full_t: doc_full_t,
                from_panel: side_panel.is_some(),
                text: s.doc_text,
                source: s.source_view,
                version: {
                    let (all, at) = s.doc_versions();
                    (at, all.len().max(1))
                },
                show_changes: s.doc_show_changes,
                page_pick: s.doc_page_pick,
                scroll: s.doc_scroll.clone(),
                height: {
                    let stage = f32::from(window.viewport_size().height)
                        - f32::from(t.size.titlebar)
                        - f32::from(t.size.statusbar);
                    (if below { stage * super::doc_pane::BELOW_SHARE } else { stage })
                        - f32::from(t.size.panel_header + t.size.panel_toolbar + t.size.panel_footer)
                },
                zoom: s.doc_zoom_here(),
                fitted: s.doc_fitted.clone(),
                thumbs: s.doc_thumbs_here(),
                menu: s.doc_menu,
                drag: s.doc_drag.clone(),
                dirty: s.doc_dirty(),
                device: s.web_device,
                edit_text: s.doc_edit.as_ref().map(|(_, t)| t.clone()),
            };
            let doc_changes = s
                .doc_changes
                .as_ref()
                .filter(|(k, _)| {
                    side_doc
                        .as_ref()
                        .is_some_and(|f| crate::state::doc_version_key(f) == *k)
                })
                .map(|(_, c)| c);
            let (pane_width, pane_below) = (fit.width, fit.below);
            let read = s
                .doc_preview
                .as_ref()
                .filter(|(k, _)| {
                    side_doc
                        .as_ref()
                        .is_some_and(|f| k.split_once('#').map(|(n, _)| n) == Some(f.name.as_ref()))
                })
                .map(|(_, r)| r);
            let panel = side_panel.map(|tab| {
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
                        ui_toggles: &s.ui_toggles,
                        selected_agent: s.selected_agent_id().map(str::to_string),
                        full: panel_full,
                        copied: s.copied.clone(),
                        width: fit.width,
                        below,
                        scale: fit.scale,
                        plan_version: s.plan_version,
                        source: s.source_view,
                        agent_strip: &s.agent_strip,
                        agent_strip_at: &s.agent_strip_at,
                        agent_jumper_focus: &s.agent_jumper_focus,
                    },
                    self.state.clone(),
                    &t,
                )
            });
            let has_top = side_doc.is_some() || side_panel.is_some();
            let extra_rows = s.sheet_extra_rows.get(&s.doc_sheet).copied().unwrap_or(0);
            let extra_cols = s.sheet_extra_cols.get(&s.doc_sheet).copied().unwrap_or(0);
            let panel_resizing = s.panel_resizing;
            let leaving_fit = fit.clone();
            let doc_view = |f: &crate::state::FileRef, fit: super::doc_pane::Fit, leaving: bool| {
                // A file leaving has nothing to edit or comment on any more.
                let (editor, comments, find, page_field, text_editor, browser, office_editor) = if leaving {
                    (
                        div().into_any_element(),
                        div().into_any_element(),
                        div().into_any_element(),
                        div().into_any_element(),
                        div().into_any_element(),
                        div().into_any_element(),
                        div().into_any_element(),
                    )
                } else {
                    (
                        self.sheet_editor.clone().into_any_element(),
                        self.doc_comments.clone().into_any_element(),
                        self.doc_find.clone().into_any_element(),
                        self.page_field.clone().into_any_element(),
                        self.text_editor.clone().into_any_element(),
                        self.browser.clone().into_any_element(),
                        self.office_editor.clone().into_any_element(),
                    )
                };
                super::doc_pane::render(
                    f,
                    self.state.clone(),
                    s.copied.clone(),
                    fit,
                    read,
                    s.doc_sheet,
                    s.doc_pick,
                    &s.sheet_edits,
                    (extra_rows, extra_cols),
                    editor,
                    doc_changes,
                    comments,
                    find,
                    page_field,
                    text_editor,
                    browser,
                    office_editor,
                    &t,
                )
            };
            use super::motion::PushFrame;
            let top = match (push_frame, side_doc.as_ref(), panel) {
                (PushFrame::Forward(k), Some(f), Some(list)) => Some(super::motion::push(
                    list,
                    doc_view(f, fit, false),
                    pane_width,
                    true,
                    k,
                )),
                (PushFrame::Back(old, k), None, Some(list)) => Some(super::motion::push(
                    list,
                    doc_view(&old, leaving_fit, true),
                    pane_width,
                    false,
                    k,
                )),
                (_, Some(f), _) => Some(doc_view(f, fit, false)),
                (_, None, panel) => panel,
            }
            .map(|p| match files_open {
                // A file or tab above, the Files pane under it.
                true => div().flex_1().min_h_0().flex().child(p).into_any_element(),
                false => p,
            });
            let column = match (top, files_open) {
                (top, true) => Some(
                    div()
                        .w(px(pane_width))
                        .h_full()
                        .flex()
                        .flex_col()
                        .border_l_1()
                        .border_color(t.colors.border)
                        .children(top)
                        .child(
                            div()
                                .when(has_top, |d| d.h(gpui::relative(FILES_SHARE)).border_t_1())
                                .when(!has_top, |d| d.flex_1())
                                .min_h_0()
                                .border_color(t.colors.border)
                                .child(self.files.clone()),
                        )
                        .into_any_element(),
                ),
                (top, false) => top,
            }
            .map(|inner| {
                if pane_below || panel_full || doc_full_active || side_leaving {
                    return inner;
                }
                let grip_state = self.state.clone();
                let line = t.colors.primary;
                div()
                    .relative()
                    .flex_none()
                    .h_full()
                    .flex()
                    .child(inner)
                    .child(
                        div()
                            .id("side-panel-grip")
                            .occlude()
                            .tooltip(|w, cx| {
                                gpui::component::tooltip::Tooltip::new(
                                    "Drag to resize · Double-click to reset",
                                )
                                .build(w, cx)
                            })
                            .absolute()
                            .left(px(-4.))
                            .top_0()
                            .bottom_0()
                            .w(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor(gpui::CursorStyle::ResizeLeftRight)
                            .when(panel_resizing, |d| d.bg(line.opacity(0.35)))
                            .hover(move |st| st.bg(line.opacity(0.35)))
                            .on_mouse_down(gpui::MouseButton::Left, move |ev, _, cx| {
                                grip_state.update(cx, |s, cx| {
                                    if ev.click_count >= 2 {
                                        s.panel_share = None;
                                        s.panel_resizing = false;
                                        s.save_window();
                                    } else {
                                        s.panel_resizing = true;
                                    }
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                    )
                    .into_any_element()
            });
            column.and_then(|p| match side_leaving {
                // Leaving: back out to the edge (below the chat, gone at once).
                true => (!pane_below && !panel_full && !doc_full_settled)
                    .then(|| super::motion::slide_out(p, pane_width, "side-panel", &t)),
                // Full-screen focus keeps the pane in the same slide-in
                // wrapper, only wider (`pane_width` follows the tween): a
                // different wrapper would remount it and replay its entrance
                // from nothing, which blanked the pane as focus began or ended.
                false => Some(if panel_full {
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .child(p)
                        .into_any_element()
                } else if pane_below {
                    super::motion::split_in(p, super::doc_pane::BELOW_SHARE, &t)
                } else {
                    super::motion::slide_in(p, pane_width, pane_below, &t)
                }),
            })
        };
        let main = if screen == Screen::Rules {
            main.child(self.rules.clone())
        } else if screen == Screen::Design {
            // The gallery, and a card's page beside it when previewed.
            main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(below, |d| d.flex_col())
                    .child(div().flex_1().min_w_0().min_h_0().flex().child(self.design_home.clone()))
                    .children(pane),
            )
        } else if screen == Screen::Documents {
            // A card opens its file beside the list, as from a chat.
            let page = {
                let s = self.state.read(cx);
                super::documents::render(&t, &self.state, s, window.scale_factor())
            };
            main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(below, |d| d.flex_col())
                    .child(div().flex_1().min_w_0().min_h_0().flex().child(page))
                    .children(pane),
            )
        } else if matches!(screen, Screen::Projects | Screen::Project(_)) {
            // A project page opens its knowledge files in the side panel too.
            let doc_open =
                matches!(screen, Screen::Project(_)) && self.state.read(cx).doc.is_some();
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
                            .flex()
                            .child(self.projects.clone()),
                    )
                    .when(doc_open, |d| d.children(pane)),
            )
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
                .relative()
                // Behind a chat the stage is plain, as the text wants it;
                // the home screen's field follows only if you asked for it.
                .children(self.chat_header(cx))
                // Always there: the bar draws itself while open and as it leaves.
                .child(self.find.clone())
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .relative()
                        .child(self.chat.clone())
                        // Each chat opened fades and rises in, its task pill
                        // with it.
                        .with_animation(
                            ("transcript-in", chat_key),
                            gpui::Animation::new(std::time::Duration::from_millis(
                                if super::motion::reduced_motion() {
                                    1
                                } else {
                                    t.size.panel_in_ms
                                },
                            ))
                            .with_easing(gpui::ease_out_quint()),
                            |d, k| d.opacity(k).mt(px(10. * (1. - k))),
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
            // The Session column, between the chat and any pane, as Claude
            // Code's: environment, context and changes at a glance.
            // It shows only while the chat keeps its narrowest beside it: a
            // side panel and the dock come first, and the column steps aside
            // (it stays on, and returns when there is room again).
            let session_fits = session_fits(
                beside,
                if side_open && !below { panel_w } else { 0. },
                if terminal_open { dock_w } else { 0. },
                f32::from(t.size.session_panel),
                f32::from(t.size.chat_min),
            );
            let session_now = {
                let s = self.state.read(cx);
                s.session_open
                    .then_some(s.current)
                    .flatten()
                    .filter(|_| session_fits)
            };
            let session = self
                .presence
                .session
                .frame(session_now, super::motion::close_ms(&t), window)
                .map(|(chat, leaving)| {
                    let w = f32::from(t.size.session_panel);
                    let col =
                        super::session_panel::render(&t, &self.state, self.state.read(cx), chat);
                    match leaving {
                        true => super::motion::slide_out(col, w, "session-column", &t),
                        false => super::motion::slide_in(col, w, false, &t),
                    }
                });
            let chat_stage = if doc_full_active && !doc_full_settled {
                let fade = (1.0 - doc_full_t).clamp(0.0, 1.0).powf(1.6);
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .flex()
                    .opacity(fade)
                    .relative()
                    .left(px(-20.0 * doc_full_t))
                    .child(conversation)
                    .children(session)
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .child(conversation)
                    .children(session)
                    .into_any_element()
            };
            main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(below, |d| d.flex_col())
                    .when(!panel_full && !doc_full_settled, |d| d.child(chat_stage))
                    .children(pane),
            )
        };

        theme::app_root(&t)
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            // While a chat's menu shows, its letters run its rows, wherever
            // the focus is (Claude Code's session menu works the same).
            // Tab and Shift Tab walk the controls (rows, pills, buttons) when
            // the focused one doesn't take the key itself; Enter or Space
            // presses it.
            .on_key_down(cx.listener(|_, ev: &gpui::KeyDownEvent, window, cx| {
                let m = &ev.keystroke.modifiers;
                if ev.keystroke.key == "tab" && !m.control && !m.alt && !m.platform {
                    cx.stop_propagation();
                    if m.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                }
            }))
            .capture_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                let k = &ev.keystroke;
                if k.modifiers.modified() {
                    return;
                }
                // Esc closes the agents' jumper before anything focused (the
                // composer) can take the key.
                if k.key == "escape" && this.state.read(cx).ui_toggles.contains(AGENT_JUMPER) {
                    cx.stop_propagation();
                    this.state.update(cx, |s, cx| {
                        s.ui_toggles.remove(AGENT_JUMPER);
                        cx.notify();
                    });
                    return;
                }
                let Some(id) = this.state.read(cx).chat_menu else {
                    return;
                };
                if let Some(f) = super::chat_menu::letter(&k.key) {
                    cx.stop_propagation();
                    this.state.update(cx, |s, cx| f(s, id, cx));
                }
            }))
            .relative()
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &ToggleSession, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_session(cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleFiles, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_files(cx))
            }))
            .on_action(cx.listener(|this, _: &ShowChatFiles, _, cx| {
                this.state
                    .update(cx, |s, cx| s.toggle_panel_tab(PanelTab::Files, cx))
            }))
            .on_action(cx.listener(|this, _: &OpenDocuments, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.screen = crate::state::Screen::Documents;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &NewChat, window, cx| {
                this.state.update(cx, |s, cx| s.new_plain_chat(cx));
                this.focus_composer(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowProjectMap, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_map(cx))
            }))
            .on_action(cx.listener(|this, _: &ChangeRemoteFolder, _, cx| {
                this.state.update(cx, |s, cx| s.change_remote_folder(cx))
            }))
            .on_action(cx.listener(|this, _: &FindInFile, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_doc_find(cx))
            }))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| {
                this.state.update(cx, |s, cx| s.step_doc_page(1, cx))
            }))
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| {
                this.state.update(cx, |s, cx| s.step_doc_page(-1, cx))
            }))
            .on_action(cx.listener(|this, _: &SaveDocEdits, _, cx| {
                this.state.update(cx, |s, cx| s.save_doc_edits(cx))
            }))
            .on_action(cx.listener(|this, _: &DiscardDocEdits, _, cx| {
                this.state.update(cx, |s, cx| s.discard_doc_edits(cx))
            }))
            .on_action(cx.listener(|this, _: &UndoDocEdit, window, cx| {
                this.text_editor.update(cx, |e, cx| e.step(true, window, cx))
            }))
            .on_action(cx.listener(|this, _: &RedoDocEdit, window, cx| {
                this.text_editor.update(cx, |e, cx| e.step(false, window, cx))
            }))
            .on_action(cx.listener(|this, _: &DocZoomIn, _, cx| {
                this.state.update(cx, |s, cx| s.zoom_doc(true, cx))
            }))
            .on_action(cx.listener(|this, _: &DocZoomOut, _, cx| {
                this.state.update(cx, |s, cx| s.zoom_doc(false, cx))
            }))
            .on_action(cx.listener(|this, _: &DocZoomFit, _, cx| {
                this.state
                    .update(cx, |s, cx| s.set_doc_zoom(crate::doc_zoom::Zoom::FitWidth, cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleThumbnails, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_doc_thumbs(cx))
            }))
            .on_action(cx.listener(|this, _: &GoToPage, window, cx| {
                this.page_field.update(cx, |f, cx| f.focus(window, cx))
            }))
            .on_action(cx.listener(|this, _: &ExportPdf, _, cx| {
                this.state.update(cx, |s, cx| match s.doc.clone() {
                    Some(f) => s.export_pdf(&f, cx),
                    None => s.show_toast("Open a Word, Excel or PowerPoint file to export it.", cx),
                })
            }))
            .on_action(cx.listener(|this, _: &TogglePanel, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_panel(cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                this.state.update(cx, |s, cx| s.web_zoom_step(1, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                this.state.update(cx, |s, cx| s.web_zoom_step(-1, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| {
                this.state.update(cx, |s, cx| s.web_zoom_to(1.0, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomFit, _, cx| {
                this.state.update(cx, |s, cx| s.web_zoom_fit(cx))
            }))
            .on_action(cx.listener(|this, _: &CritiquePage, _, cx| {
                this.state.update(cx, |s, cx| s.design_critique(cx))
            }))
            .on_action(cx.listener(|this, _: &DesignChat, _, cx| {
                this.state.update(cx, |s, cx| s.design_surface(crate::design::prompt::Surface::Any, cx))
            }))
            .on_action(cx.listener(|this, _: &DesignDeck, _, cx| {
                this.state.update(cx, |s, cx| s.design_surface(crate::design::prompt::Surface::Deck, cx))
            }))
            .on_action(cx.listener(|this, _: &DesignPrototype, _, cx| {
                this.state.update(cx, |s, cx| s.design_surface(crate::design::prompt::Surface::Prototype, cx))
            }))
            .on_action(cx.listener(|this, _: &DesignDocument, _, cx| {
                this.state.update(cx, |s, cx| s.design_surface(crate::design::prompt::Surface::Document, cx))
            }))
            .on_action(cx.listener(|this, _: &EndDesignChat, _, cx| {
                this.state.update(cx, |s, cx| s.set_design(None, cx))
            }))
            .on_action(cx.listener(|this, _: &Present, _, cx| {
                this.state.update(cx, |s, cx| s.set_presenting(!s.presenting, cx))
            }))
            .on_action(cx.listener(|this, _: &SavePagePng, _, cx| {
                this.state.update(cx, |s, cx| s.design_export(crate::design::export::Format::Png, cx))
            }))
            .on_action(cx.listener(|this, _: &SavePagePptx, _, cx| {
                this.state.update(cx, |s, cx| s.design_export(crate::design::export::Format::Pptx, cx))
            }))
            .on_action(cx.listener(|this, _: &SavePagePptxEditable, _, cx| {
                this.state.update(cx, |s, cx| s.design_export(crate::design::export::Format::PptxEditable, cx))
            }))
            .on_action(cx.listener(|this, _: &SavePageFigma, _, cx| {
                this.state.update(cx, |s, cx| s.design_export(crate::design::export::Format::Figma, cx))
            }))
            .on_action(cx.listener(|this, _: &SavePageHtml, _, cx| {
                this.state.update(cx, |s, cx| s.design_export(crate::design::export::Format::Html, cx))
            }))
            .on_action(cx.listener(|this, _: &SavePagePdf, _, cx| {
                this.state.update(cx, |s, cx| s.design_export_pdf(cx))
            }))
            .on_action(cx.listener(|this, _: &LayoutGuides, _, cx| {
                this.state.update(cx, |s, cx| s.design_set_guides(!s.design_guides, cx))
            }))
            .on_action(cx.listener(|this, _: &PageOutline, _, cx| {
                this.state.update(cx, |s, cx| s.design_set_outline(!s.design_outline, cx))
            }))
            .on_action(cx.listener(|this, _: &UndoPage, _, cx| {
                this.state.update(cx, |s, cx| s.design_step(-1, cx))
            }))
            .on_action(cx.listener(|this, _: &RedoPage, _, cx| {
                this.state.update(cx, |s, cx| s.design_step(1, cx))
            }))
            .on_action(cx.listener(|this, _: &DuplicatePage, _, cx| {
                this.state.update(cx, |s, cx| s.design_duplicate(cx))
            }))
            .on_action(cx.listener(|this, _: &CommentTool, _, cx| {
                this.state.update(cx, |s, cx| {
                    if s.design_on_screen().is_some() {
                        s.design_set_select(!s.design_select, cx)
                    } else {
                        s.show_toast("Show a page KodAct designed to comment on it.", cx)
                    }
                })
            }))
            .on_action(cx.listener(|this, _: &NewDesignPage, window, cx| {
                this.state.update(cx, |s, cx| s.design_new_page(cx));
                this.focus_composer(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowPlanView, _, cx| {
                this.state
                    .update(cx, |s, cx| s.show_panel(PanelTab::Plan, cx))
            }))
            .on_action(cx.listener(|this, _: &ShowAgentsView, _, cx| {
                this.state
                    .update(cx, |s, cx| s.show_panel(PanelTab::Agents, cx))
            }))
            .on_action(cx.listener(|this, _: &ToggleAgentJumper, _, cx| {
                this.state.update(cx, |s, cx| s.toggle_agent_jumper(cx))
            }))
            .on_action(cx.listener(|this, _: &ShowFilesView, _, cx| {
                this.state
                    .update(cx, |s, cx| s.show_panel(PanelTab::Files, cx))
            }))
            .on_action(cx.listener(|this, _: &ShowBrowserView, _, cx| {
                this.state
                    .update(cx, |s, cx| s.show_panel(PanelTab::Browser, cx))
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
            .on_action(cx.listener(|this, _: &ToggleDocFull, _, cx| {
                this.state.update(cx, |s, cx| {
                    if s.doc.is_none() {
                        s.show_toast("Open a file first to view it full screen.", cx);
                    } else {
                        s.toggle_doc_full(cx);
                    }
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
            .on_action(cx.listener(|this, _: &SetUpDocumentTools, _, cx| {
                this.state.update(cx, |s, cx| s.set_up_doc_tools(cx))
            }))
            .on_action(cx.listener(|this, _: &InstallLibreOffice, _, cx| {
                this.state.update(cx, |s, cx| s.install_libreoffice(cx))
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
            .on_action(
                cx.listener(|this, _: &FocusComposer, window, cx| this.focus_composer(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleModelPicker, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.picker_open = !s.picker_open;
                    s.modes_open = false;
                    s.bypass_warning = false;
                    s.design_menu = None;
                    s.project_menu = false;
                    s.context_open = false;
                    s.title_menu = None;
                    s.title_menu_dismissed = None;
                    cx.notify();
                })
            }))
            .on_action(cx.listener(|this, _: &CycleMode, _, cx| {
                this.state.update(cx, |s, cx| s.cycle_mode(cx))
            }))
            .on_action(cx.listener(|this, _: &StopAgents, _, cx| {
                this.state.update(cx, |s, cx| s.stop_agents(cx))
            }))
            // The agent the Agents panel shows: stop it, copy what it said,
            // or give the panel the whole window.
            .on_action(cx.listener(|this, _: &StopShownAgent, _, cx| {
                this.state.update(cx, |s, cx| {
                    if let Some(id) = s.selected_agent_id().map(str::to_string) {
                        s.stop_agent(&id, cx);
                    }
                })
            }))
            .on_action(cx.listener(|this, _: &CopyAgentOutput, _, cx| {
                this.state.update(cx, |s, cx| {
                    if let Some(id) = s.selected_agent_id().map(str::to_string) {
                        s.copy_agent_output(&id, cx);
                    }
                })
            }))
            .on_action(cx.listener(|this, _: &TogglePanelFull, window, cx| {
                let full = this.state.update(cx, |s, cx| {
                    if s.panel != Some(crate::state::PanelTab::Agents) {
                        s.show_panel(crate::state::PanelTab::Agents, cx);
                    }
                    s.toggle_panel_full(cx);
                    s.panel_full
                });
                // Full view hides the composer: the keyboard stays with the
                // workspace, so its shortcuts (this one too) keep working.
                if full {
                    window.focus(&this.focus, cx);
                } else {
                    this.focus_composer(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &EditQueued, window, cx| {
                this.state.update(cx, |s, cx| s.edit_last_queued(cx));
                window.dispatch_action(Box::new(FocusComposer), cx);
            }))
            .on_action(cx.listener(|this, _: &RemoveQueued, _, cx| {
                this.state.update(cx, |s, cx| s.remove_all_queued(cx))
            }))
            .on_action(cx.listener(|this, _: &StopReply, window, cx| {
                let (palette, picker) = {
                    let s = this.state.read(cx);
                    (s.palette_open, s.picker_open)
                };
                this.state.update(cx, |s, cx| {
                    // Esc closes what is open, the newest first, before it stops a reply.
                    if s.ui_toggles.contains(AGENT_JUMPER) {
                        s.ui_toggles.remove(AGENT_JUMPER);
                        cx.notify();
                    } else if s.presenting {
                        s.set_presenting(false, cx);
                    } else if s.design_menu.is_some() {
                        s.design_menu_to(None, cx);
                    } else if s.design_pick.is_some() {
                        s.design_cancel_pick(cx);
                    } else if s.design_tool.is_some() {
                        s.design_set_tool(None, cx);
                    } else if s.design_select {
                        s.design_set_select(false, cx);
                    } else if s.map_open {
                        s.toggle_map(cx);
                    } else if s.shortcuts_open
                        || s.chat_menu.is_some()
                        || s.title_menu.is_some()
                        || s.approvals_open
                        || s.context_open
                        || s.tasks_open
                        || s.versions_open.is_some()
                    {
                        s.shortcuts_open = false;
                        s.chat_menu = None;
                        s.title_menu = None;
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
                    } else if s.doc_finding {
                        // The find bar first, then the file.
                        s.toggle_doc_find(cx);
                    } else if s.panel_full && s.panel == Some(crate::state::PanelTab::Agents) {
                        s.panel_full = false;
                        cx.notify();
                    } else if s.doc_full && s.doc.is_some() {
                        s.doc_full = false;
                        cx.notify();
                    } else if s.doc.is_some() && !s.busy() {
                        s.close_doc(cx);
                    } else if s.answer_waiting(crate::state::Verdict::Denied, cx) {
                        // A tool waiting on you: Esc denies it (its card's key).
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
            .on_action(cx.listener(|this, _: &InstallSkill, _, cx| {
                this.state.update(cx, |s, cx| s.install_skill(cx))
            }))
            .on_action(cx.listener(|this, _: &OpenMarketplaces, _, cx| {
                this.state.update(cx, |s, cx| s.open_marketplaces(cx))
            }))
            .on_action(cx.listener(|this, _: &OpenHooks, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Hooks, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &OpenPlugins, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.open_settings(crate::state::SettingsTab::Plugins, cx)
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
            .on_action(cx.listener(|this, _: &NextLanguage, _, cx| {
                let l = super::command_palette::use_next_language(cx);
                this.state.update(cx, |s, cx| {
                    s.show_toast(format!("{}: {}", crate::i18n::tr("Language"), l.name()), cx)
                })
            }))
            .on_action(cx.listener(|this, _: &NextPet, _, cx| {
                this.state.update(cx, |s, cx| s.next_pet(cx))
            }))
            .on_action(cx.listener(|this, _: &HidePet, _, cx| {
                this.state.update(cx, |s, cx| s.hide_pet(cx))
            }))
            .on_action(cx.listener(|this, _: &OpenDesign, _, cx| {
                this.state.update(cx, |s, cx| s.open_design_home(cx))
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
            .on_action(cx.listener(|this, _: &AllowWaiting, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.answer_waiting(crate::state::Verdict::Allowed, cx)
                });
            }))
            .on_action(cx.listener(|this, _: &AlwaysAllowWaiting, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.answer_waiting(crate::state::Verdict::AllowedAlways, cx)
                });
            }))
            .on_action(cx.listener(|this, _: &DenyWaiting, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.answer_waiting(crate::state::Verdict::Denied, cx)
                });
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
            // Text selected in the transcript (`select.rs`): a press
            // anywhere clears it (a paragraph pressed starts a new one), and
            // Ctrl+C copies it, before the focused box sees the key.
            .capture_any_mouse_down(|_, window, _| {
                if super::select::clear() {
                    window.refresh();
                }
            })
            .capture_key_down(|ev: &gpui::KeyDownEvent, _, cx| {
                let k = &ev.keystroke;
                if k.key == "c" && k.modifiers.secondary() && !k.modifiers.shift && !k.modifiers.alt
                {
                    if let Some(text) = super::select::selected_text() {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                        cx.stop_propagation();
                    }
                }
            })
            .on_action(cx.listener(|this, _: &GoBack, _, cx| {
                this.state.update(cx, |s, cx| s.step_place(false, cx))
            }))
            .on_action(cx.listener(|this, _: &GoForward, _, cx| {
                this.state.update(cx, |s, cx| s.step_place(true, cx))
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
            .on_action(cx.listener(|this, _: &ArchiveChat, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.toggle_archive(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleDetailedView, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| {
                        let on = s.chats.iter().any(|c| c.id == id && c.detailed);
                        s.set_detailed(id, !on, cx)
                    });
                }
            }))
            .on_action(cx.listener(|this, _: &ContinueInNewChat, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state
                        .update(cx, |s, cx| s.continue_in_new_chat(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &CopyChatMarkdown, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.copy_chat_markdown(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &OpenInVsCode, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.open_chat_in_vscode(id, cx));
                }
            }))
            .on_action(cx.listener(|this, _: &OpenChatFolder, _, cx| {
                if let Some(id) = this.current_chat(cx) {
                    this.state.update(cx, |s, cx| s.open_chat_folder(id, cx));
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
                let (term_drag, panel_drag, sidebar_open, terminal_open) = {
                    let s = this.state.read(cx);
                    (
                        s.terminal_resizing,
                        s.panel_resizing,
                        s.sidebar_open,
                        s.terminal_open,
                    )
                };
                if !term_drag && !panel_drag {
                    return;
                }
                // A drag lasts while the button is held: a release that went
                // elsewhere (the screen changed under the press) ends it here,
                // so the panel never follows a pointer that isn't dragging.
                if ev.pressed_button != Some(gpui::MouseButton::Left) {
                    this.state.update(cx, |s, cx| {
                        s.terminal_resizing = false;
                        s.panel_resizing = false;
                        s.save_window();
                        cx.notify();
                    });
                    return;
                }
                let vw = f32::from(window.viewport_size().width);
                if term_drag {
                    let share = ((vw - f32::from(ev.position.x)) / vw).clamp(0.05, 0.6);
                    this.state.update(cx, |s, cx| {
                        s.terminal_share = share;
                        cx.notify();
                    });
                } else if panel_drag {
                    let t = cx.theme().clone();
                    let sidebar_w = if sidebar_open {
                        f32::from(t.size.sidebar)
                    } else {
                        f32::from(t.size.rail)
                    };
                    let dock_used = if terminal_open {
                        this.dock_want(window, cx)
                    } else {
                        0.
                    };
                    let after_dock = (vw - sidebar_w - dock_used).max(1.);
                    let right_edge = vw - dock_used - f32::from(t.space.sm);
                    let share =
                        ((right_edge - f32::from(ev.position.x)) / after_dock).clamp(0.25, 0.72);
                    this.state.update(cx, |s, cx| {
                        s.panel_share = Some(share);
                        cx.notify();
                    });
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        if s.terminal_resizing || s.panel_resizing {
                            s.terminal_resizing = false;
                            s.panel_resizing = false;
                            s.save_window();
                            cx.notify();
                        }
                    })
                }),
            )
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .when(!presenting, |d| d.child(self.title_bar(bar, window, cx)))
            .children(super::status_bar::banner(
                self.state.read(cx),
                self.state.clone(),
                &t,
            ))
            .when(onboarding, |d| d.child(self.onboarding.clone()))
            // Presenting: the page alone fills the window.
            .when(!onboarding && presenting, |d| {
                d.child(div().flex_1().min_h_0().flex().child(self.browser.clone()))
            })
            .when(!onboarding && !presenting, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .bg(t.extra.pane)
                        .map(|d| {
                            // It slides out of its rail, and folds back into it.
                            if sidebar_open {
                                d.child(super::motion::sidebar_slide(
                                    self.sidebar.clone().into_any_element(),
                                    t.size.rail,
                                    t.size.sidebar,
                                    "sidebar-open",
                                    &t,
                                ))
                            } else {
                                d.child(super::motion::sidebar_slide(
                                    super::sidebar::rail(&t, screen, &self.state)
                                        .into_any_element(),
                                    t.size.sidebar,
                                    t.size.rail,
                                    "sidebar-close",
                                    &t,
                                ))
                            }
                        })
                        // The sidebar's shadow falls across the session's edge.
                        .child(
                            sheet
                                .relative()
                                .child(super::motion::screen_in(main, screen_key, &t))
                                .children(super::motion::side_shade(&t))
                                .when(screen == Screen::Chat, |m| m.child(self.pet.clone())),
                        )
                        .when(terminal_open, |d| d.child(dock)),
                )
                .child(super::status_bar::render(
                    self.state.read(cx),
                    self.state.clone(),
                    self.remote_picker.clone(),
                    &t,
                ))
            })
            .when(palette_f.is_some(), |d| d.child(self.palette.clone()))
            .child(self.side.clone())
            .child(self.settings.clone())
            .children(
                map_f.map(|(_, leaving)| super::project_map::render(&t, &self.state, leaving, cx)),
            )
            .children(
                shortcuts_f.map(|(_, leaving)| super::shortcuts::render(&t, &self.state, leaving)),
            )
            .children(self.chat_menu(chat_f, window, cx))
            .children(self.title_menu(title_f, window, cx))
            .child(self.confirm.clone())
            .child(self.attachment_editor.clone())
            .child(self.askpass.clone())
            .children(self.toast(toast_f, cx))
    }
}

/// What each window the workspace draws last showed, so it can leave.
#[derive(Default)]
struct Presences {
    palette: super::motion::Presence<()>,
    shortcuts: super::motion::Presence<()>,
    map: super::motion::Presence<()>,
    title: super::motion::Presence<(TitleMenu, gpui::Point<gpui::Pixels>)>,
    chat: super::motion::Presence<usize>,
    toast: super::motion::Presence<SharedString>,
    session: super::motion::Presence<usize>,
    dock: super::motion::Presence<()>,
    switcher: super::motion::Presence<()>,
    tasks: super::motion::Presence<()>,
    approvals: super::motion::Presence<()>,
    side: super::motion::Presence<(
        Option<crate::state::FileRef>,
        Option<crate::state::PanelTab>,
        bool,
    )>,
    doc_full: super::motion::Tween,
    /// A file pushed over the Files tab's list, and pulled back.
    push: super::motion::Push<crate::state::FileRef>,
}

/// The title bar's popovers: Some(leaving) while each shows or leaves.
#[derive(Clone, Copy)]
struct BarFrames {
    switcher: Option<bool>,
    tasks: Option<bool>,
    approvals: Option<bool>,
}

/// A window to draw (its key), and whether it is leaving.
type Frame<K> = Option<(K, bool)>;

impl Workspace {
    /// A small menu: the title bar's Share or ⋮, the sidebar's More or your name.
    fn title_menu(
        &self,
        frame: Frame<(TitleMenu, gpui::Point<gpui::Pixels>)>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let ((which, at), leaving) = frame?;
        let chat = s.current;
        let close = self.state.clone();
        // The title bar's open under their button, right edges level; the
        // sidebar's where they were asked for, your name's rising from it.
        let (at, corner) = match which {
            TitleMenu::Share | TitleMenu::More => (
                gpui::point(
                    at.x - t.size.menu_width + t.space.lg,
                    t.size.titlebar - px(2.),
                ),
                gpui::Anchor::TopLeft,
            ),
            TitleMenu::Help => (at, gpui::Anchor::TopLeft),
            TitleMenu::Account => (at, gpui::Anchor::BottomLeft),
        };
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(corner)
                    .position(at)
                    .snap_to_window_with_margin(t.space.sm)
                    .child(
                        div()
                            .when(!leaving, |d| {
                                d.on_mouse_down_out(move |ev, _, cx| {
                                    close.update(cx, |s, cx| {
                                        s.dismiss_title_menu(which, ev.position, cx)
                                    })
                                })
                            })
                            .child(super::motion::unfold(
                                div().child(super::title_menus::render(
                                    &t,
                                    &self.state,
                                    which,
                                    chat,
                                    window,
                                )),
                                &format!("title-menu-{which:?}"),
                                which == TitleMenu::Account,
                                leaving,
                                &t,
                            )),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The open chat menu, anchored where it was asked for (under the
    /// title bar's chat name, or at a sidebar row), over everything.
    fn chat_menu(
        &self,
        frame: Frame<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let t = cx.theme().clone();
        let s = self.state.read(cx);
        let (id, leaving) = frame?;
        let c = s.chats.iter().find(|c| c.id == id)?;
        let info = super::chat_menu::MenuChat {
            id: c.id,
            pinned: c.pinned,
            archived: c.archived,
            detailed: c.detailed,
            remote: s.chat_remote(c.id).is_some(),
        };
        let targets = if s.chat_sub == Some(crate::state::ChatSub::Move) {
            s.move_targets(c.id)
        } else {
            vec![]
        };
        let close = self.state.clone();
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(s.chat_menu_at)
                    .snap_to_window_with_margin(t.space.sm)
                    .child(
                        div()
                            .when(!leaving, |d| {
                                d.on_mouse_down_out(move |_, _, cx| {
                                    close.update(cx, |s, cx| s.open_chat_menu(None, cx))
                                })
                            })
                            .child(super::motion::unfold(
                                div().child(super::chat_menu::render(
                                    &t,
                                    &self.state,
                                    info,
                                    s.chat_sub,
                                    targets,
                                    window,
                                )),
                                &format!("chat-menu-{id}"),
                                false,
                                leaving,
                                &t,
                            )),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

/// Under this width the chat column is narrow: the composer's tray goes to
/// icons, the welcome's artifact cards to two a row.
pub const NARROW_CHAT: f32 = 600.;

/// Whether the optional Session column fits beside the chat, in `avail`
/// beside the sidebar, after the side panel (`panel`) and the dock (`dock`)
/// have their widths: only while the chat keeps `chat_min`. Otherwise the
/// column steps aside; it never crushes the chat.
fn session_fits(avail: f32, panel: f32, dock: f32, session: f32, chat_min: f32) -> bool {
    avail - panel - dock - session >= chat_min
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

/// The greeting, in the display serif, after KodAct's mark: an editorial
/// line, not a banner.
fn greeting_line(line: &str, split: bool, t: &crate::theme::Theme) -> impl IntoElement {
    // A greeting past 22 letters steps down a size, so it stays on one
    // line beside the mark (at the large size a 26-letter line left its
    // '?' alone on a second line).
    let long = line.chars().count() > 22;
    let mark_size = if split {
        (f32::from(t.size.mark_home) * 0.78).round()
    } else {
        f32::from(t.size.mark_home)
    };
    let text_size = if split {
        px(24.)
    } else if long {
        t.size.greeting
    } else {
        t.size.home_heading
    };
    div()
        .w_full()
        .flex()
        .justify_center()
        .items_center()
        .gap(if split { t.space.sm } else { t.space.md })
        // Centred on the capitals rather than the line box, whose descender
        // room would otherwise leave the mark sitting high.
        .child(
            div()
                .flex_none()
                .mt(px(3.))
                .child(super::logo(mark_size)),
        )
        .child(
            div()
                .min_w_0()
                .font_family(t.font_display.clone())
                .font_weight(t.display_weight)
                .text_size(text_size)
                .text_color(t.colors.text_strong)
                .line_height(gpui::relative(1.15))
                .text_center()
                .whitespace_nowrap()
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
    switcher_menu(state, window, cx, t.size.titlebar)
        .absolute()
        .top(top)
        .left_0()
}

/// The switcher's list, unplaced: the title bar drops it below its opener,
/// the composer's project chip lifts it above. `room_top` is how far down
/// the window the menu can start, for its height.
pub fn switcher_menu(
    state: &Entity<AppState>,
    window: &Window,
    cx: &App,
    room_top: Pixels,
) -> gpui::Stateful<gpui::Div> {
    let t = cx.theme().clone();
    let s = state.read(cx);
    let home = crate::rules::home_dir();
    let hover = t.colors.hover;
    let mut projects: Vec<_> = s
        .projects
        .iter()
        .filter(|p| p.saved && (!p.archived || p.id == s.current_project))
        .cloned()
        .collect();
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
    let mut list = div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .when(!projects.is_empty(), |d| d.child(label("Projects")));
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
    let (all, open, change) = (state.clone(), state.clone(), state.clone());
    // Connected to a host: another folder there, without reconnecting.
    let elsewhere = s.current_remote().map(|r| r.target.label());
    theme::fit_window(
        theme::popover(&t).id("switcher"),
        window,
        &t,
        room_top,
        None,
    )
    .w(px(340.))
    .p(t.space.xs)
    .child(list)
    .child(theme::rule(&t).my(t.space.xs))
    .children(elsewhere.map(|host| {
        row("switch-change".into(), false)
            .child(
                div()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        "icons/folder-open.svg",
                        t.size.icon_sm,
                        t.colors.text_muted,
                    )),
            )
            .child(
                div()
                    .text_color(t.colors.text)
                    .child(format!("Open another folder on {host}…")),
            )
            .on_click(move |_, _, cx| {
                change.update(cx, |s, cx| {
                    s.project_menu = false;
                    s.change_remote_folder(cx)
                })
            })
    }))
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
                    s.project_menu = false;
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
                    s.project_menu = false;
                    s.open_new_project(cx)
                })
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chat_keeps_its_minimum_width_when_the_panel_opens() {
        let (chat_min, session) = (400., 300.);
        // 1280 wide with the sidebar: 990 beside it.
        let avail = 990.;
        // The panel wants 560 and may shrink to 320: the chat keeps 400.
        let (panel, dock) = share_out(avail, Some((560., 320.)), None, chat_min);
        assert!(avail - panel - dock >= chat_min, "panel {panel}");
        // With the panel open, the Session column doesn't fit: it steps aside.
        assert!(!session_fits(avail, panel, dock, session, chat_min));
        // With no panel, it fits beside the chat.
        assert!(session_fits(avail, 0., 0., session, chat_min));
        // A wide window keeps all three.
        let wide = 1700.;
        let (panel, dock) = share_out(wide, Some((560., 320.)), None, chat_min);
        assert!(session_fits(wide, panel, dock, session, chat_min));
        // The dock and the panel together never push the chat under its
        // minimum while either can still give way.
        let (panel, dock) = share_out(avail, Some((560., 320.)), Some((400., 260.)), chat_min);
        assert!(avail - panel - dock >= chat_min - 0.5, "{panel} + {dock}");
    }

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

/// The title bar's back or forward button: dimmed, and doing nothing, when
/// there's nowhere to go that way.
fn nav_button(
    id: &'static str,
    path: &'static str,
    tip: &'static str,
    can: bool,
    action: Box<dyn gpui::Action>,
    t: &crate::theme::Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .occlude()
        .tooltip(move |window, cx| gpui::component::tooltip::Tooltip::new(tip).build(window, cx))
        .size(t.size.control)
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.radius.pill)
        .child(icon(
            path,
            t.size.icon,
            if can {
                t.colors.text_muted
            } else {
                t.colors.text_muted.opacity(0.35)
            },
        ))
        .when(can, move |d| {
            d.cursor_pointer()
                .hover(move |st| st.bg(hover))
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
        })
}
