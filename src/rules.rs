//! What Claude Code reads in a project, and when.
//!
//! Claude Code loads instructions from a few kinds of file. `scan` finds them
//! all for one project and says when each loads, so the Rules screen can show
//! what costs context in every session and what loads only when it applies:
//!
//! - `CLAUDE.md` (and `.claude/CLAUDE.md`, `CLAUDE.local.md`) in the project
//!   and its parent folders, and `~/.claude/CLAUDE.md`: every session;
//! - files those import with `@path`: every session, with their importer;
//! - `.claude/rules/**/*.md` (project and `~/.claude`): every session, or,
//!   with `paths:` globs in their frontmatter, when Claude touches a match;
//! - `CLAUDE.md` in a subfolder: when Claude works in that folder;
//! - `.claude/skills/<name>/SKILL.md`: when Claude decides the skill fits.
//!
//! Everything here is plain file I/O; callers run it off the UI thread.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use globset::{GlobSet, GlobSetBuilder};

/// How far `@path` imports are followed (Claude Code stops at four hops).
const MAX_IMPORT_DEPTH: usize = 4;
/// How deep the walk for subfolder `CLAUDE.md` files goes.
const MAX_WALK_DEPTH: usize = 12;
/// Larger files are listed but not read in full.
const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// `~/.claude`: every project on this computer.
    User,
    /// Checked into the project (or a parent folder).
    Project,
    /// `CLAUDE.local.md`: this computer only, not committed.
    Local,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::User => "You",
            Scope::Project => "Project",
            Scope::Local => "Local",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Memory,
    Import,
    Rule,
    Folder,
    Skill,
}

#[derive(Clone, PartialEq, Debug)]
pub enum When {
    Always,
    /// Globs, relative to the project root.
    Paths(Vec<String>),
    /// A folder, relative to the project root, with `/` separators.
    Folder(String),
    OnDemand,
}

#[derive(Clone, Debug)]
pub struct Source {
    pub kind: Kind,
    pub scope: Scope,
    pub path: PathBuf,
    /// A short name: the rule's file stem, the skill's name, or a path.
    pub name: String,
    pub when: When,
    /// The whole file, as written.
    pub text: String,
    /// The text after any frontmatter: what Claude reads as the rule.
    pub body: String,
    /// A skill's description (how Claude decides it fits).
    pub description: Option<String>,
    /// For an import: the file that imports it.
    pub imported_by: Option<PathBuf>,
    /// Frontmatter that didn't parse, or globs that don't compile.
    pub problem: Option<String>,
    matcher: Option<GlobSet>,
}

impl Source {
    /// A rough token count (about four characters a token).
    pub fn tokens(&self) -> usize {
        self.body.chars().count().div_ceil(4)
    }

    /// Whether this loads when Claude works on `rel` (a project-relative path
    /// with `/` separators).
    pub fn loads_for(&self, rel: &str) -> bool {
        match &self.when {
            When::Always => true,
            When::Paths(_) => self.matcher.as_ref().is_some_and(|m| m.is_match(rel)),
            When::Folder(dir) => rel
                .strip_prefix(dir.as_str())
                .is_some_and(|rest| rest.starts_with('/')),
            When::OnDemand => false,
        }
    }
}

pub struct Scan {
    pub project: PathBuf,
    pub sources: Vec<Source>,
}

impl Scan {
    pub fn always_tokens(&self) -> usize {
        self.sources
            .iter()
            .filter(|s| s.when == When::Always)
            .map(Source::tokens)
            .sum()
    }

    pub fn count(&self, kind: impl Fn(&When) -> bool) -> usize {
        self.sources.iter().filter(|s| kind(&s.when)).count()
    }

    /// The project-relative form of what someone typed into the tester:
    /// separators made `/`, a leading `./` or the project path dropped.
    pub fn relative(&self, typed: &str) -> String {
        let typed = typed.trim().replace('\\', "/");
        let root = self.project.to_string_lossy().replace('\\', "/");
        let rel = typed.strip_prefix(&format!("{root}/")).unwrap_or(&typed);
        rel.trim_start_matches("./")
            .trim_start_matches('/')
            .to_string()
    }
}

