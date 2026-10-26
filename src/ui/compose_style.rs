//! Markdown as you write it in the composer: what you type stays text, but
//! **bold** reads bold, *italic* italic, `code` sits on its well, a heading
//! is strong, a quote muted, a list's marker in the accent, a link a link,
//! and the markers themselves (`**`, `` ` ``, `#`) step back. A `/skill`,
//! `/command` or `@path` reads as it does once sent. Each byte gets a style,
//! then runs of one style become the editor's decorations, so none overlap.

use std::ops::Range;

use gpui::{FontStyle, FontWeight, HighlightStyle, Hsla, StrikethroughStyle, UnderlineStyle};

use super::autocomplete::Mark;
use crate::theme::Theme;

#[derive(Clone, Copy, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    color: Option<Hsla>,
    bg: Option<Hsla>,
}

impl Style {
    fn highlight(self) -> HighlightStyle {
        HighlightStyle {
            color: self.color,
            background_color: self.bg,
            font_weight: self.bold.then_some(FontWeight::SEMIBOLD),
            font_style: self.italic.then_some(FontStyle::Italic),
            strikethrough: self.strike.then(|| StrikethroughStyle {
                thickness: gpui::px(1.),
                color: None,
            }),
            underline: self.underline.then(|| UnderlineStyle {
                thickness: gpui::px(1.),
                color: None,
                wavy: false,
            }),
            ..Default::default()
        }
    }
}

/// The styled ranges of `text`: its Markdown, and its mentions (`marks`).
pub fn styles(
    text: &str,
    marks: &[(Range<usize>, Mark)],
    t: &Theme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut s = vec![Style::default(); text.len()];
    let marker = t.colors.text_muted.opacity(0.6);
    let paint = |s: &mut [Style], r: Range<usize>, f: &dyn Fn(&mut Style)| {
        for st in &mut s[r] {
            f(st);
        }
    };
    let mut fenced = false;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let start = at;
        at += line.len();
        let body = line.trim_end_matches(['\n', '\r']);
        let end = start + body.len();
        let lead = body.len() - body.trim_start().len();
        let trimmed = body.trim_start();
        // A fence and what is inside it: code, untouched by the rest.
        if trimmed.starts_with("```") {
            fenced = !fenced;
            paint(&mut s, start..end, &|st| st.color = Some(marker));
            continue;
        }
        if fenced {
            paint(&mut s, start..end, &|st| st.bg = Some(t.extra.inset));
            continue;
        }
        let mut from = start;
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let m = start + lead + hashes + 1;
            paint(&mut s, start..m, &|st| st.color = Some(marker));
            paint(&mut s, m..end, &|st| {
                st.bold = true;
                st.color = Some(t.colors.text_strong);
            });
            from = m;
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            let m = start + lead + 1 + (rest.len() - rest.trim_start().len());
            paint(&mut s, start..m, &|st| st.color = Some(marker));
            paint(&mut s, m..end, &|st| {
                st.italic = true;
                st.color = Some(t.colors.text_muted);
            });
            from = m;
        } else {
            let bullet = trimmed.starts_with(['-', '*', '+']) && trimmed[1..].starts_with(' ');
            let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
            let numbered = digits > 0 && trimmed[digits..].starts_with(". ");
            if bullet || numbered {
                let m = start + lead + if bullet { 1 } else { digits + 1 };
                paint(&mut s, start + lead..m, &|st| {
                    st.color = Some(t.colors.primary);
                    st.bold = true;
                });
                from = m;
            }
        }
        inline(&mut s, text, from..end, marker, t);
    }
    for (r, mark) in marks {
        let r = r.start.min(text.len())..r.end.min(text.len());
        paint(&mut s, r, &|st| {
            st.bold = true;
            st.color = Some(match mark {
                Mark::Skill => t.extra.skill,
                _ => t.colors.primary,
            });
        });
    }
    // Runs of one style, the plain ones left out.
    let mut out = vec![];
    let mut i = 0;
    while i < s.len() {
        let mut j = i + 1;
        while j < s.len() && s[j] == s[i] {
            j += 1;
        }
        if s[i] != Style::default() && text.is_char_boundary(i) && text.is_char_boundary(j) {
            out.push((i..j, s[i].highlight()));
        }
        i = j;
    }
    out
}

