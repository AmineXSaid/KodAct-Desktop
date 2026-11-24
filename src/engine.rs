//! The engine: the KodAct CLI each chat runs. The installer puts one beside
//! the app; Settings → About can replace it with another `kodact.exe` (a
//! newer release) without reinstalling the app. That copy lives in the user's
//! config folder (no administrator rights), and every new chat uses it;
//! "Use the bundled engine" goes back.
//!
//! Each replacement is kept under its own name and `engine/current` names
//! the one in use: on Windows a running chat holds its `kodact.exe` open, so
//! the file it runs can't be overwritten while it works. Older copies are
//! removed when they are free.

use std::path::{Path, PathBuf};

/// The folder the replacement engines are kept in.
pub fn dir() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("engine"))
}

/// The engine you put in, if there is one and it is still there.
pub fn user_engine() -> Option<PathBuf> {
    let dir = dir()?;
    let name = std::fs::read_to_string(dir.join("current")).ok()?;
    let p = dir.join(name.trim());
    p.is_file().then_some(p)
}

/// What `path --version` says (`kodact 0.1.0 (KodAct)`), if it is KodAct CLI.
pub fn version(path: &Path) -> Result<String, String> {
    let mut cmd = std::process::Command::new(path);
    cmd.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("it didn't start: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || !text.to_ascii_lowercase().starts_with("kodact") {
        return Err("it isn't KodAct CLI (its --version doesn't name KodAct)".into());
    }
    Ok(text)
}

/// Put `picked` in as the engine: checked, copied under a name of its own,
/// then made the current one. Returns its version.
pub fn install(picked: &Path) -> Result<String, String> {
    let v = version(picked)?;
    let dir = dir().ok_or("KodAct has no config folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("kodact-{stamp}{}", if cfg!(windows) { ".exe" } else { "" });
    let to = dir.join(&name);
    let part = dir.join(format!("{name}.part"));
    std::fs::copy(picked, &part).map_err(|e| format!("couldn't copy it: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755));
    }
    std::fs::rename(&part, &to).map_err(|e| e.to_string())?;
    // The copy runs as the original did before it is named current.
    version(&to)?;
    std::fs::write(dir.join("current"), &name).map_err(|e| e.to_string())?;
    sweep(&dir, &name);
    Ok(v)
}

/// Go back to the engine the installer put beside the app.
pub fn reset() -> Result<(), String> {
    let dir = dir().ok_or("KodAct has no config folder")?;
    match std::fs::remove_file(dir.join("current")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    sweep(&dir, "");
    Ok(())
}

/// Remove the engines no longer current (one a chat still runs stays until
/// next time).
fn sweep(dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n.starts_with("kodact-") && n != keep {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn something_that_is_not_kodact_is_refused() {
        // `true` exits 0 and prints nothing: not KodAct CLI.
        if cfg!(unix) {
            assert!(version(Path::new("/bin/true")).is_err());
        }
        assert!(version(Path::new("/no/such/kodact")).is_err());
    }
}
