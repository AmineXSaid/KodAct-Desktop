//! Exports of a design page (`docs/open-design-port.md` D3): PDF, PNG,
//! PowerPoint (pictures, or editable text over a picture) and one
//! self-contained HTML file.
//!
//! The browser work happens in a headless Chromium of its own, so the panel
//! the user watches never changes. It starts on the first export and stays
//! up for the next ones (through `--browser-guard`, so it closes with the
//! app), so after the first an export takes about as long as the page takes
//! to load. A deck's slides are all laid out at once, as for print, and
//! each is captured where it lies: no stepping through them, no waiting on
//! their animations.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::pptx;
use crate::web_view::Conn;

/// What to make of the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    /// A picture of the page, or one per slide of a deck.
    Png,
    /// PowerPoint, each slide a picture.
    Pptx,
    /// PowerPoint, each slide's text as text you can edit.
    PptxEditable,
    /// One HTML file with everything it needs inside it.
    Html,
    /// Open Design's Figma capture (`.od-figma.json`), rebuilt as editable
    /// layers by its Figma plugin (`design-library/figma-plugin`).
    Figma,
}

impl Format {
    pub const ALL: [Format; 6] = [
        Format::Pdf,
        Format::Png,
        Format::Pptx,
        Format::PptxEditable,
        Format::Html,
        Format::Figma,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Png => "png",
            Format::Pptx => "pptx",
            Format::PptxEditable => "pptx-editable",
            Format::Html => "html",
            Format::Figma => "figma",
        }
    }

    pub fn from_slug(s: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.slug() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Pdf => "PDF",
            Format::Png => "PNG",
            Format::Pptx => "PowerPoint (pictures)",
            Format::PptxEditable => "PowerPoint (editable text)",
            Format::Html => "Single HTML file",
            Format::Figma => "Figma layers",
        }
    }

    /// The file it is written to, beside the page.
    fn ext(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Png => "png",
            Format::Pptx | Format::PptxEditable => "pptx",
            Format::Html => "html",
            Format::Figma => "od-figma.json",
        }
    }
}

/// What an export made.
#[derive(Debug)]
pub struct Done {
    /// The file, or for a deck's PNGs their folder.
    pub path: PathBuf,
    /// Slides (or pages) in it; 1 for a page.
    pub count: usize,
    /// What couldn't come along (a single HTML file's missing files).
    pub notes: Vec<String>,
    pub took: Duration,
}

/// How long it took, said simply: `40 ms`, `1.4 s`.
pub fn took(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis().max(1))
    } else {
        format!("{:.1} s", d.as_secs_f32())
    }
}

/// Where an export of `page` as `format` goes: beside it, named after it,
/// never over another file.
pub fn target(page: &Path, format: Format) -> PathBuf {
    let dir = page.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = page
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".into());
    let stem = match format {
        Format::Html => format!("{stem}-single"),
        Format::PptxEditable => format!("{stem}-editable"),
        _ => stem,
    };
    let name = format!("{stem}.{}", format.ext());
    crate::downloads::free_path(&dir, &crate::downloads::clean_name(&name))
}

/// Export `page` as `format` to `to` (a deck's PNGs go in a folder named
/// like `to`, without its extension).
pub fn export(page: &Path, format: Format, to: &Path) -> Result<Done, String> {
    let start = Instant::now();
    let html =
        std::fs::read_to_string(page).map_err(|e| format!("Can't read {}: {e}", page.display()))?;
    if format == Format::Html {
        let r = super::inline::single_file(&html, page, &mut fetch);
        std::fs::write(to, &r.html).map_err(|e| e.to_string())?;
        return Ok(Done {
            path: to.to_path_buf(),
            count: 1,
            notes: r
                .missing
                .into_iter()
                .map(|m| format!("not found, left as a link: {m}"))
                .collect(),
            took: start.elapsed(),
        });
    }
    let mut guard = CHROME.lock().map_err(|_| "the export browser is stuck")?;
    // A browser that has gone (or a connection that broke) starts again once.
    let mut tries = 0;
    let done = loop {
        if guard.is_none() {
            *guard = Some(Chrome::start()?);
        }
        let chrome = guard.as_mut().expect("started");
        match chrome.export(page, format, to) {
            Ok(d) => break d,
            Err(Failure::Page(e)) => return Err(e),
            Err(Failure::Browser(e)) => {
                *guard = None;
                tries += 1;
                if tries > 1 {
                    return Err(e);
                }
            }
        }
    };
    Ok(Done {
        took: start.elapsed(),
        ..done
    })
}

/// Start the export browser now, off the window's thread, so the first
/// export doesn't wait for it.
pub fn warm_up() {
    std::thread::spawn(|| {
        if let Ok(mut g) = CHROME.lock() {
            if g.is_none() {
                *g = Chrome::start().ok();
            }
        }
    });
}

