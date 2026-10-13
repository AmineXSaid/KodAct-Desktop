//! ForgeCLI's own configuration files, read and written the way ForgeCLI
//! reads them, so what the Settings editors save is what the next chat's
//! engine loads. Each kind mirrors one loader in ForgeCLI:
//!
//! - **Slash commands**: `commands/**/*.md` (a file in a subfolder is
//!   `dir:name`), frontmatter `description`, `argument-hint`, then the prompt
//!   (`forge-agents/src/commands.rs`).
//! - **Agents**: `agents/*.md`, frontmatter `name`, `description` (both
//!   required), `tools` (none = every tool), `model`, then the system prompt
//!   (`forge-agents/src/definitions.rs`).
//! - **Skills**: `skills/<name>/SKILL.md`, frontmatter `name`, `description`
//!   (required) (`forge-agents/src/skills.rs`). ForgeCLI has no setting that
//!   turns a skill off, so a disabled skill moves to `skills/.disabled/`,
//!   which its loader never opens.
//! - **Hooks**: the `hooks` key of a settings file:
//!   `{"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command",
//!   "command": "…", "timeout": 60}]}]}` (`forge-hooks`).
//! - **MCP servers**: `mcpServers` in user or local settings, or the
//!   project's `.mcp.json`, which ForgeCLI runs only once the user trusts it
//!   (`enabledMcpjsonServers` in local settings) (`forge-mcp/src/config.rs`).
//! - **Permissions**: `permissions.allow`, `.ask`, `.deny` and `.defaultMode`
//!   in a settings file (`forge-permissions`).
//!
//! The folders (`forge-config`): the user's are under ForgeCLI's config
//! folder ([`cli_home`]); a project's are `.forge/<kind>` (ForgeCLI also reads
//! `.agents/<kind>`, listed here too). Settings files: the user's
//! `settings.json`, the project's `.forge/settings.json` (shared) and
//! `.forge/settings.local.json` (this computer only).
//!
//! A settings file that isn't a JSON object is never overwritten: saving
//! reports it instead. Everything here is plain file I/O; callers run it off
//! the UI thread.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

// ---------------------------------------------------------------- where

/// ForgeCLI's configuration folder, as `forge_config::config_dir` finds it:
/// `FORGE_HOME`, else `$XDG_CONFIG_HOME/forge`, else the platform's config
/// folder (`%APPDATA%\forge`, `~/Library/Application Support/forge`,
/// `~/.config/forge`).
pub fn cli_home() -> Option<PathBuf> {
    let abs = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    if let Some(h) = abs("FORGE_HOME") {
        return Some(h);
    }
    if let Some(x) = abs("XDG_CONFIG_HOME") {
        return Some(x.join("forge"));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let platform = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else {
        home().map(|h| h.join(".config"))
    };
    platform.map(|d| d.join("forge"))
}

/// Where a Markdown resource lives.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Scope {
    /// ForgeCLI's config folder: every project.
    User,
    /// The project's `.forge/` (or `.agents/`).
    Project,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::User => "You",
            Scope::Project => "Project",
        }
    }
}

/// The folder a scope writes `kind` (`commands`, `agents`, `skills`) to.
pub fn kind_dir(kind: &str, scope: Scope, project: &Path) -> Option<PathBuf> {
    match scope {
        Scope::User => cli_home().map(|h| h.join(kind)),
        Scope::Project => Some(project.join(".forge").join(kind)),
    }
}

/// Every folder ForgeCLI reads `kind` from, lowest precedence first.
fn read_dirs(kind: &str, project: &Path) -> Vec<(Scope, PathBuf)> {
    let mut out = vec![];
    if let Some(h) = cli_home() {
        out.push((Scope::User, h.join(kind)));
    }
    out.push((Scope::Project, project.join(".agents").join(kind)));
    out.push((Scope::Project, project.join(".forge").join(kind)));
    out
}

/// A settings file ForgeCLI layers (later wins): user, project, local.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Layer {
    User,
    Project,
    Local,
}

impl Layer {
    pub const ALL: [Layer; 3] = [Layer::User, Layer::Project, Layer::Local];

    pub fn label(self) -> &'static str {
        match self {
            Layer::User => "You",
            Layer::Project => "Project",
            Layer::Local => "This PC",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Layer::User => "Every project, on this computer",
            Layer::Project => "This project, shared with everyone (.forge/settings.json)",
            Layer::Local => "This project, this computer only (.forge/settings.local.json)",
        }
    }

    pub fn path(self, project: &Path) -> Option<PathBuf> {
        match self {
            Layer::User => cli_home().map(|h| h.join("settings.json")),
            Layer::Project => Some(project.join(".forge").join("settings.json")),
            Layer::Local => Some(project.join(".forge").join("settings.local.json")),
        }
    }
}

// ---------------------------------------------------------------- settings files

/// A settings file for editing: missing or empty is `{}`; anything but a
/// JSON object is an error, so a save never wipes what it can't read.
pub fn read_json(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(format!("{} is not a JSON object", path.display())),
        Err(e) => Err(format!(
            "{} is not valid JSON ({e}); fix it, then try again",
            path.display()
        )),
    }
}

/// Write a file whole: a temporary file renamed over it, so a crash never
/// leaves it half written.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = target.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).map_err(err)?;
    std::fs::rename(&tmp, &target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        err(e)
    })
}

/// Change a settings file in place, keeping every key `f` doesn't touch.
pub fn edit_json(path: &Path, f: impl FnOnce(&mut Map<String, Value>)) -> Result<(), String> {
    let mut root = read_json(path)?;
    f(&mut root);
    let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    write_atomic(path, &(text + "\n"))
}

// ---------------------------------------------------------------- Markdown + frontmatter

/// A Markdown file with `---` frontmatter, kept in the order written so an
/// edit changes only the keys it sets. Values are one line; ForgeCLI's
/// parser also reads lists and folded blocks, which are kept as written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    /// Each key with its raw text: the rest of its line, and any indented
    /// or `- item` lines under it.
    front: Vec<(String, String)>,
    pub body: String,
}

impl Doc {
    pub fn parse(text: &str) -> Doc {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let open = text
            .strip_prefix("---\n")
            .or_else(|| text.strip_prefix("---\r\n"));
        let Some(rest) = open else {
            return Doc {
                front: vec![],
                body: text.to_string(),
            };
        };
        let (front, body) = match rest.find("\n---") {
            Some(end) => (&rest[..end], &rest[end + 4..]),
            None if rest.starts_with("---") => ("", &rest[3..]),
            None => {
                return Doc {
                    front: vec![],
                    body: text.to_string(),
                }
            }
        };
        let body = body
            .trim_start_matches('-')
            .trim_start_matches(['\r', '\n']);
        let mut out: Vec<(String, String)> = vec![];
        for line in front.lines() {
            let cont = line.starts_with(' ') || line.starts_with('\t') || line.starts_with("- ");
            match out.last_mut() {
                Some((_, raw)) if cont => {
                    raw.push('\n');
                    raw.push_str(line);
                }
                _ => {
                    if let Some((k, v)) = line.split_once(':') {
                        out.push((k.trim().to_string(), v.trim().to_string()));
                    }
                }
            }
        }
        Doc {
            front: out,
            body: body.to_string(),
        }
    }

