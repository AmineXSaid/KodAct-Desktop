//! Whether each endpoint's model answers, and how fast: the model window's
//! ping, as Forge for VS Code measures it.
//!
//! A check is a real request, not a ping: a one-token completion ("hi",
//! `max_tokens: 4`) to the model the profile runs. Its round trip is the
//! ping. Results are kept in `endpoint-health.json` beside the profiles,
//! tied to what each profile points at, and refreshed every few minutes
//! (and on demand, from the window's refresh button).
//!
//! The window offers what answers: a pair that failed its last check, or
//! whose check could not be sent, is left out, except the one in use, which
//! stays greyed with the reason. A pair never measured stays in.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::endpoints::{Auth, Profile, Wire};

/// How long to wait for a connection.
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(4_000);
/// How long a check may take in the background, or to a server on this
/// machine (a local runtime loads the model on its first request).
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(20_000);
/// How long a check you asked for may take.
pub const INTERACTIVE_TIMEOUT: Duration = Duration::from_millis(10_000);
/// How many endpoints are checked at once.
pub const SWEEP_PARALLELISM: usize = 6;
/// A check is dated when it finishes, so it counts as due this much early.
const DUE_SLACK_MS: u64 = 30_000;

pub const FAST_PING_MS: u64 = 1_000;
pub const SLOW_PING_MS: u64 = 3_000;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// One model's last verdict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelHealth {
    pub id: String,
    pub servable: bool,
    pub ms: u64,
    #[serde(default)]
    pub detail: Option<String>,
    pub checked_at: u64,
}

/// One endpoint's record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointHealth {
    pub fingerprint: String,
    #[serde(default)]
    pub last_synced_at: Option<u64>,
    /// The last check could not be sent (a missing key, DNS, TLS). Set by the
    /// newest attempt, so it wins over older verdicts.
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelHealth>,
    /// A check is running now. Not saved.
    #[serde(skip)]
    pub syncing: bool,
}

/// The state of one pair's last check, as a picker row carries it.
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    Answered { ms: u64, checked_at: u64 },
    Failed { detail: String },
    Unreachable { detail: String },
    Checking,
    Unchecked,
}

/// `model`'s state on one endpoint, and whether a check is running.
pub fn pair_check(model: &str, health: Option<&EndpointHealth>) -> (Check, bool) {
    let Some(h) = health else { return (Check::Unchecked, false) };
    if let Some(e) = &h.error {
        return (Check::Unreachable { detail: e.clone() }, h.syncing);
    }
    match h.models.iter().find(|m| m.id == model) {
        Some(v) if v.servable => (Check::Answered { ms: v.ms, checked_at: v.checked_at }, h.syncing),
        Some(v) => (Check::Failed { detail: v.detail.clone().unwrap_or_else(|| "no reply".into()) }, h.syncing),
        None if h.syncing => (Check::Checking, true),
        None => (Check::Unchecked, false),
    }
}

/// Offered in the window: it answered, or it hasn't been measured yet.
pub fn is_offered(check: &Check) -> bool {
    matches!(check, Check::Answered { .. } | Check::Checking | Check::Unchecked)
}

/// "820ms", "1.4s".
pub fn ping_text(ms: u64) -> String {
    if ms >= 1000 { format!("{:.1}s", ms as f64 / 1000.) } else { format!("{ms}ms") }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Fast,
    Fair,
    Slow,
}

/// Green, grey, orange. Never red: red is failure, and a failed pair isn't listed.
pub fn ping_tone(ms: u64) -> Tone {
    if ms < FAST_PING_MS {
        Tone::Fast
    } else if ms < SLOW_PING_MS {
        Tone::Fair
    } else {
        Tone::Slow
    }
}

/// Written after the profile and host; nothing when it answered.
pub fn status_text(check: &Check) -> String {
    match check {
        Check::Answered { .. } => String::new(),
        Check::Failed { detail } => format!("did not answer: {detail}"),
        Check::Unreachable { detail } => format!("could not be checked: {detail}"),
        Check::Checking => "checking…".into(),
        Check::Unchecked => "not checked yet".into(),
    }
}

