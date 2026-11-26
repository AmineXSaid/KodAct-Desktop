"""Check a .docx, .pptx or .xlsx against the Office Open XML schemas (ECMA-376, transitional), as an
Office app reads it.

    python ooxml_schema.py FILE [FILE ...]      prints "ok" per file, or each part's problems; exit 1

    from ooxml_schema import check
    problems = check("deck.pptx")               # ["ppt/slides/slide2.xml:14: Element ...", ...]

Each XML part is matched to its schema by the content type the package gives it. Before checking,
the part is read the way the standard's Markup Compatibility rules (ECMA-376 Part 3) tell an app to
read it: markup in a namespace the part declares ignorable (w14, a14, p14 and the like: features of
later Office versions) is dropped, and each mc:AlternateContent keeps its mc:Fallback. What remains
must be valid against the schema, or Word, PowerPoint or Excel may refuse the file ("found a problem
with its content") or repair it silently.

Parts with no schema here (properties, custom XML, media) are left to the structural checks.
"""

from __future__ import annotations

import sys
import zipfile
from functools import lru_cache
from pathlib import Path

from lxml import etree

HERE = Path(__file__).resolve().parent
SCHEMAS = HERE / "schemas" / "ooxml"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
CT_NS = "{http://schemas.openxmlformats.org/package/2006/content-types}"
MOST = 20  # problems reported per part

# Content type (its tail after the vendor prefix) → schema file.
BY_TYPE = {
    "wordprocessingml.document.main+xml": "wml", "wordprocessingml.template.main+xml": "wml",
    "wordprocessingml.styles+xml": "wml", "wordprocessingml.numbering+xml": "wml",
    "wordprocessingml.settings+xml": "wml", "wordprocessingml.footnotes+xml": "wml",
    "wordprocessingml.endnotes+xml": "wml", "wordprocessingml.comments+xml": "wml",
    "wordprocessingml.header+xml": "wml", "wordprocessingml.footer+xml": "wml",
    "wordprocessingml.fontTable+xml": "wml", "wordprocessingml.webSettings+xml": "wml",
    "presentationml.presentation.main+xml": "pml", "presentationml.slide+xml": "pml",
    "presentationml.slideLayout+xml": "pml", "presentationml.slideMaster+xml": "pml",
    "presentationml.notesSlide+xml": "pml", "presentationml.notesMaster+xml": "pml",
    "presentationml.handoutMaster+xml": "pml", "presentationml.presProps+xml": "pml",
    "presentationml.viewProps+xml": "pml", "presentationml.tableStyles+xml": "pml",
    "presentationml.commentAuthors+xml": "pml", "presentationml.comments+xml": "pml",
    "spreadsheetml.sheet.main+xml": "sml", "spreadsheetml.template.main+xml": "sml",
    "spreadsheetml.worksheet+xml": "sml", "spreadsheetml.styles+xml": "sml",
    "spreadsheetml.sharedStrings+xml": "sml", "spreadsheetml.table+xml": "sml",
    "spreadsheetml.calcChain+xml": "sml", "spreadsheetml.comments+xml": "sml",
    "drawingml.chart+xml": "dml-chart", "theme+xml": "dml-main",
    "drawing+xml": "dml-spreadsheetDrawing", "extended-properties+xml": "shared-documentPropertiesExtended",
}
PREFIX = "application/vnd.openxmlformats-officedocument."


@lru_cache(maxsize=None)
def _schema(name: str) -> etree.XMLSchema:
    return etree.XMLSchema(etree.parse(str(SCHEMAS / f"{name}.xsd")))


def _types(files: dict[str, bytes]) -> dict[str, str]:
    """Part name (no leading slash) → content type, from [Content_Types].xml."""
    root = etree.fromstring(files["[Content_Types].xml"])
    defaults = {d.get("Extension", "").lower(): d.get("ContentType", "") for d in root.iter(f"{CT_NS}Default")}
    over = {o.get("PartName", "").lstrip("/"): o.get("ContentType", "") for o in root.iter(f"{CT_NS}Override")}
    out = {}
    for name in files:
        if name.endswith("/"):
            continue
        out[name] = over.get(name) or defaults.get(name.rsplit(".", 1)[-1].lower(), "")
    return out


def _schema_for(content_type: str) -> str | None:
    if not content_type.startswith(PREFIX):
        return None
    return BY_TYPE.get(content_type[len(PREFIX):].split(";")[0])