    /// A one-line value, unquoted; a list or block reads as its items joined.
    pub fn get(&self, key: &str) -> Option<String> {
        let raw = &self.front.iter().find(|(k, _)| k == key)?.1;
        let mut lines = raw.lines();
        let first = lines.next().unwrap_or("").trim();
        let rest: Vec<String> = lines
            .map(|l| unquote(l.trim().trim_start_matches("- ").trim()))
            .filter(|l| !l.is_empty())
            .collect();
        if rest.is_empty() {
            Some(unquote(first))
        } else if first.starts_with('>') {
            Some(rest.join(" "))
        } else if first.starts_with('|') {
            Some(rest.join("\n"))
        } else {
            Some(rest.join(", "))
        }
    }

    /// A list value: `[a, b]`, `- a` lines, or `a, b`.
    pub fn list(&self, key: &str) -> Option<Vec<String>> {
        let v = self.get(key)?;
        let inner = v
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(&v);
        Some(
            inner
                .split(',')
                .map(|s| unquote(s.trim()))
                .filter(|s| !s.is_empty())
                .collect(),
        )
    }

    /// Set a one-line value (`None` or empty removes the key).
    pub fn set(&mut self, key: &str, value: Option<&str>) {
        let value = value
            .map(|v| v.replace(['\r', '\n'], " "))
            .filter(|v| !v.trim().is_empty());
        let pos = self.front.iter().position(|(k, _)| k == key);
        match (value, pos) {
            (Some(v), Some(i)) => self.front[i].1 = quote(v.trim()),
            (Some(v), None) => self.front.push((key.to_string(), quote(v.trim()))),
            (None, Some(i)) => {
                self.front.remove(i);
            }
            (None, None) => {}
        }
    }

    /// Set a list as `[a, b]`.
    pub fn set_list(&mut self, key: &str, items: Option<&[String]>) {
        let pos = self.front.iter().position(|(k, _)| k == key);
        let raw = items.map(|i| format!("[{}]", i.join(", ")));
        match (raw, pos) {
            (Some(v), Some(i)) => self.front[i].1 = v,
            (Some(v), None) => self.front.push((key.to_string(), v)),
            (None, Some(i)) => {
                self.front.remove(i);
            }
            (None, None) => {}
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        if !self.front.is_empty() {
            out.push_str("---\n");
            for (k, v) in &self.front {
                if v.is_empty() {
                    out.push_str(&format!("{k}:\n"));
                } else if v.starts_with('\n') {
                    out.push_str(&format!("{k}:{v}\n"));
                } else {
                    out.push_str(&format!("{k}: {v}\n"));
                }
            }
            out.push_str("---\n");
        }
        out.push_str(self.body.trim_end());
        out.push('\n');
        out
    }
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    let quoted = v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
    if quoted {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// Quote a value when ForgeCLI's YAML subset would read it as something
/// else (a list, a block, a comment). Its parser only strips the outer
/// quotes, so inner ones need no escaping.
fn quote(v: &str) -> String {
    let special = v.starts_with([
        '[', '{', '>', '|', '#', '"', '\'', '-', '!', '&', '*', '@', '%', '`',
    ]) || v.contains(": ")
        || v.contains(" #")
        || v.ends_with(':');
    if special {
        format!("\"{v}\"")
    } else {
        v.to_string()
    }
}

/// A name ForgeCLI can file: letters, digits, `-` and `_` (and `:` for a
/// command in a subfolder).
pub fn check_name(name: &str, allow_colon: bool) -> Result<(), String> {
    if name.is_empty() {
        return Err("Give it a name.".into());
    }
    if name.len() > 64 {
        return Err("Keep the name under 64 characters.".into());
    }
    let ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || (allow_colon && c == ':'));
    if !ok || name.starts_with([':', '-', '.']) || name.ends_with(':') || name.contains("::") {
        return Err(if allow_colon {
            "Use letters, digits, - and _ (and : for a subfolder)."
        } else {
            "Use letters, digits, - and _."
        }
        .into());
    }
    Ok(())
}

fn md_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            if depth < 6 {
                md_files(&p, out, depth + 1);
            }
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p);
        }
    }
}

fn first_line(body: &str) -> String {
    body.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(100).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- slash commands

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub scope: Scope,
    /// `name`, or `dir:name` for a file in a subfolder.
    pub name: String,
    pub description: String,
    /// The `description:` as written (empty when the first line stands in).
    pub own_description: String,
    pub hint: String,
    pub body: String,
    pub path: PathBuf,
    /// A later folder has a command with the same name, which wins.
    pub shadowed: bool,
}

pub fn commands(project: &Path, problems: &mut Vec<String>) -> Vec<Command> {
    let mut out: Vec<Command> = vec![];
    for (scope, dir) in read_dirs("commands", project) {
        let mut files = vec![];
        md_files(&dir, &mut files, 0);
        for f in files {
            let rel = f.strip_prefix(&dir).unwrap_or(&f).with_extension("");
            let name = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(":");
            let text = match std::fs::read_to_string(&f) {
                Ok(t) => t,
                Err(e) => {
                    problems.push(format!("{}: {e}", f.display()));
                    continue;
                }
            };
            let doc = Doc::parse(&text);
            let own = doc.get("description").unwrap_or_default();
            for c in out.iter_mut().filter(|c| c.name == name) {
                c.shadowed = true;
            }
            out.push(Command {
                scope,
                description: if own.is_empty() {
                    first_line(&doc.body)
                } else {
                    own.clone()
                },
                own_description: own,
                hint: doc.get("argument-hint").unwrap_or_default(),
                body: doc.body,
                name,
                path: f,
                shadowed: false,
            });
        }
    }
    out
}

/// A command's file: `dir:name` is `dir/name.md`.
pub fn command_path(scope: Scope, project: &Path, name: &str) -> Option<PathBuf> {
    let mut p = kind_dir("commands", scope, project)?;
    for part in name.split(':') {
        p.push(part);
    }
    Some(p.with_extension("md"))
}

/// Save a command (keeping other frontmatter of the file it replaces); a
/// rename removes the old file.
pub fn save_command(
    scope: Scope,
    project: &Path,
    old: Option<&Path>,
    name: &str,
    description: &str,
    hint: &str,
    body: &str,
) -> Result<PathBuf, String> {
    check_name(name, true)?;
    if body.trim().is_empty() {
        return Err("Write the prompt the command sends.".into());
    }
    let path = command_path(scope, project, name).ok_or("ForgeCLI's config folder is unknown")?;
    if old != Some(path.as_path()) && path.exists() {
        return Err(format!("/{name} already exists there."));
    }
    let mut doc = old
        .and_then(|o| std::fs::read_to_string(o).ok())
        .map(|t| Doc::parse(&t))
        .unwrap_or_default();
    doc.set("description", Some(description));
    doc.set("argument-hint", Some(hint));
    doc.body = body.trim().to_string();
    write_atomic(&path, &doc.render())?;
    if let Some(o) = old.filter(|o| *o != path) {
        let _ = std::fs::remove_file(o);
    }
    Ok(path)
}

