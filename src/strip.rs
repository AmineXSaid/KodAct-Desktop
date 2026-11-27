//! What a file's card shows of it: a strip of frames, each a part the file is
//! made of, and a line saying what it is. A deck shows its slides, a Word file
//! or a PDF its pages, a workbook its sheets, Markdown its sections (split at
//! its headings), code its functions, other text its stretches of lines, a
//! picture itself. Built off the UI thread (`build` reads and renders); the
//! card draws what it gets (`ui/file_strip.rs`).

use std::sync::Arc;

use crate::state::FileRef;

/// The shape a strip's frames take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// 16:9, a deck's slides.
    Slide,
    /// A printed page, portrait (Word, PDF).
    Page,
    /// A sheet's first rows and columns.
    Sheet,
    /// A Markdown section: its heading and first words.
    Section,
    /// Lines of code or text.
    Lines,
    /// A picture, as itself.
    Picture,
}

#[derive(Clone)]
pub enum Art {
    /// A page or slide as it prints, and its size in pixels.
    Page {
        image: Arc<gpui::Image>,
        width: u32,
        height: u32,
    },
    /// Cells as text, a row each; `true` marks a number (right-aligned).
    Cells(Vec<Vec<(String, bool)>>),
    /// A section's heading and its opening words.
    Section { heading: String, body: String },
    /// A stretch of lines.
    Lines(Vec<String>),
}

#[derive(Clone)]
pub struct Frame {
    /// Under the frame: "1 · Title", "Page 2", "Budget", "Lines 41–96 · spend()".
    pub caption: String,
    pub art: Art,
    /// Where it opens in the panel: the page, slide or sheet (from 0).
    pub at: usize,
}

#[derive(Clone)]
pub struct Strip {
    pub shape: Shape,
    pub frames: Vec<Frame>,
    /// How many parts the file has in all (slides, pages, sheets, sections,
    /// stretches), so "+N" counts what isn't shown.
    pub total: usize,
    /// Under the title: "PowerPoint · 12 slides", "Markdown · 5 sections · 1,240 words".
    pub detail: String,
}

/// How many frames a strip shows at most, for each shape: narrow pages fit
/// more across a card than slides do.
pub fn most(shape: Shape) -> usize {
    match shape {
        Shape::Page => 6,
        Shape::Slide | Shape::Section => 4,
        Shape::Sheet | Shape::Lines => 3,
        Shape::Picture => 1,
    }
}

/// The key a file's strip is kept under: its bytes' (`office_render::key_of`)
/// or its name and text's.
pub fn key(file: &FileRef) -> u64 {
    match &file.bytes {
        Some(b) => crate::office_render::key_of(b),
        None => {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            file.name.hash(&mut h);
            file.content.hash(&mut h);
            h.finish()
        }
    }
}

/// Whether a file gets a strip card: everything but a diagram (drawn where it
/// is) and an HTML page (previewed as a page).
pub fn applies(file: &FileRef) -> bool {
    !file.is_diagram() && !file.is_html()
}

