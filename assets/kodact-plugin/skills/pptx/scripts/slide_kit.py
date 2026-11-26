"""Slides as code, on the deck's design system: any layout you can describe, drawn with the theme's
colours, type sizes, spacing, cards, icons and native charts, so a slide of your own still looks
like the rest of the deck. Mixes with `pptx_tool.py add` slides in the same file.

    import sys; sys.path.insert(0, r"<this folder>")
    from slide_kit import Deck

    with Deck("deck.pptx", audience="hospital board", title="Q3 review") as deck:  # opens or makes it
        s = deck.slide("Three things drove the quarter", icon="trending-up")
        left, right = s.area.cols(7, 5)                 # 12-column grid: 7 + 5
        s.chart(left, "column", ["Q1", "Q2", "Q3"], {"Visits": [120, 150, 210]})
        for box, (head, body) in zip(right.rows(3), [("Access", "New clinic"), ...]):
            s.card(box)
            s.icon("check", box.inset(0.2).left(0.5).top(0.5))
            s.text(box.inset(0.2).cut_left(0.7), head, size="h3", bold=True, color="ink")
        s.notes("What to say.")
    # saved on leaving the `with`; text too long for its box is reported on stderr

Boxes are inches on the 13.33 x 7.5 slide. `s.area` is the content area under the title (the
whole slide below the margins when the slide has no title). A Box splits itself:
  cols(*spans)            12-column grid, e.g. cols(4, 4, 4) or cols(8, 4); gutters included
  split(n, axis="x")      n equal parts with gaps; rows(n) is split(n, axis="y")
  rows(*weights)          rows(1, 2) gives the second twice the height of the first
  inset(d) / inset(dx, dy), top(h) / bottom(h) / left(w) / right(w) (an edge strip),
  cut_top(h) / cut_bottom(h) / cut_left(w) / cut_right(w) (what is left), center(w, h)

Drawing (each returns the shape):
  s.text(box, words, size="body", color="soft", bold=False, align="left", anchor="top",
         font="body")       sizes: title h1 h2 h3 lead body small caption stat, or points;
                           text shrinks to fit its box (never under 12pt; a note says when not)
  s.bullets(box, items, marker="dot"|"number"|"check"|ICON, size="body", color="soft")
  s.card(box, style="card"|"tint"|"solid"|"outline", accent=None)
  s.icon(name, box, color="accent", badge=False)   any Lucide name, or sun star heart cloud cell
  s.stat(box, value, label, delta=None, better="up")   a big number with its label
  s.chart(box, type, categories, series, labels=None)   bar column line pie doughnut area stacked
  s.table(box, columns, rows, highlight=None)
  s.image(box, path, fit="contain"|"cover")
  s.shape(kind, box, fill="accent", line=None)     kind: rect rounded oval line-like names below
  s.line(x1, y1, x2, y2, color="soft", width=1.5)
  s.notes(text)
Colours: ink soft bg card white accent (this slide's), accent0..accent5, or a hex "1F2A3A".

deck.slide(title=None, icon=None, accent=None, section=None, chrome=True) starts a slide with the
theme's background, and, with chrome, its corner decoration, footer bar and number.
"""

from __future__ import annotations

import os
import sys
from dataclasses import dataclass

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "scripts"))

from pptx import Presentation  # noqa: E402
from pptx.enum.shapes import MSO_CONNECTOR, MSO_SHAPE  # noqa: E402
from pptx.enum.text import MSO_ANCHOR, PP_ALIGN  # noqa: E402
from pptx.util import Inches, Pt  # noqa: E402

import deck_design as dd  # noqa: E402
import pptx_tool  # noqa: E402

W, H = dd.W, dd.H
MARGIN = 0.85
GUTTER = 0.25

SIZES = {"h1": 30, "h2": 24, "h3": 20, "lead": 22, "body": 18, "small": 15, "caption": 13, "stat": 54}
ALIGN = {"left": PP_ALIGN.LEFT, "center": PP_ALIGN.CENTER, "right": PP_ALIGN.RIGHT}
ANCHOR = {"top": MSO_ANCHOR.TOP, "middle": MSO_ANCHOR.MIDDLE, "bottom": MSO_ANCHOR.BOTTOM}
SHAPES = {"rect": MSO_SHAPE.RECTANGLE, "rounded": MSO_SHAPE.ROUNDED_RECTANGLE, "oval": MSO_SHAPE.OVAL,
          "triangle": MSO_SHAPE.ISOSCELES_TRIANGLE, "chevron": MSO_SHAPE.CHEVRON,
          "arrow": MSO_SHAPE.RIGHT_ARROW, "pentagon": MSO_SHAPE.PENTAGON, "diamond": MSO_SHAPE.DIAMOND,
          "hexagon": MSO_SHAPE.HEXAGON, "star": MSO_SHAPE.STAR_5_POINT}


