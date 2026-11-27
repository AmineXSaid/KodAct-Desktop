//! The browser in the side panel: one headless Chromium that KodAct starts,
//! shown live in the panel and driven by the agent at the same time.
//!
//! KodAct starts Chromium itself (Playwright's build when it is installed,
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
/// Room the headless window has beyond the page: its own bar (139 px) and,
/// for a while, a warning bar (53 px) take some of its height.
const WINDOW_SPARE: u32 = 240;

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
    /// A page KodAct is designing (`design::live`), in a tab of its own,
    /// brought to the front: `key` is the file's address. With `draft`, the
    /// page so far is written into the tab as it streams; without, the file
    /// itself is loaded, or reloaded when the tab shows it already (which
    /// keeps the scroll and a deck's slide).
    Design {
        key: String,
        draft: Option<String>,
    },
    /// The panel's size for the page, in CSS pixels, and the screen's
    /// pixels per CSS pixel: the page lays itself out to fit, and its
    /// pictures come at the screen's density.
    Resize {
        width: u32,
        height: u32,
        scale: f64,
    },
    /// Zoom the page, as a browser does (1 is its own size).
    Zoom(f64),
    /// Print the page on screen to a PDF at this path (backgrounds kept,
    /// the page's own paper size when it sets one).
    Pdf(PathBuf),
    /// Which part of the page is at this point (CSS pixels in the panel):
    /// asked now and then as the mouse moves, at once for a click (`pick`).
    Inspect {
        x: f64,
        y: f64,
        pick: bool,
    },
    /// Scroll so this point of the page (CSS pixels from its top left) is
    /// near the top.
    ScrollTo(f64, f64),
    /// Outline every box on the page in this CSS colour (layout guides),
    /// kept through loads and drafts; None takes them off.
    Guides(Option<String>),
    /// Tell the panel the page's headings (`Update::Outline`), now and
    /// after each load, while on.
    Outline(bool),
    /// A deck's next (1) or previous (-1) slide: scrolled to when its
    /// slides are stacked, else by the deck's own arrow keys.
    Slide(i32),
    /// Go to slide n (1-based) of a deck (`live::go_to_slide_js`).
    SlideTo(usize),
    /// Scroll to the page's nth heading (`h1`–`h3` shown, as the outline
    /// lists them): it, not where it was, so a page laid out again since
    /// still lands there.
    ScrollToHeading(usize),
    /// Which part of the page is at this point (CSS pixels from the page's
    /// top left): a new mark's anchor, said as `Update::AnchoredAt`.
    AnchorAt {
        id: u64,
        x: f64,
        y: f64,
    },
    /// The parts (CSS selectors) comments and marks are on: where each is
    /// now is said as `Update::Anchors` whenever it changes (a zoom, a
    /// wider panel, an edit), so they follow their part.
    Anchors(Vec<String>),
    /// A picture of part of the page (CSS pixels from its top left, as
    /// x, y, width, height), with a script run before it (to put something
    /// on the page) and after (to take it off again): `Update::Picture`.
    Picture {
        before: String,
        clip: (f64, f64, f64, f64),
        after: String,
    },
}

/// A heading of the page, for its outline.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Heading {
    /// 1 to 3, from `h1` to `h3`.
    pub level: u8,
    pub text: String,
    /// Where it is, in CSS pixels from the page's top.
    pub y: f64,
}

/// A part of the page, as the comment tool picks it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PagePart {
    pub tag: String,
    /// The start of its text, on one line.
    pub text: String,
    /// A CSS selector that finds it again.
    pub selector: String,
    /// Where it is, in CSS pixels from the page's top left (scrolling
    /// doesn't move it).
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl PagePart {
    /// What it is, in a few words: `h2 · Three things you can use…`.
    pub fn label(&self) -> String {
        let text: String = self.text.chars().take(48).collect();
        // The page as a whole (a click on its background) says so once.
        if self.tag == "page" {
            return self.text.clone();
        }
        if text.is_empty() {
            self.tag.clone()
        } else if text.len() < self.text.len() {
            format!("{} · {}…", self.tag, text.trim_end())
        } else {
            format!("{} · {text}", self.tag)
        }
    }
}

