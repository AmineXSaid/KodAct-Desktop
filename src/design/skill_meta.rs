//! What Open Design makes of a skill's front-matter: its id, the mode,
//! surface, platform and scenario it settles on (with the same defaults and
//! the same guesses from the text), its craft rules and example prompt.
//! Adapted from Open Design (Apache-2.0), `listSkills` in
//! `apps/daemon/src/skills.ts`, and checked against its output for all 284
//! skills and templates. Where Open Design is wrong (backslashes left in
//! `\"`, `>` not folded) the YAML reading is kept instead.

use std::collections::BTreeMap;

use serde_json::Value;

use super::frontmatter::Doc;

pub const MODES: [&str; 8] = ["utility", "image", "video", "audio", "deck", "design-system", "template", "prototype"];
pub const SURFACES: [&str; 4] = ["web", "image", "video", "audio"];

#[derive(Debug, Clone, PartialEq)]
pub struct SkillMeta {
    /// Front-matter `name` when it is a non-empty string, else the folder.
    pub id: String,
    pub folder: String,
    pub display_name: BTreeMap<String, String>,
    pub description: String,
    pub description_i18n: BTreeMap<String, String>,
    pub triggers: Vec<String>,
    pub mode: String,
    pub surface: String,
    pub platform: Option<String>,
    pub scenario: String,
    pub category: Option<String>,
    pub preview_type: String,
    pub design_system_required: bool,
    pub default_for: Vec<String>,
    pub upstream: Option<String>,
    pub featured: Option<f64>,
    pub fidelity: Option<String>,
    pub speaker_notes: Option<bool>,
    pub animations: Option<bool>,
    pub example_prompt: String,
    pub example_prompt_i18n: BTreeMap<String, String>,
    pub critique_policy: Option<String>,
    pub craft_requires: Vec<String>,
}

impl SkillMeta {
    pub fn from_doc(folder: &str, doc: &Doc) -> SkillMeta {
        let m = &doc.meta;
        let od = |p: &str| doc.path(&format!("od.{p}"));
        let description = doc.str("description").unwrap_or("").to_string();
        let mode = normalize_mode(od("mode"), &doc.body, &description);
        SkillMeta {
            id: doc.str("name").filter(|n| !n.is_empty()).unwrap_or(folder).to_string(),
            folder: folder.to_string(),
            display_name: localized(m.get("en_name"), m.get("zh_name")),
            description_i18n: localized(m.get("en_description"), m.get("zh_description")),
            triggers: match m.get("triggers") {
                Some(Value::Array(a)) => a.iter().filter_map(js_string).collect(),
                _ => vec![],
            },
            surface: normalize_surface(od("surface"), &mode),
            platform: normalize_platform(od("platform"), &mode, &doc.body, &description),
            scenario: normalize_scenario(od("scenario"), &doc.body, &description),
            category: normalize_category(od("category")),
            preview_type: od("preview.type").and_then(Value::as_str).unwrap_or("html").to_string(),
            design_system_required: od("design_system.requires").and_then(Value::as_bool).unwrap_or(true),
            default_for: normalize_default_for(od("default_for")),
            upstream: od("upstream").and_then(Value::as_str).map(str::to_string),
            featured: normalize_featured(od("featured")),
            fidelity: od("fidelity")
                .and_then(Value::as_str)
                .filter(|v| matches!(*v, "wireframe" | "high-fidelity"))
                .map(str::to_string),
            speaker_notes: bool_hint(od("speaker_notes")),
            animations: bool_hint(od("animations")),
            example_prompt: derive_prompt(od("example_prompt"), &description),
            example_prompt_i18n: match od("example_prompt_i18n") {
                Some(Value::Object(o)) => o
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.trim().to_string())))
                    .filter(|(_, s)| !s.is_empty())
                    .collect(),
                _ => BTreeMap::new(),
            },
            critique_policy: od("critique.policy")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_lowercase())
                .filter(|s| matches!(s.as_str(), "required" | "opt-in" | "opt-out")),
            craft_requires: match od("craft.requires") {
                Some(Value::Array(a)) => slugs(a.iter().filter_map(Value::as_str)),
                _ => vec![],
            },
            mode,
            description,
        }
    }
}

/// `^[a-z0-9][a-z0-9-]*$`
pub fn is_slug(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase() || f.is_ascii_digit())
        && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '-')
}

/// Craft slugs as the daemon cleans them: trimmed, lower-cased, valid, once each.
pub fn slugs<'a>(it: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for v in it {
        let s = v.trim().to_lowercase();
        if is_slug(&s) && !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// JS `String(x)` for the values a list can hold.
fn js_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        _ => Some("[object Object]".into()),
    }
}

fn localized(en: Option<&Value>, zh: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (k, v) in [("en", en), ("zh-CN", zh)] {
        if let Some(s) = v.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            out.insert(k.to_string(), s.to_string());
        }
    }
    out
}

fn normalize_default_for(v: Option<&Value>) -> Vec<String> {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => vec![],
        Some(Value::String(s)) if s.is_empty() => vec![],
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => vec![],
        Some(Value::Array(a)) => a.iter().map(|x| js_string(x).unwrap_or_else(|| "null".into())).collect(),
        Some(x) => js_string(x).into_iter().collect(),
    }
}

