//! The window's views. `workspace` composes them; each file is one component.

pub mod activity;
pub mod add_menu;
pub mod agents_editor;
pub mod agents_view;
pub mod approvals;
pub mod askpass;
pub mod attachment_editor;
pub mod attachments;
pub mod autocomplete;
pub mod browser_view;
pub mod button;
pub mod changed_files;
pub mod chat_menu;
pub mod chat_view;
pub mod command_palette;
pub mod compose_style;
pub mod composer;
pub mod confirm;
pub mod design_bar;
pub mod design_chips;
pub mod design_comment;
pub mod design_home;
pub mod design_marks;
pub mod context_menu;
pub mod context_ring;
pub mod diagnostics;
pub mod diagram_view;
pub mod doc_changes;
pub mod doc_comments;
pub mod doc_find;
pub mod doc_pane;
pub mod documents;
pub mod editor_kit;
pub mod env_editor;
pub mod file_card;
pub mod file_icon;
pub mod file_icon_map;
pub mod file_mark;
pub mod files_pane;
pub mod find_bar;
pub mod goal_bar;
pub mod guide;
pub mod hooks_editor;
pub mod kodact_mark;
mod lens;
pub mod markdown;
pub mod marketplaces;
pub mod mcp_editor;
pub mod memory_view;
pub mod message;
pub mod mode_menu;
pub mod heat_rail;
pub mod pet;
pub mod model_menu;
pub mod motion;
pub mod new_project;
pub mod onboarding;
pub mod opener;
pub mod permissions_editor;
pub mod planned;
pub mod plugins_editor;
pub mod profiles_editor;
pub mod project_defaults;
pub mod project_github;
pub mod project_knowledge;
pub mod project_map;
pub mod project_memory;
pub mod project_rail;
pub mod project_sharing;
pub mod projects;
pub mod question_card;
pub mod queued;
pub mod remote_menu;
pub mod rules_view;
pub mod sandbox_editor;
pub mod select;
pub mod session_panel;
pub mod settings;
pub mod sheet_view;
pub mod shortcuts;
pub mod side_chat;
pub mod sidebar;
pub mod skeleton;
pub mod skill_install;
pub mod skills_editor;
pub mod slash_commands;
pub mod source_view;
pub mod starters;
pub mod status_bar;
pub mod tasks;
pub mod terminal;
pub mod terminal_dock;
pub mod title_menus;
pub mod versions;
pub mod work_group;
pub mod workbench;
pub mod workspace;

use gpui::{div, prelude::*, svg, App, Div, Hsla, Pixels, SharedString, Svg};

use crate::theme::{ActiveTheme, Theme};

gpui::actions!(
    forge,
    [
        NewChat,
        ToggleSidebar,
        ToggleCommandPalette,
        ToggleTheme,
        ToggleBrowser,
        SetUpDocumentTools,
        InstallLibreOffice,
        OpenRemoteMenu,
        ConnectWsl,
        ConnectHost,
        ReopenInContainer,
        RebuildContainer,
        ReopenLocally,
        AttachContainer,
        CloseRemote,
        ToggleTerminal,
        ShowAgentOutput,
        OpenAppSettings,
        KillTerminal,
        ClearTerminal,
        FocusTerminal,
        NewTerminal,
        FocusComposer,
        ToggleModelPicker,
        CycleMode,
        StopReply,
        StopAgents,
        EditQueued,
        RemoveQueued,
        OpenSettings,
        OpenSlashCommands,
        OpenAgents,
        OpenSkills,
        InstallSkill,
        OpenMarketplaces,
        OpenHooks,
        OpenPlugins,
        OpenMcpServers,
        OpenPermissions,
        OpenProfiles,
        OpenGuide,
        ToggleSplit,
        TogglePanel,
        ShowPlanView,
        ShowAgentsView,
        ShowFilesView,
        ShowBrowserView,
        ShowProjectMap,
        CopyLastReply,
        ChangeName,
        ChangeAvatar,
        OpenRules,
        OpenDesign,
        NextPet,
        NextLanguage,
        HidePet,
        OpenProjects,
        EndpointGuide,
        OpenEndpointsFile,
        ToggleFind,
        FindInFile,
        ChangeRemoteFolder,
        NextPage,
        PreviousPage,
        ExportPdf,
        AttachFiles,
        ExportChatJson,
        ShareChat,
        ShowContext,
        ToggleTasks,
        ShowApprovals,
        AllowWaiting,
        AlwaysAllowWaiting,
        DenyWaiting,
        JumpToLatest,
        GoBack,
        GoForward,
        ShowShortcuts,
        RenameChat,
        DuplicateChat,
        ExportChat,
        DeleteChat,
        ArchiveChat,
        OpenDocuments,
        ToggleSession,
        ToggleFiles,
        ShowChatFiles,
        ToggleDetailedView,
        ContinueInNewChat,
        CopyChatMarkdown,
        OpenInVsCode,
        OpenChatFolder,
        RegenerateReply,
        ContinueReply,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        ZoomFit,
        Present,
        SavePagePdf,
        SavePagePng,
        SavePagePptx,
        SavePagePptxEditable,
        SavePageHtml,
        SavePageFigma,
        NewDesignPage,
        CommentTool,
        LayoutGuides,
        PageOutline,
        UndoPage,
        RedoPage,
        DuplicatePage,
        CritiquePage,
        DesignChat,
        DesignDeck,
        DesignPrototype,
        DesignDocument,
        EndDesignChat,
        Quit,
    ]
);

