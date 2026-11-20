//! Starting points for a new project (New project › Start fresh): each a
//! description and the instructions its `KODACT.md` starts with, so the
//! first chat already knows what kind of work this is. Blank has none.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Template {
    pub key: &'static str,
    pub label: &'static str,
    /// The project's description on its card.
    pub description: &'static str,
    /// Its KODACT.md, `{name}` for the project's name; empty for none.
    pub instructions: &'static str,
    /// Start it as a git repository.
    pub git: bool,
}

pub const ALL: [Template; 5] = [
    Template {
        key: "blank",
        label: "Blank",
        description: "",
        instructions: "",
        git: false,
    },
    Template {
        key: "code",
        label: "Code",
        description: "A code project",
        instructions: "# {name}\n\nA code project.\n\n- Read the code around a change before making it, and match its style.\n- Keep each change small and say why it is needed.\n- Run the tests after a change and report what they showed.\n- Ask before adding a dependency or changing the build.\n",
        git: true,
    },
    Template {
        key: "writing",
        label: "Writing",
        description: "Writing: drafts and edits",
        instructions: "# {name}\n\nA writing project.\n\n- Keep my voice. Suggest edits and explain them; rewrite a whole section only when I ask.\n- Plain, active sentences; cut words that don't earn their place.\n- Drafts live in drafts/ as Markdown, one file per piece.\n- When I ask for feedback, start with what works, then the three changes that matter most.\n",
        git: false,
    },
    Template {
        key: "research",
        label: "Research",
        description: "Research: sources and notes",
        instructions: "# {name}\n\nA research project.\n\n- Back every claim with a source, and keep sources in sources.md with a line on each.\n- Mark guesses as guesses, and say how sure you are.\n- Notes live in notes/, one file per topic.\n- End a summary with the open questions.\n",
        git: false,
    },
    Template {
        key: "data",
        label: "Data",
        description: "Data analysis",
        instructions: "# {name}\n\nA data analysis project.\n\n- Files in data/ are read-only: write results to out/.\n- Show the code behind every number, and check the data before drawing conclusions (missing values, duplicates, units).\n- Prefer one clear chart that answers the question over many.\n- Say what the data can't tell us.\n",
        git: false,
    },
];

pub fn by_key(key: &str) -> Template {
    ALL.iter().copied().find(|t| t.key == key).unwrap_or(ALL[0])
}

/// The folder new projects are made in: `KodAct Projects` in your home, or
/// Forge's `Forge Projects` while only that one exists. Looked up once (the
/// New project sheet shows it as it renders).
pub fn projects_home() -> Option<std::path::PathBuf> {
    static HOME: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        crate::rules::home_dir()
            .map(|h| crate::names::renamed(&h, "KodAct Projects", "Forge Projects"))
    })
    .clone()
}

/// A folder name from what you typed: no separators or characters a
/// filesystem refuses, no leading dots.
pub fn folder_name(name: &str) -> Option<String> {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "/\\<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(80)
        .collect();
    let clean = clean.trim().trim_start_matches('.').trim().to_string();
    (!clean.is_empty() && clean.chars().any(|c| c != '_')).then_some(clean)
}

