//! Syntax highlighting for code blocks, as Forge for VS Code colours them.
//!
//! The grammars are Sublime's (syntect) plus bat's extras (two-face), so a
//! block in Rust, TypeScript, Python, Go, TOML, YAML, shell, SQL, a diff and
//! many more gets coloured. Each grammar scope is mapped onto the handful of
//! token kinds Forge's palette has (keyword, string, number, comment,
//! function, type, variable, operator, meta, added, removed); the theme
//! decides their colours.
//!
//! Parsing is not free, so results are cached by language and text: a block
//! is parsed once, not on every frame. The grammars themselves load on a
//! background thread at start (`warm_up`).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Plain,
    Keyword,
    Str,
    Number,
    Comment,
    Function,
    Type,
    Variable,
    Operator,
    Meta,
    Added,
    Removed,
}

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

/// Parsed blocks, by hash of language and text. Small and bounded: cleared
/// when it grows past what a few long chats need.
static CACHE: LazyLock<Mutex<HashMap<u64, Arc<Vec<(Range<usize>, Kind)>>>>> = LazyLock::new(Default::default);
const CACHE_MAX: usize = 512;

/// Load the grammars off the UI thread, so the first code block on screen
/// doesn't wait for them.
pub fn warm_up() {
    std::thread::spawn(|| LazyLock::force(&SYNTAXES));
}

fn syntax(lang: &str) -> Option<&'static SyntaxReference> {
    let lang = lang.trim().to_ascii_lowercase();
    if lang.is_empty() || lang == "text" || lang == "plaintext" {
        return None;
    }
    let alias = match lang.as_str() {
        "sh" | "shell" | "zsh" | "console" => "bash",
        "py" => "python",
        "rs" => "rust",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "yml" => "yaml",
        "md" => "markdown",
        "c++" => "cpp",
        "cs" | "c#" => "cs",
        "ps1" | "pwsh" => "powershell",
        other => other,
    };
    SYNTAXES.find_syntax_by_token(alias).or_else(|| SYNTAXES.find_syntax_by_name(alias))
}

/// What a scope stack means for colour: the innermost scope that says
/// anything wins (punctuation defers to what it belongs to).
fn kind_of(stack: &ScopeStack) -> Kind {
    // An object key is an attribute, even though its text is a string.
    if stack.as_slice().iter().any(|s| {
        let name = s.build_string();
        name.starts_with("meta.mapping.key") || name.starts_with("support.type.property-name")
    }) {
        return Kind::Variable;
    }
    for scope in stack.as_slice().iter().rev() {
        let name = scope.build_string();
        let is = |prefix: &str| name == prefix || name.starts_with(&format!("{prefix}."));
        let kind = if is("comment") {
            Kind::Comment
        } else if is("markup.inserted") {
            Kind::Added
        } else if is("markup.deleted") {
            Kind::Removed
        } else if is("string") || is("constant.character") {
            Kind::Str
        } else if is("constant.numeric") {
            Kind::Number
        } else if is("constant.language") || is("variable.language") {
            Kind::Keyword
        } else if is("keyword.operator") {
            Kind::Operator
        } else if is("keyword") || is("storage") {
            Kind::Keyword
        } else if is("support.macro") || is("entity.name.macro") || is("entity.name.function.macro") {
            Kind::Type
        } else if is("entity.name.function") || is("support.function") || is("variable.function") {
            Kind::Function
        } else if is("entity.name.type") || is("entity.name.class") || is("entity.name.struct") || is("entity.name.enum") || is("entity.name.trait") || is("entity.name.namespace") || is("support.type") || is("support.class") || is("entity.other.inherited-class") {
            Kind::Type
        } else if is("entity.other.attribute-name") || is("variable.parameter") || is("variable.other.member") || is("variable.other.property") || is("support.type.property-name") || is("meta.mapping.key") || is("variable.other.constant") || is("variable.other.readwrite.shell") {
            Kind::Variable
        } else if is("meta.annotation") || is("meta.attribute") || is("meta.preprocessor") || is("entity.name.tag") || is("meta.tag") {
            Kind::Meta
        } else if is("entity.name.section") || is("markup.heading") {
            Kind::Function
        } else {
            continue;
        };
        return kind;
    }
    Kind::Plain
}

