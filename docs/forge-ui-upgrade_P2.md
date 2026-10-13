# Forge Desktop Chat — Upgrade Prompt P2 of 3: Composer & Transcript Power Tools

> **Sequence:** This is step 2 of 3. **P1 must be completed and committed first** (foundation, stub registry, core chat components). Build P2 on top of that result; P3 follows. Do NOT start P3 work here — keep its placeholders honest via the stub registry.

**P2 goal:** make the input surface first-class (attachments, slash/@ autocomplete, context management) and add the transcript power tools (diffs, terminal, versions, tasks, approvals) — everything here is client-side and needs no new engine capability beyond what P1 already uses.

---

## 1. The repo (context)

**forge-gpui** — the Forge desktop chat app, written in Rust on GPUI + gpui-component via the `gpui-kit` crate (`=0.7.1`, re-exports the pinned `gpui-pre` 0.3.8 snapshot). It is a UI shell for a ForgeCLI process: each chat runs `forge` as its engine over the stream-json protocol. CLOSED SOURCE, ships to **Windows**, starts with demo data; `FORGE_DEMO=1` plays a fully scripted reply so every part of the UI can be exercised without a model.

**Project map (read these first):**
- `src/theme.rs` — the ONLY source of colours, sizes, radii, fonts. `.interface-design/system.md` is GENERATED from it (`src/system_md.rs`).
- `src/state.rs` — all app state, the transcript model (user turn → assistant blocks: Text / Thinking / Tool / Permission / File), demo chats, the scripted reply.
- `src/backend.rs` — each chat's `forge` process: launch, stream-json events, permission answers.
- `src/ui/mod.rs` — the `actions!` macro + shared icon/avatar/cube helpers + theme sync.
- `src/main.rs` — key bindings (`bind_keys`).
- `src/ui/` — ONE FILE PER COMPONENT. Existing: workspace, sidebar, chat_view, message, markdown, composer, command_palette, model_menu, mode_menu, onboarding, projects, rules_view, settings, side_chat, doc_pane, terminal, sheet_view, file_card, file_mark, forge_cube, context_ring, diagram_view, skeleton, voxels, motion.
- `src/assets.rs` — bundled icon/font registry (Lucide icons, OFL fonts).
- `src/endpoints.rs`, `src/health.rs`, `src/secrets.rs`, `src/profile.rs`, `src/rules.rs` — endpoint persistence, health checks, key storage, profile, rules files.
- `src/stubs.rs` — the stub registry created in P1: every not-yet-wired feature with id, file:line, planned step. UPDATE it as you wire things in this step.

**What already works (green — do NOT rebuild or break):**
- Chat engine (ForgeCLI or `FORGE_DEMO=1`); send / stop / retry / edit / permission verdicts.
- Endpoints & models: add/edit/remove, model list, health checks, `endpoints.json` load/save, key storage.
- Settings → General (name, light/dark, default mode, bypass). Projects (open folder, new project, pin, switcher). Rules & memory (load/save/delete).
- Onboarding, doc pane, terminal (expand/copy), file cards, mode menu, model menu, sidebar (search/filter/pin), command palette, theme toggle, copy-last-reply.
- P1's additions: stub registry, jump-to-latest, assistant-message actions, chat context menu (rename/delete/duplicate/export Markdown), find-in-conversation, starter prompts + resume, feedback 👍/👎, shortcut help overlay.

**Environment notes:**
- On this Linux dev box `cargo build` needs the system `fontconfig` library (`libfontconfig1-dev`; `yeslogic-fontconfig-sys` needs `fontconfig.pc`). Windows is the release target.
- Real chats need a `forge` binary + a working endpoint; `FORGE_DEMO=1` exercises the UI without either.

---

## 2. Non-negotiable repo rules (from CLAUDE.md)

