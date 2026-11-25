# OpenPyXL Engineering Reference Patterns

Reusable patterns for formula-driven `.xlsx` workbooks, dynamic function compatibility, and print-fit setup.

## 1. Styled Header, Formula Row & Number Formats

```python
from openpyxl import Workbook
from openpyxl.styles import Font, PatternFill, Alignment, Border, Side
from openpyxl.utils import get_column_letter

wb = Workbook()
ws = wb.active
ws.title = "Summary"

header_font = Font(name="Calibri", size=11, bold=True, color="FFFFFF")
header_fill = PatternFill("solid", fgColor="1E3A5F")
input_font = Font(name="Calibri", size=11, color="0000FF")
formula_font = Font(name="Calibri", size=11, color="000000")
link_font = Font(name="Calibri", size=11, color="008000")
total_font = Font(name="Calibri", size=11, bold=True, color="000000")
thin = Side(style="thin", color="CBD5E1")
double_bottom = Side(style="double", color="1E293B")

headers = ["Item", "Q1", "Q2", "Total"]
ws.append(headers)
for col_idx in range(1, len(headers) + 1):
    cell = ws.cell(row=1, column=col_idx)
    cell.font = header_font
    cell.fill = header_fill
    cell.alignment = Alignment(horizontal="center" if col_idx > 1 else "left", vertical="center")

rows = [("Product A", 12000, 14500), ("Product B", 8500, 9200)]
for r_idx, (name, q1, q2) in enumerate(rows, start=2):
    ws.cell(r_idx, 1, name).font = formula_font
    c_q1 = ws.cell(r_idx, 2, q1)
    c_q2 = ws.cell(r_idx, 3, q2)
    c_tot = ws.cell(r_idx, 4, f"=SUM(B{r_idx}:C{r_idx})")
    for c in (c_q1, c_q2):
        c.font = input_font
        c.number_format = "$#,##0;($#,##0);\"-\""
    c_tot.font = formula_font
    c_tot.number_format = "$#,##0;($#,##0);\"-\""

tot_row = len(rows) + 2
ws.cell(tot_row, 1, "Total").font = total_font
for col_idx in range(2, 5):
    col_letter = get_column_letter(col_idx)
    c = ws.cell(tot_row, col_idx, f"=SUM({col_letter}2:{col_letter}{tot_row - 1})")
    c.font = total_font
    c.number_format = "$#,##0;($#,##0);\"-\""
    c.border = Border(top=thin, bottom=double_bottom)

ws.freeze_panes = "A2"
wb.calculation.fullCalcOnLoad = True
```

## 2. Modern Excel Function Compatibility (`_xlfn.` Trap)

In raw OOXML (`openpyxl`), newer Excel functions (`XLOOKUP`, `XMATCH`, `FILTER`, `UNIQUE`, `SORT`, `LET`) **must** either:
- Be replaced with classic universal equivalents (`INDEX(..., MATCH(..., 0))`, `SUMIFS`), **OR**
- Include the `_xlfn.` prefix (`=_xlfn.XLOOKUP(...)`). Writing `=XLOOKUP(...)` bare causes `#NAME?` in Excel! `recalc_and_verify.py` automatically warns on unprefixed `_xlfn.` functions.

## 3. Auto-Fit Column Widths (Formatted-Width Aware) & Print-Fit

Remember that raw formula strings (`=SUM(B2:B10)`) don't reflect formatted display length (`$1,234,567`). Pad numeric/currency columns generously (`>= 14`):

```python
for col in ws.columns:
    col_letter = get_column_letter(col[0].column)
    max_len = 10
    for c in col:
        s = str(c.value or "")
        display_est = 14 if s.startswith("=") else len(s)
        max_len = max(max_len, display_est)
    ws.column_dimensions[col_letter].width = min(max_len + 4, 45)

ws.page_setup.orientation = ws.ORIENTATION_LANDSCAPE
ws.sheet_properties.pageSetUpPr.fitToPage = True
ws.page_setup.fitToWidth = 1
ws.page_setup.fitToHeight = 0
```
