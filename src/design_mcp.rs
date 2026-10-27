//! Forge's design tools for ForgeCLI's agents (`docs/open-design-port.md`
//! §5.3): an MCP server (stdio, JSON-RPC) served by this same executable,
//! `forge-gpui --design-mcp <devtools-url> <focus-file>`, as the browser
//! tools are (`browser_mcp.rs`). They replace Open Design's `od` CLI:
//!
//! - `design_search` and `design_read` find and read the design library
//!   (`design::library`): skills, templates, design systems, craft rules
//!   and the task profiles;
//! - `design_render` shows the agent a page, or slides n to m, from the
//!   panel's own tab, so it checks exactly what the user is watching;
//! - `design_lint` checks a page's source (`design::lint`) and, when the
//!   panel shows it, what overflows on screen.
//!
//! With no browser (an empty devtools address), render says so and lint
//! checks the source alone.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::browser_mcp::{Answer, Browser};
use crate::design::library::{Entry, Index, Kind};
use crate::design::{lint, live, prompt};

/// The server's name: its tools reach the model as `mcp__forge-design__…`.
pub const SERVER: &str = "forge-design";

/// The most of a library file `design_read` returns at once.
const READ_MAX: usize = 60_000;
/// The most slides one render shows.
const SLIDES_MAX: usize = 12;

fn tools() -> Vec<Value> {
    let kinds: Vec<&str> = Kind::ALL
        .iter()
        .map(|k| k.slug())
        .chain(["profile"])
        .collect();
    let tool = |name: &str, desc: &str, props: Value, required: &[&str]| {
        json!({ "name": name, "description": desc,
            "inputSchema": { "type": "object", "properties": props, "required": required } })
    };
    vec![
        tool("design_search", "Search Forge's design library: skills, templates, design systems (looks), craft rules, examples, task profiles. Returns kind, id, title and summary for each match; read one with design_read.",
            json!({ "query": { "type": "string", "description": "Words to look for, e.g. \"course deck\", \"dark editorial\", \"dashboard\"." },
                "kind": { "type": "string", "enum": kinds, "description": "Only this kind." },
                "limit": { "type": "integer", "description": "At most this many (default 12)." } }), &["query"]),
        tool("design_read", "Read an entry of the design library (its SKILL.md, DESIGN.md, craft rule or task profile, then the files in its folder), or one file in it. Give kind and id; add path for a file in the entry's folder. A path alone is a file anywhere in the library.",
            json!({ "kind": { "type": "string", "enum": kinds },
                "id": { "type": "string" },
                "path": { "type": "string", "description": "A file in the entry's folder, or (without kind and id) a path in the library." },
                "offset": { "type": "integer", "description": "Start this many bytes in, to read past a long file's first part." } }), &[]),
        tool("design_render", "Look at a page as the user sees it in Forge's side panel: a picture of the page, or of slides n to m of a deck. Opens or reloads the page in the panel first.",
            json!({ "path": { "type": "string", "description": "The page's HTML file: absolute, or from the project's folder." },
                "slides": { "type": "string", "description": "A slide (\"3\") or a range (\"2-5\"); at most 12 at once. Leave out for the page." },
                "full_page": { "type": "boolean", "description": "The whole page scrolled, not only what fits the panel." } }), &["path"]),
        tool("design_lint", "Check a page before you call it done: placeholder text, a file cut short, broken markup, missing files, unpinned CDN links, a missing title or viewport; and, when the panel shows the page, content that overflows its slide or the window.",
            json!({ "path": { "type": "string", "description": "The page's HTML file: absolute, or from the project's folder." } }), &["path"]),
        tool("design_export", "Export a page for the user: pdf (a deck prints a slide a page), png (the page, or each slide), pptx (each slide a picture), pptx-editable (each slide's text as PowerPoint text boxes over its picture), html (one self-contained file with its CSS, scripts, images and fonts inside), figma (an .od-figma.json the OD Figma Import plugin rebuilds as editable layers). Saved beside the page; returns where.",
            json!({ "path": { "type": "string", "description": "The page's HTML file: absolute, or from the project's folder." },
                "format": { "type": "string", "enum": crate::design::export::Format::ALL.iter().map(|f| f.slug()).collect::<Vec<_>>() } }), &["path", "format"]),
        tool("design_capture", "Capture a brand from a website: loads it and reads what it draws (colours by role and coverage, fonts, type sizes, corners, the primary button, the logo, its own CSS tokens), then writes brand/<host>/DESIGN.md, tokens.css, the logo and capture.png in the project. Use it when the user wants a design in a site's or company's look; then design with that DESIGN.md as the design system.",
            json!({ "url": { "type": "string", "description": "The site, e.g. https://stripe.com" } }), &["url"]),
        tool("design_image", "Make a picture for a design (a hero image, an illustration, a texture) with the user's own picture service (OpenAI's images, or fal), saved as PNG in the project. Describe subject, style, light and composition; say no text in the picture unless it needs some.",
            json!({ "prompt": { "type": "string" },
                "size": { "type": "string", "description": "Width x height, e.g. 1536x1024 (landscape), 1024x1536 (portrait), 1024x1024. Default 1536x1024." },
                "name": { "type": "string", "description": "The file's name, without .png, e.g. hero." },
                "folder": { "type": "string", "description": "Where to save it, from the project's folder: usually beside the page, e.g. designs/launch. Default: assets." } }), &["prompt"]),
        tool("design_media", "Make a short video (fal) or a spoken narration (ElevenLabs) for a design, with the user's own key, saved in the project: a hero loop, a product clip, a deck's voice-over.",
            json!({ "kind": { "type": "string", "enum": ["video", "speech"] },
                "prompt": { "type": "string", "description": "For video, what it shows and how it moves; for speech, the words to say." },
                "name": { "type": "string" },
                "folder": { "type": "string", "description": "Where to save it, from the project's folder. Default: assets." } }), &["kind", "prompt"]),
        tool("design_deploy", "Publish a page on the web (Vercel, with the user's token) as one self-contained file, and return its address. Only when the user asks to publish or share a link.",
            json!({ "path": { "type": "string", "description": "The page's HTML file: absolute, or from the project's folder." },
                "name": { "type": "string", "description": "The site's name, e.g. course-intro." } }), &["path"]),
    ]
}

