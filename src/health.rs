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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
    /// The context window the endpoint lists for the model (its `/v1/models`
    /// entry), read during the check.
    #[serde(default)]
    pub listed_context: Option<u64>,
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
    let Some(h) = health else {
        return (Check::Unchecked, false);
    };
    if let Some(e) = &h.error {
        return (Check::Unreachable { detail: e.clone() }, h.syncing);
    }
    match h.models.iter().find(|m| m.id == model) {
        Some(v) if v.servable => (
            Check::Answered {
                ms: v.ms,
                checked_at: v.checked_at,
            },
            h.syncing,
        ),
        Some(v) => (
            Check::Failed {
                detail: v.detail.clone().unwrap_or_else(|| "no reply".into()),
            },
            h.syncing,
        ),
        None if h.syncing => (Check::Checking, true),
        None => (Check::Unchecked, false),
    }
}

/// Offered in the window: it answered, or it hasn't been measured yet.
pub fn is_offered(check: &Check) -> bool {
    matches!(
        check,
        Check::Answered { .. } | Check::Checking | Check::Unchecked
    )
}

/// "820ms", "1.4s".
pub fn ping_text(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.)
    } else {
        format!("{ms}ms")
    }
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
    if background || crate::endpoints::is_loopback(&profile.base_url) {
        PROBE_TIMEOUT
    } else {
        INTERACTIVE_TIMEOUT
    }
}

// ---------------------------------------------------------------- the record on disk

pub type Store = HashMap<String, EndpointHealth>;

fn store_path() -> Option<std::path::PathBuf> {
    crate::profile::dir().map(|d| d.join("endpoint-health.json"))
}

