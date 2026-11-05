"""Build assets/kodact.ico (the Windows icon: taskbar, title bar, installer).

KodAct's app icon is its mark (`src/ui/mark.rs`): the cursor bar and the run
triangle, code then act, in a tile with its corners rounded a sixth. The app
icon is the ink tile with the symbol in lime: the OS shows it on light and
dark taskbars alike, and the lime tile would lose its edge on a light one
(1.18:1 on white). A keyline a step lighter than the ink gives it an edge on
a dark taskbar; the lime symbol carries it on any.

Every size Windows asks for is drawn from the geometry on its own
(supersampled, then reduced), so 16 and 24 px stay crisp.

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

TILE = (0x10, 0x12, 0x14)  # theme.rs light `mark_top`: the ink tile
RIM = (0x3A, 0x40, 0x47)  # theme.rs dark `border_strong`: its edge on a dark taskbar
GLYPH = (0xC6, 0xFF, 0x3D)  # theme.rs light `mark_left`, KodAct CLI's BRAND lime

# The glyphs, as fractions of the tile (KodAct CLI's `mark_rows`).
BAR = (0.20, 0.32)
RUN = (0.42, 0.80)
TOP, BOTTOM = 0.24, 0.76


def vector_icon(size):
    """The tile from its geometry, supersampled then reduced."""
    s = size * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    radius = s / 6
    rim = max(SS, round(s / 128))  # about 1px, 2px at 256
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=RIM)
    d.rounded_rectangle([rim, rim, s - 1 - rim, s - 1 - rim], radius=radius - rim, fill=TILE)
    top, bottom = TOP * s, BOTTOM * s
    d.rectangle([BAR[0] * s, top, BAR[1] * s, bottom], fill=GLYPH)
    d.polygon([(RUN[0] * s, top), (RUN[1] * s, s / 2), (RUN[0] * s, bottom)], fill=GLYPH)
    return img.resize((size, size), Image.LANCZOS)


def main():
    images = [vector_icon(n) for n in SIZES]
    big = images[-1]
    big.save(OUT, format="ICO", sizes=[(n, n) for n in SIZES], append_images=images[:-1])
    print(f"wrote {OUT.relative_to(ROOT)} ({len(SIZES)} sizes)")
    if "--preview" in sys.argv:
        sheet = Image.new("RGBA", (sum(SIZES) + 8 * len(SIZES), 256 + 16), (0xF6, 0xF5, 0xF1, 255))
        x = 8
        for n, im in zip(SIZES, images):
            sheet.alpha_composite(im, (x, 8))
            x += n + 8
        out = ROOT / "assets/kodact-ico-preview.png"
        sheet.save(out)
        print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