struct Tools {
    /// The panel's browser; None without one.
    browser: Option<Browser>,
    /// The library, loaded the first time a tool needs it.
    index: Option<Index>,
    /// A remote chat's project: its pages are read from there, and shown
    /// from a copy on this computer.
    remote: Option<crate::remote::Remote>,
}

/// A page a tool works on: where it is (as the agent named it), its HTML,
/// and the file on this computer the panel shows.
struct Page {
    name: String,
    html: String,
    shown: PathBuf,
    /// Read from a remote: its own folder there, for its linked files.
    remote_dir: Option<String>,
}

fn text(s: impl Into<String>) -> Vec<Value> {
    vec![json!({ "type": "text", "text": s.into() })]
}

impl Tools {
    fn index(&mut self) -> Result<&Index, String> {
        if self.index.is_none() {
            let root = crate::design::root().ok_or(
                "Forge's design library isn't installed (no design-library folder beside Forge).",
            )?;
            self.index = Some(Index::load(&root));
        }
        Ok(self.index.as_ref().expect("loaded"))
    }

    fn browser(&mut self) -> Result<&mut Browser, String> {
        self.browser.as_mut().ok_or_else(|| {
            "Forge's browser isn't on, so pages can't be rendered: turn on the web browser (side panel), or read the file back instead."
                .to_string()
        })
    }

