# Forge Desktop Chat — Upgrade Prompt P1 of 3: Foundation & Dead-Control Fixes

> **Sequence:** This is step 1 of a 3-step upgrade. Run **P1 → P2 → P3 in order**, each on top of the previous step's committed result. P2 and P3 repeat the shared context and rules, so every file can run standalone. Do NOT start work that is scheduled for a later step; keep its placeholder honest and listed in the stub registry.

**P1 goal:** remove every silent toast-only decoy, establish the central stub registry, and add the core chat-ergonomics components — all achievable with NO new engine capability.

---

## 1. The repo (context)

**forge-gpui** — the Forge desktop chat app, written in Rust on GPUI + gpui-component via the `gpui-kit` crate (`=0.7.1`, re-exports the pinned `gpui-pre` 0.3.8 snapshot). It is a UI shell for a ForgeCLI process: each chat runs `forge` as its engine over the stream-json protocol. The app is CLOSED SOURCE, ships to **Windows** as the release target, and starts with demo data; `FORGE_DEMO=1` plays a fully scripted reply so every part of the UI can be exercised without a model.

**Project map (read these first):**
- `src/theme.rs` — the ONLY source of colours, sizes, radii, fonts. Nothing else hardcodes px/hex. `.interface-design/system.md` is GENERATED from it (`src/system_md.rs`).
- `src/state.rs` — all app state, the transcript model (user turn → assistant blocks: Text / Thinking / Tool / Permission / File), demo chats, the scripted reply.
- `src/backend.rs` — each chat's `forge` process: launch, stream-json events, permission answers.
- `src/ui/mod.rs` — the `actions!` macro (all app actions) + shared icon/avatar/cube helpers + theme sync.
- `src/main.rs` — key bindings (`bind_keys`).
- `src/ui/` — ONE FILE PER COMPONENT. Existing: workspace, sidebar, chat_view, message, markdown, composer, command_palette, model_menu, mode_menu, onboarding, projects, rules_view, settings, side_chat, doc_pane, terminal, sheet_view, file_card, file_mark, forge_cube, context_ring, diagram_view, skeleton, voxels, motion.
- `src/assets.rs` — bundled icon/font registry (Lucide icons, OFL fonts).
- `src/endpoints.rs`, `src/health.rs`, `src/secrets.rs`, `src/profile.rs`, `src/rules.rs` — endpoint persistence, health checks, key storage, profile, rules files.

**What already works (green — do NOT rebuild or break):**
- Chat engine: real ForgeCLI process (`backend.rs`) or the `FORGE_DEMO=1` scripted reply; send / stop / retry / edit / permission verdicts all flow through `state.rs`.
- Endpoints & models: add/edit/remove profile, model list over HTTP, health checks ("Check all again"), `endpoints.json` load/save, key storage.
- Settings → General: name save, light/dark, default mode, bypass toggle.
- Projects: open folder, new project, pin, project switcher. Rules & memory: load/save/delete rule files.
- Onboarding (name/avatar), doc pane (copy/download/open/close), terminal (expand/copy), file cards, mode menu, model menu, sidebar (search/filter/pin), command palette (fuzzy over actions/modes/models/chats), theme toggle, copy-last-reply.
- Assets are clean: 0 referenced-but-missing icons; all 16 cube sizes present.

**Environment notes:**
- On this Linux dev box `cargo build` currently fails because the system library `fontconfig` is missing (`yeslogic-fontconfig-sys` needs `fontconfig.pc`; install `libfontconfig1-dev` or equivalent). Work on a machine with the deps; Windows remains the release target.
- Real chats need a `forge` binary (via `FORGE_CLI`, beside the exe, or on PATH) + a working endpoint; otherwise the About screen reports "ForgeCLI not found" and sends do nothing.
- The repo may hold uncommitted work on `feature/port-forge-desktop-settings` (touches `settings.rs`, `projects.rs`, `sidebar.rs`, `workspace.rs`, `state.rs`). Before editing those files, ask the user whether to commit it or work on top — never discard it silently.

---

## 2. Non-negotiable repo rules (from CLAUDE.md)

