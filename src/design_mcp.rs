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
use std::time::Duration;

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
    ]
}

struct Tools {
    /// The panel's browser; None without one.
    browser: Option<Browser>,
    /// The library, loaded the first time a tool needs it.
    index: Option<Index>,
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
                let path = page_path(a)?;
                let slides = match a["slides"].as_str().filter(|s| !s.trim().is_empty()) {
                    Some(s) => Some(slide_range(s)?),
                    None => None,
                };
                let full = a["full_page"].as_bool() == Some(true);
                self.render(&path, slides, full)
            }
            "design_lint" => {
                let path = page_path(a)?;
                let html = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Can't read {}: {e}", path.display()))?;
                let mut lines: Vec<String> = lint::check(&html, &path)
                    .iter()
                    .map(|f| f.to_string())
                    .collect();
                match self.overflow(&path) {
                    Ok(Some(found)) => lines.extend(found.into_iter().map(|f| format!("error, on screen: {f}"))),
                    Ok(None) => lines.push("(Overflow not checked: the panel isn't showing this page. design_render it, then lint again.)".into()),
                    Err(e) => lines.push(format!("(Overflow not checked: {e})")),
                }
                let problems = lines.iter().filter(|l| !l.starts_with('(')).count();
                let head = match problems {
                    0 => format!("{}: no problems found.", path.display()),
                    n => format!("{}: {n} to look at.", path.display()),
                };
                Ok(text(
                    std::iter::once(head)
                        .chain(lines)
                        .collect::<Vec<_>>()
                        .join("\n"),
                ))
            }
            _ => Err(format!("no tool {name}")),
        }
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
            return b.step("screenshot", &json!({ "full_page": full }));
        };
        let count = b.js(&slide_count_js())?.as_u64().unwrap_or(0) as usize;
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
        for n in from..=to {
            b.js(&go_to_slide_js(n))?;
            std::thread::sleep(Duration::from_millis(450));
            let shot = b.step("screenshot", &json!({}))?;
            out.push(json!({ "type": "text", "text": format!("Slide {n} of {count}:") }));
            out.extend(shot.into_iter().filter(|c| c["type"] == "image"));
        }
        Ok(out)
    }

    /// What overflows on screen, when the panel has a tab on `path`.
    fn overflow(&mut self, path: &Path) -> Result<Option<Vec<String>>, String> {
        let Some(b) = self.browser.as_mut() else {
            return Err("Forge's browser isn't on".into());
        };
        let url = live::file_url(path);
        let Some(tab) = b
            .tab_list()?
            .into_iter()
            .find(|t| live::page_key(&t.url) == live::page_key(&url))
        else {
            return Ok(None);
        };
        b.step("switch_tab", &json!({ "id": tab.id }))?;
        b.step("reload", &json!({}))?;
        let found = b.js(lint::OVERFLOW_JS)?;
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

/// Make the panel show `url`: its own tab, reloaded, or a new tab.
fn show(b: &mut Browser, url: &str) -> Result<(), String> {
    let tab = b
        .tab_list()?
        .into_iter()
        .find(|t| live::page_key(&t.url) == live::page_key(url));
    match tab {
        Some(t) => {
            b.step("switch_tab", &json!({ "id": t.id }))?;
            b.step("reload", &json!({}))?;
        }
        None => {
            b.step("new_tab", &json!({ "url": url }))?;
        }
    }
    // Web fonts and first animations.
    std::thread::sleep(Duration::from_millis(600));
    Ok(())
}

/// The deck's slides: the first group of `section.slide`, `.slide` or
/// `[data-slide]` that share a parent (an overview's copies, added later
/// in the page, aren't counted).
const SLIDES_JS: &str = r#"(() => {
  const all = [...document.querySelectorAll('section.slide, .slide, [data-slide]')];
  if (!all.length) return [];
  const parent = all[0].parentElement;
  return all.filter(s => s.parentElement === parent);
})()"#;

/// How many slides the page has.
fn slide_count_js() -> String {
    format!("{SLIDES_JS}.length")
}

/// Go to slide n (1-based). Slides stacked down the page are scrolled to;
/// a deck that shows one at a time is driven by its own keys (Home, then
/// ArrowRight), as a person would.
fn go_to_slide_js(n: usize) -> String {
    format!(
        r#"(() => {{
  const slides = {SLIDES_JS};
  const el = slides[{n} - 1];
  if (!el) return false;
  const a = slides[0].getBoundingClientRect(), b = (slides[1] || slides[0]).getBoundingClientRect();
  if (slides.length > 1 && Math.abs(b.top - a.top) >= a.height * 0.5) {{
    el.scrollIntoView({{ block: 'start' }});
    return true;
  }}
  const key = (k) => document.dispatchEvent(new KeyboardEvent('keydown', {{ key: k, code: k, bubbles: true }}));
  key('Home');
  for (let i = 1; i < {n}; i++) key('ArrowRight');
  return true;
}})()"#
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
                "design_lint"
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
