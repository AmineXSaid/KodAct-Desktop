"""Designed Excel workbooks for `xlsx_tool.py init` and `add`: the deck
themes (picked from who the workbook is for) on real Excel parts the person
keeps working with: a title band, Excel Tables (filters, banded rows, a
totals row of live formulas), number formats, conditional formatting (data
bars, colour scales, icon sets), KPI tiles that are formulas over the data,
native charts, notes, and frozen headers.

Each block is laid out below the last one on its sheet (`at` overrides it),
so a workbook is built a block at a time and stays readable as it grows.
"""

import re

from openpyxl.chart import AreaChart, BarChart, DoughnutChart, LineChart, PieChart, Reference
from openpyxl.chart.label import DataLabelList
from openpyxl.formatting.rule import ColorScaleRule, DataBarRule, IconSetRule
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
from openpyxl.utils import get_column_letter
from openpyxl.worksheet.table import Table, TableStyleInfo

BLOCKS = ("title", "table", "kpis", "chart", "note", "sheet")
FORMATS = {"money": '#,##0.00 [$€-x-euro2];[Red]-#,##0.00 [$€-x-euro2]', "usd": '"$"#,##0;[Red]-"$"#,##0',
           "eur": '#,##0 "€";[Red]-#,##0 "€"', "int": "#,##0", "dec": "#,##0.00", "pct": "0.0%",
           "date": "yyyy-mm-dd", "text": "@"}
TOTALS = ("sum", "average", "count", "max", "min")


def fill(hex_):
    return PatternFill("solid", start_color="FF" + hex_, end_color="FF" + hex_)


def mix(h, k, toward="FFFFFF"):
    out = ""
    for i in (0, 2, 4):
        x, y = int(h[i:i + 2], 16), int(toward[i:i + 2], 16)
        out += f"{int(x + (y - x) * k):02X}"
    return out


def paper(t):
    if int(t["bg"][0:2], 16) < 0x60:
        return dict(t, bg="FFFFFF", ink="111827", soft="374151", card="F3F4F6")
    return t


def font(t, size=11, bold=False, color=None, head=False):
    return Font(name=t["head"] if head else t["body"], size=size, bold=bold, color="FF" + (color or t["ink"]))


def next_row(ws):
    """The first empty row below everything on the sheet, with a gap."""
    return (ws.max_row + 2) if ws.max_row > 1 or ws["A1"].value is not None else 1


def _sheet(wb, spec):
    name = spec.get("sheet")
    if not name:
        return wb.active
    name = re.sub(r"[\[\]:*?/\\]", " ", str(name))[:31]
    if name not in wb.sheetnames:
        ws = wb.create_sheet(name)
        ws.sheet_view.showGridLines = False
    wb.active = wb.sheetnames.index(name)
    return wb[name]


def title_band(ws, t, title, subtitle=None, width=8):
    t = paper(t)
    ws.sheet_view.showGridLines = False
    a = t["accents"][0]
    for c in range(1, width + 1):
        for r in (1, 2, 3):
            ws.cell(r, c).fill = fill(a)
        ws.cell(4, c).fill = fill(t["accents"][(c - 1) % len(t["accents"])])
    ws.row_dimensions[1].height = 10
    ws.row_dimensions[2].height = 36
    ws.row_dimensions[3].height = 22 if subtitle else 8
    ws.row_dimensions[4].height = 4
    ws["B2"] = title
    ws["B2"].font = Font(name=t["head"], size=22, bold=True, color="FFFFFFFF")
    ws["B2"].alignment = Alignment(vertical="center")
    if subtitle:
        ws["B3"] = subtitle
        ws["B3"].font = Font(name=t["body"], size=11, color="FF" + mix(a, 0.8))
    ws.column_dimensions["A"].width = 2.5
    ws.freeze_panes = None
    # Prints (and previews) one page wide, landscape: a sheet isn't cut in half.
    ws.page_setup.orientation = "landscape"
    ws.page_setup.fitToWidth = 1
    ws.page_setup.fitToHeight = 0
    ws.sheet_properties.pageSetUpPr.fitToPage = True


