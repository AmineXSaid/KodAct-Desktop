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

| Keys | Does |
| --- | --- |
| Ctrl/⌘ K | Command palette |
| Ctrl/⌘ N | New chat |
| Ctrl/⌘ B | Show or hide the sidebar |
| Ctrl/⌘ / | Model picker |
| Ctrl/⌘ Shift L | Light or dark |
| Ctrl/⌘ Shift C | Copy the last reply |
| Shift Tab | Cycle the permission mode |
| Enter / Shift Enter | Send / new line |
| Esc | Stop the reply |

## Layout

- `src/theme.rs`: every colour, size, radius and font. Nothing else hardcodes them.
- `src/state.rs`: the app state, the transcript model, demo chats and the scripted reply.
- `src/backend.rs`: each chat's `forge` process: launch, the stream-json events, permission answers.
- `src/profile.rs`: your name on disk.
- `src/assets.rs`: bundled icons (Lucide, ISC) and fonts (Hanken Grotesk, JetBrains Mono, Source Serif 4; OFL).
- `src/ui/`: one file per part: `workspace` (title bar, welcome), `sidebar`,
  `chat_view`, `message`, `markdown`, `composer`, `command_palette`,
  `onboarding`, `hammer` (the Forge mark).