/// The file's strip, or None when nothing can be shown of it (a document
/// neither Office nor LibreOffice can draw, an empty file). Blocks: run it off
/// the UI thread.
pub fn build(file: &FileRef) -> Option<Strip> {
    use crate::office::Kind;
    if let Some(bytes) = &file.bytes {
        if file.is_image() {
            return picture(&file.name, bytes);
        }
        return match file.office()? {
            Kind::Xlsx => workbook(&file.name, bytes),
            kind => pages(&file.name, bytes, kind),
        };
    }
    if file.content.trim().is_empty() {
        return None;
    }
    if file.is_markdown() {
        if let Some(s) = markdown(&file.content) {
            return Some(s);
        }
    }
    let ext = file
        .name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "csv" || ext == "tsv" {
        return Some(table(&file.content, if ext == "tsv" { '\t' } else { ',' }));
    }
    Some(lines(&file.content, &ext))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", thousands(n), if n == 1 { one } else { many })
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn short(s: &str, most: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= most {
        return s.to_string();
    }
    let cut: String = s.chars().take(most.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

fn pages(name: &str, bytes: &[u8], kind: crate::office::Kind) -> Option<Strip> {
    use crate::office::Kind;
    let shape = if kind == Kind::Pptx {
        Shape::Slide
    } else {
        Shape::Page
    };
    let (drawn, total) = crate::office::first_pages(name, bytes, most(shape)).ok()?;
    let titles = if kind == Kind::Pptx {
        crate::office::slide_titles(bytes)
    } else {
        vec![]
    };
    let frames = drawn
        .into_iter()
        .enumerate()
        .map(|(i, p)| Frame {
            caption: match (kind, titles.get(i).cloned().flatten()) {
                (Kind::Pptx, Some(t)) => format!("{} · {}", i + 1, short(&t, 22)),
                (Kind::Pptx, None) => format!("Slide {}", i + 1),
                _ => format!("Page {}", i + 1),
            },
            art: Art::Page {
                image: p.image,
                width: p.width,
                height: p.height,
            },
            at: i,
        })
        .collect();
    let (app, noun) = match kind {
        Kind::Pptx => ("PowerPoint", ("slide", "slides")),
        Kind::Docx => ("Word", ("page", "pages")),
        _ => ("PDF", ("page", "pages")),
    };
    Some(Strip {
        shape,
        frames,
        total,
        detail: format!("{app} · {}", plural(total, noun.0, noun.1)),
    })
}

fn workbook(name: &str, bytes: &[u8]) -> Option<Strip> {
    let crate::office::Preview::Sheets(sheets) = crate::office::preview(name, bytes).ok()? else {
        return None;
    };
    let total = sheets.len();
    let frames = sheets
        .iter()
        .take(most(Shape::Sheet))
        .enumerate()
        .map(|(i, sh)| Frame {
            caption: short(&sh.name, 24),
            art: Art::Cells(
                sh.rows
                    .iter()
                    .take(6)
                    .map(|r| {
                        r.iter()
                            .take(4)
                            .map(|c| (short(&c.text, 12), c.numeric))
                            .collect()
                    })
                    .collect(),
            ),
            at: i,
        })
        .collect();
    Some(Strip {
        shape: Shape::Sheet,
        frames,
        total,
        detail: format!("Excel · {}", plural(total, "sheet", "sheets")),
    })
}

/// A CSV's first rows, as one sheet.
fn table(text: &str, sep: char) -> Strip {
    let rows: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let cells = rows
        .iter()
        .take(6)
        .map(|l| {
            l.split(sep)
                .take(4)
                .map(|c| {
                    let c = c.trim().trim_matches('"');
                    (short(c, 12), c.parse::<f64>().is_ok())
                })
                .collect()
        })
        .collect();
    Strip {
        shape: Shape::Sheet,
        frames: vec![Frame {
            caption: "First rows".into(),
            art: Art::Cells(cells),
            at: 0,
        }],
        total: 1,
        detail: format!(
            "CSV · {}",
            plural(rows.len().saturating_sub(1), "row", "rows")
        ),
    }
}

/// Markdown's sections: at its `##` headings when it has two or more (its `#`
/// is then its title), else at its `#` headings. None without headings.
pub fn markdown(text: &str) -> Option<Strip> {
    let mut fenced = false;
    let mut heads: Vec<(usize, usize, String)> = vec![]; // (line, level, text)
    let lines: Vec<&str> = text.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        if l.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if fenced {
            continue;
        }
        let level = l.chars().take_while(|c| *c == '#').count();
        if (1..=2).contains(&level) && l[level..].starts_with(' ') {
            heads.push((i, level, l[level..].trim().to_string()));
        }
    }
    let level = if heads.iter().filter(|h| h.1 == 2).count() >= 2 {
        2
    } else {
        1
    };
    let cuts: Vec<&(usize, usize, String)> = heads.iter().filter(|h| h.1 == level).collect();
    if cuts.is_empty() {
        return None;
    }
    let words = text.split_whitespace().count();
    let frames = cuts
        .iter()
        .enumerate()
        .take(most(Shape::Section))
        .map(|(n, (at, _, heading))| {
            let end = cuts.get(n + 1).map_or(lines.len(), |c| c.0);
            let body = lines[at + 1..end]
                .iter()
                .map(|l| plain_md(l))
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            Frame {
                caption: format!("{} · {}", n + 1, short(heading, 20)),
                art: Art::Section {
                    heading: short(heading, 48),
                    body: short(&body, 150),
                },
                at: n,
            }
        })
        .collect();
    Some(Strip {
        shape: Shape::Section,
        frames,
        total: cuts.len(),
        detail: format!(
            "Markdown · {} · {}",
            plural(cuts.len(), "section", "sections"),
            plural(words, "word", "words")
        ),
    })
}

/// A Markdown line as plain words: no list marks, emphasis or link targets.
fn plain_md(l: &str) -> String {
    let t = l.trim();
    let t = t.trim_start_matches(['#', '>', '-', '*', '+']).trim_start();
    let t = t
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .trim_start_matches(['.', ')'])
        .trim();
    // `[words](target)` keeps its words.
    let mut kept = String::new();
    let mut rest = t;
    while let Some(at) = rest.find("](") {
        kept.push_str(&rest[..at]);
        rest = match rest[at + 2..].find(')') {
            Some(end) => &rest[at + 2 + end + 1..],
            None => "",
        };
    }
    kept.push_str(rest);
    kept.chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '[' | ']' | '|'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// The language a file's extension names, for its card.
fn language(ext: &str) -> &'static str {
    match ext {
        "py" => "Python",
        "rs" => "Rust",
        "js" | "mjs" | "cjs" => "JavaScript",
        "ts" | "tsx" => "TypeScript",
        "go" => "Go",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "kt" => "Kotlin",
        "sh" | "bash" | "zsh" | "ps1" => "Shell",
        "sql" => "SQL",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "xml" => "XML",
        "css" => "CSS",
        "log" => "Log",
        "md" | "markdown" => "Markdown",
        _ => "Text",
    }
}

/// Where a top-level definition starts, and its name: `def spend(`,
/// `fn main(`, `function draw(`, `class Deck`.
fn definition(l: &str) -> Option<String> {
    if l.starts_with([' ', '\t']) {
        return None;
    }
    let t = l
        .trim_start_matches("pub ")
        .trim_start_matches("export ")
        .trim_start_matches("async ");
    for kw in ["def ", "fn ", "function ", "func ", "class "] {
        if let Some(rest) = t.strip_prefix(kw) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(if kw == "class " {
                    name
                } else {
                    format!("{name}()")
                });
            }
        }
    }
    None
}

