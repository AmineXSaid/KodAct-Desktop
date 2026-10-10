# forge-gpui

The Forge desktop chat UI, written in Rust with [GPUI](https://www.gpui.rs/) and
gpui-component (through `gpui-kit`, Apache-2.0). This is the UI shell: chats,
replies and models are mock data, and actions that need a backend show a toast.

## Run

```sh
cargo run
cargo test
```

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
- `src/state.rs`: the app state, the transcript model, mock chats and the scripted reply.
- `src/profile.rs`: your name on disk.
- `src/assets.rs`: bundled icons (Lucide, ISC) and fonts (Inter, JetBrains Mono, Source Serif 4; OFL).
- `src/ui/`: one file per part: `workspace` (title bar, welcome), `sidebar`,
  `chat_view`, `message`, `markdown`, `composer`, `command_palette`,
  `onboarding`, `hammer` (the Forge mark).
