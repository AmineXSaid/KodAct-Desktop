# forge-gpui

The Forge desktop app, written in Rust with [GPUI](https://www.gpui.rs/) and
gpui-component (through `gpui-kit`, Apache-2.0). Each chat runs
[ForgeCLI](https://github.com/AmineXSaid/ForgeCLI) (`forge`) as its engine,
on the endpoint picked in the model menu. The chats the sidebar starts with
are demo data.

## Run

```sh
cargo run
cargo test
```

Forge needs two things to answer:

- **ForgeCLI**: `forge.exe` (`forge` elsewhere) beside the app, on PATH, or
  named by `FORGE_CLI`. Build it from ForgeCLI's `ULTRA01` branch with
  `cargo build --release -p forge-cli`.
- **An endpoint**: a profile in `endpoints.json` (the model menu opens it).
  Its URL, model and key reach `forge` as `FORGE_*` variables
  (`src/backend.rs`); the key is read from the environment variable the
  profile names.

`FORGE_DEMO=1` plays the scripted demo reply instead, with no CLI or model.

On first launch Forge asks what to call you and keeps the name in its config
folder (`%APPDATA%\Forge` on Windows; `FORGE_CONFIG_DIR` overrides it).

Forge opens on plain chats, as a chat app does: no folder, so a question or
a lesson never runs in a folder of yours (they run in `workspace` inside the
config folder). Pick a folder from the chip under the message box, or from
Projects, to work on code there. Started on a folder (`cargo run -- .`,
`forge-gpui C:\code\app`, or `FORGE_PROJECT_DIR`), it opens on that project.

## Keys

The same list is the shortcuts sheet (F1) and the Guide (Settings → Guide);
`src/ui/shortcuts.rs` is its source, and tests keep it, `main.rs` and this
table in step.

| Keys | Does |
| --- | --- |
| Ctrl/⌘ K | Search files, chats and commands |
| F1 | Keyboard shortcuts |
| Ctrl/⌘ , | Settings |
| Ctrl/⌘ Shift L | Light or dark theme |
| Ctrl/⌘ B | Show or hide the sidebar |
| Ctrl/⌘ Q | Quit |
| Ctrl/⌘ N | New chat |
| F2 | Rename this chat |
| Ctrl/⌘ Shift P | Projects |
| Ctrl/⌘ Shift R | Rules and memory |
| Ctrl/⌘ Shift A | Chats that need you |
| Ctrl/⌘ Shift M | The project map |
| Ctrl/⌘ Alt O | Remote: WSL, SSH host or container |
| Ctrl ` | Show or hide the terminal |
| Ctrl Shift ` | New terminal |
| Ctrl/⌘ L | Focus the composer |
| Ctrl/⌘ U | Attach files and images |
| Ctrl/⌘ Shift T | This turn's tasks |
| Ctrl/⌘ Shift E | Side panel: plan, files, browser |
| Ctrl/⌘ \ | Side panel beside or below the chat |
| Ctrl/⌘ F | Find in the conversation |
| Ctrl/⌘ J | Jump to the latest message |
| Ctrl/⌘ Shift C | Copy the last reply |
| Ctrl/⌘ / | Choose a model |
| Shift Tab | Next permission mode |
| Esc | Stop the reply, or close what's open |
| Ctrl/⌘ Shift X | Stop every agent; the reply goes on |
| Enter / Shift Enter | Send / new line |

### Search

Ctrl/⌘ K (or the search field in the title bar) finds commands, chats, models
and modes, the project's files, and what was said in your chats, as you
type. Results stay grouped, best group first. Start with `>` for commands
only, `#` for chats, `@` for files. A document (Markdown, a diagram, an
Office file, a PDF) opens in the side panel; any other file in the app your
system opens it with; a chat found by what was said opens with the find bar
on the words. With nothing found, Enter asks Forge in a new chat. Arrows,
Page Up/Down and Ctrl+Home/End move through the list.

### Modes

- **Manual** and **Expert** ask before each edit. Commands that only read
  (`ls`, `git log`, `grep`) run without asking.
- **Edit automatically** edits files without asking, and runs commands that
  only read or only write files inside the project (`echo … > notes.txt`,
  `mkdir`). Deleting (`rm`, `git rm`), moving over a path, raising privileges
  (`sudo`) and rewriting or pushing history still ask.
- **Plan** explores and proposes; it edits nothing until you approve.
- **Bypass permissions** asks for nothing, once allowed in Settings.

`forge.autoApproveSafeCommands: false` in the settings file makes every
command ask, in every mode.

## Remote development

As in VS Code, the indicator at the left of the status bar says where the
project lives (plug icon: this computer; amber while connecting; filled when
connected; red when it failed), and its menu (Ctrl/⌘ Alt O, or the command
palette) moves it: recent folders, **Connect to SSH host** (from
`~/.ssh/config` or `user@host`), **Connect to WSL** (a distro), **Reopen in
container** (the folder's `.devcontainer`) and **Attach to running
container**, then a folder there. Inside a dev container, **Rebuild
container** starts it afresh (image rebuilt, `postCreateCommand` run again)
and **Reopen folder locally** goes back to the folder it came from.
A chat in a remote project runs its ForgeCLI on that machine, so its tools,
shell, git and files are the remote's; this app stays the UI
(`src/remote.rs`).

- The first connection installs ForgeCLI there (`~/.forge/bin`, or
  `remote.serverInstallDir`), from the static Linux build the installer puts
  in `bin\linux-x86_64\forge` (or `FORGE_REMOTE_CLI`), and checks the copy
  by SHA-256.
- SSH uses the system's OpenSSH client with your keys or agent. A password
  or a key's passphrase is asked in a dialog in the app (ssh's askpass, over
  a localhost socket with a random token), kept in memory for this run and
  never saved; a wrong one says so, and Try again asks again. A host seen
  for the first time shows its key fingerprint and waits for "Trust".
- Model calls leave from this computer: a remote ForgeCLI gets a local
  proxy's address (a port there leading back here: `ssh -R`, or ForgeCLI's
  relay for WSL and containers) and a placeholder key. The real key,
  address and headers never reach the remote (`src/model_proxy.rs`). The
  side panel's browser comes back the same way.
- If the connection drops, the next message reconnects and resumes the
  conversation.

Tested here: SSH (an OpenSSH server in a container), a running container,
and dev containers from an image and from a Dockerfile, each with the real
ForgeCLI and the scripted model (`cargo test real_ -- --include-ignored`,
see `src/remote.rs`). WSL runs the same way through `wsl.exe` but has only
been checked by its unit tests (no Windows machine here).

## Terminal

Ctrl+` (or the terminal button in the title bar) shows a terminal dock on
the right, in the current project's folder. Hiding it keeps its shells
running; the button shows a dot when one prints while hidden. Each tab is a
shell on a pseudo-terminal (ConPTY on Windows), drawn by a terminal emulator
(`alacritty_terminal`): colours, full-screen programs, scrollback.

- **Tabs:** + (Ctrl+Shift+`) opens another; the menu beside it picks the shell
  (PowerShell, cmd and WSL distros on Windows; `$SHELL`, bash, zsh, fish, sh
  elsewhere). A shell that ends says so, and any key closes its tab.
