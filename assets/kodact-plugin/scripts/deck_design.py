"""Designed slides for `pptx_tool.py init` and `add`: themes, layouts and a
small vocabulary of drawn shapes (faces with moods, sun, star, heart, cloud,
a friendly cell), so a styled deck is a few commands, not a hand-written
renderer.

Every layout fits its text: sizes come down until the words fit their box,
so nothing runs off a slide. Fonts are ones Windows ships with.
"""

from pptx.dml.color import RGBColor
from pptx.enum.shapes import MSO_SHAPE
from pptx.enum.text import MSO_ANCHOR, MSO_AUTO_SIZE, PP_ALIGN
from pptx.util import Inches, Pt

W, H = 13.333, 7.5


def rgb(hex_):
    hex_ = hex_.lstrip("#")
    return RGBColor(int(hex_[0:2], 16), int(hex_[2:4], 16), int(hex_[4:6], 16))


THEMES = {
    # Warm and playful: cream paper, bright accents, rounded everything.
    "kids": {
        "bg": "FFF9EF", "ink": "1F2A3A", "soft": "4A5568", "card": "FFFFFF",
        "accents": ["FF6B6B", "2EC4B6", "FFB703", "9D4EDD", "52B788", "48CAE4"],
        "head": "Century Gothic", "body": "Trebuchet MS",
        "radius": 0.30, "decor": True, "title_pt": 34, "playful": True,
    },
    # Quiet and clear: white, one blue, thin rules.
    "clean": {
        "bg": "FFFFFF", "ink": "17202A", "soft": "4B5563", "card": "F5F7FA",
        "accents": ["2563EB", "0EA5E9", "14B8A6", "6366F1", "F59E0B", "EF4444"],
        "head": "Segoe UI Semibold", "body": "Segoe UI",
        "radius": 0.10, "decor": False, "title_pt": 32,
    },
    # Dark stage, strong colour.
    "bold": {
        "bg": "111827", "ink": "F9FAFB", "soft": "D1D5DB", "card": "1F2937",
        "accents": ["F472B6", "34D399", "FBBF24", "60A5FA", "A78BFA", "F87171"],
        "head": "Segoe UI Black", "body": "Segoe UI",
        "radius": 0.18, "decor": False, "title_pt": 36,
    },
}

THEMES.update({
    # Patients, parents, a community: soft and calm, nothing alarming.
    "warm": {
        "bg": "FBF7F2", "ink": "2D2A32", "soft": "57534E", "card": "FFFFFF",
        "accents": ["E07A5F", "3D9A8B", "F2CC8F", "81B29A", "6D597A", "5FA8D3"],
        "head": "Georgia", "body": "Segoe UI",
        "radius": 0.22, "decor": True, "title_pt": 32,
    },
    # Boards, investors, finance: navy, teal, gold; square and sober.
    "corporate": {
        "bg": "FFFFFF", "ink": "0B1F3A", "soft": "425466", "card": "F4F6F9",
        "accents": ["0B3D91", "00A6A6", "C9A227", "5B7DB1", "7A8B99", "D1495B"],
        "head": "Segoe UI Semibold", "body": "Segoe UI",
        "radius": 0.04, "decor": False, "title_pt": 32,
    },
    # Reports, research, policy: serif heads, muted ink, generous space.
    "editorial": {
        "bg": "FCFBF8", "ink": "1C1917", "soft": "44403C", "card": "F5F2EC",
        "accents": ["9F1239", "1E3A8A", "B45309", "166534", "6B21A8", "0F766E"],
        "head": "Georgia", "body": "Cambria",
        "radius": 0.02, "decor": False, "title_pt": 34,
    },
})

# Words that say who a file is for, and the theme that fits them. Checked in this order, first match wins: who reads it (children, a board, researchers) before
# the field it is about (health), so "a hospital board" gets the board's theme, not the patients'.
AUDIENCES = [
    ("kids", ("kid", "child", "children", "pupil", "primary school", "classroom", "young", "ages 4", "ages 5",
              "ages 6", "ages 7", "ages 8", "ages 9", "ages 10", "enfant", "élève")),
    ("corporate", ("board", "investor", "executive", "finance", "quarterly", "budget", "shareholder", "bank",
                   "client", "sales", "corporate", "management", "kpi")),
    ("editorial", ("research", "academic", "paper", "thesis", "policy", "report", "study", "university",
                   "journal", "white paper")),
    ("warm", ("patient", "parent", "family", "families", "caregiver", "health", "hospital", "community",
              "charity", "volunteer", "wellbeing", "therapy", "senior")),
    ("bold", ("conference", "keynote", "launch", "pitch", "event", "talk", "meetup", "hackathon", "marketing")),
]


def for_audience(words):
    """The theme that fits who the file is for ("kids aged 8", "the board"); clean otherwise."""
    w = (words or "").lower()
    for theme_name, keys in AUDIENCES:
        if any(k in w for k in keys):
            return theme_name
    return "clean"


LAYOUTS = ("title", "cards", "steps", "statement", "faces", "bullets", "closing", "feature",
           "chart", "table", "timeline", "compare", "grid", "kpis", "quote", "image", "cycle", "pyramid")
CHARTS = ("bar", "column", "line", "pie", "doughnut", "area", "stacked")
ICONS = ("sun", "star", "heart", "cloud", "cell", "none")
MOODS = ("happy", "sad", "worried", "angry", "confused", "surprised")


def theme(name):
    if name not in THEMES:
        raise SystemExit(f"no theme {name!r}; choose one of {', '.join(THEMES)}")
    return THEMES[name]


def tint(hex_, k=0.86, toward="FFFFFF"):
    a, b = rgb(hex_), rgb(toward)
    return RGBColor(*(int(a[i] + (b[i] - a[i]) * k) for i in range(3)))


def shade(hex_, k=0.25):
    c = rgb(hex_)
    return RGBColor(*(int(c[i] * (1 - k)) for i in range(3)))


# ---------- primitives ----------

def shape(slide, kind, x, y, w, h, fill=None, line=None, lw=1.5, radius=None):
    s = slide.shapes.add_shape(kind, Inches(x), Inches(y), Inches(w), Inches(h))
    s.shadow.inherit = False
    if fill is None:
        s.fill.background()
    else:
        s.fill.solid()
        s.fill.fore_color.rgb = fill if isinstance(fill, RGBColor) else rgb(fill)
    if line is None:
        s.line.fill.background()
    else:
        s.line.color.rgb = line if isinstance(line, RGBColor) else rgb(line)
        s.line.width = Pt(lw)
    if radius is not None and kind == MSO_SHAPE.ROUNDED_RECTANGLE:
        s.adjustments[0] = radius
    return s