/// Inline Markdown in `r`: code first (nothing else inside it), then bold,
/// italic, strikethrough and links.
fn inline(s: &mut [Style], text: &str, r: Range<usize>, marker: Hsla, t: &Theme) {
    let line = &text[r.clone()];
    let b = line.as_bytes();
    let base = r.start;
    let mut code = vec![false; line.len()];
    // `code`
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`' {
            if let Some(k) = line[i + 1..].find('`') {
                let close = i + 1 + k;
                for (n, st) in s[base + i..=base + close].iter_mut().enumerate() {
                    let inside = n > 0 && n < close - i;
                    if inside {
                        st.bg = Some(t.extra.inset);
                    } else {
                        st.color = Some(marker);
                    }
                }
                code[i..=close].iter_mut().for_each(|c| *c = true);
                i = close + 1;
                continue;
            }
        }
        i += 1;
    }
    // A pair of `open` markers around text, not inside code: the markers dim,
    // the inside styled by `f`.
    let mut pairs = |open: &str, f: &dyn Fn(&mut Style)| {
        let mut i = 0;
        while let Some(k) = line[i..].find(open) {
            let a = i + k;
            if code[a] {
                i = a + open.len();
                continue;
            }
            let Some(k2) = line[a + open.len()..].find(open) else {
                break;
            };
            let c = a + open.len() + k2;
            let inner = a + open.len()..c;
            if inner.is_empty() || code[c] || line[inner.clone()].starts_with(' ') {
                i = a + open.len();
                continue;
            }
            for st in &mut s[base + a..base + a + open.len()] {
                st.color = Some(marker);
            }
            for st in &mut s[base + c..base + c + open.len()] {
                st.color = Some(marker);
            }
            for st in &mut s[base + inner.start..base + inner.end] {
                f(st);
            }
            i = c + open.len();
        }
    };
    pairs("**", &|st| st.bold = true);
    pairs("__", &|st| st.bold = true);
    pairs("~~", &|st| st.strike = true);
    // *italic* / _italic_: single markers not part of a double one.
    for m in [b'*', b'_'] {
        let mut open: Option<usize> = None;
        for i in 0..b.len() {
            let single = b[i] == m
                && !code[i]
                && (i == 0 || b[i - 1] != m)
                && (i + 1 >= b.len() || b[i + 1] != m);
            if !single {
                continue;
            }
            match open {
                None if i + 1 < b.len() && b[i + 1] != b' ' => open = Some(i),
                Some(a) if i > a + 1 && b[i - 1] != b' ' => {
                    s[base + a].color = Some(marker);
                    s[base + i].color = Some(marker);
                    for st in &mut s[base + a + 1..base + i] {
                        st.italic = true;
                    }
                    open = None;
                }
                _ => {}
            }
        }
    }
    // [text](url)
    let mut i = 0;
    while let Some(k) = line[i..].find('[') {
        let a = i + k;
        let Some(k2) = line[a..].find("](") else {
            break;
        };
        let mid = a + k2;
        let Some(k3) = line[mid..].find(')') else {
            break;
        };
        let close = mid + k3;
        if !code[a] {
            s[base + a].color = Some(marker);
            for st in &mut s[base + a + 1..base + mid] {
                st.color = Some(t.colors.primary);
                st.underline = true;
            }
            for st in &mut s[base + mid..=base + close] {
                st.color = Some(marker);
            }
        }
        i = close + 1;
    }
}

/// Wrap the selection `r` of `text` in `marker` (Ctrl+B, Ctrl+I, Ctrl+E), or
/// unwrap it when it is wrapped already. The new text and selection.
pub fn toggle(text: &str, r: Range<usize>, marker: &str) -> (String, Range<usize>) {
    let n = marker.len();
    let before = &text[..r.start];
    let after = &text[r.end..];
    if before.ends_with(marker) && after.starts_with(marker) {
        let out = format!(
            "{}{}{}",
            &before[..before.len() - n],
            &text[r.clone()],
            &after[n..]
        );
        return (out, r.start - n..r.end - n);
    }
    let out = format!("{before}{marker}{}{marker}{after}", &text[r.clone()]);
    (out, r.start + n..r.end + n)
}

/// Pasted code or logs, recognised the way an issue tracker does: two or
/// more lines that read as code or a log come back fenced, with a language
/// when one is plain (`rust`, `python`, `json`, `log`, ...). Prose, a fence
/// already there, or a single line come back `None` and paste as they are.
pub fn code_block(text: &str) -> Option<String> {
    let body = text.trim_matches('\n').trim_end();
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 || body.contains("```") {
        return None;
    }
    let lang = language(body);
    let cody = lines.iter().filter(|l| code_line(l)).count();
    let prose = lines.iter().filter(|l| prose_line(l)).count();
    if lang.is_none() && (cody * 5 < lines.len() * 2 || prose * 2 > lines.len()) {
        return None;
    }
    Some(format!("```{}\n{body}\n```", lang.unwrap_or("")))
}

/// A line that reads as code or a log line.
fn code_line(l: &str) -> bool {
    let t = l.trim();
    l.starts_with("    ")
        || l.starts_with('\t')
        || t.ends_with([';', '{', '}', '(', ')', '[', ']', ','])
        || ["=>", "->", "::", "==", "!=", "&&", "||", "</", "/>", "();"]
            .iter()
            .any(|p| t.contains(p))
        || [
            "#include", "import ", "fn ", "def ", "let ", "const ", "var ", "return ", "if (",
            "for (", "$ ", "at ", "//",
        ]
        .iter()
        .any(|p| t.starts_with(p))
        || log_line(t)
}