// ---------------------------------------------------------------- agents

#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    pub scope: Scope,
    pub name: String,
    pub description: String,
    /// `None`: every tool but Task.
    pub tools: Option<Vec<String>>,
    pub model: String,
    pub prompt: String,
    pub path: PathBuf,
    /// Why ForgeCLI would skip this file, if it would.
    pub problem: Option<String>,
}

/// The tools ForgeCLI gives an agent to choose from (`forge-tools`).
pub const TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "MultiEdit",
    "Glob",
    "Grep",
    "LS",
    "Bash",
    "BashOutput",
    "KillShell",
    "WebFetch",
    "WebSearch",
    "TodoWrite",
    "NotebookEdit",
    "Skill",
    "AskUserQuestion",
];

pub fn agents(project: &Path, problems: &mut Vec<String>) -> Vec<Agent> {
    let mut out = vec![];
    for (scope, dir) in read_dirs("agents", project) {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        for f in files {
            let text = match std::fs::read_to_string(&f) {
                Ok(t) => t,
                Err(e) => {
                    problems.push(format!("{}: {e}", f.display()));
                    continue;
                }
            };
            let doc = Doc::parse(&text);
            let name = doc.get("name").unwrap_or_default();
            let description = doc.get("description").unwrap_or_default();
            let problem = if !text.starts_with("---") {
                Some("no --- frontmatter, so ForgeCLI skips it".to_string())
            } else if name.is_empty() {
                Some("no name, so ForgeCLI skips it".to_string())
            } else if description.is_empty() {
                Some("no description, so ForgeCLI skips it".to_string())
            } else {
                None
            };
            out.push(Agent {
                scope,
                name: if name.is_empty() {
                    f.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                } else {
                    name
                },
                description,
                tools: doc.list("tools").filter(|t| !t.is_empty()),
                model: doc
                    .get("model")
                    .filter(|m| m != "inherit")
                    .unwrap_or_default(),
                prompt: doc.body,
                path: f,
                problem,
            });
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn save_agent(
    scope: Scope,
    project: &Path,
    old: Option<&Path>,
    name: &str,
    description: &str,
    tools: Option<&[String]>,
    model: &str,
    prompt: &str,
) -> Result<PathBuf, String> {
    check_name(name, false)?;
    if description.trim().is_empty() {
        return Err("Say when to use it: ForgeCLI shows the description to the model.".into());
    }
    if prompt.trim().is_empty() {
        return Err("Write the agent's instructions.".into());
    }
    if tools.is_some_and(|t| t.is_empty()) {
        return Err("Pick at least one tool, or allow every tool.".into());
    }
    let path = kind_dir("agents", scope, project)
        .ok_or("ForgeCLI's config folder is unknown")?
        .join(format!("{name}.md"));
    if old != Some(path.as_path()) && path.exists() {
        return Err(format!(
            "An agent file named {name}.md already exists there."
        ));
    }
    let mut doc = old
        .and_then(|o| std::fs::read_to_string(o).ok())
        .map(|t| Doc::parse(&t))
        .unwrap_or_default();
    doc.set("name", Some(name));
    doc.set("description", Some(description));
    doc.set_list("tools", tools);
    doc.set("model", Some(model));
    doc.body = prompt.trim().to_string();
    write_atomic(&path, &doc.render())?;
    if let Some(o) = old.filter(|o| *o != path) {
        let _ = std::fs::remove_file(o);
    }
    Ok(path)
}

// ---------------------------------------------------------------- skills

#[derive(Clone, Debug, PartialEq)]
pub struct Skill {
    pub scope: Scope,
    pub name: String,
    pub description: String,
    pub body: String,
    /// The skill's folder (holding SKILL.md and any files it uses).
    pub dir: PathBuf,
    pub enabled: bool,
    /// Files in the folder besides SKILL.md.
    pub extra_files: usize,
    pub problem: Option<String>,
}

/// Where disabled skills wait: a folder ForgeCLI's loader never opens (its
/// own folder has no SKILL.md).
pub const DISABLED_DIR: &str = ".disabled";

fn count_files(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                count_files(&p)
            } else {
                1
            }
        })
        .sum()
}

fn read_skill(scope: Scope, dir: &Path, enabled: bool) -> Option<Skill> {
    let text = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    let doc = Doc::parse(&text);
    let description = doc.get("description").unwrap_or_default();
    Some(Skill {
        scope,
        name: doc
            .get("name")
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| {
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            }),
        problem: description
            .is_empty()
            .then(|| "no description, so ForgeCLI skips it".to_string()),
        description,
        body: doc.body,
        dir: dir.to_path_buf(),
        enabled,
        extra_files: count_files(dir).saturating_sub(1),
    })
}

fn skill_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return vec![];
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("SKILL.md").is_file())
        .collect();
    v.sort();
    v
}

pub fn skills(project: &Path) -> Vec<Skill> {
    let mut out = vec![];
    for (scope, root) in read_dirs("skills", project) {
        out.extend(
            skill_dirs(&root)
                .iter()
                .filter_map(|d| read_skill(scope, d, true)),
        );
        out.extend(
            skill_dirs(&root.join(DISABLED_DIR))
                .iter()
                .filter_map(|d| read_skill(scope, d, false)),
        );
    }
    out
}

/// Create or edit a skill's SKILL.md (other files in its folder stay).
pub fn save_skill(
    scope: Scope,
    project: &Path,
    old: Option<&Path>,
    name: &str,
    description: &str,
    body: &str,
) -> Result<PathBuf, String> {
    check_name(name, false)?;
    if description.trim().is_empty() {
        return Err(
            "Say what it does and when to use it: the model reads only this to pick it.".into(),
        );
    }
    if body.trim().is_empty() {
        return Err("Write the skill's instructions.".into());
    }
    let dir = match old {
        Some(o) => o.to_path_buf(),
        None => {
            let d = kind_dir("skills", scope, project)
                .ok_or("ForgeCLI's config folder is unknown")?
                .join(name);
            if d.exists() {
                return Err(format!("A skill folder named {name} already exists there."));
            }
            d
        }
    };
    let file = dir.join("SKILL.md");
    let mut doc = std::fs::read_to_string(&file)
        .ok()
        .map(|t| Doc::parse(&t))
        .unwrap_or_default();
    doc.set("name", Some(name));
    doc.set("description", Some(description));
    doc.body = body.trim().to_string();
    write_atomic(&file, &doc.render())?;
    Ok(dir)
}