def _fits(text, w, h, pt, line=1.18, char_w=0.52):
    """Whether `text` at `pt` wraps into a w x h inch box."""
    per_line = max(1, int(w * 72 / (pt * char_w)))
    lines, cur = 1, 0
    for word in text.split():
        add = len(word) + (1 if cur else 0)
        if cur + add > per_line and cur:
            lines += 1
            cur = len(word)
        else:
            cur += add
    lines += text.count("\n")
    return lines * pt * line / 72 <= h


def fit_pt(text, w, h, max_pt, min_pt=12, line=1.18, char_w=0.52):
    """The largest size from max_pt down at which `text` wraps into a w x h inch box. When even
    min_pt is too big, says so on stderr (the text is too long for this layout) and returns min_pt:
    smaller type would be unreadable, so the words are what must change."""
    pt = max_pt
    while pt > min_pt:
        if _fits(text, w, h, pt, line, char_w):
            return pt
        pt -= 1
    if not _fits(text, w, h, min_pt, line, char_w):
        import sys

        short = " ".join(text.split())
        print(f"note: too long for its box even at {min_pt}pt, so it will overflow: "
              f"{short[:70] + '…' if len(short) > 70 else short!r}; shorten it or split it", file=sys.stderr)
    return min_pt


def text(slide, x, y, w, h, words, pt, color, font, bold=False, align=PP_ALIGN.LEFT,
         anchor=MSO_ANCHOR.TOP, fit=True, name=None):
    tb = slide.shapes.add_textbox(Inches(x), Inches(y), Inches(w), Inches(h))
    if name:
        tb.name = name
    tf = tb.text_frame
    tf.word_wrap = True
    tf.auto_size = MSO_AUTO_SIZE.NONE
    tf.vertical_anchor = anchor
    for m in ("margin_left", "margin_right", "margin_top", "margin_bottom"):
        setattr(tf, m, Inches(0.04))
    size = fit_pt(words, w - 0.1, h - 0.1, pt) if fit else pt
    for i, part in enumerate(words.split("\n")):
        p = tf.paragraphs[0] if i == 0 else tf.add_paragraph()
        p.alignment = align
        r = p.add_run()
        r.text = part
        r.font.size = Pt(size)
        r.font.bold = bold
        r.font.name = font
        r.font.color.rgb = color if isinstance(color, RGBColor) else rgb(color)
    return tb


def arc(slide, x, y, w, h, color, start, end, lw=4.0):
    """An arc of the ellipse in the box, from `start` to `end` degrees
    (0 = 3 o'clock, clockwise). python-pptx scales adjustments by 1/100000 and
    DrawingML angles are 1/60000 degree, hence the 0.6."""
    s = slide.shapes.add_shape(MSO_SHAPE.ARC, Inches(x), Inches(y), Inches(w), Inches(h))
    s.shadow.inherit = False
    s.fill.background()
    s.line.color.rgb = color if isinstance(color, RGBColor) else rgb(color)
    s.line.width = Pt(lw)
    s.adjustments[0] = start * 0.6
    s.adjustments[1] = end * 0.6
    return s


def bar(slide, cx, cy, w, h, color, rot=0.0):
    s = shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, cx - w / 2, cy - h / 2, w, h, fill=color, radius=0.5)
    s.rotation = rot
    return s


def face(slide, cx, cy, d, mood="happy", ink="1F2A3A"):
    """Eyes, brows and a mouth for `mood`, centred on a circle of diameter d."""
    if mood not in MOODS:
        raise SystemExit(f"no mood {mood!r}; choose one of {', '.join(MOODS)}")
    e = d * 0.12
    ey = cy - d * 0.12
    for sx in (-1, 1):
        shape(slide, MSO_SHAPE.OVAL, cx + sx * d * 0.18 - e / 2, ey - e / 2, e, e, fill=ink)
    bw, bh, by = d * 0.20, d * 0.045, ey - d * 0.15
    lw = max(2.0, d * 3.2)
    mw, mh = d * 0.46, d * 0.34
    if mood == "happy":
        # Lower arc: a smile whose lowest point sits at cy + 0.25d.
        arc(slide, cx - mw / 2, cy + d * 0.25 - mh, mw, mh, ink, 25, 155, lw)
    elif mood in ("sad", "angry"):
        # Upper arc: a frown whose top sits at cy + 0.14d.
        arc(slide, cx - mw / 2, cy + d * 0.14, mw, mh, ink, 205, 335, lw)
        # Angry: inner ends down (a V); sad: inner ends up.
        tilt = -18 if mood == "angry" else 18
        for sx in (-1, 1):
            bar(slide, cx + sx * d * 0.18, by, bw, bh, ink, rot=sx * tilt)
    elif mood == "worried":
        bar(slide, cx, cy + d * 0.20, d * 0.26, d * 0.05, ink, rot=-6)
        for sx in (-1, 1):
            bar(slide, cx + sx * d * 0.18, by, bw, bh, ink, rot=sx * 14)
    elif mood == "confused":
        bar(slide, cx + d * 0.02, cy + d * 0.20, d * 0.24, d * 0.05, ink, rot=-14)
        bar(slide, cx + d * 0.18, by - d * 0.04, bw, bh, ink, rot=-10)
        bar(slide, cx - d * 0.18, by, bw, bh, ink)
    elif mood == "surprised":
        o = d * 0.16
        shape(slide, MSO_SHAPE.OVAL, cx - o / 2, cy + d * 0.12, o, o * 1.2, fill=ink)


def icon(slide, kind, x, y, d, color, ink="1F2A3A"):
    """A drawn picture in a d x d box at x, y."""
    if kind == "none":
        return
    if kind not in ICONS:
        # Any of Lucide's 1,500+ icons, as a native vector shape (icons.py).
        import icons
        icons.add_to_slide(slide, icons.require(kind), x, y, d, _hex(color), 2.2)
        return
    if kind == "sun":
        shape(slide, MSO_SHAPE.SUN, x, y, d, d, fill=color)
    elif kind == "star":
        shape(slide, MSO_SHAPE.STAR_5_POINT, x, y, d, d, fill=color)
    elif kind == "heart":
        shape(slide, MSO_SHAPE.HEART, x, y + d * 0.05, d, d * 0.9, fill=color)
    elif kind == "cloud":
        shape(slide, MSO_SHAPE.CLOUD, x, y + d * 0.2, d, d * 0.6, fill=tint(_hex(color), 0.55))
    elif kind == "cell":
        cell(slide, x + d / 2, y + d / 2, d / 2, color, ink)


def badge(slide, kind, x, y, d, color, solid=True):
    """An icon in a circle: white on the colour (solid), or the colour on its tint."""
    hx = _hex(color)
    shape(slide, MSO_SHAPE.OVAL, x, y, d, d, fill=hx if solid else tint(hx, 0.8))
    pad = d * 0.24
    icon(slide, kind, x + pad, y + pad, d - 2 * pad, "FFFFFF" if solid else hx)


