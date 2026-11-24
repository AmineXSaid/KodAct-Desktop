//! What installing changes outside the install folder, as the NSIS installer did, under the same
//! keys, so a new setup updates an install the old one made and either uninstaller removes it:
//!
//! - `HKCU\Software\KodAct`: `InstallDir`, `AddedToPath`, `SetNudges`;
//! - `HKCU\...\Uninstall\KodAct`: the entry in Apps & features;
//! - the Start menu shortcut, and the desktop one if chosen;
//! - `bin` on the user's PATH, if chosen; `KODACT_NUDGES=all`, if chosen and not already set.
//!
//! Registry values go through `reg.exe` and shortcuts through PowerShell's `WScript.Shell`, both
//! part of every Windows. Off Windows (a development run) each step only says what it would do.

use std::path::Path;

pub const UNINST_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\KodAct";
pub const APP_KEY: &str = r"HKCU\Software\KodAct";
pub const ENV_KEY: &str = r"HKCU\Environment";

/// A command with no console window.
fn quiet(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    if !cfg!(windows) {
        eprintln!("(setup, not on Windows) {program} {}", args.join(" "));
        return Ok(String::new());
    }
    let out = quiet(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

pub fn reg_set(key: &str, name: &str, kind: &str, value: &str) -> Result<(), String> {
    run(
        "reg",
        &["add", key, "/v", name, "/t", kind, "/d", value, "/f"],
    )
    .map(drop)
}

/// A value, when it is there.
pub fn reg_get(key: &str, name: &str) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let out = run("reg", &["query", key, "/v", name]).ok()?;
    // `    Path    REG_EXPAND_SZ    C:\a;C:\b`
    out.lines().find_map(|l| {
        let l = l.trim_start();
        let rest = l.strip_prefix(name)?;
        let rest = rest.trim_start();
        let kind_end = rest.find(char::is_whitespace)?;
        Some(rest[kind_end..].trim().to_string())
    })
}

fn reg_delete_value(key: &str, name: &str) {
    let _ = run("reg", &["delete", key, "/v", name, "/f"]);
}

fn reg_delete_key(key: &str) {
    let _ = run("reg", &["delete", key, "/f"]);
}

/// Tell running programs (Explorer, new terminals) the environment changed.
fn environment_changed() {
    #[cfg(windows)]
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn SendMessageTimeoutW(
                hwnd: isize,
                msg: u32,
                wparam: usize,
                lparam: *const u16,
                flags: u32,
                timeout: u32,
                result: *mut usize,
            ) -> isize;
        }
        let what: Vec<u16> = "Environment\0".encode_utf16().collect();
        let mut r = 0usize;
        // HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG
        SendMessageTimeoutW(0xffff, 0x001a, 0, what.as_ptr(), 0x0002, 2000, &mut r);
    }
}

/// `dir` added to the user's PATH, unless it is there already. Returns whether it was added.
pub fn add_to_path(dir: &Path) -> Result<bool, String> {
    let dir = dir.display().to_string();
    let now = reg_get(ENV_KEY, "Path").unwrap_or_default();
    if path_has(&now, &dir) {
        return Ok(false);
    }
    let new = if now.trim().is_empty() {
        dir.clone()
    } else {
        format!("{};{dir}", now.trim_end_matches(';'))
    };
    reg_set(ENV_KEY, "Path", "REG_EXPAND_SZ", &new)?;
    environment_changed();
    Ok(true)
}

/// Every `dir` entry out of the user's PATH; the rest as it was.
pub fn remove_from_path(dir: &str) {
    let Some(now) = reg_get(ENV_KEY, "Path") else {
        return;
    };
    let kept = path_without(&now, dir);
    if kept != now {
        let _ = reg_set(ENV_KEY, "Path", "REG_EXPAND_SZ", &kept);
        environment_changed();
    }
}

fn same_dir(a: &str, b: &str) -> bool {
    a.trim()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(b.trim().trim_end_matches(['\\', '/']))
}

pub fn path_has(path: &str, dir: &str) -> bool {
    path.split(';').any(|p| same_dir(p, dir))
}

pub fn path_without(path: &str, dir: &str) -> String {
    path.split(';')
        .filter(|p| !same_dir(p, dir))
        .collect::<Vec<_>>()
        .join(";")
}

/// `KODACT_NUDGES=all` for the user, unless a value is set already. Returns whether it was set.
pub fn turn_on_nudges() -> Result<bool, String> {
    if reg_get(ENV_KEY, "KODACT_NUDGES").is_some_and(|v| !v.trim().is_empty()) {
        return Ok(false);
    }
    reg_set(ENV_KEY, "KODACT_NUDGES", "REG_SZ", "all")?;
    environment_changed();
    Ok(true)
}

/// A PowerShell string literal.
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The Start menu shortcut, and the desktop one when `desktop`; both point at KodAct.exe.
pub fn shortcuts(install: &Path, desktop: bool) -> Result<(), String> {
    let exe = install.join("KodAct.exe").display().to_string();
    let mut places = vec!["Programs"];
    if desktop {
        places.push("Desktop");
    }
    let script = places
        .iter()
        .map(|p| {
            format!(
                "$l=$s.CreateShortcut([IO.Path]::Combine([Environment]::GetFolderPath('{p}'),'KodAct.lnk'));\
                 $l.TargetPath={e};$l.IconLocation={i};$l.WorkingDirectory={w};$l.Save();",
                e = ps_quote(&exe),
                i = ps_quote(&format!("{exe},0")),
                w = ps_quote(&install.display().to_string()),
            )
        })
        .collect::<String>();
    run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!("$s=New-Object -ComObject WScript.Shell;{script}"),
        ],
    )
    .map(drop)
}

