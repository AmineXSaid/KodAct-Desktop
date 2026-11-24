"""Designed Word documents for `docx_tool.py init` and `add`: the deck themes
(`deck_design.THEMES`, picked from who the document is for), applied to
Word's own styles, and blocks built from real Word parts (styles, tables,
shading, borders, fields) so the person can keep editing them in Word:
cover, heading, text, bullets, callout, table, kpis, cards, steps, timeline,
compare, quote, chart, image, toc, pagebreak.
"""

import os
import re
import tempfile

from docx.enum.section import WD_SECTION
from docx.enum.table import WD_TABLE_ALIGNMENT, WD_CELL_VERTICAL_ALIGNMENT
from docx.enum.text import WD_ALIGN_PARAGRAPH, WD_BREAK
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.shared import Inches, Pt, RGBColor

import chart_image
import deck_design

BLOCKS = ("heading", "text", "bullets", "callout", "table", "kpis", "cards", "steps", "timeline", "compare",
          "quote", "chart", "image", "toc", "pagebreak")
CALLOUTS = {"note": 0, "tip": 4, "info": 1, "warning": 2, "danger": 5, "success": 4}


def rgb(h):
    h = str(h).lstrip("#")
    return RGBColor(int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))


def mix(h, k, toward="FFFFFF"):
    a, b = str(h).lstrip("#"), str(toward).lstrip("#")
    out = ""
    for i in (0, 2, 4):
        x, y = int(a[i:i + 2], 16), int(b[i:i + 2], 16)
        out += f"{int(x + (y - x) * k):02X}"
    return out


def paper(t):
    """Word pages are white-ish: a dark theme keeps its accents on light paper."""
    if int(t["bg"][0:2], 16) < 0x60:
        return dict(t, bg="FFFFFF", ink="111827", soft="374151", card="F3F4F6")
    return t


# ---------- low-level Word parts ----------

def _fonts(run_or_style_font, name):
    """Set a font for every script (Word otherwise keeps Calibri for some)."""
    run_or_style_font.name = name
    el = run_or_style_font.element if hasattr(run_or_style_font, "element") else None
    rpr = el.get_or_add_rPr() if el is not None else None
    if rpr is not None:
        fonts = rpr.find(qn("w:rFonts"))
        if fonts is None:
            fonts = OxmlElement("w:rFonts")
            rpr.append(fonts)
        for k in ("w:ascii", "w:hAnsi", "w:cs", "w:eastAsia"):
            fonts.set(qn(k), name)


def _style_fonts(style, name):
    style.font.name = name
    rpr = style.element.get_or_add_rPr()
    fonts = rpr.find(qn("w:rFonts"))
    if fonts is None:
        fonts = OxmlElement("w:rFonts")
        rpr.append(fonts)
    for k in ("w:ascii", "w:hAnsi", "w:cs", "w:eastAsia"):
        fonts.set(qn(k), name)
    for k in ("w:asciiTheme", "w:hAnsiTheme", "w:cstheme", "w:eastAsiaTheme"):
        if fonts.get(qn(k)) is not None:
            del fonts.attrib[qn(k)]


def shade(cell, fill):
    tcpr = cell._tc.get_or_add_tcPr()
    for old in tcpr.findall(qn("w:shd")):
        tcpr.remove(old)
    shd = OxmlElement("w:shd")
    shd.set(qn("w:val"), "clear")
    shd.set(qn("w:color"), "auto")
    shd.set(qn("w:fill"), fill)
    tcpr.append(shd)


