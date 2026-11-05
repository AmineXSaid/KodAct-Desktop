---
version: 1
slug: "kodact-desktop-shell"
primary_target: "src/ui/workspace.rs"
related_targets: ["src/theme.rs", "src/ui/ingot.rs", "src/ui/opener.rs", "src/ui/activity.rs", "src/ui/composer.rs", "src/fonts.rs"]
---

## Scope

KodAct desktop's whole shell (title bar, sidebar, chat, composer, settings, opener), restyled from Forge's world into the KodAct CLI's identity. Mode: Operate.

## Audience and task

The CLI's users, on Windows first: long agent sessions, several chats at once, watching the agent work, approving it, coming back to it. The same people run `kodact` in a terminal; the desktop must read as the same product.

## Constraints

Pinned by the owner: the KodAct CLI's identity (ink, paper and lime; the cursor-and-run tile; the pixel KodAct wordmark, "Kod" ink or paper and "Act" lime). Dark first. KodAct's own open-licensed fonts. A short opener, under a second, skippable. Nothing borrowed from Claude's look. Every action, shortcut and screen works as today; standard desktop controls, no terminal costume.

## Direction contract

THESIS: One brand from terminal to window: the desktop is KodAct drawn with real pixels. It refuses the warm-paper, terracotta, chat-app look the category defaults to, and refuses dressing the window as a terminal.

OWN-WORLD: Ink grounds (graphite, a step lighter for the work area, raised tiles for what you act on); paper text in two voices (yours brighter, KodAct's a step softer). Lime only for KodAct acting and what needs you: the mark, the live cursor bar, Send, the current selection. Teal and violet carry an answer's structure; peach marks code. Square-pixel mark and wordmark; a crisp sans of its own; mono only for code, paths and keys.

STORY: At a glance the person sees the brand they know from the terminal, what KodAct is doing (a lime bar, a shimmer), and what needs them; they act on it with the keys they already use.

FIRST VIEWPORT: Title bar with the tile and "KodAct" (Act in lime). Sidebar on the darker ink. The new-chat screen: the pixel wordmark at hero scale, the tagline, the composer as a raised tile with a lime left bar, Send as the run triangle.

FORM: Pinned (owner's brief: the KodAct CLI's identity), not rolled; seed: none (unavailable). Signature move: the mark at work, the cursor bar as what is live and the run triangle as Send.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
