# Design System: Forge desktop (forge-gpui)

> Generated from `src/theme.rs` by `src/system_md.rs`. Change a token in theme.rs, then run
> `UPDATE_SYSTEM_MD=1 cargo test system_md`; `cargo test` fails while this file is stale.
> Every colour pair below is held to its floor by `src/palette_check.rs`.

## Direction

**Personality:** Warmth & Approachability, with the Precision & Density of a desktop tool. A calm room to build in with an agent.

**Foundation:** warm ivory. Paper surfaces in one warm hue, chroma under 0.01 (OKLCH); warm ink for text; one patina-teal accent (forged copper as it weathers, deliberately not an orange), the same colour in both modes, kept for sending, the mark's core and a few dots; colour otherwise only for meaning: modes, status, a switch that is on.

**Depth:** borders-only for content (1px hairlines); shadows only on popovers, menus and the command palette.

**Shape:** softly squared. Buttons 7-9px, cards and menus 10px, large inputs 12-14px; pills only for switches, badges and avatars; the round new-chat mark is the one circular control.

## Tokens

### Spacing
Base: 4px
Scale: 4, 6, 8, 10, 12, 16, 20, 24, 32, 40

### Radius
Scale: 1px (a line's ends: every straight line is rounded just slightly), 4px (key caps), 6px (a selected segment), 7px (compact buttons, chips), 8px (buttons, icon buttons, rows), 9px (outlined squares, send), 10px (cards, tool rows, menus), 12px (large fields), 14px (the composer), pill (switches, badges, avatars)

### Typography
Interface: Hanken Grotesk (Anthropic Sans when installed or in Forge's `fonts` folder; never bundled)
Display: Source Serif 4 Display (the greeting)
Code: JetBrains Mono
Scale: 12 caption, 13 UI and chat, 14 input, 18 subheading, 20 heading, 28 greeting, 32 home heading
Weights: 400, 500 (labels, buttons), 600 (headings, the project name)
Prose line height: 1.4285715

### Icons
Lucide, 2px stroke as drawn. Sizes: 12 / 14 / 16 / 20 only.

### Sizes
- `titlebar`: 36px, the title bar's height
- `sidebar`: 256px, the sidebar's width
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
- `chat_min`: 400px, the chat's narrowest: the side panel, then the dock, give way to it
- `remote_menu`: 288px, the remote quick pick's width
- `edge_bar`: 3px, the jewel edge a window draws as it opens
- `hit_sm`: 28px, an icon button in a dense row
- `hit`: 34px, an icon button's square
- `hit_lg`: 38px, the composer's outlined + and send
- `button_h`: 32px, a labelled button
- `button_sm`: 28px, a compact button, a selector
- `switch_w`: 36px, a switch's track (20px tall, 14px thumb)
- `rail`: 52px, the collapsed sidebar
- `dot_gap`: 14px, the home screen's dot grid

### Surfaces
One ladder, darkest at the top of the window, even steps in OKLab lightness.

| Token | Light | L | Dark | L | Role |
|---|---|---|---|---|---|
| `chrome` | `#eceae4` | 0.937 | `#141312` | 0.187 | the title bar |
| `pane` | `#f3f1ec` | 0.958 | `#1d1c1b` | 0.227 | the sidebar |
| `stage` | `#f9f8f5` | 0.979 | `#262624` | 0.268 | the session: chat, welcome, Rules, Projects |
| `surface` | `#ffffff` | 1.000 | `#30302e` | 0.308 | cards, the composer, popovers |
| `selected` | `#ffffff` | 1.000 | `#2a2a28` | 0.284 | the selected sidebar row |

### Terminal
The dock's 16 colours, on `stage` (`palette_check.rs` holds them to AA).

| # | Light | Dark |
|---|---|---|
| 0 | `#28272d` | `#646163` |
| 1 | `#c02f12` | `#ec5941` |
| 2 | `#2f7549` | `#2da160` |
| 3 | `#995715` | `#d99530` |
| 4 | `#2f68b4` | `#428fdc` |
| 5 | `#6a4fb4` | `#9475db` |
| 6 | `#1f75cb` | `#63a6e9` |
| 7 | `#747273` | `#c1bfbe` |
| 8 | `#646163` | `#747273` |
| 9 | `#dd2b0e` | `#f6806d` |
| 10 | `#108548` | `#52b87a` |
| 11 | `#ab6100` | `#e9be74` |
| 12 | `#1f75cb` | `#63a6e9` |
| 13 | `#7b58cf` | `#ac93e6` |
| 14 | `#1f75cb` | `#9dc7f1` |
| 15 | `#18171d` | `#ecebea` |

### Colors
Contrast is the WCAG ratio of text on `stage`, of a colour on `surface`, or of white text on a fill.

| Token | Light | Contrast | Dark | Contrast | Role |
|---|---|---|---|---|---|
| `text_strong` | `#1f1e1c` | 15.7:1 | `#f6f5f4` | 13.9:1 | headings, names |
| `text` | `#2b2a27` | 13.5:1 | `#ecebe6` | 12.7:1 | body text |
| `text_body` | `#5e5b54` | 6.4:1 | `#c2bfb6` | 8.2:1 | secondary prose |
| `text_muted` | `#64615a` | 5.8:1 | `#a8a59c` | 6.2:1 | labels, captions, icons |
| `text_subtle` | `#8c887e` | 3.3:1 | `#7d7a72` | 3.5:1 | placeholders and disabled only |
| `border` | `#2b2a27 @ 13.0%` | 1.3:1 | `#ffffff @ 10.0%` | 1.4:1 | hairlines |
| `border_strong` | `#cfcbc0` | 1.6:1 | `#55534e` | 1.7:1 | outlined buttons, a switch at rest |
| `primary` | `#216d70` | 6.0:1 | `#8ecacd` | 7.2:1 | links, selected text, the focus edge |
| `accent` | `#216d70` | 6.0:1 | `#216d70` | 6.0:1 | send, the new-chat mark, accent dots (white icon on it) |
| `fill` | `#2b2a27` | 14.4:1 | `#ecebe6` | 12.7:1 | filled buttons: ink on paper, paper on charcoal |
| `toggle_on` | `#216d70` | 6.0:1 | `#529a9d` | 4.1:1 | a switch that is on |
| `mark_top` | `#216d70` | 5.7:1 | `#529a9d` | 4.7:1 | the ingot mark's top face (`ui/ingot.rs`) |
| `mark_left` | `#2b2a27` | 13.5:1 | `#ecebe6` | 12.7:1 | the ingot's left face |
| `mark_right` | `#8c887e` | 3.3:1 | `#6f6c64` | 2.9:1 | the ingot's right face |
| `mark_heat_white` | `#fff2cc` | 1.1:1 | `#fff2cc` | 13.6:1 | the ingot white hot from the forge (onboarding); never at rest |
| `mark_heat` | `#f5892c` | 2.3:1 | `#f5892c` | 6.1:1 | the ingot's orange glow as it cools, and a strike's flare |
| `mark_heat_ember` | `#a3311c` | 6.6:1 | `#a3311c` | 2.2:1 | the ingot's ember red as it cools |
| `mark_heat_iron` | `#3b3633` | 11.2:1 | `#3b3633` | 1.3:1 | the ingot's dark iron, last before its own colours |
| `fill_danger` | `#b4412f` | 5.6:1 | `#b7331f` | 6.0:1 | destructive confirms |
| `success` | `#276b43` | 6.4:1 | `#8fd3a8` | 7.6:1 | done, passed |
| `danger` | `#b4412f` | 5.6:1 | `#eb8c7b` | 5.4:1 | failed, errors |
| `warning` | `#8a5600` | 6.2:1 | `#ffc85a` | 8.6:1 | needs you |
| `memory` | `#75591a` | 5.6:1 | `#e5c77a` | 7.7:1 | memory and rules, on its tint |
| `mode_plan` | `#1f75cb` | 4.7:1 | `#428fdc` | 3.9:1 | Plan mode |
| `mode_bypass` | `#a32c12` | 7.2:1 | `#ec5941` | 3.8:1 | Bypass permissions |

### Contrast floors
- Meaningful text: 4.5:1 on every surface and hover wash.
- Icons, mode edges, the send arrow: 3:1.
- Placeholders and disabled text (`text_subtle`): 2.5:1, never body copy.
- Hairlines: 1.15:1 on every surface.
- Surface steps: at least 0.02 OKLab L apart, each within 0.004 of the mean step.

## Patterns

### Buttons (`ui/button.rs`)
One family; every control takes keyboard focus (a `primary` edge shows for the keyboard only), names itself to assistive technology, and swallows clicks while disabled or loading.
- Primary: 32px tall, padding 0 14px, radius 8px, `fill` with `fill_fg`, 500 weight. One per surface.
- Secondary and Outlined: `surface`, 1px `border_strong`, `hover` wash.
- Ghost: clear until hovered. Danger: `fill_danger`, white.
- Icon: 34px square, radius 8px, a 16px icon in `text_muted`; outlined squares 38px at 9px radius.
- Send: 38px, radius 9px, `accent` with a white arrow; `accent_soft` with nothing to send; Stop is an ink ring round a square.
- Switch: 36x20px pill, 14px thumb sliding over 160ms; `toggle_on` when on, `border_strong` when off.
- Segmented: a `sunk` track, radius 8px; the chosen segment raised onto `surface` at 6px.

### Composer
- Width: up to 560px, centred
- One unit: a white box (radius 14px, 1px `border`, `border_strong` while focused, tinted by the mode when one is set) resting in a `sunk` tray
- Text: 14px; the outlined + bottom left and Send bottom right, 10px from the box's edges
- The tray: the project, the context ring, the model and the mode as quiet 28px selectors: a 14px icon, the label, a chevron that flips while open; washed on hover, raised onto `surface` with a hairline while open

### Dot Field (`ui/dot_field.rs`)
- Behind the home screen: a 14px grid of 2px dots in `dot`, gathered in two clouds hugging the content (beside the heading, past the composer's lower right), their outlines warped and density varied by slow noise, faded with a smooth falloff to a quiet margin round the measured content and before every edge
- About 1.6% of dots in `accent`; fewer still are 5px outlined squares or accent diamonds
- Painted on one canvas from cached geometry; breathes over 9s, leans up to 4px from the pointer, brightens while the composer has focus; glides to its new place when the layout changes (sidebar, resize, a growing composer); rests when hidden or inactive; still under reduced motion

### Card Default
- Border: 1px `border`
- Padding: 24px
- Radius: 10px
- Background: `surface`
- Usage: tool calls, file cards, code blocks

### Popover
- Border: 1px `border`, radius 10px, padding 4px
- Background: `surface`, with the two-layer popover shadow
- Usage: menus, pickers, the command palette

### File Card
- A file Forge made, in its reply: up to 440px, `surface`, 1px `border`, radius 10px, padding 8px
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
- Current: the `active` wash, 500 weight, no border
- New chat: a round `accent` mark with a white +
- Collapsed: a 52px rail of icon buttons for the same actions
- Group labels: 12px caption

## Decisions

| Decision | Rationale | Date |
|----------|-----------|------|
| Borders-only depth | A desktop tool read for hours: hairlines separate without adding weight; shadows are kept to show what floats. | 2026-09-28 |
| Three surfaces, one step apart | Title bar, sidebar and session must read as separate areas at a glance; even OKLab steps make the split clear without contrast jumps. | 2026-10-12 |
| Ink, not colour, for the filled button | The accent is kept for sending; the one filled action per surface is ink on paper (paper on charcoal), so it never competes with Send. | 2026-10-17 |
| Muted text a step deeper than the reference | The reference's #6f6c64 and #9a968c miss their floors on the title bar; #64615a and #8c887e keep the look and pass. | 2026-10-17 |
| Softly squared controls | Large uniform radii read as a template; 7-10px keeps the shapes calm and precise. | 2026-10-17 |
| Display serif for one line per view | The greeting, and a document's title: one warm, human line; everything else stays in the interface face. | 2026-10-12 |
| Composer at 620px | A comfortable measure for a prompt, the same in a chat and on the welcome page. | 2026-10-11 |
| Icons on a 12/14/16/20 scale | Desktop density: 14 in rows, 16 in controls, 20 for the composer's +; 12 inside captions. | 2026-10-12 |
