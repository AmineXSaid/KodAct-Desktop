"""Build assets/kodact.ico (the Windows icon: taskbar, title bar, installer).

The app icon: KodAct's mark (src/ui/kodact_mark.rs) alone, with no tile behind
it, on a transparent ground like the other icons on a Windows 11 taskbar. The
mark is the same drawing as the in-app one: a step up from the left, then a
chevron pointing right, their corners rounded by the join. Each stroke is
shaded top to bottom (the step deep, the chevron bright) over a soft shadow, so
it reads on a light or a dark taskbar alike.

The mark keeps its own proportions (never stretched) and is drawn at 180% of
the icon's width (SCALE), so it fills the square like the other taskbar icons:
the icon crops the step's tail, the chevron's point and a little of the top and
bottom, and what is left is centred.

Every size Windows asks for is drawn on its own from the geometry (supersampled,
then reduced), never scaled from another size, so 16 and 24 px stay crisp.

    python3 -I scripts/make_ico.py   # writes assets/kodact.ico (+ a preview PNG with --preview)
"""

import pathlib
import sys

from PIL import Image, ImageDraw, ImageFilter

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "assets/kodact.ico"

# 100/125/150/200% taskbar and title bar, Explorer views, Alt-Tab.
SIZES = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256]
SS = 8  # supersampling

# Each stroke's shading, top to bottom: the accent's turquoise (theme.rs).
STEP_SHADE = ((0x1A, 0x8F, 0x80), (0x08, 0x55, 0x4C))
CHEVRON_SHADE = ((0x7F, 0xE6, 0xD3), (0x12, 0x9A, 0x87))
SHADOW = 0.35  # the shadow's opacity; it falls 3% of the icon below the mark

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
SCALE = 1.8  # the mark's width, as a share of the icon's


def stroke_mask(poly, size, dy=0.0):
    """One stroke, full width and centred (a touch high, for the shadow), as a mask."""
    s = size * SCALE / VIEW[0]
    sy = s
    oy = (size - VIEW[1] * sy) / 2 - size * 0.01 + dy * size
    ox = (size - VIEW[0] * s) / 2
    pts = [(ox + x * s, oy + y * sy) for x, y in poly]
    m = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(m)
    d.polygon(pts, fill=255)
    d.line(pts + pts[:2], fill=255, width=max(round(JOIN * s), 1), joint="curve")
    return m


def shade(size, top, bottom):
    col = Image.new("RGBA", (1, size))
    for y in range(size):
        t = y / (size - 1)
        col.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    return col.resize((size, size), Image.NEAREST)


def icon(n):
    big = n * SS
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    shadow = stroke_mask(STEP, big, 0.03)
    shadow.paste(255, (0, 0), stroke_mask(CHEVRON, big, 0.03))
    shadow = shadow.filter(ImageFilter.GaussianBlur(big * 0.02)).point(lambda v: round(v * SHADOW))
    img.paste((0, 0, 0, 255), (0, 0), shadow)
    img.paste(shade(big, *STEP_SHADE), (0, 0), stroke_mask(STEP, big))
    img.paste(shade(big, *CHEVRON_SHADE), (0, 0), stroke_mask(CHEVRON, big))
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
