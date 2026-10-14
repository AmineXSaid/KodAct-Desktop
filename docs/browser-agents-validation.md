# Browser, agents and Windows icon: what was validated

The side panel's browser (`src/web_view.rs`, `src/ui/browser_view.rs`), Forge's
browser tools for ForgeCLI's agents (`src/browser_mcp.rs`), the Agents panel
(`src/ui/agents_view.rs`) and the Windows .exe icon (`build.rs`,
`assets/forge.rc`). Checked on Linux unless a line says otherwise.

## Automated

- `cargo clippy --all-targets -D warnings` clean; `cargo test`: 237 tests pass.
- `cargo test real_ -- --include-ignored` (needs a Chromium):
  - `web_view::drives_a_real_browser`: the browser starts, opens a page,
    reports it loaded, and sends frames.
  - `browser_mcp::drives_a_real_page`: every tool against a real page with a
    form: navigate, state, type, select, click, text, eval, wait, console,
    screenshot, new_tab, tabs, the focus file, close_tab, press, scroll, and
    the "call state again" error for a stale element number.

## With the real ForgeCLI and a scripted model

`scripts/e2e/real_forgecli.sh` runs the real app, the real ForgeCLI
(`cargo build -p forge-cli`) and `scripts/e2e/mock_model.py`, a scripted model
on the Anthropic Messages API (streaming). What it showed:

- ForgeCLI started Forge's tool server from the app's `--mcp-config` and
  offered the main agent 44 tools (the 21 browser tools among them) and the
  sub-agent 35 (the browser tools among them).
- A `general-purpose` sub-agent navigated, read the page's elements, typed,
  clicked, read "Hello, Forge!" and took a screenshot; the main agent listed
  tabs, opened a second tab, switched back, and its JavaScript returned
  "Hello, Forge!".
- The Agents tab filled from ForgeCLI's real nested messages (one agent,
  six steps, done); the Browser tab showed both tabs and the result.
- With no allow rule (`ALLOW=0`), each step asked ("Allow Forge to open a page
  in the browser?", with the address) and went on when allowed. With
  `permissions.allow: ["mcp__forge-browser"]` nothing asked.
- After the app was killed, no browser and no Forge process was left (the
  guard, `--browser-guard`).

## Windows

- Cross-compiled with MinGW (`x86_64-pc-windows-gnu`, release): no warnings.
- The .exe is a GUI-subsystem program with icon group 1 (the one the window,
  the taskbar and Explorer use): the Forge cube at 16, 24, 32, 48, 64, 128
  and 256 px. Before, the .exe had no icon.
- Under Wine, the Windows .exe served the browser tools (`--browser-mcp`):
  all 21 listed, and it opened a page, typed, clicked, read "Hello, Windows!"
  and returned a screenshot from a real Chromium.
- Not seen: the app's window under Wine, which has no system UI font for
  GPUI (a Wine limit; every Windows has Segoe UI), and a build on GitHub's
  Windows runner: every run of the workflow, on every branch including main,
  ends in `startup_failure` before any step (an account or Actions setting).
- The .exe imports `icuuc.dll` (`u_strlen`), part of Windows 10 1703 and
  later.

## Installer (`installer/`)

- `installer/build.sh` built `Forge-Setup-0.1.0.exe` (24.5 MB, 64-bit NSIS)
  from MinGW release builds of the app and ForgeCLI, with no warnings. Neither
  .exe needs a MinGW DLL.
- Under Wine, a silent install put `Forge.exe` and `bin\forge.exe` (byte for
  byte the builds) in `C:\Program Files\Forge`, with the Start menu and
  desktop shortcuts and the Apps & features entry; `forge --version` ran from
  PATH. Uninstall removed all of it. The PATH entry was appended once (a second
  install added nothing), removed on uninstall, and left alone when it was
  already there before the install.
- The app, forge.exe, the installer and the uninstaller all carry the Forge
  cube (16 to 256 px). The wizard was seen under Wine; not yet run on a real
  Windows.

## The Windows app under Wine

- Wine 9 lacks `IDWriteFontCollection1::GetFontSet`, so GPUI found no font and
  panicked. The vendored `gpui-pre-windows` now falls back to the older
  DirectWrite lookup, and opens the window without Direct Manipulation where
  it can't start (`vendor/gpui-pre-windows/FORGE-CHANGES.md`).
- With `GPUI_DISABLE_DIRECT_COMPOSITION=1` and Microsoft's `d3dcompiler_47.dll`
  (Wine's compiler has no `StructuredBuffer`), the installed `Forge.exe` ran
  under Wine: it found `bin\forge.exe` by itself and, with the scripted model,
  went through the plan scenario: Plan chip, the plan card, "Approved · Edit
  automatically", the chip on Edit automatically, notes.txt written without an
  ask, and an ask for `rm notes.txt`.
- Not seen: the title bar. Under Wine without a window manager, the top 30 px
  (the caption area GPUI draws in through `WM_NCCALCSIZE`) is not shown. It
  needs a look on a real Windows.

## Remote development (`src/remote.rs`, `ui/remote_menu.rs`)

| Check | How | Result |
|---|---|---|
| Transports, quoting, boot script, discovery parsers, tar, errors | `cargo test remote::` (14 tests; the boot script and tar run under a real `sh`/`tar`) | pass |
| SSH: unknown host key → fingerprint → trust; install ForgeCLI once; modes ask as locally; key never in argv; process ends with its stdin | `cargo test real_remotes -- --include-ignored` against an OpenSSH server in a container | pass |
| Running container (`docker exec`) | same test, `FORGE_TEST_CONTAINER` | pass |
| Dev container from an image and from a Dockerfile, `postCreateCommand`, folder mounted, reopen reuses it | `cargo test real_devcontainer -- --include-ignored` | pass |
| Browser tools reach a remote agent over `ssh -R` (HTTP MCP) | in `real_remotes` | 21 tools offered |
| The app: Ctrl+Alt+O menu → Connect to Host → trust → folder → chat (Manual asks; commands run on the host); a dropped connection resumes on the next message (`--resume`, same id); two chats on one host at once; a written .md's card read back from the host; attach to a container (Edit automatically: only `rm` asks); Reopen in Container; Close remote connection; light and dark | driven with xdotool, screenshots | pass |
| WSL | unit tests only (command line, `wsl -l -v` in UTF-16) | not run: no Windows here |
