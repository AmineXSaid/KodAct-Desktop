#!/usr/bin/env python3
"""Inspect sheets, dimensions, headers, and formula counts in an .xlsx workbook."""

from __future__ import annotations

import warnings

# openpyxl drops the extension part of conditional formats a recalculation added (the rule itself
# stays); its warning about that says nothing the person can act on.
warnings.filterwarnings("ignore", message=r".*extension is not supported.*")


import argparse
import json
from pathlib import Path
import sys


def inspect_workbook(xlsx_path: Path, sample_rows: int = 3) -> dict:
    if not xlsx_path.is_file():
        raise FileNotFoundError(f"Input workbook not found: {xlsx_path}")

    from openpyxl import load_workbook

    wb = load_workbook(xlsx_path, data_only=False, read_only=True)
    sheets_info = []

    try:
        for name in wb.sheetnames:
            ws = wb[name]
            formula_count = 0
            non_empty = 0
            preview: list[list[str]] = []

            for r_idx, row in enumerate(ws.iter_rows(values_only=True), start=1):
                row_vals = [str(v) if v is not None else "" for v in row]
                if any(row_vals) and len(preview) < sample_rows:
                    preview.append(row_vals[:10])
                for val in row:
                    if val is not None:
                        non_empty += 1
                        if isinstance(val, str) and val.startswith("="):
                            formula_count += 1

            sheets_info.append(
                {
                    "name": name,
                    "max_row": ws.max_row,
                    "max_column": ws.max_column,
                    "non_empty_cells": non_empty,
                    "formula_cells": formula_count,
                    "sample_rows": preview,
                }
            )
    finally:
        wb.close()

    return {"file": str(xlsx_path), "sheet_count": len(sheets_info), "sheets": sheets_info}


def main() -> int:
    parser = argparse.ArgumentParser(description="Inspect an .xlsx workbook structure.")
    parser.add_argument("input_xlsx", type=Path, help="Path to .xlsx file")
    parser.add_argument("--json", dest="json_out", type=Path, help="Write JSON report to file")
    args = parser.parse_args()

    try:
        info = inspect_workbook(args.input_xlsx)
    except ImportError:
        print("Error: 'openpyxl' is required. Install with: pip install openpyxl", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(info, indent=2), encoding="utf-8")
        print(f"Wrote workbook inspection to {args.json_out}")
        return 0

    print(f"Workbook: {info['file']} ({info['sheet_count']} sheet(s))")
    for s in info["sheets"]:
        print(
            f"\n  Sheet: {s['name']!r} ({s['max_row']}x{s['max_column']}, "
            f"cells={s['non_empty_cells']}, formulas={s['formula_cells']})"
        )
        for r in s["sample_rows"]:
            print(f"    {r}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
