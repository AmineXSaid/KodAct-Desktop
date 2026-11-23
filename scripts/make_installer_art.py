"""Draw the installer's pictures (installer/kodact.nsi): the welcome and
finish pages' side panel and the inner pages' header picture.

The side panel is KodAct's burgundy (#7a1f3d, the app's one colour) in a soft
vertical gradient, a faint grid of slopes in the mark's own angle, the app
icon, the name "KodAct" (Act in the lifted rose the app uses on dark,
#e0889f) and the tagline FROM CODE TO IMPACT. The header picture is the icon
alone on white, as the inner pages' header is white.

Each picture is drawn at twice its size on screen, so it stays sharp at 200%
scaling (the installer stretches it to fit, keeping its aspect).

    python3 -I scripts/make_installer_art.py OUT_DIR
"""

import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICON = ROOT / "assets/kodact.ico"

WINE_DEEP = (0x4A, 0x12, 0x25)
WINE = (0x7A, 0x1F, 0x3D)
WINE_LIGHT = (0x93, 0x2E, 0x50)
ROSE = (0xE0, 0x88, 0x9F)
PAPER = (0xFF, 0xFF, 0xFF)

# The welcome page's side panel is 164 x 314 dialog pixels; the header 150 x 57.
SIDE = (164 * 2, 314 * 2)
HEADER = (150 * 2, 57 * 2)


def font(size, weight="SemiBold"):
    """Inter (OFL) where the build machine has it, else PIL's own face."""
    for path in (
        f"/usr/share/fonts/opentype/inter/Inter-{weight}.otf",
        f"/usr/share/fonts/truetype/inter/Inter-{weight}.ttf",
        f"/usr/share/fonts/opentype/inter/InterDisplay-{weight}.otf",
    ):
        if pathlib.Path(path).is_file():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default(size)


def icon(size):
    """The app icon's largest frame, scaled to `size`."""
    ico = Image.open(ICON)
    ico.size = max(ico.info["sizes"])
    return ico.convert("RGBA").resize((size, size), Image.LANCZOS)


def mix(a, b, k):
    return tuple(round(x + (y - x) * k) for x, y in zip(a, b))


def side_panel():
    w, h = SIDE
    img = Image.new("RGB", SIDE, WINE)
    d = ImageDraw.Draw(img)
    # Deep at the top, the brand burgundy through the middle, lighter below.
    for y in range(h):
        k = y / (h - 1)
        c = mix(WINE_DEEP, WINE, k / 0.55) if k < 0.55 else mix(WINE, WINE_LIGHT, (k - 0.55) / 0.45)
        d.line([(0, y), (w, y)], fill=c)
    # A faint field of slopes in the mark's angle.
    layer = Image.new("RGBA", SIDE, (0, 0, 0, 0))
    ld = ImageDraw.Draw(layer)
    for x in range(-h, w + h, 28):
        ld.line([(x, h), (x + int(h * 0.62), 0)], fill=(255, 255, 255, 14), width=2)
    img = Image.alpha_composite(img.convert("RGBA"), layer)

    # The icon, with a soft glow behind it.
    size = 132
    ix, iy = (w - size) // 2, 150
    glow = Image.new("RGBA", SIDE, (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    for r in range(90, 0, -6):
        gd.ellipse(
            [w // 2 - r, iy + size // 2 - r, w // 2 + r, iy + size // 2 + r],
            fill=(255, 210, 225, max(0, 10 - r // 10)),
        )
    img = Image.alpha_composite(img, glow)
    img.alpha_composite(icon(size), (ix, iy))

    # RGBA drawing, so the quiet parts blend into the panel.
    d = ImageDraw.Draw(img, "RGBA")
    # The name, centred: "Kod" in paper, "Act" in rose.
    f = font(58)
    kod, act = "Kod", "Act"
    wk = d.textlength(kod, font=f)
    wa = d.textlength(act, font=f)
    x0 = (w - (wk + wa)) / 2
    y0 = iy + size + 44
    d.text((x0, y0), kod, font=f, fill=PAPER)
    d.text((x0 + wk, y0), act, font=f, fill=ROSE)
    # The tagline, tracked wide, set to the name's width.
    tag = "FROM CODE TO IMPACT"
    tf = font(15, "Medium")
    track = 4.2
    total = sum(d.textlength(ch, font=tf) for ch in tag) + track * (len(tag) - 1)
    x = (w - total) / 2
    for ch in tag:
        d.text((x, y0 + 82), ch, font=tf, fill=(255, 255, 255, 190))
        x += d.textlength(ch, font=tf) + track
    # A hairline and the version slot's quiet footer.
    d.line([(48, h - 70), (w - 48, h - 70)], fill=(255, 255, 255, 40), width=2)
    ff = font(15, "Medium")
    foot = "The KodAct desktop app and CLI"
    d.text(((w - d.textlength(foot, font=ff)) / 2, h - 52), foot, font=ff, fill=(255, 255, 255, 150))
    return img.convert("RGB")


def header():
    img = Image.new("RGBA", HEADER, PAPER + (255,))
    size = 84
    img.alpha_composite(icon(size), (HEADER[0] - size - 18, (HEADER[1] - size) // 2))
    return img.convert("RGB")


def main():
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "target/installer/art")
    out.mkdir(parents=True, exist_ok=True)
    side = side_panel()
    side.save(out / "welcome.bmp")
    # The uninstaller's pages carry the same panel.
    side.save(out / "unwelcome.bmp")
    header().save(out / "header.bmp")
    print(f"{out}: welcome.bmp, unwelcome.bmp, header.bmp")
    return 0


if __name__ == "__main__":
    sys.exit(main())
