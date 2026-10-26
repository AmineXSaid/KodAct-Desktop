//! A project's repository, live: its branch, how far it is ahead of or
//! behind its upstream, what has changed, and, when it is on GitHub, its
//! open pull requests (with their checks), issues and latest workflow run.
//! The project page's GitHub card shows it (`ui/project_github.rs`).
//!
//! - The remote comes from `.git/config` (a file read: no git needed).
//! - Status is `git status --porcelain=v2 --branch`.
//! - GitHub through `gh` when it is installed and signed in, else GitHub's
//!   REST API (with `GH_TOKEN` / `GITHUB_TOKEN` when set; public repos need
//!   none).
//!
//! Nothing asks for a password: git and gh run with prompts off, without a
//! console window. Everything here blocks: run it off the UI thread.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repo {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Files changed (staged or not), and files git doesn't track yet.
    pub changed: u32,
    pub untracked: u32,
    /// The origin's URL, and its GitHub owner and name when it is GitHub's.
    pub remote: Option<String>,
    pub github: Option<(String, String)>,
    /// What GitHub says; None when the repo isn't on GitHub.
    pub hub: Option<Result<Hub, String>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hub {
    pub prs: Vec<Pr>,
    pub issues: Vec<Issue>,
    pub run: Option<Run>,
    /// "gh" or "GitHub API".
    pub via: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    Passing,
    Failing,
    Pending,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub branch: String,
    pub url: String,
    pub draft: bool,
    pub checks: Option<Checks>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub name: String,
    pub branch: String,
    /// queued, in_progress, completed…; and when completed, how.
    pub status: String,
    pub conclusion: Option<String>,
    pub url: String,
}

impl Run {
    pub fn checks(&self) -> Checks {
        match (self.status.as_str(), self.conclusion.as_deref()) {
            ("completed", Some("success" | "neutral" | "skipped")) => Checks::Passing,
            ("completed", _) => Checks::Failing,
            _ => Checks::Pending,
        }
    }
}

fn command(program: &str, dir: &Path) -> Command {
    let mut c = Command::new(program);
    c.current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Run a program in `dir`; its output, or what went wrong in a line.
fn run(program: &str, dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = command(program, dir)
        .args(args)
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("{program} isn't installed"),
            _ => e.to_string(),
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("it failed")
            .trim()
            .to_string())
    }
}

/// The origin's URL in a `.git/config` (else the first remote's).
pub fn parse_remote(config: &str) -> Option<String> {
    let mut section = String::new();
    let mut first = None;
    for line in config.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            section = l.to_string();
        } else if let Some(url) = l
            .strip_prefix("url")
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix('='))
            .map(str::trim)
        {
            if section == "[remote \"origin\"]" {
                return Some(url.to_string());
            }
            if section.starts_with("[remote ") && first.is_none() {
                first = Some(url.to_string());
            }
        }
    }
    first
}

/// A GitHub remote's owner and name: `git@github.com:o/r.git`,
/// `https://github.com/o/r`, `ssh://git@github.com/o/r.git`.
pub fn github_repo(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.split_once("github.com/").map(|(_, r)| r))?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let (owner, name) = rest.split_once('/')?;
    (!owner.is_empty() && !name.is_empty() && !name.contains('/'))
        .then(|| (owner.to_string(), name.to_string()))
}

/// `git status --porcelain=v2 --branch` read into a repo's state.
pub fn parse_status(text: &str) -> Repo {
    let mut r = Repo::default();
    for line in text.lines() {
        if let Some(b) = line.strip_prefix("# branch.head ") {
            r.branch = b.to_string();
        } else if let Some(u) = line.strip_prefix("# branch.upstream ") {
            r.upstream = Some(u.to_string());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            for part in ab.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    r.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    r.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
            r.changed += 1;
        } else if line.starts_with("? ") {
            r.untracked += 1;
        }
    }
    r
}