/// What a brand capture made.
pub struct Captured {
    pub folder: PathBuf,
    pub brand: super::capture::Brand,
    /// The site as captured (PNG), also saved as `capture.png`.
    pub picture: Vec<u8>,
}

/// Capture the brand of the site at `url` into `project`'s
/// `brand/<host>/` (`design::capture`): `DESIGN.md`, `tokens.css`, the
/// logo when there is one, and a picture of the site.
pub fn capture(url: &str, project: &Path) -> Result<Captured, String> {
    use super::capture as cap;
    let url = if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    let mut guard = CHROME.lock().map_err(|_| "the export browser is stuck")?;
    if guard.is_none() {
        *guard = Some(Chrome::start()?);
    }
    let chrome = guard.as_mut().expect("started");
    let (target, s) = chrome.open_url(&url, 1440, 900)?;
    let got = (|| {
        let read: cap::Read = chrome
            .eval(&s, cap::READ_JS)?
            .as_str()
            .and_then(|j| serde_json::from_str(j).ok())
            .ok_or("the page couldn't be read")?;
        let picture = chrome.shot(&s, 0.0, 0.0, 1440.0, 900.0)?;
        Ok::<_, String>((read, picture))
    })();
    chrome.close(&target);
    drop(guard);
    let (read, picture) = got?;
    let folder = cap::folder(project, &url);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    // The logo, as the site has it; else its icon.
    let icon = read.icon.clone().map(|src| cap::Logo {
        svg: None,
        src: Some(src),
    });
    match read.logo.as_ref().or(icon.as_ref()) {
        Some(cap::Logo { svg: Some(svg), .. }) => {
            let svg = if svg.contains("xmlns") {
                svg.clone()
            } else {
                svg.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1)
            };
            let _ = std::fs::write(folder.join("logo.svg"), svg);
        }
        Some(cap::Logo { src: Some(src), .. }) if src.starts_with("http") => {
            if let Some(bytes) = fetch(src) {
                let ext = src
                    .rsplit('.')
                    .next()
                    .map(|e| {
                        e.split(['?', '#'])
                            .next()
                            .unwrap_or("png")
                            .to_ascii_lowercase()
                    })
                    .filter(|e| ["png", "svg", "jpg", "jpeg", "webp", "gif"].contains(&e.as_str()))
                    .unwrap_or_else(|| "png".into());
                let _ = std::fs::write(folder.join(format!("logo.{ext}")), bytes);
            }
        }
        _ => {}
    }
    let brand = cap::brand(read, &url);
    std::fs::write(folder.join("DESIGN.md"), cap::design_md(&brand)).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("tokens.css"), cap::tokens_css(&brand))
        .map_err(|e| e.to_string())?;
    std::fs::write(folder.join("capture.png"), &picture).map_err(|e| e.to_string())?;
    Ok(Captured {
        folder,
        brand,
        picture,
    })
}

/// A thumbnail's size: a 1280 by 800 window, at 0.3.
const THUMB_SCALE: f64 = 0.3;

/// Where the thumbnail of `preview` is cached: named for its path, size
/// and time, so an edited preview gets a new one.
pub fn thumb_path(preview: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(preview).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    preview.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .hash(&mut h);
    let dir = crate::profile::dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("design-thumbs");
    Some(dir.join(format!("{:016x}.jpg", h.finish())))
}

