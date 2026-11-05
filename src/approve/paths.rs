//! Which paths must never be destroyed, and which are merely worth a prompt.
//!
//! The lists here came out of a real incident: a user lost their home directory.
//! The distinctions they encode are therefore not theoretical.
//!
//! Two of them matter more than the rest:
//!
//!   - **Exact versus recursive.** `~/.config` is protected as a directory, but
//!     `~/.config/app/stale.toml` is not: deleting individual config files is
//!     something people legitimately do all day, and protecting every file under
//!     a protected root would make the classifier so noisy it would be ignored.
//!     `/etc` is the opposite -- its contents are as unrecoverable as the
//!     directory itself.
//!   - **`/home` and `/Users` are not recursive.** A user's own project lives
//!     under them, so protecting them recursively would flag ordinary work. The
//!     home directory itself is protected separately.
//!
//! Comparison is on POSIX-style strings (backslashes become `/`), lexically and
//! without touching the filesystem; on Windows it is case-insensitive.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RiskContext {
    /// Where the command will run. Targets under it are bounded.
    pub working_directory: Option<String>,
    pub home_directory: Option<String>,
    /// True when the working directory is inside a git repo, so edits are recoverable.
    pub git_tracked: Option<bool>,
}

impl RiskContext {
    /// The working directory, when set and non-empty.
    fn cwd(&self) -> Option<&str> {
        self.working_directory.as_deref().filter(|s| !s.is_empty())
    }

    /// The home directory, when set and non-empty.
    fn home(&self) -> Option<&str> {
        self.home_directory.as_deref().filter(|s| !s.is_empty())
    }
}

/// Credential stores. Protected recursively: everything inside is a secret.
const PROTECTED_CREDENTIAL_SUBPATHS: &[&str] = &[".ssh", ".gnupg", ".aws", ".kube", ".docker"];

/// Directories whose wholesale destruction is unacceptable, but whose individual
/// files are legitimately edited and removed all the time. Matched exactly.
const PROTECTED_HOME_SUBPATHS: &[&str] = &[
    ".config",
    ".kodact",
    ".forge",
    ".claude",
    ".local",
    ".local/share",
    "Documents",
    "Desktop",
];

/// Absolute system paths that must never be recursively destroyed.
const PROTECTED_SYSTEM_PATHS: &[&str] = &[
    "/",
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/lib",
    "/lib64",
    "/opt",
    "/proc",
    "/root",
    "/sbin",
    "/srv",
    "/sys",
    "/usr",
    "/var",
    "/Applications",
    "/System",
    "/Library",
    "/Users",
    "/home",
];

/// System paths where the contents are as critical as the directory itself, so
/// deleting a single file inside them is also unacceptable.
const SYSTEM_PATHS_PROTECTED_RECURSIVELY: &[&str] = &[
    "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/sbin", "/sys", "/usr",
    "/var/lib", "/System", "/Library",
];

/// The three standard device sinks.
///
/// Exempted *before* the recursive `/dev` rule, which would otherwise swallow
/// them and flag every ordinary `cmd > /dev/null`.
const SAFE_DEVICE_SINKS: &[&str] = &["/dev/null", "/dev/stdout", "/dev/stderr"];

/// Temp directories: anything under them is disposable.
const TEMP_DIRS: &[&str] = &["/tmp", "/var/tmp", "/private/tmp"];

/// Normalise for comparison: forward slashes, no trailing slash, no case games.
fn normalize(p: &str) -> String {
    let mut collapsed = String::with_capacity(p.len());
    let mut prev_slash = false;
    for ch in p.chars() {
        let ch = if ch == '\\' { '/' } else { ch };
        if ch == '/' {
            if !prev_slash {
                collapsed.push('/');
            }
            prev_slash = true;
        } else {
            collapsed.push(ch);
            prev_slash = false;
        }
    }
    let trimmed = if collapsed.len() > 1 {
        collapsed.trim_end_matches('/')
    } else {
        collapsed.as_str()
    };
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

fn is_under(child: &str, parent: &str) -> bool {
    let c = normalize(child);
    let p = normalize(parent);
    let prefix = if p == "/" {
        "/".to_string()
    } else {
        format!("{p}/")
    };
    c == p || c.starts_with(&prefix)
}

/// `path.posix.normalize`: resolves `.` and `..` lexically, keeps a trailing
/// slash, and keeps leading `..` only on relative paths.
fn posix_normalize(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let absolute = path.starts_with('/');
    let trailing = path.ends_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if stack.last().is_some_and(|top| *top != "..") {
                    stack.pop();
                } else if !absolute {
                    stack.push("..");
                }
            }
            s => stack.push(s),
        }
    }
    let mut out = stack.join("/");
    if out.is_empty() {
        if absolute {
            return "/".to_string();
        }
        return if trailing {
            "./".to_string()
        } else {
            ".".to_string()
        };
    }
    if trailing {
        out.push('/');
    }
    if absolute {
        format!("/{out}")
    } else {
        out
    }
}

