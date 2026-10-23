# Critique: message box (`src/ui/composer.rs`)

## Run 1, baseline (2026-10-23)

Design specificity: partly specific (folder, model, mode under the box), but built as two stacked boxes.

| # | Heuristic | Score | Note |
|---|---|---|---|
| 1 | Visibility of system status | 2 | Stop/send states; "Set up an endpoint first" alarms inside a chat that works |
| 2 | Match with the real world | 2 | "What should we build today?" in a chat already under way |
| 3 | User control and freedom | 3 | Queue, stop, attachments |
| 4 | Consistency and standards | 2 | A box plus a tray of chips below it; a bordered + circle; a dim send circle |
| 5 | Error prevention | 3 | |
| 6 | Recognition over recall | 3 | |
| 7 | Flexibility and efficiency | 3 | / and @ menus, shortcuts |
| 8 | Aesthetic and minimalist design | 2 | Two levels of chrome where one surface would do |
| 9 | Error recovery | 2 | The endpoint warning names no way out in place |
| 10 | Help and documentation | 2 | |
| | **Total** | **24/40** | Acceptable |

Cognitive load: 3 failures (single focus, visual hierarchy, grouping).

Priority issues:
- P1: the placeholder speaks to a new chat inside an ongoing one.
- P1: the endpoint warning is a chip in the box; it should be a quiet link where it matters.
- P2: one surface: + and folder left, mode and model right, send filled only with text to send.
