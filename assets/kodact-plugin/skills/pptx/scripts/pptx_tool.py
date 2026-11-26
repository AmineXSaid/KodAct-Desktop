"""PowerPoint decks: read, create from an outline, replace text, set one
shape's text, write speaker notes, reorder, delete and duplicate slides.

    python pptx_tool.py init OUT.pptx [--audience WORDS | --theme NAME] [--title T] [--subtitle S] [--tag T]
    python pptx_tool.py add FILE --json SLIDE      (SLIDE: JSON text, or @file.json; appends, saves in place)
    python pptx_tool.py read FILE [--shapes] [--slides 1-5,9]   (a long deck: 30 slides, then titles)
    python pptx_tool.py create OUT.pptx --outline FILE.md
    python pptx_tool.py replace FILE OUT.pptx --find TEXT --replace TEXT [...]
    python pptx_tool.py set-text FILE OUT.pptx --slide N --shape NAME|#INDEX --text TEXT
    python pptx_tool.py notes FILE OUT.pptx --slide N --text TEXT
    python pptx_tool.py slides FILE OUT.pptx --order 1,3,2   (slides left out are deleted)
    python pptx_tool.py duplicate FILE OUT.pptx --slide N    (the copy goes right after it)

`init` makes the real file first, a designed title slide, so the person
sees the deck from the first step; each `add` appends one designed slide and
saves, so they watch it grow. A slide is JSON:

    {"layout": "cards", "title": "Your Body Is Made of Cells",
     "items": ["Billions of tiny parts", "Each has a job"], "icon": "cell",
     "notes": "What to say."}

Layouts (and their keys; every one but title and closing also takes "title" and "icon"):
  title      title, subtitle, tag, icon, style: "dark"
  feature    stat, stat_text, note, items [{"icon", "head", "text"}] (up to 4)
  cards      items (up to 6); marks: number|heart|star|sun
  steps      items "Head: body" (2-4); footer
  statement  statement, items, art
  faces      items [{"label", "mood"}] (up to 4), lines
  bullets    items (up to 7; indent two spaces per level)
  chart      chart {"type": bar|column|line|pie|doughnut|area|stacked, "categories": [...],
             "series": {"Name": [numbers]}}, items (takeaways beside it). A native, editable chart.
  table      columns [...], rows [[...]] (up to 12), highlight (row index)
  timeline   items "When: what" (up to 6)
  compare    left {"title", "items"}, right {...}, verdict
  grid       items [{"icon", "head", "text"}] or "Head: text" (3-6)
  kpis       items [{"value", "label", "delta", "better": up|down}] (2-4), footer
  quote      quote, by
  image      path (a PNG or JPG), caption, items
  cycle      items (up to 6), center
  pyramid    items "Level: what", top first (up to 5)
  closing    title, items
Items past a layout's limit are left out, with a note on stderr; so is text too long for its box
even at 12pt. Icons: any of Lucide's 1,500+ names (`python ../../../scripts/icons.py search WORD`,
from this folder), or sun star heart cloud cell. Any slide takes "section" to start a deck section,
"notes" for speaker notes and "accent" (0-5) to pick its colour.
Moods: happy sad worried angry confused surprised. Text is sized to fit.

`read --shapes` names each slide's shapes (`#0 Title 1: …`), which is what
`set-text` takes. `set-text` keeps the shape's formatting: each line of the
new text becomes a paragraph styled like the one it replaces.

An outline is Markdown: each "# Title" starts a slide (the first is the
title slide, with the paragraph under it as its subtitle); "- " lines are
its bullets, indented two spaces per level.

New decks are 16:9 (13.33 x 7.5 in), with the template's placeholders laid
out for that width, body text that shrinks to fit its box, and real
properties (title, author KodAct, dates) instead of python-pptx's defaults.
"""

import argparse
import copy
import json
import os
import re
import sys
from datetime import datetime, timezone

from pptx import Presentation
from pptx.enum.text import MSO_AUTO_SIZE
from pptx.util import Emu

# The design and icon code the document skills share: kodact-plugin/scripts.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "scripts"))
import deck_design  # noqa: E402

AUTHOR = "KodAct"
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


THEME_MARK = "kodact-theme:"


def init(out, theme="kids", title=None, subtitle=None, tag=None, style=None):
    """A new 16:9 deck holding its designed title slide, saved at once."""
    t = deck_design.theme(theme)
    prs = Presentation()
    _widen(prs)
    deck_design.add_slide(prs, t, {"layout": "title", "title": title or "Untitled deck",
                                   "subtitle": subtitle, "tag": tag, "style": style}, 1)
    _finish(prs, title, new=True)
    prs.core_properties.category = THEME_MARK + theme
    _write_theme(prs, t, theme)
    _save(prs, out)
    return 1


A = "{http://schemas.openxmlformats.org/drawingml/2006/main}"


