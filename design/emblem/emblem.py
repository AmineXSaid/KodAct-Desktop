"""KodAct's emblem: the mark's cursor bar and run triangle as two solids on a
plinth, drawn as an ink illustration on the flat lime tile.

The flat mark (src/ui/mark.rs) is the small level; this is the large one:
the app icon from 128px up, About, the installer. Three tones, as an inked
illustration on a coloured ground: the lime tile, flat; the forms in paper;
ink for every contour, the hatching where a face turns from the light, and
the cast shadow. No background texture.

    python3 -I design/emblem/emblem.py      # writes design/emblem/*.svg

Writes, in a 256-unit box, a layer each (the app tints them):
  emblem-tile.svg   the tile, with its ink keyline (a fill)
  emblem-paper.svg  the forms' faces (a fill)
  emblem-lines.svg  every ink line and the shadow
  emblem.svg        all three in colour, for previews and the icon
"""

import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))
S = 256.0
INK, LIME, PAPER = "#101214", "#c6ff3d", "#fbf9f3"

# ---- the solids, in model units (x right, y up, z away from the viewer) ----
# The mark's front silhouette (its 64-unit geometry), scaled and placed.
K = 3.7                # model units -> tile units
OX, OY = 60.0, 184.0   # where model (0, 0, 0) lands on the tile (the plinth's top, front-left of the bar)
BAR = (0.0, 20.48 - 12.8)              # x from, to (the bar starts at 0)
TRI = (26.88 - 12.8, 51.2 - 12.8)      # the triangle's back edge and apex
HEIGHT = 48.64 - 15.36                 # both stand 33.28 tall
DEPTH = 8.0                            # how deep each solid is
VIEW = (0.55, 0.38)                    # a unit of depth on the tile: right and up
LIGHT = (0.32, 0.62)                   # a unit of height's shadow on the ground: x and z
PLINTH = (-7.0, 40.0, -4.0, 13.0, 3.6) # x from, to; z from, to; its height under the ground


def P(x, y, z):
    """Model point to tile point (oblique: the front stays true)."""
    return (OX + K * (x + z * VIEW[0]), OY - K * (y + z * VIEW[1]))


def bar_faces():
    x0, x1, h, d = BAR[0], BAR[1], HEIGHT, DEPTH
    return {
        "front": [(x0, 0, 0), (x1, 0, 0), (x1, h, 0), (x0, h, 0)],
        "top": [(x0, h, 0), (x1, h, 0), (x1, h, d), (x0, h, d)],
        "side": [(x1, 0, 0), (x1, 0, d), (x1, h, d), (x1, h, 0)],
    }


def tri_faces():
    t0, t1, h, d = TRI[0], TRI[1], HEIGHT, DEPTH
    m = h / 2
    return {
        "front": [(t0, 0, 0), (t1, m, 0), (t0, h, 0)],
        "upper": [(t0, h, 0), (t1, m, 0), (t1, m, d), (t0, h, d)],
        "lower": [(t0, 0, 0), (t0, 0, d), (t1, m, d), (t1, m, 0)],
    }


def shadow(points):
    """The solid's shadow on the ground: each point dropped along the light."""
    out = []
    for x, y, z in points:
        out.append((x + y * LIGHT[0], 0, z + y * LIGHT[1]))
    return hull([(p[0], p[2]) for p in out])