    fn call(&mut self, name: &str, a: &Value) -> Answer {
        match name {
            "design_search" => {
                let query = a["query"].as_str().unwrap_or_default();
                let kind = a["kind"].as_str();
                let limit = a["limit"].as_u64().unwrap_or(12).clamp(1, 50) as usize;
                let ix = self.index()?;
                Ok(text(search(ix, query, kind, limit)))
            }
            "design_read" => {
                let offset = a["offset"].as_u64().unwrap_or(0) as usize;
                let (kind, id, path) = (a["kind"].as_str(), a["id"].as_str(), a["path"].as_str());
                let ix = self.index()?;
                read(ix, kind, id, path, offset).map(text)
            }
            "design_render" => {
                self.browser()?;
                let page = self.page(a)?;
                let slides = match a["slides"].as_str().filter(|s| !s.trim().is_empty()) {
                    Some(s) => Some(slide_range(s)?),
                    None => None,
                };
                let full = a["full_page"].as_bool() == Some(true);
                let mut out = self.render(&page.shown, slides, full)?;
                if page.remote_dir.is_some() {
                    out.push(json!({ "type": "text", "text": "Shown from a copy of the remote page: pictures, styles and scripts it loads from beside it on the remote don't show here." }));
                }
                Ok(out)
            }
            "design_lint" => {
                let page = self.page(a)?;
                let mut unchecked = None;
                let findings = match &page.remote_dir {
                    None => lint::check(&page.html, &page.shown),
                    Some(dir) => {
                        let remote = self.remote.clone().expect("a remote page");
                        lint::check_with(&page.html, &mut |files| {
                            crate::remote::exist(&remote.target, dir, files, &Default::default())
                                .unwrap_or_else(|e| {
                                    unchecked = Some(format!("{e:?}"));
                                    vec![true; files.len()]
                                })
                        })
                    }
                };
                let mut lines: Vec<String> = findings.iter().map(|f| f.to_string()).collect();
                if let Some(why) = unchecked {
                    lines.push(format!("warning: the files the page links couldn't be checked on the remote ({why}): check them before calling it done."));
                }
                let (path, name) = (page.shown.clone(), page.name.clone());
                match self.overflow(&path) {
                    Ok(Some(found)) => lines.extend(found.into_iter().map(|f| format!("error, on screen: {f}"))),
                    Ok(None) => lines.push("(Overflow not checked: the panel isn't showing this page. design_render it, then lint again.)".into()),
                    Err(e) => lines.push(format!("(Overflow not checked: {e})")),
                }
                let problems = lines.iter().filter(|l| !l.starts_with('(')).count();
                let head = match problems {
                    0 => format!("{name}: no problems found."),
                    n => format!("{name}: {n} to look at."),
                };
                Ok(text(
                    std::iter::once(head)
                        .chain(lines)
                        .collect::<Vec<_>>()
                        .join("\n"),
                ))
            }
            "design_export" => {
                use crate::design::export::{self, Format};
                let format = a["format"]
                    .as_str()
                    .and_then(Format::from_slug)
                    .ok_or("format is one of pdf, png, pptx, pptx-editable, html")?;
                let page = self.page(a)?;
                if page.remote_dir.is_some() {
                    return Err("Exports work on pages on this computer; this one is on the remote.".into());
                }
                let to = export::target(&page.shown, format);
                let d = export::export(&page.shown, format, &to)?;
                let mut lines = vec![format!(
                    "Saved {} ({}{}) in {}.",
                    d.path.display(),
                    format.label(),
                    if d.count > 1 { format!(", {} slides", d.count) } else { String::new() },
                    export::took(d.took)
                )];
                if format == Format::PptxEditable {
                    if let Ok((_, texts)) = std::fs::read(&d.path).map_err(|e| e.to_string()).and_then(|b| export::pptx_summary(&b)) {
                        lines.push(format!("{texts} text boxes you can edit in PowerPoint."));
                    }
                }
                if format == Format::Html {
                    let left = std::fs::read_to_string(&d.path)
                        .map(|h| crate::design::inline::outside_refs(&h))
                        .unwrap_or_default();
                    if !left.is_empty() {
                        lines.push(format!("Still links outside the file: {}", left.join(", ")));
                    }
                }
                lines.extend(d.notes);
                Ok(text(lines.join("\n")))
            }
            "design_capture" => {
                let url = a["url"]
                    .as_str()
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .ok_or("url is needed")?;
                if self.remote.is_some() {
                    return Err("Capture writes into a project on this computer; this chat's project is on a remote.".into());
                }
                let project = std::env::current_dir().map_err(|e| e.to_string())?;
                let c = crate::design::export::capture(url, &project)?;
                let b = &c.brand;
                let mut lines = vec![format!(
                    "Captured {} into {} (DESIGN.md, tokens.css, capture.png{}).",
                    b.source,
                    c.folder.display(),
                    if ["logo.svg", "logo.png", "logo.jpg", "logo.webp"]
                        .iter()
                        .any(|l| c.folder.join(l).is_file())
                    {
                        ", the logo"
                    } else {
                        ""
                    }
                )];
                lines.push(format!(
                    "Background {}, text {}, accent {}; fonts {}; corners {}.",
                    b.background,
                    b.text,
                    b.accent.as_deref().unwrap_or("none found"),
                    if b.fonts.is_empty() { "none read".into() } else { b.fonts.join(", ") },
                    b.radius.map_or("square".into(), |r| format!("{r}px"))
                ));
                if !b.css_failed.is_empty() {
                    lines.push(format!(
                        "Warning: {} of its stylesheets didn't load{}, so this is the page without its styles, not its brand. Say so to the user rather than designing with it.",
                        b.css_failed.len(),
                        crate::design::capture::named(&b.css_failed)
                    ));
                }
                lines.push("Read the DESIGN.md, check it against the picture, and correct what the reading got wrong before you design with it.".into());
                let mut out = text(lines.join("\n"));
                if let Some(small) = small_jpeg(&c.picture, 960) {
                    out.push(json!({ "type": "image", "data": crate::attach::base64(&small), "mimeType": "image/jpeg" }));
                }
                Ok(out)
            }
            "design_image" => {
                let prompt = a["prompt"]
                    .as_str()
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .ok_or("prompt is needed")?;
                if self.remote.is_some() {
                    return Err("Pictures are saved in a project on this computer; this chat's project is on a remote.".into());
                }
                let size = a["size"].as_str().unwrap_or("1536x1024");
                let name = a["name"].as_str().unwrap_or("image");
                let project = std::env::current_dir().map_err(|e| e.to_string())?;
                let folder = a["folder"].as_str().unwrap_or("assets");
                let dir = crate::design::inline_path(folder);
                let dir = if dir.is_absolute() { dir } else { project.join(dir) };
                let started = std::time::Instant::now();
                let (file, by) = crate::design::services::image(prompt, size, &dir, name)?;
                let mut out = text(format!(
                    "Saved {} ({:?}, {}). Link it from the page by its path relative to the page.",
                    file.display(),
                    by.label(),
                    crate::design::export::took(started.elapsed())
                ));
                if let Some(small) = std::fs::read(&file).ok().and_then(|b| small_jpeg(&b, 640)) {
                    out.push(json!({ "type": "image", "data": crate::attach::base64(&small), "mimeType": "image/jpeg" }));
                }
                Ok(out)
            }
            "design_media" => {
                let kind = a["kind"].as_str().unwrap_or("video");
                let what = a["prompt"]
                    .as_str()
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .ok_or("prompt is needed")?;
                if self.remote.is_some() {
                    return Err("Media is saved in a project on this computer; this chat's project is on a remote.".into());
                }
                let project = std::env::current_dir().map_err(|e| e.to_string())?;
                let dir = crate::design::inline_path(a["folder"].as_str().unwrap_or("assets"));
                let dir = if dir.is_absolute() { dir } else { project.join(dir) };
                let name = a["name"].as_str().unwrap_or(kind);
                let started = std::time::Instant::now();
                let file = match kind {
                    "speech" => crate::design::services::speech(what, &dir, name)?,
                    _ => crate::design::services::video(what, &dir, name)?,
                };
                Ok(text(format!(
                    "Saved {} ({}). Link it from the page by its path relative to the page.",
                    file.display(),
                    crate::design::export::took(started.elapsed())
                )))
            }
            "design_deploy" => {
                let page = self.page(a)?;
                if page.remote_dir.is_some() {
                    return Err("Publishing works on pages on this computer; this one is on the remote.".into());
                }
                let name = a["name"].as_str().map(str::to_string).unwrap_or_else(|| {
                    page.shown
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "forge-design".into())
                });
                let url = crate::design::services::deploy(&page.shown, &name)?;
                Ok(text(format!("Published {} at {url} (one self-contained file).", page.name)))
            }
            _ => Err(format!("no tool {name}")),
        }
    }

    /// The page a tool's `path` names: a file here, or on the remote (read
    /// from there, and copied here for the panel to show).
    fn page(&self, a: &Value) -> Result<Page, String> {
        let raw = a["path"]
            .as_str()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .ok_or("path is needed")?;
        let Some(remote) = &self.remote else {
            let shown = page_path(a)?;
            let html = std::fs::read_to_string(&shown)
                .map_err(|e| format!("No page at {}: {e}", shown.display()))?;
            return Ok(Page {
                name: shown.display().to_string(),
                html,
                shown,
                remote_dir: None,
            });
        };
        let name = if raw.starts_with('/') || raw.starts_with('~') {
            raw.to_string()
        } else {
            format!("{}/{raw}", remote.path.trim_end_matches('/'))
        };
        let bytes =
            crate::remote::read_file(&remote.target, &name, 16 * 1024 * 1024, &Default::default())
                .map_err(|e| format!("Can't read {name} on {}: {e:?}", remote.target.label()))?;
        let html = String::from_utf8_lossy(&bytes).into_owned();
        let dir = name
            .rsplit_once('/')
            .map_or(".".to_string(), |(d, _)| d.to_string());
        let shown = remote_copy(&remote.target.label(), &name);
        if let Some(parent) = shown.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&shown, &html).map_err(|e| e.to_string())?;
        Ok(Page {
            name,
            html,
            shown,
            remote_dir: Some(dir),
        })
    }

    /// Show `path` in the panel (its tab if it has one, reloaded; else a
    /// new one), then take the pictures.
    fn render(&mut self, path: &Path, slides: Option<(usize, usize)>, full: bool) -> Answer {
        let b = self.browser()?;
        if !path.is_file() {
            return Err(format!("No file at {}", path.display()));
        }
        let url = live::file_url(path);
        show(b, &url)?;
        let Some((from, to)) = slides else {
            return b.run_step("screenshot", &json!({ "full_page": full }));
        };
        let count = b.eval(&slide_count_js())?.as_u64().unwrap_or(0) as usize;
        if count == 0 {
            return Err(
                "This page has no slides (no section.slide or .slide): render it without slides."
                    .into(),
            );
        }
        let to = to.min(count);
        if from > to {
            return Err(format!("The deck has {count} slides."));
        }
        let mut out = vec![];
        // Each slide as it ends up, at once: its transitions and entrance
        // animations finish instantly while the pictures are taken.
        b.eval(STILL_JS)?;
        let mut shots = || -> Answer {
            for n in from..=to {
                b.eval(&format!("{} && {SETTLED_JS}", go_to_slide_js(n)))?;
                let shot = b.run_step("screenshot", &json!({}))?;
                out.push(json!({ "type": "text", "text": format!("Slide {n} of {count}:") }));
                out.extend(shot.into_iter().filter(|c| c["type"] == "image"));
            }
            Ok(vec![])
        };
        let r = shots();
        let _ = b.eval(MOVING_JS);
        r?;
        Ok(out)
    }

    /// What overflows on screen, when the panel has a tab on `path`.
    fn overflow(&mut self, path: &Path) -> Result<Option<Vec<String>>, String> {
        let Some(b) = self.browser.as_mut() else {
            return Err("Forge's browser isn't on".into());
        };
        let url = live::file_url(path);
        let Some(tab) = b
            .tabs()?
            .into_iter()
            .find(|t| live::page_key(&t.url) == live::page_key(&url))
        else {
            return Ok(None);
        };
        b.run_step("switch_tab", &json!({ "id": tab.id }))?;
        b.run_step("reload", &json!({}))?;
        let found = b.eval(lint::OVERFLOW_JS)?;
        Ok(Some(
            found
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        ))
    }
}