/// Turn a skill on or off by moving its folder in or out of `.disabled/`.
pub fn set_skill_enabled(dir: &Path, on: bool) -> Result<PathBuf, String> {
    let name = dir.file_name().ok_or("not a skill folder")?;
    let parent = dir.parent().ok_or("not a skill folder")?;
    let in_disabled = parent.file_name().is_some_and(|n| n == DISABLED_DIR);
    let target = match (on, in_disabled) {
        (true, true) => parent.parent().ok_or("not a skill folder")?.join(name),
        (false, false) => parent.join(DISABLED_DIR).join(name),
        _ => return Ok(dir.to_path_buf()),
    };
    if target.exists() {
        return Err(format!(
            "{} already exists, so this one can't move there.",
            target.display()
        ));
    }
    if let Some(p) = target.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::rename(dir, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

// ---------------------------------------------------------------- hooks

/// ForgeCLI's hook events (`forge-hooks`), with what each is for.
pub const HOOK_EVENTS: &[(&str, &str, bool)] = &[
    ("PreToolUse", "Before a tool runs. Exit 2 blocks it.", true),
    ("PostToolUse", "After a tool succeeds.", true),
    ("PostToolUseFailure", "After a tool fails.", true),
    (
        "UserPromptSubmit",
        "When you send a message. Its output joins the context.",
        false,
    ),
    (
        "Stop",
        "When Forge finishes a reply. Exit 2 makes it go on.",
        false,
    ),
    ("SubagentStop", "When a sub-agent finishes.", false),
    (
        "SessionStart",
        "When a chat's engine starts. Its output joins the context.",
        false,
    ),
    ("SessionEnd", "When a chat's engine stops.", false),
    ("PreCompact", "Before the conversation is compacted.", false),
    ("Notification", "When Forge sends a notification.", false),
];

/// Whether an event's hooks take a matcher (a tool name pattern).
pub fn event_takes_matcher(event: &str) -> bool {
    HOOK_EVENTS.iter().any(|(e, _, m)| *e == event && *m)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hook {
    pub layer: Layer,
    pub event: String,
    /// Tool name or regex (`Bash`, `Edit|Write`); empty matches every tool.
    pub matcher: String,
    pub command: String,
    /// Seconds; `None` is ForgeCLI's 60.
    pub timeout: Option<f64>,
}

fn hooks_of(layer: Layer, root: &Map<String, Value>, problems: &mut Vec<String>) -> Vec<Hook> {
    let mut out = vec![];
    let Some(obj) = root.get("hooks").and_then(Value::as_object) else {
        return out;
    };
    for (event, list) in obj {
        if !HOOK_EVENTS.iter().any(|(e, _, _)| e == event) {
            problems.push(format!("Unknown hook event {event} ({}).", layer.label()));
            continue;
        }
        for m in list.as_array().into_iter().flatten() {
            let matcher = m
                .get("matcher")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for h in m
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(command) = h.get("command").and_then(Value::as_str) else {
                    continue;
                };
                out.push(Hook {
                    layer,
                    event: event.clone(),
                    matcher: matcher.clone(),
                    command: command.to_string(),
                    timeout: h.get("timeout").and_then(Value::as_f64),
                });
            }
        }
    }
    out
}

pub fn hooks(project: &Path, problems: &mut Vec<String>) -> Vec<Hook> {
    let mut out = vec![];
    for layer in Layer::ALL {
        let Some(path) = layer.path(project) else {
            continue;
        };
        match read_json(&path) {
            Ok(root) => out.extend(hooks_of(layer, &root, problems)),
            Err(e) => problems.push(e),
        }
    }
    out
}

/// The `hooks` value for a list of hooks: grouped by event, then matcher.
pub fn hooks_json(rows: &[Hook]) -> Value {
    let mut events: Map<String, Value> = Map::new();
    for (event, _, _) in HOOK_EVENTS {
        let mut groups: Vec<(String, Vec<Value>)> = vec![];
        for h in rows.iter().filter(|h| h.event == *event) {
            let mut cmd = json!({"type": "command", "command": h.command});
            if let Some(t) = h.timeout {
                cmd["timeout"] = json!(t);
            }
            match groups.iter_mut().find(|(m, _)| *m == h.matcher) {
                Some((_, list)) => list.push(cmd),
                None => groups.push((h.matcher.clone(), vec![cmd])),
            }
        }
        if groups.is_empty() {
            continue;
        }
        let list: Vec<Value> = groups
            .into_iter()
            .map(|(m, hooks)| {
                if m.is_empty() {
                    json!({"hooks": hooks})
                } else {
                    json!({"matcher": m, "hooks": hooks})
                }
            })
            .collect();
        events.insert(event.to_string(), Value::Array(list));
    }
    Value::Object(events)
}

pub fn check_hook(h: &Hook) -> Result<(), String> {
    if !HOOK_EVENTS.iter().any(|(e, _, _)| *e == h.event) {
        return Err("Pick an event.".into());
    }
    if h.command.trim().is_empty() {
        return Err("Write the command to run.".into());
    }
    if !h.matcher.is_empty() && h.matcher != "*" {
        if !event_takes_matcher(&h.event) {
            return Err(format!("{} hooks don't take a tool matcher.", h.event));
        }
        let ok = h
            .matcher
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "|_-*.()^$:".contains(c));
        if !ok {
            return Err("A matcher is a tool name or names, like Bash or Edit|Write.".into());
        }
    }
    if h.timeout.is_some_and(|t| !(t > 0.0 && t <= 3600.0)) {
        return Err("The timeout is in seconds, 1 to 3600.".into());
    }
    Ok(())
}

/// Replace hook `index` among `layer`'s hooks with `new` (`None` deletes
/// it; an index past the end adds). The layer's other hooks are kept.
pub fn save_hook(
    project: &Path,
    layer: Layer,
    index: Option<usize>,
    new: Option<Hook>,
) -> Result<(), String> {
    if let Some(h) = &new {
        check_hook(h)?;
    }
    let path = layer
        .path(project)
        .ok_or("ForgeCLI's config folder is unknown")?;
    let root = read_json(&path)?;
    let mut rows = hooks_of(layer, &root, &mut vec![]);
    match (index, new) {
        (Some(i), Some(h)) if i < rows.len() => rows[i] = h,
        (Some(i), None) if i < rows.len() => {
            rows.remove(i);
        }
        (None, Some(h)) => rows.push(h),
        _ => return Err("That hook changed on disk; look again.".into()),
    }
    edit_json(&path, |root| {
        let v = hooks_json(&rows);
        if v.as_object().is_some_and(|o| o.is_empty()) {
            root.remove("hooks");
        } else {
            root.insert("hooks".into(), v);
        }
    })
}

// ---------------------------------------------------------------- MCP servers

/// Where a server is declared, as `forge mcp add --scope` names them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum McpScope {
    /// `mcpServers` in the user's settings.json.
    User,
    /// The project's `.mcp.json`, shared; trusted per person.
    Project,
    /// `mcpServers` in `.forge/settings.local.json`.
    Local,
}

impl McpScope {
    pub const ALL: [McpScope; 3] = [McpScope::User, McpScope::Project, McpScope::Local];

