# Critique: sidebar (`src/ui/sidebar.rs`)

## Run 1, baseline (2026-10-23, before the craft-floor and polish passes)

Design specificity: generic. The wordmark, project block and chat list could
belong to any developer tool; nothing says "agent sessions" except the dots.

| # | Heuristic | Score | Note |
|---|---|---|---|
| 1 | Visibility of system status | 3 | Working and unread marks exist; three signals per row blur them |
| 2 | Match with the real world | 3 | Plain names; branch shown as code |
| 3 | User control and freedom | 3 | Collapse, filter, rename, delete |
| 4 | Consistency and standards | 2 | Blue edge + dot + time per row; branch in sidebar and status bar; mono as chrome |
| 5 | Error prevention | 3 | Delete confirms |
| 6 | Recognition over recall | 3 | Titles and groups by date |
| 7 | Flexibility and efficiency | 3 | Ctrl+B, Ctrl+N, filter |
| 8 | Aesthetic and minimalist design | 1 | Heavy letter-spaced wordmark, dividers, a filled account card, a status bar under it |
| 9 | Error recovery | 3 | |
| 10 | Help and documentation | 2 | Empty state teaches; little else |
| | **Total** | **26/40** | Acceptable |

Cognitive load: 3 failures (single focus, visual hierarchy, minimal choices).

Priority issues:
- P1: coloured 2px edge on the open chat (craft floor: no coloured edge over 1px on list items).
- P1: branch name in mono as decoration (craft floor: no mono as a technical costume), and duplicated in the status bar.
- P2: every row carries a time; the dot shows on every row; together with the edge, three signals.
- P2: dividers between every section; the account as a filled card.
- P3: wordmark weight and letter-spacing louder than any content.

## Polish pass 2 (side2 screenshots, light and dark)

- The chat filter no longer looks switched on by default: "All projects" is
  the resting state; only a list narrowed to one project tints the icon.
- Hover swaps a row's status dot for pin and more, nothing else moves.

Mark: 8.5/10. Left (P3): the needs-you dot reads grey-blue in the Ink theme;
it matches the title-bar badge, but is faint at a glance.

## 2026-10-23: rebuilt on Claude Code's sidebar (d/d2 screenshots)

- Head: the wordmark and a Chat | Code switch (plain chats, or chats in a
  folder); New follows it. The project block is gone.
- A bordered Search field opening the palette; New in a ring; Projects,
  Documents (every file Forge made), Customize, More (a menu).
- Pinned and Recents, + on Recents, the filter on hover (or while it
  narrows); a remote chat shows its place's icon at rest; a working one
  the strike mark.
- One-line account row (picture, name · endpoint, ⌄ menu, settings).
- First pass 8.0 (⌄ pushed to the gear, rows a size small, a sky-blue
  working mark); after: 8.5.

Palette: Forge Ember (warm paper / warm graphite, one ember copper), every
pair at its floor in all eleven themes: 8.5. Face: Hanken Grotesk for the
system's theme (Anthropic Sans where installed): 8.5.
