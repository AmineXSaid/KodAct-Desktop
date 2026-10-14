//! SSH passwords and key passphrases, asked in the app.
//!
//! ssh asks through `SSH_ASKPASS`, a program it runs with the prompt as its
//! one argument and reads the answer from. That program is Forge itself:
//! with `FORGE_ASKPASS` in its environment (`port:token`), `main` runs
//! [`client`] instead of the window. The client hands the prompt to the app
//! over a localhost socket; the app shows it as a dialog
//! (`ui/askpass.rs`), and the answer goes back the same way, then out on the
//! client's stdout to ssh.
//!
//! - The socket listens on 127.0.0.1 only, and a request must carry the
//!   random token given to the ssh processes this app started; the token is
//!   never on a command line or in a file.
//! - Answers live in memory for this run, so the heartbeat and later
//!   connections don't ask again; a refused sign-in forgets them. Nothing is
//!   written to disk.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// The environment variable that turns Forge into the askpass client.
pub const ENV: &str = "FORGE_ASKPASS";

/// A prompt for the app to show.
pub struct Request {
    /// What ssh asks, as it says it ("dev@box's password:").
    pub prompt: String,
    /// The answer, or None if the person cancelled.
    pub reply: std::sync::mpsc::Sender<Option<String>>,
}

struct Server {
    port: u16,
    token: String,
}

static SERVER: OnceLock<Server> = OnceLock::new();
static ANSWERS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// Prompts open now: remote timeouts don't run while someone types.
static OPEN: AtomicUsize = AtomicUsize::new(0);
/// The last prompt was cancelled (so a refused sign-in says that).
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the last prompt was cancelled; reading it clears it.
pub fn take_cancelled() -> bool {
    CANCELLED.swap(false, Ordering::SeqCst)
}

/// A prompt is waiting on the person.
pub fn waiting() -> bool {
    OPEN.load(Ordering::SeqCst) > 0
}

/// Forget every answer: a sign-in was refused, so one of them is wrong.
pub fn forget() {
    if let Ok(mut a) = ANSWERS.lock() {
        *a = None;
    }
}