- **Remote projects:** the shell runs there, in the project folder, over
  `ssh -tt`, `wsl.exe` or `docker exec -it`; resizes reach it.
- **Keys go to the shell** (Ctrl+L, Ctrl+R, Ctrl+C...), except Ctrl+`,
  Ctrl+Shift+`, Ctrl+K and Ctrl+Alt+O. Copy with Ctrl+Shift+C, or Ctrl+C
  while text is selected; paste with Ctrl+Shift+V. Drag to select, double-
  click for a word, triple-click for a line.
- **Size:** drag the dock's left edge (between 280 px and three fifths of the
  window).
- **Right-click** for copy, paste, clear and kill; the palette has Focus,
  Clear and Kill the terminal too. The header's badge says where the active
  terminal runs (Local, or the remote).
- **Colour:** bash, zsh, sh, PowerShell and cmd start with a prompt in the
  dock's colours (bash, zsh and sh also colour `ls` and `grep`), after your
  own startup files; a prompt you coloured yourself is kept
  (`src/shell_prompt.rs`).
- The dock's open state and width come back at the next launch.
- **Agent commands:** each command the agent runs, and each approval it
  asks for, carries a badge saying where it runs ("on SSH: box", "on this
  computer"). "Show in terminal" under a command opens it and its output in
  a read-only Agent tab.

## Settings

Forge's own settings file, `settings.json` in its config folder ("Open Forge
settings" in the command palette writes one with every key):

| Key | Default | Does |
| --- | --- | --- |
| `remote.ssh.configFile` | `~/.ssh/config` | ssh config used to list and reach hosts (`ssh -F`) |
| `remote.ssh.extraArgs` | `[]` | more arguments for every `ssh` |
| `remote.serverInstallDir` | `~/.forge` | where ForgeCLI goes on a remote |
| `remote.reconnect.maxAttempts` | `8` | reconnect tries before a remote shows as failed |
| `terminal.defaultShell` | first found | the shell a new terminal runs |
| `terminal.scrollback` | `10000` | lines kept above the screen |
| `terminal.fontSize` | `13` | the terminal's text size |
| `terminal.cursorStyle` | `block` | `block`, `bar` or `underline` |
| `forge.autoApproveSafeCommands` | `true` | commands that only read (any mode) or only edit project files (Edit automatically) run without asking |

Changes apply the next time they're used; no restart.

## Layout

- `src/theme.rs`: every colour, size, radius and font, and the six themes that set them (each with its own faces, corners, density and depth). Nothing else hardcodes them. Pick one in Settings > General or the command palette ("Theme:").
- `src/state.rs`: the app state, the transcript model, demo chats and the scripted reply.
- `src/backend.rs`: each chat's `forge` process: launch, the stream-json events, permission answers.
- `src/forge_files.rs`: ForgeCLI's own files (commands, agents, skills, hooks,
  MCP servers, permissions, env, sandbox, plugin folders, FORGE.md), read and
  written exactly as ForgeCLI loads them; the Settings editors sit on it.
