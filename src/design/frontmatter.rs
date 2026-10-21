//! Front-matter: the YAML block between `---` lines at the top of a
//! `SKILL.md`, `DESIGN.md` or craft file in the design library. It is real
//! YAML (nested maps, block text, lists), read with yaml-rust2 into JSON
//! values so the rest of the index handles one kind of tree.
//!
//! A few upstream files are not valid YAML. They are read the way Open
//! Design's own daemon reads them (`od_subset`), so they keep every field;
//! if even that fails, the top-level `key: value` lines are read one by one.
//! Either way the error is kept for `--design-check` to list.

use serde_json::{Map, Value};
use yaml_rust2::{Yaml, YamlLoader};

/// Front-matter larger than this isn't parsed as YAML (the library's largest
/// is 4.4 KB): a user's skill could otherwise nest deep enough to matter.
const MAX_YAML: usize = 64 * 1024;

/// A file split into its front-matter and the text after it.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    /// The front-matter as a JSON object; empty when there is none.
    pub meta: Map<String, Value>,
    /// Everything after the closing `---`, or the whole file without one.
    pub body: String,
    /// Why the front-matter is not valid YAML, when it isn't.
    pub error: Option<String>,
}

impl Doc {
    /// A top-level string field: `name`, `description`.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.meta.get(key).and_then(Value::as_str)
    }

    /// A field at a dotted path: `od.craft.requires`.
    pub fn path(&self, path: &str) -> Option<&Value> {
        let mut parts = path.split('.');
        let mut at = self.meta.get(parts.next()?)?;
        for p in parts {
            at = at.get(p)?;
        }
        Some(at)
    }

    /// A list of strings at a dotted path; a single string counts as one.
    pub fn strings(&self, path: &str) -> Vec<String> {
        match self.path(path) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => vec![],
        }
    }
}

/// The front-matter block and the body, if the file opens with `---`.
fn split(text: &str) -> Option<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))?;
    // The closing line: `---` alone, possibly the last line of the file.
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some((&rest[..at], &rest[at + line.len()..]));
        }
        at += line.len();
    }
    None
}

/// Parse a file's front-matter. Never fails: a file without one has an
/// empty `meta`, and invalid YAML falls back to its top-level lines.
pub fn parse(text: &str) -> Doc {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some((yaml, body)) = split(text) else {
        return Doc {
            meta: Map::new(),
            body: text.to_string(),
            error: None,
        };
    };
    if yaml.len() > MAX_YAML {
        return Doc {
            meta: lines(yaml),
            body: body.to_string(),
            error: Some("front-matter larger than 64 KiB".to_string()),
        };
    }
    let (meta, error) = match YamlLoader::load_from_str(yaml) {
        Ok(docs) => match docs.into_iter().next().map(json) {
            Some(Value::Object(m)) => (m, None),
            Some(Value::Null) | None => (Map::new(), None),
            Some(_) => (Map::new(), Some("front-matter is not a map".to_string())),
        },
        Err(e) => (
            super::od_subset::parse(yaml).unwrap_or_else(|_| lines(yaml)),
            Some(e.to_string()),
        ),
    };
    Doc {
        meta,
        body: body.to_string(),
        error,
    }
}

/// YAML to JSON. Keys that aren't strings (numbers, booleans) become their
/// text; aliases and unparseable values become null.
fn json(y: Yaml) -> Value {
    match y {
        Yaml::String(s) => Value::String(s),
        Yaml::Integer(i) => Value::from(i),
        Yaml::Real(r) => r
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or(Value::String(r), Value::Number),
        Yaml::Boolean(b) => Value::Bool(b),
        Yaml::Array(a) => Value::Array(a.into_iter().map(json).collect()),
        Yaml::Hash(h) => Value::Object(
            h.into_iter()
                .map(|(k, v)| (key(k), json(v)))
                .collect(),
        ),
        Yaml::Null | Yaml::Alias(_) | Yaml::BadValue => Value::Null,
    }
}

