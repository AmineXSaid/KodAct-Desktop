//! Settings profiles: named sets of ForgeCLI settings kept side by side
//! (`profiles/<name>.json` in Forge's config folder), one of them active.
//! Each chat's ForgeCLI gets the active one as `--settings <file>`, which it
//! layers above your settings, the project's and this PC's: what the profile
//! sets wins, and what it doesn't set falls back to them. A new profile
//! starts empty (`{}`). The choice is kept in `settings-profile.txt`.
//!
//! Plain file I/O; callers run it off the UI thread.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsProfile {
    pub name: String,
    pub path: PathBuf,
    /// The top-level keys it sets (`permissions`, `model`…).
    pub keys: Vec<String>,
    /// The file isn't a JSON object, so ForgeCLI would report it.
    pub problem: Option<String>,
}

fn root() -> Result<PathBuf, String> {
    crate::profile::dir().ok_or_else(|| "no config folder".to_string())
}

fn file_in(root: &Path, name: &str) -> PathBuf {
    root.join("profiles").join(format!("{name}.json"))
}

/// A profile's file, for `--settings` (no I/O).
pub fn path_of(name: &str) -> Option<PathBuf> {
    root().ok().map(|r| file_in(&r, name))
}

pub fn list() -> Vec<SettingsProfile> {
    root().map(|r| list_in(&r)).unwrap_or_default()
}

pub fn active() -> Option<String> {
    active_in(&root().ok()?)
}

pub fn set_active(name: Option<&str>) -> Result<(), String> {
    set_active_in(&root()?, name)
}

pub fn create(name: &str) -> Result<PathBuf, String> {
    create_in(&root()?, name)
}

pub fn delete(name: &str) -> Result<(), String> {
    delete_in(&root()?, name)
}

/// A fingerprint of the profiles folder, so a watch notices an edit.
pub fn stamp() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Ok(root) = root() {
        active_in(&root).hash(&mut h);
        if let Ok(rd) = std::fs::read_dir(root.join("profiles")) {
            let mut v: Vec<_> = rd
                .flatten()
                .map(|e| (e.path(), e.metadata().ok().and_then(|m| m.modified().ok())))
                .collect();
            v.sort();
            v.hash(&mut h);
        }
    }
    h.finish()
}

fn list_in(root: &Path) -> Vec<SettingsProfile> {
    let Ok(rd) = std::fs::read_dir(root.join("profiles")) else {
        return vec![];
    };
    let mut out: Vec<SettingsProfile> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            let (keys, problem) = match crate::forge_files::read_json(&p) {
                Ok(m) => (m.keys().cloned().collect(), None),
                Err(e) => (vec![], Some(e)),
            };
            Some(SettingsProfile {
                name,
                path: p,
                keys,
                problem,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// The active profile's name, if its file still exists.
fn active_in(root: &Path) -> Option<String> {
    let name = std::fs::read_to_string(root.join("settings-profile.txt")).ok()?;
    let name = name.trim().to_string();
    (!name.is_empty() && file_in(root, &name).is_file()).then_some(name)
}

fn set_active_in(root: &Path, name: Option<&str>) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let file = root.join("settings-profile.txt");
    match name {
        Some(n) => std::fs::write(file, n).map_err(|e| e.to_string()),
        None => match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        },
    }
}

/// Make an empty profile (`{}`): anything it doesn't set falls back.
fn create_in(root: &Path, name: &str) -> Result<PathBuf, String> {
    crate::forge_files::check_name(name, false)?;
    let path = file_in(root, name);
    if path.exists() {
        return Err(format!("A profile named {name} already exists."));
    }
    crate::forge_files::write_atomic(&path, "{}\n")?;
    Ok(path)
}

/// Delete a profile; if it was active, none is.
fn delete_in(root: &Path, name: &str) -> Result<(), String> {
    std::fs::remove_file(file_in(root, name)).map_err(|e| e.to_string())?;
    if active_in(root).is_none() {
        set_active_in(root, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_are_made_picked_and_deleted() {
        let d = std::env::temp_dir().join(format!("forge-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let p = create_in(&d, "client-work").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{}\n");
        assert!(create_in(&d, "client-work").is_err());
        assert!(create_in(&d, "bad name").is_err());
        set_active_in(&d, Some("client-work")).unwrap();
        assert_eq!(active_in(&d).as_deref(), Some("client-work"));
        std::fs::write(&p, r#"{"permissions": {"deny": ["Bash(rm *)"]}}"#).unwrap();
        assert_eq!(list_in(&d)[0].keys, ["permissions"]);
        delete_in(&d, "client-work").unwrap();
        assert_eq!(active_in(&d), None);
        assert!(list_in(&d).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
