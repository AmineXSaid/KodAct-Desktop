---
name: xlsx
description: >-
  Comprehensive workflow for inspecting, creating, editing, force-recalculating
  via isolated LibreOffice, auditing models (formula errors, missing _xlfn.
  prefixes, hardcoded totals), and visually verifying Excel (.xlsx) workbooks
  with Poppler. Use whenever working with .xlsx spreadsheets or financial models.
---

# XLSX Spreadsheet Engineering Skill

Use this skill for all `.xlsx` workflows: inspecting existing workbooks, building formula-driven financial models and reports with `openpyxl`, force-recalculating cached values in an isolated LibreOffice session, auditing model hygiene, and running visual QA.

---

## 1. Decision Tree

```text
What is the task?
├── Read / Inspect / Analyze
│   ├── Structure, sheets, headers, formula count ──▶ scripts/inspect_workbook.py
│   └── Tabular data aggregation / stats          ──▶ pandas.read_excel() or openpyxl(data_only=True)
│
├── Create New Workbook / Financial Model
│   └── openpyxl (live formulas, color coding, explicit formats, auto-fit, print-fit)
│       ──▶ scripts/recalc_and_verify.py --in-place --json
│       ──▶ scripts/render_preview.py --grid ──▶ inspect PNGs
│
└── Edit Existing Workbook
    └── scripts/inspect_workbook.py (map sheets, headers, existing formulas first)
        ──▶ openpyxl.load_workbook() (preserve existing sheets/formulas)
        ──▶ scripts/recalc_and_verify.py --in-place --json (forces .calculateAll() on stale cache)
        ──▶ scripts/render_preview.py --grid ──▶ inspect PNGs
```

---

## 2. Formula-First & Financial Modeling Standards

1. **Always Use Live Excel Formulas (Zero Hardcoded Totals)**:
   - Write `=SUM(B2:B10)`, `=AVERAGE(...)`, `=SUMIFS(...)`, `=IFERROR(..., 0)`—never compute totals in Python and write static numbers. (`recalc_and_verify.py` automatically flags static numbers sitting on `Total` / `Subtotal` rows!)
   - Prefer classic cross-version functions (`INDEX`/`MATCH`, `SUMIFS`, `COUNTIFS`). If you must use `XLOOKUP`, `XMATCH`, `FILTER`, or `UNIQUE` in `openpyxl`, prefix with `_xlfn.` (`=_xlfn.XLOOKUP(...)`) or Excel will display `#NAME?`.
2. **Financial Modeling Color Convention**:
   - **Blue (`#0000FF`)**: Hardcoded inputs & assumptions.
   - **Black (`#000000`)**: Same-sheet formulas.
   - **Green (`#008000`)**: Cross-sheet formula links (`='Assumptions'!B4`).
3. **Explicit Number Formats**:
   - Currency: `$#,##0;($#,##0);"-"` | Percentages: `0.0%` | Counts: `#,##0` | Dates: `yyyy-mm-dd`.
4. **Prevent `###` Truncation & Orphan Print Columns**:
   - Estimate display width (don't size formula columns by the raw formula string length; give currency/formula columns `>= 14–18` width — see [openpyxl_patterns.md](./references/openpyxl_patterns.md)).
   - Freeze header rows (`ws.freeze_panes = "A2"`) and configure landscape `fitToWidth = 1`, `fitToHeight = 0`.

---

## 3. Mandatory Verification & QA Loop

1. **Force Recalculate & Audit Model**:
   ```bash
   python scripts/recalc_and_verify.py output.xlsx --in-place --json
   ```
   - Patches `fullCalcOnLoad="1"` and invokes `doc.calculateAll()` inside an **isolated temporary LibreOffice profile** (never mutating the user's global LibreOffice config or blocking on open GUI windows).
   - Audits every cell for `#REF!`, `#DIV/0!`, `#VALUE!`, `#NAME?`, `#N/A`, `#NUM!`, `#NULL!`, `Err:`, missing `_xlfn.` prefixes, and hardcoded numbers in `Total` rows.
2. **Render Visual Preview**:
   ```bash
   python scripts/render_preview.py output.xlsx --out-dir ./preview --dpi 150 --grid
   ```
3. **Visual & Data Inspection Checklist**:
   - [ ] `error_count == 0` and `warning_count == 0` in `recalc_and_verify.py`.
   - [ ] Zero `###` truncated numbers or clipped headers in `grid-*.png` / `page-*.png`.
   - [ ] Clean alignment, borders, number formats, and single-page-wide print layout.

---

## 4. Helper Scripts Reference

| Script | Purpose | Usage |
| :--- | :--- | :--- |
| [inspect_workbook.py](./scripts/inspect_workbook.py) | Summarize sheets, dimensions, formula counts, sample rows | `python scripts/inspect_workbook.py book.xlsx` |
| [recalc_and_verify.py](./scripts/recalc_and_verify.py) | Isolated `.calculateAll()` recalc + error/`_xlfn`/total audit | `python scripts/recalc_and_verify.py book.xlsx --in-place --json` |
| [render_preview.py](./scripts/render_preview.py) | Isolated LibreOffice + Poppler PNGs & `--grid` sheets | `python scripts/render_preview.py book.xlsx --grid` |