/// What a picture of the page shows: the area it covers, in CSS pixels
/// from the page's top left, and how far the page is scrolled.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameInfo {
    pub area: (f32, f32),
    pub scroll: (f32, f32),
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
    /// As a browser zooms: the page lays out in the panel's size divided
    /// by this, and shows that much larger.
    zoom: f64,
}

impl Metrics {
    /// The page's own size, in its CSS pixels.
    fn layout(&self) -> (u32, u32) {
        (
            (f64::from(self.width) / self.zoom).round() as u32,
            (f64::from(self.height) / self.zoom).round() as u32,
        )
    }
}

/// The zooms the panel offers, smallest first.
pub const ZOOMS: [f64; 6] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0];

/// The next zoom in (`step` 1) or out (-1) from `at`, as browsers step:
/// through these, clamped at the ends.
pub fn zoom_step(at: f64, step: i32) -> f64 {
    const STEPS: [f64; 13] = [
        0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0,
    ];
    if step > 0 {
        STEPS
            .iter()
            .copied()
            .find(|z| *z > at + 0.001)
            .unwrap_or(2.0)
    } else {
        STEPS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < at - 0.001)
            .unwrap_or(0.25)
    }
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
    /// A new picture of the page, decoded for drawing, and what it shows.
    Frame(Arc<RenderImage>, FrameInfo),
    /// The part of the page at a point (`Cmd::Inspect`), if there is one;
    /// `pick` when it was a click.
    Part {
        part: Option<PagePart>,
        pick: bool,
    },
    /// The page's headings, in order (`Cmd::Outline`).
    Outline(Vec<Heading>),
    /// The page is a deck: the slide in view and how many (None: not a
    /// deck). Said when it changes.
    Deck(Option<(usize, usize)>),
    /// The part at a point (`Cmd::AnchorAt`).
    AnchoredAt(u64, Option<PagePart>),
    /// Where the tracked parts are now, each as its selector and box (CSS
    /// pixels from the page's top left); a part not on the page is left out.
    Anchors(Vec<(String, (f64, f64, f64, f64))>),
    /// A PNG of part of the page (`Cmd::Picture`), or why not.
    Picture(Result<Vec<u8>, String>),
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
    /// The page printed to a PDF (`Cmd::Pdf`): its file, or why not.
    Printed(Result<PathBuf, String>),
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
    child: Arc<Mutex<Child>>,
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
        // Closing the guard's stdin closes the browser. The guard is given
        // a moment to do it: killed at once, it would leave the browser
        // running, holding its profile, so the next start fails. Kill
        // makes sure.
        // Waited for on a thread of its own, so the window never stops
        // for it.
        let child = self.child.clone();
        let _ = self.child.lock().map(|mut c| drop(c.stdin.take()));
        std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until
                && child
                    .lock()
                    .is_ok_and(|mut c| matches!(c.try_wait(), Ok(None)))
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = child.lock().map(|mut c| c.kill());
        });
    }
}

/// `kodact-desktop --browser-guard <chromium> <args…>`: run the browser, and
/// close it as soon as stdin closes, which happens when KodAct exits, quits
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
        let _ = watched.lock().map(|mut c| {
            let _ = c.kill();
            // Gone before the guard is, so nothing holds the profile.
            c.wait()
        });
        std::process::exit(0);
    });
    loop {
        if let Ok(Some(status)) = child.lock().map(|mut c| c.try_wait().ok().flatten()) {
            std::process::exit(status.code().unwrap_or(0));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The Chromium KodAct runs: `KODACT_CHROME`, then Playwright's Chromium,
/// then Chrome, then Edge (always there on Windows 10 and 11).
pub fn find_chromium() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KODACT_CHROME").map(PathBuf::from) {
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
pub(crate) fn needs_no_sandbox() -> bool {
    cfg!(target_os = "linux")
        && std::fs::read_to_string("/proc/self/status").is_ok_and(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1))
                == Some("0")
        })
}

