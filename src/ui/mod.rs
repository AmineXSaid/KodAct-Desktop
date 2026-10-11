//! The window's views. `workspace` composes them; each file is one component.

pub mod chat_view;
pub mod command_palette;
pub mod composer;
pub mod forge_cube;
pub mod mode_menu;
pub mod model_menu;
pub mod motion;
pub mod markdown;
pub mod message;
pub mod onboarding;
pub mod projects;
pub mod rules_view;
pub mod sidebar;
pub mod skeleton;
pub mod terminal;
pub mod voxels;
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
        FocusComposer,
        ToggleModelPicker,
        CycleMode,
        StopReply,
        OpenSettings,
        CopyLastReply,
        ChangeName,
        OpenRules,
        OpenProjects,
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
    const MADE: [u32; 13] = [32, 40, 48, 56, 64, 80, 96, 120, 144, 168, 192, 240, 288];
    let want = (size * window.scale_factor()).round() as u32;
    let file = MADE.iter().copied().find(|n| *n >= want).unwrap_or(288);
    gpui::img(SharedString::from(format!("images/cube-{file}.png"))).size(gpui::px(size)).flex_none()
}

/// The platform's name for the main modifier.
pub fn mod_key() -> &'static str {
    if cfg!(target_os = "macos") { "⌘" } else { "Ctrl " }
}

/// A shortcut hint: `Ctrl K` / `⌘K`.
pub fn shortcut(key: &str) -> SharedString {
    format!("{}{key}", mod_key()).into()
}

/// A small key hint, as shown beside menu items.
pub fn kbd(t: &Theme, text: impl Into<SharedString>) -> Div {
    div().flex_none().text_size(t.text.caption).text_color(t.colors.text_subtle).child(text.into())
}

/// Give gpui-component's widgets (text inputs, scrollbars) our tokens, so
/// they match everything drawn by hand. Called at start and on theme change.
pub fn sync_component_theme(cx: &mut App) {
    use gpui::component::theme::{Theme as ComponentTheme, ThemeMode};
    let t = cx.theme().clone();
    ComponentTheme::change(if t.is_dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
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
