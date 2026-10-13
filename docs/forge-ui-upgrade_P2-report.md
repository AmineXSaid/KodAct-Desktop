# P2 report: attachments, autocomplete, context, review, history, oversight

Done on `claude/laughing-heisenberg-07mbcg`. Every item below was run in the app on
**Linux (Xvfb), light and dark**; none of it has been seen on Windows yet.
`cargo build` and `cargo test` show 0 warnings; 190 tests pass (1 ignored), plus the 3 cube image tests.

Items marked **real CLI** were driven end to end against the real ForgeCLI binary
(`ForgeCLI/target/debug/forge`). For model-backed flows, the model was a scripted
Messages-API mock on localhost, so ForgeCLI's tools, permissions, checkpoints and
compaction ran for real. The other items were checked in demo mode (`FORGE_DEMO=1`).

## Engine plumbing (shared by the items)

- `src/backend.rs`
  - Launches ForgeCLI with `--replay-user-messages`.
  - ForgeCLI echoes each accepted prompt with its `uuid`, which arrives as `Event::UserAccepted`.
  - `state.rs` stores that uuid on the user message as its `turn`, and Revert uses it as the checkpoint id.
  - `Session::send_content` sends content blocks: text plus images.
  - `Session::request` sends a `control_request` and returns its id. The reply comes back as `Event::ControlReply`.
- Requests in flight are kept in `AppState::requests` and matched to their replies in `on_reply`.
- An immediate command (`/status`, `/context`, …) answers in the chat as a text block, and the chat does not show as busy for it.
- `/btw` answers still go to the side card. Which is which is kept per chat by `immediate_queue`.
- `history.rs` saves:
  - attachments, as name/path/kind/size; image thumbnails are read again from the path;
  - turn ids;
  - each tool row's review state (Kept / Reverted);
  - earlier versions.

  Chats saved before P2 still load.

## The P2 items

| # | Item | Files | Wiring |
|---|------|-------|--------|
| 4A | Attachments | `src/attach.rs` (new), `src/ui/attachments.rs` (new), `composer.rs`, `message.rs`, `state.rs` (`attachments`, `add_attachment_paths`, `add_pasted_image`, `remove_attachment`) | **Ways in:** the paperclip button, `AttachFiles` (Ctrl/⌘U, palette), drag and drop onto the composer (it highlights while you drag), and paste. Paste takes an image, copied files, or a copied path or `file://` URI.<br>**Chips:** they sit in the box, each with a thumbnail or kind icon, name, size and ×. Sent attachments show as chips on your message. A message can be attachments only.<br>**What goes to ForgeCLI:** images as base64 image blocks; text files inline as `<attached_file path=…>`; other files by path, which ForgeCLI's own tools can read.<br>**Limits:** 5 MB per image, 256 KB per inline text file, 20 files. Anything refused gets a toast that names it; nothing is dropped silently.<br>**Real CLI:** an image was accepted and its turn id stored. |
| 4B | Slash and @ autocomplete | `src/commands.rs` (new), `src/ui/autocomplete.rs` (new), `composer.rs`, `rules.rs` (`project_files`) | **Lists:**<br>• `/` lists ForgeCLI's commands that work in a print-mode session, the project's `.forge/commands/*.md`, and the app's own actions (Attach, Context window, Find, Export…).<br>• `@` lists the project's files (indexed off the UI thread, capped) and the modes.<br>**Keys:** ↑/↓ move, Tab/Enter complete, Esc closes and keeps focus. Enter that completes never also sends. The caret lands after the completion.<br>**Matching:** fuzzy and ranked; ranked rows carry a group tag.<br>**Placement:** on the welcome screen the list opens below the box, so it is never clipped.<br>**Real CLI:** `/status` answered in the chat. |
| 4C | Context popover and Compact | `src/ui/context_menu.rs` (new), `src/context.rs` (`breakdown`, `turns`), `composer.rs`, `state.rs` (`compact`, `toggle_context`) | **Opening it:** click the composer's meter, or `ShowContext` (palette, or `context-window` in `/`).<br>**Contents:** % used of the model's window, a bar, "What fills it" (your messages, replies, tool calls and output, thinking, files), and "How it grew" turn by turn.<br>**Compact:** sends ForgeCLI's `/compact` and shows a "Compacting…" notice. The notice becomes "Compacted…" only if the result succeeded, or shows the error. After compaction the meter starts again from the next reply. Demo mode shows the `compact-demo` stub.<br>**Real CLI:** 5.0K of 200K shown from real usage; Compact produced the Compacted notice. |
| 4D | Keep / Revert on diffs | `message.rs` (`review_controls`), `confirm.rs`, `state.rs` (`keep_change`, `ask_revert`, `revert`, `control`, `on_reply`, `Review`) | **Where it shows:** every finished edit/write row of a turn, as Keep / Revert…<br>**Revert:** first sends ForgeCLI's `rewind_files` as a dry run. A confirm window lists the files it would restore, as project-relative paths. Revert then rewinds them on disk. If the chat's process is gone, a resumed session is started for the request.<br>**After:** the row reads Kept, or Reverted.<br>**Real CLI:** `hello.txt`, written by a real Write, was listed by the dry run and deleted by Revert. |
| 4E | Terminal Copy output and Run again | `src/ui/terminal.rs`, `src/export.rs` (`plain`, now pub), `state.rs` (`rerun`) | **Copy output:** copies the output with ANSI codes stripped.<br>**Run again:** asks the agent to run the same command and say what changed. It goes through the agent, so permissions still apply; nothing runs from the UI directly. Disabled while a reply runs. |
| 4F | Conversation versions | `src/ui/versions.rs` (new), `state.rs` (`Version`, `take_from`, `toggle_versions`, `restore_version`), `history.rs` | Editing or retrying a message keeps the conversation it replaced, up to 20 per chat. The replaced version keeps its ForgeCLI session id.<br>**Under your message:** a clock reads "N earlier versions". Its list is read-only and shows when, what you asked, and the start of the reply.<br>**Restore:** swaps that version back in, along with its session, so the next message resumes the right conversation. The version that was showing is kept as a version. |
| 4G | Export JSON and Share | `state.rs` (`chat_json`, `export_chat_json`, `share_chat`), `chat_menu.rs`, `command_palette.rs` | **Export as JSON:** `ExportChatJson`, in the chat menu and the palette. Writes the saved-chat form to Downloads, including versions.<br>**Share:** `ShareChat` copies the JSON to the clipboard. A share link needs a service, so it is the `share-link` stub. |
| 4H | Task panel | `src/ui/tasks.rs` (new), `workspace.rs`, `chat_view.rs`, `state.rs` (`toggle_tasks`, `jump_to`, `jump`) | **Pill:** over the transcript's top-right corner, e.g. "1 running · 2 tasks" or "2 tasks · 1 failed". The first message now starts below it.<br>**List:** `ToggleTasks` (Ctrl/⌘⇧T, palette). It lists the newest turn's tool calls and sub-agents with their status. A row scrolls the transcript to its message. |
| 4I | Approvals center | `src/ui/approvals.rs` (new), `workspace.rs`, `state.rs` (`pending_approvals`, `reveal_approval`, `toggle_approvals`) | **Badge:** while any chat waits on a permission answer, the title bar shows "N needs you".<br>**List:** `ShowApprovals` (Ctrl/⌘⇧A, palette). It shows each waiting chat and its question. Reveal (eye) opens that chat at the question.<br>**Esc:** closes it and the other popovers before it stops a reply.<br>**Real CLI:** the badge appeared on a real Write permission. |

