"""Word documents: read, create from Markdown, replace text, append,
comment, track changes, accept them.

    python docx_tool.py read FILE
    python docx_tool.py create OUT.docx --markdown FILE.md [--page a4|letter] [--title TEXT]
    python docx_tool.py replace FILE OUT.docx --find TEXT --replace TEXT [--find ... --replace ...]
    python docx_tool.py append FILE OUT.docx --markdown FILE.md
    python docx_tool.py comment FILE OUT.docx --quote TEXT --text NOTE [--quote ... --text ...] [--author NAME]
    python docx_tool.py track FILE OUT.docx --find TEXT --replace TEXT [...] [--author NAME]
    python docx_tool.py accept FILE OUT.docx

`comment` anchors a real Word comment on the first place a quote appears;
`track` makes each replacement a tracked change (shown struck through and
inserted in Word, to accept or reject); `accept` accepts them all. Quotes
are found across Word's run boundaries, and only the quoted text is split
off, so the formatting around it is kept.

Edits keep the document's styles and the formatting of the runs they touch,
and always write OUT (never the input in place unless OUT is the input).
Every file written is schema-valid and carries real properties (title,
author KodAct, dates), not python-docx's template defaults.
"""

import argparse
import copy
import os
import re
import sys
from datetime import datetime, timezone

import docx
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.text.run import Run
from docx.shared import Mm, Pt

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # mdlite, beside this file
from mdlite import blocks, inline, plain

AUTHOR = "KodAct"
# Page sizes and margins, in millimetres.
PAGES = {"a4": (210, 297, 25), "letter": (215.9, 279.4, 25.4)}


def _now():
    return datetime.now(timezone.utc).replace(microsecond=0, tzinfo=None)


def _finish(d, title=None, new=False):
    """What every saved file gets. python-docx's template leaves <w:zoom>
    without the w:percent the schema requires, and properties from 2013 by
    "python-docx": a new file gets real ones, an edited file a new date."""
    zoom = d.settings.element.find(qn("w:zoom"))
    if zoom is not None and zoom.get(qn("w:percent")) is None:
        zoom.set(qn("w:percent"), "100")
    cp = d.core_properties
    if new:
        cp.author = AUTHOR
        cp.created = _now()
        cp.revision = 1
        cp.comments = ""
        cp.title = title or ""
    elif title:
        cp.title = title
    cp.last_modified_by = AUTHOR
    cp.modified = _now()


def _page(d, size):
    w, h, margin = PAGES[size]
    for section in d.sections:
        section.page_width, section.page_height = Mm(w), Mm(h)
        section.left_margin = section.right_margin = section.top_margin = section.bottom_margin = Mm(margin)


def _restart_numbering(d):
    """A numbered list that starts here counts from 1 again: a new w:num over
    the List Number style's own list definition. Without it, every numbered
    list in a document carries on from the one before."""
    style = d.styles["List Number"].element
    num_id = style.pPr.numPr.numId.val
    numbering = d.part.numbering_part.element
    abstract = numbering.num_having_numId(num_id).abstractNumId.val
    num = numbering.add_num(abstract)
    num.add_lvlOverride(ilvl=0).add_startOverride(1)
    return num.numId


def _number(paragraph, num_id):
    numPr = paragraph._p.get_or_add_pPr().get_or_add_numPr()
    numPr.get_or_add_ilvl().val = 0
    numPr.get_or_add_numId().val = num_id


def _changes_and_comments(d):
    """Lines describing the document's comments and tracked changes."""
    lines = []
    try:
        comments = list(d.comments)
    except AttributeError:  # python-docx before 1.2
        comments = []
    for c in comments:
        text = " ".join(p.text for p in c.paragraphs).strip()
        lines.append(f"- comment by {c.author or 'someone'}: {text}")
    body = d.element.body
    for kind, tag in (("inserted", "w:ins"), ("deleted", "w:del")):
        for el in body.iter(qn(tag)):
            words = "".join(t.text or "" for t in el.iter(qn("w:t"), qn("w:delText"))).strip()
            if words:
                lines.append(f"- {kind} by {el.get(qn('w:author')) or 'someone'}: {words}")
    return lines


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
    notes = _changes_and_comments(d)
    if notes:
        out += ["## Comments and tracked changes", ""] + notes
    return "\n".join(out).strip() + "\n"


NUMERIC = re.compile(r"^[+\-−]?[\d.,\s]*\d[\d.,]*\s*%?$")


def _numeric_columns(rows, width):
    """Columns whose every filled cell is a number ("1,200", "+350", "6.8%")."""
    cols = []
    for j in range(width):
        cells = [plain(r[j]) for r in rows if j < len(r) and plain(r[j]).strip()]
        if cells and all(NUMERIC.match(c.strip()) for c in cells):
            cols.append(j)
    return cols


