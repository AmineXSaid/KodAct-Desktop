//! KodAct: a desktop app for KodAct CLI, built on GPUI. Each chat is a `kodact`
//! process (`backend.rs`); the demo chats in the sidebar are mock data
//! (`state.rs`), and `KODACT_DEMO=1` makes every reply the scripted demo.

// A release build on Windows is a windowed app: no console opens beside it.
// Debug builds keep the console, for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// Which shell commands run without asking, per mode (KodAct for VS Code's
// rules). Called when a chat's CLI asks for permission (state.rs); the
// Edit-automatically hook (`edit_mode_asks`) is KodAct CLI's job now.
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
mod design;
mod design_mcp;
mod diagnose;
mod doc_tools;
mod diagram;
mod doc_versions;
mod doc_zoom;
mod downloads;
mod draw;
mod endpoints;
mod error_words;
mod export;
mod fonts;
mod i18n;
mod forge_files;
mod git_status;
mod github_knowledge;
mod greeting;
mod health;
mod highlight;
mod history;
mod knowledge;
mod mermaid;
mod modes;
mod names;
mod office;
mod office_render;
#[cfg(test)]
mod palette_check;
mod plugin;
mod profile;
mod project_ctx;
mod project_memory;
mod project_share;
mod remote;
mod rules;
mod secrets;
mod setting_profiles;
mod shell_prompt;
mod skill_install;
mod stage;
mod state;
mod stubs;
#[cfg(test)]
mod system_md;
mod templates;
mod term_session;
mod text_diff;
mod theme;
mod title;
mod ui;
mod web_view;
mod xlsx_style;

// KodAct's own settings file, and SSH passwords asked in the app.
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
        // Back and forward through the places you were, as Claude Desktop.
        KeyBinding::new("alt-left", GoBack, None),
        KeyBinding::new("alt-right", GoForward, None),
        KeyBinding::new("secondary-[", GoBack, None),
        KeyBinding::new("secondary-]", GoForward, None),
        KeyBinding::new("secondary-shift-l", ToggleTheme, None),
        KeyBinding::new("secondary-l", FocusComposer, None),
        KeyBinding::new("secondary-/", ToggleModelPicker, None),
        KeyBinding::new("secondary-shift-c", CopyLastReply, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-shift-r", OpenRules, None),
        KeyBinding::new("secondary-shift-d", OpenDesign, None),
        KeyBinding::new("secondary-shift-i", InstallSkill, None),
        KeyBinding::new("secondary-shift-p", OpenProjects, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("f1", ShowShortcuts, None),
        KeyBinding::new("f2", RenameChat, Some("Workspace")),
        KeyBinding::new("secondary-f", ToggleFind, Some("Workspace")),
        KeyBinding::new("secondary-shift-f", FindInFile, Some("Workspace")),
        KeyBinding::new("alt-down", NextPage, Some("Workspace")),
        KeyBinding::new("alt-up", PreviousPage, Some("Workspace")),
        KeyBinding::new("secondary-s", SaveDocEdits, Some("Workspace")),
        KeyBinding::new("secondary-=", DocZoomIn, Some("Workspace")),
        KeyBinding::new("secondary-+", DocZoomIn, Some("Workspace")),
        KeyBinding::new("secondary--", DocZoomOut, Some("Workspace")),
        KeyBinding::new("secondary-0", DocZoomFit, Some("Workspace")),
        KeyBinding::new("secondary-alt-t", ToggleThumbnails, Some("Workspace")),
        KeyBinding::new("secondary-g", GoToPage, Some("Workspace")),
        KeyBinding::new("secondary-j", JumpToLatest, Some("Workspace")),
        KeyBinding::new("secondary-u", AttachFiles, Some("Workspace")),
        KeyBinding::new("secondary-shift-t", ToggleTasks, Some("Workspace")),
        KeyBinding::new("secondary-shift-a", ShowApprovals, None),
        KeyBinding::new("secondary-\\", ToggleSplit, Some("Workspace")),
        KeyBinding::new("alt-f", ToggleDocFull, Some("Workspace")),
        KeyBinding::new("secondary-shift-e", TogglePanel, Some("Workspace")),
        KeyBinding::new("secondary-1", ShowPlanView, Some("Workspace")),
        KeyBinding::new("secondary-2", ShowAgentsView, Some("Workspace")),
        KeyBinding::new("secondary-shift-j", ToggleAgentJumper, Some("Workspace")),
        KeyBinding::new("secondary-3", ShowFilesView, Some("Workspace")),
        KeyBinding::new("secondary-4", ShowBrowserView, Some("Workspace")),
        KeyBinding::new("secondary-alt-o", OpenRemoteMenu, None),
        KeyBinding::new("ctrl-`", ToggleTerminal, None),
        KeyBinding::new("ctrl-~", NewTerminal, None),
        KeyBinding::new("secondary-shift-m", ShowProjectMap, Some("Workspace")),
        KeyBinding::new("shift-tab", CycleMode, Some("Composer")),
        KeyBinding::new("escape", StopReply, Some("Workspace")),
        // Not Ctrl+Shift+Esc: Windows keeps that for the Task Manager.
        KeyBinding::new("secondary-shift-x", StopAgents, Some("Workspace")),
        KeyBinding::new("secondary-alt-x", StopShownAgent, Some("Workspace")),
        KeyBinding::new("secondary-alt-c", CopyAgentOutput, Some("Workspace")),
        KeyBinding::new("secondary-alt-f", TogglePanelFull, Some("Workspace")),
        // The browser's page, while it has the keyboard; Present anywhere.
        KeyBinding::new("secondary-=", ZoomIn, Some("Browser")),
        KeyBinding::new("secondary--", ZoomOut, Some("Browser")),
        KeyBinding::new("secondary-0", ZoomReset, Some("Browser")),
        KeyBinding::new("f11", Present, Some("Workspace")),
    ]);
    // Ctrl+Shift+` as a platform may spell it instead of Ctrl+~ (the
    // sheet lists the key once, as `ctrl-~`).
    let new_terminal_alias = "ctrl-shift-`";
    cx.bind_keys([KeyBinding::new(new_terminal_alias, NewTerminal, None)]);
}