/// The user's home folder.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Every instruction file Claude Code would read for `project`.
pub fn scan(project: &Path, home: Option<&Path>) -> Scan {
    let mut found = Found::default();
    let user_claude = home.map(|h| h.join(".claude"));

    // The user's own files, for every project.
    if let Some(dir) = &user_claude {
        found.memory(&dir.join("CLAUDE.md"), Scope::User, home, project);
        found.rules(&dir.join("rules"), Scope::User);
        found.skills(&dir.join("skills"), Scope::User);
    }

    // The project and the folders above it (stopping short of home, whose
    // `.claude` is the user's).
    for (i, dir) in project.ancestors().enumerate() {
        if i > 0 && home.is_some_and(|h| dir == h) {
            break;
        }
        found.memory(&dir.join("CLAUDE.md"), Scope::Project, home, project);
        if i == 0 {
            found.memory(
                &dir.join(".claude").join("CLAUDE.md"),
                Scope::Project,
                home,
                project,
            );
        }
        found.memory(&dir.join("CLAUDE.local.md"), Scope::Local, home, project);
    }
    if user_claude.as_deref() != Some(&project.join(".claude")) {
        found.rules(&project.join(".claude").join("rules"), Scope::Project);
        found.skills(&project.join(".claude").join("skills"), Scope::Project);
    }
    found.folders(project);

    Scan {
        project: project.to_path_buf(),
        sources: found.sources,
    }
}

#[derive(Default)]
struct Found {
    sources: Vec<Source>,
    seen: HashSet<PathBuf>,
}

impl Found {
    fn first_time(&mut self, path: &Path) -> bool {
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.seen.insert(key)
    }

    fn memory(&mut self, path: &Path, scope: Scope, home: Option<&Path>, project: &Path) {
        if path.is_file() && self.first_time(path) {
            let text = read(path);
            let name = short_name(path, project, home);
            self.push(
                Kind::Memory,
                scope,
                path,
                name,
                When::Always,
                text,
                None,
                None,
            );
            self.imports(path, scope, home, project, 1);
        }
    }

    /// Follow `@path` imports from a file that loads every session.
    fn imports(
        &mut self,
        from: &Path,
        scope: Scope,
        home: Option<&Path>,
        project: &Path,
        depth: usize,
    ) {
        if depth > MAX_IMPORT_DEPTH {
            return;
        }
        let text = read(from);
        for target in import_paths(&text) {
            let path = resolve_import(&target, from, home);
            if path.is_file() && self.first_time(&path) {
                let body = read(&path);
                let name = short_name(&path, project, home);
                self.push(
                    Kind::Import,
                    scope,
                    &path,
                    name,
                    When::Always,
                    body,
                    None,
                    Some(from.to_path_buf()),
                );
                self.imports(&path, scope, home, project, depth + 1);
            }
        }
    }

    fn rules(&mut self, dir: &Path, scope: Scope) {
        let mut files: Vec<PathBuf> = walk(dir, usize::MAX)
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        files.sort();
        for path in files {
            if !self.first_time(&path) {
                continue;
            }
            let text = read(&path);
            let (front, _) = frontmatter(&text);
            let globs = front.as_ref().map(|f| f.list("paths")).unwrap_or_default();
            let when = if globs.is_empty() {
                When::Always
            } else {
                When::Paths(globs)
            };
            let name = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            self.push(Kind::Rule, scope, &path, name, when, text, None, None);
        }
    }

