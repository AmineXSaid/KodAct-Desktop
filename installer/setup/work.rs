//! Installing and uninstalling, step by step, on a thread of their own: each step and the bytes
//! unpacked go to the window as [`Report`]s.

use std::path::{Path, PathBuf};

use crate::payload::{Payload, PARTS};
use crate::system;

/// What the person chose on the first screen.
#[derive(Clone, Debug)]
pub struct Choices {
    pub dir: PathBuf,
    pub libreoffice: bool,
    pub path: bool,
    pub nudges: bool,
    pub desktop: bool,
    pub taskbar: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Report {
    /// The step that is running now (its index in the list the window shows).
    Step(usize),
    /// Bytes unpacked of the total, and the file being written.
    Bytes {
        done: u64,
        total: u64,
        file: String,
    },
    /// Finished; what didn't work but doesn't stop KodAct from running.
    Done {
        warnings: Vec<String>,
    },
    Failed(String),
}

/// The steps of an install, as the window lists them.
pub fn install_steps(libreoffice: bool) -> Vec<&'static str> {
    let mut v: Vec<&str> = PARTS
        .iter()
        .filter(|(id, _)| *id != "libreoffice" || libreoffice)
        .map(|(_, l)| *l)
        .collect();
    v.push("Shortcuts and Apps & features");
    v
}

pub const UNINSTALL_STEPS: [&str; 3] = [
    "Shortcuts, PATH and Apps & features",
    "KodAct's files",
    "The install folder",
];

/// A file a running program holds: KodAct, or a chat's KodAct CLI.
fn in_use(file: &Path) -> bool {
    file.exists() && std::fs::OpenOptions::new().append(true).open(file).is_err()
}

pub const RUNNING: &str =
    "KodAct is open. Close it (and any kodact running in a terminal), then try again.";

pub fn install(exe: &Path, c: &Choices, send: impl Fn(Report)) {
    match try_install(exe, c, &send) {
        Ok(warnings) => send(Report::Done { warnings }),
        Err(e) => send(Report::Failed(e)),
    }
}

fn try_install(exe: &Path, c: &Choices, send: &impl Fn(Report)) -> Result<Vec<String>, String> {
    let dir = &c.dir;
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't make {}: {e}", dir.display()))?;
    if in_use(&dir.join("KodAct.exe")) || in_use(&dir.join("bin").join("kodact.exe")) {
        return Err(RUNNING.into());
    }
    let mut payload = Payload::open(exe)?;
    let wanted = |part: &str| part != "libreoffice" || c.libreoffice;

    // Parts KodAct owns whole are replaced whole, so an update leaves nothing stale. A design
    // library is KodAct's when it has UPSTREAM.json; LibreOffice only when it is being installed.
    let _ = std::fs::remove_dir_all(dir.join("python"));
    let _ = std::fs::remove_dir_all(dir.join("bin").join("linux-x86_64"));
    if dir.join("design-library").join("UPSTREAM.json").exists() {
        let _ = std::fs::remove_dir_all(dir.join("design-library"));
    }
    if c.libreoffice {
        let _ = std::fs::remove_dir_all(dir.join("libreoffice"));
    }

    let order: Vec<&str> = PARTS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| wanted(id))
        .collect();
    let total: u64 = order
        .iter()
        .map(|id| payload.manifest.sizes.get(*id).copied().unwrap_or(0))
        .sum();
    let mut step = usize::MAX;
    payload.unpack(dir, wanted, |file, done| {
        let at = order
            .iter()
            .position(|id| *id == crate::payload::part_of(file))
            .unwrap_or(0);
        if at != step {
            step = at;
            send(Report::Step(at));
        }
        send(Report::Bytes {
            done,
            total: total.max(done),
            file: file.to_string(),
        });
    })?;

    // The uninstaller: this .exe without what it carries.
    send(Report::Step(order.len()));
    let stub = crate::payload::stub_len(exe).ok_or("this setup carries nothing to install")?;
    copy_prefix(exe, &dir.join("Uninstall.exe"), stub)?;

    let mut warnings = vec![];
    let version = payload.manifest.version.clone();
    system::register(dir, &version, total / 1024)
        .map_err(|e| format!("Couldn't add KodAct to Apps & features: {e}"))?;
    if let Err(e) = system::shortcuts(dir, c.desktop) {
        warnings.push(format!("The Start menu shortcut couldn't be made ({e})."));
    }
    if c.taskbar && !system::pin_to_taskbar() {
        warnings.push(
            "Windows lets only you pin apps to the taskbar: right-click KodAct in the Start menu, then Pin to taskbar."
                .into(),
        );
    }
    if c.path {
        let bin = dir.join("bin");
        match system::add_to_path(&bin) {
            Ok(true) => {
                let _ = system::reg_set(
                    system::APP_KEY,
                    "AddedToPath",
                    "REG_SZ",
                    &bin.display().to_string(),
                );
            }
            Ok(false) => {}
            Err(e) => warnings.push(format!("kodact couldn't be added to your PATH ({e}).")),
        }
    }
    if c.nudges {
        if let Ok(true) = system::turn_on_nudges() {
            let _ = system::reg_set(system::APP_KEY, "SetNudges", "REG_SZ", "1");
        }
    }
    Ok(warnings)
}

/// The first `len` bytes of `from`, as `to`.
fn copy_prefix(from: &Path, to: &Path, len: u64) -> Result<(), String> {
    use std::io::Read;
    let f = std::fs::File::open(from).map_err(|e| e.to_string())?;
    let mut out = std::fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
    std::io::copy(&mut f.take(len), &mut out).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok(())
}

pub fn uninstall(dir: &Path, send: impl Fn(Report)) {
    if in_use(&dir.join("KodAct.exe")) || in_use(&dir.join("bin").join("kodact.exe")) {
        return send(Report::Failed(RUNNING.into()));
    }
    send(Report::Step(0));
    system::unregister();
    send(Report::Step(1));
    let mut gone = vec![
        "KodAct.exe",
        "kodact.ico",
        "bin",
        "fonts",
        "python",
        "libreoffice",
        "Uninstall.exe",
    ];
    if dir.join("design-library").join("UPSTREAM.json").exists() {
        gone.push("design-library");
    }
    let total = gone.len() as u64;
    for (i, name) in gone.iter().enumerate() {
        let p = dir.join(name);
        let _ = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
        send(Report::Bytes {
            done: i as u64 + 1,
            total,
            file: name.to_string(),
        });
    }
    send(Report::Step(2));
    // The folder goes only when nothing else is left in it (a file of yours stays).
    let _ = std::fs::remove_dir(dir);
    send(Report::Done { warnings: vec![] });
}

/// Where KodAct goes by default: `%LOCALAPPDATA%\Programs\KodAct`, for you alone.
pub fn default_dir() -> PathBuf {
    if let Some(d) = system::installed_dir() {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("Programs").join("KodAct"))
        .unwrap_or_else(|| std::env::temp_dir().join("KodAct"))
}