def borders(cell, **sides):
    """borders(cell, left=("single", 24, "FF0000"), top=None, ...): None removes."""
    tcpr = cell._tc.get_or_add_tcPr()
    b = tcpr.find(qn("w:tcBorders"))
    if b is None:
        b = OxmlElement("w:tcBorders")
        tcpr.append(b)
    for side in ("top", "left", "bottom", "right"):
        if side not in sides:
            continue
        for old in b.findall(qn(f"w:{side}")):
            b.remove(old)
        el = OxmlElement(f"w:{side}")
        spec = sides[side]
        if spec is None:
            el.set(qn("w:val"), "nil")
        else:
            val, size, color = spec
            el.set(qn("w:val"), val)
            el.set(qn("w:sz"), str(size))
            el.set(qn("w:space"), "0")
            el.set(qn("w:color"), color)
        b.append(el)


def no_borders(table):
    tblpr = table._tbl.tblPr
    for old in tblpr.findall(qn("w:tblBorders")):
        tblpr.remove(old)
    b = OxmlElement("w:tblBorders")
    for side in ("top", "left", "bottom", "right", "insideH", "insideV"):
        el = OxmlElement(f"w:{side}")
        el.set(qn("w:val"), "nil")
        b.append(el)
    tblpr.append(b)


def cell_margins(table, top=80, bottom=80, left=140, right=140):
    tblpr = table._tbl.tblPr
    m = OxmlElement("w:tblCellMar")
    for side, v in (("top", top), ("left", left), ("bottom", bottom), ("right", right)):
        el = OxmlElement(f"w:{side}")
        el.set(qn("w:w"), str(v))
        el.set(qn("w:type"), "dxa")
        m.append(el)
    tblpr.append(m)


def keep_row_together(row):
    trpr = row._tr.get_or_add_trPr()
    el = OxmlElement("w:cantSplit")
    trpr.append(el)


def run(p, words, t, size=None, color=None, bold=None, italic=None, font=None):
    """Add `words` to paragraph p; **bold** spans are bold."""
    parts = re.split(r"(\*\*[^*]+\*\*)", str(words))
    out = None
    for part in parts:
        if not part:
            continue
        strong = part.startswith("**") and part.endswith("**")
        r = p.add_run(part[2:-2] if strong else part)
        if size:
            r.font.size = Pt(size)
        if color:
            r.font.color.rgb = rgb(color)
        if bold or strong:
            r.font.bold = True
        if italic:
            r.font.italic = True
        if font:
            _fonts(r.font, font)
        out = r
    return out


def spacing(p, before=0, after=6, line=None):
    pf = p.paragraph_format
    pf.space_before = Pt(before)
    pf.space_after = Pt(after)
    if line:
        pf.line_spacing = line


def width(doc):
    s = doc.sections[-1]
    return s.page_width - s.left_margin - s.right_margin


def field(p, code):
    """A Word field (PAGE, TOC …), updated when Word opens the file."""
    r = p.add_run()
    for kind, text in (("begin", None), (None, code), ("separate", None), (None, ""), ("end", None)):
        if kind:
            el = OxmlElement("w:fldChar")
            el.set(qn("w:fldCharType"), kind)
            if kind == "begin":
                el.set(qn("w:dirty"), "true")
            r._r.append(el)
        elif text is not None and text != "":
            el = OxmlElement("w:instrText")
            el.set(qn("xml:space"), "preserve")
            el.text = text
            r._r.append(el)
    return r


# ---------- the theme on Word's styles ----------

