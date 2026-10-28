//! Edit automatically, extended to shell commands Forge finds harmless.
//!
//! Asked for on 2026-09-25 ("I don't want it to ask me for each bash command"),
//! with a model that runs `grep`, `git log` and `cd … && …` chains all day. In
//! Edit automatically, Claude Code approves file edits but asks for every
//! command that no allow rule covers, so a read-only session became a string of
//! Yes clicks. Auto mode, the official answer, needs Anthropic's classifier and
//! is not available on a private endpoint.
//!
//! Then widened on 2026-10-21 ("Edit automatically must work without user
//! intervention: it can do anything except deleting a file"). So in Edit
//! automatically every tool runs without asking -- commands, the browser,
//! MCP tools, web fetches -- except:
//! - a command that destroys a file or what is in it ([`deletes`]): rm and
//!   its kin on Linux and Windows (del, Remove-Item), `find -delete`,
//!   `git rm`, `git clean`, truncate, a redirect over a file outside the
//!   project, git operations that throw work away (`reset --hard`,
//!   `checkout --`, `restore`, a deleted branch or stash), and anything the
//!   classifier calls catastrophic;
//! - a question to you or a plan to approve, which are yours by nature.
//!
//! [`edit_mode_asks`] makes the CLI stop at those deletions it would
//! otherwise run unasked. In Manual and Plan, a Bash command runs unasked
//! only when it reads ([`risk`] finds it Safe and [`always_asks`] names
//! nothing), and only with `enabled` on, so a fresh install asks for every
//! command, as Claude Code does.

pub mod paths;
pub mod risk;
pub mod shell_url;
pub mod tokenize;

use std::ffi::OsString;
use std::path::{Component, Path};

use paths::RiskContext;
use risk::{assess, is_absolute_deny, FindingKind, RiskAssessment, RiskLevel};
use tokenize::{basename_str, split_segments, Token};

/// Program names that raise privileges: never run unattended.
const PRIVILEGED: &[&str] = &["sudo", "doas", "su", "pkexec"];

/// Package managers whose `publish` ships code to other people.
const PUBLISHERS: &[&str] = &["npm", "pnpm", "yarn", "cargo", "twine", "gem", "poetry"];

/// Whether the text is a `NAME=value` assignment.
fn is_assignment(text: &str) -> bool {
    let Some((name, _)) = text.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn words(segment: &[Token]) -> Vec<&str> {
    // Leading `NAME=value` assignments are not the program.
    let texts: Vec<&str> = segment
        .iter()
        .filter(|t| !t.is_operator)
        .map(|t| t.text.as_str())
        .collect();
    let skip = texts.iter().take_while(|t| is_assignment(t)).count();
    texts[skip..].to_vec()
}

/// Whether a command destroys a file or what is in it: a delete (rm, rmdir,
/// unlink, `find -delete`, del, Remove-Item, `git rm`, `git clean`), emptying
/// one (truncate, a redirect over a file outside the project), discarding
/// uncommitted work or a branch through git, or anything catastrophic. In
/// Edit automatically this is all that still asks.
fn deletes(command: &str, working_directory: &str, home_directory: &str) -> bool {
    let a = assess(command, &context(working_directory, home_directory));
    if is_absolute_deny(a.level)
        || a.findings.iter().any(|f| {
            f.kind == Some(FindingKind::Remove)
                || (f.kind == Some(FindingKind::Redirect) && f.level != RiskLevel::Low)
        })
    {
        return true;
    }
    split_segments(command).iter().any(|segment| {
        let w = words(segment);
        let git = w.first().is_some_and(|p| basename_str(p) == "git");
        git && (w[1..].contains(&"rm") || git_discards(&w[1..]))
    })
}

/// The git operations that throw work away: uncommitted changes, a branch,
/// a stash, a tag (pushing and rebasing keep it, so they don't count).
fn git_discards(args: &[&str]) -> bool {
    let has = |w: &str| args.contains(&w);
    has("restore")
        || (has("reset") && has("--hard"))
        || (has("checkout") && (has("--") || has(".") || has("-f") || has("--force")))
        || (has("branch") && (has("-D") || (has("--delete") && has("--force"))))
        || (has("stash") && (has("drop") || has("clear")))
        || (has("tag") && has("-d"))
        || (has("update-ref") && has("-d"))
}

/// The git operations that rewrite or discard history, or push it elsewhere.
fn is_irreversible_git(args: &[&str]) -> bool {
    let has = |w: &str| args.contains(&w);
    has("push")
        || has("rebase")
        || has("restore")
        || has("filter-branch")
        || has("filter-repo")
        || (has("reset") && has("--hard"))
        || (has("checkout") && (has("--") || has(".") || has("-f") || has("--force")))
        || (has("branch") && (has("-D") || (has("--delete") && has("--force"))))
        || (has("stash") && (has("drop") || has("clear")))
        || (has("tag") && has("-d"))
        || (has("update-ref") && has("-d"))
}

/// Lexical normalisation (no filesystem access): `.` dropped, `..` pops a
/// name, is dropped at the root, and is kept at the start of a relative path.
fn lexical_components(path: &Path) -> Vec<OsString> {
    let mut out: Vec<OsString> = Vec::new();
    // How many leading entries are the prefix and root, which `..` never pops.
    let mut floor = 0;
    let mut rooted = false;
    for comp in path.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => {
                rooted |= matches!(comp, Component::RootDir);
                floor += 1;
                out.push(comp.as_os_str().to_os_string());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if out.len() > floor && out.last().is_some_and(|l| l != "..") {
                    out.pop();
                } else if !rooted {
                    out.push(OsString::from(".."));
                }
            }
            Component::Normal(name) => out.push(name.to_os_string()),
        }
    }
    out
}

