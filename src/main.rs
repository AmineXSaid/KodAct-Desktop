//! Forge: a desktop app for ForgeCLI, built on GPUI. Each chat is a `forge`
//! process (`backend.rs`); the demo chats in the sidebar are mock data
//! (`state.rs`), and `FORGE_DEMO=1` makes every reply the scripted demo.

// A release build on Windows is a windowed app: no console opens beside it.
// Debug builds keep the console, for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// Which shell commands run without asking, per mode (Forge for VS Code's
// rules). Called when a chat's CLI asks for permission; this UI shell has no
// CLI yet, so outside its tests nothing calls it.
#[cfg_attr(not(test), allow(dead_code))]
mod approve;
mod assets;
mod avatars;
mod backend;
mod context;
mod diagram;
mod downloads;
mod endpoints;
mod fonts;
mod greeting;
mod health;
mod highlight;
mod modes;
mod office;
#[cfg(test)]
mod palette_check;
#[cfg(test)]
mod system_md;
mod profile;
mod rules;
mod secrets;
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

/// A crash leaves its reason in `crash.log` in Forge's config folder: a
/// windowed app on Windows has no console to print it to.
fn log_crashes() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = profile::dir() {
            let _ = std::fs::create_dir_all(&dir);
            let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let trace = std::backtrace::Backtrace::force_capture();
            let entry = format!("--- {when} (Forge {})\n{info}\n{trace}\n", env!("CARGO_PKG_VERSION"));
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
                let _ = f.write_all(entry.as_bytes());
            }
        }
        default(info);
    }));
}

fn main() {
    log_crashes();
    gpui::application().with_assets(assets::Assets).run(|cx: &mut App| {
        gpui::init(cx);
        assets::load_fonts(cx);
        fonts::choose(cx);
        highlight::warm_up();
        theme::init(cx);
        ui::sync_component_theme(cx);
        bind_keys(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(1120.), px(740.)), cx);
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