/// A pull request's checks, from gh's `statusCheckRollup`.
fn rollup(v: &Value) -> Option<Checks> {
    let items = v.as_array().filter(|a| !a.is_empty())?;
    let mut pending = false;
    for c in items {
        let state = c["conclusion"]
            .as_str()
            .or_else(|| c["state"].as_str())
            .unwrap_or("")
            .to_uppercase();
        match state.as_str() {
            "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" => {
                return Some(Checks::Failing)
            }
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => {}
            _ => pending = true,
        }
    }
    Some(if pending {
        Checks::Pending
    } else {
        Checks::Passing
    })
}

fn gh_json(dir: &Path, args: &[&str]) -> Result<Value, String> {
    let out = run("gh", dir, args)?;
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

fn via_gh(dir: &Path, repo: &str) -> Result<Hub, String> {
    let prs = gh_json(
        dir,
        &[
            "pr",
            "list",
            "-R",
            repo,
            "--state",
            "open",
            "--limit",
            "8",
            "--json",
            "number,title,author,headRefName,isDraft,url,statusCheckRollup",
        ],
    )?;
    let issues = gh_json(
        dir,
        &[
            "issue",
            "list",
            "-R",
            repo,
            "--state",
            "open",
            "--limit",
            "8",
            "--json",
            "number,title,url",
        ],
    )?;
    let runs = gh_json(
        dir,
        &[
            "run",
            "list",
            "-R",
            repo,
            "--limit",
            "1",
            "--json",
            "name,headBranch,status,conclusion,url",
        ],
    )
    .unwrap_or(Value::Null);
    Ok(Hub {
        prs: prs
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| Pr {
                number: p["number"].as_u64().unwrap_or(0),
                title: p["title"].as_str().unwrap_or("").to_string(),
                author: p["author"]["login"].as_str().unwrap_or("").to_string(),
                branch: p["headRefName"].as_str().unwrap_or("").to_string(),
                url: p["url"].as_str().unwrap_or("").to_string(),
                draft: p["isDraft"].as_bool().unwrap_or(false),
                checks: rollup(&p["statusCheckRollup"]),
            })
            .collect(),
        issues: issues
            .as_array()
            .into_iter()
            .flatten()
            .map(|i| Issue {
                number: i["number"].as_u64().unwrap_or(0),
                title: i["title"].as_str().unwrap_or("").to_string(),
                url: i["url"].as_str().unwrap_or("").to_string(),
            })
            .collect(),
        run: runs.as_array().and_then(|a| a.first()).map(|r| Run {
            name: r["name"].as_str().unwrap_or("").to_string(),
            branch: r["headBranch"].as_str().unwrap_or("").to_string(),
            status: r["status"].as_str().unwrap_or("").to_lowercase(),
            conclusion: r["conclusion"]
                .as_str()
                .filter(|c| !c.is_empty())
                .map(str::to_lowercase),
            url: r["url"].as_str().unwrap_or("").to_string(),
        }),
        via: "gh",
    })
}