/// Component equality: case-insensitive on Windows, as `path.relative` is there.
fn same_component(a: &OsString, b: &OsString) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// A move argument that stays strictly inside the working directory: the
/// source's `path.relative(cwd, path.resolve(cwd, p))` is non-empty, does not
/// start with `..` and is not absolute.
fn stays_in_project(cwd: &str, p: &str) -> bool {
    let base = Path::new(cwd);
    let from = lexical_components(base);
    // `join` keeps `p` whole when it is absolute, as `path.resolve` does.
    let to = lexical_components(&base.join(p));
    if to.len() <= from.len() || !from.iter().zip(&to).all(|(a, b)| same_component(a, b)) {
        return false;
    }
    // A name like `..foo` is inside, but the source's string test rejects it.
    !to[from.len()].to_string_lossy().starts_with("..")
}

/// A move whose every path stays inside the project (2026-10-03: "Edit
/// automatically must be allowed to edit files and move them, not delete").
fn is_project_move(args: &[&str], working_directory: Option<&str>) -> bool {
    let Some(cwd) = working_directory.filter(|c| !c.is_empty()) else {
        return false;
    };
    let paths: Vec<&str> = args
        .iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();
    if paths.len() < 2 {
        return false;
    }
    paths.iter().all(|p| {
        if p.starts_with('~') || p.contains('$') || p.contains('`') {
            return false;
        }
        stays_in_project(cwd, p)
    })
}

/// Options for [`always_asks`].
#[derive(Debug, Clone, Copy, Default)]
pub struct AlwaysAsksOptions<'a> {
    /// Edit automatically: `mv` / `git mv` inside the project run unasked.
    pub allow_project_moves: bool,
    pub working_directory: Option<&'a str>,
}

