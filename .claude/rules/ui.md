---
paths:
  - "src/ui/**"
---

# Views

- Every colour, radius, space and font comes from `cx.theme()`
  (`src/theme.rs`); never a literal colour. A size used in more than one
  place becomes a token in `Sizes`; small one-off nudges may stay as `px()`.
- One filled (primary) action per surface; secondary actions are ghost or text
  buttons. Sheets and cards lift with `t.elevation_grain()`, which follows the
  theme's depth (grain, a soft lift, or a flat 1px edge); other shadows only
  on popovers. A selected row goes through `theme::selected`, so it follows
  the theme.
- Call `.hover()` at most once per element: a second call panics at runtime
  ("hover style already set"). The `theme::*_button` helpers already set it.
- No I/O or blocking work in `render`; use `cx.spawn` or the background
  executor and notify when done.
- Icons are Lucide SVGs registered in `src/assets.rs`; add the file there too,
  or it renders blank.