1. **Write everything original.** You may study GPUI's docs/examples to learn, but never copy GPL code (e.g. Zed's crates). Every new dependency must be Apache-2.0 / MIT / BSD / ISC / OFL, pinned EXACTLY (e.g. `=x.y.z`).
2. **One file per component** under `src/ui/`. State lives in `src/state.rs`, NOT in views. Wire new app-level actions into the command palette AND add their key bindings in `src/main.rs` — every action must also be reachable from the command palette.
3. All spacing/size/radius/colour/font values come ONLY from tokens in `src/theme.rs`. If a token is missing, add it there and regenerate the design system doc with `UPDATE_SYSTEM_MD=1 cargo test system_md`.
4. Icons: sizes ONLY from the token scale (12/14/16/20), Lucide 2px stroke.
5. `cargo build` with ZERO warnings and `cargo test` green before finishing this step.
6. For any visible change, run the app and verify LIGHT and DARK. Windows is the release target: report what was seen on Windows vs only on Linux.
7. Features with no engine behind them yet must show a toast (the repo's convention) — never fake data paths or crash.

---

## 3. The full 3-step roadmap (overview)

| # | Component / fix | Lands in |
|---|-----------------|----------|
| 1 | Central stub registry + honest "coming soon" treatment for dead controls | **P1** |
| 2 | Jump-to-latest button | **P1** |
| 3 | Assistant-message actions (Copy / Regenerate / Continue) | **P1** |
| 4 | Chat context menu (Rename / Delete / Duplicate / Export Markdown) | **P1** |
| 5 | Find-in-conversation | **P1** |
| 6 | Starter prompts + "resume recent chat" | **P1** |
| 7 | Feedback 👍/👎 on the last turn | **P1** |
| 8 | Shortcut help overlay | **P1** |
| 9 | Attachments (drag-drop, paste, chips, previews) — replaces the Attach toast | **P2** |
| 10 | Slash-command + @-mention autocomplete in the composer | **P2** |
| 11 | Context manager popover (token breakdown + Compact) | **P2** |
| 12 | Apply / Revert on tool diffs | **P2** |
| 13 | Terminal: Copy output + Re-run | **P2** |
| 14 | Conversation versions (view / restore) | **P2** |
| 15 | Export JSON + share | **P2** |
| 16 | Task/progress panel | **P2** |
| 17 | Approvals center ("Needs you") | **P2** |
| 18 | Settings: Slash Commands, Hooks, Skills, Agents, MCP Servers → real editors | **P3** |
| 19 | Settings: Guide → static walkthrough | **P3** |
| 20 | Engine-bound tabs (Permissions, Sandbox, Network, Environments, Memory and Rules, Plugins) → honest stubs with ready specs | **P3** |
| 21 | Endpoint Diagnostics + Capability probe (best-effort client-side) | **P3** |
| 22 | Profiles → Create (file-backed profile layering) | **P3** |
| 23 | Project/file map + split (multi-pane) view | **P3** |
| 24 | Workflow hardening: CI gates, screenshot harness, README key table, icon-coverage check | **P3** |

**Dead icons bundled but unreferenced today** (evidence of planned features — each must be wired or documented by the end of P3): `ellipsis` (context menus, **P1**), `keyboard` (shortcut help, **P1**), `archive` (export, **P1/P2**), `image`/`paperclip` (attachments, **P2**), `clock` (versions, **P2**), `eye` (reveal/preview, **P2**), `map` and `split` (**P3**). `window-*` icons are used internally by the title bar.

---

## 4. This step's work (P1)

### 4A. Central stub registry (new)
Create `src/stubs.rs` — a single, searchable table of every feature that is not wired yet: `id`, where it appears (file:line), what it should do, its toast text, and whether it depends on the engine. Replace the scattered `show_toast("… isn't available/wired yet")` strings with lookups into this registry. Choose ONE consistent visual treatment for controls that cannot work yet (e.g. a subdued "Soon" marker or disabled state with a tooltip), and apply it uniformly. Register these existing stubs with their current locations:

| Stub | Today at |
|------|----------|
| Composer Attach | `src/ui/composer.rs:370` — toast "Attaching files and images isn't wired to ForgeCLI yet." (becomes real in P2) |
| Settings → Endpoints → Diagnostics → Run | `src/ui/settings.rs:779` — toast "Diagnostics isn't available in Forge yet." (P3) |
| Settings → Endpoints → Detect capabilities → Probe | `src/ui/settings.rs:783` — toast "Capability probing isn't available in Forge yet." (P3) |
| Settings → Profiles → New profile → Create | `src/ui/settings.rs:830` — toast "Profiles aren't available in Forge yet." (P3) |
| Settings tabs: Plugins, Environments, Memory and Rules, Permissions, Sandbox, Network, Hooks, Skills, Agents, MCP Servers, Slash Commands, Guide | `src/ui/settings.rs:882` — stub screen + toast (P3) |

### 4B. Core chat components (detailed)
For each: follow the spec-before-code loop (states: default/empty/loading/error/disabled; light+dark; keyboard), then implement.

1. **Jump-to-latest button** — *Why:* a user who scrolls up to read has no way back to the live tail. Floats over the transcript when the list is not following the tail (use `ListState` follow-mode detection in `chat_view.rs`); click returns to tail; hidden while at the bottom; animate in/out with the existing motion tokens.
2. **Assistant-message actions** — *Why:* replies currently have no per-message affordances, only the global "copy last reply". Per-reply hover actions on the turn header (`message.rs`): **Copy** (whole reply), **Regenerate** (retry this turn — reuse the existing retry plumbing in `state.rs`), **Continue** (resume after a stop — add `continue_reply` to `state.rs`/`backend.rs`; if the engine cannot resume, toast via the registry).
3. **Chat context menu** — *Why:* sidebar rows only support pin; users cannot rename, delete, duplicate or export. Right-click + a hover **⋯** button on each sidebar row (`ellipsis` icon): **Rename** (inline or small modal), **Delete** (confirm modal), **Duplicate** (clone chat incl. messages), **Export as Markdown** (write a file to disk, toast the path — reuses the existing download/state pattern).
4. **Find-in-conversation** — *Why:* no way to search the current transcript. Register `ToggleFind` (Ctrl/⌘+F, context "Chat") + palette entry: a search bar at the top of the chat view with match count ("3/12"), up/down buttons, Enter/Shift+Enter, Esc closes; jump the list to each match (best-effort highlight; jumping + count is the minimum).
5. **Starter prompts + resume** — *Why:* the welcome screen is only a cube and a greeting; new users have no obvious first step. 3–4 suggestion cards under the greeting that fill the composer (reuse `Composer::fill`); plus a "Resume <recent chat>" row when a recent chat exists.
6. **Feedback** — *Why:* users cannot rate a reply or report a bad result. 👍/👎 beside the last assistant turn's end; store the verdict in state (memory-only), toast a confirmation; engine upload is out of scope — register in the stub registry.
7. **Shortcut help overlay** — *Why:* every shortcut exists but is undiscoverable. A palette command "Keyboard shortcuts" (wire the `keyboard` icon) opening an overlay that lists every binding in `main.rs`; keep the list in sync manually and note that in a comment.

### 4C. Icon coverage this step
Wire the previously-dead icons `ellipsis` (context menu), `keyboard` (help overlay), `archive` (export). Leave `clock` for P2 (conversation versions) but document the reservation in the stub registry so it is not lost. Do not add new icon assets unless a Lucide 2px icon is genuinely needed; register any new icon in `src/assets.rs`.

### 4D. Wiring requirements
For every new feature: state fields/logic in `src/state.rs`; a new action in the `actions!` macro (`src/ui/mod.rs`) where it is app-level; a key binding in `src/main.rs` where one applies; a command-palette entry (`src/ui/command_palette.rs`) so it is reachable without the mouse. One file per new UI component under `src/ui/`.

---

## 5. Workflow while you work

1. Keep every change scoped: one component file + `state.rs` wiring + action/key/palette registration — no cross-cutting rewrites.
2. Build and test incrementally: `cargo build` (zero warnings) and `cargo test` after EACH component, not once at the end.
3. Run the app and check light + dark for each visible change; report Windows observations when available, otherwise Linux.
4. Update the stub registry as you wire things up, so the remaining stubs stay accurate for P2/P3.
5. Commit at the end of P1 with a clear message so P2 can build on it.

---

## 6. Acceptance criteria (P1)

- Builds with ZERO warnings; `cargo test` green.
- No visible control silently toasts as a decoy: every dead control uses the registry's honest treatment; the registry lists every remaining stub with its file:line and planned step (P2/P3).
- Each P1 component works in light and dark, uses only theme tokens, and matches the existing visual language.
- Every new action is reachable from the command palette and has a sensible key binding where one applies.
- Icons `ellipsis`, `keyboard`, `archive` are wired; no NEW dead assets introduced.

---

## 7. Report format (per item)

- File(s) changed and how it was wired into `state.rs`, the `actions!` macro, key bindings, and the command palette.
- Token changes in `theme.rs` (and whether `system.md` was regenerated).
- What you observed when running the app (Windows if available, otherwise Linux), in light and dark.
- Any item left for P2/P3, recorded in the stub registry with a reason.
