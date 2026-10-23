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
