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

/// The project's own commands: `.forge/commands/<name>.md`, described by
/// their `description:` front matter or their first line (blocking: call
/// off the UI thread).
pub fn custom(project: &std::path::Path) -> Vec<Command> {
    let Ok(entries) = std::fs::read_dir(project.join(".forge").join("commands")) else {
        return vec![];
    };
    let mut out: Vec<Command> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let description = text
                .lines()
                .find_map(|l| {
                    l.strip_prefix("description:")
                        .map(|d| d.trim().trim_matches('"').to_string())
                })
                .or_else(|| {
                    text.lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty() && *l != "---")
                        .map(|l| l.trim_start_matches('#').trim().to_string())
                })
                .unwrap_or_else(|| "Project command".into());
            Some(Command {
                name,
                args: String::new(),
                description,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
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
    use super::*;

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
        let c = custom(&dir);
        assert_eq!(
            c.iter()
                .map(|c| (c.name.as_str(), c.description.as_str()))
                .collect::<Vec<_>>(),
            vec![("lint", "Lint everything"), ("ship", "Ship it")]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
