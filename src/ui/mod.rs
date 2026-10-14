//! The window's views. `workspace` composes them; each file is one component.

pub mod agents_editor;
pub mod agents_view;
pub mod approvals;
pub mod askpass;
pub mod attachments;
pub mod autocomplete;
pub mod browser_view;
pub mod chat_menu;
pub mod chat_view;
pub mod command_palette;
pub mod composer;
pub mod confirm;
pub mod context_menu;
pub mod context_ring;
pub mod diagnostics;
pub mod diagram_view;
pub mod doc_pane;
pub mod editor_kit;
pub mod env_editor;
pub mod file_card;
pub mod file_icon;
pub mod file_mark;
pub mod find_bar;
pub mod forge_cube;
pub mod guide;
pub mod hooks_editor;
pub mod markdown;
pub mod mcp_editor;
pub mod memory_view;
pub mod message;
pub mod mode_menu;
pub mod model_menu;
pub mod motion;
pub mod onboarding;
pub mod permissions_editor;
pub mod planned;
pub mod plugins_editor;
pub mod profiles_editor;
pub mod project_map;
pub mod projects;
pub mod remote_menu;
pub mod rules_view;
pub mod sandbox_editor;
pub mod settings;
pub mod sheet_view;
pub mod shortcuts;
pub mod side_chat;
pub mod sidebar;
pub mod skeleton;
pub mod skills_editor;
pub mod slash_commands;
pub mod starters;
pub mod status_bar;
pub mod tasks;
pub mod terminal;
pub mod terminal_dock;
pub mod versions;
pub mod voxels;
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
        OpenSettings,
        OpenSlashCommands,
        OpenAgents,
        OpenSkills,
        OpenHooks,
        OpenMcpServers,
        OpenPermissions,
        OpenProfiles,
        OpenGuide,
        ToggleSplit,
        TogglePanel,
        ShowProjectMap,
        CopyLastReply,
        ChangeName,
        ChangeAvatar,
        OpenRules,
        OpenProjects,
        EndpointGuide,
        OpenEndpointsFile,
        ToggleFind,
        AttachFiles,
        ExportChatJson,
        ShareChat,
        ShowContext,
        ToggleTasks,
        ShowApprovals,
        JumpToLatest,
        ShowShortcuts,
        RenameChat,
        DuplicateChat,
        ExportChat,
        DeleteChat,
        RegenerateReply,
        ContinueReply,
        Quit,
    ]
);

/// A Lucide icon from the bundled set, tinted with a theme colour.
pub fn icon(path: &'static str, size: Pixels, color: Hsla) -> Svg {
    svg().path(path).size(size).flex_none().text_color(color)
}

/// The Forge cube picture (beside your name, and on the welcome card), at
/// 32 or 96px. It uses the rendering made for this screen's density, so the
/// picture is shown pixel for pixel, never squeezed.
pub fn cube_image(size: f32, window: &gpui::Window) -> gpui::Img {
    const MADE: [u32; 16] = [
        32, 40, 48, 56, 60, 64, 72, 80, 84, 96, 120, 144, 168, 192, 240, 288,
    ];
    let want = (size * window.scale_factor()).round() as u32;
    let file = MADE.iter().copied().find(|n| *n >= want).unwrap_or(288);
    gpui::img(SharedString::from(format!("images/cube-{file}.png")))
        .size(gpui::px(size))
        .flex_none()
}

/// The Forge logo (the cube picture) at `size`, from the rendering made for
/// twice that, so it stays sharp up to a 2x screen without a window to ask.
pub fn logo(size: f32) -> gpui::Img {
    const MADE: [u32; 16] = [
        32, 40, 48, 56, 60, 64, 72, 80, 84, 96, 120, 144, 168, 192, 240, 288,
    ];
    let want = (size * 2.).round() as u32;
    let file = MADE.iter().copied().find(|n| *n >= want).unwrap_or(288);
    gpui::img(SharedString::from(format!("images/cube-{file}.png")))
        .size(gpui::px(size))
        .flex_none()
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

/// The one treatment for a control whose feature isn't wired yet
/// (`stubs.rs`): drawn muted, with a "Soon" tooltip naming what it will do.
/// It stays clickable, and its click says so (`AppState::stub`), never
/// pretending to work.
pub fn soon<E: gpui::StatefulInteractiveElement + gpui::Styled>(el: E, stub: &'static str) -> E {
    let line = crate::stubs::get(stub)
        .map(|s| s.soon_line())
        .unwrap_or_else(|| "Soon".into());
    el.opacity(0.5).tooltip(move |window, cx| {
        gpui::component::tooltip::Tooltip::new(line.clone()).build(window, cx)
    })
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

/// Switch light/dark: our tokens, then the component layer.
pub fn set_dark(dark: bool, cx: &mut App) {
    cx.set_global(if dark { Theme::dark() } else { Theme::light() });
    sync_component_theme(cx);
    cx.refresh_windows();
}
