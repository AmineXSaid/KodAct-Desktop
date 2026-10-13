# Prompt: implement P3 of the Forge Desktop upgrade

You are working on **forge-gpui**, the Forge desktop chat app in Rust: GPUI and gpui-component through `gpui-kit =0.7.1`. Each chat runs a **ForgeCLI** process as its engine (stream-json). P1 and P2 are done.

**Implement P3 exactly as specified in `docs/forge-ui-upgrade_P3.md`.** Read it in full first.

## Repos (all on branch `claude/laughing-heisenberg-07mbcg`; push there, open no PR)

| Path | What it is | Use it for |
|---|---|---|
| `/home/user/forge-gpui` | the app; **all P3 work goes here** | |
| `/home/user/ForgeCLI` | the engine, Rust | the **truth** for every file format an editor writes |
| `/home/user/FORGE` | Forge for VS Code | the **reference UI** for every settings tab: `src/webview/src/components/settings/tabs/SettingsTab{SlashCommands,Hooks,Skills,Agents,MCPServers,Guide,Permissions,Sandbox,Network,Environments,MemoryAndRules,Plugins,Profiles}.vue`, plus `settings/guide/*`, `hooks.ts`, `plugins.ts` |
| `/home/user/Forge-Desktop` | the Tauri version | design notes in `docs/DESIGN.md` |

## Read before coding

1. **forge-gpui rules:** `CLAUDE.md`, `.claude/rules/ui.md`, `.claude/rules/theme.md`.
2. **Reports:** `docs/forge-ui-upgrade_P1-report.md` and `docs/forge-ui-upgrade_P2-report.md`, for what exists and how it is wired.
3. **Files you will touch:**
   - `src/ui/settings.rs`: today only General, Models and About exist. `SettingsTab` is in `src/state.rs`. The P3 brief's line numbers are stale, so search instead.
   - `src/stubs.rs`: what is left: `feedback-upload`, `continue-no-session`, `revert-no-checkpoint`, `reapply`, `share-link` (P3), and `revert-demo`, `compact-demo` (demo only).
   - `src/rules.rs`: the project walker and frontmatter parser.
   - `src/commands.rs`: already reads `.forge/commands/*.md`.
   - `src/plugin.rs`: the bundled plugin, passed as `--plugin-dir`.
   - `src/browser.rs`: the browser-use MCP server.
   - `src/backend.rs`: `launch()`, which builds ForgeCLI's arguments.
   - `src/profile.rs`: the config dir (`FORGE_CONFIG_DIR` overrides it).
   - `src/endpoints.rs` and `src/health.rs`: endpoint storage and health checks; ureq is already a dependency.
   - `.github/workflows/windows.yml`.
4. **ForgeCLI formats.** Before writing any editor, find in ForgeCLI's source exactly what it reads and where:
   - commands: `.forge/commands`;
   - hooks: the `hooks` key in settings, event names, matcher;
   - skills: `SKILL.md` dirs, and any setting that disables one;
   - agents: `.forge/agents/*.md` and their frontmatter;
   - MCP: `.mcp.json` and `mcpServers`;
   - settings files and their precedence, and what `--settings` accepts (for profiles).

   Start at `crates/forge-agents`, `crates/forge-mcp/src/config.rs`, `crates/forge-hooks`, `crates/forge-config` and `crates/forge-core/src/lib.rs`. **Editors must write files ForgeCLI actually loads**, at user level and project level, and keep the layout ForgeCLI expects. The VS Code tabs show the UX to match.

## What "done" means for each item

- **4A editors** (Slash Commands, Hooks, Skills, Agents, MCP Servers):
  - each is one file under `src/ui/`, with its state in `state.rs`;
  - list, create, edit and delete (with a confirm), validate input, and show empty, error and loading states;
  - write to disk, and reload when a file changes;
  - add a palette entry "Open <tab>…";
  - **prove it:** after saving, a new ForgeCLI session sees the item. Check its stream-json `init` line, or the Skill tool list a mock model receives.
