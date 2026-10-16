//! The browser in the side panel: one headless Chromium that Forge starts,
//! shown live in the panel and driven by the agent at the same time.
//!
//! Forge starts Chromium itself (Playwright's build when it is installed,
//! else Chrome or Edge) with a DevTools port, and the agent's browser tools are
//! connected to that port (`browser_mcp.rs`), so there is no window of its own
//! (`browser.rs`). The panel watches the page through the DevTools protocol:
//! a screencast sends a JPEG each time the page changes, and your clicks,
//! scrolling and typing go back as input events. So what the agent does and
//! what you do happen in the same tab, in front of you.
//!
//! The DevTools connection runs on its own thread. The UI sends it `Cmd`s
//! and reads `Update`s from a channel; nothing here blocks the UI thread.

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use gpui::RenderImage;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// The page size before the panel says its own, in CSS pixels.
pub const VIEWPORT: (u32, u32) = (1280, 800);

/// What the panel asks of the browser.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Navigate(String),
    Back,
    Forward,
    Reload,
    /// A mouse button going down, and coming up, at a point on the page
    /// (CSS pixels): apart, so a drag between
    /// them selects text or moves a slider.
    Press {
        x: f64,
        y: f64,
        button: MouseButton,
        count: u32,
    },
    Release {
        x: f64,
        y: f64,
        button: MouseButton,
        count: u32,
    },
    /// The mouse moving; `held` is the button held down, for a drag.
    Move {
        x: f64,
        y: f64,
        held: Option<MouseButton>,
    },
    /// Answer the dialog the page shows: OK (with the text, for a prompt)
    /// or Cancel.
    Dialog {
        accept: bool,
        text: Option<String>,
    },
    /// Copy the page's selected text (it comes back as `Update::Copied`).
    Copy,
    /// Select everything in the focused field, or on the page.
    SelectAll,
    Wheel {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    /// Text typed into the focused field.
    Text(String),
    /// A key that isn't text: Enter, Backspace, an arrow…
    Key(String),
    /// Show another tab, open a new one, close one.
    Show(String),
    NewTab(String),
    Close(String),
    /// The panel's size for the page, in CSS pixels, and the screen's
    /// pixels per CSS pixel: the page lays itself out to fit, and its
    /// pictures come at the screen's density.
    Resize {
        width: u32,
        height: u32,
        scale: f64,
    },
}

/// A mouse button, as the page gets it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

impl MouseButton {
    fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Middle => "middle",
            MouseButton::Right => "right",
        }
    }

    /// The `buttons` bit DevTools wants while it is held.
    fn bit(self) -> u32 {
        match self {
            MouseButton::Left => 1,
            MouseButton::Right => 2,
            MouseButton::Middle => 4,
        }
    }
}

/// A dialog the page opened (`alert`, `confirm`, `prompt`, or the "leave
/// this page?" of `beforeunload`), waiting for an answer.
#[derive(Debug, Clone, PartialEq)]
pub struct PageDialog {
    pub kind: String,
    pub message: String,
    /// A prompt's suggested answer.
    pub default: String,
}

/// How the page is laid out and pictured.
#[derive(Clone, Copy)]
struct Metrics {
    width: u32,
    height: u32,
    scale: f64,
}

/// An open tab: its target id, address and title.
#[derive(Debug, Clone, PartialEq)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
}

/// What the browser tells the panel.
#[derive(Debug)]
pub enum Update {
    /// A new picture of the page, decoded for drawing.
    Frame(Arc<RenderImage>),
    /// The open tabs, and which one is on screen.
    Tabs(Vec<TabInfo>, String),
    /// The page's address and title.
    Page {
        url: String,
        title: String,
    },
    Loading(bool),
    /// The page opened a dialog (Some), or it was answered (None).
    Dialog(Option<PageDialog>),
    /// A download finished: its file in the Downloads folder.
    Downloaded(PathBuf),
    /// A download stopped before it finished: its name.
    DownloadFailed(String),
    /// The page's selected text, for the clipboard.
    Copied(String),
    /// The mouse pointer the page wants where the mouse is (CSS names:
    /// `pointer`, `text`, `default`…).
    Cursor(String),
    /// The connection is gone, and why.
    Closed(String),
}

/// The running browser: its process and the line to its DevTools thread.
pub struct WebView {
    child: Mutex<Child>,
    cmds: mpsc::Sender<Cmd>,
    /// Where the browser tools connect: `http://127.0.0.1:<port>`.
    pub cdp_url: String,
}

impl WebView {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }
}

