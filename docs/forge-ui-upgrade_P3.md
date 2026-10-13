# Forge Desktop Chat — Upgrade Prompt P3 of 3: Settings Screens, Engine Surfaces & Process Hardening

> **Sequence:** This is step 3 of 3. **P1 and P2 must be completed and committed first** (foundation + stub registry, core chat components, composer & transcript power tools). Build P3 on top of that result and finish the whole upgrade here.

**P3 goal:** turn the remaining stub surfaces into real screens where they can be built client-side, keep the engine-bound ones honestly stubbed with ready specs, and harden the workflow (CI, screenshot harness, docs, icon coverage) so the upgrade is complete and maintainable.

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
- `src/stubs.rs` — the stub registry from P1, maintained through P2: every not-yet-wired feature with id, file:line, planned step. This step must empty it (or leave only engine-blocked items with a clear reason).

**What already works (green — do NOT rebuild or break):**
- Chat engine (ForgeCLI or `FORGE_DEMO=1`); send / stop / retry / edit / permission verdicts.
- Endpoints & models (add/edit/remove, model list, health checks, `endpoints.json`, key storage). Settings → General (name, light/dark, default mode, bypass).
- Projects, rules & memory, onboarding, doc pane, terminal, file cards, mode/model menus, sidebar, command palette, theme toggle, copy-last-reply.
- P1: stub registry, jump-to-latest, assistant actions, chat context menu, find-in-conversation, starter prompts, feedback, shortcut help.
- P2: attachments, slash/@ autocomplete, context manager, diff Apply/Revert, terminal Copy/Re-run, conversation versions, JSON export, task panel, approvals center.

**Environment notes:**
- On this Linux dev box `cargo build` needs the system `fontconfig` library (`libfontconfig1-dev`; `yeslogic-fontconfig-sys` needs `fontconfig.pc`). Windows is the release target.
- Real chats need a `forge` binary + a working endpoint; `FORGE_DEMO=1` exercises the UI without either.

---

## 2. Non-negotiable repo rules (from CLAUDE.md)

1. **Write everything original.** Study GPUI's docs/examples, but never copy GPL code (e.g. Zed's crates). New dependencies must be Apache-2.0 / MIT / BSD / ISC / OFL, pinned EXACTLY (`=x.y.z`).
2. **One file per component** under `src/ui/`. State lives in `src/state.rs`, NOT in views. Wire new app-level actions into the command palette AND add key bindings in `src/main.rs` — every action also reachable from the palette.
3. All spacing/size/radius/colour/font values come ONLY from tokens in `src/theme.rs`; add a token if missing, then `UPDATE_SYSTEM_MD=1 cargo test system_md`.
4. Icons: sizes ONLY from the token scale (12/14/16/20), Lucide 2px stroke.
5. `cargo build` with ZERO warnings and `cargo test` green before finishing.
6. Run the app, verify LIGHT and DARK; report Windows vs Linux observations.
7. Features with no engine behind them yet show a toast (via the stub registry) — never fake or crash.

---

## 3. Where this step sits (roadmap recap)

| # | Component / fix | Lands in |
|---|-----------------|----------|
| 1–17 | Foundation, dead-control fixes, chat core, attachments, autocomplete, context manager, diffs, terminal, versions, export, task panel, approvals center | P1 + P2 (done — build on them) |
| 18 | **Settings: Slash Commands, Hooks, Skills, Agents, MCP Servers → real editors** | **P3 (this step)** |
| 19 | **Settings: Guide → static walkthrough** | **P3** |
| 20 | **Engine-bound tabs (Permissions, Sandbox, Network, Environments, Memory and Rules, Plugins) → honest stubs with ready specs** | **P3** |
| 21 | **Endpoint Diagnostics + Capability probe (best-effort client-side)** | **P3** |
| 22 | **Profiles → Create (file-backed profile layering)** | **P3** |
| 23 | **Project/file map + split (multi-pane) view** | **P3** |
| 24 | **Workflow hardening: CI gates, screenshot harness, README key table, icon-coverage check** | **P3** |

**Dead icons this step must wire:** `map` (project/file map) and `split` (multi-pane). After this step, every bundled icon must be referenced by UI code or documented as reserved — zero silent dead assets.

---

## 4. This step's work (P3)

For each: spec-before-code (states: default/empty/loading/error/disabled; light+dark; keyboard), then implement.

### 4A. Settings tabs → real editors (file-backed)
Convert these toast-only tabs into working screens backed by config files in Forge's config folder (mirror the patterns in `src/rules.rs` / `src/endpoints.rs` — list, create, edit, delete, save):
- **Slash Commands** (`src/ui/settings.rs` + one file per new component under `src/ui/`): list existing commands, create/edit with a name + prompt body, delete with confirm.
- **Hooks**: named hooks with an event trigger and a command; create/edit/delete.
- **Skills**: discover folders containing `SKILL.md` (walk the project like `src/rules.rs` does), list them with enable/disable.
- **Agents**: named agents with instructions + tool selection; create/edit/delete.
- **MCP Servers**: named servers with command/URL + auth; add/edit/remove, with a status row (check what the engine exposes; keep a stub entry where the engine has no MCP support yet).
Each editor persists to disk, validates input, shows empty/error states, and reloads on change. Wire each into the command palette ("Open Slash Commands…", etc.) where sensible.

