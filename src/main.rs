//! Forge: a desktop chat shell for Claude Code, built on GPUI.
//! UI only: the conversation, models and replies are mock data (`state.rs`).

mod assets;
mod highlight;
mod profile;
mod rules;
mod state;
mod theme;
mod ui;

use gpui::component::TitleBar;
use gpui::{px, size, App, AppContext, Bounds, KeyBinding, WindowBounds, WindowDecorations, WindowOptions};

use ui::*;

fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-k", ToggleCommandPalette, None),
        KeyBinding::new("secondary-n", NewChat, None),
        KeyBinding::new("secondary-b", ToggleSidebar, None),
        KeyBinding::new("secondary-shift-l", ToggleTheme, None),
        KeyBinding::new("secondary-l", FocusComposer, None),
        KeyBinding::new("secondary-/", ToggleModelPicker, None),
        KeyBinding::new("secondary-shift-c", CopyLastReply, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-shift-r", OpenRules, None),
        KeyBinding::new("secondary-shift-p", OpenProjects, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("shift-tab", CycleMode, Some("Composer")),
        KeyBinding::new("escape", StopReply, Some("Workspace")),
    ]);
}

fn main() {
    gpui::application().with_assets(assets::Assets).run(|cx: &mut App| {
        gpui::init(cx);
        assets::load_fonts(cx);
        highlight::warm_up();
        theme::init(cx);
        ui::sync_component_theme(cx);
        bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(1240.), px(820.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(720.), px(520.))),
            // Draw our own frame, so the title bar's minimize / maximize /
            // close buttons show on Linux as they do on Windows. GPUI falls
            // back to the system frame where the desktop can't support it.
            window_decorations: Some(WindowDecorations::Client),
            ..TitleBar::window_options()
        };
        gpui::open_window(options, cx, |window, cx| cx.new(|cx| ui::workspace::Workspace::new(window, cx))).expect("the window opens");
        cx.activate(true);
    });
}
