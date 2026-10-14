//! Remote development, as VS Code does it: a chat's ForgeCLI runs inside a
//! WSL distro, on an SSH host or in a (dev) container, and this app stays the
//! UI. Only how the process starts changes; it speaks the same stream-json
//! over its stdin and stdout (`backend.rs`).
//!
//! - WSL: `wsl.exe -d <distro> --exec sh -c <boot> …`
//! - SSH: `ssh -T <host> -- sh -c '<boot>' …` (the system's OpenSSH client,
//!   so `~/.ssh/config`, keys, the agent and ProxyJump all apply; no
//!   passwords: BatchMode)
//! - Container: `docker exec -i [-u <user>] <name> sh -c <boot> …`
//!
//! A remote project is a project whose path is a URI naming its target:
//! `wsl://Ubuntu/home/me/api`, `ssh://build-box/~/api`,
//! `container://vscode@api/workspaces/api`. Chats belong to projects, so a
//! chat runs where its project lives, and both are saved as they always were.
//!
//! [`BOOT`], a POSIX `sh` script, starts ForgeCLI on the remote: it reads the
//! launch's environment from the first lines of stdin (keys never go on a
//! command line, where `ps` would show them), expands `~/` in the arguments,
//! changes to the project folder and `exec`s ForgeCLI on the same stdin.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Where a remote project lives.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A WSL distro, by name.
    Wsl(String),
    /// An SSH host: a `Host` from `~/.ssh/config`, or `[user@]host[:port]`.
    Ssh(String),
    /// A running container, by name, and the user to run as.
    Container { name: String, user: Option<String> },
}

impl Target {
    fn scheme(&self) -> &'static str {
        match self {
            Target::Wsl(_) => "wsl",
            Target::Ssh(_) => "ssh",
            Target::Container { .. } => "container",
        }
    }

    fn authority(&self) -> String {
        match self {
            Target::Wsl(d) => d.clone(),
            Target::Ssh(h) => h.clone(),
            Target::Container {
                name,
                user: Some(u),
            } => format!("{u}@{name}"),
            Target::Container { name, user: None } => name.clone(),
        }
    }

    /// As the status chip names it: "WSL: Ubuntu", "SSH: build-box".
    pub fn label(&self) -> String {
        match self {
            Target::Wsl(d) => format!("WSL: {d}"),
            Target::Ssh(h) => format!("SSH: {h}"),
            Target::Container { name, .. } => match dev_container_slug(name) {
                Some(slug) => format!("Dev Container: {slug}"),
                None => format!("Container: {name}"),
            },
        }
    }

    /// The icon that goes with it.
    pub fn icon(&self) -> &'static str {
        match self {
            Target::Wsl(_) => "icons/terminal.svg",
            Target::Ssh(_) => "icons/server.svg",
            Target::Container { .. } => "icons/box.svg",
        }
    }
}

/// The folder name in a dev container Forge started (`forge-dc-<slug>-<hash>`).
fn dev_container_slug(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("forge-dc-")?;
    let (slug, hash) = rest.rsplit_once('-')?;
    (hash.len() == 8 && hash.chars().all(|c| c.is_ascii_hexdigit())).then_some(slug)
}

/// A folder on a target: what a remote project is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Remote {
    pub target: Target,
    /// An absolute POSIX path, or one starting `~` (the remote home).
    pub path: String,
}

impl Remote {
    /// A project path naming a remote folder, or None for a local one.
    pub fn parse(p: &Path) -> Option<Remote> {
        let s = p.to_string_lossy().replace('\\', "/");
        let (scheme, rest) = s.split_once("://")?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/~"),
        };
        if authority.is_empty() {
            return None;
        }
        let target = match scheme {
            "wsl" => Target::Wsl(authority.into()),
            "ssh" => Target::Ssh(authority.into()),
            "container" => match authority.split_once('@') {
                Some((u, n)) => Target::Container {
                    name: n.into(),
                    user: Some(u.into()),
                },
                None => Target::Container {
                    name: authority.into(),
                    user: None,
                },
            },
            _ => return None,
        };
        // "/~/x" is "~/x" (the remote home); anything else is absolute.
        let path = match path.strip_prefix("/~") {
            Some(home) if home.is_empty() || home.starts_with('/') => format!("~{home}"),
            _ => path.to_string(),
        };
        Some(Remote { target, path })
    }

    /// The project path that names it.
    pub fn to_path(&self) -> PathBuf {
        let path = if self.path.starts_with('~') {
            format!("/{}", self.path)
        } else {
            self.path.clone()
        };
        PathBuf::from(format!(
            "{}://{}{}",
            self.target.scheme(),
            self.target.authority(),
            path
        ))
    }

    /// The folder's own name, for the project list.
    pub fn name(&self) -> String {
        match self.path.trim_end_matches('/').rsplit('/').next() {
            Some("~") | Some("") | None => "home".into(),
            Some(n) => n.into(),
        }
    }
}

/// Quote one word for a POSIX shell.
pub fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./=:@,+%".contains(&b))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Options for a connection beyond the target itself.
#[derive(Clone, Debug, Default)]
pub struct Extra {
    /// SSH: trust a host key not seen before (after the person agreed).
    pub accept_new_host_key: bool,
    /// SSH: `-R` forwards, as `remote_port:127.0.0.1:local_port`.
    pub reverse: Vec<String>,
}

