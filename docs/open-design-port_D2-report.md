# D2 report: the live panel, the design canvas, the prompt and the tools

Done on `claude/laughing-albattani-7lfjod`, after D1 (`0679758`), in 22
commits from `b300b07` (the live panel) to `d1eb4fd` (the code review's
fixes). The plan is `docs/open-design-port.md`: §3 for the panel and its 34
canvas controls, §5.1 for the prompt, §5.3 for the tools.

**Everything was run and looked at on Linux only. Nothing was run on
Windows.** The whole app type-checks for Windows
(`cargo check --target x86_64-pc-windows-gnu`).

The gates, on the final commit:
- `cargo build`: 0 warnings.
- `cargo test`: 432 passed (14 ignored, among them an SSH test run by hand,
  below).
- `cargo clippy --all-targets`: only the 4 findings that were on the branch
  before D2 (`state.rs`, `ui/doc_changes.rs`, `ui/doc_pane.rs`,
  `ui/sheet_view.rs`).
- `/visual-critique:critique-screen` run on the new screens; its P2 items
  and one P3 are fixed (below).
- A code review of D2's design work (`/code-review`, medium) found 10
  problems; all 10 are fixed, with its two cleanups.

## What a person gets

**The live panel.** When Forge writes an HTML page the side panel opens on
it, shows it forming as it is typed, and loads the finished file. A badge
says "Forge is designing…" with Stop following or Show.

**The design canvas** (§3.1): all 34 of Claude's canvas controls, each in
its HTML form. Among them: the outline, pages, comments with pins, the
Insert tools and their marks, artboards, the Library, zoom, layout guides,
undo and redo, Present and Save as PDF. New in this round:
- **Decks:** "‹ 2 / 6 ›" in the design bar, and Alt ↑ / Alt ↓ (and the
  palette) step the slides. Stacked decks are scrolled; decks that show one
  slide at a time (html-ppt) are driven by their own arrow keys.
- **Save as PDF** prints a deck one slide a page, at 16:9 (1920×1080 for a
  fixed-size deck, 1280×720 for one sized to the window).
- **Comments and marks follow their part.** A zoom, a wider panel or an
  artboard lays the page out again; pins and marks now stay on what they
  were about (each is anchored to its part's content).
- **Comments, marks, page order and added pages survive a restart.** A
  page's versions (for undo) are kept in a file of their own.

**Design chats.** The composer's design chip (beside the project) makes a
chat a design chat and sets it up. Its tabs pick the kind of work (deck,
prototype, document, marketing, video, motion, or any), the look (152
design systems), the template and the skill, with a search. A design chat's
ForgeCLI gets:
- **the design prompt** (`design/prompt.rs`): Forge's charter, then Open
  Design's task profile, kept as written, then the design system, the craft
  rules, the skill and the template, each within a budget;
- **the design tools** (`design_mcp.rs`): `design_search`, `design_read`,
  `design_render` (the page, or slides n–m, from the panel's own tab) and
  `design_lint` (placeholders, a file cut short, broken markup, missing
  files, unpinned CDN links, title and viewport; and in the page, overflow
  and clipped text).

This works in remote projects too (WSL, SSH, containers). There the prompt
goes in through the boot script, and the tools reach the remote over HTTP.

**Forge's `design` skill** (`assets/forge-plugin/skills/design`) covers
design requests in any chat. The surface prompt sends visual work to it.

## How it was checked

| What | How | Seen |
|:--|:--|:--|
| Prompt and tools reach the model | The real ForgeCLI, pointed at a stand-in Messages API that records each request | The surface prompt, the prompt file and all four `mcp__forge-design__*` tools in the request |
| Render and lint | The tools server driving headless Chromium | The right slides in a stacked deck and in html-ppt; lint found all 5 problems on a broken page |
| Remote design chats | A real SSH server on localhost | Lint (a missing file found, a present one passed), render from the copy, and a 144 KB prompt written there and reaching the model intact |
| Saving | Comment, draw, quit, start again | The pin and marks back on their parts |
| Following parts | Box round the title at 100%, then 50, 80, 125, 150% and the Phone artboard | The box frames the title every time, on two lines when it wraps |
| Deck PDF | The demo's deck and html-ppt's demo deck | 6 and 8 pages, one slide each; the page unchanged after |
| Undo, then a new edit from Forge | A changed page, Forge's rewrite, undo, another rewrite, redo, undo | As an editor does: the undone version dropped, nothing to redo |
| The canvas paths left untested before | Hand and outline at 50%, Esc out of each tool, guides while a page streams in, an artboard through a window resize | All work, after the fixes below |

Screenshots in light and dark were checked for the chip and its menu, the
composer tray beside the panel, the design bar, marks, the PDF pages and the
outline.

## Bugs found by running it, fixed

- **The panel's browser could outlive the app** (from before D2). Closing it
  killed the guard before the guard could stop Chromium. The leftover
  Chromium held its profile, so the next start failed with "the browser did
  not start".
- **Marks in dark mode read soft** on light pages. A new `page_ink` token
  (6.4:1 on its halo, the same in both modes) and a white halo fix it.
- **The design chip pushed the mode button out of the composer** beside the
  panel. The tray now gives up room as ellipses.
- **Pins and marks stayed put when the page laid out again.** They are now
  anchored to their part.
- **A stroke crossing a comment pin stopped there**, and letting go over the
  pin left the shape half-drawn and unsaved.
- **Esc didn't put a tool down** after it was picked from its menu.
- **The outline landed a slide off**, because opening it narrows the page.
- **A first click with the comment tool could open nothing.** This was the
  miss the last report couldn't explain: a click on the page's background
  found no part. It now comments on the page as a whole.
- **Demo replies never said when their turn began**, so undo had nothing to
  go back to in demo chats.

## Still open

- **Windows:** never run there. It still needs checking for sharp frames at
  150%, the cost of rendering at double resolution, the browser window's
  title-bar height, the composer tray at Windows font widths, and the remote
  boot script under WSL.
- **A real model:** the prompt and tools were checked at the wire (what the
  model is sent), not by a model designing a page. A model-made deck's
  quality, and the critique loop the charter asks for, are still to see.
- **Not checkable here:** Show in folder, Open in your browser and Copy
  path need a desktop.
- **Remote design pages** aren't shown live in the panel: the live view
  follows local files. `design_render` shows a copy, without the files
  beside it on the remote.
- **The deck thumbnail rail** (§3.8) waits for D5's design screens. Slide n
  of N and back and on are done.
- **A chart on a hidden slide** (html-ppt's chart slide) prints empty in the
  PDF, because the chart library only draws on the slide that shows.
- The plan's other tools (`design_tokens`, `design_export`,
  `design_capture`, `design_ask`) and D3–D8 are ahead.
