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

## Follow-up: any file attaches, and the @ list as the extension draws it

Modelled on Forge for VS Code (`attachmentStaging.ts`, `types/attachment.ts`,
`imageResize.ts`, the @ dropdown in `ChatInputBox.vue`), written fresh in Rust.

**Attachments (`src/attach.rs`, new `src/stage.rs`).**
- **Any file attaches.** Each file shows as a chip at once, marked "Preparing…" while it is read off the UI thread. A message can't go until every chip is ready.
- **Images** are scaled to a 1568 px long edge, and re-encoded as JPEG when still over about 1 MB. A big pasted screenshot no longer ends the turn with no answer.
- **Text files** go inline, as before.
- **PDFs** go as a document block on an Anthropic endpoint. On an OpenAI-shaped endpoint they go as their text (lopdf).
- **Word and PowerPoint** go as Markdown text. **xlsx, xlsm, xls, xlsb and ods** go as CSV, one block per sheet. Text is capped at 200K characters, and the model is told when it was cut.
- **Archives** are opened in the project: zip, tar, tar.gz, gz and 7z are built in (pure Rust). rar, xz, bz2 and zstd tarballs, iso and cab go to an installed 7-Zip, then `unrar`, then `tar` (bsdtar reads rar and 7z on Windows). If none of these is installed, the archive is still saved, and the chip and the message say why it wasn't opened. Entries can't leave their folder, links are dropped, and extraction stops at 5000 files or 512 MB.
- **Anything else** (pcap, sqlite, binaries…) is copied into the project and named by its path, with a hint on what reads it (e.g. `tshark` for a capture).
- **Where copies go.** Every copy lives in `.forge/attachments/<id>/`, which carries a `.gitignore` of `*`. Removing a chip removes its copy.
- **Chips** show a coloured icon for the kind and a summary such as "Spreadsheet · 7 KB", "Archive · 2 files" or "Packet capture". A warning mark appears when the file has a note. The tooltip gives the source path, the copy's path in the project, and the note.

**ForgeCLI (`AmineXSaid/ForgeCLI`, same branch).** On OpenAI-compatible endpoints:
- A PDF document block goes as its text; ForgeCLI used to drop it.
- When the Read tool returns a PDF, it goes as text, not as its base64.
- Images that a tool returned follow in the next user message.
- Verified end to end: the real binary, a scripted OpenAI endpoint, and a LibreOffice PDF.

**The @ list (`src/ui/autocomplete.rs`, new `src/ui/file_icon.rs`).**
- **Where it opens.** It opens at the word being completed: above the caret in a chat, below it on the new-chat screen. It is 460 px wide, a new `completion_width` token.
- **Rows** show an icon for the file type, the name with the matched letters in bold, and the folder dimmed after it.
- **Folders** are listed, with a chevron.
- **Nothing typed** shows the project's top level.
- **Folders and Enter.** Tab on a folder lists what is in it; Enter adds it. Paths with spaces are quoted the way ForgeCLI reads them.
- **Ranking.** Names that start with what you typed come first.
- **Keys.** ↑, ↓, Tab and Enter now drive the list. The text box binds them to its own actions, which run before key listeners, so before this fix the arrow keys never reached the list.

**Seen on Linux (Xvfb), light and dark:**
- Seven files pasted as paths (PDF, pcapng, docx, rar, zip, xlsx, 7z), each staged and sorted by type.
- The real ForgeCLI on the mock Anthropic endpoint received the PDF as a document block.
- The OpenAI-wire mock received the PDF's text, the sheets as CSV, and the zip's file list.
- The @ list with Tab into `src` and a search.

**Not verified:**
- Opening a real RAR: no extractor is installed here, and there is no fixture.
- Drag and drop from a file manager.
- Anything on Windows.

## Follow-up: the web browser (browser-use)

You asked for this as an additional resource; until now it was left out.

**What it is.** Forge Desktop now has a **Web browser** switch: in Settings → General, in the palette, and as `/browser` in the `/` list. With it on, every chat's ForgeCLI gets browser-use's MCP server (github.com/browser-use/browser-use, MIT). The model can then:
- open pages, click and type;
- read a page's state (its interactive elements), extract its content, take screenshots;
- manage tabs.

Each step asks permission like any other tool.

**How it runs.** It runs as its own process through `uvx` (from uv), pinned to `browser-use[cli]==0.13.11` (`src/browser.rs`). Nothing is installed into the app, and nothing is linked into it.
- If `uv` is missing, turning it on says how to get it, and the switch stays off.
- Turning it on or off restarts idle chats' ForgeCLI on their next message, resuming the conversation, so the change reaches them.

**Seen on Linux:**
- The real ForgeCLI with the server: the model was offered 16 browser tools. It opened a local page, and `browser_get_state` returned its title and the page's "Press me" button.
- The app launched a chat's ForgeCLI with the server once the switch was on.
- The switch in Settings, light and dark; the palette entry and its toast.

In this container Chrome needed a display (Xvfb) and `IN_DOCKER=true`, because it runs as root. On a desktop, browser-use opens a visible Chrome window.

**Not verified:**
- On Windows.
- With a real model choosing the steps itself.