/// The command that runs `script` with `args` (as `$1`…) on a target.
pub fn command(target: &Target, script: &str, args: &[String], extra: &Extra) -> Command {
    let mut cmd = match target {
        Target::Wsl(distro) => {
            let mut c = Command::new("wsl.exe");
            c.args(["-d", distro, "--exec", "sh", "-c", script, "sh"]);
            c.args(args);
            c
        }
        Target::Ssh(host) => {
            let mut c = Command::new("ssh");
            c.args([
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=15",
                "-o",
                "ServerAliveInterval=15",
            ]);
            if extra.accept_new_host_key {
                c.args(["-o", "StrictHostKeyChecking=accept-new"]);
            }
            for r in &extra.reverse {
                c.args(["-o", "ExitOnForwardFailure=yes", "-R", r]);
            }
            let mut remote = format!("sh -c {} sh", sh_quote(script));
            for a in args {
                remote.push(' ');
                remote.push_str(&sh_quote(a));
            }
            c.args([host.as_str(), "--", &remote]);
            c
        }
        Target::Container { name, user } => {
            let mut c = Command::new("docker");
            c.args(["exec", "-i"]);
            if let Some(u) = user {
                c.args(["-u", u]);
            }
            c.args([name.as_str(), "sh", "-c", script, "sh"]);
            c.args(args);
            c
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// `~` and `~/…` to the remote home, as a shell function the scripts share.
const TILDE: &str = r#"x() { case $1 in "~") printf '%s' "$HOME";; "~/"*) printf '%s/%s' "$HOME" "${1#"~/"}";; *) printf '%s' "$1";; esac; }"#;

/// Starts ForgeCLI on the remote: `$1` the folder, `$2` ForgeCLI, the rest
/// its arguments; the environment comes first on stdin, one `KEY=value` a
/// line (backslash escapes, see [`env_line`]), ended by an empty line.
pub fn boot_script() -> String {
    format!(
        r#"{TILDE}
d=$(x "$1"); f=$(x "$2"); shift 2
n=$#
while [ "$n" -gt 0 ]; do a=$(x "$1"); shift; set -- "$@" "$a"; n=$((n-1)); done
while IFS= read -r l && [ -n "$l" ]; do export "$(printf '%b' "$l")"; done
cd "$d" 2>/dev/null || {{ echo "forge: there is no folder $d on $(uname -n)" >&2; exit 97; }}
exec "$f" "$@""#
    )
}

/// One line of the environment [`boot_script`] reads: backslashes and line
/// breaks escaped, so a value can hold either (`printf %b` undoes it).
pub fn env_line(key: &str, value: &str) -> String {
    let v = value
        .replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace('\r', r"\r");
    format!("{key}={v}\n")
}

/// How to start ForgeCLI in `remote`'s folder: the command, and the bytes to
/// write to its stdin before anything else (the environment).
pub fn launch(
    remote: &Remote,
    forge: &str,
    args: &[String],
    env: &[(String, String)],
    extra: &Extra,
) -> (Command, Vec<u8>) {
    let mut all = vec![remote.path.clone(), forge.to_string()];
    all.extend(args.iter().cloned());
    let cmd = command(&remote.target, &boot_script(), &all, extra);
    let mut prelude = String::new();
    for (k, v) in env {
        prelude.push_str(&env_line(k, v));
    }
    prelude.push('\n');
    (cmd, prelude.into_bytes())
}

/// Run a short script on a target and return its stdout. `input` goes to
/// its stdin. Errors read as the person would want them explained.
pub fn run(
    target: &Target,
    script: &str,
    args: &[String],
    input: &[u8],
    timeout: Duration,
    extra: &Extra,
) -> Result<Vec<u8>, Error> {
    let mut child = command(target, script, args, extra)
        .spawn()
        .map_err(|e| Error::spawn(target, e))?;
    let mut stdin = child.stdin.take();
    let data = input.to_vec();
    let writer = std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(&data);
        }
        drop(stdin);
    });
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut b = vec![];
        let _ = out.read_to_end(&mut b);
        b
    });
    let err_reader = std::thread::spawn(move || {
        let mut b = String::new();
        let _ = err.read_to_string(&mut b);
        b
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Timeout(target.label()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(Error::Other(e.to_string())),
        }
    };
    let _ = writer.join();
    let stdout = reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if status.success() {
        Ok(stdout)
    } else {
        Err(Error::classify(target, status.code(), &stderr))
    }
}

/// Why a remote action failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The SSH host's key isn't known yet: its fingerprint, to show and ask.
    UnknownHostKey {
        host: String,
        fingerprint: String,
    },
    Timeout(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnknownHostKey { host, fingerprint } => write!(
                f,
                "{host} isn't a known host yet. Its key fingerprint is {fingerprint}. Trust it only if it matches the server's."
            ),
            Error::Timeout(t) => write!(f, "{t} didn't answer in time."),
            Error::Other(m) => f.write_str(m),
        }
    }
}

impl Error {
    fn spawn(target: &Target, e: std::io::Error) -> Error {
        let what = match target {
            Target::Wsl(_) => "WSL isn't installed here (wsl.exe wasn't found). Install it with `wsl --install`.",
            Target::Ssh(_) => "No SSH client was found. Install OpenSSH (Windows: Settings → Optional features → OpenSSH Client).",
            Target::Container { .. } => "Docker wasn't found. Install Docker Desktop, or put `docker` on PATH.",
        };
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::Other(what.into())
        } else {
            Error::Other(format!("Couldn't start the connection: {e}"))
        }
    }

    /// Turn a failed command's stderr into what to tell the person.
    pub fn classify(target: &Target, code: Option<i32>, stderr: &str) -> Error {
        let s = stderr.trim();
        let has = |n: &str| s.contains(n);
        let msg = match target {
            Target::Ssh(host) => {
                if has("Host key verification failed") || has("No ED25519 host key is known") {
                    return Error::UnknownHostKey {
                        host: host.clone(),
                        fingerprint: fingerprint(host).unwrap_or_else(|| "unknown".into()),
                    };
                }
                if has("REMOTE HOST IDENTIFICATION HAS CHANGED") {
                    format!("{host}'s key has changed since you last connected. If that's expected, remove its old line from ~/.ssh/known_hosts.")
                } else if has("Permission denied") {
                    format!("{host} refused the sign-in. Forge signs in with your SSH keys or agent (no passwords): check `ssh {host}` works in a terminal.")
                } else if has("Could not resolve hostname") {
                    format!("{host} couldn't be found (name not resolved).")
                } else if has("Connection refused") {
                    format!("{host} refused the connection: is its SSH server running?")
                } else if has("timed out") {
                    format!("{host} didn't answer (connection timed out).")
                } else {
                    String::new()
                }
            }
            Target::Wsl(d) => {
                // wsl.exe writes UTF-16 for its own errors.
                let s = s.replace('\0', "");
                if s.contains("no distribution") || s.contains("WSL_E_DISTRO_NOT_FOUND") {
                    format!("There's no WSL distro named {d}.")
                } else {
                    String::new()
                }
            }
            Target::Container { name, .. } => {
                if has("Cannot connect to the Docker daemon") || has("docker daemon is not running")
                {
                    "Docker isn't running. Start Docker Desktop (or the docker service).".into()
                } else if has("No such container") {
                    format!("There's no container named {name}.")
                } else if has("is not running") {
                    format!("The container {name} is stopped.")
                } else {
                    String::new()
                }
            }
        };
        if !msg.is_empty() {
            return Error::Other(msg);
        }
        let last = s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
        Error::Other(match code {
            Some(c) if last.is_empty() => format!("{} failed (exit {c}).", target.label()),
            _ if last.is_empty() => format!("{} failed.", target.label()),
            _ => format!("{}: {}", target.label(), last.trim()),
        })
    }
}

/// An SSH host's key fingerprint, as `ssh-keyscan | ssh-keygen -lf -` gives it.
fn fingerprint(host: &str) -> Option<String> {
    // The real name and port, as ssh resolves them (~/.ssh/config aliases).
    let (mut name, mut port) = ssh_host_port(host);
    if let Ok(o) = Command::new("ssh")
        .args(["-G", host])
        .stderr(Stdio::null())
        .output()
    {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            match line.split_once(' ') {
                Some(("hostname", h)) => name = h.trim().to_string(),
                Some(("port", p)) => port = Some(p.trim().to_string()),
                _ => {}
            }
        }
    }
    let mut scan = Command::new("ssh-keyscan");
    scan.args(["-T", "5", "-t", "ed25519,ecdsa,rsa"]);
    if let Some(p) = port {
        scan.args(["-p", &p]);
    }
    let keys = scan.arg(&name).stderr(Stdio::null()).output().ok()?.stdout;
    let mut kg = Command::new("ssh-keygen")
        .args(["-lf", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    kg.stdin.take()?.write_all(&keys).ok()?;
    let out = String::from_utf8(kg.wait_with_output().ok()?.stdout).ok()?;
    out.split_whitespace()
        .find(|w| w.starts_with("SHA256:"))
        .map(str::to_string)
}

fn ssh_host_port(host: &str) -> (String, Option<String>) {
    let h = host.rsplit('@').next().unwrap_or(host);
    match h.rsplit_once(':') {
        Some((n, p)) if p.chars().all(|c| c.is_ascii_digit()) => (n.into(), Some(p.into())),
        _ => (h.into(), None),
    }
}

// ---------------------------------------------------------------- what's there

/// What a target is, from `uname`: only Linux runs ForgeCLI remotely.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub arch: String,
    pub home: String,
}

