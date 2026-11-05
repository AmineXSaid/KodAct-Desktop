//! Installing skills and plugins the way Claude Code users do: from a GitHub
//! repository (`owner/repo`, or a link to a folder in it), any git URL, a
//! folder, or a `.zip` / `.skill` file. KodAct CLI loads Claude Code's folders
//! (`~/.claude/skills`, a project's `.claude/skills`), so a skill installed
//! here works in both, unchanged.
//!
//! Nothing a skill carries runs while it is installed. Before installing, the
//! report shows what each skill would run (its ``!`command` `` lines, its
//! scripts) and what it allows without asking (`allowed-tools`).
//!
//! Plugins come from Claude Code marketplaces (a repository with
//! `.claude-plugin/marketplace.json`): each is copied under KodAct's config
//! folder and added to the user's `pluginDirs`, which KodAct CLI loads.
//!
//! Everything here blocks (downloads, copies): call it off the UI thread.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// The most a download may be.
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;
/// How deep a repository is searched for skills.
const SCAN_DEPTH: usize = 5;
/// What an installed skill or plugin records about where it came from.
pub const RECORD: &str = ".kodact-install.json";
/// The record as Forge (before the rename) wrote it: still read.
pub const OLD_RECORD: &str = ".forge-install.json";

/// Whether `name` is an install record, either name.
pub fn is_record(name: &str) -> bool {
    name == RECORD || name == OLD_RECORD
}

/// Where to get skills or plugins from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A GitHub repository, at a branch, tag or commit, optionally one folder of it.
    GitHub {
        owner: String,
        repo: String,
        git_ref: Option<String>,
        path: Option<String>,
    },
    /// Any other git repository (cloned with `git`).
    Git {
        url: String,
        git_ref: Option<String>,
    },
    Folder(PathBuf),
    /// A `.zip` or `.skill` file.
    Archive(PathBuf),
}

impl Source {
    /// How the source is written back (in a record, for Update).
    pub fn label(&self) -> String {
        match self {
            Source::GitHub {
                owner,
                repo,
                git_ref,
                path,
            } => {
                let mut s = format!("https://github.com/{owner}/{repo}");
                if git_ref.is_some() || path.is_some() {
                    s.push_str(&format!("/tree/{}", git_ref.as_deref().unwrap_or("HEAD")));
                    if let Some(p) = path {
                        s.push_str(&format!("/{p}"));
                    }
                }
                s
            }
            Source::Git { url, git_ref } => match git_ref {
                Some(r) => format!("{url}#{r}"),
                None => url.clone(),
            },
            Source::Folder(p) | Source::Archive(p) => p.display().to_string(),
        }
    }
}

/// Read what the person typed: a path that exists, `owner/repo`, a GitHub
/// link (`/tree/<ref>/<path>` picks a folder), or a git URL (`#ref` picks one).
pub fn parse_source(input: &str, cwd: &Path) -> Result<Source, String> {
    let input = input.trim().trim_matches(['"', '\'']);
    if input.is_empty() {
        return Err("Paste a GitHub link or owner/repo, or choose a folder or a .zip file.".into());
    }
    let expanded = match input
        .strip_prefix("~/")
        .or_else(|| input.strip_prefix("~\\"))
    {
        Some(rest) => crate::rules::home_dir().map(|h| h.join(rest)),
        None => Some(PathBuf::from(input)),
    };
    if let Some(p) = expanded.map(|p| if p.is_absolute() { p } else { cwd.join(p) }) {
        if p.is_dir() {
            return Ok(Source::Folder(p));
        }
        let lower = input.to_ascii_lowercase();
        if p.is_file() && (lower.ends_with(".zip") || lower.ends_with(".skill")) {
            return Ok(Source::Archive(p));
        }
    }
    let (url, fragment) = match input.split_once('#') {
        Some((u, f)) => (u, Some(f.to_string()).filter(|f| !f.is_empty())),
        None => (input, None),
    };
    let bare = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let bare = bare.strip_prefix("www.").unwrap_or(bare);
    let gh = bare.strip_prefix("github.com/").or_else(|| {
        // `owner/repo` alone: two plain words.
        let parts: Vec<&str> = url.split('/').collect();
        (parts.len() == 2
            && !url.contains(':')
            && parts.iter().all(|p| {
                !p.is_empty()
                    && p.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
            }))
        .then_some(url)
    });
    if let Some(rest) = gh {
        let parts: Vec<&str> = rest.trim_end_matches('/').split('/').collect();
        if parts.len() >= 2 {
            let repo = parts[1].trim_end_matches(".git").to_string();
            let (mut git_ref, mut path) = (fragment, None);
            if parts.len() >= 4 && matches!(parts[2], "tree" | "blob") {
                git_ref = Some(parts[3].to_string());
                let rest = parts[4..].join("/");
                // A link to a SKILL.md names its folder.
                let rest = rest
                    .strip_suffix("/SKILL.md")
                    .map(str::to_string)
                    .unwrap_or(rest);
                path = Some(rest).filter(|p| !p.is_empty() && p != "SKILL.md");
            }
            return Ok(Source::GitHub {
                owner: parts[0].to_string(),
                repo,
                git_ref,
                path,
            });
        }
    }
    if url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("git@")
        || url.starts_with("ssh://")
    {
        return Ok(Source::Git {
            url: url.to_string(),
            git_ref: fragment,
        });
    }
    Err(format!(
        "{input} is not a folder, a .zip or .skill file, a GitHub repository (owner/repo) or a git URL."
    ))
}