fn normalize_featured(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Bool(true) => Some(1.0),
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        Value::String(s) if !s.trim().is_empty() => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

fn bool_hint(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn normalize_category(v: Option<&Value>) -> Option<String> {
    let s = v?.as_str()?.trim().to_lowercase();
    is_slug(&s).then(|| s.chars().take(64).collect())
}

/// An explicit `od.mode` wins when it is one of [`MODES`]; anything else
/// (`design`, `scenario`, `hyperframes`, missing) is guessed from the text.
fn normalize_mode(v: Option<&Value>, body: &str, description: &str) -> String {
    match v.and_then(Value::as_str) {
        Some(m) if MODES.contains(&m) => m.to_string(),
        _ => infer_mode(body, description).to_string(),
    }
}

fn normalize_surface(v: Option<&Value>, mode: &str) -> String {
    if let Some(s) = v.and_then(Value::as_str).map(|s| s.trim().to_lowercase()) {
        if SURFACES.contains(&s.as_str()) {
            return s;
        }
    }
    if matches!(mode, "image" | "video" | "audio") { mode } else { "web" }.to_string()
}

fn normalize_platform(v: Option<&Value>, mode: &str, body: &str, description: &str) -> Option<String> {
    if let Some(p @ ("desktop" | "mobile")) = v.and_then(Value::as_str) {
        return Some(p.to_string());
    }
    if mode != "prototype" {
        return None;
    }
    let hay = format!("{description}\n{body}").to_lowercase();
    let mobile = ["mobile", "phone", "ios", "android", "手机", "移动端"].iter().any(|k| hay.contains(k));
    Some(if mobile { "mobile" } else { "desktop" }.to_string())
}

fn normalize_scenario(v: Option<&Value>, body: &str, description: &str) -> String {
    if let Some(s) = v.and_then(Value::as_str).map(|s| s.trim().to_lowercase()) {
        if !s.is_empty() {
            return s;
        }
    }
    infer_scenario(body, description).to_string()
}

fn derive_prompt(explicit: Option<&Value>, description: &str) -> String {
    if let Some(s) = explicit.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
        return s.to_string();
    }
    let desc = description.trim();
    if desc.is_empty() {
        return String::new();
    }
    let collapsed = desc.split_whitespace().collect::<Vec<_>>().join(" ");
    // `^.+?[.!?。！？](?:\s|$)`: the first sentence end, at least one char in.
    let chars: Vec<(usize, char)> = collapsed.char_indices().collect();
    let mut first = collapsed.as_str();
    for (n, &(at, c)) in chars.iter().enumerate().skip(1) {
        if matches!(c, '.' | '!' | '?' | '。' | '！' | '？') && chars.get(n + 1).is_none_or(|&(_, nx)| nx.is_whitespace()) {
            first = collapsed[..at + c.len_utf8()].trim();
            break;
        }
    }
    let first = if first.is_empty() { collapsed.as_str() } else { first };
    first.chars().take(320).collect()
}

/// JS `\b` before an ASCII word: the previous character is not `[A-Za-z0-9_]`.
fn word(hay: &str, needle: &str, lead: bool, trail: bool) -> bool {
    let w = |c: char| c.is_ascii_alphanumeric() || c == '_';
    hay.match_indices(needle).any(|(at, _)| {
        (!lead || !hay[..at].chars().next_back().is_some_and(w))
            && (!trail || !hay[at + needle.len()..].chars().next().is_some_and(w))
    })
}

/// `/\bimage|poster|…/`: the `\b` binds to the first alternative only.
fn inferred(hay: &str, bounded: &[&str], plain: &[&str]) -> bool {
    bounded.iter().any(|k| word(hay, k, true, false)) || plain.iter().any(|k| hay.contains(k))
}

pub fn infer_mode(body: &str, description: &str) -> &'static str {
    let hay = format!("{description}\n{body}").to_lowercase();
    if inferred(&hay, &["image"], &["poster", "illustration", "photography", "图片", "海报", "插画"]) {
        "image"
    } else if inferred(&hay, &["video"], &["motion", "shortform", "animation", "视频", "动效", "短片"]) {
        "video"
    } else if inferred(&hay, &["audio"], &["music", "jingle", "tts", "sound", "音频", "音乐", "配音", "音效"]) {
        "audio"
    } else if inferred(&hay, &["ppt"], &["deck", "slide", "presentation", "幻灯", "投影"]) {
        "deck"
    } else if ["design system", "design-system", "design.md", "design tokens"].iter().any(|k| word(&hay, k, true, false)) {
        "design-system"
    } else if word(&hay, "template", true, true) {
        "template"
    } else {
        "prototype"
    }
}

pub fn infer_scenario(body: &str, description: &str) -> &'static str {
    let hay = format!("{description}\n{body}").to_lowercase();
    let any = |ks: &[&str]| ks.iter().any(|k| hay.contains(k));
    if any(&["finance", "invoice", "expense", "budget", "p&l", "revenue"]) {
        "finance"
    } else if word(&hay, "hr", true, true) || any(&["onboarding", "payroll", "employee", "人事"]) {
        "hr"
    } else if any(&["marketing", "campaign", "brand", "landing"]) {
        "marketing"
    } else if any(&["runbook", "incident", "deploy", "engineering", "sre", "api"]) {
        "engineering"
    } else if any(&["spec", "prd", "roadmap", "product manager", "product team"]) {
        "product"
    } else if any(&["design system", "moodboard", "mockup", "ui kit"]) {
        "design"
    } else if any(&["sales", "quote", "proposal", "lead"]) {
        "sales"
    } else if any(&["operations", "ops", "logistics", "inventory"]) {
        "operations"
    } else {
        "general"
    }
}
