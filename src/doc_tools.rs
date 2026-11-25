//! What the office-documents skill needs on this computer, found or set up
//! by the app so the person never installs it by hand:
//!
//! - **Python with the document libraries** (python-docx, openpyxl,
//!   python-pptx, pypdf, reportlab). The Windows installer puts one beside
//!   the app (`python\python.exe`). Without that, the app looks for a Python
//!   that already has them, and failing that makes its own in the config
//!   folder (`doc-tools/venv`, a venv of the system's Python) and installs
//!   the pinned libraries there, once.
//! - **LibreOffice**, which recalculates workbooks, exports to PDF and draws
//!   pages. It is far too large to carry: the app says when it is missing
//!   and installs it on request (winget on Windows; elsewhere it opens the
//!   download page).
//!
//! The Python found is handed to every local KodAct CLI session: its folder
//! goes first on PATH, so the skill's `python …` runs it, and
//! `KODACT_PYTHON` names it exactly. Everything here blocks (it starts
//! programs): call it off the UI thread.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// The libraries, pinned to the versions the skill's own tests pass with
/// (`assets/kodact-plugin/skills/office-documents/scripts/test_tools.py`),
/// and the module each is imported as.
pub const LIBS: [(&str, &str); 8] = [
    ("python-docx==1.2.0", "docx"),
    ("openpyxl==3.1.5", "openpyxl"),
    ("python-pptx==1.0.2", "pptx"),
    ("pypdf==6.17.0", "pypdf"),
    ("pdfplumber==0.11.4", "pdfplumber"),
    ("reportlab==5.0.1", "reportlab"),
    ("pillow==11.1.0", "PIL"),
    // Draws PDF pages as images for `render_preview.py` where poppler isn't installed
    // (every Windows PC): what lets a chat look at the slides or pages it made.
    ("pypdfium2==5.14.0", "pypdfium2"),
];

/// Where the document tools stand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Python {
    /// Not looked at yet.
    Unknown,
    /// A Python with every library: the one sessions are given.
    Ready(PathBuf),
    /// Being set up now (the venv, then pip).
    Installing,
    /// A Python is here, without the libraries, and setting them up failed
    /// (or was never tried): why.
    Missing(String),
    /// No Python on this computer at all.
    NoPython,
}

static PYTHON: Mutex<Python> = Mutex::new(Python::Unknown);

/// The document tools' state, as last found.
pub fn python() -> Python {
    PYTHON.lock().map(|p| p.clone()).unwrap_or(Python::Unknown)
}

fn set(p: Python) {
    if let Ok(mut now) = PYTHON.lock() {
        *now = p;
    }
}

/// The Python sessions are given, once one is ready.
pub fn ready() -> Option<PathBuf> {
    match python() {
        Python::Ready(p) => Some(p),
        _ => None,
    }
}

/// What a local KodAct CLI session's environment gains: `KODACT_SOFFICE`, the
/// LibreOffice the app found (the installer's own copy is on no PATH, so a
/// chat can't find it otherwise); UTF-8 Python stdio; then, once the document
/// tools' Python is ready, PATH with its folder first and `KODACT_PYTHON`.
pub fn session_env() -> Vec<(String, std::ffi::OsString)> {
    let soffice = crate::office_render::soffice().map(|p| ("KODACT_SOFFICE".to_string(), p.into_os_string()));
    // How the skill's `render` draws pages: the route the panel uses, so the
    // model never looks for tools (`office`, `libreoffice` or `none`).
    let route = (
        "KODACT_OFFICE_RENDER".to_string(),
        std::ffi::OsString::from(crate::office_render::route_name()),
    );
    let utf8 = [
        ("PYTHONUTF8".to_string(), std::ffi::OsString::from("1")),
        ("PYTHONIOENCODING".to_string(), std::ffi::OsString::from("utf-8")),
    ];
    let Some(python) = ready() else {
        return soffice.into_iter().chain([route]).chain(utf8).collect();
    };
    let mut out = vec![("KODACT_PYTHON".to_string(), python.clone().into_os_string())];
    out.extend(soffice);
    out.push(route);
    out.extend(utf8);
    if let Some(dir) = python.parent() {
        let rest = std::env::var_os("PATH").unwrap_or_default();
        let all = std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&rest));
        if let Ok(path) = std::env::join_paths(all) {
            out.push(("PATH".to_string(), path));
        }
    }
    out
}