def add_block(wb, t, spec):
    t = paper(t)
    kind = spec.get("block") or spec.get("type")
    if kind not in BLOCKS:
        raise SystemExit(f"no block {kind!r}; choose one of {', '.join(BLOCKS)}")
    ws = _sheet(wb, spec)
    run = {"title": _title, "table": _table, "kpis": _kpis, "chart": _chart, "note": _note,
           "sheet": _sheet_block}[kind]
    return run(wb, ws, t, spec)


def _anchor(ws, spec, col=2):
    at = spec.get("at")
    if at:
        m = re.fullmatch(r"([A-Za-z]+)(\d+)", at)
        if not m:
            raise SystemExit(f'"at" is a cell like "B6", not {at!r}')
        from openpyxl.utils import column_index_from_string
        return int(m.group(2)), column_index_from_string(m.group(1).upper())
    return next_row(ws), col


def _sheet_block(wb, ws, t, spec):
    """A new sheet with its own title band: {"block": "sheet", "sheet": "Data", "title": "…"}."""
    title_band(ws, t, spec.get("title", ws.title), spec.get("subtitle"))


def _title(wb, ws, t, spec):
    title_band(ws, t, spec.get("title", ""), spec.get("subtitle"))


def _heading(ws, t, row, col, words):
    c = ws.cell(row, col, words)
    c.font = font(t, 14, True, t["ink"], head=True)
    ws.row_dimensions[row].height = 24
    return row + 1


def _table(wb, ws, t, spec):
    """An Excel Table: {"columns": [...], "rows": [[...]], "formats": {"Q3": "usd"},
    "totals": {"Q3": "sum"}, "bars": ["Q3"], "scale": ["Change"], "icons": [...],
    "formulas": {"Change": "=[@Q3]-[@Q2]"}, "name": "Budget", "heading": "…"}."""
    cols, rows = spec.get("columns"), spec.get("rows")
    if not cols or rows is None:
        raise SystemExit('a table needs "columns": [...] and "rows": [[...], ...]')
    row0, col0 = _anchor(ws, spec)
    if spec.get("heading"):
        row0 = _heading(ws, t, row0, col0, spec["heading"])
    formulas = spec.get("formulas") or {}
    a = t["accents"][0]
    thin = Side(style="thin", color="FF" + mix(a, 0.75))
    for j, name in enumerate(cols):
        c = ws.cell(row0, col0 + j, str(name))
        c.font = font(t, 11, True, "FFFFFF", head=True)
        c.fill = fill(a)
        c.alignment = Alignment(vertical="center", horizontal="center", wrap_text=True)
    ws.row_dimensions[row0].height = 24
    for i, row in enumerate(rows, start=1):
        if len(row) + len(formulas) < len(cols):
            row = list(row) + [None] * (len(cols) - len(row))
        values = iter(row)
        for j, name in enumerate(cols):
            v = formulas.get(name)
            if v is None:
                v = next(values, None)
            else:
                # Structured references read [@Col]; write plain cell references so every app computes them.
                v = re.sub(r"\[@([^\]]+)\]", lambda m: get_column_letter(col0 + list(map(str, cols)).index(m.group(1)))
                           + str(row0 + i), v)
            c = ws.cell(row0 + i, col0 + j, v)
            c.font = font(t, 11, color=t["ink"])
            c.border = Border(bottom=thin)
            fmt = (spec.get("formats") or {}).get(name)
            if fmt:
                c.number_format = FORMATS.get(fmt, fmt)
            elif isinstance(v, float):
                c.number_format = FORMATS["dec"]
            elif isinstance(v, int):
                c.number_format = FORMATS["int"]
    last = row0 + len(rows)
    totals = spec.get("totals") or {}
    if totals:
        tr = last + 1
        for j, name in enumerate(cols):
            c = ws.cell(tr, col0 + j)
            fn = totals.get(name)
            if j == 0 and not fn:
                c.value = "Total"
            elif fn:
                if fn not in TOTALS:
                    raise SystemExit(f"no total {fn!r}; choose one of {', '.join(TOTALS)}")
                L = get_column_letter(col0 + j)
                c.value = f"={fn.upper()}({L}{row0 + 1}:{L}{last})"
                fmt = (spec.get("formats") or {}).get(name)
                c.number_format = FORMATS.get(fmt, fmt) if fmt else FORMATS["int"]
            c.font = font(t, 11, True, t["ink"])
            c.fill = fill(mix(a, 0.82))
            c.border = Border(top=Side(style="medium", color="FF" + a))
    ref = f"{get_column_letter(col0)}{row0}:{get_column_letter(col0 + len(cols) - 1)}{last}"
    if rows:
        name = re.sub(r"\W", "_", spec.get("name") or f"Table{len(ws.tables) + 1}_{ws.title}")
        if name[0].isdigit():
            name = "T_" + name
        tab = Table(displayName=name[:250], ref=ref)
        tab.tableStyleInfo = TableStyleInfo(name="TableStyleLight1", showRowStripes=True)
        ws.add_table(tab)
        # Banding in the theme's colours (the table style's own is grey).
        for r in range(row0 + 1, last + 1):
            if (r - row0) % 2 == 0:
                for j in range(len(cols)):
                    ws.cell(r, col0 + j).fill = fill(mix(a, 0.93))
    # Conditional formatting: bars, colour scales and icon sets, by column name.
    def col_range(name):
        if name not in cols:
            raise SystemExit(f"no column {name!r} in {cols}")
        L = get_column_letter(col0 + list(cols).index(name))
        return f"{L}{row0 + 1}:{L}{last}"
    for name in spec.get("bars") or []:
        ws.conditional_formatting.add(col_range(name), DataBarRule(start_type="min", end_type="max",
                                                                   color="FF" + t["accents"][1]))
    for name in spec.get("scale") or []:
        ws.conditional_formatting.add(col_range(name), ColorScaleRule(
            start_type="min", start_color="FFF8696B", mid_type="percentile", mid_value=50,
            mid_color="FFFFEB84", end_type="max", end_color="FF63BE7B"))
    for name in spec.get("icons") or []:
        ws.conditional_formatting.add(col_range(name), IconSetRule("3Arrows", "percent", [0, 33, 67]))
    # Widths from the longest entry; frozen header for a table at the top.
    for j, name in enumerate(cols):
        longest = max([len(str(name))] + [len(f"{r[j]:,}" if isinstance(r[j], (int, float)) else str(r[j]))
                                          for r in rows if j < len(r) and r[j] is not None])
        L = get_column_letter(col0 + j)
        ws.column_dimensions[L].width = max(ws.column_dimensions[L].width or 0, min(48, longest + 4))
    if spec.get("freeze", row0 <= 8):
        ws.freeze_panes = ws.cell(row0 + 1, 1)
    return ref