def cell(slide, cx, cy, r, color, ink="1F2A3A"):
    """A friendly cell: round body, a nucleus up and to the left, a smile."""
    hx = _hex(color)
    shape(slide, MSO_SHAPE.OVAL, cx - r, cy - r, 2 * r, 2 * r, fill=tint(hx, 0.35), line=shade(hx, 0.1), lw=2.5)
    n = r * 0.42
    shape(slide, MSO_SHAPE.OVAL, cx - r * 0.62, cy - r * 0.70, n, n, fill=shade(hx, 0.15))
    face(slide, cx + r * 0.08, cy + r * 0.12, r * 1.15, "happy", ink)


def _hex(c):
    if isinstance(c, RGBColor):
        return str(c)
    return c.lstrip("#")


# ---------- slides ----------

def background(slide, t):
    fill = slide.background.fill
    fill.solid()
    fill.fore_color.rgb = rgb(t["bg"])


def chrome(slide, t, n, accent):
    """What every content slide shares: decor in the corners, a footer bar, the number."""
    if t["decor"]:
        shape(slide, MSO_SHAPE.OVAL, W - 1.6, -1.1, 2.6, 2.6, fill=tint(accent, 0.8, t["bg"]))
        shape(slide, MSO_SHAPE.OVAL, -0.9, H - 0.9, 1.8, 1.8, fill=tint(accent, 0.86, t["bg"]))
    shape(slide, MSO_SHAPE.RECTANGLE, 0, H - 0.1, W, 0.1, fill=accent)
    text(slide, W - 1.3, H - 0.52, 0.9, 0.34, str(n), 12, accent, t["body"], bold=True,
         align=PP_ALIGN.RIGHT, fit=False, name="Decor number")


def header(slide, t, title, accent, icon_kind):
    right = W - 0.85 - (1.15 if icon_kind != "none" else 0)
    text(slide, 0.85, 0.36, right - 0.85, 0.95, title, t["title_pt"], t["ink"], t["head"], bold=True,
         anchor=MSO_ANCHOR.MIDDLE, name="Title")
    shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.88, 1.36, 2.2, 0.11, fill=accent, radius=0.5)
    if icon_kind != "none":
        icon(slide, icon_kind, W - 1.85, 0.35, 0.95, accent, t["ink"])


def _items(spec, most=None):
    items = spec.get("items") or []
    if not isinstance(items, list) or not items:
        raise SystemExit(f"layout {spec.get('layout')!r} needs \"items\": a list")
    return _keep(items, most, f"layout {spec.get('layout')!r}", "items")


def _keep(items, most, what, noun):
    """The first `most` of `items`; says on stderr what was left out, so nothing goes silently."""
    if most is not None and len(items) > most:
        import sys

        print(f"note: {what} shows {most} {noun}; {len(items) - most} more were left out "
              f"(split them over another slide)", file=sys.stderr)
        return items[:most]
    return items


def add_slide(prs, t, spec, n):
    """Add slide `n` from `spec` (a dict, see SPEC in pptx_tool)."""
    layout = spec.get("layout", "bullets")
    if layout not in LAYOUTS:
        raise SystemExit(f"no layout {layout!r}; choose one of {', '.join(LAYOUTS)}")
    acc_list = t["accents"]
    accent = acc_list[(n - 1) % len(acc_list)]
    if spec.get("accent") is not None:
        accent = acc_list[int(spec["accent"]) % len(acc_list)]
    icon_kind = spec.get("icon", "star" if t["decor"] else "none")
    slide = prs.slides.add_slide(prs.slide_layouts[6])
    background(slide, t)
    title = spec.get("title", "")
    if layout == "title":
        _title(slide, t, spec, accent)
    elif layout == "closing":
        _closing(slide, t, spec, accent, n)
    else:
        chrome(slide, t, n, accent)
        header(slide, t, title, accent, icon_kind)
        {"cards": _cards, "steps": _steps, "statement": _statement, "faces": _faces,
         "bullets": _bullets, "chart": _chart, "table": _table, "timeline": _timeline,
         "compare": _compare, "grid": _grid, "kpis": _kpis, "quote": _quote, "image": _image,
         "cycle": _cycle, "pyramid": _pyramid, "feature": _feature}[layout](slide, t, spec, accent)
    if spec.get("notes"):
        slide.notes_slide.notes_text_frame.text = spec["notes"]
    return slide


def _title(slide, t, spec, accent):
    a = t["accents"]
    if spec.get("style") == "dark":
        return _title_dark(slide, t, spec, accent)
    if t["decor"]:
        shape(slide, MSO_SHAPE.OVAL, -1.6, -1.6, 4.2, 4.2, fill=tint(a[2], 0.7, t["bg"]))
        shape(slide, MSO_SHAPE.OVAL, W - 2.6, H - 2.2, 4.0, 4.0, fill=tint(a[1], 0.8, t["bg"]))
        # Sun, stars, a heart and a cloud are for children's decks; other soft themes keep the discs.
        if t.get("playful"):
            icon(slide, "sun", 0.55, 0.45, 1.2, a[2])
            for (x, y, d, c) in [(10.6, 0.6, 0.45, a[2]), (11.6, 1.5, 0.3, a[3]), (2.3, 1.1, 0.3, a[0])]:
                icon(slide, "star", x, y, d, c)
            icon(slide, "heart", 10.3, 1.5, 0.4, a[0])
            icon(slide, "cloud", 1.0, 5.6, 1.3, "FFFFFF")
        art = spec.get("icon", "cell" if t.get("playful") else "none")
        if art != "none":
            icon(slide, art, W / 2 - 0.9, 0.75, 1.8, a[1], t["ink"])
    if not t["decor"]:
        # A geometric panel on the right: three overlapping discs in the accents.
        shape(slide, MSO_SHAPE.RECTANGLE, W - 4.6, 0, 4.6, H, fill=tint(a[0], 0.9, t["bg"]))
        for (x, y, d, c, k) in [(W - 4.0, 1.1, 2.6, a[0], 0.0), (W - 2.6, 2.6, 2.2, a[1], 0.15),
                                (W - 3.9, 3.9, 1.8, a[2], 0.3)]:
            shape(slide, MSO_SHAPE.OVAL, x, y, d, d, fill=tint(c, k, t["bg"]))
        text(slide, 0.9, 2.2, W - 6.2, 1.9, spec.get("title", ""), 50, t["ink"], t["head"], bold=True,
             anchor=MSO_ANCHOR.BOTTOM, name="Title")
        shape(slide, MSO_SHAPE.RECTANGLE, 0.95, 4.25, 1.6, 0.1, fill=accent)
        if spec.get("subtitle"):
            text(slide, 0.9, 4.5, W - 6.2, 1.0, spec["subtitle"], 22, t["soft"], t["body"])
        if spec.get("tag"):
            text(slide, 0.9, 5.6, W - 6.2, 0.5, spec["tag"], 15, accent, t["body"], bold=True)
        return
    top = 2.85
    text(slide, 1.2, top, W - 2.4, 1.3, spec.get("title", ""), 50, t["ink"], t["head"], bold=True,
         align=PP_ALIGN.CENTER, anchor=MSO_ANCHOR.MIDDLE, name="Title")
    shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, W / 2 - 1.0, top + 1.38, 2.0, 0.12, fill=accent, radius=0.5)
    if spec.get("subtitle"):
        text(slide, 2.0, top + 1.65, W - 4.0, 0.9, spec["subtitle"], 22, t["soft"], t["body"],
             align=PP_ALIGN.CENTER)
    if spec.get("tag"):
        tw = min(4.0, 0.6 + len(spec["tag"]) * 0.12)
        pill = shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, W / 2 - tw / 2, top + 2.65, tw, 0.48, fill=accent, radius=0.5)
        _label(pill, spec["tag"], 14, "FFFFFF", t["body"])
    shape(slide, MSO_SHAPE.RECTANGLE, 0, H - 0.12, W, 0.12, fill=accent)