/// A Lucide icon from the bundled set, tinted with a theme colour.
pub fn icon(path: &'static str, size: Pixels, color: Hsla) -> Svg {
    svg().path(path).size(size).flex_none().text_color(color)
}

/// KodAct's mark (`kodact_mark.rs`), `size` px square.
pub fn logo(size: f32) -> kodact_mark::Mark {
    kodact_mark::mark(size)
}

/// The mark while KodAct works: its three strokes (`.` `/` `<`) sweep
/// through a 3D perspective wave. Reduced motion: the mark at rest.
#[allow(dead_code)]
pub fn turning_logo(size: f32, id: impl Into<gpui::ElementId>) -> gpui::AnyElement {
    kodact_mark::working(size, id, None)
}

/// The working mark for work in progress in the conversation, retaining the
/// high-contrast `./<` sprite palette with its 3D wave.
pub fn turning_logo_in(
    size: f32,
    id: impl Into<gpui::ElementId>,
    _color: gpui::Hsla,
) -> gpui::AnyElement {
    // Drawn a little larger than an icon so its three strokes read.
    kodact_mark::working(size * 1.25, id, None)
}

/// Your avatar, drawn `size` px square at this screen's density.
pub fn avatar_image(name: &str, size: f32, window: &gpui::Window) -> gpui::Img {
    let px = (size * window.scale_factor()).round() as u32;
    gpui::img(SharedString::from(crate::avatars::path(name, px)))
        .size(gpui::px(size))
        .flex_none()
}

/// The platform's name for the main modifier.
pub fn mod_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl "
    }
}

/// A shortcut hint: `Ctrl K` / `⌘K`.
pub fn shortcut(key: &str) -> SharedString {
    format!("{}{key}", mod_key()).into()
}

/// A search field as the app draws one (the sidebar's): a recessed box
/// with a hairline edge that firms on hover, a search icon, the input.
pub fn search_field(t: &Theme, input: &gpui::Entity<gpui::component::input::InputState>) -> Div {
    let edge = t.extra.border_strong;
    div()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .h(t.size.row + gpui::px(2.))
        .px(t.space.sm)
        .rounded(t.radius.field)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.extra.sunk)
        .hover(move |d| d.border_color(edge))
        .child(icon("icons/search.svg", t.size.icon_sm, t.colors.text_muted))
        .child(
            div().flex_1().min_w_0().child(
                gpui::component::input::Input::new(input)
                    .appearance(false)
                    .text_size(t.text.body_sm),
            ),
        )
}

/// A small key hint, as shown beside menu items.
pub fn kbd(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .flex_none()
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(text.into())
}