    fn skills(&mut self, dir: &Path, scope: Scope) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("SKILL.md"))
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        for path in paths {
            if !self.first_time(&path) {
                continue;
            }
            let text = read(&path);
            let (front, _) = frontmatter(&text);
            let folder = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = front
                .as_ref()
                .and_then(|f| f.value("name"))
                .unwrap_or(folder);
            let description = front.as_ref().and_then(|f| f.value("description"));
            self.push(
                Kind::Skill,
                scope,
                &path,
                name,
                When::OnDemand,
                text,
                description,
                None,
            );
        }
    }

    /// `CLAUDE.md` files in the project's subfolders (git-ignored folders
    /// such as `target/` and `node_modules/` are skipped).
    fn folders(&mut self, project: &Path) {
        let mut paths: Vec<PathBuf> = walk(project, MAX_WALK_DEPTH)
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n == "CLAUDE.md" || n == "CLAUDE.local.md")
            })
            .filter(|p| p.parent() != Some(project))
            .collect();
        paths.sort();
        for path in paths {
            if !self.first_time(&path) {
                continue;
            }
            let Some(dir) = path.parent().and_then(|d| d.strip_prefix(project).ok()) else {
                continue;
            };
            let dir = dir.to_string_lossy().replace('\\', "/");
            let local = path.file_name().is_some_and(|n| n == "CLAUDE.local.md");
            let name = format!(
                "{dir}/{}",
                if local {
                    "CLAUDE.local.md"
                } else {
                    "CLAUDE.md"
                }
            );
            let text = read(&path);
            self.push(
                Kind::Folder,
                if local { Scope::Local } else { Scope::Project },
                &path,
                name,
                When::Folder(dir),
                text,
                None,
                None,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        kind: Kind,
        scope: Scope,
        path: &Path,
        name: String,
        when: When,
        text: String,
        description: Option<String>,
        imported_by: Option<PathBuf>,
    ) {
        let (front, body) = frontmatter(&text);
        let mut problem = (text.starts_with("---") && front.is_none())
            .then(|| "The frontmatter has no closing `---`.".to_string());
        let matcher = match &when {
            When::Paths(globs) => match compile(globs) {
                Ok(m) => Some(m),
                Err(e) => {
                    problem = Some(e);
                    None
                }
            },
            _ => None,
        };
        let body = body.trim_start().to_string();
        self.sources.push(Source {
            kind,
            scope,
            path: path.to_path_buf(),
            name,
            when,
            text,
            body,
            description,
            imported_by,
            problem,
            matcher,
        });
    }
}

fn read(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > MAX_FILE_BYTES => format!(
            "(This file is {} KB; Forge shows files up to {} KB.)",
            m.len() / 1024,
            MAX_FILE_BYTES / 1024
        ),
        _ => std::fs::read_to_string(path).unwrap_or_default(),
    }
}

/// The project's files for `@` completion: paths relative to `dir`, with
/// `/`, as git sees them (ignored and hidden ones left out), at most `cap`
/// (blocking: call off the UI thread).
pub fn project_files(dir: &Path, cap: usize) -> Vec<String> {
    let mut out: Vec<String> = walk(dir, 16)
        .into_iter()
        .filter_map(|p| {
            p.strip_prefix(dir)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .take(cap)
        .collect();
    // Shallow paths first: they're the ones people mean most often.
    out.sort_by_key(|p| (p.matches('/').count(), p.clone()));
    out
}

/// The project's files and their folders, for the @ list: a folder ends
/// in `/`; shallow entries first, a folder before the files beside it.
/// As the VS Code extension lists them (`rg --files --hidden --follow`):
/// hidden files too, minus what it always leaves out.
pub fn project_entries(dir: &Path, cap: usize) -> Vec<String> {
    if !dir.is_dir() {
        return vec![];
    }
    let files = ignore::WalkBuilder::new(dir)
        .max_depth(Some(16))
        .hidden(false)
        .follow_links(true)
        .require_git(false)
        .filter_entry(|e| !left_out(&e.file_name().to_string_lossy()))
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            e.path()
                .strip_prefix(dir)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .take(cap)
        .collect();
    entries_of(files)
}

/// What the extension never lists, whatever `.gitignore` says.
fn left_out(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "dist" | "build" | ".next" | ".nuxt" | ".DS_Store" | "Thumbs.db" | ".env"
    ) || name.starts_with(".env.")
        || name.ends_with(".log")
}

/// Files (project-relative, `/`-separated) and the folders they're in, in
/// the @ list's order.
pub fn entries_of(files: Vec<String>) -> Vec<String> {
    let mut dirs = std::collections::BTreeSet::new();
    for f in &files {
        let mut p = f.as_str();
        while let Some(i) = p.rfind('/') {
            p = &p[..i];
            if !dirs.insert(format!("{p}/")) {
                break;
            }
        }
    }
    let mut out: Vec<String> = dirs.into_iter().chain(files).collect();
    out.sort_by(|a, b| {
        let depth = |s: &str| s.trim_end_matches('/').matches('/').count();
        (depth(a), !a.ends_with('/'), a.as_str()).cmp(&(depth(b), !b.ends_with('/'), b.as_str()))
    });
    out
}

/// Files under `dir`, honouring `.gitignore` and skipping hidden folders.
fn walk(dir: &Path, depth: usize) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return vec![];
    }
    ignore::WalkBuilder::new(dir)
        .max_depth(Some(depth))
        .hidden(true)
        .require_git(false)
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.into_path())
        .collect()
}

