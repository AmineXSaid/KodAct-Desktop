//! Whether a model reasons, and whether its level can be set: found by
//! asking it, not by its name. A few short requests with the reasoning level
//! KodAct CLI would send (`reasoning_effort` on chat completions, a thinking
//! budget on Gemini, `output_config.effort` on the Messages API), and what
//! comes back is compared: reasoning in the reply, how much at low and at
//! high, and what a refusal says the server takes. The verdict sets the
//! profile's effort levels (`endpoints::Capabilities`), which the composer's
//! Reasoning chip offers. Blocking; run it off the UI thread.

use std::time::Duration;

use serde_json::{json, Value};

use crate::endpoints::{Effort, Profile, Wire};

/// What a model does with reasoning.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "levels")]
pub enum Reasoning {
    /// No reasoning came back, whatever level was asked for.
    None,
    /// It reasons, but the level asked for changes nothing (or is refused):
    /// a gateway that ignores it, or a model that always thinks.
    Fixed,
    /// It reasons more when asked for more: these levels can be chosen.
    Levels(Vec<Effort>),
}

impl Reasoning {
    /// The levels the Reasoning chip offers (empty: nothing to choose).
    pub fn levels(&self) -> Vec<Effort> {
        match self {
            Reasoning::Levels(l) => l.clone(),
            _ => vec![],
        }
    }

