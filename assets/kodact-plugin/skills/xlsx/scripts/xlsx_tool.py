"""Excel workbooks: read, set cells, append rows, create from CSV, recalculate.

    python xlsx_tool.py init OUT.xlsx [--audience WORDS | --theme NAME] --title T [--subtitle S] [--sheet NAME]
    python xlsx_tool.py add FILE --json BLOCK       (BLOCK: JSON object or list, or @file.json; saves in place)
    python xlsx_tool.py read FILE [--sheet NAME] [--formulas] [--rows N]   (50 rows a sheet unless --rows; 0: all)
    python xlsx_tool.py read FILE --range "Sheet1!B2:D9"   (each cell's value and formula)
    python xlsx_tool.py set FILE OUT.xlsx --cell "Sheet1!B3=42" [--cell "C3==B3*2" ...]
    python xlsx_tool.py append-rows FILE OUT.xlsx --csv ROWS.csv [--sheet NAME]
    python xlsx_tool.py create OUT.xlsx --csv DATA.csv [--sheet NAME]
    python xlsx_tool.py recalc FILE OUT.xlsx

`init` makes the requested workbook first, with its theme (from who it is
for) and a title band; each `add` lays a block below the last one and saves:

    {"block": "table", "columns": [...], "rows": [[...]], "formats": {"Q3": "usd"},
     "formulas": {"Change": "=[@Q3]-[@Q2]"}, "totals": {"Q3": "sum"},
     "bars": ["Q3"], "scale": ["Change"], "icons": ["Q3"], "heading": "Costs", "name": "Costs"}
    {"block": "kpis", "items": [{"label": "Total", "formula": "=SUM(D7:D9)", "format": "usd"}]}
    {"block": "chart", "type": "column|bar|line|area|pie|doughnut", "data": "C6:D9",
     "categories": "B7:B9", "title": "…", "at": "H6"}
    {"block": "note", "text": "…", "kind": "note|warning|success"}
    {"block": "sheet", "sheet": "Data", "title": "…"}          (a new sheet; "sheet" on any block targets one)

Tables are real Excel Tables (filters, banding, a totals row of formulas);
formats: money usd eur int dec pct date text, or an Excel format. `add`
prints each table's range, which charts and KPI formulas refer to. Run
`recalc` after adding formulas.

A value starting with "=" is a formula ("C3==B3*2" sets C3 to =B3*2).
Edits keep every sheet, style, number format, width and formula they don't
touch. openpyxl stores formulas without computing them: run `recalc`
(LibreOffice, headless) so the saved file carries computed values too.
`recalc` fails, naming the cells, when any formula computes to an error
(#DIV/0!, #REF!, #NAME?, #VALUE!, #N/A, #NUM!, #NULL!): fix them first.
Fonts are always named (a nameless one becomes whatever font the opening
app picks), and every file carries real properties: author KodAct, dates.
"""

import argparse
import csv
import io
import os
import re
import shutil
import subprocess
import sys
import tempfile

import zipfile
from datetime import datetime, timezone

import openpyxl
import warnings

# openpyxl drops the extension part of conditional formats a recalculation added (the rule itself
# stays); its warning about that says nothing the person can act on.
warnings.filterwarnings("ignore", message=r".*extension is not supported.*")
from openpyxl.styles import Border, Font, Side
from openpyxl.utils import get_column_letter

# The design and icon code the document skills share: kodact-plugin/scripts.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "scripts"))

AUTHOR = "KodAct"
FONT = "Calibri"


def _now():
    return datetime.now(timezone.utc).replace(microsecond=0, tzinfo=None)


def _finish(wb, title=None, new=False):
    p = wb.properties
    if new:
        p.creator = AUTHOR
        p.created = _now()
        p.title = title
    p.lastModifiedBy = AUTHOR
    p.modified = _now()


def _number_format(values):
    """A column of numbers gets thousands separators; decimals if it has any."""
    nums = [v for v in values if isinstance(v, (int, float)) and not isinstance(v, bool)]
    if not nums or len(nums) < len([v for v in values if v not in (None, "")]):
        return None
    return "#,##0" if all(float(v).is_integer() for v in nums) else "#,##0.00"


