<!-- diagram-design-profile
name: KodAct
slug: forge
source-url: none
created: 2026-10-12
updated: 2026-10-12
notes: KodAct desktop's own skin, bundled with KodAct
-->
# Style Guide

**The single source of truth for colors, typography, and tokens.** Every diagram draws from this — not from hex values inlined in other reference files. If you want to change the visual skin of Diagram Design, change this file.

This copy ships with KodAct in KodAct's own skin: warm paper, graphite ink, one blue accent, Anthropic Sans, Anthropic Sans and Anthropic Sans, so a diagram reads as part of the app. It is the active `kodact` profile; swap these values (or run [`onboarding.md`](onboarding.md)) and every new diagram inherits the new skin without touching any type-specific logic.

To generate your own from a website URL, see [`onboarding.md`](onboarding.md).

---

## Tokens

### Semantic roles

Every token is referred to by **semantic role**, not by its hex value. Type references (`type-*.md`) and SKILL.md say `accent`, not `#f7591f`.

| Role | Purpose | Default (light) | Default (dark) |
|---|---|---|---|
| `paper` | Page background, default node fill | `#f9f7f5` (KodAct stage) | `#1c1b19` (KodAct dark stage) |
| `paper-2` | Diagram container bg, secondary fill | `#f1efeb` (KodAct pane) | `#22211f` (KodAct card) |
| `ink` | Primary text, primary stroke | `#1c1b19` | `#f6f5f4` |
| `ink-strong` | High-contrast text on accent fills | `#ffffff` | `#111111` |
| `muted` | Secondary text, default arrow stroke | `#615d59` (KodAct graphite) | `#b5b0aa` |
| `soft` | Sublabels, boundary labels | `#6f6a64` | `#9d978f` |
| `rule` | Hairline borders | `rgba(28,27,25,0.12)` | `rgba(246,245,244,0.12)` |
| `rule-solid` | Stronger borders, baselines | `#c9c4bd` | `rgba(181,176,170,0.25)` |
| `accent` | Focal / 1–2 max per diagram | `#005fb3` (KodAct blue, a step deeper) | `#4ba1f3` |
| `accent-tint` | Fill for accent-bordered boxes | `rgba(0,95,179,0.08)` | `rgba(75,161,243,0.10)` |
| `link` | HTTP/API calls, external arrows | `#6a4fb4` (KodAct violet) | `#ac93e6` |

> **Brand palette source:** KodAct desktop. `paper`, `paper-2`, `ink` and `muted` are KodAct's own surfaces and text (`src/theme.rs`); the accent is KodAct's one blue, taken a step deeper (`#005fb3` in place of `#0075de`) so it clears 4.5:1 as text on `paper` and under white labels on an 85% accent fill; `link` is KodAct's violet so it never reads as a second blue. Every value is held to the contrast contract below.

> **Note:** The pre-baked example HTML files in `assets/` were built under an earlier skin. Regenerating them against the current `style-guide.md` is a v5.1 task. New diagrams the skill produces will use the tokens above.

### Contrast contract

Every token must clear the threshold for each role it plays, against the background it actually sits on, in both skins. `scripts/verify-contrast.py` computes the WCAG 2.x ratio for every row above against the `paper` of its skin (`ink-strong` against the 0.85 `accent` fill it labels) and fails CI when one falls short.

| Use | Minimum vs. its actual background |
|---|---|
| Regular text, including any `soft`, `link`, or `accent` used as text | 4.5:1 |
| Large text (24px+, or 18.7px+ at weight 600+) | 3:1 |
| Essential non-text marks: lines, arrows, borders, focus outlines, data points | 3:1 against the adjacent color |
| Decorative rules and translucent tints (`rule`, `rule-solid`, `paper-2`, `accent-tint`, fills at opacity) | No minimum, but never the only carrier of meaning |

