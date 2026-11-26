"""Word documents as code, on the document's theme: what the blocks don't do (footnotes, sections
with their own orientation, columns, headers and footers, tables with set widths and merged cells,
mixed formatting in a sentence, links), next to the blocks themselves, in one script.

    import sys; sys.path.insert(0, r"<this folder>")
    from doc_kit import Doc

    with Doc("report.docx", audience="hospital board", title="Q3 review") as d:  # opens or makes it
        d.heading("Results", 1)
        d.para("Visits grew ").add("75%", bold=True).add(" after the east clinic opened.") \\
            .footnote("Count of booked visits, July to September; source: scheduling system.")
        d.block({"block": "kpis", "items": [{"value": "210k", "label": "Visits", "delta": "+40%"}]})
        d.table(["Site", "Visits", "Wait"], [["Central", 120, "35 min"], ["East", 60, "24 min"]],
                widths=[3, 1.5, 1.5], total="Visits")
        d.section(orientation="landscape", header="Appendix: site data")
        d.para("A ").link("dashboard", "https://example.org/q3").add(" has the live numbers.")
    # saved when the `with` ends; then check it with validate.py and look at the preview

Doc(path, theme=None, audience=None, title=None, subtitle=None, page="letter"|"a4")
  d.heading(text, level=1)            Word's Heading styles: they build the contents and navigation
  d.para(text="", style=None, align="left"|"center"|"right"|"justify", size=None, color=None)
      -> Para: .add(text, bold=, italic=, underline=, color=, size=, code=), .link(text, url),
         .footnote(text): a real Word footnote, numbered by Word, at the page's foot
  d.bullets(items, numbered=False)
  d.block(spec)                       any docx_tool.py block (kpis, callout, table, chart, cards, …)
  d.table(columns, rows, widths=None, merge=None, align=None, total=None, header_repeat=True,
          style="theme"|"grid"|"plain")
      widths in inches; merge: [(row, col, to_row, to_col), …] counted with the header as row 0;
      align: {"Column": "right"}; total: a column (or list) summed into a last row (numbers only)
  d.image(path, width=None, caption=None)       width in inches (default: the text width)
  d.section(orientation="portrait"|"landscape", columns=1, header=None, footer=None,
            restart_numbering=False)  starts a new page with its own layout and running text
  d.page_break(), d.toc()
Colours: ink soft accent accent0..accent5 or a hex. Sizes in points.
"""

from __future__ import annotations

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "scripts"))

import docx  # noqa: E402
from docx.enum.section import WD_ORIENT, WD_SECTION  # noqa: E402
from docx.enum.text import WD_ALIGN_PARAGRAPH  # noqa: E402
from docx.opc.constants import RELATIONSHIP_TYPE as RT  # noqa: E402
from docx.opc.packuri import PackURI  # noqa: E402
from docx.opc.part import Part  # noqa: E402
from docx.oxml import OxmlElement, parse_xml  # noqa: E402
from docx.oxml.ns import qn  # noqa: E402
from docx.shared import Inches, Pt, RGBColor  # noqa: E402

import deck_design  # noqa: E402
import doc_design as dd  # noqa: E402
import docx_tool  # noqa: E402

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
FOOTNOTES_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"
ALIGN = {"left": WD_ALIGN_PARAGRAPH.LEFT, "center": WD_ALIGN_PARAGRAPH.CENTER,
         "right": WD_ALIGN_PARAGRAPH.RIGHT, "justify": WD_ALIGN_PARAGRAPH.JUSTIFY}
EMPTY_FOOTNOTES = (
    f'<w:footnotes xmlns:w="{W_NS}">'
    '<w:footnote w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:after="0" w:line="240" '
    'w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p></w:footnote>'
    '<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:pPr><w:spacing w:after="0" '
    'w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>'
    '</w:footnotes>'
)