### 4B. Guide tab
A static, scrollable walkthrough of Forge: what it is, how endpoints/models work, the key bindings (from `main.rs`), modes, projects/rules, and the new P1/P2 features. No engine needed; keep copy short and in the app's voice (see the existing section blurbs).

### 4C. Engine-bound tabs → honest stubs with ready specs
For **Permissions, Sandbox, Network, Environments, Memory and Rules, Plugins**: keep the honest "coming soon" treatment from the stub registry, but upgrade each stub screen to show a concrete spec of what it WILL do (the one-line blurbs already exist in `settings.rs` — expand them into a short "Planned" list) and what engine support it waits on. Do NOT build fake controls that do nothing; remove the toast-only decoys in favour of clearly-labelled planned screens.

### 4D. Endpoint Diagnostics + Capability probe
Replace the two toasts (`settings.rs:779` and `:783`) with real, client-side checks using the existing deps (ureq is already a dependency): DNS resolve, TCP connect, TLS handshake, HTTP ping of the endpoint's base URL, auth sanity check where possible. Present results as a step list with pass/fail icons and per-step detail. Anything model-side (streaming, tools, vision, reasoning probes) stays in the stub registry with an honest note.

### 4E. Profiles → Create
Replace the "Profiles aren't available yet" toast (`settings.rs:830`) with real file-backed profiles: create a named profile file (starts empty; anything it does not set falls back to user settings), switch the active profile, delete a profile. Layer over the existing `src/profile.rs` persistence. If the engine cannot consume layered profiles yet, switching persists the choice locally and toasts the limitation via the registry.

### 4F. Project/file map + split view
- **Map** (wire the `map` icon): a compact overview of the current project — folders/files at a glance (reuse the git-style walker in `src/rules.rs`) — from the project page, showing recent edits where the engine provides them.
- **Split** (wire the `split` icon): allow the side `doc_pane` (and, where sensible, the chat + a second view) to be arranged side-by-side or stacked; extend the existing doc-pane layout rather than inventing a parallel pane system.

### 4G. Workflow hardening (finish the upgrade)
- **CI**: extend the GitHub Actions workflow (`.github/workflows/windows.yml`) with `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` as a gate; add a Linux build/test job if the runner matrix allows.
- **Screenshot/verification harness**: add a script (under `scripts/`) that launches the app with `FORGE_DEMO=1` and captures light + dark screenshots of the key screens (welcome, chat, settings, projects) so "run it and look at it" is repeatable; document how to run it.
- **Docs**: update `README.md` — the key-binding table must list every binding added in P1–P3, and the Layout section must mention the new components.
- **Icon-coverage check**: verify every icon in `src/assets.rs` is referenced by UI code or documented; remove genuinely dead assets only with the user's agreement.
- **Final pass**: `cargo build` zero warnings, `cargo test` green, light + dark run of every screen, stub registry emptied (only engine-blocked items may remain, each with a reason).

---

## 5. Workflow while you work

1. Keep every change scoped: one component file + `state.rs` wiring + action/key/palette registration.
2. Build and test incrementally: `cargo build` (zero warnings) and `cargo test` after EACH screen.
3. Run the app and check light + dark per change; report Windows vs Linux.
4. Update `src/stubs.rs` as you wire each item — by the end it must contain only engine-blocked entries with reasons.
5. Commit in small, described commits; the final commit closes the 3-step upgrade.

---

## 6. Acceptance criteria (P3 — and the whole upgrade)

- Builds with ZERO warnings; `cargo test` green; CI gates (fmt/clippy/test) in place.
- Slash Commands, Hooks, Skills, Agents, MCP Servers and the Guide are real working screens; engine-bound tabs are honestly planned, never fake controls.
- Diagnostics and Capability probe show real client-side results; model-side probes are honestly stubbed.
- Profile create/switch/delete works file-backed; the engine limitation (if any) is toasted honestly.
- Map and split views work; `map` and `split` icons are wired; zero silent dead icon assets.
- README key table and Layout section are current; the screenshot script is documented and runnable.
- Stub registry contains only engine-blocked items, each with a reason. Light + dark verified on every screen.

---

## 7. Report format (per item)

- File(s) changed and how it was wired into `state.rs`, the `actions!` macro, key bindings, and the command palette.
- Token changes in `theme.rs` (and whether `system.md` was regenerated).
- What you observed when running the app (Windows if available, otherwise Linux), in light and dark.
- Final state of the stub registry (what remains and why).