fn token() -> String {
    use ring::rand::SecureRandom as _;
    let mut b = [0u8; 24];
    ring::rand::SystemRandom::new()
        .fill(&mut b)
        .expect("system random");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Start answering askpass requests: each goes to `ask` (the app's dialog).
/// Once per run; later calls do nothing.
pub fn start(ask: async_channel::Sender<Request>) -> std::io::Result<()> {
    if SERVER.get().is_some() {
        return Ok(());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let server = Server {
        port: listener.local_addr()?.port(),
        token: token(),
    };
    let token = server.token.clone();
    if SERVER.set(server).is_err() {
        return Ok(());
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (ask, token) = (ask.clone(), token.clone());
            std::thread::spawn(move || {
                let _ = serve(stream, &token, &ask);
            });
        }
    });
    Ok(())
}

/// One request: `token\nprompt\n` in, `ok\nanswer\n` or `cancel\n` out.
fn serve(
    stream: TcpStream,
    token: &str,
    ask: &async_channel::Sender<Request>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let (mut got, mut prompt) = (String::new(), String::new());
    reader.read_line(&mut got)?;
    reader.read_line(&mut prompt)?;
    let mut out = stream;
    if got.trim_end() != token {
        return out.write_all(b"cancel\n");
    }
    let prompt = prompt.trim_end().to_string();
    let cached = ANSWERS
        .lock()
        .ok()
        .and_then(|a| a.as_ref().and_then(|m| m.get(&prompt).cloned()));
    let answer = match cached {
        Some(a) => Some(a),
        None => {
            let (tx, rx) = std::sync::mpsc::channel();
            OPEN.fetch_add(1, Ordering::SeqCst);
            let sent = ask.send_blocking(Request {
                prompt: prompt.clone(),
                reply: tx,
            });
            let answer = if sent.is_ok() {
                rx.recv().ok().flatten()
            } else {
                None
            };
            OPEN.fetch_sub(1, Ordering::SeqCst);
            CANCELLED.store(answer.is_none(), Ordering::SeqCst);
            if let (Some(a), Ok(mut m)) = (&answer, ANSWERS.lock()) {
                m.get_or_insert_with(HashMap::new).insert(prompt, a.clone());
            }
            answer
        }
    };
    match answer {
        Some(a) => out.write_all(format!("ok\n{a}\n").as_bytes()),
        None => out.write_all(b"cancel\n"),
    }
}

/// What an ssh started by this app needs in its environment to ask here:
/// None until the server runs (ssh then stays in batch mode, keys only).
pub fn env() -> Option<Vec<(String, String)>> {
    let s = SERVER.get()?;
    // `FORGE_ASKPASS_PROGRAM`: the app's binary when this runs elsewhere (tests).
    let exe = std::env::var_os("FORGE_ASKPASS_PROGRAM")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())?;
    let mut out = vec![
        (
            "SSH_ASKPASS".to_string(),
            exe.to_string_lossy().into_owned(),
        ),
        // OpenSSH 8.4+: ask through the program even without a display.
        ("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string()),
        (ENV.to_string(), format!("{}:{}", s.port, s.token)),
    ];
    // Older OpenSSH asks through SSH_ASKPASS only with a display set.
    if std::env::var_os("DISPLAY").is_none() {
        out.push(("DISPLAY".to_string(), "forge:0".to_string()));
    }
    Some(out)
}

/// The askpass client: run by ssh as `forge <prompt>` with `FORGE_ASKPASS`
/// set. Prints the answer for ssh; exits 1 when cancelled or unreachable.
pub fn client(addr: &str, prompt: &str) -> i32 {
    let Some((port, token)) = addr.split_once(':') else {
        return 1;
    };
    let Ok(port) = port.parse::<u16>() else {
        return 1;
    };
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return 1;
    };
    let one_line = prompt.replace(['\r', '\n'], " ");
    if s.write_all(format!("{token}\n{one_line}\n").as_bytes())
        .is_err()
    {
        return 1;
    }
    let mut reader = BufReader::new(s);
    let mut status = String::new();
    if reader.read_line(&mut status).is_err() || status.trim_end() != "ok" {
        return 1;
    }
    let mut answer = String::new();
    if reader.read_line(&mut answer).is_err() {
        return 1;
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(answer.as_bytes());
    let _ = out.flush();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole round trip on this computer: a client with the token gets
    /// the answer given in the "dialog" (here a thread), once; the second
    /// ask is answered from memory; a wrong token gets nothing; forget()
    /// asks again.
    #[test]
    fn prompts_round_trip_and_are_remembered() {
        let (tx, rx) = async_channel::unbounded::<Request>();
        start(tx).unwrap();
        let asked = std::sync::Arc::new(AtomicUsize::new(0));
        let n = asked.clone();
        std::thread::spawn(move || {
            while let Ok(r) = rx.recv_blocking() {
                n.fetch_add(1, Ordering::SeqCst);
                let answer = if r.prompt.contains("cancel") {
                    None
                } else {
                    Some(format!("secret for {}", r.prompt))
                };
                let _ = r.reply.send(answer);
            }
        });
        let s = SERVER.get().unwrap();
        let addr = format!("{}:{}", s.port, s.token);
        // The client prints to stdout; check the server's side directly.
        let ask = |addr: &str, prompt: &str| -> Option<String> {
            let (port, token) = addr.split_once(':').unwrap();
            let mut c = TcpStream::connect(("127.0.0.1", port.parse::<u16>().unwrap())).unwrap();
            c.write_all(format!("{token}\n{prompt}\n").as_bytes())
                .unwrap();
            let mut r = BufReader::new(c);
            let mut status = String::new();
            r.read_line(&mut status).unwrap();
            if status.trim_end() != "ok" {
                return None;
            }
            let mut a = String::new();
            r.read_line(&mut a).unwrap();
            Some(a.trim_end().to_string())
        };
        assert_eq!(
            ask(&addr, "dev@box's password:").as_deref(),
            Some("secret for dev@box's password:")
        );
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert_eq!(
            ask(&addr, "dev@box's password:").as_deref(),
            Some("secret for dev@box's password:"),
            "answered from memory"
        );
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert_eq!(ask(&format!("{}:wrongtoken", s.port), "x"), None);
        assert_eq!(ask(&addr, "please cancel"), None);
        forget();
        ask(&addr, "dev@box's password:");
        assert_eq!(asked.load(Ordering::SeqCst), 3, "asked again after forget");
        assert!(!waiting());
        // The env an ssh gets names this program and the server.
        let env = env().unwrap();
        assert!(env.iter().any(|(k, v)| k == ENV && *v == addr));
        assert!(env
            .iter()
            .any(|(k, v)| k == "SSH_ASKPASS_REQUIRE" && v == "force"));
        // A client pointed nowhere fails quietly.
        assert_eq!(client("1:x", "p"), 1);
        assert_eq!(client("garbage", "p"), 1);
    }

    /// Real ssh asking through the app's binary: a passphrase-locked key
    /// (`FORGE_TEST_SSH_PASSPHRASE_HOST`, passphrase `FORGE_TEST_PASSPHRASE`)
    /// and a password host (`FORGE_TEST_SSH_PASSWORD_HOST`, password
    /// `FORGE_TEST_PASSWORD`). Right answers connect and are remembered;
    /// a wrong one is refused with a clear message, and asked again after.
    /// `FORGE_ASKPASS_PROGRAM` must be the built app.
    #[test]
    #[ignore]
    fn real_ssh_prompts() {
        use crate::remote::{probe, Extra, Target};
        let (tx, rx) = async_channel::unbounded::<Request>();
        start(tx).unwrap();
        let wrong = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let asked = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let (w, a) = (wrong.clone(), asked.clone());
        std::thread::spawn(move || {
            while let Ok(r) = rx.recv_blocking() {
                a.lock().unwrap().push(r.prompt.clone());
                let p = r.prompt.to_lowercase();
                let answer = if w.load(Ordering::SeqCst) {
                    "not-it".to_string()
                } else if p.contains("passphrase") {
                    std::env::var("FORGE_TEST_PASSPHRASE").unwrap()
                } else {
                    std::env::var("FORGE_TEST_PASSWORD").unwrap()
                };
                let _ = r.reply.send(Some(answer));
            }
        });
        let extra = Extra {
            accept_new_host_key: true,
            reverse: vec![],
        };
        for var in [
            "FORGE_TEST_SSH_PASSPHRASE_HOST",
            "FORGE_TEST_SSH_PASSWORD_HOST",
        ] {
            let host = std::env::var(var).expect(var);
            let t = Target::Ssh(host.clone());
            forget();
            asked.lock().unwrap().clear();
            wrong.store(false, Ordering::SeqCst);
            probe(&t, &extra).unwrap_or_else(|e| panic!("{host}: {e}"));
            assert_eq!(asked.lock().unwrap().len(), 1, "{host}: asked once");
            probe(&t, &extra).unwrap();
            assert_eq!(asked.lock().unwrap().len(), 1, "{host}: remembered");
            // Wrong: refused, said plainly, and forgotten.
            forget();
            wrong.store(true, Ordering::SeqCst);
            let err = probe(&t, &extra).unwrap_err().to_string();
            assert!(
                err.contains("password or passphrase was wrong"),
                "{host}: {err}"
            );
            wrong.store(false, Ordering::SeqCst);
            probe(&t, &extra).unwrap();
            eprintln!("{host}: asked {:?}", asked.lock().unwrap());
        }
    }
}