/// A free port on this machine for the DevTools server.
pub(crate) fn free_port() -> Result<u16, String> {
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
        // Its pictures are taken at twice the CSS pixels (they follow this,
        // not the page's own density), so a zoomed page, and a screen at
        // 150 %, stay sharp; they are shrunk to the panel's size.
        "--force-device-scale-factor=2".into(),
        format!("--window-size={},{}", VIEWPORT.0, VIEWPORT.1),
        format!("--user-data-dir={}", profile.display()),
    ];
    if needs_no_sandbox() {
        chrome.push("--no-sandbox".into());
    }
    chrome.push("about:blank".into());
    // Through the guard (this program, `--browser-guard`), which closes the
    // browser when KodAct goes, however it goes; tests run it directly.
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
            child: Arc::new(Mutex::new(child)),
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

/// Print the page to PDF, read back in pieces: a large PDF in one message
/// trips the WebSocket reader, so Chrome hands it over as a stream.
pub(crate) fn print_pdf(
    c: &mut Conn,
    s: Option<&str>,
    events: &mut Vec<Value>,
) -> Result<Vec<u8>, String> {
    let r = c.call(
        s,
        "Page.printToPDF",
        json!({ "printBackground": true, "preferCSSPageSize": true, "transferMode": "ReturnAsStream" }),
        events,
    )?;
    let handle = r["stream"]
        .as_str()
        .ok_or("the browser gave no PDF")?
        .to_string();
    let mut out = vec![];
    loop {
        let r = c.call(
            s,
            "IO.read",
            json!({ "handle": handle, "size": 512 * 1024 }),
            events,
        )?;
        let data = r["data"].as_str().unwrap_or("");
        // The last piece can be empty, with end of file.
        if data.is_empty() {
        } else if r["base64Encoded"].as_bool() == Some(true) {
            out.extend(crate::browser::decode_base64(data).ok_or("the PDF came back unreadable")?);
        } else {
            out.extend_from_slice(data.as_bytes());
        }
        if r["eof"].as_bool() != Some(false) {
            break;
        }
    }
    let _ = c.call(s, "IO.close", json!({ "handle": handle }), events);
    if out.is_empty() {
        return Err("the browser gave an empty PDF".into());
    }
    Ok(out)
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
    /// How long a read waits for the next message before saying none came.
    pub(crate) fn set_read_timeout(&self, wait: Duration) -> Result<(), String> {
        self.ws
            .get_ref()
            .set_read_timeout(Some(wait))
            .map_err(|e| e.to_string())
    }

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
    // The page is laid out at the panel's size (below), but after some
    // loads Chromium pictures the whole window instead, with the page at its
    // top left (the frames say so, and the panel draws them to scale). The
    // window is larger than the page, so that picture still holds all of it,
    // and the pictures are never shrunk to fit.
    let (width, height) = m.layout();
    // Pixels per CSS pixel, as drawn: the screen's, times the zoom.
    let density = m.scale * m.zoom;
    if let Ok(w) = c.call(
        None,
        "Browser.getWindowForTarget",
        json!({ "targetId": target }),
        events,
    ) {
        let _ = c.call(
            None,
            "Browser.setWindowBounds",
            json!({ "windowId": w["windowId"], "bounds": { "width": width + WINDOW_SPARE, "height": height + WINDOW_SPARE } }),
            events,
        );
    }
    c.call(
        s,
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": width, "height": height, "deviceScaleFactor": density, "mobile": false }),
        events,
    )?;
    let _ = c.call(s, "Page.stopScreencast", json!({}), events);
    c.call(
        s,
        "Page.startScreencast",
        json!({ "format": "jpeg", "quality": 80,
            "maxWidth": (f64::from(width + WINDOW_SPARE) * density).round() as u32,
            "maxHeight": (f64::from(height + WINDOW_SPARE) * density).round() as u32 }),
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
        zoom: 1.0,
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
    // Pages KodAct is designing, by address: the tab each one has.
    let mut designs: std::collections::HashMap<String, String> = Default::default();
    // A page's draft, before its file exists, is shown from a file of its own.
    let draft_file = crate::design::live::draft_file;
    let mut pending: std::collections::HashMap<String, String> = Default::default();
    // Answers awaited without blocking the loop: the pointer under the
    // mouse, and the selection to copy.
    let (mut cursor_req, mut copy_req) = (None::<u64>, None::<u64>);
    let mut cursor_asked = Instant::now();
    let mut cursor_at: Option<(f64, f64)> = None;
    // The part of the page under the comment tool: asked as the pointer is.
    let (mut part_req, mut pick_req) = (None::<u64>, None::<u64>);
    // Where the comment tool was clicked (viewport CSS pixels), for a
    // click on the page's own background.
    let mut pick_at: Option<(f64, f64)> = None;
    // The page's scroll, as its last picture said.
    let mut frame_scroll = (0.0f64, 0.0f64);
    // Layout guides' colour while they are on; the outline, while asked for.
    let mut guides: Option<String> = None;
    let (mut outline_on, mut outline_req) = (false, None::<u64>);
    // The parts comments and marks are on, where they were last said to
    // be, and the answers awaited: their boxes, and a new mark's part.
    let mut anchors: Vec<String> = vec![];
    let mut anchors_now: Vec<(String, (f64, f64, f64, f64))> = vec![];
    let (mut anchors_req, mut anchors_asked) = (None::<u64>, Instant::now());
    let mut anchor_at_req: std::collections::HashMap<u64, u64> = Default::default();
    // Which slide a deck shows, asked a few times a second.
    let (mut deck_req, mut deck_now) = (None::<u64>, None::<(usize, usize)>);
    let mut deck_asked = Instant::now();
    let mut part_asked = Instant::now();
    let mut part_at: Option<(f64, f64)> = None;
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
        // What the panel asked for. A page's draft that a later one in the
        // same batch replaces is skipped: the page shows the newest.
        let batch: Vec<Cmd> = std::iter::from_fn(|| cmds.try_recv().ok()).collect();
        let newest: std::collections::HashMap<&str, usize> = batch
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Cmd::Design { key, .. } => Some((key.as_str(), i)),
                _ => None,
            })
            .collect();
        let stale: Vec<bool> = batch
            .iter()
            .enumerate()
            .map(|(i, c)| matches!(c, Cmd::Design { key, draft: Some(_) } if newest.get(key.as_str()).is_some_and(|&n| n > i)))
            .collect();
        for (cmd, stale) in batch.into_iter().zip(stale) {
            if stale {
                continue;
            }
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
                Cmd::Design { key, draft } => {
                    // Its tab: the one made for it, else one already on the
                    // file, else a new one, made and shown here, now, so the
                    // draft can't land in another tab.
                    let open = pages(&mut c, &mut events)?;
                    let target = designs
                        .get(&key)
                        .filter(|id| open.iter().any(|t| &t.id == *id))
                        .cloned()
                        .or_else(|| {
                            open.iter()
                                .find(|t| crate::design::live::page_key(&t.url) == key)
                                .map(|t| t.id.clone())
                        });
                    // A blank tab nobody has used (the one the browser
                    // starts with) is taken rather than left beside it.
                    let blank = open.iter().any(|t| {
                        t.id == tab.target
                            && (t.url.is_empty() || t.url == "about:blank")
                            && !designs.values().any(|d| d == &t.id)
                    });
                    let target = match target {
                        Some(id) => id,
                        None if blank => tab.target.clone(),
                        None => {
                            let made = c.call(
                                None,
                                "Target.createTarget",
                                json!({ "url": "about:blank" }),
                                &mut events,
                            )?;
                            made["targetId"].as_str().ok_or("no new tab")?.to_string()
                        }
                    };
                    designs.insert(key.clone(), target.clone());
                    if target != tab.target {
                        let old = tab.session.clone();
                        let _ = c.send(Some(&old), "Page.stopScreencast", json!({}));
                        tab = attach(&mut c, &target, metrics, &mut events)?;
                        c.send(None, "Target.activateTarget", json!({ "targetId": target }))?;
                        write_focus(focus, &tab.target);
                        report(&mut c, &mut events, &tab.target)?;
                    }
                    let s = Some(tab.session.as_str());
                    let on = open
                        .iter()
                        .find(|t| t.id == tab.target)
                        .map(|t| crate::design::live::page_key(&t.url).to_string())
                        .unwrap_or_default();
                    let file = draft_file(&key);
                    let draft_url = crate::design::live::file_url(&file);
                    match draft {
                        // The first draft opens as a file of its own (a
                        // file:// page, so its pictures load); later ones are
                        // written into it, keeping the scroll. A draft that
                        // can't be written is skipped; the file still loads.
                        Some(page) if on != draft_url => {
                            if std::fs::write(&file, page).is_ok() {
                                c.send(s, "Page.navigate", json!({ "url": draft_url }))?;
                            }
                        }
                        Some(page) => {
                            let js = format!(
                                "(()=>{{const y=scrollY;document.open();document.write({});document.close();scrollTo(0,y);}})()",
                                Value::from(page)
                            );
                            c.send(s, "Runtime.evaluate", json!({ "expression": js }))?;
                            // A draft is a new document: its guides again.
                            if guides.is_some() {
                                c.send(
                                    s,
                                    "Runtime.evaluate",
                                    json!({ "expression": guides_js(guides.as_deref()) }),
                                )?;
                            }
                        }
                        None if on == key => {
                            c.send(s, "Page.reload", json!({}))?;
                        }
                        None => {
                            let _ = std::fs::remove_file(&file);
                            c.send(s, "Page.navigate", json!({ "url": key }))?;
                        }
                    }
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
                        ..metrics
                    };
                    let (id, s) = (tab.target.clone(), tab.session.clone());
                    fit(&mut c, &id, &s, metrics, &mut events)?;
                    anchors_asked = Instant::now() - Duration::from_millis(500);
                    if outline_on {
                        outline_req = Some(c.send(
                            Some(s.as_str()),
                            "Runtime.evaluate",
                            json!({ "expression": OUTLINE_JS, "returnByValue": true }),
                        )?);
                    }
                }
                Cmd::Pdf(to) => {
                    // A deck prints a slide a page (`live::DECK_PRINT_JS`).
                    let _ = c.call(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": crate::design::live::DECK_PRINT_JS, "returnByValue": true }),
                        &mut events,
                    );
                    let printed = print_pdf(&mut c, s, &mut events)
                        .and_then(|pdf| std::fs::write(&to, pdf).map_err(|e| e.to_string()))
                        .map(|()| to);
                    let _ = c.call(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": crate::design::live::DECK_UNPRINT_JS }),
                        &mut events,
                    );
                    say(Update::Printed(printed))?;
                }
                Cmd::Inspect { x, y, pick: false } => part_at = Some((x, y)),
                Cmd::Inspect { x, y, pick: true } => {
                    pick_at = Some((x, y));
                    // Its box is where its content is, as its pin's anchor
                    // will say (`Cmd::Anchors`), so the pin stays put.
                    let js = format!(
                        "(() => {{ const j = {}; if (!j) return ''; const p = JSON.parse(j); const e = document.querySelector(p.selector); if (e) [p.x, p.y, p.w, p.h] = ({CONTENT_BOX_JS})(e); return JSON.stringify(p); }})()",
                        part_js(x, y)
                    );
                    pick_req = Some(c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": js, "returnByValue": true }),
                    )?);
                }
                Cmd::Guides(colour) => {
                    guides = colour;
                    c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": guides_js(guides.as_deref()) }),
                    )?;
                }
                Cmd::AnchorAt { id, x, y } => {
                    let js = format!(
                        "(() => {{ const x = {x} - scrollX, y = {y} - scrollY; const j = ({})(); if (!j) return ''; const p = JSON.parse(j); const e = document.querySelector(p.selector); if (e) [p.x, p.y, p.w, p.h] = ({CONTENT_BOX_JS})(e); return JSON.stringify(p); }})()",
                        part_fn_js()
                    );
                    let req = c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": js, "returnByValue": true }),
                    )?;
                    anchor_at_req.insert(req, id);
                }
                Cmd::Anchors(list) => {
                    anchors = list;
                    anchors_now.clear();
                    anchors_asked = Instant::now() - Duration::from_secs(1);
                }
                Cmd::ScrollToHeading(n) => {
                    let js = format!(
                        "(() => {{ const h = [...document.querySelectorAll('h1,h2,h3')].filter(e => e.getClientRects().length && (e.innerText || '').trim())[{n}]; if (h) scrollTo({{ top: Math.max(0, h.getBoundingClientRect().top + scrollY - 120), behavior: 'smooth' }}); }})()"
                    );
                    c.send(s, "Runtime.evaluate", json!({ "expression": js }))?;
                }
                Cmd::SlideTo(n) => {
                    c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": crate::design::live::go_to_slide_js(n) }),
                    )?;
                    deck_asked = Instant::now() - Duration::from_millis(250);
                }
                Cmd::Slide(step) => {
                    c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": crate::design::live::slide_step_js(step) }),
                    )?;
                    // Say where it got to soon, not at the next ask.
                    deck_asked = Instant::now() - Duration::from_millis(250);
                }
                Cmd::Outline(on) => {
                    outline_on = on;
                    if on {
                        outline_req = Some(c.send(
                            s,
                            "Runtime.evaluate",
                            json!({ "expression": OUTLINE_JS, "returnByValue": true }),
                        )?);
                    }
                }
                Cmd::Picture {
                    before,
                    clip,
                    after,
                } => {
                    let _ = c.call(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": before }),
                        &mut events,
                    );
                    let (x, y, w, h) = clip;
                    let shot = c
                        .call(
                            s,
                            "Page.captureScreenshot",
                            json!({ "format": "png", "captureBeyondViewport": true,
                                "clip": { "x": x, "y": y, "width": w, "height": h, "scale": 1 } }),
                            &mut events,
                        )
                        .and_then(|r| {
                            r["data"]
                                .as_str()
                                .and_then(crate::browser::decode_base64)
                                .ok_or_else(|| "the browser gave no picture".to_string())
                        });
                    c.send(s, "Runtime.evaluate", json!({ "expression": after }))?;
                    say(Update::Picture(shot))?;
                }
                Cmd::ScrollTo(x, y) => {
                    let js = format!(
                        "scrollTo({{ left: Math.max(0, {x} - 40), top: Math.max(0, {y} - 80), behavior: 'smooth' }})"
                    );
                    c.send(s, "Runtime.evaluate", json!({ "expression": js }))?;
                }
                Cmd::Zoom(zoom) => {
                    metrics.zoom = zoom.clamp(0.25, 2.0);
                    let (id, s) = (tab.target.clone(), tab.session.clone());
                    fit(&mut c, &id, &s, metrics, &mut events)?;
                    // The page laid out again: where its parts went, soon,
                    // and its headings.
                    anchors_asked = Instant::now() - Duration::from_millis(500);
                    if outline_on {
                        outline_req = Some(c.send(
                            Some(s.as_str()),
                            "Runtime.evaluate",
                            json!({ "expression": OUTLINE_JS, "returnByValue": true }),
                        )?);
                    }
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
        if let Some((x, y)) = part_at
            .filter(|_| part_req.is_none() && part_asked.elapsed() > Duration::from_millis(60))
        {
            part_at = None;
            part_asked = Instant::now();
            part_req = Some(c.send(
                Some(&tab.session),
                "Runtime.evaluate",
                json!({ "expression": part_js(x, y), "returnByValue": true }),
            )?);
        }
        if !anchors.is_empty()
            && anchors_req.is_none()
            && anchors_asked.elapsed() > Duration::from_millis(700)
        {
            anchors_asked = Instant::now();
            anchors_req = Some(c.send(
                Some(&tab.session),
                "Runtime.evaluate",
                json!({ "expression": anchors_js(&anchors), "returnByValue": true }),
            )?);
        }
        if deck_req.is_none() && deck_asked.elapsed() > Duration::from_millis(400) {
            deck_asked = Instant::now();
            deck_req = Some(c.send(
                Some(&tab.session),
                "Runtime.evaluate",
                json!({ "expression": crate::design::live::DECK_AT_JS, "returnByValue": true }),
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
            } else if Some(id) == anchors_req {
                anchors_req = None;
                let boxes: Vec<Option<(f64, f64, f64, f64)>> = value
                    .and_then(|v| serde_json::from_str(&v).ok())
                    .unwrap_or_default();
                let now: Vec<_> = anchors
                    .iter()
                    .zip(boxes)
                    .filter_map(|(sel, b)| b.map(|b| (sel.clone(), b)))
                    .collect();
                if now != anchors_now {
                    anchors_now = now.clone();
                    say(Update::Anchors(now))?;
                }
            } else if let Some(mark) = anchor_at_req.remove(&id) {
                let part = value.and_then(|v| serde_json::from_str::<PagePart>(&v).ok());
                say(Update::AnchoredAt(mark, part))?;
            } else if Some(id) == deck_req {
                deck_req = None;
                let at = value
                    .and_then(|v| serde_json::from_str::<(usize, usize)>(&v).ok())
                    .filter(|(n, of)| *of > 1 && *n >= 1);
                if at != deck_now {
                    deck_now = at;
                    say(Update::Deck(at))?;
                }
            } else if Some(id) == outline_req {
                outline_req = None;
                let heads = value
                    .and_then(|v| serde_json::from_str::<Vec<Heading>>(&v).ok())
                    .unwrap_or_default();
                say(Update::Outline(heads))?;
            } else if Some(id) == part_req || Some(id) == pick_req {
                let pick = Some(id) == pick_req;
                if pick {
                    pick_req = None;
                } else {
                    part_req = None;
                }
                let mut part = value.and_then(|v| serde_json::from_str::<PagePart>(&v).ok());
                // A click on the page's background (its margins, a gap):
                // the comment is on the page as a whole.
                if pick && part.is_none() {
                    if let Some((x, y)) = pick_at.take() {
                        let scroll = frame_scroll;
                        part = Some(PagePart {
                            tag: "page".into(),
                            text: "The whole page".into(),
                            selector: "body".into(),
                            x: x + scroll.0,
                            y: y + scroll.1,
                            w: 0.0,
                            h: 0.0,
                        });
                    }
                }
                say(Update::Part { part, pick })?;
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
                    // The area pictured, in CSS pixels: the page's size,
                    // or, after some loads, the whole window's (see `fit`).
                    let meta = &p["metadata"];
                    let num = |k: &str| meta[k].as_f64().unwrap_or(0.0) as f32;
                    let info = FrameInfo {
                        area: (num("deviceWidth"), num("deviceHeight")),
                        scroll: (num("scrollOffsetX"), num("scrollOffsetY")),
                    };
                    frame_scroll = (f64::from(info.scroll.0), f64::from(info.scroll.1));
                    say(Update::Frame(img, info))?;
                }
            }
            "Page.frameStartedLoading" if on_tab => say(Update::Loading(true))?,
            "Page.frameStoppedLoading" if on_tab => {
                say(Update::Loading(false))?;
                // Where the comments' and marks' parts are on it, soon.
                anchors_asked = Instant::now() - Duration::from_millis(500);
                // A page loaded: its guides again, and its headings.
                let s = Some(tab.session.as_str());
                if guides.is_some() {
                    c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": guides_js(guides.as_deref()) }),
                    )?;
                }
                if outline_on {
                    outline_req = Some(c.send(
                        s,
                        "Runtime.evaluate",
                        json!({ "expression": OUTLINE_JS, "returnByValue": true }),
                    )?);
                }
            }
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

/// The part of the page at (x, y), as JSON for `PagePart` (empty over the
/// page itself): the innermost element there, a selector that finds it
/// again (by id, else tag, two classes and place, up to six levels), the
/// start of its text, and its box on the page.
fn part_js(x: f64, y: f64) -> String {
    format!(
        "(() => {{ const x = {x}, y = {y}; return ({})(); }})()",
        part_fn_js()
    )
}

/// The part of the page at viewport point (`x`, `y`, in scope), as a
/// function: its tag, text, a selector that finds it again, and its box.
fn part_fn_js() -> &'static str {
    r#"() => {
const e = document.elementFromPoint(x, y);
if (!e || e === document.body || e === document.documentElement) return '';
const step = el => {
  if (el.id) return '#' + CSS.escape(el.id);
  let s = el.tagName.toLowerCase();
  for (const c of [...el.classList].slice(0, 2)) s += '.' + CSS.escape(c);
  const p = el.parentElement;
  if (p) {
    const same = [...p.children].filter(c => c.tagName === el.tagName);
    if (same.length > 1) s += ':nth-of-type(' + (same.indexOf(el) + 1) + ')';
  }
  return s;
};
const path = [];
for (let el = e; el && el !== document.body && el !== document.documentElement && path.length < 6; el = el.parentElement) {
  const s = step(el);
  path.unshift(s);
  if (s.startsWith('#')) break;
}
const r = e.getBoundingClientRect();
const text = (e.innerText || e.getAttribute('alt') || e.getAttribute('aria-label') || '').replace(/\s+/g, ' ').trim().slice(0, 160);
return JSON.stringify({ tag: e.tagName.toLowerCase(), text, selector: path.join(' > '), x: r.left + scrollX, y: r.top + scrollY, w: r.width, h: r.height });
}"#
}