/// Connect and ask what the target is. Fails with the reason if it can't.
pub fn probe(target: &Target, extra: &Extra) -> Result<Info, Error> {
    let out = run(
        target,
        r#"uname -sm; printf '%s\n' "$HOME""#,
        &[],
        b"",
        Duration::from_secs(30),
        extra,
    )?;
    let text = String::from_utf8_lossy(&out);
    let mut lines = text.lines();
    let uname = lines.next().unwrap_or_default();
    let home = lines.next().unwrap_or_default().trim().to_string();
    let mut words = uname.split_whitespace();
    let (os, arch) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    if os != "Linux" {
        return Err(Error::Other(format!(
            "{} runs {os}; Forge runs remotely on Linux only.",
            target.label()
        )));
    }
    Ok(Info {
        arch: arch.to_string(),
        home,
    })
}

/// One entry of a remote folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
}

/// A remote folder's entries (hidden ones included), and its full path.
pub fn list_dir(target: &Target, path: &str, extra: &Extra) -> Result<(String, Vec<Entry>), Error> {
    let script = format!(
        r#"{TILDE}
cd "$(x "$1")" 2>/dev/null || {{ echo "There's no folder $1." >&2; exit 3; }}
pwd; ls -1Ap"#
    );
    let out = run(
        target,
        &script,
        &[path.to_string()],
        b"",
        Duration::from_secs(20),
        extra,
    )?;
    Ok(parse_listing(&String::from_utf8_lossy(&out)))
}

fn parse_listing(text: &str) -> (String, Vec<Entry>) {
    let mut lines = text.lines();
    let full = lines.next().unwrap_or("/").to_string();
    let mut entries: Vec<Entry> = lines
        .filter(|l| !l.is_empty())
        .map(|l| match l.strip_suffix('/') {
            Some(d) => Entry {
                name: d.into(),
                dir: true,
            },
            None => Entry {
                name: l.into(),
                dir: false,
            },
        })
        .collect();
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    (full, entries)
}

/// A remote file's bytes, up to `max` (bigger files are refused).
pub fn read_file(target: &Target, path: &str, max: u64, extra: &Extra) -> Result<Vec<u8>, Error> {
    let script = format!(
        r#"{TILDE}
f=$(x "$1")
[ -f "$f" ] || {{ echo "There's no file $1." >&2; exit 3; }}
s=$(wc -c < "$f")
[ "$s" -le "$2" ] || {{ echo "$1 is too big to open here ($s bytes)." >&2; exit 4; }}
cat -- "$f""#
    );
    run(
        target,
        &script,
        &[path.to_string(), max.to_string()],
        b"",
        Duration::from_secs(60),
        extra,
    )
}

/// A remote project's files, relative to its folder, for the @ list:
/// git's view (tracked and untracked, ignored ones left out) where it's a
/// repository, else every file outside hidden folders.
pub fn project_files(
    target: &Target,
    path: &str,
    cap: usize,
    extra: &Extra,
) -> Result<Vec<String>, Error> {
    let script = format!(
        r#"{TILDE}
cd "$(x "$1")" 2>/dev/null || exit 3
{{ git ls-files -co --exclude-standard 2>/dev/null || find . -type f -not -path '*/.*' | sed 's|^\./||'; }} | head -n "$2""#
    );
    let out = run(
        target,
        &script,
        &[path.to_string(), cap.to_string()],
        b"",
        Duration::from_secs(60),
        extra,
    )?;
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

// ---------------------------------------------------------------- discovery

/// A WSL distro, from `wsl.exe -l -v`.
#[derive(Clone, Debug, PartialEq)]
pub struct Distro {
    pub name: String,
    pub running: bool,
    pub version: u8,
    pub default: bool,
}

/// `wsl.exe -l -v`'s output (UTF-16LE, sometimes with a BOM).
pub fn parse_wsl_list(bytes: &[u8]) -> Vec<Distro> {
    let body = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
    let utf16 = body.len() >= 2 && body.iter().skip(1).step_by(2).take(8).all(|b| *b == 0);
    let text = if utf16 {
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    let text = text.trim_start_matches('\u{feff}');
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let l = l.trim_end();
            let default = l.trim_start().starts_with('*');
            let l = l.trim_start().trim_start_matches('*').trim();
            let mut cols: Vec<&str> = l.split_whitespace().collect();
            let version = cols.pop()?.parse().ok()?;
            let state = cols.pop()?;
            let name = cols.join(" ");
            (!name.is_empty()).then(|| Distro {
                name,
                running: state.eq_ignore_ascii_case("Running"),
                version,
                default,
            })
        })
        .collect()
}

/// The WSL distros on this machine (none where there's no WSL).
pub fn wsl_distros() -> Result<Vec<Distro>, Error> {
    let mut c = Command::new("wsl.exe");
    c.args(["-l", "-v"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    match c.output() {
        Ok(o) => Ok(parse_wsl_list(&o.stdout)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::Other(
            "WSL isn't installed here. Install it with `wsl --install`.".into(),
        )),
        Err(e) => Err(Error::Other(e.to_string())),
    }
}

/// The `Host` names in an ssh config (patterns with `*`, `?` or `!` left
/// out), following `Include`s of plain files.
pub fn parse_ssh_config(text: &str, include: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut hosts = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (key, rest) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, r)) => (k, r.trim().trim_start_matches('=').trim()),
            None => continue,
        };
        if key.eq_ignore_ascii_case("Host") {
            for h in rest.split_whitespace() {
                if !h.contains(['*', '?', '!']) && !hosts.iter().any(|x: &String| x == h) {
                    hosts.push(h.to_string());
                }
            }
        } else if key.eq_ignore_ascii_case("Include") {
            for f in rest.split_whitespace() {
                if let Some(t) = include(f) {
                    for h in parse_ssh_config(&t, include) {
                        if !hosts.contains(&h) {
                            hosts.push(h);
                        }
                    }
                }
            }
        }
    }
    hosts
}

/// The hosts in `~/.ssh/config`.
pub fn ssh_hosts() -> Vec<String> {
    let Some(home) = home_dir() else {
        return vec![];
    };
    let ssh = home.join(".ssh");
    let text = std::fs::read_to_string(ssh.join("config")).unwrap_or_default();
    let include = |f: &str| {
        if f.contains(['*', '?']) {
            return None;
        }
        let p = match f.strip_prefix("~/") {
            Some(r) => home.join(r),
            None if Path::new(f).is_absolute() => PathBuf::from(f),
            None => ssh.join(f),
        };
        std::fs::read_to_string(p).ok()
    };
    parse_ssh_config(&text, &include)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// A running container, from `docker ps`.
#[derive(Clone, Debug, PartialEq)]
pub struct Container {
    pub name: String,
    pub image: String,
    pub status: String,
}

pub fn parse_docker_ps(text: &str) -> Vec<Container> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some(Container {
                name: f.next()?.trim().to_string(),
                image: f.next().unwrap_or_default().trim().to_string(),
                status: f.next().unwrap_or_default().trim().to_string(),
            })
        })
        .filter(|c| !c.name.is_empty())
        .collect()
}

