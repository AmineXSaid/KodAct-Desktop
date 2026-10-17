# Forge: brand profile for films

Read before every film (brand-motion-design skill, Step 0). Source of truth:
the forge-gpui codebase (`src/theme.rs`, `.interface-design/system.md`,
`src/ui/ingot.rs`, `src/ui/dot_field.rs`, `assets/fonts/`).

## The brand

- **Product:** Forge, a desktop app where coding agents work in your
  projects. Each chat runs ForgeCLI; Forge asks before it changes anything.
- **Loop, in three lines:** Describe it. Forge builds it. You approve.
- **Approved line:** "Forge asks before it changes anything." (the app's own copy)
- **Mark:** the ingot, an isometric cube of three flat faces (top in the
  accent). Wordmark: "forge" in Source Serif 4 Display with the ingot as its o.
- **Patterns:** the dot field (a fine grid of warm dots, quiet round the
  content, a ring running through it on a strike); the dotted orbit with a
  spark under the hero cube; hairline borders; flat tonal surfaces.

## Palette (hex, from `src/theme.rs`, dark mode)

| Role | Hex |
|---|---|
| Stage (window) | #262624 |
| Chrome (title bar) | #141312 |
| Pane | #1d1c1b |
| Surface (cards, fields) | #30302e |
| Paper (text strong, ingot left face) | #ecebe6 |
| Text muted | #a8a59c |
| Text subtle, dots | #7d7a72 / #9a958b |
| Accent (fill) | #216d70 |
| Ingot top (dark) | #529a9d |
| Link teal | #8ecacd |
| Ingot right face (dark) | #6f6c64 |
| Heat: white, glow, ember, iron | #fff2cc, #f5892c, #a3311c, #3b3633 |

Light mode: stage #f9f8f5, accent #216d70, ingot faces #216d70 / #2b2a27 / #8c887e.

## Type

- Hanken Grotesk (UI, words in films), Source Serif 4 Display (greeting,
  wordmark), JetBrains Mono (HUD, code). Files in `assets/fonts/` (OFL).

## Brand moment

What bends in a film: pace, light, depth, 3D, transitions, slams. What never
bends: the palette above, the ingot geometry and colours, the fonts, the
voice (calm, plain, sentence case), and the claims (only what Forge does).

## Owner choices

- Films open the desktop app; 16:9; first launch ends on the naming screen.
- Silent by default (an app should not play music when it opens).
- Inspiration the owner gave: the Forge for VS Code welcome art (a smith,
  a hammer, the cube shattering), a raw star falling with an aura and light
  on the ground, the ingot cooling from white hot through orange, ember and
  iron. Respect forge-gpui's design and colours over the VS Code purple.

## Chosen by Claude

- Type: brand teaser. Music: none. Words in Hanken Grotesk Bold, sentence case.
- The star's aura is the link teal, not a new blue, to stay in the palette.

## Films

- `opener` (2026-10-17): first-launch opener, 12.9 s, 16:9.
