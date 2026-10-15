//! A port on the remote that leads to a port here, for targets with no
//! `ssh -R`: WSL and containers. ForgeCLI's hidden `forge relay --port N`
//! listens on the remote's 127.0.0.1:N and carries each connection over its
//! stdin/stdout (`wsl.exe` / `docker exec -i`); this side joins each one to
//! 127.0.0.1:<local> here: the model proxy, or the browser tools.
//!
//! Frames, both ways: `kind: u8`, `id: u32` BE, `len: u32` BE, payload.
//! 0 ready (once, from the relay), 1 open, 2 data, 3 close.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::remote::{Error, Extra, Target};

const READY: u8 = 0;
const OPEN: u8 = 1;
const DATA: u8 = 2;
const CLOSE: u8 = 3;

fn frame(kind: u8, id: u32, data: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(9 + data.len());
    f.push(kind);
    f.extend_from_slice(&id.to_be_bytes());
    f.extend_from_slice(&(data.len() as u32).to_be_bytes());
    f.extend_from_slice(data);
    f
}

/// Read one frame; None at the end of the stream.
fn read_frame(r: &mut impl Read) -> Option<(u8, u32, Vec<u8>)> {
    let mut head = [0u8; 9];
    r.read_exact(&mut head).ok()?;
    let id = u32::from_be_bytes([head[1], head[2], head[3], head[4]]);
    let len = u32::from_be_bytes([head[5], head[6], head[7], head[8]]) as usize;
    if len > 1 << 20 {
        return None;
    }
    let mut data = vec![0u8; len];
    r.read_exact(&mut data).ok()?;
    Some((head[0], id, data))
}

/// A running relay; dropping it ends the process on the remote.
pub struct Relay {
    child: Mutex<Child>,
    ready: Mutex<Option<std::sync::mpsc::Receiver<Result<(), String>>>>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Start a relay on `target` (its ForgeCLI at `forge`) listening on
/// `remote_port`, joined to `local_port` here. Returns at once (the UI
/// thread may call it); [`Relay::wait_ready`] waits for it to listen.
pub fn start(
    target: &Target,
    forge: &str,
    remote_port: u16,
    local_port: u16,
    extra: &Extra,
) -> Result<Relay, Error> {
    let script = r#"x() { case $1 in "~") printf '%s' "$HOME";; "~/"*) printf '%s/%s' "$HOME" "${1#"~/"}";; *) printf '%s' "$1";; esac; }
exec "$(x "$1")" relay --port "$2""#;
    let mut cmd = crate::remote::command(
        target,
        script,
        &[forge.to_string(), remote_port.to_string()],
        extra,
    );
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| Error::Other(e.to_string()))?;
    let stdin: Arc<Mutex<ChildStdin>> = Arc::new(Mutex::new(child.stdin.take().expect("piped")));
    let mut stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let input = stdin.clone();
    let label = target.label();
    std::thread::spawn(move || {
        match read_frame(&mut stdout) {
            Some((READY, _, _)) => {
                let _ = ready_tx.send(Ok(()));
            }
            _ => {
                let mut why = String::new();
                let mut stderr = stderr;
                let _ = stderr.read_to_string(&mut why);
                let _ = ready_tx.send(Err(format!(
                    "Couldn't open a way back from {label} (port {remote_port}): {}",
                    why.trim()
                )));
                return;
            }
        }
        pump(&mut stdout, input, local_port);
    });
    Ok(Relay {
        child: Mutex::new(child),
        ready: Mutex::new(Some(ready_rx)),
    })
}

impl Relay {
    /// Wait until the relay listens there, or say why it couldn't.
    pub fn wait_ready(&self, timeout: Duration) -> Result<(), Error> {
        let rx = self.ready.lock().ok().and_then(|mut r| r.take());
        match rx.map(|rx| rx.recv_timeout(timeout)) {
            Some(Ok(Ok(()))) | None => Ok(()),
            Some(Ok(Err(why))) => Err(Error::Other(why)),
            Some(Err(_)) => Err(Error::Other("The relay didn't start in time.".into())),
        }
    }
}