def _label(shp, words, pt, color, font):
    shp.name = "Decor badge"
    tf = shp.text_frame
    tf.word_wrap = True
    tf.vertical_anchor = MSO_ANCHOR.MIDDLE
    p = tf.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    r = p.add_run()
    r.text = words
    r.font.size = Pt(pt)
    r.font.bold = True
    r.font.name = font
    r.font.color.rgb = rgb(color)


def _rows(n, top=1.75, bottom=6.95, gap=0.2, max_h=1.15):
    """n rows between top and bottom, none taller than max_h; a short list sits
    centred in the space instead of stretching."""
    if n <= 0:
        return []
    h = min(max_h, (bottom - top - gap * (n - 1)) / n)
    used = n * h + gap * (n - 1)
    top += (bottom - top - used) / 2 if n <= 3 else 0
    return [(top + i * (h + gap), h) for i in range(n)]


def _cards(slide, t, spec, accent):
    items = _items(spec, 6)
    numbered = spec.get("numbered", True)
    marks = spec.get("marks", "number" if numbered else "heart")
    a = t["accents"]
    for i, ((y, h), item) in enumerate(zip(_rows(len(items)), items)):
        c = a[i % len(a)]
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.85, y, W - 2.05, h, fill=t["card"],
              line=tint(c, 0.5, t["bg"]), lw=1.5, radius=t["radius"])
        d = min(0.62, h - 0.2)
        if marks == "number":
            b = shape(slide, MSO_SHAPE.OVAL, 1.08, y + (h - d) / 2, d, d, fill=c)
            _label(b, str(i + 1), 18, "FFFFFF", t["head"])
        else:
            icon(slide, marks, 1.08, y + (h - d) / 2, d, c, t["ink"])
        text(slide, 1.95, y + 0.05, W - 3.3, h - 0.1, str(item), 22, t["soft"], t["body"],
             anchor=MSO_ANCHOR.MIDDLE)


def _steps(slide, t, spec, accent):
    items = _items(spec, 4)
    n = len(items)
    gap, arrow = 0.2, 0.5
    w = (W - 1.7 - (n - 1) * (gap * 2 + arrow)) / n
    a = t["accents"]
    for i, item in enumerate(items):
        x = 0.85 + i * (w + gap * 2 + arrow)
        c = a[i % len(a)]
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, 2.0, w, 3.6, fill=t["card"], line=tint(c, 0.5, t["bg"]),
              lw=1.5, radius=t["radius"] * 0.6)
        b = shape(slide, MSO_SHAPE.OVAL, x + w / 2 - 0.4, 2.3, 0.8, 0.8, fill=c)
        _label(b, str(i + 1), 24, "FFFFFF", t["head"])
        head, _, body = str(item).partition(":")
        if body:
            text(slide, x + 0.2, 3.25, w - 0.4, 0.7, head.strip(), 20, shade(c, 0.3), t["head"], bold=True,
                 align=PP_ALIGN.CENTER)
            text(slide, x + 0.2, 3.95, w - 0.4, 1.5, body.strip(), 18, t["soft"], t["body"], align=PP_ALIGN.CENTER)
        else:
            text(slide, x + 0.2, 3.3, w - 0.4, 2.1, head.strip(), 20, t["soft"], t["body"], align=PP_ALIGN.CENTER,
                 anchor=MSO_ANCHOR.MIDDLE)
        if i < n - 1:
            shape(slide, MSO_SHAPE.RIGHT_ARROW, x + w + gap, 3.55, arrow, 0.5, fill=tint(c, 0.3, t["bg"]))
    if spec.get("footer"):
        text(slide, 0.85, 5.9, W - 2.0, 0.8, spec["footer"], 20, t["ink"], t["body"], bold=True,
             align=PP_ALIGN.CENTER, anchor=MSO_ANCHOR.MIDDLE)


def _statement(slide, t, spec, accent):
    shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.85, 1.8, W - 2.05, 2.3, fill=tint(accent, 0.82, t["bg"]),
          radius=t["radius"] * 0.7)
    big = spec.get("statement") or spec.get("title", "")
    if t["decor"]:
        icon(slide, spec.get("art", "heart"), 1.2, 2.25, 1.4, accent, t["ink"])
        text(slide, 2.9, 1.95, W - 4.2, 2.0, big, 32, t["ink"], t["head"], bold=True, anchor=MSO_ANCHOR.MIDDLE)
    else:
        text(slide, 1.3, 1.95, W - 2.9, 2.0, big, 32, t["ink"], t["head"], bold=True, anchor=MSO_ANCHOR.MIDDLE)
    items = spec.get("items") or []
    for i, ((y, h), item) in enumerate(zip(_rows(len(items), 4.35, 6.95, 0.15), items)):
        icon(slide, "star" if t["decor"] else "none", 1.0, y + (h - 0.4) / 2, 0.4, t["accents"][i % 6])
        text(slide, 1.6, y, W - 2.9, h, str(item), 20, t["soft"], t["body"], anchor=MSO_ANCHOR.MIDDLE)


