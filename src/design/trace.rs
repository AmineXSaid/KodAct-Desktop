//! Traces to your own Langfuse (Open Design's observability, ported as an
//! option): each reply, when it ends, as a trace with your message, Forge's
//! reply, the tools it used, the model, the tokens and the time, and for a
//! design chat its setup. Sent only when `LANGFUSE_PUBLIC_KEY` and
//! `LANGFUSE_SECRET_KEY` are set where Forge runs (`LANGFUSE_HOST` for a
//! self-hosted one), off the window's thread; nothing goes anywhere else.

use std::time::Duration;

use serde_json::{json, Value};

/// One reply, as it is traced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turn {
    pub chat: String,
    pub session: String,
    pub input: String,
    pub output: String,
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub seconds: Option<f32>,
    pub tokens: Option<(u64, u64)>,
    pub design: Option<Value>,
    pub failed: bool,
}

/// Where to send them, from the environment; None: not set up.
pub fn target() -> Option<(String, String)> {
    let get = |k: &str| {
        std::env::var(k)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let (public, secret) = (get("LANGFUSE_PUBLIC_KEY")?, get("LANGFUSE_SECRET_KEY")?);
    let host = get("LANGFUSE_HOST")
        .or_else(|| get("LANGFUSE_BASE_URL"))
        .unwrap_or_else(|| "https://cloud.langfuse.com".into());
    let auth = format!(
        "Basic {}",
        crate::attach::base64(format!("{public}:{secret}").as_bytes())
    );
    Some((
        format!("{}/api/public/ingestion", host.trim_end_matches('/')),
        auth,
    ))
}

/// The ingestion batch for a turn: a trace, and a generation in it for the
/// model's part.
pub fn batch(t: &Turn, id: &str, now: &str) -> Value {
    let mut metadata = json!({ "tools": t.tools, "app": "Forge" });
    if let Some(d) = &t.design {
        metadata["design"] = d.clone();
    }
    if let Some(s) = t.seconds {
        metadata["seconds"] = json!(s);
    }
    let mut generation = json!({
        "id": format!("{id}-gen"),
        "traceId": id,
        "name": "reply",
        "startTime": now,
        "input": t.input,
        "output": t.output,
        "level": if t.failed { "ERROR" } else { "DEFAULT" },
    });
    if let Some(m) = &t.model {
        generation["model"] = json!(m);
    }
    if let Some((i, o)) = t.tokens {
        generation["usage"] = json!({ "input": i, "output": o, "unit": "TOKENS" });
    }
    json!({ "batch": [
        { "id": format!("{id}-t"), "timestamp": now, "type": "trace-create", "body": {
            "id": id, "timestamp": now, "name": t.chat, "sessionId": t.session,
            "input": t.input, "output": t.output, "metadata": metadata,
            "tags": if t.design.is_some() { json!(["forge", "design"]) } else { json!(["forge"]) },
        } },
        { "id": format!("{id}-g"), "timestamp": now, "type": "generation-create", "body": generation },
    ] })
}

/// Send a turn's trace. Errors are said, not raised: a trace is never worth
/// interrupting anything for.
pub fn send(t: &Turn) -> Result<(), String> {
    let Some((url, auth)) = target() else {
        return Ok(());
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let id = format!(
        "forge-{}-{}",
        t.session
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(16)
            .collect::<String>(),
        chrono::Utc::now().timestamp_millis()
    );
    match ureq::post(&url)
        .timeout(Duration::from_secs(20))
        .set("Authorization", &auth)
        .send_json(batch(t, &id, &now))
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, r)) => Err(format!(
            "Langfuse said {code}: {}",
            r.into_string().unwrap_or_default()
        )),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};

    #[test]
    fn a_turn_becomes_a_trace_and_a_generation() {
        let t = Turn {
            chat: "Course deck".into(),
            session: "abc-123".into(),
            input: "Make a deck".into(),
            output: "Done.".into(),
            tools: vec!["Write deck.html".into()],
            model: Some("claude-x".into()),
            seconds: Some(12.5),
            tokens: Some((1200, 300)),
            design: Some(json!({ "surface": "deck" })),
            failed: false,
        };
        let b = batch(&t, "T", "2026-10-28T10:00:00.000Z");
        let items = b["batch"].as_array().unwrap();
        assert_eq!(items[0]["type"], "trace-create");
        assert_eq!(items[0]["body"]["sessionId"], "abc-123");
        assert_eq!(items[0]["body"]["metadata"]["design"]["surface"], "deck");
        assert_eq!(items[0]["body"]["tags"][1], "design");
        assert_eq!(items[1]["type"], "generation-create");
        assert_eq!(items[1]["body"]["traceId"], "T");
        assert_eq!(items[1]["body"]["usage"]["input"], 1200);
        assert_eq!(items[1]["body"]["model"], "claude-x");
    }

    #[test]
    fn it_is_sent_to_your_langfuse_only_when_set_up() {
        // Not set up: nothing is sent.
        for k in [
            "LANGFUSE_PUBLIC_KEY",
            "LANGFUSE_SECRET_KEY",
            "LANGFUSE_HOST",
        ] {
            std::env::remove_var(k);
        }
        assert!(target().is_none());
        assert!(send(&Turn::default()).is_ok());
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let got = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let (mut first, mut auth, mut len) = (String::new(), String::new(), 0usize);
            r.read_line(&mut first).unwrap();
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                let lower = h.to_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
                if lower.starts_with("authorization:") {
                    auth = h["authorization:".len()..].trim().to_string();
                }
            }
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let mut s = s;
            let _ = s.write_all(
                b"HTTP/1.1 207 Multi-Status\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            );
            (first, auth, String::from_utf8(body).unwrap())
        });
        std::env::set_var("LANGFUSE_PUBLIC_KEY", "pk");
        std::env::set_var("LANGFUSE_SECRET_KEY", "sk");
        std::env::set_var("LANGFUSE_HOST", format!("http://{addr}"));
        let r = send(&Turn {
            chat: "c".into(),
            input: "hi".into(),
            ..Turn::default()
        });
        for k in [
            "LANGFUSE_PUBLIC_KEY",
            "LANGFUSE_SECRET_KEY",
            "LANGFUSE_HOST",
        ] {
            std::env::remove_var(k);
        }
        r.unwrap();
        let (first, auth, body) = got.join().unwrap();
        assert!(first.starts_with("POST /api/public/ingestion "));
        assert_eq!(auth, format!("Basic {}", crate::attach::base64(b"pk:sk")));
        assert!(body.contains("trace-create") && body.contains("\"hi\""));
    }
}