/// A picture made `width` wide, as a JPEG: small enough for the model.
fn small_jpeg(png: &[u8], width: u32) -> Option<Vec<u8>> {
    let img = image::load_from_memory(png).ok()?;
    let img = img.resize(width, width * 4, image::imageops::FilterType::Triangle);
    let mut out = std::io::Cursor::new(Vec::new());
    img.to_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .ok()?;
    Some(out.into_inner())
}

/// Make the panel show `url`: its own tab, reloaded, or a new tab.
fn show(b: &mut Browser, url: &str) -> Result<(), String> {
    let tab = b
        .tabs()?
        .into_iter()
        .find(|t| live::page_key(&t.url) == live::page_key(url));
    match tab {
        Some(t) => {
            b.run_step("switch_tab", &json!({ "id": t.id }))?;
            b.run_step("reload", &json!({}))?;
        }
        None => {
            b.run_step("new_tab", &json!({ "url": url }))?;
        }
    }
    // Web fonts in and the first frames drawn (not a fixed wait).
    b.eval(SETTLED_JS)?;
    Ok(())
}

/// Resolves once fonts are in and two frames are drawn, or after 1.5 s.
const SETTLED_JS: &str = "Promise.race([(document.fonts ? document.fonts.ready : Promise.resolve()).then(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => r(true))))), new Promise(r => setTimeout(() => r(true), 1500))])";

