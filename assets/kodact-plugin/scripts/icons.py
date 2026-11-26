"""Lucide's 1,500+ icons (ISC, icons/LICENSE-lucide.txt) for the documents:
as native PowerPoint shapes (vector paths, recolourable in PowerPoint, sharp
at any size) and as PNG pictures for Word and Excel.

    names()                          every icon name ("heart", "droplet", "brain"…)
    find(word)                       the closest name for a word ("doctor" → "stethoscope")
    add_to_slide(slide, name, x, y, size, color, stroke=2.0)   inches, hex colour
    png(name, path, px=256, color="1F2A3A", stroke=2.0)

Icons are drawn on a 24-unit grid as strokes (2 units wide, round ends),
exactly as Lucide draws them.
"""

import json
import math
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
_ICONS = None

# Words people use for things Lucide names otherwise.
ALIASES = {
    "doctor": "stethoscope", "nurse": "stethoscope", "medicine": "pill", "drug": "pill", "blood": "droplet",
    "water": "droplet", "skin": "hand", "body": "person-standing", "brain": "brain", "mind": "brain",
    "bone": "bone", "cell": "microscope", "cells": "microscope", "science": "flask-conical", "dna": "dna",
    "family": "users", "people": "users", "team": "users", "friend": "smile", "happy": "smile",
    "sad": "frown", "feelings": "heart-handshake", "love": "heart", "care": "hand-heart", "help": "life-buoy",
    "hospital": "hospital", "money": "wallet", "revenue": "trending-up", "growth": "trending-up",
    "cost": "receipt", "time": "clock", "date": "calendar", "idea": "lightbulb", "goal": "target",
    "security": "shield-check", "safe": "shield-check", "speed": "zap", "fast": "zap", "quality": "badge-check",
    "school": "school", "learn": "graduation-cap", "book": "book-open", "question": "circle-help",
    "talk": "message-circle", "chat": "messages-square", "home": "house", "world": "globe", "cloud": "cloud",
    "data": "database", "chart": "chart-column", "report": "file-text", "phone": "phone", "mail": "mail",
    "star": "star", "sun": "sun", "sleep": "moon", "food": "apple", "play": "gamepad-2", "music": "music",
    "art": "palette", "tumor": "circle-dot", "treatment": "syringe", "radiation": "radiation",
}


def _icons():
    global _ICONS
    if _ICONS is None:
        with open(os.path.join(HERE, "icons", "lucide.json"), encoding="utf-8") as f:
            _ICONS = json.load(f)
    return _ICONS


def names():
    return sorted(_icons())


def find(word):
    """An icon name for `word`: itself, an alias, or the first name containing it."""
    w = str(word).strip().lower().replace(" ", "-")
    icons = _icons()
    if w in icons:
        return w
    if w in ALIASES and ALIASES[w] in icons:
        return ALIASES[w]
    for n in sorted(icons, key=len):
        if w in n.split("-"):
            return n
    return None


def require(name):
    n = find(name)
    if n is None:
        raise SystemExit(f"no icon {name!r}; `python icons.py search WORD` lists names")
    return n


# ---------- SVG geometry → subpaths of (op, points) in the 24-unit grid ----------

_NUM = re.compile(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")
_TOKEN = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")


def _arc_to_beziers(x1, y1, rx, ry, phi, large, sweep, x2, y2):
    """An SVG arc as cubic Béziers (endpoint to centre parametrisation)."""
    if rx == 0 or ry == 0:
        return [("L", [(x2, y2)])]
    phi = math.radians(phi)
    cp, sp = math.cos(phi), math.sin(phi)
    dx, dy = (x1 - x2) / 2, (y1 - y2) / 2
    x1p, y1p = cp * dx + sp * dy, -sp * dx + cp * dy
    rx, ry = abs(rx), abs(ry)
    lam = (x1p ** 2) / (rx ** 2) + (y1p ** 2) / (ry ** 2)
    if lam > 1:
        rx, ry = rx * math.sqrt(lam), ry * math.sqrt(lam)
    num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
    den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
    co = math.sqrt(max(0.0, num / den)) if den else 0.0
    if large == sweep:
        co = -co
    cxp, cyp = co * rx * y1p / ry, -co * ry * x1p / rx
    cx = cp * cxp - sp * cyp + (x1 + x2) / 2
    cy = sp * cxp + cp * cyp + (y1 + y2) / 2

    def ang(ux, uy, vx, vy):
        a = math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)
        return a

    t1 = ang(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry)
    dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry)
    if not sweep and dt > 0:
        dt -= 2 * math.pi
    elif sweep and dt < 0:
        dt += 2 * math.pi
    n = max(1, int(math.ceil(abs(dt) / (math.pi / 2))))
    step = dt / n
    out = []
    for i in range(n):
        a1, a2 = t1 + i * step, t1 + (i + 1) * step
        k = 4 / 3 * math.tan((a2 - a1) / 4)

        def pt(a, d=0.0):
            ex, ey = math.cos(a) - d * math.sin(a), math.sin(a) + d * math.cos(a)
            return (cx + rx * ex * cp - ry * ey * sp, cy + rx * ex * sp + ry * ey * cp)

        out.append(("C", [pt(a1, k), pt(a2, -k), pt(a2)]))
    return out