/// Where an element's content is (page CSS pixels), as `[x, y, w, h]`: its
/// text's lines together, which a heading's own box (as wide as its
/// container) doesn't show; its box when it has no text.
const CONTENT_BOX_JS: &str = "e => { const r = document.createRange(); r.selectNodeContents(e); let b = r.getBoundingClientRect(); if (!b.width || !b.height) b = e.getBoundingClientRect(); return [b.left + scrollX, b.top + scrollY, b.width, b.height]; }";

/// Where each selector's element's content is now, as JSON: a box, or null
/// when it isn't on the page.
fn anchors_js(selectors: &[String]) -> String {
    format!(
        "JSON.stringify({}.map(s => {{ try {{ const e = document.querySelector(s); return e ? ({CONTENT_BOX_JS})(e) : null; }} catch (_) {{ return null; }} }}))",
        serde_json::to_string(selectors).unwrap_or_else(|_| "[]".into())
    )
}

/// Put layout guides on the page (an outline round every box, in `colour`),
/// or take them off.
fn guides_js(colour: Option<&str>) -> String {
    match colour {
        Some(c) => format!(
            "(() => {{ let s = document.getElementById('forge-guides'); if (!s) {{ s = document.createElement('style'); s.id = 'forge-guides'; (document.head || document.documentElement).appendChild(s); }} s.textContent = '* {{ outline: 1px solid {} !important; outline-offset: -1px !important; }}'; }})()",
            c.replace(['\'', '\\'], "")
        ),
        None => "document.getElementById('forge-guides')?.remove()".to_string(),
    }
}