/// `path.posix.join`: empty parts dropped, the rest joined and normalised.
fn posix_join(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() {
        return ".".to_string();
    }
    posix_normalize(&joined)
}

/// `path.posix.dirname`.
fn posix_dirname(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let bytes = path.as_bytes();
    let has_root = bytes[0] == b'/';
    let mut end: Option<usize> = None;
    let mut matched_slash = true;
    for i in (1..bytes.len()).rev() {
        if bytes[i] == b'/' {
            if !matched_slash {
                end = Some(i);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    match end {
        None => (if has_root { "/" } else { "." }).to_string(),
        Some(1) if has_root => "//".to_string(),
        // `end` is the index of an ASCII `/`, so it is a char boundary.
        Some(e) => path[..e].to_string(),
    }
}

/// Whether the text references a shell variable: `$` then a word character,
/// optionally through `{` (the source's `/\$\{?\w+/`).
fn has_variable(value: &str) -> bool {
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    let chars: Vec<char> = value.chars().collect();
    chars.iter().enumerate().any(|(i, &c)| {
        if c != '$' {
            return false;
        }
        let next = chars.get(i + 1).copied();
        is_word(next) || (next == Some('{') && is_word(chars.get(i + 2).copied()))
    })
}

/// The rest of the text after a leading `$HOME` / `${HOME}`, when what follows
/// is `/` or nothing (the source's `/^\$\{?HOME\}?(\/|$)/`).
fn after_home_variable(value: &str) -> Option<&str> {
    let rest = value.strip_prefix('$')?;
    let rest = rest.strip_prefix('{').unwrap_or(rest);
    let rest = rest.strip_prefix("HOME")?;
    let rest = rest.strip_prefix('}').unwrap_or(rest);
    (rest.is_empty() || rest.starts_with('/')).then_some(rest)
}

fn has_drive_letter(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':')
}

/// A raw argument resolved for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub path: String,
    /// The text holds a variable whose value cannot be known here.
    pub unknown: bool,
}

/// Resolve a raw argument into a path for comparison.
///
/// `~` is expanded because it is unambiguous. `$VAR` is deliberately **not**
/// expanded: the value is unknown here, and guessing would be worse than
/// admitting it. An unexpanded variable is reported as unknown so the caller
/// escalates rather than assuming the best case.
pub fn expand(raw: &str, ctx: &RiskContext) -> Expanded {
    let mut value = raw.to_string();
    let mut unknown = false;

    if let Some(home) = ctx.home() {
        if value == "~" {
            value = home.to_string();
        } else if let Some(rest) = value.strip_prefix("~/") {
            value = posix_join(&[&normalize(home), rest]);
        }
    }

    if has_variable(&value) {
        // `$HOME` and `${HOME}` are the ones that actually matter, and treating
        // them as the home directory is more accurate than treating them as opaque.
        match (after_home_variable(&value), ctx.home()) {
            (Some(rest), Some(home)) => value = format!("{}{rest}", normalize(home)),
            _ => unknown = true,
        }
    }

    let relative = !normalize(&value).starts_with('/') && !has_drive_letter(&value);
    if let Some(cwd) = ctx.cwd().filter(|_| relative) {
        value = posix_join(&[&normalize(cwd), &normalize(&value)]);
    }

    Expanded {
        path: normalize(&value),
        unknown,
    }
}

/// Would destroying this path be unacceptable no matter the justification?
pub fn is_catastrophic_target(target: &str, ctx: &RiskContext) -> bool {
    let p = normalize(target);

    // Before the recursive /dev rule below, which would otherwise swallow them.
    if SAFE_DEVICE_SINKS.iter().any(|sink| normalize(sink) == p) {
        return false;
    }

    if PROTECTED_SYSTEM_PATHS.iter().any(|sys| normalize(sys) == p) {
        return true;
    }
    if SYSTEM_PATHS_PROTECTED_RECURSIVELY
        .iter()
        .any(|sys| is_under(&p, sys))
    {
        return true;
    }

    let Some(home) = ctx.home().map(normalize).filter(|h| !h.is_empty()) else {
        return false;
    };

    // The home directory itself.
    if p == home {
        return true;
    }

    // Credential stores, including anything inside them.
    if PROTECTED_CREDENTIAL_SUBPATHS
        .iter()
        .any(|sub| is_under(&p, &format!("{home}/{sub}")))
    {
        return true;
    }

    // Config and document roots, but not their individual files.
    PROTECTED_HOME_SUBPATHS
        .iter()
        .any(|sub| normalize(&format!("{home}/{sub}")) == p)
}

/// How much scrutiny destroying this path earns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetClass {
    /// A device sink: nothing is destroyed.
    Harmless,
    /// Inside the working directory, or a temp dir -- recoverable or
    /// disposable, so it runs with a record kept.
    Bounded,
    /// Irreversible and reaches outside the working directory.
    Outside,
    /// Home, root or credentials -- never runs.
    Catastrophic,
    /// The parser could not resolve it, which escalates rather than being
    /// assumed safe.
    Unknown,
}