/// A timestamp or a level at the start: a log line.
fn log_line(t: &str) -> bool {
    let b = t.trim_start_matches(['[', '(']).as_bytes();
    let date = b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-';
    let clock =
        b.len() >= 8 && b[2] == b':' && b[5] == b':' && b[..2].iter().all(u8::is_ascii_digit);
    let head: String = t.chars().take(40).collect();
    date || clock
        || [
            "ERROR", "WARN", "INFO", "DEBUG", "TRACE", "FATAL", "CRITICAL",
        ]
        .iter()
        .any(|lv| head.contains(lv))
}

/// A sentence: several words, ending as prose does, no code punctuation.
fn prose_line(l: &str) -> bool {
    let t = l.trim();
    t.split_whitespace().count() >= 6
        && t.ends_with(['.', '?', '!', ':'])
        && !t.contains(['{', '}', ';', '='])
}

/// The fence's language, when the text says it plainly.
fn language(body: &str) -> Option<&'static str> {
    let t = body.trim_start();
    let has = |ps: &[&str]| ps.iter().any(|p| body.contains(p));
    if (t.starts_with('{') || t.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(body).is_ok()
    {
        return Some("json");
    }
    if t.starts_with("diff --git")
        || (t.starts_with("--- ") && body.contains("\n+++ "))
        || t.starts_with("@@ ")
    {
        return Some("diff");
    }
    if has(&["Traceback (most recent call last)"])
        || (has(&["def ", "self.", "elif "]) && has(&[":\n"]))
    {
        return Some("python");
    }
    if has(&[
        "panicked at",
        "error[E",
        "fn main",
        "impl ",
        "let mut ",
        "pub fn ",
        "::new(",
    ]) {
        return Some("rust");
    }
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.iter().filter(|l| log_line(l.trim())).count() * 2 >= lines.len() {
        return Some("log");
    }
    if has(&["interface ", ": string", ": number"]) && has(&["=>", "const ", "export "]) {
        return Some("ts");
    }
    if has(&["const ", "function ", "console.", "=> {", "require("]) {
        return Some("js");
    }
    if t.starts_with('<') && has(&["</", "/>"]) {
        return Some("html");
    }
    let upper = body.to_uppercase();
    if upper.contains("SELECT ") && upper.contains(" FROM ") {
        return Some("sql");
    }
    if t.starts_with("#!/bin/")
        || lines.iter().filter(|l| l.starts_with("$ ")).count() * 2 >= lines.len()
    {
        return Some("bash");
    }
    if has(&["#include", "int main("]) {
        return Some("c");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(String, bool, bool, bool)> {
        let t = crate::theme::Theme::light();
        styles(text, &[], &t)
            .into_iter()
            .map(|(r, h)| {
                (
                    text[r].to_string(),
                    h.font_weight.is_some(),
                    h.font_style.is_some(),
                    h.background_color.is_some(),
                )
            })
            .collect()
    }

    #[test]
    fn markdown_styles_as_typed() {
        let k = kinds("say **hi** and *there* with `x*y*z`");
        assert!(k.contains(&("hi".into(), true, false, false)));
        assert!(k.contains(&("there".into(), false, true, false)));
        // Inside code, nothing else applies.
        assert!(k.contains(&("x*y*z".into(), false, false, true)));
        let h = kinds("# Title\n- item");
        assert!(h.contains(&("Title".into(), true, false, false)));
        assert!(h.iter().any(|(s, b, ..)| s == "-" && *b));
        // Unfinished markers style nothing.
        assert!(kinds("a ** b").is_empty());
    }

    #[test]
    fn shortcuts_wrap_and_unwrap() {
        assert_eq!(
            toggle("make it bold", 8..12, "**"),
            ("make it **bold**".into(), 10..14)
        );
        assert_eq!(
            toggle("make it **bold**", 10..14, "**"),
            ("make it bold".into(), 8..12)
        );
        assert_eq!(toggle("x", 1..1, "`"), ("x``".into(), 2..2));
    }

    #[test]
    fn pasted_code_and_logs_are_fenced() {
        let rust = "fn main() {\n    let x = 1;\n}";
        assert_eq!(code_block(rust).unwrap(), format!("```rust\n{rust}\n```"));
        let log = "2026-10-21 10:02:11 INFO starting\n2026-10-21 10:02:12 ERROR boom";
        assert!(code_block(log).unwrap().starts_with("```log\n"));
        assert!(code_block("{\n  \"a\": 1\n}")
            .unwrap()
            .starts_with("```json\n"));
        let py = "Traceback (most recent call last):\n  File \"x.py\", line 1\nValueError: no";
        assert!(code_block(py).unwrap().starts_with("```python\n"));
        // Prose, one line, and text already fenced paste as they are.
        assert_eq!(code_block("Hello there, I wanted to ask about this thing today.\nAnd here is one more sentence for you to read."), None);
        assert_eq!(code_block("let x = 1;"), None);
        assert_eq!(code_block("```\nx\n```"), None);
    }
}