fn http() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .try_proxy_from_env(true)
        .build()
}

/// Download `url` into `file`, at most [`MAX_DOWNLOAD`].
fn download(url: &str, file: &Path) -> Result<(), String> {
    let resp = http()
        .get(url)
        .set("User-Agent", "KodAct")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => {
                "not found (a private repository, or a wrong name or branch)".to_string()
            }
            ureq::Error::Status(code, _) => format!("the server answered {code}"),
            ureq::Error::Transport(t) => t.to_string(),
        })?;
    let mut bytes = vec![];
    resp.into_reader()
        .take(MAX_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        return Err(format!(
            "it is larger than {} MB",
            MAX_DOWNLOAD / 1024 / 1024
        ));
    }
    std::fs::write(file, bytes).map_err(|e| e.to_string())
}

/// The one folder an archive holds (GitHub's `repo-main/`), else the archive's root.
fn single_root(dir: &Path) -> PathBuf {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    match entries.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => dir.to_path_buf(),
    }
}

/// Get the source into `work` (a fresh folder of its own) and return the
/// folder to look in. A folder source is used where it is.
pub fn fetch(src: &Source, work: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    match src {
        Source::Folder(p) => Ok(p.clone()),
        Source::Archive(p) => {
            let dest = work.join("archive");
            crate::stage::zip(p, &dest).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok(single_root(&dest))
        }
        Source::GitHub {
            owner,
            repo,
            git_ref,
            path,
        } => {
            let r = git_ref.as_deref().unwrap_or("HEAD");
            let url = format!("https://codeload.github.com/{owner}/{repo}/zip/{r}");
            let file = work.join("repo.zip");
            download(&url, &file).map_err(|e| format!("Couldn't download {owner}/{repo}: {e}"))?;
            let dest = work.join("repo");
            crate::stage::zip(&file, &dest).map_err(|e| format!("{owner}/{repo}: {e}"))?;
            let _ = std::fs::remove_file(&file);
            let root = single_root(&dest);
            match path {
                Some(p) => {
                    let sub = crate::stage::safe_entry(&root, p)
                        .ok_or("that folder is outside the repository")?;
                    sub.is_dir()
                        .then_some(sub)
                        .ok_or_else(|| format!("{owner}/{repo} has no folder {p}"))
                }
                None => Ok(root),
            }
        }
        Source::Git { url, git_ref } => {
            let dest = work.join("repo");
            let mut cmd = std::process::Command::new("git");
            cmd.args(["clone", "--depth", "1", "--quiet"]);
            if let Some(r) = git_ref {
                cmd.args(["--branch", r]);
            }
            cmd.arg(url).arg(&dest).env("GIT_TERMINAL_PROMPT", "0");
            let out = cmd.output().map_err(|e| {
                format!("Couldn't run git ({e}): git is needed for {url}; a GitHub link works without it")
            })?;
            if !out.status.success() {
                return Err(format!(
                    "git couldn't clone {url}: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            let _ = std::fs::remove_dir_all(dest.join(".git"));
            Ok(dest)
        }
    }
}

/// A skill found in a source, with what it would run and allow.
#[derive(Clone, Debug, PartialEq)]
pub struct FoundSkill {
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    /// Files in its folder (SKILL.md included).
    pub files: usize,
    /// Its ``!`command` `` lines and ` ```! ` blocks: they run each time it is used.
    pub commands: Vec<String>,
    /// Scripts in its folder the model may run.
    pub scripts: Vec<String>,
    /// What it allows without asking while it runs.
    pub allowed_tools: Vec<String>,
    /// Why it can't be installed as it is.
    pub problem: Option<String>,
}

/// A plugin found in a source (`.claude-plugin/plugin.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct FoundPlugin {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub dir: PathBuf,
}

/// What a source holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub skills: Vec<FoundSkill>,
    pub plugin: Option<FoundPlugin>,
    pub marketplace: Option<Marketplace>,
}

impl Found {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty() && self.plugin.is_none() && self.marketplace.is_none()
    }
}

