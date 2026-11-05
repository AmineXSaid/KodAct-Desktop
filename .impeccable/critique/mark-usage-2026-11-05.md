---
target: "the KodAct mark across the desktop UI (src/ui/mark.rs and its callers)"
date: 2026-11-05
total_score: 27
max_score: 40
na_heuristics: ""
p0_count: 3
p1_count: 2
---

# The mark in the desktop UI: critique, 2026-11-05

Method: design review only (detector not applicable, native platform).
Evidence: the app at `a5723c4` on Xvfb (lavapipe) in demo mode, home and a
working chat, dark and light; sources `ui/mark.rs`, `workspace.rs`,
`sidebar.rs`, `chat_view.rs`, `work_group.rs`, `message.rs`, `theme.rs`.
Seen on Linux only; Windows not checked (the taskbar findings are from the
colours, not a Windows capture).

The identity system and the three mockups that answer this critique are in
`design/kodact-identity.html`.

## Design health score

| # | Heuristic | Score | Key issue |
|---|-----------|-------|-----------|
| 1 | Visibility of system status | 3 | Working is shown, but by the brand mark, so status and identity are one signal |
| 2 | Match system / real world | 3 | Plain words; the tile at 12 px reads as a lime square, not a symbol |
| 3 | User control and freedom | 3 | Stop, queue, continue are there |
| 4 | Consistency and standards | 2 | One tile means app, working, sub-agent, and (as Send) your action |
| 5 | Error prevention | 3 | n/a to the mark; permissions solid |
| 6 | Recognition rather than recall | 3 | Status slots are fixed and learnable |
| 7 | Flexibility and efficiency | 3 | Keyboard and palette throughout |
| 8 | Aesthetic and minimalist design | 1 | Up to seven lime tiles on one screen, each louder than the reply |
| 9 | Error recovery | 3 | Failures say so in words |
| 10 | Help and documentation | 3 | Setup guide in place |
| **Total** | | **27/40** | **Good, held back by the mark** |

## Design specificity verdict

The mark is specific and good: the cursor bar and the run triangle, "code,
then act", belong to KodAct and nobody else. The problem is volume and
ground. It is used as a status icon, so it multiplies with every working
chat and sub-agent, and its lime fill is either the brightest thing on ink
or invisible on paper.

## Priority issues

- **[P0] The mark is a status icon, so it is everywhere.** Title bar, home,
  agents header (32 px), every working sidebar row (12 px), every running
  sub-agent row and work-group live step (14 px), the working line. The
  demo shows two at once (title bar, working line); each background chat
  and running sub-agent adds one, animated: two background chats and three
  sub-agents make seven tiles, six moving.
  Fix: one mark per screen; running things elsewhere take the running
  status icon. Command: /impeccable quieter.
- **[P0] In dark, the mark outshouts the reply.** Lime `#c6ff3d` luminance
  0.84 against body ink `#d9d7cf` 0.68; a filled square of it beats any
  line of text (principle 1). Fix: inside the product, the glyph alone
  (bar and triangle), no tile. Command: /impeccable quieter.
- **[P0] On paper, the tile has no edge.** `#b5ea2c` on canvas 1.30:1, on
  the sidebar 1.16:1, on a white Windows taskbar 1.18:1. Only the hard 1 px
  olive offset shadow (`mark_right`) outlines it, a zero-blur block shadow
  the craft floor refuses; the light pixel wordmark's shadow is lime on
  paper (`brand_shadow`). Fix: the tile takes the opposite of its ground,
  ink tile on paper, lime tile on ink; no shadows. Command: /impeccable colorize.
- **[P1] Below 16 px the tile is a blob.** At 12 px the cursor bar is 1.4 px.
  Fix: no tile under 16 px; glyph down to 12 px, else nothing.
- **[P1] One shape, four meanings.** App, working, sub-agent, and Send (the
  same run triangle in the same lime tile). Fix: tile = the product; glyph
  at work = KodAct acting, in the working line only; Send = the triangle
  alone in a fill; running elsewhere = the neutral arc.
- **[P2] On the 16-pixel grid the triangle becomes a D** (from
  `PIXEL_FROM` 40 px up). Fix: vector geometry at every size in the
  desktop; pixels stay with the wordmark.
- **[P2] The title bar repeats the OS's icon.** Fix: the name as type, no tile.

## Persona red flags

- **Sam, long-session developer (project persona):** three chats running;
  the sidebar column of animated lime squares pulls the eye off the reply
  every second; can't tell at a glance which of them needs them, because
  "needs you" is also lime.
- **Jordan, first-timer:** sees the same tile in the title bar, the sidebar
  and in the agent rows, and can't tell whether it means "KodAct", "busy"
  or "click me".
- **Casey, low vision / light mode:** the lime tile on paper is 1.16:1;
  the mark is a faint smudge with an olive line.

## Minor observations

- `ui::turning_logo` and `turning_logo_in` are the same function.
- The working-line glyph is the strongest asset: it is the CLI's beat and
  should be the one animated brand moment.

## Questions to consider

- What if the brand were found once per screen, and recognised there?
- Is the app icon ink or lime? (Ink holds on both taskbars; lime is the CLI.)