    /// One line for the menu and the toast.
    pub fn summary(&self) -> String {
        match self {
            Reasoning::None => "Doesn't reason: no reasoning came back at any level.".into(),
            Reasoning::Fixed => "Always reasons: the level asked for changes nothing here.".into(),
            Reasoning::Levels(l) => format!(
                "Adjustable: {}.",
                l.iter().map(|e| e.label()).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

/// One answer to a probe: refused for what was asked (a 400 or 422, with
/// what the server said), or how much reasoning it carried (tokens when the
/// usage says, else characters / 4).
#[derive(Clone, Debug, PartialEq)]
pub enum Probe {
    Refused(String),
    Reasoned(u64),
}

/// Refusals that say nothing about reasoning (the key, the model, the
/// rate, the server): the check stops and says why, rather than read them
/// as a verdict.
fn not_a_verdict(code: u16) -> bool {
    !matches!(code, 400 | 422)
}

impl Probe {
    fn amount(&self) -> Option<u64> {
        match self {
            Probe::Reasoned(n) => Some(*n),
            Probe::Refused(_) => None,
        }
    }
}

/// The verdict from the probes: none asked for (`plain`), the lowest and
/// highest level tried, and the levels a refusal of a made-up level named,
/// if it named any.
pub fn classify(plain: &Probe, low: &Probe, high: &Probe, named: Option<Vec<Effort>>) -> Reasoning {
    let any = [plain, low, high]
        .iter()
        .filter_map(|p| p.amount())
        .any(|n| n > 0);
    if !any {
        return Reasoning::None;
    }
    match (low.amount(), high.amount()) {
        // More reasoning when more is asked for: the level is honoured.
        // A margin, as one sample of each varies.
        (Some(l), Some(h)) if h >= l + 24 && h as f64 >= l as f64 * 1.5 => {
            let mut levels = named
                .filter(|n| n.len() >= 2)
                .unwrap_or_else(|| vec![Effort::Low, Effort::Medium, Effort::High]);
            levels.sort();
            levels.dedup();
            Reasoning::Levels(levels)
        }
        _ => Reasoning::Fixed,
    }
}

/// The levels a refusal says the server takes ("must be low, high, xhigh,
/// or max"; "Supported values are: 'low', 'medium', and 'high'"), when it
/// names at least two after the parameter's name.
pub fn named_levels(refusal: &str) -> Option<Vec<Effort>> {
    let m = refusal.to_ascii_lowercase();
    let at = ["reasoning_effort", "reasoning effort", "effort", "thinking"]
        .iter()
        .filter_map(|w| m.find(w))
        .min()?;
    let words: Vec<&str> = m[at..]
        // `max_tokens` is one word, not the level Max.
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    let mut found = vec![];
    for w in words {
        let e = match w {
            "low" => Effort::Low,
            "medium" => Effort::Medium,
            "high" => Effort::High,
            "xhigh" => Effort::Xhigh,
            "max" => Effort::Max,
            _ => continue,
        };
        if !found.contains(&e) {
            found.push(e);
        }
    }
    (found.len() >= 2).then(|| {
        found.sort();
        found
    })
}

/// How much reasoning a reply carried, from any of the shapes servers use.
pub fn reasoning_in(wire: Wire, reply: &Value) -> u64 {
    let chars = |s: &str| (s.chars().count() as u64).div_ceil(4);
    match wire {
        Wire::Openai => {
            let usage = reply
                .pointer("/usage/completion_tokens_details/reasoning_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let msg = reply.pointer("/choices/0/message").cloned().unwrap_or_default();
            let text = ["reasoning_content", "reasoning"]
                .iter()
                .filter_map(|k| msg.get(*k).and_then(Value::as_str))
                .map(chars)
                .sum::<u64>();
            // A model that writes its thinking into the answer, in tags.
            let tagged = msg
                .get("content")
                .and_then(Value::as_str)
                .and_then(|c| {
                    let start = c.find("<think>")? + 7;
                    let end = c[start..].find("</think>").map_or(c.len(), |e| start + e);
                    Some(chars(&c[start..end]))
                })
                .unwrap_or(0);
            usage.max(text).max(tagged)
        }
        Wire::Gemini => {
            let usage = reply
                .pointer("/usageMetadata/thoughtsTokenCount")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let text = reply
                .pointer("/candidates/0/content/parts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|p| p["thought"] == true)
                .filter_map(|p| p["text"].as_str())
                .map(chars)
                .sum::<u64>();
            usage.max(text)
        }
        Wire::Anthropic => reply["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "thinking")
            .filter_map(|b| b["thinking"].as_str())
            .map(chars)
            .sum(),
    }
}

/// A question that takes a little thought, with a short answer.
const QUESTION: &str = "How many prime numbers are there between 1 and 100? Reply with the number only.";

/// The probe's body at `level` (`None`: no level asked for; `Some("bogus")`
/// to make the server say what it takes).
fn body(profile: &Profile, level: Option<&str>) -> Value {
    match profile.wire {
        Wire::Openai => {
            let mut b = json!({
                "model": profile.model,
                "messages": [{"role": "user", "content": QUESTION}],
                "max_completion_tokens": 4096,
            });
            if let Some(l) = level {
                b["reasoning_effort"] = l.into();
            }
            b
        }
        Wire::Gemini => {
            let mut b = json!({
                "contents": [{"role": "user", "parts": [{"text": QUESTION}]}],
                "generationConfig": {"maxOutputTokens": 4096},
            });
            let budget = match level {
                None => None,
                Some("low") => Some(json!(512)),
                Some("high") => Some(json!(8192)),
                Some(_) => Some(json!("bogus")),
            };
            if let Some(n) = budget {
                b["generationConfig"]["thinkingConfig"] =
                    json!({"thinkingBudget": n, "includeThoughts": true});
            }
            b
        }
        Wire::Anthropic => {
            let mut b = json!({
                "model": profile.model,
                "max_tokens": 4096,
                "messages": [{"role": "user", "content": QUESTION}],
            });
            // As KodAct CLI asks (`thinking_params`): adaptive thinking,
            // shown summarized, so a reply's thinking can be measured.
            b["thinking"] = json!({"type": "adaptive", "display": "summarized"});
            if let Some(l) = level {
                b["output_config"] = json!({"effort": l});
            }
            b
        }
    }
}

/// Send one probe: a refusal that only names the length key, or thinking
/// the model doesn't take, is asked once more as it asks (the model check's
/// own retries, `health::swap_length_key`).
fn ask(
    agent: &ureq::Agent,
    profile: &Profile,
    token: Option<&str>,
    level: Option<&str>,
) -> Result<Probe, String> {
    let mut b = body(profile, level);
    let send = |b: &Value| {
        crate::health::request(agent, "POST", &profile.chat_url(), profile, token)
            .set("content-type", "application/json")
            .send_string(&b.to_string())
    };
    let mut tries = 0;
    loop {
        tries += 1;
        match send(&b) {
            Ok(r) => {
                let v: Value = r
                    .into_json()
                    .map_err(|e| format!("the answer wasn't JSON: {e}"))?;
                return Ok(Probe::Reasoned(reasoning_in(profile.wire, &v)));
            }
            Err(ureq::Error::Status(code, r)) => {
                let text = r.into_string().unwrap_or_default();
                if not_a_verdict(code) {
                    let said: String = text.chars().take(300).collect();
                    return Err(format!("HTTP {code}: {said}"));
                }
                let retried = tries < 4
                    && (crate::health::swap_length_key(&mut b, &text)
                        || crate::health::drop_length_keys(&mut b, &text)
                        || (text.to_ascii_lowercase().contains("thinking")
                            && b.as_object_mut().and_then(|o| o.remove("thinking")).is_some()));
                if !retried {
                    return Ok(Probe::Refused(text));
                }
            }
            Err(ureq::Error::Transport(t)) => return Err(t.to_string()),
        }
    }
}

/// Ask the model: its reasoning, and whether its level can be set.
pub fn detect(profile: &Profile, timeout: Duration) -> Result<Reasoning, String> {
    let token = crate::health::token(&profile.auth)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(timeout)
        .build();
    let ask = |level: Option<&str>| ask(&agent, profile, token.as_deref(), level);
    // Without a level: refused, the model can't be checked at all.
    let plain = ask(None)?;
    if let Probe::Refused(text) = &plain {
        let said: String = text.chars().take(300).collect();
        return Err(format!("the model refused a plain request: {said}"));
    }
    // A level no server takes: the refusal often lists the ones it does,
    // and those are the ends tried.
    let named = match ask(Some("bogus"))? {
        Probe::Refused(text) => named_levels(&text),
        Probe::Reasoned(_) => None,
    };
    let (lo, hi) = match named.as_deref() {
        Some([first, .., last]) => (first.cli_name(), last.cli_name()),
        _ => ("low", "high"),
    };
    let low = ask(Some(lo))?;
    let high = ask(Some(hi))?;
    Ok(classify(&plain, &low, &high, named))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_that_changes_the_thinking_is_adjustable() {
        let r = classify(&Probe::Reasoned(300), &Probe::Reasoned(120), &Probe::Reasoned(900), None);
        assert_eq!(r, Reasoning::Levels(vec![Effort::Low, Effort::Medium, Effort::High]));
        let named = Some(vec![Effort::Max, Effort::Low, Effort::High, Effort::Xhigh]);
        let r = classify(&Probe::Reasoned(0), &Probe::Reasoned(0), &Probe::Reasoned(400), named);
        assert_eq!(r.levels(), [Effort::Low, Effort::High, Effort::Xhigh, Effort::Max]);
    }

    #[test]
    fn a_gateway_that_ignores_the_level_is_fixed() {
        // The deep-thinking gateway: as much thought whatever is asked.
        let r = classify(&Probe::Reasoned(800), &Probe::Reasoned(760), &Probe::Reasoned(830), None);
        assert_eq!(r, Reasoning::Fixed);
        let refused = Probe::Refused("reasoning_effort is not supported".into());
        assert_eq!(classify(&Probe::Reasoned(500), &refused, &refused, None), Reasoning::Fixed);
    }

    #[test]
    fn no_reasoning_at_any_level_is_none() {
        let r = classify(
            &Probe::Reasoned(0),
            &Probe::Refused("unknown field reasoning_effort".into()),
            &Probe::Reasoned(0),
            None,
        );
        assert_eq!(r, Reasoning::None);
    }

    #[test]
    fn reads_the_levels_a_refusal_names() {
        let deepseek = "DeepSeek V4.1 reasoning_effort must be low, high, xhigh, max, or an integer within [1, 100]";
        assert_eq!(
            named_levels(deepseek).unwrap(),
            [Effort::Low, Effort::High, Effort::Xhigh, Effort::Max]
        );
        let openai = "Unsupported value: 'reasoning_effort' does not support 'bogus' with this model. Supported values are: 'low', 'medium', and 'high'.";
        assert_eq!(named_levels(openai).unwrap(), [Effort::Low, Effort::Medium, Effort::High]);
        assert_eq!(named_levels("reasoning_effort is not supported"), None);
        // `max_tokens` is a length key, not the level Max.
        assert_eq!(named_levels("reasoning_effort: 'bogus' is invalid; max_tokens is too large"), None);
        assert_eq!(
            named_levels("reasoning_effort must be 'low' or 'high' (max_completion_tokens 4096)").unwrap(),
            [Effort::Low, Effort::High]
        );
    }

    #[test]
    fn finds_reasoning_in_every_shape() {
        let o = json!({"choices": [{"message": {"content": "25", "reasoning_content": "a".repeat(400)}}]});
        assert_eq!(reasoning_in(Wire::Openai, &o), 100);
        let o = json!({"choices": [{"message": {"content": "25"}}], "usage": {"completion_tokens_details": {"reasoning_tokens": 512}}});
        assert_eq!(reasoning_in(Wire::Openai, &o), 512);
        let o = json!({"choices": [{"message": {"content": format!("<think>{}</think>25", "b".repeat(80))}}]});
        assert_eq!(reasoning_in(Wire::Openai, &o), 20);
        let g = json!({"usageMetadata": {"thoughtsTokenCount": 77}});
        assert_eq!(reasoning_in(Wire::Gemini, &g), 77);
        let a = json!({"content": [{"type": "thinking", "thinking": "c".repeat(40)}, {"type": "text", "text": "25"}]});
        assert_eq!(reasoning_in(Wire::Anthropic, &a), 10);
        assert_eq!(reasoning_in(Wire::Openai, &json!({"choices": [{"message": {"content": "25"}}]})), 0);
    }

    // ------------------------------------------------ against a server

    /// A model server on this machine: each request's body goes to
    /// `answer`, which returns the status and reply. Every body it was sent
    /// is kept, in order.
    fn serve(
        answer: impl Fn(&Value) -> (u16, Value) + Send + 'static,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut buf = vec![0; len];
                reader.read_exact(&mut buf).unwrap();
                let body: Value = serde_json::from_slice(&buf).unwrap_or_default();
                log.lock().unwrap().push(body.clone());
                let (code, reply) = answer(&body);
                let text = reply.to_string();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                );
            }
        });
        (url, seen)
    }

    fn profile(url: &str, wire: Wire) -> Profile {
        Profile {
            name: "p".into(),
            base_url: url.into(),
            wire,
            model: "m".into(),
            display_name: None,
            chat_path: None,
            headers: Default::default(),
            auth: crate::endpoints::Auth::None,
            capabilities: Default::default(),
        }
    }

    fn run(wire: Wire, answer: impl Fn(&Value) -> (u16, Value) + Send + 'static) -> (Result<Reasoning, String>, Vec<Value>) {
        let (url, seen) = serve(answer);
        let r = detect(&profile(&url, wire), Duration::from_secs(10));
        let sent = seen.lock().unwrap().clone();
        (r, sent)
    }

    /// A chat completion with `n` characters of reasoning.
    fn chat(n: usize) -> (u16, Value) {
        (200, json!({"choices": [{"message": {"content": "25", "reasoning_content": "r".repeat(n)}}]}))
    }

    fn effort(b: &Value) -> Option<&str> {
        b["reasoning_effort"].as_str()
    }

    #[test]
    fn an_openai_model_whose_level_is_honoured() {
        let (r, sent) = run(Wire::Openai, |b| match effort(b) {
            Some("bogus") => (400, json!({"error": {"message": "Unsupported value: 'reasoning_effort' does not support 'bogus' with this model. Supported values are: 'low', 'medium', and 'high'."}})),
            Some("low") => chat(200),
            Some("high") => chat(2400),
            _ => chat(1000),
        });
        assert_eq!(r.unwrap(), Reasoning::Levels(vec![Effort::Low, Effort::Medium, Effort::High]));
        let asked: Vec<_> = sent.iter().map(|b| effort(b).map(str::to_string)).collect();
        assert_eq!(asked, [None, Some("bogus".into()), Some("low".into()), Some("high".into())]);
    }

    #[test]
    fn a_gateway_that_ignores_the_level() {
        // The deep-thinking gateway: any level accepted, the same thinking.
        let (r, _) = run(Wire::Openai, |_| chat(3200));
        assert_eq!(r.unwrap(), Reasoning::Fixed);
    }

    #[test]
    fn a_model_that_does_not_reason() {
        let (r, _) = run(Wire::Openai, |b| match effort(b) {
            Some(_) => (400, json!({"error": {"message": "Unrecognized request argument supplied: reasoning_effort"}})),
            None => (200, json!({"choices": [{"message": {"content": "25"}}]})),
        });
        assert_eq!(r.unwrap(), Reasoning::None);
    }

    #[test]
    fn a_server_that_wants_max_tokens_is_asked_again() {
        let (r, sent) = run(Wire::Openai, |b| {
            if b.get("max_completion_tokens").is_some() {
                return (400, json!({"error": {"message": "Unsupported parameter: 'max_completion_tokens' is not supported with this model. Use 'max_tokens' instead."}}));
            }
            match effort(b) {
                Some("low") => chat(100),
                Some("high") => chat(1600),
                _ => chat(800),
            }
        });
        assert_eq!(r.unwrap().levels(), [Effort::Low, Effort::Medium, Effort::High]);
        assert!(sent.iter().any(|b| b.get("max_tokens").is_some()));
        // A false Max from "max_tokens" in the refusal would show here.
        assert!(!sent.iter().any(|b| effort(b) == Some("max")));
    }

    #[test]
    fn deepseek_is_tried_at_the_levels_it_names() {
        let (r, sent) = run(Wire::Openai, |b| match effort(b) {
            Some("bogus") | Some("medium") => (400, json!({"detail": "DeepSeek V4.1 reasoning_effort must be low, high, xhigh, max, or an integer within [1, 100] in chat_template_kwargs"})),
            Some("low") => chat(120),
            Some("max") => chat(5000),
            _ => chat(900),
        });
        assert_eq!(r.unwrap().levels(), [Effort::Low, Effort::High, Effort::Xhigh, Effort::Max]);
        assert_eq!(effort(&sent[3]), Some("max"), "its highest level was tried");
    }

    #[test]
    fn refusals_that_say_nothing_about_reasoning_stop_the_check() {
        let (r, _) = run(Wire::Openai, |_| (401, json!({"error": {"message": "invalid api key"}})));
        assert!(r.unwrap_err().contains("HTTP 401"));
        let (r, _) = run(Wire::Openai, |b| match effort(b) {
            Some("high") => (429, json!({"error": {"message": "rate limited"}})),
            _ => chat(500),
        });
        assert!(r.unwrap_err().contains("HTTP 429"), "a rate limit is not a verdict");
        let (r, _) = run(Wire::Openai, |_| (400, json!({"error": {"message": "model m does not exist"}})));
        assert!(r.unwrap_err().contains("refused a plain request"));
    }

    #[test]
    fn claude_with_adaptive_thinking() {
        let (r, sent) = run(Wire::Anthropic, |b| {
            let n = match b["output_config"]["effort"].as_str() {
                Some("bogus") => return (400, json!({"error": {"message": "output_config.effort: Input should be 'low', 'medium', 'high' or 'max'"}})),
                Some("low") => 60,
                Some("max") => 4000,
                _ => 800,
            };
            (200, json!({"content": [{"type": "thinking", "thinking": "t".repeat(n)}, {"type": "text", "text": "25"}]}))
        });
        assert_eq!(r.unwrap().levels(), [Effort::Low, Effort::Medium, Effort::High, Effort::Max]);
        assert!(sent.iter().all(|b| b["thinking"]["type"] == "adaptive"));
    }

    #[test]
    fn an_anthropic_server_without_thinking_is_asked_again_without_it() {
        let (r, sent) = run(Wire::Anthropic, |b| {
            if b.get("thinking").is_some() {
                return (400, json!({"error": {"message": "thinking: adaptive thinking is not supported on this model"}}));
            }
            (200, json!({"content": [{"type": "text", "text": "25"}]}))
        });
        assert_eq!(r.unwrap(), Reasoning::None);
        assert!(sent.iter().any(|b| b.get("thinking").is_none()), "retried without thinking");
    }

    #[test]
    fn gemini_with_a_thinking_budget() {
        let (r, _) = run(Wire::Gemini, |b| {
            let n = match &b["generationConfig"]["thinkingConfig"]["thinkingBudget"] {
                Value::String(_) => return (400, json!({"error": {"message": "Invalid value at 'generation_config.thinking_config.thinking_budget'"}})),
                v if v == &json!(512) => 300,
                v if v == &json!(8192) => 3000,
                _ => 900,
            };
            (200, json!({"candidates": [{"content": {"parts": [{"text": "25"}]}}], "usageMetadata": {"thoughtsTokenCount": n}}))
        });
        assert_eq!(r.unwrap().levels(), [Effort::Low, Effort::Medium, Effort::High]);
    }
}