@dataclass(frozen=True)
class Box:
    """A rectangle in inches: x, y from the slide's top-left corner, w, h its size."""

    x: float
    y: float
    w: float
    h: float

    def split(self, n: int, axis: str = "x", gap: float = GUTTER) -> list["Box"]:
        if n < 1:
            return []
        if axis == "x":
            w = (self.w - gap * (n - 1)) / n
            return [Box(self.x + i * (w + gap), self.y, w, self.h) for i in range(n)]
        h = (self.h - gap * (n - 1)) / n
        return [Box(self.x, self.y + i * (h + gap), self.w, h) for i in range(n)]

    def cols(self, *spans: int, gap: float = GUTTER) -> list["Box"]:
        """Columns on a 12-column grid: cols(4, 4, 4), cols(7, 5); spans must add up to 12 or less."""
        if not spans or sum(spans) > 12:
            raise ValueError(f"cols takes spans adding up to 12 at most, got {spans}")
        unit = (self.w - gap * 11) / 12
        out, x = [], self.x
        for s in spans:
            w = unit * s + gap * (s - 1)
            out.append(Box(x, self.y, w, self.h))
            x += w + gap
        return out

    def rows(self, *weights: float, gap: float = GUTTER) -> list["Box"]:
        """rows(3): three equal rows; rows(1, 2): the second twice the first."""
        if len(weights) == 1 and float(weights[0]).is_integer() and weights[0] >= 1:
            return self.split(int(weights[0]), "y", gap)
        total = sum(weights)
        free = self.h - gap * (len(weights) - 1)
        out, y = [], self.y
        for wgt in weights:
            h = free * wgt / total
            out.append(Box(self.x, y, self.w, h))
            y += h + gap
        return out

    def inset(self, dx: float, dy: float | None = None) -> "Box":
        dy = dx if dy is None else dy
        return Box(self.x + dx, self.y + dy, max(0.0, self.w - 2 * dx), max(0.0, self.h - 2 * dy))

    def top(self, h: float) -> "Box":
        return Box(self.x, self.y, self.w, min(h, self.h))

    def bottom(self, h: float) -> "Box":
        h = min(h, self.h)
        return Box(self.x, self.y + self.h - h, self.w, h)

    def left(self, w: float) -> "Box":
        return Box(self.x, self.y, min(w, self.w), self.h)

    def right(self, w: float) -> "Box":
        w = min(w, self.w)
        return Box(self.x + self.w - w, self.y, w, self.h)

    def cut_top(self, h: float) -> "Box":
        return Box(self.x, self.y + h, self.w, max(0.0, self.h - h))

    def cut_bottom(self, h: float) -> "Box":
        return Box(self.x, self.y, self.w, max(0.0, self.h - h))

    def cut_left(self, w: float) -> "Box":
        return Box(self.x + w, self.y, max(0.0, self.w - w), self.h)

    def cut_right(self, w: float) -> "Box":
        return Box(self.x, self.y, max(0.0, self.w - w), self.h)

    def center(self, w: float, h: float) -> "Box":
        w, h = min(w, self.w), min(h, self.h)
        return Box(self.x + (self.w - w) / 2, self.y + (self.h - h) / 2, w, h)


def _box(b) -> Box:
    if isinstance(b, Box):
        return b
    if isinstance(b, (tuple, list)) and len(b) == 4:
        return Box(*map(float, b))
    raise TypeError(f"a box is a Box or (x, y, w, h) in inches, not {b!r}")


