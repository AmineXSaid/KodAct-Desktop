# Forge Design: Open Design, in Rust

> **What this is:** the plan for bringing **all** of Open Design into Forge.
> Its logic becomes Rust. Its content, and the few scripts that can only run
> inside a web page or inside Figma, are kept as they are. Nothing is left
> out except what physically cannot work inside Forge (§4.6).
>
> **Source:** [nexu-io/open-design](https://github.com/nexu-io/open-design),
> Apache-2.0, v0.25.1, pinned at commit `3b0df90` (2026-10-20).
>
> **Status:** plan only. No code yet. Phases D1–D8 are in §6.

---

## 1. The goal in one paragraph

You ask Forge for a course, an introduction deck, a report, a landing page or
a prototype. **The right panel opens on the design and you watch it being
built as the agent types it.** It's a real page you can click, scroll and
flip through. You pick the look (a design system), the starting point (a
template) and the kind of work (deck, document, prototype, media). Forge
gives the agent exactly the craft rules, design system and skill that job
needs. When it's done, you export to PDF, an editable PowerPoint, PNG, a
single HTML file, or Figma. That's what Claude Design and Open Design do,
running on ForgeCLI, written in Rust, with no Node.js.

## 2. Rules this plan keeps

- **Licences (CLAUDE.md rule 1).**
  - Open Design is Apache-2.0. We keep its `LICENSE`, add its `NOTICE`, and
    mark every file we change (Apache §4(b)).
  - Folders with their own licence (MIT from Vercel, GreenSock, Emil Kowalski
    and others) keep it beside their files.
  - The library ships a `THIRD_PARTY_LICENSES.md` that lists all of them.
  - Each Rust module that ports Open Design logic opens with
    `//! Adapted from Open Design (Apache-2.0), <path>`.
  - A test fails the build on any GPL, AGPL or unknown licence.
- **Pinned versions (rule 2).**
  - The library is pinned to one upstream commit. Re-syncing it is a
    reviewed diff, never a silent update.
  - `dom-to-pptx` stays at 2.0.1. Any new crate is pinned `=x.y.z`.
- **One file per component (rule 3).**
  - Every new screen is its own file under `src/ui/`. State lives in
    `src/state.rs`.
  - Every action gets a key binding in `main.rs` and an entry in the command
    palette.
- **Zero warnings, tests green (rule 4)** on every commit.
- **Run it and look at it (rule 5).**
  - Every visible change gets screenshots in light and dark, plus
    `/visual-critique:critique-screen`.
  - Each report says what was seen on Windows and what only on Linux.
- **Tokens only.**
  - New sizes and colours go into `src/theme.rs` first.
  - Then run `UPDATE_SYSTEM_MD=1 cargo test system_md`.

## 3. The live design panel (core requirement)

**Forge opens the design in the right panel and you see it while it's being
designed.** This is the centre of the feature, so it ships in D2, right after
the library. It's built on what Forge already has:

- **`web_view.rs`** already runs Chromium over the DevTools protocol (DevTools
  is Chromium's remote-control interface). It shows the page live in the side
  panel through a screencast, and sends your clicks, scrolling and typing back
  to the page.
- **`browser_mcp.rs`** already lets the agent drive that same tab.
- **`state.rs::open_doc` / `show_doc`** already open a file in the panel, but
  only when you click its card. And `doc_pane.rs` only shows HTML when the
  page wraps a diagram's `<svg>`.

What gets built (D2, `src/design/live.rs` + `src/ui/design_panel.rs`):

1. **It opens by itself.** The first time the agent writes an `.html`
   artifact in a design chat (a Write tool call on an `.html` path), Forge:
   - starts Chromium if it isn't running yet, even when the browser setting
     is off (a design chat needs it);
   - opens the right panel on a **Design** tab;
   - shows that file.
   You don't click anything.
2. **You watch it being typed.** ForgeCLI already streams each tool call's
   input as it's generated (`--include-partial-messages`). While a Write call
   streams:
   - Forge reads the `content` written so far, about every 250 ms;
   - it closes any tags left open;
   - it puts that HTML into the preview tab with `Page.setDocumentContent`.
   So the page forms in front of you, section by section, like Open Design's
   streamed preview. When the call finishes, the tab loads the real file, so
   images, fonts and relative links resolve.
3. **Edits refresh in place.** After each Edit or Write to a file being
   shown, the tab reloads, and Forge puts back where you were: the scroll
   position and the current slide, read and restored over DevTools. A deck
   doesn't jump back to slide 1 every time the agent changes one word.
4. **The panel follows the agent.** When the agent moves to a different file
   (a second page, `styles.css`, the next deck), the panel switches to it.
   Each artifact gets its own tab in the Design view. A **Follow** switch
   turns this off when you want to stay where you are. This is the idea behind
   Open Design's `artifact-focus-marker.ts`.
5. **It's a real page.**
   - You can click, scroll, play animations, and use html-ppt's presenter
     mode (`S`) and keyboard navigation.
   - You can take over at any time, as with the browser panel today.
6. **The agent sees what you see.** The design tools' `render` (§5) takes
   screenshots of that same tab, so the agent's look-and-fix step checks
   exactly the page you're watching.
7. **Pages are served from a loopback-only server.** A small read-only HTTP
   server in Rust (`src/design/serve.rs`, `std::net`):
   - listens on 127.0.0.1 only, on a random port, with a random token in
     every path;
   - serves the project folder and the design library, nothing else;
   - exists because `file://` blocks the `fetch()` that some templates use.
   This ports Open Design's `/raw/` static route (`routes/static-resource.ts`).
8. **The rest of the panel.**
   - A deck shows a thumbnail rail (from screenshots), slide n of N, and
     previous/next.
   - A page shows a device-width switch: desktop, tablet, phone (Open
     Design's `assets/frames`).
   - Export and Comment buttons sit in the panel header.

Acceptance for this section (checked in D2, on Windows and Linux):
- Ask "make a 6-slide course introduction" in a design chat. The panel opens
  within a second of the first Write and fills in while the agent types.
- After an edit to slide 4, the panel is still on slide 4.
- Recorded as a short screen capture, plus screenshots in light and dark.

Changed while building D2a: pages are shown as `file://` addresses, not
through a loopback server (item 7). Of Open Design's 281 example pages only
7 use `fetch()` or ES modules, and `file://` keeps a page's scripts away
from the rest of the computer.

### 3.1 The design canvas controls, after Claude Desktop

Claude Desktop's design canvas has 34 controls. Each one is adopted into the
browser panel's design view: the panel shows a page Forge designed with the
**design bar** in place of the address bar (`src/ui/design_bar.rs`). Forge's
designs are live HTML pages, not artboards on a canvas, so a few controls
become their HTML counterpart; the table says how.

| # | Claude Desktop | In Forge | Step |
|--:|:--|:--|:--|
| | **Header** | | |
| 1 | Sidebar toggle | The page's outline beside it: its headings (h1–h3), the one at the top marked; a click scrolls to one | **Done** (Canvas 3) |
| 2 | Title, with its menu | The page's menu (the Pages menu): Duplicate (a copy beside it, listed with the chat's pages), Rename… (asked of Forge, so it knows the new name) | **Done** (Canvas 3) |
| 3 | Comments on/off | Show or hide the comment pins on the page (the count of open ones) | **Done** (Canvas 2) |
| 4 | Comments menu | The page's comments, each with its part and whether it was sent, waits or is a note; a click scrolls to it; Resolve; Show resolved; Show pins | **Done** (Canvas 2) |
| 5 | Share | Share menu: Save as PDF beside the page, Open in your browser | **Done** (Canvas 1) |
| 6 | Collapse / expand | Present (the page fills the window), and the panel beside or below the chat | **Done** (Canvas 1) |
| 7 | Close | The panel's close button | Already there |
| | **Left toolbar** | | |
| 8 | Pages menu: Page 1 ✓, the page's ⋯ menu, + New page | The chat's pages, ✓ on the one shown; New page… starts the request in the composer; Show in folder; Copy the file's path | **Done** (Canvas 1) |
| 9 | Select tool | The comment tool: point at any part of the page and it is outlined, with its tag; a click opens a comment on it; Esc puts it down | **Done** (Canvas 2) |
| 10 | Insert menu | The tools below, in Claude's order; its button shows the tool in hand. Marks stay on the page (they follow its scroll and zoom) until you comment on them (Comment…: they go to Forge with a picture of the page with them on it, drawn by Chromium, and a list of what each is) or clear them | **Done** (Canvas 4) |
| 11 | Hand | Drag the page to move round it | **Done** (Canvas 4) |
| 12 | Text | A label: click, type, Enter | **Done** (Canvas 4) |
| 13 | Artboard | The page in an artboard: Desktop 1440, Laptop 1280, Tablet 834, Phone 390 or the panel's width; the zoom keeps the artboard's width in view as the panel changes | **Done** (Canvas 4) |
| 14 | Note | A sticky note: click, type, Enter | **Done** (Canvas 4) |
| 15 | Pencil | Freehand strokes | **Done** (Canvas 4) |
| 16 | Pen | A path, a click a point; a double-click or Enter ends it | **Done** (Canvas 4) |
| 17 | Rectangle | A box, dragged | **Done** (Canvas 4) |
| 18 | Oval | A ring, dragged | **Done** (Canvas 4) |
| 19 | Arrow | An arrow, dragged to its tip | **Done** (Canvas 4) |
| 20 | Line | A line, dragged | **Done** (Canvas 4) |
| | **Right toolbar** | | |
| 21 | Play | Present: the page fills the window, back and on a screen, Esc to end (F11) | **Done** (Canvas 1) |
| 22 | Library (book) | The library's 152 design systems, searched; one restyles the page (the request names its DESIGN.md, in the composer) | **Done** (Canvas 4) |
| 23 | Zoom menu: in, out, Fit artboards, 25–200 % | Zoom as a browser zooms (the page lays out wider or narrower): in, out, Fit a desktop width, 25–200 %; Ctrl =, Ctrl -, Ctrl 0 | **Done** (Canvas 1) |
| 24 | Layout guides ✓ | Every box on the page outlined in the theme's accent, kept through reloads and drafts; a Guides chip turns them off elsewhere | **Done** (Canvas 3) |
| 25 | Undo | The page's version before (kept before Forge's first change and after each), written back and reloaded | **Done** (Canvas 3) |
| 26 | Redo | The version after it again; a new change after an undo drops the undone ones | **Done** (Canvas 3) |
| | **Comment box** | | |
| 27 | What it's on ("Canvas") | The part chosen: its tag and the start of its text | **Done** (Canvas 2) |
| 28 | @Claude | @Forge: on, the comment asks Forge to revise the part (sent with its selector); off, it is a note for you | **Done** (Canvas 2) |
| 29 | Queue for Claude | Queue for Forge, while Forge replies: ticked, the comment waits for the reply to end; unticked, Forge reads it at its next step. A stopped reply leaves waiting comments as notes | **Done** (Canvas 2) |
| 30 | Send | Send (or Enter) | **Done** (Canvas 2) |
| 31 | "Claude is working. Your comment goes after that." | The same, for Forge (or "It reads your comment at its next step") | **Done** (Canvas 2) |
| | **Tips** | | |
| 32 | Use @ in a comment to mention someone | @Forge's tip: with it Forge revises the part, without it the comment is a note | **Done** (Canvas 2) |
| 33 | Drag an artboard by its title to move it | Drag a page in the Pages menu onto another to put it before that one | **Done** (Canvas 4) |
| 34 | Comment on any part, and Claude can revise it | Said in the comment box; the tool's own tip says how (point, click, Esc) | **Done** (Canvas 2) |

Zoom works the way a browser's does: at 50 % the page lays out twice as
wide and shows at half size, so a desktop layout fits a narrow panel.
Chromium pictures pages at twice their CSS pixels
(`--force-device-scale-factor=2`), so a zoomed page, and a screen at 150 %,
stay sharp.

## 4. Everything in Open Design, and where it goes

Every part gets exactly one of four outcomes:

- **Port:** becomes Rust in Forge.
- **Keep:** shipped unchanged in the design library (§5.2), because it's
  content for the model, or code that must run in a web page, in Figma, or
  under Python or Node.
- **Covered:** Forge already does it. We take the ideas, not the code.
- **Drop:** can't work inside Forge, because it only talks to Open Design's
  own servers or only builds Open Design's own Node distribution.

### 4.1 Content: Keep, all of it (≈110–160 MB, ≈6,000 files)

| Open Design | Size | Notes |
|:--|:--|:--|
| `skills/` (169) | 6 MB | All of them. **85 are catalogue entries** that point at 33 upstream repositories (fal, Anthropic, OpenAI, Figma, MiniMax, Venice, Composio, Google, …). D1 fetches each upstream skill whose licence allows it into Forge (MIT, Apache, BSD), so the entry becomes the real skill. Where the licence doesn't allow it (Anthropic's `docx`, `pdf`, `pptx`, which aren't open source), the entry stays as it is, a pointer the person can follow, and Forge's own `office-documents` and design export cover the job. |
| `design-systems/` (152 + `_schema`) | 42 MB | All, brand-styled ones included (apple, stripe, nike, …). See §8, risk 1. |
| `design-templates/` (116) | 40 MB | All: html-ppt (36 themes, 15 decks, 31 layouts, presenter mode), course module, digital e-guide, onboarding, reports, dashboards, pitch, product launch, … |
| `plugins/_official/examples/` (188) | 51 MB | 142 overlap `design-templates/` (as plugin packages with an `open-design.json`). 46 are only here (fs-*, hps-*, huashu-*, motion-*, …). For each overlap D1 keeps the newer copy and records the choice. |
| `plugins/_official/design-systems/` (148) | 3.4 MB | Their `open-design.json` manifests, merged with `design-systems/`. |
| `plugins/_official/atoms/` (13) | 180 KB | Prompt bodies: critique theater, direction picker, discovery form, design extract, figma extract, handoff, patch edit, token map, todo-write, … Runtime gets ported (D2/D5). |
| `plugins/_official/scenarios/` (14) | 468 KB | `od-next-strategy` holds the core system prompt and task profiles (ppt, prototype, marketing, motion, hyperframes). There are also code-export scenarios (React, Vue, Next.js), figma migration, design refine, media generation, … |
| `plugins/_official/image-templates/` (45), `video-templates/` (64) | 1 MB, 19 MB | For D6. |
| `plugins/community/`, `plugins/registry/`, `plugins/spec/` | 2.4 MB | Community plugins are installable but not preinstalled. Registry JSON and spec are kept. |
| `craft/` (13) | 140 KB | Also built into the .exe (`assets/forge-plugin`), so they're there even without the library. |
| `prompt-templates/`, `assets/prompt-templates/` | 0.6 MB | Image and video prompt packs (D6). |
| `templates/` (deck-framework, kami-deck, live-artifacts) | 192 KB | |
| `assets/frames/` | 36 KB | Device frames for the panel's width switch (§3). |
| `assets/community-pets/` + `skills/hatch-pet` | 13 MB | D8. |
| `data/plugin-previews/` | | Gallery thumbnails. |

### 4.2 Code that can't become Rust: Keep, and how Forge runs it

| What | Why it stays | How Forge runs it |
|:--|:--|:--|
| `clipper/capture.js` (565 lines, Apache) | Reads a page's live layout. Only code running in the page can. | Rust injects it into the tab over DevTools (`Runtime.evaluate`) and gets back the HTML snapshot and the Figma IR as JSON. |
| `clipper/brand-capture.js` (Apache) | Same reason: reads colours, fonts and the logo from a live site. | Same. Used by "Brand from URL" (D5). |
| `dom-to-pptx` 2.0.1 bundle (MIT, 3.8 MB, includes PptxGenJS) | Turns the rendered page into native, editable PowerPoint shapes. | Rust injects it into the rendered deck and receives the `.pptx` bytes. No Node needed. |
| Template runtimes: html-ppt `runtime.js`, 47 animations and presenter mode; scripts in 60 templates (12 with canvas or animation) | They are the page. | They run as they are in the panel's Chromium. Exports wait for the page to settle and use its final frame. |
| `figma-plugin/` (`code.js`, `ui.html`, `manifest.json`) | Figma only runs JavaScript plugins. | Shipped in the library. The Figma export dialog shows its path and the one-time "Import plugin from manifest" step. |
| Python scripts: `chat-motion-overlay`, `hatch-pet`, `oil-motion`, `pptx-html-fidelity-audit`, `ui-animation`, `last30days` | Agent tools, written in Python. | Kept. The library index marks each skill `needs: python`. Forge labels it, and hides its button when Python isn't on PATH, the same as `office-documents` today. |
| Node scripts: `web-clone`, `hyperframes`, `open-design-landing`, `open-design-landing-deck` | Agent tools, written in Node. | Same, marked `needs: node`. |
| Shell: html-ppt `render.sh`, `new-deck.sh` | | Kept. html-ppt's SKILL.md also points to the Rust `render` tool, which works on Windows. That's a modification, so the file is marked. |

### 4.3 The daemon (`apps/daemon`, ≈306k lines TypeScript)

| Module | ≈Lines | Outcome | In Forge |
|:--|--:|:--|:--|
| `prompts/`, `strategies/` (od-next) | 11k | **Port.** This is "the secret": the system prompt built per run. | `src/design/prompt.rs` |
| `plugins/`, `packages/plugin-runtime`, `registry-protocol`, `registry/` | 17k | **Port:** manifests, resolution, install from folder/git/zip, lockfile, marketplace sources | `src/design/library.rs`, `manifest.rs`; UI in the existing `plugins_editor.rs` |
| `skills.ts`, `plugins/context-craft.ts`, `scripts/lint-craft-references.ts` | 1.5k | **Port** | `library.rs`, `craft.rs`, plus a test |
| `design-systems/` (index, preview, token contract, import from GitHub and shadcn, create) | 11.5k | **Port** | `src/design/systems.rs`, `tokens.rs` |
| `brands/` (a brand from a URL → DESIGN.md + kit) | 14k | **Port**, with `brand-capture.js` kept | `src/design/brand.rs` |
| `artifacts/`, `chat-artifacts/`, `lint-artifact.ts`, `run-deliverable-validation.ts`, stub guard, deliverable-syntax repair | 10k | **Port** | `src/design/artifact.rs`, `lint.rs` |
| `artifact-focus-marker.ts`, `next-step-marker.ts` | 0.7k | **Port** (the "panel follows the agent" behaviour, §3) | `src/design/live.rs` |
| `routes/static-resource.ts` | | **Port** (§3.7) | `src/design/serve.rs` |
| `inline-assets.ts`, standalone HTML | 0.5k | **Port** | `src/design/inline.rs` |
| `deck-export.ts`, export routes; desktop `pdf-export.ts`, `deck-capture.ts`, `artifact-export.ts` | 4k | **Port** | `src/design/export/{pdf,pptx,png,html}.rs` |
| `figma/fig-decode.ts`, `figma-import.ts` (read `.fig`: zip + kiwi + deflate/zstd) | 0.7k | **Port** | `src/design/fig.rs` |
| `design/claude-design-import.ts`, `handoff-design.ts`, `finalize-design.ts` | 1.5k | **Port** | `src/design/handoff.rs` |
| `critique/` (review passes, scoreboard, conformance) | 4.4k | **Port** | `src/design/critique.rs` |
| `genui/` (forms the agent shows: discovery questions, direction picker) | 0.6k | **Port** | `src/ui/design_form.rs` |
| `live-artifacts/` (artifacts refreshed from a data source) | 3.5k | **Port** (D7) | `src/design/live_data.rs` |
| `media/`, `media-adapters/`, media parts of `integrations/` (fal, OpenAI, Google, xAI, ElevenLabs, AIHubMix) | 10k | **Port** (D6): HTTP with `ureq`, keys in `secrets.rs` | `src/design/media.rs` |
| `deploy.ts` (Vercel, Cloudflare Pages) | 2.4k | **Port** (D7) | `src/design/deploy.rs` |
| `library.ts`, `library-store.ts`, `library-sync.ts` (saved page captures) | 1.5k | **Port** (D7) | `src/design/captures.rs` |
| `connectors/` (Composio) | 4.6k | **Port** (D7), as an optional connector with your own Composio key | `src/design/composio.rs` |
| `routines.ts`, `automation-*.ts` | 1.6k | **Port** (D8) | `src/design/routines.rs` |
| `runtimes/` (16 agent CLIs: Claude Code, Codex, Gemini, …), `agent-protocol/`, ACP | 35k | **Covered** by `backend.rs` (ForgeCLI) and `endpoints.rs` (your own key). Running other CLIs as engines is D8, optional. | `backend.rs` |
| `mcp.ts`, `mcp-brief.ts`, `mcp-routes.ts`, `mcp-apps/` (Open Design as an MCP server) | 5k | **Port** as Forge's design tools for its own agent (§5.3) | `src/design_mcp.rs` |
| `mcp-config.ts`, `mcp-oauth.ts`, `mcp-agent-install.ts`, `mcp-bootstrap.ts` | 2.6k | **Covered** (ForgeCLI manages MCP servers) | |
| `memory*.ts` | 4k | **Covered** | `project_memory.rs` |
| `project-file-versions.ts` | 0.8k | **Covered** | `doc_versions.rs` |
| `terminals.ts`, `node-pty.ts` | | **Covered** | `ui/terminal.rs` |
| `document-preview.ts` | 0.3k | **Covered** | `office_render.rs` |
| `tool-loop-guard.ts`, `run-retry-policy.ts`, `run-failure-classification.ts` | 2.6k | **Port** the failure messages and loop guard where ForgeCLI doesn't already | `src/design/run_guard.rs` |
| `langfuse-*.ts`, `observability/` | 11k | **Port** (D8), optional: send traces to *your own* Langfuse | `src/design/trace.rs` |
| `server.ts`, `routes/`, `http/`, `cli.ts`, `db.ts` (SQLite), `storage/`, `migration/`, `sidecar/` | 75k | **Covered.** Forge is one process with files, so Rust calls replace HTTP routes. The `od` CLI's design commands become the design MCP tools. | |
| `collab/`, `team-*`, `social-share.ts`, `integrations/vela*`, AMR cloud, billing, wallet, `xai-oauth` | 30k | **Drop.** Needs Open Design's servers and accounts. | |
| `telemetry`, `analytics.ts`, `run-analytics-*`, `prompt-telemetry.ts`, diagnostic relay | 5k | **Drop.** Reports to Open Design. Forge has `diagnose.rs`. | |

### 4.4 The web app (`apps/web`, ≈456k lines React) → GPUI

React doesn't translate. Each surface is redesigned natively in Forge's look,
with Forge's tokens.

| Open Design surface | Outcome | Forge file |
|:--|:--|:--|
| Live file preview (`FileViewer`, `FileWorkspace`, `DesignFilesPanel`, `DesignBrowserPanel`) | **Port** (§3) | `ui/design_panel.rs` |
| `DeckThumbnailRail`, `DeckSlideThumbnail` | **Port** | `ui/deck_rail.rs` |
| Home hero + chip rail (deck, prototype, document, image, video, HyperFrames, audio) | **Port** | `ui/design_home.rs` |
| Examples / Designs / Community tabs | **Port** | `ui/design_gallery.rs` |
| Design systems: tab, picker, switch picker, preview modal, create flow, kit view, spec view, asset dropzone | **Port** | `ui/design_systems.rs`, `ui/design_system_view.rs` |
| Brands: tab, picker, preview card, enrichment | **Port** | `ui/design_brand.rs` |
| `edit-mode/`, `comments/` (pick an element, comment, the agent redoes it) | **Port.** DevTools inspect mode picks the element. | `ui/design_comments.rs` |
| Figma import and help modals | **Port** | `ui/figma_import.rs` |
| Handoff, Finalize, export menu | **Port** | `ui/design_export.rs` |
| Critique theater | **Port** | `ui/design_critique.rs` |
| GenUI inbox and surfaces | **Port** | `ui/design_form.rs` |
| `media/` views | **Port** (D6) | `ui/design_media.rs` |
| Composer mode picker, plugin preview, context chips | **Port** into Forge's composer | `ui/composer.rs` + `ui/design_chips.rs` |
| Chat pane, messages, settings, onboarding, connectors browser | **Covered** by Forge's own | |
| `i18n/` (UI in several languages) | **Port** (D8): the strings into a Rust table, English first | `src/i18n.rs` |
| AMR, Vela, billing, campaign, survey and sign-in dialogs | **Drop** (Open Design's cloud) | |

### 4.5 Desktop, packages, tooling

| Open Design | Outcome |
|:--|:--|
| `apps/desktop` (Electron) | **Covered:** Forge is the desktop app. Its export code is ported (§4.3). |
| `packages/contracts` | **Port** the types Forge uses (manifests, Figma IR, export inputs) as serde structs. |
| `packages/plugin-runtime`, `registry-protocol` | **Port** (§4.3) |
| `packages/diagnostics` | **Covered** (`diagnose.rs`) |
| `packages/components` (React), `agui-adapter`, `dsh-runtime`, `host`, `platform`, `sidecar(-proto)`, `launcher-proto`, `standalone`, `download`, `release`, `metatool`; `apps/packaged`, `apps/closure`, `shells/terminal` | **Drop:** they build and launch Open Design's own Node app. Forge's equivalent is `cargo` + `installer/`. |
| `tools/dev`, `pack`, `release`, `serve` | **Drop** (replaced by `installer/build.sh`) |
| `scripts/` | **Port** the content checks Forge needs (craft references, design-system manifests, component fixtures) as Rust tests. Drop the rest (GitHub workflow helpers). |
| `e2e/`, `mocks/` | **Covered:** Forge's Rust tests and `FORGE_DEMO=1` (extended with a scripted design reply). |
| `deploy/` (Docker, AWS, Azure), `charts/` (Helm) | **Drop:** server hosting for Open Design. |
| `clipper/` (Chrome extension) | **Covered** by Forge's own browser panel. `capture.js` and `brand-capture.js` are kept (§4.2). |
| `docs/`, `specs/`, `story/`, `ui-audit/` | Reference while porting. Not shipped. |

### 4.6 What is left out, all of it

Only these, because they can't work without Open Design's own services, or
only exist to package Open Design:
- team collaboration, sharing and billing;
- AMR / Vela cloud;
- telemetry to Open Design;
- its Node, Electron and Docker distribution tooling.

Everything else is ported, kept or already in Forge.

## 5. How it fits together

```
 You ──► Forge (Rust, GPUI)
          ├─ ui/design_*.rs ............ design home, gallery, systems, brand, export, comments
          ├─ ui/design_panel.rs ........ the live right panel (§3)
          ├─ design/prompt.rs .......... builds the system prompt for each run
          ├─ design/library.rs ......... indexes the design library (§5.2)
          ├─ design/serve.rs ........... loopback page server for the panel
          ├─ design/export/* ........... PDF, PPTX, PNG, single HTML, Figma IR
          ├─ design_mcp.rs ............. design tools for the agent (§5.3)
          └─ web_view.rs (exists) ...... Chromium over DevTools: preview, screenshots, PDF, capture
                 │
                 ▼
          ForgeCLI (the agent) ◄── --append-system-prompt (composed)
                                ◄── --plugin-dir (forge plugin: design skill + craft)
                                ◄── design MCP tools
```

### 5.1 The prompt (the "secret")

`design::prompt::compose(surface, system, template, skill)` ports Open
Design's `prompts/system.ts` and the od-next strategy. It concatenates, in
order:
1. the core design charter;
2. the task profile for the surface (deck, prototype, marketing, motion,
   HyperFrames, document);
3. the chosen design system's `DESIGN.md` and tokens;
4. only the craft sections the skill lists under `od.craft.requires`;
5. the skill;
6. the template, as the starting file.

Stable parts come first so prompt caching holds. Golden-file tests pin the
output for every surface, and a token budget is tested per surface.
`backend.rs` passes the result with `--append-system-prompt` for design
chats. The flag is already in use.

### 5.2 The design library on disk

- Everything in §4.1 and §4.2 lives in a `design/` folder.
- The installer puts it in `$INSTDIR\design\`, beside `fonts\` and `bin\`
  (`installer/forge.nsi` already ships folders that way).
- Development uses `FORGE_DESIGN_LIB`, or the synced copy in the repo.
- `design/library.toml` lists what's taken from upstream, at which commit,
  and any local changes.
- `scripts/sync-open-design.sh` rebuilds the folder from that list:
  - it fetches the 85 upstream skills where their licences allow it;
  - it writes `THIRD_PARTY_LICENSES.md`;
  - it dedupes the examples.

### 5.3 The design tools for the agent (`forge-gpui --design-mcp`)

These replace Open Design's `od` CLI. They're an MCP server served by the same
.exe, like `browser_mcp.rs`, so nothing has to be installed:

| Tool | Does |
|:--|:--|
| `design_search`, `design_read` | Find and read skills, templates, design systems and craft |
| `design_render` | Screenshots of the page or of slides n..m, from the panel's own tab |
| `design_lint` | Open Design's artifact checks: stubs, broken syntax, missing assets, overflow |
| `design_export` | `pdf`, `pptx` (editable or image), `png`, `html` (single file), `figma` |
| `design_capture` | A URL → HTML snapshot, Figma IR, or brand kit |
| `design_tokens` | Read or suggest a project's tokens |
| `design_ask` | Show a GenUI form in Forge (discovery questions, direction picker) |

### 5.4 Exports

| Format | How |
|:--|:--|
| PDF (page) | DevTools `Page.printToPDF` |
| PDF (deck) | Each slide at 1920×1080 through `printToPDF`, with the page size set; joined with `lopdf` (already a dependency) |
| PPTX, editable | `dom-to-pptx` injected into the rendered deck (§4.2) |
| PPTX, picture per slide | Rust writes it with `zip` + `quick-xml` (both already dependencies) |
| PNG | `Page.captureScreenshot` per slide or page |
| Single HTML | `design/inline.rs` inlines CSS, fonts and images |
| Figma | `capture.js` → IR → `.od-figma.json` → the kept Figma plugin |
| Import `.fig` | `design/fig.rs` |
| Code (React, Vue, Next.js) | The kept export scenarios, run by the agent |

Every export opens in the panel afterwards (`office_render.rs` already shows
PPTX and PDF), so you check the result in Forge.

## 6. Phases

Each phase ends green (`cargo build` with zero warnings, `cargo test`), with
screenshots in light and dark and a short report in `docs/` (as P1–P3 did).

**D1: The library and licences**
- `design/library.toml`, `scripts/sync-open-design.sh`, the synced library.
- `THIRD_PARTY_LICENSES.md` and a licence test.
- `src/design/{manifest,library,craft}.rs`: parse every `SKILL.md`
  front-matter, `open-design.json`, `DESIGN.md` and `design-tokens.json`.
- The 85 upstream skills fetched where their licence allows. A table records
  each one and its licence.
- Tests:
  - every item parses;
  - every craft reference resolves;
  - counts match `library.toml`.
- The installer ships `design\`.

**D2: Prompt, design tools, and the live panel (§3)**
- `design/prompt.rs`, `design_mcp.rs` (search, read, render, lint),
  `design/serve.rs`, `design/live.rs`, `ui/design_panel.rs`, `ui/deck_rail.rs`.
- A design chat: a toggle in the composer, plus chips for the surface, design
  system and template.
- Forge's own `design` skill in `assets/forge-plugin`.
- `FORGE_DEMO=1` gets a scripted design reply that streams an HTML deck, so
  the panel can be checked without a model.
- Acceptance: §3's list, on Windows and Linux.

**D3: Exports**
- PDF, PPTX (editable and picture), PNG, single HTML.
- Tests:
  - a PDF's page count equals the slide count;
  - a PPTX reads back (zip + XML) and renders in the panel;
  - the HTML has no external references.
- Windows: open the PPTX in PowerPoint.

**D4: Figma**
- `capture.js` → IR (validated against `figma-plugin/IR.md`), the
  `.od-figma.json` export, and the plugin shipped with its install steps.
- `.fig` import (`design/fig.rs`).

**D5: Design screens**
- Design home and chip rail.
- Gallery.
- Design systems: picker, view, create, import from GitHub and shadcn.
- Brand from URL.
- Element comments (edit mode).
- Critique.
- GenUI forms.
- Handoff and Claude Design import.
- Every action in the palette, with a key binding where it fits.

**D6: Media**
- Image, video and audio providers (fal, OpenAI, Google, xAI, ElevenLabs,
  AIHubMix) with your own keys; prompt templates; image and video templates.
- HyperFrames video export needs `ffmpeg`. Like Python, it's used when
  present, and Forge says so when it isn't.

**D7: Extras**
- Live artifacts, deploy to Vercel or Cloudflare Pages, saved page captures,
  Composio connector, installing community plugins.

**D8: The rest**
- Routines and automations, interface translations, Langfuse traces, other
  agent CLIs as engines, pets.

## 7. Decisions (made)

- **Use all of it.** Every skill, template and design system, brand-styled
  ones included, plus the 85 catalogue skills, filled in from upstream where
  their licences allow.
- **No Node.js at runtime.** Python and ffmpeg are optional, used when present.
- **Base:** built on `claude/wonderful-pasteur-1b732o`, pushed to
  `claude/laughing-albattani-7lfjod`.

Still open: whether the 110–160 MB library is committed to this repo
(simple, but the repo grows from 15 MB) or synced at build time from the
pinned commit (the repo stays small, but the build needs the network once).
**Recommended: synced at build time,** with a cached copy for offline builds.

## 8. Risks

1. **Brand-styled design systems.** apple, nike, stripe, tesla and others are
   named after real companies. Shipping them in a closed-source product
   carries trademark risk. They're included as decided, but they could be
   labelled "inspired by" or renamed later if that becomes a concern.
2. **Upstream licences.** Some of the 85 upstream skills may not allow
   redistribution. Those stay as pointers. D1's table makes this visible.
3. **Editable PPTX fidelity.** It's limited by `dom-to-pptx`. Web fonts must
   exist where the deck is opened. CSS filters and blend modes fall back to a
   picture.
4. **No Chromium.** Edge can be removed in the EU. `web_view.rs` already
   falls back to Chrome or Playwright's Chromium and says when there's none.
5. **Installer size** grows by about 70–100 MB, compressed.
6. **Upstream moves fast** (daily commits). We stay pinned, and re-syncing is
   a reviewed change.
7. **Prompt size.** Only what the run needs is composed, and a budget is
   tested per surface.