**Keys:** Ctrl/⌘U (attach), Ctrl/⌘⇧T (tasks), Ctrl/⌘⇧A (approvals). They are in `main.rs`
and in the shortcuts sheet; the existing test keeps the two in sync. Context, JSON export and
Share are palette entries, with no key.

**Icons:** `paperclip` (attach button, chips, `/` row), `image` (image chips without a
thumbnail), `clock` (versions) and `eye` (Reveal) are wired and no longer reserved. Only
`map` and `split` stay in `stubs::RESERVED_ICONS`, for P3.

**Tokens:** three `Sizes` were added to `theme.rs`:
- `attach_thumb` (28)
- `attach_chip` (220)
- `popover_list` (300)

`UPDATE_SYSTEM_MD=1 cargo test system_md` was run. `system.md` documents the colour, space,
radius and type scales, not each component size, so it did not change.

**Stubs** (`src/stubs.rs`): `attach` is gone, because it is wired now. New stubs:

| Stub | Kind | When it shows |
|------|------|---------------|
| `revert-demo` | demo | Revert in demo mode |
| `revert-no-checkpoint` | P3 | a turn with no checkpoint id, from a chat saved before P2 |
| `reapply` | P3 | Re-apply. It needs a forward checkpoint, which ForgeCLI does not expose. |
| `share-link` | P3 | Share, which needs a hosted service |
| `compact-demo` | demo | Compact in demo mode |

The registry test now checks that each stub is referenced through its `stubs::` constant.

## Seen, and not seen

- **Seen on Linux, light and dark:**
  - Every popover and badge: autocomplete, context, versions, tasks, approvals.
  - Chips, Keep/Revert rows, the revert confirm, and the terminal footer.
  - Dark checks were added this round for tasks, context, versions and approvals.
- **Not verified:**
  - The native file picker: there is no desktop portal under Xvfb.
  - Real drag and drop from a file manager. The drop handler shares its code with paste, which was tested.
  - Anything on Windows. On Windows, copied files arrive as `ExternalPaths`; on X11, GPUI does not read `text/uri-list`, which is why the path-text fallback exists.
- **Not done:** browser-use. It fits better as a ForgeCLI tool than as UI, so it was left for a ForgeCLI change.
