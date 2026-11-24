//! Word, PowerPoint and Excel files as they print: Microsoft Office (on
//! Windows, through COM, by the office-documents skill's `office_to_pdf.ps1`)
//! or else LibreOffice turns them into a PDF, which `office.rs` draws page by
//! page, so the side panel shows a document's layout, fonts and pictures, and
//! a deck's slides as designed. Office comes first: it draws a deck exactly as
//! the person's PowerPoint will, and most Windows computers have it.
//!
//! `KODACT_OFFICE_RENDER` (`office`, `libreoffice` or `none`) picks one, for
//! tests and for a computer where one of them misbehaves.
//!
//! LibreOffice is used when it is on this computer (`KODACT_SOFFICE`, the
//! copy KodAct's installer puts beside the app, PATH, or where LibreOffice's
//! own installer puts it); without it the panel shows the file's
//! text, as before. Each file is rendered once, keyed by its bytes, into
//! KodAct's temp folder. Rendering blocks for seconds: run it off the UI
//! thread.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The longest one file may take; a stuck LibreOffice is stopped.
const TIME_LIMIT: Duration = Duration::from_secs(90);

/// LibreOffice's program, if this computer has it.
pub fn soffice() -> Option<PathBuf> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND.get_or_init(find).clone()
}

fn find() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KODACT_SOFFICE").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    // The copy the installer put beside the app (`installer/stage_libreoffice.sh`).
    let beside = std::env::current_exe().ok().and_then(|exe| {
        let p = exe
            .parent()?
            .join("libreoffice")
            .join("program")
            .join(if cfg!(windows) { "soffice.exe" } else { "soffice" });
        p.is_file().then_some(p)
    });
    if beside.is_some() {
        return beside;
    }
    let names: &[&str] = if cfg!(windows) {
        &["soffice.exe", "soffice.com"]
    } else {
        &["soffice", "libreoffice"]
    };
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
            .find(|p| p.is_file())
    });
    on_path.or_else(|| {
        let known: Vec<PathBuf> = if cfg!(windows) {
            ["ProgramFiles", "ProgramFiles(x86)"]
                .iter()
                .filter_map(std::env::var_os)
                .map(|d| PathBuf::from(d).join("LibreOffice/program/soffice.exe"))
                .collect()
        } else if cfg!(target_os = "macos") {
            vec!["/Applications/LibreOffice.app/Contents/MacOS/soffice".into()]
        } else {
            vec![
                "/usr/lib/libreoffice/program/soffice".into(),
                "/opt/libreoffice/program/soffice".into(),
            ]
        };
        known.into_iter().find(|p| p.is_file())
    })
}

/// What draws Office files here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renderer {
    /// Microsoft Office, through COM.
    Office,
    LibreOffice,
}

fn forced() -> Option<String> {
    std::env::var("KODACT_OFFICE_RENDER")
        .ok()
        .map(|v| v.to_ascii_lowercase())
        .filter(|v| !v.is_empty())
}

/// The skill's helper that asks Office for a PDF.
fn helper() -> Option<PathBuf> {
    let p = crate::plugin::dir()?
        .join("skills")
        .join("office-documents")
        .join("scripts")
        .join("office_to_pdf.ps1");
    p.is_file().then_some(p)
}

static OFFICE: OnceLock<bool> = OnceLock::new();

/// Whether Microsoft Office can draw files here: asked once (it starts
/// PowerShell, so never on the UI thread: [`warm`] asks at launch).
fn office_blocking() -> bool {
    *OFFICE.get_or_init(|| {
        if !cfg!(windows) {
            return false;
        }
        let Some(helper) = helper() else { return false };
        Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(helper)
            .arg("-Probe")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success() && !o.stdout.trim_ascii().is_empty())
    })
}

/// Find out at launch, off the UI thread, whether Office is here.
pub fn warm() {
    std::thread::spawn(office_blocking);
}

/// What draws Office files here, as far as is known now (never blocks).
pub fn renderer() -> Option<Renderer> {
    choose(
        forced().as_deref(),
        OFFICE.get().copied().unwrap_or(false),
        soffice().is_some(),
    )
}

/// The route from what was asked for (`KODACT_OFFICE_RENDER`) and what is
/// here: Office first, then LibreOffice.
fn choose(forced: Option<&str>, office: bool, libreoffice: bool) -> Option<Renderer> {
    match forced {
        Some("none") => None,
        Some("libreoffice") => libreoffice.then_some(Renderer::LibreOffice),
        Some("office" | "powerpoint") => Some(Renderer::Office),
        _ if office => Some(Renderer::Office),
        _ => libreoffice.then_some(Renderer::LibreOffice),
    }
}

/// Can Office files be drawn as pages here?
pub fn can_draw() -> bool {
    renderer().is_some()
}

/// For a chat's KodAct CLI: `KODACT_OFFICE_RENDER`, so the skill's renderer
/// uses the same route and the model never looks for tools.
pub fn route_name() -> &'static str {
    match renderer() {
        Some(Renderer::Office) => "office",
        Some(Renderer::LibreOffice) => "libreoffice",
        None => "none",
    }
}

/// Where rendered files are kept.
fn cache_dir() -> PathBuf {
    std::env::temp_dir().join("forge-previews")
}