/// Text cut where it divides: code at its top-level definitions, other text
/// in stretches of 40 lines.
pub fn lines(text: &str, ext: &str) -> Strip {
    const STRETCH: usize = 40;
    let all: Vec<&str> = text.lines().collect();
    let lang = language(ext);
    let code = lang != "Text" && lang != "Log";
    let mut cuts: Vec<(usize, Option<String>)> = vec![];
    if code {
        for (i, l) in all.iter().enumerate() {
            if let Some(name) = definition(l) {
                cuts.push((i, Some(name)));
            }
        }
    }
    let functions = cuts.len();
    if cuts.len() < 2 {
        cuts = (0..all.len().max(1))
            .step_by(STRETCH)
            .map(|i| (i, None))
            .collect();
    } else if cuts[0].0 > 0 {
        // What comes before the first definition (imports) joins it.
        cuts[0].0 = 0;
    }
    let total = cuts.len();
    let frames = cuts
        .iter()
        .enumerate()
        .take(most(Shape::Lines))
        .map(|(n, (from, name))| {
            let to = cuts.get(n + 1).map_or(all.len(), |c| c.0);
            let shown = all[*from..to]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .take(7)
                .map(|l| {
                    // Indentation kept: code reads by it.
                    let l = l.replace('\t', "    ");
                    let body = l.trim_start();
                    format!("{}{}", &l[..l.len() - body.len()], short(body, 48))
                })
                .collect();
            let span = format!("Lines {}–{}", from + 1, to.max(from + 1));
            Frame {
                caption: match name {
                    Some(n) => format!("{span} · {n}"),
                    None => span,
                },
                art: Art::Lines(shown),
                at: *from,
            }
        })
        .collect();
    let mut detail = format!("{lang} · {}", plural(all.len(), "line", "lines"));
    if functions >= 2 {
        detail.push_str(&format!(
            " · {}",
            plural(functions, "definition", "definitions")
        ));
    }
    Strip {
        shape: Shape::Lines,
        frames,
        total,
        detail,
    }
}

