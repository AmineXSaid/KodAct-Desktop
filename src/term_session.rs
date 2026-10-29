//! A terminal in the dock (`ui/terminal_dock.rs`): a shell on a
//! pseudo-terminal (ConPTY on Windows), its output run through a terminal
//! emulator (alacritty_terminal: the grid, scrollback, the alternate screen,
//! the modes) that the dock draws.
//!
//! A remote project's terminal is the same thing with another command: the
//! local pseudo-terminal runs `ssh -tt`, `wsl.exe` or `docker exec -it`, which
//! gives the shell a terminal on the remote and carries resizes across. So
//! this computer and every remote share one code path.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use portable_pty::{CommandBuilder, MasterPty, PtySize};

use crate::remote::{sh_quote, Remote, Target};

/// What a terminal runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    /// As the tab and the shell picker name it: "bash", "PowerShell".
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    /// Where it starts (a local terminal; a remote one starts through its command).
    pub cwd: Option<PathBuf>,
    /// More environment, over the app's own (ForgeCLI's endpoint and key).
    pub env: Vec<(String, String)>,
}

/// The shells a new terminal can run, the default first: this computer's,
/// or the remote's (its login shell, bash, sh).
/// The shells a new terminal can run here. `trust_new_key`: an SSH host's
/// key was accepted in Forge, so a first connection takes it.
pub fn shells(
    remote: Option<&Remote>,
    local_dir: &std::path::Path,
    trust_new_key: bool,
) -> Vec<Shell> {
    match remote {
        Some(r) => ["", "bash", "sh"]
            .iter()
            .map(|sh| remote_shell(r, sh, trust_new_key))
            .collect(),
        None => {
            let mut out = local_shells(local_dir);
            // The setting's shell first: a new terminal runs it.
            if let Some(want) = crate::app_settings::get().default_shell {
                let name = |p: &str| {
                    std::path::Path::new(p)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_lowercase())
                };
                if let Some(i) = out.iter().position(|s| name(&s.program) == name(&want)) {
                    let s = out.remove(i);
                    out.insert(0, s);
                } else {
                    out.insert(
                        0,
                        Shell {
                            label: want.clone(),
                            program: want,
                            args: vec![],
                            cwd: local_dir.is_dir().then(|| local_dir.to_path_buf()),
                            env: vec![],
                        },
                    );
                }
            }
            out
        }
    }
}

