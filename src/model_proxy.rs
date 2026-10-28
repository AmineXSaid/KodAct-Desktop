//! Model calls from a remote agent go out from this computer.
//!
//! A remote ForgeCLI is not given the endpoint's key or address. It gets
//! `http://127.0.0.1:<port>/p/<token>` and a placeholder key; the port leads
//! back here (`ssh -R`), to this small HTTP/1.1 proxy, which looks the token
//! up, puts the real credentials and headers on the request, sends it to the
//! real endpoint, and streams the answer back (server-sent events as they
//! come). So keys stay on this computer and model traffic leaves from it,
//! as the network expects.
//!
//! A token names one chat's route and is dropped with it. The proxy listens
//! on 127.0.0.1 only.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Where a token's requests go: the real base URL (ForgeCLI's, before it
/// adds `/v1/messages` or `/chat/completions`) and the headers that carry
/// the key and any custom ones.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub base: String,
    pub headers: Vec<(String, String)>,
}

static PORT: OnceLock<u16> = OnceLock::new();
static ROUTES: Mutex<Option<HashMap<String, Route>>> = Mutex::new(None);

/// The key a remote ForgeCLI is given in place of the real one.
pub const PLACEHOLDER_KEY: &str = "forge-proxied";

fn token() -> String {
    use ring::rand::SecureRandom as _;
    let mut b = [0u8; 18];
    ring::rand::SystemRandom::new()
        .fill(&mut b)
        .expect("system random");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Start the proxy (once per run) and say its port.
pub fn start() -> std::io::Result<u16> {
    if let Some(p) = PORT.get() {
        return Ok(*p);
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    if PORT.set(port).is_err() {
        return Ok(*PORT.get().expect("set"));
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = serve(stream);
            });
        }
    });
    Ok(port)
}

/// Add a route; its token goes in the remote's base URL.
pub fn add(route: Route) -> String {
    let t = token();
    if let Ok(mut r) = ROUTES.lock() {
        r.get_or_insert_with(HashMap::new).insert(t.clone(), route);
    }
    t
}

/// Drop a route: its token stops working.
pub fn remove(token: &str) {
    if let Ok(mut r) = ROUTES.lock() {
        if let Some(m) = r.as_mut() {
            m.remove(token);
        }
    }
}

fn route(token: &str) -> Option<Route> {
    ROUTES
        .lock()
        .ok()
        .and_then(|r| r.as_ref().and_then(|m| m.get(token).cloned()))
}

/// Headers the proxy sets itself, never passed through from the remote.
fn hop_or_auth(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "content-length"
            | "connection"
            | "transfer-encoding"
            | "accept-encoding"
            | "authorization"
            | "x-api-key"
            | "x-goog-api-key"
            | "keep-alive"
            | "proxy-connection"
            | "upgrade"
            | "te"
    )
}

/// Split `/p/<token>/rest?query` into the token and `/rest?query`.
fn split_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/p/")?;
    let cut = rest.find('/').unwrap_or(rest.len());
    let (token, tail) = rest.split_at(cut);
    (!token.is_empty()).then_some((token, tail))
}

