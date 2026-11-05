//! The live design panel's plain parts: what a page being written says so
//! far, where it lives as an address, and how a draft finds its pictures.
//! The panel itself is the browser (`web_view.rs`, `Cmd::Design`); the
//! chat decides when to show it (`state.rs`, `design_*`).
//!
//! Pages are shown as `file://` addresses, not through a local server: a
//! page that loads someone else's script can then read nothing else on
//! this computer. (Of Open Design's 281 example pages, 7 use `fetch()` or
//! ES modules, which `file://` limits; their fetches go to the web.)

use std::path::Path;

/// Whether KodAct shows this file live while it is written: an HTML page.
pub fn is_page(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

/// A Write call's `file_path` and `content` so far, read from the input
/// JSON as it streams in (`{"file_path": "a.html", "content": "<!doctype…`).
/// The content stops where the stream has got to, mid-escape included. None
/// until the path is complete.
pub fn partial_write(json: &str) -> Option<(String, String)> {
    let path = string_field(json, "file_path").filter(|(_, done)| *done)?.0;
    let content = string_field(json, "content").map_or(String::new(), |(s, _)| s);
    Some((path, content))
}

/// The value of `"key": "…"` at the top of a partial JSON object, decoded,
/// and whether its closing quote has arrived.
fn string_field(json: &str, key: &str) -> Option<(String, bool)> {
    let needle = format!("\"{key}\"");
    let mut from = 0;
    while let Some(at) = json[from..].find(&needle) {
        let after = from + at + needle.len();
        let rest = json[after..].trim_start();
        if let Some(rest) = rest.strip_prefix(':') {
            if let Some(body) = rest.trim_start().strip_prefix('"') {
                return Some(decode(body));
            }
        }
        from = after;
    }
    None
}

/// A JSON string's text up to its closing quote (or the end of what has
/// arrived), with its escapes undone.
fn decode(body: &str) -> (String, bool) {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return (out, true),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let Ok(hi) = u32::from_str_radix(&hex, 16) else {
                        return (out, false);
                    };
                    if hex.len() < 4 {
                        return (out, false);
                    }
                    if (0xD800..0xDC00).contains(&hi) {
                        // A surrogate pair: `\ud83d\ude00`.
                        let rest: String = chars.by_ref().take(6).collect();
                        let lo = rest
                            .strip_prefix("\\u")
                            .filter(|h| h.len() == 4)
                            .and_then(|h| u32::from_str_radix(h, 16).ok());
                        match lo {
                            Some(lo) if (0xDC00..0xE000).contains(&lo) => {
                                let c = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                out.extend(char::from_u32(c));
                            }
                            _ => return (out, false),
                        }
                    } else {
                        out.extend(char::from_u32(hi));
                    }
                }
                Some(other) => out.push(other),
                None => return (out, false),
            },
            c => out.push(c),
        }
    }
    (out, false)
}

/// A file's `file://` address: `/home/a/b c.html` → `file:///home/a/b%20c.html`,
/// `C:\Users\a\b.html` → `file:///C:/Users/a/b.html`.
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A draft as it can be shown before its file exists: the same page, told
/// where its folder is, so its relative pictures and styles load. The
/// `<base>` goes inside `<head>` (or after the doctype), never before it,
/// which would put the page in quirks mode.
pub fn draft_page(content: &str, dir: &Path) -> String {
    let mut base = file_url(dir);
    if !base.ends_with('/') {
        base.push('/');
    }
    let tag = format!("<base href=\"{base}\">");
    let lower = content.to_ascii_lowercase();
    let after = |open: &str| {
        lower
            .find(open)
            .and_then(|at| lower[at..].find('>').map(|end| at + end + 1))
    };
    match after("<head").or_else(|| after("<!doctype")) {
        Some(at) => format!("{}{tag}{}", &content[..at], &content[at..]),
        None => format!("{tag}{content}"),
    }
}

/// Where a page's draft is written while its file doesn't exist yet: one
/// file per page (`key` is the page's address), in the temp folder.
pub fn draft_file(key: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    std::env::temp_dir().join(format!(
        "forge-draft-{}-{:016x}.html",
        std::process::id(),
        h.finish()
    ))
}

/// An address without its `#fragment`: a deck's `#/3` doesn't make it
/// another page.
pub fn page_key(url: &str) -> &str {
    url.split_once('#').map_or(url, |(page, _)| page)
}

/// The deck's slides, as `design_render` and the PDF find them: the first
/// group of `section.slide`, `.slide` or `[data-slide]` sharing a parent.
macro_rules! deck_slides {
    () => {
        "[...document.querySelectorAll('section.slide, .slide, [data-slide]')].filter((s, _, a) => s.parentElement === a[0].parentElement)"
    };
}
pub(crate) use deck_slides;