fn local_shells(dir: &std::path::Path) -> Vec<Shell> {
    let cwd = dir.is_dir().then(|| dir.to_path_buf());
    let mk = |label: &str, program: &str, args: &[&str]| Shell {
        label: label.into(),
        program: program.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: cwd.clone(),
        env: vec![],
    };
    let mut out = vec![];
    if cfg!(windows) {
        if which("pwsh.exe") {
            out.push(mk("PowerShell 7", "pwsh.exe", &["-NoLogo"]));
        }
        out.push(mk("PowerShell", "powershell.exe", &["-NoLogo"]));
        out.push(mk("Command Prompt", "cmd.exe", &[]));
        if let Ok(distros) = crate::remote::wsl_distros() {
            for d in distros
                .iter()
                .filter(|d| !d.name.starts_with("docker-desktop"))
            {
                out.push(mk(
                    &format!("WSL: {}", d.name),
                    "wsl.exe",
                    &["-d", &d.name, "--cd", "~"],
                ));
            }
        }
    } else {
        let login = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let name = login.rsplit('/').next().unwrap_or("sh").to_string();
        out.push(mk(&name, &login, &[]));
        for sh in ["bash", "zsh", "fish", "sh"] {
            if sh != name && which(sh) {
                out.push(mk(sh, sh, &[]));
            }
        }
    }
    out
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// A shell on a remote, in the project's folder: `""` for its login shell.
/// `trust_new_key`: an SSH host the person trusted in this run.
pub fn remote_shell(r: &Remote, shell: &str, trust_new_key: bool) -> Shell {
    // The shell, as a login shell.
    let start = if shell.is_empty() {
        r#"exec "${SHELL:-sh}" -l"#.to_string()
    } else {
        format!("exec {} -l", sh_quote(shell))
    };
    let label = if shell.is_empty() {
        "Login shell".to_string()
    } else {
        shell.to_string()
    };
    remote_command(r, &start, label, trust_new_key, &[])
}

/// A command line run on a remote, in the project's folder, in a terminal.
/// `reverse`: SSH `-R` forwards (`there:127.0.0.1:here`) for its lifetime.
pub fn remote_command(
    r: &Remote,
    start: &str,
    label: String,
    trust_new_key: bool,
    reverse: &[String],
) -> Shell {
    // Into the folder (`~` expanded there), then the command.
    let script = format!(
        r#"case $1 in "~") d=$HOME;; "~/"*) d=$HOME/${{1#"~/"}};; *) d=$1;; esac; cd "$d" 2>/dev/null; {start}"#
    );
    let (program, args): (&str, Vec<String>) = match &r.target {
        Target::Ssh(host) => {
            let mut a = crate::app_settings::get().ssh_args();
            a.extend([
                "-tt".to_string(),
                "-o".into(),
                "ServerAliveInterval=15".into(),
            ]);
            if trust_new_key {
                a.extend(["-o".into(), "StrictHostKeyChecking=accept-new".into()]);
            }
            for f in reverse {
                a.extend(["-R".to_string(), f.clone()]);
            }
            a.extend([
                host.clone(),
                "--".into(),
                format!("sh -c {} sh {}", sh_quote(&script), sh_quote(&r.path)),
            ]);
            ("ssh", a)
        }
        Target::Wsl(distro) => (
            "wsl.exe",
            ["-d", distro, "--exec", "sh", "-c", &script, "sh", &r.path]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ),
        Target::Container { name, user, wsl } => {
            // On a WSL distro's Docker: through wsl.exe, as every call to it.
            let mut a = match wsl {
                Some(d) => vec![
                    "-d".to_string(),
                    d.clone(),
                    "--exec".into(),
                    "docker".into(),
                ],
                None => vec![],
            };
            a.extend(["exec".to_string(), "-it".into()]);
            if let Some(u) = user {
                a.extend(["-u".into(), u.clone()]);
            }
            a.extend([
                name.clone(),
                "sh".into(),
                "-c".into(),
                script,
                "sh".into(),
                r.path.clone(),
            ]);
            (if wsl.is_some() { "wsl.exe" } else { "docker" }, a)
        }
    };
    Shell {
        label,
        program: program.into(),
        args,
        cwd: None,
        env: vec![],
    }
}

/// The grid size, for the emulator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}

/// Hands the emulator's requests back: answers it writes to the shell (a
/// cursor position report), and its title.
#[derive(Clone)]
pub struct Listener {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    title: Arc<Mutex<Option<String>>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(s) => {
                let _ = self.writer.lock().unwrap().write_all(s.as_bytes());
            }
            Event::Title(t) => *self.title.lock().unwrap() = Some(t),
            Event::ResetTitle => *self.title.lock().unwrap() = None,
            _ => {}
        }
    }
}

/// One terminal: its shell's pseudo-terminal and the emulator's state.
pub struct Session {
    pub shell: Shell,
    pub term: Arc<FairMutex<Term<Listener>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// None for a replay: output shown, no process behind it.
    master: Option<Mutex<Box<dyn MasterPty + Send>>>,
    child: Option<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    title: Arc<Mutex<Option<String>>>,
    size: Mutex<Size>,
    /// The shell ended (the tab says so; it no longer takes input).
    pub exited: Arc<AtomicBool>,
}