1. **Write everything original.** Study GPUI's docs/examples, but never copy GPL code (e.g. Zed's crates). New dependencies must be Apache-2.0 / MIT / BSD / ISC / OFL, pinned EXACTLY (`=x.y.z`).
2. **One file per component** under `src/ui/`. State lives in `src/state.rs`, NOT in views. Wire new app-level actions into the command palette AND add key bindings in `src/main.rs` — every action also reachable from the palette.
3. All spacing/size/radius/colour/font values come ONLY from tokens in `src/theme.rs`; add a token if missing, then `UPDATE_SYSTEM_MD=1 cargo test system_md`.
4. Icons: sizes ONLY from the token scale (12/14/16/20), Lucide 2px stroke.
5. `cargo build` with ZERO warnings and `cargo test` green before finishing this step.
6. Run the app, verify LIGHT and DARK; report Windows vs Linux observations.
7. Features with no engine behind them yet show a toast (via the stub registry) — never fake or crash.

---

## 3. Where this step sits (roadmap recap)

| # | Component / fix | Lands in |
|---|-----------------|----------|
| 1–8 | Foundation: stub registry, dead-control fixes, jump-to-latest, assistant actions, chat context menu, find-in-conversation, starter prompts, feedback, shortcut help | **P1 (done — build on it)** |
| 9 | **Attachments (drag-drop, paste, chips, previews) — replaces the Attach toast** | **P2 (this step)** |
| 10 | **Slash-command + @-mention autocomplete in the composer** | **P2** |
| 11 | **Context manager popover (token breakdown + Compact)** | **P2** |
| 12 | **Apply / Revert on tool diffs** | **P2** |
| 13 | **Terminal: Copy output + Re-run** | **P2** |
| 14 | **Conversation versions (view / restore)** | **P2** |
| 15 | **Export JSON + share** | **P2** |
| 16 | **Task/progress panel** | **P2** |
| 17 | **Approvals center ("Needs you")** | **P2** |
| 18–24 | Settings screens, engine-bound tabs, Diagnostics/Capability probe, Profiles Create, map/split, CI & screenshot harness | P3 (later) |

**Dead icons this step must wire:** `image`, `paperclip` (attachments), `clock` (conversation versions), `eye` (reveal/preview). Remaining dead icons `map` and `split` are reserved for P3 — keep them documented in the stub registry, not silent.

---

## 4. This step's work (P2)

For each: spec-before-code (states: default/empty/loading/error/disabled; light+dark; keyboard), then implement.

### 4A. Attachments (replaces the Attach toast)
- *Why:* the composer's Attach (+) button currently toasts "not wired" — users cannot share files, screenshots or images with the agent.
- Drag-and-drop files/images onto the composer; paste images/files from the clipboard (Ctrl/⌘+V when the clipboard holds one).
- Removable attachment chips under the input: icon + name + size + × remove; image thumbnails where feasible.
- The Attach button opens a real file picker (reuse the `PathPromptOptions` pattern from `projects.rs`).
- State holds pending attachments (`src/state.rs`); on Send, pass them through `state.send`/`backend.rs` where the CLI accepts them; where the CLI cannot yet, toast via the stub registry — never silently drop them.
- Wire the `image` and `paperclip` icons.