def _faces(slide, t, spec, accent):
    items = _items(spec, 4)
    n = len(items)
    gap = 0.22
    cw = (W - 2.05 - gap * (n - 1)) / n
    a = t["accents"]
    for i, item in enumerate(items):
        if isinstance(item, str):
            item = {"label": item, "mood": "happy"}
        x = 0.85 + i * (cw + gap)
        c = a[i % len(a)]
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, 1.8, cw, 2.5, fill=t["card"], line=tint(c, 0.5, t["bg"]),
              lw=1.5, radius=0.16)
        d = 1.2
        fc = x + cw / 2
        shape(slide, MSO_SHAPE.OVAL, fc - d / 2, 2.0, d, d, fill=c, line=shade(c, 0.15), lw=2.0)
        face(slide, fc, 2.0 + d / 2, d, item.get("mood", "happy"), t["ink"])
        text(slide, x, 3.45, cw, 0.6, item.get("label", ""), 20, shade(c, 0.3), t["head"], bold=True,
             align=PP_ALIGN.CENTER)
    lines = spec.get("lines") or []
    for i, ((y, h), line) in enumerate(zip(_rows(len(lines), 4.55, 6.95, 0.18), lines)):
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.85, y, W - 2.05, h, fill=t["card"],
              line=tint(accent, 0.5, t["bg"]), lw=1.5, radius=t["radius"])
        text(slide, 1.2, y, W - 2.8, h, str(line), 20, t["soft"], t["body"], anchor=MSO_ANCHOR.MIDDLE)


def _bullets(slide, t, spec, accent):
    items = _items(spec, 7)
    for i, ((y, h), item) in enumerate(zip(_rows(len(items), 1.8, 6.9, 0.12), items)):
        shape(slide, MSO_SHAPE.OVAL, 1.0, y + h / 2 - 0.09, 0.18, 0.18, fill=t["accents"][i % 6])
        text(slide, 1.4, y, W - 2.7, h, str(item), 22, t["soft"], t["body"], anchor=MSO_ANCHOR.MIDDLE)


def _closing(slide, t, spec, accent, n):
    a = t["accents"]
    if t["decor"]:
        icon(slide, "heart", W / 2 - 0.8, 0.7, 1.6, a[0])
        for (x, y, c) in [(3.0, 1.0, a[2]), (9.8, 1.2, a[3]), (2.2, 5.6, a[1]), (10.6, 5.4, a[4])]:
            icon(slide, "star", x, y, 0.5, c)
    text(slide, 1.5, 2.55, W - 3.0, 1.2, spec.get("title", "Thank you"), 44, t["ink"], t["head"], bold=True,
         align=PP_ALIGN.CENTER, anchor=MSO_ANCHOR.MIDDLE, name="Title")
    items = spec.get("items") or []
    for i, ((y, h), item) in enumerate(zip(_rows(len(items), 3.95, 6.8, 0.14), items)):
        text(slide, 2.0, y, W - 4.0, h, str(item), 22, t["soft"], t["body"], align=PP_ALIGN.CENTER,
             anchor=MSO_ANCHOR.MIDDLE)
    shape(slide, MSO_SHAPE.RECTANGLE, 0, H - 0.12, W, 0.12, fill=accent)


# ---------- data and structure layouts ----------

def chart_in(slide, t, c, x, y, w, h):
    """A native PowerPoint chart (editable in PowerPoint, its data in the deck) in the box
    x, y, w, h: c = {"type": "column", "categories": [...], "series": {"Name": [...]}}."""
    from pptx.chart.data import CategoryChartData
    from pptx.enum.chart import XL_CHART_TYPE, XL_LEGEND_POSITION, XL_LABEL_POSITION

    c = c or {}
    kind = c.get("type", "column")
    if kind not in CHARTS:
        raise SystemExit(f"no chart type {kind!r}; choose one of {', '.join(CHARTS)}")
    cats, series = c.get("categories"), c.get("series")
    if not cats or not isinstance(series, dict) or not series:
        raise SystemExit('a chart needs "chart": {"categories": [...], "series": {"Name": [numbers]}}')
    for name, vals in series.items():
        if len(vals) != len(cats):
            raise SystemExit(f"series {name!r} has {len(vals)} values for {len(cats)} categories")
    data = CategoryChartData()
    data.categories = cats
    for name, vals in series.items():
        data.add_series(name, vals)
    xl = {"bar": XL_CHART_TYPE.BAR_CLUSTERED, "column": XL_CHART_TYPE.COLUMN_CLUSTERED,
          "line": XL_CHART_TYPE.LINE_MARKERS, "pie": XL_CHART_TYPE.PIE, "doughnut": XL_CHART_TYPE.DOUGHNUT,
          "area": XL_CHART_TYPE.AREA, "stacked": XL_CHART_TYPE.COLUMN_STACKED}[kind]
    gf = slide.shapes.add_chart(xl, Inches(x), Inches(y), Inches(w), Inches(h), data)
    gf.name = "Chart"
    ch = gf.chart
    ch.has_title = False
    ch.font.name = t["body"]
    ch.font.size = Pt(13)
    ch.font.color.rgb = rgb(t["soft"])
    round_kind = kind in ("pie", "doughnut")
    ch.has_legend = round_kind or len(series) > 1
    if ch.has_legend:
        ch.legend.position = XL_LEGEND_POSITION.BOTTOM
        ch.legend.include_in_layout = False
    plot = ch.plots[0]
    if round_kind:
        plot.has_data_labels = True
        plot.data_labels.number_format = c.get("format", '0"%"' if c.get("percent") else "General")
        plot.data_labels.number_format_is_linked = False
        plot.data_labels.font.color.rgb = rgb("FFFFFF")
        plot.data_labels.font.bold = True
        plot.data_labels.font.size = Pt(16)
        for i, point in enumerate(plot.series[0].points):
            point.format.fill.solid()
            point.format.fill.fore_color.rgb = rgb(t["accents"][i % len(t["accents"])])
    else:
        if kind in ("bar", "column", "stacked"):
            plot.gap_width = 60
        if len(series) == 1 and kind != "line":
            plot.vary_by_categories = False
        for i, ser in enumerate(plot.series):
            col = rgb(t["accents"][i % len(t["accents"])])
            if kind == "line":
                ser.format.line.color.rgb = col
                ser.format.line.width = Pt(3)
                ser.smooth = False
            else:
                ser.format.fill.solid()
                ser.format.fill.fore_color.rgb = col
        plot.has_data_labels = bool(c.get("labels", len(cats) <= 8))
        if plot.has_data_labels:
            plot.data_labels.font.size = Pt(12)
            plot.data_labels.font.color.rgb = rgb(t["ink"])
            if kind in ("bar", "column"):
                plot.data_labels.position = XL_LABEL_POSITION.OUTSIDE_END
        va = ch.value_axis
        va.has_major_gridlines = True
        va.major_gridlines.format.line.color.rgb = tint(t["soft"], 0.8, t["bg"])
        va.format.line.fill.background()
        va.tick_labels.font.size = Pt(12)
        ch.category_axis.tick_labels.font.size = Pt(13)
        ch.category_axis.format.line.color.rgb = tint(t["soft"], 0.6, t["bg"])
    return gf


