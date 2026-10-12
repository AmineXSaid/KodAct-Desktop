# forge-gpui — working rules

The Forge desktop chat UI in Rust, on GPUI and gpui-component (through
`gpui-kit`). Each chat's engine is a ForgeCLI process (`src/backend.rs`,
`forge -p --input-format stream-json`); the starting chats are demo data in
`src/state.rs`, and features with no engine behind them yet show a toast.

These rules apply everywhere. Rules for one part of the tree live in
`.claude/rules/` and load when Claude works on those files: `ui.md` (the
views), `theme.md` (the tokens).

1. **Write everything original.** You may read Zed's source to learn GPUI, but
   never copy code from Zed's GPL crates. The app is closed source; every
   dependency must be Apache-2.0, MIT, BSD, ISC or similar (fonts OFL).
2. **Pin versions.** Dependencies use exact versions (`=x.y.z`) where an API
   could shift under us; GPUI's API changes between snapshots.
3. **One file per component** under `src/ui/`; state in `src/state.rs`.
   Shortcuts are GPUI actions with key bindings in `main.rs`, and every action
   is also reachable from the command palette.
4. **No warnings.** `cargo build` with zero warnings and `cargo test` green
   before every commit.
5. **Run it and look at it.** For any visible change, run the app and check a
   screenshot, in light and dark. Windows is the release target: say what was
   seen on Windows and what only on Linux.

## Design workflow

- Tokens live in `src/theme.rs`. `.interface-design/system.md` is generated
  from it (`src/system_md.rs`) and is what the design skills read. Never
  invent spacing, sizes, radii or colours outside it: add a token to
  theme.rs, then run `UPDATE_SYSTEM_MD=1 cargo test system_md`.
- New palette: run `/ui-design:color-palette` (with contrast checks and dark
  mode), put the result in theme.rs, and keep `src/palette_check.rs` green.
- Use ui-ux-pro-max only as a reference for icons, UX rules and
  anti-patterns. Its suggested palettes do not override system.md.
- Icons: sizes come only from the token scale (12, 14, 16, 20), and the
  stroke stays Lucide's 2px.
- Before finishing any UI change, run `/visual-critique:critique-screen` on a
  screenshot and fix the high-priority items.