def apply_theme(doc, t, title=""):
    t = paper(t)
    styles = doc.styles
    normal = styles["Normal"]
    _style_fonts(normal, t["body"])
    normal.font.size = Pt(11)
    normal.font.color.rgb = rgb(t["soft"])
    normal.paragraph_format.space_after = Pt(8)
    normal.paragraph_format.line_spacing = 1.2
    for level, size, color in ((1, 20, t["ink"]), (2, 15, t["accents"][0]), (3, 12.5, t["ink"])):
        st = styles[f"Heading {level}"]
        _style_fonts(st, t["head"])
        st.font.size = Pt(size)
        st.font.bold = True
        st.font.color.rgb = rgb(color)
        st.paragraph_format.space_before = Pt(18 if level == 1 else 12)
        st.paragraph_format.space_after = Pt(6)
        st.paragraph_format.keep_with_next = True
    for name in ("Title", "Subtitle", "List Bullet", "List Number", "Caption"):
        if name in [s.name for s in styles]:
            _style_fonts(styles[name], t["head"] if name in ("Title", "Subtitle") else t["body"])
    sec = doc.sections[0]
    for side in ("left_margin", "right_margin"):
        setattr(sec, side, Inches(1.0))
    sec.top_margin, sec.bottom_margin = Inches(0.9), Inches(0.9)
    # Header: the title, small; footer: the page number, both in the theme.
    hp = sec.header.paragraphs[0]
    hp.text = ""
    run(hp, title, t, size=9, color=t["soft"], font=t["body"])
    hp.alignment = WD_ALIGN_PARAGRAPH.RIGHT
    fp = sec.footer.paragraphs[0]
    fp.text = ""
    fp.alignment = WD_ALIGN_PARAGRAPH.CENTER
    r = field(fp, " PAGE ")
    r.font.size = Pt(9)
    r.font.color.rgb = rgb(t["accents"][0])
    sec.different_first_page_header_footer = True
    return t


# ---------- blocks ----------

def cover(doc, t, title, subtitle=None, tag=None):
    t = paper(t)
    a = t["accents"]
    tbl = doc.add_table(rows=1, cols=1)
    tbl.alignment = WD_TABLE_ALIGNMENT.CENTER
    no_borders(tbl)
    cell_margins(tbl, 500, 500, 420, 420)
    c = tbl.cell(0, 0)
    shade(c, a[0])
    c.vertical_alignment = WD_CELL_VERTICAL_ALIGNMENT.CENTER
    p = c.paragraphs[0]
    if tag:
        run(p, tag.upper(), t, size=10, color=mix(a[0], 0.7), bold=True, font=t["body"])
        p = c.add_paragraph()
    run(p, title, t, size=32, color="FFFFFF", bold=True, font=t["head"])
    spacing(p, 4, 6, 1.05)
    if subtitle:
        q = c.add_paragraph()
        run(q, subtitle, t, size=14, color=mix(a[0], 0.85), font=t["body"])
    # A strip of the theme's colours under the band.
    strip = doc.add_table(rows=1, cols=len(a))
    no_borders(strip)
    for i, col in enumerate(a):
        shade(strip.cell(0, i), col)
        sp = strip.cell(0, i).paragraphs[0]
        spacing(sp, 0, 0)
        sp.add_run().font.size = Pt(3)
    doc.add_paragraph()


def add_block(doc, t, spec):
    t = paper(t)
    kind = spec.get("block") or spec.get("type") or spec.get("layout")
    if kind not in BLOCKS:
        raise SystemExit(f"no block {kind!r}; choose one of {', '.join(BLOCKS)}")
    globals()["_" + kind](doc, t, spec)


def _heading(doc, t, spec):
    doc.add_heading(spec.get("text") or spec.get("title", ""), level=int(spec.get("level", 1)))


def _text(doc, t, spec):
    for para in str(spec.get("text", "")).split("\n\n"):
        p = doc.add_paragraph()
        run(p, para.replace("\n", " "), t)
        if spec.get("lead"):
            for r in p.runs:
                r.font.size = Pt(13)
                r.font.color.rgb = rgb(t["ink"])


def _bullets(doc, t, spec):
    style = "List Number" if spec.get("numbered") else "List Bullet"
    for item in spec.get("items") or []:
        p = doc.add_paragraph(style=style)
        run(p, item, t)
        spacing(p, 0, 3)