def _chart(slide, t, spec, accent):
    """The chart layout: the chart, and its takeaways beside it when there are items."""
    side = bool(spec.get("items"))
    cw = W - 1.7 - (4.0 if side else 0)
    chart_in(slide, t, spec.get("chart"), 0.85, 1.75, cw, 5.1)
    if side:
        for i, ((y, h), item) in enumerate(zip(_rows(len(spec["items"]), 1.9, 6.8, 0.2, 1.3), spec["items"])):
            x = W - 4.65
            shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, y, 3.8, h, fill=t["card"],
                  line=tint(t["accents"][i % 6], 0.5, t["bg"]), radius=0.2)
            shape(slide, MSO_SHAPE.RECTANGLE, x, y + 0.15, 0.08, h - 0.3, fill=t["accents"][i % 6])
            text(slide, x + 0.25, y, 3.45, h, str(item), 16, t["soft"], t["body"], anchor=MSO_ANCHOR.MIDDLE)


def table_in(slide, t, cols, rows, x, y, w, h_max, accent, highlight=None):
    """A real, editable table in the box x, y, w (rows as tall as they need, up to h_max):
    a header row in `accent`, banded rows, numbers right-aligned."""
    if not cols or not rows:
        raise SystemExit('a table needs columns [...] and rows [[...], ...]')
    rows = _keep(rows, 12, "a table", "rows")
    n_r, n_c = len(rows) + 1, len(cols)
    h = min(h_max, 0.55 * n_r)
    gf = slide.shapes.add_table(n_r, n_c, Inches(x), Inches(y), Inches(w), Inches(h))
    gf.name = "Table"
    tbl = gf.table
    tbl.first_row = True
    size = 16 if n_r <= 7 else 13
    for ci, name in enumerate(cols):
        _cell(tbl.cell(0, ci), str(name), size, "FFFFFF", t["head"], accent, bold=True)
    for ri, row in enumerate(rows, start=1):
        if len(row) != n_c:
            raise SystemExit(f"row {ri} has {len(row)} cells for {n_c} columns")
        zebra = t["card"] if ri % 2 else _hex(tint(accent, 0.92, t["bg"]))
        fill = _hex(tint(accent, 0.7, t["bg"])) if highlight == ri - 1 else zebra
        for ci, v in enumerate(row):
            num = isinstance(v, (int, float))
            _cell(tbl.cell(ri, ci), f"{v:,}" if num else str(v), size, t["ink"], t["body"], fill,
                  bold=highlight == ri - 1, align=PP_ALIGN.RIGHT if num else PP_ALIGN.LEFT)
    return gf


def _table(slide, t, spec, accent):
    """A real table: "columns": [...], "rows": [[...], ...]; "highlight": row index."""
    table_in(slide, t, spec.get("columns"), spec.get("rows"), 0.85, 1.8, W - 2.05, 5.1, accent,
             spec.get("highlight"))


def _cell(cell, words, pt, color, font, fill, bold=False, align=PP_ALIGN.LEFT):
    cell.fill.solid()
    cell.fill.fore_color.rgb = rgb(fill) if isinstance(fill, str) else fill
    cell.margin_left = cell.margin_right = Inches(0.12)
    cell.vertical_anchor = MSO_ANCHOR.MIDDLE
    tf = cell.text_frame
    tf.word_wrap = True
    p = tf.paragraphs[0]
    p.alignment = align
    r = p.add_run()
    r.text = words
    r.font.size = Pt(pt)
    r.font.bold = bold
    r.font.name = font
    r.font.color.rgb = rgb(color) if isinstance(color, str) else color


def _timeline(slide, t, spec, accent):
    """Dated milestones along a line: items as "When: what"."""
    items = _items(spec, 6)
    n = len(items)
    y = 4.0
    shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.85, y - 0.05, W - 2.05, 0.1, fill=tint(accent, 0.5, t["bg"]), radius=0.5)
    span = (W - 2.05) / n
    for i, item in enumerate(items):
        c = t["accents"][i % 6]
        cx = 0.85 + span * (i + 0.5)
        shape(slide, MSO_SHAPE.OVAL, cx - 0.22, y - 0.22, 0.44, 0.44, fill=c, line=t["bg"], lw=3)
        when, _, what = str(item).partition(":")
        up = i % 2 == 0
        box_y = 1.85 if up else 4.5
        shape(slide, MSO_SHAPE.RECTANGLE, cx - 0.015, (box_y + 1.6) if up else y + 0.22, 0.03,
              (y - 0.22 - box_y - 1.6) if up else (box_y - y - 0.22), fill=tint(c, 0.4, t["bg"]))
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, cx - span / 2 + 0.08, box_y, span - 0.16, 1.6, fill=t["card"],
              line=tint(c, 0.5, t["bg"]), radius=0.15)
        text(slide, cx - span / 2 + 0.15, box_y + 0.08, span - 0.3, 0.5, when.strip(), 18, shade(c, 0.25),
             t["head"], bold=True, align=PP_ALIGN.CENTER)
        text(slide, cx - span / 2 + 0.15, box_y + 0.55, span - 0.3, 1.0, (what or "").strip(), 14, t["soft"],
             t["body"], align=PP_ALIGN.CENTER)


def _compare(slide, t, spec, accent):
    """Two columns side by side: "left": {"title", "items"}, "right": {...}, "verdict"."""
    a, b = spec.get("left"), spec.get("right")
    if not a or not b:
        raise SystemExit('compare needs "left": {"title", "items"} and "right": {"title", "items"}')
    cw = (W - 2.05 - 0.9) / 2
    bottom = 5.95 if spec.get("verdict") else 6.9
    for k, (side, c) in enumerate(((a, t["accents"][0]), (b, t["accents"][1]))):
        x = 0.85 + k * (cw + 0.9)
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, 1.8, cw, bottom - 1.8, fill=t["card"],
              line=tint(c, 0.4, t["bg"]), lw=2, radius=0.08)
        head = shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, 1.8, cw, 0.75, fill=c, radius=0.3)
        _label(head, side.get("title", ""), 20, "FFFFFF", t["head"])
        for i, ((y, h), item) in enumerate(zip(_rows(len(side.get("items", [])), 2.75, bottom - 0.15, 0.08, 0.75),
                                               side.get("items", []))):
            mark = "✓" if k == 0 else "•"
            mark = side.get("mark", mark)
            text(slide, x + 0.25, y, 0.4, h, mark, 18, c, t["head"], bold=True, anchor=MSO_ANCHOR.MIDDLE, fit=False)
            text(slide, x + 0.65, y, cw - 0.9, h, str(item), 17, t["soft"], t["body"], anchor=MSO_ANCHOR.MIDDLE)
    vs = shape(slide, MSO_SHAPE.OVAL, W / 2 - 0.6 - 0.15, 3.6, 0.6, 0.6, fill=t["ink"])
    _label(vs, "vs", 14, t["bg"], t["head"])
    if spec.get("verdict"):
        box = shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.85, 6.1, W - 2.05, 0.8, fill=tint(accent, 0.8, t["bg"]),
                    radius=0.4)
        box.name = "Verdict"
        text(slide, 1.1, 6.1, W - 2.6, 0.8, spec["verdict"], 18, t["ink"], t["body"], bold=True,
             align=PP_ALIGN.CENTER, anchor=MSO_ANCHOR.MIDDLE)