/// Make a new project's folder from a template (blocking). Fails when the
/// folder exists and has something in it.
pub fn create(dir: &std::path::Path, name: &str, t: Template) -> Result<(), String> {
    if dir.exists() && std::fs::read_dir(dir).map_or(true, |mut d| d.next().is_some()) {
        return Err(format!("{} already has files in it", dir.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if !t.instructions.is_empty() {
        let text = t.instructions.replace("{name}", name.trim());
        std::fs::write(dir.join("KODACT.md"), text).map_err(|e| e.to_string())?;
    }
    if t.git {
        // Best effort: without git it is still a folder to work in.
        let _ = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    Ok(())
}

/// The folder a clone of `url` goes in, under `home`: the repository's name.
pub fn clone_target(home: &std::path::Path, url: &str) -> Option<std::path::PathBuf> {
    let name = url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit(['/', ':'])
        .next()?;
    folder_name(name).map(|n| home.join(n))
}

/// Normalize a repository address or shorthand (`owner/repo`, `group/sub/repo`)
/// into a cloneable URL for GitHub, GitLab, or a custom self-hosted Git host.
pub fn normalize_git_clone_url(
    kind: crate::git_status::ForgeKind,
    custom_host: Option<&str>,
    raw_input: &str,
) -> Option<String> {
    let raw = raw_input.trim();
    if raw.is_empty() {
        return None;
    }
    let is_full_ssh = raw.starts_with("git@")
        || (!raw.contains("://")
            && raw
                .split_once(':')
                .is_some_and(|(lhs, _)| !lhs.contains('/') && lhs.contains('@')));
    if raw.contains("://") || is_full_ssh {
        return Some(raw.to_string());
    }

    let slug = raw.trim_start_matches('/').trim_end_matches('/');
    if !slug.contains('/') {
        return None;
    }

    let clean_host = custom_host
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
        });

    let first_seg = slug.split('/').next().unwrap_or("");
    if clean_host.is_none() && first_seg.contains('.') {
        let with_scheme = format!("https://{slug}");
        return Some(if with_scheme.ends_with(".git") {
            with_scheme
        } else {
            format!("{with_scheme}.git")
        });
    }

    let host = match kind {
        crate::git_status::ForgeKind::GitHub => clean_host.unwrap_or("github.com"),
        crate::git_status::ForgeKind::GitLab => clean_host.unwrap_or("gitlab.com"),
        crate::git_status::ForgeKind::Custom => clean_host?,
    };
    let out = format!("https://{host}/{slug}");
    Some(if out.ends_with(".git") {
        out
    } else {
        format!("{out}.git")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_starts_from_its_template() {
        let dir = std::env::temp_dir().join(format!("forge-tpl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create(&dir, "Launch notes", by_key("writing")).unwrap();
        let text = std::fs::read_to_string(dir.join("KODACT.md")).unwrap();
        assert!(text.starts_with("# Launch notes\n\nA writing project."));
        assert!(
            create(&dir, "Again", by_key("blank")).is_err(),
            "a folder with files isn't reused"
        );
        let blank = dir.join("blank");
        create(&blank, "x", by_key("blank")).unwrap();
        assert!(!blank.join("KODACT.md").exists());
        assert_eq!(by_key("nope").key, "blank");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_and_clone_folders_are_safe() {
        assert_eq!(
            folder_name("  My: project/1 ").as_deref(),
            Some("My_ project_1")
        );
        assert_eq!(folder_name("..."), None);
        let home = std::path::Path::new("/h");
        assert_eq!(
            clone_target(home, "https://github.com/a/kodact-desktop.git"),
            Some(home.join("kodact-desktop"))
        );
        assert_eq!(
            clone_target(home, "git@github.com:a/tool"),
            Some(home.join("tool"))
        );
        assert_eq!(
            clone_target(home, "https://gitlab.company.com/team/sub/repo.git"),
            Some(home.join("repo"))
        );
    }

    #[test]
    fn normalizes_clone_urls_for_github_gitlab_and_custom_hosts() {
        use crate::git_status::ForgeKind;
        assert_eq!(
            normalize_git_clone_url(ForgeKind::GitHub, None, "owner/repo").as_deref(),
            Some("https://github.com/owner/repo.git")
        );
        assert_eq!(
            normalize_git_clone_url(ForgeKind::GitLab, None, "team/sub/service").as_deref(),
            Some("https://gitlab.com/team/sub/service.git")
        );
        assert_eq!(
            normalize_git_clone_url(
                ForgeKind::GitLab,
                Some("https://gitlab.company.com/"),
                "team/sub/service"
            )
            .as_deref(),
            Some("https://gitlab.company.com/team/sub/service.git")
        );
        assert_eq!(
            normalize_git_clone_url(ForgeKind::Custom, Some("git.acme.internal"), "org/app")
                .as_deref(),
            Some("https://git.acme.internal/org/app.git")
        );
        assert_eq!(
            normalize_git_clone_url(ForgeKind::Custom, None, "org/app"),
            None
        );
        assert_eq!(
            normalize_git_clone_url(ForgeKind::Custom, None, "ssh://git@git.acme.internal/org/app.git")
                .as_deref(),
            Some("ssh://git@git.acme.internal/org/app.git")
        );
    }
}
