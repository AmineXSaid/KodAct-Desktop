"""Check a file before handing it back: is it sound, and does it look right?

    python check_tool.py validate FILE [FILE ...]
    python check_tool.py render FILE OUT_DIR [--dpi 80] [--pages 1-3]

`validate` opens a .docx, .xlsx, .pptx or .pdf the way an app would: the
zip and every XML part in it parse, the parts the format needs are there,
every relationship points at a part that exists, and the library for its
kind loads it. It prints `ok` per file, or what is wrong, and exits 1 on
any problem.

`render` draws the file's pages (slides, sheets) as PNG images, through
LibreOffice for Office files, so you can look at what you made before
replying: text that runs off a slide, a table cut in two, an empty page.
It prints each image's path.
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
import xml.etree.ElementTree as ET

NEEDS = {
    ".docx": ("[Content_Types].xml", "word/document.xml"),
    ".xlsx": ("[Content_Types].xml", "xl/workbook.xml"),
    ".pptx": ("[Content_Types].xml", "ppt/presentation.xml"),
}
REL_NS = "{http://schemas.openxmlformats.org/package/2006/relationships}"


def _rels_problems(z, names):
    """Relationships whose internal target is not in the package."""
    out = []
    for rels in (n for n in names if n.endswith(".rels")):
        folder = os.path.dirname(os.path.dirname(rels))
        for rel in ET.fromstring(z.read(rels)).iter(REL_NS + "Relationship"):
            if rel.get("TargetMode") == "External":
                continue
            target = rel.get("Target", "")
            path = target.lstrip("/") if target.startswith("/") else os.path.normpath(os.path.join(folder, target)).replace(os.sep, "/")
            if path not in names:
                out.append(f"{rels} points at {target}, which is missing")
    return out


def validate(path):
    """What is wrong with the file; empty when it is sound."""
    ext = os.path.splitext(path)[1].lower()
    if not os.path.isfile(path):
        return ["there is no such file"]
    if ext == ".pdf":
        try:
            from pypdf import PdfReader
            pages = len(PdfReader(path).pages)
        except Exception as e:  # noqa: BLE001 - any failure to open is the answer
            return [f"pypdf cannot open it: {e}"]
        return [] if pages else ["it has no pages"]
    if ext not in NEEDS:
        return [f"validate checks .docx, .xlsx, .pptx and .pdf, not {ext or 'this file'}"]
    try:
        z = zipfile.ZipFile(path)
    except zipfile.BadZipFile:
        return ["it is not a zip, so not an Office file"]
    problems = []
    with z:
        names = set(z.namelist())
        problems += [f"{n} is missing" for n in NEEDS[ext] if n not in names]
        for n in sorted(names):
            if n.endswith(".xml") or n.endswith(".rels"):
                try:
                    ET.fromstring(z.read(n))
                except ET.ParseError as e:
                    problems.append(f"{n} is not well-formed XML: {e}")
        if not problems:
            problems += _rels_problems(z, names)
    if problems:
        return problems
    try:
        if ext == ".docx":
            import docx
            docx.Document(path)
        elif ext == ".xlsx":
            import openpyxl
            openpyxl.load_workbook(path)
        else:
            import pptx
            pptx.Presentation(path)
    except Exception as e:  # noqa: BLE001
        problems.append(f"it does not load: {e}")
    return problems


def _soffice():
    found = shutil.which("soffice") or shutil.which("libreoffice")
    for p in (r"C:\Program Files\LibreOffice\program\soffice.exe", "/Applications/LibreOffice.app/Contents/MacOS/soffice"):
        if not found and os.path.exists(p):
            found = p
    return found


def to_pdf(path, tmp):
    """The file as a PDF in `tmp` (itself, if it is one)."""
    if path.lower().endswith(".pdf"):
        return path
    soffice = _soffice()
    if not soffice:
        raise SystemExit("render needs LibreOffice (soffice) for Office files")
    profile = "file://" + os.path.join(tmp, "profile").replace(os.sep, "/")
    subprocess.run([soffice, f"-env:UserInstallation={profile}", "--headless", "--convert-to", "pdf", "--outdir", tmp, path],
                   check=True, capture_output=True, timeout=120)
    pdf = os.path.join(tmp, os.path.splitext(os.path.basename(path))[0] + ".pdf")
    if not os.path.exists(pdf):
        raise SystemExit("LibreOffice could not convert it")
    return pdf


def render(path, out_dir, dpi=80, pages=None):
    """PNG images of the pages, `name-1.png`, … in `out_dir`."""
    if not shutil.which("pdftoppm"):
        raise SystemExit("render needs pdftoppm (poppler-utils)")
    os.makedirs(out_dir, exist_ok=True)
    stem = os.path.join(out_dir, os.path.splitext(os.path.basename(path))[0])
    with tempfile.TemporaryDirectory() as tmp:
        cmd = ["pdftoppm", "-png", "-r", str(dpi)]
        if pages:
            first, _, last = pages.partition("-")
            cmd += ["-f", first, "-l", last or first]
        subprocess.run(cmd + [to_pdf(os.path.abspath(path), tmp), stem], check=True, capture_output=True, timeout=120)
    prefix = os.path.basename(stem) + "-"
    return sorted((os.path.join(out_dir, f) for f in os.listdir(out_dir) if f.startswith(prefix) and f.endswith(".png")),
                  key=lambda p: int(p.rsplit("-", 1)[1][:-4]))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    v = sub.add_parser("validate"); v.add_argument("files", nargs="+")
    r = sub.add_parser("render"); r.add_argument("file"); r.add_argument("out_dir")
    r.add_argument("--dpi", type=int, default=80); r.add_argument("--pages")
    args = ap.parse_args(argv)
    if args.cmd == "validate":
        bad = False
        for f in args.files:
            problems = validate(f)
            bad |= bool(problems)
            print(f"{f}: ok" if not problems else f"{f}:\n  " + "\n  ".join(problems))
        sys.exit(1 if bad else 0)
    else:
        for p in render(args.file, args.out_dir, args.dpi, args.pages):
            print(p)


if __name__ == "__main__":
    main()
