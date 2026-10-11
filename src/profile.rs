//! The person's profile, kept between launches: the name Forge calls
//! them, and their avatar. A plain text file in the platform's config folder
//! (`FORGE_CONFIG_DIR` overrides it, for tests and portable installs).

use std::path::PathBuf;

const MAX_NAME: usize = 40;

pub(crate) fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FORGE_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Forge"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Forge"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("forge"))
    }
}

/// A name as typed: trimmed, one line, at most 40 characters; None if empty.
pub fn clean(name: &str) -> Option<String> {
    let n: String = name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    (!n.trim().is_empty()).then(|| n.trim().to_string())
}

pub fn load_name() -> Option<String> {
    let text = std::fs::read_to_string(dir()?.join("name.txt")).ok()?;
    clean(&text)
}

pub fn save_name(name: &str) -> std::io::Result<()> {
    let dir = dir().ok_or_else(|| std::io::Error::other("no config folder"))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("name.txt"), name)
}

/// The avatar chosen (or given at random), if one was saved and still exists.
pub fn load_avatar() -> Option<String> {
    let name = std::fs::read_to_string(dir()?.join("avatar.txt")).ok()?;
    let name = name.trim();
    crate::avatars::is_known(name).then(|| name.to_string())
}

pub fn save_avatar(name: &str) -> std::io::Result<()> {
    let dir = dir().ok_or_else(|| std::io::Error::other("no config folder"))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("avatar.txt"), name)
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean("  Amine \n").as_deref(), Some("Amine"));
        assert_eq!(clean("   "), None);
        assert_eq!(clean(&"x".repeat(80)).map(|n| n.len()), Some(40));
    }
}
