//! `design_lint`'s checks on a page's source: what makes a design look
//! unfinished or broken before anyone sees it. Placeholder text, a file cut
//! short ("rest of the slides here"), markup left open, files it links that
//! aren't there, CDN links with no pinned version, and a head missing its
//! title or viewport. What can only be seen rendered (content overflowing
//! its slide or the window) is checked in the page by `design_mcp.rs`.

use std::path::Path;

/// How much a finding matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// The page is broken or unfinished.
    Error,
    /// Worth fixing.
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub level: Level,
    /// 1-based, 0 for the whole file.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        if self.line > 0 {
            write!(f, "{level}, line {}: {}", self.line, self.message)
        } else {
            write!(f, "{level}: {}", self.message)
        }
    }
}

/// Elements whose open and close tags must pair up: the page's structure.
const PAIRED: [&str; 13] = [
    "html", "head", "body", "main", "section", "article", "div", "script", "style", "svg", "ul",
    "ol", "table",
];

/// Visible text that means the page isn't finished (lower case).
const PLACEHOLDERS: [&str; 9] = [
    "lorem ipsum",
    "dolor sit amet",
    "your title here",
    "your text here",
    "your company",
    "placeholder text",
    "[insert",
    "[todo",
    "coming soon...",
];

/// Comments a model leaves where it stopped writing (lower case).
const STUBS: [&str; 6] = [
    "rest of the",
    "more slides",
    "remaining slides",
    "add more",
    "same as above",
    "repeat for",
];

/// Check `html`, the page at `path` (relative links resolve from its folder).
pub fn check(html: &str, path: &Path) -> Vec<Finding> {
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    check_with(html, &mut |files| {
        files
            .iter()
            .map(|f| {
                let p = Path::new(f);
                if p.is_absolute() {
                    p.exists()
                } else {
                    dir.join(p).exists()
                }
            })
            .collect()
    })
}

