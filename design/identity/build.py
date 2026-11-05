"""Builds design/kodact-identity.html from template.html: the marks drawn from
ui/mark.rs's geometry, the fonts and the before-screenshots inlined.

    python3 design/identity/build.py
"""
import base64, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
INK, INK1, LIME, LIME_DAY, OLIVE, PAPER = "#101214", "#15181b", "#c6ff3d", "#b5ea2c", "#4a7200", "#f6f5f1"

BAR = '<rect x="12.8" y="15.36" width="7.68" height="33.28" fill="{c}"/>'
RUN = '<path d="M26.88 15.36L51.2 32L26.88 48.64Z" fill="{c}"/>'
SHAPE = '<use href="#tile-shape" x="{o}" y="{o}" width="64" height="64" fill="{c}"/>'


def tile(size, ground, glyph, shadow=None, keyline=None, label="KodAct"):
    pad = 4 if shadow else 0
    vb = 64 + pad
    parts = []
    if shadow:
        parts.append(SHAPE.format(o=pad, c=shadow))
    parts.append(SHAPE.format(o=0, c=ground))
    if keyline:
        parts.append(f'<path d="M10.67 .5H53.33A10.17 10.17 0 0 1 63.5 10.67V53.33A10.17 10.17 0 0 1 53.33 63.5H10.67A10.17 10.17 0 0 1 .5 53.33V10.67A10.17 10.17 0 0 1 10.67 .5Z" fill="none" stroke="{keyline}" stroke-width="1.5"/>')
    parts.append(BAR.format(c=glyph))
    parts.append(RUN.format(c=glyph))
    s = size * vb / 64
    return (f'<svg class="k" width="{s:g}" height="{s:g}" viewBox="0 0 {vb} {vb}" role="img" '
            f'aria-label="{label}">{"".join(parts)}</svg>')


def tile_rows(n=16):
    f = lambda v: round(v * n)
    bar0 = f(0.20); bar1 = max(f(0.32), bar0 + 1)
    tri0 = max(f(0.42), bar1 + 1); tri1 = max(f(0.80), tri0 + 2)
    top, bottom = f(0.24), f(0.76)
    mid = (top + bottom) / 2; half = (bottom - top) / 2
    rows = []
    for y in range(n):
        row = ""
        for x in range(n):
            r = 2 if n >= 16 else 1
            cx, cy = min(x, n - 1 - x), min(y, n - 1 - y)
            if cx + cy < r:
                row += "."; continue
            inside = top <= y < bottom
            bar = inside and bar0 <= x < bar1
            reach = 1 - abs(y + 0.5 - mid) / half
            tri = inside and x >= tri0 and x < tri0 + (tri1 - tri0) * reach
            row += "B" if bar else "R" if tri else "M"
        rows.append(row)
    return rows


def runs(rows, keep):
    d = ""
    for y, row in enumerate(rows):
        x = 0
        while x < len(row):
            if row[x] not in keep:
                x += 1; continue
            s = x
            while x < len(row) and row[x] in keep:
                x += 1
            d += f"M{s} {y}h{x-s}v1h-{x-s}Z"
    return d


ROWS = tile_rows()


def pixel_tile(size, ground, glyph):
    return (f'<svg class="k" width="{size}" height="{size}" viewBox="0 0 16 16" shape-rendering="crispEdges" '
            f'role="img" aria-label="KodAct"><path d="{runs(ROWS, "MBR")}" fill="{ground}"/>'
            f'<path d="{runs(ROWS, "BR")}" fill="{glyph}"/></svg>')


def glyph(w, color, label="KodAct"):
    h = round(w * 36 / 44, 1)
    return f'<svg class="k" width="{w}" height="{h:g}" style="color:{color}" role="img" aria-label="{label}"><use href="#glyph"/></svg>'


def word(size, kod, act):
    return (f'<span style="font:600 {size}px/1 \'KodAct Sans\';letter-spacing:-.01em;color:{kod}">'
            f'Kod<span style="color:{act}">Act</span></span>')


def lock_h(tile_svg, size, kod, act):
    return f'<span style="display:inline-flex;align-items:center;gap:{round(size*0.5)}px">{tile_svg}{word(round(size*0.62), kod, act)}</span>'


def font(name):
    p = os.path.join(HERE, "..", "..", "assets", "fonts", name)
    return base64.b64encode(open(p, "rb").read()).decode()