fn key(k: Yaml) -> String {
    match k {
        Yaml::String(s) | Yaml::Real(s) => s,
        Yaml::Integer(i) => i.to_string(),
        Yaml::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

/// The last resort, when even Open Design's reader fails: each unindented
/// `key: value` line, with
/// one layer of quotes taken off. Block text (`description: |`) keeps the
/// indented lines that follow, joined by spaces.
fn lines(yaml: &str) -> Map<String, Value> {
    let mut out = Map::new();
    let mut block: Option<(String, Vec<String>)> = None;
    for line in yaml.lines() {
        let indented = line.starts_with([' ', '\t']);
        if indented {
            if let Some((_, text)) = block.as_mut() {
                text.push(line.trim().to_string());
            }
            continue;
        }
        if let Some((k, text)) = block.take() {
            out.insert(k, Value::String(text.join(" ").trim().to_string()));
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty() || k.starts_with('#') || k.contains(' ') {
            continue;
        }
        if matches!(v, "|" | "|-" | "|+" | ">" | ">-" | ">+") {
            block = Some((k.to_string(), vec![]));
        } else if !v.is_empty() {
            let v = v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(v);
            out.insert(k.to_string(), Value::String(v.to_string()));
        }
    }
    if let Some((k, text)) = block {
        out.insert(k, Value::String(text.join(" ").trim().to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_maps_lists_and_block_text() {
        let d = parse(
            "---\nname: deck\ndescription: |\n  Two lines\n  of text.\nod:\n  mode: deck\n  craft:\n    requires: [typography, color]\ntriggers:\n  - \"slides\"\n  - deck\n---\n# Body\n",
        );
        assert_eq!(d.error, None);
        assert_eq!(d.str("name"), Some("deck"));
        assert_eq!(d.str("description"), Some("Two lines\nof text.\n"));
        assert_eq!(d.path("od.mode").and_then(Value::as_str), Some("deck"));
        assert_eq!(d.strings("od.craft.requires"), ["typography", "color"]);
        assert_eq!(d.strings("triggers"), ["slides", "deck"]);
        assert_eq!(d.body, "# Body\n");
    }

    #[test]
    fn no_front_matter_is_all_body() {
        let d = parse("# Just text\n");
        assert!(d.meta.is_empty());
        assert_eq!(d.body, "# Just text\n");
        assert_eq!(d.error, None);
    }

    #[test]
    fn bom_crlf_and_a_closing_line_at_the_end() {
        let d = parse("\u{feff}---\r\nname: a\r\n---");
        assert_eq!(d.str("name"), Some("a"));
        assert_eq!(d.body, "");
    }

    #[test]
    fn invalid_yaml_is_read_the_way_open_design_reads_it() {
        let d = parse(
            "---\nname: promo\ndescription: A 30s promo: cut: fast\ntitle: \"Promo\"\nnotes: >\n  folded\n  text\nod:\n  mode: video\n  craft:\n    requires: [typography]\n---\nbody",
        );
        assert!(d.error.is_some());
        assert_eq!(d.str("name"), Some("promo"));
        assert_eq!(d.str("description"), Some("A 30s promo: cut: fast"));
        assert_eq!(d.str("title"), Some("Promo"));
        // Open Design keeps the line break in `>` text.
        assert_eq!(d.str("notes"), Some("folded\ntext"));
        // Nested fields survive, unlike a reader of top-level lines.
        assert_eq!(d.path("od.mode").and_then(Value::as_str), Some("video"));
        assert_eq!(d.strings("od.craft.requires"), ["typography"]);
        assert_eq!(d.body, "body");
    }

    #[test]
    fn the_last_resort_reads_top_level_lines() {
        let m = lines("name: a\ndescription: |\n  two\n  lines\nbad line\nkey with space: x\n");
        assert_eq!(m["name"], "a");
        assert_eq!(m["description"], "two lines");
        assert!(!m.contains_key("key with space"));
    }

    #[test]
    fn oversized_front_matter_is_not_parsed_as_yaml() {
        let big = format!("---\nname: big\nnotes: \"{}\"\n---\n", "x".repeat(MAX_YAML));
        let d = parse(&big);
        assert_eq!(d.str("name"), Some("big"));
        assert!(d.error.is_some_and(|e| e.contains("64 KiB")));
    }

    #[test]
    fn numbers_and_booleans_keep_their_types() {
        let d = parse("---\nfeatured: 3\nratio: 1.5\nrecommended: true\n1: one\n---\n");
        assert_eq!(d.meta["featured"], 3);
        assert_eq!(d.meta["ratio"], 1.5);
        assert_eq!(d.meta["recommended"], true);
        assert_eq!(d.meta["1"], "one");
    }
}
