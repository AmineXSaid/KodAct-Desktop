"""PowerPoint decks: read, create from an outline, replace text, set one
shape's text, write speaker notes, reorder, delete and duplicate slides.

    python pptx_tool.py read FILE [--shapes]
    python pptx_tool.py create OUT.pptx --outline FILE.md
    python pptx_tool.py replace FILE OUT.pptx --find TEXT --replace TEXT [...]
    python pptx_tool.py set-text FILE OUT.pptx --slide N --shape NAME|#INDEX --text TEXT
    python pptx_tool.py notes FILE OUT.pptx --slide N --text TEXT
    python pptx_tool.py slides FILE OUT.pptx --order 1,3,2   (slides left out are deleted)
    python pptx_tool.py duplicate FILE OUT.pptx --slide N    (the copy goes right after it)

`read --shapes` names each slide's shapes (`#0 Title 1: …`), which is what
`set-text` takes. `set-text` keeps the shape's formatting: each line of the
new text becomes a paragraph styled like the one it replaces.

An outline is Markdown: each "# Title" starts a slide (the first is the
title slide, with the paragraph under it as its subtitle); "- " lines are
its bullets, indented two spaces per level.

New decks are 16:9 (13.33 x 7.5 in), with the template's placeholders laid
out for that width, body text that shrinks to fit its box, and real
properties (title, author Forge, dates) instead of python-pptx's defaults.
"""

import argparse
import copy
import re
import sys
from datetime import datetime, timezone

from pptx import Presentation
from pptx.enum.text import MSO_AUTO_SIZE
from pptx.util import Emu

AUTHOR = "Forge"
# 16:9, as PowerPoint makes new decks.
WIDE = (Emu(12192000), Emu(6858000))


def _now():
    return datetime.now(timezone.utc).replace(microsecond=0, tzinfo=None)


def _widen(prs):
    """Make the deck 16:9. python-pptx's template is 4:3 and its master and
    layouts place their placeholders for that width, so stretch them across."""
    k = WIDE[0] / prs.slide_width
    prs.slide_width, prs.slide_height = WIDE
    for owner in [prs.slide_master, *prs.slide_layouts]:
        for shape in owner.shapes:
            # Only shapes placed by their own xfrm: the rest inherit from the
            # master, already stretched.
            if shape._element.xpath("./p:spPr/a:xfrm") and shape.width:
                shape.left, shape.width = Emu(round(shape.left * k)), Emu(round(shape.width * k))


def _finish(prs, title=None, new=False):
    cp = prs.core_properties
    if new:
        cp.author = AUTHOR
        cp.created = _now()
        cp.revision = 1
        cp.title = title or ""
        cp.subject = cp.keywords = cp.comments = cp.category = ""
    cp.last_modified_by = AUTHOR
    cp.modified = _now()


def read(path, shapes=False):
    """Each slide's title and text, as Markdown; with `shapes`, each text
    shape by index and name, as `set-text` names them."""
    prs = Presentation(path)
    out = []
    for n, slide in enumerate(prs.slides, start=1):
        title = slide.shapes.title.text_frame.text.strip() if slide.shapes.title is not None and slide.shapes.title.has_text_frame else ""
        out.append(f"## Slide {n}" + (f": {title}" if title else ""))
        if shapes:
            for i, shape in enumerate(slide.shapes):
                text = shape.text_frame.text.strip().replace("\n", " / ") if shape.has_text_frame else ""
                out.append(f"- #{i} {shape.name}" + (f": {text}" if text else ""))
            out.append("")
            continue
        for shape in slide.shapes:
            if not shape.has_text_frame or shape == slide.shapes.title:
                continue
            for p in shape.text_frame.paragraphs:
                text = "".join(r.text for r in p.runs).strip()
                if text:
                    out.append("  " * p.level + "- " + text)
        if slide.has_notes_slide and slide.notes_slide.notes_text_frame.text.strip():
            out.append("> Notes: " + slide.notes_slide.notes_text_frame.text.strip())
        out.append("")
    return "\n".join(out)