/// Whether a deck's `slides` (in scope) are stacked down the page, so going
/// to one is scrolling to it: its first two slides that show sit one below
/// the other. A deck that shows one at a time (the others hidden, or laid
/// over each other) isn't.
macro_rules! deck_stacked {
    () => {
        "(() => { const vis = slides.map(s => s.getBoundingClientRect()).filter(r => r.width && r.height); return vis.length > 1 && Math.abs(vis[1].top - vis[0].top) >= vis[0].height * 0.5; })()"
    };
}

/// The slide a deck shows (the visible one nearest the middle of the
/// window) and how many, as JSON `[n, count]`; "" for a page that isn't a
/// deck.
pub const DECK_AT_JS: &str = concat!(
    "(() => { const slides = ",
    deck_slides!(),
    r#"; if (slides.length < 2) return '';
  let best = -1, far = Infinity;
  slides.forEach((s, i) => {
    const r = s.getBoundingClientRect(), cs = getComputedStyle(s);
    if (!r.width || !r.height || cs.visibility === 'hidden' || cs.display === 'none' || +cs.opacity === 0) return;
    if (r.bottom < 0 || r.top > innerHeight) return;
    const d = Math.abs((r.top + r.bottom) / 2 - innerHeight / 2);
    if (d < far) { far = d; best = i; }
  });
  return best < 0 ? '' : JSON.stringify([best + 1, slides.length]);
})()"#
);

/// Go a slide on (1) or back (-1): scroll to it when the slides are
/// stacked down the page, else press the deck's own arrow key.
pub fn slide_step_js(step: i32) -> String {
    let key = if step > 0 { "ArrowRight" } else { "ArrowLeft" };
    format!(
        r#"(() => {{ const slides = {}; if (slides.length < 2) return;
  if ({}) {{
    let at = 0, far = Infinity;
    slides.forEach((s, i) => {{ const r = s.getBoundingClientRect(); const d = Math.abs(r.top); if (d < far) {{ far = d; at = i; }} }});
    const to = slides[Math.max(0, Math.min(slides.length - 1, at + ({step})))];
    to.scrollIntoView({{ block: 'start', behavior: 'smooth' }});
    return;
  }}
  const el = document.activeElement || document.body;
  el.dispatchEvent(new KeyboardEvent('keydown', {{ key: '{key}', code: '{key}', bubbles: true }}));
  el.dispatchEvent(new KeyboardEvent('keyup', {{ key: '{key}', code: '{key}', bubbles: true }}));
}})()"#,
        deck_slides!(),
        deck_stacked!()
    )
}

/// Before a deck is printed (Save as PDF): one slide a page, at 16:9. A
/// deck built at a fixed size (its slides 1600px wide or more before any
/// scaling, as html-ppt's 1920) prints at 1920×1080; one sized to the
/// window, at 1280×720, so its type keeps its weight on the page.
/// The deck's slides (the first group of `section.slide`, `.slide` or
/// `[data-slide]` sharing a parent) are marked; the elements holding them
/// lose their scaling and fixed boxes, everything else beside them (a
/// deck's controls, an overview) is left out, and hidden slides show.
/// Returns how many slides, or null for a page that isn't a deck.
pub const DECK_PRINT_JS: &str = concat!(
    "(() => { const slides = ",
    deck_slides!(),
    r#";
  if (slides.length < 2) return null;
  const parent = slides[0].parentElement;
  const W = slides[0].offsetWidth >= 1600 ? 1920 : 1280, H = W * 9 / 16;
  for (let a = parent; a && a !== document.documentElement; a = a.parentElement) a.classList.add('forge-print-host');
  for (const s of slides) {
    s.classList.add('forge-print-slide');
    if (getComputedStyle(s).display === 'none') s.classList.add('forge-print-shown');
  }
  const st = document.createElement('style');
  st.id = 'forge-print';
  st.textContent = `@page { size: ${W}px ${H}px; margin: 0 }
@media print {
  html, body { margin: 0 !important; padding: 0 !important; width: ${W}px !important; height: auto !important; overflow: visible !important; }
  .forge-print-host { transform: none !important; position: static !important; width: auto !important; height: auto !important; overflow: visible !important; margin: 0 !important; padding: 0 !important; display: block !important; }
  .forge-print-host > :not(.forge-print-host):not(.forge-print-slide) { display: none !important; }
  .forge-print-slide { position: relative !important; inset: auto !important; transform: none !important; width: ${W}px !important; height: ${H}px !important; max-width: none !important; max-height: none !important; margin: 0 !important; opacity: 1 !important; visibility: visible !important; break-after: page !important; break-inside: avoid !important; overflow: hidden !important; box-sizing: border-box !important; }
  .forge-print-slide.forge-print-shown { display: block !important; }
  .forge-print-slide, .forge-print-slide * { animation: none !important; transition: none !important; }
}`;
  document.head.appendChild(st);
  return slides.length;
})()"#
);

/// Go to slide n (1-based). Slides stacked down the page are scrolled to;
/// a deck that shows one at a time is driven by its own keys (Home, then
/// ArrowRight), as a person would.
pub fn go_to_slide_js(n: usize) -> String {
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
        deck_slides!(),
        deck_stacked!()
    )
}