impl Drop for WebView {
    fn drop(&mut self) {
        // Closing the guard's stdin closes the browser; kill makes sure.
        let _ = self.child.lock().map(|mut c| {
            drop(c.stdin.take());
            c.kill()
        });
    }
}

/// `forge-gpui --browser-guard <chromium> <args…>`: run the browser, and
/// close it as soon as stdin closes, which happens when Forge exits, quits
/// or crashes. So the browser never outlives the app.
pub fn guard(args: &[String]) {
    let Some((exe, rest)) = args.split_first() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.args(rest).stdin(Stdio::null()).stdout(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(child) = cmd.spawn() else {
        std::process::exit(1);
    };
    let child = Arc::new(Mutex::new(child));
    let watched = child.clone();
    std::thread::spawn(move || {
        let mut sink = Vec::new();
        let _ = std::io::Read::read_to_end(&mut std::io::stdin(), &mut sink);
        let _ = watched.lock().map(|mut c| c.kill());
        std::process::exit(0);
    });
    loop {
        if let Ok(Some(status)) = child.lock().map(|mut c| c.try_wait().ok().flatten()) {
            std::process::exit(status.code().unwrap_or(0));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The Chromium Forge runs: `FORGE_CHROME`, then Playwright's Chromium,
/// then Chrome, then Edge (always there on Windows 10 and 11).
pub fn find_chromium() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FORGE_CHROME").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    let home = std::env::var_os(if cfg!(windows) {
        "LOCALAPPDATA"
    } else {
        "HOME"
    })
    .map(PathBuf::from);
    let mut roots: Vec<PathBuf> = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    if let Some(h) = &home {
        roots.push(if cfg!(windows) {
            h.join("ms-playwright")
        } else if cfg!(target_os = "macos") {
            h.join("Library/Caches/ms-playwright")
        } else {
            h.join(".cache/ms-playwright")
        });
    }
    let inner: &[&str] = if cfg!(windows) {
        &["chrome-win64/chrome.exe", "chrome-win/chrome.exe"]
    } else if cfg!(target_os = "macos") {
        &[
            "chrome-mac-arm64/Chromium.app/Contents/MacOS/Chromium",
            "chrome-mac/Chromium.app/Contents/MacOS/Chromium",
        ]
    } else {
        &["chrome-linux64/chrome", "chrome-linux/chrome"]
    };
    for root in &roots {
        // The newest `chromium-<build>` first.
        let mut builds: Vec<PathBuf> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("chromium-"))
            })
            .collect();
        builds.sort();
        for b in builds.iter().rev() {
            if let Some(exe) = inner.iter().map(|i| b.join(i)).find(|p| p.is_file()) {
                return Some(exe);
            }
        }
    }
    let installed: Vec<PathBuf> = if cfg!(windows) {
        let pf = |k: &str| std::env::var_os(k).map(PathBuf::from);
        [
            pf("ProgramFiles").map(|p| p.join("Google/Chrome/Application/chrome.exe")),
            pf("ProgramFiles(x86)").map(|p| p.join("Google/Chrome/Application/chrome.exe")),
            home.clone()
                .map(|p| p.join("Google/Chrome/Application/chrome.exe")),
            pf("ProgramFiles(x86)").map(|p| p.join("Microsoft/Edge/Application/msedge.exe")),
            pf("ProgramFiles").map(|p| p.join("Microsoft/Edge/Application/msedge.exe")),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else if cfg!(target_os = "macos") {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
            "/Applications/Chromium.app/Contents/MacOS/Chromium".into(),
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge".into(),
        ]
    } else {
        let path: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        [
            "google-chrome",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
        ]
        .iter()
        .flat_map(|n| path.iter().map(move |d| d.join(n)))
        .collect()
    };
    installed.into_iter().find(|p| p.is_file())
}

/// Running as root on Linux (a container): Chromium's sandbox won't start.
fn needs_no_sandbox() -> bool {
    cfg!(target_os = "linux")
        && std::fs::read_to_string("/proc/self/status").is_ok_and(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1))
                == Some("0")
        })
}

/// A free port on this machine for the DevTools server.
fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

