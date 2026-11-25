//! Remote development, as VS Code does it: a chat's KodAct CLI runs inside a
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
//! [`BOOT`], a POSIX `sh` script, starts KodAct CLI on the remote: it reads the
//! launch's environment from the first lines of stdin (keys never go on a
//! command line, where `ps` would show them), expands `~/` in the arguments,
//! changes to the project folder and `exec`s KodAct CLI on the same stdin.

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
    /// A running container, by name, and the user to run as; `wsl`: the
    /// distro whose Docker runs it (a dev container started from a folder
    /// in WSL), else this computer's `docker`.
    Container {
        name: String,
        user: Option<String>,
        wsl: Option<String>,
    },
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
            Target::Container { name, user, wsl } => {
                let mut a = match user {
                    Some(u) => format!("{u}@{name}"),
                    None => name.clone(),
                };
                if let Some(d) = wsl {
                    a.push_str(WSL_HOST);
                    a.push_str(d);
                }
                a
            }
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

/// In a container's project path, what names the WSL distro whose Docker
/// runs it: `container://vscode@forge-dc-x;wsl=Ubuntu/workspaces/x`.
const WSL_HOST: &str = ";wsl=";

/// The `docker` that reaches containers on `wsl` (a distro's own, through
/// `wsl.exe`), or this computer's.
pub fn docker_command(wsl: Option<&str>) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut c = match wsl {
        Some(d) => {
            let mut c = Command::new("wsl.exe");
            c.args(["-d", d, "--exec", "docker"]);
            c
        }
        None => Command::new("docker"),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

/// The folder name in a dev container KodAct started (`forge-dc-<slug>-<hash>`).
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
            "container" => {
                let (authority, wsl) = match authority.split_once(WSL_HOST) {
                    Some((a, d)) => (a, Some(d.to_string())),
                    None => (authority, None),
                };
                match authority.split_once('@') {
                    Some((u, n)) => Target::Container {
                        name: n.into(),
                        user: Some(u.into()),
                        wsl,
                    },
                    None => Target::Container {
                        name: authority.into(),
                        user: None,
                        wsl,
                    },
                }
            }
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
            c.args(crate::app_settings::get().ssh_args());
            // Passwords and passphrases are asked in the app when it can
            // (once, then remembered for this run); else keys only.
            match crate::askpass::env() {
                Some(env) => {
                    c.args(["-o", "BatchMode=no", "-o", "NumberOfPasswordPrompts=1"]);
                    c.envs(env);
                }
                None => {
                    c.args(["-o", "BatchMode=yes"]);
                }
            }
            c.args([
                "-T",
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
        Target::Container { name, user, wsl } => {
            let mut c = docker_command(wsl.as_deref());
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

/// A file the boot script writes before KodAct CLI starts: `HOST_FILE` names
/// it, `HOST_TEXT` holds it. A design chat's prompt goes this way: too big
/// for a command line, and too big for the environment (Linux caps one
/// string at 128 KB, and a big one stops every program starting), so the
/// script keeps the text in a shell variable it never exports.
pub const HOST_FILE: &str = "KODACT_HOST_FILE";
pub const HOST_TEXT: &str = "KODACT_HOST_TEXT";

/// Starts KodAct CLI on the remote: `$1` the folder, `$2` KodAct CLI, the rest
/// its arguments; the environment comes first on stdin, one `KEY=value` a
/// line (backslash escapes, see [`env_line`]), ended by an empty line.
pub fn boot_script() -> String {
    format!(
        r#"{TILDE}
d=$(x "$1"); f=$(x "$2"); shift 2
n=$#
while [ "$n" -gt 0 ]; do a=$(x "$1"); shift; set -- "$@" "$a"; n=$((n-1)); done
t=
unset FORGE_NUDGES
while IFS= read -r l && [ -n "$l" ]; do
  case $l in
    {HOST_TEXT}=*) t=$(printf '%b' "${{l#{HOST_TEXT}=}}");;
    *) export "$(printf '%b' "$l")";;
  esac
done
if [ -n "${{{HOST_FILE}:-}}" ]; then
  h=$(x "${HOST_FILE}"); mkdir -p "${{h%/*}}" && printf '%s' "$t" > "$h"
  unset {HOST_FILE}
fi
unset t
cd "$d" 2>/dev/null || {{ echo "kodact: there is no folder $d on $(uname -n)" >&2; exit 97; }}
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

/// How to start KodAct CLI in `remote`'s folder: the command, and the bytes to
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
    run_once(target, script, args, input, timeout, extra, true)
}

/// A dev container that stopped (Docker or WSL restarted, the machine slept)
/// is started again, and one deleted since is built again from its folder's
/// devcontainer.json; then the script is retried once. VS Code's Dev
/// Containers does the same when it reconnects.
fn bring_back(target: &Target, stderr: &str) -> bool {
    let Target::Container { name, wsl, .. } = target else {
        return false;
    };
    if stderr.contains("is not running") {
        let started = docker(wsl.as_deref(), &["start", name]).is_ok();
        // Made before KodAct kept its folder: keep it now, from its label.
        if started && is_devcontainer(target) && remembered_source(name).is_none() {
            if let Ok(src) = labelled_source(name, wsl.as_deref()) {
                remember_source(name, &src);
            }
        }
        return started;
    }
    if stderr.contains("No such container") && is_devcontainer(target) {
        return remembered_source(name)
            .is_some_and(|src| devcontainer_up(&src, false, &|_| {}).is_ok());
    }
    false
}

fn run_once(
    target: &Target,
    script: &str,
    args: &[String],
    input: &[u8],
    timeout: Duration,
    extra: &Extra,
    may_start: bool,
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
    let mut start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            // A password being typed doesn't count against the timeout.
            Ok(None) if crate::askpass::waiting() => {
                start = Instant::now();
                std::thread::sleep(Duration::from_millis(50));
            }
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
    } else if may_start && bring_back(target, &stderr) {
        run_once(target, script, args, input, timeout, extra, false)
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
                } else if has("Permission denied") || has("Too many authentication failures") {
                    // A wrong answer is among the remembered ones: ask again next time.
                    crate::askpass::forget();
                    if crate::askpass::take_cancelled() {
                        format!("Signing in to {host} was cancelled.")
                    } else if crate::askpass::env().is_some() {
                        format!("{host} refused the sign-in: the password or passphrase was wrong, or no key it accepts was offered. Connect again to retry.")
                    } else {
                        format!("{host} refused the sign-in. KodAct signs in with your SSH keys or agent: check `ssh {host}` works in a terminal.")
                    }
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
                } else if has("No such container") && name.starts_with("forge-dc-") {
                    format!("The dev container {name} was deleted. Open the folder again (Reopen in container) and KodAct builds it anew.")
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
        .args(crate::app_settings::get().ssh_args())
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

/// What a target is, from `uname`: only Linux runs KodAct CLI remotely.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub arch: String,
    pub home: String,
}

/// Connect and ask what the target is. Fails with the reason if it can't.
pub fn probe(target: &Target, extra: &Extra) -> Result<Info, Error> {
    probe_within(target, extra, Duration::from_secs(30))
}

/// A failure that a target still waking up gives: no answer in time, WSL's
/// own errors while its VM boots, Docker's daemon not up yet. Not a missing
/// distro, a refused sign-in or a deleted container: those stay failures.
fn waking(target: &Target, e: &Error) -> bool {
    match e {
        Error::Timeout(_) => true,
        Error::UnknownHostKey { .. } => false,
        Error::Other(m) => match target {
            Target::Wsl(_) => {
                !(m.contains("There's no WSL distro")
                    || m.contains("isn't installed")
                    || m.contains("KodAct runs remotely"))
            }
            Target::Container { .. } => m.contains("Docker isn't running") || m.contains("failed"),
            Target::Ssh(_) => false,
        },
    }
}

/// [`probe`] for a first connect: a target that was asleep (WSL's VM
/// stopped, Docker still starting) can fail or hang the first time while it
/// boots, and answer a moment later. It gets a minute and a half, then two
/// more tries, as a second click would, saying so meanwhile.
pub fn probe_waking(
    target: &Target,
    extra: &Extra,
    progress: &dyn Fn(&str),
) -> Result<Info, Error> {
    let limits = [90, 45, 45].map(Duration::from_secs);
    let mut last = None;
    for (i, limit) in limits.into_iter().enumerate() {
        if i > 0 {
            progress(&format!("Waiting for {} to start…", target.label()));
            std::thread::sleep(Duration::from_secs(2 * i as u64));
        }
        match probe_within(target, extra, limit) {
            Ok(info) => return Ok(info),
            Err(e) if waking(target, &e) => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| Error::Timeout(target.label())))
}

fn probe_within(target: &Target, extra: &Extra, limit: Duration) -> Result<Info, Error> {
    let out = run(
        target,
        r#"uname -sm; printf '%s\n' "$HOME""#,
        &[],
        b"",
        limit,
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
            "{} runs {os}; KodAct runs remotely on Linux only.",
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
/// Which of `paths` (from `dir`, or absolute) are files that changed at or
/// after `since` (seconds since the epoch), each as its absolute path there.
pub fn changed_since(
    target: &Target,
    dir: &str,
    paths: &[String],
    since: u64,
    extra: &Extra,
) -> Vec<String> {
    let script = format!(
        r#"{TILDE}
cd "$(x "$1")" 2>/dev/null || exit 3; shift
for f; do f=$(x "$f"); [ -f "$f" ] && printf '%s\t%s/%s\n' "$(stat -c %Y -- "$f")" "$(cd "$(dirname -- "$f")" && pwd)" "$(basename -- "$f")"; done; true"#
    );
    let mut args = vec![dir.to_string()];
    args.extend(paths.iter().cloned());
    let out = run(target, &script, &args, b"", Duration::from_secs(20), extra).unwrap_or_default();
    String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| {
            let (t, full) = l.split_once('\t')?;
            (t.parse::<u64>().ok()? >= since).then(|| full.to_string())
        })
        .collect()
}

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

/// Which of `files` are there, each relative to `dir` (or absolute, or from
/// `~`), checked in one go: a remote page's linked files, for `design_lint`.
pub fn exist(target: &Target, dir: &str, files: &[String], extra: &Extra) -> Result<Vec<bool>, Error> {
    let script = format!(
        r#"{TILDE}
cd "$(x "$1")" 2>/dev/null || exit 3
shift
for f; do if [ -e "$(x "$f")" ]; then echo 1; else echo 0; fi; done"#
    );
    let mut args = vec![dir.to_string()];
    args.extend(files.iter().cloned());
    let out = run(target, &script, &args, b"", Duration::from_secs(30), extra)?;
    Ok(String::from_utf8_lossy(&out).lines().map(|l| l == "1").collect())
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

/// Put `files` (paths inside the folder, and their bytes) on the remote as the
/// folder `dest`, whole: unpacked beside it first, then swapped in. With
/// `replace` false, a folder already there is left alone and that is an error.
pub fn put_dir(
    target: &Target,
    dest: &str,
    files: &[(String, Vec<u8>)],
    replace: bool,
    extra: &Extra,
) -> Result<(), Error> {
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let mode = if replace { "replace" } else { "keep" };
    run(
        target,
        &put_dir_script(),
        &[dest.to_string(), mode.to_string()],
        &tar(&refs),
        Duration::from_secs(300),
        extra,
    )
    .map(|_| ())
}

/// [`put_dir`]'s script: `$1` the folder, `$2` `keep` or `replace`, the tar on stdin.
fn put_dir_script() -> String {
    format!(
        r#"{TILDE}
d=$(x "$1"); t="$d.forge-tmp"
if [ "$2" = keep ] && [ -e "$d" ]; then echo "$1 is already there" >&2; exit 3; fi
mkdir -p "$(dirname "$d")" && rm -rf "$t" && mkdir -p "$t" && tar -xf - -C "$t" && rm -rf "$d" && mv "$t" "$d""#
    )
}

/// The skills KodAct CLI on the remote loads, from its folders there (Claude
/// Code's `~/.claude/skills`, yours in `$KODACT_HOME` or `~/.config/kodact`
/// (ForgeCLI's `forge` while only that exists), then the project's
/// `.claude/skills` from its repository root down, `.agents`, ForgeCLI's
/// `.forge` and `.kodact`): each skill's folder name and its SKILL.md frontmatter.
pub fn skill_heads(
    target: &Target,
    path: &str,
    extra: &Extra,
) -> Result<Vec<(String, String)>, Error> {
    let script = format!(
        r#"{TILDE}
d=$(x "$1")
u=${{KODACT_HOME:-${{FORGE_HOME:-}}}}
if [ -z "$u" ]; then
  b=${{XDG_CONFIG_HOME:-$HOME/.config}}
  if [ ! -e "$b/kodact" ] && [ -e "$b/forge" ]; then u=$b/forge; else u=$b/kodact; fi
fi
c=${{CLAUDE_CONFIG_DIR:-$HOME/.claude}}
chain=""; p=$d; found=""
while [ -n "$p" ]; do
  chain="$p/.claude/skills
$chain"
  if [ -e "$p/.git" ]; then found=1; break; fi
  [ "$p" = / ] && break
  p=$(dirname "$p")
done
[ -n "$found" ] || chain="$d/.claude/skills"
IFS='
'
for r in "$c/skills" "$u/skills" $chain "$d/.agents/skills" "$d/.forge/skills" "$d/.kodact/skills"; do
  for f in "$r"/*/SKILL.md; do
    [ -f "$f" ] || continue
    printf '\036%s\n' "$(basename "$(dirname "$f")")"
    head -n 80 "$f" | awk 'NR==1 && !/^---/ {{exit}} {{print}} NR>1 && /^---/ {{exit}}'
  done
done
true"#
    );
    let out = run(
        target,
        &script,
        &[path.to_string()],
        b"",
        Duration::from_secs(30),
        extra,
    )?;
    Ok(parse_skill_heads(&String::from_utf8_lossy(&out)))
}

/// [`skill_heads`]' output: a record separator, the folder, then its frontmatter.
fn parse_skill_heads(out: &str) -> Vec<(String, String)> {
    out.split('\u{1e}')
        .filter_map(|rec| {
            let (folder, head) = rec.split_once('\n')?;
            (!folder.is_empty()).then(|| (folder.to_string(), head.to_string()))
        })
        .collect()
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
    let file = crate::app_settings::get()
        .ssh_config_file
        .map(|f| PathBuf::from(crate::app_settings::expand_home(&f)))
        .unwrap_or_else(|| ssh.join("config"));
    let text = std::fs::read_to_string(file).unwrap_or_default();
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
    let mut c = docker_command(None);
    c.args(["ps", "--format", "{{.Names}}\t{{.Image}}\t{{.Status}}"]);
    let target = Target::Container {
        name: String::new(),
        user: None,
        wsl: None,
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

/// What KodAct uses from a `devcontainer.json`.
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

fn docker(wsl: Option<&str>, args: &[&str]) -> Result<String, Error> {
    let target = Target::Container {
        name: String::new(),
        user: None,
        wsl: wsl.map(str::to_string),
    };
    let mut c = docker_command(wsl);
    c.args(args);
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

/// Wait until Docker's daemon answers (up to a minute and a half): WSL's VM
/// boots on the first call, and its dockerd (or Docker Desktop's) starts
/// after. Any other failure is said at once.
fn wait_for_docker(wsl: Option<&str>, progress: &dyn Fn(&str)) -> Result<(), Error> {
    let start = Instant::now();
    let mut said = false;
    loop {
        match docker(wsl, &["version", "--format", "{{.Server.Version}}"]) {
            Ok(_) => return Ok(()),
            Err(e)
                if start.elapsed() < Duration::from_secs(90)
                    && waking(
                        &Target::Container {
                            name: String::new(),
                            user: None,
                            wsl: wsl.map(str::to_string),
                        },
                        &e,
                    ) =>
            {
                if !said {
                    progress(match wsl {
                        Some(_) => "Waiting for WSL and Docker to start…",
                        None => "Waiting for Docker to start…",
                    });
                    said = true;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Reopen a local folder in its dev container: build or pull the image,
/// start (or reuse) a container with the folder mounted, run
/// `postCreateCommand` once, and return the remote project.
/// A container KodAct started from a folder's devcontainer.json.
pub fn is_devcontainer(target: &Target) -> bool {
    matches!(target, Target::Container { name, .. } if name.starts_with("forge-dc-"))
}

/// Where a dev container's folder is: on this computer, or in a WSL distro
/// (whose Docker then runs the container).
#[derive(Clone, Debug, PartialEq)]
pub enum DevSource {
    Local(PathBuf),
    Wsl { distro: String, path: String },
}

impl DevSource {
    fn wsl(&self) -> Option<&str> {
        match self {
            DevSource::Wsl { distro, .. } => Some(distro),
            DevSource::Local(_) => None,
        }
    }

    /// As its `forge.devcontainer` label keeps it: the local path, or the
    /// WSL folder's project path (`wsl://Ubuntu/home/me/x`).
    fn label(&self) -> String {
        match self {
            DevSource::Local(p) => p.to_string_lossy().into_owned(),
            DevSource::Wsl { distro, path } => Remote {
                target: Target::Wsl(distro.clone()),
                path: path.clone(),
            }
            .to_path()
            .to_string_lossy()
            .replace('\\', "/"),
        }
    }

    fn from_label(label: &str) -> DevSource {
        match Remote::parse(Path::new(label)) {
            Some(Remote {
                target: Target::Wsl(distro),
                path,
            }) => DevSource::Wsl { distro, path },
            _ => DevSource::Local(PathBuf::from(label)),
        }
    }

    /// The folder's last name.
    fn base(&self) -> String {
        let s = match self {
            DevSource::Local(p) => p.to_string_lossy().replace('\\', "/"),
            DevSource::Wsl { path, .. } => path.clone(),
        };
        s.trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|n| !n.is_empty() && *n != "~")
            .unwrap_or("workspace")
            .to_string()
    }
}

/// Where KodAct keeps each dev container's folder (`devcontainers.json`,
/// name → its `forge.devcontainer` label), so a container deleted since can
/// be built again: its own label went with it.
fn sources_file() -> Option<PathBuf> {
    crate::profile::dir().map(|d| d.join("devcontainers.json"))
}

fn remember_source(name: &str, src: &DevSource) {
    let Some(file) = sources_file() else {
        return;
    };
    let mut all: std::collections::BTreeMap<String, String> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if all.get(name) != Some(&src.label()) {
        all.insert(name.to_string(), src.label());
        if let Some(d) = file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(t) = serde_json::to_string_pretty(&all) {
            let _ = std::fs::write(&file, t + "\n");
        }
    }
}

fn remembered_source(name: &str) -> Option<DevSource> {
    let t = std::fs::read_to_string(sources_file()?).ok()?;
    let all: std::collections::BTreeMap<String, String> = serde_json::from_str(&t).ok()?;
    all.get(name).map(|l| DevSource::from_label(l))
}

/// The folder a dev container was started from (its `forge.devcontainer`
/// label), on the Docker of `wsl` or this computer; for one deleted since,
/// the folder KodAct remembered when it made it.
pub fn devcontainer_source(name: &str, wsl: Option<&str>) -> Result<DevSource, Error> {
    match labelled_source(name, wsl) {
        Ok(src) => Ok(src),
        Err(e) => remembered_source(name).ok_or(e),
    }
}

fn labelled_source(name: &str, wsl: Option<&str>) -> Result<DevSource, Error> {
    let dir = docker(
        wsl,
        &[
            "inspect",
            "--format",
            "{{index .Config.Labels \"forge.devcontainer\"}}",
            name,
        ],
    )?;
    let dir = dir.trim();
    if dir.is_empty() || dir == "<no value>" {
        return Err(Error::Other(format!(
            "{name} doesn't say which folder it came from."
        )));
    }
    Ok(DevSource::from_label(dir))
}

/// A WSL folder's dev container definition: (its file's folder, the
/// definition), the path made absolute (`~` resolved).
fn find_wsl_devcontainer(
    distro: &str,
    path: &str,
) -> Result<(String, String, DevContainer), Error> {
    let target = Target::Wsl(distro.to_string());
    let script = format!(
        r#"{TILDE}
d=$(x "$1"); cd "$d" || exit 3; pwd
for f in .devcontainer/devcontainer.json .devcontainer.json; do
  [ -f "$f" ] && {{ echo "$f"; cat "$f"; exit 0; }}
done
exit 4"#
    );
    let out = run(
        &target,
        &script,
        &[path.to_string()],
        b"",
        Duration::from_secs(30),
        &Extra::default(),
    )
    .map_err(|_| Error::Other("This folder has no .devcontainer/devcontainer.json.".into()))?;
    let text = String::from_utf8_lossy(&out);
    let mut lines = text.splitn(3, '\n');
    let (dir, file, json) = (
        lines.next().unwrap_or_default().trim().to_string(),
        lines.next().unwrap_or_default().trim().to_string(),
        lines.next().unwrap_or_default(),
    );
    let dc = parse_devcontainer(json).map_err(Error::Other)?;
    let folder = match file.rsplit_once('/') {
        Some((f, _)) => format!("{dir}/{f}"),
        None => dir.clone(),
    };
    Ok((dir, folder, dc))
}

/// Whether a WSL folder has a dev container definition (blocking).
pub fn wsl_has_devcontainer(distro: &str, path: &str) -> bool {
    find_wsl_devcontainer(distro, path).is_ok()
}

/// Start (or reuse) the dev container for a folder, on this computer or in
/// WSL (its Docker builds and runs it, the folder mounted from there).
/// `fresh` rebuilds it: the old container goes, a Dockerfile is built again
/// without cache, and the image is pulled again; postCreateCommand runs again.
pub fn devcontainer_up(
    src: &DevSource,
    fresh: bool,
    progress: &dyn Fn(&str),
) -> Result<Remote, Error> {
    // The folder (absolute), the definition's folder, and the definition.
    // Docker first: a WSL distro just booted, or Docker Desktop just
    // started, has no daemon for a while, and the first build failed on it.
    wait_for_docker(src.wsl().as_deref(), progress)?;
    let (dir, folder, dc) = match src {
        DevSource::Local(dir) => {
            let (file, dc) = find_devcontainer(dir).ok_or_else(|| {
                Error::Other("This folder has no .devcontainer/devcontainer.json.".into())
            })?;
            let folder = file.parent().unwrap_or(dir).to_string_lossy().into_owned();
            (dir.to_string_lossy().into_owned(), folder, dc)
        }
        DevSource::Wsl { distro, path } => {
            progress("Reading devcontainer.json in WSL…");
            find_wsl_devcontainer(distro, path)?
        }
    };
    let wsl = src.wsl();
    let join = |a: &str, b: &str| match src {
        DevSource::Local(_) => Path::new(a).join(b).to_string_lossy().into_owned(),
        DevSource::Wsl { .. } => {
            format!("{}/{}", a.trim_end_matches('/'), b.trim_start_matches("./"))
        }
    };
    let base = src.base();
    let mut h = DefaultHasher::new();
    src.label().hash(&mut h);
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
    remember_source(&name, src);
    let workspace = dc
        .workspace_folder
        .clone()
        .unwrap_or_else(|| format!("/workspaces/{base}"));
    let target = Target::Container {
        name: name.clone(),
        user: dc.remote_user.clone(),
        wsl: wsl.map(str::to_string),
    };
    let remote = Remote {
        target,
        path: workspace.clone(),
    };

    if fresh {
        progress("Removing the old container…");
        let _ = docker(wsl, &["rm", "-f", &name]);
    }
    // Already there: start it if stopped.
    let existing = docker(
        wsl,
        &[
            "ps",
            "-a",
            "--filter",
            &format!("name=^{name}$"),
            "--format",
            "{{.State}}",
        ],
    )?;
    if !existing.is_empty() {
        if existing != "running" {
            progress("Starting the dev container…");
            docker(wsl, &["start", &name])?;
        }
        return Ok(remote);
    }

    let image = match (&dc.image, &dc.dockerfile) {
        (Some(i), _) => {
            progress(&format!("Pulling {i}…"));
            // A local image needs no pull; a failed pull shows when it runs.
            let _ = docker(wsl, &["pull", i]);
            i.clone()
        }
        (None, Some(df)) => {
            let ctx = join(&folder, dc.context.as_deref().unwrap_or("."));
            let tag = format!("forge-dc-{slug}-{key}:latest");
            progress("Building the dev container image…");
            let file = join(&folder, df);
            let mut args = vec!["build", "-t", &tag, "-f", &file];
            if fresh {
                args.push("--no-cache");
            }
            args.push(&ctx);
            docker(wsl, &args)?;
            tag
        }
        (None, None) => {
            return Err(Error::Other(
                "devcontainer.json names neither an image nor a Dockerfile (Compose isn't supported yet).".into(),
            ))
        }
    };
    progress("Starting the dev container…");
    let mount = format!("{dir}:{workspace}");
    let label = format!("forge.devcontainer={}", src.label());
    let mut args: Vec<String> = [
        "run", "-d", "--name", &name, "--label", &label, "-v", &mount, "-w", &workspace,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(dc.run_args.iter().cloned());
    args.extend([image, "sleep".into(), "infinity".into()]);
    docker(wsl, &args.iter().map(String::as_str).collect::<Vec<_>>())?;
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

// ---------------------------------------------------------------- KodAct CLI on the remote

/// Where a target's KodAct CLI and KodAct's plugin were put.
#[derive(Clone, Debug, PartialEq)]
pub struct Provisioned {
    pub forge: String,
    pub plugin: String,
}

/// The Linux KodAct CLI this app carries for an architecture (`uname -m`):
/// `KODACT_REMOTE_CLI`, else `bin/linux-<arch>/kodact` beside the app.
pub fn local_forge(arch: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KODACT_REMOTE_CLI").map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        a => a,
    };
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [
        dir.join("bin").join(format!("linux-{arch}")).join("kodact"),
        dir.join(format!("linux-{arch}")).join("kodact"),
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

/// Make sure the target has this app's KodAct CLI and plugin, uploading what's
/// missing (once per version), and say where they are.
pub fn provision(
    target: &Target,
    extra: &Extra,
    progress: &dyn Fn(&str),
) -> Result<Provisioned, Error> {
    progress(&format!("Connecting to {}…", target.label()));
    let info = probe_waking(target, extra, progress)?;
    let local = local_forge(&info.arch).ok_or_else(|| {
        Error::Other(format!(
            "This KodAct has no KodAct CLI for Linux {} to put on {}.",
            info.arch,
            target.label()
        ))
    })?;
    let bin = std::fs::read(&local).map_err(|e| Error::Other(e.to_string()))?;
    let base = crate::app_settings::get().server_install_dir;
    let forge = format!("{base}/bin/kodact-{}", content_id(&bin));
    let plugin_files = crate::plugin::files();
    let plugin = format!("{base}/plugin-{}/kodact", crate::plugin::hash());
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
        progress(&format!("Installing KodAct CLI on {}…", target.label()));
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
        progress(&format!("Checking KodAct CLI on {}…", target.label()));
        verify_upload(target, &forge, &bin, extra)?;
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

/// The uploaded file matches what was sent: its SHA-256 there (or, on a
/// system without `sha256sum`/`shasum`, its size). A mismatch removes it, so
/// the next connect uploads it again.
fn verify_upload(target: &Target, file: &str, bytes: &[u8], extra: &Extra) -> Result<(), Error> {
    let script = format!(
        r#"{TILDE}
f=$(x "$1")
if command -v sha256sum >/dev/null 2>&1; then echo "sha256 $(sha256sum "$f" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then echo "sha256 $(shasum -a 256 "$f" | cut -d' ' -f1)"
else echo "size $(wc -c < "$f" | tr -d ' ')"; fi"#
    );
    let out = run(
        target,
        &script,
        &[file.to_string()],
        b"",
        Duration::from_secs(120),
        extra,
    )?;
    let out = String::from_utf8_lossy(&out);
    if upload_matches(out.trim(), bytes) {
        return Ok(());
    }
    let _ = run(
        target,
        &format!("{TILDE}\nrm -f \"$(x \"$1\")\""),
        &[file.to_string()],
        b"",
        Duration::from_secs(30),
        extra,
    );
    Err(Error::Other(format!(
        "KodAct CLI arrived damaged on {}. It was removed; connect again to upload it once more.",
        target.label()
    )))
}

/// Whether the remote's report (`sha256 <hex>` or `size <n>`) fits `bytes`.
fn upload_matches(report: &str, bytes: &[u8]) -> bool {
    match report.split_once(' ') {
        Some(("sha256", hex)) => {
            let d = ring::digest::digest(&ring::digest::SHA256, bytes);
            let want: String = d.as_ref().iter().map(|b| format!("{b:02x}")).collect();
            hex.trim().eq_ignore_ascii_case(&want)
        }
        Some(("size", n)) => n.trim().parse::<usize>().ok() == Some(bytes.len()),
        _ => false,
    }
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
#[cfg(test)]
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
#[cfg(test)]
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

/// The browser's tool server as a remote KodAct CLI reaches it: over HTTP on
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
    #[test]
    fn a_folder_goes_up_whole_and_stays_unless_replaced() {
        use std::io::Write;
        let home = std::env::temp_dir().join(format!("forge-put-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let put = |files: &[(&str, &[u8])], mode: &str| {
            let mut child = std::process::Command::new("sh")
                .args([
                    "-c",
                    &super::put_dir_script(),
                    "_",
                    "~/.claude/skills/pdf",
                    mode,
                ])
                .env("HOME", &home)
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            // "keep" may leave without reading the archive: a broken pipe
            // then is the script's answer, not a failure.
            if let Err(e) = child.stdin.take().unwrap().write_all(&super::tar(files)) {
                assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
            }
            child.wait_with_output().unwrap()
        };
        let dir = home.join(".claude/skills/pdf");
        assert!(put(
            &[("SKILL.md", b"v1"), ("scripts/x.py", b"print(1)")],
            "keep"
        )
        .status
        .success());
        assert_eq!(
            std::fs::read(dir.join("scripts/x.py")).unwrap(),
            b"print(1)"
        );
        let again = put(&[("SKILL.md", b"v2")], "keep");
        assert!(
            !again.status.success()
                && String::from_utf8_lossy(&again.stderr).contains("already there")
        );
        assert!(put(&[("SKILL.md", b"v2")], "replace").status.success());
        assert_eq!(std::fs::read(dir.join("SKILL.md")).unwrap(), b"v2");
        assert!(!dir.join("scripts").exists(), "replaced whole");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_remote_s_skills_are_read_from_their_frontmatter() {
        let out = "\u{1e}tls\n---\nname: tls\ndescription: TLS\n---\n\u{1e}empty\n";
        let heads = super::parse_skill_heads(out);
        assert_eq!(
            heads[0],
            (
                "tls".to_string(),
                "---\nname: tls\ndescription: TLS\n---\n".to_string()
            )
        );
        assert_eq!(heads[1], ("empty".to_string(), String::new()));
        assert!(super::parse_skill_heads("").is_empty());
    }

    use super::*;

    #[test]
    fn a_target_waking_up_is_tried_again_and_a_real_failure_is_not() {
        let wsl = Target::Wsl("Fedora".into());
        let docker = Target::Container {
            name: "x".into(),
            user: None,
            wsl: None,
        };
        let ssh = Target::Ssh("box".into());
        assert!(waking(&wsl, &Error::Timeout("WSL: Fedora".into())));
        assert!(waking(
            &wsl,
            &Error::Other("WSL: Fedora: Wsl/Service/CreateInstance/E_FAIL".into())
        ));
        assert!(!waking(
            &wsl,
            &Error::Other("There's no WSL distro named Fedora.".into())
        ));
        assert!(waking(
            &docker,
            &Error::Other(
                "Docker isn't running. Start Docker Desktop (or the docker service).".into()
            )
        ));
        assert!(!waking(
            &docker,
            &Error::Other("There's no container named x.".into())
        ));
        assert!(!waking(
            &ssh,
            &Error::Other("box refused the connection: is its SSH server running?".into())
        ));
    }

    // A dev container that stopped, or was deleted, comes back as VS Code's
    // does. These need a Docker daemon and alpine:3.20, so they're ignored:
    // run with `cargo test comes_back -- --ignored --test-threads=1`.
    fn container(name: &str) -> Target {
        Target::Container {
            name: name.into(),
            user: None,
            wsl: None,
        }
    }

    #[test]
    #[ignore]
    fn documents_made_in_a_container_are_found_by_when_they_changed() {
        let name = "forge-test-docs";
        let _ = docker(None, &["rm", "-f", name]);
        docker(
            None,
            &[
                "run",
                "-d",
                "--name",
                name,
                "alpine:3.20",
                "sleep",
                "infinity",
            ],
        )
        .unwrap();
        let t = container(name);
        let x = Extra::default();
        run(
            &t,
            "mkdir -p /w/out && touch -d @1000 /w/old.pdf && touch /w/out/new.docx",
            &[],
            b"",
            Duration::from_secs(20),
            &x,
        )
        .unwrap();
        let paths = ["old.pdf", "out/new.docx", "missing.xlsx"]
            .map(String::from)
            .to_vec();
        let found = changed_since(&t, "/w", &paths, 5000, &x);
        let _ = docker(None, &["rm", "-f", name]);
        assert_eq!(found, ["/w/out/new.docx"]);
    }

    #[test]
    #[ignore]
    fn a_stopped_container_comes_back() {
        let name = "forge-test-stopped";
        let _ = docker(None, &["rm", "-f", name]);
        docker(
            None,
            &[
                "run",
                "-d",
                "--name",
                name,
                "alpine:3.20",
                "sleep",
                "infinity",
            ],
        )
        .unwrap();
        docker(None, &["stop", "-t", "1", name]).unwrap();
        let out = run(
            &container(name),
            "echo hi",
            &[],
            b"",
            Duration::from_secs(60),
            &Extra::default(),
        );
        let _ = docker(None, &["rm", "-f", name]);
        assert_eq!(String::from_utf8_lossy(&out.unwrap()).trim(), "hi");
    }

    #[test]
    #[ignore]
    fn a_deleted_dev_container_comes_back_from_its_folder() {
        let root = std::env::temp_dir().join(format!("forge-dc-back-{}", std::process::id()));
        let dir = root.join("proj");
        std::fs::create_dir_all(dir.join(".devcontainer")).unwrap();
        std::fs::write(
            dir.join(".devcontainer/devcontainer.json"),
            r#"{"image":"alpine:3.20"}"#,
        )
        .unwrap();
        std::env::set_var("KODACT_CONFIG_DIR", root.join("config"));
        let remote = devcontainer_up(&DevSource::Local(dir.clone()), true, &|_| {}).unwrap();
        let Target::Container { name, .. } = &remote.target else {
            panic!()
        };
        docker(None, &["rm", "-f", name]).unwrap();
        let out = run(
            &remote.target,
            "echo back",
            &[],
            b"",
            Duration::from_secs(120),
            &Extra::default(),
        );
        let _ = docker(None, &["rm", "-f", name]);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(String::from_utf8_lossy(&out.unwrap()).trim(), "back");
    }

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
                    wsl: None,
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
            wsl: None,
        };
        assert_eq!(dc.label(), "Dev Container: api");
        let plain = Target::Container {
            name: "web".into(),
            user: None,
            wsl: None,
        };
        assert_eq!(plain.label(), "Container: web");
    }

    // Runs the remote's scripts with a local sh: Unix only.
    #[cfg(unix)]
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
        // Keys only, unless the app's askpass runs (another test may have
        // started it in this process): then ssh may ask, through it.
        if crate::askpass::env().is_some() {
            assert!(a.contains(&"BatchMode=no".into()));
            assert!(ssh
                .get_envs()
                .any(|(k, v)| k == "SSH_ASKPASS_REQUIRE" && v == Some("force".as_ref())));
        } else {
            assert!(a.contains(&"BatchMode=yes".into()));
        }
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
                wsl: None,
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
    // Runs the remote's scripts with a local sh: Unix only.
    #[cfg(unix)]
    #[test]
    fn boot_reads_the_environment_from_stdin() {
        let home = std::env::temp_dir().join(format!("forge-boot-{}", std::process::id()));
        std::fs::create_dir_all(home.join("proj")).unwrap();
        let mut prelude = String::new();
        prelude.push_str(&env_line("KODACT_API_KEY", "sk-secret"));
        prelude.push_str(&env_line("KODACT_CUSTOM_HEADERS", "A: 1\nB: 2\\x"));
        prelude.push('\n');
        let script = boot_script();
        let mut child = Command::new("sh")
            .args(["-c", &script, "sh", "~/proj", "/bin/sh", "-c"])
            .arg(r#"printf '%s|%s|%s|' "$PWD" "$KODACT_API_KEY" "$KODACT_CUSTOM_HEADERS"; cat; printf '|%s' "$1""#)
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

    /// A long file in the environment (a design chat's prompt) is written
    /// where it's named, and gone from what KodAct CLI inherits.
    #[cfg(unix)]
    #[test]
    fn boot_writes_the_host_file() {
        let home = std::env::temp_dir().join(format!("forge-boot-file-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let text = format!("# Prompt\nwith a \\ and 'quotes' $HOME\n{}", "x".repeat(200_000));
        let mut prelude = env_line(HOST_FILE, "~/p/design.md");
        prelude.push_str(&env_line(HOST_TEXT, &text));
        prelude.push('\n');
        let mut child = Command::new("sh")
            .args(["-c", &boot_script(), "sh", "~", "/bin/sh", "-c"])
            .arg(r#"printf '%s|%s' "${KODACT_HOST_TEXT:-gone}" "${KODACT_HOST_FILE:-gone}""#)
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(prelude.as_bytes()).unwrap();
        let out = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
        assert_eq!(out, "gone|gone");
        assert_eq!(std::fs::read_to_string(home.join("p/design.md")).unwrap(), text);
        std::fs::remove_dir_all(home).ok();
    }

    // Runs the remote's scripts with a local sh: Unix only.
    #[cfg(unix)]
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
            wsl: None,
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
    // Runs the remote's scripts with a local sh: Unix only.
    #[cfg(unix)]
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

    /// Drive a remote KodAct CLI through [`launch`] as the app does: the
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
            ("KODACT_BASE_URL".to_string(), model.to_string()),
            (
                "KODACT_API_KEY".to_string(),
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
        // stdin closed: the remote KodAct CLI ends, and so does the connection.
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
    /// `ssh -R`, are offered to the remote KodAct CLI's agent.
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
            ("KODACT_BASE_URL".to_string(), model.to_string()),
            ("KODACT_API_KEY".to_string(), "t".to_string()),
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
            names.iter().any(|n| n == "mcp__kodact-browser__navigate"),
            "{names:?}"
        );
        eprintln!(
            "{}: the browser's {} tools reach the remote agent",
            t.label(),
            names
                .iter()
                .filter(|n| n.starts_with("mcp__kodact-browser__"))
                .count()
        );
    }

    /// Against real targets: `KODACT_TEST_SSH` (a Host whose key isn't known
    /// yet), `KODACT_TEST_CONTAINER` (a running container), with
    /// `KODACT_REMOTE_CLI` (a Linux KodAct CLI) and `KODACT_TEST_MODEL` (the
    /// scripted model, reachable at the same URL from the targets).
    #[test]
    #[ignore]
    fn real_remotes() {
        let model = std::env::var("KODACT_TEST_MODEL").expect("KODACT_TEST_MODEL");
        let mut targets = vec![];
        if let Ok(h) = std::env::var("KODACT_TEST_SSH") {
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
        if let Ok(c) = std::env::var("KODACT_TEST_CONTAINER") {
            targets.push((
                Target::Container {
                    name: c,
                    user: None,
                    wsl: None,
                },
                Extra::default(),
            ));
        }
        assert!(!targets.is_empty());
        for (t, extra) in targets {
            let info = probe(&t, &extra).unwrap();
            assert_eq!(info.arch, "x86_64");
            // KodAct CLI goes up once; the second time it's already there.
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
    /// Needs `KODACT_TEST_IMAGE` (a local image with sh) and the variables
    /// [`real_remotes`] uses.
    #[test]
    #[ignore]
    fn real_devcontainer() {
        let image = std::env::var("KODACT_TEST_IMAGE").expect("KODACT_TEST_IMAGE");
        let model = std::env::var("KODACT_TEST_MODEL").expect("KODACT_TEST_MODEL");
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
            let remote = devcontainer_up(&DevSource::Local(dir.clone()), false, &|m| {
                said.lock().unwrap().push(m.to_string())
            })
            .unwrap();
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
            let again = devcontainer_up(&DevSource::Local(dir.clone()), false, &|_| {}).unwrap();
            assert_eq!(again, remote);
            let (asks, _) = modetest(&remote.target, "acceptEdits", &model, &Extra::default());
            assert_eq!(
                asks,
                ["Bash:rm a.txt", "Bash:curl -s http://127.0.0.1:1 || true"]
            );
            if let Target::Container { name, .. } = &remote.target {
                let _ = docker(None, &["rm", "-f", name]);
            }
            std::fs::remove_dir_all(&dir).ok();
            eprintln!("dev container ({case}) ok: {}", remote.target.label());
        }
    }

    /// Rebuild and the way back: a dev container knows its folder, and a
    /// fresh start replaces it (its changes gone, postCreateCommand run
    /// again, the folder still mounted). Needs `KODACT_TEST_IMAGE`.
    #[test]
    #[ignore]
    fn real_devcontainer_rebuild() {
        let image = std::env::var("KODACT_TEST_IMAGE").expect("KODACT_TEST_IMAGE");
        let dir = std::env::temp_dir().join(format!("forge-dc-rebuild-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".devcontainer")).unwrap();
        std::fs::write(
            dir.join(".devcontainer/devcontainer.json"),
            r#"{ "build": { "dockerfile": "Dockerfile" },
                "postCreateCommand": "echo ran >> /tmp/post-create" }"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(".devcontainer/Dockerfile"),
            format!("FROM {image}\n"),
        )
        .unwrap();
        std::fs::write(dir.join("hello.txt"), "from the host\n").unwrap();
        let sh = |r: &Remote, script: &str| {
            let out = run(
                &r.target,
                script,
                &[],
                b"",
                Duration::from_secs(20),
                &Extra::default(),
            )
            .unwrap();
            String::from_utf8_lossy(&out).trim().to_string()
        };
        let first = devcontainer_up(&DevSource::Local(dir.clone()), false, &|_| {}).unwrap();
        assert!(is_devcontainer(&first.target));
        let Target::Container { name, .. } = &first.target else {
            unreachable!()
        };
        assert_eq!(
            devcontainer_source(name, None).unwrap(),
            DevSource::Local(dir.clone())
        );
        sh(&first, "touch /tmp/changed-inside");
        let said = std::sync::Mutex::new(vec![]);
        let again = devcontainer_up(&DevSource::Local(dir.clone()), true, &|m| {
            said.lock().unwrap().push(m.to_string())
        })
        .unwrap();
        assert_eq!(again, first, "same name and folder");
        let said = said.lock().unwrap().clone();
        assert!(said.iter().any(|m| m.contains("Removing")), "{said:?}");
        assert!(said.iter().any(|m| m.contains("Building")), "{said:?}");
        assert_eq!(
            sh(
                &again,
                "[ -e /tmp/changed-inside ] && echo kept || echo gone"
            ),
            "gone"
        );
        assert_eq!(
            sh(&again, "wc -l < /tmp/post-create"),
            "1",
            "postCreateCommand ran on the new one"
        );
        assert_eq!(
            sh(&again, &format!("cat {}/hello.txt", again.path)),
            "from the host"
        );
        let _ = docker(None, &["rm", "-f", name]);
        std::fs::remove_dir_all(&dir).ok();
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

    #[test]
    fn uploads_are_checked() {
        let bytes = b"forge";
        let d = ring::digest::digest(&ring::digest::SHA256, bytes);
        let hex: String = d.as_ref().iter().map(|b| format!("{b:02x}")).collect();
        assert!(upload_matches(&format!("sha256 {hex}"), bytes));
        assert!(upload_matches(
            &format!("sha256 {}", hex.to_uppercase()),
            bytes
        ));
        assert!(upload_matches("size 5", bytes));
        assert!(!upload_matches("size 4", bytes));
        assert!(!upload_matches("", bytes));
        assert!(!upload_matches("sha256 00", bytes));
    }

    /// A real upload is checked, and a damaged one is caught and removed:
    /// `KODACT_TEST_SSH` names the host, `KODACT_REMOTE_CLI` the KodAct CLI.
    #[test]
    #[ignore]
    fn real_upload_is_verified() {
        let host = std::env::var("KODACT_TEST_SSH").expect("KODACT_TEST_SSH");
        let t = Target::Ssh(host);
        let extra = Extra {
            accept_new_host_key: true,
            reverse: vec![],
        };
        let base = crate::app_settings::get().server_install_dir;
        // Start clean, so the upload (and its check) happens.
        run(
            &t,
            &format!("{TILDE}\nrm -rf \"$(x \"$1\")/bin\""),
            &[base],
            b"",
            Duration::from_secs(30),
            &extra,
        )
        .unwrap();
        let steps = std::sync::Mutex::new(vec![]);
        let p = provision(&t, &extra, &|m: &str| {
            steps.lock().unwrap().push(m.to_string())
        })
        .unwrap();
        assert!(
            steps
                .lock()
                .unwrap()
                .iter()
                .any(|m| m.starts_with("Checking KodAct CLI")),
            "{:?}",
            steps.lock().unwrap()
        );
        // Damage the copy there: the check fails and removes it.
        run(
            &t,
            &format!("{TILDE}\nprintf x >> \"$(x \"$1\")\""),
            std::slice::from_ref(&p.forge),
            b"",
            Duration::from_secs(30),
            &extra,
        )
        .unwrap();
        let bin = std::fs::read(local_forge("x86_64").unwrap()).unwrap();
        let err = verify_upload(&t, &p.forge, &bin, &extra).unwrap_err();
        assert!(err.to_string().contains("damaged"), "{err}");
        let gone = run(
            &t,
            &format!("{TILDE}\n[ -e \"$(x \"$1\")\" ] && echo there || echo gone"),
            std::slice::from_ref(&p.forge),
            b"",
            Duration::from_secs(30),
            &extra,
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&gone).trim(), "gone");
    }
}

#[cfg(test)]
mod wsl_devcontainer_tests {
    use super::*;

    #[test]
    fn a_container_on_wsl_docker_round_trips_as_a_project_path() {
        let r = Remote {
            target: Target::Container {
                name: "forge-dc-app-0123abcd".into(),
                user: Some("vscode".into()),
                wsl: Some("CentOSStream9".into()),
            },
            path: "/workspaces/app".into(),
        };
        let p = r.to_path();
        assert_eq!(
            p.to_string_lossy().replace('\\', "/"),
            "container://vscode@forge-dc-app-0123abcd;wsl=CentOSStream9/workspaces/app"
        );
        assert_eq!(Remote::parse(&p), Some(r));
        // One on this computer's Docker is as before.
        let plain = Remote::parse(Path::new("container://web/srv")).unwrap();
        assert_eq!(
            plain.target,
            Target::Container {
                name: "web".into(),
                user: None,
                wsl: None
            }
        );
    }

    #[test]
    fn a_wsl_folder_is_kept_as_its_project_path_on_the_container() {
        let src = DevSource::Wsl {
            distro: "Ubuntu".into(),
            path: "/home/me/ta_testhouse_main".into(),
        };
        assert_eq!(src.label(), "wsl://Ubuntu/home/me/ta_testhouse_main");
        assert_eq!(DevSource::from_label(&src.label()), src);
        assert_eq!(src.base(), "ta_testhouse_main");
        assert_eq!(src.wsl(), Some("Ubuntu"));
        let local = DevSource::from_label("/home/me/app");
        assert_eq!(local, DevSource::Local(PathBuf::from("/home/me/app")));
        assert_eq!(local.wsl(), None);
    }

    #[test]
    fn docker_on_wsl_goes_through_wsl_exe() {
        let c = docker_command(Some("Ubuntu"));
        assert_eq!(c.get_program(), "wsl.exe");
        let args: Vec<_> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["-d", "Ubuntu", "--exec", "docker"]);
        assert_eq!(docker_command(None).get_program(), "docker");
    }
}