def _runs(paragraph, text):
    for t, bold, italic, code in inline(text):
        run = paragraph.add_run(t)
        run.bold = bold or None
        run.italic = italic or None
        if code:
            run.font.name = "Consolas"
            run.font.size = Pt(10)


def add_markdown(d, markdown):
    num_id, prev = None, None
    for b in blocks(markdown):
        kind = b[0]
        if kind == "number" and prev != "number":
            num_id = _restart_numbering(d)
        prev = kind
        if kind == "heading":
            p = d.add_heading(level=b[1])
            _runs(p, b[2])
        elif kind == "para":
            _runs(d.add_paragraph(), b[1])
        elif kind == "bullet":
            _runs(d.add_paragraph(style="List Bullet"), b[1])
        elif kind == "number":
            p = d.add_paragraph(style="List Number")
            _number(p, num_id)
            _runs(p, b[1])
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
            # Numbers line up on the right, header included, as in a ledger.
            for j in _numeric_columns(rows, len(header)):
                for row in t.rows:
                    row.cells[j].paragraphs[0].alignment = WD_ALIGN_PARAGRAPH.RIGHT


def _first_heading(markdown):
    return next((plain(b[2]) for b in blocks(markdown) if b[0] == "heading"), None)


def create(out, markdown, page="letter", title=None):
    d = docx.Document()
    _page(d, page)
    add_markdown(d, markdown)
    _finish(d, title or _first_heading(markdown), new=True)
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
    _finish(d)
    d.save(out)
    return total


def _paragraphs(d):
    """Every paragraph that holds text: the body, tables, headers, footers."""
    paragraphs = list(d.paragraphs)
    for t in d.tables:
        for row in t.rows:
            for cell in row.cells:
                paragraphs.extend(cell.paragraphs)
    for section in d.sections:
        paragraphs.extend(section.header.paragraphs)
        paragraphs.extend(section.footer.paragraphs)
    return paragraphs


def _split_run(run, at):
    """Split `run` at character `at`: it keeps the text before, and a copy
    with the same formatting, placed after it, takes the rest. Returns the copy."""
    new_r = copy.deepcopy(run._r)
    run._r.addnext(new_r)
    rest = Run(new_r, run._parent)
    text = run.text
    run.text, rest.text = text[:at], text[at:]
    return rest


def _isolate(paragraph, quote):
    """The runs that hold exactly the first `quote` in `paragraph`, split off
    from the text around them (each part keeps its formatting); [] when the
    quote is not there."""
    runs = list(paragraph.runs)
    full = "".join(r.text for r in runs)
    start = full.find(quote)
    if start < 0 or not quote:
        return []
    end = start + len(quote)
    out, pos = [], 0
    for r in runs:
        a, b = pos, pos + len(r.text)
        pos = b
        if b <= start or a >= end:
            continue
        if start > a:
            r = _split_run(r, start - a)
            a = start
        if end < b:
            _split_run(r, end - a)
        out.append(r)
    return out


def _find(d, quote):
    """The runs of the first place `quote` appears, wherever in the document."""
    for p in _paragraphs(d):
        if quote in "".join(r.text for r in p.runs):
            return _isolate(p, quote)
    return []


def comment(path, out, pairs, author=AUTHOR):
    """Anchor a Word comment on each quote. Returns the quotes not found."""
    d = docx.Document(path)
    if not hasattr(d, "add_comment"):
        sys.exit("Word comments need python-docx 1.2 or later: pip install -U python-docx")
    missing = []
    initials = "".join(w[0] for w in author.split()[:3]).upper() or "F"
    for quote, note in pairs:
        runs = _find(d, quote)
        if not runs:
            missing.append(quote)
            continue
        d.add_comment(runs, text=note, author=author, initials=initials)
    _finish(d)
    d.save(out)
    return missing


def _next_id(d):
    """A revision id not yet used in the document."""
    ids = [int(v) for v in d.element.body.xpath(".//@w:id") if str(v).lstrip("-").isdigit()]
    return max(ids, default=0) + 1