/// The app's own venv, in the config folder.
fn venv_dir() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("doc-tools").join("venv"))
}

fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

/// The Python the installer put beside the app.
fn bundled() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("python").join(if cfg!(windows) {
        "python.exe"
    } else {
        "bin/python3"
    });
    p.is_file().then_some(p)
}

fn quiet(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Does this Python run, and import every library?
fn has_libs(python: &Path) -> bool {
    let imports: Vec<&str> = LIBS.iter().map(|(_, module)| *module).collect();
    quiet(python)
        .args(["-c", &format!("import {}", imports.join(", "))])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A Python on PATH that really runs (Windows keeps a `python.exe` that only
/// opens the Store): its own path, as it reports it.
fn system_python() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["python.exe", "python3.exe", "py.exe"]
    } else {
        &["python3", "python"]
    };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if !candidate.is_file() {
                continue;
            }
            let out = quiet(&candidate)
                .args(["-c", "import sys; print(sys.executable)"])
                .stderr(Stdio::null())
                .output();
            if let Ok(out) = out {
                let real = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if out.status.success() && !real.is_empty() {
                    return Some(PathBuf::from(real));
                }
            }
        }
    }
    None
}

/// Look for a Python that has the libraries: the installer's, the app's own
/// venv, the system's. Sets and returns what was found.
pub fn find() -> Python {
    let own = venv_dir().map(|v| venv_python(&v)).filter(|p| p.is_file());
    for candidate in [bundled(), own].into_iter().flatten() {
        if has_libs(&candidate) {
            set(Python::Ready(candidate.clone()));
            return Python::Ready(candidate);
        }
    }
    let found = match system_python() {
        Some(p) if has_libs(&p) => Python::Ready(p),
        Some(_) => Python::Missing("The document libraries aren't installed yet.".into()),
        None => Python::NoPython,
    };
    set(found.clone());
    found
}

/// The last lines a failed program wrote, for the person to read.
fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join(" ")
}

/// Make the app's own venv and install the pinned libraries in it. Needs a
/// Python on this computer and the network (PyPI). Sets the state as it goes.
pub fn install() -> Python {
    set(Python::Installing);
    let done = (|| -> Result<PathBuf, Python> {
        let system = system_python().ok_or(Python::NoPython)?;
        let venv = venv_dir().ok_or_else(|| {
            Python::Missing("KodAct has no config folder to install into.".into())
        })?;
        let fail = |what: &str, out: &[u8]| Python::Missing(format!("{what}: {}", tail(out)));
        let python = venv_python(&venv);
        if !python.is_file() {
            if let Some(parent) = venv.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let made = quiet(&system)
                .args(["-m", "venv"])
                .arg(&venv)
                .output()
                .map_err(|e| Python::Missing(format!("Python did not start: {e}")))?;
            if !made.status.success() {
                return Err(fail("Could not make a Python environment", &made.stderr));
            }
        }
        let pip = quiet(&python)
            .args([
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-input",
            ])
            .args(LIBS.iter().map(|(spec, _)| *spec))
            .output()
            .map_err(|e| Python::Missing(format!("pip did not start: {e}")))?;
        if !pip.status.success() {
            return Err(fail(
                "Could not install the document libraries",
                &pip.stderr,
            ));
        }
        if !has_libs(&python) {
            return Err(Python::Missing(
                "The libraries installed, but Python can't import them.".into(),
            ));
        }
        Ok(python)
    })();
    let now = match done {
        Ok(p) => Python::Ready(p),
        Err(why) => why,
    };
    set(now.clone());
    now
}

/// At launch: find a Python with the libraries, and when there is a Python
/// without them, set them up (once: a failure is remembered in the config
/// folder, so a computer without the network isn't asked at every start;
/// Settings and the command palette try again).
pub fn prepare() -> Python {
    let found = find();
    if !matches!(found, Python::Missing(_)) {
        return found;
    }
    let tried = crate::profile::dir().map(|d| d.join("doc-tools").join("tried"));
    if tried.as_deref().is_some_and(Path::is_file) {
        return found;
    }
    let now = install();
    if let (Python::Missing(why), Some(mark)) = (&now, tried) {
        if let Some(parent) = mark.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(mark, why);
    }
    now
}

