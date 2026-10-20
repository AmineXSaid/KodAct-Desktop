//! The design library's index: every skill, template, example, design
//! system, craft rule, atom and scenario in `design-library/`, read once
//! from Open Design's own files (`open-design.json` manifests, `SKILL.md`
//! and `DESIGN.md` front-matter), so the gallery and the prompt builder
//! never walk the folder themselves.
//!
//! Adapted from Open Design (Apache-2.0): how its daemon finds and reads
//! these files (`apps/daemon/src/skills.ts`, `apps/daemon/src/plugins/`,
//! `apps/daemon/src/design-systems/`, `packages/plugin-runtime`).
//!
//! Loading never stops at a bad file: what can't be read becomes a
//! [`Problem`], which `--design-check` and the tests report.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::frontmatter;

/// What an entry is, by where it lives in the library.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Kind {
    /// A workflow the agent follows: `skills/<name>/SKILL.md`.
    Skill,
    /// A starting point with an example file: `design-templates/<name>/`.
    Template,
    /// A ready-made brief and its example output:
    /// `plugins/_official/examples/<name>/`.
    Example,
    /// A look: `design-systems/<name>/DESIGN.md`, with its manifest from
    /// `plugins/_official/design-systems/<name>/`.
    DesignSystem,
    /// A rule any designer applies: `craft/<slug>.md`.
    Craft,
    /// One step of a run (direction picker, critique, handoff…):
    /// `plugins/_official/atoms/<name>/`.
    Atom,
    /// A kind of run (new generation, refine, export, migration…):
    /// `plugins/_official/scenarios/<name>/`.
    Scenario,
    /// An image prompt and its picture: `plugins/_official/image-templates/`.
    ImageTemplate,
    /// A video or motion brief: `plugins/_official/video-templates/`.
    VideoTemplate,
    /// Installable, not preinstalled: `plugins/community/<name>/`.
    Community,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::Skill,
        Kind::Template,
        Kind::Example,
        Kind::DesignSystem,
        Kind::Craft,
        Kind::Atom,
        Kind::Scenario,
        Kind::ImageTemplate,
        Kind::VideoTemplate,
        Kind::Community,
    ];

    /// The name `--design-check <kind> <id>` takes: `skill`, `design-system`, …
    pub fn slug(self) -> &'static str {
        match self {
            Kind::Skill => "skill",
            Kind::Template => "template",
            Kind::Example => "example",
            Kind::DesignSystem => "design-system",
            Kind::Craft => "craft",
            Kind::Atom => "atom",
            Kind::Scenario => "scenario",
            Kind::ImageTemplate => "image-template",
            Kind::VideoTemplate => "video-template",
            Kind::Community => "community",
        }
    }

    pub fn from_slug(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.slug() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Skill => "skills",
            Kind::Template => "templates",
            Kind::Example => "examples",
            Kind::DesignSystem => "design systems",
            Kind::Craft => "craft rules",
            Kind::Atom => "atoms",
            Kind::Scenario => "scenarios",
            Kind::ImageTemplate => "image templates",
            Kind::VideoTemplate => "video templates",
            Kind::Community => "community plugins",
        }
    }
}

/// One thing in the library.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Entry {
    /// Its folder's name (a craft rule: its file's name without `.md`).
    pub id: String,
    pub kind: Option<Kind>,
    /// Its folder (a craft rule: its file), relative to the library root.
    pub dir: PathBuf,
    pub title: String,
    pub description: String,
    /// `od.mode`: deck, prototype, design-system, image, video, …
    pub mode: Option<String>,
    /// `od.scenario`: design, marketing, education, finance, …
    pub scenario: Option<String>,
    /// `od.surface`: web, image, video, hyperframes, audio.
    pub surface: Option<String>,
    /// `od.category`: professional-training, fundraising-pitch, …
    pub category: Option<String>,
    pub tags: Vec<String>,
    /// The instructions the agent reads, relative to the root.
    pub skill: Option<PathBuf>,
    /// The example output to show: an HTML page, a poster, a video.
    pub preview: Option<PathBuf>,
    /// The example output when it is on the web, not in the library (some
    /// image templates show a picture from their author's site).
    pub preview_url: Option<String>,
    /// The ready-made request (`od.useCase.query.en`).
    pub query: Option<String>,
    /// The craft rules it asks for (`od.craft.requires`).
    pub craft: Vec<String>,
    /// What its scripts need to run: `python`, `node`, `bash`.
    pub needs: BTreeSet<String>,
    /// Where it came from (`od.upstream`): the source it credits, or, for a
    /// catalogue entry, the only place the skill is.
    pub upstream: Option<String>,
    /// Open Design lists it by link only: its `SKILL.md` describes the skill
    /// and says to install the upstream bundle.
    pub catalogue: bool,
    /// For a catalogue entry: whether Forge ships the real upstream skill.
    pub upstream_shipped: bool,
    /// The full manifest (`open-design.json`), for what the index doesn't
    /// lift out: inputs, pipeline, capabilities.
    pub manifest: Option<Map<String, Value>>,
}