/// Give gpui-component's widgets (text inputs, scrollbars) our tokens, so
/// they match everything drawn by hand. Called at start and on theme change.
pub fn sync_component_theme(cx: &mut App) {
    use gpui::component::theme::{Theme as ComponentTheme, ThemeMode};
    let t = cx.theme().clone();
    ComponentTheme::change(
        if t.is_dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    ComponentTheme::update(cx, |c| {
        c.font_family = t.font_sans.clone();
        c.mono_font_family = t.font_mono.clone();
        c.font_size = t.text.body_sm;
        c.radius = t.radius.button;
        c.radius_lg = t.radius.card;
        c.shadow = false;
        c.background = t.extra.stage;
        c.foreground = t.colors.text;
        c.border = t.colors.border;
        c.input = t.colors.border;
        c.ring = t.colors.primary_tint;
        c.caret = t.colors.primary;
        c.selection = t.colors.primary_tint;
        c.primary = t.colors.primary;
        c.primary_hover = t.colors.primary_hover;
        c.primary_foreground = t.colors.primary_fg;
        c.muted = t.colors.hover;
        c.muted_foreground = t.colors.text_subtle;
        c.popover = t.colors.surface;
        c.popover_foreground = t.colors.text;
        c.accent = t.colors.hover;
        c.accent_foreground = t.colors.text;
        c.title_bar = t.extra.chrome;
        c.title_bar_border = t.colors.border;
        c.window_border = t.colors.border;
        c.scrollbar = t.extra.stage.opacity(0.);
        c.scrollbar_thumb = t.colors.text_subtle.opacity(0.35);
        c.scrollbar_thumb_hover = t.colors.text_subtle.opacity(0.6);
    });
}

/// A scroll box inside something that scrolls (a list in Settings, a
/// tool's output in a reply). GPUI hands each wheel tick to every scroll
/// box under the pointer, so the page behind moved with the list. This one
/// keeps the wheel while it has more to show that way; one that fits, or
/// is at its end, lets the page scroll as usual. `handle` is the box's own, kept by its view.
pub fn hold_scroll(
    d: gpui::Stateful<gpui::Div>,
    handle: &gpui::ScrollHandle,
) -> gpui::Stateful<gpui::Div> {
    let h = handle.clone();
    d.overflow_y_scroll()
        .track_scroll(handle)
        .on_scroll_wheel(move |ev, window, cx| {
            // At its end the wheel goes on to the page around it, so a list
            // that fills the view never traps the page.
            let dy = ev.delta.pixel_delta(window.line_height()).y;
            let (y, max) = (h.offset().y, h.max_offset().y);
            let can = if dy < gpui::px(0.) {
                y > -max + gpui::px(0.5)
            } else {
                y < gpui::px(-0.5)
            };
            if max > gpui::px(0.) && can {
                cx.stop_propagation();
            }
        })
}

/// Keep the wheel inside a box known to have more than it shows (a
/// fixed-height prompt box, a long tool output): it scrolls, the page
/// around it stays.
pub fn hold_wheel<E: gpui::InteractiveElement>(e: E) -> E {
    e.on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

/// Switch theme and mode: our tokens, then the component layer.
pub fn set_theme(look: crate::theme::Look, dark: bool, cx: &mut App) {
    cx.set_global(Theme::of(look, dark));
    sync_component_theme(cx);
    cx.refresh_windows();
}

/// Mouse-down position for a `ClickEvent` (matches `MouseDownOutEvent.position`).
pub fn click_down(ev: &gpui::ClickEvent) -> gpui::Point<Pixels> {
    match ev {
        gpui::ClickEvent::Mouse(m) => m.down.position,
        gpui::ClickEvent::Keyboard(_) | gpui::ClickEvent::Touch(_) => ev.position(),
    }
}

/// A line-limited view over tool output or diffs (`> max_lines`), keeping the
/// full string intact behind a "Show more" toggle.
pub struct Truncated<'a> {
    pub shown: &'a str,
    pub hidden_lines: usize,
    pub truncated: bool,
}

/// Return at most `max_lines` of `text` (and cap an enormous single line at ~800
/// chars) so long output collapses cleanly without deleting anything.
pub fn truncate_lines(text: &str, max_lines: usize) -> Truncated<'_> {
    let trimmed = text.trim_end();
    let total_lines = if trimmed.is_empty() {
        0
    } else {
        trimmed.lines().count()
    };
    if total_lines > max_lines {
        let mut seen = 0usize;
        let mut cut = trimmed.len();
        for (i, ch) in trimmed.char_indices() {
            if ch == '\n' {
                seen += 1;
                if seen == max_lines {
                    cut = i;
                    break;
                }
            }
        }
        return Truncated {
            shown: trimmed[..cut].trim_end_matches('\r'),
            hidden_lines: total_lines - max_lines,
            truncated: true,
        };
    }
    const MAX_SINGLE_CHARS: usize = 800;
    if trimmed.chars().count() > MAX_SINGLE_CHARS {
        let cut = trimmed
            .char_indices()
            .nth(MAX_SINGLE_CHARS)
            .map(|(i, _)| i)
            .unwrap_or(trimmed.len());
        return Truncated {
            shown: trimmed[..cut].trim_end(),
            hidden_lines: 0,
            truncated: true,
        };
    }
    Truncated {
        shown: trimmed,
        hidden_lines: 0,
        truncated: false,
    }
}

#[cfg(test)]
mod ui_helper_tests {
    use super::*;

    #[test]
    fn truncate_lines_keeps_short_and_splits_long() {
        let short = "line 1\nline 2\nline 3";
        let t = truncate_lines(short, 10);
        assert!(!t.truncated);
        assert_eq!(t.hidden_lines, 0);
        assert_eq!(t.shown, short);

        let long = (1..=15)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let t = truncate_lines(&long, 10);
        assert!(t.truncated);
        assert_eq!(t.hidden_lines, 5);
        assert_eq!(t.shown.lines().count(), 10);
        assert!(t.shown.ends_with("line 10"));
    }

    #[test]
    fn truncate_lines_caps_huge_single_line() {
        let big = "a".repeat(1200);
        let t = truncate_lines(&big, 10);
        assert!(t.truncated);
        assert_eq!(t.shown.len(), 800);
    }
}