/// A name Claude Code accepts: no control or invisible characters, no `<` or `>`.
fn name_problem(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("it has no name".into());
    }
    let bad = name.chars().any(|c| {
        c.is_control()
            || matches!(c, '<' | '>' | '/' | '\\')
            || ('\u{200b}'..='\u{200f}').contains(&c)
            || ('\u{202a}'..='\u{202e}').contains(&c)
            || ('\u{2060}'..='\u{206f}').contains(&c)
            || c == '\u{feff}'
            || ('\u{e000}'..='\u{f8ff}').contains(&c)
    });
    if bad {
        return Some(format!(
            "its name {name:?} has characters Claude Code and KodAct refuse"
        ));
    }
    if name == "synced" || name.starts_with("anthropic-skills") {
        return Some(format!("{name} is a reserved name"));
    }
    None
}

/// The ``!`command` `` lines and ` ```! ` blocks in a skill's text.
pub fn commands_in(body: &str) -> Vec<String> {
    let mut out = vec![];
    let mut in_block: Option<Vec<&str>> = None;
    for line in body.lines() {
        if let Some(block) = in_block.as_mut() {
            if line.trim_start().starts_with("```") {
                out.push(block.join("\n"));
                in_block = None;
            } else {
                block.push(line);
            }
            continue;
        }
        if line.trim_start().starts_with("```!") {
            in_block = Some(vec![]);
            continue;
        }
        let mut rest = line;
        let mut prev_ok = true;
        while let Some(at) = rest.find("!`") {
            let before_ok = if at == 0 {
                prev_ok
            } else {
                rest[..at].ends_with([' ', '\t'])
            };
            let after = &rest[at + 2..];
            match after.find('`') {
                Some(end) if before_ok && end > 0 => {
                    out.push(after[..end].trim().to_string());
                    rest = &after[end + 1..];
                }
                Some(end) => rest = &after[end + 1..],
                None => break,
            }
            prev_ok = false;
        }
    }
    out
}

