"""The demo chat's files, made the way the chat says they were.

    python assets/docs/samples/make_samples.py

The reply in `src/state.rs` updates a Q3 budget workbook (Licences 640 -> 700,
Travel empty -> 520), recalculates it, and writes a memo, a deck and a PDF.
This script does exactly that with the bundled office-documents tools, so the
files the demo shows are the tools' real output, and can be made again.
Needs LibreOffice (recalc and convert).
"""

import os
import shutil
import sys
import tempfile

import openpyxl
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLS = os.path.join(HERE, "..", "..", "forge-plugin", "skills", "office-documents", "scripts")
sys.path.insert(0, TOOLS)

import docx_tool  # noqa: E402
import pdf_tool  # noqa: E402
import pptx_tool  # noqa: E402
import xlsx_tool  # noqa: E402

MEMO = """# Q3 budget update

The Q3 forecast is **5,500 EUR**, up 350 EUR (6.8%) on Q2. Two lines changed this week.

## What changed

- **Licences** fall to 700 EUR (from 800): the renewal was renegotiated.
- **Travel** is forecast at 520 EUR, up 70 EUR for the October offsite.

## By line

| Item | Q2 | Q3 | Change |
|---|---|---|---|
| Servers | 1,200 | 1,350 | +150 |
| Licences | 800 | 700 | -100 |
| Contractors | 2,400 | 2,650 | +250 |
| Training | 300 | 280 | -20 |
| Travel | 450 | 520 | +70 |
| **Total** | **5,150** | **5,500** | **+350** |

## Next

1. Finance signs off the forecast by Friday.
2. Contractors are reviewed again in November.
"""

DECK = """# Q3 budget
Forecast and what changed

# The forecast
- 5,500 EUR, up 350 on Q2 (6.8%)
- Contractors are the largest line: 2,650

# What changed
- Licences: 700, down 100
  - renewal renegotiated
- Travel: 520, up 70
  - October offsite

# Next
- Sign-off by Friday
- Contractors reviewed in November
"""


def budget_before(path):
    """The workbook as the person had it: styled, with formulas, Q3 Licences
    at 640 and Travel not yet forecast."""
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Budget"
    rows = [("Item", "Q2", "Q3"), ("Servers", 1200, 1350), ("Licences", 800, 640), ("Contractors", 2400, 2650), ("Training", 300, 280), ("Travel", 450, None)]
    for r, row in enumerate(rows, start=1):
        for c, v in enumerate(row, start=1):
            ws.cell(r, c, v)
    ws["D1"] = "Change"
    for r in range(2, 7):
        ws[f"D{r}"] = f"=C{r}-B{r}"
    ws["A7"], ws["B7"], ws["C7"], ws["D7"] = "Total", "=SUM(B2:B6)", "=SUM(C2:C6)", "=C7-B7"

    body, bold = Font(name="Calibri", size=11), Font(name="Calibri", size=11, bold=True)
    head_fill = PatternFill("solid", fgColor="FFF1EFEB")
    line = Side(style="thin", color="FF8A8580")
    for r in range(1, 8):
        for c in range(1, 5):
            cell = ws.cell(r, c)
            cell.font = bold if r in (1, 7) else body
            if c > 1:
                cell.alignment = Alignment(horizontal="right")
                if r > 1:
                    cell.number_format = "+#,##0;-#,##0;0" if c == 4 else "#,##0"
    for c in range(1, 5):
        ws.cell(1, c).fill = head_fill
        ws.cell(1, c).border = Border(bottom=line)
        ws.cell(7, c).border = Border(top=line)
    ws.column_dimensions["A"].width = 16
    for col in "BCD":
        ws.column_dimensions[col].width = 12
    ws.freeze_panes = "A2"

    notes = wb.create_sheet("Notes")
    notes["A1"] = "Figures in EUR. Q3 is the forecast."
    notes["A1"].font = body
    notes.column_dimensions["A"].width = 40

    wb.properties.creator = "Finance"
    wb.properties.title = "Q3 budget"
    wb.save(path)


def main():
    out = HERE
    with tempfile.TemporaryDirectory() as tmp:
        before = os.path.join(tmp, "q3-budget.xlsx")
        budget_before(before)
        edited = os.path.join(tmp, "edited.xlsx")
        xlsx_tool.set_cells(before, edited, ["Budget!C3=700", "Budget!C6=520"])
        xlsx_tool.recalc(edited, os.path.join(out, "q3-budget.xlsx"))

        memo_md, deck_md = os.path.join(tmp, "memo.md"), os.path.join(tmp, "deck.md")
        for path, text in ((memo_md, MEMO), (deck_md, DECK)):
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
        memo = os.path.join(out, "q3-budget-memo.docx")
        docx_tool.create(memo, MEMO, page="a4")
        pptx_tool.create(os.path.join(out, "q3-budget.pptx"), DECK)
        pdf_tool.convert(memo, os.path.join(out, "q3-budget.pdf"))
    shutil.rmtree(os.path.join(TOOLS, "__pycache__"), ignore_errors=True)
    for name in ("q3-budget.xlsx", "q3-budget-memo.docx", "q3-budget.pptx", "q3-budget.pdf"):
        print(f"wrote {name} ({os.path.getsize(os.path.join(out, name)):,} bytes)")


if __name__ == "__main__":
    main()
