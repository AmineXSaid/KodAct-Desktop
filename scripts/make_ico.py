"""Build assets/forge.ico (the Windows icon: taskbar, title bar, installer).

The app icon: KodAct's mark (src/ui/kodact_mark.rs) in paper on a tile of the
accent, the turquoise as it is set on paper, so it reads on a light or a dark
taskbar alike. The mark is the same drawing as the in-app one: a step up from
the left, then a chevron pointing right, their corners rounded by the join.

Every size Windows asks for is drawn on its own from the geometry (supersampled,
then reduced), never scaled from another size, so 16 and 24 px stay crisp.

    python3 -I scripts/make_ico.py   # writes assets/forge.ico (+ a preview PNG with --preview)
"""

import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "assets/forge.ico"

# 100/125/150/200% taskbar and title bar, Explorer views, Alt-Tab.
SIZES = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256]
SS = 8  # supersampling

TILE = (0x0E, 0x75, 0x68)  # theme.rs `accent`, light
MARK = (0xFF, 0xFF, 0xFF)  # paper

# The mark, as kodact_mark.rs: its box, its two strokes, the join's width.
VIEW = (505.7, 302.0)
STEP = [(4, 242), (177.17, 100), (241, 100), (295.88, 145), (244, 145), (125.7, 242)]
CHEVRON = [
    (316, 4),
    (497.71, 153),
    (320.88, 298),
    (171, 298),
    (307.59, 186),
    (347, 186),
    (378.71, 160),
    (277.49, 77),
    (226.98, 77),
]
JOIN = 8.0


def strokes(size, fill):
    """The mark centred in a `size` box, `fill` of it wide: (polygons, join width)."""
    s = size * fill / VIEW[0]
    ox, oy = (size - VIEW[0] * s) / 2, (size - VIEW[1] * s) / 2
    return [[(ox + x * s, oy + y * s) for x, y in poly] for poly in (STEP, CHEVRON)], JOIN * s


def icon(n):
    big = n * SS
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # Small sizes fill the whole square (Windows draws no plate behind them);
    # the corner radius follows Windows 11's app tiles, about a fifth.
    margin = 0 if n <= 24 else round(big * 0.04)
    d.rounded_rectangle(
        (margin, margin, big - 1 - margin, big - 1 - margin),
        radius=round((big - 2 * margin) * 0.22),
        fill=TILE + (255,),
    )
    # The mark fills more of a small tile, so it still reads at 16 px.
    polys, join = strokes(big, 0.8 if n <= 24 else 0.68)
    for poly in polys:
        d.polygon(poly, fill=MARK + (255,))
        d.line(poly + poly[:2], fill=MARK + (255,), width=max(round(join), 1), joint="curve")
    return img.resize((n, n), Image.LANCZOS)


def main() -> int:
    frames = [icon(n) for n in SIZES]
    frames[-1].save(OUT, format="ICO", sizes=[f.size for f in frames], append_images=frames[:-1])
    print(f"{OUT.relative_to(ROOT)}: {len(frames)} sizes")
    if "--preview" in sys.argv:
        strip = Image.new("RGBA", (sum(SIZES) + 8 * len(SIZES), 256), (0, 0, 0, 0))
        x = 0
        for f in frames:
            strip.alpha_composite(f, (x, 256 - f.height))
            x += f.width + 8
        strip.save(sys.argv[sys.argv.index("--preview") + 1])
    return 0


if __name__ == "__main__":
    sys.exit(main())
