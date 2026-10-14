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

## Keys

The same list is the shortcuts sheet (F1) and the Guide (Settings → Guide);
`src/ui/shortcuts.rs` is its source, and tests keep it, `main.rs` and this
table in step.

| Keys | Does |
| --- | --- |
| Ctrl/⌘ K | Search or run a command |
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
| Enter / Shift Enter | Send / new line |

## Layout

- `src/theme.rs`: every colour, size, radius and font. Nothing else hardcodes them.
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
- `src/assets.rs`: bundled icons (Lucide, ISC) and fonts (Hanken Grotesk, JetBrains Mono, Source Serif 4; OFL).
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
    `planned` (Network), `guide`, and `editor_kit`, the pieces they share.
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