/// Where Windows keeps the taskbar's pinned shortcuts for this user.
fn taskbar_pins() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|a| {
        std::path::PathBuf::from(a)
            .join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar")
    })
}

/// KodAct pinned to the taskbar, if Windows lets setup do it. Since Windows 10 1809 it doesn't
/// (pinning is the person's to do), so this tries the Start menu shortcut's "pin to taskbar"
/// verb and then looks for the pin: true only when it is there.
pub fn pin_to_taskbar() -> bool {
    if !cfg!(windows) {
        eprintln!("(setup, not on Windows) pin KodAct to the taskbar");
        return false;
    }
    let _ = run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "$d=[Environment]::GetFolderPath('Programs');$i=(New-Object -ComObject Shell.Application).Namespace($d).ParseName('KodAct.lnk');\
             if($i){$v=$i.Verbs()|Where-Object{$_.Name.Replace('&','') -match 'taskbar|barre des t|Taskleiste|barra de tareas'};if($v){$v|ForEach-Object{$_.DoIt()}}else{$i.InvokeVerb('taskbarpin')}}",
        ],
    );
    taskbar_pins().is_some_and(|d| {
        std::fs::read_dir(d).is_ok_and(|rd| {
            rd.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("KodAct.lnk")
            })
        })
    })
}

fn remove_shortcuts() {
    if let Some(pin) = taskbar_pins().map(|d| d.join("KodAct.lnk")) {
        let _ = std::fs::remove_file(pin);
    }
    let _ = run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "foreach($p in 'Programs','Desktop'){$f=[IO.Path]::Combine([Environment]::GetFolderPath($p),'KodAct.lnk');if(Test-Path $f){Remove-Item -Force $f}}",
        ],
    );
}

/// The entry in Apps & features, and where the install is.
pub fn register(install: &Path, version: &str, kilobytes: u64) -> Result<(), String> {
    let dir = install.display().to_string();
    let uninstaller = install.join("Uninstall.exe").display().to_string();
    reg_set(APP_KEY, "InstallDir", "REG_SZ", &dir)?;
    for (name, value) in [
        ("DisplayName", "KodAct".to_string()),
        ("DisplayVersion", version.to_string()),
        ("Publisher", "KodAct".to_string()),
        ("DisplayIcon", format!("{}\\KodAct.exe,0", dir)),
        ("InstallLocation", dir.clone()),
        ("UninstallString", format!("\"{uninstaller}\" --uninstall")),
        (
            "QuietUninstallString",
            format!("\"{uninstaller}\" --uninstall /S"),
        ),
    ] {
        reg_set(UNINST_KEY, name, "REG_SZ", &value)?;
    }
    for (name, value) in [
        ("NoModify", 1),
        ("NoRepair", 1),
        ("EstimatedSize", kilobytes),
    ] {
        reg_set(UNINST_KEY, name, "REG_DWORD", &value.to_string())?;
    }
    Ok(())
}

/// Where KodAct is installed, if it is.
pub fn installed_dir() -> Option<String> {
    reg_get(APP_KEY, "InstallDir").filter(|d| !d.trim().is_empty())
}

/// Everything installing changed outside the folder, put back: PATH, the nudges setting it
/// made, shortcuts, the Apps & features entry.
pub fn unregister() {
    if let Some(dir) = reg_get(APP_KEY, "AddedToPath") {
        remove_from_path(&dir);
    }
    if reg_get(APP_KEY, "SetNudges").as_deref() == Some("1") {
        reg_delete_value(ENV_KEY, "KODACT_NUDGES");
        environment_changed();
    }
    remove_shortcuts();
    reg_delete_key(UNINST_KEY);
    reg_delete_key(APP_KEY);
}

/// Start KodAct, apart from the setup.
pub fn launch(install: &Path) {
    let mut c = std::process::Command::new(install.join("KodAct.exe"));
    c.current_dir(install);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_0008 | 0x0000_0200); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    }
    let _ = c.spawn();
}

/// Remove `file` once this process has ended (the uninstaller can't delete itself while it runs).
pub fn delete_after_exit(file: &Path) {
    if !cfg!(windows) {
        return;
    }
    let f = file.display().to_string();
    let _ = quiet("cmd")
        .args([
            "/C",
            &format!("ping -n 3 127.0.0.1 >nul & del /f /q \"{f}\""),
        ])
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entries_are_added_once_and_removed_alone() {
        let p = r"C:\Windows;C:\Users\me\AppData\Local\Programs\KodAct\bin\;C:\tools";
        assert!(path_has(
            p,
            r"c:\users\me\appdata\local\programs\kodact\bin"
        ));
        assert_eq!(
            path_without(p, r"C:\Users\me\AppData\Local\Programs\KodAct\bin"),
            r"C:\Windows;C:\tools"
        );
        assert!(!path_has(r"C:\Windows", r"C:\Win"));
    }

    #[test]
    fn a_powershell_string_keeps_its_quotes() {
        assert_eq!(
            ps_quote(r"C:\Users\O'Neil\KodAct"),
            r"'C:\Users\O''Neil\KodAct'"
        );
    }
}
