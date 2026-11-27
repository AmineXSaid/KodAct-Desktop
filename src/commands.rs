//! KodAct CLI's slash commands, as the composer's `/` menu offers them.
//!
//! The list is KodAct CLI's own catalogue (`kodact-core/src/commands/mod.rs`),
//! less the commands that only work in its terminal UI and those the desktop
//! does itself or that would pull the chat out from under it (`/exit`,
//! `/clear`, `/resume`, `/branch`, `/cd`, `/rewind`, `/model`, `/rename`,
//! `/btw` — the side card is the desktop's `/btw`). Custom commands a project
//! keeps in `.forge/commands/*.md` are added from disk.

/// Whether a slash command talks to the model in the chat transcript (`Prompt`)
/// or controls the desktop UI / engine out-of-band without creating a user bubble (`Action`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Prompt,
    Action,
}

/// A command: its name, the arguments it takes, what it does, and how it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub name: String,
    pub args: String,
    pub description: String,
    pub kind: CommandKind,
}

/// (name, args, description, answered at once, kind)
const BUILTIN: &[(&str, &str, &str, bool, CommandKind)] = &[
    (
        "compact",
        "[instructions]",
        "Free context by summarizing the conversation so far",
        false,
        CommandKind::Action,
    ),
    (
        "context",
        "[all]",
        "Show what fills the context window",
        true,
        CommandKind::Action,
    ),
    (
        "usage",
        "",
        "Show this session's cost, token use and activity",
        true,
        CommandKind::Action,
    ),
    (
        "status",
        "",
        "Show version, model, session and setup status",
        true,
        CommandKind::Action,
    ),
    (
        "tasks",
        "[stop <id>]",
        "View and stop background work in this session",
        true,
        CommandKind::Action,
    ),
    (
        "mcp",
        "[reconnect|enable|disable <server|all>]",
        "Show MCP server status, or reconnect, enable or disable a server",
        true,
        CommandKind::Action,
    ),
    (
        "plan",
        "[description]",
        "Enter plan mode; with a description, start planning it",
        false,
        CommandKind::Prompt,
    ),
    (
        "goal",
        "[condition|clear]",
        "Set a goal KodAct keeps working toward until a check finds it met",
        false,
        CommandKind::Prompt,
    ),
    (
        "loop",
        "[interval] [prompt]",
        "Run a prompt on a schedule, or let KodAct pace it",
        false,
        CommandKind::Prompt,
    ),
    (
        "subtask",
        "<task>",
        "Hand a task to a background agent that starts from this conversation",
        false,
        CommandKind::Prompt,
    ),
    (
        "diff",
        "",
        "Show uncommitted changes, and the files each prompt changed",
        false,
        CommandKind::Action,
    ),
    (
        "recap",
        "",
        "Summarize the session in one line",
        false,
        CommandKind::Prompt,
    ),
    (
        "debug",
        "[description]",
        "Turn on debug logging, and have KodAct read the log to find a problem",
        false,
        CommandKind::Prompt,
    ),
    (
        "doctor",
        "",
        "Check the installation and this session's setup",
        false,
        CommandKind::Action,
    ),
    (
        "effort",
        "[low|medium|high|xhigh|max|auto]",
        "Show or set the model's reasoning effort",
        false,
        CommandKind::Action,
    ),
    (
        "fast",
        "[on|off]",
        "Turn fast mode on or off, where the model offers it",
        false,
        CommandKind::Action,
    ),
    (
        "advisor",
        "[model|off]",
        "Let KodAct consult a second model for advice at key moments",
        false,
        CommandKind::Action,
    ),
    (
        "autocompact",
        "[on|off|auto|<tokens>]",
        "Show or set when the conversation is compacted automatically",
        false,
        CommandKind::Action,
    ),
    (
        "memory",
        "",
        "List the memory files in use (KODACT.md, AGENTS.md)",
        false,
        CommandKind::Action,
    ),
    (
        "permissions",
        "[add|remove ...]",
        "Show the permission rules and working directories, or change the rules",
        false,
        CommandKind::Action,
    ),
    (
        "sandbox",
        "[on|off|read-only|workspace-write]",
        "Show or change the OS sandbox for shell commands",
        false,
        CommandKind::Action,
    ),
    (
        "hooks",
        "",
        "View the configured hooks, or add or remove one",
        false,
        CommandKind::Action,
    ),
    (
        "agents",
        "[create <name> ...]",
        "List subagents, or create one",
        false,
        CommandKind::Action,
    ),
    (
        "skills",
        "",
        "List available skills",
        false,
        CommandKind::Action,
    ),
    (
        "plugin",
        "[list]",
        "List loaded plugins",
        false,
        CommandKind::Action,
    ),
    (
        "output-style",
        "[style]",
        "Show the output styles, or switch to one",
        false,
        CommandKind::Action,
    ),
    (
        "add-dir",
        "<path> [--save]",
        "Add a working directory for this session",
        false,
        CommandKind::Action,
    ),
    (
        "config",
        "[key=value ...]",
        "Show the settings, or change them with key=value",
        false,
        CommandKind::Action,
    ),
    (
        "export",
        "[file]",
        "Export the conversation as plain text",
        false,
        CommandKind::Action,
    ),
    (
        "feedback",
        "[description]",
        "Save a bug-report bundle on this machine (nothing is uploaded)",
        false,
        CommandKind::Action,
    ),
    (
        "import",
        "[codex|gemini|cursor] [--yes]",
        "Bring MCP servers and instructions over from other coding agents",
        false,
        CommandKind::Action,
    ),
    (
        "reload-skills",
        "",
        "Reload skills, commands and agents without restarting",
        false,
        CommandKind::Action,
    ),
    (
        "reload-plugins",
        "",
        "Reload plugins without restarting",
        false,
        CommandKind::Action,
    ),
    (
        "release-notes",
        "",
        "Show what changed in each version",
        false,
        CommandKind::Action,
    ),
    (
        "help",
        "",
        "Show help and the available commands",
        false,
        CommandKind::Action,
    ),
];