/// [`check`], with `exists` saying which linked files are there (each as
/// the page writes it: relative to its folder, or absolute), all at once:
/// a remote page's are checked on the remote in one go.
pub fn check_with(html: &str, exists: &mut dyn FnMut(&[String]) -> Vec<bool>) -> Vec<Finding> {
    let mut out = vec![];
    let line_of = |at: usize| html[..at].matches('\n').count() + 1;
    let mut push = |level, line, message: String| {
        out.push(Finding {
            level,
            line,
            message,
        })
    };

    if html.trim().len() < 200 {
        push(Level::Error, 0, "The file is almost empty.".into());
        return out;
    }
    let lower = html.to_ascii_lowercase();

    // Comments: stubs where the model stopped.
    let mut from = 0;
    while let Some(i) = lower[from..].find("<!--") {
        let start = from + i;
        let end = lower[start..]
            .find("-->")
            .map_or(lower.len(), |e| start + e + 3);
        let body = lower[start + 4..end.saturating_sub(3).max(start + 4)].trim();
        let ellipsis = !body.is_empty() && body.trim_matches(['.', '…', ' ']).is_empty();
        if ellipsis || (STUBS.iter().any(|s| body.contains(s)) && body.len() < 120) {
            push(
                Level::Error,
                line_of(start),
                format!(
                    "A comment stands in for content: <!-- {} -->. Write it out.",
                    body.trim()
                ),
            );
        }
        from = end;
    }

    let text = visible_text(&lower);
    for p in PLACEHOLDERS {
        if let Some(&(at, _)) = text.iter().find(|(_, t)| t.contains(p)) {
            push(
                Level::Error,
                line_of(at),
                format!("Placeholder text: \"{p}\"."),
            );
        }
    }
    for word in ["todo", "tbd", "fixme", "xxx"] {
        if let Some(&(at, _)) = text.iter().find(|(_, t)| has_word(t, word)) {
            push(
                Level::Error,
                line_of(at),
                format!("\"{}\" left in the page's text.", word.to_uppercase()),
            );
        }
    }

    // Structure: paired tags, outside scripts, styles and comments.
    let tags = tags(&lower);
    for name in PAIRED {
        let opens = tags
            .iter()
            .filter(|t| !t.close && t.name == name && !t.self_closed)
            .count();
        let closes = tags.iter().filter(|t| t.close && t.name == name).count();
        if opens != closes {
            let at = tags
                .iter()
                .rev()
                .find(|t| t.name == name)
                .map_or(0, |t| line_of(t.at));
            push(
                Level::Error,
                at,
                format!("<{name}> opened {opens} times and closed {closes}: the markup is broken or the file was cut short."),
            );
        }
    }
    if !lower.contains("</html>") {
        push(
            Level::Error,
            0,
            "No </html>: the file ends before the page does.".into(),
        );
    }

    // The head.
    if !tags.iter().any(|t| t.name == "title" && !t.close) {
        push(Level::Warning, 0, "No <title>.".into());
    }
    if !lower.contains("name=\"viewport\"") && !lower.contains("name=viewport") {
        push(
            Level::Warning,
            0,
            "No viewport <meta>: the page won't fit a narrow window.".into(),
        );
    }

    // Links: local files that aren't there, CDN links with no version.
    let mut files: Vec<(usize, String, String)> = vec![];
    for (at, url) in links(html) {
        let u = url.trim();
        let low = u.to_ascii_lowercase();
        if low.starts_with("http://") || low.starts_with("https://") || low.starts_with("//") {
            if let Some(m) = unpinned(&low) {
                push(Level::Warning, line_of(at), format!("{m}: {u}"));
            }
            continue;
        }
        if u.is_empty()
            || [
                "#",
                "data:",
                "mailto:",
                "tel:",
                "javascript:",
                "blob:",
                "about:",
                "{",
                "$",
            ]
            .iter()
            .any(|p| low.starts_with(p))
        {
            continue;
        }
        let file = u.split(['?', '#']).next().unwrap_or(u).replace("%20", " ");
        let file = file.strip_prefix("file://").unwrap_or(&file).to_string();
        files.push((at, file, u.to_string()));
    }
    let names: Vec<String> = files.iter().map(|(_, f, _)| f.clone()).collect();
    let there = if names.is_empty() {
        vec![]
    } else {
        exists(&names)
    };
    for ((at, _, u), there) in files
        .iter()
        .zip(there.iter().chain(std::iter::repeat(&true)))
    {
        if !there {
            push(
                Level::Error,
                line_of(*at),
                format!("Links a file that isn't there: {u}"),
            );
        }
    }
    out.sort_by_key(|f| (f.level, f.line));
    out
}

/// Why a CDN link isn't pinned, if it isn't.
fn unpinned(url: &str) -> Option<&'static str> {
    for cdn in ["cdn.jsdelivr.net/npm/", "unpkg.com/", "esm.sh/"] {
        if let Some(i) = url.find(cdn) {
            let rest = &url[i + cdn.len()..];
            // `@scope/name@1.2.3/…` or `name@1.2.3/…`
            let pkg_end = if rest.starts_with('@') {
                rest.match_indices('/')
                    .nth(1)
                    .map_or(rest.len(), |(i, _)| i)
            } else {
                rest.find('/').unwrap_or(rest.len())
            };
            let pkg = &rest[..pkg_end];
            let version = pkg.get(1..).and_then(|p| p.split_once('@')).map(|(_, v)| v);
            return match version {
                None => Some("A CDN link with no version"),
                Some("latest" | "next") => Some("A CDN link on a moving version"),
                Some(_) => None,
            };
        }
    }
    None
}

/// `word` alone in `text` (not inside another word).
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_alphanumeric())
    })
}

/// The page's text between tags, outside scripts, styles and comments:
/// (offset, text) runs.
fn visible_text(lower: &str) -> Vec<(usize, String)> {
    let mut out = vec![];
    let mut i = 0;
    let b = lower.as_bytes();
    while i < b.len() {
        if lower[i..].starts_with("<!--") {
            i = lower[i..].find("-->").map_or(b.len(), |e| i + e + 3);
            continue;
        }
        if b[i] == b'<' {
            let skip_to = ["script", "style"]
                .iter()
                .find(|n| lower[i + 1..].starts_with(*n))
                .and_then(|n| lower[i..].find(&format!("</{n}")).map(|e| i + e));
            i = match skip_to {
                Some(e) => e,
                None => lower[i..].find('>').map_or(b.len(), |e| i + e + 1),
            };
            continue;
        }
        let end = lower[i..].find('<').map_or(b.len(), |e| i + e);
        let t = lower[i..end].trim();
        if !t.is_empty() {
            out.push((i, t.to_string()));
        }
        i = end;
    }
    out
}

