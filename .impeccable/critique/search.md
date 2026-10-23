# Critique: search palette (`src/ui/command_palette.rs`)

## Run 1, baseline (2026-10-23)

Design specificity: specific to Forge (chats, files, "said in chats", modes), but dressed like an IDE.

| # | Heuristic | Score | Note |
|---|---|---|---|
| 1 | Visibility of system status | 3 | Current mode marked; counts per group |
| 2 | Match with the real world | 2 | "Mode: Expert", "Said in chats", "All 54 ›" |
| 3 | User control and freedom | 3 | Esc, scopes, Tab |
| 4 | Consistency and standards | 3 | |
| 5 | Error prevention | 3 | |
| 6 | Recognition over recall | 3 | Shortcuts shown |
| 7 | Flexibility and efficiency | 4 | Prefixes, scopes, keys |
| 8 | Aesthetic and minimalist design | 2 | Keycaps on almost every row, five icon tabs, three accent links before typing |
| 9 | Error recovery | 3 | Ask Forge when nothing matches |
| 10 | Help and documentation | 3 | Footer key hints |
| | **Total** | **29/40** | Good |

Cognitive load: 2 failures (minimal choices: 11+ rows and 5 tabs before typing; single focus).

Priority issues:
- P1: accent colour on "All N ›" links that are neither primary, selected nor state (Operate: restrained accent).
- P2: keycaps on nearly every row; tabs shown before anything is typed.
- P2: "Mode: Expert" reads as a setting dump; "Expert mode".