/// Start the browser and its DevTools thread. `profile` is the folder for
/// its cookies and history, kept between launches. It returns at once: the
/// browser comes up meanwhile, and the updates say when it is there.
pub fn start(
    exe: &Path,
    profile: &Path,
    focus: &Path,
) -> Result<(WebView, async_channel::Receiver<Update>), String> {
    let focus = focus.to_path_buf();
    std::fs::create_dir_all(profile).map_err(|e| e.to_string())?;
    let port = free_port()?;
    let mut chrome: Vec<String> = vec![
        "--headless=new".into(),
        format!("--remote-debugging-port={port}"),
        "--remote-debugging-address=127.0.0.1".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-features=Translate,MediaRouter".into(),
        "--hide-scrollbars".into(),
        "--mute-audio".into(),
        format!("--window-size={},{}", VIEWPORT.0, VIEWPORT.1),
        format!("--user-data-dir={}", profile.display()),
    ];
    if needs_no_sandbox() {
        chrome.push("--no-sandbox".into());
    }
    chrome.push("about:blank".into());
    // Through the guard (this program, `--browser-guard`), which closes the
    // browser when Forge goes, however it goes; tests run it directly.
    let mut cmd = match std::env::current_exe() {
        Ok(me) if !cfg!(test) => {
            let mut c = Command::new(me);
            c.arg("--browser-guard").arg(exe).args(&chrome);
            c.stdin(Stdio::piped());
            c
        }
        _ => {
            let mut c = Command::new(exe);
            c.args(&chrome);
            c.stdin(Stdio::null());
            c
        }
    };
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", exe.display()))?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (up_tx, up_rx) = async_channel::unbounded();
    std::thread::spawn(move || {
        // Chromium names its DevTools address on stderr once it listens.
        let (found_tx, found_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut sent = false;
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(ws) = line.strip_prefix("DevTools listening on ") {
                    if !sent {
                        let _ = found_tx.send(ws.trim().to_string());
                        sent = true;
                    }
                }
            }
        });
        let why = match found_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(ws) => match devtools(&ws, cmd_rx, &up_tx, &focus) {
                Ok(()) => "the browser closed".to_string(),
                Err(e) => e,
            },
            Err(_) => "the browser did not start".to_string(),
        };
        let _ = up_tx.send_blocking(Update::Closed(why));
    });
    Ok((
        WebView {
            child: Mutex::new(child),
            cmds: cmd_tx,
            cdp_url: format!("http://127.0.0.1:{port}"),
        },
        up_rx,
    ))
}

/// A screencast JPEG as a picture GPUI draws (BGRA).
fn decode(jpeg: &[u8]) -> Option<Arc<RenderImage>> {
    let mut rgba = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .ok()?
        .into_rgba8();
    for px in rgba.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}

// ---------------------------------------------------------------- DevTools

type Socket = tungstenite::WebSocket<TcpStream>;

/// A DevTools connection: commands out, answers and events back.
pub(crate) struct Conn {
    ws: Socket,
    next: u64,
    /// Answer the page's dialogs while waiting on a call: Some(true) says
    /// OK to every one, Some(false) OK to an alert and Cancel to a question.
    /// None (the panel's connection) leaves them to whoever shows them.
    pub(crate) answer_dialogs: Option<bool>,
    /// The dialogs answered that way, said in words, for the tool's answer.
    pub(crate) dialogs: Vec<String>,
}

/// Connect to a DevTools WebSocket (`ws://127.0.0.1:<port>/devtools/…`).
pub(crate) fn connect(ws_url: &str) -> Result<Conn, String> {
    let host = ws_url
        .strip_prefix("ws://")
        .and_then(|r| r.split('/').next())
        .ok_or("a bad DevTools address")?;
    let stream = TcpStream::connect(host).map_err(|e| e.to_string())?;
    let (ws, _) = tungstenite::client(ws_url, stream).map_err(|e| e.to_string())?;
    ws.get_ref()
        .set_read_timeout(Some(Duration::from_millis(30)))
        .map_err(|e| e.to_string())?;
    Ok(Conn {
        ws,
        next: 0,
        answer_dialogs: None,
        dialogs: vec![],
    })
}