struct Tag<'a> {
    at: usize,
    name: &'a str,
    close: bool,
    self_closed: bool,
}

/// Every tag, outside comments and inside-script text.
fn tags(lower: &str) -> Vec<Tag<'_>> {
    let mut out = vec![];
    let mut i = 0;
    let mut raw: Option<&str> = None;
    while let Some(off) = lower[i..].find('<') {
        let at = i + off;
        if lower[at..].starts_with("<!--") {
            i = lower[at..].find("-->").map_or(lower.len(), |e| at + e + 3);
            continue;
        }
        let close = lower[at + 1..].starts_with('/');
        let start = at + 1 + usize::from(close);
        let name_end = lower[start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .map_or(lower.len(), |e| start + e);
        let name = &lower[start..name_end];
        let end = lower[at..].find('>').map_or(lower.len(), |e| at + e);
        i = (end + 1).min(lower.len());
        if name.is_empty() {
            continue;
        }
        // Inside <script> or <style>, only its own close tag counts.
        if let Some(r) = raw {
            if !(close && name == r) {
                continue;
            }
            raw = None;
        } else if !close && (name == "script" || name == "style") {
            raw = Some(name);
        }
        out.push(Tag {
            at,
            name,
            close,
            self_closed: lower[at..end].ends_with('/'),
        });
    }
    out
}

/// `src` and `href` values, with their offsets.
fn links(html: &str) -> Vec<(usize, String)> {
    let mut out = vec![];
    for attr in ["src=", "href="] {
        for (i, _) in html.match_indices(attr) {
            // Only attributes: after whitespace inside a tag.
            if !html[..i].ends_with(|c: char| c.is_whitespace()) {
                continue;
            }
            let rest = &html[i + attr.len()..];
            let value = match rest.chars().next() {
                Some(q @ ('"' | '\'')) => rest[1..].split(q).next(),
                _ => rest.split(|c: char| c.is_whitespace() || c == '>').next(),
            };
            if let Some(v) = value {
                out.push((i, v.to_string()));
            }
        }
    }
    out
}

