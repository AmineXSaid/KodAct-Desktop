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

/// The credential helper a token clone hands git for that one command. Git
/// runs a helper that starts with `!` through its own shell (Git for Windows
/// ships one), passing `get`, `store` or `erase`; this one answers `get` from
/// the environment and ignores the rest, so the token is never stored.
const TOKEN_HELPER: &str = "!f() { test \"$1\" = get && printf 'username=%s\\npassword=%s\\n' \"${KODACT_GIT_USER:-oauth2}\" \"$KODACT_GIT_TOKEN\"; }; f";

/// `git clone` of `url` into `dir`, with no prompts. With a `token`, the
/// user's own credential helpers are switched off for this command (so a
/// saved credential isn't tried first and the token isn't saved after) and
/// `TOKEN_HELPER` reads the token from `KODACT_GIT_TOKEN`: it is never in the
/// arguments, a file, or `.git/config`.
pub fn clone_command(
    url: &str,
    dir: &std::path::Path,
    token: Option<&str>,
) -> std::process::Command {
    let mut c = std::process::Command::new("git");
    if let Some(token) = token {
        c.args(["-c", "credential.helper=", "-c"])
            .arg(format!("credential.helper={TOKEN_HELPER}"))
            .env("KODACT_GIT_USER", "oauth2")
            .env("KODACT_GIT_TOKEN", token);
    }
    c.args(["clone", "--", url])
        .arg(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
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
    fn a_token_clone_keeps_the_token_out_of_the_arguments() {
        let dir = std::path::Path::new("/tmp/x");
        let c = clone_command("https://gitlab.com/a/b.git", dir, Some("glpat-SECRET"));
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into()).collect();
        assert_eq!(
            args[..3],
            ["-c", "credential.helper=", "-c"],
            "own helpers off first"
        );
        assert!(args[3].starts_with("credential.helper=!"));
        assert_eq!(
            args[4..],
            ["clone", "--", "https://gitlab.com/a/b.git", "/tmp/x"]
        );
        assert!(args.iter().all(|a| !a.contains("SECRET")), "{args:?}");
        let env: Vec<_> = c.get_envs().collect();
        assert!(env.contains(&("KODACT_GIT_TOKEN".as_ref(), Some("glpat-SECRET".as_ref()))));

        let plain = clone_command("https://github.com/a/b", dir, None);
        let args: Vec<String> = plain
            .get_args()
            .map(|a| a.to_string_lossy().into())
            .collect();
        assert_eq!(args, ["clone", "--", "https://github.com/a/b", "/tmp/x"]);
        assert!(plain.get_envs().all(|(k, _)| k != "KODACT_GIT_TOKEN"));
    }

    /// Against a real server that wants `oauth2` and `KODACT_TEST_CLONE_TOKEN`
    /// at `KODACT_TEST_CLONE_URL`: the clone succeeds with the token, fails
    /// without it, and never asks or feeds the user's own credential helper
    /// (point `GIT_CONFIG_GLOBAL` at a config whose helper logs to
    /// `KODACT_TEST_HELPER_LOG`).
    #[test]
    #[ignore = "needs a git server; see the doc comment"]
    fn real_token_clone() {
        let url = std::env::var("KODACT_TEST_CLONE_URL").expect("KODACT_TEST_CLONE_URL");
        let token = std::env::var("KODACT_TEST_CLONE_TOKEN").expect("KODACT_TEST_CLONE_TOKEN");
        let log = std::env::var("KODACT_TEST_HELPER_LOG").ok();
        let base = std::env::temp_dir().join(format!("kodact-clone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let with = clone_command(&url, &base.join("with"), Some(&token))
            .output()
            .unwrap();
        assert!(
            with.status.success(),
            "{}",
            String::from_utf8_lossy(&with.stderr)
        );
        let config = std::fs::read_to_string(base.join("with/.git/config")).unwrap();
        assert!(
            !config.contains(&token) && !config.contains("credential"),
            "{config}"
        );
        let wrong = clone_command(&url, &base.join("wrong"), Some("nope"))
            .output()
            .unwrap();
        assert!(!wrong.status.success());
        if let Some(log) = log {
            let calls = std::fs::read_to_string(&log).unwrap_or_default();
            assert!(calls.is_empty(), "the user's helper was used: {calls}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn the_token_helper_answers_get_and_stores_nothing() {
        // Git runs `!helper` as `sh -c '<helper> "$@"' <helper> <action>`.
        let body = TOKEN_HELPER.strip_prefix('!').unwrap();
        let run = |action: &str| {
            let out = std::process::Command::new("sh")
                .args(["-c", &format!("{body} \"$@\""), "helper", action])
                .env("KODACT_GIT_TOKEN", "tok&en|^x")
                .env_remove("KODACT_GIT_USER")
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap()
        };
        assert_eq!(run("get"), "username=oauth2\npassword=tok&en|^x\n");
        assert_eq!(run("store"), "");
        assert_eq!(run("erase"), "");
    }

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
            normalize_git_clone_url(
                ForgeKind::Custom,
                None,
                "ssh://git@git.acme.internal/org/app.git"
            )
            .as_deref(),
            Some("ssh://git@git.acme.internal/org/app.git")
        );
    }
}