impl Conn {
    pub(crate) fn send(
        &mut self,
        session: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<u64, String> {
        self.next += 1;
        let mut msg = json!({ "id": self.next, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = s.into();
        }
        self.ws
            .send(tungstenite::Message::text(msg.to_string()))
            .map_err(|e| e.to_string())?;
        Ok(self.next)
    }

    /// The next message, or None when nothing came within the read timeout.
    pub(crate) fn read(&mut self) -> Result<Option<Value>, String> {
        match self.ws.read() {
            Ok(tungstenite::Message::Text(t)) => Ok(serde_json::from_str(t.as_str()).ok()),
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Send and wait for the answer, passing on events that arrive meanwhile.
    pub(crate) fn call(
        &mut self,
        session: Option<&str>,
        method: &str,
        params: Value,
        events: &mut Vec<Value>,
    ) -> Result<Value, String> {
        let id = self.send(session, method, params)?;
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            if let Some(m) = self.read()? {
                if m["id"].as_u64() == Some(id) {
                    if let Some(e) = m.get("error") {
                        return Err(format!(
                            "{method}: {}",
                            e["message"].as_str().unwrap_or("failed")
                        ));
                    }
                    return Ok(m["result"].clone());
                }
                // A dialog would hold the call (and the page) until answered.
                if let (Some(accept), Some("Page.javascriptDialogOpening")) =
                    (self.answer_dialogs, m["method"].as_str())
                {
                    self.answer_dialog(&m, accept)?;
                    continue;
                }
                events.push(m);
            }
        }
        Err(format!(
            "{method}: no answer. The page may be stuck in a long script, or showing a dialog."
        ))
    }

    /// Answer a dialog the page opened, and note what it said.
    fn answer_dialog(&mut self, m: &Value, accept_questions: bool) -> Result<(), String> {
        let p = &m["params"];
        let kind = p["type"].as_str().unwrap_or("alert");
        let message = p["message"].as_str().unwrap_or("");
        let question = matches!(kind, "confirm" | "prompt");
        let accept = !question || accept_questions;
        let mut answer = json!({ "accept": accept });
        if kind == "prompt" && accept {
            answer["promptText"] = p["defaultPrompt"].clone();
        }
        self.send(
            m["sessionId"].as_str(),
            "Page.handleJavaScriptDialog",
            answer,
        )?;
        self.dialogs.push(dialog_note(kind, message, accept));
        Ok(())
    }
}

/// A dialog the agent's step met, said for the agent.
pub(crate) fn dialog_note(kind: &str, message: &str, accepted: bool) -> String {
    match (kind, accepted) {
        ("confirm" | "prompt", true) => {
            format!("The page asked ({kind}): \"{message}\". Answered OK.")
        }
        ("confirm" | "prompt", false) => format!(
            "The page asked ({kind}): \"{message}\". Answered Cancel; to answer OK, do the step again with dialog: \"accept\"."
        ),
        ("beforeunload", _) => "The page asked to confirm leaving it. Left it.".to_string(),
        _ => format!("The page showed an alert: \"{message}\". Closed it."),
    }
}

/// The tab on screen: its target, and the session attached to it.
struct Tab {
    target: String,
    session: String,
}

/// Lay the page out at `m` and (re)start its pictures at that size.
fn fit(
    c: &mut Conn,
    target: &str,
    session: &str,
    m: Metrics,
    events: &mut Vec<Value>,
) -> Result<(), String> {
    let s = Some(session);
    // A headless window pictures its whole size: make it the panel's.
    if let Ok(w) = c.call(
        None,
        "Browser.getWindowForTarget",
        json!({ "targetId": target }),
        events,
    ) {
        let _ = c.call(
            None,
            "Browser.setWindowBounds",
            json!({ "windowId": w["windowId"], "bounds": { "width": m.width, "height": m.height } }),
            events,
        );
    }
    c.call(
        s,
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": m.width, "height": m.height, "deviceScaleFactor": m.scale, "mobile": false }),
        events,
    )?;
    let _ = c.call(s, "Page.stopScreencast", json!({}), events);
    c.call(
        s,
        "Page.startScreencast",
        json!({ "format": "jpeg", "quality": 80,
            "maxWidth": (m.width as f64 * m.scale).round() as u32,
            "maxHeight": (m.height as f64 * m.scale).round() as u32 }),
        events,
    )?;
    Ok(())
}

fn attach(c: &mut Conn, target: &str, m: Metrics, events: &mut Vec<Value>) -> Result<Tab, String> {
    let r = c.call(
        None,
        "Target.attachToTarget",
        json!({ "targetId": target, "flatten": true }),
        events,
    )?;
    let session = r["sessionId"].as_str().ok_or("no session")?.to_string();
    let s = Some(session.as_str());
    c.call(s, "Page.enable", json!({}), events)?;
    fit(c, target, &session, m, events)?;
    Ok(Tab {
        target: target.to_string(),
        session,
    })
}

/// The page tabs open now, newest last.
pub(crate) fn pages(c: &mut Conn, events: &mut Vec<Value>) -> Result<Vec<TabInfo>, String> {
    let r = c.call(None, "Target.getTargets", json!({}), events)?;
    Ok(r["targetInfos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["type"] == "page")
        .map(|t| TabInfo {
            id: t["targetId"].as_str().unwrap_or_default().to_string(),
            url: t["url"].as_str().unwrap_or_default().to_string(),
            title: t["title"].as_str().unwrap_or_default().to_string(),
        })
        .collect())
}

/// Say which tab is on screen, for the agent's tools (`browser_mcp.rs`).
fn write_focus(path: &Path, id: &str) {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, id).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn devtools(
    ws_url: &str,
    cmds: mpsc::Receiver<Cmd>,
    up: &async_channel::Sender<Update>,
    focus: &Path,
) -> Result<(), String> {
    let mut c = connect(ws_url)?;
    let mut events = vec![];
    c.call(
        None,
        "Target.setDiscoverTargets",
        json!({ "discover": true }),
        &mut events,
    )?;
    let mut tabs = pages(&mut c, &mut events)?;
    if tabs.is_empty() {
        c.call(
            None,
            "Target.createTarget",
            json!({ "url": "about:blank" }),
            &mut events,
        )?;
        tabs = pages(&mut c, &mut events)?;
    }
    let first = tabs.last().ok_or("no tab")?.id.clone();
    let mut metrics = Metrics {
        width: VIEWPORT.0,
        height: VIEWPORT.1,
        scale: 1.0,
    };
    let mut tab = attach(&mut c, &first, metrics, &mut events)?;
    let say = |u: Update| {
        up.send_blocking(u)
            .map_err(|_| "the panel is gone".to_string())
    };
    // Tell the panel the tabs and the one on screen, with its address.
    let report = |c: &mut Conn, events: &mut Vec<Value>, on: &str| -> Result<(), String> {
        let tabs = pages(c, events)?;
        if let Some(t) = tabs.iter().find(|t| t.id == on) {
            say(Update::Page {
                url: t.url.clone(),
                title: t.title.clone(),
            })?;
        }
        say(Update::Tabs(tabs, on.to_string()))
    };
    report(&mut c, &mut events, &tab.target)?;
    write_focus(focus, &tab.target);
    // Downloads go to your Downloads folder, under a temporary name until
    // they finish; then they get their own (never overwriting), and the
    // panel and the agent's tools are told.
    let downloads = crate::downloads::dir().filter(|d| std::fs::create_dir_all(d).is_ok());
    if let Some(dir) = &downloads {
        let _ = c.call(
            None,
            "Browser.setDownloadBehavior",
            json!({ "behavior": "allowAndName", "downloadPath": dir, "eventsEnabled": true }),
            &mut events,
        );
    }
    let log = crate::browser::downloads_log(focus);
    let mut pending: std::collections::HashMap<String, String> = Default::default();
    // Answers awaited without blocking the loop: the pointer under the
    // mouse, and the selection to copy.
    let (mut cursor_req, mut copy_req) = (None::<u64>, None::<u64>);
    let mut cursor_asked = Instant::now();
    let mut cursor_at: Option<(f64, f64)> = None;
    let mut focus_checked = Instant::now();
    loop {
        // The agent's tools switched tabs: show the one they use.
        if focus_checked.elapsed() > Duration::from_millis(250) {
            focus_checked = Instant::now();
            let want = std::fs::read_to_string(focus).unwrap_or_default();
            let want = want.trim();
            if !want.is_empty() && want != tab.target {
                if pages(&mut c, &mut events)?.iter().any(|t| t.id == want) {
                    let old = tab.session.clone();
                    let _ = c.send(Some(&old), "Page.stopScreencast", json!({}));
                    tab = attach(&mut c, want, metrics, &mut events)?;
                    report(&mut c, &mut events, &tab.target)?;
                } else {
                    write_focus(focus, &tab.target);
                }
            }
        }
        // What the panel asked for.
        while let Ok(cmd) = cmds.try_recv() {
            let s = Some(tab.session.as_str());
            let mouse = |kind: &str, x: f64, y: f64, button: &str, count: u32| json!({ "type": kind, "x": x, "y": y, "button": button, "clickCount": count });
            match cmd {
                Cmd::Navigate(url) => {
                    c.send(s, "Page.navigate", json!({ "url": url }))?;
                }
                Cmd::Reload => {
                    c.send(s, "Page.reload", json!({}))?;
                }
                Cmd::Back | Cmd::Forward => {
                    let h = c.call(s, "Page.getNavigationHistory", json!({}), &mut events)?;
                    let at = h["currentIndex"].as_i64().unwrap_or(0);
                    let to = if cmd == Cmd::Back { at - 1 } else { at + 1 };
                    if let Some(e) = h["entries"].as_array().and_then(|e| e.get(to as usize)) {
                        c.send(
                            s,
                            "Page.navigateToHistoryEntry",
                            json!({ "entryId": e["id"] }),
                        )?;
                    }
                }
                Cmd::Press {
                    x,
                    y,
                    button,
                    count,
                }
                | Cmd::Release {
                    x,
                    y,
                    button,
                    count,
                } => {
                    let down = matches!(cmd, Cmd::Press { .. });
                    let mut e = mouse(
                        if down {
                            "mousePressed"
                        } else {
                            "mouseReleased"
                        },
                        x,
                        y,
                        button.name(),
                        count,
                    );
                    e["buttons"] = (if down { button.bit() } else { 0 }).into();
                    c.send(s, "Input.dispatchMouseEvent", e)?;
                }
                Cmd::Move { x, y, held } => {
                    let mut e = mouse(
                        "mouseMoved",
                        x,
                        y,
                        held.map_or("none", MouseButton::name),
                        0,
                    );
                    e["buttons"] = held.map_or(0, MouseButton::bit).into();
                    c.send(s, "Input.dispatchMouseEvent", e)?;
                    cursor_at = Some((x, y));
                }
                Cmd::Dialog { accept, text } => {
                    let mut p = json!({ "accept": accept });
                    if let Some(t) = text {
                        p["promptText"] = t.into();
                    }
                    c.send(s, "Page.handleJavaScriptDialog", p)?;
                }
                Cmd::Copy => {
                    copy_req = Some(c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": SELECTION_JS, "returnByValue": true }),
                    )?);
                }
                Cmd::SelectAll => {
                    c.send(
                        s,
                        "Input.dispatchKeyEvent",
                        json!({ "type": "rawKeyDown", "key": "a", "code": "KeyA",
                            "windowsVirtualKeyCode": 65, "commands": ["selectAll"] }),
                    )?;
                    c.send(
                        s,
                        "Input.dispatchKeyEvent",
                        json!({ "type": "keyUp", "key": "a", "code": "KeyA", "windowsVirtualKeyCode": 65 }),
                    )?;
                }
                Cmd::Wheel { x, y, dx, dy } => {
                    c.send(
                        s,
                        "Input.dispatchMouseEvent",
                        json!({ "type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy }),
                    )?;
                }
                Cmd::Text(t) => {
                    c.send(s, "Input.insertText", json!({ "text": t }))?;
                }
                Cmd::Key(k) => {
                    let (key, code, vk) = match k.as_str() {
                        "enter" => ("Enter", "Enter", 13),
                        "backspace" => ("Backspace", "Backspace", 8),
                        "delete" => ("Delete", "Delete", 46),
                        "tab" => ("Tab", "Tab", 9),
                        "escape" => ("Escape", "Escape", 27),
                        "left" => ("ArrowLeft", "ArrowLeft", 37),
                        "up" => ("ArrowUp", "ArrowUp", 38),
                        "right" => ("ArrowRight", "ArrowRight", 39),
                        "down" => ("ArrowDown", "ArrowDown", 40),
                        "home" => ("Home", "Home", 36),
                        "end" => ("End", "End", 35),
                        "pageup" => ("PageUp", "PageUp", 33),
                        "pagedown" => ("PageDown", "PageDown", 34),
                        _ => continue,
                    };
                    for kind in ["rawKeyDown", "keyUp"] {
                        let mut p = json!({ "type": kind, "key": key, "code": code,
                            "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk });
                        if key == "Enter" && kind == "rawKeyDown" {
                            // Enter also types its character, so forms submit.
                            p["type"] = "keyDown".into();
                            p["text"] = "\r".into();
                        }
                        c.send(s, "Input.dispatchKeyEvent", p)?;
                    }
                }
                Cmd::Show(id) => {
                    if id != tab.target {
                        let old = tab.session.clone();
                        let _ = c.send(Some(&old), "Page.stopScreencast", json!({}));
                        tab = attach(&mut c, &id, metrics, &mut events)?;
                        c.send(None, "Target.activateTarget", json!({ "targetId": id }))?;
                        write_focus(focus, &tab.target);
                        report(&mut c, &mut events, &tab.target)?;
                    }
                }
                Cmd::NewTab(url) => {
                    // The tab shows itself when it is created (below).
                    c.send(None, "Target.createTarget", json!({ "url": url }))?;
                }
                Cmd::Close(id) => {
                    c.send(None, "Target.closeTarget", json!({ "targetId": id }))?;
                }
                Cmd::Resize {
                    width,
                    height,
                    scale,
                } => {
                    metrics = Metrics {
                        width: width.max(200),
                        height: height.max(150),
                        scale: scale.clamp(1.0, 3.0),
                    };
                    let (id, s) = (tab.target.clone(), tab.session.clone());
                    fit(&mut c, &id, &s, metrics, &mut events)?;
                }
            }
        }
        // Ask, now and then, which pointer the page wants under the mouse.
        if let Some((x, y)) = cursor_at
            .filter(|_| cursor_req.is_none() && cursor_asked.elapsed() > Duration::from_millis(80))
        {
            cursor_at = None;
            cursor_asked = Instant::now();
            cursor_req = Some(c.send(
                Some(&tab.session),
                "Runtime.evaluate",
                json!({ "expression": cursor_js(x, y), "returnByValue": true }),
            )?);
        }
        // What the browser said.
        let next = if events.is_empty() {
            c.read()?
        } else {
            Some(events.remove(0))
        };
        let Some(m) = next else { continue };
        if let Some(id) = m["id"].as_u64() {
            let value = m["result"]["result"]["value"].as_str().map(str::to_string);
            if Some(id) == cursor_req {
                cursor_req = None;
                if let Some(v) = value {
                    say(Update::Cursor(v))?;
                }
            } else if Some(id) == copy_req {
                copy_req = None;
                if let Some(v) = value.filter(|v| !v.is_empty()) {
                    say(Update::Copied(v))?;
                }
            }
            continue;
        }
        let method = m["method"].as_str().unwrap_or_default();
        let p = &m["params"];
        let on_tab = m["sessionId"].as_str() == Some(tab.session.as_str());
        match method {
            "Page.javascriptDialogOpening" if on_tab => say(Update::Dialog(Some(PageDialog {
                kind: p["type"].as_str().unwrap_or("alert").to_string(),
                message: p["message"].as_str().unwrap_or_default().to_string(),
                default: p["defaultPrompt"].as_str().unwrap_or_default().to_string(),
            })))?,
            "Page.javascriptDialogClosed" if on_tab => {
                say(Update::Dialog(None))?;
                // The pictures stop while a dialog is up: start them again.
                let (id, s) = (tab.target.clone(), tab.session.clone());
                fit(&mut c, &id, &s, metrics, &mut events)?;
            }
            "Browser.downloadWillBegin" => {
                let guid = p["guid"].as_str().unwrap_or_default().to_string();
                let name = crate::downloads::clean_name(
                    p["suggestedFilename"].as_str().unwrap_or("download"),
                );
                note_download(&log, &format!("started\t{name}"));
                pending.insert(guid, name);
            }
            "Browser.downloadProgress" => {
                let guid = p["guid"].as_str().unwrap_or_default();
                match (p["state"].as_str(), &downloads) {
                    (Some("completed"), Some(dir)) => {
                        if let Some(name) = pending.remove(guid) {
                            let to = crate::downloads::free_path(dir, &name);
                            match std::fs::rename(dir.join(guid), &to) {
                                Ok(()) => {
                                    note_download(&log, &format!("saved\t{}", to.display()));
                                    say(Update::Downloaded(to))?;
                                }
                                Err(_) => {
                                    note_download(&log, &format!("failed\t{name}"));
                                    say(Update::DownloadFailed(name))?;
                                }
                            }
                        }
                    }
                    (Some("canceled"), _) => {
                        if let Some(name) = pending.remove(guid) {
                            note_download(&log, &format!("failed\t{name}"));
                            say(Update::DownloadFailed(name))?;
                        }
                    }
                    _ => {}
                }
            }
            "Page.screencastFrame" if on_tab => {
                c.send(
                    Some(&tab.session),
                    "Page.screencastFrameAck",
                    json!({ "sessionId": p["sessionId"] }),
                )?;
                if let Some(img) = p["data"]
                    .as_str()
                    .and_then(crate::browser::decode_base64)
                    .and_then(|j| decode(&j))
                {
                    say(Update::Frame(img))?;
                }
            }
            "Page.frameStartedLoading" if on_tab => say(Update::Loading(true))?,
            "Page.frameStoppedLoading" if on_tab => say(Update::Loading(false))?,
            // A tab the agent (or a link) opened: show it.
            "Target.targetCreated" if p["targetInfo"]["type"] == "page" => {
                let id = p["targetInfo"]["targetId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                if id != tab.target {
                    let s = tab.session.clone();
                    let _ = c.send(Some(&s), "Page.stopScreencast", json!({}));
                    tab = attach(&mut c, &id, metrics, &mut events)?;
                    write_focus(focus, &tab.target);
                }
                report(&mut c, &mut events, &tab.target)?;
            }
            "Target.targetDestroyed" => {
                if p["targetId"].as_str() == Some(tab.target.as_str()) {
                    let left = pages(&mut c, &mut events)?;
                    let next = match left.last() {
                        Some(t) => t.id.clone(),
                        None => {
                            c.call(
                                None,
                                "Target.createTarget",
                                json!({ "url": "about:blank" }),
                                &mut events,
                            )?;
                            pages(&mut c, &mut events)?
                                .last()
                                .ok_or("no tab")?
                                .id
                                .clone()
                        }
                    };
                    tab = attach(&mut c, &next, metrics, &mut events)?;
                    write_focus(focus, &tab.target);
                }
                report(&mut c, &mut events, &tab.target)?;
            }
            "Target.targetInfoChanged" if p["targetInfo"]["type"] == "page" => {
                report(&mut c, &mut events, &tab.target)?;
            }
            _ => {}
        }
    }
}

/// The text selected on the page, in a field or out of one.
const SELECTION_JS: &str = "(() => { const e = document.activeElement; if (e && typeof e.selectionStart === 'number' && typeof e.value === 'string') return e.value.slice(e.selectionStart, e.selectionEnd); return String(getSelection()); })()";

/// The pointer the page wants at (x, y): its CSS `cursor`, or what a
/// browser shows by default there (a hand on links, a bar on text fields).
fn cursor_js(x: f64, y: f64) -> String {
    format!(
        "(() => {{ const e = document.elementFromPoint({x}, {y}); if (!e) return 'default'; const c = getComputedStyle(e).cursor; if (c && c !== 'auto') return c; if (e.closest('a[href],button,summary,select,label,[role=button],[role=link],[onclick]')) return 'pointer'; if (e.closest('input,textarea,[contenteditable=\"\"],[contenteditable=true]')) return 'text'; return 'default'; }})()"
    )
}

/// One line in the downloads log the agent's tools read.
fn note_download(log: &Path, line: &str) {
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// What a typed address means: a URL as it is, a host given a scheme, and
/// anything else searched for.
pub fn address(typed: &str) -> String {
    let t = typed.trim();
    let scheme_only = [
        "about:",
        "data:",
        "file:",
        "view-source:",
        "chrome:",
        "blob:",
        "javascript:",
    ];
    if t.contains("://") || scheme_only.iter().any(|s| t.starts_with(s)) {
        return t.to_string();
    }
    let host_like = !t.contains(' ') && (t.contains('.') || t.starts_with("localhost"));
    if host_like {
        let local = t.starts_with("localhost") || t.starts_with("127.") || t.starts_with("0.0.0.0");
        format!("{}://{t}", if local { "http" } else { "https" })
    } else {
        let q: String = url::form_urlencoded::byte_serialize(t.as_bytes()).collect();
        format!("https://duckduckgo.com/?q={q}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_addresses_become_urls() {
        assert_eq!(address("example.com"), "https://example.com");
        assert_eq!(address("localhost:3000/a"), "http://localhost:3000/a");
        assert_eq!(address("http://x.test"), "http://x.test");
        assert_eq!(
            address("data:text/html,<p>hi there</p>"),
            "data:text/html,<p>hi there</p>"
        );
        assert_eq!(address("file:///C:/a b.html"), "file:///C:/a b.html");
        assert_eq!(address("rust gpui"), "https://duckduckgo.com/?q=rust+gpui");
    }

    /// Start the browser, open a page, and see it reported and drawn: needs
    /// a Chromium (`FORGE_CHROME` or Playwright's). Clicking and typing into
    /// a page are checked in the app itself (see the README's screenshots).
    #[test]
    #[ignore]
    fn drives_a_real_browser() {
        let exe = find_chromium().expect("no Chromium");
        let dir = std::env::temp_dir().join(format!("forge-web-{}", std::process::id()));
        let (view, rx) = start(&exe, &dir.join("profile"), &dir.join("focus")).unwrap();
        view.send(Cmd::Navigate("data:text/html,<h1>Forge</h1>".into()));
        let (mut frames, mut on_page, mut loaded) = (0, false, false);
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until && !(loaded && frames > 1) {
            match rx.try_recv() {
                Ok(Update::Frame(_)) => frames += 1,
                Ok(Update::Page { url, .. }) => on_page = url.starts_with("data:"),
                Ok(Update::Loading(false)) => loaded |= on_page,
                Ok(Update::Closed(why)) => panic!("{why}"),
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        assert!(loaded, "the page was not reported loaded");
        assert!(frames > 1, "no frames");
        drop(view);
        let _ = std::fs::remove_dir_all(dir);
    }
}
