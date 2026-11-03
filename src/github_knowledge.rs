//! Knowledge from GitHub, as Claude's projects take it: pick a repository,
//! tick files or folders, and their text joins the project's knowledge
//! (`knowledge.rs`, On demand). The files are downloaded into
//! `.forge/knowledge/github/<owner>-<repo>/`, so every chat reads them like
//! any other knowledge file; Sync fetches the branch's latest and drops
//! what was deleted upstream.
//!
//! - GitHub through `gh` when it is installed and signed in (private
//!   repositories work), else GitHub's REST API, with `GH_TOKEN` /
//!   `GITHUB_TOKEN` when set (public repositories need none). The same
//!   route as the project's Repository card (`git_status.rs`).
//! - `.forge/github-knowledge.json` keeps each repository's branch, the
//!   commit last synced, what was picked and the files that came of it.
//!
//! Everything here blocks: run it off the UI thread.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SOURCES: &str = ".forge/github-knowledge.json";
/// Where downloaded files go, under the knowledge folder.
const DIR: &str = ".forge/knowledge/github";
/// A file larger than this stays on GitHub.
pub const FILE_MAX: u64 = 2 * 1024 * 1024;
/// One add or sync downloads at most this many files.
pub const FILES_MAX: usize = 300;

/// A repository as typed: `owner/repo`, a github.com link, or a link to a
/// branch or folder in it (`…/tree/main/docs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRef {
    pub repo: String,
    /// The branch, tag or commit named, if any (else the default branch).
    pub reference: Option<String>,
    /// A folder the link points into, if any.
    pub folder: Option<String>,
}

pub fn parse(input: &str) -> Option<RepoRef> {
    let s = input.trim().trim_end_matches('/');
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let s = s.strip_prefix("github.com/").unwrap_or(s);
    let mut parts = s.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let name = parts.next()?.trim_end_matches(".git");
    let ok = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if !ok(owner) || !ok(name) || matches!(owner, "." | "..") || matches!(name, "." | "..") {
        return None;
    }
    let rest: Vec<&str> = parts.collect();
    let (reference, folder) = match rest.as_slice() {
        ["tree" | "blob", r, more @ ..] => (
            Some(r.to_string()),
            (!more.is_empty()).then(|| more.join("/")),
        ),
        [] => (None, None),
        _ => return None,
    };
    Some(RepoRef {
        repo: format!("{owner}/{name}"),
        reference,
        folder,
    })
}

/// One path in a repository's tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub path: String,
    pub dir: bool,
    pub size: u64,
}

/// A repository's tree at a commit, as listed for picking.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub repo: String,
    pub reference: String,
    pub commit: String,
    pub nodes: Vec<Node>,
    /// GitHub cut the listing short (a very large repository).
    pub truncated: bool,
}

/// A repository in the project's knowledge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub repo: String,
    pub reference: String,
    pub commit: String,
    /// What was picked: files and folders, from the repository's root.
    pub picks: Vec<String>,
    /// The files those came to, from the repository's root.
    pub files: Vec<String>,
    /// When it was last synced (Unix seconds).
    pub synced_at: u64,
}

fn sources_path(project: &Path) -> PathBuf {
    project.join(SOURCES)
}

