"""Build assets/forge.ico (the Windows icon: taskbar, title bar, installer)
from assets/images/cube-original.webp.

The cube is cut to its visible edge (the original's faint haze around it
dropped), so it fills the icon's full height instead of ~90%, and each size
Windows asks for is rendered on its own (no size is left for Windows to
scale), the small ones sharpened so the blocks still read at 16-32 px.

    python3 -I scripts/make_ico.py
"""

import pathlib
import sys

from PIL import Image, ImageFilter

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "assets/images/cube-original.webp"
OUT = ROOT / "assets/forge.ico"

# 100/125/150/200% taskbar and title bar, Explorer views, Alt-Tab.
SIZES = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256]


def main() -> int:
    src = Image.open(SRC).convert("RGBA")
    # The visible cube: alpha above the haze.
    box = src.getchannel("A").point(lambda a: 255 if a > 16 else 0).getbbox()
    cube = src.crop(box)
    side = max(cube.size)
    square = Image.new("RGBA", (side, side))
    square.alpha_composite(cube, ((side - cube.width) // 2, (side - cube.height) // 2))
    # Anything fainter than the haze cut goes, so the edge is clean.
    alpha = square.getchannel("A").point(lambda a: 0 if a <= 16 else a)
    square.putalpha(alpha)

    frames = []
    for n in SIZES:
        img = square.resize((n, n), Image.LANCZOS)
        if n <= 48:
            img = img.filter(ImageFilter.UnsharpMask(radius=0.6, percent=60, threshold=1))
        frames.append(img)
    big = frames[-1]
    big.save(OUT, format="ICO", sizes=[f.size for f in frames], append_images=frames[:-1])
    print(f"{OUT.relative_to(ROOT)}: {len(frames)} sizes, cube box {box} of {src.size}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
