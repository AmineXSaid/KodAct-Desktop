#!/usr/bin/env python3
"""Validate a .pptx: OOXML integrity and relationships, then what a reader would see: content cut off
by the slide edge, text overflowing its box, text printed over text, an off-centre title, empty
placeholders, tiny type and walls of text. Errors are things that look broken; warnings are
things worth a look. Decoration (shapes without text) may bleed off the slide on purpose."""

from __future__ import annotations

import argparse
from pathlib import Path
import posixpath
import sys
import xml.etree.ElementTree as ET
import zipfile

# The shared modules (textfit, ooxml_schema) in kodact-plugin/scripts.
_SHARED = Path(__file__).resolve().parents[3] / "scripts"
if str(_SHARED) not in sys.path:
    sys.path.insert(0, str(_SHARED))

REQUIRED_PARTS = (
    "[Content_Types].xml",
    "_rels/.rels",
    "ppt/presentation.xml",
)
EMU_PER_INCH = 914400


def _overlap_area(a: tuple[int, int, int, int], b: tuple[int, int, int, int]) -> int:
    ax1, ay1, aw, ah = a
    bx1, by1, bw, bh = b
    ix1 = max(ax1, bx1)
    iy1 = max(ay1, by1)
    ix2 = min(ax1 + aw, bx1 + bw)
    iy2 = min(ay1 + ah, by1 + bh)
    if ix2 <= ix1 or iy2 <= iy1:
        return 0
    return (ix2 - ix1) * (iy2 - iy1)


def _check_rels(names: set[str], rels_path: str, root: ET.Element, errors: list[str]) -> None:
    rels_dir = posixpath.dirname(rels_path)
    base_dir = "" if rels_dir == "_rels" else posixpath.dirname(rels_dir)
    for rel in root:
        if rel.tag.rsplit("}", 1)[-1] != "Relationship":
            continue
        if rel.attrib.get("TargetMode", "").lower() == "external":
            continue
        target = rel.attrib.get("Target", "")
        if not target:
            continue
        resolved = (
            target.lstrip("/")
            if target.startswith("/")
            else posixpath.normpath(posixpath.join(base_dir, target))
        )
        if resolved not in names:
            errors.append(f"Broken relationship in {rels_path}: '{target}' -> '{resolved}' missing")


def _schema_problems(parts_or_path):
    """Problems against the Office Open XML schemas (kodact-plugin/scripts/ooxml_schema.py)."""
    shared = Path(__file__).resolve().parents[3] / "scripts"
    if str(shared) not in sys.path:
        sys.path.insert(0, str(shared))
    try:
        import ooxml_schema
    except ImportError as exc:  # lxml missing: the structural checks still ran
        return [], [f"schema check skipped: {exc}"]
    if isinstance(parts_or_path, dict):
        return ooxml_schema.check_parts(parts_or_path), []
    return ooxml_schema.check(parts_or_path), []


def validate_pptx(pptx_path: Path) -> tuple[list[str], list[str]]:
    if not pptx_path.is_file():
        raise FileNotFoundError(f"Input PPTX not found: {pptx_path}")

    errors: list[str] = []
    warnings: list[str] = []

    with zipfile.ZipFile(pptx_path, "r") as zf:
        names = {i.filename for i in zf.infolist() if not i.is_dir()}
        for req in REQUIRED_PARTS:
            if req not in names:
                errors.append(f"Missing required OOXML part: {req}")
        for name in names:
            if name.endswith((".xml", ".rels")):
                try:
                    root = ET.fromstring(zf.read(name))
                    if name.endswith(".rels"):
                        _check_rels(names, name, root, errors)
                except ET.ParseError as exc:
                    errors.append(f"Malformed XML in {name}: {exc}")

    if not errors:
        found, notes = _schema_problems(pptx_path)
        errors.extend(f"Schema: {p}" for p in found)
        warnings.extend(notes)

    try:
        from pptx import Presentation
    except ImportError:
        warnings.append("'python-pptx' not installed; skipped slide geometry checks.")
        return errors, warnings

    prs = Presentation(str(pptx_path))
    sw, sh = prs.slide_width, prs.slide_height
    tol = int(EMU_PER_INCH * 0.05)
    body = _body_font(prs)
    for s_idx, slide in enumerate(prs.slides, start=1):
        _check_slide(slide, s_idx, sw, sh, tol, errors, warnings, body)
    return errors, warnings



# What a reader sees: a shape with text, a picture, a chart or a table. A shape without any of
# these is decoration (a corner blob, an accent bar, a card's background) and may bleed off the
# slide on purpose; only content that leaves the canvas is cut off.
def _content_kind(shape) -> str | None:
    if shape.has_text_frame and shape.text_frame.text.strip():
        return "text"
    if getattr(shape, "has_chart", False) and shape.has_chart:
        return "chart"
    if getattr(shape, "has_table", False) and shape.has_table:
        return "table"
    if shape.shape_type is not None and int(shape.shape_type) == 13:  # MSO_SHAPE_TYPE.PICTURE
        return "picture"
    return None


