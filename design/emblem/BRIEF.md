# KodAct emblem: the brief

KodAct's logo is an **illustrated emblem**: one subject, drawn by hand with
real character, on the flat lime tile. The bar is Claude's spark: a mark that
looks made by a person, with its own irregular rhythm, not assembled from
geometric parts. Everything drawn in code so far (`design/logo/`,
`emblem.py`) was geometric and read as generic; it is a placeholder only.

Read `BRAND.md` first: the Operator, "Code, then act, on the model you
choose", the control panel of a precise machine at night, one lime indicator
that is on when KodAct acts.

## What it must be

- **One subject, front and centre,** filling most of the tile, as an
  illustrated app icon (the ArtCraft family is the reference for the format
  only: brand above, icon, product name below; not its animals).
- **Not a living being.** No animal, person, face, hand, robot or mascot.
- **An object from the Operator's world,** made with care: something you
  control and that acts when you tell it to.
- **Hand-made:** ink line with weight and slight irregularity, as a pen or a
  burin cuts it. Not vector-perfect, not 3D-rendered.
- **Three tones only:** the lime tile `#c6ff3d` (flat, no texture, no
  gradient), the subject in paper `#fbf9f3` and ink `#101214`. Ink carries
  every line, all shading (hatching) and the shadow.
- **A silhouette that survives:** at 128px the subject still reads; below
  that the app uses a flat one-colour silhouette cut from the same drawing
  (deliver it too).

## Never

Play or pause shapes; a letter K; `>_`; chips, circuits, brains, sparkles,
glow, gradients, neon, purple; anything from `BRAND.md`'s anti-references.

## Three subjects to brief (pick one, or brief all three and choose)

1. **The two monoliths.** The cursor bar as a tall block and the run
   triangle as a prism, standing side by side on a low stone plinth, like a
   monument. (The owner liked this form; the drawn treatment was the
   problem.) Drawn by hand, the stone given texture by hatching, the
   triangle's edge just catching the light.
2. **The indicator lamp.** A single old machine indicator lamp, a heavy
   bezel and a domed lens, set in a riveted panel plate, the lens lit: the
   one lime light that is on when KodAct acts. On the lime tile the lens is
   left as bare lime inside the ink drawing, so the light is the tile itself.
3. **The throw switch.** A heavy knife switch or lever on a panel, thrown to
   on: the Operator's hand has just set it going. Industrial, honest, made of
   metal and porcelain.

## Prompts for an image model

Use one per subject; generate several, pick, then have it redrawn or traced
cleanly into the layers below.

**1 · Monoliths**
> App icon. A flat lime-green (#c6ff3d) rounded-square tile, no texture. In
> the centre, a hand-inked pen-and-ink engraving of two stone monoliths on a
> low plinth: a tall rectangular block on the left and a triangular prism
> pointing right, side by side. Off-white paper faces, black ink linework
> with natural weight variation, fine hatching on the shaded faces and in the
> shadow on the plinth. Hand-drawn, slightly irregular lines, like an old
> engraving. Only three colours: lime, off-white, black. No gradients, no
> glow, no 3D render, no text.

**2 · Indicator lamp**
> App icon. A flat lime-green (#c6ff3d) rounded-square tile, no texture. In
> the centre, a hand-inked pen-and-ink engraving of a single vintage machine
> indicator lamp in a riveted metal panel plate: a heavy round bezel and a
> domed glass lens, seen slightly from above. The lens is left the same lime
> as the background, as if lit. Off-white metal, black ink linework with
> natural weight, fine hatching for the shading. Hand-drawn, slightly
> irregular, like an old technical engraving. Only lime, off-white and black.
> No gradients, no glow, no 3D render, no text.

**3 · Throw switch**
> App icon. A flat lime-green (#c6ff3d) rounded-square tile, no texture. In
> the centre, a hand-inked pen-and-ink engraving of a heavy industrial knife
> switch on a porcelain base, its lever thrown up to on. Off-white porcelain
> and metal, black ink linework with natural weight variation, fine hatching
> for the shading. Hand-drawn, slightly irregular, like an old catalogue
> engraving. Only lime, off-white and black. No gradients, no glow, no 3D
> render, no text.

## Deliverables

- The emblem as layered SVG in a 256 by 256 box, each layer one flat colour
  (the app tints them): `emblem-tile.svg` (the tile shape, corners rounded a
  sixth of its side), `emblem-paper.svg` (the subject's fills),
  `emblem-lines.svg` (all ink), and `emblem.svg` (all three in colour).
- `emblem-silhouette.svg`: the subject as one flat shape, for 16 to 96px
  (the taskbar, the title bar, the favicon).
- A 1024px PNG of the full-colour emblem (`emblem-1024.png`) for the
  Windows icon's largest sizes.

Dropped into this folder, these replace the placeholder; the app reads them
through `src/ui/mark.rs` and `scripts/make_ico.py` with no other change.