/// How much scrutiny destroying this path earns. See [`TargetClass`].
pub fn classify_target(raw: &str, ctx: &RiskContext) -> TargetClass {
    let Expanded {
        path: resolved,
        unknown,
    } = expand(raw, ctx);

    // Writing to a device sink destroys nothing, so it earns no finding at all.
    // Without this it would fall through to "outside the working directory" and
    // turn every ordinary `cmd > /dev/null` into a permission prompt.
    if SAFE_DEVICE_SINKS
        .iter()
        .any(|sink| normalize(sink) == resolved)
    {
        return TargetClass::Harmless;
    }

    // Checked first: a glob or variable that resolves *into* a protected root is
    // still catastrophic, and `unknown` must not downgrade that.
    if is_catastrophic_target(&resolved, ctx) {
        return TargetClass::Catastrophic;
    }

    // A wildcard directly under a protected root is the `rm -rf /*` shape.
    if raw.contains(['*', '?']) {
        let parent = normalize(&posix_dirname(&resolved));
        if is_catastrophic_target(&parent, ctx) {
            return TargetClass::Catastrophic;
        }
    }

    if unknown {
        return TargetClass::Unknown;
    }

    if TEMP_DIRS.iter().any(|t| is_under(&resolved, t)) {
        return TargetClass::Bounded;
    }

    if let Some(cwd) = ctx.cwd().filter(|cwd| is_under(&resolved, cwd)) {
        // The working directory *itself* is not bounded: wiping the whole
        // project is a different act from deleting a file in it.
        return if normalize(&resolved) == normalize(cwd) {
            TargetClass::Outside
        } else {
            TargetClass::Bounded
        };
    }

    TargetClass::Outside
}

/// The protected lists, exposed for tests and for explaining a refusal
/// (nothing explains one yet).
#[allow(dead_code)]
pub struct ProtectedPaths;

