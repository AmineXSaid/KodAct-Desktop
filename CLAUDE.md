# forge-gpui — working rules

The Forge desktop chat UI in Rust, on GPUI and gpui-component (through
`gpui-kit`). It is a UI shell: chats, replies and models are mock data in
`src/state.rs`; anything that needs a backend shows a toast saying so.

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