/// Transitions and animations end at once (for pictures of each slide).
const STILL_JS: &str = "(() => { const st = document.createElement('style'); st.id = 'forge-still'; st.textContent = '*, *::before, *::after { transition-duration: 0s !important; transition-delay: 0s !important; animation-duration: 0s !important; animation-delay: 0s !important; scroll-behavior: auto !important; }'; document.head.appendChild(st); return true; })()";

/// The page moves again.
const MOVING_JS: &str = "document.getElementById('forge-still')?.remove()";

/// How many slides the page has (`live::deck_slides`: an overview's
/// copies, added later in the page, aren't counted).
fn slide_count_js() -> String {
    format!("{}.length", live::deck_slides!())
}

/// Go to slide n (1-based). Slides stacked down the page are scrolled to;
/// a deck that shows one at a time is driven by its own keys (Home, then
/// ArrowRight), as a person would.
fn go_to_slide_js(n: usize) -> String {
    format!(
        r#"(() => {{
  const slides = {};
  const el = slides[{n} - 1];
  if (!el) return false;
  if ({}) {{
    el.scrollIntoView({{ block: 'start' }});
    return true;
  }}
  const key = (k) => document.dispatchEvent(new KeyboardEvent('keydown', {{ key: k, code: k, bubbles: true }}));
  key('Home');
  for (let i = 1; i < {n}; i++) key('ArrowRight');
  return true;
}})()"#,
        live::deck_slides!(),
        live::deck_stacked!()
    )
}

/// `"3"` → (3, 3), `"2-5"` → (2, 5), at most `SLIDES_MAX` slides.
fn slide_range(s: &str) -> Result<(usize, usize), String> {
    let bad = || format!("slides is a number or a range like 2-5, not {s:?}");
    let num = |t: &str| t.trim().parse::<usize>().ok().filter(|n| *n > 0);
    let (from, to) = match s.split_once('-') {
        Some((a, b)) => (num(a).ok_or_else(bad)?, num(b).ok_or_else(bad)?),
        None => {
            let n = num(s).ok_or_else(bad)?;
            (n, n)
        }
    };
    if from > to {
        return Err(bad());
    }
    Ok((from, to.min(from + SLIDES_MAX - 1)))
}

/// Where a remote page is copied to be shown: one folder per page, named
/// for its place, keeping its file name (the tab shows it).
pub fn remote_copy(target: &str, path: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (target, path).hash(&mut h);
    let file = path
        .rsplit('/')
        .next()
        .filter(|f| !f.is_empty())
        .unwrap_or("page.html");
    std::env::temp_dir()
        .join("forge-remote-pages")
        .join(format!("{:016x}", h.finish()))
        .join(file)
}

/// A tool's `path`, absolute or from the project's folder (this server's
/// working folder, as ForgeCLI starts it).
fn page_path(a: &Value) -> Result<PathBuf, String> {
    let p = a["path"]
        .as_str()
        .filter(|p| !p.trim().is_empty())
        .ok_or("path is needed")?;
    let p = Path::new(p.trim());
    Ok(if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    })
}