- **4B Guide:** static and scrollable. The key table is generated from `main.rs`, the way `shortcuts.rs` does it, so it can't drift.
- **4C** Permissions, Sandbox, Network, Environments, Memory and Rules, Plugins: each is a "Planned" screen listing what it will do and which engine support it waits on, with **no fake controls**. Where ForgeCLI already supports a setting (check `permissions.allow/deny`, for example), a real editor is better than a stub.
- **4D Diagnostics:** DNS, TCP, TLS, an HTTP ping, then an auth check, shown as a step list with pass/fail and the detail. Model-side probes stay as honest stubs in the registry. Run network work off the UI thread.
- **4E Profiles:** create, switch and delete profiles stored as files. Pass the active profile to ForgeCLI through `--settings` if it layers; otherwise persist the choice and show the limitation through the stub registry.
- **4F**
  - **Map** (the `map` icon): an overview of the project tree, plus files edited in this chat (Write and Edit tool rows).
  - **Split** (the `split` icon): the doc pane beside or below the chat. Extend `doc_pane`; don't add a parallel pane system.
  - Afterwards, remove both icons from `stubs::RESERVED_ICONS`. `assets::tests` must stay green: no dead icons.
- **4G CI and tooling:**
  - add `fmt --check`, `clippy -D warnings`, and `test` jobs, plus a Linux job; on Linux, install `libfontconfig1-dev`, `libxkbcommon-x11-dev` and the X11 libraries;
  - add `scripts/screenshots.sh`: Xvfb `:99`, `FORGE_DEMO=1`, `FORGE_CONFIG_DIR` set to a temp dir, then `xdotool` and `import` to capture welcome, chat, settings (every tab) and projects, in light (Ctrl+Shift+L toggles the theme) and dark;
  - update the README's key table and Layout section.
  - Note: GitHub Actions currently fails at startup at the account level, so don't spend time debugging the runner.

## Rules (from CLAUDE.md, not optional)

- **Code:** original code only, no GPL/Zed copying. New dependencies must be MIT/Apache/BSD/ISC, pinned with `=x.y.z`.
- **Tokens:** every colour, size, space and radius comes from `src/theme.rs`. After adding a token, run `UPDATE_SYSTEM_MD=1 cargo test system_md`.
- **Icons:** Lucide, at the 12/14/16/20 sizes only. Register each one in `src/assets.rs`; take the files from the `lucide-static` 0.469.0 package (ISC).
- **Actions:** every new action goes in `actions!` (`src/ui/mod.rs`), the command palette, and a key binding in `main.rs` where sensible. Keep the shortcuts sheet in sync; a test enforces it.
- **gpui-component input quirk:** the text input binds Up, Down, Tab, Enter, Esc and Paste to its own actions, which run before any key listener. Intercept them with `capture_action::<gpui::component::input::X>`; see `composer.rs`.
- **No `.hover()` twice** on one element (it panics). **No I/O in `render`**: use `cx.spawn` with `background_spawn`.
- **Before every commit:** `cargo build` with zero warnings, `cargo fmt`, and `cargo test` green.

## How to run and verify

- **Build:** `cargo build`. The system needs `libfontconfig1-dev`.
- **Demo:**
  ```bash
  Xvfb :99 -screen 0 1280x860x24 &
  DISPLAY=:99 FORGE_DEMO=1 FORGE_CONFIG_DIR=/tmp/fcfg FORGE_PROJECT_DIR=$PWD target/debug/forge-gpui
  ```
  Drive it with `xdotool` and capture with `import -window root shot.png`.
- **Real engine:** build `/home/user/ForgeCLI` (`cargo build -p forge-cli`) and set `FORGE_CLI=/home/user/ForgeCLI/target/debug/forge`.
  - Without an API key, point an endpoint profile (`<config>/endpoints.json`, wire `anthropic` or `openai`) at a small scripted Python server that streams SSE.
  - Use it to prove that the engine picks up what an editor writes.
  - Don't run `pkill -f <pattern>`: it can match your own shell. Kill by pid.
- **Check every screen** in light and dark, at 1280×860.

## Deliver

1. Small commits, one per item. Commit trailers:
   ```
   Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
   Claude-Session: https://claude.ai/code/session_01Q4uHeBAT1z252c95HzJmcw
   ```
   Push to `claude/laughing-heisenberg-07mbcg`.
2. **`src/stubs.rs` at the end** may hold only engine-blocked entries, each with a reason.
3. **`docs/forge-ui-upgrade_P3-report.md`**, in the brief's per-item format (§7):
   - the files changed, and the wiring through state, actions, keys and the palette;
   - token changes;
   - what you saw on Linux, in light and dark;
   - what is unverified. Windows can't be checked from here: say so plainly, and never claim it.
