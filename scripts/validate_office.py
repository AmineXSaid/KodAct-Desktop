#!/usr/bin/env python3
"""Is this Office file or PDF legitimate? A development check, not shipped.

    python3 -I scripts/validate_office.py FILE... [--original FILE] [--render DIR] [--recalc]

For .xlsx, .docx and .pptx:
  - the package: zip integrity, every part has a content type, every
    override and every internal relationship points at a part that exists,
    unique relationship ids, every XML part well-formed;
  - the schema: ECMA-376 / ISO 29500 transitional XSDs (.docx and .pptx
    through the document skills' validate.py, .xlsx with lxml here); with
    --original, only errors the original did not already have count;
  - it opens in openpyxl / python-docx / python-pptx, all the way through;
  - its properties are not a library's defaults ("python-docx", 2013).
For .pdf: a strict pypdf parse of every page, text, form fields and
annotations read back, pdfinfo, and fonts embedded (pdffonts).
--render DIR: LibreOffice converts each file to PDF and pdftoppm draws its
pages into DIR, to be looked at.
--recalc (.xlsx): LibreOffice recalculates the workbook and every formula's
saved value must match.

The schemas come from a local clone of github.com/anthropics/skills:
OOXML_SCHEMAS (default below) is its docx/scripts/office folder.
Exit status 1 if any check fails.
"""

import argparse
import os
import posixpath
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

OFFICE = os.environ.get("OOXML_SCHEMAS", "/home/user/anthropics/skills/skills/docx/scripts/office")
SCHEMAS = os.path.join(OFFICE, "schemas")
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
LIBRARY_AUTHORS = {"python-docx", "python-pptx", "openpyxl", "IronCalc User", "Steve Canny"}

PASS, WARN, FAIL = "PASS", "WARN", "FAIL"


class Report:
    def __init__(self, path):
        self.path = path
        self.rows = []

    def add(self, check, status, detail=""):
        self.rows.append((check, status, detail))

    @property
    def failed(self):
        return any(s == FAIL for _, s, _ in self.rows)

    def print(self):
        print(f"\n== {self.path}")
        for check, status, detail in self.rows:
            print(f"  {status}  {check}" + (f": {detail}" if detail else ""))


# --- The package ---------------------------------------------------------

def check_package(z, r):
    bad = z.testzip()
    r.add("zip integrity", FAIL if bad else PASS, f"bad entry {bad}" if bad else f"{len(z.namelist())} entries")
    names = [n for n in z.namelist() if not n.endswith("/")]
    if len(set(names)) != len(names):
        r.add("unique entry names", FAIL, "duplicates")
    unsafe = [n for n in names if n.startswith("/") or ".." in n.split("/")]
    if unsafe:
        r.add("entry paths", FAIL, ", ".join(unsafe[:3]))
    from lxml import etree

    broken = []
    for n in names:
        if n.endswith((".xml", ".rels", ".vml")):
            try:
                etree.fromstring(z.read(n))
            except etree.XMLSyntaxError as e:
                broken.append(f"{n}: {e}")
    r.add("XML well-formed", FAIL if broken else PASS, "; ".join(broken[:3]))

    if "[Content_Types].xml" not in names:
        r.add("content types", FAIL, "no [Content_Types].xml")
        return
    ct = etree.fromstring(z.read("[Content_Types].xml"))
    defaults = {d.get("Extension").lower() for d in ct.findall(f"{{{CT_NS}}}Default")}
    overrides = {o.get("PartName") for o in ct.findall(f"{{{CT_NS}}}Override")}
    untyped = [n for n in names if n != "[Content_Types].xml" and "/" + n not in overrides and n.rsplit(".", 1)[-1].lower() not in defaults]
    missing = [p for p in overrides if p.lstrip("/") not in names]
    r.add("every part has a content type", FAIL if untyped else PASS, ", ".join(untyped[:4]))
    r.add("every override names a part", FAIL if missing else PASS, ", ".join(sorted(missing)[:4]))

    dangling, dup_ids = [], []
    for n in names:
        if not n.endswith(".rels"):
            continue
        base = posixpath.dirname(posixpath.dirname(n))  # _rels/x.rels -> its part's folder
        root = etree.fromstring(z.read(n))
        ids = [rel.get("Id") for rel in root]
        if len(ids) != len(set(ids)):
            dup_ids.append(n)
        for rel in root:
            if rel.get("TargetMode") == "External":
                continue
            target = rel.get("Target", "")
            path = target.lstrip("/") if target.startswith("/") else posixpath.normpath(posixpath.join(base, target))
            if path not in names:
                dangling.append(f"{n} -> {target}")
    r.add("every relationship target exists", FAIL if dangling else PASS, "; ".join(dangling[:4]))
    if dup_ids:
        r.add("unique relationship ids", FAIL, ", ".join(dup_ids))