class Slide:
    def __init__(self, deck: "Deck", slide, accent: str, titled: bool):
        self.deck, self.raw, self.accent = deck, slide, accent
        t = deck.t
        self.t = t
        top = 1.75 if titled else 0.6
        self.area = Box(MARGIN, top, W - 2 * MARGIN, H - 0.55 - top)
        self.full = Box(0, 0, W, H)

    # ---------- tokens ----------

    def color(self, c) -> str:
        t = self.t
        if c is None:
            return None
        if isinstance(c, int):
            return t["accents"][c % len(t["accents"])]
        named = {"ink": t["ink"], "soft": t["soft"], "bg": t["bg"], "card": t["card"], "white": "FFFFFF",
                 "accent": self.accent}
        if c in named:
            return named[c]
        if isinstance(c, str) and c.startswith("accent") and c[6:].isdigit():
            return t["accents"][int(c[6:]) % len(t["accents"])]
        hx = str(c).lstrip("#")
        if len(hx) == 6 and all(ch in "0123456789abcdefABCDEF" for ch in hx):
            return hx.upper()
        raise ValueError(f"no colour {c!r}: use ink soft bg card white accent accent0-5 or a hex")

    def size(self, s) -> float:
        if isinstance(s, (int, float)):
            return float(s)
        if s == "title":
            return float(self.t["title_pt"])
        if s in SIZES:
            return float(SIZES[s])
        raise ValueError(f"no size {s!r}: use title {' '.join(SIZES)} or points")

    # ---------- drawing ----------

    def text(self, box, words, size="body", color="soft", bold=False, align="left", anchor="top",
             font="body", fit=True):
        b = _box(box)
        face = self.t["head"] if font == "head" or size in ("title", "h1", "h2", "stat") else self.t["body"]
        return dd.text(self.raw, b.x, b.y, b.w, b.h, str(words), self.size(size), self.color(color), face,
                       bold=bold, align=ALIGN[align], anchor=ANCHOR[anchor], fit=fit)

    def bullets(self, box, items, marker="dot", size="body", color="soft", gap=0.12, marker_color=None):
        """One row per item, its marker beside it; rows share the box, text fits each row. Markers
        are in the accent, or in the text's colour when the text is white (on a coloured panel,
        an accent marker would vanish into it)."""
        b = _box(box)
        rows = b.split(len(items), "y", gap)
        shapes = []
        pt = self.size(size)
        mark = min(0.42, pt / 72 * 1.3)
        for i, (row, item) in enumerate(zip(rows, items)):
            m = row.left(mark).top(mark).inset(0.02)
            if marker_color is not None:
                c = self.color(marker_color)
            elif color == "white":
                c = "FFFFFF"
            else:
                c = self.color(i % 6) if marker == "number" else self.accent
            if marker == "dot":
                d = mark * 0.4
                dd.shape(self.raw, MSO_SHAPE.OVAL, m.x + (m.w - d) / 2, m.y + (m.h - d) / 2, d, d, fill=c)
            elif marker == "number":
                dd.shape(self.raw, MSO_SHAPE.OVAL, m.x, m.y, m.w, m.h, fill=c)
                num_ink = self.accent if c == "FFFFFF" else "FFFFFF"
                dd.text(self.raw, m.x, m.y, m.w, m.h, str(i + 1), max(10, pt * 0.6), num_ink, self.t["head"],
                        bold=True, align=PP_ALIGN.CENTER, anchor=MSO_ANCHOR.MIDDLE, fit=False, name="Bullet number")
            else:
                dd.icon(self.raw, "check" if marker == "check" else marker, m.x, m.y, m.w, c, self.t["ink"])
            shapes.append(self.text(row.cut_left(mark + 0.18), item, size=size, color=color))
        return shapes

    def card(self, box, style="card", accent=None):
        b = _box(box)
        a = self.color(accent) if accent is not None else self.accent
        t = self.t
        fill, line = {
            "card": (t["card"], dd.tint(a, 0.55, t["bg"])),
            "tint": (dd._hex(dd.tint(a, 0.86, t["bg"])), None),
            "solid": (a, None),
            "outline": (None, a),
        }.get(style, (None, None))
        if style not in ("card", "tint", "solid", "outline"):
            raise ValueError(f"no card style {style!r}: card tint solid outline")
        return dd.shape(self.raw, MSO_SHAPE.ROUNDED_RECTANGLE, b.x, b.y, b.w, b.h, fill=fill, line=line,
                        lw=1.5, radius=min(0.3, t["radius"] * 0.6) if t["radius"] else 0.02)

    def icon(self, name, box, color="accent", badge=False):
        b = _box(box)
        d = min(b.w, b.h)
        x, y = b.x + (b.w - d) / 2, b.y + (b.h - d) / 2
        if badge:
            return dd.badge(self.raw, name, x, y, d, self.color(color))
        return dd.icon(self.raw, name, x, y, d, self.color(color), self.t["ink"])

    def stat(self, box, value, label, delta=None, better="up"):
        """A big number, its label under it, and a change pill when `delta` is given."""
        b = _box(box)
        top, low = b.rows(3, 1, gap=0.05) if not delta else b.rows(3, 1, 1, gap=0.05)[:2]
        self.text(top, value, size="stat", color=dd._hex(dd.shade(self.accent, 0.15)), bold=True,
                  align="center", anchor="bottom")
        self.text(low, label, size="small", color="soft", align="center")
        if delta:
            pill = b.bottom(b.h / 5).center(min(1.6, b.w), 0.36)
            d = str(delta).strip()
            up = not d.startswith(("-", "−", "▼"))
            good = up if better == "up" else not up
            col = "16A34A" if good else "DC2626"
            dd.shape(self.raw, MSO_SHAPE.ROUNDED_RECTANGLE, pill.x, pill.y, pill.w, pill.h, fill=col, radius=0.5)
            dd.text(self.raw, pill.x, pill.y, pill.w, pill.h, ("▲ " if up else "▼ ") + d.lstrip("+-−▲▼ "), 12,
                    "FFFFFF", self.t["body"], bold=True, align=PP_ALIGN.CENTER, anchor=MSO_ANCHOR.MIDDLE,
                    fit=False, name="Delta")

    def chart(self, box, type, categories, series, labels=None):
        b = _box(box)
        spec = {"type": type, "categories": list(categories), "series": dict(series)}
        if labels is not None:
            spec["labels"] = labels
        return dd.chart_in(self.raw, self.t, spec, b.x, b.y, b.w, b.h)

    def table(self, box, columns, rows, highlight=None):
        b = _box(box)
        return dd.table_in(self.raw, self.t, list(columns), [list(r) for r in rows], b.x, b.y, b.w, b.h,
                           self.accent, highlight)

    def image(self, box, path, fit="contain"):
        b = _box(box)
        if not os.path.isfile(path):
            raise FileNotFoundError(f"no image at {path}")
        from PIL import Image

        with Image.open(path) as im:
            iw, ih = im.size
        ratio = iw / ih
        if fit == "cover":
            w, h = (b.w, b.w / ratio) if b.w / ratio >= b.h else (b.h * ratio, b.h)
            pic = self.raw.shapes.add_picture(path, Inches(b.x), Inches(b.y), Inches(w), Inches(h))
            # Crop what spills over the box, evenly on both sides.
            cx, cy = max(0.0, (w - b.w) / w), max(0.0, (h - b.h) / h)
            pic.crop_left = pic.crop_right = cx / 2
            pic.crop_top = pic.crop_bottom = cy / 2
            pic.left, pic.top, pic.width, pic.height = Inches(b.x), Inches(b.y), Inches(b.w), Inches(b.h)
        else:
            w, h = (b.w, b.w / ratio) if b.w / ratio <= b.h else (b.h * ratio, b.h)
            pic = self.raw.shapes.add_picture(path, Inches(b.x + (b.w - w) / 2), Inches(b.y + (b.h - h) / 2),
                                              Inches(w), Inches(h))
        pic.name = "Picture"
        return pic

    def shape(self, kind, box, fill="accent", line=None, width=1.5):
        b = _box(box)
        if kind not in SHAPES:
            raise ValueError(f"no shape {kind!r}: {' '.join(SHAPES)}")
        return dd.shape(self.raw, SHAPES[kind], b.x, b.y, b.w, b.h, fill=self.color(fill), line=self.color(line),
                        lw=width)

    def line(self, x1, y1, x2, y2, color="soft", width=1.5):
        c = self.raw.shapes.add_connector(MSO_CONNECTOR.STRAIGHT, Inches(x1), Inches(y1), Inches(x2), Inches(y2))
        c.line.color.rgb = dd.rgb(self.color(color))
        c.line.width = Pt(width)
        return c

    def notes(self, text):
        self.raw.notes_slide.notes_text_frame.text = str(text)