fn via_api(repo: &str) -> Result<Hub, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let token = std::env::var("GH_TOKEN")
        .or_else(|_| std::env::var("GITHUB_TOKEN"))
        .ok()
        .filter(|t| !t.trim().is_empty());
    let get = |path: &str| -> Result<Value, String> {
        let mut req = agent
            .get(&format!("https://api.github.com/repos/{repo}/{path}"))
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "Forge-Desktop");
        if let Some(t) = &token {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        match req.call() {
            Ok(r) => r.into_json().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(404, _)) => Err(
                "GitHub doesn't show this repository: it may be private. Sign in with gh (gh auth login) to see it.".into(),
            ),
            Err(ureq::Error::Status(403, _)) => {
                Err("GitHub's limit for unsigned requests was reached. Sign in with gh to see more.".into())
            }
            Err(e) => Err(e.to_string()),
        }
    };
    let prs = get("pulls?state=open&per_page=8")?;
    let issues = get("issues?state=open&per_page=12")?;
    let runs = get("actions/runs?per_page=1").unwrap_or(Value::Null);
    Ok(Hub {
        prs: prs
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| Pr {
                number: p["number"].as_u64().unwrap_or(0),
                title: p["title"].as_str().unwrap_or("").to_string(),
                author: p["user"]["login"].as_str().unwrap_or("").to_string(),
                branch: p["head"]["ref"].as_str().unwrap_or("").to_string(),
                url: p["html_url"].as_str().unwrap_or("").to_string(),
                draft: p["draft"].as_bool().unwrap_or(false),
                checks: None,
            })
            .collect(),
        // The issues endpoint lists pull requests too.
        issues: issues
            .as_array()
            .into_iter()
            .flatten()
            .filter(|i| i.get("pull_request").is_none())
            .take(8)
            .map(|i| Issue {
                number: i["number"].as_u64().unwrap_or(0),
                title: i["title"].as_str().unwrap_or("").to_string(),
                url: i["html_url"].as_str().unwrap_or("").to_string(),
            })
            .collect(),
        run: runs["workflow_runs"]
            .as_array()
            .and_then(|a| a.first())
            .map(|r| Run {
                name: r["name"].as_str().unwrap_or("").to_string(),
                branch: r["head_branch"].as_str().unwrap_or("").to_string(),
                status: r["status"].as_str().unwrap_or("").to_string(),
                conclusion: r["conclusion"].as_str().map(str::to_string),
                url: r["html_url"].as_str().unwrap_or("").to_string(),
            }),
        via: "GitHub API",
    })
}

/// The project's repository as it is now; None when it isn't one.
pub fn read(project: &Path) -> Option<Repo> {
    let config = std::fs::read_to_string(project.join(".git").join("config")).ok()?;
    let mut repo = run("git", project, &["status", "--porcelain=v2", "--branch"])
        .map(|s| parse_status(&s))
        .unwrap_or_else(|_| Repo {
            branch: crate::state::git_branch(project),
            ..Repo::default()
        });
    repo.remote = parse_remote(&config);
    repo.github = repo.remote.as_deref().and_then(github_repo);
    if let Some((owner, name)) = &repo.github {
        let slug = format!("{owner}/{name}");
        repo.hub = Some(if crate::state::demo_mode() {
            Ok(demo_hub(&slug))
        } else {
            via_gh(project, &slug).or_else(|_| via_api(&slug))
        });
    }
    Some(repo)
}

/// `FORGE_DEMO=1`: GitHub as a busy repository would show it, for the
/// screenshots and the tour, with no network.
fn demo_hub(slug: &str) -> Hub {
    let url = |kind: &str, n: u64| format!("https://github.com/{slug}/{kind}/{n}");
    let pr = |n, title: &str, author: &str, branch: &str, checks| Pr {
        number: n,
        title: title.into(),
        author: author.into(),
        branch: branch.into(),
        url: url("pull", n),
        draft: false,
        checks: Some(checks),
    };
    let issue = |n, title: &str| Issue {
        number: n,
        title: title.into(),
        url: url("issues", n),
    };
    Hub {
        prs: vec![
            pr(
                42,
                "Project knowledge: Always or On demand",
                "amine",
                "knowledge",
                Checks::Passing,
            ),
            pr(
                41,
                "Speed up the side panel's PDF pages",
                "sam",
                "faster-pdf",
                Checks::Failing,
            ),
            pr(
                39,
                "Remote: reconnect after sleep",
                "lee",
                "reconnect",
                Checks::Pending,
            ),
        ],
        issues: vec![
            issue(57, "Ctrl+N does nothing on the Projects page"),
            issue(55, "Comments on a file are lost after a restart"),
        ],
        run: Some(Run {
            name: "CI".into(),
            branch: "faster-pdf".into(),
            status: "completed".into(),
            conclusion: Some("failure".into()),
            url: format!("https://github.com/{slug}/actions/runs/1"),
        }),
        via: "the demo",
    }
}