- `src/setting_profiles.rs`: settings profiles, passed to ForgeCLI as `--settings`.
- `src/diagnose.rs`: an endpoint's DNS, TCP, TLS, HTTP and key checks.
- `src/endpoints.rs`, `src/health.rs`, `src/secrets.rs`: endpoint profiles, their checks, and keys.
- `src/profile.rs`: your name and avatar on disk. `src/stubs.rs`: what isn't wired yet, and why.
- `src/web_view.rs`: the side panel's browser: a headless Chromium (Playwright's, else Chrome or Edge)
  shown live over the DevTools protocol, started through a guard (`--browser-guard`) so it closes
  with the app however the app ends.
- `src/browser_mcp.rs`: the browser tools for ForgeCLI's agents, served by this same program
  (`--browser-mcp`): navigate, state (numbered elements), click, type, press, select, hover,
  scroll, text, html, eval, wait, screenshot, console and tabs, on the page the panel shows.
- `src/assets.rs`: bundled icons (Lucide, ISC) and fonts (Archivo, Hanken Grotesk, Source Serif 4, JetBrains Mono, and the themes' Inter, Geist and Geist Mono, IBM Plex Sans and Mono, Instrument Sans and Serif; all OFL).
- `src/ui/`: one file per part:
  - the window: `workspace` (title bar, welcome), `sidebar`, `chat_view`,
    `message`, `markdown`, `composer`, `command_palette`, `onboarding`,
    `shortcuts`, `projects`, `rules_view`, `doc_pane` (a file in the side panel,
    beside or below the chat), `workbench` (the side panel's Plan, Files
    and Browser tabs), `browser_view` (the live browser in it), `agents_view` (sub-agents at work), `project_map`;
  - in a chat: `attachments`, `autocomplete`, `context_menu`, `tasks`,
    `approvals`, `versions`, `find_bar`, `terminal`, `file_card`;
  - Settings: `settings` (the window and its nav, General, Models, About),
    `diagnostics`, `profiles_editor`, `slash_commands`, `agents_editor`,
    `skills_editor`, `hooks_editor`, `mcp_editor`, `permissions_editor`,
    `sandbox_editor`, `env_editor`, `memory_view`, `plugins_editor`,
    `planned` (Network), `guide`, and `editor_kit`, the pieces they share;
  - the brand: `ingot` (Forge's mark, the ingot cube, still and turning, and
    the wordmark; its faces are SVGs drawn from its geometry and tinted from
    the theme).
- `scripts/make_ico.py`: `assets/forge.ico`, the ingot on an accent tile, each
  size Windows asks for drawn on its own.
- `scripts/screenshots.sh`: light and dark screenshots of every screen (see below).
- `.github/workflows/windows.yml`: fmt, clippy (`-D warnings`) and tests on
  Linux for every push and pull request; the Windows build and tests.

## Screenshots

```sh
cargo build && scripts/screenshots.sh            # into screenshots/
```

It runs the app on a virtual display (Xvfb `:99`, 1280×860) in demo mode
with a throwaway config folder, drives it with `xdotool`, and saves
welcome, chat, map, split, projects and every Settings section, light and
dark. It needs Xvfb, xdotool, ImageMagick and a Vulkan driver (without a
GPU, Mesa's lavapipe: `mesa-vulkan-drivers`). Linux only.
