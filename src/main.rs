//! Forge: a desktop app for ForgeCLI, built on GPUI. Each chat is a `forge`
//! process (`backend.rs`); the demo chats in the sidebar are mock data
//! (`state.rs`), and `FORGE_DEMO=1` makes every reply the scripted demo.

// A release build on Windows is a windowed app: no console opens beside it.
// Debug builds keep the console, for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// Which shell commands run without asking, per mode (Forge for VS Code's
// rules). Called when a chat's CLI asks for permission (state.rs); the
// Edit-automatically hook (`edit_mode_asks`) is ForgeCLI's job now.
#[cfg_attr(not(test), allow(dead_code))]
mod approve;
mod assets;
mod attach;
mod avatars;
mod backend;
mod browser;
mod browser_mcp;
mod commands;
mod context;
mod diagnose;
mod diagram;
mod downloads;
mod endpoints;
mod export;
mod fonts;
mod forge_files;
mod greeting;
mod health;
mod highlight;
mod history;
mod mermaid;
mod modes;
mod office;
#[cfg(test)]
mod palette_check;
mod plugin;
mod profile;
mod remote;
mod rules;
mod secrets;
mod setting_profiles;
mod stage;
mod state;
mod stubs;
#[cfg(test)]
mod system_md;
mod term_session;
mod theme;
mod ui;
mod web_view;

// Forge's own settings file, and SSH passwords asked in the app.
mod app_settings;
mod askpass;
mod model_proxy;
mod relay;

use gpui::component::TitleBar;
use gpui::{
    px, size, App, AppContext, Bounds, KeyBinding, WindowBounds, WindowDecorations, WindowOptions,
};

use ui::*;

/// Every binding here is listed on the shortcuts sheet (`ui/shortcuts.rs`);
/// its test fails when the two disagree.
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
        KeyBinding::new("f1", ShowShortcuts, None),
        KeyBinding::new("f2", RenameChat, Some("Workspace")),
        KeyBinding::new("secondary-f", ToggleFind, Some("Workspace")),
        KeyBinding::new("secondary-j", JumpToLatest, Some("Workspace")),
        KeyBinding::new("secondary-u", AttachFiles, Some("Workspace")),
        KeyBinding::new("secondary-shift-t", ToggleTasks, Some("Workspace")),
        KeyBinding::new("secondary-shift-a", ShowApprovals, None),
        KeyBinding::new("secondary-\\", ToggleSplit, Some("Workspace")),
        KeyBinding::new("secondary-shift-e", TogglePanel, Some("Workspace")),
        KeyBinding::new("secondary-alt-o", OpenRemoteMenu, None),
        KeyBinding::new("ctrl-`", ToggleTerminal, None),
        KeyBinding::new("ctrl-~", NewTerminal, None),
        KeyBinding::new("secondary-shift-m", ShowProjectMap, Some("Workspace")),
        KeyBinding::new("shift-tab", CycleMode, Some("Composer")),
        KeyBinding::new("escape", StopReply, Some("Workspace")),
    ]);
    // Ctrl+Shift+` as a platform may spell it instead of Ctrl+~ (the
    // sheet lists the key once, as `ctrl-~`).
    let new_terminal_alias = "ctrl-shift-`";
    cx.bind_keys([KeyBinding::new(new_terminal_alias, NewTerminal, None)]);
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
            let entry = format!(
                "--- {when} (Forge {})\n{info}\n{trace}\n",
                env!("CARGO_PKG_VERSION")
            );
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("crash.log"))
            {
                let _ = f.write_all(entry.as_bytes());
            }
        }
        default(info);
    }));
}

fn main() {
    // `--browser-mcp <devtools> <focus-file>`: serve the browser tools to a
    // ForgeCLI (`browser_mcp.rs`) instead of opening the window.
    let args: Vec<String> = std::env::args().collect();
    // Run by ssh as its askpass program: hand the prompt to the app's dialog
    // and print the answer (`askpass.rs`).
    if let Ok(addr) = std::env::var(askpass::ENV) {
        let prompt = args.get(1).cloned().unwrap_or_default();
        std::process::exit(askpass::client(&addr, &prompt));
    }
    // `--browser-guard <chromium> <args…>`: the side panel's browser, closed
    // when the app goes (`web_view.rs`).
    if args.get(1).map(String::as_str) == Some("--browser-guard") {
        web_view::guard(&args[2..]);
        return;
    }
    if args.get(1).map(String::as_str) == Some("--browser-mcp") {
        let devtools = args.get(2).cloned().unwrap_or_default();
        let focus = args
            .get(3)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(browser::focus_file);
        browser_mcp::serve(&devtools, &focus);
        return;
    }
    log_crashes();
    gpui::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
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
            gpui::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::workspace::Workspace::new(window, cx))
            })
            .expect("the window opens");
            cx.activate(true);
        });
}
