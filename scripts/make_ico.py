"""Build assets/kodact.ico (the Windows icon: taskbar, title bar, Start,
Explorer, the installer).

The icon is KodAct's mark, `./<` (src/ui/kodact_mark.rs: the dot, the
slash and the angle that read as a K), in paper white on a burgundy tile
(`#7a1f3d`, the app's one colour), with a soft shadow that lifts the tile off
a busy wallpaper. The tile's rounded corners follow Windows 11's.

Every size Windows asks for is drawn on its own from the geometry
(supersampled, then reduced), never scaled from another size, so 16 and
24 px stay crisp; the keyline never drops below one pixel.

    python3 -I scripts/make_ico.py                  # writes assets/kodact.ico
    python3 -I scripts/make_ico.py --preview out.png  # and a sheet of it on desktops
"""

import pathlib
import sys

from PIL import Image, ImageDraw, ImageFilter

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "assets/kodact.ico"

# 100/125/150/200% taskbar and title bar, Explorer views, Alt-Tab, Start.
SIZES = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256]
SS = 8  # supersampling

# The mark, as kodact_mark.rs: its box and its three strokes, in drawing order
# (the angle last, over the slash it meets).
VIEW = (260.0, 252.0)
DOT = [
    (53.00, 205.00), (51.63, 211.89), (47.73, 217.73), (41.89, 221.63),
    (35.00, 223.00), (28.11, 221.63), (22.27, 217.73), (18.37, 211.89),
    (17.00, 205.00), (18.37, 198.11), (22.27, 192.27), (28.11, 188.37),
    (35.00, 187.00), (41.89, 188.37), (47.73, 192.27), (51.63, 198.11),
]
SLASH = [(68.0, 224.0), (96.0, 232.0), (148.0, 22.0), (120.0, 14.0)]
ANGLE = [(114.0, 130.0), (223.0, 22.0), (243.0, 42.0), (154.0, 130.0), (243.0, 218.0), (223.0, 238.0)]
JOIN = 3.0  # the strokes' rounded join, in the mark's units

# Colours: the burgundy tile (lit a touch along the top) and the paper mark.
TILE = ((0x8A, 0x26, 0x47), (0x6C, 0x1A, 0x35))  # `#7a1f3d` at the middle
PAPER = ((0xFF, 0xFC, 0xF8), (0xF0, 0xEA, 0xE2))
STROKES = [(DOT, PAPER), (SLASH, PAPER), (ANGLE, PAPER)]

TILE_FILL = 0.94  # the tile's width, as a share of the icon
TILE_RADIUS = 0.22  # its corner radius, as a share of the tile
FILL = 0.56  # the mark's width, as a share of the icon
SHADOW = 0.35  # the shadow's opacity; it falls 2% of the icon below the tile


def mask(poly, size, dy=0.0):
    """One stroke filled, the mark centred in a `size` square, as a mask."""
    s = size * FILL / VIEW[0]
    ox = (size - VIEW[0] * s) / 2
    oy = (size - VIEW[1] * s) / 2 + dy * size
    pts = [(ox + x * s, oy + y * s) for x, y in poly]
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


def tile(big, dy=0.0):
    """The rounded tile as a mask."""
    m = Image.new("L", (big, big), 0)
    side = big * TILE_FILL
    o = (big - side) / 2
    y = o + dy * big
    ImageDraw.Draw(m).rounded_rectangle([o, y, o + side, y + side], radius=side * TILE_RADIUS, fill=255)
    return m


def icon(n):
    big = n * SS
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))

    # The soft shadow under the tile.
    shadow = tile(big, 0.02).filter(ImageFilter.GaussianBlur(big * 0.02)).point(lambda v: round(v * SHADOW))
    img.paste((0, 0, 0, 255), (0, 0), shadow)

    # The burgundy tile, then the paper mark on it.
    img.paste(shade(big, *TILE), (0, 0), tile(big))
    for poly, colours in STROKES:
        img.paste(shade(big, *colours), (0, 0), mask(poly, big))
    return img.resize((n, n), Image.LANCZOS)


def desktops(w, h):
    """Wallpapers to judge it on: white, black, Windows 11's light and dark
    bloom, a warm photo-like gradient, and a busy pattern."""
    out = []
    for c in [(255, 255, 255), (0, 0, 0)]:
        out.append(("plain", Image.new("RGBA", (w, h), c + (255,))))

    def grad(a, b, c):
        img = Image.new("RGBA", (w, h))
        for y in range(h):
            for x in range(w):
                t, u = x / (w - 1), y / (h - 1)
                img.putpixel(
                    (x, y),
                    tuple(round(a[i] * (1 - t) * (1 - u) + b[i] * t + c[i] * u * (1 - t)) for i in range(3)) + (255,),
                )
        return img

    out.append(("bloom light", grad((0xDD, 0xE8, 0xF7), (0x9F, 0xC3, 0xF0), (0xF4, 0xF6, 0xFB))))
    out.append(("bloom dark", grad((0x0B, 0x1A, 0x3A), (0x1F, 0x4E, 0xA8), (0x05, 0x08, 0x14))))
    out.append(("photo", grad((0xF2, 0xB8, 0x6A), (0x5A, 0x9E, 0x4B), (0x2E, 0x55, 0x7A))))
    busy = Image.new("RGBA", (w, h), (0x7C, 0xB3, 0x42, 255))
    d = ImageDraw.Draw(busy)
    for i in range(0, w + h, 14):
        d.line([(i, 0), (i - h, h)], fill=(0xE8, 0xF5, 0xD0, 255), width=6)
    out.append(("busy", busy))
    return out


def preview(frames, path):
    shown = [f for f in frames if f.width in (16, 24, 32, 48, 96, 256)]
    w = sum(f.width for f in shown) + 24 * (len(shown) + 1)
    h = 256 + 48
    rows = []
    for _, bg in desktops(w, h):
        row = bg.copy()
        x = 24
        for f in shown:
            row.alpha_composite(f, (x, (h - f.height) // 2))
            x += f.width + 24
        rows.append(row)
    sheet = Image.new("RGBA", (w, h * len(rows)))
    for i, r in enumerate(rows):
        sheet.paste(r, (0, i * h))
    sheet.save(path)


def main() -> int:
    frames = [icon(n) for n in SIZES]
    frames[-1].save(OUT, format="ICO", sizes=[f.size for f in frames], append_images=frames[:-1])
    print(f"{OUT.relative_to(ROOT)}: {len(frames)} sizes")
    if "--preview" in sys.argv:
        preview(frames, sys.argv[sys.argv.index("--preview") + 1])
    return 0


if __name__ == "__main__":
    sys.exit(main())
