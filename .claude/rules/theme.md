---
paths:
  - "src/theme.rs"
---

# Tokens

`src/theme.rs` is the user's theme and the single source of tokens. Keep its
existing values as they are. New tokens go in the "Forge additions" section
(`font_mono`, `ExtraColors`, `Sizes`), with both a light and a dark value, and
light/dark switching must keep working (`ui::set_dark`).
