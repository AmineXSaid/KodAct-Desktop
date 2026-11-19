# Design System: KodAct desktop (kodact-desktop)

> Generated from `src/theme.rs` by `src/system_md.rs`. Change a token in theme.rs, then run
> `UPDATE_SYSTEM_MD=1 cargo test system_md`; `cargo test` fails while this file is stale.
> Every colour pair below is held to its floor by `src/palette_check.rs`.

## Direction

**Personality:** calmly smooth: a warm editorial workspace, confident and unhurried, closer to a printed quarterly than a dashboard (after the Anchor style reference).

**Foundation:** KodAct Signal. Warm paper by day: the window and sidebar #efedea, the session a sheet of #f8f7f5 on it, cards white on that. Warm ink at night: #0e0d0c, #171615, cards #201f1d. One ink carries the type and the buttons. Two colours with separate jobs: turquoise (#0e7568; #5fd8c4 at night) for the mark, Send, the one decisive action and a waiting dot; peach (#a4461c; #ffb98f at night) for links, focus, selection and a switch that is on. Work in progress and inline code are ink. Everything else is neutral; colour otherwise only for meaning (modes, status).

**Depth:** nearly flat. Hairlines inside cards; the session and the user's message lift on paper grain (three 1px shadows at 6/4/1% ink); popovers and the permission card a touch more.

**Shape:** two shapes. Everything you press is a pill (buttons, chips, icon buttons, the new-chat and needs-you pills); everything that holds content is 4px (cards, rows, menus, fields, the composer, the session).

**Type:** Anthropic Sans only, the Text cut for every word (code included) and the Display cut for the greeting and headings: 400 for text, 500 for labels, 600 for headings and buttons. Small labels in sentence case, never uppercase and never above a heading. The app bundles no font; diagrams are redrawn in it whatever they name.

## Tokens

### Spacing
Base: 4px
Scale: 4, 6, 8, 10, 12, 16, 20, 24, 32, 40

