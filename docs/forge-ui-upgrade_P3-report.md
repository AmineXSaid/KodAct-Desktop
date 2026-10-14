# P3 report: settings screens, engine surfaces, process hardening

Done on `ccr-02da188f-b3znjk`.

The handoff named `claude/laughing-heisenberg-07mbcg`. This session was
assigned `ccr-02da188f-b3znjk`, so the work is there, in one commit per
item, starting after `a1ec1e1`.

Every item was run in the app on **Linux (Xvfb, 1280×860, Mesa lavapipe),
light and dark**. **Nothing was seen on Windows**: it can't be run from
here, so this report claims nothing about it.

The gates, on the final commit:
- `cargo build`: 0 warnings.
- `cargo clippy --all-targets -D warnings`: clean.
- `cargo fmt --check`: clean.
- `cargo test`: 222 passed (1 ignored), plus the 3 cube image tests.

## How it was decided

ForgeCLI's source (`AmineXSaid/ForgeCLI`, branch
`claude/laughing-heisenberg-07mbcg`) is the truth for every file an editor
writes. The VS Code extension (`AmineXSaid/FORGE`,
`claude/dazzling-cori-sdq7bx`) is the UX reference.

Two findings changed the plan:

1. **ForgeCLI supports more than the brief assumed.**
   - `sandbox.mode`/`.network`, the `env` key, `pluginDirs` and FORGE.md
     are real settings, so Sandbox, Environments, Plugins and Memory and
     rules are real editors, not planned stubs. The handoff says a real
     editor beats a stub where the engine supports it.
   - Only Network has nothing behind it: no proxy or CA settings. It is a
     Planned screen.
2. **`--settings <file>` layers in ForgeCLI.** It is a "flag" layer above
   user, project and local settings. So Profiles are real: the profile in
   use goes to each chat's ForgeCLI as `--settings`, and no limitation
   toast is needed.

**Proof against the real engine.** ForgeCLI was built (`cargo build -p
forge-cli`) and run in the test project. A scripted Messages-API mock
model on localhost stood in for a real model. ForgeCLI ran for real.

## Shared plumbing

- `src/forge_files.rs` (new) mirrors ForgeCLI's loaders:
  - the config folder (`FORGE_HOME`, `XDG_CONFIG_HOME/forge`, then the
    platform folder);
  - `commands/**/*.md` (a file in a subfolder is `dir:name`),
    `agents/*.md`, and `skills/<n>/SKILL.md`;
  - the three settings files (user `settings.json`,
    `.forge/settings.json`, `.forge/settings.local.json`), and `.mcp.json`;
  - frontmatter in ForgeCLI's YAML subset. An edit keeps the keys it
    doesn't touch, and values ForgeCLI would misread are quoted.
- **Safe writes.** Writes are atomic (a temporary file, then rename). A
  settings file that isn't a JSON object is never overwritten: saving
  names the problem instead.
- `AppState::engine` holds the last read (`Catalog`).
  - `reload_engine` reads off the UI thread.
  - `watch_engine` polls a fingerprint every 2 s while an editor is open,
    so an edit in another program shows up.
  - `engine_write` runs a change in the background, toasts the result,
    and reads again.
- `src/ui/editor_kit.rs` (new) holds the pieces every editor shares:
  header, cards, inline delete confirm, fields, the empty, loading and
  error states, and the layer chips.
- The `/` list now reads commands the way ForgeCLI does: yours,
  `.agents/`, and subfolders, besides `.forge/commands`.
- **Settings window.**
  - The nav is grouped (Forge, Engine, Workspace, Help) and scrolls.
  - **Focus fix (found while testing):** Settings never took keyboard
    focus. Text typed while it was open went to the hidden composer, and
    Enter sent it as a chat. Settings now takes focus when it opens and
    returns it to the composer when it closes.
  - Esc closes a form, then a diagnosis, then Settings. It also catches
    the workspace's `StopReply`, which Esc maps to.
  - **Corner fix (you spotted it):** the nav pane's background showed
    past the panel's rounded corners, because GPUI clips children to a
    rectangle. The pane now rounds its own corners (`PANEL_RADIUS` minus
    the hairline).

