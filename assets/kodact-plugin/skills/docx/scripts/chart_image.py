"""Charts drawn as pictures, in a theme's colours and fonts, for Word files
(python-docx can't write native Word charts). Drawn at twice the size they
show, so they stay sharp when printed. Decks and workbooks use native charts.

    draw(spec, theme, path, width_in=6.3, height_in=3.4)

spec: {"type": "column|bar|line|area|pie|doughnut", "categories": [...],
       "series": {"Name": [numbers], ...}, "title": "...", "percent": bool}
"""

import math
import os

from PIL import Image, ImageDraw, ImageFont

DPI = 200
TYPES = ("column", "bar", "line", "area", "pie", "doughnut")

# Fonts Windows ships, then common Linux ones, then Pillow's own.
FONT_FILES = {
    "regular": ["segoeui.ttf", "arial.ttf", "DejaVuSans.ttf", "LiberationSans-Regular.ttf"],
    "bold": ["segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf", "LiberationSans-Bold.ttf"],
}
FONT_DIRS = [os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts"),
             "/usr/share/fonts/truetype/dejavu", "/usr/share/fonts/truetype/liberation",
             "/usr/share/fonts/TTF", "/Library/Fonts"]


def _font(size, bold=False):
    for name in FONT_FILES["bold" if bold else "regular"]:
        for d in FONT_DIRS:
            p = os.path.join(d, name)
            if os.path.isfile(p):
                return ImageFont.truetype(p, size)
    try:
        return ImageFont.load_default(size)
    except TypeError:
        return ImageFont.load_default()


def _rgb(h):
    h = str(h).lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def _mix(a, b, k):
    return tuple(int(a[i] + (b[i] - a[i]) * k) for i in range(3))


def _nice(top):
    """A round axis maximum above `top`, and its step."""
    if top <= 0:
        return 1, 0.2
    mag = 10 ** math.floor(math.log10(top))
    for m in (1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10):
        if m * mag >= top:
            hi = m * mag
            break
    step = hi / 5
    return hi, step


def _fmt(v, percent=False):
    s = f"{v:,.0f}" if abs(v) >= 100 or float(v).is_integer() else f"{v:,.1f}"
    return s + ("%" if percent else "")


def validate(spec):
    kind = spec.get("type", "column")
    if kind not in TYPES:
        raise SystemExit(f"no chart type {kind!r}; choose one of {', '.join(TYPES)}")
    cats, series = spec.get("categories"), spec.get("series")
    if not cats or not isinstance(series, dict) or not series:
        raise SystemExit('a chart needs "categories": [...] and "series": {"Name": [numbers]}')
    for name, vals in series.items():
        if len(vals) != len(cats):
            raise SystemExit(f"series {name!r} has {len(vals)} values for {len(cats)} categories")
    return kind, cats, series


