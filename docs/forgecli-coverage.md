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
| `forge.includeHookEvents` | `--include-hook-events`, with `--debug=hooks --verbose`: each hook run shows as a line (below) |
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
model, endpoint, mode and settings (palette). On WSL, SSH and container
projects the tab runs the remote's ForgeCLI in the project's folder, and its
model calls come back through the app's proxy (an SSH `-R` forward, or one
ForgeCLI relay per distro or container, waited for before the tab opens), so
the real key stays on this computer, as it does for those chats.

| Palette | Runs |
|---|---|
| Open this chat in ForgeCLI's terminal UI | `forge --resume <chat> --name <title>` (with no chat open, `--continue`; `forge.noTui` for the line prompt): the TUI and everything only it has (themes, key bindings, status line, `/terminal-setup`, `/color`, `/focus`, `/copy`...). The chat's own process steps aside so one process writes the conversation; the chat resumes it on its next message (close the tab first) |
| New terminal with ForgeCLI ready | A shell with `forge` on the path, `FORGE_MODEL` and the endpoint set: one-shot runs (`forge -p --output-format text\|json`, `--json-schema`, `--quiet`, `--no-input`, `--color`), `forge --help`, `--version`, `forge completion <shell>`, `forge config get/paths`, `forge mcp add/add-json/remove/get/approve/list`, `forge mcp serve` |
| Check ForgeCLI and the endpoint | `forge doctor --probe` |
| Show ForgeCLI's merged settings | `forge config list` |

`forge relay` also carries the browser, design tools and model calls of WSL
and container chats (`src/relay.rs`). `forge mcp list` is the MCP editor's
check, and its Approve writes what `forge mcp approve` does.

## Checked

- `backend::tests::every_forgecli_flag_reaches_forgecli`: each of the 56
  flags `forge --help` lists reaches ForgeCLI from a chat's launch, a
  `forge.*` setting, or the app around them.
- Live on Linux, local project: every new setting at once on one chat (the
  process took them all, the turn ran, the debug log was written); a
  structured result under the reply; retry lines in an autonomous run on an
  overloaded model; the TUI resumed the chat's session (`/status`); the
  shell ran a one-shot JSON run and `forge completion bash`; `forge doctor
  --probe` and `forge config list`.
- Live on Linux, container project (`container://forgebox/root/proj`): a
  chat; the TUI on that chat in the container (the model saw both the chat's
  and the TUI's message); `forge doctor --probe` through the relay (the key
  it saw was the placeholder); the shell ran `forge --version`, `forge
  config paths` and a one-shot JSON run.
- Live on Linux, SSH project (`ssh://box/…`, an `sshd` on 127.0.0.1 run
  from the scratchpad): a chat; the TUI on that chat over SSH, its model
  calls on an `ssh -R` forward to the proxy (the model saw both messages);
  `forge doctor --probe` (the placeholder key); the shell ran `forge
  --version` and `forge mcp list`.
- Live on Linux, hooks: a PreToolUse hook's `systemMessage` and a
  PostToolUse hook's error (exit 3) each showed as a line in the chat,
  between the tool steps they ran on.
- WSL runs only on Windows: its tabs use the same relay path as containers
  (checked above) with `wsl.exe` in place of `docker`, not run here.

## Notes

- Hook runs: ForgeCLI at this commit declares `--include-hook-events` but
  sends no hook events on its stream. It logs each run (`forge_hooks:
  running hooks event=… matcher=…`) with `--debug=hooks`, and with
  `--verbose` writes what its hooks said (a message, an error, a block,
  "Stopped by hook") to stderr; the app reads both (`backend::HookLog`) and
  shows each run as "PreToolUse hook on Bash: …". `hook_response` stream
  events are read too, for when ForgeCLI sends them. With `forge.quiet`
  only the runs show, not what the hooks said.
- `api_retry` comes only from autonomous runs (`forge.autonomous`).
- ForgeCLI only fills `structured_output` when the final reply is JSON that
  parses (`forge-engine`, `TurnResult`).
