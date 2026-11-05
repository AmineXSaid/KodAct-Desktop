"""Build assets/kodact.ico (the Windows icon: taskbar, title bar, installer).

KodAct's app icon is its mark, as KodAct CLI draws it (`tui/brand.rs`) and the
app does (`src/ui/mark.rs`): a lime tile, corners rounded, holding an ink
cursor bar and a run triangle (code, then act). A rim a step deeper than the
lime keeps its edge on a light taskbar; the ink glyphs carry it on any.

Every size Windows asks for is drawn on its own: where a size is a whole
number of the CLI's 16-pixel grid (48, 64, 96, 128, 256) the tile is drawn
on that grid, square pixels as the brand's material; every other size is
drawn from the geometry (supersampled, then reduced), so 16 and 24 px stay
crisp.

    python3 -I scripts/make_ico.py   # writes assets/kodact.ico (+ a preview PNG with --preview)
"""

import pathlib
import sys

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "assets/kodact.ico"

# 100/125/150/200% taskbar and title bar, Explorer views, Alt-Tab.
SIZES = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256]
SS = 8  # supersampling
GRID = 16

TILE = (0xC6, 0xFF, 0x3D)  # theme.rs dark `accent`, KodAct CLI's BRAND
RIM = (0x8F, 0xC4, 0x1E)  # the lime a step deeper: the tile's edge on paper
INK = (0x10, 0x12, 0x14)  # the glyphs' ink

# The glyphs, as fractions of the tile (KodAct CLI's `mark_rows`).
BAR = (0.20, 0.32)
RUN = (0.42, 0.80)
TOP, BOTTOM = 0.24, 0.76


def tile_rows(n):
    """The tile on the CLI's grid: '.' empty, 'M' tile, 'K' ink."""
    f = lambda v: round(v * n)
    bar0 = f(BAR[0])
    bar1 = max(f(BAR[1]), bar0 + 1)
    tri0 = max(f(RUN[0]), bar1 + 1)
    tri1 = max(f(RUN[1]), tri0 + 2)
    top, bottom = f(TOP), f(BOTTOM)
    mid = (top + bottom) / 2
    half = (bottom - top) / 2
    rows = []
    for y in range(n):
        row = ""
        for x in range(n):
            r = 2 if n >= 16 else 1
            if min(x, n - 1 - x) + min(y, n - 1 - y) < r:
                row += "."
                continue
            inside = top <= y < bottom
            bar = inside and bar0 <= x < bar1
            reach = 1 - abs(y + 0.5 - mid) / half
            tri = inside and x >= tri0 and x < tri0 + (tri1 - tri0) * reach
            row += "K" if bar or tri else "M"
        rows.append(row)
    return rows


def pixel_icon(size):
    """The tile on its 16-pixel grid, each grid pixel size/16 px, a 1-pixel rim."""
    k = size // GRID
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    rows = tile_rows(GRID)
    filled = lambda x, y: 0 <= x < GRID and 0 <= y < GRID and rows[y][x] != "."
    for y, row in enumerate(rows):
        for x, c in enumerate(row):
            if c == ".":
                continue
            edge = not all(filled(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
            colour = INK if c == "K" else (RIM if edge else TILE)
            d.rectangle([x * k, y * k, (x + 1) * k - 1, (y + 1) * k - 1], fill=colour)
    return img


def vector_icon(size):
    """The tile from its geometry, supersampled then reduced."""
    s = size * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    radius = s / 6
    rim = max(SS, round(s / 32))
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=RIM)
    d.rounded_rectangle([rim, rim, s - 1 - rim, s - 1 - rim], radius=radius - rim, fill=TILE)
    top, bottom = TOP * s, BOTTOM * s
    d.rectangle([BAR[0] * s, top, BAR[1] * s, bottom], fill=INK)
    d.polygon([(RUN[0] * s, top), (RUN[1] * s, s / 2), (RUN[0] * s, bottom)], fill=INK)
    return img.resize((size, size), Image.LANCZOS)


def icon(size):
    if size % GRID == 0 and size >= 48:
        return pixel_icon(size)
    return vector_icon(size)


def main():
    images = [icon(n) for n in SIZES]
    big = images[-1]
    big.save(OUT, format="ICO", sizes=[(n, n) for n in SIZES], append_images=images[:-1])
    print(f"wrote {OUT.relative_to(ROOT)} ({len(SIZES)} sizes)")
    if "--preview" in sys.argv:
        sheet = Image.new("RGBA", (sum(SIZES) + 8 * len(SIZES), 256 + 16), (0x15, 0x18, 0x1B, 255))
        x = 8
        for n, im in zip(SIZES, images):
            sheet.alpha_composite(im, (x, 8))
            x += n + 8
        out = ROOT / "assets/kodact-ico-preview.png"
        sheet.save(out)
        print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