class Para:
    """A paragraph being written: chain .add(), .link() and .footnote()."""

    def __init__(self, doc: "Doc", p):
        self.doc, self.p = doc, p

    def add(self, text, bold=None, italic=None, underline=None, color=None, size=None, code=False):
        r = self.p.add_run(str(text))
        r.bold, r.italic, r.underline = bold, italic, underline
        if color is not None:
            r.font.color.rgb = RGBColor.from_string(self.doc.color(color))
        if size is not None:
            r.font.size = Pt(size)
        if code:
            dd._fonts(r.font, "Consolas")
            r.font.size = Pt(size or 10)
        return self

    def link(self, text, url):
        """A clickable link, in the theme's accent and underlined."""
        rid = self.p.part.relate_to(url, RT.HYPERLINK, is_external=True)
        h = OxmlElement("w:hyperlink")
        h.set(qn("r:id"), rid)
        r = OxmlElement("w:r")
        rpr = OxmlElement("w:rPr")
        col = OxmlElement("w:color")
        col.set(qn("w:val"), self.doc.color("accent"))
        u = OxmlElement("w:u")
        u.set(qn("w:val"), "single")
        rpr.append(col)
        rpr.append(u)
        r.append(rpr)
        t = OxmlElement("w:t")
        t.text = str(text)
        t.set(qn("xml:space"), "preserve")
        r.append(t)
        h.append(r)
        self.p._p.append(h)
        return self

    def footnote(self, text):
        """A Word footnote: its number here, its text at the foot of the page."""
        n = self.doc._new_footnote(str(text))
        r = self.p.add_run()
        rpr = r._r.get_or_add_rPr()
        va = OxmlElement("w:vertAlign")
        va.set(qn("w:val"), "superscript")
        rpr.append(va)
        ref = OxmlElement("w:footnoteReference")
        ref.set(qn("w:id"), str(n))
        r._r.append(ref)
        return self