pub fn sources(project: &Path) -> Vec<Source> {
    std::fs::read_to_string(sources_path(project))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(project: &Path, list: &[Source]) -> Result<(), String> {
    let path = sources_path(project);
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Where a repository's files are kept in the project.
fn local_dir(project: &Path, repo: &str) -> PathBuf {
    project.join(DIR).join(repo.replace('/', "-"))
}

/// A repository path as a safe path under its folder: no `..`, no root.
fn local_path(project: &Path, repo: &str, path: &str) -> Option<PathBuf> {
    let mut out = local_dir(project, repo);
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

/// The files picks come to: a picked file, and every file under a picked
/// folder, small enough, at most `FILES_MAX`.
pub fn expand(nodes: &[Node], picks: &[String]) -> Vec<String> {
    let mut out: Vec<String> = nodes
        .iter()
        .filter(|n| !n.dir && n.size <= FILE_MAX)
        .filter(|n| {
            picks
                .iter()
                .any(|p| n.path == *p || n.path.starts_with(&format!("{p}/")))
        })
        .map(|n| n.path.clone())
        .collect();
    out.sort();
    out.dedup();
    out.truncate(FILES_MAX);
    out
}

/// How GitHub is reached: `gh` when it is installed (its own sign-in, so
/// private repositories work), and GitHub's REST API when `gh` isn't there
/// or a call through it fails.
struct Hub {
    gh: bool,
    agent: ureq::Agent,
    token: Option<String>,
}

fn hub() -> Hub {
    let gh = crate::git_status::command("gh", Path::new("."))
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build();
    let token = std::env::var("GH_TOKEN")
        .or_else(|_| std::env::var("GITHUB_TOKEN"))
        .ok()
        .filter(|t| !t.trim().is_empty());
    Hub { gh, agent, token }
}

impl Hub {
    fn gh(&self, accept: Option<&str>, path: &str) -> Result<Vec<u8>, String> {
        let mut c = crate::git_status::command("gh", Path::new("."));
        c.arg("api");
        if let Some(a) = accept {
            c.args(["-H", &format!("Accept: {a}")]);
        }
        let out = c.arg(path).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    /// One REST call; a token GitHub turns down is tried once without, for
    /// a public repository.
    fn rest(&self, accept: &str, path: &str) -> Result<ureq::Response, String> {
        let call = |token: Option<&String>| {
            let mut req = self
                .agent
                .get(&format!("https://api.github.com/{path}"))
                .set("Accept", accept)
                .set("User-Agent", "Forge-Desktop");
            if let Some(t) = token {
                req = req.set("Authorization", &format!("Bearer {t}"));
            }
            req.call().map_err(api_error)
        };
        match call(self.token.as_ref()) {
            Err(e) if e == UNAUTHORIZED => call(None),
            other => other,
        }
    }

    fn json(&self, path: &str) -> Result<Value, String> {
        if self.gh {
            if let Ok(out) = self.gh(None, path) {
                return serde_json::from_slice(&out).map_err(|e| e.to_string());
            }
        }
        self.rest("application/vnd.github+json", path)?
            .into_json()
            .map_err(|e| e.to_string())
    }

    fn raw(&self, path: &str) -> Result<Vec<u8>, String> {
        let accept = "application/vnd.github.raw";
        if self.gh {
            if let Ok(out) = self.gh(Some(accept), path) {
                return Ok(out);
            }
        }
        let r = self.rest(accept, path)?;
        let mut bytes = Vec::new();
        r.into_reader()
            .take(FILE_MAX + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}

use std::io::Read as _;

const UNAUTHORIZED: &str = "GitHub turned down the token in GH_TOKEN or GITHUB_TOKEN.";

fn api_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) => UNAUTHORIZED.into(),
        ureq::Error::Status(404, _) => "GitHub doesn't show this repository: check its name, or sign in with gh (gh auth login) for a private one.".into(),
        ureq::Error::Status(403 | 429, _) => {
            "GitHub's limit for unsigned requests was reached. Sign in with gh (gh auth login) to go on.".into()
        }
        e => e.to_string(),
    }
}

/// A path's segments, encoded for a URL.
fn encode(path: &str) -> String {
    path.split('/')
        .map(|p| {
            p.bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The commit a reference stands at, and the reference resolved (the
/// default branch when none is given).
fn resolve(hub: &Hub, repo: &str, reference: Option<&str>) -> Result<(String, String), String> {
    let reference = match reference {
        Some(r) => r.to_string(),
        None => hub.json(&format!("repos/{repo}"))?["default_branch"]
            .as_str()
            .unwrap_or("main")
            .to_string(),
    };
    let commit = hub.json(&format!("repos/{repo}/commits/{}", encode(&reference)))?["sha"]
        .as_str()
        .ok_or_else(|| format!("{repo} has no commit at {reference}"))?
        .to_string();
    Ok((reference, commit))
}

/// List a repository's files for picking.
pub fn tree(r: &RepoRef) -> Result<Tree, String> {
    let hub = hub();
    let (reference, commit) = resolve(&hub, &r.repo, r.reference.as_deref())?;
    let v = hub.json(&format!("repos/{}/git/trees/{commit}?recursive=1", r.repo))?;
    let mut nodes: Vec<Node> = v["tree"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let path = n["path"].as_str()?.to_string();
            let dir = match n["type"].as_str()? {
                "tree" => true,
                "blob" => false,
                _ => return None,
            };
            Some(Node {
                path,
                dir,
                size: n["size"].as_u64().unwrap_or(0),
            })
        })
        .collect();
    if let Some(f) = &r.folder {
        // The folder itself stays, so its files hang under it.
        let under = format!("{f}/");
        nodes.retain(|n| n.path == *f || n.path.starts_with(&under));
    }
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Tree {
        repo: r.repo.clone(),
        reference,
        commit,
        nodes,
        truncated: v["truncated"].as_bool().unwrap_or(false),
    })
}

/// Download `files` of `repo` at `commit` into the project and list them
/// in its knowledge. Returns the files that came.
fn download(
    project: &Path,
    hub: &Hub,
    repo: &str,
    commit: &str,
    files: &[String],
) -> Result<Vec<String>, String> {
    let mut got = vec![];
    let mut placed = vec![];
    for f in files {
        let Some(to) = local_path(project, repo, f) else {
            continue;
        };
        let bytes = hub.raw(&format!("repos/{repo}/contents/{}?ref={commit}", encode(f)))?;
        if bytes.len() as u64 > FILE_MAX {
            continue;
        }
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&to, bytes).map_err(|e| format!("{f}: {e}"))?;
        got.push(f.clone());
        placed.push(to);
    }
    crate::knowledge::add(project, &placed)?;
    Ok(got)
}

/// Add the picked files and folders of `tree` to the project's knowledge.
/// Returns how many files came.
pub fn add(project: &Path, tree: &Tree, picks: &[String]) -> Result<usize, String> {
    let files = expand(&tree.nodes, picks);
    if files.is_empty() {
        return Err("Nothing to add: pick a file or a folder with text in it.".into());
    }
    let hub = hub();
    let got = download(project, &hub, &tree.repo, &tree.commit, &files)?;
    let mut list = sources(project);
    let now = now();
    match list.iter_mut().find(|s| s.repo == tree.repo) {
        Some(s) => {
            s.reference = tree.reference.clone();
            s.commit = tree.commit.clone();
            for p in picks {
                if !s.picks.contains(p) {
                    s.picks.push(p.clone());
                }
            }
            for f in &got {
                if !s.files.contains(f) {
                    s.files.push(f.clone());
                }
            }
            s.synced_at = now;
        }
        None => list.push(Source {
            repo: tree.repo.clone(),
            reference: tree.reference.clone(),
            commit: tree.commit.clone(),
            picks: picks.to_vec(),
            files: got.clone(),
            synced_at: now,
        }),
    }
    save(project, &list)?;
    Ok(got.len())
}

/// What a sync did, for the note it leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Synced {
    pub updated: usize,
    pub removed: usize,
    pub unchanged: bool,
}

/// Bring `repo`'s files up to its branch's latest commit: download what
/// the picks come to now, and take out files gone upstream.
pub fn sync(project: &Path, repo: &str) -> Result<Synced, String> {
    let mut list = sources(project);
    let Some(i) = list.iter().position(|s| s.repo == repo) else {
        return Err(format!("{repo} isn't in this project's knowledge"));
    };
    let s = list[i].clone();
    let r = RepoRef {
        repo: s.repo.clone(),
        reference: Some(s.reference.clone()),
        folder: None,
    };
    let t = tree(&r)?;
    if t.commit == s.commit {
        list[i].synced_at = now();
        save(project, &list)?;
        return Ok(Synced {
            unchanged: true,
            ..Default::default()
        });
    }
    let files = expand(&t.nodes, &s.picks);
    let gone: Vec<String> = s
        .files
        .iter()
        .filter(|f| !files.contains(f))
        .cloned()
        .collect();
    for f in &gone {
        if let Some(p) = local_path(project, repo, f) {
            if let Some(rel) = rel(project, &p) {
                let _ = crate::knowledge::remove(project, &rel);
            }
        }
    }
    let hub = hub();
    let got = download(project, &hub, repo, &t.commit, &files)?;
    list[i].commit = t.commit;
    list[i].files = got.clone();
    list[i].synced_at = now();
    save(project, &list)?;
    Ok(Synced {
        updated: got.len(),
        removed: gone.len(),
        unchanged: false,
    })
}

/// Take a repository out of the knowledge, with its files.
pub fn remove(project: &Path, repo: &str) -> Result<(), String> {
    let mut list = sources(project);
    let Some(i) = list.iter().position(|s| s.repo == repo) else {
        return Ok(());
    };
    let s = list.remove(i);
    for f in &s.files {
        if let Some(p) = local_path(project, repo, f) {
            if let Some(rel) = rel(project, &p) {
                let _ = crate::knowledge::remove(project, &rel);
            }
        }
    }
    let _ = std::fs::remove_dir_all(local_dir(project, repo));
    save(project, &list)
}

fn rel(project: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(project)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_reads_however_it_is_typed() {
        let plain = parse("anthropics/claude-code").unwrap();
        assert_eq!(plain.repo, "anthropics/claude-code");
        assert_eq!((plain.reference, plain.folder), (None, None));
        let link = parse("https://github.com/owner/repo.git/").unwrap();
        assert_eq!(link.repo, "owner/repo");
        let deep = parse("github.com/owner/repo/tree/dev/docs/api").unwrap();
        assert_eq!(deep.reference.as_deref(), Some("dev"));
        assert_eq!(deep.folder.as_deref(), Some("docs/api"));
        assert!(parse("just-one-word").is_none());
        assert!(parse("owner/repo/issues/3").is_none());
        assert!(parse("own er/repo").is_none());
    }

    #[test]
    fn picks_come_to_their_files_small_enough() {
        let n = |path: &str, dir: bool, size: u64| Node {
            path: path.into(),
            dir,
            size,
        };
        let nodes = vec![
            n("README.md", false, 10),
            n("docs", true, 0),
            n("docs/a.md", false, 10),
            n("docs/big.pdf", false, FILE_MAX + 1),
            n("docs/deep/b.md", false, 10),
            n("docsx/c.md", false, 10),
        ];
        assert_eq!(
            expand(&nodes, &["docs".into(), "README.md".into()]),
            vec!["README.md", "docs/a.md", "docs/deep/b.md"]
        );
    }

    #[test]
    fn a_path_never_leaves_its_folder() {
        let p = Path::new("/p");
        assert!(local_path(p, "o/r", "../x").is_none());
        assert!(local_path(p, "o/r", "a/../../x").is_none());
        assert!(local_path(p, "o/r", "a\\b").is_none());
        assert_eq!(
            local_path(p, "o/r", "docs/a.md").unwrap(),
            Path::new("/p/.forge/knowledge/github/o-r/docs/a.md")
        );
    }

    #[test]
    fn a_path_is_encoded_by_segment() {
        assert_eq!(encode("docs/a b#c.md"), "docs/a%20b%23c.md");
    }

    #[test]
    fn removing_a_repository_takes_its_files_out_of_the_knowledge() {
        let dir = std::env::temp_dir().join(format!("forge-ghk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = local_path(&dir, "o/r", "docs/a.md").unwrap();
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "hello").unwrap();
        crate::knowledge::add(&dir, std::slice::from_ref(&f)).unwrap();
        save(
            &dir,
            &[Source {
                repo: "o/r".into(),
                reference: "main".into(),
                commit: "abc".into(),
                picks: vec!["docs".into()],
                files: vec!["docs/a.md".into()],
                synced_at: 1,
            }],
        )
        .unwrap();
        let k = crate::knowledge::compile(&dir, 1000);
        assert!(k
            .items
            .iter()
            .any(|i| i.rel.ends_with("github/o-r/docs/a.md")));
        remove(&dir, "o/r").unwrap();
        assert!(sources(&dir).is_empty());
        assert!(!f.exists());
        let k = crate::knowledge::compile(&dir, 1000);
        assert!(k.items.iter().all(|i| !i.rel.contains("github/o-r")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