/// The thumbnail of `preview` (an HTML page): from the cache, else taken
/// now (about a quarter of a second in a warm browser) and cached.
pub fn thumbnail(preview: &Path) -> Result<PathBuf, String> {
    let to = thumb_path(preview).ok_or_else(|| format!("No file at {}", preview.display()))?;
    if to.is_file() {
        return Ok(to);
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut guard = CHROME.lock().map_err(|_| "the export browser is stuck")?;
    for attempt in 0..2 {
        if guard.is_none() {
            *guard = Some(Chrome::start()?);
        }
        let chrome = guard.as_mut().expect("started");
        match chrome.thumbnail(preview) {
            Ok(jpg) => {
                // Written whole, then named, so a half file is never read.
                let part = to.with_extension("part");
                std::fs::write(&part, jpg).map_err(|e| e.to_string())?;
                std::fs::rename(&part, &to).map_err(|e| e.to_string())?;
                return Ok(to);
            }
            Err(e) if attempt == 1 => return Err(e),
            Err(_) => *guard = None,
        }
    }
    unreachable!()
}

/// A deck's slides as small pictures (192 wide), for the panel's rail:
/// cached by the page's file and date like the gallery's. Empty for a page
/// that isn't a deck.
pub fn slide_thumbs(page: &Path) -> Result<Vec<PathBuf>, String> {
    let base = thumb_path(page).ok_or_else(|| format!("No file at {}", page.display()))?;
    let dir = base.with_extension("slides");
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut have: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jpg"))
            .collect();
        if !have.is_empty() {
            have.sort();
            return Ok(have);
        }
    }
    let mut guard = CHROME.lock().map_err(|_| "the export browser is stuck")?;
    if guard.is_none() {
        *guard = Some(Chrome::start()?);
    }
    let chrome = guard.as_mut().expect("started");
    let (target, s) = chrome.open(page, 1920, 1080)?;
    let got = (|| -> Result<Vec<Vec<u8>>, String> {
        let laid = chrome.eval(
            &s,
            &LAYOUT_JS.replace("__PRINT__", crate::design::live::DECK_PRINT_JS),
        )?;
        if laid.is_null() {
            return Ok(vec![]);
        }
        chrome.call(
            Some(&s),
            "Emulation.setEmulatedMedia",
            json!({ "media": "print" }),
        )?;
        let d: Value = chrome
            .eval(&s, &LAYOUT_JS.replace("__PRINT__", "1"))?
            .as_str()
            .and_then(|j| serde_json::from_str(j).ok())
            .ok_or("the deck couldn't be laid out")?;
        let (w, h) = (
            d["w"].as_f64().unwrap_or(1920.0),
            d["h"].as_f64().unwrap_or(1080.0),
        );
        let mut out = vec![];
        for p in d["slides"].as_array().into_iter().flatten().take(200) {
            let r = chrome.call(
                Some(&s),
                "Page.captureScreenshot",
                json!({ "format": "jpeg", "quality": 80, "captureBeyondViewport": true, "fromSurface": true,
                    "clip": { "x": p[0].as_f64().unwrap_or(0.0), "y": p[1].as_f64().unwrap_or(0.0), "width": w, "height": h, "scale": 192.0 / w } }),
            )?;
            out.push(
                r["data"]
                    .as_str()
                    .and_then(crate::browser::decode_base64)
                    .ok_or("the browser gave no picture")?,
            );
        }
        Ok(out)
    })();
    chrome.close(&target);
    drop(guard);
    let pictures = got?;
    if pictures.is_empty() {
        return Ok(vec![]);
    }
    let part = dir.with_extension("part");
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(|e| e.to_string())?;
    for (i, p) in pictures.iter().enumerate() {
        std::fs::write(part.join(format!("{:03}.jpg", i + 1)), p).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&part, &dir).map_err(|e| e.to_string())?;
    Ok((1..=pictures.len())
        .map(|i| dir.join(format!("{i:03}.jpg")))
        .collect())
}

/// A web address's bytes, for a single HTML file (fonts and CDN files).
fn fetch(url: &str) -> Option<Vec<u8>> {
    let r = ureq::get(url)
        .timeout(Duration::from_secs(15))
        .call()
        .ok()?;
    let mut out = vec![];
    std::io::Read::read_to_end(&mut r.into_reader().take(25 * 1024 * 1024), &mut out).ok()?;
    Some(out)
}

static CHROME: Mutex<Option<Chrome>> = Mutex::new(None);

/// Close the export browser (a test's: the app's closes with the app).
#[cfg(test)]
fn shutdown() {
    if let Ok(mut g) = CHROME.lock() {
        *g = None;
    }
}

/// Why an export failed: the page (say so), or the browser (start another).
enum Failure {
    Page(String),
    Browser(String),
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Browser(e)
    }
}

struct Chrome {
    /// The guard; its stdin held open, closing it (we went) closes it.
    _guard: Child,
    conn: Conn,
    _profile: TempDir,
    /// The tab thumbnails are taken in, kept for the next: target, session.
    thumb_tab: Option<(String, String)>,
}

/// A folder removed when dropped.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = self._guard.kill();
        let _ = self._guard.wait();
    }
}

/// A deck laid out for capture: every slide, one under another, at the
/// size it was designed for. JSON `{w, h, slides: [[x, y], …], notes}`;
/// null for a page that isn't a deck.
const LAYOUT_JS: &str = r#"(() => {
  const n = (__PRINT__);
  if (!n) return null;
  const slides = [...document.getElementsByClassName('forge-print-slide')];
  const r0 = slides[0].getBoundingClientRect();
  return JSON.stringify({
    w: Math.round(r0.width), h: Math.round(r0.height),
    slides: slides.map(s => { const r = s.getBoundingClientRect(); return [r.left + scrollX, r.top + scrollY]; }),
    notes: slides.map(s => [...s.querySelectorAll('aside.notes, .notes, [data-notes]')].map(a => (a.innerText || a.getAttribute('data-notes') || '').trim()).join('\n')),
  });
})()"#;

