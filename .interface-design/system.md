# Design System: Forge desktop (forge-gpui)

> Generated from `src/theme.rs` by `src/system_md.rs`. Change a token in theme.rs, then run
> `UPDATE_SYSTEM_MD=1 cargo test system_md`; `cargo test` fails while this file is stale.
> Every colour pair below is held to its floor by `src/palette_check.rs`.

## Direction

**Personality:** calmly smooth: a warm editorial workspace, confident and unhurried, closer to a printed quarterly than a dashboard (after the Anchor style reference).

**Foundation:** sun-bleached cream. The window is Cream Canvas (#e3d7cd), the session a sheet of Warm White (#f0e9e5) on it, cards white on that: paper on paper. One ink (#1e1e1e) carries the type. One colour, dusty blue (#41608a; #9db7d9 at night), where the reference has terracotta: the decisive action, links, a switch that is on, the mark's top, a waiting dot. Everything else is neutral; colour otherwise only for meaning (modes, status).

**Depth:** nearly flat. Hairlines inside cards; the session and the user's message lift on paper grain (three 1px shadows at 6/4/1% ink); popovers and the permission card a touch more.

**Shape:** two shapes. Everything you press is a pill (buttons, chips, icon buttons, the new-chat and needs-you pills); everything that holds content is 4px (cards, rows, menus, fields, the composer, the session).

**Type:** Archivo only, in two cuts: 400 for text, 600 for headings, buttons and labels. Small labels in sentence case, never uppercase and never above a heading. Code in JetBrains Mono.

## Tokens

### Spacing
Base: 4px
Scale: 4, 6, 8, 10, 12, 16, 20, 24, 32, 40

### Radius
Two shapes: 4px for everything that holds content, pill for everything you press.
Scale: 1px (a line's ends), 4px (key caps), 4px (rows and squarer buttons), 4px (cards, tool rows, menus), 4px (fields), 4px (the composer), pill (buttons, chips, icon buttons, segments, the send button, switches, badges, avatars)

### Typography
Interface: Archivo in 400 and 600 only, bundled (OFL); headings in 600
Display: Archivo 600 (the greeting, headings)
Code: JetBrains Mono
Scale: 12 caption, 13 UI and chat, 14 input, 18 subheading, 20 heading, 32 greeting, 36 home heading
Weights: 400, 500 (labels, buttons), 600 (headings, the project name)
Prose line height: 1.5333333

### Icons
Lucide, 2px stroke as drawn. Sizes: 12 / 14 / 16 / 20 only.

### Sizes
- `titlebar`: 36px, the title bar's height
- `sidebar`: 288px, the sidebar's width
- `column`: 720px, the reading column
- `menu_row`: 26px, a row in a chat's menu
- `menu_width`: 200px, a chat menu's width, and its flyouts' least
- `thumb_w`: 34px, a picture a call returned, as a thumbnail at the end of its line
- `thumb_h`: 22px, that thumbnail's height, inside the quiet line
- `tool_image`: 360px, that picture's widest, in the call's opened body
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
- `mark_home`: 44px, the mark beside the home greeting

### Surfaces
The cream stack: the title bar and the sidebar share the canvas, the session steps up from it, cards step up again (`palette_check.rs`).

| Token | Light | L | Dark | L | Role |
|---|---|---|---|---|---|
| `chrome` | `#e3d7cd` | 0.886 | `#171513` | 0.197 | the title bar |
| `pane` | `#e3d7cd` | 0.886 | `#171513` | 0.197 | the sidebar |
| `stage` | `#f0e9e5` | 0.938 | `#1e1c1a` | 0.228 | the session: chat, welcome, Rules, Projects |
| `surface` | `#ffffff` | 1.000 | `#2b2825` | 0.279 | cards, the composer, popovers |
| `selected` | `#f0e9e5` | 0.938 | `#262321` | 0.259 | the selected sidebar row |

### Themes
Picked in Settings > General > Theme and kept in `window.json`. Each is a whole look: its surfaces and one colour, and its form (faces, corners, density, depth, how a selected row shows). Calmly smooth is the system's own form; every theme passes `palette_check.rs` in both modes.

| Theme | Light canvas / session / accent | Dark canvas / session / accent |
|---|---|---|
| Calmly smooth (Cream and dusty blue, Archivo) | `#e3d7cd` / `#f0e9e5` / `#41608a` | `#171513` / `#1e1c1a` / `#9db7d9` |
| Terracotta (Warm paper, replies in a serif) | `#e3d7cd` / `#f0e9e5` / `#b85a2a` | `#171513` / `#1e1c1a` / `#ee884f` |
| Patina (Ivory and teal, Inter, flat) | `#e9e6df` / `#f9f8f5` / `#216d70` | `#141312` / `#232321` / `#529a9d` |
| Ink (Greys and Geist, nothing else) | `#e2e2df` / `#f1f1ef` / `#1e1e1e` | `#121212` / `#1b1b1a` / `#f0f0ee` |
| Slate (Cool grey, Plex, compact) | `#d8dde3` / `#edf0f3` / `#3b5b8c` | `#111418` / `#181c21` / `#9db7e0` |
| Plum (Heather, an editorial serif) | `#e5d9e0` / `#f2ebef` / `#7a3e65` | `#171217` / `#1f1a1f` / `#d3a3c6` |
| Layers (Raised cards, a grouped + menu) | `#e3d7cd` / `#f0e9e5` / `#3d4f8f` | `#171513` / `#1e1c1a` / `#a9b5e8` |
| Tray (Pills, the + as a tray in the box) | `#e3d7cd` / `#f0e9e5` / `#2e6077` | `#171513` / `#1e1c1a` / `#93c2d6` |
| Studio (Cool greys, a split + menu) | `#dcdde3` / `#eeeff3` / `#4a4fa0` | `#131417` / `#1a1b1f` / `#b3b6f0` |
| Tiles (Round and roomy, + as tiles) | `#e3d7cd` / `#f0e9e5` / `#3f6b4f` | `#171513` / `#1e1c1a` / `#9fcfae` |
| Command (Violet, a + you type to filter) | `#e3d7cd` / `#f0e9e5` / `#6a4fa3` | `#171513` / `#1e1c1a` / `#c2adf0` |

| Theme | Faces (UI / display / code / replies) | Corners button · card · field · composer · pressed | Row · title bar · chat | Depth | Selected row |
|---|---|---|---|---|---|
| Calmly smooth | Archivo / Archivo / JetBrains Mono / Archivo | 4 · 4 · 4 · 4 · pill | 30 · 36 · 16 | Grain | Soft |
| Terracotta | Hanken Grotesk / Source Serif 4 Display / JetBrains Mono / Source Serif 4 | 8 · 12 · 10 · 16 · pill | 32 · 40 · 16 | Lift | Card |
| Patina | Inter / Inter / JetBrains Mono / Inter | 6 · 8 · 6 · 12 · 6 | 30 · 36 · 15 | Flat | Tint |
| Ink | Geist / Geist / Geist Mono / Geist | 6 · 8 · 8 · 12 · 8 | 30 · 36 · 15 | Flat | Wash |
| Slate | IBM Plex Sans / IBM Plex Sans / IBM Plex Mono / IBM Plex Sans | 3 · 4 · 3 · 6 · 4 | 26 · 32 · 14 | Flat | Tint |
| Plum | Instrument Sans / Instrument Serif / JetBrains Mono / Instrument Sans | 12 · 16 · 14 · 22 · pill | 32 · 38 · 15 | Lift | Soft |
| Layers | Hanken Grotesk / Archivo / JetBrains Mono / Hanken Grotesk | 8 · 14 · 10 · 18 · pill | 32 · 40 · 16 | Lift | Card |
| Tray | Hanken Grotesk / Archivo / JetBrains Mono / Hanken Grotesk | 9999 · 12 · 10 · 20 · pill | 32 · 38 · 16 | Lift | Card |
| Studio | Hanken Grotesk / Archivo / JetBrains Mono / Hanken Grotesk | 8 · 10 · 8 · 18 · 8 | 30 · 36 · 15 | Lift | Soft |
| Tiles | Hanken Grotesk / Archivo / JetBrains Mono / Hanken Grotesk | 12 · 18 · 12 · 22 · 12 | 34 · 40 · 15 | Lift | Wash |
| Command | Hanken Grotesk / Archivo / JetBrains Mono / Hanken Grotesk | 8 · 10 · 9 · 16 · 10 | 30 · 36 · 15 | Grain | Wash |

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
| `text_strong` | `#1e1e1e` | 13.9:1 | `#f5f2f0` | 15.2:1 | headings, names |
| `text` | `#1e1e1e` | 13.9:1 | `#f0e9e5` | 14.1:1 | body text |
| `text_body` | `#2b2825` | 12.2:1 | `#d9d1cb` | 11.3:1 | secondary prose |
| `text_muted` | `#5a554f` | 6.1:1 | `#a8a29b` | 6.7:1 | labels, captions, icons |
| `text_subtle` | `#7d776f` | 3.7:1 | `#857f77` | 4.3:1 | placeholders and disabled only |
| `border` | `#1e1e1e @ 13.0%` | 1.3:1 | `#f2e9e3 @ 12.0%` | 1.4:1 | hairlines |
| `border_strong` | `#c9c9c7` | 1.7:1 | `#4a4540` | 1.5:1 | outlined buttons, a switch at rest |
| `primary` | `#41608a` | 6.4:1 | `#9db7d9` | 7.1:1 | links, selected text, the focus edge |
| `accent` | `#41608a` | 6.4:1 | `#9db7d9` | 8.9:1 | send, the new-chat mark, accent dots (white icon on it) |
| `fill` | `#1e1e1e` | 13.9:1 | `#f0e9e5` | 15.2:1 | filled buttons: ink on paper, paper on charcoal |
| `toggle_on` | `#41608a` | 6.4:1 | `#9db7d9` | 7.1:1 | a switch that is on |
| `mark_top` | `#41608a` | 5.4:1 | `#9db7d9` | 8.3:1 | the ingot mark's top face (`ui/ingot.rs`) |
| `mark_left` | `#1e1e1e` | 13.9:1 | `#f0e9e5` | 14.1:1 | the ingot's left face |
| `mark_right` | `#8b857e` | 3.0:1 | `#7d776f` | 3.8:1 | the ingot's right face |
| `mark_heat_white` | `#fff2cc` | 1.1:1 | `#fff2cc` | 15.2:1 | the ingot white hot from the forge (onboarding); never at rest |
| `mark_heat` | `#f5892c` | 2.1:1 | `#f5892c` | 6.9:1 | the ingot's orange glow as it cools, and a strike's flare |
| `mark_heat_ember` | `#a3311c` | 5.8:1 | `#a3311c` | 2.4:1 | the ingot's ember red as it cools |
| `mark_heat_iron` | `#3b3633` | 9.9:1 | `#3b3633` | 1.4:1 | the ingot's dark iron, last before its own colours |
| `fill_danger` | `#a3402c` | 6.3:1 | `#e98a72` | 7.2:1 | destructive confirms |
| `success` | `#3f6b4b` | 6.1:1 | `#8fbf9b` | 7.0:1 | done, passed |
| `danger` | `#a3402c` | 6.3:1 | `#e98a72` | 5.8:1 | failed, errors |
| `warning` | `#8a5600` | 6.2:1 | `#ffc85a` | 9.5:1 | needs you |
| `memory` | `#75591a` | 5.6:1 | `#e5c77a` | 7.7:1 | memory and rules, on its tint |
| `skill` | `#7a3fb0` | 6.6:1 | `#c3a3ec` | 6.8:1 | a skill named in the composer, bold |
| `mode_plan` | `#1f75cb` | 4.7:1 | `#428fdc` | 4.3:1 | Plan mode |
| `mode_bypass` | `#a32c12` | 7.2:1 | `#ec5941` | 4.2:1 | Bypass permissions |

### Contrast floors
- Meaningful text: 4.5:1 on every surface and hover wash.
- Icons, mode edges, the send arrow: 3:1.
- Placeholders and disabled text (`text_subtle`): 2.5:1, never body copy.
- Hairlines: 1.15:1 on every surface.
- Surface steps: at least 0.02 OKLab L apart, each within 0.004 of the mean step.

## Patterns

### Buttons (`ui/button.rs`)
One family; every control takes keyboard focus (a `primary` edge shows for the keyboard only), names itself to assistive technology, and swallows clicks while disabled or loading.
- Primary: 32px tall, padding 0 14px, radius 4px, `fill` with `fill_fg`, 500 weight. One per surface.
- Secondary and Outlined: `surface`, 1px `border_strong`, `hover` wash.
- Ghost: clear until hovered. Danger: `fill_danger`, white.
- Icon: 34px square, radius 4px, a 16px icon in `text_muted`; outlined squares 38px at 9999px radius.
- Send: 38px, radius 9999px, `accent` with a white arrow; `accent_soft` with nothing to send; Stop is an ink ring round a square.
- Switch: 36x20px pill, 14px thumb sliding over 160ms; `toggle_on` when on, `border_strong` when off.
- Segmented: a `sunk` track, radius 4px; the chosen segment raised onto `surface` at 9999px.

### Composer
- Width: up to 560px, centred
- One unit: a white box (radius 4px, 1px `border`, `border_strong` while focused, tinted by the mode when one is set) resting in a `sunk` tray
- Text: 14px; the outlined + bottom left and Send bottom right, 10px from the box's edges
- The tray: the project, the context ring, the model and the mode as quiet 28px selectors: a 14px icon, the label, a chevron that flips while open; washed on hover, raised onto `surface` with a hairline while open

### Dot Field (`ui/dot_field.rs`)
- Behind the home screen: a 14px grid of 2px dots in `dot`, gathered in two clouds hugging the content (beside the heading, past the composer's lower right), their outlines warped and density varied by slow noise, faded with a smooth falloff to a quiet margin round the measured content and before every edge
- About 1.6% of dots in `accent`; fewer still are 5px outlined squares or accent diamonds
- Painted on one canvas from cached geometry; breathes over 9s, leans up to 4px from the pointer, brightens while the composer has focus; glides to its new place when the layout changes (sidebar, resize, a growing composer); rests when hidden or inactive; still under reduced motion
- Behind a chat it fills both margins beside the reading column (kept 18px clear of it), at 30% of its home strength (40% in dark); while Forge replies a slow wave runs down through it (crest 80%), easing in and out with the reply; still and faint under reduced motion

### Card Default
- Border: 1px `border`
- Padding: 24px
- Radius: 4px
- Background: `surface`
- Usage: tool calls, file cards, code blocks

### Popover
- Border: 1px `border`, radius 4px, padding 4px
- Background: `surface`, with the two-layer popover shadow
- Usage: menus, pickers, the command palette

### File Card
- A file Forge made, in its reply: up to 440px, `surface`, 1px `border`, radius 4px, padding 8px
- A 36x44 page tile (radius 4px, concentric with the card) previews it, its type in mono `primary`
- Title 600 `text_strong`; "Document · MD" caption `text_muted`; a bordered Download button
- Open: the edge turns `primary` at 60%

### Document Pane
- Beside the conversation, half the session, at least 460px; `surface` with a hairline on its left
- Header 48px: file icon, title, type; Copy, Download, Close as 26px quiet buttons
- Text 15px at 1.65 leading, lines up to 680px; the title in the display serif at 28px
- Enters with a 220ms ease-out, fading in from 16px to the right; Escape closes it

### Sidebar Row
- Height: 30px, radius 4px
- Current: the `active` wash, 500 weight, no border
- New chat: a round `accent` mark with a white +
- Collapsed: a 52px rail of icon buttons for the same actions
- Group labels: 12px caption

## Decisions

| Decision | Rationale | Date |
|----------|-----------|------|
| One colour, moss, not terracotta | The reference rations one warm orange to the primary action; Forge keeps the rationing and takes the reference's own moss, calmer on cream and kin to the mark. | 2026-10-18 |
| Dusty blue, not moss | Moss read as too green beside the cream; a dusty blue from the Slate theme's family, warmed for cream, keeps the same rationing and contrast (5.4:1 on the session). | 2026-10-19 |
| Paper on paper | The session is a 4px sheet on the cream canvas with paper grain; the title bar and sidebar have no dividers, the sheet's edge is the split. | 2026-10-18 |
| Pills to press, 4px to hold | The contrast between the two shapes is the system: it says at a glance what acts and what holds. | 2026-10-18 |
| Ink pills for the loud neutrals | New chat and needs-you are ink pills; the accent is kept for the one decision in front of you (Allow once, Send). | 2026-10-18 |
| Archivo for everything | A single face set in two weights reads as set type; headings in 600 replace the display serif. | 2026-10-18 |
| Composer at 620px | A comfortable measure for a prompt, the same in a chat and on the welcome page. | 2026-10-11 |
| Composer on the reading column | In a chat the composer takes the 720px column the reply is read in, so the two line up; on the welcome page it keeps its own measure. | 2026-10-23 |
| No name over a reply | The user's bubble already separates the turns; a name and mark over every reply only repeated it. | 2026-10-23 |
| Deny is quiet | A permission has one filled action, Allow once; Deny is a ghost button, a way out rather than an alarm. | 2026-10-23 |
| Icons on a 12/14/16/20 scale | Desktop density: 14 in rows, 16 in controls, 20 for the composer's +; 12 inside captions. | 2026-10-12 |
