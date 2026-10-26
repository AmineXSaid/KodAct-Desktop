//! Forge's own plugin (`assets/forge-plugin`), given to every ForgeCLI
//! session with `--plugin-dir`: the Diagram Design skill (MIT,
//! cathrynlavery/diagram-design, in Forge's skin) and the office-documents
//! skill. It is built into the app and unpacked once per version into the
//! config folder, `plugin/<hash>/forge/`, so a release needs no files beside it.

use std::path::PathBuf;
use std::sync::OnceLock;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/plugin_files.rs"));
}

/// The unpacked plugin folder, unpacking it on first use; None if the
/// config folder can't be written (Forge then runs without it).
pub fn dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = crate::profile::dir()?.join("plugin");
        // ForgeCLI names a plugin's skills after its folder: `forge:diagram-design`.
        let version = root.join(embedded::HASH);
        let dir = version.join("forge");
        let done = version.join(".unpacked");
        if !done.exists() {
            for (rel, bytes) in embedded::FILES {
                let p = dir.join(rel);
                std::fs::create_dir_all(p.parent()?).ok()?;
                std::fs::write(&p, bytes).ok()?;
            }
            std::fs::write(&done, embedded::HASH).ok()?;
            // Older versions' folders go.
            for e in std::fs::read_dir(&root).ok()?.flatten() {
                if e.path() != version {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        Some(dir)
    })
    .clone()
}

/// The plugin's files, `(path inside the plugin, bytes)`, for a remote copy.
pub fn files() -> &'static [(&'static str, &'static [u8])] {
    embedded::FILES
}

/// A content hash of the plugin: a new plugin unpacks to a new folder.
pub fn hash() -> &'static str {
    embedded::HASH
}

/// The skills the plugin brings, as ForgeCLI names them (`forge:<name>`),
/// with their descriptions.
pub fn skills() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = embedded::FILES
        .iter()
        .filter_map(|(path, bytes)| {
            let name = path.strip_prefix("skills/")?.strip_suffix("/SKILL.md")?;
            let doc = crate::forge_files::Doc::parse(&String::from_utf8_lossy(bytes));
            Some((
                format!("forge:{name}"),
                doc.get("description").unwrap_or_default(),
            ))
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::embedded;

    #[test]
    fn the_plugin_lists_its_skills() {
        let names: Vec<String> = super::skills().into_iter().map(|(n, _)| n).collect();
        assert!(
            names.contains(&"forge:diagram-design".to_string()),
            "{names:?}"
        );
        assert!(super::skills().iter().all(|(_, d)| !d.is_empty()));
    }

    #[test]
    fn the_plugin_teaches_with_the_expert_skill() {
        let skills = super::skills();
        let (_, description) = skills
            .iter()
            .find(|(n, _)| n == "forge:expert")
            .expect("forge:expert is bundled");
        assert!(description.contains("Teach the user"), "{description}");
        let names: Vec<&str> = embedded::FILES.iter().map(|(p, _)| *p).collect();
        for f in ["methods", "shapes", "levels"] {
            assert!(
                names.contains(&format!("skills/expert/reference/{f}.md").as_str()),
                "{f}"
            );
        }
        // Expert is no longer an output style.
        assert!(!names.iter().any(|n| n.starts_with("output-styles/")));
    }

    #[test]
    fn the_plugin_carries_its_manifest_and_diagram_design() {
        let names: Vec<&str> = embedded::FILES.iter().map(|(p, _)| *p).collect();
        assert!(names.contains(&".claude-plugin/plugin.json"));
        assert!(names.contains(&"skills/diagram-design/SKILL.md"));
        assert!(names.contains(&"skills/diagram-design/LICENSE"));
        assert_eq!(embedded::HASH.len(), 16);
    }
}