/// KodAct CLI's built-in commands the desktop offers.
pub fn builtin() -> Vec<Command> {
    BUILTIN
        .iter()
        .map(|(n, a, d, _, kind)| Command {
            name: n.to_string(),
            args: a.to_string(),
            description: d.to_string(),
            kind: *kind,
        })
        .collect()
}

/// Split a leading `/command [args]` line into `(command, trimmed_args)`.
/// Rejects bare `/`, `//...`, and filesystem paths like `/etc/hosts`.
pub fn parse_slash(text: &str) -> Option<(&str, &str)> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix('/')?;
    if rest.is_empty() || rest.starts_with('/') {
        return None;
    }
    let (cmd, args) = match rest.find(char::is_whitespace) {
        Some(idx) => (&rest[..idx], rest[idx..].trim()),
        None => (rest, ""),
    };
    if cmd.is_empty() || cmd.contains('/') {
        return None;
    }
    Some((cmd, args))
}

/// Normalize a slash command token (converting `_` to `-`, case-insensitively)
/// to its canonical Desktop/Engine action identifier, if it is an action command.
pub fn normalize_action(cmd: &str) -> Option<&'static str> {
    let norm = cmd
        .trim()
        .trim_start_matches('/')
        .to_ascii_lowercase()
        .replace('_', "-");
    Some(match norm.as_str() {
        "plugin" | "plugins" | "show-plugin" | "show-plugins" => "plugins",
        "skills" | "show-skill" | "show-skills" => "skills",
        "reload-plugin" | "reload-plugins" => "reload-plugins",
        "reload-skill" | "reload-skills" => "reload-skills",
        "commands" | "slash-commands" | "show-commands" => "commands",
        "mcp" | "mcp-servers" => "mcp",
        "hooks" | "show-hooks" => "hooks",
        "agents" | "show-agents" => "agents",
        "permissions" => "permissions",
        "sandbox" => "sandbox",
        "memory" => "memory",
        "autocompact" | "auto-compact" => "autocompact",
        "config" => "config",
        "settings" => "settings",
        "environments" => "environments",
        "model" | "models" => "model",
        "effort" => "effort",
        "context" => "context",
        "context-window" => "context-window",
        "tasks" => "tasks",
        "status" => "status",
        "usage" => "usage",
        "doctor" => "doctor",
        "diff" => "diff",
        "compact" => "compact",
        "fast" => "fast",
        "advisor" => "advisor",
        "output-style" => "output-style",
        "add-dir" => "add-dir",
        "export" => "export",
        "export-json" => "export-json",
        "feedback" => "feedback",
        "import" => "import",
        "release-notes" => "release-notes",
        "help" | "guide" => "help",
        "about" => "about",
        "new" | "clear" | "reset" => "new",
        "session" => "session",
        "files" => "files",
        "terminal" => "terminal",
        "browser" => "browser",
        "theme" => "theme",
        "shortcuts" => "shortcuts",
        "find" => "find",
        "attach" => "attach",
        "manual" => "manual",
        "edit-automatically" | "accept-edits" => "edit-automatically",
        "auto" => "auto",
        "bypass-permissions" | "bypass" => "bypass-permissions",
        "btw" => "btw",
        "routine" => "routine",
        _ => return None,
    })
}

