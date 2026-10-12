//! Endpoint profiles: where Forge's models come from, as Forge for VS Code
//! has them. A profile is one endpoint together with the one model it runs,
//! so the model window lists pairs: you read the model, the profile name and
//! its host underneath. No model is built in, and none is the default: the
//! selected profile is, else the first one, else there is nothing to pick
//! and the composer says to set up an endpoint.
//!
//! Read from `endpoints.json` in Forge's config folder:
//!
//! ```json
//! { "selected": "local",
//!   "syncIntervalMinutes": 5,
//!   "profiles": [
//!     { "name": "local", "baseUrl": "http://127.0.0.1:11434", "wire": "openai",
//!       "model": "qwen3-coder:30b", "displayName": "Qwen3 Coder",
//!       "auth": { "type": "bearer", "token": "${env:LOCAL_KEY}" },
//!       "capabilities": { "effort": true, "effortLevels": ["low", "medium", "high"], "fast": false } } ] }
//! ```
//!
//! Keys are never written into the file: a bearer token names an environment
//! variable (`${env:NAME}`). Windows Credential Manager comes with the gateway.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Wire {
    #[default]
    Anthropic,
    Openai,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Auth {
    #[default]
    None,
    Bearer { token: String },
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Capabilities {
    /// The model's context window in tokens, when the profile states it.
    pub context_window: Option<u64>,
    pub effort: bool,
    pub effort_levels: Option<Vec<Effort>>,
    pub fast: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub wire: Wire,
    pub model: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub chat_path: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Endpoints {
    #[serde(default)]
    pub selected: Option<String>,
    /// How often every model is checked, in minutes; 0 turns it off.
    #[serde(default = "default_interval")]
    pub sync_interval_minutes: f64,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

fn default_interval() -> f64 {
    5.
}

/// Effort levels, in order. The extension's labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn label(self) -> &'static str {
        match self {
            Effort::Low => "Low",
            Effort::Medium => "Medium",
            Effort::High => "High",
            Effort::Xhigh => "Extra high",
            Effort::Max => "Max",
        }
    }
}

/// The setup guide (`docs/endpoints.md`), shown in the side panel.
pub const GUIDE: &str = include_str!("../docs/endpoints.md");

/// What "Open endpoints.json" writes when there is no file yet: one local
/// profile that needs no key, to edit from.
pub const STARTER: &str = r#"{
  "selected": "ollama",
  "syncIntervalMinutes": 5,
  "profiles": [
    {
      "name": "ollama",
      "baseUrl": "http://127.0.0.1:11434",
      "wire": "openai",
      "model": "qwen3-coder:30b",
      "displayName": "Qwen3 Coder"
    }
  ]
}
"#;

/// The file, created from `STARTER` if it doesn't exist (never replaced).
/// Blocking: call it off the UI thread.
pub fn ensure_file() -> std::io::Result<PathBuf> {
    let path = path().ok_or_else(|| std::io::Error::other("no config folder"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => std::io::Write::write_all(&mut f, STARTER.as_bytes())?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    Ok(path)
}

pub fn path() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("endpoints.json"))
}

/// The profiles on disk; none when the file is missing or unreadable.
pub fn load() -> Endpoints {
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Endpoints { sync_interval_minutes: default_interval(), ..Default::default() };
    };
    parse(&text).unwrap_or_else(|_| Endpoints { sync_interval_minutes: default_interval(), ..Default::default() })
}

pub fn parse(text: &str) -> serde_json::Result<Endpoints> {
    let mut e: Endpoints = serde_json::from_str(text)?;
    // Names are what you pick and what the health record is keyed on: one each.
    let mut seen = std::collections::HashSet::new();
    e.profiles.retain(|p| !p.name.trim().is_empty() && seen.insert(p.name.clone()));
    Ok(e)
}

/// Save which profile is picked (the rest of the file is left as it was).
pub fn save_selected(name: &str) -> std::io::Result<()> {
    let path = path().ok_or_else(|| std::io::Error::other("no config folder"))?;
    let mut value: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert("selected".into(), serde_json::Value::String(name.into()));
    }
    std::fs::write(&path, serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?)
}