### Radius
Two shapes: 4px for everything that holds content, pill for everything you press.
Scale: 1px (a line's ends), 4px (key caps), 4px (rows and squarer buttons), 4px (cards, tool rows, menus), 4px (fields), 4px (the composer), pill (buttons, chips, icon buttons, segments, the send button, switches, badges, avatars)

### Typography
Interface: Anthropic Sans Text in 400 and 600 only, bundled (OFL); headings in 600
Display: Anthropic Sans Display 600 (the greeting, headings)
Code: Anthropic Sans Text
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
- `mark_home`: 44px, the mark beside the home greeting

### Surfaces
The cream stack: the title bar and the sidebar share the canvas, the session steps up from it, cards step up again (`palette_check.rs`).

| Token | Light | L | Dark | L | Role |
|---|---|---|---|---|---|
| `chrome` | `#efedea` | 0.947 | `#15181e` | 0.209 | the title bar |
| `pane` | `#efedea` | 0.947 | `#15181e` | 0.209 | the sidebar |
| `stage` | `#f8f7f5` | 0.976 | `#0a0b0d` | 0.149 | the session: chat, welcome, Rules, Projects |
| `surface` | `#ffffff` | 1.000 | `#1d2129` | 0.247 | cards, the composer, popovers |
| `selected` | `#f8f7f5` | 0.976 | `#242934` | 0.281 | the selected sidebar row |

### Theme
One look, Calmly smooth, in light and dark (`window.json` keeps the mode). Every face is Anthropic Sans: Text for every word (interface, replies, code, the terminal, diagrams), Display for the large lines (`fonts.rs`). It passes `palette_check.rs` in both modes.

| | Light | Dark |
|---|---|---|
| Canvas / session / accent | `#efedea` / `#f8f7f5` / `#0e7568` | `#15181e` / `#0a0b0d` / `#5fd8c4` |

Faces (UI / display / code / replies): Anthropic Sans Text / Anthropic Sans Display / Anthropic Sans Text / Anthropic Sans Text. Corners button · card · field · composer · pressed: 4 · 4 · 4 · 4 · pill. Row · title bar · chat: 30 · 36 · 14.

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
| `text_strong` | `#0e0d0c` | 18.1:1 | `#f5f7fa` | 18.3:1 | headings, names |
| `text` | `#1a1816` | 16.5:1 | `#ebeef2` | 16.9:1 | body text |
| `text_body` | `#2a2724` | 13.9:1 | `#d2d7e0` | 13.6:1 | secondary prose |
| `text_muted` | `#5c5853` | 6.6:1 | `#abaeb5` | 8.9:1 | labels, captions, icons |
| `text_subtle` | `#7a756f` | 4.3:1 | `#7c8391` | 5.2:1 | placeholders and disabled only |
| `border` | `#0e0d0c @ 13.0%` | 1.3:1 | `#e1e4ea @ 12.0%` | 1.3:1 | hairlines |
| `border_strong` | `#cbc8c3` | 1.7:1 | `#363d4a` | 1.5:1 | outlined buttons, a switch at rest |
| `primary` | `#a4461c` | 6.0:1 | `#ffb98f` | 9.7:1 | links, selected text, the focus edge |
| `accent` | `#0e7568` | 5.6:1 | `#5fd8c4` | 11.4:1 | send, the new-chat mark, accent dots (white icon on it) |
| `fill` | `#0e0d0c` | 17.7:1 | `#ebeef2` | 16.9:1 | filled buttons: ink on paper, paper on charcoal |
| `toggle_on` | `#a4461c` | 6.0:1 | `#ffb98f` | 9.7:1 | a switch that is on |
| `mark_top` | `#0e7568` | 5.2:1 | `#5fd8c4` | 11.4:1 | KodAct's mark, both strokes (`ui/kodact_mark.rs`) |
| `page_ink` | `#0e7568` | 5.6:1 | `#0e7568` | 5.6:1 | marks drawn on a design page, on their halo (`ui/design_marks.rs`) |
| `mark_heat_white` | `#f2fffc` | 1.0:1 | `#f2fffc` | 19.2:1 | the mark at its brightest as it arrives (onboarding); never at rest |
| `mark_heat` | `#7df0dc` | 1.3:1 | `#7df0dc` | 14.4:1 | the mark's bright signal as it settles, and a push's flare |
| `mark_heat_ember` | `#1f8f80` | 3.7:1 | `#1f8f80` | 5.0:1 | the mark's deep turquoise as it settles |
| `mark_heat_iron` | `#16302d` | 13.1:1 | `#16302d` | 1.4:1 | the mark near dark, last before its own colour |
| `fill_danger` | `#a3402c` | 6.3:1 | `#e98a72` | 7.8:1 | destructive confirms |
| `success` | `#3f6b4b` | 6.1:1 | `#8fbf9b` | 7.8:1 | done, passed |
| `danger` | `#a3402c` | 6.3:1 | `#e98a72` | 6.4:1 | failed, errors |
| `warning` | `#8a5600` | 6.2:1 | `#ffc85a` | 10.5:1 | needs you |
| `memory` | `#75591a` | 5.6:1 | `#e5c77a` | 8.5:1 | memory and rules, on its tint |
| `skill` | `#5a44c0` | 6.9:1 | `#b4a2ff` | 7.3:1 | a skill named in the composer, bold |
| `mode_plan` | `#1f75cb` | 4.7:1 | `#428fdc` | 4.8:1 | Plan mode |
| `mode_bypass` | `#a32c12` | 7.2:1 | `#ec5941` | 4.7:1 | Bypass permissions |

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
- Switch: 36x20px pill, 14px thumb sliding over 120ms; `toggle_on` when on, `border_strong` when off.
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
- New chat: a round `accent` mark with a white +
- Collapsed: a 52px rail of icon buttons for the same actions
- Group labels: 12px caption

## Decisions

| Decision | Rationale | Date |
|----------|-----------|------|
| One colour, moss, not terracotta | The reference rations one warm orange to the primary action; KodAct keeps the rationing and takes the reference's own moss, calmer on cream and kin to the mark. | 2026-10-18 |
| Dusty blue, not moss | Moss read as too green beside the cream; a dusty blue from the Slate theme's family, warmed for cream, keeps the same rationing and contrast (5.4:1 on the session). | 2026-10-19 |
| Paper on paper | The session is a 4px sheet on the cream canvas with paper grain; the title bar and sidebar have no dividers, the sheet's edge is the split. | 2026-10-18 |
| Pills to press, 4px to hold | The contrast between the two shapes is the system: it says at a glance what acts and what holds. | 2026-10-18 |
| Ink pills for the loud neutrals | New chat and needs-you are ink pills; the accent is kept for the one decision in front of you (Allow once, Send). | 2026-10-18 |
| Anthropic Sans only | One family everywhere, in its Text and Display cuts: the interface, replies, code, the terminal and diagrams. No other face is bundled or named. | 2026-10-26 |
| One look | The theme picker and its eleven other looks are gone; Calmly smooth, light and dark, is the app's one look. | 2026-10-26 |
| No generic AI icons | No sparkles, magic wands, robot heads or brains: each icon names its thing. Design is pencil-ruler (the Pen tool keeps pen-tool), skills a scroll, agents boxes (KodAct's cubes, many), memory a notebook, effort a gauge; thinking shows KodAct's own strike mark. | 2026-11-02 |
| Three durations | Every transition takes one of three times: fast 120ms (anything leaving, a menu arriving, the edge draw), base 200ms (hovers, dialogs and notices arriving, the panel leaving), slow 280ms (the side panel arriving). Loops (the strike mark, a working node) keep their own beat. | 2026-11-02 |
| The heat rail | KodAct's signature: a run's opened steps hang on one rail, a node each in the shared status language (ember working, ink done, danger failed or declined, warning stopped); while the run is live the rail glows ember and the working node breathes, then it cools to a hairline (`heat_rail.rs`). | 2026-11-02 |
| One control per kind of choice | One of a few short options is a segmented control (`editor_kit::choice`); one of several options that each need a line of description is a radio list; chips are for picking several, or one from a long list. Where a setting is kept reads the same everywhere: This project, just me · This project, shared · All my projects. | 2026-11-02 |
| KodAct Ember | Warm paper and warm graphite, the sidebar a step deeper than the session, and one copper heat for the action, the current state and links: the forge's own colour, as calm as Claude Code's. | 2026-10-23 |
| KodAct Signal | The app takes KodAct's identity: its mark (a step, then a chevron) in place of the ingot, warm ink and paper, and two colours with separate jobs. Turquoise is rationed to the mark, Send, the one decisive action and a waiting dot; peach, the terminal's warm accent, is for links, focus, selection and switches; buttons and work in progress are ink. It replaces KodAct Ember, and an all-turquoise first pass that put the accent on every control. | 2026-11-11 |
| A neutral wash for the open row | The accent stays for the action; the open chat and the open screen lift by a grey wash. | 2026-10-23 |
| Composer at 620px | A comfortable measure for a prompt, the same in a chat and on the welcome page. | 2026-10-11 |
| Composer on the reading column | In a chat the composer takes the 760px column the reply is read in, so the two line up; on the welcome page it keeps its own measure. | 2026-10-23 |
| No name over a reply | The user's bubble already separates the turns; a name and mark over every reply only repeated it. | 2026-10-23 |
| Deny is quiet | A permission has one filled action, Allow once; Deny is a ghost button, a way out rather than an alarm. | 2026-10-23 |
| Every window leaves as it came | Each group has its own motion, in and out (`ui/motion.rs`): dialogs rise 14px over a dimming backdrop and sink away (200/120ms); the palette drops from above and lifts away; menus at an anchor unfold and fade back toward it (120ms); the composer's menus draw their jewel edge and fade back down; questions draw their edge in their colour and sink away; side panels and the dock slide in and back out (280/200ms); a notice springs up and drops away. A window that closes is kept a beat (`Presence`) so it leaves rather than vanishes. | 2026-10-25 |
| Replies at 15/24, rows 8 apart | Calm sets a reply at 15px on 24px lines and keeps a reply's rows (text, tool lines, cards) 8px apart, as Claude Code does: a third more on screen, read as a tool rather than a document. Paragraphs keep their own gap. | 2026-10-24 |
| Icons on a 12/14/16/20 scale | Desktop density: 14 in rows, 16 in controls, 20 for the composer's +; 12 inside captions. | 2026-10-12 |