class Deck:
    """A deck to draw slides into: opens `path`, or makes it (a designed title slide first) when it
    isn't there. Saves on `save()`, or when a `with` block ends without an error."""

    def __init__(self, path, theme=None, audience=None, title=None, subtitle=None):
        self.path = str(path)
        if not os.path.exists(self.path):
            name = theme or (dd.for_audience(audience) if audience else "clean")
            pptx_tool.init(self.path, name, title=title, subtitle=subtitle)
        self.prs = Presentation(self.path)
        self.theme = pptx_tool._theme_of(self.prs, theme)
        self.t = dd.theme(self.theme)
        self.made = 0

    def slide(self, title=None, icon=None, accent=None, section=None, chrome=True) -> Slide:
        t = self.t
        n = len(self.prs.slides) + 1
        acc = t["accents"]
        a = acc[(n - 1) % len(acc)] if accent is None else (
            acc[accent % len(acc)] if isinstance(accent, int) else str(accent).lstrip("#"))
        raw = self.prs.slides.add_slide(self.prs.slide_layouts[6])
        dd.background(raw, t)
        if chrome:
            dd.chrome(raw, t, n, a)
        if title:
            dd.header(raw, t, title, a, icon or ("star" if t["decor"] else "none"))
        pptx_tool._place_in_section(self.prs, raw, section)
        self.made += 1
        return Slide(self, raw, a, bool(title))

    def save(self):
        pptx_tool._finish(self.prs)
        pptx_tool._save(self.prs, self.path)
        print(f"saved {self.path}: {self.made} slide(s) drawn, {len(self.prs.slides)} in the deck; "
              f"now check it: python validate.py {os.path.basename(self.path)}")

    def __enter__(self):
        return self

    def __exit__(self, kind, value, tb):
        if kind is None:
            self.save()
        return False