/// Compile rule globs the way Claude Code reads them: relative to the
/// project, `*` stays within a folder, `**` crosses folders, `{a,b}` works.
pub fn compile(globs: &[String]) -> Result<GlobSet, String> {
    let mut set = GlobSetBuilder::new();
    for g in globs {
        let glob = globset::GlobBuilder::new(g.trim_start_matches("./"))
            .literal_separator(true)
            .build()
            .map_err(|e| format!("`{g}` is not a valid glob: {e}"))?;
        set.add(glob);
    }
    set.build().map_err(|e| e.to_string())
}

/// A name for a file in the list: project-relative inside the project,
/// `~/…` under home, the full path otherwise.
pub fn short_name(path: &Path, project: &Path, home: Option<&Path>) -> String {
    match path.strip_prefix(project) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => display_path(path, home),
    }
}

/// `~/x` for paths under home; the full path otherwise.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
        None => path.to_string_lossy().into_owned(),
    }
}

// ----------------------------------------------------------- frontmatter

#[derive(Debug, Default, PartialEq)]
pub struct Frontmatter {
    entries: Vec<(String, Vec<String>)>,
}

impl Frontmatter {
    pub fn value(&self, key: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.first().cloned())
            .filter(|v| !v.is_empty())
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
}

/// Split `---` frontmatter from the body. Reads the small part of YAML that
/// rules and skills use: `key: value`, `key: [a, b]`, and `- item` lists.
/// Returns `None` for the frontmatter if there is none (or it never closes).
pub fn frontmatter(text: &str) -> (Option<Frontmatter>, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, text);
    };
    let mut offset = 0;
    let mut end = None;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            end = Some((offset, offset + line.len()));
            break;
        }
        offset += line.len();
    }
    let Some((yaml_end, body_start)) = end else {
        return (None, text);
    };
    let yaml = &rest[..yaml_end];
    let mut front = Frontmatter::default();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(item) = trimmed.strip_prefix("- ") {
            if let Some((_, values)) = front.entries.last_mut() {
                values.push(unquote(item));
            }
        } else if let Some((key, value)) = trimmed.split_once(':') {
            let value = value.trim();
            let values =
                if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                    inner
                        .split(',')
                        .map(unquote)
                        .filter(|v| !v.is_empty())
                        .collect()
                } else if value.is_empty() {
                    vec![]
                } else if key.trim() == "paths" {
                    value
                        .split(',')
                        .map(unquote)
                        .filter(|v| !v.is_empty())
                        .collect()
                } else {
                    vec![unquote(value)]
                };
            front.entries.push((key.trim().to_string(), values));
        }
    }
    (Some(front), &rest[body_start..])
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(s);
    s.trim().to_string()
}

// --------------------------------------------------------------- imports

