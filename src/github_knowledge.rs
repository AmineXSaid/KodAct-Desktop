//! Knowledge from GitHub, as Claude's projects take it: pick a repository,
//! tick files or folders, and their text joins the project's knowledge
//! (`knowledge.rs`, On demand). The files are downloaded into
//! `.kodact/knowledge/github/<owner>-<repo>/`, so every chat reads them like
//! any other knowledge file; Sync fetches the branch's latest and drops
//! what was deleted upstream.
//!
//! - GitHub through `gh` when it is installed and signed in (private
//!   repositories work), else GitHub's REST API, with `GH_TOKEN` /
//!   `GITHUB_TOKEN` when set (public repositories need none). The same
//!   route as the project's Repository card (`git_status.rs`).
//! - `.kodact/github-knowledge.json` keeps each repository's branch, the
//!   commit last synced, what was picked and the files that came of it.
//!
//! Everything here blocks: run it off the UI thread.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SOURCES: &str = "github-knowledge.json";
/// Where downloaded files go, under the knowledge folder.
const DIR: &str = "knowledge/github";
/// A file larger than this stays on GitHub.
pub const FILE_MAX: u64 = 2 * 1024 * 1024;
/// One add or sync downloads at most this many files.
pub const FILES_MAX: usize = 300;

/// A repository as typed: `owner/repo`, a github.com / gitlab.com / custom
/// self-hosted link, or a link to a branch or folder in it (`…/tree/main/docs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRef {
    pub kind: crate::git_status::ForgeKind,
    pub host: String,
    pub repo: String,
    /// The branch, tag or commit named, if any (else the default branch).
    pub reference: Option<String>,
    /// A folder the link points into, if any.
    pub folder: Option<String>,
}

