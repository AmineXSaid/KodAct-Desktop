//! ForgeCLI's slash commands, as the composer's `/` menu offers them.
//!
//! The list is ForgeCLI's own catalogue (`forge-core/src/commands/mod.rs`),
//! less the commands that only work in its terminal UI and those the desktop
//! does itself or that would pull the chat out from under it (`/exit`,
//! `/clear`, `/resume`, `/branch`, `/cd`, `/rewind`, `/model`, `/rename`,
//! `/btw` — the side card is the desktop's `/btw`). Custom commands a project
//! keeps in `.forge/commands/*.md` are added from disk.

/// A command: its name, the arguments it takes, and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub name: String,
    pub args: String,
    pub description: String,
}

/// (name, args, description, answered at once)
const BUILTIN: &[(&str, &str, &str, bool)] = &[
    (
        "compact",
        "[instructions]",
        "Free context by summarizing the conversation so far",
        false,
    ),
    (
        "context",
        "[all]",
        "Show what fills the context window",
        true,
    ),
    (
        "usage",
        "",
        "Show this session's cost, token use and activity",
        true,
    ),
    (
        "status",
        "",
        "Show version, model, session and setup status",
        true,
    ),
    (
        "tasks",
        "[stop <id>]",
        "View and stop background work in this session",
        true,
    ),
    (
        "mcp",
        "[reconnect|enable|disable <server|all>]",
        "Show MCP server status, or reconnect, enable or disable a server",
        true,
    ),
    (
        "plan",
        "[description]",
        "Enter plan mode; with a description, start planning it",
        false,
    ),
    (
        "goal",
        "[condition|clear]",
        "Set a goal Forge keeps working toward until a check finds it met",
        false,
    ),
    (
        "loop",
        "[interval] [prompt]",
        "Run a prompt on a schedule, or let Forge pace it",
        false,
    ),
    (
        "subtask",
        "<task>",
        "Hand a task to a background agent that starts from this conversation",
        false,
    ),
    (
        "diff",
        "",
        "Show uncommitted changes, and the files each prompt changed",
        false,
    ),
    ("recap", "", "Summarize the session in one line", false),
    (
        "debug",
        "[description]",
        "Turn on debug logging, and have Forge read the log to find a problem",
        false,
    ),
    (
        "doctor",
        "",
        "Check the installation and this session's setup",
        false,
    ),
    (
        "effort",
        "[low|medium|high|xhigh|max|auto]",
        "Show or set the model's reasoning effort",
        false,
    ),
    (
        "fast",
        "[on|off]",
        "Turn fast mode on or off, where the model offers it",
        false,
    ),
    (
        "advisor",
        "[model|off]",
        "Let Forge consult a second model for advice at key moments",
        false,
    ),
    (
        "autocompact",
        "[on|off|auto|<tokens>]",
        "Show or set when the conversation is compacted automatically",
        false,
    ),
    (
        "memory",
        "",
        "List the memory files in use (FORGE.md, AGENTS.md)",
        false,
    ),
    (
        "permissions",
        "[add|remove ...]",
        "Show the permission rules and working directories, or change the rules",
        false,
    ),
    (
        "sandbox",
        "[on|off|read-only|workspace-write]",
        "Show or change the OS sandbox for shell commands",
        false,
    ),
    (
        "hooks",
        "",
        "View the configured hooks, or add or remove one",
        false,
    ),
    (
        "agents",
        "[create <name> ...]",
        "List subagents, or create one",
        false,
    ),
    ("skills", "", "List available skills", false),
    ("plugin", "[list]", "List loaded plugins", false),
    (
        "output-style",
        "[style]",
        "Show the output styles, or switch to one",
        false,
    ),
    (
        "add-dir",
        "<path> [--save]",
        "Add a working directory for this session",
        false,
    ),
    (
        "config",
        "[key=value ...]",
        "Show the settings, or change them with key=value",
        false,
    ),
    (
        "export",
        "[file]",
        "Export the conversation as plain text",
        false,
    ),
    (
        "feedback",
        "[description]",
        "Save a bug-report bundle on this machine (nothing is uploaded)",
        false,
    ),
    (
        "import",
        "[codex|gemini|cursor] [--yes]",
        "Bring MCP servers and instructions over from other coding agents",
        false,
    ),
    (
        "reload-skills",
        "",
        "Reload skills, commands and agents without restarting",
        false,
    ),
    (
        "reload-plugins",
        "",
        "Reload plugins without restarting",
        false,
    ),
    (
        "release-notes",
        "",
        "Show what changed in each version",
        false,
    ),
    ("help", "", "Show help and the available commands", false),
];