/// "just now", "4 min ago", "2 h ago", "3 d ago".
pub fn ago_text(checked_at: u64, now: u64) -> String {
    let mins = now.saturating_sub(checked_at) / 60_000;
    match mins {
        0 => "just now".into(),
        1..=59 => format!("{mins} min ago"),
        60..=1439 => format!("{} h ago", mins / 60),
        _ => format!("{} d ago", mins / 1440),
    }
}

/// Whether a profile last checked at `last` is due, with the timer at
/// `minutes` (0: off). The slack is at most half the interval.
pub fn is_due(last: Option<u64>, now: u64, minutes: f64) -> bool {
    if minutes.is_nan() || minutes <= 0. {
        return false;
    }
    let Some(last) = last else { return true };
    let interval = (minutes * 60_000.) as u64;
    now.saturating_sub(last) >= interval - DUE_SLACK_MS.min(interval / 2)
}

/// How long a check may take.
pub fn timeout_for(profile: &Profile, background: bool) -> Duration {
    if background || crate::endpoints::is_loopback(&profile.base_url) { PROBE_TIMEOUT } else { INTERACTIVE_TIMEOUT }
}

// ---------------------------------------------------------------- the record on disk

pub type Store = HashMap<String, EndpointHealth>;

fn store_path() -> Option<std::path::PathBuf> {
    crate::profile::dir().map(|d| d.join("endpoint-health.json"))
}

/// The saved record, keeping only verdicts that still describe `profiles`.
pub fn load(profiles: &[Profile]) -> Store {
    let saved: Store = store_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    profiles.iter().filter_map(|p| saved.get(&p.name).filter(|h| h.fingerprint == p.fingerprint()).map(|h| (p.name.clone(), h.clone()))).collect()
}

pub fn save(store: &Store) {
    if let Some(path) = store_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(store) {
            let _ = std::fs::write(path, text);
        }
    }
}

/// Fold a finished check into the record. A check that could not be sent
/// keeps the old verdicts and records why.
pub fn apply(store: &mut Store, profile: &Profile, outcome: Result<ModelHealth, String>) {
    let entry = store.entry(profile.name.clone()).or_default();
    if entry.fingerprint != profile.fingerprint() {
        *entry = EndpointHealth { fingerprint: profile.fingerprint(), ..Default::default() };
    }
    entry.syncing = false;
    match outcome {
        Ok(verdict) => {
            entry.error = None;
            entry.last_synced_at = Some(verdict.checked_at);
            entry.models.retain(|m| m.id != verdict.id);
            entry.models.push(verdict);
        }
        Err(e) => entry.error = Some(e),
    }
}

// ---------------------------------------------------------------- the check

/// `${env:NAME}` read from the environment; anything else is refused, so a
/// key is never kept in the profile file.
fn token(auth: &Auth) -> Result<Option<String>, String> {
    match auth {
        Auth::None => Ok(None),
        Auth::Bearer { token } => {
            let name = token.strip_prefix("${env:").and_then(|t| t.strip_suffix('}')).ok_or("a key must be given as ${env:NAME}, not written into endpoints.json")?;
            std::env::var(name).map(Some).map_err(|_| format!("the key ${{env:{name}}} is not set"))
        }
    }
}