impl Endpoints {
    /// The profile in use: the selected one, else the first.
    pub fn active(&self) -> Option<&Profile> {
        match self.selected.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(name) => self.profiles.iter().find(|p| p.name == name).or(self.profiles.first()),
            None => self.profiles.first(),
        }
    }
}

impl Profile {
    /// What you read in the picker: the model.
    pub fn display(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.model)
    }

    /// The model's context window: what the profile states, else what the
    /// endpoint lists for it (read during its check). None when neither says.
    pub fn context_window(&self, health: Option<&crate::health::EndpointHealth>) -> Option<u64> {
        self.capabilities.context_window.filter(|w| *w > 0).or_else(|| health.and_then(|h| h.listed_context).filter(|w| *w > 0))
    }

    /// Where the endpoint lists its models: `/v1/models` (one version
    /// segment, whichever way the base URL is written).
    pub fn models_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match self.wire {
            Wire::Anthropic => anthropic_url(base, "/v1/models"),
            Wire::Openai if ends_in_version(base) => format!("{base}/models"),
            Wire::Openai => format!("{base}/v1/models"),
        }
    }

    /// The effort levels this pair offers: none without effort support, else
    /// the profile's levels (Low, Medium, High when it names none).
    pub fn effort_levels(&self) -> Vec<Effort> {
        if !self.capabilities.effort {
            return vec![];
        }
        let mut levels = self.capabilities.effort_levels.clone().unwrap_or_else(|| vec![Effort::Low, Effort::Medium, Effort::High]);
        levels.sort();
        levels.dedup();
        levels
    }

    /// The chips after the model's name: Max, Ultracode, Fast.
    pub fn chips(&self) -> Vec<&'static str> {
        let levels = self.effort_levels();
        let mut chips = vec![];
        if levels.contains(&Effort::Max) {
            chips.push("Max");
        }
        if levels.contains(&Effort::Xhigh) {
            chips.push("Ultracode");
        }
        if self.capabilities.fast {
            chips.push("Fast");
        }
        chips
    }

    /// The URL the check (and a chat) posts to.
    pub fn chat_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match self.wire {
            Wire::Openai => match &self.chat_path {
                Some(p) => format!("{base}{}{p}", if p.starts_with('/') { "" } else { "/" }),
                None if ends_in_version(base) => format!("{base}/chat/completions"),
                None => format!("{base}/v1/chat/completions"),
            },
            Wire::Anthropic => anthropic_url(base, self.chat_path.as_deref().unwrap_or("/v1/messages")),
        }
    }

    /// What the verdicts are of: editing the URL or model under the same
    /// name must not inherit the old gateway's results.
    pub fn fingerprint(&self) -> String {
        serde_json::json!([self.base_url, self.wire, self.model, self.chat_path.clone().unwrap_or_default()]).to_string()
    }
}

/// `https://host/v1` and `https://host` both take `/v1/messages` without
/// doubling the version.
fn anthropic_url(base: &str, route: &str) -> String {
    let route = if route.starts_with('/') { route.to_string() } else { format!("/{route}") };
    let pathname = base.split("://").nth(1).and_then(|r| r.find('/').map(|i| &r[i..])).unwrap_or("").trim_end_matches('/');
    if ends_in_version(pathname) {
        // Drop the route's own leading version segment.
        let rest = route.trim_start_matches('/');
        match rest.split_once('/') {
            Some((first, tail)) if is_version(first) => format!("{base}/{tail}"),
            _ => format!("{base}{route}"),
        }
    } else {
        format!("{base}{route}")
    }
}

fn is_version(seg: &str) -> bool {
    let mut chars = seg.chars();
    matches!(chars.next(), Some('v' | 'V')) && {
        let rest: String = chars.collect();
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        digits > 0 && rest[digits..].chars().all(|c| c.is_ascii_alphabetic())
    }
}

fn ends_in_version(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(is_version)
}

/// The host shown under a model: `host:port`, else the URL as written.
pub fn host_of(base_url: &str) -> String {
    base_url.split("://").nth(1).map(|r| r.split('/').next().unwrap_or(r).to_string()).filter(|h| !h.is_empty()).unwrap_or_else(|| base_url.to_string())
}