fn picture(name: &str, bytes: &Arc<[u8]>) -> Option<Strip> {
    let format = crate::ui::image_view::format_of(name)?;
    let (w, h) = image::load_from_memory(bytes)
        .map(|i| (i.width(), i.height()))
        .unwrap_or((0, 0));
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_uppercase())
        .unwrap_or_default();
    Some(Strip {
        shape: Shape::Picture,
        frames: vec![Frame {
            caption: String::new(),
            art: Art::Page {
                image: Arc::new(gpui::Image::from_bytes(format, bytes.to_vec())),
                width: w,
                height: h,
            },
            at: 0,
        }],
        total: 1,
        detail: if w > 0 {
            format!("{ext} · {w}×{h}")
        } else {
            ext
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_splits_at_its_sections_under_its_title() {
        let md = "# What is cancer?\n\nIntro.\n\n## Cells\n\nYour body is made of **cells**.\n\n## When cells go wrong\n\n- They copy too fast\n\n```\n## not a heading\n```\n\n## How doctors help\n\nThey [find](x) it.\n";
        let s = markdown(md).unwrap();
        assert_eq!(s.total, 3);
        let caps: Vec<&str> = s.frames.iter().map(|f| f.caption.as_str()).collect();
        assert_eq!(
            caps,
            [
                "1 · Cells",
                "2 · When cells go wrong",
                "3 · How doctors help"
            ]
        );
        let Art::Section { body, .. } = &s.frames[2].art else {
            panic!()
        };
        assert_eq!(body, "They find it.");
        assert!(s.detail.starts_with("Markdown · 3 sections · "));
        assert!(markdown("no headings here").is_none());
    }

    #[test]
    fn code_splits_at_its_definitions_and_text_in_stretches() {
        let py = "import os\n\ndef title(deck):\n    pass\n\ndef spend(deck):\n    pass\n\nclass Deck:\n    pass\n";
        let s = lines(py, "py");
        let caps: Vec<&str> = s.frames.iter().map(|f| f.caption.as_str()).collect();
        assert_eq!(
            caps,
            [
                "Lines 1–5 · title()",
                "Lines 6–8 · spend()",
                "Lines 9–10 · Deck"
            ]
        );
        assert_eq!(s.detail, "Python · 10 lines · 3 definitions");
        let text: String = (1..=90).map(|i| format!("line {i}\n")).collect();
        let t = lines(&text, "txt");
        assert_eq!(t.total, 3);
        assert_eq!(t.frames[1].caption, "Lines 41–80");
        assert_eq!(t.detail, "Text · 90 lines");
    }

    #[test]
    fn counts_read_as_people_write_them() {
        assert_eq!(plural(1, "slide", "slides"), "1 slide");
        assert_eq!(plural(1240, "word", "words"), "1,240 words");
        assert_eq!(table("a,b\n1,2\n3,4\n", ',').detail, "CSV · 2 rows");
    }
}
