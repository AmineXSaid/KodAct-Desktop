#!/usr/bin/env python3
"""Recalculate .xlsx formulas (Excel on Windows when installed, else an isolated LibreOffice) and audit
for errors, uncomputed formulas, _xlfn traps and hardcoded totals."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
import zipfile
import warnings

# openpyxl drops the extension part of conditional formats a recalculation added (the rule itself
# stays); its warning about that says nothing the person can act on.
warnings.filterwarnings("ignore", message=r".*extension is not supported.*")

EXCEL_ERROR_PREFIXES = (
    "#REF!",
    "#DIV/0!",
    "#VALUE!",
    "#NAME?",
    "#N/A",
    "#NUM!",
    "#NULL!",
    "Err:",
)

XLFN_FUNCTIONS = (
    "XLOOKUP",
    "XMATCH",
    "FILTER",
    "UNIQUE",
    "SORT",
    "SORTBY",
    "SEQUENCE",
    "LET",
    "TEXTSPLIT",
    "VSTACK",
    "HSTACK",
)
XLFN_REGEX = re.compile(
    r"(?<!_xlfn\.)\b(" + "|".join(XLFN_FUNCTIONS) + r")\s*\(", re.IGNORECASE
)
TOTAL_LABEL_REGEX = re.compile(r"^\s*(sub)?total\b|\bnet\s+total\b", re.IGNORECASE)

SHARED_SCRIPTS = Path(__file__).resolve().parents[3] / "scripts"
if str(SHARED_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SHARED_SCRIPTS))

from _kodact_office import recalc_xlsx  # noqa: E402
import ooxml_schema  # noqa: E402


def _patch_full_calc_on_load(xlsx_path: Path) -> None:
    """Ensure xl/workbook.xml sets fullCalcOnLoad=1 without altering drawings/charts."""
    main_ns = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    ET.register_namespace("", main_ns)

    with zipfile.ZipFile(xlsx_path, "r") as zin:
        entries = {info.filename: zin.read(info.filename) for info in zin.infolist()}

    wb_xml = entries.get("xl/workbook.xml")
    if not wb_xml:
        return

    root = ET.fromstring(wb_xml)
    calc_pr = root.find(f"{{{main_ns}}}calcPr")
    if calc_pr is None:
        calc_pr = ET.SubElement(root, f"{{{main_ns}}}calcPr")
    calc_pr.set("calcMode", "auto")
    calc_pr.set("fullCalcOnLoad", "1")
    calc_pr.set("forceFullCalc", "1")
    entries["xl/workbook.xml"] = ET.tostring(root, encoding="utf-8", xml_declaration=True)

    with zipfile.ZipFile(xlsx_path, "w", compression=zipfile.ZIP_DEFLATED) as zout:
        for name, data in entries.items():
            zout.writestr(name, data)


def recalculate_xlsx(xlsx_path: Path, output_path: Path, timeout: int = 120) -> Path:
    """Recalculate every formula (Excel on Windows when installed, else an isolated LibreOffice),
    after marking the workbook for a full calculation on load, so stale cached values go too."""
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp) / xlsx_path.name
        shutil.copy2(xlsx_path, work)
        _patch_full_calc_on_load(work)
        return recalc_xlsx(work, output_path, timeout=timeout)


def audit_workbook(xlsx_path: Path) -> dict:
    """Scan recalculated workbook for Excel errors, missing _xlfn. prefixes, and hardcoded totals."""
    from openpyxl import load_workbook
    from openpyxl.utils import get_column_letter

    wb_vals = load_workbook(xlsx_path, data_only=True, read_only=True)
    wb_form = load_workbook(xlsx_path, data_only=False, read_only=True)
    errors: list[dict] = []
    warnings: list[dict] = []
    formula_count = 0
    uncomputed: list[dict] = []

    try:
        for name in wb_vals.sheetnames:
            ws_v = wb_vals[name]
            ws_f = wb_form[name]

            rows_v = list(ws_v.iter_rows(values_only=True))
            rows_f = list(ws_f.iter_rows(values_only=True))
            max_rows = max(len(rows_v), len(rows_f))

            for r_idx in range(max_rows):
                rv = rows_v[r_idx] if r_idx < len(rows_v) else ()
                rf = rows_f[r_idx] if r_idx < len(rows_f) else ()
                max_cols = max(len(rv), len(rf))

                # Check if row looks like a Total/Subtotal row based on first 2 columns
                first_labels = " ".join(
                    str(rf[c]) for c in range(min(2, len(rf))) if isinstance(rf[c], str)
                )
                is_total_row = bool(TOTAL_LABEL_REGEX.search(first_labels))

                for c_idx in range(max_cols):
                    val = rv[c_idx] if c_idx < len(rv) else None
                    raw_f = rf[c_idx] if c_idx < len(rf) else None
                    coord = f"{get_column_letter(c_idx + 1)}{r_idx + 1}"

                    if isinstance(raw_f, str) and raw_f.startswith("="):
                        formula_count += 1
                        if val is None:
                            uncomputed.append({"type": "not_computed", "sheet": name, "cell": coord,
                                               "detail": f"No computed value for {raw_f}"})
                        m = XLFN_REGEX.search(raw_f)
                        if m:
                            warnings.append(
                                {
                                    "type": "missing_xlfn_prefix",
                                    "sheet": name,
                                    "cell": coord,
                                    "detail": (
                                        f"Formula uses '{m.group(1)}' without '_xlfn.' prefix "
                                        f"(may open as #NAME? in Excel): {raw_f}"
                                    ),
                                }
                            )
                    elif is_total_row and c_idx >= 1 and isinstance(raw_f, (int, float)):
                        warnings.append(
                            {
                                "type": "hardcoded_total",
                                "sheet": name,
                                "cell": coord,
                                "detail": f"Static number ({raw_f}) in Total row instead of =SUM(...) formula.",
                            }
                        )

                    if isinstance(val, str) and any(
                        val.startswith(prefix) for prefix in EXCEL_ERROR_PREFIXES
                    ):
                        errors.append(
                            {
                                "type": "formula_error",
                                "sheet": name,
                                "cell": coord,
                                "value": val,
                                "formula": raw_f if isinstance(raw_f, str) else "",
                            }
                        )
    finally:
        wb_vals.close()
        wb_form.close()

    # No formula has a value: the workbook was never calculated (openpyxl writes none), so it shows
    # empty cells and no error could have been found. That is a failure, not a pass. A few empty
    # results among computed ones can be formulas that give "" on purpose: a warning each.
    # A workbook Excel would repair or refuse, whatever its values.
    for problem in ooxml_schema.check(xlsx_path):
        errors.append({"type": "schema", "sheet": "", "cell": "", "detail": problem})

    if formula_count and len(uncomputed) == formula_count:
        errors.append({"type": "not_recalculated", "sheet": "", "cell": "",
                       "detail": f"None of the {formula_count} formula(s) has a computed value: "
                                 "the workbook was not recalculated, so its errors are unknown."})
    else:
        warnings.extend(uncomputed)

    return {
        "file": str(xlsx_path),
        "status": "failed" if errors else "passed",
        "formula_count": formula_count,
        "error_count": len(errors),
        "warning_count": len(warnings),
        "errors": errors,
        "warnings": warnings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Force-recalculate formulas via isolated LibreOffice and audit workbook quality."
    )
    parser.add_argument("input_xlsx", type=Path, help="Path to .xlsx workbook")
    parser.add_argument(
        "--in-place",
        action="store_true",
        help="Overwrite input_xlsx with recalculated cached values",
    )
    parser.add_argument("-o", "--out", type=Path, help="Write recalculated workbook to new path")
    parser.add_argument(
        "--scan-only",
        action="store_true",
        help="Skip LibreOffice recalculation and only audit existing cached values",
    )
    parser.add_argument("--json", action="store_true", help="Print structured JSON report")
    parser.add_argument("--timeout", type=int, default=90, help="LibreOffice timeout in seconds")
    args = parser.parse_args()

    if not args.input_xlsx.is_file():
        print(f"Error: Input file not found: {args.input_xlsx}", file=sys.stderr)
        return 1

    target = args.input_xlsx
    try:
        if not args.scan_only:
            dest = args.input_xlsx if args.in_place else (args.out or args.input_xlsx)
            target = recalculate_xlsx(args.input_xlsx, dest, timeout=args.timeout)
            if not args.json:
                print(f"Recalculated formulas -> {target}")

        report = audit_workbook(target)
    except ImportError:
        print("Error: 'openpyxl' is required. Install with: pip install openpyxl", file=sys.stderr)
        return 1
    except subprocess.CalledProcessError as exc:
        print(f"LibreOffice failed: {exc.stderr or exc}", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(report, indent=2))
        return 1 if report["errors"] else 0

    for w in report["warnings"]:
        print(f"WARNING [{w['sheet']}!{w['cell']}]: {w['detail']}", file=sys.stderr)
    for err in report["errors"]:
        print(
            (f"ERROR: {err['detail']}" if err.get("type") in ("not_recalculated", "schema")
             else f"ERROR [{err['sheet']}!{err['cell']}]: {err['value']} (formula={err['formula']!r})"),
            file=sys.stderr,
        )

    if report["errors"]:
        print(
            f"Workbook audit FAILED ({report['error_count']} error(s), "
            f"{report['warning_count']} warning(s) across {report['formula_count']} formula(s))."
        )
        return 1

    print(
        f"Workbook audit PASSED ({report['formula_count']} formula(s), 0 errors, "
        f"{report['warning_count']} warning(s))."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