/// Is LibreOffice here?
pub fn libreoffice() -> bool {
    crate::office_render::soffice().is_some()
}

/// Where LibreOffice is downloaded from, for a computer without winget.
pub const LIBREOFFICE_URL: &str = "https://www.libreoffice.org/download/download-libreoffice/";

/// Install LibreOffice with winget (Windows). Blocks for minutes. Elsewhere,
/// or without winget, Err says so and the caller opens `LIBREOFFICE_URL`.
pub fn install_libreoffice() -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Install LibreOffice from its download page.".into());
    }
    let out = quiet(Path::new("winget"))
        .args([
            "install",
            "--exact",
            "--id",
            "TheDocumentFoundation.LibreOffice",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .output()
        .map_err(|_| {
            "winget isn't on this PC. Install LibreOffice from its download page.".to_string()
        })?;
    if out.status.success() && libreoffice() {
        Ok(())
    } else {
        Err(format!(
            "winget could not install LibreOffice. {}",
            tail(if out.stderr.is_empty() {
                &out.stdout
            } else {
                &out.stderr
            })
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_library_is_pinned_and_names_its_module() {
        for (spec, module) in LIBS {
            let (name, version) = spec.split_once("==").expect("pinned with ==");
            assert!(!name.is_empty() && version.chars().next().is_some_and(|c| c.is_ascii_digit()));
            assert!(!module.is_empty() && !module.contains('-'), "{module}");
        }
    }

    /// One test for both: they share the state, so two would race on it.
    #[test]
    fn a_ready_python_goes_first_on_the_sessions_path_and_nothing_before_that() {
        let dir = std::env::temp_dir()
            .join("kodact-doc-tools-test")
            .join("bin");
        let python = dir.join("python");
        let before = python_swap(Python::Ready(python.clone()));
        let env = session_env();
        python_swap(Python::Missing("no network".into()));
        let none = session_env();
        python_swap(before);
        assert_eq!(env[0].0, "KODACT_PYTHON");
        assert_eq!(PathBuf::from(&env[0].1), python);
        let path = &env.iter().find(|(k, _)| k == "PATH").expect("PATH").1;
        assert_eq!(std::env::split_paths(path).next(), Some(dir));
        // Before Python is ready, at most where LibreOffice is and UTF-8 Python I/O.
        assert!(
            none.iter().all(|(k, _)| matches!(
                k.as_str(),
                "KODACT_SOFFICE" | "KODACT_OFFICE_RENDER" | "PYTHONUTF8" | "PYTHONIOENCODING"
            )),
            "{none:?}"
        );
        assert!(
            none.iter().any(|(k, _)| k == "KODACT_OFFICE_RENDER"),
            "the render route is always named"
        );
    }

    /// The whole set-up, for real: a Python without the libraries, an empty
    /// config folder, PyPI. Slow and needs the network, and it changes this
    /// process's environment, so it runs alone:
    /// `cargo test installs_the_libraries -- --ignored`.
    #[test]
    #[ignore]
    fn installs_the_libraries_into_its_own_venv() {
        let Some(system) = system_python() else {
            return;
        };
        let root = std::env::temp_dir().join(format!("kodact-doc-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // A bare Python: a venv of the system's, with nothing installed.
        let bare = root.join("bare");
        assert!(Command::new(&system)
            .args(["-m", "venv"])
            .arg(&bare)
            .status()
            .unwrap()
            .success());
        let bare_python = venv_python(&bare);
        assert!(!has_libs(&bare_python), "a new venv has no libraries");
        std::env::set_var("KODACT_CONFIG_DIR", root.join("config"));
        std::env::set_var("PATH", bare_python.parent().unwrap());
        assert!(matches!(find(), Python::Missing(_)));
        let now = prepare();
        let Python::Ready(python) = now else {
            panic!("not ready: {now:?}");
        };
        assert!(python.starts_with(root.join("config").join("doc-tools")));
        assert!(has_libs(&python));
        // Found again at the next start, with nothing to install.
        assert_eq!(find(), Python::Ready(python));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn python_swap(to: Python) -> Python {
        let mut now = PYTHON.lock().unwrap();
        std::mem::replace(&mut *now, to)
    }
}
