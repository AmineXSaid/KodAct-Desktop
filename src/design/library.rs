//! The design library's index: every skill, template, example, design
//! system, craft rule, atom and scenario in `design-library/`, read once
//! from Open Design's own files (`open-design.json` manifests, `SKILL.md`
//! and `DESIGN.md` front-matter, a design system's `manifest.json`), so the
//! gallery and the prompt builder never walk the folder themselves.
//!
//! Adapted from Open Design (Apache-2.0): how its daemon finds and reads
//! these files (`apps/daemon/src/skills.ts`, `apps/daemon/src/plugins/`,
//! `apps/daemon/src/design-systems/`, `packages/plugin-runtime`). Where its
//! rules decide what a person sees (which `DESIGN.md`, which preview, which
//! craft list), they are followed exactly; `plugins/spec/` is left out, as
//! Open Design leaves it out: it is examples for plugin authors.
//!
//! Loading never stops at a bad file: what can't be read becomes a
//! [`Problem`], which `--design-check` and the tests report.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::frontmatter::{self, Doc};
use super::skill_meta::{self, SkillMeta};

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
    /// A look: `design-systems/<name>/` (its `DESIGN.md`, tokens and
    /// manifest), with its gallery card from
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

/// Open Design's old skill names, still accepted: (old, new).
const ALIASES: [(&str, &str); 3] = [
    ("editorial-collage", "open-design-landing"),
    ("editorial-collage-deck", "open-design-landing-deck"),
    ("taste-skill", "design-taste-frontend"),
];

/// Where Open Design serves its own repository on the web: a preview there
/// is a file the library already has.
const REPO_URLS: [&str; 2] = [
    "https://cdn.jsdelivr.net/gh/nexu-io/open-design@main/",
    "https://raw.githubusercontent.com/nexu-io/open-design/main/",
];

/// What 150 design systems' `manifest.json` says instead of describing
/// itself: their real summary is in `DESIGN.md`.
const BOILERPLATE: &str = "Bundled OpenDesign package for";

/// One thing in the library.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Entry {
    /// Its folder's name, unique within its kind. A craft rule: its file's
    /// name without `.md`; a card made from one of a template's example
    /// files: `<template>:<file>`.
    pub id: String,
    /// Open Design's own id for it: a skill's front-matter `name`
    /// (`design-taste-frontend` for `skills/taste-skill`), a plugin's
    /// manifest `name` (`example-article-magazine`).
    pub name: String,
    pub kind: Option<Kind>,
    /// Its folder (a craft rule: its file), relative to the library root.
    pub dir: PathBuf,
    pub title: String,
    pub description: String,
    /// `od.mode`: deck, prototype, design-system, image, video, …
    pub mode: Option<String>,
    /// `od.scenario`: design, marketing, education, finance, …
    pub scenario: Option<String>,
    /// `od.surface`: web, image, video, audio (hyperframes for some video).
    pub surface: Option<String>,
    /// `desktop` or `mobile`, for a prototype.
    pub platform: Option<String>,
    /// `od.category` (professional-training, fundraising-pitch, …); a
    /// design system's own category (`Media & Consumer`).
    pub category: Option<String>,
    /// The template gallery's grouping (`slides`, `poster`, `card`…): a
    /// top-level `category` in some `SKILL.md`, not `od.category`.
    pub gallery_kind: Option<String>,
    pub tags: Vec<String>,
    /// What a person might say that should bring it up.
    pub triggers: Vec<String>,
    /// The instructions the agent reads, relative to the root.
    pub skill: Option<PathBuf>,
    /// Another skill it runs, by Open Design's name (`{ref: web-clone}`),
    /// when it has no instructions of its own.
    pub skill_ref: Option<String>,
    /// The example output to show: an HTML page, a poster, a video.
    pub preview: Option<PathBuf>,
    /// The example output when it is on the web, not in the library (some
    /// image and video templates show their author's copy).
    pub preview_url: Option<String>,
    /// The ready-made request (`od.useCase.query.en`), its `{{inputs}}`
    /// still in place.
    pub query: Option<String>,
    /// What to ask for with a skill: its `od.example_prompt`, else the first
    /// sentence of its description.
    pub example_prompt: String,
    /// The craft rules it asks for: `od.craft.requires` in a `SKILL.md`,
    /// `od.context.craft` in a manifest.
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
    /// Not for the gallery: atoms, Open Design's router and its internal
    /// prompt package (`od.hidden`, `od.strategy`), and a template whose
    /// example files are cards of their own.
    pub hidden: bool,
    /// Gallery order, lower first (`od.featured`).
    pub featured: Option<f64>,
    /// For a card made from one of a template's `examples/*.html`: the
    /// template's id.
    pub parent: Option<String>,
    /// The same thing under another kind: an example and the template or
    /// skill it was made from.
    pub twin: Option<(Kind, String)>,
    pub license: Option<String>,
    /// Who to credit, for a licence that asks for it (CC-BY).
    pub credit: Option<String>,
    /// The full plugin manifest (`open-design.json`), for what the index
    /// doesn't lift out: inputs, pipeline, capabilities, translations.
    pub manifest: Option<Map<String, Value>>,
    /// A design system's own manifest (`design-systems/<id>/manifest.json`):
    /// its files, usage notes, tokens and preview pages.
    pub kit: Option<Map<String, Value>>,
}

/// A file the index could not read as expected.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub path: PathBuf,
    pub message: String,
    /// Upstream ships it this way and Forge copes (an invalid YAML header
    /// that is still read, a preview upstream names but doesn't ship):
    /// reported, but not a failure.
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
    /// The media files `media.lock.json` pins (fetched, not committed), as
    /// library paths with `/`.
    media: BTreeSet<String>,
}

/// Image, video and audio: fetched, not committed (`media.lock.json`).
const MEDIA: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "avif", "bmp", "ico", "mp4", "webm", "mov", "mp3", "wav",
    "ogg", "m4a", "gltf", "glb",
];

pub fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA.contains(&e.to_ascii_lowercase().as_str()))
}

/// The category of the design systems you captured yourself.
pub const OWN_CATEGORY: &str = "Captured by you";