/// A file the index could not read as expected.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub path: PathBuf,
    pub message: String,
    /// Upstream ships it this way and Forge copes (an invalid YAML header
    /// whose fields are still read): reported, but not a failure.
    pub tolerated: bool,
}

/// The whole library, indexed.
#[derive(Debug, Default)]
pub struct Index {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    pub problems: Vec<Problem>,
    /// `UPSTREAM.json`: the sync's record of what was taken from where.
    pub upstream: Value,
}

/// Image, video and audio: fetched, not committed (`media.lock.json`).
const MEDIA: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "avif", "bmp", "ico", "mp4", "webm", "mov", "mp3",
    "wav", "ogg", "m4a", "gltf", "glb",
];

pub fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA.contains(&e.to_ascii_lowercase().as_str()))
}

impl Index {
    pub fn load(root: &Path) -> Index {
        let mut ix = Index {
            root: root.to_path_buf(),
            ..Index::default()
        };
        ix.upstream = std::fs::read_to_string(root.join("UPSTREAM.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        ix.load_skills("skills", Kind::Skill);
        ix.load_skills("design-templates", Kind::Template);
        ix.load_plugins("plugins/_official/examples", Kind::Example);
        ix.load_plugins("plugins/_official/atoms", Kind::Atom);
        ix.load_plugins("plugins/_official/scenarios", Kind::Scenario);
        ix.load_plugins("plugins/_official/image-templates", Kind::ImageTemplate);
        ix.load_plugins("plugins/_official/video-templates", Kind::VideoTemplate);
        ix.load_plugins("plugins/community", Kind::Community);
        ix.load_design_systems();
        ix.load_craft();
        ix.link_upstream_skills();
        ix.check_references();
        ix.entries.sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
        ix
    }

    /// Every entry of one kind, by id.
    pub fn of(&self, kind: Kind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == Some(kind))
    }

    pub fn get(&self, kind: Kind, id: &str) -> Option<&Entry> {
        self.of(kind).find(|e| e.id == id)
    }

    /// Problems that are failures (not the tolerated upstream quirks).
    pub fn errors(&self) -> impl Iterator<Item = &Problem> {
        self.problems.iter().filter(|p| !p.tolerated)
    }

    fn problem(&mut self, path: impl Into<PathBuf>, message: impl Into<String>, tolerated: bool) {
        self.problems.push(Problem {
            path: path.into(),
            message: message.into(),
            tolerated,
        });
    }

    /// The sub-folders of `rel`, sorted, skipping `_schema` and the like.
    fn folders(&self, rel: &str) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(self.root.join(rel))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with(['_', '.']))
            .collect();
        out.sort();
        out
    }

    fn read_doc(&mut self, rel: &Path) -> Option<frontmatter::Doc> {
        let text = std::fs::read_to_string(self.root.join(rel)).ok()?;
        let doc = frontmatter::parse(&text);
        if let Some(e) = &doc.error {
            self.problem(rel, format!("front-matter is not valid YAML ({e}); read line by line"), true);
        }
        Some(doc)
    }

    /// `skills/<name>/SKILL.md` and `design-templates/<name>/SKILL.md`.
    fn load_skills(&mut self, base: &str, kind: Kind) {
        for name in self.folders(base) {
            let dir = Path::new(base).join(&name);
            let skill = dir.join("SKILL.md");
            let Some(doc) = self.read_doc(&skill) else {
                self.problem(&dir, "no SKILL.md", false);
                continue;
            };
            let mut e = from_skill(&name, kind, &dir, &skill, &doc);
            if kind == Kind::Template {
                e.preview = ["example.html", "index.html"]
                    .iter()
                    .map(|f| dir.join(f))
                    .find(|p| self.root.join(p).is_file());
            }
            e.needs = needs(&self.root.join(&dir));
            self.entries.push(e);
        }
    }

    /// `<base>/<name>/open-design.json`, with its `SKILL.md` when it has one.
    fn load_plugins(&mut self, base: &str, kind: Kind) {
        for name in self.folders(base) {
            let dir = Path::new(base).join(&name);
            let Some(e) = self.read_plugin(&name, kind, &dir) else {
                continue;
            };
            self.entries.push(e);
        }
    }

    fn read_plugin(&mut self, name: &str, kind: Kind, dir: &Path) -> Option<Entry> {
        let path = dir.join("open-design.json");
        let text = match std::fs::read_to_string(self.root.join(&path)) {
            Ok(t) => t,
            Err(_) => {
                self.problem(dir, "no open-design.json", false);
                return None;
            }
        };
        let manifest: Map<String, Value> = match serde_json::from_str(&text) {
            Ok(Value::Object(m)) => m,
            Ok(_) => {
                self.problem(&path, "open-design.json is not an object", false);
                return None;
            }
            Err(e) => {
                self.problem(&path, format!("open-design.json: {e}"), false);
                return None;
            }
        };
        let od = manifest.get("od").cloned().unwrap_or(Value::Null);
        if od["kind"].as_str().is_none() {
            self.problem(&path, "open-design.json has no od.kind", false);
        }
        // The agent's instructions: the manifest's own skill path, else SKILL.md.
        let skill = manifest
            .get("compat")
            .and_then(|c| c["agentSkills"].get(0))
            .and_then(|s| s["path"].as_str())
            .or_else(|| od["context"]["skills"].get(0).and_then(|s| s["path"].as_str()))
            .map(|p| clean(dir, p))
            .or_else(|| Some(dir.join("SKILL.md")).filter(|p| self.root.join(p).is_file()));
        let doc = skill.as_ref().and_then(|p| self.read_doc(p));
        let text_of = |key: &str| manifest.get(key).and_then(Value::as_str).map(str::to_string);
        let shown = od["preview"]["entry"]
            .as_str()
            .or_else(|| od["preview"]["poster"].as_str())
            .or_else(|| od["preview"]["video"].as_str());
        let web = shown.filter(|p| p.starts_with("https://") || p.starts_with("http://"));
        let preview = shown.filter(|_| web.is_none()).map(|p| clean(dir, p));
        let mut e = Entry {
            id: name.to_string(),
            kind: Some(kind),
            dir: dir.to_path_buf(),
            title: text_of("title").unwrap_or_else(|| name.to_string()),
            description: text_of("description").unwrap_or_default().trim().to_string(),
            mode: od["mode"].as_str().map(str::to_string),
            scenario: od["scenario"].as_str().map(str::to_string),
            surface: od["surface"].as_str().map(str::to_string),
            category: od["category"].as_str().map(str::to_string),
            tags: strings(manifest.get("tags")),
            skill,
            preview,
            preview_url: web.map(str::to_string),
            query: query(&od["useCase"]["query"]),
            craft: strings(od.get("craft").and_then(|c| c.get("requires"))),
            needs: needs(&self.root.join(dir)),
            manifest: Some(manifest),
            ..Entry::default()
        };
        if let Some(doc) = doc {
            if e.description.is_empty() {
                e.description = doc.str("description").unwrap_or_default().trim().to_string();
            }
            if e.craft.is_empty() {
                e.craft = doc.strings("od.craft.requires");
            }
        }
        Some(e)
    }

    /// One entry per design system: the full kit in `design-systems/<name>/`,
    /// with the official manifest from `plugins/_official/design-systems/`
    /// where there is one.
    fn load_design_systems(&mut self) {
        let kits = self.folders("design-systems");
        let official = self.folders("plugins/_official/design-systems");
        let names: BTreeSet<&String> = kits.iter().chain(&official).collect();
        for name in names {
            let kit = Path::new("design-systems").join(name);
            let plug = Path::new("plugins/_official/design-systems").join(name);
            let has_kit = self.root.join(&kit).join("DESIGN.md").is_file();
            let mut e = if official.contains(name) {
                match self.read_plugin(name, Kind::DesignSystem, &plug) {
                    Some(e) => e,
                    None => continue,
                }
            } else {
                Entry {
                    id: name.clone(),
                    kind: Some(Kind::DesignSystem),
                    ..Entry::default()
                }
            };
            e.dir = if has_kit { kit.clone() } else { plug.clone() };
            let design = e.dir.join("DESIGN.md");
            match self.read_doc(&design) {
                Some(doc) => {
                    if e.title.is_empty() || e.title == *name {
                        e.title = doc
                            .str("name")
                            .or_else(|| doc.str("title"))
                            .map(str::to_string)
                            .or_else(|| heading(&doc.body))
                            .unwrap_or_else(|| name.clone());
                    }
                    if e.description.is_empty() {
                        e.description = doc.str("description").unwrap_or_default().trim().to_string();
                    }
                    e.skill = Some(design);
                }
                None => self.problem(&e.dir, "design system without DESIGN.md", false),
            }
            if e.mode.is_none() {
                e.mode = Some("design-system".into());
            }
            self.entries.push(e);
        }
    }

    /// `craft/<slug>.md`, but not the folder's README or its list of
    /// sections still to come.
    fn load_craft(&mut self) {
        let mut files: Vec<String> = std::fs::read_dir(self.root.join("craft"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".md") && n != "README.md" && n != "FUTURE_SECTIONS.md")
            .collect();
        files.sort();
        for file in files {
            let rel = Path::new("craft").join(&file);
            let Some(doc) = self.read_doc(&rel) else {
                continue;
            };
            let id = file.trim_end_matches(".md").to_string();
            self.entries.push(Entry {
                title: doc
                    .str("title")
                    .map(str::to_string)
                    .or_else(|| heading(&doc.body))
                    .unwrap_or_else(|| id.clone()),
                description: doc.str("description").unwrap_or_default().trim().to_string(),
                id,
                kind: Some(Kind::Craft),
                dir: rel.clone(),
                skill: Some(rel),
                ..Entry::default()
            });
        }
    }

    /// Catalogue entries (a `SKILL.md` that only links to another repository)
    /// point at the real skill when the sync fetched it into `upstream/`.
    fn link_upstream_skills(&mut self) {
        let shipped: BTreeMap<String, PathBuf> = self.upstream["skills"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| {
                let stub = s["stub"].as_str()?;
                let (owner, repo) = owner_repo(s["repo"].as_str()?)?;
                let path = Path::new("upstream")
                    .join(format!("{owner}__{repo}"))
                    .join(s["path"].as_str()?)
                    .join("SKILL.md");
                Some((stub.to_string(), path))
            })
            .collect();
        let root = self.root.clone();
        let mut missing = vec![];
        for e in self.entries.iter_mut().filter(|e| e.kind == Some(Kind::Skill)) {
            if let Some(path) = shipped.get(&e.id) {
                if root.join(path).is_file() {
                    e.skill = Some(path.clone());
                    e.upstream_shipped = true;
                    e.needs = needs(&root.join(path.parent().unwrap_or(path)));
                } else {
                    missing.push(path.clone());
                }
            }
        }
        for p in missing {
            self.problem(p, "upstream skill listed in UPSTREAM.json is missing", false);
        }
    }

    /// Every file an entry names exists: its skill, its preview (media is
    /// checked against `media.lock.json`, since it is fetched), and every
    /// craft rule it asks for, or that rule is one `craft/FUTURE_SECTIONS.md`
    /// lists as still to come. A preview upstream names but doesn't ship is
    /// dropped (the gallery shows none) and listed as tolerated.
    fn check_references(&mut self) {
        let locked: BTreeSet<String> = std::fs::read_to_string(self.root.join("media.lock.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map(|v| {
                v["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f["path"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let craft: BTreeSet<String> = self.of(Kind::Craft).map(|e| e.id.clone()).collect();
        let future: BTreeSet<String> = std::fs::read_to_string(self.root.join("craft/FUTURE_SECTIONS.md"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.trim().strip_prefix("- "))
            .map(|s| s.trim().trim_matches('`').to_string())
            .collect();
        let present = |p: &Path| {
            if is_media(p) {
                locked.contains(&slash(p))
            } else {
                self.root.join(p).is_file()
            }
        };
        let mut found = vec![];
        let mut no_preview = vec![];
        for (i, e) in self.entries.iter().enumerate() {
            if let Some(p) = e.skill.as_ref().filter(|p| !present(p)) {
                found.push((p.clone(), format!("{}: its instructions are not there", e.id), false));
            }
            if let Some(p) = e.preview.as_ref().filter(|p| !present(p)) {
                found.push((p.clone(), format!("{}: upstream names this preview but doesn't ship it", e.id), true));
                no_preview.push(i);
            }
            for slug in &e.craft {
                if !craft.contains(slug) && !future.contains(slug) {
                    found.push((e.dir.clone(), format!("asks for craft rule `{slug}`, which doesn't exist"), false));
                }
            }
        }
        for i in no_preview {
            self.entries[i].preview = None;
        }
        for (p, m, tolerated) in found {
            self.problem(p, m, tolerated);
        }
    }

    /// How many entries of each kind, for `--design-check`.
    pub fn counts(&self) -> BTreeMap<Kind, usize> {
        let mut out = BTreeMap::new();
        for e in &self.entries {
            if let Some(k) = e.kind {
                *out.entry(k).or_default() += 1;
            }
        }
        out
    }
}

fn from_skill(name: &str, kind: Kind, dir: &Path, skill: &Path, doc: &frontmatter::Doc) -> Entry {
    let od = |k: &str| doc.path(&format!("od.{k}")).and_then(Value::as_str).map(str::to_string);
    Entry {
        id: name.to_string(),
        kind: Some(kind),
        dir: dir.to_path_buf(),
        title: doc
            .str("en_name")
            .or_else(|| doc.str("title"))
            .or_else(|| doc.str("name"))
            .map(str::to_string)
            .unwrap_or_else(|| name.to_string()),
        description: doc
            .str("en_description")
            .or_else(|| doc.str("description"))
            .unwrap_or_default()
            .trim()
            .to_string(),
        mode: od("mode"),
        scenario: od("scenario").or_else(|| doc.str("scenario").map(str::to_string)),
        surface: od("surface"),
        category: od("category").or_else(|| doc.str("category").map(str::to_string)),
        tags: doc.strings("tags"),
        skill: Some(skill.to_path_buf()),
        craft: doc.strings("od.craft.requires"),
        upstream: od("upstream"),
        catalogue: doc
            .body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains("install the upstream bundle"),
        ..Entry::default()
    }
}

/// `./SKILL.md` inside `dir` as a library-relative path.
fn clean(dir: &Path, p: &str) -> PathBuf {
    let mut out = dir.to_path_buf();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            _ => out.push(part),
        }
    }
    out
}

fn slash(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}

/// The English ready-made request: a string, or `{ "en": … }`.
fn query(v: &Value) -> Option<String> {
    v.as_str()
        .or_else(|| v["en"].as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The first Markdown heading.
fn heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.trim_start().strip_prefix("# "))
        .map(|h| h.trim().to_string())
}

/// `https://github.com/<owner>/<repo>[/…]` → (owner, repo).
fn owner_repo(url: &str) -> Option<(String, String)> {
    let rest = url.split("github.com/").nth(1)?;
    let mut parts = rest.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.trim_end_matches(".git").to_string();
    Some((owner, repo))
}

/// What an entry's own scripts need: Python for `.py`, Node for `.mjs`,
/// `.cjs` or a `package.json`, a shell for `.sh`. Scripts that run in the
/// page (`.js` under `assets/`) need nothing.
fn needs(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if name != "node_modules" && name != "assets" {
                    stack.push(p);
                }
                continue;
            }
            match p.extension().and_then(|x| x.to_str()) {
                Some("py") => {
                    out.insert("python".to_string());
                }
                Some("mjs" | "cjs") => {
                    out.insert("node".to_string());
                }
                Some("sh") => {
                    out.insert("bash".to_string());
                }
                _ if name == "package.json" => {
                    out.insert("node".to_string());
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Index {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("design-library");
        Index::load(&root)
    }

    #[test]
    fn the_whole_library_loads_without_errors() {
        let ix = library();
        let errors: Vec<String> = ix
            .errors()
            .map(|p| format!("{}: {}", p.path.display(), p.message))
            .collect();
        assert!(errors.is_empty(), "{} problems:\n{}", errors.len(), errors.join("\n"));
    }

    #[test]
    fn counts_match_what_the_sync_recorded() {
        let ix = library();
        let c = &ix.upstream["counts"];
        let n = |k: Kind| ix.of(k).count() as u64;
        assert_eq!(Some(n(Kind::Skill)), c["skills"].as_u64());
        let catalogue = ix.of(Kind::Skill).filter(|e| e.catalogue).count() as u64;
        assert_eq!(Some(catalogue), c["catalogue_skills"].as_u64());
        assert_eq!(Some(n(Kind::Template)), c["design_templates"].as_u64());
        assert_eq!(Some(n(Kind::Craft)), c["craft"].as_u64());
        assert_eq!(Some(n(Kind::Example)), c["official_examples"].as_u64());
        assert_eq!(Some(n(Kind::Atom)), c["official_atoms"].as_u64());
        assert_eq!(Some(n(Kind::Scenario)), c["official_scenarios"].as_u64());
        assert_eq!(Some(n(Kind::ImageTemplate)), c["official_image_templates"].as_u64());
        assert_eq!(Some(n(Kind::VideoTemplate)), c["official_video_templates"].as_u64());
        // Every kit, plus the official manifests that have no kit folder.
        assert!(n(Kind::DesignSystem) >= c["design_systems"].as_u64().unwrap());
    }

    #[test]
    fn a_deck_example_has_its_brief_preview_and_skill() {
        let ix = library();
        let e = ix.get(Kind::Example, "html-ppt-course-module").expect("course module");
        assert_eq!(e.mode.as_deref(), Some("deck"));
        assert_eq!(e.scenario.as_deref(), Some("education"));
        assert!(e.query.as_deref().is_some_and(|q| q.contains("onboarding")));
        assert_eq!(e.preview.as_deref(), Some(Path::new("plugins/_official/examples/html-ppt-course-module/example.html")));
        assert!(e.skill.as_ref().is_some_and(|s| ix.root.join(s).is_file()));
    }

    #[test]
    fn design_systems_merge_kit_and_manifest() {
        let ix = library();
        // In both trees: the kit's folder, the manifest's data.
        let apple = ix.get(Kind::DesignSystem, "apple").expect("apple");
        assert_eq!(apple.dir, Path::new("design-systems/apple"));
        assert!(apple.manifest.is_some());
        assert!(apple.query.is_some());
        // Only a kit, no official manifest.
        let hud = ix.get(Kind::DesignSystem, "hud").expect("hud");
        assert!(hud.manifest.is_none());
        assert_eq!(hud.skill.as_deref(), Some(Path::new("design-systems/hud/DESIGN.md")));
    }

    #[test]
    fn craft_rules_are_indexed_by_slug() {
        let ix = library();
        for slug in ["typography", "color", "anti-ai-slop"] {
            assert!(ix.get(Kind::Craft, slug).is_some(), "{slug}");
        }
    }

    #[test]
    fn scripts_say_what_they_need() {
        let ix = library();
        let pet = ix.get(Kind::Skill, "hatch-pet").expect("hatch-pet");
        assert!(pet.needs.contains("python"));
        let clone = ix.get(Kind::Skill, "web-clone").expect("web-clone");
        assert!(clone.needs.contains("node"));
    }

    #[test]
    fn relative_paths_resolve_inside_the_entry() {
        assert_eq!(clean(Path::new("a/b"), "./SKILL.md"), Path::new("a/b/SKILL.md"));
        assert_eq!(clean(Path::new("a/b"), "../c/x.html"), Path::new("a/c/x.html"));
        assert_eq!(owner_repo("https://github.com/anthropics/skills/tree/main/skills/pdf"),
            Some(("anthropics".into(), "skills".into())));
    }
}