/// Check one pair: Ok with its verdict (answered or not), or Err when the
/// request could not be sent at all. Blocking; run it off the UI thread.
pub fn probe(profile: &Profile, timeout: Duration) -> Result<ModelHealth, String> {
    let token = token(&profile.auth)?;
    let agent = ureq::AgentBuilder::new().timeout_connect(CONNECT_TIMEOUT).timeout(timeout).build();
    let mut req = agent.post(&profile.chat_url()).set("content-type", "application/json");
    if profile.wire == Wire::Anthropic {
        req = req.set("anthropic-version", "2023-06-01");
    }
    for (k, v) in &profile.headers {
        req = req.set(k, v);
    }
    if let Some(t) = &token {
        req = match profile.wire {
            Wire::Anthropic => req.set("x-api-key", t).set("authorization", &format!("Bearer {t}")),
            Wire::Openai => req.set("authorization", &format!("Bearer {t}")),
        };
    }
    let body = serde_json::json!({ "model": profile.model, "max_tokens": 4, "messages": [{ "role": "user", "content": "hi" }] });
    let started = Instant::now();
    let result = req.send_string(&body.to_string());
    let ms = started.elapsed().as_millis() as u64;
    let verdict = |servable: bool, detail: Option<String>| Ok(ModelHealth { id: profile.model.clone(), servable, ms, detail, checked_at: now_ms() });
    match result {
        Ok(resp) => {
            // Read the reply through, so the time covers the answer.
            let _ = resp.into_string();
            verdict(true, None)
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(str::to_string));
            let detail: String = message.unwrap_or_else(|| format!("HTTP {code}")).chars().take(160).collect();
            verdict(false, Some(detail))
        }
        Err(ureq::Error::Transport(t)) => {
            let text = t.to_string();
            if matches!(t.kind(), ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Dns) {
                if text.contains("timed out") {
                    return verdict(false, Some("could not connect".into()));
                }
                return Err(match t.kind() {
                    ureq::ErrorKind::Dns => "the host could not be found".into(),
                    _ => "could not connect".into(),
                });
            }
            if text.contains("timed out") || text.contains("WouldBlock") {
                return verdict(false, Some("listed, but accepted the request and never answered".into()));
            }
            verdict(false, Some(text.chars().take(160).collect()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(error: Option<&str>, models: Vec<ModelHealth>, syncing: bool) -> EndpointHealth {
        EndpointHealth { fingerprint: String::new(), last_synced_at: None, error: error.map(Into::into), models, syncing }
    }

    fn verdict(id: &str, servable: bool, ms: u64) -> ModelHealth {
        ModelHealth { id: id.into(), servable, ms, detail: (!servable).then(|| "HTTP 401".into()), checked_at: 5 }
    }

    #[test]
    fn the_newest_error_wins_over_old_verdicts() {
        let h = record(Some("no key"), vec![verdict("m", true, 300)], false);
        assert_eq!(pair_check("m", Some(&h)).0, Check::Unreachable { detail: "no key".into() });
        let h = record(None, vec![verdict("m", true, 300)], true);
        assert_eq!(pair_check("m", Some(&h)), (Check::Answered { ms: 300, checked_at: 5 }, true));
        let h = record(None, vec![verdict("m", false, 30)], false);
        assert_eq!(pair_check("m", Some(&h)).0, Check::Failed { detail: "HTTP 401".into() });
        assert_eq!(pair_check("m", Some(&record(None, vec![], true))).0, Check::Checking);
        assert_eq!(pair_check("m", None).0, Check::Unchecked);
    }

    #[test]
    fn what_the_window_offers() {
        assert!(is_offered(&Check::Unchecked) && is_offered(&Check::Checking));
        assert!(!is_offered(&Check::Failed { detail: "x".into() }) && !is_offered(&Check::Unreachable { detail: "x".into() }));
    }

    #[test]
    fn pings_read_and_colour_as_in_the_extension() {
        assert_eq!(ping_text(820), "820ms");
        assert_eq!(ping_text(1440), "1.4s");
        assert_eq!((ping_tone(999), ping_tone(1000), ping_tone(2999), ping_tone(3000)), (Tone::Fast, Tone::Fair, Tone::Fair, Tone::Slow));
        assert_eq!(status_text(&Check::Unchecked), "not checked yet");
        assert_eq!(ago_text(0, 61 * 60_000), "1 h ago");
    }

    #[test]
    fn checks_fall_due_with_slack() {
        let min5 = 5. * 60_000.;
        assert!(is_due(None, 0, 5.));
        assert!(!is_due(None, 0, 0.));
        assert!(!is_due(Some(0), (min5 as u64) - 31_000, 5.));
        assert!(is_due(Some(0), (min5 as u64) - 30_000, 5.));
        // A short interval waits at least half of it.
        assert!(!is_due(Some(0), 29_000, 1.));
        assert!(is_due(Some(0), 30_000, 1.));
    }

    #[test]
    fn keys_are_never_read_from_the_file() {
        assert!(token(&Auth::Bearer { token: "sk-123".into() }).is_err());
        assert!(token(&Auth::Bearer { token: "${env:FORGE_TEST_SURELY_UNSET}".into() }).unwrap_err().contains("not set"));
    }
}