def _grid(slide, t, spec, accent):
    """Tiles with an icon, a head and a line: items as {"icon", "head", "text"} (3-6)."""
    items = _items(spec, 6)
    n = len(items)
    cols = 3 if n in (3, 5, 6) else 2 if n in (2, 4) else n
    rws = (n + cols - 1) // cols
    gap = 0.25
    tw = (W - 2.05 - gap * (cols - 1)) / cols
    th = min(2.1, (5.15 - gap * (rws - 1)) / rws)
    top = 1.8 if rws > 1 else 2.6  # one row sits in the middle of the space
    for i, item in enumerate(items):
        if isinstance(item, str):
            head, _, body = item.partition(":")
            item = {"head": head, "text": body}
        c = t["accents"][i % 6]
        x = 0.85 + (i % cols) * (tw + gap)
        y = top + (i // cols) * (th + gap)
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, y, tw, th, fill=t["card"], line=tint(c, 0.5, t["bg"]),
              radius=0.12)
        badge(slide, item.get("icon", "star"), x + 0.25, y + 0.25, 0.8, c)
        text(slide, x + 1.2, y + 0.25, tw - 1.4, 0.8, item.get("head", "").strip(), 19, t["ink"], t["head"],
             bold=True, anchor=MSO_ANCHOR.MIDDLE)
        text(slide, x + 0.25, y + 1.15, tw - 0.5, th - 1.3, item.get("text", "").strip(), 17, t["soft"], t["body"])


def _kpis(slide, t, spec, accent):
    """Big numbers: items as {"value", "label", "delta"} (2-4)."""
    items = _items(spec, 4)
    n = len(items)
    gap = 0.3
    w = (W - 2.05 - gap * (n - 1)) / n
    for i, item in enumerate(items):
        c = t["accents"][i % 6]
        x = 0.85 + i * (w + gap)
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, 2.0, w, 3.6, fill=t["card"], line=tint(c, 0.5, t["bg"]),
              radius=0.1)
        shape(slide, MSO_SHAPE.RECTANGLE, x + 0.3, 2.0, w - 0.6, 0.1, fill=c)
        text(slide, x + 0.2, 2.4, w - 0.4, 1.4, str(item.get("value", "")), 54, shade(c, 0.15), t["head"],
             bold=True, align=PP_ALIGN.CENTER, anchor=MSO_ANCHOR.MIDDLE)
        text(slide, x + 0.2, 3.85, w - 0.4, 0.9, str(item.get("label", "")), 17, t["soft"], t["body"],
             align=PP_ALIGN.CENTER)
        if item.get("delta"):
            d = str(item["delta"])
            up = not d.strip().startswith("-")
            # "better": "down" for a number that should fall (churn, cost, wait).
            good = up if item.get("better", "up") == "up" else not up
            pill = shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x + w / 2 - 0.8, 4.85, 1.6, 0.45,
                         fill="16A34A" if good else "DC2626", radius=0.5)
            _label(pill, ("▲ " if up else "▼ ") + d.lstrip("+-"), 13, "FFFFFF", t["body"])
    if spec.get("footer"):
        text(slide, 0.85, 5.9, W - 2.05, 0.8, spec["footer"], 17, t["soft"], t["body"], align=PP_ALIGN.CENTER)


def _quote(slide, t, spec, accent):
    q = spec.get("quote") or spec.get("statement")
    if not q:
        raise SystemExit('quote needs "quote" and, optionally, "by"')
    text(slide, 1.0, 1.4, 1.5, 1.6, "\u201c", 140, tint(accent, 0.4, t["bg"]), "Georgia", bold=True, fit=False)
    text(slide, 2.2, 2.1, W - 4.2, 3.0, q, 34, t["ink"], t["head"], bold=True, anchor=MSO_ANCHOR.MIDDLE)
    if spec.get("by"):
        shape(slide, MSO_SHAPE.RECTANGLE, 2.25, 5.35, 0.6, 0.06, fill=accent)
        text(slide, 3.0, 5.1, W - 5.0, 0.6, spec["by"], 18, t["soft"], t["body"], anchor=MSO_ANCHOR.MIDDLE)


def _image(slide, t, spec, accent):
    """A picture with a caption and points: "path", "caption", "items"."""
    import os
    path = spec.get("path")
    if not path or not os.path.isfile(path):
        raise SystemExit(f'image needs "path" to a picture file; {path!r} was not found')
    from PIL import Image
    with Image.open(path) as im:
        iw, ih = im.size
    box_w, box_h = (7.2 if spec.get("items") else W - 2.05), 4.6
    k = min(box_w / iw, box_h / ih)
    w, h = iw * k, ih * k
    pic = slide.shapes.add_picture(path, Inches(0.85 + (box_w - w) / 2), Inches(1.8 + (box_h - h) / 2),
                                   Inches(w), Inches(h))
    pic.name = "Picture"
    if spec.get("caption"):
        text(slide, 0.85, 6.5, box_w, 0.4, spec["caption"], 13, t["soft"], t["body"], align=PP_ALIGN.CENTER)
    for i, ((y, hh), item) in enumerate(zip(_rows(len(spec.get("items") or []), 1.9, 6.8, 0.15, 1.1),
                                            spec.get("items") or [])):
        shape(slide, MSO_SHAPE.OVAL, 8.5, y + hh / 2 - 0.09, 0.18, 0.18, fill=t["accents"][i % 6])
        text(slide, 8.85, y, W - 10.0, hh, str(item), 17, t["soft"], t["body"], anchor=MSO_ANCHOR.MIDDLE)


