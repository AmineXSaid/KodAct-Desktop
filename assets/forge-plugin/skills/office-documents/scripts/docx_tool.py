"""Word documents: read, create from Markdown, replace text, append.

    python docx_tool.py read FILE
    python docx_tool.py create OUT.docx --markdown FILE.md
    python docx_tool.py replace FILE OUT.docx --find TEXT --replace TEXT [--find ... --replace ...]
    python docx_tool.py append FILE OUT.docx --markdown FILE.md

Edits keep the document's styles and the formatting of the runs they touch,
and always write OUT (never the input in place unless OUT is the input).
"""

import argparse
import os
import sys

import docx
from docx.shared import Pt

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # mdlite, beside this file
from mdlite import blocks, inline


def read(path):
    """The document as Markdown: headings, lists, paragraphs and tables, in order."""
    d = docx.Document(path)
    out = []
    body = d.element.body
    tables = {t._tbl: t for t in d.tables}
    paras = {p._p: p for p in d.paragraphs}
    for child in body.iterchildren():
        if child in paras:
            p = paras[child]
            text = p.text.strip()
            if not text:
                continue
            style = (p.style.name or "").lower() if p.style is not None else ""
            if style.startswith("heading"):
                level = int(style.split()[-1]) if style.split()[-1].isdigit() else 1
                out.append("#" * min(level, 3) + " " + text)
            elif style == "title":
                out.append("# " + text)
            elif "list bullet" in style:
                out.append("- " + text)
            elif "list number" in style:
                out.append("1. " + text)
            else:
                out.append(text)
        elif child in tables:
            t = tables[child]
            rows = [[c.text.strip() for c in r.cells] for r in t.rows]
            if rows:
                out.append("| " + " | ".join(rows[0]) + " |")
                out.append("|" + "---|" * len(rows[0]))
                out.extend("| " + " | ".join(r) + " |" for r in rows[1:])
        out.append("")
    return "\n".join(out).strip() + "\n"


def _runs(paragraph, text):
    for t, bold, italic, code in inline(text):
        run = paragraph.add_run(t)
        run.bold = bold or None
        run.italic = italic or None
        if code:
            run.font.name = "Consolas"
            run.font.size = Pt(10)


def add_markdown(d, markdown):
    for b in blocks(markdown):
        kind = b[0]
        if kind == "heading":
            p = d.add_heading(level=b[1])
            _runs(p, b[2])
        elif kind == "para":
            _runs(d.add_paragraph(), b[1])
        elif kind == "bullet":
            _runs(d.add_paragraph(style="List Bullet"), b[1])
        elif kind == "number":
            _runs(d.add_paragraph(style="List Number"), b[1])
        elif kind == "code":
            p = d.add_paragraph()
            run = p.add_run(b[1])
            run.font.name = "Consolas"
            run.font.size = Pt(10)
        elif kind == "table":
            header, rows = b[1], b[2]
            t = d.add_table(rows=1 + len(rows), cols=len(header))
            t.style = "Table Grid"
            for j, h in enumerate(header):
                # A new table's cells are empty: write runs straight in.
                cell = t.rows[0].cells[j]
                _runs(cell.paragraphs[0], h)
                for run in cell.paragraphs[0].runs:
                    run.bold = True
            for i, r in enumerate(rows, start=1):
                for j in range(len(header)):
                    _runs(t.rows[i].cells[j].paragraphs[0], r[j] if j < len(r) else "")


def create(out, markdown):
    d = docx.Document()
    add_markdown(d, markdown)
    d.save(out)


def _replace_in_paragraph(p, find, repl):
    """Replace inside one paragraph, keeping the first touched run's formatting."""
    if find not in p.text:
        return 0
    count = 0
    # Fast path: the text sits inside single runs.
    for run in p.runs:
        if find in run.text:
            count += run.text.count(find)
            run.text = run.text.replace(find, repl)
    if count:
        return count
    # The text spans runs: rebuild them, keeping the first run's style.
    full = p.text
    count = full.count(find)
    first = p.runs[0]
    first.text = full.replace(find, repl)
    for run in p.runs[1:]:
        run.text = ""
    return count


def replace(path, out, pairs):
    d = docx.Document(path)
    total = 0
    paragraphs = list(d.paragraphs)
    for t in d.tables:
        for row in t.rows:
            for cell in row.cells:
                paragraphs.extend(cell.paragraphs)
    for section in d.sections:
        paragraphs.extend(section.header.paragraphs)
        paragraphs.extend(section.footer.paragraphs)
    for find, repl in pairs:
        for p in paragraphs:
            total += _replace_in_paragraph(p, find, repl)
    d.save(out)
    return total


def append(path, out, markdown):
    d = docx.Document(path)
    add_markdown(d, markdown)
    d.save(out)


def _text(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("read"); r.add_argument("file")
    c = sub.add_parser("create"); c.add_argument("out"); c.add_argument("--markdown", required=True)
    p = sub.add_parser("replace"); p.add_argument("file"); p.add_argument("out")
    p.add_argument("--find", action="append", required=True); p.add_argument("--replace", action="append", required=True)
    a = sub.add_parser("append"); a.add_argument("file"); a.add_argument("out"); a.add_argument("--markdown", required=True)
    args = ap.parse_args(argv)
    if args.cmd == "read":
        sys.stdout.write(read(args.file))
    elif args.cmd == "create":
        create(args.out, _text(args.markdown))
        print(f"wrote {args.out}")
    elif args.cmd == "replace":
        if len(args.find) != len(args.replace):
            ap.error("give one --replace for each --find")
        n = replace(args.file, args.out, list(zip(args.find, args.replace)))
        print(f"replaced {n} occurrence(s); wrote {args.out}")
    elif args.cmd == "append":
        append(args.file, args.out, _text(args.markdown))
        print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
