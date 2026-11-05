# Design System: KodAct desktop (kodact-desktop)

> Generated from `src/theme.rs` by `src/system_md.rs`. Change a token in theme.rs, then run
> `UPDATE_SYSTEM_MD=1 cargo test system_md`; `cargo test` fails while this file is stale.
> Every colour pair below is held to its floor by `src/palette_check.rs`.

## Direction

**Personality:** KodAct, as KodAct CLI draws it: a precise, quick working tool for long agent sessions, calm because nothing performs, sure because state is never in doubt. One brand from terminal to window.

**Foundation:** ink, paper and lime (KodAct CLI's `tui/palette.rs`). Ink at night, the default: the title bar and sidebar #0f1113, the session #15181b, cards #1c2024; paper text in two voices (yours #f2f0e9, KodAct's a softer #d9d7cf). Paper by day: #e9e8e2, #f6f5f1, cards #fdfdfb, ink text. Lime #c6ff3d is KodAct acting and what needs you: as a fill (the primary button; Send and the mark's tile on ink; #b5ea2c by day) it carries ink; as a line or a word (what is live, links, a switch that is on) it is olive #4a7200 on paper. Teal and violet carry structure (modes, skills, memory, code), peach marks code in prose; green, red and amber only mean done, failed and a warning.

**Depth:** nearly flat. Hairlines inside cards; the session and the user's message lift on paper grain (three 1px shadows at 6/4/1% ink); popovers and the permission card a touch more.

**Shape:** two shapes, and the mark's. Everything you press is a pill (buttons, chips, icon buttons); everything that holds content is 4px (cards, rows, menus, fields, the composer, the session); Send and the mark are tiles, squares with their corners rounded a sixth, each the opposite of its ground: lime on ink, ink on paper, never a shadow.

**Type:** KodAct's own open faces, carried in the app (OFL, `assets/fonts/`): KodAct Sans (KodAct's cut of Host Grotesk) for every word, 400 for text, 500 for labels, 600 for headings and buttons, its squared terminals kin to the pixel letters; KodAct Mono (its cut of JetBrains Mono, without the arrow ligatures) for code, paths and keys. Named KodAct's own, so a copy installed on the computer is never drawn in their place. The wordmark is not a font: it is the CLI's pixel letters, "Kod" in the strong ink and "Act" in lime. Small labels in sentence case, never uppercase and never above a heading; diagrams are redrawn in KodAct Sans whatever they name.

## Tokens

### Spacing
Base: 4px
Scale: 4, 6, 8, 10, 12, 16, 20, 24, 32, 40