    pub fn path(self, project: &Path) -> Option<PathBuf> {
        match self {
            McpScope::User => Layer::User.path(project),
            McpScope::Project => Some(project.join(".mcp.json")),
            McpScope::Local => Layer::Local.path(project),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Transport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    /// `sse` is the older streaming transport.
    Http {
        url: String,
        headers: BTreeMap<String, String>,
        sse: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub scope: McpScope,
    pub name: String,
    pub transport: Transport,
    /// For `.mcp.json` servers: whether this person trusts it (ForgeCLI
    /// runs it only then). Others are always `true`.
    pub trusted: bool,
    /// Turned off with `disabledMcpjsonServers`.
    pub disabled: bool,
}

impl Server {
    pub fn summary(&self) -> String {
        match &self.transport {
            Transport::Stdio { command, args, .. } => {
                let mut s = command.clone();
                for a in args {
                    s.push(' ');
                    s.push_str(a);
                }
                s
            }
            Transport::Http { url, sse, .. } => {
                format!("{url}{}", if *sse { " (SSE)" } else { "" })
            }
        }
    }
}

fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// One entry, as `forge_mcp::config::parse_server` reads it.
pub fn parse_server(v: &Value) -> Result<Transport, String> {
    let kind = v.get("type").and_then(Value::as_str);
    let url = v.get("url").and_then(Value::as_str);
    match (kind, url) {
        (Some("http" | "streamable-http" | "streamableHttp") | None, Some(u)) => {
            Ok(Transport::Http {
                url: u.into(),
                headers: string_map(v.get("headers")),
                sse: false,
            })
        }
        (Some("sse"), Some(u)) => Ok(Transport::Http {
            url: u.into(),
            headers: string_map(v.get("headers")),
            sse: true,
        }),
        (Some("http" | "sse"), None) => Err("\"url\" is required".into()),
        (Some("stdio") | None, None) => Ok(Transport::Stdio {
            command: v
                .get("command")
                .and_then(Value::as_str)
                .ok_or("\"command\" is required")?
                .into(),
            args: v
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            env: string_map(v.get("env")),
        }),
        (Some(other), _) => Err(format!("unknown transport {other:?}")),
    }
}

/// The JSON ForgeCLI writes for a server (`ServerConfig::to_json`).
pub fn server_json(t: &Transport) -> Value {
    match t {
        Transport::Stdio { command, args, env } => {
            let mut v = json!({"type": "stdio", "command": command, "args": args});
            if !env.is_empty() {
                v["env"] = json!(env);
            }
            v
        }
        Transport::Http { url, headers, sse } => {
            let mut v = json!({"type": if *sse { "sse" } else { "http" }, "url": url});
            if !headers.is_empty() {
                v["headers"] = json!(headers);
            }
            v
        }
    }
}

fn names_in(root: &Map<String, Value>, key: &str) -> Vec<String> {
    root.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn servers(project: &Path, problems: &mut Vec<String>) -> Vec<Server> {
    let read = |p: Option<PathBuf>, problems: &mut Vec<String>| match p.map(|p| read_json(&p)) {
        Some(Ok(m)) => m,
        Some(Err(e)) => {
            problems.push(e);
            Map::new()
        }
        None => Map::new(),
    };
    let user = read(Layer::User.path(project), problems);
    let local = read(Layer::Local.path(project), problems);
    let lists = |key: &str| {
        let mut v = names_in(&user, key);
        v.extend(names_in(&local, key));
        v
    };
    let (enabled, disabled) = (
        lists("enabledMcpjsonServers"),
        lists("disabledMcpjsonServers"),
    );
    let trust_all = [&user, &local]
        .iter()
        .any(|m| m.get("enableAllProjectMcpServers") == Some(&json!(true)));
    let mut out = vec![];
    for scope in McpScope::ALL {
        let root = match scope {
            McpScope::User => user.clone(),
            McpScope::Local => local.clone(),
            McpScope::Project => read(scope.path(project), problems),
        };
        let Some(map) = root.get("mcpServers").and_then(Value::as_object) else {
            continue;
        };
        for (name, entry) in map {
            match parse_server(entry) {
                Ok(transport) => out.push(Server {
                    scope,
                    name: name.clone(),
                    transport,
                    trusted: scope != McpScope::Project || trust_all || enabled.contains(name),
                    disabled: disabled.contains(name),
                }),
                Err(e) => problems.push(format!("MCP server {name}: {e}")),
            }
        }
    }
    out
}

/// Split an argument line the way a shell would for plain words and quotes.
pub fn split_args(line: &str) -> Result<Vec<String>, String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("A quote in the arguments isn't closed.".into());
    }
    if started {
        out.push(cur);
    }
    Ok(out)
}

/// `KEY=value` (env) or `Name: value` (headers) lines.
pub fn parse_pairs(text: &str, sep: char) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match line.split_once(sep) {
            Some((k, v)) if !k.trim().is_empty() && !k.trim().contains(' ') => {
                out.insert(k.trim().to_string(), v.trim().to_string());
            }
            _ => {
                return Err(if sep == '=' {
                    format!("{line:?}: write each variable as KEY=value.")
                } else {
                    format!("{line:?}: write each header as Name: value.")
                })
            }
        }
    }
    Ok(out)
}