/// The library's matches for `query`, best first.
fn search(ix: &Index, query: &str, kind: Option<&str>, limit: usize) -> String {
    let words: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let whole = query.trim().to_lowercase().replace(' ', "-");
    let score = |e: &Entry| -> usize {
        let fields: [(&str, usize); 4] = [
            (&e.id, 5),
            (&e.title, 4),
            (&e.description, 1),
            (&e.mode.clone().unwrap_or_default(), 2),
        ];
        let lists = e.tags.iter().chain(&e.triggers);
        // The query naming the entry outright comes first.
        let exact = if e.id == whole || e.name == whole {
            30
        } else {
            0
        };
        exact
            + words
                .iter()
                .map(|w| {
                    fields
                        .iter()
                        .filter(|(f, _)| f.to_lowercase().contains(w.as_str()))
                        .map(|(_, s)| s)
                        .sum::<usize>()
                        + lists
                            .clone()
                            .filter(|t| t.to_lowercase().contains(w.as_str()))
                            .count()
                            * 2
                })
                .sum::<usize>()
    };
    let mut lines: Vec<(usize, String)> = vec![];
    if kind.is_none_or(|k| k == "profile") {
        for (id, _) in prompt::profiles() {
            let s = words.iter().filter(|w| id.contains(w.as_str())).count() * 5
                + if id == whole { 30 } else { 0 };
            if s > 0 || words.is_empty() {
                lines.push((
                    s,
                    format!("profile {id}: Open Design's task profile for this kind of work"),
                ));
            }
        }
    }
    let want = kind.and_then(Kind::from_slug);
    if kind.is_none() || want.is_some() {
        for e in ix.entries.iter().filter(|e| !e.hidden) {
            let Some(k) = e.kind else { continue };
            if want.is_some_and(|w| w != k) {
                continue;
            }
            let s = score(e);
            if s == 0 && !words.is_empty() {
                continue;
            }
            let mut line = format!("{} {}: {}", k.slug(), e.id, e.title);
            let summary: String = e
                .description
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !summary.is_empty() {
                let short: String = summary.chars().take(160).collect();
                line.push_str(" — ");
                line.push_str(&short);
                if summary.chars().count() > 160 {
                    line.push('…');
                }
            }
            if !e.needs.is_empty() {
                line.push_str(&format!(
                    " (needs {})",
                    e.needs.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            if e.catalogue && !e.upstream_shipped {
                line.push_str(" (a link only: not in the library)");
            }
            lines.push((s, line));
        }
    }
    lines.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let total = lines.len();
    if total == 0 {
        return format!("Nothing in the design library matches {query:?}.");
    }
    let mut out: Vec<String> = lines.into_iter().take(limit).map(|(_, l)| l).collect();
    if total > limit {
        out.push(format!(
            "({} more; narrow the query or give a kind)",
            total - limit
        ));
    }
    out.join("\n")
}

/// An entry's main text and folder listing, or one file.
fn read(
    ix: &Index,
    kind: Option<&str>,
    id: Option<&str>,
    path: Option<&str>,
    offset: usize,
) -> Result<String, String> {
    let root = &ix.root;
    let (file, listing_dir) = match (kind, id) {
        (Some("profile"), Some(id)) => {
            let rel = prompt::profiles()
                .into_iter()
                .find(|(p, _)| *p == id)
                .map(|(_, rel)| rel)
                .ok_or_else(|| format!("No profile {id:?}: one of ppt, prototype, marketing, hyperframes, motion-design"))?;
            (root.join(rel), None)
        }
        (Some(kind), Some(id)) => {
            let k = Kind::from_slug(kind).ok_or_else(|| format!("Unknown kind {kind:?}"))?;
            let e = ix
                .get(k, id)
                .ok_or_else(|| format!("No {kind} named {id:?}: try design_search"))?;
            let dir = root.join(&e.dir);
            match path {
                Some(p) => (inside(root, &dir, p)?, None),
                None => {
                    let main = e
                        .skill
                        .as_ref()
                        .map(|s| root.join(s))
                        .ok_or("This entry has no text of its own")?;
                    (main, dir.is_dir().then_some(dir))
                }
            }
        }
        (None, None) => {
            let p = path.ok_or("Give kind and id, or a path in the library")?;
            (inside(root, root, p)?, None)
        }
        _ => return Err("Give both kind and id (or only a path)".into()),
    };
    if file.is_dir() {
        return Ok(listing(root, &file));
    }
    let bytes = std::fs::read(&file).map_err(|e| format!("Can't read {}: {e}", file.display()))?;
    if crate::design::library::is_media(&file) || bytes.contains(&0) {
        return Ok(format!(
            "{} is a {} byte media file; refer to it by its path.",
            file.display(),
            bytes.len()
        ));
    }
    let all = String::from_utf8_lossy(&bytes);
    let mut start = offset.min(all.len());
    while !all.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + READ_MAX).min(all.len());
    while !all.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = format!("{}\n\n{}", file.display(), &all[start..end]);
    if end < all.len() {
        out.push_str(&format!(
            "\n\n[{} more bytes: read again with offset {end}.]",
            all.len() - end
        ));
    }
    if let Some(dir) = listing_dir {
        out.push_str("\n\n");
        out.push_str(&listing(root, &dir));
    }
    Ok(out)
}

/// `rel` inside `base`, never outside the library.
fn inside(root: &Path, base: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_start_matches(['/', '\\']);
    let p = base.join(rel);
    let real = p
        .canonicalize()
        .map_err(|_| format!("No file {rel:?} there"))?;
    let lib = root.canonicalize().map_err(|e| e.to_string())?;
    if !real.starts_with(&lib) {
        return Err("That path is outside the design library".into());
    }
    Ok(real)
}

/// The files in an entry's folder (two levels, at most 60).
fn listing(root: &Path, dir: &Path) -> String {
    let mut files = vec![];
    let mut walk = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = walk.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() && depth < 1 {
                walk.push((p, depth + 1));
            } else if p.is_file() {
                files.push(
                    p.strip_prefix(root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    files.sort();
    let total = files.len();
    files.truncate(60);
    let mut out = format!(
        "Files (read one with design_read and its path):\n{}",
        files.join("\n")
    );
    if total > 60 {
        out.push_str(&format!("\n({} more)", total - 60));
    }
    out
}

/// Answer one JSON-RPC message; None for a notification.
fn handle(t: &mut Tools, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    Some(match msg["method"].as_str().unwrap_or_default() {
        "initialize" => reply(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2024-11-05"),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": SERVER, "version": env!("CARGO_PKG_VERSION") },
            "instructions": "Forge's design library and checks. Search and read the library before designing; render a page to see it as the user does; lint it before calling it done.",
        })),
        "ping" => reply(json!({})),
        "tools/list" => reply(json!({ "tools": tools() })),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or_default();
            let args = msg["params"]["arguments"].clone();
            match t.call(name, &args) {
                Ok(content) => reply(json!({ "content": content, "isError": false })),
                Err(e) => reply(json!({ "content": text(e), "isError": true })),
            }
        }
        m => json!({ "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("no method {m}") } }),
    })
}

/// Serve MCP on stdin and stdout until stdin closes. An empty `devtools`
/// means no browser.
pub fn serve(devtools: &str, focus_file: &Path) {
    let mut t = Tools {
        browser: (!devtools.trim().is_empty()).then(|| Browser::new(devtools, focus_file)),
        index: None,
        remote: None,
    };
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(r) = handle(&mut t, &msg) {
            if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}

/// Serve the design tools over HTTP for a ForgeCLI on `remote` (as the
/// browser tools are, `browser_mcp::serve_http`): pages are read from the
/// remote, the library and the panel's browser are this computer's.
/// Returns the port.
pub fn serve_http(
    devtools: &str,
    focus_file: &Path,
    token: &str,
    remote: crate::remote::Remote,
) -> std::io::Result<u16> {
    let mut t = Tools {
        browser: (!devtools.trim().is_empty()).then(|| Browser::new(devtools, focus_file)),
        index: None,
        remote: Some(remote),
    };
    crate::browser_mcp::serve_json_http(token, move |msg| handle(&mut t, msg))
}

/// The design tools as a remote ForgeCLI reaches them: over HTTP on the
/// remote's `127.0.0.1:<port>`, forwarded back here.
pub fn http_mcp_config(port: u16, token: &str) -> String {
    json!({ "mcpServers": { SERVER: {
        "type": "http",
        "url": format!("http://127.0.0.1:{port}/mcp/{token}"),
    } } })
    .to_string()
}

/// The `--mcp-config` JSON that gives a ForgeCLI the design tools; an
/// empty `devtools` gives them without a browser.
pub fn mcp_config(exe: &Path, devtools: &str, focus: &Path) -> String {
    json!({ "mcpServers": { SERVER: {
        "command": exe.to_string_lossy(),
        "args": ["--design-mcp", devtools, focus.to_string_lossy()],
    } } })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools_without_browser() -> Tools {
        Tools {
            browser: None,
            index: None,
            remote: None,
        }
    }

    fn call(t: &mut Tools, name: &str, args: Value) -> (bool, String) {
        let r = handle(t, &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
        let text = r["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        (r["result"]["isError"] == true, text)
    }

    #[test]
    fn lists_its_tools() {
        let mut t = tools_without_browser();
        let r = handle(
            &mut t,
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
        let names: Vec<_> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "design_search",
                "design_read",
                "design_render",
                "design_lint",
                "design_export",
                "design_capture",
                "design_image",
                "design_media",
                "design_deploy"
            ]
        );
        assert!(handle(
            &mut t,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .is_none());
    }

    #[test]
    fn searches_and_reads_the_library() {
        if crate::design::root().is_none() {
            return;
        }
        let mut t = tools_without_browser();
        let (err, found) = call(
            &mut t,
            "design_search",
            json!({"query": "course deck", "limit": 5}),
        );
        assert!(!err, "{found}");
        assert!(
            found.lines().count() <= 6 && found.contains("course"),
            "{found}"
        );
        let (_, systems) = call(
            &mut t,
            "design_search",
            json!({"query": "apple", "kind": "design-system"}),
        );
        assert!(
            systems.starts_with("design-system apple: Apple"),
            "{systems}"
        );
        let (_, profiles) = call(&mut t, "design_search", json!({"query": "ppt"}));
        assert!(profiles.starts_with("profile ppt"), "{profiles}");

        let (err, apple) = call(
            &mut t,
            "design_read",
            json!({"kind": "design-system", "id": "apple"}),
        );
        assert!(
            !err && apple.contains("DESIGN.md") && apple.contains("Files (read one"),
            "{apple}"
        );
        let (err, ppt) = call(
            &mut t,
            "design_read",
            json!({"kind": "profile", "id": "ppt"}),
        );
        assert!(!err && ppt.contains("Presentation Task Profile"), "{ppt}");
        let (err, craft) = call(&mut t, "design_read", json!({"path": "craft/color.md"}));
        assert!(!err && craft.contains("craft/color.md"), "{craft}");
        // Long files come in parts.
        let (_, part) = call(
            &mut t,
            "design_read",
            json!({"kind": "profile", "id": "motion-design", "offset": 100}),
        );
        assert!(
            !part.contains("# OD Next"),
            "{}",
            &part[..200.min(part.len())]
        );
    }

    #[test]
    fn reads_stay_inside_the_library() {
        if crate::design::root().is_none() {
            return;
        }
        let mut t = tools_without_browser();
        let (err, msg) = call(&mut t, "design_read", json!({"path": "../Cargo.toml"}));
        assert!(err && msg.contains("outside"), "{msg}");
        let (err, msg) = call(
            &mut t,
            "design_read",
            json!({"kind": "skill", "id": "no-such-skill"}),
        );
        assert!(err && msg.contains("design_search"), "{msg}");
    }

    #[test]
    fn lint_without_a_browser_checks_the_source() {
        let dir = std::env::temp_dir().join(format!("forge-design-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("index.html");
        std::fs::write(
            &page,
            format!(
                "<html><body><h1>Lorem ipsum</h1>{}</body></html>",
                "<p>words</p>".repeat(30)
            ),
        )
        .unwrap();
        let mut t = tools_without_browser();
        let (err, msg) = call(
            &mut t,
            "design_lint",
            json!({"path": page.to_string_lossy()}),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!err, "{msg}");
        assert!(
            msg.contains("to look at") && msg.contains("lorem ipsum"),
            "{msg}"
        );
        assert!(msg.contains("Overflow not checked"), "{msg}");
        let (err, msg) = call(
            &mut t,
            "design_render",
            json!({"path": "/no/such/page.html"}),
        );
        assert!(err && msg.contains("browser isn't on"), "{msg}");
    }

    /// Over a real SSH remote (`FORGE_TEST_SSH=user@host:port`, and
    /// `FORGE_TEST_DEVTOOLS` for render): a remote page is linted with its
    /// linked files checked there, and rendered from a copy here.
    #[test]
    #[ignore]
    fn remote_pages_over_ssh() {
        let Ok(host) = std::env::var("FORGE_TEST_SSH") else {
            return;
        };
        let dir = format!("/tmp/forge-remote-design-{}", std::process::id());
        let page = format!(
            "<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width\"><title>R</title></head><body><h1>Remote page</h1><img src=\"here.png\"><img src=\"gone.png\"><p>{}</p></body></html>",
            "Words enough for a page. ".repeat(12)
        );
        let target = crate::remote::Target::Ssh(host);
        let extra = crate::remote::Extra::default();
        crate::remote::run(
            &target,
            "mkdir -p \"$1\" && cat > \"$1/index.html\" && : > \"$1/here.png\"",
            std::slice::from_ref(&dir),
            page.as_bytes(),
            std::time::Duration::from_secs(30),
            &extra,
        )
        .unwrap();
        let devtools = std::env::var("FORGE_TEST_DEVTOOLS").unwrap_or_default();
        let focus = std::env::temp_dir().join("forge-remote-design-focus.txt");
        let port = serve_http(
            &devtools,
            &focus,
            "tok",
            crate::remote::Remote {
                target: target.clone(),
                path: dir.clone(),
            },
        )
        .unwrap();
        let call = |name: &str, args: Value| -> Value {
            use std::io::Read;
            let body = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string();
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                s,
                "POST /mcp/tok HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            serde_json::from_str(out.split("\r\n\r\n").nth(1).unwrap()).unwrap()
        };
        let lint = call("design_lint", json!({"path": "index.html"}));
        let said = lint["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            said.starts_with(&format!("{dir}/index.html: 1 to look at.")),
            "{said}"
        );
        assert!(
            said.contains("isn't there: gone.png") && !said.contains("here.png"),
            "{said}"
        );
        if !devtools.is_empty() {
            let shot = call(
                "design_render",
                json!({"path": format!("{dir}/index.html")}),
            );
            let content = shot["result"]["content"].as_array().unwrap();
            assert!(content.iter().any(|c| c["type"] == "image"), "{shot}");
            assert!(content.iter().any(|c| c["text"]
                .as_str()
                .is_some_and(|t| t.contains("copy of the remote page"))));
        }
        crate::remote::run(
            &target,
            "rm -rf \"$1\"",
            &[dir],
            b"",
            std::time::Duration::from_secs(30),
            &extra,
        )
        .unwrap();
    }

    #[test]
    fn slide_ranges() {
        assert_eq!(slide_range("3"), Ok((3, 3)));
        assert_eq!(slide_range(" 2 - 5 "), Ok((2, 5)));
        assert_eq!(slide_range("1-40"), Ok((1, 12)));
        assert!(
            slide_range("0").is_err() && slide_range("5-2").is_err() && slide_range("x").is_err()
        );
    }

    #[test]
    fn its_config_runs_this_app() {
        let c: Value = serde_json::from_str(&mcp_config(
            Path::new("/app/forge-gpui"),
            "",
            Path::new("/cfg/focus.txt"),
        ))
        .unwrap();
        let s = &c["mcpServers"]["forge-design"];
        assert_eq!(s["command"], "/app/forge-gpui");
        assert_eq!(s["args"], json!(["--design-mcp", "", "/cfg/focus.txt"]));
    }
}