#[allow(dead_code)]
impl ProtectedPaths {
    pub fn home_subpaths() -> &'static [&'static str] {
        PROTECTED_HOME_SUBPATHS
    }
    pub fn credential_subpaths() -> &'static [&'static str] {
        PROTECTED_CREDENTIAL_SUBPATHS
    }
    pub fn system_paths() -> &'static [&'static str] {
        PROTECTED_SYSTEM_PATHS
    }
    pub fn recursive_system_paths() -> &'static [&'static str] {
        SYSTEM_PATHS_PROTECTED_RECURSIVELY
    }
    pub fn safe_device_sinks() -> &'static [&'static str] {
        SAFE_DEVICE_SINKS
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in super::super) const HOME: &str = "/home/dev";
    pub(in super::super) const CWD: &str = "/home/dev/projects/forge";

    pub(in super::super) fn ctx() -> RiskContext {
        RiskContext {
            home_directory: Some(HOME.into()),
            working_directory: Some(CWD.into()),
            git_tracked: None,
        }
    }

    // isCatastrophicTarget

    #[test]
    fn protects_the_system_paths() {
        for p in ["/", "/etc", "/usr", "/System", "/home", "/Users", "/var"] {
            assert!(is_catastrophic_target(p, &ctx()), "{p}");
        }
    }

    #[test]
    fn protects_files_inside_roots_whose_contents_are_as_critical_as_the_root() {
        assert!(is_catastrophic_target("/etc/passwd", &ctx()));
        assert!(is_catastrophic_target("/usr/bin/env", &ctx()));
    }

    #[test]
    fn protects_the_home_directory_itself() {
        assert!(is_catastrophic_target(HOME, &ctx()));
    }

    #[test]
    fn protects_the_credential_stores_recursively() {
        for dir in [".ssh", ".gnupg", ".aws", ".kube", ".docker"] {
            assert!(
                is_catastrophic_target(&format!("{HOME}/{dir}"), &ctx()),
                "{dir}"
            );
            assert!(
                is_catastrophic_target(&format!("{HOME}/{dir}/id_rsa"), &ctx()),
                "{dir}"
            );
        }
    }

    #[test]
    fn protects_config_as_a_directory_but_not_the_files_in_it() {
        // Deleting individual config files is something people do all day, and
        // protecting every one would make the classifier noisy enough to ignore.
        assert!(is_catastrophic_target(&format!("{HOME}/.config"), &ctx()));
        assert!(!is_catastrophic_target(
            &format!("{HOME}/.config/app/stale.toml"),
            &ctx()
        ));
    }

    #[test]
    fn does_not_protect_home_recursively_because_projects_live_under_it() {
        assert!(is_catastrophic_target("/home", &ctx()));
        assert!(!is_catastrophic_target(
            "/home/dev/projects/forge/build",
            &ctx()
        ));
    }

    #[test]
    fn exempts_the_device_sinks_before_the_recursive_dev_rule() {
        for sink in ["/dev/null", "/dev/stdout", "/dev/stderr"] {
            assert!(!is_catastrophic_target(sink, &ctx()), "{sink}");
        }
    }

    #[test]
    fn still_protects_the_rest_of_dev() {
        assert!(is_catastrophic_target("/dev/sda", &ctx()));
    }

    // classifyTarget

    #[test]
    fn treats_a_file_inside_the_working_directory_as_bounded() {
        assert_eq!(classify_target("build/tmp.o", &ctx()), TargetClass::Bounded);
        assert_eq!(
            classify_target(&format!("{CWD}/dist"), &ctx()),
            TargetClass::Bounded
        );
    }

    #[test]
    fn treats_the_working_directory_itself_as_not_bounded() {
        // Wiping the whole project is a different act from deleting a file in it.
        assert_eq!(classify_target(CWD, &ctx()), TargetClass::Outside);
    }

    #[test]
    fn treats_temp_directories_as_bounded() {
        assert_eq!(
            classify_target("/tmp/scratch", &ctx()),
            TargetClass::Bounded
        );
    }

    #[test]
    fn treats_an_unresolvable_variable_as_unknown_not_as_safe() {
        assert_eq!(classify_target("$TARGET", &ctx()), TargetClass::Unknown);
    }

    #[test]
    fn resolves_home_because_that_one_actually_matters() {
        assert_eq!(classify_target("$HOME", &ctx()), TargetClass::Catastrophic);
        assert_eq!(
            classify_target("${HOME}", &ctx()),
            TargetClass::Catastrophic
        );
    }

    #[test]
    fn expands_tilde() {
        assert_eq!(classify_target("~", &ctx()), TargetClass::Catastrophic);
        assert_eq!(classify_target("~/.ssh", &ctx()), TargetClass::Catastrophic);
    }

    #[test]
    fn catches_a_wildcard_directly_under_a_protected_root() {
        // The `rm -rf /*` shape.
        assert_eq!(classify_target("/*", &ctx()), TargetClass::Catastrophic);
        assert_eq!(
            classify_target(&format!("{HOME}/*"), &ctx()),
            TargetClass::Catastrophic
        );
    }
}