/// The page's headings, h1 to h3, as JSON for `Heading`: the shown ones,
/// with text, at most 200.
const OUTLINE_JS: &str = r"JSON.stringify([...document.querySelectorAll('h1,h2,h3')].filter(e => e.getClientRects().length).slice(0, 200).map(e => ({ level: +e.tagName[1], text: (e.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 80), y: e.getBoundingClientRect().top + scrollY })).filter(h => h.text))";

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
    fn zoom_steps_as_a_browser_does() {
        assert_eq!(zoom_step(1.0, 1), 1.1);
        assert_eq!(zoom_step(1.0, -1), 0.9);
        assert_eq!(zoom_step(0.6, 1), 0.67);
        assert_eq!(zoom_step(2.0, 1), 2.0);
        assert_eq!(zoom_step(0.25, -1), 0.25);
        // The page lays out in the panel's room over the zoom.
        let m = Metrics {
            width: 511,
            height: 643,
            scale: 1.0,
            zoom: 0.5,
        };
        assert_eq!(m.layout(), (1022, 1286));
        let m = Metrics { zoom: 2.0, ..m };
        assert_eq!(m.layout(), (256, 322));
    }

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
    /// a Chromium (`KODACT_CHROME` or Playwright's). Clicking and typing into
    /// a page are checked in the app itself (see the README's screenshots).
    #[test]
    #[ignore]
    fn drives_a_real_browser() {
        let exe = find_chromium().expect("no Chromium");
        let dir = std::env::temp_dir().join(format!("forge-web-{}", std::process::id()));
        let (view, rx) = start(&exe, &dir.join("profile"), &dir.join("focus")).unwrap();
        view.send(Cmd::Navigate("data:text/html,<h1>KodAct</h1>".into()));
        let (mut frames, mut on_page, mut loaded) = (0, false, false);
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until && !(loaded && frames > 1) {
            match rx.try_recv() {
                Ok(Update::Frame(..)) => frames += 1,
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