/// Bring the branch up to its upstream (fast-forward only: never a merge
/// you didn't ask for).
pub fn pull(project: &Path) -> Result<String, String> {
    run("git", project, &["pull", "--ff-only"]).map(|o| {
        if o.contains("Already up to date") {
            "Already up to date.".to_string()
        } else {
            "Pulled: the branch is up to date.".to_string()
        }
    })
}

/// Switch to `branch`, making it from here when `create`.
pub fn switch(project: &Path, branch: &str, create: bool) -> Result<(), String> {
    let args: Vec<&str> = if create {
        vec!["switch", "-c", branch]
    } else {
        vec!["switch", branch]
    };
    run("git", project, &args).map(|_| ())
}

/// The local branches, the current one first.
pub fn branches(project: &Path) -> Vec<String> {
    run(
        "git",
        project,
        &["branch", "--format=%(HEAD) %(refname:short)"],
    )
    .map(|o| {
        let mut v: Vec<(bool, String)> = o
            .lines()
            .filter_map(|l| {
                let (head, name) = l.split_at_checked(2)?;
                Some((head.starts_with('*'), name.trim().to_string()))
            })
            .collect();
        v.sort_by_key(|(head, _)| !head);
        v.into_iter().map(|(_, n)| n).collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remote_and_its_github_repo_are_read() {
        let config = "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://github.com/acme/tool.git\n[remote \"origin\"]\n\turl = git@github.com:amine/forge-gpui.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n";
        assert_eq!(
            parse_remote(config).as_deref(),
            Some("git@github.com:amine/forge-gpui.git")
        );
        assert_eq!(
            parse_remote("[remote \"up\"]\n url = https://x/y\n").as_deref(),
            Some("https://x/y")
        );
        for url in [
            "git@github.com:amine/forge-gpui.git",
            "https://github.com/amine/forge-gpui",
            "https://github.com/amine/forge-gpui.git/",
            "ssh://git@github.com/amine/forge-gpui.git",
        ] {
            assert_eq!(
                github_repo(url),
                Some(("amine".into(), "forge-gpui".into())),
                "{url}"
            );
        }
        assert_eq!(github_repo("https://gitlab.com/a/b"), None);
    }

    #[test]
    fn status_reads_the_branch_and_its_changes() {
        let s = "# branch.oid 1234\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 a b src/x.rs\n2 R. N... 100644 100644 100644 a b R100 new.rs\told.rs\n? notes.txt\n";
        let r = parse_status(s);
        assert_eq!(
            (r.branch.as_str(), r.upstream.as_deref()),
            ("main", Some("origin/main"))
        );
        assert_eq!((r.ahead, r.behind, r.changed, r.untracked), (2, 1, 2, 1));
    }

    #[test]
    fn checks_roll_up_failing_first_then_pending() {
        let v: Value = serde_json::from_str(
            r#"[{"conclusion":"SUCCESS"},{"status":"IN_PROGRESS","conclusion":""}]"#,
        )
        .unwrap();
        assert_eq!(rollup(&v), Some(Checks::Pending));
        let v: Value =
            serde_json::from_str(r#"[{"conclusion":"SUCCESS"},{"conclusion":"FAILURE"}]"#).unwrap();
        assert_eq!(rollup(&v), Some(Checks::Failing));
        let v: Value = serde_json::from_str(r#"[{"state":"SUCCESS"}]"#).unwrap();
        assert_eq!(rollup(&v), Some(Checks::Passing));
        assert_eq!(rollup(&Value::Null), None);
        let run = Run {
            name: "CI".into(),
            branch: "main".into(),
            status: "completed".into(),
            conclusion: Some("failure".into()),
            url: String::new(),
        };
        assert_eq!(run.checks(), Checks::Failing);
    }
}