/// Classify a full composer message starting with `/` into `CommandKind::Action`
/// (handled out-of-band without entering `c.messages`) or `CommandKind::Prompt`
/// (sent to the model as a conversational turn in `c.messages`).
pub fn classify(text: &str) -> Option<CommandKind> {
    let (cmd, args) = parse_slash(text)?;
    let key = cmd.to_ascii_lowercase().replace('_', "-");
    match key.as_str() {
        "plan" => Some(if args.is_empty() {
            CommandKind::Action
        } else {
            CommandKind::Prompt
        }),
        "goal" => Some(if args.is_empty() || args.eq_ignore_ascii_case("clear") {
            CommandKind::Action
        } else {
            CommandKind::Prompt
        }),
        "subtask" | "loop" => Some(if args.is_empty() {
            CommandKind::Action
        } else {
            CommandKind::Prompt
        }),
        "debug" | "recap" => Some(CommandKind::Prompt),
        _ if normalize_action(cmd).is_some() => Some(CommandKind::Action),
        _ => Some(CommandKind::Prompt),
    }
}

/// Your commands and the project's, as KodAct CLI finds them
/// (`forge_files::commands`: yours, then `.agents/commands` and
/// `.forge/commands`, subfolders as `dir:name`), described by their
/// `description:` front matter or their first line (blocking: call off the
/// UI thread).
pub fn custom(project: &std::path::Path) -> Vec<Command> {
    let mut out: Vec<Command> = crate::forge_files::commands(project, &mut vec![])
        .into_iter()
        .filter(|c| !c.shadowed)
        .map(|c| Command {
            name: c.name,
            args: c.hint,
            description: if c.description.is_empty() {
                "Custom command".into()
            } else {
                c.description
            },
            kind: CommandKind::Prompt,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The skills KodAct CLI ships (`kodact-agents/bundled`), by name and what
/// they do. A user or project skill of the same name replaces one.
const BUNDLED_SKILLS: &[(&str, &str)] = &[
    (
        "batch",
        "Make the same change across many files, in parallel sub-agents",
    ),
    (
        "code-review",
        "Review code changes for bugs, with a depth level and an optional fix pass",
    ),
    (
        "fewer-permission-prompts",
        "Cut down permission prompts by allowing safe commands this project uses often",
    ),
    (
        "init",
        "Write or improve KODACT.md, the project's instructions for KodAct",
    ),
    (
        "review",
        "Review code changes for bugs (same as /code-review)",
    ),
    ("run", "Start the project and show it working"),
    (
        "run-skill-generator",
        "Write a project skill that captures how to build, run and check this project",
    ),
    (
        "security-review",
        "Review pending changes for exploitable security vulnerabilities",
    ),
    (
        "simplify",
        "Review recent changes for reuse, clarity and efficiency, then fix what you find",
    ),
    (
        "update-config",
        "Change KodAct's settings files: permissions, hooks, environment variables, model and more",
    ),
    (
        "verify",
        "Prove a change works by running it, not by reading it",
    ),
];

/// The skills you can run with `/name`: the project's and yours (from the
/// folders KodAct CLI reads, `forge_files::skills`), then KodAct CLI's own.
/// A skill marked `user-invocable: false` is the model's alone. Reads
/// files: call it off the UI thread.
pub fn skills(project: &std::path::Path) -> Vec<Command> {
    let own = crate::forge_files::skills(project)
        .into_iter()
        .filter(|s| s.enabled && s.problem.is_none())
        .filter(|s| {
            std::fs::read_to_string(s.dir.join("SKILL.md")).map_or(true, |md| invocable(&md))
        })
        .map(|s| Command {
            name: s.name,
            args: String::new(),
            description: s.description,
            kind: CommandKind::Prompt,
        })
        .collect();
    with_shipped(own)
}

/// The skills a remote's KodAct CLI loads: its own folders' (each SKILL.md's
/// frontmatter, read over the connection: `remote::skill_heads`), then
/// KodAct's plugin and KodAct CLI's own, as [`skills`] lists them here.
pub fn remote_skills(heads: &[(String, String)]) -> Vec<Command> {
    let own = heads
        .iter()
        .filter(|(_, md)| invocable(md))
        .filter_map(|(folder, md)| {
            let doc = crate::forge_files::Doc::parse(md);
            let description = doc.get("description").filter(|d| !d.is_empty())?;
            Some(Command {
                name: doc
                    .get("name")
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| folder.clone()),
                args: String::new(),
                description,
                kind: CommandKind::Prompt,
            })
        })
        .collect();
    with_shipped(own)
}

/// A skill you may run yourself: not marked `user-invocable: false`.
fn invocable(md: &str) -> bool {
    !md.lines()
        .take_while(|l| !l.is_empty())
        .any(|l| l.replace(' ', "") == "user-invocable:false")
}

/// Your skills, sorted, then the ones that come with KodAct: its plugin's
/// (`kodact:diagram-design`) and KodAct CLI's own.
fn with_shipped(mut out: Vec<Command>) -> Vec<Command> {
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    let shipped: Vec<Command> = plugin_skills()
        .chain(bundled_skills())
        .filter(|b| !out.iter().any(|c| c.name == b.name))
        .collect();
    out.extend(shipped);
    out
}

/// KodAct's plugin's skills, as KodAct CLI names them (`kodact:diagram-design`).
pub fn plugin_skills() -> impl Iterator<Item = Command> {
    crate::plugin::skills()
        .into_iter()
        .map(|(name, description)| Command {
            name,
            args: String::new(),
            description,
            kind: CommandKind::Prompt,
        })
}

/// KodAct CLI's own skills, which every project has.
pub fn bundled_skills() -> impl Iterator<Item = Command> {
    BUNDLED_SKILLS.iter().map(|(name, what)| Command {
        name: name.to_string(),
        args: String::new(),
        description: what.to_string(),
        kind: CommandKind::Prompt,
    })
}

/// Whether `text` is a command KodAct CLI answers at once (`/status`, `/usage`…).
pub fn is_immediate(text: &str) -> bool {
    let Some((cmd, _)) = parse_slash(text) else {
        return false;
    };
    let name = cmd.to_ascii_lowercase().replace('_', "-");
    BUILTIN.iter().any(|(n, _, _, i, _)| *i && *n == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remote_lists_its_own_skills_and_forge_s() {
        let heads = vec![
            (
                "tls-notes".to_string(),
                "---\nname: tls-notes\ndescription: Explain TLS\n---\n".to_string(),
            ),
            (
                "nameless".to_string(),
                "---\ndescription: Folder-named\n---\n".to_string(),
            ),
            (
                "model-only".to_string(),
                "---\nname: model-only\ndescription: x\nuser-invocable: false\n---\n".to_string(),
            ),
            ("broken".to_string(), "---\nname: broken\n---\n".to_string()),
        ];
        let names: Vec<String> = remote_skills(&heads).into_iter().map(|c| c.name).collect();
        assert_eq!(names[..2], ["nameless", "tls-notes"]);
        assert!(!names.iter().any(|n| n == "model-only" || n == "broken"));
        assert!(
            names.iter().any(|n| n == "kodact:diagram-design"),
            "{names:?}"
        );
        assert!(names.iter().any(|n| n == "init"));
    }

    #[test]
    fn immediate_commands_are_known() {
        assert!(is_immediate("/status"));
        assert!(is_immediate("  /context all"));
        assert!(!is_immediate("/compact"));
        assert!(!is_immediate("status"));
        assert!(!is_immediate("/btw hi"), "the side card handles /btw");
    }

    #[test]
    fn the_desktop_leaves_out_what_would_pull_the_chat_away() {
        let names: Vec<String> = builtin().into_iter().map(|c| c.name).collect();
        for gone in [
            "exit", "clear", "resume", "branch", "cd", "rewind", "model", "rename", "btw", "color",
            "theme",
        ] {
            assert!(!names.iter().any(|n| n == gone), "{gone}");
        }
        assert!(names.iter().any(|n| n == "compact"));
    }

    #[test]
    fn two_tier_classification_and_alias_normalization() {
        for action in [
            "/plugin",
            "/plugins",
            "/show-plugins",
            "/show_plugins",
            "/skills",
            "/show_skills",
            "/reload-plugins",
            "/reload_plugins",
            "/reload-skills",
            "/reload_skills",
            "/mcp",
            "/mcp reconnect all",
            "/hooks",
            "/agents",
            "/permissions",
            "/sandbox",
            "/memory",
            "/autocompact",
            "/config",
            "/settings",
            "/slash-commands",
            "/commands",
            "/model sonnet",
            "/models",
            "/effort low",
            "/context",
            "/context all",
            "/context-window",
            "/tasks",
            "/status",
            "/usage",
            "/doctor",
            "/diff",
            "/compact",
            "/compact keep api notes",
            "/fast on",
            "/release-notes",
            "/help",
            "/guide",
            "/about",
            "/new",
            "/clear",
            "/reset",
            "/theme",
            "/plan",
            "/goal",
            "/goal clear",
            "/subtask",
            "/loop",
        ] {
            assert_eq!(
                classify(action),
                Some(CommandKind::Action),
                "expected Action for {action}"
            );
        }

        for prompt in [
            "/review check error handling",
            "/kodact:xlsx build budget",
            "/i-have-adhd hi",
            "/plan redesign auth",
            "/goal cargo test passes with 0 warnings",
            "/subtask audit auth middleware",
            "/loop 5m check build status",
            "/debug login timeout",
            "/recap",
            "/simplify",
            "/init",
        ] {
            assert_eq!(
                classify(prompt),
                Some(CommandKind::Prompt),
                "expected Prompt for {prompt}"
            );
        }

        assert_eq!(classify("plain message"), None);
        assert_eq!(classify("/usr/local/bin/kodact"), None);
        assert_eq!(classify("//comment"), None);
        assert_eq!(normalize_action("show_plugins"), Some("plugins"));
        assert_eq!(normalize_action("reload_skills"), Some("reload-skills"));
    }

    #[test]
    fn project_commands_are_read_from_disk() {
        let dir = std::env::temp_dir().join(format!("forge-cmds-{}", std::process::id()));
        let cmds = dir.join(".forge").join("commands");
        std::fs::create_dir_all(&cmds).unwrap();
        std::fs::write(
            cmds.join("ship.md"),
            "---\ndescription: \"Ship it\"\n---\nRun the release.",
        )
        .unwrap();
        std::fs::write(cmds.join("lint.md"), "# Lint everything\n").unwrap();
        // Yours (KodAct CLI's config folder) may add more; these are the project's.
        let c: Vec<Command> = custom(&dir)
            .into_iter()
            .filter(|c| c.name == "lint" || c.name == "ship")
            .collect();
        assert_eq!(
            c.iter()
                .map(|c| (c.name.as_str(), c.description.as_str()))
                .collect::<Vec<_>>(),
            vec![("lint", "Lint everything"), ("ship", "Ship it")]
        );
        assert!(c.iter().all(|c| c.kind == CommandKind::Prompt));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