### 4B. Slash-command + @-mention autocomplete
- *Why:* the composer is a plain textarea — `/` and `@` trigger nothing, so commands, files and modes are undiscoverable.
- On input, detect a trigger token: `/` → commands (existing app actions + modes, from the command palette's command list); `@` → file/project references (walk the current project the way `src/rules.rs` already does — reuse that walker).
- Popover anchored above the composer, keyboard navigable (↑/↓ move, Enter/Tab complete, Esc closes); selecting replaces the trigger token and moves the caret.
- Highlight the matched part of each row (reuse the palette's fuzzy-highlight style). Empty result shows "No matches".

### 4C. Context manager popover
- *Why:* the context meter ring shows only a %; users cannot see what is consuming the window or act on it.
- Click the context meter/ring (`composer.rs`) → popover: per-message token breakdown (use `crate::context::Usage` and the turn structure), totals, % bar, the model's window from the active profile, and a **Compact conversation** action (wire to state; if the engine cannot compact, toast via the registry).

### 4D. Apply / Revert on tool diffs
- *Why:* edits render as a read-only diff — users cannot accept or undo what a tool changed.
- Per-file **Apply** / **Revert** buttons on tool diff blocks (`message.rs`), and per-hunk where feasible: Apply accepts the shown diff, Revert restores the pre-tool state. Where the engine provides no way back, Revert toasts via the registry — Apply must be honest about what it does.

### 4E. Terminal improvements
- *Why:* the terminal block is static output; users cannot copy the output or re-run a command.
- Add **Copy output** on the terminal tail (`terminal.rs`) and a **Re-run** action on completed commands (re-issues through the engine; toast via registry if the engine can't).

### 4F. Conversation versions
- *Why:* retry/edit forks the transcript, but prior versions cannot be browsed or restored.
- When a turn was retried or edited, show a small control to view and restore the previous version (keep the replaced version in `state.rs`). Wire the `clock` icon. Restore swaps the transcript back; viewing is read-only.

### 4G. Export JSON + share
- *Why:* P1 added Markdown export; users also want structured export and a share path.
- **Export conversation** as JSON (full transcript incl. tool calls) from the chat context menu and command palette, alongside the P1 Markdown export. A **Share** entry: copy the JSON to the clipboard now, and register the upload/share-link engine capability in the stub registry.

### 4H. Task/progress panel
- *Why:* parallel subagent and build runs are only inline spinner rows — no overview.
- A collapsible panel listing the turn's running + finished tool/subagent tasks (from the existing Tool blocks in the transcript), each with its status icon and a click to jump to its row. Collapsed by default; a small badge shows the running count.

### 4I. Approvals center
- *Why:* a background chat waiting on a permission is only a tiny sidebar dot; there is no queue or jump-to.
- A badge/panel listing chats with a pending permission ("Needs you"), each clickable to jump to that chat and open its permission card. Reuse the sidebar status dots as the data source (`ChatStatus::NeedsYou`). Wire the `eye` icon where it fits (e.g. "reveal" the pending item in the transcript).

### 4J. Wiring requirements
Every new feature: state in `src/state.rs`; app-level action in the `actions!` macro; key binding in `src/main.rs` where one applies; command-palette entry; one file per new UI component under `src/ui/`; stub registry updated.

---

## 5. Workflow while you work

1. Keep every change scoped: one component file + `state.rs` wiring + action/key/palette registration.
2. Build and test incrementally: `cargo build` (zero warnings) and `cargo test` after EACH component.
3. Run the app and check light + dark per change; report Windows vs Linux.
4. Update `src/stubs.rs` as each item is wired; anything deferred to P3 must remain listed there.
5. Commit at the end of P2 so P3 can build on it.

---

## 6. Acceptance criteria (P2)

- Builds with ZERO warnings; `cargo test` green.
- The Attach button no longer toasts — attachments add real chips; anything the CLI cannot receive is handled via the stub registry, never dropped silently.
- Slash/@ autocomplete, context manager, diff Apply/Revert, terminal Copy/Re-run, conversation versions, JSON export, task panel and approvals center all work in light and dark with theme tokens only.
- Every new action is reachable from the command palette and has a sensible key binding where one applies.
- Icons `image`, `paperclip`, `clock`, `eye` are wired; `map`/`split` remain documented reservations for P3.

---

## 7. Report format (per item)

- File(s) changed and how it was wired into `state.rs`, the `actions!` macro, key bindings, and the command palette.
- Token changes in `theme.rs` (and whether `system.md` was regenerated).
- What you observed when running the app (Windows if available, otherwise Linux), in light and dark.
- Any item left for P3, recorded in the stub registry with a reason.