def _cycle(slide, t, spec, accent):
    """3-6 stages round a circle, each pointing to the next."""
    import math
    items = _items(spec, 6)
    n = len(items)
    cx, cy, r = W / 2, 4.35, 2.05
    hub = shape(slide, MSO_SHAPE.OVAL, cx - 0.95, cy - 0.95, 1.9, 1.9, fill=tint(accent, 0.8, t["bg"]))
    if spec.get("center"):
        _label(hub, spec["center"], 15, t["ink"], t["head"])
    for i, item in enumerate(items):
        a = -math.pi / 2 + 2 * math.pi * i / n
        c = t["accents"][i % 6]
        x, y = cx + r * math.cos(a), cy + r * math.sin(a)
        node = shape(slide, MSO_SHAPE.OVAL, x - 0.42, y - 0.42, 0.84, 0.84, fill=c)
        _label(node, str(i + 1), 20, "FFFFFF", t["head"])
        # Arrow along the circle to the next stage.
        b = a + math.pi / n
        ax, ay = cx + r * math.cos(b), cy + r * math.sin(b)
        arr = shape(slide, MSO_SHAPE.RIGHT_ARROW, ax - 0.22, ay - 0.15, 0.44, 0.3, fill=tint(c, 0.4, t["bg"]))
        arr.rotation = math.degrees(b) + 90
        right = math.cos(a) >= -0.01
        lx = x + 0.55 if right else x - 0.55 - 3.3
        if abs(math.cos(a)) < 0.2:
            lx = x + 0.55
        text(slide, lx, y - 0.45, 3.3, 0.9, str(item), 15, t["soft"], t["body"],
             align=PP_ALIGN.LEFT if lx > x else PP_ALIGN.RIGHT, anchor=MSO_ANCHOR.MIDDLE)


def _pyramid(slide, t, spec, accent):
    """Levels stacked from a wide base to a narrow top: items top first."""
    items = _items(spec, 5)
    n = len(items)
    top, h, gap = 1.8, (5.1 - 0.1 * (n - 1)) / n, 0.1
    full = 7.0
    for i, item in enumerate(items):
        w = full * (i + 1) / n
        c = t["accents"][i % 6]
        x = 0.85 + (full - w) / 2
        y = top + i * (h + gap)
        tr = shape(slide, MSO_SHAPE.TRAPEZOID if i else MSO_SHAPE.ISOSCELES_TRIANGLE, x, y, w, h, fill=c)
        if i:
            # Inset each side by half a step, so the levels' edges line up.
            tr.adjustments[0] = (full / n / 2) / min(w, h)
        head, _, body = str(item).partition(":")
        text(slide, 8.3, y, W - 9.5, h, head.strip() + ("\n" + body.strip() if body else ""), 16, t["soft"],
             t["body"], anchor=MSO_ANCHOR.MIDDLE)
        shape(slide, MSO_SHAPE.RECTANGLE, 0.85 + full / 2 + w / 2 + 0.1, y + h / 2 - 0.015,
              8.2 - (0.85 + full / 2 + w / 2 + 0.1), 0.03, fill=tint(c, 0.4, t["bg"]))


def _title_dark(slide, t, spec, accent):
    """A deep background, the title large on the left, and a cluster of
    friendly faces (or an icon) on the right: a cover that sets the tone."""
    a = t["accents"]
    deep = _hex(shade(a[1], 0.62))
    fill = slide.background.fill
    fill.solid()
    fill.fore_color.rgb = rgb(deep)
    text(slide, 0.9, 2.3, 7.2, 1.6, spec.get("title", ""), 50, "FFFFFF", t["head"], bold=True,
         anchor=MSO_ANCHOR.BOTTOM, name="Title")
    if spec.get("subtitle"):
        text(slide, 0.9, 4.05, 6.8, 1.0, spec["subtitle"], 20, tint(deep, 0.75), t["body"])
    if spec.get("tag"):
        pill = shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.9, 5.3, min(4.5, 0.6 + len(spec["tag"]) * 0.12), 0.5,
                     fill=a[0], radius=0.5)
        _label(pill, spec["tag"], 14, "FFFFFF", t["body"])
    art = spec.get("icon", "faces")
    if art == "faces":
        for (cx, cy, d, c, mood) in [(10.2, 2.9, 2.6, a[1], "happy"), (8.9, 5.2, 1.35, a[2], "happy"),
                                     (11.8, 5.4, 1.05, mood_pink(a), "happy")]:
            shape(slide, MSO_SHAPE.OVAL, cx - d / 2, cy - d / 2, d, d, fill=c)
            face(slide, cx, cy, d, mood, _hex(shade(deep, 0.2)))
        for (x, y, d, c) in [(9.3, 1.0, 0.22, a[2]), (12.2, 0.9, 0.3, a[0]), (12.6, 3.4, 0.18, a[3])]:
            shape(slide, MSO_SHAPE.OVAL, x, y, d, d, fill=c)
    elif art != "none":
        badge(slide, art, 8.9, 1.7, 3.4, a[1])


def mood_pink(a):
    return "FF8FAB" if "FF6B6B" in a else a[3]


def _feature(slide, t, spec, accent):
    """One big number in a hero card, and icon cards beside it:
    {"stat": "Trillions", "stat_text": "of cells…", "note": "…", "items": [{"icon", "head", "text"}]}."""
    items = _keep(spec.get("items") or [], 4, f"layout {spec.get('layout')!r}", "items")
    hero_w = 4.2 if items else W - 2.05
    shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, 0.85, 1.8, hero_w, 5.05, fill=tint(accent, 0.86, t["bg"]),
          radius=0.06)
    text(slide, 1.2, 2.2, hero_w - 0.7, 1.4, str(spec.get("stat", "")), 60, accent, t["head"], bold=True,
         anchor=MSO_ANCHOR.MIDDLE, name="Stat")
    if spec.get("stat_text"):
        text(slide, 1.2, 3.7, hero_w - 0.7, 1.3, spec["stat_text"], 20, t["ink"], t["body"])
    if spec.get("note"):
        text(slide, 1.2, 5.5, hero_w - 0.7, 1.1, spec["note"], 15, t["soft"], t["body"])
        for r in slide.shapes[-1].text_frame.paragraphs[0].runs:
            r.font.italic = True
    if not items:
        return
    gap = 0.25
    cols = 2
    rows = (len(items) + 1) // 2
    x0 = 0.85 + hero_w + gap
    cw = (W - 1.2 - x0 - gap) / cols
    ch = (5.05 - gap * (rows - 1)) / rows
    for i, item in enumerate(items):
        if isinstance(item, str):
            head, _, body = item.partition(":")
            item = {"head": head, "text": body}
        c = t["accents"][i % 6]
        x = x0 + (i % cols) * (cw + gap)
        y = 1.8 + (i // cols) * (ch + gap)
        shape(slide, MSO_SHAPE.ROUNDED_RECTANGLE, x, y, cw, ch, fill=t["card"], line=tint(c, 0.75, t["bg"]),
              radius=0.06)
        badge(slide, item.get("icon", "star"), x + 0.3, y + 0.3, 0.85, c)
        text(slide, x + 0.3, y + 1.3, cw - 0.6, 0.55, item.get("head", "").strip(), 19, shade(c, 0.2), t["head"],
             bold=True)
        text(slide, x + 0.3, y + 1.8, cw - 0.6, ch - 1.95, item.get("text", "").strip(), 15, t["soft"], t["body"])