/// A key for these bytes: the same file renders once.
pub fn key(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// `name` (whose extension says what it is) as a PDF.
pub fn to_pdf(name: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let forced = forced();
    let use_office = match forced.as_deref() {
        Some("none") => {
            return Err("drawing Office files is turned off (KODACT_OFFICE_RENDER)".into())
        }
        Some("libreoffice") => false,
        Some("office" | "powerpoint") => true,
        _ => office_blocking(),
    };
    if use_office {
        match office_pdf(name, bytes) {
            Ok(pdf) => return Ok(pdf),
            // Office is missing that app (Excel, say), or failed: LibreOffice may still draw it.
            Err(e) if soffice().is_none() || forced.is_some() => return Err(e),
            Err(_) => {}
        }
    }
    let program = soffice().ok_or("neither Microsoft Office nor LibreOffice is installed")?;
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()))
        .ok_or("the file has no type")?;
    let dir = cache_dir();
    let id = format!("{:016x}-{ext}", key(bytes));
    let done = dir.join(format!("{id}.pdf"));
    if let Ok(pdf) = std::fs::read(&done) {
        return Ok(pdf);
    }
    // One LibreOffice at a time: it keeps one profile, and two at once
    // trip over it.
    static ONE: Mutex<()> = Mutex::new(());
    let _turn = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let job = dir.join(&id);
    std::fs::create_dir_all(&job).map_err(|e| e.to_string())?;
    let input = job.join(format!("file.{ext}"));
    std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
    let result = run(&program, &input, &job).and_then(|()| {
        std::fs::read(job.join("file.pdf")).map_err(|_| "LibreOffice made no PDF".to_string())
    });
    if let Ok(pdf) = &result {
        let _ = std::fs::write(&done, pdf);
    }
    let _ = std::fs::remove_dir_all(&job);
    result
}

/// `name` as a PDF drawn by Microsoft Office. The bytes are copied into
/// KodAct's temp folder first: a deck still open in PowerPoint, or being
/// rewritten, is never touched.
fn office_pdf(name: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let helper = helper().ok_or("the office-documents helper is missing")?;
    let ext = extension(name)?;
    let dir = cache_dir();
    let id = format!("{:016x}-{ext}", key(bytes));
    let done = dir.join(format!("{id}.office.pdf"));
    if let Ok(pdf) = std::fs::read(&done) {
        return Ok(pdf);
    }
    // One Office export at a time: COM apps are single instances.
    static ONE: Mutex<()> = Mutex::new(());
    let _turn = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let job = dir.join(format!("{id}-office"));
    std::fs::create_dir_all(&job).map_err(|e| e.to_string())?;
    let input = job.join(format!("file.{ext}"));
    std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
    let out = job.join("file.pdf");
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&helper)
        .arg(&input)
        .arg(&out);
    let result = wait(cmd, "Microsoft Office")
        .and_then(|()| std::fs::read(&out).map_err(|_| "Office made no PDF".to_string()));
    if let Ok(pdf) = &result {
        let _ = std::fs::write(&done, pdf);
    }
    let _ = std::fs::remove_dir_all(&job);
    result
}

fn extension(name: &str) -> Result<String, String> {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()))
        .ok_or_else(|| "the file has no type".to_string())
}

/// Run `cmd` to its end, or stop it after [`TIME_LIMIT`].
fn wait(mut cmd: Command, who: &str) -> Result<(), String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{who} did not start: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("{who} stopped ({status})")),
            Ok(None) if start.elapsed() > TIME_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{who} took too long"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn run(program: &Path, input: &Path, out: &Path) -> Result<(), String> {
    let profile = url(&cache_dir().join("libreoffice-profile"));
    let mut child = Command::new(program)
        .arg(format!("-env:UserInstallation={profile}"))
        .args([
            "--headless",
            "--norestore",
            "--nolockcheck",
            "--convert-to",
            "pdf",
            "--outdir",
        ])
        .arg(out)
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("LibreOffice did not start: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("LibreOffice stopped ({status})")),
            Ok(None) if start.elapsed() > TIME_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("LibreOffice took too long".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// A folder as a file URL, as LibreOffice's `-env:` wants it
/// (`file:///C:/Users/…` on Windows, `file:///tmp/…` elsewhere).
pub(crate) fn url(path: &Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    let p = p.trim_start_matches('/');
    format!("file:///{}", p.replace(' ', "%20"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_urls_are_file_urls() {
        assert_eq!(url(Path::new("/tmp/a b")), "file:///tmp/a%20b");
        assert_eq!(url(Path::new("C:\\Users\\me")), "file:///C:/Users/me");
    }

    #[test]
    fn office_comes_first_and_the_route_can_be_named() {
        use Renderer::*;
        assert_eq!(choose(None, true, true), Some(Office));
        assert_eq!(choose(None, false, true), Some(LibreOffice));
        assert_eq!(choose(None, false, false), None);
        assert_eq!(choose(Some("libreoffice"), true, true), Some(LibreOffice));
        assert_eq!(choose(Some("libreoffice"), true, false), None);
        assert_eq!(choose(Some("none"), true, true), None);
        assert_eq!(choose(Some("office"), false, false), Some(Office));
        if !cfg!(windows) {
            assert!(!office_blocking(), "Office is only asked for on Windows");
        }
    }

    #[test]
    fn the_same_bytes_have_the_same_key() {
        assert_eq!(key(b"abc"), key(b"abc"));
        assert_ne!(key(b"abc"), key(b"abd"));
    }

    /// With LibreOffice here, the memo renders as a real PDF.
    #[test]
    fn renders_the_memo_when_libreoffice_is_here() {
        if soffice().is_none() {
            return;
        }
        let docx = include_bytes!("../assets/docs/samples/q3-budget-memo.docx");
        let pdf = to_pdf("memo.docx", docx).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }
}