def draw(spec, theme, path, width_in=6.3, height_in=3.4):
    kind, cats, series = validate(spec)
    W, H = int(width_in * DPI), int(height_in * DPI)
    bg = _rgb(theme.get("card_bg", "FFFFFF"))
    ink, soft = _rgb(theme["ink"]), _rgb(theme["soft"])
    accents = [_rgb(a) for a in theme["accents"]]
    im = Image.new("RGB", (W, H), bg)
    d = ImageDraw.Draw(im)
    f_small, f_label, f_title = _font(int(DPI * 0.11)), _font(int(DPI * 0.12), True), _font(int(DPI * 0.15), True)
    pct = bool(spec.get("percent"))
    top = int(DPI * 0.12)
    if spec.get("title"):
        d.text((int(DPI * 0.15), top), spec["title"], font=f_title, fill=ink)
        top += int(DPI * 0.32)
    names = list(series)
    # Legend, bottom, centred: when there are several series or slices.
    legend = names if kind not in ("pie", "doughnut") else list(cats)
    leg_h = int(DPI * 0.3) if (len(names) > 1 or kind in ("pie", "doughnut")) else 0
    if leg_h:
        sw = int(DPI * 0.11)
        widths = [sw + 10 + d.textlength(str(n), font=f_small) + 30 for n in legend]
        x = (W - sum(widths)) / 2
        y = H - leg_h + (leg_h - sw) / 2
        for i, n in enumerate(legend):
            d.rounded_rectangle((x, y, x + sw, y + sw), radius=sw // 4, fill=accents[i % len(accents)])
            d.text((x + sw + 10, y - sw * 0.25), str(n), font=f_small, fill=soft)
            x += widths[i]
    area = (int(DPI * 0.15), top, W - int(DPI * 0.15), H - leg_h - int(DPI * 0.08))

    if kind in ("pie", "doughnut"):
        vals = list(series.values())[0]
        total = sum(vals) or 1
        size = min(area[2] - area[0], area[3] - area[1])
        cx, cy = (area[0] + area[2]) / 2, (area[1] + area[3]) / 2
        box = (cx - size / 2, cy - size / 2, cx + size / 2, cy + size / 2)
        start = -90.0
        for i, v in enumerate(vals):
            sweep = 360 * v / total
            d.pieslice(box, start, start + sweep, fill=accents[i % len(accents)], outline=bg, width=6)
            mid = math.radians(start + sweep / 2)
            r = size * (0.37 if kind == "doughnut" else 0.32)
            label = _fmt(v / total * 100 if pct and total != 100 else v, pct)
            tw = d.textlength(label, font=f_label)
            if sweep > 12:
                d.text((cx + r * math.cos(mid) - tw / 2, cy + r * math.sin(mid) - DPI * 0.07), label,
                       font=f_label, fill=(255, 255, 255))
            start += sweep
        if kind == "doughnut":
            hole = size * 0.26
            d.ellipse((cx - hole, cy - hole, cx + hole, cy + hole), fill=bg)
        im.save(path)
        return path

    vals_all = [v for vs in series.values() for v in vs]
    hi, step = _nice(max(max(vals_all), 0))
    lo = min(0, min(vals_all))
    left = area[0] + int(max(d.textlength(_fmt(hi, pct), font=f_small), DPI * 0.3)) + 12
    plot = (left, area[1] + 10, area[2], area[3] - int(DPI * 0.22))
    grid = _mix(soft, bg, 0.85)
    horizontal = kind == "bar"
    span = hi - lo or 1
    # Gridlines and value labels.
    n_steps = int(round(span / step))
    for k in range(n_steps + 1):
        v = lo + k * step
        if horizontal:
            x = plot[0] + (v - lo) / span * (plot[2] - plot[0])
            d.line((x, plot[1], x, plot[3]), fill=grid, width=2)
            t = _fmt(v, pct)
            d.text((x - d.textlength(t, font=f_small) / 2, plot[3] + 6), t, font=f_small, fill=soft)
        else:
            y = plot[3] - (v - lo) / span * (plot[3] - plot[1])
            d.line((plot[0], y, plot[2], y), fill=grid, width=2)
            t = _fmt(v, pct)
            d.text((plot[0] - 12 - d.textlength(t, font=f_small), y - DPI * 0.06), t, font=f_small, fill=soft)
    n, m = len(cats), len(names)
    if horizontal:
        # Bars across: categories down the left side, inside the plot.
        band = (plot[3] - plot[1]) / n
        cat_w = max(d.textlength(str(c), font=f_small) for c in cats) + 16
        x0 = plot[0] + cat_w
        for ci, c in enumerate(cats):
            y = plot[1] + ci * band
            d.text((plot[0], y + band / 2 - DPI * 0.06), str(c), font=f_small, fill=ink)
            bh = band * 0.7 / m
            for si, name in enumerate(names):
                v = series[name][ci]
                x1 = x0 + (v - lo) / span * (plot[2] - x0)
                yy = y + band * 0.15 + si * bh
                d.rounded_rectangle((x0, yy, max(x1, x0 + 2), yy + bh - 4), radius=6, fill=accents[si % len(accents)])
                if n * m <= 16:
                    d.text((x1 + 8, yy + bh / 2 - DPI * 0.07), _fmt(v, pct), font=f_small, fill=ink)
        im.save(path)
        return path

    band = (plot[2] - plot[0]) / n
    base = plot[3] - (0 - lo) / span * (plot[3] - plot[1])
    for ci, c in enumerate(cats):
        t = str(c)
        d.text((plot[0] + ci * band + band / 2 - d.textlength(t, font=f_small) / 2, plot[3] + 8), t,
               font=f_small, fill=ink)
    if kind == "column":
        bw = band * 0.7 / m
        for ci in range(n):
            for si, name in enumerate(names):
                v = series[name][ci]
                x = plot[0] + ci * band + band * 0.15 + si * bw
                y = plot[3] - (v - lo) / span * (plot[3] - plot[1])
                d.rounded_rectangle((x + 2, min(y, base), x + bw - 4, max(y, base)), radius=6,
                                    fill=accents[si % len(accents)])
                if n * m <= 16:
                    t = _fmt(v, pct)
                    d.text((x + bw / 2 - d.textlength(t, font=f_small) / 2, y - DPI * 0.16), t, font=f_small, fill=ink)
    else:
        for si, name in enumerate(names):
            col = accents[si % len(accents)]
            pts = [(plot[0] + ci * band + band / 2, plot[3] - (v - lo) / span * (plot[3] - plot[1]))
                   for ci, v in enumerate(series[name])]
            if kind == "area":
                fill = Image.new("RGBA", im.size, (0, 0, 0, 0))
                ImageDraw.Draw(fill).polygon([(pts[0][0], base)] + pts + [(pts[-1][0], base)],
                                             fill=col + (70,))
                im.paste(fill, (0, 0), fill)
                d = ImageDraw.Draw(im)
            d.line(pts, fill=col, width=7, joint="curve")
            for x, y in pts:
                d.ellipse((x - 9, y - 9, x + 9, y + 9), fill=bg, outline=col, width=5)
    d.line((plot[0], base, plot[2], base), fill=_mix(soft, bg, 0.5), width=3)
    im.save(path)
    return path