fn files_in(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n != ".git" && n != "node_modules")
            {
                files_in(&p, base, out);
            }
        } else {
            out.push(
                p.strip_prefix(base)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn is_script(rel: &str) -> bool {
    let ext = rel
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "sh" | "bash"
            | "zsh"
            | "py"
            | "js"
            | "mjs"
            | "cjs"
            | "ts"
            | "ps1"
            | "psm1"
            | "bat"
            | "cmd"
            | "rb"
            | "pl"
            | "php"
    )
}

/// Read the skill in `dir` (its SKILL.md).
pub fn read_skill(dir: &Path) -> Option<FoundSkill> {
    let text = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    let doc = crate::forge_files::Doc::parse(&text);
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = doc.get("name").filter(|n| !n.is_empty()).unwrap_or(folder);
    let description = doc
        .get("description")
        .filter(|d| !d.is_empty())
        .or_else(|| {
            doc.body
                .lines()
                .map(|l| l.trim().trim_start_matches('#').trim())
                .find(|l| !l.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let mut files = vec![];
    files_in(dir, dir, &mut files);
    files.sort();
    let allowed = doc
        .get("allowed-tools")
        .map(|raw| split_rules(raw.trim_start_matches('[').trim_end_matches(']')))
        .unwrap_or_default();
    let problem =
        name_problem(&name).or_else(|| description.is_empty().then(|| "it is empty".to_string()));
    Some(FoundSkill {
        commands: commands_in(&doc.body),
        scripts: files.iter().filter(|f| is_script(f)).cloned().collect(),
        files: files.len(),
        allowed_tools: allowed,
        problem,
        name,
        description,
        dir: dir.to_path_buf(),
    })
}

/// `Bash(git add *) Bash(git commit *), Read`: split outside parentheses.
fn split_rules(raw: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (vec![], String::new(), 0usize);
    for c in raw.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' | ' ' | '\t' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out.into_iter()
        .map(|s| s.trim().trim_matches(['"', '\'']).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A plugin's manifest, if `dir` is a plugin.
pub fn read_plugin(dir: &Path) -> Option<FoundPlugin> {
    let file = [
        ".claude-plugin/plugin.json",
        ".kodact-plugin/plugin.json",
        "plugin.json",
    ]
    .iter()
    .map(|f| dir.join(f))
    .find(|f| f.is_file())?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(FoundPlugin {
        name: v["name"]
            .as_str()
            .filter(|n| !n.is_empty())
            .unwrap_or(&folder)
            .to_string(),
        description: v["description"].as_str().unwrap_or_default().to_string(),
        version: v["version"].as_str().map(str::to_string),
        dir: dir.to_path_buf(),
    })
}

fn scan_dir(dir: &Path, depth: usize, out: &mut Vec<FoundSkill>) {
    if dir.join("SKILL.md").is_file() {
        if let Some(s) = read_skill(dir) {
            out.push(s);
        }
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    // `.claude` holds skills; other hidden folders and dependencies don't.
                    n == ".claude" || (!n.starts_with('.') && n != "node_modules" && n != "target")
                })
        })
        .collect();
    subs.sort();
    for s in subs {
        scan_dir(&s, depth - 1, out);
    }
}

/// What `root` holds: a marketplace, a plugin, or skills (a skill at the root,
/// or skill folders anywhere below it).
pub fn scan(root: &Path) -> Found {
    let marketplace = read_marketplace(root).ok();
    let plugin = read_plugin(root);
    let mut skills = vec![];
    if marketplace.is_none() {
        scan_dir(root, SCAN_DEPTH, &mut skills);
    }
    Found {
        skills,
        plugin,
        marketplace,
    }
}

/// Where an installed thing came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Origin {
    pub source: String,
    /// For a skill: its folder in the source, to find it again on Update.
    pub path_in_source: Option<String>,
    pub installed: String,
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let p = e.path();
        let name = e.file_name();
        if name == ".git" {
            continue;
        }
        let ft = e.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            copy_dir(&p, &to.join(&name))?;
        } else {
            std::fs::copy(&p, to.join(&name)).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(())
}

/// Copy `from` to `to` through a temporary sibling, so a failure leaves no half copy
/// and `to` (when `replace`) is swapped only once the copy is whole.
fn place(from: &Path, to: &Path, replace: bool) -> Result<(), String> {
    if to.exists() && !replace {
        return Err(format!("{} is already installed there", to.display()));
    }
    let parent = to.parent().ok_or("no folder to install into")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(
        ".{}.installing",
        to.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(e) = copy_dir(from, &tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if to.exists() {
        std::fs::remove_dir_all(to)
            .map_err(|e| format!("couldn't replace {}: {e}", to.display()))?;
    }
    std::fs::rename(&tmp, to).map_err(|e| e.to_string())
}

fn write_record(dir: &Path, origin: &Origin) -> Result<(), String> {
    let v = json!({
        "source": origin.source,
        "path": origin.path_in_source,
        "installed": origin.installed,
    });
    std::fs::write(
        dir.join(RECORD),
        serde_json::to_string_pretty(&v).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}

/// Where an installed skill or plugin came from, if KodAct installed it.
pub fn origin(dir: &Path) -> Option<Origin> {
    let text = std::fs::read_to_string(dir.join(RECORD))
        .or_else(|_| std::fs::read_to_string(dir.join(OLD_RECORD)))
        .ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    Some(Origin {
        source: v["source"].as_str()?.to_string(),
        path_in_source: v["path"].as_str().map(str::to_string),
        installed: v["installed"].as_str().unwrap_or_default().to_string(),
    })
}

/// The folder a skill installs to under `skills_root` (`~/.claude/skills` or a
/// project's `.claude/skills`): its own folder's name.
pub fn skill_target(skill: &FoundSkill, skills_root: &Path) -> PathBuf {
    let folder = skill
        .dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && n != "." && name_problem(n).is_none())
        .unwrap_or_else(|| skill.name.clone());
    skills_root.join(crate::downloads::clean_name(&folder))
}

/// Install `skill` into `skills_root`. `replace` overwrites one already there.
pub fn install_skill(
    skill: &FoundSkill,
    source: &Source,
    source_root: &Path,
    skills_root: &Path,
    replace: bool,
) -> Result<PathBuf, String> {
    if let Some(p) = &skill.problem {
        return Err(format!("{} can't be installed: {p}", skill.name));
    }
    let to = skill_target(skill, skills_root);
    place(&skill.dir, &to, replace)?;
    let rel = skill
        .dir
        .strip_prefix(source_root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|p| !p.is_empty());
    write_record(
        &to,
        &Origin {
            source: source.label(),
            path_in_source: rel,
            installed: now(),
        },
    )?;
    Ok(to)
}

/// Every file of `dir` (its path inside, and its bytes), for an upload.
fn dir_files(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut rel = vec![];
    files_in(dir, dir, &mut rel);
    rel.sort();
    rel.into_iter()
        .map(|r| {
            std::fs::read(dir.join(&r))
                .map(|b| (r.clone(), b))
                .map_err(|e| format!("{r}: {e}"))
        })
        .collect()
}

/// Install `skill` on a remote (WSL, SSH): into its `~/.claude/skills`
/// (`user`), else the remote project's `.claude/skills`. Returns where it went.
pub fn install_skill_remote(
    skill: &FoundSkill,
    source: &Source,
    source_root: &Path,
    remote: &crate::remote::Remote,
    user: bool,
    replace: bool,
    extra: &crate::remote::Extra,
) -> Result<String, String> {
    if let Some(p) = &skill.problem {
        return Err(format!("{} can't be installed: {p}", skill.name));
    }
    let folder = skill_target(skill, Path::new(""))
        .to_string_lossy()
        .into_owned();
    let root = if user {
        "~/.claude/skills".to_string()
    } else {
        format!("{}/.claude/skills", remote.path.trim_end_matches('/'))
    };
    let dest = format!("{root}/{folder}");
    let mut files = dir_files(&skill.dir)?;
    files.retain(|(r, _)| !is_record(r));
    let origin = Origin {
        source: source.label(),
        path_in_source: skill
            .dir
            .strip_prefix(source_root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .filter(|p| !p.is_empty()),
        installed: now(),
    };
    let record = json!({"source": origin.source, "path": origin.path_in_source, "installed": origin.installed});
    files.push((
        RECORD.to_string(),
        serde_json::to_vec_pretty(&record).unwrap_or_default(),
    ));
    crate::remote::put_dir(&remote.target, &dest, &files, replace, extra)
        .map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Install again, from where it came, the skill KodAct installed at `dir`.
pub fn update_skill(dir: &Path) -> Result<(), String> {
    let o = origin(dir)
        .ok_or("KodAct didn't install this skill, so it doesn't know where it came from.")?;
    let source = parse_source(&o.source, Path::new("/"))?;
    let work = work_dir();
    let result = (|| {
        let root = fetch(&source, &work)?;
        let from = match &o.path_in_source {
            Some(p) => crate::stage::safe_entry(&root, p).filter(|d| d.join("SKILL.md").is_file()),
            None => Some(root.clone()).filter(|d| d.join("SKILL.md").is_file()),
        };
        let name = read_skill(dir).map(|s| s.name);
        let skill = match from {
            Some(d) => read_skill(&d),
            None => scan(&root)
                .skills
                .into_iter()
                .find(|s| Some(&s.name) == name.as_ref()),
        }
        .ok_or_else(|| format!("{} no longer has this skill", o.source))?;
        if let Some(p) = &skill.problem {
            return Err(format!("the new version can't be installed: {p}"));
        }
        place(&skill.dir, dir, true)?;
        write_record(
            dir,
            &Origin {
                installed: now(),
                ..o.clone()
            },
        )
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Remove an installed skill or plugin folder.
pub fn uninstall(dir: &Path) -> Result<(), String> {
    std::fs::remove_dir_all(dir).map_err(|e| format!("couldn't remove {}: {e}", dir.display()))
}

// ---------------------------------------------------------------- marketplaces

/// A Claude Code plugin marketplace: `.claude-plugin/marketplace.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Marketplace {
    pub name: String,
    pub description: String,
    pub plugins: Vec<MarketPlugin>,
}

/// One plugin a marketplace offers.
#[derive(Clone, Debug, PartialEq)]
pub struct MarketPlugin {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub source: PluginSource,
    /// The entry as written: with `strict: false` it is the plugin's manifest.
    pub entry: Value,
}

/// Where a marketplace's plugin lives.
#[derive(Clone, Debug, PartialEq)]
pub enum PluginSource {
    /// A folder in the marketplace's own repository.
    Relative(String),
    /// Somewhere else: a repository, optionally one folder of it.
    Remote(Source),
}

pub fn read_marketplace(root: &Path) -> Result<Marketplace, String> {
    let file = root.join(".claude-plugin").join("marketplace.json");
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("marketplace.json: {e}"))?;
    let plugin_root = v["metadata"]["pluginRoot"]
        .as_str()
        .unwrap_or("")
        .trim_matches('/')
        .to_string();
    let mut plugins = vec![];
    for p in v["plugins"].as_array().into_iter().flatten() {
        let Some(name) = p["name"].as_str() else {
            continue;
        };
        let source = match &p["source"] {
            // `./path` is from the marketplace's root; a bare name is under `metadata.pluginRoot`.
            Value::String(s) if !s.contains("://") && !s.starts_with("git@") => {
                let rel = match s.strip_prefix("./") {
                    Some(r) => r.to_string(),
                    None if plugin_root.is_empty() => s.clone(),
                    None => format!("{plugin_root}/{s}"),
                };
                PluginSource::Relative(rel)
            }
            Value::String(url) => PluginSource::Remote(Source::Git {
                url: url.clone(),
                git_ref: None,
            }),
            Value::Object(o) => {
                let git_ref = o
                    .get("ref")
                    .or_else(|| o.get("sha"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let path = o
                    .get("path")
                    .and_then(Value::as_str)
                    .map(|p| p.trim_matches('/').to_string());
                match o.get("source").and_then(Value::as_str) {
                    Some("github") => {
                        let Some((owner, repo)) = o
                            .get("repo")
                            .and_then(Value::as_str)
                            .and_then(|r| r.split_once('/'))
                        else {
                            continue;
                        };
                        PluginSource::Remote(Source::GitHub {
                            owner: owner.to_string(),
                            repo: repo.to_string(),
                            git_ref,
                            path,
                        })
                    }
                    Some("url" | "git" | "git-subdir") => {
                        let Some(url) = o.get("url").and_then(Value::as_str) else {
                            continue;
                        };
                        // A GitHub URL downloads without git.
                        match parse_source(url, Path::new("/")) {
                            Ok(Source::GitHub { owner, repo, .. }) => {
                                PluginSource::Remote(Source::GitHub {
                                    owner,
                                    repo,
                                    git_ref,
                                    path,
                                })
                            }
                            _ => PluginSource::Remote(Source::Git {
                                url: url.to_string(),
                                git_ref,
                            }),
                        }
                    }
                    _ => continue,
                }
            }
            _ => continue,
        };
        plugins.push(MarketPlugin {
            name: name.to_string(),
            description: p["description"].as_str().unwrap_or_default().to_string(),
            version: p["version"].as_str().map(str::to_string),
            source,
            entry: p.clone(),
        });
    }
    let folder = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Marketplace {
        name: v["name"].as_str().unwrap_or(&folder).to_string(),
        description: v["description"]
            .as_str()
            .or(v["metadata"]["description"].as_str())
            .unwrap_or_default()
            .to_string(),
        plugins,
    })
}

/// The folder KodAct installs plugins into: `plugins/<marketplace>/<plugin>`
/// under KodAct CLI's config folder.
pub fn plugins_root() -> Option<PathBuf> {
    crate::forge_files::cli_home().map(|h| h.join("plugins"))
}

/// Install `plugin` of `market` (fetched at `market_root`) under `plugins_root`,
/// using `work` for downloads. Returns the plugin's folder (for `pluginDirs`).
pub fn install_plugin(
    plugin: &MarketPlugin,
    market: &Marketplace,
    market_root: &Path,
    market_source: &str,
    plugins_root: &Path,
    work: &Path,
) -> Result<PathBuf, String> {
    let from = match &plugin.source {
        PluginSource::Relative(rel) => crate::stage::safe_entry(market_root, rel)
            .filter(|p| p.is_dir())
            .ok_or_else(|| format!("the marketplace has no folder {rel} for {}", plugin.name))?,
        PluginSource::Remote(src) => fetch(src, work)?,
    };
    let to = plugins_root
        .join(crate::downloads::clean_name(&market.name))
        .join(crate::downloads::clean_name(&plugin.name));
    place(&from, &to, true)?;
    // No manifest of its own (`strict: false`): the marketplace entry is its manifest.
    if read_plugin(&to).is_none() || plugin.entry["strict"] == Value::Bool(false) {
        let mut manifest = plugin.entry.clone();
        if let Some(o) = manifest.as_object_mut() {
            o.remove("source");
            o.remove("strict");
        }
        let dir = to.join(".claude-plugin");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        if !dir.join("plugin.json").is_file() || plugin.entry["strict"] == Value::Bool(false) {
            std::fs::write(
                dir.join("plugin.json"),
                serde_json::to_string_pretty(&manifest).unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    write_record(
        &to,
        &Origin {
            source: market_source.to_string(),
            path_in_source: Some(format!("{}@{}", plugin.name, market.name)),
            installed: now(),
        },
    )?;
    Ok(to)
}

/// Add `dir` to your `pluginDirs` (KodAct CLI's user settings), once.
pub fn add_plugin_dir(project: &Path, dir: &Path) -> Result<(), String> {
    use crate::forge_files::{set_setting, setting, Layer};
    let mut dirs: Vec<Value> = setting(project, Layer::User, "pluginDirs")?
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let d = dir.display().to_string();
    if !dirs.iter().any(|v| v.as_str() == Some(d.as_str())) {
        dirs.push(Value::String(d));
        set_setting(project, Layer::User, "pluginDirs", Some(Value::Array(dirs)))?;
    }
    Ok(())
}

/// Take `dir` out of your `pluginDirs`.
pub fn remove_plugin_dir(project: &Path, dir: &Path) -> Result<(), String> {
    use crate::forge_files::{set_setting, setting, Layer};
    let d = dir.display().to_string();
    let dirs: Vec<Value> = setting(project, Layer::User, "pluginDirs")?
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|v| v.as_str() != Some(d.as_str()))
        .collect();
    set_setting(
        project,
        Layer::User,
        "pluginDirs",
        (!dirs.is_empty()).then_some(Value::Array(dirs)),
    )
}

/// A plugin found on its own (not in a marketplace): copied to
/// `plugins/local/<name>` and added to your `pluginDirs`.
pub fn install_local_plugin(
    plugin: &FoundPlugin,
    source: &str,
    plugins_root: &Path,
    project: &Path,
) -> Result<(), String> {
    let to = plugins_root
        .join("local")
        .join(crate::downloads::clean_name(&plugin.name));
    place(&plugin.dir, &to, true)?;
    write_record(
        &to,
        &Origin {
            source: source.to_string(),
            path_in_source: None,
            installed: now(),
        },
    )?;
    add_plugin_dir(project, &to)
}

/// The marketplaces you added: `marketplaces.json` in KodAct's config folder,
/// `[{ "name": …, "source": … }]`.
pub fn known_marketplaces() -> Vec<(String, String)> {
    let Some(file) = marketplaces_file() else {
        return vec![];
    };
    let v: Value = std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some((
                m["name"].as_str()?.to_string(),
                m["source"].as_str()?.to_string(),
            ))
        })
        .collect()
}

fn marketplaces_file() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("marketplaces.json"))
}

/// Remember (or, with `None`, forget) a marketplace by name.
pub fn remember_marketplace(name: &str, source: Option<&str>) -> Result<(), String> {
    let file = marketplaces_file().ok_or("KodAct's config folder is unknown")?;
    let mut list = known_marketplaces();
    list.retain(|(n, _)| n != name);
    if let Some(s) = source {
        list.push((name.to_string(), s.to_string()));
    }
    let v: Vec<Value> = list
        .iter()
        .map(|(n, s)| json!({"name": n, "source": s}))
        .collect();
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, serde_json::to_string_pretty(&v).unwrap_or_default())
        .map_err(|e| e.to_string())
}

/// A fresh folder for one install's downloads, under the system's temp folder.
pub fn work_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "forge-install-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("forge-si-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_every_way_people_name_a_source() {
        let cwd = tmp("parse");
        let gh = |o: &str, r: &str, git_ref: Option<&str>, path: Option<&str>| Source::GitHub {
            owner: o.into(),
            repo: r.into(),
            git_ref: git_ref.map(str::to_string),
            path: path.map(str::to_string),
        };
        assert_eq!(
            parse_source("cathrynlavery/diagram-design", &cwd).unwrap(),
            gh("cathrynlavery", "diagram-design", None, None)
        );
        assert_eq!(
            parse_source("https://github.com/cathrynlavery/diagram-design.git", &cwd).unwrap(),
            gh("cathrynlavery", "diagram-design", None, None)
        );
        assert_eq!(
            parse_source(
                "https://github.com/anthropics/skills/tree/main/skills/pdf",
                &cwd
            )
            .unwrap(),
            gh("anthropics", "skills", Some("main"), Some("skills/pdf"))
        );
        assert_eq!(
            parse_source(
                "github.com/anthropics/skills/blob/v2/skills/pdf/SKILL.md",
                &cwd
            )
            .unwrap(),
            gh("anthropics", "skills", Some("v2"), Some("skills/pdf"))
        );
        assert_eq!(
            parse_source("https://gitlab.com/team/skills.git#stable", &cwd).unwrap(),
            Source::Git {
                url: "https://gitlab.com/team/skills.git".into(),
                git_ref: Some("stable".into())
            }
        );
        std::fs::create_dir_all(cwd.join("local-skill")).unwrap();
        std::fs::write(cwd.join("pack.skill"), b"x").unwrap();
        assert_eq!(
            parse_source("local-skill", &cwd).unwrap(),
            Source::Folder(cwd.join("local-skill"))
        );
        assert_eq!(
            parse_source("pack.skill", &cwd).unwrap(),
            Source::Archive(cwd.join("pack.skill"))
        );
        assert!(parse_source("not a thing", &cwd).is_err());
        assert!(parse_source("", &cwd).is_err());
        assert_eq!(
            gh("anthropics", "skills", Some("main"), Some("skills/pdf")).label(),
            "https://github.com/anthropics/skills/tree/main/skills/pdf"
        );
        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn finds_skills_and_what_they_run() {
        let root = tmp("scan");
        let skill = root.join("skills/pdf");
        std::fs::create_dir_all(skill.join("scripts")).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: pdf\ndescription: Read PDFs\nallowed-tools: Bash(python3 ${CLAUDE_SKILL_DIR}/scripts/x.py *) Read\n---\nPages: !`pdfinfo $0`\nKEY=!`date`\n```!\necho hi\n```\n",
        )
        .unwrap();
        std::fs::write(skill.join("scripts/x.py"), "print(1)").unwrap();
        std::fs::write(skill.join("REFERENCE.md"), "ref").unwrap();
        let other = root.join(".claude/skills/terse");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("SKILL.md"), "# Tidy imports\nSort them.").unwrap();
        std::fs::create_dir_all(root.join("node_modules/dep/skills/nope")).unwrap();
        std::fs::write(
            root.join("node_modules/dep/skills/nope/SKILL.md"),
            "---\ndescription: x\n---\n",
        )
        .unwrap();
        let found = scan(&root);
        let names: Vec<&str> = found.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["terse", "pdf"]);
        let pdf = &found.skills[1];
        assert_eq!(pdf.commands, ["pdfinfo $0", "echo hi"]);
        assert_eq!(pdf.scripts, ["scripts/x.py"]);
        assert_eq!(
            pdf.allowed_tools,
            ["Bash(python3 ${CLAUDE_SKILL_DIR}/scripts/x.py *)", "Read"]
        );
        assert_eq!((pdf.files, pdf.problem.as_deref()), (3, None));
        assert_eq!(found.skills[0].description, "Tidy imports");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn installs_records_and_never_overwrites_unasked() {
        let root = tmp("install");
        let src = root.join("src/diagram-design");
        std::fs::create_dir_all(src.join("assets")).unwrap();
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: diagram-design\ndescription: Diagrams\n---\nDraw.",
        )
        .unwrap();
        std::fs::write(src.join("assets/t.html"), "<svg/>").unwrap();
        let skill = read_skill(&src).unwrap();
        let dest = root.join("home/.claude/skills");
        let source = Source::GitHub {
            owner: "cathrynlavery".into(),
            repo: "diagram-design".into(),
            git_ref: None,
            path: None,
        };
        let to = install_skill(&skill, &source, &root.join("src"), &dest, false).unwrap();
        assert_eq!(to, dest.join("diagram-design"));
        assert!(to.join("SKILL.md").is_file() && to.join("assets/t.html").is_file());
        let o = origin(&to).unwrap();
        assert_eq!(o.source, "https://github.com/cathrynlavery/diagram-design");
        assert_eq!(o.path_in_source.as_deref(), Some("diagram-design"));
        assert!(install_skill(&skill, &source, &root, &dest, false)
            .unwrap_err()
            .contains("already installed"));
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: diagram-design\ndescription: Diagrams v2\n---\nDraw.",
        )
        .unwrap();
        install_skill(&read_skill(&src).unwrap(), &source, &root, &dest, true).unwrap();
        assert!(std::fs::read_to_string(to.join("SKILL.md"))
            .unwrap()
            .contains("v2"));
        // A name Claude Code refuses is not installed.
        let bad = FoundSkill {
            problem: name_problem("a<b>"),
            ..skill.clone()
        };
        assert!(install_skill(&bad, &source, &root, &dest, false).is_err());
        uninstall(&to).unwrap();
        assert!(!to.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_zip_of_a_skill_installs_and_its_entries_stay_inside() {
        let root = tmp("zip");
        let file = root.join("pack.skill");
        {
            let f = std::fs::File::create(&file).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            z.start_file("pack/SKILL.md", opts).unwrap();
            std::io::Write::write_all(&mut z, b"---\nname: pack\ndescription: Packed\n---\nGo.")
                .unwrap();
            z.start_file("../../escape.txt", opts).unwrap();
            std::io::Write::write_all(&mut z, b"no").unwrap();
            z.finish().unwrap();
        }
        let work = root.join("work");
        let got = fetch(&Source::Archive(file), &work).unwrap();
        let found = scan(&got);
        assert_eq!(found.skills.len(), 1);
        assert_eq!(found.skills[0].name, "pack");
        assert!(!root.join("escape.txt").exists() && !work.join("escape.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_a_marketplace_and_installs_its_plugins() {
        let root = tmp("market");
        let m = root.join("market");
        std::fs::create_dir_all(m.join(".claude-plugin")).unwrap();
        std::fs::create_dir_all(m.join("plugins/fmt/skills/fmt")).unwrap();
        std::fs::create_dir_all(m.join("loose")).unwrap();
        std::fs::write(
            m.join("plugins/fmt/skills/fmt/SKILL.md"),
            "---\ndescription: Format\n---\nfmt",
        )
        .unwrap();
        std::fs::write(m.join("loose/README.md"), "x").unwrap();
        std::fs::write(
            m.join(".claude-plugin/marketplace.json"),
            r#"{"name": "team-tools", "owner": {"name": "Team"}, "plugins": [
                {"name": "fmt", "source": "./plugins/fmt", "description": "Formatting", "version": "1.2.0"},
                {"name": "loose", "source": "./loose", "strict": false, "description": "No manifest", "skills": ["./skills"]},
                {"name": "remote", "source": {"source": "github", "repo": "acme/remote-plugin", "ref": "v1"}},
                {"name": "gitlab", "source": {"source": "url", "url": "https://gitlab.com/acme/p.git"}}
            ]}"#,
        )
        .unwrap();
        let market = read_marketplace(&m).unwrap();
        assert_eq!(market.name, "team-tools");
        let names: Vec<&str> = market.plugins.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["fmt", "loose", "remote", "gitlab"]);
        assert_eq!(
            market.plugins[2].source,
            PluginSource::Remote(Source::GitHub {
                owner: "acme".into(),
                repo: "remote-plugin".into(),
                git_ref: Some("v1".into()),
                path: None
            })
        );
        let plugins = root.join("plugins");
        let fmt = install_plugin(
            &market.plugins[0],
            &market,
            &m,
            "acme/team-tools",
            &plugins,
            &root.join("w"),
        )
        .unwrap();
        assert_eq!(fmt, plugins.join("team-tools/fmt"));
        assert!(fmt.join("skills/fmt/SKILL.md").is_file());
        // Its manifest comes from the marketplace entry when it has none.
        let loose = install_plugin(
            &market.plugins[1],
            &market,
            &m,
            "acme/team-tools",
            &plugins,
            &root.join("w"),
        )
        .unwrap();
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(loose.join(".claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            (manifest["name"].as_str(), manifest["skills"][0].as_str()),
            (Some("loose"), Some("./skills"))
        );
        assert!(manifest.get("source").is_none());
        assert_eq!(
            origin(&fmt).unwrap().path_in_source.as_deref(),
            Some("fmt@team-tools")
        );
        assert!(scan(&m).marketplace.is_some() && scan(&m).skills.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
