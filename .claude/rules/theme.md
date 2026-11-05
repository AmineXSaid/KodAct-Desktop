---
paths:
  - "src/theme.rs"
---

# Tokens

`src/theme.rs` is the user's theme and the single source of tokens. Keep its
existing values as they are. New tokens go in the "KodAct additions" section
(`font_mono`, `ExtraColors`, `Sizes`), with both a light and a dark value, and
light/dark switching must keep working (`ui::set_theme`).

There is one look (`theme::Look::KodAct`): KodAct CLI's identity, ink,
paper and lime (the CLI's `tui/palette.rs`), dark first. Its colours
(`Look::tint`, light and dark) and its form (`Look::form`: corners, row and
title-bar height, chat text). It keeps its rule (pills to press, 4px to
hold, tiles for the mark and Send, each the opposite of its ground) and passes `palette_check.rs` and the
button tests in both modes.

Lime is a fill under ink (`accent`, `fill`: the primary button; the
mark's tile and Send on ink) or a line and a word (`live`, `primary`, `toggle_on`: what is
live, links, a switch that is on), olive on paper. An indicator takes
`live`, never `accent`; a running spinner and a working chat's sidebar
shimmer take the muted and strong inks, so lime in motion is the working
line's alone. The mark (`mark_top`, `mark_left`) takes the
opposite of its ground, lime on ink and ink on paper, with no shadow, and
its lime only touches ink. The mark and the wordmark come only from
`ui/mark.rs`; the faces only from `fonts.rs` (KodAct Sans and KodAct
Mono, cut from Host Grotesk and JetBrains Mono, OFL). Never bring back Anthropic Sans or Claude's cream and
terracotta.

After changing a token, regenerate `.interface-design/system.md` with
`UPDATE_SYSTEM_MD=1 cargo test system_md` (the test fails while it is stale),
and keep `src/palette_check.rs` green: surfaces step evenly, text meets its
contrast floor on every surface.