def _value(text):
    """A cell value from text: a formula, a number, or the text itself."""
    if text.startswith("="):
        return text
    try:
        return int(text)
    except ValueError:
        pass
    try:
        return float(text)
    except ValueError:
        return text


THEME_MARK = "kodact-theme:"


def init(out, theme="clean", title="", subtitle=None, sheet=None):
    import deck_design
    import sheet_design
    t = deck_design.theme(theme)
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = (sheet or "Summary")[:31]
    sheet_design.title_band(ws, t, title, subtitle)
    _finish(wb, title, new=True)
    wb.properties.category = THEME_MARK + theme
    _save(wb, out)


def add(path, specs, theme=None):
    """Lay out blocks (one dict or a list) and save in place; returns what each made."""
    import deck_design
    import sheet_design
    wb = openpyxl.load_workbook(path)
    cat = wb.properties.category or ""
    t = deck_design.theme(theme or (cat[len(THEME_MARK):] if cat.startswith(THEME_MARK) else "clean"))
    made = []
    for spec in specs if isinstance(specs, list) else [specs]:
        if not isinstance(spec, dict):
            sys.exit('a block is a JSON object, e.g. {"block": "table", "columns": [...], "rows": [...]}')
        made.append(sheet_design.add_block(wb, t, spec))
    _finish(wb)
    _save(wb, path)
    return made


def _save(wb, out):
    tmp = out + ".part"
    wb.save(tmp)
    os.replace(tmp, out)


def _json(arg):
    import json
    raw = arg
    if arg.startswith("@"):
        with open(arg[1:], encoding="utf-8") as f:
            raw = f.read()
    try:
        return json.loads(raw)
    except json.JSONDecodeError as e:
        sys.exit(f"--json is not valid JSON ({e}); quote keys and strings with double quotes")


def _sheet(wb, name):
    """The sheet called `name`, or a clear error naming the sheets there are (quotes allowed)."""
    name = name.strip().strip("'")
    if name in wb.sheetnames:
        return wb[name]
    folded = {n.casefold(): n for n in wb.sheetnames}
    if name.casefold() in folded:
        return wb[folded[name.casefold()]]
    raise SystemExit(f"no sheet {name!r}; the sheets are {', '.join(repr(n) for n in wb.sheetnames)}")


ROWS_SHOWN = 50  # rows printed per sheet unless asked for more


def read(path, sheet=None, formulas=False, rows=ROWS_SHOWN):
    """Each sheet as a Markdown table, with its column letters and row numbers, up to `rows` rows
    a sheet (None: all), then how many more there are and how to read them: 100,000 rows printed
    whole are 3.7 million characters. Shows computed values (what Excel last saved) unless
    `formulas`. Streams the file (read-only), so a big workbook reads in seconds."""
    wb = openpyxl.load_workbook(path, data_only=not formulas, read_only=True)
    try:
        names = [_sheet(wb, sheet).title] if sheet else wb.sheetnames
        out = []
        for name in names:
            ws = wb[name]
            n_rows, n_cols = ws.max_row or 0, ws.max_column or 0
            out.append(f"## {name} ({n_rows:,} rows × {n_cols} columns)\n")
            cols = [get_column_letter(c) for c in range(1, n_cols + 1)]
            out.append("| | " + " | ".join(cols) + " |")
            out.append("|---|" + "---|" * len(cols))
            shown = 0
            for r, row in enumerate(ws.iter_rows(values_only=True), start=1):
                if rows is not None and shown >= rows:
                    break
                cells = ["" if v is None else str(v) for v in row]
                out.append(f"| {r} | " + " | ".join(cells) + " |")
                shown += 1
            if rows is not None and n_rows > shown:
                last = get_column_letter(max(1, n_cols))
                out.append(f"\n… {n_rows - shown:,} more row(s) not shown. Read a part with "
                           f"--range \"'{name}'!A{shown + 1}:{last}{min(n_rows, shown + 50)}\", more with --rows N, "
                           f"or all with --rows 0 (long).")
            out.append("")
        return "\n".join(out)
    finally:
        wb.close()


