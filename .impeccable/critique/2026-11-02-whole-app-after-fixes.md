---
target: whole app (forge-gpui, after the critique fixes, e973df9)
date: 2026-11-02
method: design review only (native platform)
total_score: 30
max_score: 40
na_heuristics: ""
p0_count: 0
p1_count: 0
---
# Critique: Forge desktop, whole app, after fixes

Evidence: 120 screenshots (60 light, 60 dark, Linux/Xvfb) of every page, Settings section, composer menu, panel and dialog, plus the new routine form, rule line, cross-tab search, project menu and heat rail; Esc checked on every composer menu.

| # | Heuristic | Before | Now | What changed / what's left |
|---|---|---|---|---|
| 1 | Visibility of system status | 3 | 3 | Covers replace "Drawing…"; the heat rail shows each step's state. Live rail glow seen only in code, not on screen. |
| 2 | Match system / real world | 2 | 3 | Plain scopes everywhere, Sandbox wording, "every Monday at 09:00". "HyperFrames" remains. |
| 3 | User control and freedom | 2 | 3 | Esc closes every composer menu; project chip opens a menu; design tabs hold still. |
| 4 | Consistency and standards | 2 | 3 | One control per kind of choice, one scope wording, token radii, three durations. |
| 5 | Error prevention | 3 | 3 | Rules explained/flagged before Add; routine form checks the time. |
| 6 | Recognition rather than recall | 2 | 3 | Routine form, rule line, e.g. placeholders. Icon-only design chip and endpoint actions remain. |
| 7 | Flexibility and efficiency | 3 | 3 | Keyboard works in every menu; /routine kept for power users. |
| 8 | Aesthetic and minimalist design | 3 | 3 | Network hidden; Design page still stacks tabs, three buttons and the routines panel. |
| 9 | Error recovery | 3 | 3 | Unchanged, good. |
| 10 | Help and documentation | 3 | 3 | Inline explanations in Permissions and Routines. |
| **Total** | | **26** | **30/40** | Good |

Specificity: the heat rail and the shared status language give Forge a signature of its own on the transcript; the chrome around it (cream paper, copper, Settings sheet) still reads close to Claude Desktop.

## Remaining
1. [P2] Identity beyond the transcript: carry the rail's language into the title bar and sidebar (a running chat's row, the endpoint badge).
2. [P2] Icon-only controls: the design chip with no design set, the endpoint edit/check/delete icons.
3. [P3] The design panel inside a live design chat is not reviewed yet (needs a live model).
4. [P3] Design page header: tabs, three buttons and a routines panel before the cards.