def shots():
    d = os.path.join(HERE, "before")
    items = [
        ("chat-dark.jpg", "Now, dark, mid-reply: a tile in the title bar and a tile on the working line, each brighter than the reply's text. The lime arcs, dot and shimmer add to it."),
        ("chat-light.jpg", "Now, light: the same two tiles on paper at 1.16–1.30:1, outlined only by their hard olive shadow."),
        ("home-light.jpg", "Now, light home: the title-bar tile plus the 52 px pixel tile next to \"KodAct returns\", two brand marks in one view."),
        ("detail.png", "Now, at 4×: the title-bar tile on paper, the home pixel tile (its triangle stepped into a D, over a stair-stepped olive shadow), and the working-line tile on ink."),
    ]
    out = []
    for f, cap in items:
        p = os.path.join(d, f)
        if not os.path.exists(p):
            continue
        mime = "image/png" if f.endswith(".png") else "image/jpeg"
        data = base64.b64encode(open(p, "rb").read()).decode()
        out.append(f'<figure><img src="data:{mime};base64,{data}" alt="{cap}" loading="lazy"><figcaption>{cap}</figcaption></figure>')
    return f'<div class="shots">{"".join(out)}</div>' if out else ""


R = {
    "F_REG": font("HostGrotesk-Regular.ttf"),
    "F_MED": font("HostGrotesk-Medium.ttf"),
    "F_SEMI": font("HostGrotesk-SemiBold.ttf"),
    "F_MONO": font("JetBrainsMono-Regular.ttf"),
    "KOD": open(os.path.join(HERE, "kod.txt")).read(),
    "ACT": open(os.path.join(HERE, "act.txt")).read(),
    "TILE_INK_16": tile(16, INK, LIME),
    "TILE_INK_24": tile(24, INK, LIME),
    "TILE_INK_32": tile(32, INK, LIME),
    "TILE_INK_40": tile(40, INK, LIME),
    "TILE_INK_64": tile(64, INK, LIME),
    "TILE_INK_48": tile(48, INK, LIME),
    "TILE_LIME_56": tile(56, LIME, INK),
    "TILE_LIME_28": tile(28, LIME, INK),
    "GLYPH_INK_14": glyph(14, INK),
    "TILE_LIME_40": tile(40, LIME, INK),
    "TILE_LIME_64": tile(64, LIME, INK),
    "TILE_LIME_56PX": pixel_tile(56, LIME, INK),
    "TILE_LIME_32PX": pixel_tile(32, LIME, INK),
    "TILE_LIME_40_SHADOW": tile(40, LIME_DAY, INK, shadow=OLIVE),
    "TILE_INK_40_ONINK": tile(40, "#121417", LIME),
    "GLYPH_LIME_56": glyph(56, LIME),
    "GLYPH_LIME_32": glyph(32, LIME),
    "GLYPH_INK_32": glyph(32, INK),
    "GLYPH_SIZE_14": glyph(14, "var(--text)"),
    "ICON_24": tile(24, INK, LIME, keyline="#3a4047"),
    "LOCK_H_PAPER": lock_h(tile(40, INK, LIME), 40, "#08090b", OLIVE),
    "LOCK_H_PAPER_SM": lock_h(tile(28, INK, LIME), 28, "#08090b", OLIVE),
    "LOCK_H_INK": lock_h(tile(40, LIME, INK), 40, "#fbf9f3", LIME),
    "LOCK_STACK": ('<span style="display:inline-flex;flex-direction:column;align-items:center;gap:16px">'
                   + tile(48, LIME, INK)
                   + '<svg width="104" height="20" viewBox="0 0 52 10" aria-label="KodAct"><use href="#pxkod" x="0" y="0" width="26" height="10" fill="#fbf9f3"/><use href="#pxact" x="27" y="0" width="25" height="10" fill="#c6ff3d"/></svg></span>'),
    "SHOTS": shots(),
}

t = open(os.path.join(HERE, "template.html")).read()
for k, v in R.items():
    t = t.replace("{{" + k + "}}", v)
left = [s for s in t.split("{{")[1:]]
if left:
    sys.exit("unfilled: " + ", ".join(s.split("}}")[0] for s in left))
out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "kodact-identity.html")
open(out, "w").write(t)
print(out, len(t) // 1024, "KB")