## The P3 items

| # | Item | Files | Wiring and evidence |
|---|---|---|---|
| 4A | Slash commands | `src/ui/slash_commands.rs` | **Does:** list (yours / project); create, edit, delete with confirm; name check (`dir:name`); prompt required; a shadowed command is flagged; open either folder.<br>**Wiring:** `OpenSlashCommands`, palette "Open Slash commands…".<br>**Real CLI:** a command made in the UI was in the `init` line's `slash_commands`. |
| 4A | Agents | `src/ui/agents_editor.rs` | **Does:** name, "when to use it" (required, the model reads it), model, and a tool picker (every tool, or chosen ones from ForgeCLI's tools); a file ForgeCLI would skip is named with the reason.<br>**Wiring:** `OpenAgents`.<br>**Real CLI:** `reviewer` (tools `[Read, Grep]`, model sonnet) was in `init.agents`. |
| 4A | Skills | `src/ui/skills_editor.rs`, `plugin.rs` (`skills()`) | **Does:** yours and the project's, with On/Off; Forge's plugin skills shown read-only; delete names the extra files that go with the folder.<br>**Off:** ForgeCLI has no setting for it, so the folder moves to `skills/.disabled/`, which its loader never opens.<br>**Wiring:** `OpenSkills`.<br>**Real CLI:** `pdf-reader` was in `init.skills`; after Off it was gone. |
| 4A | Hooks | `src/ui/hooks_editor.rs` | **Does:** ForgeCLI's 10 events, each with what it does; a tool matcher only on the tool events (checked); command; timeout 1–3600 s; saved in project, project-local or yours; written as ForgeCLI's `{"Event": [{"matcher", "hooks": [{type, command, timeout}]}]}`.<br>**Wiring:** `OpenHooks`.<br>**Real CLI:** a `UserPromptSubmit` hook's output reached the model's request. |
| 4A | MCP servers | `src/ui/mcp_editor.rs` | **Does:** stdio (command, args with quotes, env) or HTTP/SSE (URL, headers); `${VAR}` for keys; saved where `forge mcp add` saves (user, `.mcp.json` trusted for the person adding it, local); Trust button for an untrusted `.mcp.json` server.<br>**Status row:** "Check each server" runs `forge mcp list`, which starts each server the way a chat would.<br>**Wiring:** `OpenMcpServers`.<br>**Real CLI:** "connected, 15 tools" (ForgeCLI's own `forge mcp serve` as the server); `init.mcp_servers` said connected. |
| 4C | Permissions (real) | `src/ui/permissions_editor.rs` | **Does:** allow / ask / deny rules per settings file, checked as `Tool` or `Tool(spec)`; defaultMode shown.<br>**Wiring:** `OpenPermissions`.<br>**Real CLI:** `forge config get permissions` returned the rules. |
| 4B | Guide | `src/ui/guide.rs` | Static and scrollable, in the app's voice. Its key table is `shortcuts::SHORTCUTS`, which a test keeps equal to `main.rs`; another test checks the Guide uses the sheet rather than a copy.<br>**Wiring:** `OpenGuide`. |
| 4C | Sandbox (real) | `src/ui/sandbox_editor.rs` | **Does:** `sandbox.mode` (not set / off / read-only / workspace-write) and `sandbox.network`, per file; shows which file wins.<br>**Windows:** it says plainly that ForgeCLI has no sandbox there; with a mode set, ForgeCLI asks before each command instead. |
| 4C | Environments (real) | `src/ui/env_editor.rs` | `env` as KEY=value lines (names checked), per file; Save only when changed.<br>**Real CLI:** `forge config get env` returned the variable. |
| 4C | Memory and rules (real) | `src/ui/memory_view.rs` | The FORGE.md / AGENTS.md / FORGE.local.md files ForgeCLI reads, in its order, with a size estimate; Open, or Make it for the project's missing files; a link to Rules and memory. |
| 4C | Plugins (real) | `src/ui/plugins_editor.rs` | `pluginDirs` per file: add a folder (checked), remove it from the list (the folder stays); the manifest's name and description; Forge's built-in plugin. The marketplace is named as planned (ForgeCLI has none). |
| 4C | Network (planned) | `src/ui/planned.rs` | No controls. What it will do (proxy, CA certificates, site lists, reachability), what it waits on (ForgeCLI proxy and CA settings), and what works today (WebFetch rules in Permissions; Diagnose). |
| 4D | Diagnostics | `src/diagnose.rs`, `src/ui/diagnostics.rs`, `state.rs` (`diagnose`, `Diagnosis`) | **Steps:** a stethoscope button on each endpoint runs five steps off the UI thread, each shown as it ends with pass or fail, its detail and its time:<br>1. DNS<br>2. TCP<br>3. TLS: a real rustls handshake with ureq's roots; skipped on http, with a note<br>4. HTTP ping of the base URL<br>5. the key, via the model list<br>A failure skips the rest and says what to fix. Model-side probes are the `capability-probe` stub.<br>**Seen:** the mock passed all five; with the mock down, TCP showed connection refused; api.anthropic.com failed at TLS with UnknownIssuer, because this container's proxy inspects TLS. |
| 4E | Profiles | `src/setting_profiles.rs`, `src/ui/profiles_editor.rs`, `state.rs` (`use_settings_profile`, `restart_idle_engines`) | **Does:** create (an empty `{}`), use, open its file, delete with confirm; "No profile" option; the list refreshes on change. Switching restarts idle chats' ForgeCLI on their next message (the browser switch's mechanism, now shared).<br>**Wiring:** `OpenProfiles`.<br>**Real CLI:** a chat's ForgeCLI was launched with `--settings …/profiles/client-work.json`, and a hook set only in the profile ran. |
| 4F | Project map | `src/ui/project_map.rs`, `state.rs` (`ProjectMap`, `Edited`, `toggle_map`, `edited_files`) | **Shows:** top-level folders by file count (git-aware walker from `rules.rs`, off the UI thread, capped at 20,000), the top-level files, and the files this chat changed (Write and Edit rows) with their Keep/Revert state. A click opens a changed file.<br>**Wiring:** `ShowProjectMap`, Ctrl/⌘ Shift M, palette, and Map on a project's page. |
| 4F | Split | `src/ui/doc_pane.rs` (`Fit::below`, `finish_at`), `workspace.rs`, `state.rs` (`doc_below`) | The side panel goes beside the chat or below it (full width, 45% of the height). Only doc_pane's layout is extended; there is no second pane system.<br>**Wiring:** `ToggleSplit`, Ctrl/⌘ \\, palette, and the panel header's split button. |
| 4G | CI | `.github/workflows/windows.yml` | **Linux `checks` job**, on every push and pull request: fmt --check, clippy `-D warnings`, then build and tests with `RUSTFLAGS=-D warnings`. It installs fontconfig, xkbcommon (with X11), and the X11, xcb and Wayland libraries.<br>**Windows job:** now also runs clippy.<br>**Clippy:** 23 older findings were fixed so the gate starts green.<br>**Not run on GitHub:** Actions fails at startup at the account level (per the handoff). The steps were run locally on Linux. |
| 4G | Screenshot harness | `scripts/screenshots.sh` | Runs the app on Xvfb `:99` with `FORGE_DEMO=1` and a throwaway `FORGE_CONFIG_DIR`/`FORGE_HOME`; xdotool and `import`. Captures 42 PNGs: welcome, chat, map, split, projects, and all 16 Settings sections, light and dark. Uses lavapipe without a GPU. Documented in the README. |
| 4G | README | `README.md` | The key table lists every binding from P1 to P3 in the sheet's words; a new test fails when one is missing. The Layout section covers every new module and component; there is a Screenshots section. |
| 4G | Icon coverage | `src/assets.rs`, `stubs.rs` | `map` and `split` are drawn, so `RESERVED_ICONS` is empty. `assets::tests` checks every bundled icon is drawn and every drawn icon is bundled.<br>**New icons** (lucide-static 0.469.0, ISC): square-slash, bot, webhook, plug, layers, box, puzzle, server, circle-dashed, stethoscope. |

**Actions, keys and palette.**
- New actions:
  - `OpenSlashCommands`, `OpenAgents`, `OpenSkills`, `OpenHooks`,
    `OpenMcpServers`, `OpenPermissions`, `OpenProfiles`, `OpenGuide`
    (palette only);
  - `ShowProjectMap` (Ctrl/⌘ Shift M);
  - `ToggleSplit` (Ctrl/⌘ \\).
- All are in `actions!`, the palette, and the workspace's handlers. Both
  new keys are on the shortcuts sheet, in the Guide and in the README;
  tests keep the three equal to `main.rs`.

**Dependencies.** `rustls =0.23.45`, `webpki-roots =0.26.11` and `url
=2.5.8` became direct dependencies, for Diagnose's TLS step. They are the
exact versions ureq already built, so no new code enters the build
(MIT/Apache).

**Tokens.**
- Two `Sizes` were added in `theme.rs`, with the same value in light and
  dark:
  - `settings_nav` (200), the Settings list;
  - `prompt_box` (180), a prompt or instructions box.
- No colours changed.
- `UPDATE_SYSTEM_MD=1 cargo test system_md` was run. `system.md` lists
  the scales, not every component size, so it didn't change.

## Seen, and not seen

- **Seen on Linux, light and dark:** every Settings section, Diagnose,
  the map, split beside and below, welcome, chat and projects. These are
  the 42 screenshots from `scripts/screenshots.sh`; the dark pass was
  checked screen by screen on a contact sheet.
- **Visual critique** (`/visual-critique:critique-screen` on MCP servers
  and Permissions):
  - No P1 issues.
  - One P2 fixed: "Check each server" showed with no servers, so it could
    do nothing; it now shows only when a server is listed.
  - P3 left as is: the section titles are 13 px, like the nav. That is
    the existing Settings style, and changing it is a separate,
    Settings-wide decision.
- **Not verified:**
  - **Anything on Windows**: layout, focus, `%APPDATA%\forge` as
    ForgeCLI's folder, Explorer opening folders, and CI on GitHub.
  - **The sandbox actually confining commands:** no bubblewrap here.
    Only that the settings are written as ForgeCLI reads them.
  - **A real (non-mock) model.**
  - **Network on Windows or macOS.**

## Found along the way

- **Tracked Windows download artifacts.** `forge-ui-upgrade_P{1,2,3}.md:Zone.Identifier`
  are tracked in the repo root (the map shows them). They are Windows
  download artifacts. I left them; they can be deleted if you agree.
- **`forge config` ignores `--settings`,** so it can't show a profile's
  values. That is a small ForgeCLI gap; a chat session does apply the
  profile.

## The stub registry, at the end

Every row now names what it waits on (`waits_on`); a test requires it.
`Step` is `Blocked` or `Demo`; nothing is left for a later step.

| Stub | Waits on |
|---|---|
| `feedback-upload` | a feedback service for Forge |
| `continue-no-session` | ForgeCLI keeping a conversation it never wrote |
| `revert-no-checkpoint` | a checkpoint this old message never had (chats from before P2) |
| `reapply` | ForgeCLI rolling checkpoints forward |
| `share-link` | a hosted service for share links |
| `capability-probe` | ForgeCLI reporting what a model supports (streaming, tools, images, thinking) |
| `revert-demo`, `compact-demo` | demo mode only: no ForgeCLI behind it |