class Doc:
    """A Word document to write into: opens `path`, or makes it (a themed cover first) when it
    isn't there. Saves on `save()`, or when a `with` block ends without an error."""

    def __init__(self, path, theme=None, audience=None, title=None, subtitle=None, page="letter"):
        self.path = str(path)
        if not os.path.exists(self.path):
            name = theme or (deck_design.for_audience(audience) if audience else "clean")
            docx_tool.init(self.path, name, title=title or "Untitled", subtitle=subtitle, page=page)
        self.d = docx.Document(self.path)
        t = docx_tool._doc_theme(self.d)
        self.t = dd.paper(t if t is not None else deck_design.theme(theme or "clean"))
        self._footnotes = None

    # ---------- tokens ----------

    def color(self, c) -> str:
        t = self.t
        if isinstance(c, int):
            return t["accents"][c % len(t["accents"])]
        named = {"ink": t["ink"], "soft": t["soft"], "accent": t["accents"][0], "white": "FFFFFF"}
        if c in named:
            return named[c]
        if isinstance(c, str) and c.startswith("accent") and c[6:].isdigit():
            return t["accents"][int(c[6:]) % len(t["accents"])]
        hx = str(c).lstrip("#")
        if len(hx) == 6 and all(ch in "0123456789abcdefABCDEF" for ch in hx):
            return hx.upper()
        raise ValueError(f"no colour {c!r}: use ink soft accent accent0-5 white or a hex")

    def room(self) -> float:
        """The width text runs in here, in inches: the page's text width, or one column's when the
        current section has columns."""
        sec = self.d.sections[-1]
        width = (sec.page_width - sec.left_margin - sec.right_margin) / 914400
        cols = sec._sectPr.find(qn("w:cols"))
        n = int(cols.get(qn("w:num"), "1")) if cols is not None else 1
        space = int(cols.get(qn("w:space"), "720")) / 1440 if cols is not None else 0.5
        return (width - space * (n - 1)) / max(1, n)

    # ---------- writing ----------

    def heading(self, text, level=1):
        return self.d.add_heading(str(text), level=int(level))

    def para(self, text="", style=None, align="left", size=None, color=None) -> Para:
        p = self.d.add_paragraph(style=style)
        p.alignment = ALIGN[align]
        para = Para(self, p)
        if text:
            para.add(text, size=size, color=color)
        return para

    def bullets(self, items, numbered=False):
        dd.add_block(self.d, self.t, {"block": "bullets", "items": list(items), "numbered": numbered})

    def block(self, spec):
        dd.add_block(self.d, self.t, spec)

    def table(self, columns, rows, widths=None, merge=None, align=None, total=None, header_repeat=True,
              style="theme"):
        cols = [str(c) for c in columns]
        rows = [list(r) for r in rows]
        for i, r in enumerate(rows, start=1):
            if len(r) != len(cols):
                raise SystemExit(f"row {i} has {len(r)} cells for {len(cols)} columns")
        if total:
            names = [total] if isinstance(total, str) else list(total)
            last = [""] * len(cols)
            last[0] = "Total"
            for name in names:
                if name not in cols:
                    raise SystemExit(f"no column {name!r} to total; the columns are {', '.join(cols)}")
                j = cols.index(name)
                vals = [r[j] for r in rows]
                if not all(isinstance(v, (int, float)) for v in vals):
                    raise SystemExit(f"column {name!r} holds text: only numbers can be totalled")
                last[j] = sum(vals)
            rows.append(last)
        tbl = self.d.add_table(rows=1 + len(rows), cols=len(cols))
        a = self.t["accents"][0]
        numeric = [all(isinstance(r[j], (int, float)) or r[j] in ("", None) for r in rows) for j in range(len(cols))]
        align = align or {}
        for ri in range(len(rows) + 1):
            for ci in range(len(cols)):
                c = tbl.cell(ri, ci)
                v = cols[ci] if ri == 0 else rows[ri - 1][ci]
                head = ri == 0
                last = total and ri == len(rows)
                text = f"{v:,}" if isinstance(v, (int, float)) and not isinstance(v, bool) else ("" if v is None else str(v))
                p = c.paragraphs[0]
                dd.run(p, text, self.t, size=10.5, bold=head or bool(last),
                       color=("FFFFFF" if head and style == "theme" else self.t["ink"]))
                dd.spacing(p, 0, 0)
                side = align.get(cols[ci]) or ("right" if numeric[ci] and not head else None)
                if head and numeric[ci]:
                    side = align.get(cols[ci], "right")
                if side:
                    p.alignment = ALIGN[side]
                if style == "theme":
                    if head:
                        dd.shade(c, a)
                    else:
                        dd.shade(c, dd.mix(a, 0.86) if last else ("FFFFFF" if ri % 2 else dd.mix(a, 0.94)))
                        dd.borders(c, bottom=("single", 4, dd.mix(a, 0.8)))
        if style == "theme":
            dd.no_borders(tbl)
        elif style == "grid":
            tbl.style = "Table Grid"
        dd.cell_margins(tbl, 70, 70, 120, 120)
        if header_repeat:
            dd.put(tbl.rows[0]._tr.get_or_add_trPr(), OxmlElement("w:tblHeader"))
        room = self.room()
        if widths:
            if len(widths) != len(cols):
                raise SystemExit(f"widths has {len(widths)} values for {len(cols)} columns")
            if sum(widths) > room + 0.01:
                raise SystemExit(f"the widths add up to {sum(widths):.2f}in; the text here is {room:.2f}in wide")
        else:
            # Equal columns across the text width here: python-docx sizes a table to the page, which
            # overflows a column of a multi-column section.
            widths = [room / len(cols)] * len(cols)
        dd.col_widths(tbl, widths)
        for m in merge or []:
            r0, c0, r1, c1 = m
            tbl.cell(r0, c0).merge(tbl.cell(r1, c1))
        self.d.add_paragraph()
        return tbl

    def image(self, path, width=None, caption=None):
        if not os.path.isfile(path):
            raise FileNotFoundError(f"no image at {path}")
        room = Inches(self.room())
        w = Inches(width) if width else room
        self.d.add_picture(path, width=min(w, room))
        self.d.paragraphs[-1].alignment = WD_ALIGN_PARAGRAPH.CENTER
        if caption:
            p = self.d.add_paragraph()
            dd.run(p, caption, self.t, size=9, color=self.t["soft"], italic=True)
            p.alignment = WD_ALIGN_PARAGRAPH.CENTER

    def section(self, orientation="portrait", columns=1, header=None, footer=None, restart_numbering=False):
        """A new page with its own layout. Header and footer text replace the previous section's;
        left out, they continue from it."""
        prev = self.d.sections[-1]
        s = self.d.add_section(WD_SECTION.NEW_PAGE)
        w, h = prev.page_width, prev.page_height
        landscape = orientation == "landscape"
        s.orientation = WD_ORIENT.LANDSCAPE if landscape else WD_ORIENT.PORTRAIT
        s.page_width, s.page_height = (max(w, h), min(w, h)) if landscape else (min(w, h), max(w, h))
        s.different_first_page_header_footer = False
        cols = s._sectPr.find(qn("w:cols"))
        if cols is None:
            cols = OxmlElement("w:cols")
            s._sectPr.append(cols)
        cols.set(qn("w:num"), str(int(columns)))
        cols.set(qn("w:space"), "720")
        if restart_numbering:
            pg = s._sectPr.find(qn("w:pgNumType"))
            if pg is None:
                pg = OxmlElement("w:pgNumType")
                s._sectPr.append(pg)
            pg.set(qn("w:start"), "1")
        for which, words, align in ((s.header, header, WD_ALIGN_PARAGRAPH.RIGHT),
                                    (s.footer, footer, WD_ALIGN_PARAGRAPH.CENTER)):
            if words is None:
                continue
            which.is_linked_to_previous = False
            p = which.paragraphs[0]
            for r in list(p.runs):
                r._r.getparent().remove(r._r)
            p.alignment = align
            dd.run(p, words, self.t, size=9, color=self.t["soft"])
            if which is s.footer:
                p.add_run("  ·  ")
                num = dd.field(p, " PAGE ")
                num.font.size = Pt(9)
        return s

    def page_break(self):
        dd.add_block(self.d, self.t, {"block": "pagebreak"})

    def toc(self):
        dd.add_block(self.d, self.t, {"block": "toc"})

    # ---------- footnotes ----------

    def _footnotes_root(self):
        """The footnotes part's XML, made (with Word's two separators) when the file has none."""
        if self._footnotes is not None:
            return self._footnotes
        doc_part = self.d.part
        part = next((r.target_part for r in doc_part.rels.values() if r.reltype == RT.FOOTNOTES), None)
        if part is None:
            part = Part(PackURI("/word/footnotes.xml"), FOOTNOTES_CT, EMPTY_FOOTNOTES.encode(), doc_part.package)
            doc_part.relate_to(part, RT.FOOTNOTES)
        root = parse_xml(part.blob) if not hasattr(part, "_element") else part._element
        self._footnotes = (part, root)
        return self._footnotes

    def _new_footnote(self, text) -> int:
        part, root = self._footnotes_root()
        ids = [int(f.get(qn("w:id"))) for f in root.findall(qn("w:footnote"))]
        n = max([0, *ids]) + 1
        fn = OxmlElement("w:footnote")
        fn.set(qn("w:id"), str(n))
        p = OxmlElement("w:p")
        ppr = OxmlElement("w:pPr")
        sp = OxmlElement("w:spacing")
        sp.set(qn("w:after"), "0")
        ppr.append(sp)
        p.append(ppr)
        mark = OxmlElement("w:r")
        mrpr = OxmlElement("w:rPr")
        va = OxmlElement("w:vertAlign")
        va.set(qn("w:val"), "superscript")
        mrpr.append(va)
        mark.append(mrpr)
        mark.append(OxmlElement("w:footnoteRef"))
        p.append(mark)
        r = OxmlElement("w:r")
        rpr = OxmlElement("w:rPr")
        sz = OxmlElement("w:sz")
        sz.set(qn("w:val"), "18")
        rpr.append(sz)
        r.append(rpr)
        t = OxmlElement("w:t")
        t.set(qn("xml:space"), "preserve")
        t.text = " " + text
        r.append(t)
        p.append(r)
        fn.append(p)
        root.append(fn)
        return n

    # ---------- saving ----------

    def save(self):
        if self._footnotes is not None:
            part, root = self._footnotes
            if not hasattr(part, "_element"):
                from lxml import etree

                part._blob = etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone=True)
        docx_tool._finish(self.d)
        docx_tool._save(self.d, self.path)
        print(f"saved {self.path}; now check it: python validate.py {os.path.basename(self.path)}")

    def __enter__(self):
        return self

    def __exit__(self, kind, value, tb):
        if kind is None:
            self.save()
        return False