def hull(pts):
    pts = sorted(set(pts))
    if len(pts) < 3:
        return pts
    cross = lambda o, a, b: (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    lo, hi = [], []
    for p in pts:
        while len(lo) >= 2 and cross(lo[-2], lo[-1], p) <= 0:
            lo.pop()
        lo.append(p)
    for p in reversed(pts):
        while len(hi) >= 2 and cross(hi[-2], hi[-1], p) <= 0:
            hi.pop()
        hi.append(p)
    return lo[:-1] + hi[:-1]


def poly(face):
    return [P(*p) for p in face]


def d_of(pts):
    return "M" + "L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + "Z"


# ---- hatching ----
clips = []
lines = []
fills = []
# What stands in front: the solids' faces, and the plinth's. Lines drawn
# before a silhouette is added are masked out under it, as an engraver
# leaves the paper bare where a nearer form passes.
knock = []


def hatch(name, pts, angle, gap, width, grad=None, cross=None):
    """Parallel lines at `angle` degrees, `gap` apart, clipped to `pts`.
    `grad` (a, b) scales the gap from a at one side to b at the other, so a
    face darkens toward its far edge, as an engraver cuts it."""
    cid = f"c{len(clips)}"
    clips.append(f'<clipPath id="{cid}"><path d="{d_of(pts)}"/></clipPath>')
    a = math.radians(angle)
    ux, uy = math.cos(a), math.sin(a)     # along the line
    nx, ny = -uy, ux                      # across
    xs = [p[0] for p in pts]; ys = [p[1] for p in pts]
    cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    r = math.hypot(max(xs) - min(xs), max(ys) - min(ys)) / 2 + 2
    seg = []
    t = -r
    while t <= r:
        k = (t + r) / (2 * r)
        g = gap if grad is None else gap * (grad[0] + (grad[1] - grad[0]) * k)
        px, py = cx + nx * t, cy + ny * t
        seg.append(f"M{px - ux * r:.2f} {py - uy * r:.2f}L{px + ux * r:.2f} {py + uy * r:.2f}")
        t += max(g, 0.9)
    lines.append(f'<g clip-path="url(#{cid})" stroke-width="{width}"><path d="{"".join(seg)}"/></g>')
    if cross:
        hatch(name + "x", pts, cross[0], cross[1], cross[2])


def contour(pts, width):
    lines.append(f'<path d="{d_of(pts)}" fill="none" stroke-width="{width}" stroke-linejoin="round"/>')


def visible(pts):
    """A face faces the viewer when it winds counter-clockwise on the tile (y down)."""
    area = 0.0
    for (x0, y0), (x1, y1) in zip(pts, pts[1:] + pts[:1]):
        area += x0 * y1 - x1 * y0
    return area < 0


def masked(start, holes):
    """Wrap every line drawn since `start` in a mask that cuts `holes` out."""
    mid = f"m{len(clips)}"
    clips.append(
        f'<mask id="{mid}" maskUnits="userSpaceOnUse" x="0" y="0" width="{S:g}" height="{S:g}">'
        f'<rect width="{S:g}" height="{S:g}" fill="#fff"/>'
        + "".join(f'<path d="{d_of(h)}" fill="#000"/>' for h in holes)
        + "</mask>"
    )
    body = "".join(lines[start:])
    del lines[start:]
    lines.append(f'<g mask="url(#{mid})">{body}</g>')


def fill(pts):
    fills.append(f'<path d="{d_of(pts)}"/>')


def face(pts, hatching=None, width=2.0):
    """A face: paper, an ink contour, and hatching if it turns from the light."""
    if not visible(pts):
        return
    fill(pts)
    if hatching:
        ang, gap, w, grad, cross = hatching
        hatch("f", pts, ang, gap, w, grad=grad, cross=cross)
    contour(pts, width)


def build():
    clips.clear(); lines.clear(); fills.clear()
    bar, tri = bar_faces(), tri_faces()
    x0, x1, z0, z1, ph = PLINTH
    top = [(x0, 0, z0), (x1, 0, z0), (x1, 0, z1), (x0, 0, z1)]
    front = [(x0, -ph, z0), (x1, -ph, z0), (x1, 0, z0), (x0, 0, z0)]
    end = [(x1, -ph, z0), (x1, -ph, z1), (x1, 0, z1), (x1, 0, z0)]
    solids = [poly(f) for sol in (bar, tri) for f in sol.values()]

    # The plinth: its top in paper with the forms' shadows on it, its front
    # lightly hatched, its far end dark.
    face(poly(end), (90, 1.3, 0.9, None, (0, 1.9, 0.6)))
    face(poly(front), (0, 2.6, 0.7, (1.4, 0.8), None))
    start = len(lines)
    fill(poly(top))
    cid = f"c{len(clips)}"
    clips.append(f'<clipPath id="{cid}"><path d="{d_of(poly(top))}"/></clipPath>')
    for solid in (bar, tri):
        pts3 = [p for f in solid.values() for p in f]
        sh = [P(x, 0, z) for x, z in shadow(pts3)]
        hatch("shadow", sh, -10, 1.35, 0.95, cross=(40, 1.9, 0.7))
        lines[-1] = f'<g clip-path="url(#{cid})">{lines[-1]}</g>'
        lines[-2] = f'<g clip-path="url(#{cid})">{lines[-2]}</g>'
    contour(poly(top), 2.0)
    masked(start, solids)

    # The triangle, then the bar: lit fronts and tops clean, the turned
    # faces hatched densest at their far edge.
    t = tri
    face(poly(t["upper"]), (-33, 3.4, 0.6, (1.8, 0.8), None))
    face(poly(t["lower"]), (70, 1.3, 0.9, (1.4, 0.7), (-24, 1.8, 0.6)))
    face(poly(t["front"]), (90, 6.5, 0.5, (0.4, 3.6), None))
    b = bar
    face(poly(b["top"]))
    face(poly(b["side"]), (90, 1.1, 0.9, (1.6, 0.7), (14, 1.7, 0.6)))
    face(poly(b["front"]), (90, 6.5, 0.5, (0.4, 3.6), None))


def tile_path():
    r = S / 6
    a = S - r
    return f"M{r} 0H{a}A{r} {r} 0 0 1 {S} {r}V{a}A{r} {r} 0 0 1 {a} {S}H{r}A{r} {r} 0 0 1 0 {a}V{r}A{r} {r} 0 0 1 {r} 0Z"


def group(color):
    return (f'<defs>{"".join(clips)}</defs><g fill="none" stroke="{color}" stroke-linecap="round" stroke-linejoin="round">'
            + "".join(lines) + "</g>")


def doc(body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {S:g} {S:g}">{body}</svg>\n'


def keyline(color):
    """The tile's edge: an ink keyline just inside it, so the lime holds on
    white as on ink."""
    k = S / 256 * 3
    return (f'<path d="{tile_path()}" fill="none" stroke="{color}" stroke-width="{2 * k:g}" '
            f'transform="translate({k / 2:g} {k / 2:g}) scale({(S - k) / S:g})"/>')


def main():
    build()
    w = lambda n, s: open(os.path.join(HERE, n), "w").write(s)
    paper = f'<g fill="{{c}}">{"".join(fills)}</g>'
    w("emblem-tile.svg", doc(f'<path d="{tile_path()}"/>'))
    w("emblem-keyline.svg", doc(keyline("#000")))
    w("emblem-paper.svg", doc(paper.format(c="#000")))
    w("emblem-lines.svg", doc(group("#000")))
    w("emblem.svg", doc(f'<path d="{tile_path()}" fill="{LIME}"/>' + keyline(INK)
                        + paper.format(c=PAPER) + group(INK)))
    for old in ("emblem-lime.svg", "emblem-ink.svg"):
        if os.path.exists(os.path.join(HERE, old)):
            os.remove(os.path.join(HERE, old))
    print("wrote", ", ".join(n for n in sorted(os.listdir(HERE)) if n.endswith(".svg")))


if __name__ == "__main__":
    main()