def _text_height_in(shape, body_font: str) -> float | None:
    """The height in inches the shape's text needs at its fonts and sizes, wrapped to its width
    (textfit.py: measured with the font itself where installed); None when a run's size isn't set
    on the slide (it comes from the layout, unknown here)."""
    import textfit

    tf = shape.text_frame
    m = lambda v, d: (v if v is not None else d) / EMU_PER_INCH  # noqa: E731
    width = (shape.width or 0) / EMU_PER_INCH - m(tf.margin_left, 91440) - m(tf.margin_right, 91440)
    if width <= 0.1:
        return None
    total = m(tf.margin_top, 45720) + m(tf.margin_bottom, 45720)
    for p in tf.paragraphs:
        runs = [r for r in p.runs if r.text]
        if any(r.font.size is None for r in runs):
            return None
        if not runs:
            total += 12 * 1.15 / 72
            continue
        # The paragraph's largest run sets its line height; its widest face, its wrapping.
        big = max(runs, key=lambda r: r.font.size.pt)
        family = big.font.name or body_font
        if tf.word_wrap is False:
            total += big.font.size.pt * 1.15 / 72
            continue
        total += textfit.height(p.text, family, big.font.size.pt, width, bool(big.font.bold), 1.15)
    return total


def _body_font(prs) -> str:
    """The deck theme's body font (its minor Latin typeface), which runs without a font use."""
    from lxml import etree

    for rel in prs.slide_master.part.rels.values():
        if rel.reltype.endswith("/theme"):
            root = etree.fromstring(rel.target_part.blob)
            el = root.find(".//{http://schemas.openxmlformats.org/drawingml/2006/main}minorFont/"
                           "{http://schemas.openxmlformats.org/drawingml/2006/main}latin")
            if el is not None and el.get("typeface"):
                return el.get("typeface")
    return "Calibri"


def _autofits(shape) -> bool:
    """PowerPoint shrinks this text to its box (normAutofit) or grows the box (spAutoFit)."""
    body = shape.text_frame._txBody
    return any(el.tag.endswith(("}normAutofit", "}spAutoFit")) for el in body.iter())


def _check_slide(slide, s_idx, sw, sh, tol, errors, warnings, body_font="Calibri") -> None:
    where = f"Slide {s_idx}"
    texts: list[tuple[str, tuple[int, int, int, int]]] = []
    for shape in slide.shapes:
        l, t, w, h = shape.left or 0, shape.top or 0, shape.width or 0, shape.height or 0
        kind = _content_kind(shape)
        if kind is None:
            if shape.is_placeholder and shape.has_text_frame:
                warnings.append(f"{where}: empty placeholder '{shape.name}' (delete it, or it shows "
                                "'Click to add text' to whoever edits the deck).")
            continue
        if l < -tol or t < -tol or l + w > sw + tol or t + h > sh + tol:
            errors.append(f"{where}: {kind} '{shape.name}' runs off the slide: it is cut off.")
        if kind != "text":
            continue
        texts.append((shape.name, (l, t, w, h)))
        paras = [p for p in shape.text_frame.paragraphs if p.text.strip()]
        if len(paras) > 8:
            warnings.append(f"{where}: '{shape.name}' has {len(paras)} paragraphs (a wall of text; aim for <= 6).")
        tiny = [r.font.size.pt for p in paras for r in p.runs if r.font.size is not None and r.font.size.pt < 9.0]
        if tiny:
            warnings.append(f"{where}: '{shape.name}' uses tiny text ({min(tiny):.1f}pt < 9pt).")
        # A glyph or a number in a badge (a 140pt quote mark, "3") is drawn, not read as lines.
        need = _text_height_in(shape, body_font) if len("".join(shape.text_frame.text.split())) > 3 else None
        have = h / EMU_PER_INCH
        if need is not None and have > 0 and need > have * 1.2 + 0.1:
            msg = (f"{where}: the text of '{shape.name}' needs about {need:.1f}in of height but its box "
                   f"is {have:.1f}in: it overflows the box")
            if _autofits(shape):
                warnings.append(msg + " (PowerPoint shrinks it to fit, LibreOffice may not).")
            else:
                errors.append(msg + ". Shorten it, enlarge the box or lower the size.")
        # A centred title placed for another slide size: python-pptx's default template is laid out
        # for 4:3, so on a 16:9 slide its title sits left of centre.
        if shape.is_placeholder and paras and getattr(shape.placeholder_format.type, "name", "") in ("TITLE", "CENTER_TITLE"):
            off = abs((l + w / 2) - sw / 2) / max(1, sw)
            if off > 0.08:
                warnings.append(f"{where}: title '{shape.name}' sits {off:.0%} off the slide's centre "
                                "(a layout made for another slide size); move or resize it.")
    for i in range(len(texts)):
        for j in range(i + 1, len(texts)):
            (n1, b1), (n2, b2) = texts[i], texts[j]
            area = _overlap_area(b1, b2)
            share = area / max(1, min(b1[2] * b1[3], b2[2] * b2[3]))
            if share > 0.3:
                errors.append(f"{where}: text shapes '{n1}' and '{n2}' overlap ({share:.0%}): words print over words.")
            elif share > 0.15:
                warnings.append(f"{where}: text shapes '{n1}' and '{n2}' overlap ({share:.0%}).")

def main() -> int:
    parser = argparse.ArgumentParser(description="Validate a .pptx file and its slide hygiene.")
    parser.add_argument("input_pptx", type=Path, help="Path to .pptx file")
    args = parser.parse_args()

    try:
        errors, warnings = validate_pptx(args.input_pptx)
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    for w in warnings:
        print(f"WARNING: {w}", file=sys.stderr)
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)

    if errors:
        print(f"Validation FAILED ({len(errors)} error(s), {len(warnings)} warning(s)).")
        return 1

    print(f"Validation PASSED ({len(warnings)} warning(s)).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
