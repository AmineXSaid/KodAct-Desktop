"""Build assets/forge.ico (the Windows icon: taskbar, title bar, installer).

Forge's app icon: the ingot (src/ui/ingot.rs) reversed onto a tile of the
accent, so it reads on a light or a dark taskbar alike. The cube is the same
isometric drawing as the in-app mark: its top in paper, like metal at heat,
its left face the accent's shadow and its right the accent lit, the faces a
hair apart so the tile shows between them.

Every size Windows asks for is drawn on its own from the geometry (supersampled,
then reduced), never scaled from another size, so 16 and 24 px stay crisp.

    python3 -I scripts/make_ico.py   # writes assets/forge.ico (+ a preview PNG with --preview)
"""

import math
import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "assets/forge.ico"

# 100/125/150/200% taskbar and title bar, Explorer views, Alt-Tab.
SIZES = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256]
SS = 8  # supersampling

TILE = (0x21, 0x6D, 0x70)  # theme.rs `accent`
TOP = (0xF9, 0xF8, 0xF5)  # paper
LEFT = (0x0E, 0x3B, 0x3D)  # the accent in shadow
RIGHT = (0x8E, 0xCA, 0xCD)  # the accent lit (dark theme's `primary`)

PITCH = math.atan(1 / math.sqrt(2))  # the true isometric view, as ingot.rs


def cube_faces(size, fill, gap, cx, cy):
    """The ingot at rest: (colour, polygon) for left, right and top."""
    sp, cp = math.sin(PITCH), math.cos(PITCH)
    scale = size * fill / (math.sqrt(2) * sp + cp)
    yaw = math.pi / 4
    s, c = math.sin(yaw), math.cos(yaw)

    def p(x, y, z):
        xr = x * c + z * s
        zr = -x * s + z * c
        return (cx + xr * scale, cy + (-y * cp + zr * sp) * scale)

    left = [p(-0.5, 0.5, -0.5), p(-0.5, 0.5, 0.5), p(-0.5, -0.5, 0.5), p(-0.5, -0.5, -0.5)]
    right = [p(-0.5, 0.5, 0.5), p(0.5, 0.5, 0.5), p(0.5, -0.5, 0.5), p(-0.5, -0.5, 0.5)]
    top = [p(-0.5, 0.5, -0.5), p(0.5, 0.5, -0.5), p(0.5, 0.5, 0.5), p(-0.5, 0.5, 0.5)]
    return [(LEFT, inset(left, gap)), (RIGHT, inset(right, gap)), (TOP, inset(top, gap))]


def inset(poly, d):
    """A convex polygon with each edge moved `d` inward (as ingot.rs)."""
    n = len(poly)
    area = sum(poly[i][0] * poly[(i + 1) % n][1] - poly[(i + 1) % n][0] * poly[i][1] for i in range(n)) / 2
    sign = 1 if area > 0 else -1
    lines = []
    for i in range(n):
        a, b = poly[i], poly[(i + 1) % n]
        ex, ey = b[0] - a[0], b[1] - a[1]
        ln = math.hypot(ex, ey)
        nx, ny = -ey / ln * sign, ex / ln * sign
        lines.append(((a[0] + nx * d, a[1] + ny * d), (ex, ey)))
    out = []
    for i in range(n):
        (p, r), (q, s) = lines[i - 1], lines[i]
        cross = r[0] * s[1] - r[1] * s[0]
        t = ((q[0] - p[0]) * s[1] - (q[1] - p[1]) * s[0]) / cross
        out.append((p[0] + r[0] * t, p[1] + r[1] * t))
    return out


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
    # The cube fills more of a small tile, so it still reads at 16 px.
    fill = 0.72 if n <= 24 else 0.64
    gap = max(big / 64, SS * 0.5) if n <= 32 else big / 64
    for colour, poly in cube_faces(big, fill, gap, big / 2, big / 2):
        d.polygon(poly, fill=colour + (255,))
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
