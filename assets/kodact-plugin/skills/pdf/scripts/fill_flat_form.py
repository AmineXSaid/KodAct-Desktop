#!/usr/bin/env python3
"""Validate bounding boxes and stamp text/checkmarks onto flat (non-fillable) PDF forms."""

from __future__ import annotations

import argparse
import io
import json
from pathlib import Path
import sys


def _to_bottom_left_pt(
    box: list[float],
    page_w: float,
    page_h: float,
    origin: str,
    unit: str,
) -> tuple[float, float, float, float]:
    """Convert [x0, y0, x1, y1] into bottom-left PDF points (x0, y0_bl, x1, y1_bl)."""
    if len(box) != 4:
        raise ValueError(f"Bounding box must have 4 numbers [x0, y0, x1, y1], got {box}")
    x0, y0, x1, y1 = (float(v) for v in box)
    if unit == "normalized":
        x0, x1 = x0 * page_w, x1 * page_w
        y0, y1 = y0 * page_h, y1 * page_h
    if x1 <= x0 or y1 <= y0:
        raise ValueError(f"Invalid box dimensions (x1<=x0 or y1<=y0): {box}")

    if origin == "top-left":
        return (x0, page_h - y1, x1, page_h - y0)
    return (x0, y0, x1, y1)


def _boxes_overlap(
    a: tuple[float, float, float, float], b: tuple[float, float, float, float]
) -> bool:
    return min(a[2], b[2]) - max(a[0], b[0]) > 1.0 and min(a[3], b[3]) - max(a[1], b[1]) > 1.0


def validate_and_resolve_spec(
    spec: dict, page_sizes: list[tuple[float, float]]
) -> tuple[list[dict], list[str], list[str]]:
    """Return (resolved_fields, errors, warnings)."""
    origin = spec.get("coordinate_origin", "top-left")
    unit = spec.get("unit", "normalized")
    if origin not in ("top-left", "bottom-left"):
        raise ValueError("coordinate_origin must be 'top-left' or 'bottom-left'")
    if unit not in ("normalized", "pt"):
        raise ValueError("unit must be 'normalized' (0..1) or 'pt'")

    errors: list[str] = []
    warnings: list[str] = []
    resolved: list[dict] = []
    by_page: dict[int, list[tuple[str, tuple[float, float, float, float]]]] = {}

    for idx, item in enumerate(spec.get("fields", []), start=1):
        name = item.get("name", f"field_{idx}")
        page_num = int(item.get("page", 1))
        if not (1 <= page_num <= len(page_sizes)):
            errors.append(f"{name}: page {page_num} out of range (1..{len(page_sizes)})")
            continue

        pw, ph = page_sizes[page_num - 1]
        try:
            ebox = _to_bottom_left_pt(item["entry_box"], pw, ph, origin, unit)
        except Exception as exc:
            errors.append(f"{name}: {exc}")
            continue

        lbox = None
        if "label_box" in item and item["label_box"]:
            try:
                lbox = _to_bottom_left_pt(item["label_box"], pw, ph, origin, unit)
            except Exception as exc:
                errors.append(f"{name} label_box: {exc}")
                continue
            if _boxes_overlap(ebox, lbox):
                errors.append(f"{name}: entry_box overlaps its own label_box.")

        if ebox[0] < -1 or ebox[1] < -1 or ebox[2] > pw + 1 or ebox[3] > ph + 1:
            errors.append(f"{name}: entry_box {ebox} extends outside page {page_num} ({pw}x{ph} pt)")

        kind = item.get("kind", "text")
        val = str(item.get("value", ""))
        font_size = float(item.get("font_size", 10))
        box_w, box_h = ebox[2] - ebox[0], ebox[3] - ebox[1]

        if kind == "text":
            if box_h < font_size * 1.05:
                errors.append(
                    f"{name}: entry_box height ({box_h:.1f}pt) too short for font_size={font_size}pt"
                )
            est_text_w = len(val) * font_size * 0.52
            if est_text_w > box_w + 2:
                warnings.append(
                    f"{name}: text {val!r} (~{est_text_w:.0f}pt) may exceed box width ({box_w:.0f}pt)"
                )

        for other_name, other_box in by_page.get(page_num, []):
            if _boxes_overlap(ebox, other_box):
                errors.append(f"Page {page_num}: '{name}' overlaps '{other_name}'.")
        by_page.setdefault(page_num, []).append((name, ebox))
        if lbox is not None:
            by_page[page_num].append((f"{name}[label]", lbox))

        resolved.append(
            {
                "name": name,
                "page": page_num,
                "entry_box": ebox,
                "label_box": lbox,
                "kind": kind,
                "value": val,
                "font_size": font_size,
                "align": item.get("align", "left"),
            }
        )

    return resolved, errors, warnings