/// ForgeCLI's commands the desktop does itself, with its own controls, or
/// that would pull the chat out from under it: `/` never offers them, even
/// when a session lists them (`from_catalog`).
pub const HANDLED: &[&str] = &[
    "exit", "quit", "clear", "reset", "new", "resume", "continue", "branch", "cd", "rewind",
    "checkpoint", "undo", "model", "rename", "btw",
];

/// A session's commands as ForgeCLI lists them (the `initialize` answer's
/// `commands`: `{name, description, argumentHint}`).
pub fn from_catalog(rows: &serde_json::Value) -> Vec<Command> {
    rows.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let name = r["name"].as_str()?.trim_start_matches('/').to_string();
            (!name.is_empty()).then(|| Command {
                name,
                args: r["argumentHint"].as_str().unwrap_or_default().to_string(),
                description: r["description"].as_str().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// ForgeCLI's built-in commands the desktop offers.
pub fn builtin() -> Vec<Command> {
    BUILTIN
        .iter()
        .map(|(n, a, d, _)| Command {
            name: n.to_string(),
            args: a.to_string(),
            description: d.to_string(),
        })
        .collect()
}

/// Your commands and the project's, as ForgeCLI finds them
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
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The skills ForgeCLI ships (`forge-agents/bundled`), by name and what
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
        "Write or improve FORGE.md, the project's instructions for Forge",
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
        "Change Forge's settings files: permissions, hooks, environment variables, model and more",
    ),
    (
        "verify",
        "Prove a change works by running it, not by reading it",
    ),
];

/// The skills you can run with `/name`: the project's and yours (from the
/// folders ForgeCLI reads, `forge_files::skills`), then ForgeCLI's own.
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
        })
        .collect();
    with_shipped(own)
}

/// The skills a remote's ForgeCLI loads: its own folders' (each SKILL.md's
/// frontmatter, read over the connection: `remote::skill_heads`), then
/// Forge's plugin and ForgeCLI's own, as [`skills`] lists them here.
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

/// Your skills, sorted, then the ones that come with Forge: its plugin's
/// (`forge:diagram-design`) and ForgeCLI's own.
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

/// Forge's plugin's skills, as ForgeCLI names them (`forge:diagram-design`).
pub fn plugin_skills() -> impl Iterator<Item = Command> {
    crate::plugin::skills()
        .into_iter()
        .map(|(name, description)| Command {
            name,
            args: String::new(),
            description,
        })
}

/// ForgeCLI's own skills, which every project has.
pub fn bundled_skills() -> impl Iterator<Item = Command> {
    BUNDLED_SKILLS.iter().map(|(name, what)| Command {
        name: name.to_string(),
        args: String::new(),
        description: what.to_string(),
    })
}

/// Whether `text` is a command ForgeCLI answers at once (`/status`, `/usage`…).
pub fn is_immediate(text: &str) -> bool {
    let Some(rest) = text.trim().strip_prefix('/') else {
        return false;
    };
    let name = rest.split_whitespace().next().unwrap_or_default();
    BUILTIN.iter().any(|(n, _, _, i)| *i && *n == name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_session_catalog_reads_as_commands() {
        let rows = serde_json::json!([
            {"name": "compact", "description": "Free context", "argumentHint": "[instructions]"},
            {"name": "deploy", "description": "Ship it", "argumentHint": ""},
            {"name": "mcp__docs__summarize", "description": "MCP prompt", "argumentHint": ""},
            {"name": "", "description": "nameless"},
            {"description": "no name"}
        ]);
        let c = super::from_catalog(&rows);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].args, "[instructions]");
        assert_eq!(c[2].name, "mcp__docs__summarize");
        // What the desktop does itself never reaches `/`.
        for name in ["clear", "resume", "rewind", "model", "btw", "cd"] {
            assert!(super::HANDLED.contains(&name), "{name}");
        }
    }

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
            names.iter().any(|n| n == "forge:diagram-design"),
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
        // Yours (ForgeCLI's config folder) may add more; these are the project's.
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}
