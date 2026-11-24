//! KodAct's setup: one dark window that installs the desktop app, KodAct CLI and what they work
//! with, for the person alone (no administrator rights), and later removes them
//! (`Uninstall.exe --uninstall`, the same program without its payload).
//!
//!   KodAct-Setup.exe                 the window
//!   KodAct-Setup.exe /S              installs everything, silently
//!   Uninstall.exe --uninstall [/S]   removes KodAct
//!
//! Off Windows, or run without a payload, it is a preview: the screens with an install played
//! back (`KODACT_SETUP_SCREEN=welcome|working|done|failed|uninstall` holds one, for screenshots).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod payload;
mod system;
mod ui;
mod work;

use std::path::PathBuf;

/// What the setup was asked to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Install,
    Uninstall {
        dir: PathBuf,
    },
    /// No payload: screens only, an install played back.
    Preview,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |a: &str| args.iter().skip(1).any(|x| x.eq_ignore_ascii_case(a));
    let silent = has("/S") || has("--silent");
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let exe = payload::own_exe().unwrap_or_default();

    let mode = if has("--uninstall") {
        let dir = value("--dir")
            .map(PathBuf::from)
            .or_else(|| exe.parent().map(PathBuf::from))
            .unwrap_or_default();
        // The uninstaller can't delete itself while it runs: it carries on from a copy in the
        // temp folder, which removes itself when it ends.
        if cfg!(windows) && value("--dir").is_none() {
            let copy =
                std::env::temp_dir().join(format!("KodAct-Uninstall-{}.exe", std::process::id()));
            if std::fs::copy(&exe, &copy).is_ok() {
                let mut c = std::process::Command::new(&copy);
                c.args(["--uninstall", "--dir", &dir.display().to_string()]);
                if silent {
                    c.arg("/S");
                }
                if c.spawn().is_ok() {
                    return;
                }
            }
        }
        Mode::Uninstall { dir }
    } else if payload::locate(&exe).is_some() {
        Mode::Install
    } else {
        Mode::Preview
    };

    if silent {
        std::process::exit(run_silent(&mode, &exe));
    }
    ui::run(mode, exe);
}

/// `/S`: every option on, no window; 0 when it worked.
fn run_silent(mode: &Mode, exe: &std::path::Path) -> i32 {
    let failed = std::cell::Cell::new(false);
    let report = |r: work::Report| {
        if let work::Report::Failed(e) = r {
            eprintln!("{e}");
            failed.set(true);
        }
    };
    match mode {
        Mode::Install => {
            let has_office = payload::Payload::open(exe)
                .is_ok_and(|p| p.manifest.sizes.contains_key("libreoffice"));
            let c = work::Choices {
                dir: work::default_dir(),
                libreoffice: has_office,
                path: true,
                nudges: true,
                desktop: true,
            };
            work::install(exe, &c, report);
        }
        Mode::Uninstall { dir } => {
            work::uninstall(dir, report);
            system::delete_after_exit(exe);
        }
        Mode::Preview => {
            eprintln!("This setup carries nothing to install.");
            return 1;
        }
    }
    i32::from(failed.get())
}