def _callout(doc, t, spec):
    kind = spec.get("kind", "note")
    if kind not in CALLOUTS:
        raise SystemExit(f"no callout kind {kind!r}; choose one of {', '.join(CALLOUTS)}")
    col = {"warning": "D97706", "danger": "DC2626", "success": "16A34A"}.get(kind, t["accents"][CALLOUTS[kind] % 6])
    tbl = doc.add_table(rows=1, cols=1)
    no_borders(tbl)
    cell_margins(tbl, 140, 140, 240, 200)
    c = tbl.cell(0, 0)
    shade(c, mix(col, 0.88))
    borders(c, left=("single", 36, col))
    if spec.get("title"):
        p = c.paragraphs[0]
        run(p, spec["title"], t, size=11.5, color=col, bold=True, font=t["head"])
        spacing(p, 0, 2)
        p = c.add_paragraph()
    else:
        p = c.paragraphs[0]
    run(p, spec.get("text", ""), t, color=t["ink"])
    spacing(p, 0, 0)
    keep_row_together(tbl.rows[0])
    doc.add_paragraph()


def _table(doc, t, spec):
    cols, rows = spec.get("columns"), spec.get("rows")
    if not cols or not rows:
        raise SystemExit('a table needs "columns": [...] and "rows": [[...], ...]')
    a = t["accents"][0]
    tbl = doc.add_table(rows=1 + len(rows), cols=len(cols))
    tbl.alignment = WD_TABLE_ALIGNMENT.CENTER
    no_borders(tbl)
    cell_margins(tbl, 70, 70, 120, 120)
    hl = spec.get("highlight")
    for ci, name in enumerate(cols):
        c = tbl.cell(0, ci)
        shade(c, a)
        run(c.paragraphs[0], name, t, size=10.5, color="FFFFFF", bold=True, font=t["head"])
    # The header row repeats on each page the table runs onto.
    trpr = tbl.rows[0]._tr.get_or_add_trPr()
    trpr.append(OxmlElement("w:tblHeader"))
    for ri, row in enumerate(rows, start=1):
        if len(row) != len(cols):
            raise SystemExit(f"row {ri} has {len(row)} cells for {len(cols)} columns")
        fill = mix(a, 0.78) if hl == ri - 1 else ("FFFFFF" if ri % 2 else mix(a, 0.94))
        for ci, v in enumerate(row):
            c = tbl.cell(ri, ci)
            shade(c, fill)
            borders(c, bottom=("single", 4, mix(a, 0.8)))
            num = isinstance(v, (int, float))
            p = c.paragraphs[0]
            run(p, f"{v:,}" if num else v, t, size=10.5, color=t["ink"], bold=hl == ri - 1)
            spacing(p, 0, 0)
            if num:
                p.alignment = WD_ALIGN_PARAGRAPH.RIGHT
        keep_row_together(tbl.rows[ri])
    _caption(doc, t, spec)


def _caption(doc, t, spec):
    if spec.get("caption"):
        p = doc.add_paragraph()
        run(p, spec["caption"], t, size=9, color=t["soft"], italic=True)
        p.alignment = WD_ALIGN_PARAGRAPH.CENTER
    else:
        doc.add_paragraph()


def _kpis(doc, t, spec):
    items = (spec.get("items") or [])[:4]
    if not items:
        raise SystemExit('kpis needs "items": [{"value", "label", "delta"}]')
    tbl = doc.add_table(rows=1, cols=len(items))
    no_borders(tbl)
    cell_margins(tbl, 160, 160, 120, 120)
    for i, item in enumerate(items):
        col = t["accents"][i % 6]
        c = tbl.cell(0, i)
        shade(c, mix(col, 0.9))
        borders(c, top=("single", 30, col), left=("single", 24, "FFFFFF"), right=("single", 24, "FFFFFF"))
        p = c.paragraphs[0]
        p.alignment = WD_ALIGN_PARAGRAPH.CENTER
        run(p, str(item.get("value", "")), t, size=26, color=col, bold=True, font=t["head"])
        spacing(p, 0, 0)
        q = c.add_paragraph()
        q.alignment = WD_ALIGN_PARAGRAPH.CENTER
        run(q, str(item.get("label", "")), t, size=10, color=t["soft"])
        spacing(q, 0, 2)
        if item.get("delta"):
            d = str(item["delta"])
            up = not d.strip().startswith("-")
            good = up if item.get("better", "up") == "up" else not up
            r_ = c.add_paragraph()
            r_.alignment = WD_ALIGN_PARAGRAPH.CENTER
            run(r_, ("▲ " if up else "▼ ") + d.lstrip("+-"), t, size=10, bold=True,
                color="16A34A" if good else "DC2626")
            spacing(r_, 0, 0)
    keep_row_together(tbl.rows[0])
    _caption(doc, t, spec)