def understood(root) -> None:
    """Read `root` as an app that knows only the standard does (Markup Compatibility, in place):
    mc:AlternateContent becomes its mc:Fallback's content (or nothing), and elements and attributes
    in namespaces declared mc:Ignorable go."""
    ignorable: set[str] = set()
    for el in root.iter():
        if not isinstance(el.tag, str):
            continue
        val = el.get(f"{{{MC}}}Ignorable")
        if val:
            for prefix in val.split():
                ns = el.nsmap.get(prefix)
                if ns:
                    ignorable.add(ns)
    for alt in list(root.iter(f"{{{MC}}}AlternateContent")):
        parent = alt.getparent()
        if parent is None:
            continue
        fallback = alt.find(f"{{{MC}}}Fallback")
        at = parent.index(alt)
        for child in list(fallback) if fallback is not None else []:
            parent.insert(at, child)
            at += 1
        parent.remove(alt)
    for el in list(root.iter()):
        if not isinstance(el.tag, str):
            continue
        ns = etree.QName(el).namespace
        if (ns in ignorable or ns == MC) and el.getparent() is not None:
            el.getparent().remove(el)
            continue
        for attr in list(el.attrib):
            if attr.startswith("{"):
                ans = attr[1:].split("}", 1)[0]
                if ans in ignorable or ans == MC:
                    del el.attrib[attr]


XML_SPACE = "{http://www.w3.org/XML/1998/namespace}space"
CHART = "{http://schemas.openxmlformats.org/drawingml/2006/chart}"
SML = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"


def tolerate(schema: str, root) -> None:
    """Where the schemas and every Office app disagree, read the part as Office does, so the check
    flags what Office refuses and nothing it opens. Each gap is one Office writes or reads itself:

    - sml: Excel writes xml:space="preserve" on the <t> of every string with outer spaces; the
      ECMA-376 spreadsheet schema never declared it.
    - sml: workbookPr/@dateCompatibility is ISO/IEC 29500's (Excel reads it, LibreOffice writes it on
      every workbook it saves, so on every one the skills recalculate); this schema lacks it.
    - dml-chart: an axis id (c:axId, c:crossAx) is unsignedInt in the schema; PowerPoint, Excel and
      Word read it as a 32-bit number and open negative ids, which python-pptx writes. Read as
      Office reads them: the same 32 bits, unsigned."""
    if schema == "sml":
        for el in root.iter():
            if isinstance(el.tag, str) and XML_SPACE in el.attrib:
                del el.attrib[XML_SPACE]
        for el in root.iter(SML + "workbookPr"):
            el.attrib.pop("dateCompatibility", None)
    elif schema == "dml-chart":
        for tag in ("axId", "crossAx"):
            for el in root.iter(CHART + tag):
                v = el.get("val", "")
                if v.lstrip("-").isdigit() and v.startswith("-"):
                    el.set("val", str(int(v) & 0xFFFFFFFF))


def check(path) -> list[str]:
    """Every schema problem in the .docx/.pptx/.xlsx at `path`, as "part:line: message"; [] when
    it is valid."""
    with zipfile.ZipFile(path) as z:
        return check_parts({n: z.read(n) for n in z.namelist() if not n.endswith("/")})


def check_parts(files: dict[str, bytes]) -> list[str]:
    """The same for a package's parts already read (name → bytes), e.g. an unpacked folder."""
    problems: list[str] = []
    try:
        types = _types(files)
    except (KeyError, etree.XMLSyntaxError) as exc:
        return [f"[Content_Types].xml: {exc}"]
    for part, ctype in sorted(types.items()):
        name = _schema_for(ctype)
        if name is None:
            continue
        try:
            root = etree.fromstring(files[part])
        except etree.XMLSyntaxError as exc:
            problems.append(f"{part}: not well-formed XML: {exc}")
            continue
        understood(root)
        tolerate(name, root)
        schema = _schema(name)
        # Re-parse so line numbers and ids are fresh after the compatibility pass.
        doc = etree.fromstring(etree.tostring(root))
        if schema.validate(doc):
            continue
        seen = set()
        for err in schema.error_log:
            msg = err.message.replace("{http://schemas.openxmlformats.org/", "{…/")
            if msg in seen:
                continue
            seen.add(msg)
            problems.append(f"{part}:{err.line}: {msg}")
            if len(seen) >= MOST:
                problems.append(f"{part}: … more problems not shown")
                break
    return problems


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__.strip().splitlines()[0])
        return 2
    bad = 0
    for f in argv:
        found = check(f)
        if found:
            bad += 1
            print(f"{f}: {len(found)} schema problem(s)")
            for p in found:
                print(f"  {p}")
        else:
            print(f"{f}: ok")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
