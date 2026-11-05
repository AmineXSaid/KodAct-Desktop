//! KodAct's browser tools for KodAct CLI's agents: an MCP server (stdio,
//! JSON-RPC) that drives the browser in the side panel (`web_view.rs`)
//! over the DevTools protocol. KodAct CLI starts it as this same executable,
//! `kodact-desktop --browser-mcp <devtools-url> <focus-file>` (`browser.rs`),
//! so it needs nothing installed and acts on exactly the page you see.
//!
//! The tools cover the whole browser: open pages and tabs; read a page's
//! state as numbered interactive elements, its text or its HTML; click,
//! type, press keys, choose options, hover and scroll; run JavaScript;
//! wait for something to appear; read the console; and take screenshots.
//!
//! The agent and the panel keep to the same tab through the focus file:
//! whichever switches tabs writes the tab's id there, and the other follows.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::web_view::{connect, pages, Conn, TabInfo};

/// The server's name: its tools reach the model as `mcp__kodact-browser__…`.
pub const SERVER: &str = "kodact-browser";

/// Every tool: its name, what it does, and its input's schema.
fn tools() -> Vec<Value> {
    let target = json!({
        "index": { "type": "integer", "description": "The element's number from `state`." },
        "selector": { "type": "string", "description": "A CSS selector, instead of an index." }
    });
    // How to answer a dialog (alert, confirm, prompt) the step makes the
    // page open: an alert is always closed; a question gets Cancel unless
    // this says accept.
    let dialog = json!({ "dialog": { "type": "string", "enum": ["accept", "dismiss"],
        "description": "If the step makes the page ask something (confirm, prompt): accept (OK) or dismiss (Cancel, the default). The answer says what was asked." } });
    let plus = |mut p: Value, extra: &Value| {
        if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        p
    };
    let with = |extra: Value| {
        let mut p = plus(target.clone(), &dialog);
        if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        p
    };
    let tool = |name: &str, desc: &str, props: Value, required: &[&str]| {
        json!({ "name": name, "description": desc,
            "inputSchema": { "type": "object", "properties": props, "required": required } })
    };
    vec![
        tool("navigate", "Open a URL in the current tab (or a new one), and wait for it to load. The page shows live in KodAct's side panel.",
            plus(json!({ "url": { "type": "string" }, "new_tab": { "type": "boolean", "description": "Open it in a new tab." } }), &dialog), &["url"]),
        tool("state", "The current page: its URL, title, tabs, scroll position, and every visible interactive element, numbered. Use the numbers with click, type, select and hover. Numbers change when the page changes: call state again after it does.",
            json!({}), &[]),
        tool("click", "Click an element (by number or CSS selector), or a point on the page (x, y in CSS pixels). Scrolls it into view first.",
            with(json!({ "x": { "type": "number" }, "y": { "type": "number" },
                "double": { "type": "boolean" }, "button": { "type": "string", "enum": ["left", "right", "middle"] } })), &[]),
        tool("type", "Type text into an element (by number or selector), or into whatever has focus. Optionally clear it first, and press Enter after.",
            with(json!({ "text": { "type": "string" }, "clear": { "type": "boolean" }, "submit": { "type": "boolean" } })), &["text"]),
        tool("press", "Press a key, with modifiers: Enter, Tab, Escape, Backspace, Delete, ArrowUp/Down/Left/Right, Home, End, PageUp, PageDown, F1–F12, or a character (\"a\", \"/\").",
            plus(json!({ "key": { "type": "string" }, "modifiers": { "type": "array", "items": { "type": "string", "enum": ["ctrl", "shift", "alt", "meta"] } } }), &dialog), &["key"]),
        tool("select", "Choose an option in a <select>, by its value or its visible text.",
            with(json!({ "value": { "type": "string" } })), &["value"]),
        tool("hover", "Move the mouse over an element (menus, tooltips).", with(json!({})), &[]),
        tool("scroll", "Scroll the page (up, down, left, right; amount in CSS pixels, default one screen), or scroll an element into view.",
            with(json!({ "direction": { "type": "string", "enum": ["up", "down", "left", "right"] }, "amount": { "type": "number" } })), &[]),
        tool("text", "The visible text of the page, or of one element (CSS selector).",
            json!({ "selector": { "type": "string" }, "max_chars": { "type": "integer" } }), &[]),
        tool("html", "The HTML of the page, or of one element (CSS selector).",
            json!({ "selector": { "type": "string" }, "max_chars": { "type": "integer" } }), &[]),
        tool("eval", "Run JavaScript in the page and return its result (awaited if it is a promise, returned as JSON). Full access to the page: read or change the DOM, call its functions, fetch.",
            plus(json!({ "expression": { "type": "string" } }), &dialog), &["expression"]),
        tool("wait", "Wait until a selector is on the page, a text appears, or the URL contains something (default 10 s, at most 60 s).",
            json!({ "selector": { "type": "string" }, "text": { "type": "string" }, "url_contains": { "type": "string" }, "timeout_ms": { "type": "integer" } }), &[]),
        tool("screenshot", "A picture of the page as it looks now (or the whole page, scrolled).",
            json!({ "full_page": { "type": "boolean" } }), &[]),
        tool("console", "The page's console messages and errors since the tab was opened (newest last).",
            json!({ "limit": { "type": "integer" } }), &[]),
        tool("tabs", "The open tabs, and which one is current.", json!({}), &[]),
        tool("new_tab", "Open a new tab (optionally at a URL) and make it current.", json!({ "url": { "type": "string" } }), &[]),
        tool("switch_tab", "Make a tab current (its id from `tabs`); the side panel shows it.", json!({ "id": { "type": "string" } }), &["id"]),
        tool("close_tab", "Close a tab (its id from `tabs`).", json!({ "id": { "type": "string" } }), &["id"]),
        tool("back", "Go back in the current tab's history.", dialog.clone(), &[]),
        tool("forward", "Go forward in the current tab's history.", dialog.clone(), &[]),
        tool("reload", "Reload the current page.", dialog.clone(), &[]),
    ]
}