/// `@path` imports in a memory file: outside code fences and code spans,
/// at the start of a word (so `me@example.com` is not one).
pub fn import_paths(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut in_code = false;
        let mut prev = ' ';
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '`' {
                in_code = !in_code;
            } else if c == '@' && !in_code && prev.is_whitespace() {
                let mut path = String::new();
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == '\\' && j + 1 < chars.len() && chars[j + 1] == ' ' {
                        path.push(' ');
                        j += 2;
                        continue;
                    }
                    if chars[j].is_whitespace() || chars[j] == '`' {
                        break;
                    }
                    path.push(chars[j]);
                    j += 1;
                }
                let path = path
                    .trim_end_matches(['.', ',', ';', ':', ')', '!', '?'])
                    .to_string();
                if path.contains('/') || path.contains('.') {
                    out.push(path);
                }
                i = j;
                prev = ' ';
                continue;
            }
            prev = c;
            i += 1;
        }
    }
    out
}

fn resolve_import(target: &str, from: &Path, home: Option<&Path>) -> PathBuf {
    if let (Some(rest), Some(home)) = (target.strip_prefix("~/"), home) {
        return home.join(rest);
    }
    let p = Path::new(target);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        from.parent().unwrap_or(Path::new(".")).join(p)
    }
}

// ----------------------------------------------------------------- writes

/// A rule file name from what someone typed: lowercase words joined by `-`.
pub fn slug(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    (!out.is_empty() && out.len() <= 48).then_some(out)
}

/// The text of a new rule file.
pub fn new_rule_text(title: &str, globs: &[String]) -> String {
    let mut text = String::new();
    if !globs.is_empty() {
        text.push_str("---\npaths:\n");
        for g in globs {
            text.push_str(&format!("  - \"{g}\"\n"));
        }
        text.push_str("---\n\n");
    }
    text.push_str(&format!("# {}\n\n- \n", title.trim()));
    text
}