def _kpis(wb, ws, t, spec):
    """Tiles: {"items": [{"label", "value" or "formula", "format"}]}; a formula
    ("=SUM(D7:D9)") keeps the tile live as the data changes."""
    items = spec.get("items") or []
    if not items:
        raise SystemExit('kpis needs "items": [{"label", "value" or "formula", "format"}]')
    row, col = _anchor(ws, spec)
    for i, item in enumerate(items[:6]):
        c0 = col + i * 3
        acc = t["accents"][i % len(t["accents"])]
        ws.merge_cells(start_row=row, start_column=c0, end_row=row, end_column=c0 + 1)
        ws.merge_cells(start_row=row + 1, start_column=c0, end_row=row + 1, end_column=c0 + 1)
        top = ws.cell(row, c0, item.get("formula") or item.get("value"))
        top.font = Font(name=t["head"], size=24, bold=True, color="FF" + acc)
        top.alignment = Alignment(horizontal="center", vertical="center")
        if item.get("format"):
            top.number_format = FORMATS.get(item["format"], item["format"])
        lab = ws.cell(row + 1, c0, item.get("label", ""))
        lab.font = font(t, 10, color=t["soft"])
        lab.alignment = Alignment(horizontal="center", vertical="top")
        for r in (row, row + 1):
            for c in (c0, c0 + 1):
                ws.cell(r, c).fill = fill(mix(acc, 0.9))
        for c in (c0, c0 + 1):
            ws.cell(row, c).border = Border(top=Side(style="thick", color="FF" + acc))
            ws.column_dimensions[get_column_letter(c)].width = max(
                ws.column_dimensions[get_column_letter(c)].width or 0, 11)
    ws.row_dimensions[row].height = 40
    ws.row_dimensions[row + 1].height = 20