def _parse_d(d):
    toks = _TOKEN.findall(d)
    subs, cur, i = [], None, 0
    x = y = sx = sy = 0.0
    last_c = last_q = None
    cmd = None

    def nums(k):
        nonlocal i
        vals = [float(t) for t in toks[i:i + k]]
        i += k
        return vals

    while i < len(toks):
        t = toks[i]
        if re.fullmatch(r"[A-Za-z]", t):
            cmd = t
            i += 1
            if cmd in "Zz":
                if cur is not None:
                    cur.append(("Z", []))
                x, y = sx, sy
                last_c = last_q = None
                continue
        rel = cmd.islower()
        c = cmd.upper()
        if c == "M":
            px, py = nums(2)
            if rel:
                px, py = x + px, y + py
            cur = [("M", [(px, py)])]
            subs.append(cur)
            x, y, sx, sy = px, py, px, py
            cmd = "l" if rel else "L"
            last_c = last_q = None
        elif c == "L":
            px, py = nums(2)
            if rel:
                px, py = x + px, y + py
            cur.append(("L", [(px, py)]))
            x, y = px, py
            last_c = last_q = None
        elif c == "H":
            (px,) = nums(1)
            x = x + px if rel else px
            cur.append(("L", [(x, y)]))
            last_c = last_q = None
        elif c == "V":
            (py,) = nums(1)
            y = y + py if rel else py
            cur.append(("L", [(x, y)]))
            last_c = last_q = None
        elif c == "C":
            a = nums(6)
            if rel:
                a = [a[0] + x, a[1] + y, a[2] + x, a[3] + y, a[4] + x, a[5] + y]
            cur.append(("C", [(a[0], a[1]), (a[2], a[3]), (a[4], a[5])]))
            last_c, last_q = (a[2], a[3]), None
            x, y = a[4], a[5]
        elif c == "S":
            a = nums(4)
            if rel:
                a = [a[0] + x, a[1] + y, a[2] + x, a[3] + y]
            c1 = (2 * x - last_c[0], 2 * y - last_c[1]) if last_c else (x, y)
            cur.append(("C", [c1, (a[0], a[1]), (a[2], a[3])]))
            last_c, last_q = (a[0], a[1]), None
            x, y = a[2], a[3]
        elif c in "QT":
            if c == "Q":
                a = nums(4)
                if rel:
                    a = [a[0] + x, a[1] + y, a[2] + x, a[3] + y]
                q, end = (a[0], a[1]), (a[2], a[3])
            else:
                a = nums(2)
                if rel:
                    a = [a[0] + x, a[1] + y]
                q = (2 * x - last_q[0], 2 * y - last_q[1]) if last_q else (x, y)
                end = (a[0], a[1])
            c1 = (x + 2 / 3 * (q[0] - x), y + 2 / 3 * (q[1] - y))
            c2 = (end[0] + 2 / 3 * (q[0] - end[0]), end[1] + 2 / 3 * (q[1] - end[1]))
            cur.append(("C", [c1, c2, end]))
            last_q, last_c = q, None
            x, y = end
        elif c == "A":
            rx, ry, phi, large, sweep, px, py = nums(7)
            if rel:
                px, py = x + px, y + py
            cur.extend(_arc_to_beziers(x, y, rx, ry, phi, int(large), int(sweep), px, py))
            x, y = px, py
            last_c = last_q = None
        else:
            i += 1
    return subs


