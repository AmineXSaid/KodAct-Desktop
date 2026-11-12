//! Settings → Models → Diagnose: why an endpoint doesn't answer, step by
//! step, from this PC. Each step needs the one before it:
//!
//! 1. **DNS**: the URL's host resolves (an IP address needs no lookup);
//! 2. **TCP**: a connection opens to one of its addresses;
//! 3. **TLS** (https only): the handshake completes and the certificate is
//!    valid for the host, checked against the same roots ureq uses;
//! 4. **HTTP**: the base URL answers at all (any status shows a server);
//! 5. **Key**: the model list loads with the profile's key and headers.
//!
//! Anything model-side (does it stream, call tools, read images, think?)
//! needs a model request per feature and KodAct CLI's own view of the model,
//! so it isn't checked here.
//!
//! Blocking network I/O: call off the UI thread.

use std::io::Write;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::endpoints::Profile;

const TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Waiting,
    Running,
    Pass,
    Fail,
    /// Not run: an earlier step failed, or it doesn't apply (TLS on http).
    Skip,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Step {
    pub name: &'static str,
    pub outcome: Outcome,
    pub detail: String,
    pub ms: Option<u64>,
}

pub const STEPS: [&str; 5] = [
    "Name lookup (DNS)",
    "Connection (TCP)",
    "Secure connection (TLS)",
    "The server answers (HTTP)",
    "The key works",
];

pub fn waiting() -> Vec<Step> {
    STEPS
        .iter()
        .map(|n| Step {
            name: n,
            outcome: Outcome::Waiting,
            detail: String::new(),
            ms: None,
        })
        .collect()
}

/// Where the profile's base URL points.
pub struct Target {
    pub https: bool,
    pub host: String,
    pub port: u16,
}

pub fn target(base_url: &str) -> Result<Target, String> {
    let u = url::Url::parse(base_url.trim()).map_err(|e| format!("the URL doesn't parse: {e}"))?;
    let https = match u.scheme() {
        "https" => true,
        "http" => false,
        s => return Err(format!("{s}:// isn't http or https")),
    };
    let host = u.host_str().ok_or("the URL has no host")?;
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    Ok(Target {
        https,
        port: u
            .port_or_known_default()
            .unwrap_or(if https { 443 } else { 80 }),
        host,
    })
}

fn ms(since: Instant) -> u64 {
    since.elapsed().as_millis() as u64
}

fn dns(t: &Target) -> Result<(Vec<SocketAddr>, String), String> {
    if let Ok(ip) = t.host.parse::<std::net::IpAddr>() {
        return Ok((
            vec![SocketAddr::new(ip, t.port)],
            format!("{ip} is an address: no lookup needed"),
        ));
    }
    let addrs: Vec<SocketAddr> = (t.host.as_str(), t.port)
        .to_socket_addrs()
        .map_err(|e| {
            format!(
                "{} doesn't resolve: {e}. Check the spelling, or your DNS and VPN.",
                t.host
            )
        })?
        .collect();
    let first = addrs
        .first()
        .ok_or_else(|| format!("{} resolves to no address", t.host))?;
    let more = if addrs.len() > 1 {
        format!(" (and {} more)", addrs.len() - 1)
    } else {
        String::new()
    };
    Ok((addrs.clone(), format!("{} → {}{more}", t.host, first.ip())))
}

fn tcp(addrs: &[SocketAddr]) -> Result<(TcpStream, String), String> {
    let mut last = String::new();
    for a in addrs.iter().take(4) {
        match TcpStream::connect_timeout(a, TIMEOUT) {
            Ok(s) => return Ok((s, format!("connected to {a}"))),
            Err(e) => last = format!("{a}: {e}"),
        }
    }
    Err(format!(
        "no connection ({last}). A firewall, a proxy, or a server that isn't running?"
    ))
}

fn tls(host: &str, mut tcp: TcpStream) -> Result<String, String> {
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("{host} isn't a name TLS can check: {e}"))?;
    let mut conn =
        rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    tcp.set_read_timeout(Some(TIMEOUT)).ok();
    tcp.set_write_timeout(Some(TIMEOUT)).ok();
    while conn.is_handshaking() {
        conn.complete_io(&mut tcp).map_err(|e| {
            let text = e.to_string();
            if text.contains("certificate") || text.contains("UnknownIssuer") {
                format!("the certificate isn't trusted ({text}). A network that inspects TLS, or a self-signed server?")
            } else {
                format!("the handshake failed: {text}")
            }
        })?;
    }
    let _ = tcp.flush();
    let version = match conn.protocol_version() {
        Some(rustls::ProtocolVersion::TLSv1_3) => "TLS 1.3",
        Some(rustls::ProtocolVersion::TLSv1_2) => "TLS 1.2",
        _ => "TLS",
    };
    Ok(format!("{version}, certificate valid for {host}"))
}

