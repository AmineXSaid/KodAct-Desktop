#!/usr/bin/env python3
"""Fill interactive AcroForm fields in a PDF with full /AS checkbox state sync for Poppler QA."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def _normalize_btn_state(raw_val: object, allowed_states: list[str]) -> str:
    on_states = [s for s in allowed_states if s != "/Off"]
    default_on = on_states[0] if on_states else "/Yes"
    if isinstance(raw_val, bool):
        return default_on if raw_val else "/Off"
    s = str(raw_val).strip()
    if s.lower() in ("false", "off", "/off", "0", "no", ""):
        return "/Off"
    if s.startswith("/") and (not allowed_states or s in allowed_states):
        return s
    if f"/{s}" in allowed_states:
        return f"/{s}"
    if s.lower() in ("true", "yes", "/yes", "1", "on", "checked"):
        return default_on
    raise ValueError(f"Invalid button value {raw_val!r}; allowed states: {allowed_states}")


def fill_pdf_form(pdf_path: Path, values: dict[str, object], output_pdf: Path) -> int:
    """Validate and populate AcroForm fields, syncing widget /AS states for checkboxes/radios."""
    if not pdf_path.is_file():
        raise FileNotFoundError(f"Input PDF not found: {pdf_path}")

    from pypdf import PdfReader, PdfWriter
    from pypdf.generic import BooleanObject, NameObject, TextStringObject

    reader = PdfReader(str(pdf_path))
    known_fields = reader.get_fields() or {}
    if not known_fields:
        raise ValueError(
            "PDF has no fillable AcroForm fields. Use scripts/fill_flat_form.py instead."
        )

    unknown = [k for k in values if k not in known_fields]
    if unknown:
        raise KeyError(
            f"Unknown field name(s): {unknown}. Run inspect_forms.py to list exact names."
        )

    writer = PdfWriter()
    writer.append(reader)

    acroform = writer._root_object.get("/AcroForm")
    if acroform is not None:
        acroform.get_object()[NameObject("/NeedAppearances")] = BooleanObject(True)

    text_updates: dict[str, str] = {}
    btn_updates: dict[str, str] = {}

    for fname, val in values.items():
        meta = known_fields[fname]
        ftype = str(meta.get("/FT", ""))
        if ftype == "/Btn":
            states = [str(s) for s in (meta.get("/_States_") or [])]
            btn_updates[fname] = _normalize_btn_state(val, states)
        else:
            if ftype == "/Ch" and "/Opt" in meta:
                opts = [str(o) for o in meta["/Opt"]]
                if str(val) not in opts:
                    raise ValueError(
                        f"Value {val!r} for choice field '{fname}' not in options: {opts}"
                    )
            text_updates[fname] = str(val)

    for page in writer.pages:
        if text_updates:
            writer.update_page_form_field_values(page, text_updates, auto_regenerate=False)
        annots = page.get("/Annots") or []
        for annot_ref in annots:
            annot = annot_ref.get_object()
            if annot.get("/Subtype") != "/Widget":
                continue
            name = annot.get("/T")
            parent_obj = annot.get("/Parent").get_object() if annot.get("/Parent") else None
            if not name and parent_obj is not None:
                name = parent_obj.get("/T")
            if not name:
                continue
            fname = str(name)
            if fname in btn_updates:
                target_state = NameObject(btn_updates[fname])
                # Check widget's own /AP /N keys so radio group children get /Off when unselected.
                ap = annot.get("/AP")
                widget_on_keys: set[str] = set()
                if ap and "/N" in ap.get_object():
                    n_dict = ap.get_object()["/N"].get_object()
                    if hasattr(n_dict, "keys"):
                        widget_on_keys = {str(k) for k in n_dict.keys()}
                widget_as = (
                    target_state
                    if (not widget_on_keys or str(target_state) in widget_on_keys)
                    else NameObject("/Off")
                )
                annot[NameObject("/AS")] = widget_as
                if parent_obj is not None:
                    parent_obj[NameObject("/V")] = target_state
                else:
                    annot[NameObject("/V")] = target_state
            elif fname in text_updates:
                txt_obj = TextStringObject(text_updates[fname])
                if parent_obj is not None:
                    parent_obj[NameObject("/V")] = txt_obj
                else:
                    annot[NameObject("/V")] = txt_obj

    output_pdf.parent.mkdir(parents=True, exist_ok=True)
    with output_pdf.open("wb") as fh:
        writer.write(fh)

    return len(values)


def main() -> int:
    parser = argparse.ArgumentParser(description="Fill PDF AcroForm fields from a JSON file.")
    parser.add_argument("input_pdf", type=Path, help="Path to source fillable .pdf")
    parser.add_argument("values_json", type=Path, help="Path to JSON file mapping field -> value")
    parser.add_argument("-o", "--out", type=Path, required=True, help="Destination .pdf path")
    args = parser.parse_args()

    try:
        values = json.loads(args.values_json.read_text(encoding="utf-8"))
        if not isinstance(values, dict):
            raise ValueError("values_json must contain a JSON object mapping field names to values.")
        count = fill_pdf_form(args.input_pdf, values, args.out)
    except ImportError:
        print("Error: 'pypdf' is required. Install with: pip install pypdf", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(f"Applied {count} field value(s) -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
