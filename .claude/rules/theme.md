---
paths:
  - "src/theme.rs"
---

# Tokens

`src/theme.rs` is the user's theme and the single source of tokens. Keep its
existing values as they are. New tokens go in the "Forge additions" section
(`font_mono`, `ExtraColors`, `Sizes`), with both a light and a dark value, and
light/dark switching must keep working (`ui::set_theme`).

A theme (`theme::Look`) is a whole look: its colours (`Look::tint`, light and
dark) and its form (`Look::form`: faces, corners, row and title-bar height,
chat text, depth, selected-row style). Calmly smooth's form is the system's
own and keeps its rule (pills to press, 4px to hold); the other themes set
their own. A new theme needs both, a command-palette action, and must pass
`palette_check.rs` and the button tests, which run every theme in both modes.

After changing a token, regenerate `.interface-design/system.md` with
`UPDATE_SYSTEM_MD=1 cargo test system_md` (the test fails while it is stale),
and keep `src/palette_check.rs` green: surfaces step evenly, text meets its
contrast floor on every surface.