impl Index {
    pub fn load(root: &Path) -> Index {
        let read_json = |name: &str| {
            std::fs::read_to_string(root.join(name))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .unwrap_or(Value::Null)
        };
        let media = read_json("media.lock.json")["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f["path"].as_str().map(str::to_string))
            .collect();
        let mut ix = Index {
            root: root.to_path_buf(),
            upstream: read_json("UPSTREAM.json"),
            media,
            ..Index::default()
        };
        ix.load_skills("skills", Kind::Skill);
        ix.load_skills("design-templates", Kind::Template);
        ix.load_plugins("plugins/_official/examples", Kind::Example);
        ix.load_plugins("plugins/_official/atoms", Kind::Atom);
        ix.load_plugins("plugins/_official/scenarios", Kind::Scenario);
        ix.load_plugins("plugins/_official/image-templates", Kind::ImageTemplate);
        ix.load_plugins("plugins/_official/video-templates", Kind::VideoTemplate);
        ix.load_plugins("plugins/community", Kind::Community);
        ix.load_design_systems();
        if let Some(own) = super::own_systems() {
            ix.load_own_systems(&own);
        }
        ix.load_craft();
        ix.link_upstream_skills();
        ix.resolve_skill_refs();
        ix.link_twins();
        ix.check_references();
        ix.entries
            .sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
        ix
    }

    /// Every entry of one kind, by id.
    pub fn of(&self, kind: Kind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == Some(kind))
    }

    /// An entry by its folder id, or by Open Design's name for it (old
    /// names included).
    pub fn get(&self, kind: Kind, key: &str) -> Option<&Entry> {
        let key = ALIASES
            .iter()
            .find(|(old, _)| *old == key)
            .map_or(key, |(_, new)| new);
        self.of(kind)
            .find(|e| e.id == key)
            .or_else(|| self.of(kind).find(|e| e.name == key))
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

    /// Whether a library path is there: media against `media.lock.json`
    /// (it may not be fetched yet), anything else on disk.
    fn present(&self, p: &Path) -> bool {
        is_media(p) && self.media.contains(&slash(p)) || self.root.join(p).is_file()
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

    /// A Markdown file and its front-matter; a missing or unreadable file
    /// (a user's skill saved as UTF-16, say) is a problem, not a panic.
    fn read_doc(&mut self, rel: &Path) -> Option<Doc> {
        let text = match std::fs::read_to_string(self.root.join(rel)) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.problem(rel, "is not there", false);
                return None;
            }
            Err(e) => {
                self.problem(rel, format!("can't be read as UTF-8 text: {e}"), false);
                return None;
            }
        };
        let doc = frontmatter::parse(&text);
        if let Some(e) = &doc.error {
            self.problem(
                rel,
                format!("front-matter is not valid YAML ({e}); read as Open Design reads it"),
                true,
            );
        }
        Some(doc)
    }

    /// A JSON manifest; `None` (and a problem) when it can't be read.
    fn read_manifest(&mut self, rel: &Path) -> Option<Map<String, Value>> {
        let text = match std::fs::read_to_string(self.root.join(rel)) {
            Ok(t) => t,
            Err(e) => {
                self.problem(rel, format!("can't be read: {e}"), false);
                return None;
            }
        };
        match serde_json::from_str(&text) {
            Ok(Value::Object(m)) => Some(m),
            Ok(_) => {
                self.problem(rel, "is not a JSON object", false);
                None
            }
            Err(e) => {
                self.problem(rel, format!("is not valid JSON: {e}"), false);
                None
            }
        }
    }

    /// `skills/<name>/SKILL.md` and `design-templates/<name>/SKILL.md`, read
    /// as Open Design's skill list reads them, each with its example and the
    /// cards its `examples/*.html` make.
    fn load_skills(&mut self, base: &str, kind: Kind) {
        for folder in self.folders(base) {
            let dir = Path::new(base).join(&folder);
            let skill = dir.join("SKILL.md");
            let Some(doc) = self.read_doc(&skill) else {
                continue;
            };
            let mut e = from_skill(&folder, kind, &dir, &skill, &doc);
            e.preview = example_preview(&self.root, &dir);
            e.needs = needs(&self.root.join(&dir));
            // A template that is also packaged as a plugin (aster-atelier):
            // its manifest gives the ready-made request.
            let packaged = dir.join("open-design.json");
            if self.root.join(&packaged).is_file() {
                if let Some(m) = self.read_manifest(&packaged) {
                    e.query = query(&m.get("od").cloned().unwrap_or_default()["useCase"]["query"]);
                    e.manifest = Some(m);
                }
            }
            let cards = derived_examples(&self.root, &e);
            e.hidden = !cards.is_empty();
            self.entries.push(e);
            self.entries.extend(cards);
        }
    }

    /// `<base>/<name>/open-design.json`, with its `SKILL.md` when it has one.
    fn load_plugins(&mut self, base: &str, kind: Kind) {
        for folder in self.folders(base) {
            let dir = Path::new(base).join(&folder);
            if let Some(e) = self.read_plugin(&folder, kind, &dir) {
                self.entries.push(e);
            }
        }
    }

