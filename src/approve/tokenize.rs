//! Minimal shell tokenizer, tuned for risk analysis rather than execution.
//!
//! This deliberately does not implement POSIX shell. It implements just enough
//! to answer "which words are the targets of a destructive command", and it is
//! written to **fail loud rather than quiet**: when it cannot understand
//! something, the caller escalates instead of assuming safety.
//!
//! A regex over the raw string cannot do this job. `rm -rf a && rm -rf b` is two
//! commands; `sh -c "rm -rf ~"` hides one inside a quoted argument; `echo x >
//! important.txt` destroys a file with no destructive verb anywhere in it.
//! Splitting into segments is what makes each of those visible, and chaining
//! would otherwise be a one-character bypass.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    /// This segment's stdin comes from a pipe, so some of its operands are
    /// supplied at runtime by the previous command and cannot be seen here.
    pub receives_pipe: bool,
    /// A `>` or `>|` destination, which is truncated when opened.
    pub is_truncating_redirect_target: bool,
    /// A control operator like `&&`, which is never a path target.
    pub is_operator: bool,
}

impl Token {
    /// A plain word with no flags set.
    pub fn word(text: impl Into<String>) -> Self {
        Token {
            text: text.into(),
            receives_pipe: false,
            is_truncating_redirect_target: false,
            is_operator: false,
        }
    }
}

/// The command name without its directory, so `/bin/rm` matches `rm`.
pub fn basename(token: &Token) -> &str {
    basename_str(&token.text)
}

/// [`basename`] over bare text. A trailing `/` leaves the text whole.
pub fn basename_str(text: &str) -> &str {
    match text.rsplit('/').next() {
        Some(last) if !last.is_empty() => last,
        _ => text,
    }
}

pub fn is_flag(token: &Token) -> bool {
    token.text.starts_with('-') && token.text.len() > 1
}

/// Whether this flag requests recursion, including bundles like `-rf`.
pub fn is_recursive_flag(token: &Token) -> bool {
    if !is_flag(token) {
        return false;
    }
    if token.text.starts_with("--") {
        return token.text == "--recursive";
    }
    token.text.contains('r') || token.text.contains('R')
}

/// Whether this flag forces, including bundles like `-rf`.
#[allow(dead_code)] // kept beside is_recursive_flag for the rules that will need it
pub fn is_force_flag(token: &Token) -> bool {
    if !is_flag(token) {
        return false;
    }
    if token.text.starts_with("--") {
        return token.text == "--force";
    }
    token.text.contains('f')
}

const SEGMENT_SEPARATORS: [&str; 5] = ["&&", "||", ";", "|", "\n"];

struct Lexer {
    tokens: Vec<Token>,
    current: String,
    has_content: bool,
    pending_redirect: bool,
}

impl Lexer {
    fn flush(&mut self) {
        if !self.has_content {
            return;
        }
        let mut token = Token::word(std::mem::take(&mut self.current));
        if self.pending_redirect {
            token.is_truncating_redirect_target = true;
            self.pending_redirect = false;
        }
        self.has_content = false;
        self.tokens.push(token);
    }

    fn push_operator(&mut self, text: String) {
        let mut op = Token::word(text);
        op.is_operator = true;
        self.tokens.push(op);
    }
}

