"""PowerPoint decks: read, create from an outline, replace text.

    python pptx_tool.py read FILE
    python pptx_tool.py create OUT.pptx --outline FILE.md
    python pptx_tool.py replace FILE OUT.pptx --find TEXT --replace TEXT [...]

An outline is Markdown: each "# Title" starts a slide (the first is the
title slide, with the paragraph under it as its subtitle); "- " lines are
its bullets, indented two spaces per level.
"""

import argparse
import re
import sys

from pptx import Presentation


def read(path):
    """Each slide's title and text, as Markdown."""
    prs = Presentation(path)
    out = []
    for n, slide in enumerate(prs.slides, start=1):
        title = slide.shapes.title.text_frame.text.strip() if slide.shapes.title is not None and slide.shapes.title.has_text_frame else ""
        out.append(f"## Slide {n}" + (f": {title}" if title else ""))
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
        for j, (level, text) in enumerate(s["bullets"]):
            p = body.paragraphs[0] if j == 0 else body.add_paragraph()
            p.text = text
            p.level = min(level, 4)
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
    prs.save(out)
    return total


def _text(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("read"); r.add_argument("file")
    c = sub.add_parser("create"); c.add_argument("out"); c.add_argument("--outline", required=True)
    p = sub.add_parser("replace"); p.add_argument("file"); p.add_argument("out")
    p.add_argument("--find", action="append", required=True); p.add_argument("--replace", action="append", required=True)
    args = ap.parse_args(argv)
    if args.cmd == "read":
        sys.stdout.write(read(args.file))
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