/// Create `.claude/rules/<slug>.md` in the project. Never overwrites.
pub fn create_rule(project: &Path, name: &str, globs: &[String]) -> Result<PathBuf, String> {
    let slug = slug(name).ok_or("Give the rule a name (letters or numbers).")?;
    compile(globs)?;
    let dir = project.join(".claude").join("rules");
    let path = dir.join(format!("{slug}.md"));
    if path.exists() {
        return Err(format!("`.claude/rules/{slug}.md` already exists."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, new_rule_text(name, globs)).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Split what someone typed into globs: commas or new lines between them.
pub fn parse_globs(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(unquote)
        .filter(|g| !g.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-rules-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_at_list_lists_hidden_files_as_the_extension_does() {
        let dir = tmp("entries");
        for f in [
            ".claude/settings.json",
            ".gitignore",
            ".mcp.json",
            "src/a.rs",
            "node_modules/x/i.js",
            "dist/out.js",
            ".env",
            ".env.local",
            "run.log",
            "ignored/b.txt",
        ] {
            write(dir.join(f), "x");
        }
        std::fs::write(dir.join(".gitignore"), "ignored/\n").unwrap();
        let got = project_entries(&dir, 100);
        for want in [".claude/", ".claude/settings.json", ".gitignore", ".mcp.json", "src/a.rs"] {
            assert!(got.iter().any(|e| e == want), "missing {want}: {got:?}");
        }
        for gone in ["node_modules", "dist", ".env", "run.log", "ignored"] {
            assert!(!got.iter().any(|e| e.starts_with(gone)), "listed {gone}: {got:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    fn write(path: PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn frontmatter_forms() {
        let (f, body) = frontmatter("---\npaths:\n  - \"ui/**\"\n  - 'docs/x.md'\n---\n\n# UI\n");
        assert_eq!(f.unwrap().list("paths"), vec!["ui/**", "docs/x.md"]);
        assert_eq!(body, "\n# UI\n");
        let (f, _) = frontmatter("---\npaths: [\"a/**\", b.rs]\nname: x\n---\nbody");
        let f = f.unwrap();
        assert_eq!(f.list("paths"), vec!["a/**", "b.rs"]);
        assert_eq!(f.value("name").as_deref(), Some("x"));
        assert_eq!(frontmatter("---\nnever closes\n").0, None);
        assert_eq!(frontmatter("# no frontmatter").1, "# no frontmatter");
    }

    #[test]
    fn imports_skip_code_and_emails() {
        let text = "See @docs/a.md and @~/notes.md.\nMail me@example.com\n`@not/this.md`\n```\n@nor/this.md\n```\n@Design\\ Docs/x.md";
        assert_eq!(
            import_paths(text),
            vec!["docs/a.md", "~/notes.md", "Design Docs/x.md"]
        );
    }

    #[test]
    fn globs_match_like_claude_code() {
        let m = compile(&["ui/**".into(), "src/{a,b}.rs".into(), "*.toml".into()]).unwrap();
        assert!(m.is_match("ui/src/App.tsx"));
        assert!(m.is_match("src/a.rs") && m.is_match("src/b.rs") && !m.is_match("src/c.rs"));
        assert!(m.is_match("Cargo.toml") && !m.is_match("crates/x/Cargo.toml"));
        assert!(compile(&["[".into()]).is_err());
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("UI rules!").as_deref(), Some("ui-rules"));
        assert_eq!(slug("  ").as_deref(), None);
        assert_eq!(
            parse_globs("ui/**, \"src/*.rs\"\n\n"),
            vec!["ui/**", "src/*.rs"]
        );
    }

    #[test]
    fn scans_a_project() {
        let home = tmp("home");
        let project = tmp("proj");
        write(home.join(".claude/CLAUDE.md"), "Be brief.");
        write(
            home.join(".claude/skills/review/SKILL.md"),
            "---\nname: review\ndescription: Review a diff\n---\nSteps",
        );
        write(project.join("CLAUDE.md"), "Rules. @docs/arch.md");
        write(project.join("docs/arch.md"), "Architecture");
        write(
            project.join(".claude/rules/ui.md"),
            "---\npaths:\n  - \"ui/**\"\n---\n# UI\nTokens only.",
        );
        write(
            project.join(".claude/rules/always.md"),
            "# Always\nShip it.",
        );
        write(project.join("ui/CLAUDE.md"), "UI folder notes");
        write(project.join(".gitignore"), "target/\n");
        write(project.join("target/CLAUDE.md"), "ignored");

        let scan = scan(&project, Some(&home));
        let names: Vec<_> = scan
            .sources
            .iter()
            .map(|s| (s.kind, s.name.as_str()))
            .collect();
        assert!(names.contains(&(Kind::Memory, "~/.claude/CLAUDE.md")));
        assert!(names.contains(&(Kind::Memory, "CLAUDE.md")));
        assert!(names.contains(&(Kind::Import, "docs/arch.md")));
        assert!(names.contains(&(Kind::Rule, "ui")));
        assert!(names.contains(&(Kind::Rule, "always")));
        assert!(names.contains(&(Kind::Folder, "ui/CLAUDE.md")));
        assert!(names.contains(&(Kind::Skill, "review")));
        assert!(!names.iter().any(|(_, n)| n.contains("target")));

        let ui = scan.sources.iter().find(|s| s.name == "ui").unwrap();
        assert_eq!(ui.body, "# UI\nTokens only.");
        assert!(ui.loads_for("ui/src/App.tsx") && !ui.loads_for("src/main.rs"));
        let folder = scan
            .sources
            .iter()
            .find(|s| s.kind == Kind::Folder)
            .unwrap();
        assert!(folder.loads_for("ui/x.ts") && !folder.loads_for("uix/x.ts"));
        assert_eq!(
            scan.relative(&format!("{}/ui/a.ts", project.display())),
            "ui/a.ts"
        );
        assert_eq!(scan.relative("./ui\\a.ts"), "ui/a.ts");

        let made = create_rule(&project, "Git hygiene", &["src/**".into()]).unwrap();
        assert!(made.ends_with(".claude/rules/git-hygiene.md"));
        assert!(create_rule(&project, "Git hygiene", &[]).is_err());
        let rescan = super::scan(&project, Some(&home));
        let rule = rescan
            .sources
            .iter()
            .find(|s| s.name == "git-hygiene")
            .unwrap();
        assert_eq!(rule.when, When::Paths(vec!["src/**".into()]));
    }
}
