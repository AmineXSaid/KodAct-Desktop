# P1 report: foundation and dead-control fixes

Done on `claude/laughing-heisenberg-07mbcg`. Every item below was run in the app on
**Linux (Xvfb), light and dark**; none of it has been seen on Windows yet.
`cargo build` and `cargo test` show 0 warnings; 178 tests pass, plus the 3 cube image tests.

## The P1 items

| # | Item | Files | Wiring |
|---|------|-------|--------|
| 4A | Stub registry and the one "Soon" treatment | `src/stubs.rs` (new), `ui::soon` in `src/ui/mod.rs`, `AppState::stub` | Attach is muted, has a "Soon (next update · needs ForgeCLI support) · …" tooltip, and its click shows the registry's toast. Tests check that ids are unique, every stub is used by the view it names, and reserved icons are bundled. |
| 1 | Jump to latest | `src/ui/chat_view.rs` | `JumpToLatest`, Ctrl/⌘J, palette. The list's scroll handler watches tail-following. The pill fades in when you scroll up, reads "New messages below" while a reply runs, and a click follows the tail again. |
| 2 | Reply actions | `src/ui/message.rs`, `state.rs` (`copy_turn`, `regenerate`, `continue_reply`, `turn_text`, `turn_question`) | Under every finished turn: Copy, Good, Bad, Regenerate, and the duration. The newest turn's row is always shown; older rows appear on hover. Continue appears on a stopped reply: it asks the live ForgeCLI session to go on, and becomes a registered stub when the process is gone. `RegenerateReply` and `ContinueReply` are in the palette. |
| 3 | Chat menu | `src/ui/chat_menu.rs`, `src/ui/confirm.rs`, `src/export.rs` (new); `sidebar.rs` | Opens from ⋯ on hover or a right-click, anchored where it was asked for. Rename edits inline (F2; Enter or blur keeps it, Esc cancels). Also Pin, Duplicate, Export as Markdown (writes to Downloads; the toast names the path), and Delete… (Enter confirms, Esc cancels). `RenameChat`, `DuplicateChat`, `ExportChat` and `DeleteChat` are in the palette. |
| 4 | Find in the conversation | `src/ui/find_bar.rs` (new), `state.rs` (`Find`, `find_hits`, `find_step`) | `ToggleFind`, Ctrl/⌘F, palette. Shows "3 of 12" (or "No matches"), up/down buttons, Enter / Shift+Enter, Esc. Each match scrolls to the top; every hit gets a faint bar and the current one a strong bar. Search covers your text, prose, thinking, tool targets and output, diffs, and notices. |
| 5 | Starters and resume | `src/ui/starters.rs` (new) | Four starter cards under the welcome composer fill it via `Composer::fill`. "Resume …" opens the project's most recent chat. |
| 6 | Feedback | `message.rs`, `state.rs` (`Rating`, `ratings`, `rate`) | 👍/👎 on each turn. Pressing the same one again clears it. Ratings are cleared with any turns that get cut. Uploading a rating is the `feedback-upload` stub. |
| 7 | Shortcuts sheet | `src/ui/shortcuts.rs` (new) | `ShowShortcuts`, F1, palette, `keyboard` icon. A test reads `main.rs` and fails if the sheet and the keymap disagree, so nothing has to be kept in sync by hand. |

**Icons (4C):** `ellipsis`, `keyboard` and `archive` are now wired. `clock`, `eye`, `image`,
`paperclip`, `map` and `split` are reserved in `stubs::RESERVED_ICONS`. `bot`, `command` and
`zap` were never drawn and are removed. A new test fails if an icon is drawn but not bundled,
or bundled but neither drawn nor reserved. The Lucide icons added (`arrow-down`, `thumbs-*`,
`play`, `copy-plus`, `history`, `book-open`, `bug`, `flask-conical`, `wand-sparkles`) come
from `lucide-static` 0.469.0 (ISC).

**Tokens:** `Sizes::composer_clearance` was added so toasts sit above the composer instead of
over its chips; `system.md` was regenerated. `theme::danger_button` was added as a primitive.

**Remaining stubs:** see `src/stubs.rs`, which records where each one is drawn and which step
wires it. The Settings stubs the brief lists (Diagnostics, Probe, Profiles, the twelve
placeholder tabs) were already removed when Settings was rebuilt in `a1a9a82`. Their specs
come back in P3.

## Additions beyond the brief

- **Diagrams render in the chat** (`src/mermaid.rs`, `ui/diagram_view.rs::fence`). Mermaid
  flowcharts, state diagrams and sequence diagrams are laid out natively, and so are ```svg
  and HTML fences. They use theme colours and Forge's font. Each card has Code / Open
  (side panel: zoom, browser, download) / Copy. ForgeCLI is told through
  `--append-system-prompt` to draw in Mermaid rather than ASCII art.
- **Chats persist across launches and resume their ForgeCLI conversation**
  (`src/history.rs`). A chat starts with `--session-id` and later runs use `--resume`. If a
  resume fails, the chat falls back to a new conversation and says so. Events from replaced
  processes are dropped. All of this was verified against the real ForgeCLI.
- **Error notices** wrap in full, which fixes the clipped proxy 400 in the first screenshot,
  and offer Retry and Copy.
- Sidebar ages ("5m", "2h") now update while the app is open.

## Not verified

- Windows: nothing in this report has been run there.
- Real replies from a model: no API key was available. The ForgeCLI path was exercised up to
  the endpoint, which was deliberately unreachable.
- Settings in a very small window, from the second screenshot: no window manager under
  Xvfb, so the window could not be resized.