def _ellipse(cx, cy, rx, ry):
    k = 0.5522847498
    return [("M", [(cx + rx, cy)]),
            ("C", [(cx + rx, cy + k * ry), (cx + k * rx, cy + ry), (cx, cy + ry)]),
            ("C", [(cx - k * rx, cy + ry), (cx - rx, cy + k * ry), (cx - rx, cy)]),
            ("C", [(cx - rx, cy - k * ry), (cx - k * rx, cy - ry), (cx, cy - ry)]),
            ("C", [(cx + k * rx, cy - ry), (cx + rx, cy - k * ry), (cx + rx, cy)]),
            ("Z", [])]


def _rect(x, y, w, h, rx=0.0, ry=None):
    ry = rx if ry is None else ry
    rx, ry = min(rx, w / 2), min(ry, h / 2)
    if not rx:
        return [("M", [(x, y)]), ("L", [(x + w, y)]), ("L", [(x + w, y + h)]), ("L", [(x, y + h)]), ("Z", [])]
    k = 0.5522847498
    return [("M", [(x + rx, y)]), ("L", [(x + w - rx, y)]),
            ("C", [(x + w - rx + k * rx, y), (x + w, y + ry - k * ry), (x + w, y + ry)]),
            ("L", [(x + w, y + h - ry)]),
            ("C", [(x + w, y + h - ry + k * ry), (x + w - rx + k * rx, y + h), (x + w - rx, y + h)]),
            ("L", [(x + rx, y + h)]),
            ("C", [(x + rx - k * rx, y + h), (x, y + h - ry + k * ry), (x, y + h - ry)]),
            ("L", [(x, y + ry)]),
            ("C", [(x, y + ry - k * ry), (x + rx - k * rx, y), (x + rx, y)]), ("Z", [])]


def geometry(name):
    """The icon as subpaths: lists of (op, points), op in M L C Z."""
    subs = []
    f = lambda a, k, d=0.0: float(a.get(k, d))
    for tag, a in _icons()[require(name)]:
        if tag == "path":
            subs += _parse_d(a.get("d", ""))
        elif tag == "circle":
            subs.append(_ellipse(f(a, "cx"), f(a, "cy"), f(a, "r"), f(a, "r")))
        elif tag == "ellipse":
            subs.append(_ellipse(f(a, "cx"), f(a, "cy"), f(a, "rx"), f(a, "ry")))
        elif tag == "rect":
            subs.append(_rect(f(a, "x"), f(a, "y"), f(a, "width"), f(a, "height"), f(a, "rx"),
                              f(a, "ry", f(a, "rx"))))
        elif tag == "line":
            subs.append([("M", [(f(a, "x1"), f(a, "y1"))]), ("L", [(f(a, "x2"), f(a, "y2"))])])
        elif tag in ("polyline", "polygon"):
            pts = [float(v) for v in _NUM.findall(a.get("points", ""))]
            pairs = list(zip(pts[0::2], pts[1::2]))
            if pairs:
                sub = [("M", [pairs[0]])] + [("L", [p]) for p in pairs[1:]]
                if tag == "polygon":
                    sub.append(("Z", []))
                subs.append(sub)
    return subs


# ---------- PowerPoint: a native shape ----------

