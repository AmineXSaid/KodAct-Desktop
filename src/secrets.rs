//! Where endpoint keys live: Windows Credential Manager on Windows (per user,
//! never in a file), elsewhere a `secrets.json` in Forge's config folder that
//! only you can read (for development on Linux and macOS). A profile names
//! its key as `${secret:<key>}`; the key itself is never in endpoints.json.

/// The key a profile's token is filed under, as Forge for VS Code names it.
pub fn key_for(profile: &str) -> String {
    format!("forge.endpoint.{profile}.token")
}

/// The reference a profile stores.
pub fn reference(key: &str) -> String {
    format!("${{secret:{key}}}")
}

#[cfg(windows)]
mod store {
    const SERVICE: &str = "Forge";

    fn entry(key: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, key).map_err(|e| format!("Credential Manager: {e}"))
    }

    pub fn get(key: &str) -> Option<String> {
        entry(key).ok()?.get_password().ok()
    }

    pub fn set(key: &str, value: &str) -> Result<(), String> {
        entry(key)?
            .set_password(value)
            .map_err(|e| format!("Credential Manager: {e}"))
    }

    pub fn delete(key: &str) {
        if let Ok(e) = entry(key) {
            let _ = e.delete_credential();
        }
    }
}

#[cfg(not(windows))]
mod store {
    use std::collections::BTreeMap;

    fn path() -> Option<std::path::PathBuf> {
        crate::profile::dir().map(|d| d.join("secrets.json"))
    }

    fn read() -> BTreeMap<String, String> {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn write(map: &BTreeMap<String, String>) -> Result<(), String> {
        let p = path().ok_or("no config folder")?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, serde_json::to_string_pretty(map).unwrap_or_default())
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn get(key: &str) -> Option<String> {
        read().remove(key)
    }

    pub fn set(key: &str, value: &str) -> Result<(), String> {
        let mut m = read();
        m.insert(key.to_string(), value.to_string());
        write(&m)
    }

    pub fn delete(key: &str) {
        let mut m = read();
        if m.remove(key).is_some() {
            let _ = write(&m);
        }
    }
}

pub use store::{delete, get, set};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_name_the_key_never_the_value() {
        assert_eq!(key_for("kgpt"), "forge.endpoint.kgpt.token");
        assert_eq!(
            reference(&key_for("kgpt")),
            "${secret:forge.endpoint.kgpt.token}"
        );
    }
}