/// What runs in the page to find content that overflows: the page wider
/// than the window, a slide's content outside its slide, text clipped by
/// its box. Returns a JSON list of strings.
pub const OVERFLOW_JS: &str = r#"(() => {
  const out = [];
  const name = (el) => {
    let s = el.tagName.toLowerCase();
    if (el.id) s += '#' + el.id;
    else if (el.classList.length) s += '.' + [...el.classList].slice(0, 2).join('.');
    const t = (el.innerText || '').trim().replace(/\s+/g, ' ').slice(0, 40);
    return t ? `${s} ("${t}")` : s;
  };
  const de = document.documentElement;
  if (de.scrollWidth > innerWidth + 2)
    out.push(`The page is ${de.scrollWidth}px wide in a ${innerWidth}px window: something sticks out sideways.`);
  const slides = [...document.querySelectorAll('section.slide, .slide, [data-slide]')];
  slides.forEach((s, n) => {
    const r = s.getBoundingClientRect();
    if (!r.width || !r.height) return;
    for (const el of s.querySelectorAll('*')) {
      const e = el.getBoundingClientRect();
      if (!e.width || !e.height) continue;
      if (e.right > r.right + 2 || e.bottom > r.bottom + 2 || e.left < r.left - 2 || e.top < r.top - 2) {
        out.push(`Slide ${n + 1}: ${name(el)} goes outside the slide.`);
        break;
      }
    }
  });
  for (const el of document.body.querySelectorAll('*')) {
    const cs = getComputedStyle(el);
    if (!/(hidden|clip)/.test(cs.overflow + cs.overflowX + cs.overflowY)) continue;
    if (!(el.innerText || '').trim()) continue;
    if (el.scrollHeight > el.clientHeight + 2 || el.scrollWidth > el.clientWidth + 2) {
      if (cs.textOverflow === 'ellipsis' || /line-clamp/.test(el.getAttribute('style') || '')) continue;
      out.push(`${name(el)} is cut off by its box.`);
      if (out.length > 20) break;
    }
  }
  return out;
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width\"><title>Deck</title>\
<style>body{margin:0} .a > .b {color:red} /* <div> */</style></head>";

    fn page(body: &str) -> String {
        format!("{HEAD}<body>{body}<p>Enough words here to make a real page of it, more than two hundred bytes in all.</p></body></html>")
    }

    fn messages(html: &str) -> Vec<String> {
        check(html, Path::new("/nowhere/index.html"))
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn a_clean_page_has_no_findings() {
        let html = page("<section class=\"slide\"><h1>Why it matters</h1><br/><img src=\"https://example.com/a.png\"></section><script>if (a < b && c > d) { x = '<div>'; }</script>");
        assert_eq!(messages(&html), Vec::<String>::new());
    }

    #[test]
    fn placeholders_and_stubs_are_errors() {
        let html = page("<h1>Lorem ipsum</h1><p>TODO: numbers</p><!-- rest of the slides here -->");
        let m = messages(&html);
        assert!(m.iter().any(|m| m.contains("lorem ipsum")), "{m:?}");
        assert!(m.iter().any(|m| m.contains("\"TODO\"")), "{m:?}");
        assert!(
            m.iter().any(|m| m.contains("stands in for content")),
            "{m:?}"
        );
        // A word that only contains one isn't one.
        assert!(messages(&page("<p>Mastodon todos</p>")).is_empty());
    }

    #[test]
    fn cut_short_markup_is_found() {
        let html = format!(
            "{HEAD}<body><section class=\"slide\"><div><h1>One</h1></div>{}",
            "x".repeat(200)
        );
        let m = messages(&html);
        assert!(
            m.iter()
                .any(|m| m.starts_with("<section> opened 1 times and closed 0")),
            "{m:?}"
        );
        assert!(m.iter().any(|m| m.contains("No </html>")), "{m:?}");
    }

    #[test]
    fn missing_files_and_unpinned_cdns() {
        let dir = std::env::temp_dir().join(format!("forge-lint-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("img/here.png"), b"png").unwrap();
        let html = page(
            "<img src=\"img/here.png\"><img src=\"img/gone.png?v=2\"><a href=\"#top\">top</a>\
<script src=\"https://cdn.jsdelivr.net/npm/gsap@3.12.5/dist/gsap.min.js\"></script>\
<script src=\"https://unpkg.com/react/umd/react.js\"></script>\
<script src=\"https://cdn.jsdelivr.net/npm/@scope/pkg@latest/x.js\"></script>",
        );
        let m: Vec<_> = check(&html, &dir.join("index.html"))
            .into_iter()
            .map(|f| f.message)
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(m.iter().any(|m| m.ends_with("img/gone.png?v=2")), "{m:?}");
        assert!(
            !m.iter()
                .any(|m| m.contains("here.png") || m.contains("gsap")),
            "{m:?}"
        );
        assert!(
            m.iter()
                .any(|m| m.starts_with("A CDN link with no version") && m.contains("react")),
            "{m:?}"
        );
        assert!(
            m.iter()
                .any(|m| m.starts_with("A CDN link on a moving version")),
            "{m:?}"
        );
    }

    #[test]
    fn an_empty_file_and_a_bare_head() {
        assert_eq!(messages("<html></html>"), ["The file is almost empty."]);
        let bare = format!(
            "<html><head></head><body>{}</body></html>",
            "<p>words</p>".repeat(30)
        );
        let m = messages(&bare);
        assert!(m.contains(&"No <title>.".to_string()), "{m:?}");
        assert!(m.iter().any(|m| m.starts_with("No viewport")), "{m:?}");
    }

    #[test]
    fn findings_read_with_their_line() {
        let f = Finding {
            level: Level::Error,
            line: 3,
            message: "x".into(),
        };
        assert_eq!(f.to_string(), "error, line 3: x");
    }
}
