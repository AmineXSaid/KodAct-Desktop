---
name: gpui-ui
description: Use to build or change a view in src/ui (a panel, menu, card, dialog) the way this app builds them: GPUI elements, theme tokens, one component per file, an action in the command palette
---
You build user interface for Forge GPUI with GPUI and gpui-component (through
`gpui-kit`).

Read first: `.claude/rules/ui.md`, `.claude/rules/theme.md`, and one or two
neighbouring files in `src/ui/` that do something similar; match their
structure and comment density.

Rules that are easy to break:
- Every colour, radius, space and font comes from `cx.theme()` (`src/theme.rs`).
  A size used in more than one place becomes a token; never a literal colour.
  Adding a token means running `UPDATE_SYSTEM_MD=1 cargo test system_md`.
- One filled (primary) action per surface; others are ghost or text buttons.
- Call `.hover()` at most once per element (a second call panics); the
  `theme::*_button` helpers already set it.
- No I/O or blocking work in `render`: use `cx.spawn` or the background
  executor, then `cx.notify()`.
- Icons are Lucide SVGs registered in `src/assets.rs`, sized 12, 14, 16 or 20.
- A new action goes in `src/ui/mod.rs`'s `actions!`, is handled in
  `workspace.rs`, appears in `command_palette.rs`, and its shortcut (if any) is
  bound in `main.rs` and listed in `shortcuts.rs`.
- Copy for people is plain and short; Forge is the subject ("Forge will…").

Finish with `cargo build` (zero warnings) and say which screens changed so
they can be checked in light and dark.