def _cards(doc, t, spec):
    items = spec.get("items") or []
    if not items:
        raise SystemExit('cards needs "items": [{"head", "text"}] or ["Head: text"]')
    n_cols = int(spec.get("columns", 3 if len(items) % 3 == 0 else 2))
    n_rows = (len(items) + n_cols - 1) // n_cols
    tbl = doc.add_table(rows=n_rows, cols=n_cols)
    no_borders(tbl)
    cell_margins(tbl, 160, 160, 160, 160)
    for i, item in enumerate(items):
        if isinstance(item, str):
            head, _, body = item.partition(":")
            item = {"head": head, "text": body}
        col = t["accents"][i % 6]
        c = tbl.cell(i // n_cols, i % n_cols)
        shade(c, t["card"] if t["card"] != "FFFFFF" else mix(col, 0.93))
        borders(c, top=("single", 24, col), left=("single", 36, "FFFFFF"), right=("single", 36, "FFFFFF"),
                bottom=("single", 36, "FFFFFF"))
        p = c.paragraphs[0]
        run(p, item.get("head", "").strip(), t, size=12, color=t["ink"], bold=True, font=t["head"])
        spacing(p, 0, 3)
        q = c.add_paragraph()
        run(q, item.get("text", "").strip(), t, size=10)
        spacing(q, 0, 0)
    for r in tbl.rows:
        keep_row_together(r)
    _caption(doc, t, spec)


def _steps(doc, t, spec):
    items = spec.get("items") or []
    if not items:
        raise SystemExit('steps needs "items": ["Head: what happens", ...]')
    tbl = doc.add_table(rows=len(items), cols=2)
    no_borders(tbl)
    cell_margins(tbl, 100, 100, 120, 120)
    for i, item in enumerate(items):
        col = t["accents"][i % 6]
        head, _, body = str(item).partition(":")
        n = tbl.cell(i, 0)
        n.width = Inches(0.55)
        shade(n, col)
        n.vertical_alignment = WD_CELL_VERTICAL_ALIGNMENT.CENTER
        p = n.paragraphs[0]
        p.alignment = WD_ALIGN_PARAGRAPH.CENTER
        run(p, str(i + 1), t, size=16, color="FFFFFF", bold=True, font=t["head"])
        spacing(p, 0, 0)
        c = tbl.cell(i, 1)
        c.width = Inches(5.9)
        shade(c, mix(col, 0.92))
        p = c.paragraphs[0]
        run(p, head.strip(), t, size=11.5, color=t["ink"], bold=True, font=t["head"])
        spacing(p, 0, 1)
        if body.strip():
            q = c.add_paragraph()
            run(q, body.strip(), t, size=10.5)
            spacing(q, 0, 0)
        borders(n, bottom=("single", 24, "FFFFFF"))
        borders(c, bottom=("single", 24, "FFFFFF"))
        keep_row_together(tbl.rows[i])
    _caption(doc, t, spec)


def _timeline(doc, t, spec):
    items = spec.get("items") or []
    if not items:
        raise SystemExit('timeline needs "items": ["When: what", ...]')
    tbl = doc.add_table(rows=len(items), cols=2)
    no_borders(tbl)
    cell_margins(tbl, 90, 90, 160, 160)
    for i, item in enumerate(items):
        col = t["accents"][i % 6]
        when, _, what = str(item).partition(":")
        a, b = tbl.cell(i, 0), tbl.cell(i, 1)
        a.width, b.width = Inches(1.4), Inches(5.0)
        p = a.paragraphs[0]
        p.alignment = WD_ALIGN_PARAGRAPH.RIGHT
        run(p, when.strip(), t, size=11.5, color=col, bold=True, font=t["head"])
        borders(b, left=("single", 24, col))
        q = b.paragraphs[0]
        run(q, "● ", t, size=11, color=col)
        run(q, what.strip(), t, color=t["ink"])
        keep_row_together(tbl.rows[i])
    _caption(doc, t, spec)


def _compare(doc, t, spec):
    left, right = spec.get("left"), spec.get("right")
    if not left or not right:
        raise SystemExit('compare needs "left": {"title", "items"} and "right": {"title", "items"}')
    tbl = doc.add_table(rows=2, cols=2)
    no_borders(tbl)
    cell_margins(tbl, 120, 120, 160, 160)
    for k, side in enumerate((left, right)):
        col = t["accents"][k]
        h = tbl.cell(0, k)
        shade(h, col)
        run(h.paragraphs[0], side.get("title", ""), t, size=13, color="FFFFFF", bold=True, font=t["head"])
        b = tbl.cell(1, k)
        shade(b, mix(col, 0.92))
        first = True
        for item in side.get("items") or []:
            p = b.paragraphs[0] if first else b.add_paragraph()
            first = False
            run(p, ("✓  " if k == 0 else "•  "), t, color=col, bold=True)
            run(p, item, t, color=t["ink"])
            spacing(p, 0, 4)
        for c in (h, b):
            borders(c, left=("single", 24, "FFFFFF"), right=("single", 24, "FFFFFF"))
    if spec.get("verdict"):
        _callout(doc, t, {"kind": "success", "title": spec.get("verdict_title", "Our choice"),
                          "text": spec["verdict"]})
    else:
        doc.add_paragraph()


def _quote(doc, t, spec):
    tbl = doc.add_table(rows=1, cols=1)
    no_borders(tbl)
    cell_margins(tbl, 160, 160, 360, 200)
    c = tbl.cell(0, 0)
    borders(c, left=("single", 30, t["accents"][0]))
    p = c.paragraphs[0]
    run(p, "“" + str(spec.get("quote", "")) + "”", t, size=15, color=t["ink"], italic=True,
        font=t["head"])
    if spec.get("by"):
        q = c.add_paragraph()
        run(q, "— " + spec["by"], t, size=10, color=t["soft"])
    doc.add_paragraph()


def _chart(doc, t, spec):
    c = spec.get("chart") or spec
    with tempfile.TemporaryDirectory() as tmp:
        png = os.path.join(tmp, "chart.png")
        theme = dict(t, card_bg="FFFFFF")
        chart_image.draw(c, theme, png, 6.3, float(spec.get("height", 3.3)))
        doc.add_picture(png, width=Inches(6.3))
    doc.paragraphs[-1].alignment = WD_ALIGN_PARAGRAPH.CENTER
    _caption(doc, t, spec)


def _image(doc, t, spec):
    path = spec.get("path")
    if not path or not os.path.isfile(path):
        raise SystemExit(f'image needs "path" to a picture file; {path!r} was not found')
    doc.add_picture(path, width=Inches(float(spec.get("width", 6.3))))
    doc.paragraphs[-1].alignment = WD_ALIGN_PARAGRAPH.CENTER
    _caption(doc, t, spec)


def _toc(doc, t, spec):
    doc.add_heading(spec.get("title", "Contents"), level=1)
    p = doc.add_paragraph()
    field(p, ' TOC \\o "1-3" \\h \\z \\u ')
    run(doc.add_paragraph(), "(Word fills this in when the document is opened; press F9 to refresh.)", t,
        size=9, color=t["soft"], italic=True)


def _pagebreak(doc, t, spec):
    doc.add_paragraph().add_run().add_break(WD_BREAK.PAGE)