/// Each slide's text, as boxes for PowerPoint, then the text hidden so the
/// picture under them has none. Blocks with text and no block of text
/// inside them; their box relative to their slide.
const TEXT_JS: &str = r#"(() => {
  const slides = [...document.getElementsByClassName('forge-print-slide')];
  const hex = c => { const m = c.match(/\d+(\.\d+)?/g); if (!m) return '000000'; return m.slice(0, 3).map(v => (+v | 0).toString(16).padStart(2, '0')).join(''); };
  const inlineKinds = new Set(['inline', 'inline-block', 'contents']);
  const isBlock = e => !inlineKinds.has(getComputedStyle(e).display);
  const hasOwnText = e => [...e.childNodes].some(c => c.nodeType === 3 && c.textContent.trim());
  const out = slides.map(slide => {
    const sr = slide.getBoundingClientRect();
    const boxes = [];
    const walk = e => {
      if (e.matches('script, style, svg, canvas, aside.notes, .notes, [data-notes]')) return;
      const cs = getComputedStyle(e);
      if (cs.display === 'none' || cs.visibility === 'hidden') return;
      const blockKids = [...e.children].filter(k => isBlock(k) && (k.innerText || '').trim());
      const text = (e.innerText || '').trim();
      if (text && blockKids.length === 0 && (isBlock(e) || hasOwnText(e)) && e !== slide) {
        const r = e.getBoundingClientRect();
        if (r.width > 1 && r.height > 1) {
          const align = { center: 'ctr', right: 'r', end: 'r', justify: 'just' }[cs.textAlign] || 'l';
          boxes.push({ text, x: r.left - sr.left, y: r.top - sr.top, w: r.width, h: r.height,
            size: parseFloat(cs.fontSize) || 16, bold: (+cs.fontWeight || 400) >= 600, italic: cs.fontStyle === 'italic',
            color: hex(cs.color), font: cs.fontFamily, align });
          e.classList.add('forge-text-out');
        }
        return;
      }
      for (const k of e.children) walk(k);
    };
    walk(slide);
    return boxes;
  });
  const st = document.createElement('style');
  st.id = 'forge-text-out';
  st.textContent = '.forge-text-out, .forge-text-out * { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; text-decoration-color: transparent !important; caret-color: transparent !important; }';
  document.head.appendChild(st);
  return JSON.stringify(out);
})()"#;