/// The relay's frames: each opened connection joined to the local port.
fn pump(stdout: &mut impl Read, input: Arc<Mutex<ChildStdin>>, local_port: u16) {
    let mut conns: HashMap<u32, TcpStream> = HashMap::new();
    let send = |input: &Arc<Mutex<ChildStdin>>, f: Vec<u8>| -> bool {
        input
            .lock()
            .map(|mut i| i.write_all(&f).and_then(|_| i.flush()).is_ok())
            .unwrap_or(false)
    };
    while let Some((kind, id, data)) = read_frame(stdout) {
        match kind {
            OPEN => match TcpStream::connect(("127.0.0.1", local_port)) {
                Ok(sock) => {
                    if let Ok(mut rd) = sock.try_clone() {
                        let input = input.clone();
                        std::thread::spawn(move || {
                            let mut buf = vec![0u8; 32 * 1024];
                            loop {
                                match rd.read(&mut buf) {
                                    Ok(0) | Err(_) => break,
                                    Ok(n) => {
                                        if !send(&input, frame(DATA, id, &buf[..n])) {
                                            return;
                                        }
                                    }
                                }
                            }
                            send(&input, frame(CLOSE, id, &[]));
                        });
                    }
                    conns.insert(id, sock);
                }
                Err(_) => {
                    send(&input, frame(CLOSE, id, &[]));
                }
            },
            DATA => {
                if let Some(s) = conns.get_mut(&id) {
                    if s.write_all(&data).is_err() {
                        let _ = s.shutdown(Shutdown::Both);
                    }
                }
            }
            CLOSE => {
                if let Some(s) = conns.remove(&id) {
                    let _ = s.shutdown(Shutdown::Write);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let f = frame(DATA, 258, b"hi");
        assert_eq!(f, [2, 0, 0, 1, 2, 0, 0, 0, 2, b'h', b'i']);
        let mut r = &f[..];
        assert_eq!(read_frame(&mut r), Some((DATA, 258, b"hi".to_vec())));
        assert_eq!(read_frame(&mut r), None);
    }

    /// A real relay on a container: a request made inside it to the relay's
    /// port reaches a server here and its answer comes back. Needs
    /// `FORGE_TEST_CONTAINER` (running, with the relay-capable ForgeCLI
    /// already provisioned at `FORGE_TEST_FORGE`, a remote path).
    #[test]
    #[ignore]
    fn real_relay_reaches_back() {
        let name = std::env::var("FORGE_TEST_CONTAINER").expect("FORGE_TEST_CONTAINER");
        let forge = std::env::var("FORGE_TEST_FORGE").expect("FORGE_TEST_FORGE");
        let t = Target::Container { name, user: None };
        // A tiny server here: answers each request with what it got.
        let server = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let local = server.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in server.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut buf = [0u8; 4096];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let got = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let first = got.lines().next().unwrap_or("").to_string();
                    let body = format!("here: {first}");
                    let _ = write!(
                        s,
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        let relay = start(&t, &forge, 41234, local, &Extra::default()).unwrap();
        relay.wait_ready(Duration::from_secs(20)).unwrap();
        // From inside the container: bash's /dev/tcp, no curl needed.
        let script = r#"exec 3<>/dev/tcp/127.0.0.1/41234; printf 'GET /from-inside HTTP/1.1\r\nhost: x\r\n\r\n' >&3; cat <&3"#;
        for _ in 0..3 {
            let out = crate::remote::run(
                &t,
                &format!("bash -c {}", crate::remote::sh_quote(script)),
                &[],
                b"",
                Duration::from_secs(20),
                &Extra::default(),
            )
            .unwrap();
            let text = String::from_utf8_lossy(&out);
            assert!(text.contains("here: GET /from-inside HTTP/1.1"), "{text}");
        }
        drop(relay);
    }
}
