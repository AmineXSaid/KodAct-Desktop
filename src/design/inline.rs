//! A page as one HTML file: its stylesheets, scripts, pictures, fonts and
//! media, from beside it on disk (or, when `fetch` says so, from the web),
//! put into the file itself, so it opens anywhere with nothing else.

use std::path::{Path, PathBuf};

/// The most of any one file put inline.
const FILE_MAX: usize = 25 * 1024 * 1024;

/// What `single_file` did: the HTML, and what it couldn't put inline.
pub struct Inlined {
    pub html: String,
    pub missing: Vec<String>,
}

/// Where the page's files come from.
struct Source<'a> {
    fetch: &'a mut dyn FnMut(&str) -> Option<Vec<u8>>,
    missing: Vec<String>,
}

/// A file's media type, by its name.
pub fn mime_of(name: &str) -> &'static str {
    mime(name)
}

fn mime(name: &str) -> &'static str {
    let ext = name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// A reference that is already in the file, or isn't a file.
fn is_inline(r: &str) -> bool {
    let r = r.trim();
    r.is_empty()
        || r.starts_with('#')
        || r.starts_with("data:")
        || r.starts_with("javascript:")
        || r.starts_with("mailto:")
        || r.starts_with("tel:")
        || r.starts_with("blob:")
        || r.starts_with("about:")
}

fn is_web(r: &str) -> bool {
    r.starts_with("http://") || r.starts_with("https://") || r.starts_with("//")
}

/// Where `r` points from a file at `base` (a folder on disk, or a web
/// address's folder).
fn resolve(base: &Base, r: &str) -> Base {
    let r = r.trim();
    let clean = r.split(['?', '#']).next().unwrap_or(r);
    if is_web(r) {
        let url = if r.starts_with("//") {
            format!("https:{r}")
        } else {
            r.to_string()
        };
        return Base::Web(url);
    }
    match base {
        Base::Disk(dir) => {
            let decoded = percent_decode(clean);
            let p = if let Some(abs) = decoded.strip_prefix("file://") {
                PathBuf::from(abs)
            } else {
                dir.join(decoded)
            };
            Base::Disk(p)
        }
        Base::Web(url) => {
            if r.starts_with('/') {
                let origin: String = url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
                Base::Web(format!("{origin}{r}"))
            } else {
                let dir = url.rsplit_once('/').map_or(url.as_str(), |(d, _)| d);
                Base::Web(format!("{dir}/{r}"))
            }
        }
    }
}

/// `%20` and the like, decoded.
pub fn decode(s: &str) -> String {
    percent_decode(s)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Clone)]
enum Base {
    Disk(PathBuf),
    Web(String),
}

impl Base {
    fn dir(&self) -> Base {
        match self {
            Base::Disk(p) => Base::Disk(p.parent().map(Path::to_path_buf).unwrap_or_default()),
            // A web base is its file: `resolve` drops the name.
            Base::Web(u) => Base::Web(u.clone()),
        }
    }

    fn name(&self) -> String {
        match self {
            Base::Disk(p) => p.display().to_string(),
            Base::Web(u) => u.clone(),
        }
    }
}

impl Source<'_> {
    fn get(&mut self, at: &Base) -> Option<Vec<u8>> {
        let got = match at {
            Base::Disk(p) => std::fs::read(p).ok(),
            Base::Web(u) => (self.fetch)(u),
        }
        .filter(|b| b.len() <= FILE_MAX);
        if got.is_none() {
            self.missing.push(at.name());
        }
        got
    }

    fn data_uri(&mut self, base: &Base, r: &str) -> Option<String> {
        let at = resolve(base, r);
        let bytes = self.get(&at)?;
        let name = at.name();
        let kind = mime(&name);
        // A stylesheet's own references, in turn.
        if kind == "text/css" {
            let css = self.css(&String::from_utf8_lossy(&bytes), &at.dir());
            return Some(format!(
                "data:text/css;base64,{}",
                crate::attach::base64(css.as_bytes())
            ));
        }
        Some(format!(
            "data:{kind};base64,{}",
            crate::attach::base64(&bytes)
        ))
    }

    /// A stylesheet with its `url(…)`s and `@import`s inline.
    fn css(&mut self, css: &str, dir: &Base) -> String {
        let mut out = String::with_capacity(css.len());
        let mut rest = css;
        while let Some(i) = rest.find("url(") {
            out.push_str(&rest[..i]);
            let after = &rest[i + 4..];
            let Some(end) = after.find(')') else {
                out.push_str(&rest[i..]);
                return out;
            };
            let raw = after[..end].trim().trim_matches(['"', '\'']);
            match (!is_inline(raw)).then(|| self.data_uri(dir, raw)).flatten() {
                Some(uri) => out.push_str(&format!("url(\"{uri}\")")),
                None => out.push_str(&rest[i..i + 4 + end + 1]),
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        // `@import "x.css";` (the url() form was handled above).
        let mut done = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(i) = rest.find("@import") {
            done.push_str(&rest[..i]);
            let after = &rest[i + 7..];
            let Some(semi) = after.find(';') else {
                done.push_str(&rest[i..]);
                rest = "";
                break;
            };
            let what = after[..semi].trim();
            let quoted = what.trim_start_matches(['"', '\'']);
            let target = quoted.split(['"', '\'']).next().unwrap_or("");
            let is_url = what.starts_with("url(");
            match (!is_url && !target.is_empty())
                .then(|| {
                    self.get(&resolve(dir, target))
                        .map(|b| (b, resolve(dir, target)))
                })
                .flatten()
            {
                Some((b, at)) => done.push_str(&self.css(&String::from_utf8_lossy(&b), &at.dir())),
                None => done.push_str(&rest[i..i + 7 + semi + 1]),
            }
            rest = &after[semi + 1..];
        }
        done.push_str(rest);
        done
    }
}

/// The value of attribute `name` in a tag's text, and where it is.
fn attr(tag: &str, name: &str) -> Option<(std::ops::Range<usize>, String)> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before = lower[..at].chars().last();
        if !matches!(before, Some(c) if c.is_whitespace()) {
            continue;
        }
        let rest = &tag[from..];
        let trimmed = rest.trim_start();
        let Some(after_eq) = trimmed.strip_prefix('=') else {
            continue;
        };
        let v = after_eq.trim_start();
        let start = tag.len() - v.len();
        let (val, len) = match v.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let end = v[1..].find(q)?;
                (v[1..1 + end].to_string(), end + 2)
            }
            _ => {
                let end = v
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(v.len());
                (v[..end].to_string(), end)
            }
        };
        return Some((start..start + len, val));
    }
    None
}