impl Chrome {
    fn start() -> Result<Chrome, String> {
        let exe = crate::web_view::find_chromium()
            .ok_or("No Chromium, Chrome or Edge on this computer to export with.")?;
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        let port = crate::web_view::free_port()?;
        let profile = TempDir(
            std::env::temp_dir().join(format!("forge-export-{}-{port}", std::process::id())),
        );
        let mut args = vec![
            "--headless=new".to_string(),
            format!("--remote-debugging-port={port}"),
            "--remote-debugging-address=127.0.0.1".into(),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
            "--hide-scrollbars".into(),
            "--mute-audio".into(),
            "--disable-extensions".into(),
            "--force-device-scale-factor=1".into(),
            format!("--user-data-dir={}", profile.0.display()),
        ];
        if crate::web_view::needs_no_sandbox() {
            args.push("--no-sandbox".into());
        }
        args.push("about:blank".into());
        // Through the guard (this program), which closes the browser when
        // the app goes; a test binary can't be one, so it runs it itself.
        let mut cmd = if cfg!(test) {
            let mut c = Command::new(exe);
            c.args(&args);
            c
        } else {
            let mut c = Command::new(me);
            c.arg("--browser-guard").arg(exe).args(&args);
            c
        };
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut guard = cmd
            .spawn()
            .map_err(|e| format!("The export browser didn't start: {e}"))?;
        // A cold first start can take a while.
        let until = Instant::now() + Duration::from_secs(30);
        let ws = loop {
            let got = ureq::get(&format!("http://127.0.0.1:{port}/json/version"))
                .timeout(Duration::from_secs(2))
                .call()
                .ok()
                .and_then(|r| r.into_json::<Value>().ok())
                .and_then(|v| v["webSocketDebuggerUrl"].as_str().map(str::to_string));
            if let Some(ws) = got {
                break ws;
            }
            if Instant::now() > until || matches!(guard.try_wait(), Ok(Some(_))) {
                let _ = guard.kill();
                return Err("The export browser didn't answer.".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let mut conn = crate::web_view::connect(&ws)?;
        // Waits of its own (no frames to keep up with): a large answer (a
        // PDF, a long page's picture) arrives in one read, not cut by a
        // timeout mid-frame, which the WebSocket reader doesn't survive.
        conn.set_read_timeout(Duration::from_secs(5))?;
        // A page's alert or confirm would hold the export.
        conn.answer_dialogs = Some(false);
        Ok(Chrome {
            _guard: guard,
            conn,
            _profile: profile,
            thumb_tab: None,
        })
    }

    fn call(&mut self, s: Option<&str>, method: &str, params: Value) -> Result<Value, String> {
        let mut ev = vec![];
        self.conn.call(s, method, params, &mut ev)
    }

    fn eval(&mut self, s: &str, js: &str) -> Result<Value, String> {
        let r = self.call(
            Some(s),
            "Runtime.evaluate",
            json!({ "expression": js, "returnByValue": true, "awaitPromise": true }),
        )?;
        if let Some(e) = r.get("exceptionDetails") {
            return Err(format!(
                "the page's script failed: {}",
                e["exception"]["description"]
                    .as_str()
                    .or(e["text"].as_str())
                    .unwrap_or("?")
            ));
        }
        Ok(r["result"]["value"].clone())
    }

    /// A tab on `path`, `width` by `height`, loaded, fonts in.
    fn open(&mut self, path: &Path, width: u32, height: u32) -> Result<(String, String), String> {
        self.open_url(&crate::design::live::file_url(path), width, height)
    }

    /// A tab on `url`, `width` by `height`, loaded, fonts in.
    fn open_url(&mut self, url: &str, width: u32, height: u32) -> Result<(String, String), String> {
        let t = self.call(None, "Target.createTarget", json!({ "url": "about:blank" }))?;
        let target = t["targetId"].as_str().ok_or("no tab")?.to_string();
        let a = self.call(
            None,
            "Target.attachToTarget",
            json!({ "targetId": target, "flatten": true }),
        )?;
        let s = a["sessionId"].as_str().ok_or("no session")?.to_string();
        let ss = Some(s.as_str());
        self.call(ss, "Page.enable", json!({}))?;
        self.call(
            ss,
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
        )?;
        let mut ev = vec![];
        let nav = self
            .conn
            .call(ss, "Page.navigate", json!({ "url": url }), &mut ev)?;
        if let Some(e) = nav["errorText"].as_str() {
            return Err(format!("{url} didn't load: {e}"));
        }
        let until = Instant::now() + Duration::from_secs(20);
        while !ev.iter().any(|m| m["method"] == "Page.loadEventFired") && Instant::now() < until {
            if let Some(m) = self.conn.read()? {
                ev.push(m);
            }
        }
        // Fonts, then two frames for the first layout and scripts.
        let _ = self.eval(
            &s,
            "(document.fonts ? document.fonts.ready : Promise.resolve()).then(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => r(1)))))",
        );
        Ok((target, s))
    }

    /// A small picture of `preview`'s first screen, in the tab kept for it.
    fn thumbnail(&mut self, preview: &Path) -> Result<Vec<u8>, String> {
        let s = match self.thumb_tab.clone() {
            Some((_, s)) => s,
            None => {
                let (target, s) = self.open(preview, 1280, 800)?;
                self.thumb_tab = Some((target, s.clone()));
                return self.thumb_shot(&s);
            }
        };
        let mut ev = vec![];
        let nav = self.conn.call(
            Some(&s),
            "Page.navigate",
            json!({ "url": crate::design::live::file_url(preview) }),
            &mut ev,
        );
        if let Err(e) = nav {
            self.thumb_tab = None;
            return Err(e);
        }
        let until = Instant::now() + Duration::from_secs(8);
        while !ev.iter().any(|m| m["method"] == "Page.loadEventFired") && Instant::now() < until {
            if let Some(m) = self.conn.read()? {
                ev.push(m);
            }
        }
        self.thumb_shot(&s)
    }

    fn thumb_shot(&mut self, s: &str) -> Result<Vec<u8>, String> {
        let _ = self.eval(
            s,
            "Promise.race([(document.fonts ? document.fonts.ready : Promise.resolve()).then(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => r(1))))), new Promise(r => setTimeout(() => r(1), 1200))])",
        );
        let r = self.call(
            Some(s),
            "Page.captureScreenshot",
            json!({ "format": "jpeg", "quality": 82, "fromSurface": true,
                "clip": { "x": 0, "y": 0, "width": 1280, "height": 800, "scale": THUMB_SCALE } }),
        )?;
        r["data"]
            .as_str()
            .and_then(crate::browser::decode_base64)
            .ok_or_else(|| "the browser gave no picture".to_string())
    }

    fn close(&mut self, target: &str) {
        let _ = self.call(None, "Target.closeTarget", json!({ "targetId": target }));
    }

    fn shot(&mut self, s: &str, x: f64, y: f64, w: f64, h: f64) -> Result<Vec<u8>, String> {
        let r = self.call(
            Some(s),
            "Page.captureScreenshot",
            json!({ "format": "png", "captureBeyondViewport": true, "fromSurface": true,
                "clip": { "x": x, "y": y, "width": w, "height": h, "scale": 1 } }),
        )?;
        r["data"]
            .as_str()
            .and_then(crate::browser::decode_base64)
            .ok_or_else(|| "the browser gave no picture".to_string())
    }

    fn export(&mut self, page: &Path, format: Format, to: &Path) -> Result<Done, Failure> {
        // A deck lays out at 1920 (or 1280) wide; a page as on a laptop.
        let (target, s) = self.open(page, 1920, 1080)?;
        let r = self.export_in(&s, page, format, to);
        self.close(&target);
        r
    }

    fn export_in(
        &mut self,
        s: &str,
        page: &Path,
        format: Format,
        to: &Path,
    ) -> Result<Done, Failure> {
        let ss = Some(s);
        let page_err = |e: String| Failure::Page(e);
        // Lay a deck out as for print: every slide, at its size.
        let laid = self.eval(
            s,
            &LAYOUT_JS.replace("__PRINT__", crate::design::live::DECK_PRINT_JS),
        )?;
        let deck: Option<Value> = laid.as_str().and_then(|j| serde_json::from_str(j).ok());
        let title = page
            .file_stem()
            .map(|t| t.to_string_lossy().into_owned())
            .unwrap_or_default();
        let done = |path: PathBuf, count: usize| Done {
            path,
            count,
            notes: vec![],
            took: Duration::ZERO,
        };
        match (format, &deck) {
            // Made without the browser (`export`).
            (Format::Html, _) => Err(Failure::Page("not a browser export".into())),
            (Format::Figma, _) => {
                // A deck's slides all laid out, one under another.
                if deck.is_some() {
                    self.call(
                        ss,
                        "Emulation.setEmulatedMedia",
                        json!({ "media": "print" }),
                    )?;
                }
                let lib = crate::design::root().ok_or_else(|| {
                    Failure::Page("Figma export needs KodAct's design library (its clipper).".into())
                })?;
                let clipper =
                    std::fs::read_to_string(lib.join("clipper/capture.js")).map_err(|e| {
                        Failure::Page(format!("The clipper isn't in the design library: {e}"))
                    })?;
                self.eval(s, &clipper)?;
                let got = self.eval(
                    s,
                    "(() => { const c = window.__odCapture({ includeImages: true }); return JSON.stringify({ ir: c.figmaIr, nodes: c.figmaNodeCount, cut: c.figmaTruncated }); })()",
                )?;
                let got: Value = got
                    .as_str()
                    .and_then(|j| serde_json::from_str(j).ok())
                    .ok_or_else(|| "the capture gave nothing".to_string())?;
                let mut ir = got["ir"].clone();
                if ir.is_null() {
                    return Err(Failure::Page(
                        "The page couldn't be captured for Figma.".into(),
                    ));
                }
                let base = page.parent().map(Path::to_path_buf).unwrap_or_default();
                let missing = inline_images(&mut ir, &base);
                let json = serde_json::to_string(&ir).map_err(|e| page_err(e.to_string()))?;
                std::fs::write(to, json).map_err(|e| page_err(e.to_string()))?;
                let mut d = done(to.to_path_buf(), 1);
                d.notes = missing
                    .into_iter()
                    .map(|m| format!("a picture that couldn't be read, left out: {m}"))
                    .collect();
                if got["cut"].as_bool() == Some(true) {
                    d.notes.push(format!(
                        "The page is large: the capture stopped at {} layers.",
                        got["nodes"]
                    ));
                }
                Ok(d)
            }
            (Format::Pdf, _) => {
                let mut ev = vec![];
                let pdf = crate::web_view::print_pdf(&mut self.conn, ss, &mut ev)?;
                std::fs::write(to, &pdf).map_err(|e| page_err(e.to_string()))?;
                let count = deck
                    .as_ref()
                    .map_or(1, |d| d["slides"].as_array().map_or(1, Vec::len));
                Ok(done(to.to_path_buf(), count))
            }
            (Format::Png, None) => {
                // The page as on a laptop, all of it (to a limit).
                self.call(
                    ss,
                    "Emulation.setDeviceMetricsOverride",
                    json!({ "width": 1280, "height": 800, "deviceScaleFactor": 1, "mobile": false }),
                )?;
                let h = self
                    .eval(s, "Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0)")?
                    .as_f64()
                    .unwrap_or(800.0)
                    .clamp(200.0, 16_000.0);
                let png = self.shot(s, 0.0, 0.0, 1280.0, h)?;
                std::fs::write(to, png).map_err(|e| page_err(e.to_string()))?;
                Ok(done(to.to_path_buf(), 1))
            }
            (Format::Pptx | Format::PptxEditable, None) => Err(Failure::Page(
                "This page isn't a deck (no section.slide): export it as PDF, PNG or HTML.".into(),
            )),
            (_, Some(d)) => {
                self.call(
                    ss,
                    "Emulation.setEmulatedMedia",
                    json!({ "media": "print" }),
                )?;
                // Positions as printed.
                let d: Value = self
                    .eval(s, &LAYOUT_JS.replace("__PRINT__", "1"))?
                    .as_str()
                    .and_then(|j| serde_json::from_str(j).ok())
                    .unwrap_or_else(|| d.clone());
                let (w, h) = (
                    d["w"].as_f64().unwrap_or(1920.0),
                    d["h"].as_f64().unwrap_or(1080.0),
                );
                let at: Vec<(f64, f64)> = d["slides"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| (p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)))
                    .collect();
                let notes: Vec<String> = d["notes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|n| n.as_str().unwrap_or("").to_string())
                    .collect();
                let text: Vec<Vec<pptx::TextBox>> = if format == Format::PptxEditable {
                    self.eval(s, TEXT_JS)?
                        .as_str()
                        .and_then(|j| serde_json::from_str(j).ok())
                        .unwrap_or_default()
                } else {
                    vec![]
                };
                let mut pictures = Vec::with_capacity(at.len());
                for &(x, y) in &at {
                    pictures.push(self.shot(s, x, y, w, h)?);
                }
                match format {
                    Format::Png => {
                        let dir = to.with_extension("");
                        let dir = if dir.exists() {
                            crate::downloads::free_path(
                                dir.parent().unwrap_or(Path::new(".")),
                                &dir.file_name().unwrap_or_default().to_string_lossy(),
                            )
                        } else {
                            dir
                        };
                        std::fs::create_dir_all(&dir).map_err(|e| page_err(e.to_string()))?;
                        for (i, png) in pictures.iter().enumerate() {
                            std::fs::write(dir.join(format!("slide-{:02}.png", i + 1)), png)
                                .map_err(|e| page_err(e.to_string()))?;
                        }
                        Ok(done(dir, pictures.len()))
                    }
                    _ => {
                        let slides: Vec<pptx::Slide> = pictures
                            .into_iter()
                            .enumerate()
                            .map(|(i, png)| pptx::Slide {
                                png,
                                width: w,
                                height: h,
                                text: text.get(i).cloned().unwrap_or_default(),
                                notes: notes.get(i).cloned().unwrap_or_default(),
                            })
                            .collect();
                        let bytes = pptx::write(&title, &slides).map_err(page_err)?;
                        std::fs::write(to, bytes).map_err(|e| page_err(e.to_string()))?;
                        Ok(done(to.to_path_buf(), slides.len()))
                    }
                }
            }
        }
    }
}

/// Put a Figma capture's pictures inside it: each IMAGE fill's `url` read
/// (a file beside the page, or from the web) and replaced by a `dataUri`,
/// as the plugin expects. Returns the ones that couldn't be read (their
/// fills are dropped).
fn inline_images(node: &mut Value, base: &Path) -> Vec<String> {
    let mut missing = vec![];
    fn walk(v: &mut Value, base: &Path, missing: &mut Vec<String>) {
        match v {
            Value::Object(map) => {
                if let Some(Value::Array(fills)) = map.get_mut("fills") {
                    fills.retain_mut(|f| {
                        let Some(url) = f.get("url").and_then(Value::as_str).map(str::to_string)
                        else {
                            return true;
                        };
                        let bytes = if let Some(rest) = url.strip_prefix("data:") {
                            f["dataUri"] = Value::String(format!("data:{rest}"));
                            f.as_object_mut().map(|o| o.remove("url"));
                            return true;
                        } else if let Some(path) = url.strip_prefix("file://") {
                            std::fs::read(super::inline_path(path)).ok()
                        } else if url.starts_with("http") {
                            fetch(&url)
                        } else {
                            std::fs::read(base.join(&url)).ok()
                        };
                        match bytes {
                            Some(b) => {
                                let kind = super::inline::mime_of(&url);
                                f["dataUri"] = Value::String(format!(
                                    "data:{kind};base64,{}",
                                    crate::attach::base64(&b)
                                ));
                                f.as_object_mut().map(|o| o.remove("url"));
                                true
                            }
                            None => {
                                missing.push(url);
                                false
                            }
                        }
                    });
                }
                for (_, child) in map.iter_mut() {
                    walk(child, base, missing);
                }
            }
            Value::Array(items) => {
                for i in items {
                    walk(i, base, missing);
                }
            }
            _ => {}
        }
    }
    walk(node, base, &mut missing);
    missing
}

/// The slide pictures' count and size in a `.pptx`, read back: for tests
/// and for the tool's answer.
pub fn pptx_summary(bytes: &[u8]) -> Result<(usize, usize), String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut pres = String::new();
    z.by_name("ppt/presentation.xml")
        .map_err(|e| e.to_string())?
        .read_to_string(&mut pres)
        .map_err(|e| e.to_string())?;
    let slides = pres.matches("<p:sldId ").count();
    let mut texts = 0;
    for i in 1..=slides {
        let mut x = String::new();
        z.by_name(&format!("ppt/slides/slide{i}.xml"))
            .map_err(|e| e.to_string())?
            .read_to_string(&mut x)
            .map_err(|e| e.to_string())?;
        texts += x.matches("txBox=\"1\"").count();
    }
    Ok((slides, texts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/demo/course-intro.html")
    }

    #[test]
    fn formats_round_trip_and_name_their_files() {
        for f in Format::ALL {
            assert_eq!(Format::from_slug(f.slug()), Some(f));
        }
        let dir = std::env::temp_dir().join(format!("forge-target-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("deck.html");
        assert_eq!(target(&page, Format::Pdf), dir.join("deck.pdf"));
        assert_eq!(target(&page, Format::Html), dir.join("deck-single.html"));
        assert_eq!(
            target(&page, Format::PptxEditable),
            dir.join("deck-editable.pptx")
        );
        std::fs::write(dir.join("deck.pdf"), b"x").unwrap();
        assert_ne!(target(&page, Format::Pdf), dir.join("deck.pdf"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_single_html_file_of_the_demo_deck_needs_nothing_else() {
        let dir = std::env::temp_dir().join(format!("forge-single-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let to = dir.join("deck.html");
        let d = export(&deck(), Format::Html, &to).unwrap();
        let html = std::fs::read_to_string(&d.path).unwrap();
        assert!(super::super::inline::outside_refs(&html).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every format from a real headless Chromium: run where one is
    /// installed (`cargo test -- --ignored design::export`).
    #[test]
    #[ignore]
    fn the_demo_deck_exports_to_every_format_fast() {
        let dir = std::env::temp_dir().join(format!("forge-export-t-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = export(&deck(), Format::Pdf, &dir.join("deck.pdf")).unwrap();
        let pdf = std::fs::read(&first.path).unwrap();
        let pages = lopdf::Document::load_mem(&pdf).unwrap().get_pages().len();
        assert_eq!(pages, first.count, "a page a slide");
        assert!(first.count >= 6, "{first:?}");
        let n = first.count;
        let t = Instant::now();
        let pptx = export(&deck(), Format::Pptx, &dir.join("deck.pptx")).unwrap();
        assert_eq!(
            pptx_summary(&std::fs::read(&pptx.path).unwrap()).unwrap(),
            (n, 0)
        );
        let ed = export(&deck(), Format::PptxEditable, &dir.join("deck-e.pptx")).unwrap();
        let (slides, texts) = pptx_summary(&std::fs::read(&ed.path).unwrap()).unwrap();
        assert_eq!(slides, n);
        assert!(texts >= n, "text boxes: {texts}");
        let png = export(&deck(), Format::Png, &dir.join("deck.png")).unwrap();
        assert_eq!(png.count, n);
        let one = image::open(png.path.join("slide-01.png")).unwrap();
        // At the size the deck was designed for, 16:9.
        assert!([1280, 1920].contains(&one.width()), "{}", one.width());
        assert_eq!(one.width() * 9, one.height() * 16);
        // Warm: three exports in well under the time of a cold start.
        eprintln!("first {:?}; warm three {:?}", first.took, t.elapsed());
        let warm = t.elapsed();
        // Thumbnails: taken once, then from the cache.
        let t = Instant::now();
        let th = thumbnail(&deck()).unwrap();
        let first_thumb = t.elapsed();
        let pic = image::open(&th).unwrap();
        assert_eq!((pic.width(), pic.height()), (384, 240));
        let t = Instant::now();
        assert_eq!(thumbnail(&deck()).unwrap(), th);
        assert!(t.elapsed() < Duration::from_millis(50));
        eprintln!("a thumbnail {first_thumb:?}");
        let _ = std::fs::remove_file(&th);
        // The rail's pictures: one per slide, then from the cache.
        let rail = slide_thumbs(&deck()).unwrap();
        assert_eq!(rail.len(), n);
        assert_eq!(image::open(&rail[0]).unwrap().width(), 192);
        let t = Instant::now();
        assert_eq!(slide_thumbs(&deck()).unwrap(), rail);
        assert!(t.elapsed() < Duration::from_millis(50));
        let _ = std::fs::remove_dir_all(rail[0].parent().unwrap());
        // Figma: the plugin's IR, every picture inside it.
        let fig = export(&deck(), Format::Figma, &dir.join("deck.od-figma.json")).unwrap();
        let ir: Value = serde_json::from_str(&std::fs::read_to_string(&fig.path).unwrap()).unwrap();
        assert_eq!(ir["version"], 1);
        assert_eq!(ir["root"]["type"], "FRAME");
        let text = ir.to_string();
        assert!(text.contains("\"type\":\"TEXT\""));
        assert!(text.contains("Design Systems 101"));
        fn linked(v: &Value) -> bool {
            match v {
                Value::Object(m) => {
                    m.get("fills").and_then(Value::as_array).is_some_and(|f| {
                        f.iter()
                            .any(|p| p["type"] == "IMAGE" && p.get("url").is_some())
                    }) || m.values().any(linked)
                }
                Value::Array(a) => a.iter().any(linked),
                _ => false,
            }
        }
        assert!(!linked(&ir["root"]), "a picture left as a link");
        // A brand, captured from the demo deck as if it were a site.
        let c = capture(&crate::design::live::file_url(&deck()), &dir).unwrap();
        let md = std::fs::read_to_string(c.folder.join("DESIGN.md")).unwrap();
        assert!(md.contains("## 2. Color Palette & Roles"), "{md}");
        assert!(c.brand.accent.is_some(), "{:?}", c.brand);
        assert!(c.folder.join("tokens.css").is_file() && c.folder.join("capture.png").is_file());
        eprintln!("captured: {:?}", c.brand);
        shutdown();
        assert!(warm < Duration::from_secs(8), "{warm:?}");
        if std::env::var_os("KODACT_KEEP_EXPORTS").is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