/// Tokenize a command line, resolving quotes so `rm "$HOME"` and `rm $HOME`
/// produce the same target text.
///
/// Note the deliberate asymmetry with a real shell: `$VAR` is kept intact rather
/// than expanded, and the path layer treats an unexpanded variable as
/// unknown-and-therefore-risky. Guessing at an expansion would be worse than
/// admitting the value is not known.
pub fn tokenize(command: &str) -> Vec<Token> {
    let chars: Vec<char> = command.chars().collect();
    let n = chars.len();
    let mut lx = Lexer {
        tokens: Vec::new(),
        current: String::new(),
        has_content: false,
        pending_redirect: false,
    };

    let mut i = 0usize;
    while i < n {
        let c = chars[i];

        if c == '\'' {
            lx.has_content = true;
            i += 1;
            while i < n && chars[i] != '\'' {
                lx.current.push(chars[i]);
                i += 1;
            }
        } else if c == '"' {
            lx.has_content = true;
            i += 1;
            while i < n && chars[i] != '"' {
                if chars[i] == '\\' {
                    match chars.get(i + 1) {
                        Some(&next) if "\"\\$`".contains(next) => {
                            lx.current.push(next);
                            i += 2;
                            continue;
                        }
                        // A trailing backslash in an unterminated quote: the
                        // source appends JS `undefined` here; nothing is kept.
                        None => {
                            i += 2;
                            continue;
                        }
                        _ => {}
                    }
                }
                lx.current.push(chars[i]);
                i += 1;
            }
        } else if c == '\\' {
            if i + 1 < n {
                lx.has_content = true;
                lx.current.push(chars[i + 1]);
                i += 1;
            }
        } else if c == ' ' || c == '\t' {
            lx.flush();
        } else if c == '\n' || c == ';' {
            lx.flush();
            lx.push_operator(c.to_string());
        } else if c == '&' || c == '|' {
            lx.flush();
            let mut text = c.to_string();
            if chars.get(i + 1) == Some(&c) {
                text.push(c);
                i += 1;
            }
            lx.push_operator(text);
        } else if c == '>' {
            lx.flush();
            // `>>` appends and does not truncate, so it is far less destructive;
            // only a single `>` clobbers.
            if chars.get(i + 1) == Some(&'>') {
                i += 1;
            } else {
                if chars.get(i + 1) == Some(&'|') {
                    i += 1;
                }
                lx.pending_redirect = true;
            }
        } else if c == '<' {
            lx.flush();
        } else {
            lx.has_content = true;
            lx.current.push(c);
        }
        i += 1;
    }

    lx.flush();
    lx.tokens
}

/// Split a command line into command segments, each tokenized.
///
/// `rm -rf a && rm -rf b` yields two segments so both are assessed. Without
/// this, chaining is a trivial bypass.
pub fn split_segments(command: &str) -> Vec<Vec<Token>> {
    let mut segments = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    let mut next_receives_pipe = false;

    for token in tokenize(command) {
        if token.is_operator && SEGMENT_SEPARATORS.contains(&token.text.as_str()) {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            // The command after `|` consumes the previous one's output as
            // operands, which this parser cannot see.
            next_receives_pipe = token.text == "|";
            continue;
        }
        current.push(Token {
            receives_pipe: next_receives_pipe,
            ..token
        });
    }

    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(tokens: &[Token]) -> Vec<&str> {
        tokens.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn resolves_quotes_so_the_target_text_is_the_same_either_way() {
        assert_eq!(texts(&tokenize(r#"rm "$HOME""#)), ["rm", "$HOME"]);
        assert_eq!(texts(&tokenize("rm '/a b/c'")), ["rm", "/a b/c"]);
    }

    #[test]
    fn keeps_var_intact_rather_than_guessing_at_its_value() {
        // Guessing would be worse than admitting the value is unknown.
        assert!(texts(&tokenize("rm -rf $TARGET")).contains(&"$TARGET"));
    }

    #[test]
    fn marks_a_truncating_redirect_but_not_an_appending_one() {
        let t = tokenize("echo x > f");
        let hit = t.iter().find(|t| t.is_truncating_redirect_target);
        assert_eq!(hit.map(|t| t.text.as_str()), Some("f"));
        assert!(!tokenize("echo x >> f")
            .iter()
            .any(|t| t.is_truncating_redirect_target));
    }

    #[test]
    fn handles_redirect_pipe_as_truncating() {
        let t = tokenize("echo x >| f");
        let hit = t.iter().find(|t| t.is_truncating_redirect_target);
        assert_eq!(hit.map(|t| t.text.as_str()), Some("f"));
    }

    #[test]
    fn strips_the_directory_from_a_program_name() {
        let t = tokenize("/bin/rm -rf x");
        assert_eq!(t.first().map(basename), Some("rm"));
    }

    #[test]
    fn reads_flags_as_recursive_or_not() {
        for (flag, expected) in [
            ("-rf", true),
            ("-R", true),
            ("--recursive", true),
            ("-f", false),
            ("--force", false),
        ] {
            assert_eq!(is_recursive_flag(&Token::word(flag)), expected, "{flag}");
        }
    }

    #[test]
    fn splits_on_each_separator() {
        for sep in ["&&", "||", ";", "|"] {
            assert_eq!(
                split_segments(&format!("ls {sep} rm -rf /")).len(),
                2,
                "{sep}"
            );
        }
    }

    #[test]
    fn marks_the_segment_after_a_pipe_as_receiving_it() {
        let segs = split_segments("cat f | xargs rm");
        assert!(segs[1][0].receives_pipe);
    }

    #[test]
    fn does_not_mark_the_segment_after_and_as_piped() {
        let segs = split_segments("ls && rm x");
        assert!(!segs[1][0].receives_pipe);
    }
}