/// Colour runs for `code` in `lang`, as byte ranges. Unknown languages (and
/// an empty language) come back as one plain run.
pub fn highlight(code: &str, lang: &str) -> Arc<Vec<(Range<usize>, Kind)>> {
    let key = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (lang, code).hash(&mut h);
        h.finish()
    };
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let runs = Arc::new(parse(code, lang));
    if let Ok(mut cache) = CACHE.lock() {
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
        cache.insert(key, runs.clone());
    }
    runs
}

fn parse(code: &str, lang: &str) -> Vec<(Range<usize>, Kind)> {
    let Some(syntax) = syntax(lang) else { return vec![(0..code.len(), Kind::Plain)] };
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out: Vec<(Range<usize>, Kind)> = vec![];
    let mut push = |range: Range<usize>, kind: Kind| {
        if range.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((r, k)) if *k == kind && r.end == range.start => r.end = range.end,
            _ => out.push((range, kind)),
        }
    };
    let mut offset = 0;
    for line in code.split_inclusive('\n') {
        let Ok(ops) = state.parse_line(line, &SYNTAXES) else {
            push(offset..code.len(), Kind::Plain);
            return out;
        };
        let mut at = 0;
        for (pos, op) in ops {
            push(offset + at..offset + pos, kind_of(&stack));
            let _ = stack.apply(&op);
            at = pos;
        }
        push(offset + at..offset + line.len(), kind_of(&stack));
        offset += line.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(code: &str, lang: &str) -> Vec<(String, Kind)> {
        highlight(code, lang).iter().map(|(r, k)| (code[r.clone()].to_string(), *k)).filter(|(s, k)| *k != Kind::Plain && !s.trim().is_empty()).collect()
    }

    fn has(k: &[(String, Kind)], text: &str, kind: Kind) -> bool {
        k.iter().any(|(s, kk)| s.contains(text) && *kk == kind)
    }

    #[test]
    fn colours_rust() {
        let k = kinds("#[test]\nfn greets() -> &'static str {\n    // hi\n    let n = 42;\n    println!(\"Hello, {}\", n);\n}\n", "rust");
        assert!(has(&k, "fn", Kind::Keyword), "{k:?}");
        assert!(has(&k, "greets", Kind::Function), "{k:?}");
        assert!(has(&k, "// hi", Kind::Comment), "{k:?}");
        assert!(has(&k, "42", Kind::Number), "{k:?}");
        assert!(has(&k, "Hello", Kind::Str), "{k:?}");
        assert!(has(&k, "println!", Kind::Type) || has(&k, "println", Kind::Function), "{k:?}");
    }

    #[test]
    fn colours_typescript_python_and_json() {
        let ts = kinds("export const greet = (name: string): string => `hi ${name}`;\n", "ts");
        assert!(has(&ts, "const", Kind::Keyword), "{ts:?}");
        assert!(has(&ts, "string", Kind::Type), "{ts:?}");
        let py = kinds("def greet(name):\n    return f\"hi {name}\"  # done\n", "py");
        assert!(has(&py, "def", Kind::Keyword) && has(&py, "greet", Kind::Function) && has(&py, "# done", Kind::Comment), "{py:?}");
        let json = kinds("{\"name\": \"forge\", \"n\": 3, \"ok\": true}", "json");
        assert!(has(&json, "name", Kind::Variable) && has(&json, "forge", Kind::Str) && has(&json, "3", Kind::Number), "{json:?}");
    }

    #[test]
    fn unknown_languages_stay_plain() {
        let runs = highlight("anything at all", "no-such-language");
        assert_eq!(runs.as_slice(), &[(0..15, Kind::Plain)]);
    }
}
