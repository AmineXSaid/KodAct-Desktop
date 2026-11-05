//! Word, PowerPoint and Excel files as they print: LibreOffice turns them
//! into a PDF, which `office.rs` draws page by page, so the side panel shows
//! a document's layout, fonts and pictures, and a deck's slides as designed.
//!
//! LibreOffice is used when it is on this computer (`KODACT_SOFFICE`, PATH,
//! or where its installer puts it); without it the panel shows the file's
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
    let program = soffice().ok_or("LibreOffice is not installed")?;
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
fn url(path: &Path) -> String {
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
