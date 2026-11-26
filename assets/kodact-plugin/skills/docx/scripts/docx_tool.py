"""Word documents: read, create from Markdown, replace text, append,
comment, track changes, accept them.

    python docx_tool.py init OUT.docx [--audience WORDS | --theme NAME] --title T [--subtitle S] [--tag T] [--page a4|letter]
    python docx_tool.py add FILE --json BLOCK      (BLOCK: JSON object or list, or @file.json; saves in place)
    python docx_tool.py read FILE [--section WORDS | --all | -o OUT.md]   (a long one: its opening and an outline)
    python docx_tool.py create OUT.docx --markdown FILE.md [--page a4|letter] [--title TEXT]
    python docx_tool.py replace FILE OUT.docx --find TEXT --replace TEXT [--find ... --replace ...]
    python docx_tool.py append FILE OUT.docx --markdown FILE.md
    python docx_tool.py comment FILE OUT.docx --quote TEXT --text NOTE [--quote ... --text ...] [--author NAME]
    python docx_tool.py track FILE OUT.docx --find TEXT --replace TEXT [...] [--author NAME]
    python docx_tool.py accept FILE OUT.docx

`init` makes the requested document first: the theme (picked from who it is
for) on Word's own styles, a cover band, a header and page numbers. Each
`add` appends blocks and saves, so the person watches it grow. Blocks are
real Word parts the person can keep editing:

    {"block": "heading", "text": "…", "level": 1}
    {"block": "text", "text": "Paragraphs; **bold** spans", "lead": true}
    {"block": "bullets", "items": [...], "numbered": false}
    {"block": "callout", "kind": "note|tip|info|warning|danger|success", "title": "…", "text": "…"}
    {"block": "table", "columns": [...], "rows": [[...]], "highlight": 2, "caption": "…"}
    {"block": "kpis", "items": [{"value": "185k", "label": "Revenue", "delta": "+23%", "better": "up"}]}
    {"block": "cards", "items": [{"head": "…", "text": "…"}], "columns": 3}
    {"block": "steps", "items": ["Head: what happens", ...]}
    {"block": "timeline", "items": ["Jan: kickoff", ...]}
    {"block": "compare", "left": {"title", "items"}, "right": {...}, "verdict": "…"}
    {"block": "quote", "quote": "…", "by": "…"}
    {"block": "chart", "chart": {"type": "column|bar|line|area|pie|doughnut", "categories": [...],
     "series": {"Name": [...]}}, "caption": "…"}
    {"block": "image", "path": "pic.png", "caption": "…"}
    {"block": "toc"}  {"block": "pagebreak"}

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
from mdlite import blocks, inline, plain  # noqa: E402
# The design and icon code the document skills share: kodact-plugin/scripts.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "scripts"))

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


THEME_MARK = "kodact-theme:"


def init(out, theme="clean", title="", subtitle=None, tag=None, page="letter"):
    import deck_design
    import doc_design
    t = deck_design.theme(theme)
    d = docx.Document()
    _page(d, page)
    doc_design.apply_theme(d, t, title)
    doc_design.cover(d, t, title or "Untitled", subtitle, tag)
    _finish(d, title, new=True)
    d.core_properties.category = THEME_MARK + theme
    _save(d, out)


def add(path, specs, theme=None):
    """Append blocks (one dict, or a list) and save in place; returns how many."""
    import deck_design
    import doc_design
    d = docx.Document(path)
    cat = d.core_properties.category or ""
    name = theme or (cat[len(THEME_MARK):] if cat.startswith(THEME_MARK) else "clean")
    t = deck_design.theme(name)
    specs = specs if isinstance(specs, list) else [specs]
    for spec in specs:
        if not isinstance(spec, dict):
            sys.exit('a block is a JSON object, e.g. {"block": "text", "text": "…"}')
        doc_design.add_block(d, t, spec)
    _finish(d)
    _save(d, path)
    return len(specs)


def _save(d, out):
    """Write beside the target and move it into place: a viewer never reads half a file."""
    tmp = out + ".part"
    d.save(tmp)
    os.replace(tmp, out)


def _json(arg):
    import json
    raw = arg
    if arg.startswith("@"):
        with open(arg[1:], encoding="utf-8") as f:
            raw = f.read()
    try:
        return json.loads(raw)
    except json.JSONDecodeError as e:
        sys.exit(f"--json is not valid JSON ({e}); quote keys and strings with double quotes")


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


CHARS_SHOWN = 20_000  # what `read` prints of a long document unless asked for all of it


def bound(markdown, section=None, most=CHARS_SHOWN):
    """What `read` prints: one section ("--section": from the heading that contains `section` to
    the next heading of its level or above), or the document's opening up to `most` characters and
    an outline of the headings after it. A 200-page report printed whole was 127,000 characters."""
    lines = markdown.splitlines()
    heads = [(i, len(ln) - len(ln.lstrip("#")), ln.lstrip("#").strip())
             for i, ln in enumerate(lines) if re.match(r"^#{1,6} ", ln)]
    if section:
        want = section.strip().lower()
        hit = next((h for h in heads if h[2].lower() == want), None) or \
            next((h for h in heads if want in h[2].lower()), None)
        if hit is None:
            names = ", ".join(repr(h[2]) for h in heads[:40])
            raise SystemExit(f"no heading contains {section!r}; the headings are {names}"
                             + (" …" if len(heads) > 40 else ""))
        start, level = hit[0], hit[1]
        end = next((i for i, lv, _ in heads if i > start and lv <= level), len(lines))
        return "\n".join(lines[start:end]).strip() + "\n"
    if most is None or len(markdown) <= most:
        return markdown
    used, cut = 0, 0
    for i, ln in enumerate(lines):
        if used + len(ln) + 1 > most:
            break
        used += len(ln) + 1
        cut = i + 1
    # End at a block boundary, not mid-table or mid-paragraph.
    while cut > 0 and lines[cut - 1].strip():
        cut -= 1
    rest = [h for h in heads if h[0] >= cut]
    out = lines[:cut] + ["", f"… {len(markdown) - used:,} more characters not shown. The headings after this point:"]
    for _, lv, name in rest[:60]:
        out.append("  " * (lv - 1) + f"- {name}")
    if len(rest) > 60:
        out.append(f"  … and {len(rest) - 60} more headings")
    out += ["", 'Read one with --section "heading words", all of it with --all, or write it to a file with '
            "-o out.md and search that."]
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


def _doc_theme(d):
    """The theme `init` gave this document (kept in its category), or None for a plain one."""
    cat = d.core_properties.category or ""
    if not cat.startswith(THEME_MARK):
        return None
    import deck_design

    return deck_design.theme(cat[len(THEME_MARK):])


def _theme_table(t, tbl):
    """A Markdown table in a themed document, drawn like the `table` block: the header in the
    accent, banded rows, hairlines, and a header that repeats on each page."""
    import doc_design as dd
    from docx.shared import RGBColor

    t = dd.paper(t)
    a = t["accents"][0]
    tbl.style = None
    dd.no_borders(tbl)
    dd.cell_margins(tbl, 70, 70, 120, 120)
    dd.put(tbl.rows[0]._tr.get_or_add_trPr(), OxmlElement("w:tblHeader"))
    for ri, row in enumerate(tbl.rows):
        for c in row.cells:
            if ri == 0:
                dd.shade(c, a)
            else:
                dd.shade(c, "FFFFFF" if ri % 2 else dd.mix(a, 0.94))
                dd.borders(c, bottom=("single", 4, dd.mix(a, 0.8)))
            for p in c.paragraphs:
                dd.spacing(p, 0, 0)
                for r in p.runs:
                    r.font.size = Pt(10.5)
                    r.font.color.rgb = RGBColor.from_string("FFFFFF" if ri == 0 else t["ink"])


def add_markdown(d, markdown):
    theme = _doc_theme(d)
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
            if theme:
                _theme_table(theme, t)
                d.add_paragraph()


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
    i = sub.add_parser("init"); i.add_argument("out"); i.add_argument("--theme"); i.add_argument("--audience")
    i.add_argument("--title", required=True); i.add_argument("--subtitle"); i.add_argument("--tag")
    i.add_argument("--page", choices=sorted(PAGES), default="letter")
    b = sub.add_parser("add"); b.add_argument("file"); b.add_argument("--json", required=True); b.add_argument("--theme")
    r = sub.add_parser("read"); r.add_argument("file")
    r.add_argument("--section", help="print only the section under the heading containing these words")
    r.add_argument("--all", action="store_true", help=f"print everything (default: the first {CHARS_SHOWN:,} characters)")
    r.add_argument("-o", "--out", help="write the whole document as Markdown to this file")
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
    if args.cmd == "init":
        import deck_design
        theme = args.theme or deck_design.for_audience(args.audience)
        deck_design.theme(theme)
        init(args.out, theme, args.title, args.subtitle, args.tag, args.page)
        print(f"wrote {args.out}: its {theme} cover; add blocks with `add`")
    elif args.cmd == "add":
        n = add(args.file, _json(args.json), args.theme)
        print(f"added {n} block(s); saved {args.file}")
    elif args.cmd == "read":
        text = read(args.file)
        if args.out:
            with open(args.out, "w", encoding="utf-8") as f:
                f.write(text)
            print(f"wrote {len(text):,} characters to {args.out}")
        else:
            sys.stdout.write(bound(text, args.section, None if args.all else CHARS_SHOWN))
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
