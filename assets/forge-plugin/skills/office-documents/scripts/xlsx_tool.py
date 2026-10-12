"""Excel workbooks: read, set cells, append rows, create from CSV, recalculate.

    python xlsx_tool.py read FILE [--sheet NAME] [--formulas]
    python xlsx_tool.py set FILE OUT.xlsx --cell "Sheet1!B3=42" [--cell "C3==B3*2" ...]
    python xlsx_tool.py append-rows FILE OUT.xlsx --csv ROWS.csv [--sheet NAME]
    python xlsx_tool.py create OUT.xlsx --csv DATA.csv [--sheet NAME]
    python xlsx_tool.py recalc FILE OUT.xlsx

A value starting with "=" is a formula ("C3==B3*2" sets C3 to =B3*2).
Edits keep every sheet, style, number format, width and formula they don't
touch. openpyxl stores formulas without computing them: run `recalc`
(LibreOffice, headless) so the saved file carries computed values too.
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

import openpyxl
from openpyxl.utils import get_column_letter


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


def read(path, sheet=None, formulas=False):
    """Each sheet as a Markdown table, with its column letters and row numbers.
    Shows computed values (what Excel last saved) unless `formulas`."""
    wb = openpyxl.load_workbook(path, data_only=not formulas)
    names = [sheet] if sheet else wb.sheetnames
    out = []
    for name in names:
        ws = wb[name]
        out.append(f"## {name} ({ws.max_row} rows × {ws.max_column} columns)\n")
        cols = [get_column_letter(c) for c in range(1, ws.max_column + 1)]
        out.append("| | " + " | ".join(cols) + " |")
        out.append("|---|" + "---|" * len(cols))
        for r, row in enumerate(ws.iter_rows(values_only=True), start=1):
            cells = ["" if v is None else str(v) for v in row]
            out.append(f"| {r} | " + " | ".join(cells) + " |")
        out.append("")
    return "\n".join(out)


CELL = re.compile(r"^(?:(?P<sheet>[^!]+)!)?(?P<ref>[A-Za-z]{1,3}\d{1,7})=(?P<value>.*)$", re.S)


def set_cells(path, out, assignments):
    wb = openpyxl.load_workbook(path)
    for a in assignments:
        m = CELL.match(a)
        if not m:
            raise SystemExit(f"not a cell assignment: {a!r} (use Sheet!B3=value)")
        ws = wb[m.group("sheet")] if m.group("sheet") else wb.active
        ws[m.group("ref").upper()] = _value(m.group("value"))
    wb.save(out)
    return len(assignments)


def append_rows(path, out, rows, sheet=None):
    wb = openpyxl.load_workbook(path)
    ws = wb[sheet] if sheet else wb.active
    for row in rows:
        ws.append([_value(c) for c in row])
    wb.save(out)
    return len(rows)


def create(out, rows, sheet=None):
    wb = openpyxl.Workbook()
    ws = wb.active
    if sheet:
        ws.title = sheet
    for i, row in enumerate(rows):
        ws.append([_value(c) for c in row])
        if i == 0:
            for cell in ws[1]:
                cell.font = openpyxl.styles.Font(bold=True)
    for c in range(1, ws.max_column + 1):
        width = max(len(str(ws.cell(r, c).value or "")) for r in range(1, ws.max_row + 1))
        ws.column_dimensions[get_column_letter(c)].width = min(max(10, width + 2), 60)
    wb.save(out)


def recalc(path, out):
    """Open and save the workbook in LibreOffice so formulas carry values."""
    soffice = shutil.which("soffice") or shutil.which("libreoffice")
    if not soffice:
        raise SystemExit("recalc needs LibreOffice (soffice) on PATH")
    with tempfile.TemporaryDirectory() as tmp:
        src = os.path.join(tmp, "in.xlsx")
        shutil.copyfile(path, src)
        profile = "file://" + os.path.join(tmp, "profile").replace(os.sep, "/")
        subprocess.run([soffice, f"-env:UserInstallation={profile}", "--headless", "--calc", "--convert-to", "xlsx:Calc MS Excel 2007 XML", "--outdir", os.path.join(tmp, "out"), src],
                       check=True, capture_output=True, timeout=120)
        shutil.copyfile(os.path.join(tmp, "out", "in.xlsx"), out)


def _csv(path):
    with open(path, newline="", encoding="utf-8") as f:
        return [row for row in csv.reader(f)]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("read"); r.add_argument("file"); r.add_argument("--sheet"); r.add_argument("--formulas", action="store_true")
    s = sub.add_parser("set"); s.add_argument("file"); s.add_argument("out"); s.add_argument("--cell", action="append", required=True)
    a = sub.add_parser("append-rows"); a.add_argument("file"); a.add_argument("out"); a.add_argument("--csv", required=True); a.add_argument("--sheet")
    c = sub.add_parser("create"); c.add_argument("out"); c.add_argument("--csv", required=True); c.add_argument("--sheet")
    x = sub.add_parser("recalc"); x.add_argument("file"); x.add_argument("out")
    args = ap.parse_args(argv)
    if args.cmd == "read":
        sys.stdout.write(read(args.file, args.sheet, args.formulas))
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