fn http(base_url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(TIMEOUT)
        .timeout(Duration::from_secs(10))
        .redirects(0)
        .build();
    match agent.get(base_url.trim()).call() {
        Ok(r) => Ok(format!("HTTP {} from {}", r.status(), base_url.trim())),
        // Any status means a web server is there; 404 on a base URL is normal.
        Err(ureq::Error::Status(code, _)) => Ok(format!("HTTP {code}: a server answers")),
        Err(e) => Err(format!("no HTTP answer: {e}")),
    }
}

/// Run every step, calling `report` after each one changes. A failed step
/// marks the ones after it as skipped.
pub fn run(profile: &Profile, key: Option<String>, mut report: impl FnMut(usize, Step)) {
    let mut set = |i: usize, outcome: Outcome, detail: String, ms: Option<u64>| {
        report(
            i,
            Step {
                name: STEPS[i],
                outcome,
                detail,
                ms,
            },
        )
    };
    let skip_rest = |from: usize, set: &mut dyn FnMut(usize, Outcome, String, Option<u64>)| {
        for i in from..STEPS.len() {
            set(i, Outcome::Skip, "Needs the step before it.".into(), None);
        }
    };
    let target = match target(&profile.base_url) {
        Ok(t) => t,
        Err(e) => {
            set(0, Outcome::Fail, e, None);
            return skip_rest(1, &mut set);
        }
    };
    set(0, Outcome::Running, String::new(), None);
    let t0 = Instant::now();
    let addrs = match dns(&target) {
        Ok((a, d)) => {
            set(0, Outcome::Pass, d, Some(ms(t0)));
            a
        }
        Err(e) => {
            set(0, Outcome::Fail, e, Some(ms(t0)));
            return skip_rest(1, &mut set);
        }
    };
    set(1, Outcome::Running, String::new(), None);
    let t1 = Instant::now();
    let stream = match tcp(&addrs) {
        Ok((s, d)) => {
            set(1, Outcome::Pass, d, Some(ms(t1)));
            s
        }
        Err(e) => {
            set(1, Outcome::Fail, e, Some(ms(t1)));
            return skip_rest(2, &mut set);
        }
    };
    if target.https {
        set(2, Outcome::Running, String::new(), None);
        let t2 = Instant::now();
        match tls(&target.host, stream) {
            Ok(d) => set(2, Outcome::Pass, d, Some(ms(t2))),
            Err(e) => {
                set(2, Outcome::Fail, e, Some(ms(t2)));
                return skip_rest(3, &mut set);
            }
        }
    } else {
        // Close the probe's connection: a one-at-a-time server would wait on it.
        drop(stream);
        let local = crate::endpoints::is_loopback(&profile.base_url);
        set(
            2,
            Outcome::Skip,
            if local {
                "Plain http on this PC: nothing to encrypt.".into()
            } else {
                "Plain http: requests and the key travel unencrypted.".into()
            },
            None,
        );
    }
    set(3, Outcome::Running, String::new(), None);
    let t3 = Instant::now();
    match http(&profile.base_url) {
        Ok(d) => set(3, Outcome::Pass, d, Some(ms(t3))),
        Err(e) => {
            set(3, Outcome::Fail, e, Some(ms(t3)));
            return skip_rest(4, &mut set);
        }
    }
    set(4, Outcome::Running, String::new(), None);
    let t4 = Instant::now();
    match crate::endpoints::list_models(
        &profile.base_url,
        profile.wire,
        key.as_deref(),
        &profile.headers,
    ) {
        Ok(models) => set(
            4,
            Outcome::Pass,
            format!(
                "{} the model list ({} model{}){}",
                if key.is_some() {
                    "Accepted: it loaded"
                } else {
                    "No key needed: it loaded"
                },
                models.len(),
                if models.len() == 1 { "" } else { "s" },
                if models.is_empty() || models.contains(&profile.model) {
                    String::new()
                } else {
                    format!("; {} isn't in it", profile.model)
                }
            ),
            Some(ms(t4)),
        ),
        Err(e) => set(4, Outcome::Fail, e, Some(ms(t4))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_read_the_url() {
        let t = target("https://api.example.com/v1").unwrap();
        assert!(t.https && t.host == "api.example.com" && t.port == 443);
        let t = target("http://[::1]:11434").unwrap();
        assert!(!t.https && t.host == "::1" && t.port == 11434);
        assert!(target("ftp://x").is_err());
        assert!(target("not a url").is_err());
    }

    #[test]
    fn a_closed_port_fails_at_tcp_and_skips_the_rest() {
        // Bind and drop a listener, so the port is (almost surely) closed.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let profile = Profile {
            name: "t".into(),
            base_url: format!("http://127.0.0.1:{port}/v1"),
            wire: crate::endpoints::Wire::Openai,
            model: "m".into(),
            display_name: None,
            chat_path: None,
            headers: Default::default(),
            auth: Default::default(),
            capabilities: Default::default(),
        };
        let mut steps = waiting();
        run(&profile, None, |i, s| steps[i] = s);
        assert_eq!(steps[0].outcome, Outcome::Pass, "{:?}", steps[0]);
        assert_eq!(steps[1].outcome, Outcome::Fail, "{:?}", steps[1]);
        assert!(steps[2..].iter().all(|s| s.outcome == Outcome::Skip));
    }
}