impl Session {
    /// A read-only terminal showing `bytes` as a terminal would (colours,
    /// line ends): what an agent's command printed, replayed in the dock.
    pub fn replay(shell: Shell, bytes: &[u8], size: Size, scrollback: usize) -> Session {
        let writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(Box::new(std::io::sink())));
        let title = Arc::new(Mutex::new(None));
        let listener = Listener {
            writer: writer.clone(),
            title: title.clone(),
        };
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let mut term = Term::new(config, &size, listener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        Session {
            shell,
            term: Arc::new(FairMutex::new(term)),
            writer,
            master: None,
            child: None,
            title,
            size: Mutex::new(size),
            exited: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Output only: no process to type into.
    pub fn is_replay(&self) -> bool {
        self.child.is_none()
    }
}

impl Session {
    /// Start `shell` at `size`. `output` is called (on a reader thread)
    /// whenever new output was taken in, so the view can redraw.
    pub fn start(
        shell: Shell,
        size: Size,
        scrollback: usize,
        output: impl Fn() + Send + 'static,
    ) -> Result<Session, String> {
        let pty = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: size.rows,
                cols: size.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Couldn't open a terminal: {e}"))?;
        // A prompt in the dock's colours (shell_prompt.rs).
        let dressed = crate::shell_prompt::dress(&shell.program, &shell.args);
        let mut cmd = CommandBuilder::new(&dressed.program);
        cmd.args(&dressed.args);
        if let Some(d) = &shell.cwd {
            cmd.cwd(d);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, v) in &dressed.env {
            cmd.env(k, v);
        }
        for (k, v) in &shell.env {
            cmd.env(k, v);
        }
        let child = pty
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Couldn't start {}: {e}", shell.label))?;
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(
            pty.master.take_writer().map_err(|e| e.to_string())?,
        ));
        let title = Arc::new(Mutex::new(None));
        let listener = Listener {
            writer: writer.clone(),
            title: title.clone(),
        };
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener)));
        let exited = Arc::new(AtomicBool::new(false));
        let (t, done) = (term.clone(), exited.clone());
        std::thread::spawn(move || {
            let mut parser: Processor = Processor::new();
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        parser.advance(&mut *t.lock(), &buf[..n]);
                        output();
                    }
                }
            }
            done.store(true, Ordering::SeqCst);
            output();
        });
        Ok(Session {
            shell,
            term,
            writer,
            master: Some(Mutex::new(pty.master)),
            child: Some(Mutex::new(child)),
            title,
            size: Mutex::new(size),
            exited,
        })
    }

    /// Send keys (or pasted text) to the shell.
    pub fn input(&self, bytes: &[u8]) {
        if self.exited.load(Ordering::SeqCst) {
            return;
        }
        let mut w = self.writer.lock().unwrap();
        let _ = w.write_all(bytes);
        let _ = w.flush();
    }

    /// A new grid size: the emulator reflows and the shell is told.
    pub fn resize(&self, size: Size) {
        let mut cur = self.size.lock().unwrap();
        if *cur == size || size.cols < 2 || size.rows < 1 {
            return;
        }
        *cur = size;
        self.term.lock().resize(size);
        if let Some(m) = &self.master {
            let _ = m.lock().unwrap().resize(PtySize {
                rows: size.rows,
                cols: size.cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn size(&self) -> Size {
        *self.size.lock().unwrap()
    }

    /// The tab's name: the title the program set, else the shell's.
    pub fn title(&self) -> String {
        self.title
            .lock()
            .unwrap()
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| self.shell.label.clone())
    }

    pub fn kill(&self) {
        if let Some(c) = &self.child {
            let _ = c.lock().unwrap().kill();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line, Point};
    use std::time::{Duration, Instant};

    fn row(s: &Session, line: i32) -> String {
        let t = s.term.lock();
        (0..t.columns())
            .map(|c| t.grid()[Point::new(Line(line), Column(c))].c)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn wait(s: &Session, what: &str) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            let t = s.term.lock();
            let lines = t.screen_lines() as i32;
            drop(t);
            if (0..lines).any(|l| row(s, l).contains(what)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        false
    }

    /// A real shell on a real pseudo-terminal: typed input runs, output and
    /// colours land in the grid, the size reaches the shell, and it ends.
    #[cfg(unix)]
    #[test]
    fn a_shell_runs_in_the_grid() {
        let dir = std::env::temp_dir();
        let shell = Shell {
            label: "sh".into(),
            program: "sh".into(),
            args: vec![],
            cwd: Some(dir),
            env: vec![],
        };
        let s = Session::start(shell, Size { cols: 80, rows: 24 }, 1000, || {}).unwrap();
        s.input(b"printf '\\033[31mred\\033[0m %s\\n' hello-forge\r");
        assert!(wait(&s, "red hello-forge"), "output reached the grid");
        s.resize(Size {
            cols: 100,
            rows: 30,
        });
        s.input(b"stty size\r");
        assert!(wait(&s, "30 100"), "the shell sees the new size");
        assert_eq!(s.term.lock().columns(), 100);
        s.input(b"exit\r");
        let start = Instant::now();
        while !s.exited.load(Ordering::SeqCst) {
            assert!(start.elapsed() < Duration::from_secs(10), "the shell ends");
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    /// Windows: cmd.exe on ConPTY, as the dock runs it. Typed input runs,
    /// output lands in the grid, a resize reaches the console, and it ends.
    #[cfg(windows)]
    #[test]
    fn a_windows_shell_runs_in_the_grid() {
        let shell = Shell {
            label: "Command Prompt".into(),
            program: "cmd.exe".into(),
            args: vec![],
            cwd: Some(std::env::temp_dir()),
            env: vec![],
        };
        let s = Session::start(shell, Size { cols: 80, rows: 24 }, 1000, || {}).unwrap();
        s.input(b"echo hello-forge-win\r");
        assert!(wait(&s, "hello-forge-win"), "output reached the grid");
        s.resize(Size {
            cols: 100,
            rows: 30,
        });
        assert_eq!(s.term.lock().columns(), 100);
        s.input(b"exit\r");
        let start = Instant::now();
        while !s.exited.load(Ordering::SeqCst) {
            assert!(start.elapsed() < Duration::from_secs(15), "the shell ends");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn remote_shells_start_in_the_folder() {
        let r = Remote {
            target: Target::Ssh("box".into()),
            path: "~/api".into(),
        };
        let sh = remote_shell(&r, "", true);
        assert_eq!(sh.program, "ssh");
        assert!(sh.args.starts_with(&["-tt".to_string()]));
        assert!(sh
            .args
            .contains(&"StrictHostKeyChecking=accept-new".to_string()));
        let last = sh.args.last().unwrap();
        assert!(
            last.starts_with("sh -c ") && last.ends_with(" '~/api'"),
            "{last}"
        );
        // The script, run by a real sh: it changes into the folder and runs the shell.
        let script = match &remote_shell(
            &Remote {
                target: Target::Wsl("U".into()),
                path: "/tmp".into(),
            },
            "sh",
            false,
        )
        .args[..]
        {
            [_, _, _, _, _, script, ..] => script.clone(),
            other => panic!("{other:?}"),
        };
        // Run by a local sh where there is one (the remote's own shell runs it).
        if cfg!(unix) {
            let out = std::process::Command::new("sh")
                .args(["-c", &script.replace("exec sh -l", "pwd"), "sh", "/tmp"])
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/tmp");
        }
        let dc = remote_shell(
            &Remote {
                target: Target::Container {
                    name: "api".into(),
                    user: Some("dev".into()),
                    wsl: None,
                },
                path: "/w".into(),
            },
            "bash",
            false,
        );
        assert_eq!(dc.args[..5], ["exec", "-it", "-u", "dev", "api"]);
        assert_eq!(dc.label, "bash");
    }

    /// A real remote terminal: `FORGE_TEST_SSH` names an SSH host (and
    /// `FORGE_TEST_CONTAINER` a running container, optionally). The shell
    /// starts in the folder, runs commands, sees resizes, and ends.
    #[test]
    #[ignore]
    fn real_remote_terminals() {
        let mut remotes = vec![];
        if let Ok(h) = std::env::var("FORGE_TEST_SSH") {
            remotes.push(Remote {
                target: Target::Ssh(h),
                path: "/tmp".into(),
            });
        }
        if let Ok(c) = std::env::var("FORGE_TEST_CONTAINER") {
            remotes.push(Remote {
                target: Target::Container {
                    name: c,
                    user: None,
                    wsl: None,
                },
                path: "/tmp".into(),
            });
        }
        assert!(
            !remotes.is_empty(),
            "set FORGE_TEST_SSH or FORGE_TEST_CONTAINER"
        );
        for r in remotes {
            let shell = remote_shell(&r, "", true);
            let s = Session::start(shell, Size { cols: 90, rows: 20 }, 1000, || {}).unwrap();
            s.input(b"echo \"at $(pwd)\"\r");
            assert!(wait(&s, "at /tmp"), "{:?}: starts in the folder", r.target);
            s.resize(Size {
                cols: 101,
                rows: 33,
            });
            // Without stty (a minimal image), interactive bash reads the size.
            s.input(b"stty size 2>/dev/null || bash -ic 'echo $LINES $COLUMNS' 2>/dev/null\r");
            let seen = wait(&s, "33 101");
            if !seen {
                let lines = s.term.lock().screen_lines() as i32;
                for l in 0..lines {
                    eprintln!("|{}", row(&s, l));
                }
            }
            assert!(seen, "{:?}: the remote sees the size", r.target);
            s.input(b"exit\r");
            let start = Instant::now();
            while !s.exited.load(Ordering::SeqCst) {
                assert!(
                    start.elapsed() < Duration::from_secs(15),
                    "{:?}: ends",
                    r.target
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    #[test]
    fn a_replay_shows_output_without_a_process() {
        let shell = Shell {
            label: "Agent".into(),
            program: String::new(),
            args: vec![],
            cwd: None,
            env: vec![],
        };
        let s = Session::replay(
            shell,
            b"$ ls\r\n\x1b[34mbin\x1b[0m  src\r\n",
            Size { cols: 40, rows: 5 },
            100,
        );
        assert!(s.is_replay());
        assert_eq!(row(&s, 0), "$ ls");
        assert_eq!(row(&s, 1), "bin  src");
        s.input(b"ignored");
        s.resize(Size { cols: 30, rows: 4 });
        s.kill();
        assert_eq!(s.term.lock().columns(), 30);
    }
}