fn reply(out: &mut TcpStream, code: u16, text: &str) -> std::io::Result<()> {
    let body = format!(
        "{{\"error\":{{\"type\":\"forge_proxy\",\"message\":{}}}}}",
        serde_json::Value::from(text)
    );
    write!(
        out,
        "HTTP/1.1 {code} Forge\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn serve(stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return reply(&mut out, 400, "Not an HTTP request.");
    };
    let (method, path) = (method.to_string(), path.to_string());
    let mut headers = vec![];
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 {
            break;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            if k.eq_ignore_ascii_case("content-length") {
                length = v.parse().unwrap_or(0);
            }
            headers.push((k, v));
        }
        if headers.len() > 100 {
            return reply(&mut out, 431, "Too many headers.");
        }
    }
    if length > 64 * 1024 * 1024 {
        return reply(&mut out, 413, "Request too large.");
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let Some((token, tail)) = split_path(&path) else {
        return reply(&mut out, 404, "Unknown path.");
    };
    let Some(route) = route(token) else {
        return reply(&mut out, 403, "This chat's model route has ended.");
    };
    let url = format!("{}{}", route.base.trim_end_matches('/'), tail);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(600))
        .build();
    let mut req = agent.request(&method, &url);
    for (k, v) in headers.iter().filter(|(k, _)| !hop_or_auth(k)) {
        req = req.set(k, v);
    }
    for (k, v) in &route.headers {
        req = req.set(k, v);
    }
    let resp = match if body.is_empty() && method == "GET" {
        req.call()
    } else {
        req.send_bytes(&body)
    } {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => {
            return reply(
                &mut out,
                502,
                &format!("Forge couldn't reach the model endpoint from this computer: {e}"),
            )
        }
    };
    let mut head = format!("HTTP/1.1 {} {}\r\n", resp.status(), resp.status_text());
    for name in resp.headers_names() {
        if hop_or_auth(&name) || name.eq_ignore_ascii_case("content-encoding") {
            continue;
        }
        for v in resp.all(&name) {
            head.push_str(&format!("{name}: {v}\r\n"));
        }
    }
    // The body runs to the end of the connection: streaming needs no length.
    head.push_str("connection: close\r\n\r\n");
    out.write_all(head.as_bytes())?;
    out.flush()?;
    let mut body = resp.into_reader();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        out.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_split_into_token_and_rest() {
        assert_eq!(
            split_path("/p/abc/v1/messages"),
            Some(("abc", "/v1/messages"))
        );
        assert_eq!(
            split_path("/p/abc/chat/completions?x=1"),
            Some(("abc", "/chat/completions?x=1"))
        );
        assert_eq!(split_path("/p/abc"), Some(("abc", "")));
        assert_eq!(split_path("/p//v1"), None);
        assert_eq!(split_path("/v1/messages"), None);
    }

    /// A fake endpoint that echoes what reached it, streamed in pieces: the
    /// proxy swaps the placeholder key for the real one, keeps other
    /// headers and the body, streams the answer, and refuses an unknown or
    /// dropped token.
    #[test]
    fn requests_carry_the_real_key_and_stream_back() {
        let upstream = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let up_port = upstream.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in upstream.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut r = BufReader::new(s.try_clone().unwrap());
                    let mut seen = String::new();
                    let mut len = 0;
                    loop {
                        let mut l = String::new();
                        r.read_line(&mut l).unwrap();
                        if l.trim().is_empty() {
                            break;
                        }
                        if let Some(v) = l.to_lowercase().strip_prefix("content-length:") {
                            len = v.trim().parse().unwrap();
                        }
                        seen.push_str(&l);
                    }
                    let mut body = vec![0; len];
                    r.read_exact(&mut body).unwrap();
                    let mut s = s;
                    write!(s, "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n").unwrap();
                    for piece in [
                        seen.clone(),
                        format!("BODY:{}", String::from_utf8_lossy(&body)),
                    ] {
                        write!(s, "{:x}\r\n{piece}\r\n", piece.len()).unwrap();
                        s.flush().unwrap();
                        std::thread::sleep(Duration::from_millis(30));
                    }
                    write!(s, "0\r\n\r\n").unwrap();
                });
            }
        });
        let port = start().unwrap();
        let token = add(Route {
            base: format!("http://127.0.0.1:{up_port}/api"),
            headers: vec![
                ("x-api-key".into(), "real-key".into()),
                ("x-team".into(), "blue".into()),
            ],
        });
        let ask = |path: &str| -> (u16, String) {
            let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let body = r#"{"model":"m"}"#;
            write!(
                c,
                "POST {path} HTTP/1.1\r\nhost: x\r\nx-api-key: {PLACEHOLDER_KEY}\r\nanthropic-version: 2023-06-01\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut text = String::new();
            c.read_to_string(&mut text).unwrap();
            let code = text[9..12].parse().unwrap();
            (code, text)
        };
        let (code, text) = ask(&format!("/p/{token}/v1/messages"));
        assert_eq!(code, 200, "{text}");
        let low = text.to_lowercase();
        assert!(low.contains("post /api/v1/messages"), "{text}");
        assert!(low.contains("x-api-key: real-key"), "{text}");
        assert!(!low.contains(PLACEHOLDER_KEY), "{text}");
        assert!(low.contains("x-team: blue"), "{text}");
        assert!(low.contains("anthropic-version: 2023-06-01"), "{text}");
        assert!(text.contains(r#"BODY:{"model":"m"}"#), "{text}");
        assert!(low.contains("text/event-stream"), "{text}");
        assert_eq!(ask("/p/nope/v1/messages").0, 403);
        remove(&token);
        assert_eq!(ask(&format!("/p/{token}/v1/messages")).0, 403);
        assert_eq!(ask("/other").0, 404);
    }
}
