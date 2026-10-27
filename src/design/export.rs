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
}

impl Format {
    pub const ALL: [Format; 5] = [
        Format::Pdf,
        Format::Png,
        Format::Pptx,
        Format::PptxEditable,
        Format::Html,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Png => "png",
            Format::Pptx => "pptx",
            Format::PptxEditable => "pptx-editable",
            Format::Html => "html",
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
        }
    }

    /// The file it is written to, beside the page.
    fn ext(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Png => "png",
            Format::Pptx | Format::PptxEditable => "pptx",
            Format::Html => "html",
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
        // A page's alert or confirm would hold the export.
        conn.answer_dialogs = Some(false);
        Ok(Chrome {
            _guard: guard,
            conn,
            _profile: profile,
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
        self.conn.call(
            ss,
            "Page.navigate",
            json!({ "url": crate::design::live::file_url(path) }),
            &mut ev,
        )?;
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
            (Format::Pdf, _) => {
                let r = self.call(
                    ss,
                    "Page.printToPDF",
                    json!({ "printBackground": true, "preferCSSPageSize": true }),
                )?;
                let pdf = r["data"]
                    .as_str()
                    .and_then(crate::browser::decode_base64)
                    .ok_or_else(|| "the browser gave no PDF".to_string())?;
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
        shutdown();
        assert!(warm < Duration::from_secs(8), "{warm:?}");
        if std::env::var_os("FORGE_KEEP_EXPORTS").is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