def read_range(path, ref):
    """Each cell of a range: its address, value (as last computed) and formula."""
    sheet, _, cells = ref.rpartition("!")
    shown = openpyxl.load_workbook(path, data_only=True)
    raw = openpyxl.load_workbook(path)
    name = sheet.strip("'") or raw.active.title
    if name not in raw.sheetnames:
        raise SystemExit(f"no sheet {name!r}; the sheets are {', '.join(raw.sheetnames)}")
    values, formulas = shown[name], raw[name]
    out = [f"## {name}!{cells.upper()}\n", "| Cell | Value | Formula |", "|---|---|---|"]
    rows = formulas[cells.upper()]
    if not isinstance(rows, tuple):
        rows = ((rows,),)
    elif rows and not isinstance(rows[0], tuple):
        rows = (rows,)
    for row in rows:
        for cell in row:
            value = values[cell.coordinate].value
            f = cell.value if isinstance(cell.value, str) and cell.value.startswith("=") else ""
            out.append(f"| {cell.coordinate} | {'' if value is None else value} | {f} |")
    return "\n".join(out) + "\n"


ERRORS = ("#DIV/0!", "#REF!", "#NAME?", "#VALUE!", "#N/A", "#NUM!", "#NULL!")


def errors(path):
    """The cells whose computed value is a formula error, as `Sheet!A1: #REF!`."""
    wb = openpyxl.load_workbook(path, data_only=True)
    found = []
    for ws in wb.worksheets:
        for row in ws.iter_rows():
            for cell in row:
                if isinstance(cell.value, str) and cell.value.strip() in ERRORS:
                    found.append(f"{ws.title}!{cell.coordinate}: {cell.value.strip()}")
    return found


CELL = re.compile(r"^(?:(?P<sheet>[^!]+)!)?(?P<ref>[A-Za-z]{1,3}\d{1,7})=(?P<value>.*)$", re.S)


def set_cells(path, out, assignments):
    wb = openpyxl.load_workbook(path)
    for a in assignments:
        m = CELL.match(a)
        if not m:
            raise SystemExit(f"not a cell assignment: {a!r} (use Sheet!B3=value)")
        ws = _sheet(wb, m.group("sheet")) if m.group("sheet") else wb.active
        ws[m.group("ref").upper()] = _value(m.group("value"))
    _finish(wb)
    wb.save(out)
    return len(assignments)


def append_rows(path, out, rows, sheet=None):
    wb = openpyxl.load_workbook(path)
    ws = _sheet(wb, sheet) if sheet else wb.active
    for row in rows:
        ws.append([_value(c) for c in row])
    _finish(wb)
    wb.save(out)
    return len(rows)


def create(out, rows, sheet=None):
    """A workbook from CSV rows: a bold header with a rule under it, frozen, numbers formatted per
    column and columns as wide as their longest entry. Linear in the rows (asking openpyxl for
    ws.max_row after each append made 100,000 rows take 19 minutes, 2026-11-26)."""
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = sheet or "Sheet1"  # Excel's name for a first sheet, which formulas and people assume
    # Body cells keep the workbook's default style, Calibri 11 (named, so the font never varies by
    # app); only the header is styled.
    values = [[_value(c) for c in row] for row in rows]
    for row in values:
        ws.append(row)
    if not values:
        _finish(wb, sheet, new=True)
        wb.save(out)
        return
    head = Font(name=FONT, size=11, bold=True)
    rule = Border(bottom=Side(style="thin", color="FF808080"))
    for cell in ws[1]:
        cell.font, cell.border = head, rule
    ws.freeze_panes = "A2"
    n_cols = max(len(r) for r in values)
    for c in range(n_cols):
        column = [r[c] if c < len(r) else None for r in values[1:]]
        fmt = _number_format(column)
        if fmt:
            for i, v in enumerate(column, start=2):
                if v is not None:
                    ws.cell(i, c + 1).number_format = fmt
        width = max((len(str(r[c])) for r in values if c < len(r) and r[c] is not None), default=0)
        ws.column_dimensions[get_column_letter(c + 1)].width = min(max(10, width + 2), 60)
    _finish(wb, sheet, new=True)
    wb.save(out)