def _write_theme(prs, t, name):
    """The deck's own theme (Design → Variants in PowerPoint): its colours and
    fonts, so the person can restyle the whole deck from one place."""
    from lxml import etree
    part = next(r.target_part for r in prs.slide_master.part.rels.values() if r.reltype.endswith("/theme"))
    root = etree.fromstring(part.blob)
    el = root.find(f".//{A}themeElements")
    root.set("name", f"KodAct {name}")
    scheme = el.find(f"{A}clrScheme")
    scheme.set("name", f"KodAct {name}")
    colours = {"dk1": t["ink"], "lt1": "FFFFFF", "dk2": t["soft"], "lt2": t["bg"]}
    for i, c in enumerate(t["accents"][:6], start=1):
        colours[f"accent{i}"] = c
    for key, hexv in colours.items():
        slot = scheme.find(f"{A}{key}")
        if slot is None:
            continue
        for child in list(slot):
            slot.remove(child)
        etree.SubElement(slot, f"{A}srgbClr").set("val", hexv.upper())
    fonts = el.find(f"{A}fontScheme")
    fonts.set("name", f"KodAct {name}")
    for which, face in (("majorFont", t["head"]), ("minorFont", t["body"])):
        latin = fonts.find(f"{A}{which}/{A}latin")
        if latin is not None:
            latin.set("typeface", face)
    part._blob = etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone=True)


P14 = "http://schemas.microsoft.com/office/powerpoint/2010/main"
SECTIONS_EXT = "{521415D9-36F7-43E2-AB2F-B90AF26B5E84}"


def _sections(prs):
    """The deck's sections (PowerPoint 2010+): [(name, [slide ids])]."""
    ext = _section_ext(prs, create=False)
    if ext is None:
        return []
    out = []
    for sec in ext.iter(f"{{{P14}}}section"):
        out.append((sec.get("name"), [int(i.get("id")) for i in sec.iter(f"{{{P14}}}sldId")]))
    return out


def _section_ext(prs, create=True):
    from lxml import etree
    pres = prs.part._element
    PNS = "http://schemas.openxmlformats.org/presentationml/2006/main"
    lst = pres.find(f"{{{PNS}}}extLst")
    if lst is None:
        if not create:
            return None
        lst = etree.SubElement(pres, f"{{{PNS}}}extLst")
    for ext in lst.findall(f"{{{PNS}}}ext"):
        if ext.get("uri") == SECTIONS_EXT:
            return ext
    if not create:
        return None
    ext = etree.SubElement(lst, f"{{{PNS}}}ext")
    ext.set("uri", SECTIONS_EXT)
    etree.SubElement(ext, f"{{{P14}}}sectionLst", nsmap={"p14": P14})
    return ext


def _place_in_section(prs, slide, section):
    """Put the new slide in `section` (a new one when named), else in the last one."""
    import uuid
    from lxml import etree
    sid = int(prs.slides._sldIdLst[-1].get("id"))
    if section is None and _section_ext(prs, create=False) is None:
        return
    ext = _section_ext(prs)
    lst = ext.find(f"{{{P14}}}sectionLst")
    secs = lst.findall(f"{{{P14}}}section")
    if section is not None or not secs:
        sec = etree.SubElement(lst, f"{{{P14}}}section")
        sec.set("name", section or "Slides")
        sec.set("id", "{" + str(uuid.uuid4()).upper() + "}")
        if not secs and len(prs.slides) > 1:
            # Slides made before the first section join an opening one.
            first = etree.Element(f"{{{P14}}}section")
            first.set("name", "Opening")
            first.set("id", "{" + str(uuid.uuid4()).upper() + "}")
            ids = etree.SubElement(first, f"{{{P14}}}sldIdLst")
            for e in list(prs.slides._sldIdLst)[:-1]:
                etree.SubElement(ids, f"{{{P14}}}sldId").set("id", e.get("id"))
            lst.insert(0, first)
        etree.SubElement(sec, f"{{{P14}}}sldIdLst")
    else:
        sec = secs[-1]
    sec.find(f"{{{P14}}}sldIdLst").append(etree.Element(f"{{{P14}}}sldId", id=str(sid)))


def _theme_of(prs, override=None):
    if override:
        return override
    cat = prs.core_properties.category or ""
    return cat[len(THEME_MARK):] if cat.startswith(THEME_MARK) else "clean"


def add(path, spec, theme=None):
    """Append one designed slide from `spec` and save the deck in place."""
    if not isinstance(spec, dict):
        sys.exit("a slide is a JSON object, e.g. {\"layout\": \"cards\", \"title\": \"…\", \"items\": […]}")
    prs = Presentation(path)
    t = deck_design.theme(_theme_of(prs, theme))
    n = len(prs.slides) + 1
    slide = deck_design.add_slide(prs, t, spec, n)
    _place_in_section(prs, slide, spec.get("section"))
    _finish(prs)
    _save(prs, path)
    return n