/// A server on this machine, where a slow first answer is expected.
pub fn is_loopback(base_url: &str) -> bool {
    let host = host_of(base_url);
    let host = if host.starts_with('[') { host.trim_start_matches('[').split(']').next().unwrap_or("").to_string() } else { host.split(':').next().unwrap_or("").to_string() };
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(base: &str, wire: Wire) -> Profile {
        Profile {
            name: "p".into(),
            base_url: base.into(),
            wire,
            model: "m".into(),
            display_name: None,
            chat_path: None,
            headers: Default::default(),
            auth: Auth::None,
            capabilities: Default::default(),
        }
    }

    #[test]
    fn urls_accept_both_spellings() {
        assert_eq!(profile("https://h", Wire::Anthropic).chat_url(), "https://h/v1/messages");
        assert_eq!(profile("https://h/v1/", Wire::Anthropic).chat_url(), "https://h/v1/messages");
        assert_eq!(profile("https://h", Wire::Openai).chat_url(), "https://h/v1/chat/completions");
        assert_eq!(profile("https://h/api/v1", Wire::Openai).chat_url(), "https://h/api/v1/chat/completions");
    }

    #[test]
    fn no_model_is_built_in() {
        let e = parse(r#"{"profiles":[{"name":"a","baseUrl":"http://127.0.0.1:1","model":"x"},{"name":"b","baseUrl":"https://h","model":"y"},{"name":"a","baseUrl":"z","model":"dup"}]}"#).unwrap();
        assert_eq!(e.profiles.len(), 2);
        assert_eq!(e.active().map(|p| p.name.as_str()), Some("a"));
        assert_eq!(parse(r#"{"selected":"b","profiles":[{"name":"a","baseUrl":"u","model":"x"},{"name":"b","baseUrl":"u","model":"y"}]}"#).unwrap().active().map(|p| p.model.as_str()), Some("y"));
        assert!(parse("{}").unwrap().active().is_none());
        assert!(is_loopback("http://127.0.0.1:11434") && is_loopback("http://localhost") && !is_loopback("https://api.example.com"));
    }

    #[test]
    fn the_context_window_is_stated_or_listed_never_guessed() {
        let mut p = profile("https://h/v1", Wire::Openai);
        assert_eq!(p.context_window(None), None);
        let listed = crate::health::EndpointHealth { listed_context: Some(131_072), ..Default::default() };
        assert_eq!(p.context_window(Some(&listed)), Some(131_072));
        p.capabilities.context_window = Some(64_000);
        assert_eq!(p.context_window(Some(&listed)), Some(64_000));
        assert_eq!(p.models_url(), "https://h/v1/models");
        assert_eq!(profile("https://h", Wire::Anthropic).models_url(), "https://h/v1/models");
    }

    #[test]
    fn chips_follow_capabilities() {
        let mut p = profile("u", Wire::Openai);
        assert!(p.chips().is_empty() && p.effort_levels().is_empty());
        p.capabilities = Capabilities { context_window: None, effort: true, effort_levels: Some(vec![Effort::Max, Effort::Low, Effort::Xhigh]), fast: true };
        assert_eq!(p.effort_levels(), [Effort::Low, Effort::Xhigh, Effort::Max]);
        assert_eq!(p.chips(), ["Max", "Ultracode", "Fast"]);
    }

    #[test]
    fn the_starter_and_the_guide_examples_parse() {
        let starter = parse(STARTER).unwrap();
        assert_eq!(starter.profiles.len(), 1);
        assert_eq!(starter.active().unwrap().chat_url(), "http://127.0.0.1:11434/v1/chat/completions");
        // Every JSON block in the guide that is a whole file or one profile reads.
        let blocks: Vec<&str> = GUIDE.split("```json").skip(1).filter_map(|b| b.split("```").next()).collect();
        assert!(blocks.len() >= 4, "{}", blocks.len());
        for b in blocks {
            let whole = parse(b).ok().filter(|e| !e.profiles.is_empty());
            let one = serde_json::from_str::<Profile>(b).ok();
            assert!(whole.is_some() || one.is_some(), "a guide example does not parse:\n{b}");
            if let Some(p) = one {
                if let Auth::Bearer { token } = &p.auth {
                    assert!(token.starts_with("${env:"), "the guide must never show a key in the file");
                }
            }
        }
    }
}