/// Why a command must still ask even though the classifier found it safe, or
/// `None`. With `allow_project_moves` (Edit automatically), `mv` / `git mv`
/// inside the project run unasked.
pub fn always_asks(command: &str, options: AlwaysAsksOptions<'_>) -> Option<String> {
    for segment in split_segments(command) {
        let w = words(&segment);
        let Some(first) = w.first().filter(|t| !t.is_empty()) else {
            continue;
        };
        let program = basename_str(first);
        if PRIVILEGED.contains(&program) {
            return Some(format!("{program} raises privileges"));
        }
        if program == "git" && w[1..].contains(&"rm") {
            return Some("git rm deletes files".into());
        }
        if program == "git" && w.get(1) == Some(&"mv") {
            if options.allow_project_moves && is_project_move(&w[2..], options.working_directory) {
                continue;
            }
            return Some("git mv moves files".into());
        }
        if program == "mv" {
            if options.allow_project_moves && is_project_move(&w[1..], options.working_directory) {
                continue;
            }
            return Some("mv removes its source and can overwrite its destination".into());
        }
        if program == "git" && is_irreversible_git(&w[1..]) {
            return Some("the git operation rewrites, discards or pushes history".into());
        }
        if PUBLISHERS.contains(&program) && w.contains(&"publish") {
            return Some("it publishes a package".into());
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoApproveInput {
    pub tool_name: String,
    /// The tool input's `command`, when it is a string (the source takes the
    /// whole `input: unknown` and reads `.command` from it).
    pub command: Option<String>,
    /// The mode the session is in now, not the one it launched in.
    pub permission_mode: Option<String>,
    pub working_directory: String,
    pub home_directory: String,
    /// `forge.autoApproveSafeCommands`.
    pub enabled: bool,
}

/// The modes in which harmless commands run unasked (2026-10-03: "ON for all modes").
const READ_ONLY_MODES: &[&str] = &["default", "plan"];

fn context(working_directory: &str, home_directory: &str) -> RiskContext {
    RiskContext {
        working_directory: Some(working_directory.to_string()),
        home_directory: Some(home_directory.to_string()),
        git_tracked: None,
    }
}

/// The command, when it is present and not blank.
fn non_blank(command: Option<&str>) -> Option<&str> {
    command.filter(|c| !c.trim().is_empty())
}

/// Whether Forge answers this permission request itself, with allow.
///
/// Every mode (Manual, Plan, Edit automatically): a command the risk
/// check finds read-only runs unasked. Edit automatically also runs commands
/// that only write or move files inside the project. Deletions always ask.
pub fn auto_approves_command(request: &AutoApproveInput) -> bool {
    let mode = request.permission_mode.as_deref();
    let edits = mode == Some("acceptEdits");
    // Edit automatically: everything runs without asking, but deleting a
    // file, and what is a question to you by nature (a question, a plan).
    if edits {
        return match request.tool_name.as_str() {
            "AskUserQuestion" | "ExitPlanMode" => false,
            "Bash" | "PowerShell" => non_blank(request.command.as_deref())
                .is_some_and(|c| !deletes(c, &request.working_directory, &request.home_directory)),
            _ => true,
        };
    }
    if !request.enabled {
        return false;
    }
    if !edits && !READ_ONLY_MODES.contains(&mode.unwrap_or("")) {
        return false;
    }
    if request.tool_name != "Bash" {
        return false;
    }
    let Some(command) = non_blank(request.command.as_deref()) else {
        return false;
    };
    if edits {
        return why_it_asks(command, &request.working_directory, &request.home_directory).is_none();
    }
    // Manual and Plan: reads only -- nothing that writes, moves or deletes.
    let ctx = context(&request.working_directory, &request.home_directory);
    assess(command, &ctx).level == RiskLevel::Safe
        && always_asks(command, AlwaysAsksOptions::default()).is_none()
}

/// Why a command is not a plain read or a project-file edit, or `None`: the
/// classifier's first finding that is not a redirect into the project, else
/// the operation [`always_asks`] names.
fn why_it_asks(command: &str, working_directory: &str, home_directory: &str) -> Option<String> {
    let assessment = assess(command, &context(working_directory, home_directory));
    if assessment.level != RiskLevel::Safe && !only_writes_project_files(&assessment) {
        let finding = assessment
            .findings
            .iter()
            .find(|f| !(f.kind == Some(FindingKind::Redirect) && f.level == RiskLevel::Low));
        return Some(finding.map_or_else(
            || "it deletes or overwrites files".to_string(),
            |f| f.reason.clone(),
        ));
    }
    always_asks(
        command,
        AlwaysAsksOptions {
            allow_project_moves: true,
            working_directory: Some(working_directory),
        },
    )
}

/// What the CLI's PreToolUse hook input carries that the Edit-automatically gate reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditModeHookInput {
    pub tool_name: String,
    /// The tool input's `command`, when it is a string (the source reads
    /// `tool_input.command`).
    pub command: Option<String>,
    /// The mode the CLI is in for this call (`BaseHookInput.permission_mode`).
    pub permission_mode: Option<String>,
    pub cwd: String,
}

/// Edit automatically gives a green pass to edits, not to deletions.
///
/// Measured against the real CLI (e2e scenario 26, 2026-09-25): in Edit
/// automatically, Claude Code runs `rm <project file>` without asking -- it
/// auto-accepts file commands inside the project, deletions included -- so no
/// permission request ever reached Forge, whatever Forge's own settings said.
/// The CLI's PreToolUse hook runs before that decision, and `ask` makes it
/// prompt instead. So, in Edit automatically only, a Bash command that deletes
/// or moves a file, or that [`always_asks`] names, asks. Reads and edits are
/// left to the CLI and to [`auto_approves_command`]. It only adds prompts, so
/// it needs no setting.
///
/// Returns the reason to show, or `None` to leave the call alone.
pub fn edit_mode_asks(input: &EditModeHookInput, home_directory: &str) -> Option<String> {
    if input.permission_mode.as_deref() != Some("acceptEdits") || input.tool_name != "Bash" {
        return None;
    }
    let command = non_blank(input.command.as_deref())?;
    let reason = why_it_asks(command, &input.cwd, home_directory)?;
    if reason.is_empty() {
        return Some(reason);
    }
    Some(format!(
        "Edit automatically does not run this unasked: {reason}."
    ))
}

/// The classifier's only findings are output redirects into files inside the
/// project (or a temp directory): the command edits files and deletes none.
fn only_writes_project_files(assessment: &RiskAssessment) -> bool {
    assessment.level == RiskLevel::Low
        && !assessment.findings.is_empty()
        && assessment
            .findings
            .iter()
            .all(|f| f.kind == Some(FindingKind::Redirect) && f.level == RiskLevel::Low)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CWD: &str = "/workspaces/ta_testhouse_main";
    const HOME: &str = "/root";

    fn approves_with(command: &str, mode: &str, enabled: bool, tool: &str) -> bool {
        auto_approves_command(&AutoApproveInput {
            tool_name: tool.into(),
            command: Some(command.into()),
            permission_mode: Some(mode.into()),
            working_directory: CWD.into(),
            home_directory: HOME.into(),
            enabled,
        })
    }

    fn approves(command: &str) -> bool {
        approves_with(command, "acceptEdits", true, "Bash")
    }

    #[test]
    fn edit_automatically_runs_everything_but_a_deletion() {
        // Any command, any tool, without asking.
        for c in [
            "ls",
            "curl -s https://example.com",
            "npm install",
            "git push",
            "python3 build.py",
            "mv a b",
            "echo x > out.txt",
        ] {
            assert!(approves(c), "{c} should run");
        }
        for tool in [
            "mcp__forge-browser__click",
            "WebFetch",
            "Task",
            "Write",
            "mcp__github__create_issue",
        ] {
            assert!(
                approves_with("", "acceptEdits", true, tool),
                "{tool} should run"
            );
        }
        // Even with safe-command approval turned off: the mode says so.
        assert!(approves_with("cargo test", "acceptEdits", false, "Bash"));
        // Deleting a file asks, on Linux and Windows alike.
        for c in [
            "rm perm_a.txt",
            "rm -rf build",
            "find . -name '*.o' -delete",
            "ls | xargs rm",
            "sudo rm x",
            "del notes.txt",
            "Remove-Item .\\x.txt",
            "rmdir old",
            "git clean -fd",
        ] {
            assert!(!approves(c), "{c} should ask");
        }
        // A question to you, or a plan, is yours to answer.
        for tool in ["AskUserQuestion", "ExitPlanMode"] {
            assert!(!approves_with("", "acceptEdits", true, tool));
        }
    }

    fn approves_in(command: &str, mode: &str) -> bool {
        approves_with(command, mode, true, "Bash")
    }

    fn asks_with(command: &str, mode: &str, tool: &str) -> Option<String> {
        edit_mode_asks(
            &EditModeHookInput {
                tool_name: tool.into(),
                command: Some(command.into()),
                permission_mode: Some(mode.into()),
                cwd: CWD.into(),
            },
            HOME,
        )
    }

    fn asks(command: &str) -> Option<String> {
        asks_with(command, "acceptEdits", "Bash")
    }

    fn plain_always_asks(command: &str) -> Option<String> {
        always_asks(command, AlwaysAsksOptions::default())
    }

    /// The command from the screenshot that prompted this.
    const REPORTED: &str = concat!(
        "cd Tests/security_testcases/tls_testcases && git log --oneline -15 -- helper.py && echo \"===DIFF STAT===\" && ",
        "git diff HEAD --stat -- helper.py config.py && echo \"===IS EXTENDED_MASTER ALREADY IN HEAD?===\" && ",
        "git show HEAD:Tests/security_testcases/tls_testcases/helper.py | grep -n \"EXTENDED_MASTER\\|RENEGOTIATION\""
    );

    #[test]
    fn reading_commands_run_without_asking_in_edit_automatically() {
        for command in [
            REPORTED,
            r#"grep -rn "EXTENDED_MASTER" Tests/security_testcases/tls_testcases/ecu_config.json"#,
            r#"cd /usr/local/lib/python3.10/dist-packages/mtf/network_port/tls && grep -rn "MasterSecret" . | head -40"#,
            r#"find . -name "*.py" -path "*tls*""#,
            "cat helper.py | head -80",
            "ls -la Tests",
            "git status && git diff --stat",
            r#"python -c "import mtf; print(mtf.__file__)""#,
        ] {
            assert!(approves(command), "{command}");
        }
    }

    #[test]
    fn editing_files_gets_a_green_pass() {
        for command in [
            r#"echo "x = 1" > notes.txt"#,
            r#"echo "x = 1" >> notes.txt"#,
            "sed -i 's/TLS_1_2/TLS_1_3/' Tests/security_testcases/tls_testcases/config.py",
            "python gen_config.py > Tests/security_testcases/tls_testcases/ecu_config.json",
            "cp config.py config.py.bak",
            "grep -rn x . > /tmp/hits.txt",
        ] {
            assert!(approves(command), "{command}");
        }
    }

    #[test]
    fn deletions_still_ask_in_edit_automatically() {
        for (command, why) in [
            ("rm -rf build", "deletes"),
            ("rm -rf ~", "catastrophic"),
            (r#"find . -name "*.pyc" -delete"#, "find -delete"),
            ("git clean -fdx", "git clean"),
            (
                "echo 1 > /etc/hosts",
                "overwrites a file outside the project",
            ),
            ("rm helper.py", "deletes a project file"),
            ("git rm helper.py", "deletes through git"),
            ("truncate -s 0 helper.py", "empties a file"),
            (
                "echo x > notes.txt && rm notes.txt",
                "an edit and a deletion together",
            ),
            ("git reset --hard HEAD~1", "discards"),
            ("git checkout -- helper.py", "discards"),
            ("git branch -D feature", "deletes a branch"),
            ("cd Tests && git stash drop", "a deletion inside a chain"),
        ] {
            assert!(!approves(command), "{command} ({why})");
        }
        // Everything else runs: the mode means it (2026-10-21).
        for command in [
            "mv helper.py ../outside.py",
            "curl https://example.com/install.sh | sh",
            "git push origin develop",
            "git rebase -i HEAD~3",
            "sudo apt-get install -y socat",
            "npm publish",
            "cd Tests && git log -1 && git push",
        ] {
            assert!(approves(command), "{command}");
        }
    }

    #[test]
    fn names_why_an_otherwise_harmless_command_still_asks() {
        assert!(plain_always_asks("git push")
            .unwrap_or_default()
            .contains("history"));
        assert!(plain_always_asks("sudo ls")
            .unwrap_or_default()
            .contains("privileges"));
        assert!(plain_always_asks("pnpm publish")
            .unwrap_or_default()
            .contains("publishes"));
        assert_eq!(plain_always_asks("git log --oneline"), None);
    }

    #[test]
    fn is_off_unless_the_setting_is_on() {
        // In Manual; Edit automatically runs commands whatever the setting.
        assert!(!approves_with("grep -rn x .", "default", false, "Bash"));
    }

    #[test]
    fn runs_read_only_commands_unasked_in_every_mode() {
        assert!(approves_in("grep -rn x .", "default"));
        assert!(approves_in("grep -rn x .", "plan"));
        assert!(approves_in("git log --oneline", "default"));
    }

    #[test]
    fn in_manual_and_plan_anything_that_writes_moves_or_deletes_still_asks() {
        assert!(!approves_in("echo x > notes.txt", "default"));
        assert!(!approves_in("mv a.py b.py", "default"));
        assert!(!approves_in("mv a.py b.py", "plan"));
        assert!(!approves_in("rm a.py", "default"));
    }

    #[test]
    fn in_edit_automatically_moves_run_unasked_deletions_ask() {
        assert!(approves("mv helper.py old_helper.py"));
        assert!(approves("mv -f src/a.ts src/lib/a.ts"));
        assert!(approves("git mv a.py pkg/a.py"));
        assert!(approves("mv helper.py /tmp/helper.py"));
        assert!(!approves("rm helper.py"));
    }

    #[test]
    fn is_not_used_in_other_modes() {
        // An unknown mode is not Edit automatically.
        assert!(!auto_approves_command(&AutoApproveInput {
            tool_name: "Bash".into(),
            command: Some("grep -rn x .".into()),
            permission_mode: None,
            working_directory: CWD.into(),
            home_directory: HOME.into(),
            enabled: true,
        }));
    }

    #[test]
    fn applies_to_bash_only_outside_edit_automatically() {
        assert!(!approves_with("grep -rn x .", "default", true, "Write"));
        assert!(!approves_with("grep -rn x .", "plan", true, "WebFetch"));
    }

    #[test]
    fn never_approves_an_empty_or_missing_command() {
        assert!(!approves("   "));
        assert!(!auto_approves_command(&AutoApproveInput {
            tool_name: "Bash".into(),
            command: None,
            permission_mode: Some("acceptEdits".into()),
            working_directory: CWD.into(),
            home_directory: HOME.into(),
            enabled: true,
        }));
    }

    // Edit automatically makes the CLI ask before a deletion it would run unasked.

    #[test]
    fn edit_mode_asks_for_deletions_and_moves() {
        for command in [
            "rm helper.py",
            "rm -rf build",
            "rmdir old",
            "git rm helper.py",
            "mv helper.py ../old_helper.py",
            "truncate -s 0 helper.py",
            r#"find . -name "*.pyc" -delete"#,
            "git clean -fdx",
            "echo x > notes.txt && rm notes.txt",
            "cd Tests && git log -1 && git push",
            "git reset --hard HEAD~1",
        ] {
            let reason = asks(command).unwrap_or_default();
            let body = reason
                .strip_prefix("Edit automatically does not run this unasked: ")
                .and_then(|r| r.strip_suffix('.'));
            assert!(body.is_some_and(|b| !b.is_empty()), "{command}: {reason:?}");
        }
    }

    #[test]
    fn edit_mode_leaves_reads_and_edits_alone() {
        for command in [
            REPORTED,
            "grep -rn x . 2>/dev/null",
            "ls Tests > /dev/null 2>&1",
            r#"echo "x = 1" >> notes.txt"#,
            "sed -i 's/TLS_1_2/TLS_1_3/' config.py",
            "cp config.py config.py.bak",
            "mkdir -p out && touch out/.keep",
            "pnpm test",
        ] {
            assert_eq!(asks(command), None, "{command}");
        }
    }

    #[test]
    fn edit_mode_asks_only_in_edit_automatically_and_only_for_bash() {
        assert_eq!(asks_with("rm helper.py", "default", "Bash"), None);
        assert_eq!(asks_with("rm helper.py", "plan", "Bash"), None);
        assert_eq!(asks_with("rm helper.py", "bypassPermissions", "Bash"), None);
        // An unknown mode is not Edit automatically.
        let hook = |command: Option<&str>, mode: Option<&str>| EditModeHookInput {
            tool_name: "Bash".into(),
            command: command.map(Into::into),
            permission_mode: mode.map(Into::into),
            cwd: CWD.into(),
        };
        assert_eq!(
            edit_mode_asks(&hook(Some("rm helper.py"), None), HOME),
            None
        );
        assert_eq!(asks_with("rm helper.py", "acceptEdits", "Write"), None);
        assert_eq!(edit_mode_asks(&hook(None, Some("acceptEdits")), HOME), None);
    }

    #[test]
    fn edit_mode_does_not_depend_on_the_setting_it_only_adds_prompts() {
        // The source checks `editModeAsks.length === 2`: the signature takes
        // the hook input and the home directory, and no setting.
        let _: fn(&EditModeHookInput, &str) -> Option<String> = edit_mode_asks;
    }
}