pub fn parse(input: &str) -> Option<RepoRef> {
    let s = input.trim().trim_end_matches('/');
    let has_scheme = s.starts_with("https://") || s.starts_with("http://");
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let ok = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !matches!(p, "." | "..")
    };

    let first = s.split('/').next().unwrap_or("");
    let (kind, host, path_after_host) = if first.contains('.') || has_scheme {
        let (h, rest) = s.split_once('/')?;
        let host = h.to_ascii_lowercase();
        let kind = if host == "github.com" {
            crate::git_status::ForgeKind::GitHub
        } else if host == "gitlab.com" || host.contains("gitlab") {
            crate::git_status::ForgeKind::GitLab
        } else {
            crate::git_status::ForgeKind::Custom
        };
        (kind, host, rest)
    } else {
        (
            crate::git_status::ForgeKind::GitHub,
            "github.com".to_string(),
            s,
        )
    };

    if kind == crate::git_status::ForgeKind::GitHub {
        let mut parts = path_after_host.split('/').filter(|p| !p.is_empty());
        let owner = parts.next()?;
        let name = parts.next()?.trim_end_matches(".git");
        if !ok(owner) || !ok(name) {
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
        return Some(RepoRef {
            kind,
            host,
            repo: format!("{owner}/{name}"),
            reference,
            folder,
        });
    }

    let (repo_part, after_sep) = match path_after_host.split_once("/-/") {
        Some((l, r)) => (l, Some(r)),
        None => (path_after_host, None),
    };
    let mut segs: Vec<&str> = repo_part.split('/').filter(|p| !p.is_empty()).collect();
    if segs.len() < 2 {
        return None;
    }
    let last_clean = segs.last()?.trim_end_matches(".git");
    *segs.last_mut()? = last_clean;
    if segs.iter().any(|p| !ok(p)) {
        return None;
    }
    let (reference, folder) = match after_sep {
        None => (None, None),
        Some(tail) => {
            let tail_parts: Vec<&str> = tail.split('/').filter(|p| !p.is_empty()).collect();
            match tail_parts.as_slice() {
                ["tree" | "blob", r, more @ ..] => (
                    Some(r.to_string()),
                    (!more.is_empty()).then(|| more.join("/")),
                ),
                _ => return None,
            }
        }
    };
    Some(RepoRef {
        kind,
        host,
        repo: segs.join("/"),
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
    pub kind: crate::git_status::ForgeKind,
    pub host: String,
    pub repo: String,
    pub reference: String,
    pub commit: String,
    pub nodes: Vec<Node>,
    /// The remote cut the listing short (a very large repository).
    pub truncated: bool,
}

fn default_host() -> String {
    "github.com".to_string()
}

/// A repository in the project's knowledge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    #[serde(default)]
    pub kind: crate::git_status::ForgeKind,
    #[serde(default = "default_host")]
    pub host: String,
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
    crate::names::project_dir(project).join(SOURCES)
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
    crate::names::project_dir(project).join(DIR).join(repo.replace('/', "-"))
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

/// How GitHub / GitLab / self-hosted Git is reached: `gh` when installed
/// for GitHub, else GitHub REST API or GitLab REST v4 API (`GITLAB_TOKEN` /
/// `GL_TOKEN` when set).
struct Hub {
    gh: bool,
    agent: ureq::Agent,
    token: Option<String>,
    gl_token: Option<String>,
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
    let gl_token = std::env::var("GITLAB_TOKEN")
        .or_else(|_| std::env::var("GL_TOKEN"))
        .ok()
        .filter(|t| !t.trim().is_empty());
    Hub {
        gh,
        agent,
        token,
        gl_token,
    }
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

    /// One GitHub REST call; a token GitHub turns down is tried once without,
    /// for a public repository.
    fn rest(&self, accept: &str, path: &str) -> Result<ureq::Response, String> {
        let call = |token: Option<&String>| {
            let mut req = self
                .agent
                .get(&format!("https://api.github.com/{path}"))
                .set("Accept", accept)
                .set("User-Agent", "KodAct-Desktop");
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

    fn gl_rest(&self, host: &str, path: &str) -> Result<ureq::Response, String> {
        let call = |token: Option<&String>| {
            let mut req = self
                .agent
                .get(&format!("https://{host}/api/v4/{path}"))
                .set("Accept", "application/json")
                .set("User-Agent", "KodAct-Desktop");
            if let Some(t) = token {
                req = req.set("PRIVATE-TOKEN", t);
            }
            req.call().map_err(|e| gl_api_error(host, e))
        };
        match call(self.gl_token.as_ref()) {
            Err(e) if e.starts_with("GitLab turned down") => call(None),
            other => other,
        }
    }

    fn gl_json(&self, host: &str, path: &str) -> Result<Value, String> {
        self.gl_rest(host, path)?
            .into_json()
            .map_err(|e| e.to_string())
    }

    fn gl_raw(&self, host: &str, path: &str) -> Result<Vec<u8>, String> {
        let r = self.gl_rest(host, path)?;
        let mut bytes = Vec::new();
        r.into_reader()
            .take(FILE_MAX + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Ok(bytes)
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

fn gl_api_error(host: &str, e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) => {
            format!("GitLab turned down the token in GITLAB_TOKEN or GL_TOKEN for {host}.")
        }
        ureq::Error::Status(404, _) => format!(
            "{host} doesn't show this repository via GitLab API: check the path or set GITLAB_TOKEN for private projects."
        ),
        ureq::Error::Status(403 | 429, _) => {
            format!("{host} rejected or rate-limited the request. Set GITLAB_TOKEN to continue.")
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
fn resolve(hub: &Hub, r: &RepoRef) -> Result<(String, String), String> {
    if r.kind == crate::git_status::ForgeKind::GitHub {
        let reference = match &r.reference {
            Some(rf) => rf.clone(),
            None => hub.json(&format!("repos/{}", r.repo))?["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_string(),
        };
        let commit = hub.json(&format!("repos/{}/commits/{}", r.repo, encode(&reference)))?["sha"]
            .as_str()
            .ok_or_else(|| format!("{} has no commit at {reference}", r.repo))?
            .to_string();
        Ok((reference, commit))
    } else {
        let pid = crate::git_status::url_encode_component(&r.repo);
        let reference = match &r.reference {
            Some(rf) => rf.clone(),
            None => hub.gl_json(&r.host, &format!("projects/{pid}"))?["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_string(),
        };
        let commit = hub.gl_json(
            &r.host,
            &format!("projects/{pid}/repository/commits/{}", encode(&reference)),
        )?["id"]
            .as_str()
            .ok_or_else(|| format!("{} has no commit at {reference}", r.repo))?
            .to_string();
        Ok((reference, commit))
    }
}

/// List a repository's files for picking.
pub fn tree(r: &RepoRef) -> Result<Tree, String> {
    let hub = hub();
    let (reference, commit) = resolve(&hub, r)?;
    let (mut nodes, truncated) = if r.kind == crate::git_status::ForgeKind::GitHub {
        let v = hub.json(&format!("repos/{}/git/trees/{commit}?recursive=1", r.repo))?;
        let nodes = v["tree"]
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
        (nodes, v["truncated"].as_bool().unwrap_or(false))
    } else {
        let pid = crate::git_status::url_encode_component(&r.repo);
        let v = hub.gl_json(
            &r.host,
            &format!("projects/{pid}/repository/tree?recursive=1&per_page=100&ref={commit}"),
        )?;
        let nodes: Vec<Node> = v
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
                    size: 0,
                })
            })
            .collect();
        let truncated = nodes.len() >= 100;
        (nodes, truncated)
    };
    if let Some(f) = &r.folder {
        // The folder itself stays, so its files hang under it.
        let under = format!("{f}/");
        nodes.retain(|n| n.path == *f || n.path.starts_with(&under));
    }
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Tree {
        kind: r.kind,
        host: r.host.clone(),
        repo: r.repo.clone(),
        reference,
        commit,
        nodes,
        truncated,
    })
}

/// Download `files` of `repo` at `commit` into the project and list them
/// in its knowledge. Returns the files that came.
fn download(
    project: &Path,
    hub: &Hub,
    kind: crate::git_status::ForgeKind,
    host: &str,
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
        let bytes = if kind == crate::git_status::ForgeKind::GitHub {
            hub.raw(&format!("repos/{repo}/contents/{}?ref={commit}", encode(f)))?
        } else {
            let pid = crate::git_status::url_encode_component(repo);
            let f_enc = crate::git_status::url_encode_component(f);
            hub.gl_raw(
                host,
                &format!("projects/{pid}/repository/files/{f_enc}/raw?ref={commit}"),
            )?
        };
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
    let got = download(
        project,
        &hub,
        tree.kind,
        &tree.host,
        &tree.repo,
        &tree.commit,
        &files,
    )?;
    let mut list = sources(project);
    let now = now();
    match list.iter_mut().find(|s| s.repo == tree.repo) {
        Some(s) => {
            s.kind = tree.kind;
            s.host = tree.host.clone();
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
            kind: tree.kind,
            host: tree.host.clone(),
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
        kind: s.kind,
        host: s.host.clone(),
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
    let got = download(project, &hub, s.kind, &s.host, repo, &t.commit, &files)?;
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
        assert_eq!(plain.kind, crate::git_status::ForgeKind::GitHub);
        assert_eq!(plain.host, "github.com");
        assert_eq!(plain.repo, "anthropics/claude-code");
        assert_eq!((plain.reference, plain.folder), (None, None));
        let link = parse("https://github.com/owner/repo.git/").unwrap();
        assert_eq!(link.repo, "owner/repo");
        let deep = parse("github.com/owner/repo/tree/dev/docs/api").unwrap();
        assert_eq!(deep.reference.as_deref(), Some("dev"));
        assert_eq!(deep.folder.as_deref(), Some("docs/api"));

        let gl = parse("https://gitlab.com/team/sub/service.git").unwrap();
        assert_eq!(gl.kind, crate::git_status::ForgeKind::GitLab);
        assert_eq!(gl.host, "gitlab.com");
        assert_eq!(gl.repo, "team/sub/service");

        let gl_deep = parse("https://gitlab.company.com/team/repo/-/tree/main/docs/adr").unwrap();
        assert_eq!(gl_deep.kind, crate::git_status::ForgeKind::GitLab);
        assert_eq!(gl_deep.host, "gitlab.company.com");
        assert_eq!(gl_deep.repo, "team/repo");
        assert_eq!(gl_deep.reference.as_deref(), Some("main"));
        assert_eq!(gl_deep.folder.as_deref(), Some("docs/adr"));

        let custom = parse("https://git.acme.internal/org/app").unwrap();
        assert_eq!(custom.kind, crate::git_status::ForgeKind::Custom);
        assert_eq!(custom.host, "git.acme.internal");
        assert_eq!(custom.repo, "org/app");

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
            Path::new("/p/.kodact/knowledge/github/o-r/docs/a.md")
        );
    }

    #[test]
    fn a_path_is_encoded_by_segment() {
        assert_eq!(encode("docs/a b#c.md"), "docs/a%20b%23c.md");
    }

    #[test]
    fn removing_a_repository_takes_its_files_out_of_the_knowledge() {
        let dir = std::env::temp_dir().join(format!("kodact-ghk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = local_path(&dir, "o/r", "docs/a.md").unwrap();
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "hello").unwrap();
        crate::knowledge::add(&dir, std::slice::from_ref(&f)).unwrap();
        save(
            &dir,
            &[Source {
                kind: crate::git_status::ForgeKind::GitHub,
                host: "github.com".into(),
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