/// `tag` without attribute `name`, whose value is at `range`.
fn remove_attr(tag: &str, name: &str, range: std::ops::Range<usize>) -> String {
    let start = tag[..range.start]
        .to_ascii_lowercase()
        .rfind(name)
        .unwrap_or(range.start);
    format!("{}{}", tag[..start].trim_end(), &tag[range.end..])
}

fn set_attr(tag: &str, range: std::ops::Range<usize>, value: &str) -> String {
    format!(
        "{}\"{}\"{}",
        &tag[..range.start],
        value.replace('"', "&quot;"),
        &tag[range.end..]
    )
}

/// `html` (the page at `path`) as one file. `fetch` gets a web address's
/// bytes (or None to leave it a link).
pub fn single_file(
    html: &str,
    path: &Path,
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Inlined {
    let mut src = Source {
        fetch,
        missing: vec![],
    };
    let dir = Base::Disk(path.parent().map(Path::to_path_buf).unwrap_or_default());
    let mut out = String::with_capacity(html.len() * 2);
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // Comments pass as they are.
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map_or(rest.len(), |e| e + 3);
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        let Some(close) = rest.find('>') else {
            break;
        };
        let tag = &rest[..=close];
        let name: String = tag[1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        rest = &rest[close + 1..];
        match name.as_str() {
            "link" => {
                let rel = attr(tag, "rel")
                    .map(|(_, v)| v.to_ascii_lowercase())
                    .unwrap_or_default();
                let href = attr(tag, "href");
                match href {
                    Some((_, h)) if rel.contains("stylesheet") && !is_inline(&h) => {
                        let at = resolve(&dir, &h);
                        match src.get(&at) {
                            Some(b) => {
                                let css = src.css(&String::from_utf8_lossy(&b), &at.dir());
                                let media = attr(tag, "media")
                                    .map(|(_, m)| format!(" media=\"{m}\""))
                                    .unwrap_or_default();
                                out.push_str(&format!("<style{media}>{css}</style>"));
                            }
                            None => out.push_str(tag),
                        }
                    }
                    Some((range, h))
                        if (rel.contains("icon") || rel.contains("preload")) && !is_inline(&h) =>
                    {
                        match src.data_uri(&dir, &h) {
                            Some(uri) => out.push_str(&set_attr(tag, range, &uri)),
                            None => out.push_str(tag),
                        }
                    }
                    // Preconnects and the like mean nothing in one file.
                    _ if rel.contains("preconnect") || rel.contains("dns-prefetch") => {}
                    _ => out.push_str(tag),
                }
            }
            "script" => {
                // Its body (up to </script>) passes through, or, with a
                // src, takes the file's place.
                let end = rest
                    .to_ascii_lowercase()
                    .find("</script")
                    .unwrap_or(rest.len());
                let body = &rest[..end];
                rest = &rest[end..];
                match attr(tag, "src") {
                    Some((range, s)) if !is_inline(&s) => {
                        let at = resolve(&dir, &s);
                        match src.get(&at) {
                            Some(b) => {
                                let js =
                                    String::from_utf8_lossy(&b).replace("</script", "<\\/script");
                                out.push_str(&remove_attr(tag, "src", range));
                                out.push_str(&js);
                            }
                            None => {
                                out.push_str(tag);
                                out.push_str(body);
                            }
                        }
                    }
                    _ => {
                        out.push_str(tag);
                        out.push_str(body);
                    }
                }
            }
            "style" => {
                let end = rest
                    .to_ascii_lowercase()
                    .find("</style")
                    .unwrap_or(rest.len());
                let css = src.css(&rest[..end], &dir);
                rest = &rest[end..];
                out.push_str(tag);
                out.push_str(&css);
            }
            "img" | "source" | "video" | "audio" | "input" | "track" | "embed" | "image"
            | "use" => {
                let mut t = tag.to_string();
                for a in ["src", "poster", "href", "xlink:href"] {
                    if let Some((range, v)) = attr(&t, a) {
                        if !is_inline(&v) {
                            if let Some(uri) = src.data_uri(&dir, &v) {
                                t = set_attr(&t, range, &uri);
                            }
                        }
                    }
                }
                // A srcset's pictures are left out: src stands for them.
                if let Some((range, _)) = attr(&t, "srcset") {
                    t = remove_attr(&t, "srcset", range);
                }
                out.push_str(&inline_style(&t, &mut src, &dir));
            }
            _ => out.push_str(&inline_style(tag, &mut src, &dir)),
        }
    }
    out.push_str(rest);
    Inlined {
        html: out,
        missing: src.missing,
    }
}

/// A tag's `style="…"` with its `url(…)`s inline.
fn inline_style(tag: &str, src: &mut Source, dir: &Base) -> String {
    match attr(tag, "style") {
        Some((range, v)) if v.contains("url(") => {
            let css = src.css(&v.replace("&quot;", "\""), dir).replace('"', "'");
            set_attr(tag, range, &css)
        }
        _ => tag.to_string(),
    }
}

/// The page's references that still leave the file: what a single file
/// should have none of (links to other pages aside).
pub fn outside_refs(html: &str) -> Vec<String> {
    let mut found = vec![];
    for key in ["src=", "url(", "href="] {
        let mut rest = html;
        while let Some(i) = rest.find(key) {
            rest = &rest[i + key.len()..];
            let v = rest.trim_start_matches(['"', '\'']);
            let end = v.find(['"', '\'', ')', ' ', '>']).unwrap_or(v.len());
            let v = &v[..end];
            // Links to pages are for clicking, not part of this one.
            if key == "href=" {
                let before = &html[..html.len() - rest.len()];
                let tag_start = before.rfind('<').unwrap_or(0);
                if !before[tag_start..].starts_with("<link") {
                    continue;
                }
            }
            if !is_inline(v) {
                found.push(v.to_string());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_and_its_files_become_one() {
        let dir = std::env::temp_dir().join(format!("forge-inline-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("css/fonts")).unwrap();
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::write(dir.join("css/site.css"), "@import \"more.css\";\nbody{background:url('../img/bg.png')}\n@font-face{src:url(fonts/a.woff2)}").unwrap();
        std::fs::write(dir.join("css/more.css"), "h1{color:red}").unwrap();
        std::fs::write(dir.join("css/fonts/a.woff2"), b"wOF2").unwrap();
        std::fs::write(dir.join("img/bg.png"), b"\x89PNG").unwrap();
        std::fs::write(dir.join("img/a b.png"), b"\x89PNG2").unwrap();
        std::fs::write(dir.join("app.js"), "console.log('</script>')").unwrap();
        let page = dir.join("index.html");
        let html = r#"<!doctype html><html><head><link rel="preconnect" href="https://x.com"><link rel="stylesheet" href="css/site.css"><link rel=stylesheet href="https://cdn.example/x.css"><script src="app.js"></script></head>
<body><img src="img/a%20b.png" srcset="img/a%20b.png 2x" alt="x"><div style="background:url(img/bg.png)">hi</div><a href="other.html">next</a><img src="gone.png"><!-- <img src="no.png"> --></body></html>"#;
        let mut asked = vec![];
        let r = single_file(html, &page, &mut |u| {
            asked.push(u.to_string());
            (u == "https://cdn.example/x.css").then(|| b"p{margin:0}".to_vec())
        });
        let out = &r.html;
        assert!(out.contains("h1{color:red}"), "{out}");
        assert!(out.contains("p{margin:0}"), "{out}");
        assert!(out.contains("console.log('<\\/script>')"), "{out}");
        assert!(!out.contains("app.js"), "{out}");
        assert!(out.contains("data:font/woff2;base64,"), "{out}");
        assert!(out.contains("data:image/png;base64,"), "{out}");
        assert!(!out.contains("srcset"), "{out}");
        assert!(!out.contains("preconnect"), "{out}");
        assert!(out.contains(r#"href="other.html""#));
        assert!(out.contains("<!-- <img src=\"no.png\"> -->"));
        assert_eq!(asked, vec!["https://cdn.example/x.css".to_string()]);
        assert_eq!(r.missing.len(), 1, "{:?}", r.missing);
        assert!(r.missing[0].ends_with("gone.png"));
        let outside = outside_refs(out);
        assert!(
            outside
                .iter()
                .all(|o| o.contains("gone.png") || o.contains("no.png")),
            "{outside:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
