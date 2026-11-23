---
paths:
  - "src/theme.rs"
---

# Tokens

`src/theme.rs` is the user's theme and the single source of tokens. Keep its
existing values as they are. New tokens go in the "KodAct additions" section
(`font_mono`, `ExtraColors`, `Sizes`), with both a light and a dark value, and
light/dark switching must keep working (`ui::set_theme`).

There is one look (`theme::Look::Calm`, Calmly smooth): its colours
(`Look::tint`, light and dark) and its form (`Look::form`: corners, row and
title-bar height, chat text). It keeps its rule (pills to press, rounded cards to
hold) and passes `palette_check.rs` and the button tests in both modes.
Every face is Anthropic Sans (`fonts.rs`): never name or bundle another.

After changing a token, regenerate `.interface-design/system.md` with
`UPDATE_SYSTEM_MD=1 cargo test system_md` (the test fails while it is stale),
and keep `src/palette_check.rs` green: surfaces step evenly, text meets its
contrast floor on every surface.
