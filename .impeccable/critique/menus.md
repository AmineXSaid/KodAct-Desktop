# Critique: menus and popovers (`add_menu`, `model_menu`, `mode_menu`, `chat_menu`, `context_menu`, `tasks`, `theme::popover`)

## Run 1, baseline (2026-10-23)

Design specificity: neutral; the + menu is close to Claude's.

| # | Heuristic | Score | Note |
|---|---|---|---|
| 1 | Visibility of system status | 3 | Toggles show state |
| 2 | Match with the real world | 3 | |
| 3 | User control and freedom | 3 | Esc and click-out close |
| 4 | Consistency and standards | 2 | Each menu sets its own padding, rows and section labels |
| 5 | Error prevention | 3 | |
| 6 | Recognition over recall | 3 | Icons and shortcuts |
| 7 | Flexibility and efficiency | 3 | |
| 8 | Aesthetic and minimalist design | 3 | |
| 9 | Error recovery | 3 | |
| 10 | Help and documentation | 2 | |
| | **Total** | **28/40** | Good |

Cognitive load: 1 failure (the + menu's 6 rows sit at the edge).

Priority issues:
- P1: a button's tooltip stays over its open menu (the + button).
- P2: no shared menu style: radius, padding, row height and labels differ per menu.

## 2026-10-23: step C, the title bar and the chat menu (c-* screenshots)

- The chat's name with ⌄ heads the main column; its menu is Claude Code's:
  plain rows, letter keys (R C P A D), three groups, flyouts (Open in,
  Transcript view, Move to project, Export), Delete in red. First pass 8.0
  (flyouts clipped by the menu's scroll); after: 8.5.
- Right side: Plan, Files, Share, ⋮ with the pane's open state; Terminal,
  theme, shortcuts and settings in ⋮. First pass 7.5 (tooltip over its own
  menu, mixed key styles); after: 8.5.
- Search: a bordered field atop the sidebar opens the palette centred over
  a dimmed window: 8.5.
- Archive (A) and the archived filter; an empty list says why: 8.5.
- Transcript view Detailed opens every run and thought: 8.5.