- **A token used in several roles must pass each one.** `accent` is a stroke in one place and a label in another, so it is held to 4.5:1, not the 3:1 its strokes alone would need. Don't assume a token is non-text because it usually is.
- **Translucent colors are composited first.** An `rgba()` fill or stroke is blended over `paper` before its ratio is measured. A value that clears the bar at full strength can fail at 0.5 opacity.
- **Surfaces other than `paper` cost contrast.** Inside an `accent-tint` box or on `paper-2` every ratio drops. Measure text placed there against that surface, not against `paper`.
- **Current values (KodAct skin):** light `accent` 5.98:1, `soft` 5.01:1, `link` 5.80:1, `muted` 6.11:1 on `#f9f7f5`, white on the 85% accent fill 4.77:1; dark `accent` 6.30:1, `soft` 5.95:1, `link` 6.61:1, `muted` 7.99:1 on `#1c1b19`, `#111111` on the 85% accent fill 5.38:1.

### Inversion rule (light → dark)

Any `rgba(28,25,23, X)` in light becomes `rgba(250,247,242, X)` in dark. Same opacities, RGB flipped. The accent gets a slight hue-shift brighter to read on dark paper.

### Series palette (multi-series chart types only)

A small set of desaturated, editorial-tone colors for chart types that genuinely need to distinguish multiple overlapping entities (currently: **radar**). The "1-focal" rule still holds — `accent` is reserved for the focal series; the palette below covers the rest.

| Token | Light | Dark | Notes |
|---|---|---|---|
| `series-1` | `#7c8f6f` (sage) | `#9caf8f` | Non-focal series |
| `series-2` | `#5e7a9b` (dusty-blue) | `#82a0c0` | Non-focal series |
| `series-3` | `#b8915a` (mustard) | `#d3ad7a` | Non-focal series |
| `series-4` | `#9c6b50` (rust-brown) | `#b88670` | Non-focal series |
| `series-5` | `#6e6479` (slate) | `#8d8298` | Non-focal series |

Fills sit at `0.18` opacity light, `0.22` dark; strokes use the full color. **Don't backfill these tokens to non-chart types** — architecture, swimlane, etc. continue to use muted-ink variants. The series palette is opt-in for diagrams where overlapping shapes demand distinguishable color, not a license to add color elsewhere.

### Terminal skin (opt-in alternate)

A self-contained palette for the terminal-window primitive (see [primitive-terminal.md](primitive-terminal.md)) — a CLI-chrome register for dev-tool posts and technical social cards. It does not replace the default skin above and isn't affected by onboarding; it's a second, fixed skin you opt into per-diagram.

| Token | Hex | Purpose |
|---|---|---|
| `terminal-page` | `#0a0a0a` | Page background behind the window |
| `terminal-paper` | `#141414` | Window body, node fill |
| `terminal-bar` | `#1b1b1b` | Titlebar strip |
| `terminal-border` | `#2b2b2b` | Window border, hairlines |
| `terminal-ink` | `#f5f5f5` | Primary text, primary stroke (same white-smoke as default `ink`) |
| `terminal-muted` | `#9a9a9a` | Secondary text, sublabels, ring stroke |
| `terminal-soft` | `#5c5c5c` | Tertiary — inactive dots, spokes |
| `terminal-accent` | `#ff5a36` | The one accent — focal station, prompt sign, active dot |
| `terminal-accent-tint` | `rgba(255,90,54,0.12)` | Fill for accent-bordered boxes |

**1-accent rule still holds.** Everything that isn't `terminal-ink` or `terminal-muted`/`terminal-soft` should be `terminal-accent` — never introduce a second hue.

---

## Typography

One family, Anthropic Sans, and nothing else: KodAct draws every diagram in
it (it rewrites any other `font-family` before drawing), so naming another
face only makes the file disagree with what is seen.

| Role | Face | Size | Weight | Use |
|---|---|---|---|---|
| `title` | Anthropic Sans Display | 1.75rem | 400 | Page H1 |
| `node-name` | Anthropic Sans Text | 12px | 600 | Human-readable labels |
| `sublabel` | Anthropic Sans Text | 9px | 400 | Port, protocol, URL, field type |
| `eyebrow` | Anthropic Sans Text | 7–8px | 500, tracked 0.18em, uppercase | Type tags, axis labels |
| `arrow-label` | Anthropic Sans Text | 8px | 400, tracked 0.06em | Arrow annotations |
| `callout` | Anthropic Sans Text *italic* | 14px | 400 | Editorial asides only |