### Radius
Two shapes: 4px for everything that holds content, pill for everything you press.
Scale: 1px (a line's ends), 4px (key caps), 4px (rows and squarer buttons), 4px (cards, tool rows, menus), 4px (fields), 4px (the composer), pill (buttons, chips, icon buttons, segments, the send button, switches, badges, avatars)

### Typography
Interface: KodAct Sans in 400 and 600 only, bundled (OFL); headings in 600
Display: KodAct Sans 600 (the greeting, headings)
Code: KodAct Mono
Scale: 12 caption, 13 UI and chat, 14 input, 18 subheading, 20 heading, 32 greeting, 36 home heading
Weights: 400, 500 (labels, buttons), 600 (headings, the project name)
Prose line height: 1.5333333

### Icons
Lucide, 2px stroke as drawn. Sizes: 12 / 14 / 16 / 20 only.

### Sizes
- `titlebar`: 36px, the title bar's height
- `sidebar`: 288px, the sidebar's width
- `column`: 760px, the reading column
- `menu_row`: 26px, a row in a chat's menu
- `session_panel`: 300px, the Session column beside a chat: environment, context, changes
- `composer_line`: 46px, the composer box with one line in it: +, the text, send
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
- `design_menu`: 340px, the composer's design menu's width
- `design_menu_list`: 400px, the design menu's list at its tallest: every kind of work
- `edge_bar`: 3px, the jewel edge a window draws as it opens
- `hit_sm`: 28px, an icon button in a dense row
- `hit`: 34px, an icon button's square
- `hit_lg`: 38px, the composer's outlined + and send
- `button_h`: 32px, a labelled button
- `button_sm`: 28px, a compact button, a selector
- `switch_w`: 36px, a switch's track (20px tall, 14px thumb)
- `rail`: 52px, the collapsed sidebar
- `mark_home`: 34px, the symbol beside the home greeting

### Surfaces
The ink stack (paper by day): the title bar and the sidebar share the deepest ground, the session steps up from it, cards step up again (`palette_check.rs`).

| Token | Light | L | Dark | L | Role |
|---|---|---|---|---|---|
| `chrome` | `#e9e8e2` | 0.930 | `#0f1113` | 0.176 | the title bar |
| `pane` | `#e9e8e2` | 0.930 | `#0f1113` | 0.176 | the sidebar |
| `stage` | `#f6f5f1` | 0.970 | `#15181b` | 0.207 | the session: chat, welcome, Rules, Projects |
| `surface` | `#fdfdfb` | 0.993 | `#1c2024` | 0.241 | cards, the composer, popovers |
| `selected` | `#f6f5f1` | 0.970 | `#15181b` | 0.207 | the selected sidebar row |

### Theme
One look, KodAct's, in dark (the default) and light (`window.json` keeps the mode). KodAct Sans for words, KodAct Mono for code (`fonts.rs`). It passes `palette_check.rs` in both modes.

| | Light | Dark |
|---|---|---|
| Canvas / session / accent | `#e9e8e2` / `#f6f5f1` / `#b5ea2c` | `#0f1113` / `#15181b` / `#c6ff3d` |

Faces (UI / display / code / replies): KodAct Sans / KodAct Sans / KodAct Mono / KodAct Sans. Corners button · card · field · composer · pressed: 4 · 4 · 4 · 4 · pill. Row · title bar · chat: 30 · 36 · 14.

### Terminal
The dock's 16 colours, on `stage` (`palette_check.rs` holds them to AA).

| # | Light | Dark |
|---|---|---|
| 0 | `#101214` | `#4a5058` |
| 1 | `#b8392e` | `#ff7a6b` |
| 2 | `#2f7a45` | `#7ccf8f` |
| 3 | `#8f6200` | `#f0c25e` |
| 4 | `#2f63a8` | `#8fbef0` |
| 5 | `#5a44c0` | `#b4a2ff` |
| 6 | `#0e7568` | `#5fd8c4` |
| 7 | `#55595e` | `#d9d7cf` |
| 8 | `#63676d` | `#868b92` |
| 9 | `#a8322a` | `#ff9d92` |
| 10 | `#2a6e3e` | `#a8e3b6` |
| 11 | `#805700` | `#f7d58a` |
| 12 | `#2a5896` | `#b7d6f6` |
| 13 | `#513cad` | `#cfc4ff` |
| 14 | `#0c685d` | `#93e7d9` |
| 15 | `#2b2e33` | `#fbf9f3` |

### Colors
Contrast is the WCAG ratio of text on `stage`, of a colour on `surface`, or of white text on a fill.

| Token | Light | Contrast | Dark | Contrast | Role |
|---|---|---|---|---|---|
| `text_strong` | `#08090b` | 18.3:1 | `#fbf9f3` | 16.9:1 | headings, names |
| `text` | `#101214` | 17.2:1 | `#f2f0e9` | 15.6:1 | body text |
| `text_body` | `#2b2e33` | 12.5:1 | `#d9d7cf` | 12.4:1 | secondary prose |
| `text_muted` | `#55595e` | 6.5:1 | `#a9adb2` | 7.9:1 | labels, captions, icons |
| `text_subtle` | `#63676d` | 5.2:1 | `#868b92` | 5.2:1 | placeholders and disabled only |
| `border` | `#101214 @ 13.0%` | 1.3:1 | `#e2e6e9 @ 12.0%` | 1.4:1 | hairlines |
| `border_strong` | `#c5c8cb` | 1.6:1 | `#3a4047` | 1.6:1 | outlined buttons, a switch at rest |
| `primary` | `#4a7200` | 5.6:1 | `#c6ff3d` | 13.9:1 | links, selected text, the focus edge |
| `accent` | `#b5ea2c` | 13.2:1 | `#c6ff3d` | 15.9:1 | the brand as a fill: Send, the primary button, the new-chat mark (ink on it) |
| `fill` | `#b5ea2c` | 13.2:1 | `#c6ff3d` | 15.9:1 | filled buttons: KodAct's lime tile under ink, in both modes |
| `toggle_on` | `#4a7200` | 5.6:1 | `#c6ff3d` | 13.9:1 | a switch that is on |
| `mark_top` | `#101214` | 17.2:1 | `#c6ff3d` | 15.1:1 | the mark's tile and the symbol alone, opposite the ground (`ui/mark.rs`) |
| `page_ink` | `#4a7200` | 5.7:1 | `#4a7200` | 5.7:1 | marks drawn on a design page, on their halo (`ui/design_marks.rs`) |
| `mark_left` | `#c6ff3d` | 15.9:1 | `#101214` | 15.9:1 | the mark's cursor bar and run triangle inside its tile |
| `live` | `#4a7200` | 5.6:1 | `#c6ff3d` | 13.9:1 | what is live: dots that need you, the shimmer's peak; lime on ink, olive on paper |
| `fill_danger` | `#b8392e` | 5.6:1 | `#ff7a6b` | 7.4:1 | destructive confirms |
| `success` | `#2a6f3e` | 6.0:1 | `#7ccf8f` | 8.7:1 | done, passed |
| `danger` | `#b8392e` | 5.6:1 | `#ff7a6b` | 6.4:1 | failed, errors |
| `warning` | `#8f6200` | 5.3:1 | `#f0c25e` | 9.8:1 | needs you |
| `memory` | `#0e7568` | 4.7:1 | `#5fd8c4` | 8.1:1 | memory and rules, on its tint |
| `skill` | `#5a44c0` | 6.8:1 | `#b4a2ff` | 7.5:1 | a skill named in the composer, bold |
| `mode_plan` | `#5a44c0` | 6.8:1 | `#b4a2ff` | 7.5:1 | Plan mode |
| `mode_bypass` | `#b8392e` | 5.6:1 | `#ff7a6b` | 6.4:1 | Bypass permissions |

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
- Send: 38px, the mark's tile (corners a sixth of it, beside 9999px pills), `mark_top` under the `mark_left` run triangle (lime under ink at night, ink under lime by day); faint with nothing to send; Stop: an ink square on a soft tile.
- Switch: 36x20px pill, 14px thumb sliding over 160ms; `toggle_on` when on, `border_strong` when off.
- Segmented: a `sunk` track, radius 4px; the chosen segment raised onto `surface` at 9999px.

### Composer
- Width: up to 560px, centred
- One unit: a white box (radius 4px, 1px `border`, `border_strong` while focused, tinted by the mode when one is set) resting in a `sunk` tray
- Text: 14px; the outlined + bottom left and Send bottom right, 10px from the box's edges
- The tray: the project, the context ring, the model and the mode as quiet 28px selectors: a 14px icon, the label, a chevron that flips while open; washed on hover, raised onto `surface` with a hairline while open

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
- A file KodAct made, in its reply: up to 440px, `surface`, 1px `border`, radius 4px, padding 8px
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
- New chat: a round `accent` mark with an ink +
- Collapsed: a 52px rail of icon buttons for the same actions
- Group labels: 12px caption

## Decisions

| Decision | Rationale | Date |
|----------|-----------|------|
| KodAct CLI's identity | Forge became KodAct, and the desktop takes the CLI's own identity in place of Forge's forge-and-ember world: ink, paper and lime (`tui/palette.rs`), the cursor-and-run tile and the pixel wordmark (`tui/brand.rs`). Lime fills carry ink; where lime would be a line or a word on paper it is olive. | 2026-11-05 |
| Dark first | KodAct is mostly seen in a dark terminal, and lime is at full strength on ink. Light stays whole, with the CLI's olive, and the mark in ink with no shadow (lime would have no edge on paper). | 2026-11-05 |
| KodAct Sans and KodAct Mono, carried | KodAct's own open faces replace Anthropic Sans, which was Anthropic's and not ours to ship: cuts of Host Grotesk for words and JetBrains Mono for code (without its arrow ligatures), named KodAct's so an installed copy never stands in. | 2026-11-05 |
| The mark at work | One working mark (the hammer, the turning ingot and the VS Code cube are gone): the cursor bar and run triangle take turns, code then act. Send is the run triangle; the working words shimmer up to lime, as in the CLI. | 2026-11-05 |
| One mark per screen, opposite its ground | The tile had become a status icon (title bar, sidebar rows, agent rows, work groups, the agents header): up to seven on a screen, brighter than the reply on ink and 1.16 to 1.30:1 on paper behind a hard olive shadow. Now the symbol is at home and at work on the working line only; anything else that runs takes the running icon; the title bar carries the name as type; the mark and Send take the opposite of the ground, and the app icon is the ink tile (`design/kodact-identity.html`). | 2026-11-05 |
| An opener under a second | The forging film (2.6s each launch, 8.3s the first) is replaced by the mark's beat and the name sliding out beside it, then a fade: 0.9s, on the theme's own ground, skippable. | 2026-11-05 |
| One colour, moss, not terracotta | The reference rations one warm orange to the primary action; KodAct keeps the rationing and takes the reference's own moss, calmer on cream and kin to the mark. | 2026-10-18 |
| Dusty blue, not moss | Moss read as too green beside the cream; a dusty blue from the Slate theme's family, warmed for cream, keeps the same rationing and contrast (5.4:1 on the session). | 2026-10-19 |
| Paper on paper | The session is a 4px sheet on the cream canvas with paper grain; the title bar and sidebar have no dividers, the sheet's edge is the split. | 2026-10-18 |
| Pills to press, 4px to hold | The contrast between the two shapes is the system: it says at a glance what acts and what holds. | 2026-10-18 |
| Ink pills for the loud neutrals | New chat and needs-you are ink pills; the accent is kept for the one decision in front of you (Allow once, Send). | 2026-10-18 |
| Anthropic Sans only (since replaced) | One family everywhere, in its Text and Display cuts: the interface, replies, code, the terminal and diagrams. | 2026-10-26 |
| One look | The theme picker and its eleven other looks are gone; Calmly smooth, light and dark, is the app's one look. | 2026-10-26 |
| Forge Ember (since replaced) | Warm paper and warm graphite, the sidebar a step deeper than the session, and one copper heat for the action, the current state and links. | 2026-10-23 |
| A neutral wash for the open row | The accent stays for the action; the open chat and the open screen lift by a grey wash. | 2026-10-23 |
| Composer at 620px | A comfortable measure for a prompt, the same in a chat and on the welcome page. | 2026-10-11 |
| Composer on the reading column | In a chat the composer takes the 760px column the reply is read in, so the two line up; on the welcome page it keeps its own measure. | 2026-10-23 |
| No name over a reply | The user's bubble already separates the turns; a name and mark over every reply only repeated it. | 2026-10-23 |
| Deny is quiet | A permission has one filled action, Allow once; Deny is a ghost button, a way out rather than an alarm. | 2026-10-23 |
| Every window leaves as it came | Each group has its own motion, in and out (`ui/motion.rs`): dialogs rise 14px over a dimming backdrop and sink away (240/170ms); the palette drops from above and lifts away; menus at an anchor unfold and fade back toward it (110ms); the composer's menus draw their jewel edge and fade back down; questions draw their edge in their colour and sink away; side panels and the dock slide in and back out (280/200ms); a notice springs up and drops away. A window that closes is kept a beat (`Presence`) so it leaves rather than vanishes. | 2026-10-25 |
| Replies at 15/24, rows 8 apart | Calm sets a reply at 15px on 24px lines and keeps a reply's rows (text, tool lines, cards) 8px apart, as Claude Code does: a third more on screen, read as a tool rather than a document. Paragraphs keep their own gap. | 2026-10-24 |
| Icons on a 12/14/16/20 scale | Desktop density: 14 in rows, 16 in controls, 20 for the composer's +; 12 inside captions. | 2026-10-12 |