def check_properties(z, r):
    from lxml import etree

    if "docProps/core.xml" not in z.namelist():
        r.add("properties", WARN, "no docProps/core.xml")
        return
    core = etree.fromstring(z.read("docProps/core.xml"))
    text = {el.tag.split("}")[1]: (el.text or "").strip() for el in core}
    creator, modified_by = text.get("creator", ""), text.get("lastModifiedBy", "")
    created = text.get("created", "")
    problems = []
    if creator in LIBRARY_AUTHORS or modified_by in LIBRARY_AUTHORS:
        problems.append(f"a library's name as author (creator {creator!r}, lastModifiedBy {modified_by!r})")
    if created[:4].isdigit() and int(created[:4]) < 2015:
        problems.append(f"created {created} (a template's date)")
    r.add("properties", WARN if problems else PASS, "; ".join(problems) or f"creator {creator!r}, title {text.get('title', '')!r}, modified {text.get('modified', '')}")


# --- Schemas ---------------------------------------------------------------

XML_SPACE = "{http://www.w3.org/XML/1998/namespace}space"


def _strip_compat(root):
    """What a consumer may drop: mc:AlternateContent resolved to its
    Fallback, then everything in an mc:Ignorable namespace. Also xml:space,
    which Excel itself writes on <t> though sml.xsd types <t> as a bare
    string (the XML spec reserves the attribute for every element)."""
    from lxml import etree

    for el in root.iter():
        if isinstance(el.tag, str) and XML_SPACE in el.attrib:
            del el.attrib[XML_SPACE]
    for alt in list(root.iter(f"{{{MC}}}AlternateContent")):
        parent = alt.getparent()
        fallback = alt.find(f"{{{MC}}}Fallback")
        i = parent.index(alt)
        parent.remove(alt)
        if fallback is not None:
            for k, child in enumerate(list(fallback)):
                parent.insert(i + k, child)
    ign = root.get(f"{{{MC}}}Ignorable")
    if ign:
        nss = {root.nsmap.get(p) for p in ign.split() if root.nsmap.get(p)}
        for el in list(root.iter()):
            if not isinstance(el.tag, str):
                continue
            for a in list(el.attrib):
                if a.startswith("{") and a[1:].split("}")[0] in nss:
                    del el.attrib[a]
            if el.tag.startswith("{") and el.tag[1:].split("}")[0] in nss and el.getparent() is not None:
                el.getparent().remove(el)
        del root.attrib[f"{{{MC}}}Ignorable"]
    etree.cleanup_namespaces(root)
    return root


_SCHEMA_CACHE = {}


def _schema(name):
    from lxml import etree

    if name not in _SCHEMA_CACHE:
        _SCHEMA_CACHE[name] = etree.XMLSchema(etree.parse(os.path.join(SCHEMAS, name)))
    return _SCHEMA_CACHE[name]


XLSX_SCHEMAS = {
    "http://schemas.openxmlformats.org/spreadsheetml/2006/main": "ISO-IEC29500-4_2016/sml.xsd",
    "http://schemas.openxmlformats.org/drawingml/2006/chart": "ISO-IEC29500-4_2016/dml-chart.xsd",
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing": "ISO-IEC29500-4_2016/dml-spreadsheetDrawing.xsd",
    CT_NS: "ecma/fouth-edition/opc-contentTypes.xsd",
    REL_NS: "ecma/fouth-edition/opc-relationships.xsd",
}


def xlsx_schema_errors(path):
    """{(part, message)} of every sml/dml/opc schema error in the workbook."""
    from lxml import etree

    errors = set()
    with zipfile.ZipFile(path) as z:
        for name in sorted(z.namelist()):
            if not name.endswith((".xml", ".rels")):
                continue
            try:
                root = etree.fromstring(z.read(name))
            except etree.XMLSyntaxError:
                continue  # reported by the package check
            ns = root.tag[1:].split("}")[0] if root.tag.startswith("{") else ""
            if ns not in XLSX_SCHEMAS:
                continue
            schema = _schema(XLSX_SCHEMAS[ns])
            if not schema.validate(etree.ElementTree(_strip_compat(root))):
                for e in schema.error_log:
                    errors.add((name, re.sub(r"\{[^}]+\}", "", e.message)))
    return errors