### Font stack

```css
--font-sans: 'Anthropic Sans Text';
--font-mono: 'Anthropic Sans Text';
--font-serif: 'Anthropic Sans Display';
```

Write exactly these, with no fallback list and no font `<link>` or
`@import`: a diagram makes no network request. Technical content (ports,
commands, URLs, field types) keeps its smaller size and tracking rather
than a different face.

### Labels in other scripts

Anthropic Sans has no Hangul or Han. Keep the same `font-family`; the
viewer's system draws those glyphs. Measure per character: every wide or
full-width character costs 1em, every other character its Latin advance
(0.60em), nonspacing marks nothing; sum, multiply by the font size, add
padding and round the box up to a multiple of 4. Wide-script labels stay
at 12px or larger, and never go into the 7–8px uppercase slots.

## Stroke, radius, spacing

| Token | Value | Use |
|---|---|---|
| `stroke-thin` | `0.8` | Tag-box outlines, leaf nodes |
| `stroke-default` | `1` | Most strokes |
| `stroke-strong` | `1.2` | Emphasis strokes |
| `radius-sm` | `4` | Small tags |
| `radius-md` | `6` | Node boxes |
| `radius-lg` | `8` | Containers, rings |
| `grid` | `4` | Every coord, size, and gap is divisible by 4 (hard rule) |

---

## Node type → treatment

Semantic role combinations — reference these by name in type specs.

| Type | Fill | Stroke |
|---|---|---|
| `focal` (1–2 max) | `accent-tint` | `accent` |
| `backend` | `#ffffff` (white) | `ink` |
| `store` | `ink @ 0.05` | `muted` |
| `external` | `ink @ 0.03` | `ink @ 0.30` |
| `input` | `muted @ 0.10` | `soft` |
| `optional` | `ink @ 0.02` | `ink @ 0.20` dashed `4,3` |
| `security` | `accent @ 0.05` | `accent @ 0.50` dashed `4,4` |

---

## Customizing the skin

Four options:

1. **Run onboarding** — see [`onboarding.md`](onboarding.md). Drop a URL; the skill extracts the palette + fonts and rewrites this file.
2. **Edit by hand** — change the hex values in the tables above. Run the pre-output taste gate afterward to verify the accent still reads as "focal" against the new paper color.
3. **Brand handoff** — paste your existing design-token JSON into a new section here and map its tokens to the semantic roles above.
4. **Client profiles** — save and switch named skins, or bind one to a project, using [`profiles.md`](profiles.md).

### Constraints (don't break these)

- **Contrast**: every text token (`ink`, `muted`, `soft`, `link`, and `accent`) must clear 4.5:1 on `paper`, in both skins. See the [contrast contract](#contrast-contract); `scripts/verify-contrast.py` checks it.
- **One accent**: pick one color for `accent`. Two accents erases the focal signal.
- **No rainbow palette**: if your brand ships 8 colors, pick 3 (paper, ink, accent). The rest become `muted` variants.
- **Serif + sans + mono**: three families, not more. If brand typography is all sans, keep Anthropic Sans for `title` and `callout` anyway — the contrast is load-bearing.
- **Paper is warm-neutral, not pure white**: pure white turns the design sterile. Pick a cream, bone, or light grey with a hint of warmth.
- **Dot pattern is optional, not default**: the 22×22 dot pattern is an opt-in "dotted paper" variant (good for long-form editorial hero diagrams). The default background is a clean `paper` fill, no pattern. When the pattern is enabled, it should sit at ~10% opacity of `ink` on `paper` — visible but quiet.
- **Container is clean by default**: the diagram sits directly on the page paper, no secondary container background or border. A framed variant (`paper-2` bg + `rule` border + 8px radius + padding) is available as an opt-in for card-heavy layouts, but don't reach for it by default — the extra chrome fights the figure.