def create(out, outline):
    prs = Presentation()
    _widen(prs)
    slides, current = [], None
    for line in outline.splitlines():
        if m := re.match(r"^#\s+(.*)$", line.strip()):
            current = {"title": m.group(1).strip(), "bullets": [], "subtitle": []}
            slides.append(current)
        elif current is not None and (m := re.match(r"^(\s*)[-*]\s+(.*)$", line)):
            current["bullets"].append((len(m.group(1)) // 2, m.group(2).strip()))
        elif current is not None and line.strip():
            current["subtitle"].append(line.strip())
    for i, s in enumerate(slides):
        if i == 0:
            slide = prs.slides.add_slide(prs.slide_layouts[0])
            slide.shapes.title.text = s["title"]
            if len(slide.placeholders) > 1:
                slide.placeholders[1].text = " ".join(s["subtitle"]) or ""
            continue
        slide = prs.slides.add_slide(prs.slide_layouts[1])
        slide.shapes.title.text = s["title"]
        body = slide.placeholders[1].text_frame
        body.clear()
        body.auto_size = MSO_AUTO_SIZE.TEXT_TO_FIT_SHAPE
        for j, (level, text) in enumerate(s["bullets"]):
            p = body.paragraphs[0] if j == 0 else body.add_paragraph()
            p.text = text
            p.level = min(level, 4)
    _finish(prs, slides[0]["title"] if slides else None, new=True)
    prs.save(out)
    return len(slides)


def replace(path, out, pairs):
    prs = Presentation(path)
    total = 0
    for slide in prs.slides:
        for shape in slide.shapes:
            if not shape.has_text_frame:
                continue
            for p in shape.text_frame.paragraphs:
                for find, repl in pairs:
                    for run in p.runs:
                        if find in run.text:
                            total += run.text.count(find)
                            run.text = run.text.replace(find, repl)
    _finish(prs)
    prs.save(out)
    return total


def _slide(prs, n):
    if not 1 <= n <= len(prs.slides):
        sys.exit(f"the deck has {len(prs.slides)} slide(s); there is no slide {n}")
    return prs.slides[n - 1]


def _shape(slide, which):
    """A shape by `#index` (as `read --shapes` lists them) or by name."""
    shapes = list(slide.shapes)
    if which.startswith("#") and which[1:].isdigit():
        i = int(which[1:])
        if i < len(shapes):
            return shapes[i]
    for shape in shapes:
        if shape.name == which:
            return shape
    sys.exit(f"no shape {which!r} on that slide; `read --shapes` lists them")


def _set_paragraphs(frame, lines):
    """Put `lines` in a text frame, one paragraph each, each styled as the
    paragraph it replaces (the last one's style for any extra lines)."""
    paras = list(frame.paragraphs)
    for i, line in enumerate(lines):
        if i < len(paras):
            p = paras[i]
        else:
            new_p = copy.deepcopy(paras[-1]._p)
            paras[-1]._p.addnext(new_p)
            p = frame.paragraphs[i]
        runs = p.runs
        if runs:
            runs[0].text = line
            for r in runs[1:]:
                r._r.getparent().remove(r._r)
        else:
            p.text = line
    for p in list(frame.paragraphs)[len(lines):]:
        p._p.getparent().remove(p._p)


def set_text(path, out, slide_n, which, text):
    prs = Presentation(path)
    shape = _shape(_slide(prs, slide_n), which)
    if not shape.has_text_frame:
        sys.exit(f"{shape.name} holds no text")
    _set_paragraphs(shape.text_frame, text.split("\n"))
    _finish(prs)
    prs.save(out)


def notes(path, out, slide_n, text):
    prs = Presentation(path)
    frame = _slide(prs, slide_n).notes_slide.notes_text_frame
    frame.text = text
    _finish(prs)
    prs.save(out)


def reorder(path, out, order):
    """Keep the slides in `order` (numbers from 1), in that order; the rest are deleted."""
    prs = Presentation(path)
    ids = prs.slides._sldIdLst
    entries = list(ids)
    if any(not 1 <= n <= len(entries) for n in order) or len(set(order)) != len(order):
        sys.exit(f"--order takes each of slides 1-{len(entries)} at most once")
    for n, e in enumerate(entries, start=1):
        ids.remove(e)
        if n not in order:
            prs.part.drop_rel(e.rId)
    for n in order:
        ids.append(entries[n - 1])
    _finish(prs)
    prs.save(out)
    return len(order)


def duplicate(path, out, slide_n):
    """A copy of slide N, with its pictures and notes, right after it."""
    prs = Presentation(path)
    src = _slide(prs, slide_n)
    dst = prs.slides.add_slide(src.slide_layout)
    for shape in list(dst.shapes):
        shape._element.getparent().remove(shape._element)
    # Pictures, media and links the shapes point at, under new ids.
    remap = {}
    for rid, rel in src.part.rels.items():
        if "notesSlide" in rel.reltype or "slideLayout" in rel.reltype:
            continue
        if rel.is_external:
            remap[rid] = dst.part.relate_to(rel.target_ref, rel.reltype, is_external=True)
        else:
            remap[rid] = dst.part.relate_to(rel.target_part, rel.reltype)
    tree = dst.shapes._spTree
    for el in src.shapes._spTree.iterchildren():
        if el.tag.endswith("}nvGrpSpPr") or el.tag.endswith("}grpSpPr"):
            continue
        new = copy.deepcopy(el)
        for node in new.iter():
            for key, value in list(node.attrib.items()):
                if key.startswith("{http://schemas.openxmlformats.org/officeDocument/2006/relationships}") and value in remap:
                    node.set(key, remap[value])
        tree.append(new)
    if src.has_notes_slide and src.notes_slide.notes_text_frame.text:
        dst.notes_slide.notes_text_frame.text = src.notes_slide.notes_text_frame.text
    # Move the copy (added last) to right after its source.
    ids = prs.slides._sldIdLst
    entry = list(ids)[-1]
    ids.remove(entry)
    list(ids)[slide_n - 1].addnext(entry)
    _finish(prs)
    prs.save(out)


def _text(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("read"); r.add_argument("file"); r.add_argument("--shapes", action="store_true")
    c = sub.add_parser("create"); c.add_argument("out"); c.add_argument("--outline", required=True)
    p = sub.add_parser("replace"); p.add_argument("file"); p.add_argument("out")
    p.add_argument("--find", action="append", required=True); p.add_argument("--replace", action="append", required=True)
    t = sub.add_parser("set-text"); t.add_argument("file"); t.add_argument("out")
    t.add_argument("--slide", type=int, required=True); t.add_argument("--shape", required=True); t.add_argument("--text", required=True)
    n = sub.add_parser("notes"); n.add_argument("file"); n.add_argument("out")
    n.add_argument("--slide", type=int, required=True); n.add_argument("--text", required=True)
    o = sub.add_parser("slides"); o.add_argument("file"); o.add_argument("out"); o.add_argument("--order", required=True)
    d = sub.add_parser("duplicate"); d.add_argument("file"); d.add_argument("out"); d.add_argument("--slide", type=int, required=True)
    args = ap.parse_args(argv)
    if args.cmd == "read":
        sys.stdout.write(read(args.file, args.shapes))
    elif args.cmd == "set-text":
        set_text(args.file, args.out, args.slide, args.shape, args.text)
        print(f"set slide {args.slide} {args.shape}; wrote {args.out}")
    elif args.cmd == "notes":
        notes(args.file, args.out, args.slide, args.text)
        print(f"wrote slide {args.slide}'s notes; wrote {args.out}")
    elif args.cmd == "slides":
        order = [int(x) for x in args.order.replace(" ", "").split(",") if x]
        kept = reorder(args.file, args.out, order)
        print(f"kept {kept} slide(s) in the order {order}; wrote {args.out}")
    elif args.cmd == "duplicate":
        duplicate(args.file, args.out, args.slide)
        print(f"copied slide {args.slide} after it; wrote {args.out}")
    elif args.cmd == "create":
        n = create(args.out, _text(args.outline))
        print(f"wrote {n} slide(s) to {args.out}")
    elif args.cmd == "replace":
        if len(args.find) != len(args.replace):
            ap.error("give one --replace for each --find")
        n = replace(args.file, args.out, list(zip(args.find, args.replace)))
        print(f"replaced {n} occurrence(s); wrote {args.out}")


if __name__ == "__main__":
    main()