def check_schema(path, kind, original, r):
    if not os.path.isdir(SCHEMAS):
        r.add("schema", WARN, f"no schemas at {SCHEMAS} (set OOXML_SCHEMAS)")
        return
    if kind == "xlsx":
        errors = xlsx_schema_errors(path)
        if original:
            errors -= xlsx_schema_errors(original)
        r.add("schema (sml, dml, opc)" + (" — new errors only" if original else ""), FAIL if errors else PASS,
              "; ".join(f"{p}: {m[:160]}" for p, m in sorted(errors)[:4]) + (f" (+{len(errors) - 4} more)" if len(errors) > 4 else ""))
        return
    cmd = [sys.executable, os.path.join(OFFICE, "validate.py"), os.path.abspath(path)] + (["--original", os.path.abspath(original)] if original else [])
    run = subprocess.run(cmd, capture_output=True, text=True, timeout=300, cwd=OFFICE)
    out = (run.stdout + run.stderr).strip()
    detail = " ".join(l.strip() for l in out.splitlines() if "error" in l.lower() or "Element" in l or "FAIL" in l)[:400]
    r.add(f"schema ({'wml' if kind == 'docx' else 'pml'}, dml, opc)" + (" — new errors only" if original else ""),
          PASS if run.returncode == 0 else FAIL, detail if run.returncode else out.splitlines()[-1] if out else "")


# --- Opening it ------------------------------------------------------------

def check_open(path, kind, r):
    try:
        if kind == "xlsx":
            import openpyxl

            wb = openpyxl.load_workbook(path)
            cells = sum(1 for ws in wb.worksheets for row in ws.iter_rows() for c in row if c.value is not None)
            vals = openpyxl.load_workbook(path, data_only=True)
            uncached = [f"{ws.title}!{c.coordinate}" for ws in wb.worksheets for row in ws.iter_rows() for c in row
                        if isinstance(c.value, str) and c.value.startswith("=") and vals[ws.title][c.coordinate].value is None]
            r.add("opens in openpyxl", PASS, f"{len(wb.sheetnames)} sheets, {cells:,} cells")
            r.add("formulas carry saved values", WARN if uncached else PASS,
                  f"{len(uncached)} without: {', '.join(uncached[:5])}" if uncached else "")
        elif kind == "docx":
            import docx

            d = docx.Document(path)
            r.add("opens in python-docx", PASS, f"{len(d.paragraphs)} paragraphs, {len(d.tables)} tables, {len(d.sections)} sections")
        elif kind == "pptx":
            from pptx import Presentation

            p = Presentation(path)
            ratio = p.slide_width / p.slide_height
            r.add("opens in python-pptx", PASS, f"{len(p.slides)} slides, {p.slide_width / 914400:.2f} x {p.slide_height / 914400:.2f} in")
            r.add("slide size", PASS if abs(ratio - 16 / 9) < 0.01 else WARN, "16:9" if abs(ratio - 16 / 9) < 0.01 else f"{ratio:.3f}:1, not 16:9")
    except Exception as e:  # noqa: BLE001 - any failure to open is the finding
        r.add(f"opens in its library", FAIL, f"{type(e).__name__}: {e}")


def check_pdf(path, r):
    from pypdf import PdfReader

    try:
        reader = PdfReader(path, strict=True)
        texts = [p.extract_text() or "" for p in reader.pages]
        r.add("strict parse (pypdf)", PASS, f"{len(reader.pages)} pages, {sum(len(t) for t in texts):,} characters of text")
        meta = reader.metadata or {}
        title, author = (meta.get("/Title") or "").strip(), (meta.get("/Author") or "").strip()
        r.add("Title and Author", PASS if title and author else WARN, f"Title {title!r}, Author {author!r}")
        fields = reader.get_fields() or {}
        if fields:
            r.add("form fields", PASS, ", ".join(f"{k}={(v.get('/V') if isinstance(v, dict) else v)!r}" for k, v in list(fields.items())[:6]))
        annots = sum(len(p.get("/Annots") or []) for p in reader.pages)
        if annots:
            r.add("annotations", PASS, f"{annots}")
    except Exception as e:  # noqa: BLE001
        r.add("strict parse (pypdf)", FAIL, f"{type(e).__name__}: {e}")
    if shutil.which("pdfinfo"):
        run = subprocess.run(["pdfinfo", path], capture_output=True, text=True, timeout=60)
        info = dict(l.split(":", 1) for l in run.stdout.splitlines() if ":" in l)
        r.add("pdfinfo", PASS if run.returncode == 0 else FAIL,
              f"PDF {info.get('PDF version', '?').strip()}, {info.get('Pages', '?').strip()} pages, {info.get('Page size', '?').strip()}" if run.returncode == 0 else run.stderr.strip()[:200])
    if shutil.which("pdffonts"):
        run = subprocess.run(["pdffonts", path], capture_output=True, text=True, timeout=60)
        rows = [l.split() for l in run.stdout.splitlines()[2:] if l.strip()]
        # Columns: name type [encoding] emb sub uni object ID; emb is 5th from the end.
        loose = [row[0] for row in rows if len(row) >= 6 and row[-5] == "no"]
        r.add("fonts embedded", WARN if loose else PASS, f"not embedded: {', '.join(loose)}" if loose else f"{len(rows)} fonts")