/// After the print: the page as it was.
pub const DECK_UNPRINT_JS: &str = r#"(() => {
  document.getElementById('forge-print')?.remove();
  for (const c of ['forge-print-host', 'forge-print-slide', 'forge-print-shown'])
    for (const el of [...document.getElementsByClassName(c)]) el.classList.remove(c);
})()"#;

/// A comment on part of a page, as the message KodAct gets: the page, the
/// part (its tag, the start of its text, a selector that finds it), what
/// you said, and to change only that part.
pub fn comment_message(page: &str, part: &crate::web_view::PagePart, comment: &str) -> String {
    let what = if part.text.is_empty() {
        format!("the <{}>", part.tag)
    } else {
        let text: String = part.text.chars().take(80).collect();
        let more = if text.len() < part.text.len() {
            "…"
        } else {
            ""
        };
        format!("the <{}> \"{}{more}\"", part.tag, text.trim_end())
    };
    format!(
        "A comment on {page}, on {what} (`{}`):\n\n{}\n\nRevise that part of the page for it, and keep the rest as it is.",
        part.selector,
        comment.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_write_is_read_as_it_streams() {
        let full = serde_json::json!({ "file_path": "deck.html", "content": "<h1>Hi \"you\"</h1>\n<p>😀 é</p>" })
            .to_string()
            .replace("😀", "\\ud83d\\ude00");
        // The path alone is not enough until it is whole.
        assert_eq!(partial_write(&full[..14]), None);
        let mut last = String::new();
        for end in (0..=full.len()).filter(|e| full.is_char_boundary(*e)) {
            if let Some((path, content)) = partial_write(&full[..end]) {
                assert_eq!(path, "deck.html");
                // What has arrived only grows, and never shows a half escape.
                assert!(content.starts_with(&last), "{content:?} after {last:?}");
                assert!(!content.contains('\\'));
                last = content;
            }
        }
        assert_eq!(last, "<h1>Hi \"you\"</h1>\n<p>😀 é</p>");
    }

    #[test]
    fn content_before_the_path_waits_for_the_path() {
        let json = r#"{"content": "<p>x</p>", "file_path": "a.ht"#;
        assert_eq!(partial_write(json), None);
        let json = r#"{"content": "<p>x</p>", "file_path": "a.html"}"#;
        assert_eq!(
            partial_write(json),
            Some(("a.html".into(), "<p>x</p>".into()))
        );
    }

    #[test]
    fn a_comment_names_its_part() {
        let part = crate::web_view::PagePart {
            tag: "h2".into(),
            text: "Three things you can use the Monday after".into(),
            selector: "main.deck > section.slide:nth-of-type(2) > h2".into(),
            x: 0.,
            y: 0.,
            w: 1.,
            h: 1.,
        };
        assert_eq!(
            comment_message("deck.html", &part, " Shorter, please. "),
            "A comment on deck.html, on the <h2> \"Three things you can use the Monday after\" (`main.deck > section.slide:nth-of-type(2) > h2`):\n\nShorter, please.\n\nRevise that part of the page for it, and keep the rest as it is."
        );
        assert_eq!(
            part.label(),
            "h2 · Three things you can use the Monday after"
        );
        let long = crate::web_view::PagePart {
            text: "x".repeat(100),
            ..part
        };
        assert!(comment_message("d.html", &long, "c").contains(&format!("\"{}…\"", "x".repeat(80))));
        assert!(long.label().ends_with('…'));
    }

    #[test]
    fn pages_are_html() {
        assert!(is_page("site/index.html") && is_page("A.HTM"));
        assert!(!is_page("notes.md") && !is_page("page.html.bak"));
    }

    #[test]
    fn file_addresses_on_every_system() {
        assert_eq!(
            file_url(&PathBuf::from("/home/a/b c.html")),
            "file:///home/a/b%20c.html"
        );
        assert_eq!(
            file_url(&PathBuf::from("C:\\Users\\a\\deck.html")),
            "file:///C:/Users/a/deck.html"
        );
        assert_eq!(
            file_url(&PathBuf::from("\\\\?\\C:\\x\\é.html")),
            "file:///C:/x/%C3%A9.html"
        );
        assert_eq!(page_key("file:///a/deck.html#/3"), "file:///a/deck.html");
    }

    #[test]
    fn a_draft_knows_its_folder_without_leaving_standards_mode() {
        let dir = PathBuf::from("/p/site");
        let page = draft_page("<!doctype html><html><head><title>x</title>", &dir);
        assert_eq!(
            page,
            "<!doctype html><html><head><base href=\"file:///p/site/\"><title>x</title>"
        );
        assert_eq!(
            draft_page("<!DOCTYPE html>\n<body>", &dir),
            "<!DOCTYPE html><base href=\"file:///p/site/\">\n<body>"
        );
        assert_eq!(
            draft_page("<p>", &dir),
            "<base href=\"file:///p/site/\"><p>"
        );
    }
}
