"""PDFs: read, create from Markdown, merge, extract pages, fill forms, convert.

    python pdf_tool.py read FILE
    python pdf_tool.py create OUT.pdf --markdown FILE.md [--title TEXT]
    python pdf_tool.py merge OUT.pdf A.pdf B.pdf [...]
    python pdf_tool.py pages FILE OUT.pdf --pages 1-3,5
    python pdf_tool.py fields FILE
    python pdf_tool.py fill FILE OUT.pdf --field "Name=Ada" [--field ...]
    python pdf_tool.py convert FILE.docx|.xlsx|.pptx OUT.pdf

`convert` exports an Office file through LibreOffice (headless).
Every PDF written has a Title and an Author (KodAct, unless the source says).
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile

from pypdf import PdfReader, PdfWriter
from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.platypus import ListFlowable, ListItem, Paragraph, Preformatted, SimpleDocTemplate, Spacer, Table, TableStyle

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # mdlite, beside this file
from mdlite import blocks, inline, plain

AUTHOR = "KodAct"


def read(path):
    """The text of every page, each under its page number."""
    r = PdfReader(path)
    out = []
    for n, page in enumerate(r.pages, start=1):
        out.append(f"## Page {n}\n")
        out.append((page.extract_text() or "").strip())
        out.append("")
    return "\n".join(out)


def _markup(text):
    from xml.sax.saxutils import escape
    parts = []
    for t, bold, italic, code in inline(text):
        t = escape(t)
        if code:
            t = f'<font face="Courier">{t}</font>'
        if bold:
            t = f"<b>{t}</b>"
        if italic:
            t = f"<i>{t}</i>"
        parts.append(t)
    return "".join(parts)


def create(out, markdown, title=None):
    ss = getSampleStyleSheet()
    body = ParagraphStyle("body", parent=ss["BodyText"], fontSize=10.5, leading=15)
    heads = {1: ss["Title"], 2: ss["Heading2"], 3: ss["Heading3"]}
    story, bullets, numbers = [], [], []

    def flush_lists():
        for items, kind in ((bullets, "bullet"), (numbers, "1")):
            if items:
                story.append(ListFlowable([ListItem(Paragraph(_markup(t), body)) for t in items], bulletType=kind, leftIndent=14))
                items.clear()

    for b in blocks(markdown):
        if b[0] not in ("bullet", "number"):
            flush_lists()
        if b[0] == "heading":
            story.append(Paragraph(_markup(b[2]), heads[b[1]]))
        elif b[0] == "para":
            story.append(Paragraph(_markup(b[1]), body))
            story.append(Spacer(1, 4))
        elif b[0] == "bullet":
            bullets.append(b[1])
        elif b[0] == "number":
            numbers.append(b[1])
        elif b[0] == "code":
            story.append(Preformatted(b[1], ParagraphStyle("code", fontName="Courier", fontSize=9, leading=12)))
        elif b[0] == "table":
            data = [[Paragraph(f"<b>{_markup(h)}</b>", body) for h in b[1]]] + [[Paragraph(_markup(c), body) for c in r] for r in b[2]]
            t = Table(data, hAlign="LEFT")
            t.setStyle(TableStyle([("GRID", (0, 0), (-1, -1), 0.5, colors.HexColor("#c9c4bd")), ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#f1efeb")), ("VALIGN", (0, 0), (-1, -1), "TOP")]))
            story.append(t)
            story.append(Spacer(1, 6))
    flush_lists()
    if not title:
        title = next((plain(b[2]) for b in blocks(markdown) if b[0] == "heading"), "")
    doc = SimpleDocTemplate(out, pagesize=A4, leftMargin=20 * mm, rightMargin=20 * mm, topMargin=18 * mm, bottomMargin=18 * mm,
                            title=title, author=AUTHOR, creator=AUTHOR)
    doc.build(story)


def merge(out, inputs):
    w = PdfWriter()
    for path in inputs:
        w.append(path)
    with open(out, "wb") as f:
        w.write(f)
    return len(w.pages)


def _ranges(spec, count):
    pages = []
    for part in spec.split(","):
        a, _, b = part.strip().partition("-")
        start, end = int(a), int(b or a)
        if not (1 <= start <= end <= count):
            raise SystemExit(f"pages {part!r} are outside 1-{count}")
        pages.extend(range(start, end + 1))
    return pages


def pages(path, out, spec):
    r = PdfReader(path)
    w = PdfWriter()
    for n in _ranges(spec, len(r.pages)):
        w.add_page(r.pages[n - 1])
    with open(out, "wb") as f:
        w.write(f)
    return len(w.pages)


def fields(path):
    return {k: (v.get("/V") if isinstance(v, dict) else v) for k, v in (PdfReader(path).get_fields() or {}).items()}


def fill(path, out, values):
    r = PdfReader(path)
    w = PdfWriter()
    w.append(r)
    for page in w.pages:
        w.update_page_form_field_values(page, values, auto_regenerate=False)
    w.set_need_appearances_writer(True)
    with open(out, "wb") as f:
        w.write(f)


def convert(path, out):
    soffice = shutil.which("soffice") or shutil.which("libreoffice")
    if not soffice:
        raise SystemExit("convert needs LibreOffice (soffice) on PATH")
    with tempfile.TemporaryDirectory() as tmp:
        profile = "file://" + os.path.join(tmp, "profile").replace(os.sep, "/")
        subprocess.run([soffice, f"-env:UserInstallation={profile}", "--headless", "--convert-to", "pdf", "--outdir", tmp, path], check=True, capture_output=True, timeout=180)
        made = os.path.join(tmp, os.path.splitext(os.path.basename(path))[0] + ".pdf")
        _ensure_metadata(made, out, os.path.splitext(os.path.basename(path))[0])


def _ensure_metadata(path, out, fallback_title):
    """LibreOffice copies Title and Author from the source's properties; fill
    in what it left empty, so the PDF never shows "Untitled"."""
    r = PdfReader(path)
    meta = r.metadata or {}
    missing = {}
    if not (meta.get("/Title") or "").strip():
        missing["/Title"] = fallback_title
    if not (meta.get("/Author") or "").strip():
        missing["/Author"] = AUTHOR
    if not missing:
        shutil.copyfile(path, out)
        return
    w = PdfWriter(clone_from=r)
    w.add_metadata({**{k: v for k, v in meta.items()}, **missing})
    with open(out, "wb") as f:
        w.write(f)


def _text(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("read"); r.add_argument("file")
    c = sub.add_parser("create"); c.add_argument("out"); c.add_argument("--markdown", required=True); c.add_argument("--title")
    m = sub.add_parser("merge"); m.add_argument("out"); m.add_argument("inputs", nargs="+")
    p = sub.add_parser("pages"); p.add_argument("file"); p.add_argument("out"); p.add_argument("--pages", required=True)
    f = sub.add_parser("fields"); f.add_argument("file")
    fl = sub.add_parser("fill"); fl.add_argument("file"); fl.add_argument("out"); fl.add_argument("--field", action="append", required=True)
    cv = sub.add_parser("convert"); cv.add_argument("file"); cv.add_argument("out")
    args = ap.parse_args(argv)
    if args.cmd == "read":
        sys.stdout.write(read(args.file))
    elif args.cmd == "create":
        create(args.out, _text(args.markdown), args.title)
        print(f"wrote {args.out}")
    elif args.cmd == "merge":
        print(f"merged {merge(args.out, args.inputs)} page(s) into {args.out}")
    elif args.cmd == "pages":
        print(f"wrote {pages(args.file, args.out, args.pages)} page(s) to {args.out}")
    elif args.cmd == "fields":
        for k, v in fields(args.file).items():
            print(f"{k} = {v}")
    elif args.cmd == "fill":
        fill(args.file, args.out, dict(f.split("=", 1) for f in args.field))
        print(f"wrote {args.out}")
    elif args.cmd == "convert":
        convert(args.file, args.out)
        print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
