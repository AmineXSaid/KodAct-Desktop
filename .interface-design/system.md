# Design System: Forge desktop (forge-gpui)

> Generated from `src/theme.rs` by `src/system_md.rs`. Change a token in theme.rs, then run
> `UPDATE_SYSTEM_MD=1 cargo test system_md`; `cargo test` fails while this file is stale.
> Every colour pair below is held to its floor by `src/palette_check.rs`.

## Direction

**Personality:** Warmth & Approachability, with the Precision & Density of a desktop tool. A calm room to build in with an agent.

**Foundation:** warm. Paper surfaces in one warm hue, chroma under 0.01 (OKLCH); colour is kept for meaning: the one blue action, modes, status.

**Depth:** borders-only for content (1px hairlines); shadows only on popovers, menus and the command palette.

## Tokens

### Spacing
Base: 4px
Scale: 4, 8, 12, 16, 24, 32

### Radius
Scale: 4px (small), 8px (buttons, rows), 12px (cards, popovers), 16px (the composer), pill

### Typography
Interface: Hanken Grotesk (Anthropic Sans when installed or in Forge's `fonts` folder; never bundled)
Display: Source Serif 4 Display (the greeting)
Code: JetBrains Mono
Scale: 12 caption, 13 UI and chat, 14 input, 18 subheading, 20 heading, 28 greeting
Weights: 400, 500 (labels, buttons), 600 (headings, the project name)
Prose line height: 1.4285715

### Icons
Lucide, 2px stroke as drawn. Sizes: 12 / 14 / 16 / 20 only.

### Sizes
- `titlebar`: 36px, the title bar's height
- `sidebar`: 240px, the sidebar's width
- `column`: 680px, the reading column
- `composer_width`: 560px, the composer's widest
- `row`: 30px, sidebar rows, menu items
- `control`: 26px, small buttons, pills, title-bar controls
- `palette`: 560px, the command palette's width
- `hairline`: 1px, every border
- `doc_pane`: 460px, the document panel's narrowest (it takes half the session)
- `doc_measure`: 680px, a document's widest line
- `terminal_text`: 13px, the terminal dock's monospace
- `terminal_min`: 280px, the terminal dock's narrowest
- `edge_bar`: 3px, the jewel edge a window draws as it opens

### Surfaces
One ladder, darkest at the top of the window, even steps in OKLab lightness.

| Token | Light | L | Dark | L | Role |
|---|---|---|---|---|---|
| `chrome` | `#e9e6e1` | 0.926 | `#100f0e` | 0.169 | the title bar |
| `pane` | `#f1efeb` | 0.953 | `#161514` | 0.196 | the sidebar |
| `stage` | `#f9f7f5` | 0.977 | `#1c1b19` | 0.222 | the session: chat, welcome, Rules, Projects |
| `surface` | `#ffffff` | 1.000 | `#22211f` | 0.248 | cards, the composer, popovers |
| `selected` | `#ffffff` | 1.000 | `#2a2927` | 0.281 | the selected sidebar row |

### Terminal
The dock's 16 colours, on `stage` (`palette_check.rs` holds them to AA).

| # | Light | Dark |
|---|---|---|
| 0 | `#2b2a28` | `#3a3936` |
| 1 | `#b8322a` | `#ff7b72` |
| 2 | `#2a7a35` | `#7ee08f` |
| 3 | `#845d00` | `#f2cc60` |
| 4 | `#1f5fbf` | `#79b8ff` |
| 5 | `#8a3fa8` | `#d2a8ff` |
| 6 | `#1d737c` | `#76e3ea` |
| 7 | `#6d6a65` | `#d6d3ce` |
| 8 | `#5c5a56` | `#6e6c68` |
| 9 | `#c23d30` | `#ffa198` |
| 10 | `#2e7f38` | `#a5eda9` |
| 11 | `#8d6400` | `#f8e08e` |
| 12 | `#2f6fd1` | `#a5d0ff` |
| 13 | `#9c4fbb` | `#e2c5ff` |
| 14 | `#1e7a84` | `#b3f0ff` |
| 15 | `#3d3b38` | `#ffffff` |

### Colors
Contrast is the WCAG ratio of text on `stage`, of a colour on `surface`, or of white text on a fill.

| Token | Light | Contrast | Dark | Contrast | Role |
|---|---|---|---|---|---|
| `text_strong` | `#000000` | 19.7:1 | `#f6f5f4` | 15.8:1 | headings, names |
| `text` | `#000000 @ 95.0%` | 18.3:1 | `#f6f5f4 @ 92.0%` | 13.5:1 | body text |
| `text_body` | `#615d59` | 6.1:1 | `#b5b0aa` | 8.0:1 | secondary prose |
| `text_muted` | `#000000 @ 60.0%` | 5.6:1 | `#f6f5f4 @ 60.0%` | 6.4:1 | labels, captions, icons |
| `text_subtle` | `#000000 @ 40.0%` | 2.8:1 | `#f6f5f4 @ 40.0%` | 3.6:1 | placeholders and disabled only |
| `border` | `#000000 @ 8.0%` | 1.2:1 | `#ffffff @ 9.0%` | 1.3:1 | hairlines |
| `primary` | `#0075de` | 4.6:1 | `#2b8ff0` | 4.8:1 | links, the one accent |
| `fill` | `#0075de` | 4.6:1 | `#0069c2` | 5.5:1 | filled buttons (white text on it) |
| `fill_danger` | `#a32c12` | 7.2:1 | `#b7331f` | 6.0:1 | destructive confirms |
| `success` | `#1f7a35` | 5.4:1 | `#6fd389` | 8.7:1 | done, passed |
| `danger` | `#c42a14` | 5.7:1 | `#ff8a78` | 7.0:1 | failed, errors |
| `warning` | `#8a5600` | 6.2:1 | `#ffc85a` | 10.5:1 | needs you |
| `mode_plan` | `#1f75cb` | 4.7:1 | `#428fdc` | 4.7:1 | Plan mode |
| `mode_bypass` | `#a32c12` | 7.2:1 | `#ec5941` | 4.7:1 | Bypass permissions |

### Contrast floors
- Meaningful text: 4.5:1 on every surface and hover wash.
- Icons, mode edges, the send arrow: 3:1.
- Placeholders and disabled text (`text_subtle`): 2.5:1, never body copy.
- Hairlines: 1.15:1 on every surface.
- Surface steps: at least 0.02 OKLab L apart, each within 0.004 of the mean step.

## Patterns

### Button Primary
- Padding: 6px 15px
- Radius: 8px
- Font: 13px, 500 weight
- Background: `fill`, `fill_hover` on hover; text `primary_fg`
- Usage: one per surface (Allow, Continue, Save, Create)

### Button Quiet
- Height: 26px
- Padding: 0 8px
- Radius: 8px
- No fill until hover (`hover` wash)
- Usage: the model and mode under the composer, title-bar icons

### Composer
- Width: up to 560px, centred
- Radius: 16px; 1px `border`, tinted by the mode when one is set
- Text: 14px, inset to start where the + glyph starts
- A soft rule, fading at both ends, between the text and its tools
- Tools: + (16px glyph) left, a 26px round send button right, 7px from both edges of its corner
- Under it: the context ring (14px, 2px stroke) and its percent, then outlined 24px chips for the model and the mode

### Card Default
- Border: 1px `border`
- Padding: 24px
- Radius: 12px
- Background: `surface`
- Usage: tool calls, file cards, code blocks

### Popover
- Border: 1px `border`, radius 12px, padding 4px
- Background: `surface`, with the two-layer popover shadow
- Usage: menus, pickers, the command palette

### File Card
- A file Forge made, in its reply: up to 440px, `surface`, 1px `border`, radius 12px, padding 8px
- A 36x44 page tile (radius 4px, concentric with the card) previews it, its type in mono `primary`
- Title 600 `text_strong`; "Document · MD" caption `text_muted`; a bordered Download button
- Open: the edge turns `primary` at 60%

### Document Pane
- Beside the conversation, half the session, at least 460px; `surface` with a hairline on its left
- Header 48px: file icon, title, type; Copy, Download, Close as 26px quiet buttons
- Text 15px at 1.65 leading, lines up to 680px; the title in the display serif at 28px
- Enters with a 220ms ease-out, fading in from 16px to the right; Escape closes it

### Sidebar Row
- Height: 30px, radius 8px
- Selected: `selected` with a hairline, 500 weight
- Group labels: 12px caption

## Decisions

| Decision | Rationale | Date |
|----------|-----------|------|
| Borders-only depth | A desktop tool read for hours: hairlines separate without adding weight; shadows are kept to show what floats. | 2026-09-28 |
| Three surfaces, one step apart | Title bar, sidebar and session must read as separate areas at a glance; even OKLab steps make the split clear without contrast jumps. | 2026-10-12 |
| A deeper fill for dark buttons | White on the dark theme's link blue is 3.3:1; the fill is the same hue at 5.5:1 and links keep the brighter blue. | 2026-10-12 |
| Display serif for one line per view | The greeting, and a document's title: one warm, human line; everything else stays in the interface face. | 2026-10-12 |
| Composer at 620px | A comfortable measure for a prompt, the same in a chat and on the welcome page. | 2026-10-11 |
| Icons on a 12/14/16/20 scale | Desktop density: 14 in rows, 16 in controls, 20 for the composer's +; 12 inside captions. | 2026-10-12 |
