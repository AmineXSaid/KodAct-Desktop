# ForgeCLI coverage

Every ForgeCLI capability, and where Forge GPUI offers it. Checked against
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
| `hook_response` (with `forge.includeHookEvents`) | "Hook … ran" / "failed", with its output |
| `api_retry` | Notice: the failed call and the next attempt |
| `result.structured_output` (with `forge.jsonSchema`) | A JSON block under the reply |

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

Every flag a chat's process can carry has a key; each was passed to a live
chat at once and the turn ran.

| Key | Flag |
|---|---|
| `forge.fallbackModel` | `--fallback-model` |
| `forge.maxTurns` | `--max-turns` |
| `forge.maxBudgetUsd` | `--max-budget-usd` |
| `forge.maxTimeMinutes` | `--max-time` |
| `forge.worktree` | `--worktree` |
| `forge.autonomous` | `--autonomous` |
| `forge.maxThinkingTokens` | `FORGE_MAX_THINKING_TOKENS` (`set_max_thinking_tokens`) |
| `forge.addDirs` | `--add-dir` (one per folder) |
| `forge.agent` | `--agent` |
| `forge.tools` | `--tools` (`[]` offers none) |
| `forge.allowedTools`, `forge.disallowedTools` | `--allowedTools`, `--disallowedTools` |
| `forge.sandbox` | `--sandbox` (`off`, `read-only`, `workspace-write`) |
| `forge.systemPrompt`, `forge.systemPromptFile` | `--system-prompt`, `--system-prompt-file` |
| `forge.excludeDynamicSystemPromptSections` | `--exclude-dynamic-system-prompt-sections` |
| `forge.strictMcpConfig` | `--strict-mcp-config` |
| `forge.settingSources` | `--setting-sources` |
| `forge.bare`, `forge.safeMode` | `--bare`, `--safe-mode` |
| `forge.betas` | `--betas` |
| `forge.debug`, `forge.debugFile` | `--debug`, `--debug-file` |
| `forge.autocompact` | `--autocompact` |
| `forge.includeHookEvents` | `--include-hook-events`; hook results show as lines (`hook_response`) |
| `forge.noSessionPersistence` | `--no-session-persistence`; such a chat starts afresh with each process |
| `forge.noInput` | `--no-input` |
| `forge.jsonSchema` | `--json-schema`; the turn's `structured_output` shows under the reply |
| `forge.extraArgs` | any other flag (`--color never`, `--verbose`) |
| Model menu effort | `--effort` |
| Chat title | `--name` |

`forge.extraArgs` drops the flags the chat runs on (print mode, the stream
formats, the session, the permission channel and mode, the model, and the
prompt and settings Forge passes once), and skipping permissions, which goes
through Settings → Bypass permissions behind its warning.

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
`/rename` (chat title), `/btw` (side chat). Commands ForgeCLI keeps to its
terminal UI run there, in the dock (below), as do all of these.

## ForgeCLI in the terminal dock

What only a terminal has runs in the app's own terminal, with the chats'
model, endpoint, key, mode and settings (palette):

| Palette | Runs |
|---|---|
| Open this chat in ForgeCLI's terminal UI | `forge --resume <chat>`: the TUI and everything only it has (themes, key bindings, status line, `/terminal-setup`, `/color`, `/focus`, `/copy`...). The chat's own process steps aside so one process writes the conversation; the chat resumes it on its next message (close the tab first) |
| New terminal with ForgeCLI ready | Your shell, `forge` on the path, `FORGE_MODEL` and the endpoint set: one-shot runs (`forge -p --output-format text\|json`, `--json-schema`, `--quiet`, `--no-input`, `--color`), `forge completion <shell>`, `forge mcp add/list/remove`, `forge mcp serve` (Forge's tools for another MCP client) and any other subcommand |
| Check ForgeCLI and the endpoint | `forge doctor --probe` |
| Show ForgeCLI's merged settings | `forge config` |

`forge relay` carries the browser, design tools and model calls to WSL and
container projects (`src/relay.rs`), so those chats need no open port.

Seen live on Linux: the TUI resumed the chat's session (`/status` showed its
id, endpoint and project), a one-shot `--output-format json` run and
`forge completion bash` in the shell, and `forge doctor --probe` reaching
the endpoint.

## Notes

- `--include-hook-events` is passed, and `hook_response` lines are read, but
  this ForgeCLI branch doesn't emit hook events yet: nothing shows until it
  does. `api_retry` lines are read the same way.
- ForgeCLI only fills `structured_output` when the final reply is JSON that
  parses (`forge-engine`, `TurnResult`).
- The terminal tabs open for projects on this computer; a remote project's
  chats still get every flag above.
