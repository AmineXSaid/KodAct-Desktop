# Open Design in Forge: D3 to D7 report

What Open Design does, where it is in Forge now, and how fast. All of it was
run on Linux (headless Chromium 1194, the demo deck of six slides) and checked
in screenshots in light and dark. Nothing here has been run on Windows yet;
`cargo check --target x86_64-pc-windows-gnu` passes.

## Open Design's features, and Forge's

| Open Design | In Forge | Where |
|:--|:--|:--|
| The library: skills, templates, examples, design systems, craft | All of it, indexed: 169 skills, 126 templates, 188 examples, 152 design systems, 11 craft rules | `design/library.rs`, `design-library/` |
| Home gallery to start from | Design screen: searchable cards with pictures, Preview beside it, Use starts a set-up design chat | `ui/design_home.rs` (Ctrl/⌘ Shift D) |
| Design chat setup: surface, design system, template, skill | The composer's design chip | `ui/design_chips.rs` |
| The design prompt | Charter, task profile, design system, craft, skill, template | `design/prompt.rs` |
| `od` tools for the agent | `design_search`, `design_read`, `design_render`, `design_lint`, `design_export`, `design_capture` | `design_mcp.rs` |
| Live preview while it designs | The side panel shows each page as it is written | `design/live.rs`, `web_view.rs` |
| Comments on any part, drawing on the page | The design bar's comment tool, pins, marks | `ui/design_bar.rs`, `ui/design_comment.rs`, `ui/design_marks.rs` |
| Critique | "Ask Forge to review the page": the five-part review, scored and fixed | Comments menu, palette |
| Brand from URL | `design_capture`: a site's colours, type, corners, button, logo into `brand/<host>/DESIGN.md` and `tokens.css` | `design/capture.rs`; gallery "From a website…" |
| Export PDF | A deck a slide a page | Share menu |
| Export PNG | The page, or every slide | Share menu, `design/export.rs` |
| Export PPTX (pictures and editable) | Written from scratch; editable text boxes over a text-free picture; speaker notes | `design/pptx.rs` |
| Export single HTML | Styles, imports, scripts, pictures, fonts and media inline | `design/inline.rs` |
| Figma | `.od-figma.json` from the library's clipper, for the shipped plugin | Share menu, `design/export.rs` |
| `.fig` import | Pages, frames, fills, strokes, corners, shadows, text, pictures and vector paths (as SVG) rebuilt as HTML | `design/fig.rs`; gallery "Open a Figma file…", `design_import_fig` |
| GenUI forms (discovery questions, direction picker) | Forge's questions as a clickable form; answers go back to the agent | `ui/question_form.rs` |
| Media: images, video, audio | `design_image` (OpenAI or fal), `design_media` (fal video, ElevenLabs narration), your own keys | `design/services.rs` |
| Deploy | `design_deploy`: Vercel or Cloudflare Pages (its direct upload), one self-contained file | `design/services.rs` |
| Deck thumbnail rail | A deck's outline shows its slides as pictures; a click goes to the slide | `ui/browser_view.rs`, `export::slide_thumbs` |
| Versions, undo and redo of a page | Kept per page | design bar |
| Present | The page fills the window, slide by slide | F11 |

## Speed

| What | Time |
|:--|:--|
| First export, starting its browser | 0.34 s |
| A deck as PPTX, PDF or PNGs, warm | about 0.5 s each |
| Editable PPTX of six slides, from the app | 0.5 s |
| Single HTML file | a few ms |
| Figma capture, warm | 0.12 s |
| A gallery picture | 0.16 s, then cached (instant) |
| `design_render`, six slides | 3.7 s → 0.53 s |
| `design_render`, a page | 0.81 s → 0.26 s |

Exports and pictures run in a headless Chromium of Forge's own. It starts when
the Share menu or the gallery opens and stays up, so the panel the user
watches never changes. It closes with the app; that was checked by looking
for its processes after quitting. A deck is laid out once as for print, and
every slide is captured where it lies: no stepping through slides, no waiting
on their animations. `design_render` now waits for fonts and two drawn
frames instead of fixed sleeps, and pictures slides with their transitions
finished at once.

## Checked

- `cargo build` with no warnings. `cargo test`: 437 pass. The browser tests
  (`cargo test -- --ignored design::export`) export every format, check that
  the PDF's page count equals the slide count, read back the PPTX's slides
  and text boxes, check the HTML for links outside the file and the Figma IR
  for pictures still linked, and time a warm run.
- The editable PPTX, drawn by LibreOffice, has its text where the slide had
  it, over a picture without the text.
- In the app: the Share menu's exports and their toasts, the gallery
  (pictures fill in, Preview, Use, dark mode), the review row and the message
  it sends.

## Not done or not checked

- **Windows:** not run there (`cargo check` for Windows passes). This container
  has no Windows or Wine, and GPUI draws with DirectX there.
- **A real model:** design chats were checked at the wire (the prompt and
  tools ForgeCLI is given, the answers they return) and with the demo. They
  weren't driven by a model here: the only model access in this container
  is the session's own, which isn't Forge's to use.
- **Real outside services:** OpenAI, fal, ElevenLabs and Vercel were
  checked against mock servers that answer as their APIs do, not the
  services themselves. That needs your keys.
- **Brand from a live site:** it loaded github.com live, but this
  sandbox's proxy blocks the site's stylesheet host. Capture now detects
  that and warns, rather than reporting an unstyled page as the brand.
- **Real Figma and PowerPoint files:** `.fig` import is tested on files
  the tests write, not one saved by Figma. The PPTX hasn't been opened in
  PowerPoint, and the Figma export hasn't been imported in Figma.
- **Not built:** Open Design's routines, translations and pets (D8).
- **Charts:** a chart drawn only on the slide that shows still prints empty
  on the other slides.