/// The page's visible interactive elements, numbered (`data-forge-idx`).
const STATE_JS: &str = r#"(() => {
  const sel = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=option],[role=switch],[role=textbox],[role=combobox],[contenteditable=""],[contenteditable=true],[onclick]';
  document.querySelectorAll('[data-forge-idx]').forEach(e => e.removeAttribute('data-forge-idx'));
  const out = []; let i = 0;
  const vw = innerWidth, vh = innerHeight;
  for (const el of document.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) continue;
    const st = getComputedStyle(el);
    if (st.visibility === 'hidden' || st.display === 'none' || st.opacity === '0') continue;
    i += 1; el.setAttribute('data-forge-idx', String(i));
    const text = (el.innerText || el.value || el.getAttribute('aria-label') || el.getAttribute('title') || el.getAttribute('alt') || '').trim().replace(/\s+/g, ' ').slice(0, 80);
    const o = { index: i, tag: el.tagName.toLowerCase() };
    if (el.getAttribute('role')) o.role = el.getAttribute('role');
    if (el.type && el.tagName !== 'BUTTON' && el.tagName !== 'SELECT') o.type = el.type;
    if (text) o.text = text;
    if (el.name) o.name = el.name;
    if (el.placeholder) o.placeholder = el.placeholder;
    if (el.href) o.href = String(el.href).slice(0, 160);
    if (el.type === 'checkbox' || el.type === 'radio') o.checked = el.checked;
    if (el.tagName === 'SELECT') o.options = [...el.options].slice(0, 20).map(x => x.text);
    if (el.disabled) o.disabled = true;
    if (!(r.bottom > 0 && r.top < vh && r.right > 0 && r.left < vw)) o.offscreen = true;
    out.push(o);
    if (i >= 400) break;
  }
  return JSON.stringify({ url: location.href, title: document.title,
    scroll: { x: Math.round(scrollX), y: Math.round(scrollY), page_height: (document.documentElement || document.body || {}).scrollHeight || 0, viewport_height: vh },
    elements: out });
})()"#;

fn element_selector(args: &Value) -> Option<String> {
    if let Some(i) = args["index"].as_u64() {
        return Some(format!("[data-forge-idx=\"{i}\"]"));
    }
    args["selector"].as_str().map(str::to_string)
}

/// A JS string literal for `s`.
fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// The DevTools key fields for a key name.
fn key_info(key: &str) -> Option<(String, String, i64, Option<String>)> {
    let named: &[(&str, &str, i64, Option<&str>)] = &[
        ("Enter", "Enter", 13, Some("\r")),
        ("Tab", "Tab", 9, None),
        ("Escape", "Escape", 27, None),
        ("Backspace", "Backspace", 8, None),
        ("Delete", "Delete", 46, None),
        ("ArrowUp", "ArrowUp", 38, None),
        ("ArrowDown", "ArrowDown", 40, None),
        ("ArrowLeft", "ArrowLeft", 37, None),
        ("ArrowRight", "ArrowRight", 39, None),
        ("Home", "Home", 36, None),
        ("End", "End", 35, None),
        ("PageUp", "PageUp", 33, None),
        ("PageDown", "PageDown", 34, None),
        (" ", "Space", 32, Some(" ")),
    ];
    let k = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => "Enter",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "backspace" => "Backspace",
        "delete" | "del" => "Delete",
        "up" | "arrowup" => "ArrowUp",
        "down" | "arrowdown" => "ArrowDown",
        "left" | "arrowleft" => "ArrowLeft",
        "right" | "arrowright" => "ArrowRight",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "space" | " " => " ",
        _ => "",
    };
    if let Some((k, code, vk, text)) = named.iter().find(|n| n.0 == k) {
        return Some((
            k.to_string(),
            code.to_string(),
            *vk,
            text.map(str::to_string),
        ));
    }
    if let Some(n) = key
        .strip_prefix(['F', 'f'])
        .and_then(|n| n.parse::<i64>().ok())
    {
        if (1..=12).contains(&n) {
            return Some((format!("F{n}"), format!("F{n}"), 111 + n, None));
        }
    }
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let up = c.to_ascii_uppercase();
    let (code, vk) = if c.is_ascii_alphabetic() {
        (format!("Key{up}"), up as i64)
    } else if c.is_ascii_digit() {
        (format!("Digit{c}"), c as i64)
    } else {
        (String::new(), 0)
    };
    Some((c.to_string(), code, vk, Some(c.to_string())))
}

pub(crate) struct Browser {
    http: String,
    focus_file: PathBuf,
    conn: Option<Conn>,
    sessions: HashMap<String, String>,
    current: Option<String>,
    console: HashMap<String, Vec<String>>,
    events: Vec<Value>,
    /// Lines of the downloads log already reported.
    downloads_seen: usize,
}