def recalc(path, out):
    """Compute every formula (Excel on Windows when installed, else LibreOffice) so the file carries
    values; fails, naming the cells, when a formula computes to an error."""
    from _kodact_office import recalc_xlsx

    with tempfile.TemporaryDirectory() as tmp:
        src = os.path.join(tmp, "in.xlsx")
        shutil.copyfile(path, src)
        done = os.path.join(tmp, "done.xlsx")
        try:
            recalc_xlsx(src, done)
        except FileNotFoundError as exc:
            raise SystemExit(f"recalc needs Microsoft Excel (Windows) or LibreOffice: {exc}")
        _keep_properties(src, done, out)
    bad = errors(out)
    if bad:
        raise SystemExit("recalculated, but these formulas give errors (the file was still written):\n  " + "\n  ".join(bad))


def _keep_properties(original, recalculated, out):
    """LibreOffice rewrites docProps (it names itself as the application and
    drops the author); put the original's back, so a recalculation doesn't
    change who made the file."""
    with zipfile.ZipFile(original) as z:
        keep = {n: z.read(n) for n in ("docProps/core.xml", "docProps/app.xml") if n in z.namelist()}
    with zipfile.ZipFile(recalculated) as src, zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as dst:
        for item in src.infolist():
            dst.writestr(item, keep.get(item.filename, src.read(item.filename)))


def _csv(path):
    with open(path, newline="", encoding="utf-8") as f:
        return [row for row in csv.reader(f)]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    i = sub.add_parser("init"); i.add_argument("out"); i.add_argument("--theme"); i.add_argument("--audience")
    i.add_argument("--title", required=True); i.add_argument("--subtitle"); i.add_argument("--sheet")
    b = sub.add_parser("add"); b.add_argument("file"); b.add_argument("--json", required=True); b.add_argument("--theme")
    r = sub.add_parser("read"); r.add_argument("file"); r.add_argument("--sheet"); r.add_argument("--formulas", action="store_true"); r.add_argument("--range")
    r.add_argument("--rows", type=int, default=ROWS_SHOWN, help=f"rows per sheet (default {ROWS_SHOWN}; 0: all)")
    s = sub.add_parser("set"); s.add_argument("file"); s.add_argument("out"); s.add_argument("--cell", action="append", required=True)
    a = sub.add_parser("append-rows"); a.add_argument("file"); a.add_argument("out"); a.add_argument("--csv", required=True); a.add_argument("--sheet")
    c = sub.add_parser("create"); c.add_argument("out"); c.add_argument("--csv", required=True); c.add_argument("--sheet")
    x = sub.add_parser("recalc"); x.add_argument("file"); x.add_argument("out")
    args = ap.parse_args(argv)
    if args.cmd == "init":
        import deck_design
        theme = args.theme or deck_design.for_audience(args.audience)
        deck_design.theme(theme)
        init(args.out, theme, args.title, args.subtitle, args.sheet)
        print(f"wrote {args.out}: its {theme} title band; add blocks with `add`")
        return
    if args.cmd == "add":
        for m in add(args.file, _json(args.json), args.theme):
            if m:
                print(f"table at {m}")
        print(f"saved {args.file}")
        return
    if args.cmd == "read":
        sys.stdout.write(read_range(args.file, args.range) if args.range else read(args.file, args.sheet, args.formulas, args.rows or None))
    elif args.cmd == "set":
        n = set_cells(args.file, args.out, args.cell)
        print(f"set {n} cell(s); wrote {args.out}")
    elif args.cmd == "append-rows":
        n = append_rows(args.file, args.out, _csv(args.csv), args.sheet)
        print(f"appended {n} row(s); wrote {args.out}")
    elif args.cmd == "create":
        create(args.out, _csv(args.csv), args.sheet)
        print(f"wrote {args.out}")
    elif args.cmd == "recalc":
        recalc(args.file, args.out)
        print(f"recalculated; wrote {args.out}")


if __name__ == "__main__":
    main()