def track(path, out, pairs, author=AUTHOR):
    """Each replacement as a tracked change by `author`: the old words
    deleted, the new ones inserted, in the old words' formatting. Returns
    the quotes not found."""
    d = docx.Document(path)
    when = _now().strftime("%Y-%m-%dT%H:%M:%SZ")
    missing = []
    for find, repl in pairs:
        runs = _find(d, find)
        if not runs:
            missing.append(find)
            continue
        attrs = {qn("w:author"): author, qn("w:date"): when}
        for r in runs:
            deleted = OxmlElement("w:del")
            for k, v in attrs.items():
                deleted.set(k, v)
            deleted.set(qn("w:id"), str(_next_id(d)))
            r._r.addprevious(deleted)
            deleted.append(r._r)
            for t in r._r.findall(qn("w:t")):
                dt = OxmlElement("w:delText")
                dt.text = t.text
                dt.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
                t.addprevious(dt)
                r._r.remove(t)
        if repl:
            inserted = OxmlElement("w:ins")
            for k, v in attrs.items():
                inserted.set(k, v)
            inserted.set(qn("w:id"), str(_next_id(d)))
            new_r = copy.deepcopy(runs[0]._r)
            for child in list(new_r):
                if child.tag != qn("w:rPr"):
                    new_r.remove(child)
            t = OxmlElement("w:t")
            t.text = repl
            t.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
            new_r.append(t)
            inserted.append(new_r)
            runs[-1]._r.getparent().addnext(inserted)
    _finish(d)
    d.save(out)
    return missing


def accept(path, out):
    """Accept every tracked change: deletions go, insertions stay as text.
    Returns how many changes were accepted."""
    d = docx.Document(path)
    root = d.element
    n = 0
    for el in list(root.iter(qn("w:del"))):
        parent = el.getparent()
        if parent is not None and parent.tag != qn("w:rPr"):
            parent.remove(el)
            n += 1
    for el in list(root.iter(qn("w:ins"))):
        parent = el.getparent()
        if parent is None:
            continue
        for child in list(el):
            el.addprevious(child)
        parent.remove(el)
        n += 1
    # A paragraph mark marked deleted or inserted is kept as it is.
    for mark in list(root.iter(qn("w:rPr"))):
        for el in mark.findall(qn("w:del")) + mark.findall(qn("w:ins")):
            mark.remove(el)
    _finish(d)
    d.save(out)
    return n


def append(path, out, markdown):
    d = docx.Document(path)
    add_markdown(d, markdown)
    _finish(d)
    d.save(out)


def _text(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("read"); r.add_argument("file")
    c = sub.add_parser("create"); c.add_argument("out"); c.add_argument("--markdown", required=True)
    c.add_argument("--page", choices=sorted(PAGES), default="letter"); c.add_argument("--title")
    p = sub.add_parser("replace"); p.add_argument("file"); p.add_argument("out")
    p.add_argument("--find", action="append", required=True); p.add_argument("--replace", action="append", required=True)
    a = sub.add_parser("append"); a.add_argument("file"); a.add_argument("out"); a.add_argument("--markdown", required=True)
    m = sub.add_parser("comment"); m.add_argument("file"); m.add_argument("out")
    m.add_argument("--quote", action="append", required=True); m.add_argument("--text", action="append", required=True)
    m.add_argument("--author", default=AUTHOR)
    k = sub.add_parser("track"); k.add_argument("file"); k.add_argument("out")
    k.add_argument("--find", action="append", required=True); k.add_argument("--replace", action="append", required=True)
    k.add_argument("--author", default=AUTHOR)
    x = sub.add_parser("accept"); x.add_argument("file"); x.add_argument("out")
    args = ap.parse_args(argv)
    if args.cmd == "read":
        sys.stdout.write(read(args.file))
    elif args.cmd == "create":
        create(args.out, _text(args.markdown), args.page, args.title)
        print(f"wrote {args.out}")
    elif args.cmd == "replace":
        if len(args.find) != len(args.replace):
            ap.error("give one --replace for each --find")
        n = replace(args.file, args.out, list(zip(args.find, args.replace)))
        print(f"replaced {n} occurrence(s); wrote {args.out}")
    elif args.cmd == "append":
        append(args.file, args.out, _text(args.markdown))
        print(f"wrote {args.out}")
    elif args.cmd in ("comment", "track"):
        firsts, seconds = (args.quote, args.text) if args.cmd == "comment" else (args.find, args.replace)
        if len(firsts) != len(seconds):
            ap.error("give one note or replacement for each quote")
        pairs = list(zip(firsts, seconds))
        run = comment if args.cmd == "comment" else track
        missing = run(args.file, args.out, pairs, args.author)
        done = len(pairs) - len(missing)
        what = "comment(s)" if args.cmd == "comment" else "tracked change(s)"
        print(f"added {done} {what}; wrote {args.out}")
        for q in missing:
            print(f"not found: {q!r}", file=sys.stderr)
        if missing:
            sys.exit(1)
    elif args.cmd == "accept":
        n = accept(args.file, args.out)
        print(f"accepted {n} change(s); wrote {args.out}")


if __name__ == "__main__":
    main()