def _chart(wb, ws, t, spec):
    """A native chart over a range: {"type": "column|bar|line|area|pie|doughnut",
    "data": "C6:D9" (series in columns, header row first), "categories": "B7:B9",
    "title": "…", "at": "G6"}; or over the last table: {"table": true, "values": ["Q2", "Q3"]}."""
    kind = spec.get("type") or spec.get("chart", "column")
    makers = {"column": BarChart, "bar": BarChart, "line": LineChart, "area": AreaChart, "pie": PieChart,
              "doughnut": DoughnutChart}
    if kind not in makers:
        raise SystemExit(f"no chart type {kind!r}; choose one of {', '.join(makers)}")
    data_ref, cat_ref = spec.get("data"), spec.get("categories")
    if not data_ref or not cat_ref:
        raise SystemExit('a chart needs "data": "C6:D9" (with its header row) and "categories": "B7:B9"')
    ch = makers[kind]()
    if kind in ("column", "bar"):
        ch.type = "col" if kind == "column" else "bar"
        ch.gapWidth = 60
    from openpyxl.worksheet.cell_range import CellRange
    d, cr = CellRange(data_ref), CellRange(cat_ref)
    ch.add_data(Reference(ws, min_col=d.min_col, min_row=d.min_row, max_col=d.max_col, max_row=d.max_row),
                titles_from_data=True)
    ch.set_categories(Reference(ws, min_col=cr.min_col, min_row=cr.min_row, max_col=cr.max_col, max_row=cr.max_row))
    ch.title = spec.get("title")
    ch.width, ch.height = float(spec.get("width", 16)), float(spec.get("height", 8))
    ch.style = 10
    acc = t["accents"]
    if kind in ("pie", "doughnut"):
        from openpyxl.chart.series import DataPoint
        s = ch.series[0]
        for i in range(cr.max_row - cr.min_row + 1):
            pt = DataPoint(idx=i)
            pt.graphicalProperties.solidFill = acc[i % len(acc)]
            pt.graphicalProperties.line.solidFill = "FFFFFF"
            s.dPt.append(pt)
        ch.dataLabels = DataLabelList()
        ch.dataLabels.showPercent = True
        for k in ("showVal", "showCatName", "showSerName", "showLegendKey"):
            setattr(ch.dataLabels, k, False)
    else:
        for i, s in enumerate(ch.series):
            if kind == "line":
                s.graphicalProperties.line.solidFill = acc[i % len(acc)]
                s.graphicalProperties.line.width = 28000
                s.smooth = False
            else:
                s.graphicalProperties.solidFill = acc[i % len(acc)]
                s.graphicalProperties.line.solidFill = acc[i % len(acc)]
        ch.y_axis.majorGridlines.spPr = None
        ch.legend.position = "b"
    at = spec.get("at")
    if not at:
        at = f"{get_column_letter(d.max_col + 2)}{d.min_row}"
    ws.add_chart(ch, at)


def _note(wb, ws, t, spec):
    """A highlighted note across the sheet: {"text": "…", "kind": "note|warning|success"}."""
    row, col = _anchor(ws, spec)
    acc = {"warning": "D97706", "success": "16A34A", "danger": "DC2626"}.get(spec.get("kind"), t["accents"][0])
    width = int(spec.get("span", 8))
    ws.merge_cells(start_row=row, start_column=col, end_row=row, end_column=col + width - 1)
    c = ws.cell(row, col, spec.get("text", ""))
    c.font = font(t, 11, False, t["ink"])
    c.alignment = Alignment(wrap_text=True, vertical="center", indent=1)
    for k in range(width):
        ws.cell(row, col + k).fill = fill(mix(acc, 0.88))
    ws.cell(row, col).border = Border(left=Side(style="thick", color="FF" + acc))
    ws.row_dimensions[row].height = max(22, 15 * (1 + len(str(spec.get("text", ""))) // (width * 11)))