def stamp_flat_form(
    pdf_path: Path,
    resolved_fields: list[dict],
    output_pdf: Path,
    debug_boxes: bool = False,
) -> None:
    from pypdf import PdfReader, PdfWriter
    from reportlab.lib import colors
    from reportlab.pdfgen import canvas

    reader = PdfReader(str(pdf_path))
    writer = PdfWriter()

    fields_by_page: dict[int, list[dict]] = {}
    for f in resolved_fields:
        fields_by_page.setdefault(f["page"], []).append(f)

    for p_idx, page in enumerate(reader.pages, start=1):
        p_fields = fields_by_page.get(p_idx)
        if p_fields:
            pw = float(page.mediabox.width)
            ph = float(page.mediabox.height)
            buf = io.BytesIO()
            c = canvas.Canvas(buf, pagesize=(pw, ph))

            for f in p_fields:
                x0, y0, x1, y1 = f["entry_box"]
                bw, bh = x1 - x0, y1 - y0
                fs = f["font_size"]

                if debug_boxes:
                    c.saveState()
                    c.setStrokeColor(colors.HexColor("#DC2626"))
                    c.setLineWidth(0.8)
                    c.rect(x0, y0, bw, bh, stroke=1, fill=0)
                    c.setFont("Helvetica", 6)
                    c.setFillColor(colors.HexColor("#DC2626"))
                    c.drawString(x0, y1 + 1.5, f["name"])
                    if f["label_box"]:
                        lx0, ly0, lx1, ly1 = f["label_box"]
                        c.setStrokeColor(colors.HexColor("#2563EB"))
                        c.rect(lx0, ly0, lx1 - lx0, ly1 - ly0, stroke=1, fill=0)
                    c.restoreState()

                c.saveState()
                c.setFillColor(colors.HexColor("#0F172A"))
                if f["kind"] == "check":
                    if f["value"].lower() not in ("", "false", "0", "off", "no"):
                        mark_fs = min(bw, bh) * 0.85
                        c.setFont("Helvetica-Bold", mark_fs)
                        c.drawCentredString(x0 + bw / 2, y0 + (bh - mark_fs * 0.75) / 2, "X")
                else:
                    c.setFont("Helvetica", fs)
                    baseline_y = y0 + max(1.5, (bh - fs * 0.8) / 2)
                    if f["align"] == "center":
                        c.drawCentredString(x0 + bw / 2, baseline_y, f["value"])
                    elif f["align"] == "right":
                        c.drawRightString(x1 - 2, baseline_y, f["value"])
                    else:
                        c.drawString(x0 + 2, baseline_y, f["value"])
                c.restoreState()

            c.save()
            buf.seek(0)
            overlay_page = PdfReader(buf).pages[0]
            page.merge_page(overlay_page)

        writer.add_page(page)

    output_pdf.parent.mkdir(parents=True, exist_ok=True)
    with output_pdf.open("wb") as fh:
        writer.write(fh)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate bounding boxes and fill a flat (non-fillable) PDF form."
    )
    parser.add_argument("input_pdf", type=Path, help="Path to flat .pdf form")
    parser.add_argument("spec_json", type=Path, help="Bounding-box & values JSON spec")
    parser.add_argument("-o", "--out", type=Path, help="Output filled .pdf path")
    parser.add_argument(
        "--debug-out",
        type=Path,
        help="Optional debug .pdf drawing red/blue bounding boxes for visual verification",
    )
    parser.add_argument(
        "--check-only",
        action="store_true",
        help="Only validate bounding boxes without writing PDF output",
    )
    args = parser.parse_args()

    try:
        from pypdf import PdfReader

        if not args.input_pdf.is_file():
            raise FileNotFoundError(f"Input PDF not found: {args.input_pdf}")
        reader = PdfReader(str(args.input_pdf))
        page_sizes = [
            (float(p.mediabox.width), float(p.mediabox.height)) for p in reader.pages
        ]
        spec = json.loads(args.spec_json.read_text(encoding="utf-8"))
        resolved, errors, warnings = validate_and_resolve_spec(spec, page_sizes)
    except ImportError as exc:
        print(
            f"Error: 'pypdf' and 'reportlab' are required ({exc}).",
            file=sys.stderr,
        )
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    for w in warnings:
        print(f"WARNING: {w}", file=sys.stderr)
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)

    if errors:
        print(f"Bounding-box validation FAILED ({len(errors)} error(s)).")
        return 1

    print(f"Bounding-box validation PASSED ({len(resolved)} field(s), {len(warnings)} warning(s)).")
    if args.check_only:
        return 0

    try:
        if args.debug_out:
            stamp_flat_form(args.input_pdf, resolved, args.debug_out, debug_boxes=True)
            print(f"Wrote debug bounding-box overlay -> {args.debug_out}")
        if args.out:
            stamp_flat_form(args.input_pdf, resolved, args.out, debug_boxes=False)
            print(f"Wrote filled flat PDF -> {args.out}")
    except ImportError:
        print(
            "Error: 'reportlab' is required to stamp PDF overlays (pip install reportlab).",
            file=sys.stderr,
        )
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
