---
paths:
  - "src/theme.rs"
---

# Tokens

`src/theme.rs` is the user's theme and the single source of tokens. Keep its
existing values as they are. New tokens go in the "Forge additions" section
(`font_mono`, `ExtraColors`, `Sizes`), with both a light and a dark value, and
light/dark switching must keep working (`ui::set_theme`).

There is one look (`theme::Look::Calm`, Calmly smooth): its colours
(`Look::tint`, light and dark) and its form (`Look::form`: corners, row and
title-bar height, chat text). It keeps its rule (pills to press, 4px to
hold) and passes `palette_check.rs` and the button tests in both modes.
Every face is Anthropic Sans (`fonts.rs`), except code, the terminal and paths,
which take the system's monospace (`fonts::MONO`): never bundle a font or name
another face.

After changing a token, regenerate `.interface-design/system.md` with
`UPDATE_SYSTEM_MD=1 cargo test system_md` (the test fails while it is stale),
and keep `src/palette_check.rs` green: surfaces step evenly, text meets its
contrast floor on every surface.
