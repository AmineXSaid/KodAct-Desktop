#!/usr/bin/env python3
"""Validate .pptx OOXML integrity, relationships, slide bounds, overlaps, font legibility, and bullet density."""

from __future__ import annotations

import argparse
from pathlib import Path
import posixpath
import sys
import xml.etree.ElementTree as ET
import zipfile

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

    try:
        from pptx import Presentation

        prs = Presentation(str(pptx_path))
        sw, sh = prs.slide_width, prs.slide_height
        slide_area = max(1, sw * sh)

        for s_idx, slide in enumerate(prs.slides, start=1):
            text_boxes: list[tuple[str, tuple[int, int, int, int]]] = []
            for shape in slide.shapes:
                l, t, w, h = (
                    shape.left or 0,
                    shape.top or 0,
                    shape.width or 0,
                    shape.height or 0,
                )
                tol = int(EMU_PER_INCH * 0.05)
                if l < -tol or t < -tol or l + w > sw + tol or t + h > sh + tol:
                    warnings.append(
                        f"Slide {s_idx}: '{shape.name}' extends outside slide canvas."
                    )
                if shape.has_text_frame:
                    paras = [p for p in shape.text_frame.paragraphs if p.text.strip()]
                    if paras:
                        if w * h < slide_area * 0.85:
                            text_boxes.append((shape.name, (l, t, w, h)))
                        if len(paras) > 8:
                            warnings.append(
                                f"Slide {s_idx}: '{shape.name}' has {len(paras)} paragraphs (dense text wall; aim for <= 6)."
                            )
                        for p in paras:
                            for r in p.runs:
                                if r.font.size is not None and r.font.size.pt < 9.0:
                                    warnings.append(
                                        f"Slide {s_idx}: '{shape.name}' uses tiny font ({r.font.size.pt:.1f}pt < 9pt)."
                                    )
                                    break

            for i in range(len(text_boxes)):
                for j in range(i + 1, len(text_boxes)):
                    n1, b1 = text_boxes[i]
                    n2, b2 = text_boxes[j]
                    area = _overlap_area(b1, b2)
                    min_box = max(1, min(b1[2] * b1[3], b2[2] * b2[3]))
                    if area / min_box > 0.15:
                        warnings.append(
                            f"Slide {s_idx}: text shapes '{n1}' and '{n2}' overlap ({int(area / min_box * 100)}%)."
                        )
    except ImportError:
        warnings.append("'python-pptx' not installed; skipped slide geometry checks.")

    return errors, warnings


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