/// A crash leaves its reason in `crash.log` in KodAct's config folder: a
/// windowed app on Windows has no console to print it to.
fn log_crashes() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = profile::dir() {
            let _ = std::fs::create_dir_all(&dir);
            let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let trace = std::backtrace::Backtrace::force_capture();
            let entry = format!(
                "--- {when} (KodAct {})\n{info}\n{trace}\n",
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

/// Settings made before the app was renamed (from Forge) keep working: each
/// `FORGE_*` variable stands for its `KODACT_*` name when that one is unset,
/// as the engine does (KodAct CLI's `adopt_forgecli_env`). Runs first, while
/// the process has one thread, before anything reads a setting.
fn adopt_forge_env() {
    let legacy: Vec<(String, std::ffi::OsString)> = std::env::vars_os()
        .filter_map(|(k, v)| {
            let rest = k.into_string().ok()?.strip_prefix("FORGE_")?.to_string();
            Some((format!("KODACT_{rest}"), v))
        })
        .collect();
    for (name, value) in legacy {
        if std::env::var_os(&name).is_none() {
            std::env::set_var(name, value);
        }
    }
}

fn main() {
    adopt_forge_env();
    // `--browser-mcp <devtools> <focus-file>`: serve the browser tools to a
    // KodAct CLI (`browser_mcp.rs`) instead of opening the window.
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
    // `--design-check`: load the design library and report on it (`design::check`).
    if args.get(1).map(String::as_str) == Some("--design-check") {
        std::process::exit(design::check(&args[2..]));
    }
    // `--design-mcp <devtools> <focus-file>`: serve the design tools
    // (`design_mcp.rs`); an empty devtools address means no browser.
    if args.get(1).map(String::as_str) == Some("--design-mcp") {
        let devtools = args.get(2).cloned().unwrap_or_default();
        let focus = args
            .get(3)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(browser::focus_file);
        design_mcp::serve(&devtools, &focus);
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
    i18n::init();
    gpui::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui::init(cx);
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
                // The title bar is drawn by the app, but the window still
                // needs a name for the taskbar, Alt+Tab and the window list.
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("KodAct".into()),
                    ..TitleBar::title_bar_options()
                }),
                ..TitleBar::window_options()
            };
            gpui::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::workspace::Workspace::new(window, cx))
            })
            .expect("the window opens");
            cx.activate(true);
        });
}
