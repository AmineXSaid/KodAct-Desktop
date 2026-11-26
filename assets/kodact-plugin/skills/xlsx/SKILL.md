---
name: xlsx
description: >-
  Make, edit and check Excel workbooks (.xlsx): designed sheets in a theme picked for the reader
  (a title band, real Excel Tables with filters, formulas and totals rows, number formats, data
  bars and colour scales, KPI tiles, native charts, notes), built block by block so the person
  watches the file grow; cell edits that keep every style and formula; recalculation by Excel or
  LibreOffice so the file carries values; and an audit that fails on formula errors and on
  formulas that were never computed. Use for any .xlsx, spreadsheet, budget or model.
---

# Excel workbooks

The scripts are in `${KODACT_SKILL_DIR}/scripts`. Call them with `python` and that full path.
`T="${KODACT_SKILL_DIR}/scripts"` below stands for that folder.

## Make a new workbook: plan, build, compute, audit, look

1. **Plan before any file.** What question does the sheet answer, and what will the person
   change? Inputs (numbers people type) go in one place, calculations refer to them, results are
   formulas: never type a number a formula could compute, never type a total.
2. **Start the real file first**:
   `python "$T/xlsx_tool.py" init book.xlsx --audience "WHO USES IT" --title "…" --subtitle "…"`.
3. **Add blocks** as JSON (a list, or `@blocks.json`):
   `python "$T/xlsx_tool.py" add book.xlsx --json @blocks.json`. Every block and its keys:
   `python "$T/xlsx_tool.py" --help`. A `table` is a real Excel Table: `formats` per column (usd,
   eur, money, int, dec, pct, date, text or an Excel format), `formulas` per column in Table
   syntax (`"=[@Q3]-[@Q2]"`), `totals` (`sum`, `average`…), `bars` and `scale` for data bars and
   colour scales. `add` prints each table's range: `kpis` formulas and `chart` ranges refer to it.
   A new sheet: `{"block": "sheet", "sheet": "Data"}`; `"sheet"` on any block targets one.
4. **Compute and audit**: `python "$T/recalc_and_verify.py" book.xlsx --in-place`. It has Excel
   (on Windows, when installed) or LibreOffice compute every formula, saves the values in the
   file, and fails on any formula error (#DIV/0!, #REF!, #NAME?…), when the formulas could not be
   computed at all, or on XML Excel would refuse or repair (the Office Open XML schemas). Fix each error at its cause and run it again. Its warnings flag a total typed
   as a number and functions that need the `_xlfn.` prefix.
5. **Look at it once**: `python "$T/render_preview.py" book.xlsx --out-dir preview --grid`, then
   open `preview/grid-1.png`: columns too narrow (####), a chart over a table, a sheet that prints
   on ten pages. Don't open every page image.
6. **Reply** with the file's name, what it calculates, and the key results (from the audit or
   `read`). Don't paste whole sheets back.

From a CSV in one step: `python "$T/xlsx_tool.py" create book.xlsx --csv data.csv` (sheet
`Sheet1` unless `--sheet NAME`), then audit and look.

## Change or read a workbook

- Its shape first: `python "$T/inspect_workbook.py" book.xlsx` (sheets, sizes, formula counts).
- Values as last computed: `python "$T/xlsx_tool.py" read book.xlsx [--sheet NAME]`; formulas:
  `--formulas`; one range with both: `read book.xlsx --range "Sheet1!B2:D9"`. It prints 50 rows a
  sheet and how many more there are: read further with `--range` or `--rows N`, rather than all.
- Set cells, keeping every style, format and formula untouched:
  `python "$T/xlsx_tool.py" set book.xlsx out.xlsx --cell "Budget!C3=700" --cell "Budget!D3==C3-B3"`
  (`==` starts a formula). A wrong sheet name lists the sheets there are.
- Append rows: `append-rows book.xlsx out.xlsx --csv rows.csv [--sheet NAME]`.
- After any formula change, compute and audit again (step 4). openpyxl saves formulas without
  values: until then the file shows empty cells in viewers.

Something the blocks don't cover (a pivot-like summary, named ranges, data validation lists):
openpyxl directly, with the patterns in `references/openpyxl_patterns.md`; then compute and audit.

## Rules that hold for every workbook

- One table per question, a header row, units in the header ("Cost ($)"), frozen panes on long
  tables. Inputs in a light fill, outputs bold.
- Percentages as `pct`, money as `usd`/`eur`, never text that looks like a number.
- A chart only where a trend or a comparison is the point; label its axes.
- Never claim the workbook was computed or audited when a step failed: say which, and that its
  numbers are unchecked.