    fn read_plugin(&mut self, folder: &str, kind: Kind, dir: &Path) -> Option<Entry> {
        let path = dir.join("open-design.json");
        let manifest = self.read_manifest(&path)?;
        let od = manifest.get("od").cloned().unwrap_or(Value::Null);
        if od["kind"].as_str().is_none() {
            self.problem(&path, "has no od.kind", false);
        }
        // Its instructions: `od.context.skills` (a path, or another skill by
        // name), then `compat.agentSkills`, then a SKILL.md beside it.
        let listed = od["context"]["skills"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                manifest
                    .get("compat")
                    .and_then(|c| c["agentSkills"].as_array())
                    .into_iter()
                    .flatten(),
            );
        let (mut skill, mut skill_ref) = (None, None);
        let mut escapes = vec![];
        for s in listed {
            match (s["path"].as_str(), s["ref"].as_str()) {
                (Some(p), _) if p.contains('/') || p.ends_with(".md") => {
                    match join_inside(dir, p) {
                        Some(p) => {
                            skill.get_or_insert(p);
                        }
                        None => escapes.push(p.to_string()),
                    }
                }
                // A bare path is a skill's name.
                (Some(name), _) | (None, Some(name)) => {
                    skill_ref.get_or_insert(name.to_string());
                }
                _ => {}
            }
        }
        for p in escapes {
            self.problem(&path, format!("names `{p}`, outside its folder"), false);
        }
        if skill.is_none() {
            skill = Some(dir.join("SKILL.md")).filter(|p| self.root.join(p).is_file());
        }
        if skill.is_some() {
            skill_ref = None;
        }
        let doc = match &skill {
            Some(p) => self.read_doc(p),
            None => None,
        };
        let meta = doc.as_ref().map(|d| SkillMeta::from_doc(folder, d));
        // The manifest wins; the SKILL.md fills what it leaves out.
        let own = |key: &str| od[key].as_str().map(str::to_string);
        let fill = |key: &str| {
            doc.as_ref()
                .and_then(|d| d.path(&format!("od.{key}")))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let text_of = |key: &str| {
            manifest
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let name = text_of("name").unwrap_or_else(|| folder.to_string());
        let mut craft = skill_meta::slugs(
            od["context"]["craft"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        );
        if craft.is_empty() {
            craft = meta
                .as_ref()
                .map(|m| m.craft_requires.clone())
                .unwrap_or_default();
        }
        let license = text_of("license");
        let credit = license
            .as_deref()
            .filter(|l| l.starts_with("CC-BY"))
            .and_then(|_| manifest.get("author"))
            .and_then(|a| {
                let name = a["name"].as_str()?;
                Some(match a["url"].as_str() {
                    Some(url) => format!("{name} ({url})"),
                    None => name.to_string(),
                })
            });
        let (preview, preview_url) = self.plugin_preview(dir, &od["preview"]);
        Some(Entry {
            id: folder.to_string(),
            title: text_of("title")
                .or_else(|| {
                    meta.as_ref()
                        .and_then(|m| m.display_name.get("en").cloned())
                })
                .unwrap_or_else(|| humanize(&name)),
            name,
            kind: Some(kind),
            dir: dir.to_path_buf(),
            description: text_of("description")
                .or_else(|| meta.as_ref().map(|m| m.description.clone()))
                .unwrap_or_default()
                .trim()
                .to_string(),
            mode: own("mode").or_else(|| fill("mode")),
            scenario: own("scenario").or_else(|| fill("scenario")),
            surface: own("surface").or_else(|| fill("surface")),
            platform: own("platform").or_else(|| fill("platform")),
            category: own("category"),
            tags: strings(manifest.get("tags")),
            triggers: meta
                .as_ref()
                .map(|m| m.triggers.clone())
                .unwrap_or_default(),
            skill,
            skill_ref,
            preview,
            preview_url,
            query: query(&od["useCase"]["query"]),
            example_prompt: meta
                .as_ref()
                .map(|m| m.example_prompt.clone())
                .unwrap_or_default(),
            craft,
            needs: needs(&self.root.join(dir)),
            upstream: fill("upstream"),
            hidden: kind == Kind::Atom || od["hidden"] == true || od.get("strategy").is_some(),
            featured: od["featured"].as_f64(),
            license,
            credit,
            manifest: Some(manifest),
            ..Entry::default()
        })
    }

    /// A plugin's preview: its `od.preview` file, the library's copy of one
    /// Open Design serves from its own repository, a web address otherwise,
    /// and, when the named file isn't there, the example Open Design would
    /// find in the folder instead.
    fn plugin_preview(&self, dir: &Path, preview: &Value) -> (Option<PathBuf>, Option<String>) {
        let shown = ["entry", "poster", "video"]
            .iter()
            .find_map(|k| preview[*k].as_str());
        let Some(shown) = shown else {
            return (None, None);
        };
        if shown.starts_with("https://") || shown.starts_with("http://") {
            let local = REPO_URLS
                .iter()
                .find_map(|u| shown.strip_prefix(u))
                .and_then(|rest| join_inside(Path::new(""), rest))
                .filter(|p| self.present(p));
            return match local {
                Some(p) => (Some(p), None),
                None => (None, Some(shown.to_string())),
            };
        }
        match join_inside(dir, shown) {
            Some(p) if self.present(&p) => (Some(p), None),
            // Not there: what Open Design's preview search finds, else the
            // named file (reported later as not shipped).
            named => (example_preview(&self.root, dir).or(named), None),
        }
    }

    /// One entry per design system, from `design-systems/<id>/`: its
    /// `DESIGN.md` and `manifest.json`, which Open Design reads at runtime.
    /// The card in `plugins/_official/design-systems/<id>/` adds the
    /// gallery's request, inputs and translations; its own `DESIGN.md` copy
    /// is older (seven of them lack the usage guardrails) and never used.
    fn load_design_systems(&mut self) {
        let cards: BTreeSet<String> = self
            .folders("plugins/_official/design-systems")
            .into_iter()
            .collect();
        let kits = self.folders("design-systems");
        for name in cards.iter().filter(|n| !kits.contains(n)) {
            self.problem(
                Path::new("plugins/_official/design-systems").join(name),
                "a design system card with no design-systems/ folder (its DESIGN.md copy is stale; not used)",
                false,
            );
        }
        for folder in kits {
            let dir = Path::new("design-systems").join(&folder);
            let design = dir.join("DESIGN.md");
            let Some(doc) = self.read_doc(&design) else {
                continue;
            };
            let kit = self.read_kit(&folder, &dir);
            let card = if cards.contains(&folder) {
                self.read_manifest(
                    &Path::new("plugins/_official/design-systems")
                        .join(&folder)
                        .join("open-design.json"),
                )
            } else {
                None
            };
            let od = card
                .as_ref()
                .and_then(|c| c.get("od"))
                .cloned()
                .unwrap_or(Value::Null);
            let kit_str = |key: &str| {
                kit.as_ref()
                    .and_then(|k| k.get(key))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            };
            let quoted = |label: &str| {
                doc.body.lines().find_map(|l| {
                    let rest = l.trim().strip_prefix('>')?.trim();
                    let (k, v) = rest.split_once(':')?;
                    k.trim()
                        .eq_ignore_ascii_case(label)
                        .then(|| v.trim().to_string())
                })
            };
            let description = kit_str("description")
                .filter(|d| !d.starts_with(BOILERPLATE))
                .or_else(|| Some(summarize(&doc.body)).filter(|s| !s.is_empty()))
                .or_else(|| {
                    card.as_ref()
                        .and_then(|c| c.get("description"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let preview = kit
                .as_ref()
                .and_then(|k| k.get("preview"))
                .and_then(|p| p["pages"].get(0))
                .and_then(|p| p["path"].as_str())
                .and_then(|p| join_inside(&dir, p));
            self.entries.push(Entry {
                id: folder.clone(),
                name: kit_str("id").unwrap_or_else(|| folder.clone()),
                kind: Some(Kind::DesignSystem),
                title: kit_str("name")
                    .or_else(|| heading(&doc.body).map(|h| clean_title(&h)))
                    .unwrap_or_else(|| humanize(&folder)),
                description,
                mode: Some("design-system".into()),
                scenario: od["scenario"].as_str().map(str::to_string),
                surface: Some(quoted("surface").unwrap_or_else(|| "web".into())),
                category: kit_str("category").or_else(|| quoted("category")),
                tags: strings(card.as_ref().and_then(|c| c.get("tags"))),
                skill: Some(design),
                preview,
                query: query(&od["useCase"]["query"]),
                license: card
                    .as_ref()
                    .and_then(|c| c.get("license"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                dir,
                manifest: card,
                kit,
                ..Entry::default()
            });
        }
    }

    /// One entry per design system you captured (`own_systems`): each
    /// folder in `own` with a `DESIGN.md`, named for its site. Its paths
    /// are absolute, as it is outside the library root.
    fn load_own_systems(&mut self, own: &Path) {
        let mut folders: Vec<PathBuf> = std::fs::read_dir(own)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("DESIGN.md").is_file())
            .collect();
        folders.sort();
        for dir in folders {
            let Some(folder) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
                continue;
            };
            if folder.starts_with(['_', '.']) || self.get(Kind::DesignSystem, &folder).is_some() {
                continue;
            }
            let design = dir.join("DESIGN.md");
            let Some(doc) = self.read_doc(&design) else {
                continue;
            };
            let title = heading(&doc.body)
                .map(|h| {
                    h.trim_start_matches("Design System Captured from ")
                        .trim()
                        .to_string()
                })
                .filter(|h| !h.is_empty())
                .unwrap_or_else(|| folder.clone());
            // What section 1 says of it, else its own summary line.
            let description = doc
                .body
                .split("\n## ")
                .nth(1)
                .and_then(|sec| sec.split("\n\n").nth(1))
                .map(|p| p.replace('`', "").trim().to_string())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| summarize(&doc.body));
            let preview = Some(dir.join("preview.html")).filter(|p| p.is_file());
            self.entries.push(Entry {
                id: folder.clone(),
                name: folder.clone(),
                kind: Some(Kind::DesignSystem),
                title,
                description,
                preview,
                mode: Some("design-system".into()),
                surface: Some("web".into()),
                category: Some(OWN_CATEGORY.into()),
                tags: vec!["captured".into()],
                skill: Some(design),
                license: Some(format!(
                    "Captured from {folder} for your own reference; not part of the library"
                )),
                dir,
                ..Entry::default()
            });
        }
    }

    /// `design-systems/<id>/manifest.json`, checked the way Open Design
    /// checks it. One that fails is reported and the system is read from
    /// its `DESIGN.md` alone, as Open Design does.
    fn read_kit(&mut self, folder: &str, dir: &Path) -> Option<Map<String, Value>> {
        let path = dir.join("manifest.json");
        if !self.root.join(&path).is_file() {
            return None;
        }
        let kit = self.read_manifest(&path)?;
        let ok = kit.get("schemaVersion").and_then(Value::as_str)
            == Some("od-design-system-project/v1")
            && kit.get("id").and_then(Value::as_str) == Some(folder)
            && kit.get("files").map(|f| &f["design"]) == Some(&Value::from("DESIGN.md"))
            && kit.get("files").map(|f| &f["tokens"]) == Some(&Value::from("tokens.css"));
        if !ok {
            self.problem(&path, "is not an od-design-system-project/v1 manifest for this folder; using DESIGN.md alone", false);
            return None;
        }
        Some(kit)
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
                    .unwrap_or_else(|| humanize(&id)),
                description: doc
                    .str("description")
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                name: id.clone(),
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
                let inside = format!("{owner}__{repo}/{}/SKILL.md", s["path"].as_str()?);
                Some((
                    stub.to_string(),
                    join_inside(Path::new("upstream"), &inside)?,
                ))
            })
            .collect();
        let root = self.root.clone();
        let mut missing = vec![];
        for e in self
            .entries
            .iter_mut()
            .filter(|e| e.kind == Some(Kind::Skill))
        {
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
            self.problem(
                p,
                "upstream skill listed in UPSTREAM.json is missing",
                false,
            );
        }
    }

    /// A plugin that runs another skill by name (`examples/web-clone` runs
    /// `skills/web-clone`) gets that skill's instructions.
    fn resolve_skill_refs(&mut self) {
        let skills: BTreeMap<String, PathBuf> = self
            .of(Kind::Skill)
            .chain(self.of(Kind::Template))
            .filter_map(|e| Some((e.name.clone(), e.skill.clone()?)))
            .chain(
                self.of(Kind::Skill)
                    .filter_map(|e| Some((e.id.clone(), e.skill.clone()?))),
            )
            .collect();
        let mut unknown = vec![];
        for e in self.entries.iter_mut().filter(|e| e.skill.is_none()) {
            if let Some(name) = &e.skill_ref {
                match skills.get(name) {
                    Some(p) => e.skill = Some(p.clone()),
                    None => unknown.push((e.dir.clone(), name.clone())),
                }
            }
        }
        for (dir, name) in unknown {
            self.problem(
                dir,
                format!("runs skill `{name}`, which isn't in the library"),
                false,
            );
        }
    }

    /// An example made from a template or a skill of the same folder name:
    /// each points at the other, so the gallery shows one card.
    fn link_twins(&mut self) {
        let made_from: BTreeMap<String, Kind> = self
            .of(Kind::Skill)
            .chain(self.of(Kind::Template))
            .filter(|e| e.parent.is_none())
            .map(|e| (e.id.clone(), e.kind.unwrap_or(Kind::Skill)))
            .collect();
        let examples: BTreeSet<String> = self.of(Kind::Example).map(|e| e.id.clone()).collect();
        for e in &mut self.entries {
            match e.kind {
                Some(Kind::Example) => e.twin = made_from.get(&e.id).map(|k| (*k, e.id.clone())),
                Some(Kind::Skill | Kind::Template)
                    if e.parent.is_none() && examples.contains(&e.id) =>
                {
                    e.twin = Some((Kind::Example, e.id.clone()))
                }
                _ => {}
            }
        }
    }

    /// Every file an entry names exists: its instructions, its preview
    /// (media is checked against `media.lock.json`, since it is fetched),
    /// and every craft rule it asks for, or that rule is one
    /// `craft/FUTURE_SECTIONS.md` lists as still to come. A preview upstream
    /// names but doesn't ship is dropped (the gallery shows none) and listed
    /// as tolerated.
    fn check_references(&mut self) {
        let craft: BTreeSet<String> = self.of(Kind::Craft).map(|e| e.id.clone()).collect();
        let future: BTreeSet<String> =
            std::fs::read_to_string(self.root.join("craft/FUTURE_SECTIONS.md"))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.trim().strip_prefix("- "))
                .map(|s| s.trim().trim_matches('`').to_string())
                .collect();
        let mut found = vec![];
        let mut no_preview = vec![];
        for (i, e) in self.entries.iter().enumerate() {
            if let Some(p) = e.skill.as_ref().filter(|p| !self.present(p)) {
                found.push((
                    p.clone(),
                    format!("{}: its instructions are not there", e.id),
                    false,
                ));
            }
            if let Some(p) = e.preview.as_ref().filter(|p| !self.present(p)) {
                found.push((
                    p.clone(),
                    format!("{}: upstream names this preview but doesn't ship it", e.id),
                    true,
                ));
                no_preview.push(i);
            }
            for slug in &e.craft {
                if !craft.contains(slug) && !future.contains(slug) {
                    found.push((
                        e.dir.clone(),
                        format!("asks for craft rule `{slug}`, which doesn't exist"),
                        false,
                    ));
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

/// A `skills/` or `design-templates/` entry, as Open Design's skill list
/// reads its `SKILL.md`.
fn from_skill(folder: &str, kind: Kind, dir: &Path, skill: &Path, doc: &Doc) -> Entry {
    let m = SkillMeta::from_doc(folder, doc);
    Entry {
        id: folder.to_string(),
        title: m
            .display_name
            .get("en")
            .cloned()
            .unwrap_or_else(|| humanize(&m.id)),
        kind: Some(kind),
        dir: dir.to_path_buf(),
        description: m.description.trim().to_string(),
        mode: Some(m.mode),
        scenario: Some(m.scenario),
        surface: Some(m.surface),
        platform: m.platform,
        category: m.category,
        gallery_kind: doc.str("category").map(str::to_string),
        tags: doc.strings("tags"),
        triggers: m.triggers,
        skill: Some(skill.to_path_buf()),
        example_prompt: m.example_prompt,
        craft: m.craft_requires,
        upstream: m.upstream,
        featured: m.featured,
        license: doc.str("license").map(str::to_string),
        catalogue: doc
            .body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains("install the upstream bundle"),
        name: m.id,
        ..Entry::default()
    }
}

/// The example Open Design shows for a skill or template folder, in its
/// order: `example.html`; `assets/template.html` (Open Design fills it with
/// `assets/example-slides.html` when asked; the template alone is shown
/// here); `assets/index.html`; the first `examples/*.html`.
fn example_preview(root: &Path, dir: &Path) -> Option<PathBuf> {
    let has = |p: &str| root.join(dir).join(p).is_file();
    for p in ["example.html", "assets/template.html", "assets/index.html"] {
        if has(p) {
            return Some(dir.join(p));
        }
    }
    example_files(root, dir)
        .into_iter()
        .next()
        .map(|f| dir.join("examples").join(f))
}

/// The `examples/*.html` files of a folder, sorted: names Open Design
/// accepts as card ids (`[A-Za-z0-9._-]`, no leading dot).
fn example_files(root: &Path, dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(root.join(dir).join("examples"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            // `.html` is ASCII, so once it's there the cut is on a character.
            n.to_ascii_lowercase().ends_with(".html") && {
                let stem = &n[..n.len() - 5];
                !stem.is_empty()
                    && !stem.starts_with('.')
                    && stem
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            }
        })
        .collect();
    out.sort();
    out
}

/// A card for each of a template's `examples/*.html`, as Open Design's
/// gallery lists them (`live-artifact:stock-dashboard`); the template itself
/// is then hidden.
fn derived_examples(root: &Path, parent: &Entry) -> Vec<Entry> {
    example_files(root, &parent.dir)
        .into_iter()
        .map(|file| {
            let key = &file[..file.len() - 5];
            Entry {
                id: format!("{}:{key}", parent.id),
                name: format!("{}:{key}", parent.name),
                title: key
                    .split(['-', '_', '.'])
                    .filter(|w| !w.is_empty())
                    .map(|w| {
                        let lower = w.to_lowercase();
                        let mut c = lower.chars();
                        c.next()
                            .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                preview: Some(parent.dir.join("examples").join(&file)),
                parent: Some(parent.id.clone()),
                craft: vec![],
                featured: None,
                hidden: false,
                ..parent.clone()
            }
        })
        .collect()
}

/// `./SKILL.md` inside `dir` as a library path; `None` when it would leave
/// the folder (`../`, an absolute path, a drive or a backslash).
fn join_inside(dir: &Path, p: &str) -> Option<PathBuf> {
    if p.starts_with('/') || p.contains('\\') || p.contains(':') {
        return None;
    }
    let mut out = dir.to_path_buf();
    let floor = out.components().count();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if out.components().count() <= floor {
                    return None;
                }
                out.pop();
            }
            _ => out.push(part),
        }
    }
    Some(out)
}

fn slash(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
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

/// A `DESIGN.md` heading as a name: `Design System Inspired by Apple` is
/// `Apple`.
fn clean_title(h1: &str) -> String {
    let lower = h1.to_lowercase();
    ["design system inspired by ", "design system for "]
        .iter()
        .find(|p| lower.starts_with(*p))
        .map_or(h1, |p| &h1[p.len()..])
        .trim()
        .to_string()
}

/// A design system's one-line summary, as Open Design takes it from
/// `DESIGN.md`: the first paragraph after the title, without its
/// `> Category:` and `> Surface:` lines or the `> ` quoting, at most 240
/// characters.
fn summarize(body: &str) -> String {
    let mut lines = body
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("# "));
    lines.next();
    let mut para = vec![];
    for l in lines {
        let t = l.trim();
        if t.starts_with('#') {
            break;
        }
        let quoted = t.strip_prefix('>').map(str::trim);
        let low = quoted.unwrap_or(t).to_ascii_lowercase();
        if low.starts_with("category:") || low.starts_with("surface:") {
            continue;
        }
        let t = quoted.unwrap_or(t);
        if t.is_empty() {
            if !para.is_empty() {
                break;
            }
            continue;
        }
        para.push(t);
    }
    para.join(" ").chars().take(240).collect()
}

/// `audio-jingle` → `Audio Jingle`.
fn humanize(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
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
/// page (`.js` under `assets/`) need nothing. Symlinks are never followed,
/// and the walk stops at a depth and a file count no real skill reaches, so
/// a user's odd folder can't stall loading.
fn needs(dir: &Path) -> BTreeSet<String> {
    const SKIP: [&str; 6] = [
        "node_modules",
        "assets",
        ".git",
        ".venv",
        "venv",
        "__pycache__",
    ];
    let mut out = BTreeSet::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    let mut seen = 0;
    while let Some((d, depth)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            seen += 1;
            if seen > 20_000 {
                return out;
            }
            let Ok(kind) = e.file_type() else {
                continue;
            };
            let name = e.file_name().to_string_lossy().to_string();
            if kind.is_dir() {
                if depth < 8 && !SKIP.contains(&name.as_str()) {
                    stack.push((e.path(), depth + 1));
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            match Path::new(&name).extension().and_then(|x| x.to_str()) {
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
        assert!(
            errors.is_empty(),
            "{} problems:\n{}",
            errors.len(),
            errors.join("\n")
        );
        // Upstream's own quirks: two invalid YAML headers, and previews it
        // names but doesn't ship.
        let tolerated: Vec<_> = ix.problems.iter().filter(|p| p.tolerated).collect();
        assert_eq!(
            tolerated
                .iter()
                .filter(|p| p.message.contains("YAML"))
                .count(),
            2
        );
    }

    #[test]
    fn counts_match_what_the_sync_recorded() {
        let ix = library();
        let c = &ix.upstream["counts"];
        let n = |k: Kind| ix.of(k).filter(|e| e.parent.is_none()).count() as u64;
        assert_eq!(Some(n(Kind::Skill)), c["skills"].as_u64());
        let catalogue = ix.of(Kind::Skill).filter(|e| e.catalogue).count() as u64;
        assert_eq!(Some(catalogue), c["catalogue_skills"].as_u64());
        assert_eq!(Some(n(Kind::Template)), c["design_templates"].as_u64());
        assert_eq!(Some(n(Kind::Craft)), c["craft"].as_u64());
        assert_eq!(Some(n(Kind::Example)), c["official_examples"].as_u64());
        assert_eq!(Some(n(Kind::Atom)), c["official_atoms"].as_u64());
        assert_eq!(Some(n(Kind::Scenario)), c["official_scenarios"].as_u64());
        assert_eq!(
            Some(n(Kind::ImageTemplate)),
            c["official_image_templates"].as_u64()
        );
        assert_eq!(
            Some(n(Kind::VideoTemplate)),
            c["official_video_templates"].as_u64()
        );
        assert_eq!(Some(n(Kind::DesignSystem)), c["design_systems"].as_u64());
    }

    #[test]
    fn a_deck_example_has_its_brief_preview_and_skill() {
        let ix = library();
        let e = ix
            .get(Kind::Example, "html-ppt-course-module")
            .expect("course module");
        assert_eq!(e.name, "example-html-ppt-course-module");
        assert_eq!(e.mode.as_deref(), Some("deck"));
        assert_eq!(e.scenario.as_deref(), Some("education"));
        assert!(e.query.as_deref().is_some_and(|q| q.contains("onboarding")));
        assert_eq!(
            e.preview.as_deref(),
            Some(Path::new(
                "plugins/_official/examples/html-ppt-course-module/example.html"
            ))
        );
        assert!(e.skill.as_ref().is_some_and(|s| ix.root.join(s).is_file()));
        assert_eq!(
            e.twin,
            Some((Kind::Template, "html-ppt-course-module".to_string()))
        );
    }

    #[test]
    fn skills_go_by_folder_or_by_open_designs_name() {
        let ix = library();
        let taste = ix.get(Kind::Skill, "taste-skill").expect("by folder");
        assert_eq!(taste.name, "design-taste-frontend");
        assert_eq!(ix.get(Kind::Skill, "design-taste-frontend"), Some(taste));
        let guizang = ix.get(Kind::Template, "magazine-web-ppt").expect("by name");
        assert_eq!(guizang.id, "guizang-ppt");
        assert_eq!(guizang.mode.as_deref(), Some("deck"));
        // `od.mode: design` is not a mode: it is guessed from the text.
        let brand = ix.get(Kind::Skill, "brand-extract").expect("brand-extract");
        assert!(skill_meta::MODES.contains(&brand.mode.as_deref().unwrap()));
    }

    #[test]
    fn the_two_invalid_headers_keep_their_fields() {
        let ix = library();
        let landing = ix
            .get(Kind::Example, "open-design-landing")
            .expect("landing");
        assert_eq!(landing.craft, ["pixel-discipline", "typographic-rhythm"]);
        let promo = ix
            .get(Kind::VideoTemplate, "frame-product-promo-30s")
            .expect("promo");
        assert_eq!(promo.mode.as_deref(), Some("video"));
    }

    #[test]
    fn manifests_keep_craft_under_od_context() {
        let ix = library();
        let voltage = ix
            .get(Kind::Example, "fs-creative-voltage")
            .expect("fs-creative-voltage");
        assert_eq!(voltage.craft, ["typography"]);
        for id in ["od-default", "od-new-generation"] {
            let s = ix.get(Kind::Scenario, id).expect(id);
            assert_eq!(s.craft, ["typography", "color", "anti-ai-slop"], "{id}");
        }
        // Only in its SKILL.md: the manifest leaves it out, the skill fills it.
        let webgl = ix
            .of(Kind::Example)
            .find(|e| e.id.starts_with("webgl-"))
            .expect("a webgl example");
        assert!(!webgl.craft.is_empty());
    }

    #[test]
    fn a_plugin_can_run_another_skill_by_name() {
        let ix = library();
        let clone = ix
            .get(Kind::Example, "web-clone")
            .expect("web-clone example");
        assert_eq!(
            clone.skill.as_deref(),
            Some(Path::new("skills/web-clone/SKILL.md"))
        );
    }

    #[test]
    fn previews_follow_open_designs_search() {
        let ix = library();
        let dash = ix.get(Kind::Template, "dashboard").expect("dashboard");
        assert_eq!(
            dash.preview.as_deref(),
            Some(Path::new("design-templates/dashboard/example.html"))
        );
        // Skills show their example too.
        assert!(ix.of(Kind::Skill).filter(|e| e.preview.is_some()).count() >= 39);
        // Named by upstream, shipped nowhere: no preview.
        assert_eq!(
            ix.get(Kind::Example, "dcf-valuation").unwrap().preview,
            None
        );
        // Open Design's own copy on the web is the library's file.
        let play = ix
            .get(Kind::VideoTemplate, "frame-play-mode")
            .expect("frame-play-mode");
        assert_eq!(
            play.preview.as_deref(),
            Some(Path::new(
                "plugins/_official/video-templates/frame-play-mode/preview.png"
            ))
        );
        assert_eq!(play.preview_url, None);
        // Someone else's site stays a link.
        assert!(ix.of(Kind::ImageTemplate).any(|e| e
            .preview_url
            .as_deref()
            .is_some_and(|u| u.contains("youmind"))));
    }

    #[test]
    fn a_template_with_example_files_becomes_cards() {
        let ix = library();
        let live = ix
            .get(Kind::Template, "live-artifact")
            .expect("live-artifact");
        assert!(live.hidden);
        let cards: Vec<_> = ix
            .of(Kind::Template)
            .filter(|e| e.parent.as_deref() == Some("live-artifact"))
            .collect();
        assert_eq!(cards.len(), 6);
        assert!(cards
            .iter()
            .all(|c| c.id.starts_with("live-artifact:") && c.preview.is_some()));
        assert_eq!(ix.entries.iter().filter(|e| e.parent.is_some()).count(), 11);
    }

    #[test]
    fn design_systems_come_from_their_kit() {
        let ix = library();
        let apple = ix.get(Kind::DesignSystem, "apple").expect("apple");
        assert_eq!(apple.dir, Path::new("design-systems/apple"));
        assert_eq!(apple.title, "Apple");
        assert_eq!(apple.category.as_deref(), Some("Media & Consumer"));
        assert!(!apple.description.starts_with(BOILERPLATE));
        assert!(apple.kit.is_some() && apple.manifest.is_some() && apple.query.is_some());
        assert_eq!(
            apple.skill.as_deref(),
            Some(Path::new("design-systems/apple/DESIGN.md"))
        );
        // The kit's DESIGN.md, never the older copy beside the card.
        let github = ix.get(Kind::DesignSystem, "github").expect("github");
        let text = std::fs::read_to_string(ix.root.join(github.skill.as_ref().unwrap())).unwrap();
        assert!(text.contains("Usage Guardrails"));
        // Kits without a gallery card.
        let kit_only: Vec<&str> = ix
            .of(Kind::DesignSystem)
            .filter(|e| e.manifest.is_none())
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(
            kit_only,
            [
                "cisco",
                "cloudflare-kumo",
                "hud",
                "loom",
                "slack",
                "tom-modern",
                "trading-terminal",
                "webex",
                "wechat"
            ]
        );
        let hud = ix.get(Kind::DesignSystem, "hud").unwrap();
        assert!(!hud.title.starts_with("Design System"));
        assert!(!hud.description.is_empty());
    }

    #[test]
    fn hidden_entries_are_atoms_and_internal_scenarios() {
        let ix = library();
        assert!(ix.of(Kind::Atom).all(|e| e.hidden));
        assert!(ix.get(Kind::Scenario, "od-default").unwrap().hidden);
        assert!(ix.get(Kind::Scenario, "od-next-strategy").unwrap().hidden);
        assert!(!ix.get(Kind::Scenario, "od-design-refine").unwrap().hidden);
    }

    #[test]
    fn cc_by_templates_credit_their_author() {
        let ix = library();
        let credited = ix.of(Kind::ImageTemplate).chain(ix.of(Kind::VideoTemplate));
        for e in credited.filter(|e| e.license.as_deref().is_some_and(|l| l.starts_with("CC-BY"))) {
            assert!(e.credit.is_some(), "{}", e.id);
        }
    }

    #[test]
    fn upstream_skills_the_sync_fetched_are_the_ones_used() {
        let ix = library();
        let listed = ix.upstream["skills"].as_array().map_or(0, Vec::len);
        let shipped: Vec<_> = ix.of(Kind::Skill).filter(|e| e.upstream_shipped).collect();
        assert_eq!(shipped.len(), listed);
        assert!(shipped
            .iter()
            .all(|e| e.catalogue && e.skill.as_ref().is_some_and(|p| p.starts_with("upstream"))));
        let canvas = ix.get(Kind::Skill, "canvas-design").expect("canvas-design");
        assert_eq!(
            canvas.skill.as_deref(),
            Some(Path::new(
                "upstream/anthropics__skills/skills/canvas-design/SKILL.md"
            ))
        );
        // Kept as a link: Anthropic's document skills aren't open source.
        let pptx = ix.get(Kind::Skill, "pptx").expect("pptx");
        assert!(pptx.catalogue && !pptx.upstream_shipped);
    }

    /// CLAUDE.md rule 1: Forge is closed source, so every licence in the
    /// library must let Forge redistribute it: no copyleft, nothing
    /// non-commercial or proprietary, and every declared licence one Forge
    /// knows to be permissive.
    #[test]
    fn every_licence_in_the_library_lets_forge_ship_it() {
        let ix = library();
        let permissive = [
            "MIT",
            "Apache-2.0",
            "BSD",
            "ISC",
            "0BSD",
            "Unlicense",
            "CC0-1.0",
            "CC-BY-4.0",
        ];
        let forbidden = [
            "gnu general public license",
            "gnu lesser general public",
            "gnu affero",
            "server side public license",
            "business source license",
            "commons clause",
            "noncommercial",
            "non-commercial",
            "noderivatives",
        ];
        let grants = [
            "permission is hereby granted",
            "apache license",
            "redistribution and use in source and binary forms",
            "permission to use, copy, modify",
        ];
        let mut stack = vec![ix.root.clone()];
        let mut checked = 0;
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_uppercase();
                if !(name.starts_with("LICENSE")
                    || name.starts_with("LICENCE")
                    || name.starts_with("COPYING"))
                {
                    continue;
                }
                let text = std::fs::read_to_string(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
                    .to_lowercase();
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                for bad in forbidden {
                    assert!(!text.contains(bad), "{} says {bad:?}", p.display());
                }
                let reserved = text[..text.len().min(600)].contains("all rights reserved");
                assert!(
                    !reserved
                        || grants.iter().any(|g| text.contains(g))
                        || text.contains("creative commons"),
                    "{} reserves all rights",
                    p.display()
                );
                checked += 1;
            }
        }
        assert!(checked > 100, "only {checked} licence files found");
        for s in ix.upstream["skills"].as_array().into_iter().flatten() {
            let l = s["license"].as_str().unwrap_or_default();
            assert!(permissive.contains(&l), "{}: {l}", s["stub"]);
            assert!(
                s["license_file"].as_str().is_some(),
                "{}: no licence text ships",
                s["stub"]
            );
        }
        for e in &ix.entries {
            let Some(l) = &e.license else {
                continue;
            };
            if l == "Complete terms in LICENSE.txt" {
                // Anthropic's skills point at their file: it must be Apache.
                let text = std::fs::read_to_string(ix.root.join(&e.dir).join("LICENSE.txt"))
                    .unwrap_or_else(|_| panic!("{}: no LICENSE.txt", e.dir.display()));
                assert!(text.contains("Apache License"), "{}", e.dir.display());
            } else {
                assert!(permissive.contains(&l.as_str()), "{}: {l}", e.dir.display());
            }
        }
    }

    #[test]
    fn your_captured_systems_join_the_design_systems() {
        let tmp = std::env::temp_dir().join(format!("forge-design-own-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let (lib, own) = (tmp.join("lib"), tmp.join("own"));
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("UPSTREAM.json"), "{}").unwrap();
        let site = own.join("kiln.test");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(
            site.join("DESIGN.md"),
            "# Design System Captured from Kiln Studio\n\n> Source: https://kiln.test/\n\n## 1. Visual Theme\n\nA warm light canvas.\n",
        )
        .unwrap();
        // Not a system: no DESIGN.md.
        std::fs::create_dir_all(own.join("half")).unwrap();
        let mut ix = Index::load(&lib);
        ix.load_own_systems(&own);
        let e = ix.get(Kind::DesignSystem, "kiln.test").expect("listed");
        assert_eq!(e.title, "Kiln Studio");
        assert_eq!(e.category.as_deref(), Some(OWN_CATEGORY));
        assert_eq!(e.description, "A warm light canvas.");
        // Absolute, so the library root doesn't move it.
        assert_eq!(
            ix.root.join(e.skill.as_ref().unwrap()),
            site.join("DESIGN.md")
        );
        assert!(ix.get(Kind::DesignSystem, "half").is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn odd_folders_cant_panic_or_stall_loading() {
        let tmp = std::env::temp_dir().join(format!("forge-design-odd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let skill = tmp.join("skills/a");
        std::fs::create_dir_all(skill.join("examples")).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: a\ndescription: d\n---\nbody",
        )
        .unwrap();
        // Multi-byte characters at the end of names that aren't .html.
        for f in ["café.txt", "示例.md", "résumé.pdf", "ok.html"] {
            std::fs::write(skill.join("examples").join(f), "x").unwrap();
        }
        #[cfg(unix)]
        for i in 0..3 {
            std::fs::create_dir_all(skill.join("sub")).unwrap();
            std::os::unix::fs::symlink("..", skill.join(format!("sub/up{i}"))).unwrap();
        }
        std::fs::write(tmp.join("UPSTREAM.json"), "{}").unwrap();
        let ix = Index::load(&tmp);
        let a = ix.get(Kind::Skill, "a").expect("a");
        assert_eq!(
            a.preview.as_deref(),
            Some(Path::new("skills/a/examples/ok.html"))
        );
        let _ = std::fs::remove_dir_all(&tmp);
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
    fn paths_stay_inside_their_folder() {
        assert_eq!(
            join_inside(Path::new("a/b"), "./SKILL.md"),
            Some(PathBuf::from("a/b/SKILL.md"))
        );
        assert_eq!(
            join_inside(Path::new("a/b"), "x/../c.html"),
            Some(PathBuf::from("a/b/c.html"))
        );
        assert_eq!(join_inside(Path::new("a/b"), "../c/x.html"), None);
        assert_eq!(join_inside(Path::new("a/b"), "/etc/passwd"), None);
        assert_eq!(join_inside(Path::new("a/b"), "..\\x"), None);
        assert_eq!(join_inside(Path::new("a/b"), "C:x"), None);
        // Open Design's own web address, with a path that tries to leave.
        assert_eq!(join_inside(Path::new(""), "/etc/passwd"), None);
        assert_eq!(join_inside(Path::new(""), "../../etc/hostname"), None);
        assert_eq!(join_inside(Path::new(""), "C:/Windows/win.ini"), None);
        assert_eq!(
            join_inside(Path::new(""), "a/b.png"),
            Some(PathBuf::from("a/b.png"))
        );
    }

    #[test]
    fn names_titles_and_summaries() {
        assert_eq!(humanize("audio-jingle"), "Audio Jingle");
        assert_eq!(clean_title("Design System Inspired by Apple"), "Apple");
        assert_eq!(clean_title("HUD Design System"), "HUD Design System");
        assert_eq!(
            summarize("# Design System Inspired by X\n\n> Category: Tools\n> A calm, dense look.\n\n## 1. Theme\n"),
            "A calm, dense look."
        );
        assert_eq!(
            owner_repo("https://github.com/anthropics/skills/tree/main/skills/pdf"),
            Some(("anthropics".into(), "skills".into()))
        );
    }
}