/// The running containers.
pub fn containers() -> Result<Vec<Container>, Error> {
    let mut c = Command::new("docker");
    c.args(["ps", "--format", "{{.Names}}\t{{.Image}}\t{{.Status}}"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let target = Target::Container {
        name: String::new(),
        user: None,
    };
    let o = c.output().map_err(|e| Error::spawn(&target, e))?;
    if !o.status.success() {
        return Err(Error::classify(
            &target,
            o.status.code(),
            &String::from_utf8_lossy(&o.stderr),
        ));
    }
    Ok(parse_docker_ps(&String::from_utf8_lossy(&o.stdout)))
}

// ---------------------------------------------------------------- dev containers

/// What Forge uses from a `devcontainer.json`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DevContainer {
    pub name: Option<String>,
    pub image: Option<String>,
    /// The Dockerfile and build context, relative to the `.devcontainer` folder.
    pub dockerfile: Option<String>,
    pub context: Option<String>,
    pub workspace_folder: Option<String>,
    pub remote_user: Option<String>,
    pub post_create: Option<String>,
    pub run_args: Vec<String>,
}

/// `devcontainer.json` is JSON with comments and trailing commas.
pub fn parse_devcontainer(text: &str) -> Result<DevContainer, String> {
    let clean = strip_jsonc(text);
    let v: serde_json::Value = serde_json::from_str(&clean).map_err(|e| e.to_string())?;
    let s = |p: &str| v.pointer(p).and_then(|x| x.as_str()).map(str::to_string);
    let post_create = match v.get("postCreateCommand") {
        Some(serde_json::Value::String(c)) => Some(c.clone()),
        Some(serde_json::Value::Array(a)) => Some(
            a.iter()
                .filter_map(|x| x.as_str())
                .map(sh_quote)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    };
    Ok(DevContainer {
        name: s("/name"),
        image: s("/image"),
        dockerfile: s("/build/dockerfile").or_else(|| s("/dockerFile")),
        context: s("/build/context").or_else(|| s("/context")),
        workspace_folder: s("/workspaceFolder"),
        remote_user: s("/remoteUser").or_else(|| s("/containerUser")),
        post_create,
        run_args: v
            .get("runArgs")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_str = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    // Trailing commas before } or ].
    let mut res = String::with_capacity(out.len());
    let bytes: Vec<char> = out.chars().collect();
    let mut in_str = false;
    for (i, c) in bytes.iter().enumerate() {
        if *c == '"' && (i == 0 || bytes[i - 1] != '\\') {
            in_str = !in_str;
        }
        if !in_str && *c == ',' {
            let next = bytes[i + 1..].iter().find(|c| !c.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }
        res.push(*c);
    }
    res
}

/// The dev container definition in a local folder, if it has one.
pub fn find_devcontainer(dir: &Path) -> Option<(PathBuf, DevContainer)> {
    for p in [
        dir.join(".devcontainer").join("devcontainer.json"),
        dir.join(".devcontainer.json"),
    ] {
        if let Ok(t) = std::fs::read_to_string(&p) {
            return parse_devcontainer(&t).ok().map(|d| (p, d));
        }
    }
    None
}

fn docker(args: &[&str]) -> Result<String, Error> {
    let target = Target::Container {
        name: String::new(),
        user: None,
    };
    let mut c = Command::new("docker");
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().map_err(|e| Error::spawn(&target, e))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(Error::classify(
            &target,
            o.status.code(),
            &String::from_utf8_lossy(&o.stderr),
        ))
    }
}

/// Reopen a local folder in its dev container: build or pull the image,
/// start (or reuse) a container with the folder mounted, run
/// `postCreateCommand` once, and return the remote project.
pub fn devcontainer_up(dir: &Path, progress: &dyn Fn(&str)) -> Result<Remote, Error> {
    let (file, dc) = find_devcontainer(dir).ok_or_else(|| {
        Error::Other("This folder has no .devcontainer/devcontainer.json.".into())
    })?;
    let base = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workspace".into());
    let mut h = DefaultHasher::new();
    dir.hash(&mut h);
    let key = format!("{:08x}", h.finish() as u32);
    let slug: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let name = format!("forge-dc-{slug}-{key}");
    let workspace = dc
        .workspace_folder
        .clone()
        .unwrap_or_else(|| format!("/workspaces/{base}"));
    let target = Target::Container {
        name: name.clone(),
        user: dc.remote_user.clone(),
    };
    let remote = Remote {
        target,
        path: workspace.clone(),
    };

    // Already there: start it if stopped.
    let existing = docker(&[
        "ps",
        "-a",
        "--filter",
        &format!("name=^{name}$"),
        "--format",
        "{{.State}}",
    ])?;
    if !existing.is_empty() {
        if existing != "running" {
            progress("Starting the dev container…");
            docker(&["start", &name])?;
        }
        return Ok(remote);
    }

    let image = match (&dc.image, &dc.dockerfile) {
        (Some(i), _) => {
            progress(&format!("Pulling {i}…"));
            // A local image needs no pull; a failed pull shows when it runs.
            let _ = docker(&["pull", i]);
            i.clone()
        }
        (None, Some(df)) => {
            let folder = file.parent().unwrap_or(dir);
            let ctx = folder.join(dc.context.as_deref().unwrap_or("."));
            let tag = format!("forge-dc-{slug}-{key}:latest");
            progress("Building the dev container image…");
            docker(&[
                "build",
                "-t",
                &tag,
                "-f",
                &folder.join(df).to_string_lossy(),
                &ctx.to_string_lossy(),
            ])?;
            tag
        }
        (None, None) => {
            return Err(Error::Other(
                "devcontainer.json names neither an image nor a Dockerfile (Compose isn't supported yet).".into(),
            ))
        }
    };
    progress("Starting the dev container…");
    let mount = format!("{}:{workspace}", dir.to_string_lossy());
    let label = format!("forge.devcontainer={}", dir.to_string_lossy());
    let mut args: Vec<String> = [
        "run", "-d", "--name", &name, "--label", &label, "-v", &mount, "-w", &workspace,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(dc.run_args.iter().cloned());
    args.extend([image, "sleep".into(), "infinity".into()]);
    docker(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
    if let Some(cmd) = &dc.post_create {
        progress("Running postCreateCommand…");
        run(
            &remote.target,
            r#"cd "$1" && sh -c "$2""#,
            &[workspace.clone(), cmd.clone()],
            b"",
            Duration::from_secs(1800),
            &Extra::default(),
        )?;
    }
    Ok(remote)
}

// ---------------------------------------------------------------- ForgeCLI on the remote

/// Where a target's ForgeCLI and Forge's plugin were put.
#[derive(Clone, Debug, PartialEq)]
pub struct Provisioned {
    pub forge: String,
    pub plugin: String,
}

/// The Linux ForgeCLI this app carries for an architecture (`uname -m`):
/// `FORGE_REMOTE_CLI`, else `bin/linux-<arch>/forge` beside the app.
pub fn local_forge(arch: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FORGE_REMOTE_CLI").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        a => a,
    };
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [
        dir.join("bin").join(format!("linux-{arch}")).join("forge"),
        dir.join(format!("linux-{arch}")).join("forge"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

fn content_id(bytes: &[u8]) -> String {
    // FNV-1a: stable across builds of this app, unlike DefaultHasher.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Make sure the target has this app's ForgeCLI and plugin, uploading what's
/// missing (once per version), and say where they are.
pub fn provision(
    target: &Target,
    extra: &Extra,
    progress: &dyn Fn(&str),
) -> Result<Provisioned, Error> {
    progress(&format!("Connecting to {}…", target.label()));
    let info = probe(target, extra)?;
    let local = local_forge(&info.arch).ok_or_else(|| {
        Error::Other(format!(
            "This Forge has no ForgeCLI for Linux {} to put on {}.",
            info.arch,
            target.label()
        ))
    })?;
    let bin = std::fs::read(&local).map_err(|e| Error::Other(e.to_string()))?;
    let forge = format!("~/.forge/bin/forge-{}", content_id(&bin));
    let plugin_files = crate::plugin::files();
    let plugin = format!("~/.forge/plugin-{}/forge", crate::plugin::hash());
    let check = format!(
        r#"{TILDE}
[ -x "$(x "$1")" ] && echo forge
[ -f "$(x "$2")/.unpacked" ] && echo plugin
true"#
    );
    let have = String::from_utf8_lossy(&run(
        target,
        &check,
        &[forge.clone(), plugin.clone()],
        b"",
        Duration::from_secs(30),
        extra,
    )?)
    .into_owned();
    if !have.lines().any(|l| l == "forge") {
        progress(&format!("Installing ForgeCLI on {}…", target.label()));
        let put = format!(
            r#"{TILDE}
f=$(x "$1"); mkdir -p "$(dirname "$f")" && cat > "$f.part" && chmod 755 "$f.part" && mv -f "$f.part" "$f""#
        );
        run(
            target,
            &put,
            std::slice::from_ref(&forge),
            &bin,
            Duration::from_secs(600),
            extra,
        )?;
    }
    if !have.lines().any(|l| l == "plugin") {
        let archive = tar(plugin_files);
        let put = format!(
            r#"{TILDE}
d=$(x "$1"); mkdir -p "$d" && tar -xf - -C "$d" && touch "$d/.unpacked""#
        );
        run(
            target,
            &put,
            std::slice::from_ref(&plugin),
            &archive,
            Duration::from_secs(120),
            extra,
        )?;
    }
    Ok(Provisioned { forge, plugin })
}

/// A ustar archive of `(path, bytes)` files, for `tar -x` on the remote.
pub fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = vec![];
    for (path, bytes) in files {
        let mut h = [0u8; 512];
        let (prefix, name) = if path.len() <= 100 {
            ("", *path)
        } else {
            let cut = path[..path.len().min(155)].rfind('/').unwrap_or(0);
            (&path[..cut], &path[cut + 1..])
        };
        let put = |h: &mut [u8; 512], at: usize, s: &[u8]| h[at..at + s.len()].copy_from_slice(s);
        put(&mut h, 0, &name.as_bytes()[..name.len().min(100)]);
        put(&mut h, 100, b"0000644\0");
        put(&mut h, 108, b"0000000\0");
        put(&mut h, 116, b"0000000\0");
        put(&mut h, 124, format!("{:011o}\0", bytes.len()).as_bytes());
        put(&mut h, 136, b"00000000000\0");
        put(&mut h, 148, b"        ");
        h[156] = b'0';
        put(&mut h, 257, b"ustar\0");
        put(&mut h, 263, b"00");
        put(&mut h, 345, &prefix.as_bytes()[..prefix.len().min(155)]);
        let sum: u32 = h.iter().map(|b| *b as u32).sum();
        put(&mut h, 148, format!("{sum:06o}\0 ").as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(bytes);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

/// The port of a URL on this computer (`localhost`, `127.0.0.1`, `[::1]`),
/// to forward to a remote; None for any other host.
pub fn local_port(url: &str) -> Option<u16> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split('/').next()?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && !p.contains(']') => (h, p.parse().ok()?),
        _ if url.starts_with("https") => (authority, 443),
        _ => (authority, 80),
    };
    matches!(host, "localhost" | "127.0.0.1" | "[::1]").then_some(port)
}

/// The port a session's forward listens on at the remote end: its own per
/// session (two chats on one host each forward the model's port), picked
/// from the session's id and the local port, in 30000–59999.
pub fn forward_port(session: &str, local: u16) -> u16 {
    let mut h: u32 = 0x811c_9dc5;
    for b in session.bytes().chain(local.to_le_bytes()) {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    30_000 + (h % 30_000) as u16
}

/// `url` with its port replaced (the forward's end on the remote).
pub fn with_port(url: &str, port: u16) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = match authority.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => h,
        _ => authority,
    };
    format!("{scheme}://{host}:{port}{path}")
}

/// The browser's tool server as a remote ForgeCLI reaches it: over HTTP on
/// the remote's `127.0.0.1:<port>` (an `ssh -R` forward; for WSL its
/// mirrored networking, for a container its host network).
pub fn browser_mcp_config(port: u16, token: &str) -> String {
    serde_json::json!({ "mcpServers": { crate::browser_mcp::SERVER: {
        "type": "http",
        "url": format!("http://127.0.0.1:{port}/mcp/{token}"),
    } } })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn project_paths_name_their_target() {
        for (s, target, path) in [
            (
                "wsl://Ubuntu/home/me/api",
                Target::Wsl("Ubuntu".into()),
                "/home/me/api",
            ),
            (
                "ssh://build-box/~/api",
                Target::Ssh("build-box".into()),
                "~/api",
            ),
            (
                "ssh://me@host:2222/~",
                Target::Ssh("me@host:2222".into()),
                "~",
            ),
            (
                "container://vscode@api/workspaces/api",
                Target::Container {
                    name: "api".into(),
                    user: Some("vscode".into()),
                },
                "/workspaces/api",
            ),
        ] {
            let r = Remote::parse(Path::new(s)).unwrap();
            assert_eq!(
                r,
                Remote {
                    target,
                    path: path.into()
                },
                "{s}"
            );
            assert_eq!(r.to_path(), PathBuf::from(s), "round trip");
        }
        assert_eq!(Remote::parse(Path::new("/home/me/api")), None);
        assert_eq!(Remote::parse(Path::new(r"C:\Users\me\api")), None);
        assert_eq!(Remote::parse(Path::new("ssh://host")).unwrap().path, "~");
        assert_eq!(
            Remote::parse(Path::new("ssh://h/~/api")).unwrap().name(),
            "api"
        );
        assert_eq!(
            Remote::parse(Path::new("ssh://h/~")).unwrap().name(),
            "home"
        );
        let dc = Target::Container {
            name: "forge-dc-api-0a1b2c3d".into(),
            user: None,
        };
        assert_eq!(dc.label(), "Dev Container: api");
        let plain = Target::Container {
            name: "web".into(),
            user: None,
        };
        assert_eq!(plain.label(), "Container: web");
    }

    #[test]
    fn quoting_survives_the_shell() {
        for s in [
            "plain",
            "with space",
            "it's",
            "$HOME `x` \"q\"",
            "ünï ✓",
            "",
            "a\nb",
        ] {
            let out = Command::new("sh")
                .args(["-c", &format!("printf '%s' {}", sh_quote(s))])
                .output()
                .unwrap();
            assert_eq!(String::from_utf8(out.stdout).unwrap(), s);
        }
    }

    #[test]
    fn commands_per_target() {
        let ssh = command(
            &Target::Ssh("box".into()),
            "echo $1",
            &["a b".into()],
            &Extra::default(),
        );
        assert_eq!(ssh.get_program(), "ssh");
        let a = args(&ssh);
        assert!(a.contains(&"BatchMode=yes".into()));
        assert_eq!(a[a.len() - 3..a.len() - 1], ["box", "--"]);
        assert_eq!(a.last().unwrap(), "sh -c 'echo $1' sh 'a b'");
        let e = Extra {
            accept_new_host_key: true,
            reverse: vec!["5000:127.0.0.1:6000".into()],
        };
        let a = args(&command(&Target::Ssh("box".into()), "true", &[], &e));
        assert!(a.contains(&"StrictHostKeyChecking=accept-new".into()));
        assert!(a.windows(2).any(|w| w == ["-R", "5000:127.0.0.1:6000"]));
        let wsl = command(
            &Target::Wsl("Ubuntu".into()),
            "s",
            &["x".into()],
            &Extra::default(),
        );
        assert_eq!(wsl.get_program(), "wsl.exe");
        assert_eq!(
            args(&wsl),
            ["-d", "Ubuntu", "--exec", "sh", "-c", "s", "sh", "x"]
        );
        let dc = command(
            &Target::Container {
                name: "api".into(),
                user: Some("vscode".into()),
            },
            "s",
            &[],
            &Extra::default(),
        );
        assert_eq!(
            args(&dc),
            ["exec", "-i", "-u", "vscode", "api", "sh", "-c", "s", "sh"]
        );
    }

    /// The boot script, run by a real `sh`: the environment arrives on
    /// stdin (escapes and all), `~/` is expanded, the folder is entered, and
    /// the program gets the rest of stdin. Nothing secret is in argv.
    #[test]
    fn boot_reads_the_environment_from_stdin() {
        let home = std::env::temp_dir().join(format!("forge-boot-{}", std::process::id()));
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let mut prelude = String::new();
        prelude.push_str(&env_line("FORGE_API_KEY", "sk-secret"));
        prelude.push_str(&env_line("FORGE_CUSTOM_HEADERS", "A: 1\nB: 2\\x"));
        prelude.push('\n');
        let script = boot_script();
        let mut child = Command::new("sh")
            .args(["-c", &script, "sh", "~/proj", "/bin/sh", "-c"])
            .arg(r#"printf '%s|%s|%s|' "$PWD" "$FORGE_API_KEY" "$FORGE_CUSTOM_HEADERS"; cat; printf '|%s' "$1""#)
            .args(["arg0", "~/x"])
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(prelude.as_bytes()).unwrap();
        stdin.write_all(b"{\"type\":\"user\"}\n").unwrap();
        drop(stdin);
        let out = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
        let want = format!(
            "{}|sk-secret|A: 1\nB: 2\\x|{{\"type\":\"user\"}}\n|{}/x",
            home.join("proj").canonicalize().unwrap().display(),
            home.display()
        );
        assert_eq!(out, want);
        std::fs::remove_dir_all(home).ok();
    }

    #[test]
    fn a_missing_folder_says_so() {
        let out = Command::new("sh")
            .args(["-c", &boot_script(), "sh", "/no/such/dir", "/bin/true"])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(97));
        assert!(String::from_utf8_lossy(&out.stderr).contains("there is no folder /no/such/dir"));
    }

    #[test]
    fn wsl_lists_in_utf16() {
        let text = "  NAME            STATE           VERSION\r\n* Ubuntu-22.04    Running         2\r\n  Debian          Stopped         2\r\n  Old Linux       Stopped         1\r\n";
        let mut bytes = vec![0xff, 0xfe];
        for u in text.encode_utf16() {
            bytes.extend(u.to_le_bytes());
        }
        let d = parse_wsl_list(&bytes);
        assert_eq!(d.len(), 3);
        assert_eq!(
            d[0],
            Distro {
                name: "Ubuntu-22.04".into(),
                running: true,
                version: 2,
                default: true
            }
        );
        assert_eq!(d[1].name, "Debian");
        assert!(!d[1].running && !d[1].default);
        assert_eq!(d[2].name, "Old Linux");
        assert_eq!(d[2].version, 1);
        assert!(parse_wsl_list(b"").is_empty());
    }

    #[test]
    fn ssh_config_hosts() {
        let main = "Host build-box gpu\n  HostName 10.0.0.2\n# comment\nHost *.internal !bad\nInclude extra\nHost=web\nMatch host x\n";
        let hosts = parse_ssh_config(main, &|f| {
            (f == "extra").then(|| "Host db\nHost build-box".to_string())
        });
        assert_eq!(hosts, ["build-box", "gpu", "db", "web"]);
    }

    #[test]
    fn docker_ps_lines() {
        let c = parse_docker_ps("api\tnode:20\tUp 3 hours\ndb\tpostgres:16\tUp 2 days\n\n");
        assert_eq!(c.len(), 2);
        assert_eq!(
            c[1],
            Container {
                name: "db".into(),
                image: "postgres:16".into(),
                status: "Up 2 days".into()
            }
        );
    }

    #[test]
    fn devcontainer_json_with_comments() {
        let dc = parse_devcontainer(
            r#"{
              // the image
              "name": "API", /* block */
              "build": { "dockerfile": "Dockerfile", "context": "..", },
              "workspaceFolder": "/workspaces/api",
              "remoteUser": "vscode",
              "postCreateCommand": ["npm", "ci"],
              "runArgs": ["--cap-add=SYS_PTRACE"],
              "url": "http://example.com/a//b",
            }"#,
        )
        .unwrap();
        assert_eq!(dc.name.as_deref(), Some("API"));
        assert_eq!(dc.dockerfile.as_deref(), Some("Dockerfile"));
        assert_eq!(dc.context.as_deref(), Some(".."));
        assert_eq!(dc.workspace_folder.as_deref(), Some("/workspaces/api"));
        assert_eq!(dc.remote_user.as_deref(), Some("vscode"));
        assert_eq!(dc.post_create.as_deref(), Some("npm ci"));
        assert_eq!(dc.run_args, ["--cap-add=SYS_PTRACE"]);
        let img = parse_devcontainer(r#"{"image":"mcr.microsoft.com/devcontainers/rust:1","postCreateCommand":"cargo build"}"#).unwrap();
        assert_eq!(
            img.image.as_deref(),
            Some("mcr.microsoft.com/devcontainers/rust:1")
        );
        assert_eq!(img.post_create.as_deref(), Some("cargo build"));
    }

    #[test]
    fn errors_explain_themselves() {
        let ssh = Target::Ssh("box".into());
        assert!(
            Error::classify(&ssh, Some(255), "me@box: Permission denied (publickey).")
                .to_string()
                .contains("refused the sign-in")
        );
        assert!(Error::classify(
            &ssh,
            Some(255),
            "ssh: Could not resolve hostname box: Name or service not known"
        )
        .to_string()
        .contains("couldn't be found"));
        let dc = Target::Container {
            name: "api".into(),
            user: None,
        };
        assert!(Error::classify(
            &dc,
            Some(1),
            "Error response from daemon: No such container: api"
        )
        .to_string()
        .contains("no container named api"));
        assert_eq!(
            Error::classify(&dc, Some(2), "line one\nthe real reason\n").to_string(),
            "Container: api: the real reason"
        );
    }

    /// The archive unpacks with the system's `tar`, long paths included.
    #[test]
    fn tar_unpacks() {
        let dir = std::env::temp_dir().join(format!("forge-tar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let long = format!("skills/{}/SKILL.md", "deep/".repeat(25));
        let files: Vec<(&str, &[u8])> = vec![
            ("a.txt", b"hello"),
            (long.as_str(), b"long one"),
            ("empty", b""),
        ];
        let mut child = Command::new("tar")
            .args(["-xf", "-", "-C"])
            .arg(&dir)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&tar(&files)).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(std::fs::read(dir.join("a.txt")).unwrap(), b"hello");
        assert_eq!(std::fs::read(dir.join(&long)).unwrap(), b"long one");
        assert_eq!(std::fs::read(dir.join("empty")).unwrap(), b"");
        std::fs::remove_dir_all(dir).ok();
    }

    /// Drive a remote ForgeCLI through [`launch`] as the app does: the
    /// scripted model's MODETEST (Read, Write, Edit, mkdir, ls, rm, curl),
    /// allowing every ask. Returns what asked, and the folder's files after.
    fn modetest(target: &Target, mode: &str, model: &str, extra: &Extra) -> (Vec<String>, String) {
        use std::io::BufRead;
        let ready = provision(target, extra, &|_| {}).expect("provision");
        let folder = format!("~/mt-{mode}");
        run(
            target,
            r#"d="$HOME/${1#"~/"}"; rm -rf "$d"; mkdir -p "$d" && echo seed > "$d/seed.txt""#,
            std::slice::from_ref(&folder),
            b"",
            Duration::from_secs(30),
            extra,
        )
        .unwrap();
        let remote = Remote {
            target: target.clone(),
            path: folder.clone(),
        };
        let args: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-prompt-tool",
            "stdio",
            "--model",
            "mock-model",
            "--permission-mode",
            mode,
            "--plugin-dir",
            &ready.plugin,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let env = vec![
            ("FORGE_BASE_URL".to_string(), model.to_string()),
            (
                "FORGE_API_KEY".to_string(),
                "sk-test-not-in-argv".to_string(),
            ),
        ];
        let (mut cmd, prelude) = launch(&remote, &ready.forge, &args, &env, extra);
        let argv = format!("{:?}", cmd.get_args().collect::<Vec<_>>());
        assert!(
            !argv.contains("sk-test-not-in-argv"),
            "the key is not on the command line"
        );
        let mut child = cmd.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&prelude).unwrap();
        writeln!(
            stdin,
            r#"{{"type":"user","message":{{"role":"user","content":"MODETEST go"}}}}"#
        )
        .unwrap();
        stdin.flush().unwrap();
        let mut asks = vec![];
        for line in std::io::BufReader::new(child.stdout.take().unwrap()).lines() {
            let Ok(line) = line else { break };
            let Ok(m) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if m["type"] == "control_request" && m["request"]["subtype"] == "can_use_tool" {
                let r = &m["request"];
                let mut what = r["tool_name"].as_str().unwrap_or_default().to_string();
                if what == "Bash" {
                    what = format!(
                        "Bash:{}",
                        r["input"]["command"].as_str().unwrap_or_default()
                    );
                }
                asks.push(what);
                let answer = serde_json::json!({"type":"control_response","response":{"subtype":"success","request_id":m["request_id"],"response":{"behavior":"allow","updatedInput":r["input"]}}});
                writeln!(stdin, "{answer}").unwrap();
                stdin.flush().unwrap();
            }
            if m["type"] == "result" {
                break;
            }
        }
        drop(stdin);
        // stdin closed: the remote ForgeCLI ends, and so does the connection.
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "the remote process outlived its stdin"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let (_, entries) = list_dir(target, &folder, extra).unwrap();
        let files = entries
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>()
            .join(",");
        (asks, files)
    }

    /// The app's browser tools, served over HTTP here and forwarded with
    /// `ssh -R`, are offered to the remote ForgeCLI's agent.
    fn browser_reaches_the_remote(t: &Target, extra: &Extra, model: &str) {
        use std::io::BufRead;
        let port =
            crate::browser_mcp::serve_http("http://127.0.0.1:1", Path::new("/nonexistent"), "tok")
                .unwrap();
        let mut extra = extra.clone();
        extra.reverse.push(format!("{port}:127.0.0.1:{port}"));
        let ready = provision(t, &extra, &|_| {}).unwrap();
        let args: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-prompt-tool",
            "stdio",
            "--model",
            "mock-model",
            "--mcp-config",
            &browser_mcp_config(port, "tok"),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let env = vec![
            ("FORGE_BASE_URL".to_string(), model.to_string()),
            ("FORGE_API_KEY".to_string(), "t".to_string()),
        ];
        let remote = Remote {
            target: t.clone(),
            path: "~".into(),
        };
        let (mut cmd, prelude) = launch(&remote, &ready.forge, &args, &env, &extra);
        let mut child = cmd.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&prelude).unwrap();
        writeln!(
            stdin,
            r#"{{"type":"user","message":{{"role":"user","content":"hello"}}}}"#
        )
        .unwrap();
        stdin.flush().unwrap();
        let mut tools = vec![];
        for line in std::io::BufReader::new(child.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok)
        {
            let Ok(m) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if m["type"] == "system" && m["subtype"] == "init" {
                tools = m["tools"].as_array().cloned().unwrap_or_default();
                break;
            }
        }
        drop(stdin);
        let _ = child.kill();
        let _ = child.wait();
        if tools.is_empty() {
            let mut e = String::new();
            let _ = child.stderr.take().unwrap().read_to_string(&mut e);
            eprintln!("STDERR {e}");
        }
        let names: Vec<String> = tools
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        assert!(
            names.iter().any(|n| n == "mcp__forge-browser__navigate"),
            "{names:?}"
        );
        eprintln!(
            "{}: the browser's {} tools reach the remote agent",
            t.label(),
            names
                .iter()
                .filter(|n| n.starts_with("mcp__forge-browser__"))
                .count()
        );
    }

    /// Against real targets: `FORGE_TEST_SSH` (a Host whose key isn't known
    /// yet), `FORGE_TEST_CONTAINER` (a running container), with
    /// `FORGE_REMOTE_CLI` (a Linux ForgeCLI) and `FORGE_TEST_MODEL` (the
    /// scripted model, reachable at the same URL from the targets).
    #[test]
    #[ignore]
    fn real_remotes() {
        let model = std::env::var("FORGE_TEST_MODEL").expect("FORGE_TEST_MODEL");
        let mut targets = vec![];
        if let Ok(h) = std::env::var("FORGE_TEST_SSH") {
            let t = Target::Ssh(h.clone());
            // A first connection: the host key is unknown, and its fingerprint shown.
            match probe(&t, &Extra::default()) {
                Err(Error::UnknownHostKey { host, fingerprint }) => {
                    assert_eq!(host, h);
                    assert!(fingerprint.starts_with("SHA256:"), "{fingerprint}");
                }
                other => panic!("expected an unknown host key, got {other:?}"),
            }
            let mut extra = Extra {
                accept_new_host_key: true,
                reverse: vec![],
            };
            if let Some(port) = local_port(&model) {
                extra.reverse.push(format!("{port}:127.0.0.1:{port}"));
            }
            targets.push((t, extra));
        }
        if let Ok(c) = std::env::var("FORGE_TEST_CONTAINER") {
            targets.push((
                Target::Container {
                    name: c,
                    user: None,
                },
                Extra::default(),
            ));
        }
        assert!(!targets.is_empty());
        for (t, extra) in targets {
            let info = probe(&t, &extra).unwrap();
            assert_eq!(info.arch, "x86_64");
            // ForgeCLI goes up once; the second time it's already there.
            let said = std::sync::Mutex::new(vec![]);
            provision(&t, &extra, &|m| said.lock().unwrap().push(m.to_string())).unwrap();
            let again = std::sync::Mutex::new(vec![]);
            provision(&t, &extra, &|m| again.lock().unwrap().push(m.to_string())).unwrap();
            assert!(
                !again
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|m| m.contains("Installing")),
                "{t:?}"
            );
            let (manual, files) = modetest(&t, "default", &model, &extra);
            assert_eq!(
                manual,
                [
                    "Write",
                    "Edit",
                    "Bash:mkdir d",
                    "Bash:rm a.txt",
                    "Bash:curl -s http://127.0.0.1:1 || true"
                ],
                "{t:?}"
            );
            assert_eq!(files, "d,seed.txt", "{t:?}");
            let (auto, _) = modetest(&t, "acceptEdits", &model, &extra);
            assert_eq!(
                auto,
                ["Bash:rm a.txt", "Bash:curl -s http://127.0.0.1:1 || true"],
                "{t:?}"
            );
            let entries = project_files(&t, "~/mt-default", 100, &extra).unwrap();
            assert!(entries.contains(&"seed.txt".to_string()), "{entries:?}");
            if let Target::Ssh(_) = &t {
                browser_reaches_the_remote(&t, &extra, &model);
            }
            eprintln!(
                "{} ok: manual asked {:?}; edit automatically asked {:?}",
                t.label(),
                manual,
                auto
            );
        }
    }

    /// "Reopen in Container" for real, from a folder's devcontainer.json:
    /// once from an image, once from a Dockerfile with postCreateCommand.
    /// Needs `FORGE_TEST_IMAGE` (a local image with sh) and the variables
    /// [`real_remotes`] uses.
    #[test]
    #[ignore]
    fn real_devcontainer() {
        let image = std::env::var("FORGE_TEST_IMAGE").expect("FORGE_TEST_IMAGE");
        let model = std::env::var("FORGE_TEST_MODEL").expect("FORGE_TEST_MODEL");
        for (case, json, dockerfile) in [
            (
                "image",
                format!(
                    r#"{{ "image": "{image}", "workspaceFolder": "/workspaces/demo", // jsonc
                "runArgs": ["--network", "host"], }}"#
                ),
                None,
            ),
            (
                "build",
                r#"{ "build": { "dockerfile": "Dockerfile" }, "runArgs": ["--network=host"],
                "postCreateCommand": "echo made > /tmp/post-create" }"#
                    .to_string(),
                Some(format!("FROM {image}\n")),
            ),
        ] {
            let dir = std::env::temp_dir().join(format!("forge-dc-{case}-{}", std::process::id()));
            std::fs::create_dir_all(dir.join(".devcontainer")).unwrap();
            std::fs::write(dir.join(".devcontainer/devcontainer.json"), json).unwrap();
            if let Some(df) = dockerfile {
                std::fs::write(dir.join(".devcontainer/Dockerfile"), df).unwrap();
            }
            std::fs::write(dir.join("hello.txt"), "from the host\n").unwrap();
            let said = std::sync::Mutex::new(vec![]);
            let remote =
                devcontainer_up(&dir, &|m| said.lock().unwrap().push(m.to_string())).unwrap();
            assert_eq!(
                remote.path,
                if case == "image" {
                    "/workspaces/demo".to_string()
                } else {
                    format!("/workspaces/{}", dir.file_name().unwrap().to_string_lossy())
                }
            );
            // The folder is mounted: the host's file is there.
            let (_, entries) = list_dir(&remote.target, &remote.path, &Extra::default()).unwrap();
            assert!(entries.iter().any(|e| e.name == "hello.txt"), "{entries:?}");
            if case == "build" {
                assert!(said.lock().unwrap().iter().any(|m| m.contains("Building")));
                let out = run(
                    &remote.target,
                    "cat /tmp/post-create",
                    &[],
                    b"",
                    Duration::from_secs(10),
                    &Extra::default(),
                )
                .unwrap();
                assert_eq!(String::from_utf8_lossy(&out).trim(), "made");
            }
            // Reopening finds the same container.
            let again = devcontainer_up(&dir, &|_| {}).unwrap();
            assert_eq!(again, remote);
            let (asks, _) = modetest(&remote.target, "acceptEdits", &model, &Extra::default());
            assert_eq!(
                asks,
                ["Bash:rm a.txt", "Bash:curl -s http://127.0.0.1:1 || true"]
            );
            if let Target::Container { name, .. } = &remote.target {
                let _ = docker(&["rm", "-f", name]);
            }
            std::fs::remove_dir_all(&dir).ok();
            eprintln!("dev container ({case}) ok: {}", remote.target.label());
        }
    }

    #[test]
    fn local_endpoints_are_forwarded() {
        assert_eq!(local_port("http://127.0.0.1:4650"), Some(4650));
        assert_eq!(local_port("http://localhost:11434/v1"), Some(11434));
        assert_eq!(local_port("http://[::1]:8080"), Some(8080));
        assert_eq!(local_port("http://localhost"), Some(80));
        assert_eq!(local_port("https://api.anthropic.com"), None);
        assert_eq!(local_port("http://10.0.0.2:8000"), None);
    }

    #[test]
    fn each_session_forwards_on_its_own_port() {
        let a = forward_port("session-a", 4650);
        let b = forward_port("session-b", 4650);
        assert_ne!(a, b);
        assert!((30_000..60_000).contains(&a));
        assert_eq!(a, forward_port("session-a", 4650), "stable for a session");
        assert_eq!(
            with_port("http://127.0.0.1:4650", 31000),
            "http://127.0.0.1:31000"
        );
        assert_eq!(
            with_port("http://localhost:11434/v1", 40000),
            "http://localhost:40000/v1"
        );
        assert_eq!(
            with_port("http://localhost/v1", 40000),
            "http://localhost:40000/v1"
        );
    }

    #[test]
    fn listings_put_folders_first() {
        let (full, e) = parse_listing("/home/me\nb.txt\nsrc/\n.git/\nA.md\n");
        assert_eq!(full, "/home/me");
        let names: Vec<_> = e.iter().map(|e| (e.name.as_str(), e.dir)).collect();
        assert_eq!(
            names,
            [
                (".git", true),
                ("src", true),
                ("A.md", false),
                ("b.txt", false)
            ]
        );
    }
}
