# KodAct emblem: brief for the final art

The emblem in this folder (`emblem.py`, drawn by code) is a working stand-in.
This brief is for an illustrator, or an image model, to make the final art
on the same composition. It replaces only the drawing: the files, layers,
sizes and placements stay as they are.

## The subject

KodAct's mark made solid: the **cursor bar** as a tall rectangular block and
the **run triangle** as a triangular prism pointing right, standing side by
side on a **low plinth**. It reads as "code, then act". No living being, no
character, no face.

- Front view kept true, so the two front faces are the flat mark's exact
  silhouette (bar 20 to 32% of the tile, triangle 42 to 80%, both 24 to 76%
  high). Depth recedes up and to the right (oblique), about a quarter of the
  bar's height.
- Light from the upper left, in front. The shadows fall back and to the
  right on the plinth's top.
- The plinth is a little wider than the pair and about a tenth of their
  height, its right end in shade.

## The treatment

Three tones only, as an inked illustration on a coloured ground (the style of
an engraved app icon, not a 3D render):

| Tone | Colour | Where |
|---|---|---|
| Ground | lime `#c6ff3d` | the tile, flat, no texture, no gradient |
| Paper | `#fbf9f3` | every face of the forms and the plinth |
| Ink | `#101214` | contours, hatching, the cast shadows, a keyline just inside the tile |

- Contours about 2px at 256px, even weight.
- Hatching only where a face turns from the light: the bar's right side and
  the triangle's lower face densest (cross-hatched), the plinth's end dark,
  its front lightly lined. Lit fronts and tops stay almost bare paper; a few
  fine lines near their far edges at most.
- The cast shadows are cross-hatched ink on the plinth's paper.
- Nothing behind the forms: no sky, no frame, no border rule, no texture.
- The tile is a square with its corners rounded a sixth of its side.

## Don't

- No gradients, glows, bevels, drop shadows or 3D rendering.
- No other colours, and no lime inside the forms.
- No animal, figure, face or hand.
- Don't borrow any other product's emblem or composition.

## Deliverables

- An SVG in a 256 by 256 box, in four layers that each use one flat colour
  (the app tints them): `emblem-tile.svg` (the tile shape),
  `emblem-keyline.svg`, `emblem-paper.svg` (the forms' faces as fills),
  `emblem-lines.svg` (all ink: contours, hatching, shadows). Plus
  `emblem.svg`, the three tones together.
- It has to hold at 128px (the smallest place it is used), and at 1024px
  for the icon's largest size.

## Prompt for an image model

> An app icon: a flat lime-green (#c6ff3d) rounded-square tile with a thin
> black keyline. On it, a fine black-ink engraved illustration of two
> geometric solids standing on a low plinth: a tall rectangular block on the
> left and a triangular prism pointing right, side by side, like a "pause
> bar and play triangle" made solid, seen front-on with depth receding up
> and to the right. Every face in off-white paper; black contour lines;
> dense cross-hatching only on the faces turned away from a light at the
> upper left and in the shadows they cast back on the plinth. Flat
> background, no texture, no gradient, no 3D render, no other colours.

Then trace it to the four layers above, or hand it to an illustrator to
redraw cleanly.
