# ForgeCLI coverage

What ForgeCLI can do, and where Forge GPUI offers it. Checked against
ForgeCLI branch `ccr-02a743e1-1gmu4f` (commit 9badc14), driven live through
`forge -p --input-format stream-json` with a scripted model.

Each chat is one ForgeCLI process (`src/backend.rs`). Anything ForgeCLI
reads from its settings files (permissions, hooks, sandbox, MCP servers,
agents, skills, plugins, environment) works in every chat as it is; the app's
editors write those same files, so they need no flag of their own.

## Host protocol

| ForgeCLI | Forge GPUI |
|---|---|
| `initialize` (command catalogue, models, agents) | Sent when a session starts; its commands join the `/` menu (`commands::from_catalog`) |
| `interrupt` | Stop button, Esc, and Send now on a queued message |
| `set_permission_mode` | Mode menu, Shift+Tab, palette "Switch to …" |
| `set_model` | Model menu, mid-session |
| `set_max_thinking_tokens` | `forge.maxThinkingTokens` (as `FORGE_MAX_THINKING_TOKENS`); effort models restart with `--effort` |
| `mcp_status` | Session panel, MCP servers card (at start, after each turn, when the panel opens) |
| `mcp_reconnect`, `mcp_toggle` | That card's restart and on/off buttons |
| `rewind_files` | Revert on a turn's changes |
| `stop_task` | Stop on a background task |
| `cancel_queued` | Remove on a queued message |
| `get_context_usage` | Session panel, Context card and the context ring |
| Permission requests, `AskUserQuestion`, `ExitPlanMode` | Permission card, question card, plan card |

## Events

| ForgeCLI `system` subtype | Shown as |
|---|---|
| `init` | Session start: model, tools, MCP servers |
| `status` (compacting) | The working line |
| `compact_boundary` | "Compacted" line (automatic ones get a notice) |
| `model_fallback` | Notice: which model took over |
| `subtask` | Notice when a sub-task starts and finishes |
| `goal` | Goal bar |
| `loop_guard`, `verification` | Notice lines |
| `background_exit`, `background_wait` | Notice lines; the task list updates |
| `tool_input_repaired` | Notice line |

Notices are plain lines in the transcript (no Retry). `microcompact`, `todos`
and `permission_mode` only live in ForgeCLI's transcript files; a resumed
session replays their effect itself.

## Tools

Every built-in tool reads as words in the activity line
(`backend::tool_call`, test `every_forgecli_tool_reads_as_words`): Bash,
BashOutput, KillShell, Glob, Grep, LS, Read, Edit, MultiEdit, Write,
NotebookEdit, TodoWrite (the plan panel), AskUserQuestion, EnterPlanMode,
ExitPlanMode, WebFetch, WebSearch, Skill, Task (sub-agents), CronCreate,
CronList, CronDelete, ScheduleWakeup, Advisor, ListMcpResourcesTool,
ReadMcpResourceTool. MCP tools show as "server · tool".

## Permission modes

All six, in the mode menu: Manual (`default`), Edit automatically
(`acceptEdits`), Auto (`auto`), Plan (`plan`), Don't ask (`dontAsk`), Bypass
permissions (`bypassPermissions`, behind its warning). Shift+Tab goes round
Manual, Edit automatically and Plan (and Bypass once allowed), as ForgeCLI's
own prompt does. A mode ForgeCLI reports (`ModeId::from_cli`) updates the
chip.

## Sessions

| ForgeCLI | Forge GPUI |
|---|---|
| `--session-id`, `--resume` | Every chat; reopening a chat resumes it |
| `--resume … --fork-session` | Duplicate chat (the copy continues the conversation) |
| `--worktree` | `forge.worktree` |
| `--settings` | The project's settings profile |
| `--append-system-prompt(-file)` | Project instructions and Design chats |
| `--mcp-config`, `--plugin-dir` | Design tools and browser MCP; installed plugins |
| `--permission-prompt-tool` | The app's permission card |

## Launch options (`settings.json`)

| Key | Flag |
|---|---|
| `forge.fallbackModel` | `--fallback-model` |
| `forge.maxTurns` | `--max-turns` |
| `forge.maxBudgetUsd` | `--max-budget-usd` |
| `forge.maxTimeMinutes` | `--max-time` |
| `forge.autonomous` | `--autonomous` |
| `forge.addDirs` | `--add-dir` (one per folder) |
| `forge.agent` | `--agent` |
| Model menu effort | `--effort` |

## Configuration files

Permissions (allow, ask, deny: what `--allowed-tools` and
`--disallowed-tools` set for one run), hooks, sandbox, MCP servers (what
`forge mcp add/remove` writes), agents (what `--agents` passes inline),
skills, slash commands, plugins and marketplaces, environment, endpoints and
memory each have an editor under Customize, writing ForgeCLI's own files at
user, project or local scope.

## Slash commands

The `/` menu is ForgeCLI's catalogue, as `initialize` returns it for a host,
plus the project's custom commands and skills. The desktop does these itself
and leaves them out (`commands::HANDLED`): `/exit`, `/quit`, `/clear`,
`/reset`, `/new` (New chat), `/resume`, `/continue` (the sidebar), `/branch`
(Duplicate), `/cd` (a chat stays in its project; open another project
instead), `/rewind`, `/checkpoint`, `/undo` (Revert), `/model` (model menu),
`/rename` (chat title), `/btw` (side chat).

## Left out, on purpose

- Terminal-only features: the TUI, its themes, key bindings, status line and
  `/terminal-setup`. ForgeCLI keeps them out of the host catalogue.
- `--output-format text|json`, `--json-schema`, `--no-input`, `--quiet`,
  `--color`: one-shot shell use; the app always speaks stream-json.
- `--name`: the app names chats itself.
- `--include-hook-events`: ForgeCLI emits nothing for it yet; hook output
  that reaches the model shows in the turn.
- `--bare`, `--safe-mode`, `--setting-sources`, `--strict-mcp-config`,
  `--system-prompt`, `--exclude-dynamic-system-prompt-sections`, `--betas`,
  `--debug`, `--no-session-persistence`: for troubleshooting ForgeCLI from a
  shell. Chats must persist to be reopened.
- Subcommands `doctor`, `config`, `completion` (also `/doctor` and `/config`
  in a chat), `relay` and `mcp serve` (plumbing for other hosts).