# --- LibreOffice -----------------------------------------------------------

def _soffice(args, tmp):
    soffice = shutil.which("soffice") or shutil.which("libreoffice")
    profile = "file://" + os.path.join(tmp, "lo-profile")
    return subprocess.run([soffice, f"-env:UserInstallation={profile}", "--headless", *args], capture_output=True, text=True, timeout=300)


def render(path, kind, out_dir, r):
    if not shutil.which("soffice"):
        r.add("LibreOffice render", WARN, "LibreOffice not installed")
        return
    os.makedirs(out_dir, exist_ok=True)
    stem = os.path.splitext(os.path.basename(path))[0] + "-" + kind
    with tempfile.TemporaryDirectory() as tmp:
        pdf = path
        if kind != "pdf":
            run = _soffice(["--convert-to", "pdf", "--outdir", tmp, path], tmp)
            pdf = os.path.join(tmp, os.path.splitext(os.path.basename(path))[0] + ".pdf")
            if run.returncode or not os.path.exists(pdf):
                r.add("LibreOffice opens and exports it", FAIL, (run.stderr or run.stdout).strip()[:200])
                return
            r.add("LibreOffice opens and exports it", PASS)
        subprocess.run(["pdftoppm", "-r", "60", "-png", pdf, os.path.join(out_dir, stem)], check=True, timeout=300)
    pngs = sorted(f for f in os.listdir(out_dir) if f.startswith(stem))
    r.add("rendered", PASS, f"{len(pngs)} page(s) in {out_dir}/{stem}-*.png")


def recalc_matches(path, r):
    """LibreOffice recalculates; every formula's saved value must agree."""
    import openpyxl

    if not shutil.which("soffice"):
        r.add("values match a recalculation", WARN, "LibreOffice not installed")
        return
    with tempfile.TemporaryDirectory() as tmp:
        src = os.path.join(tmp, "in.xlsx")
        shutil.copyfile(path, src)
        _soffice(["--calc", "--convert-to", "xlsx:Calc MS Excel 2007 XML", "--outdir", os.path.join(tmp, "out"), src], tmp)
        fresh = openpyxl.load_workbook(os.path.join(tmp, "out", "in.xlsx"), data_only=True)
    formulas = openpyxl.load_workbook(path)
    saved = openpyxl.load_workbook(path, data_only=True)
    checked, wrong = 0, []
    for ws in formulas.worksheets:
        if ws.title not in fresh.sheetnames:
            continue
        for row in ws.iter_rows():
            for c in row:
                if not (isinstance(c.value, str) and c.value.startswith("=")):
                    continue
                a, b = saved[ws.title][c.coordinate].value, fresh[ws.title][c.coordinate].value
                checked += 1
                same = (abs(a - b) <= 1e-9 * max(1, abs(a), abs(b))) if isinstance(a, (int, float)) and isinstance(b, (int, float)) else a == b
                if not same:
                    wrong.append(f"{ws.title}!{c.coordinate} saved {a!r}, LibreOffice {b!r} ({c.value[:40]})")
    r.add("values match a LibreOffice recalculation", FAIL if wrong else PASS,
          f"{len(wrong)} of {checked} differ: " + "; ".join(wrong[:4]) if wrong else f"{checked:,} formulas")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="+")
    ap.add_argument("--original", help="the file these were edited from: count only new schema errors")
    ap.add_argument("--render", metavar="DIR", help="draw every page into DIR through LibreOffice")
    ap.add_argument("--recalc", action="store_true", help="check .xlsx formula values against a LibreOffice recalculation")
    args = ap.parse_args(argv)
    failed = False
    for path in args.files:
        kind = path.rsplit(".", 1)[-1].lower()
        r = Report(path)
        if kind in ("xlsx", "docx", "pptx"):
            try:
                with zipfile.ZipFile(path) as z:
                    check_package(z, r)
                    check_properties(z, r)
            except zipfile.BadZipFile as e:
                r.add("zip", FAIL, str(e))
                r.print()
                failed = True
                continue
            check_schema(path, kind, args.original, r)
            check_open(path, kind, r)
            if kind == "xlsx" and args.recalc:
                recalc_matches(path, r)
        elif kind == "pdf":
            check_pdf(path, r)
        else:
            r.add("type", FAIL, "not .xlsx, .docx, .pptx or .pdf")
        if args.render:
            render(path, kind, args.render, r)
        r.print()
        failed |= r.failed
    print("\nFAILED" if failed else "\nAll checks passed.")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