pub fn pairs_text(map: &BTreeMap<String, String>, sep: &str) -> String {
    map.iter()
        .map(|(k, v)| format!("{k}{sep}{v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Add or replace a server (`old` names the one being edited, so a rename
/// or a move between scopes removes it there). A `.mcp.json` server is
/// trusted for the person adding it, as `forge mcp add --scope project` does.
pub fn save_server(
    project: &Path,
    old: Option<(McpScope, &str)>,
    scope: McpScope,
    name: &str,
    transport: &Transport,
) -> Result<(), String> {
    check_name(name, false)?;
    match transport {
        Transport::Stdio { command, .. } if command.trim().is_empty() => {
            return Err("Name the command that starts the server.".into())
        }
        Transport::Http { url, .. }
            if !(url.starts_with("http://") || url.starts_with("https://")) =>
        {
            return Err("The URL starts with http:// or https://.".into())
        }
        _ => {}
    }
    let path = scope
        .path(project)
        .ok_or("ForgeCLI's config folder is unknown")?;
    let same = old.is_some_and(|(s, n)| s == scope && n == name);
    if !same {
        let existing = read_json(&path)?;
        if existing
            .get("mcpServers")
            .and_then(|m| m.get(name))
            .is_some()
        {
            return Err(format!("A server named {name} is already there."));
        }
    }
    edit_json(&path, |root| {
        if !root.get("mcpServers").is_some_and(Value::is_object) {
            root.insert("mcpServers".into(), json!({}));
        }
        root["mcpServers"]
            .as_object_mut()
            .expect("object")
            .insert(name.to_string(), server_json(transport));
    })?;
    if let Some((s, n)) = old.filter(|_| !same) {
        remove_server(project, s, n)?;
    }
    if scope == McpScope::Project {
        trust_server(project, name, true)?;
    }
    Ok(())
}

pub fn remove_server(project: &Path, scope: McpScope, name: &str) -> Result<(), String> {
    let path = scope
        .path(project)
        .ok_or("ForgeCLI's config folder is unknown")?;
    edit_json(&path, |root| {
        if let Some(m) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            m.remove(name);
        }
    })
}

/// Trust (or stop trusting) a `.mcp.json` server for this person, in local
/// settings, as `forge mcp approve` does.
pub fn trust_server(project: &Path, name: &str, on: bool) -> Result<(), String> {
    let path = Layer::Local.path(project).ok_or("no project")?;
    edit_json(&path, |root| {
        let mut enabled = names_in(root, "enabledMcpjsonServers");
        let mut disabled = names_in(root, "disabledMcpjsonServers");
        enabled.retain(|n| n != name);
        disabled.retain(|n| n != name);
        if on {
            enabled.push(name.to_string());
        } else {
            disabled.push(name.to_string());
        }
        root.insert("enabledMcpjsonServers".into(), json!(enabled));
        root.insert("disabledMcpjsonServers".into(), json!(disabled));
    })
}

/// Each server's state from `forge mcp list` lines (`name: summary - status`).
pub fn parse_mcp_list(out: &str) -> BTreeMap<String, String> {
    let strip = |s: &str| {
        // ANSI colour codes, should a build print them.
        let mut o = String::new();
        let mut esc = false;
        for c in s.chars() {
            match (esc, c) {
                (false, '\u{1b}') => esc = true,
                (true, 'm') => esc = false,
                (true, _) => {}
                (false, c) => o.push(c),
            }
        }
        o
    };
    out.lines()
        .map(strip)
        .filter_map(|l| {
            let (name, rest) = l.split_once(": ")?;
            let (_, status) = rest.rsplit_once(" - ")?;
            check_name(name.trim(), false).ok()?;
            Some((name.trim().to_string(), status.trim().to_string()))
        })
        .collect()
}

// ---------------------------------------------------------------- permissions

/// One settings file's `permissions` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Permissions {
    pub allow: Vec<String>,
    pub ask: Vec<String>,
    pub deny: Vec<String>,
    pub default_mode: Option<String>,
}

pub fn permissions(project: &Path, layer: Layer) -> Result<Permissions, String> {
    let path = layer
        .path(project)
        .ok_or("ForgeCLI's config folder is unknown")?;
    let root = read_json(&path)?;
    let p = root.get("permissions");
    let list = |k: &str| {
        p.and_then(|p| p.get(k))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(Permissions {
        allow: list("allow"),
        ask: list("ask"),
        deny: list("deny"),
        default_mode: p
            .and_then(|p| p.get("defaultMode"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// A rule as ForgeCLI writes them: `Tool` or `Tool(specifier)`.
pub fn check_rule(rule: &str) -> Result<(), String> {
    let r = rule.trim();
    let (tool, spec) = match r.split_once('(') {
        Some((t, rest)) => match rest.strip_suffix(')') {
            Some(s) => (t, Some(s)),
            None => return Err(format!("{r}: close the bracket, like Bash(git status)")),
        },
        None => (r, None),
    };
    let tool_ok = !tool.is_empty()
        && tool
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '*');
    if !tool_ok {
        return Err(format!("{r}: start with a tool name, like Bash or Read"));
    }
    if spec.is_some_and(|s| s.trim().is_empty()) {
        return Err(format!("{r}: put what it applies to inside the brackets"));
    }
    Ok(())
}

/// Add a rule to `kind` (`allow`, `ask`, `deny`) or remove it.
pub fn set_rule(
    project: &Path,
    layer: Layer,
    kind: &str,
    rule: &str,
    on: bool,
) -> Result<(), String> {
    if on {
        check_rule(rule)?;
    }
    let path = layer
        .path(project)
        .ok_or("ForgeCLI's config folder is unknown")?;
    edit_json(&path, |root| {
        if !root.get("permissions").is_some_and(Value::is_object) {
            root.insert("permissions".into(), json!({}));
        }
        let perms = root["permissions"].as_object_mut().expect("object");
        let mut list: Vec<String> = perms
            .get(kind)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        list.retain(|r| r != rule.trim());
        if on {
            list.push(rule.trim().to_string());
        }
        if list.is_empty() {
            perms.remove(kind);
        } else {
            perms.insert(kind.into(), json!(list));
        }
        if perms.is_empty() {
            root.remove("permissions");
        }
    })
}

// ---------------------------------------------------------------- one key of a settings file

/// A top-level key of one settings file, as JSON (`None` when unset).
pub fn setting(project: &Path, layer: Layer, key: &str) -> Result<Option<Value>, String> {
    let path = layer
        .path(project)
        .ok_or("ForgeCLI's config folder is unknown")?;
    Ok(read_json(&path)?.get(key).cloned())
}

/// Set a top-level key of a settings file (`None` removes it).
pub fn set_setting(
    project: &Path,
    layer: Layer,
    key: &str,
    value: Option<Value>,
) -> Result<(), String> {
    let path = layer
        .path(project)
        .ok_or("ForgeCLI's config folder is unknown")?;
    edit_json(&path, |root| match value {
        Some(v) => {
            root.insert(key.to_string(), v);
        }
        None => {
            root.remove(key);
        }
    })
}

/// The sandbox modes ForgeCLI takes (`sandbox.mode`).
pub const SANDBOX_MODES: [(&str, &str); 3] = [
    ("off", "Off: commands run as you"),
    ("read-only", "Read-only: commands can read, not write"),
    (
        "workspace-write",
        "Workspace: write only in the project, no network",
    ),
];

/// `env`: variables for the commands Forge runs, as `KEY=value` lines.
pub fn env_text(v: Option<&Value>) -> String {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    format!(
                        "{k}={}",
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// `KEY=value` lines as the `env` object (None when empty).
pub fn env_value(text: &str) -> Result<Option<Value>, String> {
    let pairs = parse_pairs(text, '=')?;
    for k in pairs.keys() {
        if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || k.starts_with(|c: char| c.is_ascii_digit())
        {
            return Err(format!(
                "{k}: a variable name is letters, digits and _, not starting with a digit."
            ));
        }
    }
    Ok((!pairs.is_empty()).then(|| json!(pairs)))
}

// ---------------------------------------------------------------- memory files

/// A memory file ForgeCLI loads into every session (`forge-config/src/memory.rs`).
#[derive(Clone, Debug, PartialEq)]
pub struct Memory {
    pub path: PathBuf,
    /// You (every project), Project (shared), Local (this computer).
    pub kind: &'static str,
    pub exists: bool,
    pub bytes: u64,
}

/// The files ForgeCLI reads, in its order: yours, then each folder from the
/// top down to the project (`FORGE.md`, `.forge/FORGE.md`, else
/// `AGENTS.md`, then `FORGE.local.md`), stopping below the home folder. The
/// project's own `FORGE.md` and `FORGE.local.md` and yours are listed even
/// when missing, so they can be made.
pub fn memory_files(project: &Path) -> Vec<Memory> {
    let file = |path: PathBuf, kind: &'static str| {
        let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file());
        Memory {
            exists: meta.is_some(),
            bytes: meta.map(|m| m.len()).unwrap_or(0),
            path,
            kind,
        }
    };
    let mut out = vec![];
    if let Some(h) = cli_home() {
        out.push(file(h.join("FORGE.md"), "You"));
    }
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let mut chain: Vec<&Path> = project
        .ancestors()
        .take_while(|a| Some(*a) != home.as_deref() && a.parent().is_some())
        .collect();
    chain.reverse();
    for dir in chain {
        let is_project = dir == project;
        let mut found = false;
        for name in ["FORGE.md", ".forge/FORGE.md"] {
            let m = file(dir.join(name), "Project");
            if m.exists {
                found = true;
                out.push(m);
            }
        }
        if !found {
            let agents = file(dir.join("AGENTS.md"), "Project");
            if agents.exists {
                out.push(agents);
            } else if is_project {
                out.push(file(dir.join("FORGE.md"), "Project"));
            }
        }
        let local = file(dir.join("FORGE.local.md"), "Local");
        if local.exists || is_project {
            out.push(local);
        }
    }
    out
}

// ---------------------------------------------------------------- all of it

/// Everything the editors show, read in one pass.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub commands: Vec<Command>,
    pub agents: Vec<Agent>,
    pub skills: Vec<Skill>,
    pub hooks: Vec<Hook>,
    pub servers: Vec<Server>,
    /// Files that couldn't be read or parsed.
    pub problems: Vec<String>,
}

pub fn load(project: &Path) -> Catalog {
    let mut problems = vec![];
    Catalog {
        commands: commands(project, &mut problems),
        agents: agents(project, &mut problems),
        skills: skills(project),
        hooks: hooks(project, &mut problems),
        servers: servers(project, &mut problems),
        problems,
    }
}

/// A fingerprint of every file the catalog reads (paths, sizes, times), so
/// a cheap poll can tell when to load again.
pub fn signature(project: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut stamp = |p: &Path| {
        p.hash(&mut h);
        if let Ok(m) = std::fs::metadata(p) {
            m.len().hash(&mut h);
            m.modified().ok().hash(&mut h);
        }
    };
    fn walk(dir: &Path, depth: usize, f: &mut dyn FnMut(&Path)) {
        f(dir);
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 4 {
                walk(&p, depth + 1, f);
            } else {
                f(&p);
            }
        }
    }
    for kind in ["commands", "agents", "skills"] {
        for (_, d) in read_dirs(kind, project) {
            walk(&d, 0, &mut stamp);
        }
    }
    for layer in Layer::ALL {
        if let Some(p) = layer.path(project) {
            stamp(&p);
        }
    }
    stamp(&project.join(".mcp.json"));
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj_skills(p: &Path) -> Vec<Skill> {
        skills(p)
            .into_iter()
            .filter(|s| s.scope == Scope::Project)
            .collect()
    }

    fn proj_servers(p: &Path) -> Vec<Server> {
        servers(p, &mut vec![])
            .into_iter()
            .filter(|s| s.scope != McpScope::User)
            .collect()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "forge-files-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn frontmatter_round_trips_and_keeps_other_keys() {
        let text = "---\nname: review\nallowed-tools: [Read, \"Bash(git diff:*)\"]\ntags:\n  - a\n  - b\n---\nBody\n";
        let mut d = Doc::parse(text);
        assert_eq!(d.get("name").as_deref(), Some("review"));
        assert_eq!(
            d.list("allowed-tools").unwrap(),
            ["Read", "Bash(git diff:*)"]
        );
        assert_eq!(d.list("tags").unwrap(), ["a", "b"]);
        d.set("description", Some("Review: the diff"));
        let out = d.render();
        assert!(out.contains("description: \"Review: the diff\""), "{out}");
        assert!(out.contains("tags:\n  - a\n  - b\n"), "{out}");
        let again = Doc::parse(&out);
        assert_eq!(
            again.get("description").as_deref(),
            Some("Review: the diff")
        );
        assert_eq!(again.body, "Body\n");
        // No frontmatter: all body.
        assert_eq!(Doc::parse("# Hi\n").body, "# Hi\n");
        assert_eq!(Doc::parse("just text").render(), "just text\n");
    }

    #[test]
    fn values_forgecli_would_misread_are_quoted() {
        assert_eq!(quote("[x]"), "\"[x]\"");
        assert_eq!(quote("a: b"), "\"a: b\"");
        assert_eq!(quote("> folded"), "\"> folded\"");
        assert_eq!(quote("plain words"), "plain words");
    }

    #[test]
    fn commands_are_written_where_forgecli_reads_them() {
        let p = tmp("cmd");
        let path = save_command(
            Scope::Project,
            &p,
            None,
            "git:review",
            "Review the diff",
            "[file]",
            "Review $ARGUMENTS",
        )
        .unwrap();
        assert_eq!(path, p.join(".forge/commands/git/review.md"));
        let list = commands(&p, &mut vec![]);
        let c = list.iter().find(|c| c.name == "git:review").unwrap();
        assert_eq!(c.description, "Review the diff");
        assert_eq!(c.hint, "[file]");
        assert_eq!(c.body.trim(), "Review $ARGUMENTS");
        // An argument hint in brackets stays text (ForgeCLI keeps the raw line).
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("argument-hint: \"[file]\""), "{text}");
        // Rename: the old file goes.
        let moved = save_command(Scope::Project, &p, Some(&path), "rev", "", "", "Again").unwrap();
        assert!(!path.exists() && moved.exists());
        assert!(save_command(Scope::Project, &p, None, "rev", "", "", "x").is_err());
        assert!(save_command(Scope::Project, &p, None, "bad name", "", "", "x").is_err());
        assert!(save_command(Scope::Project, &p, None, "empty", "", "", " ").is_err());
    }

    #[test]
    fn agents_need_a_name_and_description() {
        let p = tmp("agent");
        let tools = vec!["Read".to_string(), "Grep".to_string()];
        let path = save_agent(
            Scope::Project,
            &p,
            None,
            "reviewer",
            "Reviews code: carefully",
            Some(&tools),
            "",
            "You review.",
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("---\nname: reviewer\n"), "{text}");
        assert!(text.contains("tools: [Read, Grep]"), "{text}");
        let all = agents(&p, &mut vec![]);
        let a = all.iter().find(|a| a.scope == Scope::Project).unwrap();
        assert_eq!(a.tools.as_deref(), Some(&tools[..]));
        assert_eq!(a.description, "Reviews code: carefully");
        assert!(a.problem.is_none());
        assert!(save_agent(Scope::Project, &p, None, "x", "", None, "", "p").is_err());
        assert!(save_agent(Scope::Project, &p, None, "x", "d", Some(&[]), "", "p").is_err());
        std::fs::write(p.join(".forge/agents/broken.md"), "no frontmatter").unwrap();
        let all = agents(&p, &mut vec![]);
        assert!(all
            .iter()
            .any(|a| a.name == "broken" && a.problem.is_some()));
    }

    #[test]
    fn skills_turn_off_by_moving_out_of_forgeclis_sight() {
        let p = tmp("skill");
        let dir = save_skill(
            Scope::Project,
            &p,
            None,
            "pdf",
            "Read PDFs",
            "Use pdftotext.",
        )
        .unwrap();
        std::fs::write(dir.join("helper.py"), "").unwrap();
        let s = &proj_skills(&p)[0];
        assert!(s.enabled && s.extra_files == 1);
        let off = set_skill_enabled(&dir, false).unwrap();
        assert_eq!(off, p.join(".forge/skills/.disabled/pdf"));
        // ForgeCLI's loader: only direct subfolders holding SKILL.md.
        assert!(!p.join(".forge/skills/.disabled/SKILL.md").exists());
        assert!(!proj_skills(&p)[0].enabled);
        let on = set_skill_enabled(&off, true).unwrap();
        assert_eq!(on, dir);
        assert!(on.join("helper.py").exists());
        assert!(save_skill(Scope::Project, &p, None, "pdf", "x", "y").is_err());
    }

    #[test]
    fn hooks_group_by_event_and_matcher() {
        let p = tmp("hooks");
        let h = |e: &str, m: &str, c: &str| Hook {
            layer: Layer::Project,
            event: e.into(),
            matcher: m.into(),
            command: c.into(),
            timeout: None,
        };
        std::fs::create_dir_all(p.join(".forge")).unwrap();
        std::fs::write(p.join(".forge/settings.json"), r#"{"model": "x"}"#).unwrap();
        save_hook(&p, Layer::Project, None, Some(h("PreToolUse", "Bash", "a"))).unwrap();
        save_hook(&p, Layer::Project, None, Some(h("PreToolUse", "Bash", "b"))).unwrap();
        save_hook(&p, Layer::Project, None, Some(h("Stop", "", "c"))).unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(p.join(".forge/settings.json")).unwrap())
                .unwrap();
        assert_eq!(v["model"], "x");
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "Bash");
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][1]["command"], "b");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["type"], "command");
        assert!(v["hooks"]["Stop"][0].get("matcher").is_none());
        let rows = hooks(&p, &mut vec![]);
        assert_eq!(rows.iter().filter(|h| h.layer == Layer::Project).count(), 3);
        save_hook(&p, Layer::Project, Some(0), None).unwrap();
        save_hook(&p, Layer::Project, Some(0), None).unwrap();
        save_hook(&p, Layer::Project, Some(0), None).unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(p.join(".forge/settings.json")).unwrap())
                .unwrap();
        assert!(v.get("hooks").is_none());
        assert!(check_hook(&h("Stop", "Bash", "x")).is_err());
        assert!(check_hook(&h("PreToolUse", "Bash", " ")).is_err());
    }

    #[test]
    fn a_settings_file_that_does_not_parse_is_left_alone() {
        let p = tmp("broken");
        std::fs::create_dir_all(p.join(".forge")).unwrap();
        let f = p.join(".forge/settings.json");
        std::fs::write(&f, "{ // comment\n}").unwrap();
        let h = Hook {
            layer: Layer::Project,
            event: "Stop".into(),
            matcher: String::new(),
            command: "x".into(),
            timeout: None,
        };
        assert!(save_hook(&p, Layer::Project, None, Some(h)).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{ // comment\n}");
        let mut problems = vec![];
        hooks(&p, &mut problems);
        assert!(problems.iter().any(|e| e.contains("settings.json")));
    }

    #[test]
    fn project_servers_are_trusted_for_whoever_adds_them() {
        let p = tmp("mcp");
        let t = Transport::Stdio {
            command: "npx".into(),
            args: split_args("-y \"@scope/server x\"").unwrap(),
            env: parse_pairs("TOKEN=${GH_TOKEN}", '=').unwrap(),
        };
        save_server(&p, None, McpScope::Project, "gh", &t).unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(p.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(
            v["mcpServers"]["gh"]["args"],
            json!(["-y", "@scope/server x"])
        );
        assert_eq!(v["mcpServers"]["gh"]["env"]["TOKEN"], "${GH_TOKEN}");
        let list = proj_servers(&p);
        assert!(list[0].trusted && !list[0].disabled);
        // Moved to local scope: gone from .mcp.json.
        save_server(
            &p,
            Some((McpScope::Project, "gh")),
            McpScope::Local,
            "gh",
            &t,
        )
        .unwrap();
        let list = proj_servers(&p);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].scope, McpScope::Local);
        let http = Transport::Http {
            url: "ftp://x".into(),
            headers: Default::default(),
            sse: false,
        };
        assert!(save_server(&p, None, McpScope::Local, "web", &http).is_err());
        assert!(split_args("a \"b").is_err());
        assert!(parse_pairs("no-separator", '=').is_err());
    }

    #[test]
    fn mcp_list_output_is_read() {
        let out = "Checking MCP server health...\n\ngh: npx -y x (stdio) - \u{1b}[32mconnected, 3 tools\u{1b}[0m\nweb: https://a/b (http) - failed: refused\n";
        let m = parse_mcp_list(out);
        assert_eq!(m["gh"], "connected, 3 tools");
        assert_eq!(m["web"], "failed: refused");
    }

    #[test]
    fn permission_rules_are_checked_and_written() {
        let p = tmp("perm");
        set_rule(&p, Layer::Local, "allow", "Bash(git status)", true).unwrap();
        set_rule(&p, Layer::Local, "deny", "Read(./.env)", true).unwrap();
        let got = permissions(&p, Layer::Local).unwrap();
        assert_eq!(got.allow, ["Bash(git status)"]);
        assert_eq!(got.deny, ["Read(./.env)"]);
        set_rule(&p, Layer::Local, "allow", "Bash(git status)", false).unwrap();
        assert!(permissions(&p, Layer::Local).unwrap().allow.is_empty());
        assert!(check_rule("Bash(").is_err());
        assert!(check_rule("Bash()").is_err());
        assert!(check_rule("(x)").is_err());
        assert!(check_rule("WebFetch(domain:example.com)").is_ok());
    }

    #[test]
    fn env_lines_round_trip_and_names_are_checked() {
        let v = env_value("A=1\nPATH_EXTRA=/x y").unwrap().unwrap();
        assert_eq!(v["PATH_EXTRA"], "/x y");
        assert_eq!(env_text(Some(&v)), "A=1\nPATH_EXTRA=/x y");
        assert!(env_value("1A=x").is_err());
        assert_eq!(env_value("  ").unwrap(), None);
    }

    #[test]
    fn memory_lists_the_projects_files_even_before_they_exist() {
        let p = tmp("mem");
        std::fs::write(p.join("AGENTS.md"), "x").unwrap();
        let m = memory_files(&p);
        // AGENTS.md stands in for a missing FORGE.md; FORGE.local.md can be made.
        assert!(m.iter().any(|f| f.path == p.join("AGENTS.md") && f.exists));
        assert!(m
            .iter()
            .any(|f| f.path == p.join("FORGE.local.md") && !f.exists));
        assert!(!m.iter().any(|f| f.path == p.join("FORGE.md")));
    }

    #[test]
    fn the_signature_moves_when_a_file_changes() {
        let p = tmp("sig");
        let a = signature(&p);
        std::fs::create_dir_all(p.join(".forge/commands")).unwrap();
        std::fs::write(p.join(".forge/commands/x.md"), "hi").unwrap();
        assert_ne!(a, signature(&p));
    }
}