def _save(prs, out):
    """Write next to the target, then move it into place: a viewer watching the
    file (KodAct's panel) never reads half a deck."""
    tmp = out + ".part"
    prs.save(tmp)
    os.replace(tmp, out)


def _spec(arg):
    raw = arg
    if arg.startswith("@"):
        with open(arg[1:], encoding="utf-8") as f:
            raw = f.read()
    try:
        return json.loads(raw)
    except json.JSONDecodeError as e:
        sys.exit(f"--json is not valid JSON ({e}); quote keys and strings with double quotes")


def _title_shape(slide):
    """The slide's title: its title placeholder, or the text box `add` names Title."""
    if slide.shapes.title is not None and slide.shapes.title.has_text_frame:
        return slide.shapes.title
    return next((s for s in slide.shapes if s.name == "Title" and s.has_text_frame), None)


SLIDES_SHOWN = 30  # slides `read` prints unless --slides names others


def _slide_spec(spec, total):
    """1-based slide numbers from "1-5,9"."""
    picked = set()
    for part in str(spec).split(","):
        part = part.strip()
        if not part:
            continue
        a, _, b = part.partition("-")
        lo, hi = int(a), int(b or a)
        picked.update(range(max(1, lo), min(total, hi) + 1))
    return sorted(picked)


def read(path, shapes=False, slides=None):
    """Each slide's title and text, as Markdown; with `shapes`, each text
    shape by index and name, as `set-text` names them. `slides` ("1-5,9") picks slides; without
    it a deck longer than SLIDES_SHOWN prints its first ones and then every later slide's title."""
    prs = Presentation(path)
    total = len(prs.slides)
    if slides:
        wanted = set(_slide_spec(slides, total))
    else:
        wanted = set(range(1, min(total, SLIDES_SHOWN) + 1))
    out = []
    later = []
    for n, slide in enumerate(prs.slides, start=1):
        if n not in wanted:
            if not slides:
                t = _title_shape(slide)
                later.append(f"- Slide {n}: " + (t.text_frame.text.strip() if t is not None else "(no title)"))
            continue
        title_shape = _title_shape(slide)
        title = title_shape.text_frame.text.strip() if title_shape is not None else ""
        out.append(f"## Slide {n}" + (f": {title}" if title else ""))
        if shapes:
            for i, shape in enumerate(slide.shapes):
                text = shape.text_frame.text.strip().replace("\n", " / ") if shape.has_text_frame else ""
                out.append(f"- #{i} {shape.name}" + (f": {text}" if text else ""))
            out.append("")
            continue
        for shape in slide.shapes:
            if not shape.has_text_frame or shape == title_shape or shape.name.startswith("Decor"):
                continue
            for p in shape.text_frame.paragraphs:
                text = "".join(r.text for r in p.runs).strip()
                if text:
                    out.append("  " * p.level + "- " + text)
        if slide.has_notes_slide and slide.notes_slide.notes_text_frame.text.strip():
            out.append("> Notes: " + slide.notes_slide.notes_text_frame.text.strip())
        out.append("")
    if later:
        out += [f"… slides {SLIDES_SHOWN + 1}-{total} by title (read them with --slides "
                f"{SLIDES_SHOWN + 1}-{min(total, 2 * SLIDES_SHOWN)}):"] + later
    return "\n".join(out) + "\n"


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
    i = sub.add_parser("init"); i.add_argument("out")
    i.add_argument("--theme", choices=sorted(deck_design.THEMES))
    i.add_argument("--audience", help="who it is for, in words; picks the theme when --theme is not given")
    i.add_argument("--title"); i.add_argument("--subtitle"); i.add_argument("--tag")
    i.add_argument("--style", choices=["light", "dark"], help="dark: a deep cover with the title large")
    a = sub.add_parser("add"); a.add_argument("file"); a.add_argument("--json", required=True)
    a.add_argument("--theme", choices=sorted(deck_design.THEMES))
    r = sub.add_parser("read"); r.add_argument("file"); r.add_argument("--shapes", action="store_true")
    r.add_argument("--slides", help=f"slides to read, e.g. 1-5,9 (default: the first {SLIDES_SHOWN}, then titles)")
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
    if args.cmd == "init":
        theme = args.theme or deck_design.for_audience(args.audience)
        init(args.out, theme, args.title, args.subtitle, args.tag, args.style)
        print(f"wrote {args.out}: its {theme} title slide; add slides with `add`")
    elif args.cmd == "add":
        spec = _spec(args.json)
        specs = spec if isinstance(spec, list) else [spec]
        for one in specs:
            n = add(args.file, one, args.theme)
        print(f"added slide(s) up to {n}; saved {args.file}")
    elif args.cmd == "read":
        sys.stdout.write(read(args.file, args.shapes, args.slides))
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
