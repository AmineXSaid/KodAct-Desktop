#!/usr/bin/env python3
"""Make a PDF, designed, in one command: from Markdown, or from a Word, PowerPoint or Excel file.

    python make_pdf.py from-markdown NOTES.md OUT.pdf [--audience WORDS | --theme NAME] [--title T] [--subtitle S] [--keep-docx]
    python make_pdf.py from-office IN.docx|IN.pptx|IN.xlsx OUT.pdf

`from-markdown` writes the document with the docx skill's designed tool (a cover band in the
theme picked from who it is for, Word's own heading styles, real tables and lists, page numbers),
then prints it to PDF. `--keep-docx` also keeps that .docx beside the PDF, to edit and print again.

`from-office` prints an existing file. On Windows with Microsoft Office installed the PDF comes
from Word, PowerPoint or Excel themselves (the file looks exactly as it does there); otherwise
from LibreOffice. The input is never changed.

Then look at the result: `python render_preview.py OUT.pdf --grid`.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHARED = HERE.parents[2] / "scripts"
DOCX_TOOL = HERE.parents[1] / "docx" / "scripts" / "docx_tool.py"
if str(SHARED) not in sys.path:
    sys.path.insert(0, str(SHARED))

from _kodact_office import convert_office_to_pdf  # noqa: E402

OFFICE = (".docx", ".doc", ".rtf", ".pptx", ".ppt", ".xlsx", ".xls")


def from_office(src: Path, out: Path) -> Path:
    if src.suffix.lower() not in OFFICE:
        raise SystemExit(f"{src.name}: not a Word, PowerPoint or Excel file ({', '.join(OFFICE)})")
    if not src.is_file():
        raise SystemExit(f"{src}: no such file")
    with tempfile.TemporaryDirectory() as tmp:
        pdf = convert_office_to_pdf(src, Path(tmp), timeout=180)
        out.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(pdf, out)
    return out


def _docx_tool(*args: str) -> None:
    r = subprocess.run([sys.executable, str(DOCX_TOOL), *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit((r.stderr or r.stdout).strip() or f"docx_tool {args[0]} failed")


def from_markdown(md: Path, out: Path, title: str | None, subtitle: str | None,
                  theme: str | None, audience: str | None, keep_docx: bool) -> Path:
    if not md.is_file():
        raise SystemExit(f"{md}: no such file")
    text = md.read_text(encoding="utf-8")
    # The first "# Heading" is the title when none is given, and leaves the body (it is the cover).
    if not title:
        lines = text.splitlines()
        for i, line in enumerate(lines):
            if line.startswith("# "):
                title = line[2:].strip()
                text = "\n".join(lines[:i] + lines[i + 1:])
                break
    title = title or md.stem.replace("-", " ").replace("_", " ").title()
    with tempfile.TemporaryDirectory() as tmp:
        body = Path(tmp) / "body.md"
        body.write_text(text, encoding="utf-8")
        docx = Path(tmp) / f"{out.stem}.docx"
        init = ["init", str(docx), "--title", title]
        if subtitle:
            init += ["--subtitle", subtitle]
        if theme:
            init += ["--theme", theme]
        elif audience:
            init += ["--audience", audience]
        _docx_tool(*init)
        _docx_tool("append", str(docx), str(docx), "--markdown", str(body))
        from_office(docx, out)
        if keep_docx:
            shutil.copyfile(docx, out.with_suffix(".docx"))
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("from-markdown")
    m.add_argument("markdown", type=Path)
    m.add_argument("out", type=Path)
    m.add_argument("--title")
    m.add_argument("--subtitle")
    g = m.add_mutually_exclusive_group()
    g.add_argument("--theme")
    g.add_argument("--audience")
    m.add_argument("--keep-docx", action="store_true")
    o = sub.add_parser("from-office")
    o.add_argument("input", type=Path)
    o.add_argument("out", type=Path)
    args = ap.parse_args(argv)
    if args.out.suffix.lower() != ".pdf":
        raise SystemExit(f"{args.out}: the output must end in .pdf")
    try:
        if args.cmd == "from-office":
            done = from_office(args.input, args.out)
        else:
            done = from_markdown(args.markdown, args.out, args.title, args.subtitle,
                                 args.theme, args.audience, args.keep_docx)
    except (FileNotFoundError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1
    size = os.path.getsize(done)
    print(f"wrote {done} ({size:,} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