def add_to_slide(slide, name, x, y, size, color, stroke=2.0):
    """The icon as one freeform shape, `size` inches square at (x, y); its line
    is the theme colour you give (hex), round-capped, scaled with the icon."""
    from lxml import etree
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Inches, Pt
    from pptx.dml.color import RGBColor
    from pptx.oxml.ns import qn

    shp = slide.shapes.add_shape(MSO_SHAPE.RECTANGLE, Inches(x), Inches(y), Inches(size), Inches(size))
    shp.name = "Icon " + require(name)
    shp.shadow.inherit = False
    shp.fill.background()
    spPr = shp._element.spPr
    prst = spPr.find(qn("a:prstGeom"))
    A = "http://schemas.openxmlformats.org/drawingml/2006/main"
    cust = etree.SubElement(spPr, qn("a:custGeom"))
    for tag in ("avLst", "gdLst", "ahLst", "cxnLst"):
        etree.SubElement(cust, f"{{{A}}}{tag}")
    r = etree.SubElement(cust, qn("a:rect"))
    r.set("l", "l"); r.set("t", "t"); r.set("r", "r"); r.set("b", "b")
    plst = etree.SubElement(cust, qn("a:pathLst"))
    S = 1000  # 24 units → 24000 path units
    for sub in geometry(name):
        p = etree.SubElement(plst, qn("a:path"))
        p.set("w", str(24 * S)); p.set("h", str(24 * S)); p.set("fill", "none")
        for op, pts in sub:
            tag = {"M": "moveTo", "L": "lnTo", "C": "cubicBezTo", "Z": "close"}[op]
            el = etree.SubElement(p, qn(f"a:{tag}"))
            for px, py in pts:
                pt = etree.SubElement(el, qn("a:pt"))
                pt.set("x", str(int(round(px * S)))); pt.set("y", str(int(round(py * S))))
    # custGeom goes where prstGeom was (the schema's order).
    prst.addprevious(cust)
    spPr.remove(prst)
    ln = shp.line
    ln.color.rgb = RGBColor.from_string(str(color).lstrip("#").upper())
    ln.width = Pt(stroke * size * 72 / 24)
    lnel = spPr.find(qn("a:ln"))
    lnel.set("cap", "rnd")
    for old in lnel.findall(qn("a:round")):
        lnel.remove(old)
    etree.SubElement(lnel, qn("a:round"))
    return shp


# ---------- a picture (Word, Excel, previews) ----------

def _flatten(sub, steps=12):
    pts, out, cur = [], [], None
    for op, ps in sub:
        if op == "M":
            if len(pts) > 1:
                out.append((pts, False))
            pts = [ps[0]]
            cur = ps[0]
        elif op == "L":
            pts.append(ps[0]); cur = ps[0]
        elif op == "C":
            (x0, y0), (x1, y1), (x2, y2), (x3, y3) = cur, ps[0], ps[1], ps[2]
            for i in range(1, steps + 1):
                t = i / steps
                mt = 1 - t
                pts.append((mt ** 3 * x0 + 3 * mt * mt * t * x1 + 3 * mt * t * t * x2 + t ** 3 * x3,
                            mt ** 3 * y0 + 3 * mt * mt * t * y1 + 3 * mt * t * t * y2 + t ** 3 * y3))
            cur = ps[2]
        elif op == "Z":
            if pts:
                pts.append(pts[0])
                out.append((pts, True))
                cur = pts[0]
                pts = [cur]
    if len(pts) > 1:
        out.append((pts, False))
    return out


def png(name, path, px=256, color="1F2A3A", stroke=2.0, background=None):
    """The icon drawn as a PNG, `px` square, transparent unless `background`."""
    from PIL import Image, ImageDraw
    big = px * 4  # drawn large, then scaled down: smooth edges
    k = big / 24
    rgb = tuple(int(str(color).lstrip("#")[i:i + 2], 16) for i in (0, 2, 4))
    bg = (0, 0, 0, 0) if background is None else tuple(int(background[i:i + 2], 16) for i in (0, 2, 4)) + (255,)
    im = Image.new("RGBA", (big, big), bg)
    d = ImageDraw.Draw(im)
    w = max(1, int(round(stroke * k)))
    for sub in geometry(name):
        for pts, _closed in _flatten(sub):
            xy = [(x * k, y * k) for x, y in pts]
            d.line(xy, fill=rgb + (255,), width=w, joint="curve")
            for x, y in (xy[0], xy[-1]):
                d.ellipse((x - w / 2, y - w / 2, x + w / 2, y + w / 2), fill=rgb + (255,))
    im.resize((px, px), Image.LANCZOS).save(path)
    return path


if __name__ == "__main__":
    import sys
    if len(sys.argv) >= 3 and sys.argv[1] == "search":
        w = sys.argv[2].lower()
        hits = [n for n in names() if w in n]
        alias = find(w)
        print("\n".join(([f"{alias}  (best for {w!r})"] if alias else []) + hits[:60]))
    else:
        print(__doc__)