/// The downloads log's lines (`browser::downloads_log`).
fn download_lines(focus_file: &Path) -> Vec<String> {
    std::fs::read_to_string(crate::browser::downloads_log(focus_file))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// One tool's answer: MCP content blocks, and whether it failed.
pub(crate) type Answer = Result<Vec<Value>, String>;

fn text(s: impl Into<String>) -> Vec<Value> {
    vec![json!({ "type": "text", "text": s.into() })]
}

impl Browser {
    pub(crate) fn new(http: &str, focus_file: &Path) -> Self {
        Self {
            http: http.trim_end_matches('/').to_string(),
            focus_file: focus_file.to_path_buf(),
            conn: None,
            sessions: HashMap::new(),
            current: None,
            console: HashMap::new(),
            events: vec![],
            downloads_seen: download_lines(focus_file).len(),
        }
    }

    /// Run a tool, and add what else happened meanwhile: dialogs the page
    /// opened, a tab a click opened, downloads started or finished.
    /// (The design tools' render and lint use this, `tabs` and `eval`, on
    /// the panel's own tab.)
    pub(crate) fn run_step(&mut self, name: &str, a: &Value) -> Answer {
        let accept = a["dialog"].as_str() == Some("accept");
        if let Some(c) = self.conn.as_mut() {
            c.dialogs.clear();
        }
        let watch_tabs = !matches!(name, "new_tab" | "tabs" | "switch_tab" | "close_tab")
            && a["new_tab"].as_bool() != Some(true);
        let before: Option<Vec<String>> = watch_tabs
            .then(|| self.tabs().ok())
            .flatten()
            .map(|t| t.into_iter().map(|t| t.id).collect());
        if let Ok(c) = self.conn() {
            c.answer_dialogs = Some(accept);
        }
        let mut r = self.run(name, a);
        let mut notes: Vec<String> = self
            .conn
            .as_mut()
            .map(|c| std::mem::take(&mut c.dialogs))
            .unwrap_or_default();
        // A link that opened a new tab: the panel shows it, so it is current.
        if let (Some(before), Ok(now)) = (before, self.tabs()) {
            if let Some(new) = now.iter().rev().find(|t| !before.contains(&t.id)) {
                let id = new.id.clone();
                self.focus(&id);
                self.settle(10);
                let on = self.now_on();
                notes.push(format!("It opened a new tab ({id}), now current. {on}"));
            }
        }
        // A click or a key can start a download the panel hears of a moment
        // later: give it that moment.
        notes.extend(self.new_downloads(matches!(name, "click" | "press" | "type")));
        if notes.is_empty() {
            return r;
        }
        let extra = notes.join("\n");
        match &mut r {
            Ok(content) => content.push(json!({ "type": "text", "text": extra })),
            Err(e) => {
                e.push('\n');
                e.push_str(&extra);
            }
        }
        r
    }

    /// Downloads the panel logged since the last step, said for the agent;
    /// one that started is given a few seconds to finish. `may_start`: the
    /// step could have started one, so wait a moment for it to show.
    fn new_downloads(&mut self, may_start: bool) -> Vec<String> {
        let begun = Instant::now();
        let until = begun + Duration::from_secs(8);
        loop {
            let lines = download_lines(&self.focus_file);
            let fresh = &lines[self.downloads_seen.min(lines.len())..];
            let started = fresh.iter().filter(|l| l.starts_with("started\t")).count();
            let ended = fresh.iter().filter(|l| !l.starts_with("started\t")).count();
            let quiet =
                fresh.is_empty() && may_start && begun.elapsed() < Duration::from_millis(600);
            if (started <= ended && !quiet) || Instant::now() >= until {
                self.downloads_seen = lines.len();
                return fresh
                    .iter()
                    .filter_map(|l| match l.split_once('\t') {
                        Some(("saved", path)) => Some(format!("Downloaded a file: {path}")),
                        Some(("failed", name)) => {
                            Some(format!("The download of {name} did not finish."))
                        }
                        Some(("started", name)) if started > ended => {
                            Some(format!("A download of {name} started and is still going."))
                        }
                        _ => None,
                    })
                    .collect();
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// The DevTools connection, made (and remade) as needed: the browser
    /// may still be starting when the first tool runs.
    fn conn(&mut self) -> Result<&mut Conn, String> {
        if self.conn.is_none() {
            let until = Instant::now() + Duration::from_secs(30);
            let ws =
                loop {
                    let got = ureq::get(&format!("{}/json/version", self.http))
                        .timeout(Duration::from_secs(2))
                        .call()
                        .ok()
                        .and_then(|r| r.into_json::<Value>().ok())
                        .and_then(|v| v["webSocketDebuggerUrl"].as_str().map(str::to_string));
                    match got {
                        Some(ws) => break ws,
                        None if Instant::now() < until => {
                            std::thread::sleep(Duration::from_millis(300))
                        }
                        None => return Err(
                            "KodAct's browser isn't running: open the Browser tab in the side panel"
                                .into(),
                        ),
                    }
                };
            let mut c = connect(&ws)?;
            // A dialog never holds a step: alerts close, questions get Cancel
            // unless the step says accept (`run_step`).
            c.answer_dialogs = Some(false);
            c.call(
                None,
                "Target.setDiscoverTargets",
                json!({ "discover": true }),
                &mut self.events,
            )?;
            self.conn = Some(c);
            self.sessions.clear();
        }
        Ok(self.conn.as_mut().expect("connected"))
    }

    fn call(
        &mut self,
        session: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let mut events = std::mem::take(&mut self.events);
        let r = self.conn()?.call(session, method, params, &mut events);
        self.events = events;
        if r.as_ref().is_err_and(|e| {
            e.contains("Connection") || e.contains("closed") || e.contains("Broken pipe")
        }) {
            self.conn = None;
        }
        self.take_events();
        r
    }

    /// Keep what events say that tools report later: console messages.
    fn take_events(&mut self) {
        let by_session: HashMap<String, String> = self
            .sessions
            .iter()
            .map(|(t, s)| (s.clone(), t.clone()))
            .collect();
        for m in std::mem::take(&mut self.events) {
            let Some(target) = m["sessionId"].as_str().and_then(|s| by_session.get(s)) else {
                continue;
            };
            let p = &m["params"];
            let line = match m["method"].as_str() {
                Some("Runtime.consoleAPICalled") => {
                    let args: Vec<String> = p["args"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|a| match &a["value"] {
                            Value::String(s) => s.clone(),
                            Value::Null => a["description"].as_str().unwrap_or("").to_string(),
                            v => v.to_string(),
                        })
                        .collect();
                    format!(
                        "[{}] {}",
                        p["type"].as_str().unwrap_or("log"),
                        args.join(" ")
                    )
                }
                Some("Runtime.exceptionThrown") => {
                    let d = &p["exceptionDetails"];
                    format!(
                        "[error] {}",
                        d["exception"]["description"]
                            .as_str()
                            .or(d["text"].as_str())
                            .unwrap_or("exception")
                    )
                }
                _ => continue,
            };
            let log = self.console.entry(target.clone()).or_default();
            log.push(line);
            if log.len() > 300 {
                log.remove(0);
            }
        }
    }

    pub(crate) fn tabs(&mut self) -> Result<Vec<TabInfo>, String> {
        let mut events = std::mem::take(&mut self.events);
        let r = pages(self.conn()?, &mut events);
        self.events = events;
        r
    }

    /// The tab the agent acts on: the one the panel shows (the focus file),
    /// else the last one it used, else the newest.
    fn current(&mut self) -> Result<String, String> {
        let tabs = self.tabs()?;
        let focus = std::fs::read_to_string(&self.focus_file)
            .ok()
            .map(|s| s.trim().to_string());
        let pick = focus
            .filter(|f| tabs.iter().any(|t| &t.id == f))
            .or_else(|| {
                self.current
                    .clone()
                    .filter(|c| tabs.iter().any(|t| &t.id == c))
            })
            .or_else(|| tabs.last().map(|t| t.id.clone()));
        let id = match pick {
            Some(id) => id,
            None => self.call(None, "Target.createTarget", json!({ "url": "about:blank" }))?
                ["targetId"]
                .as_str()
                .ok_or("no tab")?
                .to_string(),
        };
        self.current = Some(id.clone());
        Ok(id)
    }

    /// Say which tab is current, so the side panel shows it.
    fn focus(&mut self, id: &str) {
        self.current = Some(id.to_string());
        let tmp = self.focus_file.with_extension("tmp");
        if std::fs::write(&tmp, id).is_ok() {
            let _ = std::fs::rename(&tmp, &self.focus_file);
        }
    }

    /// The DevTools session for a tab, attached once.
    fn session(&mut self, target: &str) -> Result<String, String> {
        if let Some(s) = self.sessions.get(target) {
            return Ok(s.clone());
        }
        let r = self.call(
            None,
            "Target.attachToTarget",
            json!({ "targetId": target, "flatten": true }),
        )?;
        let s = r["sessionId"].as_str().ok_or("no session")?.to_string();
        self.sessions.insert(target.to_string(), s.clone());
        self.call(Some(&s), "Page.enable", json!({}))?;
        self.call(Some(&s), "Runtime.enable", json!({}))?;
        Ok(s)
    }

    fn cur_session(&mut self) -> Result<String, String> {
        let id = self.current()?;
        self.session(&id)
    }

    /// Run JS in the current page; its value, or why it threw.
    pub(crate) fn eval(&mut self, js: &str) -> Result<Value, String> {
        let s = self.cur_session()?;
        let r = self.call(
            Some(&s),
            "Runtime.evaluate",
            json!({ "expression": js, "returnByValue": true, "awaitPromise": true, "userGesture": true }),
        )?;
        if let Some(e) = r.get("exceptionDetails") {
            return Err(e["exception"]["description"]
                .as_str()
                .or(e["text"].as_str())
                .unwrap_or("the script threw")
                .to_string());
        }
        Ok(r["result"]["value"].clone())
    }

    /// Wait for the page to finish loading (up to `secs`).
    fn settle(&mut self, secs: u64) {
        let until = Instant::now() + Duration::from_secs(secs);
        std::thread::sleep(Duration::from_millis(150));
        while Instant::now() < until {
            if self
                .eval("document.readyState")
                .ok()
                .as_ref()
                .and_then(Value::as_str)
                == Some("complete")
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    /// Where the page is now, for the end of an action's answer.
    fn now_on(&mut self) -> String {
        match self.eval("JSON.stringify([location.href, document.title])") {
            Ok(Value::String(s)) => {
                let v: Value = serde_json::from_str(&s).unwrap_or_default();
                format!(
                    "Now on {} — {}",
                    v[0].as_str().unwrap_or(""),
                    v[1].as_str().unwrap_or("")
                )
            }
            _ => String::new(),
        }
    }

    /// The centre of an element, scrolled into view.
    fn point(&mut self, sel: &str) -> Result<(f64, f64), String> {
        let js = format!(
            "(() => {{ const el = document.querySelector({}); if (!el) return null; el.scrollIntoView({{block:'center', inline:'center'}}); const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; }})()",
            js_str(sel)
        );
        match self.eval(&js)? {
            Value::Array(v) if v.len() == 2 => {
                Ok((v[0].as_f64().unwrap_or(0.), v[1].as_f64().unwrap_or(0.)))
            }
            _ if sel.starts_with("[data-forge-idx") => {
                Err("no element with that number: the page changed, call state again".into())
            }
            _ => Err(format!("nothing on the page matches {sel}")),
        }
    }

    fn mouse(
        &mut self,
        kind: &str,
        x: f64,
        y: f64,
        button: &str,
        count: u32,
    ) -> Result<(), String> {
        let s = self.cur_session()?;
        self.call(
            Some(&s),
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": x, "y": y, "button": button, "clickCount": count }),
        )
        .map(drop)
    }

    fn key(&mut self, key: &str, modifiers: &[String]) -> Result<(), String> {
        let (k, code, vk, txt) = key_info(key).ok_or_else(|| format!("no such key: {key}"))?;
        let m = modifiers.iter().fold(0, |m, x| {
            m | match x.as_str() {
                "alt" => 1,
                "ctrl" => 2,
                "meta" => 4,
                "shift" => 8,
                _ => 0,
            }
        });
        let s = self.cur_session()?;
        let typed = txt.filter(|_| m & 0b110 == 0);
        let mut down = json!({ "type": if typed.is_some() { "keyDown" } else { "rawKeyDown" },
            "key": k, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk, "modifiers": m });
        if let Some(t) = typed {
            down["text"] = t.into();
        }
        self.call(Some(&s), "Input.dispatchKeyEvent", down)?;
        self.call(
            Some(&s),
            "Input.dispatchKeyEvent",
            json!({ "type": "keyUp", "key": k, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk, "modifiers": m }),
        )
        .map(drop)
    }

    fn run(&mut self, name: &str, a: &Value) -> Answer {
        let max = |a: &Value, d: usize| {
            a["max_chars"]
                .as_u64()
                .map_or(d, |m| m as usize)
                .min(200_000)
        };
        match name {
            "navigate" => {
                let url = crate::web_view::address(a["url"].as_str().ok_or("url is needed")?);
                if a["new_tab"].as_bool() == Some(true) {
                    let r = self.call(None, "Target.createTarget", json!({ "url": url }))?;
                    let id = r["targetId"].as_str().ok_or("no tab")?.to_string();
                    self.focus(&id);
                } else {
                    let s = self.cur_session()?;
                    let r = self.call(Some(&s), "Page.navigate", json!({ "url": url }))?;
                    if let Some(e) = r["errorText"].as_str() {
                        return Err(format!("could not open {url}: {e}"));
                    }
                }
                self.settle(20);
                Ok(text(self.now_on()))
            }
            "state" => {
                let v = self.eval(STATE_JS)?;
                let mut state: Value =
                    serde_json::from_str(v.as_str().unwrap_or("{}")).unwrap_or_default();
                if state["url"]
                    .as_str()
                    .is_some_and(|u| u.starts_with("chrome-error:"))
                {
                    return Err("The page did not load (the browser shows its error page). Navigate again, or to another address.".into());
                }
                let cur = self.current.clone().unwrap_or_default();
                state["tabs"] = self
                    .tabs()?
                    .iter()
                    .map(|t| json!({ "id": t.id, "url": t.url, "title": t.title, "current": t.id == cur }))
                    .collect();
                Ok(text(
                    serde_json::to_string_pretty(&state).unwrap_or_default(),
                ))
            }
            "click" => {
                let (x, y, what) = match element_selector(a) {
                    Some(sel) => {
                        let (x, y) = self.point(&sel)?;
                        (x, y, sel)
                    }
                    None => match (a["x"].as_f64(), a["y"].as_f64()) {
                        (Some(x), Some(y)) => (x, y, format!("({x}, {y})")),
                        _ => return Err("give an index, a selector, or x and y".into()),
                    },
                };
                let button = a["button"].as_str().unwrap_or("left");
                let count = if a["double"].as_bool() == Some(true) {
                    2
                } else {
                    1
                };
                self.mouse("mouseMoved", x, y, "none", 0)?;
                for c in 1..=count {
                    self.mouse("mousePressed", x, y, button, c)?;
                    self.mouse("mouseReleased", x, y, button, c)?;
                }
                self.settle(10);
                Ok(text(format!("Clicked {what}. {}", self.now_on())))
            }
            "hover" => {
                let sel = element_selector(a).ok_or("give an index or a selector")?;
                let (x, y) = self.point(&sel)?;
                self.mouse("mouseMoved", x, y, "none", 0)?;
                Ok(text(format!("Hovering {sel}.")))
            }
            "type" => {
                let t = a["text"].as_str().ok_or("text is needed")?.to_string();
                if let Some(sel) = element_selector(a) {
                    let (x, y) = self.point(&sel)?;
                    self.mouse("mousePressed", x, y, "left", 1)?;
                    self.mouse("mouseReleased", x, y, "left", 1)?;
                    if a["clear"].as_bool() == Some(true) {
                        self.eval(&format!(
                            "(() => {{ const el = document.querySelector({}); if (!el) return; el.focus(); if ('value' in el) {{ el.value = ''; el.dispatchEvent(new Event('input', {{bubbles: true}})); }} else {{ document.execCommand('selectAll'); document.execCommand('delete'); }} }})()",
                            js_str(&sel)
                        ))?;
                    }
                }
                let s = self.cur_session()?;
                self.call(Some(&s), "Input.insertText", json!({ "text": t }))?;
                if a["submit"].as_bool() == Some(true) {
                    self.key("Enter", &[])?;
                    self.settle(15);
                }
                Ok(text(format!(
                    "Typed {} characters. {}",
                    t.chars().count(),
                    self.now_on()
                )))
            }
            "press" => {
                let key = a["key"].as_str().ok_or("key is needed")?;
                let mods: Vec<String> = a["modifiers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m.as_str().map(str::to_string))
                    .collect();
                self.key(key, &mods)?;
                self.settle(5);
                Ok(text(format!("Pressed {key}. {}", self.now_on())))
            }
            "select" => {
                let sel = element_selector(a).ok_or("give an index or a selector")?;
                let v = a["value"].as_str().ok_or("value is needed")?;
                let r = self.eval(&format!(
                    "(() => {{ const el = document.querySelector({}); if (!el || el.tagName !== 'SELECT') return 'not a select'; const want = {}; const o = [...el.options].find(o => o.value === want) || [...el.options].find(o => o.text.trim() === want); if (!o) return 'no option ' + want; el.value = o.value; el.dispatchEvent(new Event('input', {{bubbles: true}})); el.dispatchEvent(new Event('change', {{bubbles: true}})); return 'Chose ' + o.text; }})()",
                    js_str(&sel),
                    js_str(v)
                ))?;
                match r.as_str() {
                    Some(m) if m.starts_with("Chose") => Ok(text(m)),
                    Some(m) => Err(m.to_string()),
                    None => Err("the select did not answer".into()),
                }
            }
            "scroll" => {
                if let Some(sel) = element_selector(a) {
                    self.point(&sel)?;
                    return Ok(text(format!("Scrolled {sel} into view.")));
                }
                let amount = a["amount"].as_f64();
                let (dx, dy) = match a["direction"].as_str().unwrap_or("down") {
                    "up" => (0., -1.),
                    "left" => (-1., 0.),
                    "right" => (1., 0.),
                    _ => (0., 1.),
                };
                let r = self.eval(&format!(
                    "(() => {{ const a = {amt}; window.scrollBy({{left: {dx} * (a ?? innerWidth * 0.9), top: {dy} * (a ?? innerHeight * 0.9), behavior: 'instant'}}); return [Math.round(scrollX), Math.round(scrollY), (document.documentElement || {{}}).scrollHeight || 0]; }})()",
                    amt = amount.map_or("null".to_string(), |v| v.to_string())
                ))?;
                Ok(text(format!(
                    "Scrolled. Now at x {}, y {} of {} high.",
                    r[0], r[1], r[2]
                )))
            }
            "text" | "html" => {
                let sel = a["selector"].as_str();
                let prop = if name == "text" {
                    "innerText"
                } else {
                    "outerHTML"
                };
                let js = match sel {
                    Some(s) => format!("(() => {{ const el = document.querySelector({}); return el ? el.{prop} : null; }})()", js_str(s)),
                    None if name == "text" => "document.body ? document.body.innerText : ''".to_string(),
                    None => "document.documentElement.outerHTML".to_string(),
                };
                match self.eval(&js)? {
                    Value::String(s) => {
                        let m = max(a, if name == "text" { 20_000 } else { 50_000 });
                        let n = s.chars().count();
                        let mut out: String = s.chars().take(m).collect();
                        if n > m {
                            out.push_str(&format!(
                                "\n… ({} more characters; raise max_chars)",
                                n - m
                            ));
                        }
                        Ok(text(out))
                    }
                    _ => Err(format!(
                        "nothing on the page matches {}",
                        sel.unwrap_or("the page")
                    )),
                }
            }
            "eval" => {
                let e = a["expression"].as_str().ok_or("expression is needed")?;
                let v = self.eval(e)?;
                Ok(text(match v {
                    Value::String(s) => s,
                    Value::Null => "undefined".into(),
                    v => serde_json::to_string_pretty(&v).unwrap_or_default(),
                }))
            }
            "wait" => {
                let ms = a["timeout_ms"].as_u64().unwrap_or(10_000).min(60_000);
                let until = Instant::now() + Duration::from_millis(ms);
                let check = match (
                    a["selector"].as_str(),
                    a["text"].as_str(),
                    a["url_contains"].as_str(),
                ) {
                    (Some(s), _, _) => format!("!!document.querySelector({})", js_str(s)),
                    (_, Some(t), _) => format!(
                        "!!(document.body && document.body.innerText.includes({}))",
                        js_str(t)
                    ),
                    (_, _, Some(u)) => format!("location.href.includes({})", js_str(u)),
                    _ => return Err("give a selector, a text or url_contains".into()),
                };
                loop {
                    if self.eval(&check).ok() == Some(Value::Bool(true)) {
                        return Ok(text(format!("Found it. {}", self.now_on())));
                    }
                    if Instant::now() >= until {
                        return Err(format!("not there after {ms} ms"));
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
            "screenshot" => {
                let s = self.cur_session()?;
                let mut p = json!({ "format": "png" });
                if a["full_page"].as_bool() == Some(true) {
                    let m = self.call(Some(&s), "Page.getLayoutMetrics", json!({}))?;
                    let size = &m["cssContentSize"];
                    p["captureBeyondViewport"] = true.into();
                    p["clip"] = json!({ "x": 0, "y": 0, "scale": 1,
                        "width": size["width"].as_f64().unwrap_or(1280.),
                        "height": size["height"].as_f64().unwrap_or(800.).min(10_000.) });
                }
                let r = self.call(Some(&s), "Page.captureScreenshot", p)?;
                let data = r["data"].as_str().ok_or("no picture")?;
                let on = self.now_on();
                Ok(vec![
                    json!({ "type": "image", "data": data, "mimeType": "image/png" }),
                    json!({ "type": "text", "text": on }),
                ])
            }
            "console" => {
                let id = self.current()?;
                self.session(&id)?;
                // Whatever has arrived since the last tool.
                let _ = self.eval("0");
                let limit = a["limit"].as_u64().unwrap_or(50) as usize;
                let log = self.console.get(&id).cloned().unwrap_or_default();
                let from = log.len().saturating_sub(limit);
                Ok(text(if log.is_empty() {
                    "No console messages since KodAct's tools first used this tab.".to_string()
                } else {
                    log[from..].join("\n")
                }))
            }
            "tabs" => {
                let cur = self.current()?;
                let list: Vec<Value> = self
                    .tabs()?
                    .iter()
                    .map(|t| json!({ "id": t.id, "url": t.url, "title": t.title, "current": t.id == cur }))
                    .collect();
                Ok(text(
                    serde_json::to_string_pretty(&list).unwrap_or_default(),
                ))
            }
            "new_tab" => {
                let url = a["url"]
                    .as_str()
                    .map_or("about:blank".to_string(), crate::web_view::address);
                let r = self.call(None, "Target.createTarget", json!({ "url": url }))?;
                let id = r["targetId"].as_str().ok_or("no tab")?.to_string();
                self.focus(&id);
                self.settle(20);
                Ok(text(format!("Opened tab {id}. {}", self.now_on())))
            }
            "switch_tab" => {
                let id = a["id"].as_str().ok_or("id is needed")?.to_string();
                if !self.tabs()?.iter().any(|t| t.id == id) {
                    return Err(format!("no tab {id}: see tabs"));
                }
                self.call(None, "Target.activateTarget", json!({ "targetId": id }))?;
                self.focus(&id);
                Ok(text(format!("Switched. {}", self.now_on())))
            }
            "close_tab" => {
                let id = a["id"].as_str().ok_or("id is needed")?.to_string();
                self.call(None, "Target.closeTarget", json!({ "targetId": id }))?;
                self.sessions.remove(&id);
                // Gone from the list before anything else uses the tabs.
                let until = Instant::now() + Duration::from_secs(3);
                while Instant::now() < until && self.tabs()?.iter().any(|t| t.id == id) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                let focused = std::fs::read_to_string(&self.focus_file).unwrap_or_default();
                if self.current.as_deref() == Some(id.as_str()) || focused.trim() == id {
                    self.current = None;
                    if let Some(next) = self.tabs()?.last().map(|t| t.id.clone()) {
                        self.focus(&next);
                    }
                }
                Ok(text(format!("Closed tab {id}.")))
            }
            "back" | "forward" => {
                let s = self.cur_session()?;
                let h = self.call(Some(&s), "Page.getNavigationHistory", json!({}))?;
                let at = h["currentIndex"].as_i64().unwrap_or(0);
                let to = if name == "back" { at - 1 } else { at + 1 };
                let entry = h["entries"]
                    .as_array()
                    .and_then(|e| usize::try_from(to).ok().and_then(|i| e.get(i)))
                    .cloned()
                    .ok_or(if name == "back" {
                        "nothing to go back to"
                    } else {
                        "nothing to go forward to"
                    })?;
                self.call(
                    Some(&s),
                    "Page.navigateToHistoryEntry",
                    json!({ "entryId": entry["id"] }),
                )?;
                self.settle(15);
                Ok(text(self.now_on()))
            }
            "reload" => {
                let s = self.cur_session()?;
                self.call(Some(&s), "Page.reload", json!({}))?;
                self.settle(20);
                Ok(text(self.now_on()))
            }
            _ => Err(format!("no tool {name}")),
        }
    }
}

/// Answer one JSON-RPC message; None for a notification.
fn handle(b: &mut Browser, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    Some(match msg["method"].as_str().unwrap_or_default() {
        "initialize" => reply(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2024-11-05"),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": SERVER, "version": env!("CARGO_PKG_VERSION") },
            "instructions": "The browser in KodAct's side panel, where the user watches it live. Call state to see a page's numbered elements, then click, type, select or hover by number; eval runs any JavaScript in the page.",
        })),
        "ping" => reply(json!({})),
        "tools/list" => reply(json!({ "tools": tools() })),
        "tools/call" => {
            let name = msg["params"]["name"].as_str().unwrap_or_default();
            let args = msg["params"]["arguments"].clone();
            match b.run_step(name, &args) {
                Ok(content) => reply(json!({ "content": content, "isError": false })),
                Err(e) => reply(json!({ "content": text(e), "isError": true })),
            }
        }
        m => json!({ "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("no method {m}") } }),
    })
}

/// Serve MCP on stdin and stdout until stdin closes.
pub fn serve(devtools_http: &str, focus_file: &Path) {
    let mut b = Browser::new(devtools_http, focus_file);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(r) = handle(&mut b, &msg) {
            if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}

/// Serve the same MCP over HTTP on `127.0.0.1:<port>` (`POST /mcp/<token>`,
/// one JSON-RPC message per request, its answer as JSON): for a KodAct CLI on
/// a remote, which reaches it through an `ssh -R` forward (`remote.rs`).
/// Requests without the token are refused. Returns the port.
pub fn serve_http(devtools_http: &str, focus_file: &Path, token: &str) -> std::io::Result<u16> {
    let mut browser = Browser::new(devtools_http, focus_file);
    serve_json_http(token, move |msg| handle(&mut browser, msg))
}

/// An MCP server over HTTP on `127.0.0.1:<port>` (`POST /mcp/<token>`, one
/// JSON-RPC message per request, its answer as JSON), answering with
/// `answer`: the browser tools' and the design tools' for remote chats.
/// Requests without the token are refused. Returns the port.
pub(crate) fn serve_json_http(
    token: &str,
    answer: impl FnMut(&Value) -> Option<Value> + Send + 'static,
) -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let answer = std::sync::Arc::new(std::sync::Mutex::new(answer));
    let path = format!("/mcp/{token}");
    std::thread::spawn(move || {
        for stream in listener.incoming().map_while(Result::ok) {
            let (answer, path) = (answer.clone(), path.clone());
            std::thread::spawn(move || {
                let _ = http_exchange(stream, &path, &mut |msg| (answer.lock().unwrap())(msg));
            });
        }
    });
    Ok(port)
}

/// One HTTP/1.1 request and its answer (the connection then closes).
fn http_exchange(
    mut stream: std::net::TcpStream,
    path: &str,
    answer: &mut dyn FnMut(&Value) -> Option<Value>,
) -> std::io::Result<()> {
    use std::io::Read;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = std::io::BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let respond = |stream: &mut std::net::TcpStream, status: &str, body: &str| {
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|_| stream.write_all(body.as_bytes()))
            .and_then(|_| stream.flush())
    };
    if target != path {
        return respond(&mut stream, "404 Not Found", "{}");
    }
    match method {
        "POST" => {
            let mut body = vec![0u8; length.min(16 * 1024 * 1024)];
            reader.read_exact(&mut body)?;
            let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
                return respond(&mut stream, "400 Bad Request", "{}");
            };
            match answer(&msg) {
                Some(r) => respond(&mut stream, "200 OK", &r.to_string()),
                None => respond(&mut stream, "202 Accepted", ""),
            }
        }
        // No server-sent stream and no sessions: GET and DELETE just answer.
        "GET" => respond(&mut stream, "405 Method Not Allowed", "{}"),
        _ => respond(&mut stream, "200 OK", "{}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The HTTP server answers MCP with the token and refuses without it.
    #[test]
    fn speaks_mcp_over_http() {
        use std::io::Read;
        let port =
            serve_http("http://127.0.0.1:1", Path::new("/nonexistent/focus"), "tok").unwrap();
        let post = |path: &str, body: &str| {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                s,
                "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        };
        let init = post(
            "/mcp/tok",
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
        );
        assert!(init.starts_with("HTTP/1.1 200"), "{init}");
        assert!(init.contains(r#""name":"kodact-browser""#));
        let list = post(
            "/mcp/tok",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        );
        assert!(list.contains(r#""name":"navigate""#));
        let note = post(
            "/mcp/tok",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        );
        assert!(note.starts_with("HTTP/1.1 202"));
        assert!(post("/mcp/wrong", "{}").starts_with("HTTP/1.1 404"));
    }

    fn browser() -> Browser {
        Browser::new("http://127.0.0.1:1", Path::new("/nonexistent/focus"))
    }

    #[test]
    fn it_speaks_mcp() {
        let mut b = browser();
        let init = handle(&mut b, &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}})).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], SERVER);
        assert!(handle(
            &mut b,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .is_none());
        let list = handle(
            &mut b,
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for n in [
            "navigate",
            "state",
            "click",
            "type",
            "press",
            "eval",
            "screenshot",
            "tabs",
            "wait",
            "console",
        ] {
            assert!(names.contains(&n), "{n} missing");
        }
        // Every tool has an object schema.
        assert!(list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["inputSchema"]["type"] == "object"));
        let bad = handle(&mut b, &json!({"jsonrpc":"2.0","id":3,"method":"nope"})).unwrap();
        assert_eq!(bad["error"]["code"], -32601);
    }

    #[test]
    fn a_tool_without_its_input_says_so() {
        let mut b = browser();
        let r = handle(&mut b, &json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"type","arguments":{}}})).unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("text is needed"));
    }

    #[test]
    fn keys_map_to_devtools_names() {
        assert_eq!(key_info("enter").unwrap().0, "Enter");
        assert_eq!(key_info("a").unwrap().1, "KeyA");
        assert_eq!(key_info("F5").unwrap().2, 116);
        assert!(key_info("nonsense").is_none());
        assert_eq!(
            element_selector(&json!({"index": 3})).unwrap(),
            "[data-forge-idx=\"3\"]"
        );
    }

    /// Every tool against a real Chromium and a page with a form: needs
    /// Playwright's Chromium (or KODACT_CHROME).
    #[test]
    #[ignore]
    fn drives_a_real_page() {
        let exe = crate::web_view::find_chromium().expect("no Chromium");
        let dir = std::env::temp_dir().join(format!("kodact-mcp-{}", std::process::id()));
        let (view, _rx) =
            crate::web_view::start(&exe, &dir.join("profile"), &dir.join("focus")).unwrap();
        let mut b = Browser::new(&view.cdp_url, &dir.join("focus"));
        let call = |b: &mut Browser, name: &str, args: Value| -> Result<String, String> {
            b.run(name, &args).map(|c| {
                c.iter()
                    .filter_map(|x| x["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        };
        let page = "data:text/html,<title>Form</title><input id=n placeholder=Name><select id=s><option value=a>Apple</option><option value=b>Banana</option></select><button onclick=\"document.getElementById('out').textContent='Hello '+n.value+' '+s.value;console.log('greeted')\">Greet</button><p id=out></p><a href='about:blank'>away</a>";
        call(&mut b, "navigate", json!({"url": page})).unwrap();
        let state: Value =
            serde_json::from_str(&call(&mut b, "state", json!({})).unwrap()).unwrap();
        let idx = |tag: &str| {
            state["elements"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["tag"] == tag)
                .unwrap()["index"]
                .as_u64()
                .unwrap()
        };
        call(
            &mut b,
            "type",
            json!({"index": idx("input"), "text": "KodAct"}),
        )
        .unwrap();
        call(
            &mut b,
            "select",
            json!({"index": idx("select"), "value": "Banana"}),
        )
        .unwrap();
        call(&mut b, "click", json!({"index": idx("button")})).unwrap();
        assert_eq!(
            call(&mut b, "text", json!({"selector": "#out"})).unwrap(),
            "Hello KodAct b"
        );
        assert_eq!(
            call(&mut b, "eval", json!({"expression": "1 + 2"})).unwrap(),
            "3"
        );
        assert!(call(&mut b, "wait", json!({"text": "Hello KodAct"}))
            .unwrap()
            .starts_with("Found"));
        assert!(call(&mut b, "console", json!({}))
            .unwrap()
            .contains("greeted"));
        let shot = b.run("screenshot", &json!({})).unwrap();
        assert_eq!(shot[0]["mimeType"], "image/png");
        assert!(shot[0]["data"].as_str().unwrap().len() > 1000);
        call(
            &mut b,
            "new_tab",
            json!({"url": "data:text/html,<title>Two</title>two"}),
        )
        .unwrap();
        let tabs: Value = serde_json::from_str(&call(&mut b, "tabs", json!({})).unwrap()).unwrap();
        assert_eq!(tabs.as_array().unwrap().len(), 2);
        // The panel is told which tab is current.
        let current = tabs
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["current"] == true)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(std::fs::read_to_string(dir.join("focus")).unwrap(), current);
        call(&mut b, "close_tab", json!({"id": current})).unwrap();
        assert_eq!(
            call(&mut b, "eval", json!({"expression": "document.title"})).unwrap(),
            "Form"
        );
        assert!(call(&mut b, "press", json!({"key": "Tab"})).is_ok());
        assert!(call(&mut b, "scroll", json!({"direction": "down"})).is_ok());
        assert!(call(&mut b, "click", json!({"index": 999}))
            .unwrap_err()
            .contains("call state again"));
        drop(view);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// What a step sets off besides its own work, against a real Chromium:
    /// dialogs answered and reported (none holds the page), a link's new
    /// tab made current, a download saved and named, a page that did not
    /// load said in words.
    #[test]
    #[ignore]
    fn reports_dialogs_tabs_and_downloads() {
        let exe = crate::web_view::find_chromium().expect("no Chromium");
        let dir = std::env::temp_dir().join(format!("kodact-mcp-side-{}", std::process::id()));
        let (view, _rx) =
            crate::web_view::start(&exe, &dir.join("profile"), &dir.join("focus")).unwrap();
        let mut b = Browser::new(&view.cdp_url, &dir.join("focus"));
        let call = |b: &mut Browser, name: &str, args: Value| -> Result<String, String> {
            b.run_step(name, &args).map(|c| {
                c.iter()
                    .filter_map(|x| x["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        };
        let name = format!("forge-test-{}.txt", std::process::id());
        let page = format!("data:text/html,<title>Side</title><button id=a onclick=\"alert('hi there')\">A</button><button id=c onclick=\"out.textContent=confirm('Sure?')\">C</button><p id=out>none</p><a id=t href=about:blank target=_blank>new</a><a id=d download={name} href=\"data:text/plain,hello\">dl</a>");
        call(&mut b, "navigate", json!({"url": page})).unwrap();
        let started = Instant::now();
        let r = call(&mut b, "click", json!({"selector": "#a"})).unwrap();
        assert!(r.contains("alert: \"hi there\""), "{r}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the alert held the step"
        );
        let r = call(&mut b, "click", json!({"selector": "#c"})).unwrap();
        assert!(r.contains("Answered Cancel"), "{r}");
        assert_eq!(
            call(&mut b, "text", json!({"selector": "#out"})).unwrap(),
            "false"
        );
        let r = call(
            &mut b,
            "click",
            json!({"selector": "#c", "dialog": "accept"}),
        )
        .unwrap();
        assert!(r.contains("Answered OK"), "{r}");
        assert_eq!(
            call(&mut b, "text", json!({"selector": "#out"})).unwrap(),
            "true"
        );
        let r = call(&mut b, "click", json!({"selector": "#d"})).unwrap();
        let saved = crate::downloads::dir().unwrap().join(&name);
        assert!(
            r.contains(&format!("Downloaded a file: {}", saved.display())),
            "{r}"
        );
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "hello");
        let _ = std::fs::remove_file(&saved);
        let r = call(&mut b, "click", json!({"selector": "#t"})).unwrap();
        assert!(r.contains("opened a new tab"), "{r}");
        let focus = std::fs::read_to_string(dir.join("focus")).unwrap();
        let tabs: Value = serde_json::from_str(&call(&mut b, "tabs", json!({})).unwrap()).unwrap();
        let cur = tabs
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["current"] == true)
            .unwrap();
        assert_eq!(cur["id"].as_str().unwrap(), focus.trim());
        assert_eq!(cur["url"], "about:blank");
        let r = call(
            &mut b,
            "navigate",
            json!({"url": "http://does-not-exist.invalid/"}),
        );
        assert!(r.is_err());
        let r = call(&mut b, "state", json!({})).unwrap_err();
        assert!(r.contains("did not load"), "{r}");
        drop(view);
        let _ = std::fs::remove_dir_all(dir);
    }
}