/// The saved record, keeping only verdicts that still describe `profiles`.
pub fn load(profiles: &[Profile]) -> Store {
    let saved: Store = store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    profiles
        .iter()
        .filter_map(|p| {
            saved
                .get(&p.name)
                .filter(|h| h.fingerprint == p.fingerprint())
                .map(|h| (p.name.clone(), h.clone()))
        })
        .collect()
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
/// keeps the old verdicts and records why; a model list that couldn't be
/// read keeps the context window read before.
pub fn apply(
    store: &mut Store,
    profile: &Profile,
    outcome: Result<ModelHealth, String>,
    listed_context: Option<u64>,
) {
    let entry = store.entry(profile.name.clone()).or_default();
    if entry.fingerprint != profile.fingerprint() {
        *entry = EndpointHealth {
            fingerprint: profile.fingerprint(),
            ..Default::default()
        };
    }
    entry.syncing = false;
    if listed_context.is_some() {
        entry.listed_context = listed_context;
    }
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

/// The key a profile names: `${secret:KEY}` from the secret store
/// (`secrets.rs`, where Settings puts it), or `${env:NAME}` from the
/// environment. Anything else is refused, so a key is never kept in the
/// profile file.
pub(crate) fn token(auth: &Auth) -> Result<Option<String>, String> {
    match auth {
        Auth::None => Ok(None),
        Auth::Bearer { token } => {
            if let Some(key) = token
                .strip_prefix("${secret:")
                .and_then(|t| t.strip_suffix('}'))
            {
                return crate::secrets::get(key).map(Some).ok_or_else(|| {
                    "no key saved for this endpoint: add it in Settings > Models".to_string()
                });
            }
            let name = token.strip_prefix("${env:").and_then(|t| t.strip_suffix('}')).ok_or("a key must be saved in Settings or given as ${env:NAME}, not written into endpoints.json")?;
            env_var(name).map(Some).ok_or_else(|| {
                format!("the key ${{env:{name}}} is not set: add the key in Settings > Models")
            })
        }
    }
}

/// An environment variable as it is now. On Windows a variable set with
/// `setx` (or System Properties) after Forge started is read from the
/// registry, so it works without restarting Forge.
pub(crate) fn env_var(name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(name) {
        return Some(v);
    }
    #[cfg(windows)]
    {
        for hive in [
            "HKCU\\Environment",
            "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        ] {
            if let Some(v) = reg_value(hive, name) {
                return Some(v);
            }
        }
    }
    None
}

/// `reg query <hive> /v <name>`'s value, without a console window.
#[cfg(windows)]
fn reg_value(hive: &str, name: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args(["query", hive, "/v", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // "    NAME    REG_SZ    value"
    text.lines().find_map(|l| {
        let l = l.trim_start();
        let rest = l.strip_prefix(name)?.trim_start();
        let rest = rest
            .strip_prefix("REG_EXPAND_SZ")
            .or_else(|| rest.strip_prefix("REG_SZ"))?;
        let v = rest.trim();
        (!v.is_empty()).then(|| v.to_string())
    })
}

/// A request to the endpoint with its headers and key.
fn request(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    profile: &Profile,
    token: Option<&str>,
) -> ureq::Request {
    let mut req = agent.request(method, url);
    if profile.wire == Wire::Anthropic {
        req = req.set("anthropic-version", "2023-06-01");
    }
    for (k, v) in &profile.headers {
        req = req.set(k, v);
    }
    if let Some(t) = token {
        req = match profile.wire {
            Wire::Anthropic => req
                .set("x-api-key", t)
                .set("authorization", &format!("Bearer {t}")),
            Wire::Openai => req.set("authorization", &format!("Bearer {t}")),
        };
    }
    req
}

/// The context window an endpoint's model list states for `model`: its
/// entry's `context_length`, `max_input_tokens` or `context_window`.
pub fn context_from_listing(listing: &serde_json::Value, model: &str) -> Option<u64> {
    let entries = listing
        .get("data")
        .or_else(|| listing.get("models"))
        .and_then(|d| d.as_array())?;
    let entry = entries.iter().find(|e| {
        [e.get("id"), e.get("name")]
            .into_iter()
            .flatten()
            .any(|v| v.as_str() == Some(model))
    })?;
    ["context_length", "max_input_tokens", "context_window"]
        .iter()
        .find_map(|k| entry.get(*k).and_then(|v| v.as_u64()))
        .filter(|n| *n > 0)
}

/// Read the model's context window from the endpoint's model list, when
/// the profile doesn't state it. None when it can't be read. Blocking.
pub fn listed_context(profile: &Profile, timeout: Duration) -> Option<u64> {
    if profile.capabilities.context_window.is_some() {
        return None;
    }
    let token = token(&profile.auth).ok()?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout(timeout)
        .build();
    let text = request(
        &agent,
        "GET",
        &profile.models_url(),
        profile,
        token.as_deref(),
    )
    .call()
    .ok()?
    .into_string()
    .ok()?;
    context_from_listing(&serde_json::from_str(&text).ok()?, &profile.model)
}

/// A reply, its body read through (so the time covers the answer):
/// answered, refused with its status and body, or never sent.
enum Reply {
    Answered,
    Refused(u16, String),
    Unsent(ureq::Transport),
}

fn read(r: Result<ureq::Response, ureq::Error>) -> Reply {
    match r {
        Ok(resp) => {
            let _ = resp.into_string();
            Reply::Answered
        }
        Err(ureq::Error::Status(code, resp)) => {
            Reply::Refused(code, resp.into_string().unwrap_or_default())
        }
        Err(ureq::Error::Transport(t)) => Reply::Unsent(t),
    }
}

/// The check's request: a few tokens of answer to "hi". OpenAI's reasoning
/// models take `max_completion_tokens`, and need a little room to think.
fn probe_body(profile: &Profile) -> serde_json::Value {
    let mut body = serde_json::json!({ "model": profile.model, "messages": [{ "role": "user", "content": "hi" }] });
    if profile.wire == Wire::Openai && crate::endpoints::is_reasoning_model(&profile.model) {
        body["max_completion_tokens"] = 16.into();
    } else {
        body["max_tokens"] = 4.into();
    }
    body
}

/// When a refusal names the length key the body uses as unsupported (the
/// one it names first: "'max_tokens' is not supported ... use
/// 'max_completion_tokens'"), send the other name instead. True when changed.
fn swap_length_key(body: &mut serde_json::Value, refusal: &str) -> bool {
    let m = refusal.to_ascii_lowercase();
    let (old, new) = (m.find("max_tokens"), m.find("max_completion_tokens"));
    let refused = match (old, new) {
        (Some(a), Some(b)) if a < b => "max_tokens",
        (_, Some(_)) => "max_completion_tokens",
        _ => return false,
    };
    let other = if refused == "max_tokens" {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    let Some(o) = body.as_object_mut() else {
        return false;
    };
    match o.remove(refused) {
        Some(v) => {
            o.insert(other.into(), v);
            true
        }
        None => false,
    }
}

/// Check one pair: Ok with its verdict (answered or not), or Err when the
/// request could not be sent at all. Blocking; run it off the UI thread.
pub fn probe(profile: &Profile, timeout: Duration) -> Result<ModelHealth, String> {
    let token = token(&profile.auth)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout(timeout)
        .build();
    let send = |body: &serde_json::Value| {
        let sent = request(
            &agent,
            "POST",
            &profile.chat_url(),
            profile,
            token.as_deref(),
        )
        .set("content-type", "application/json")
        .send_string(&body.to_string());
        read(sent)
    };
    let mut body = probe_body(profile);
    let started = Instant::now();
    let mut reply = send(&body);
    // A 400 asking for the other name for the answer's length (a reasoning
    // model under a name not recognised, or a server that only knows
    // max_tokens): asked once more, as it asks.
    if let Reply::Refused(400 | 422, text) = &reply {
        if swap_length_key(&mut body, text) {
            reply = send(&body);
        }
    }
    let ms = started.elapsed().as_millis() as u64;
    let verdict = |servable: bool, detail: Option<String>| {
        Ok(ModelHealth {
            id: profile.model.clone(),
            servable,
            ms,
            detail,
            checked_at: now_ms(),
        })
    };
    match reply {
        Reply::Answered => verdict(true, None),
        Reply::Refused(code, text) => {
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                });
            let detail: String = message
                .unwrap_or_else(|| format!("HTTP {code}"))
                .chars()
                .take(160)
                .collect();
            verdict(false, Some(detail))
        }
        Reply::Unsent(t) => {
            let text = t.to_string();
            if matches!(
                t.kind(),
                ureq::ErrorKind::ConnectionFailed | ureq::ErrorKind::Dns
            ) {
                if text.contains("timed out") {
                    return verdict(false, Some("could not connect".into()));
                }
                return Err(match t.kind() {
                    ureq::ErrorKind::Dns => "the host could not be found".into(),
                    _ => "could not connect".into(),
                });
            }
            if text.contains("timed out") || text.contains("WouldBlock") {
                return verdict(
                    false,
                    Some("listed, but accepted the request and never answered".into()),
                );
            }
            verdict(false, Some(text.chars().take(160).collect()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn openai(model: &str) -> Profile {
        Profile {
            name: "t".into(),
            base_url: "http://127.0.0.1:20128/v1".into(),
            wire: Wire::Openai,
            model: model.into(),
            display_name: None,
            chat_path: None,
            headers: Default::default(),
            auth: Auth::None,
            capabilities: Default::default(),
        }
    }

    #[test]
    fn the_check_asks_reasoning_models_in_their_terms() {
        let b = probe_body(&openai("cx/gpt-5-codex"));
        assert!(
            b.get("max_tokens").is_none() && b["max_completion_tokens"] == 16,
            "{b}"
        );
        let b = probe_body(&openai("auto/best-coding"));
        assert!(
            b["max_tokens"] == 4 && b.get("max_completion_tokens").is_none(),
            "{b}"
        );
        // Anthropic's API knows only max_tokens, whatever the model is called.
        let b = probe_body(&Profile {
            wire: Wire::Anthropic,
            ..openai("gpt-5")
        });
        assert_eq!(b["max_tokens"], 4);
    }

    #[test]
    fn a_refused_length_key_is_swapped_for_the_other() {
        let mut b = probe_body(&openai("house-reasoner"));
        let openai_says = r#"{"error":{"message":"Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."}}"#;
        assert!(swap_length_key(&mut b, openai_says));
        assert!(b.get("max_tokens").is_none() && b["max_completion_tokens"] == 4);
        // Sent again, OpenAI's same words name the key it no longer has: nothing to change.
        assert!(!swap_length_key(&mut b, openai_says));
        // A server that knows only max_tokens, refusing the newer name.
        let mut b = probe_body(&openai("gpt-5"));
        assert!(swap_length_key(
            &mut b,
            "Unrecognized request argument supplied: max_completion_tokens"
        ));
        assert_eq!(b["max_tokens"], 16);
        // Other refusals are left to read as they are.
        let mut b = probe_body(&openai("auto"));
        assert!(!swap_length_key(&mut b, "The model `auto` does not exist"));
    }

    fn record(error: Option<&str>, models: Vec<ModelHealth>, syncing: bool) -> EndpointHealth {
        EndpointHealth {
            error: error.map(Into::into),
            models,
            syncing,
            ..Default::default()
        }
    }

    fn verdict(id: &str, servable: bool, ms: u64) -> ModelHealth {
        ModelHealth {
            id: id.into(),
            servable,
            ms,
            detail: (!servable).then(|| "HTTP 401".into()),
            checked_at: 5,
        }
    }

    #[test]
    fn the_newest_error_wins_over_old_verdicts() {
        let h = record(Some("no key"), vec![verdict("m", true, 300)], false);
        assert_eq!(
            pair_check("m", Some(&h)).0,
            Check::Unreachable {
                detail: "no key".into()
            }
        );
        let h = record(None, vec![verdict("m", true, 300)], true);
        assert_eq!(
            pair_check("m", Some(&h)),
            (
                Check::Answered {
                    ms: 300,
                    checked_at: 5
                },
                true
            )
        );
        let h = record(None, vec![verdict("m", false, 30)], false);
        assert_eq!(
            pair_check("m", Some(&h)).0,
            Check::Failed {
                detail: "HTTP 401".into()
            }
        );
        assert_eq!(
            pair_check("m", Some(&record(None, vec![], true))).0,
            Check::Checking
        );
        assert_eq!(pair_check("m", None).0, Check::Unchecked);
    }

    #[test]
    fn what_the_window_offers() {
        assert!(is_offered(&Check::Unchecked) && is_offered(&Check::Checking));
        assert!(
            !is_offered(&Check::Failed { detail: "x".into() })
                && !is_offered(&Check::Unreachable { detail: "x".into() })
        );
    }

    #[test]
    fn pings_read_and_colour_as_in_the_extension() {
        assert_eq!(ping_text(820), "820ms");
        assert_eq!(ping_text(1440), "1.4s");
        assert_eq!(
            (
                ping_tone(999),
                ping_tone(1000),
                ping_tone(2999),
                ping_tone(3000)
            ),
            (Tone::Fast, Tone::Fair, Tone::Fair, Tone::Slow)
        );
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
    fn the_listed_window_is_read_from_either_list_shape() {
        let openai = serde_json::json!({ "data": [{ "id": "other", "context_length": 8192 }, { "id": "qwen3", "context_length": 262144 }] });
        assert_eq!(context_from_listing(&openai, "qwen3"), Some(262_144));
        let anthropic = serde_json::json!({ "data": [{ "id": "m", "max_input_tokens": 200000 }] });
        assert_eq!(context_from_listing(&anthropic, "m"), Some(200_000));
        let ollama = serde_json::json!({ "models": [{ "name": "gpt-oss:20b" }] });
        assert_eq!(context_from_listing(&ollama, "gpt-oss:20b"), None);
        assert_eq!(context_from_listing(&openai, "missing"), None);
    }

    #[test]
    fn keys_are_never_read_from_the_file() {
        assert!(token(&Auth::Bearer {
            token: "sk-123".into()
        })
        .is_err());
        assert!(token(&Auth::Bearer {
            token: "${env:FORGE_TEST_SURELY_UNSET}".into()
        })
        .unwrap_err()
        .contains("not set"));
    }
}
