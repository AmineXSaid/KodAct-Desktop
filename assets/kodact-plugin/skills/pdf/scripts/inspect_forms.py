#!/usr/bin/env python3
"""Route PDF form type (AcroForm vs XFA vs flat) and extract field metadata + widget rects."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def inspect_pdf_forms(pdf_path: Path) -> dict:
    """Return form classification and detailed field/widget metadata."""
    if not pdf_path.is_file():
        raise FileNotFoundError(f"Input PDF not found: {pdf_path}")

    from pypdf import PdfReader

    reader = PdfReader(str(pdf_path))
    root = reader.trailer.get("/Root", {})
    acro = root.get("/AcroForm")
    has_xfa = False
    if acro is not None:
        acro_obj = acro.get_object()
        has_xfa = "/XFA" in acro_obj

    # Map widget annotations -> (page_number, rect)
    widget_locations: dict[str, list[dict]] = {}
    widget_states: dict[str, list[str]] = {}
    for p_idx, page in enumerate(reader.pages, start=1):
        annots = page.get("/Annots") or []
        for annot_ref in annots:
            annot = annot_ref.get_object()
            if annot.get("/Subtype") != "/Widget":
                continue
            name = annot.get("/T")
            parent = annot.get("/Parent")
            if not name and parent is not None:
                name = parent.get_object().get("/T")
            if not name:
                continue
            fname = str(name)
            rect = [round(float(v), 2) for v in (annot.get("/Rect") or [0, 0, 0, 0])]
            widget_locations.setdefault(fname, []).append({"page": p_idx, "rect_pt": rect})

            ap = annot.get("/AP")
            if ap and "/N" in ap.get_object():
                n_obj = ap.get_object()["/N"].get_object()
                if hasattr(n_obj, "keys"):
                    states = [str(k) for k in n_obj.keys()]
                    widget_states[fname] = sorted(set(widget_states.get(fname, []) + states))

    raw_fields = reader.get_fields() or {}
    fields: list[dict] = []
    for name, field in raw_fields.items():
        info: dict = {
            "name": name,
            "type": str(field.get("/FT", "")),
            "value": str(field.get("/V", "")) if field.get("/V") is not None else None,
            "widgets": widget_locations.get(name, []),
        }
        if "/Opt" in field:
            info["options"] = [str(opt) for opt in field["/Opt"]]
        states = field.get("/_States_") or widget_states.get(name)
        if states:
            info["states"] = [str(s) for s in states]
        fields.append(info)

    if fields:
        mode = "fillable_acroform"
    elif has_xfa:
        mode = "xfa_form"
    else:
        mode = "flat_non_fillable"

    pages_meta = [
        {
            "page": idx + 1,
            "width_pt": round(float(p.mediabox.width), 2),
            "height_pt": round(float(p.mediabox.height), 2),
        }
        for idx, p in enumerate(reader.pages)
    ]

    return {
        "file": str(pdf_path),
        "form_mode": mode,
        "field_count": len(fields),
        "pages": pages_meta,
        "fields": fields,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check if a PDF has fillable fields and inspect field types/states/rects."
    )
    parser.add_argument("input_pdf", type=Path, help="Path to source .pdf file")
    parser.add_argument("--json", dest="json_out", type=Path, help="Write report to JSON file")
    args = parser.parse_args()

    try:
        report = inspect_pdf_forms(args.input_pdf)
    except ImportError:
        print("Error: 'pypdf' is required. Install with: pip install pypdf", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(
            f"Form mode: {report['form_mode']} ({report['field_count']} fields) -> {args.json_out}"
        )
        return 0

    print(f"File: {report['file']}")
    print(f"Form Mode: {report['form_mode']} ({report['field_count']} field(s))")
    if report["form_mode"] == "flat_non_fillable":
        print("-> No interactive AcroForm fields. Use scripts/fill_flat_form.py with bounding boxes.")
    for f in report["fields"]:
        extra = ""
        if "options" in f:
            extra = f" options={f['options']}"
        elif "states" in f:
            extra = f" states={f['states']}"
        locs = ", ".join(f"p{w['page']}:{w['rect_pt']}" for w in f["widgets"])
        print(f"- {f['name']} [{f['type']}] value={f['value']!r}{extra} ({locs})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
