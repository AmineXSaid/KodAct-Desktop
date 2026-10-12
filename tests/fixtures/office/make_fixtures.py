"""Hard Office files and PDFs for the editors' tests.

    python3 tests/fixtures/office/make_fixtures.py

Each file is legitimate (it passes scripts/validate_office.py) and as hard as
real files get, so an editor that keeps them intact through an edit can be
trusted with what people actually have:

- hard.xlsx: 16 sheets (one hidden, one very hidden, tab colours), a
  20,000-row sheet, styles and number formats, merges, frozen panes,
  cross-sheet formulas, defined names, an array formula, conditional
  formatting, data validation, comments, hyperlinks, rich text, an image, a
  chart, a table and an autofilter; recalculated by LibreOffice so every
  formula carries its value. hard-openpyxl.xlsx is the same workbook as
  openpyxl writes it (no saved values, absolute part paths).
- hard.docx: a TOC field, headings, every kind of run formatting, a
  hyperlink, a bookmark, tracked insertions and deletions, a comment, a
  footnote, multi-level and numbered lists, a table with merged cells and a
  nested table, an image, a content control, and a landscape second section
  with its own header and a PAGE field in the footer.
- hard.pptx: 16:9, 12 slides on a custom theme: layouts' placeholders,
  formatted runs and a hyperlink, two-content, a table with merged cells, a
  chart, filled and rotated shapes, a connector with an arrow, nested groups,
  a cropped picture, a background fill, a hidden slide, and notes.
- hard.pdf: 8 pages with embedded fonts, an image, vector paths and a
  gradient, an outline, a form (text, multi-line, checkbox, radio, combo,
  list), a rotated page, and highlight, note and link annotations.
  appendix.pdf: 2 pages, to merge in.

Deterministic content (seeded); needs LibreOffice for the workbook's values.
"""

import io
import os
import random
import shutil
import subprocess
import sys
import tempfile
import zipfile
from datetime import date, datetime, timedelta

HERE = os.path.dirname(os.path.abspath(__file__))
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
FONT_BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"


def png(path, w=240, h=150, hue=0):
    """A small picture: a gradient with a few shapes, so crops and scaling show."""
    from PIL import Image, ImageDraw

    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for x in range(w):
        t = x / w
        d.line([(x, 0), (x, h)], fill=(int(30 + 60 * t + hue), int(90 + 100 * t), int(160 + 80 * (1 - t))))
    d.ellipse([w * 0.1, h * 0.2, w * 0.45, h * 0.8], fill=(250, 250, 250))
    d.rectangle([w * 0.55, h * 0.3, w * 0.9, h * 0.7], fill=(240, 170, 60))
    img.save(path)
    return path


def soffice(args, tmp):
    exe = shutil.which("soffice") or shutil.which("libreoffice")
    if not exe:
        sys.exit("LibreOffice (soffice) is needed for the workbook's saved values")
    profile = "file://" + os.path.join(tmp, "lo-profile")
    subprocess.run([exe, f"-env:UserInstallation={profile}", "--headless", *args], check=True, capture_output=True, timeout=600)


# --- Excel -----------------------------------------------------------------

def make_xlsx(tmp):
    import openpyxl
    from openpyxl.cell.rich_text import CellRichText, TextBlock
    from openpyxl.cell.text import InlineFont
    from openpyxl.chart import BarChart, Reference
    from openpyxl.comments import Comment
    from openpyxl.drawing.image import Image
    from openpyxl.formatting.rule import CellIsRule, ColorScaleRule, FormulaRule
    from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
    from openpyxl.workbook.defined_name import DefinedName
    from openpyxl.worksheet.datavalidation import DataValidation
    from openpyxl.worksheet.formula import ArrayFormula
    from openpyxl.worksheet.hyperlink import Hyperlink
    from openpyxl.worksheet.table import Table, TableStyleInfo

    rnd = random.Random(42)
    F = lambda **k: Font(name="Calibri", size=k.pop("size", 11), **k)  # noqa: E731
    head_fill = PatternFill("solid", fgColor="FF1F4E78")
    head_font = F(bold=True, color="FFFFFFFF")
    thin = Side(style="thin", color="FF333333")

    def header(ws, names):
        ws.append(names)
        for c in ws[1]:
            c.font, c.fill = head_font, head_fill
            c.border = Border(left=thin, right=thin, top=thin, bottom=Side(style="medium", color="FF333333"))
            c.alignment = Alignment(horizontal="center", vertical="center", wrap_text=True)
        ws.freeze_panes = "A2"

    wb = openpyxl.Workbook()
    summary = wb.active
    summary.title = "Summary"
    regions = ["North", "South", "East", "West", "Central"]

    # Products: codes, names, prices (VLOOKUP's table).
    products = wb.create_sheet("Products")
    header(products, ["Code", "Name", "Price", "Category"])
    for i in range(1, 21):
        products.append([f"P{i:03d}", f"Product {i}", round(rnd.uniform(5, 120), 2), rnd.choice(["Gadget", "Widget", "Tool"])])
    for r in range(2, 22):
        products[f"C{r}"].number_format = '"€"#,##0.00'
    products.column_dimensions["B"].width = 18
    products.sheet_properties.tabColor = "FF70AD47"

    # Sales: 200 rows, a formula column, autofilter, CF, validation.
    sales = wb.create_sheet("Sales")
    header(sales, ["Date", "Region", "Product", "Units", "Price", "Revenue"])
    start = date(2026, 1, 1)
    for r in range(2, 202):
        code = f"P{rnd.randint(1, 20):03d}"
        sales.append([start + timedelta(days=rnd.randint(0, 270)), rnd.choice(regions), code, rnd.randint(1, 40),
                      f"=VLOOKUP(C{r},Products!$A$2:$C$21,3,FALSE)", f"=D{r}*E{r}"])
        sales[f"A{r}"].number_format = "yyyy-mm-dd"
        sales[f"E{r}"].number_format = sales[f"F{r}"].number_format = '"€"#,##0.00'
    sales.auto_filter.ref = "A1:F201"
    sales.conditional_formatting.add("F2:F201", CellIsRule(operator="greaterThan", formula=["2000"], fill=PatternFill("solid", fgColor="FFFFC7CE"), font=Font(color="FF9C0006")))
    sales.conditional_formatting.add("D2:D201", ColorScaleRule(start_type="min", start_color="FFF8696B", end_type="max", end_color="FF63BE7B"))
    sales.conditional_formatting.add("A2:F201", FormulaRule(formula=['$B2="Central"'], font=Font(italic=True)))
    dv = DataValidation(type="list", formula1='"North,South,East,West,Central"', allow_blank=True, showDropDown=False)
    sales.add_data_validation(dv)
    dv.add("B2:B201")
    for col, w in zip("ABCDEF", (12, 10, 10, 8, 10, 12)):
        sales.column_dimensions[col].width = w
    sales.sheet_properties.tabColor = "FF1F4E78"

    # Regions: SUMIF/COUNTIF per region against a target.
    reg = wb.create_sheet("Regions")
    header(reg, ["Region", "Target", "Actual", "Orders", "vs target"])
    for i, name in enumerate(regions, start=2):
        reg.append([name, 15000 + 2500 * i, f"=SUMIF(Sales!$B$2:$B$201,A{i},Sales!$F$2:$F$201)", f"=COUNTIF(Sales!$B$2:$B$201,A{i})", f"=IFERROR(C{i}/B{i}-1,0)"])
        reg[f"B{i}"].number_format = reg[f"C{i}"].number_format = "#,##0"
        reg[f"E{i}"].number_format = "+0.0%;-0.0%;0.0%"

    # Dates and times.
    dates = wb.create_sheet("Dates")
    header(dates, ["Start", "+3 months", "Month end", "Weekday", "As text", "Days between", "Time"])
    for r in range(2, 14):
        dates.append([datetime(2026, r - 1, 15), f"=EDATE(A{r},3)", f"=EOMONTH(A{r},0)", f"=WEEKDAY(A{r},2)", f'=TEXT(A{r},"d mmm yyyy")', f"=B{r}-A{r}", datetime(2026, 1, 1, 8 + r % 9, 15 * (r % 4))])
        for c in "ABC":
            dates[f"{c}{r}"].number_format = "yyyy-mm-dd"
        dates[f"G{r}"].number_format = "hh:mm"

    # Formats: number formats, fonts, fills, borders, alignment.
    fm = wb.create_sheet("Formats")
    header(fm, ["Kind", "Value", "Note"])
    for i, (kind, value, nf) in enumerate([
        ("Thousands", 1234567.891, "#,##0"), ("Percent", 0.1234, "0.0%"), ("Euro", 9876.5, '"€"#,##0.00'),
        ("Scientific", 123456, "0.00E+00"), ("Signed", -4321, "+#,##0;-#,##0;0"), ("Red negative", -55.5, "#,##0.00;[Red]-#,##0.00"),
        ("Text", "00123", "@"), ("Date", date(2026, 12, 31), "d mmmm yyyy"), ("Time", datetime(2026, 1, 1, 14, 30, 15), "hh:mm:ss"),
        ("Fraction", 0.375, "# ?/?"),
    ], start=2):
        fm.append([kind, value, nf])
        fm[f"B{i}"].number_format = nf
    styled = [
        ("A13", F(italic=True, strike=True, color="FF7030A0")), ("A14", F(size=14, bold=True, color="FFC00000")),
        ("A15", F(size=9, underline="double")), ("A16", Font(name="Arial", size=10.5, vertAlign="superscript")),
    ]
    for ref, font in styled:
        fm[ref] = f"Styled {ref}"
        fm[ref].font = font
    fm["B13"].fill = PatternFill("solid", fgColor="FFFFF2CC")
    fm["B14"].fill = PatternFill("darkGrid", fgColor="FFFF0000", bgColor="FFFFFF00")
    for ref, style in (("B15", "dashed"), ("B16", "dotted"), ("B17", "double"), ("B18", "mediumDashed"), ("B19", "hair")):
        fm[ref] = style
        fm[ref].border = Border(bottom=Side(style=style, color="FF000000"), top=Side(style=style, color="FF000000"))
    fm["C13"], fm["C14"], fm["C15"], fm["C16"] = "Rotated", "Indented", "Wrapped text that runs long enough to wrap in its cell", "Shrunk to fit this narrow cell"
    fm["C13"].alignment = Alignment(textRotation=45)
    fm["C14"].alignment = Alignment(indent=2)
    fm["C15"].alignment = Alignment(wrap_text=True, vertical="top")
    fm["C16"].alignment = Alignment(shrink_to_fit=True)
    fm.column_dimensions["A"].width, fm.column_dimensions["B"].width, fm.column_dimensions["C"].width = 16, 18, 24
    fm.row_dimensions[15].height = 45

    # Merged ranges, and formulas that read them.
    mg = wb.create_sheet("Merged")
    mg["A1"] = "A title across six columns"
    mg.merge_cells("A1:F1")
    mg["A1"].font, mg["A1"].alignment = F(size=14, bold=True), Alignment(horizontal="center")
    mg["B3"], mg["E3"] = 42, "Tall"
    mg.merge_cells("B3:C5")
    mg.merge_cells("E3:E6")
    mg["A8"], mg["B8"] = "Twice B3", "=B3*2"
    mg.freeze_panes = "B2"

    # Lookups and an array formula.
    lk = wb.create_sheet("Lookups")
    header(lk, ["A", "B", "Result", "Formula"])
    for r in range(2, 11):
        lk.append([r * 3, r * 2])
    lk["C2"], lk["D2"] = '=INDEX(Products!$B$2:$B$21,MATCH("P007",Products!$A$2:$A$21,0))', "INDEX/MATCH"
    lk["C3"], lk["D3"] = "=HLOOKUP(\"Price\",Products!$A$1:$D$21,5,FALSE)", "HLOOKUP"
    lk["C4"], lk["D4"] = "=CHOOSE(2,\"one\",\"two\",\"three\")", "CHOOSE"
    lk["C5"], lk["D5"] = "=SUMPRODUCT(A2:A10,B2:B10)", "SUMPRODUCT"
    lk["C6"] = ArrayFormula("C6", "=SUM(A2:A10*B2:B10)")
    lk["D6"] = "array formula"
    lk["C7"], lk["D7"] = "=ROUND(AVERAGE(A2:A10)/7,3)", "ROUND/AVERAGE"
    lk["C8"], lk["D8"] = "=SUBSTITUTE(TRIM(\"  a  b  \"),\" \",\"-\")", "text"

    # Names: defined names used across the workbook.
    nm = wb.create_sheet("Names")
    nm["A2"], nm["B2"], nm["A3"], nm["B3"] = "Tax rate", 0.2, "Plan total", 150000
    nm["B2"].number_format, nm["B3"].number_format = "0%", "#,##0"
    for name, ref in (("TaxRate", "Names!$B$2"), ("PlanTotal", "Names!$B$3"), ("SalesRevenue", "Sales!$F$2:$F$201")):
        wb.defined_names[name] = DefinedName(name, attr_text=ref)

    # A big sheet: 20,000 rows x 6 numbers, a row total each, a grand total.
    big = wb.create_sheet("Big")
    header(big, ["N1", "N2", "N3", "N4", "N5", "N6", "Row total"])
    for r in range(2, 20002):
        big.append([rnd.randint(0, 9999) for _ in range(6)] + [f"=SUM(A{r}:F{r})"])
    big["H1"], big["H2"] = "Grand total", "=SUM(G2:G20001)"
    big["H2"].number_format = "#,##0"

    # A chart and a picture.
    ch = wb.create_sheet("Chart")
    ch.append(["Region", "Revenue"])
    for i, name in enumerate(regions, start=2):
        ch.append([name, f"=Regions!C{i}"])
    chart = BarChart()
    chart.title, chart.y_axis.title = "Revenue by region", "EUR"
    chart.add_data(Reference(ch, min_col=2, min_row=1, max_row=6), titles_from_data=True)
    chart.set_categories(Reference(ch, min_col=1, min_row=2, max_row=6))
    ch.add_chart(chart, "D2")
    ch.add_image(Image(png(os.path.join(tmp, "x.png"))), "D20")

    # Data validation of every kind.
    val = wb.create_sheet("Validation")
    header(val, ["List", "Whole 1-10", "Decimal", "Date in 2026", "Custom (even)"])
    for kind, ref, kw in (
        ("list", "A2:A20", dict(formula1='"Yes,No,Maybe"')),
        ("whole", "B2:B20", dict(operator="between", formula1="1", formula2="10")),
        ("decimal", "C2:C20", dict(operator="greaterThan", formula1="0")),
        ("date", "D2:D20", dict(operator="between", formula1="DATE(2026,1,1)", formula2="DATE(2026,12,31)")),
        ("custom", "E2:E20", dict(formula1="MOD(E2,2)=0")),
    ):
        v = DataValidation(type=kind, showErrorMessage=True, errorTitle="Not allowed", error=f"Must be {kind}", promptTitle=kind, prompt=f"Enter {kind}", showInputMessage=True, **kw)
        val.add_data_validation(v)
        v.add(ref)

    # Notes: rich text, comments, hyperlinks, long wrapped text.
    notes = wb.create_sheet("Notes")
    notes["A1"] = CellRichText("Plain, then ", TextBlock(InlineFont(b=True, color="FFC00000"), "bold red"), ", then ", TextBlock(InlineFont(i=True, u="single"), "italic underlined"), ".")
    notes["A1"].comment = Comment("Rich text: three runs in one cell.", "Ada")
    notes["A2"] = "Example site"
    notes["A2"].hyperlink = "https://example.com/budget"
    notes["A2"].font = Font(name="Calibri", size=11, color="FF0563C1", underline="single")
    notes["A3"] = "Back to the summary"
    notes["A3"].hyperlink = Hyperlink(ref="A3", location="'Summary'!A1", display="Back to the summary")
    notes["A4"] = "A note long enough that it has to wrap across several lines in this column, which is what wrapped text is for."
    notes["A4"].alignment = Alignment(wrap_text=True, vertical="top")
    notes["B2"] = 7
    notes["B2"].comment = Comment("A second comment, on a number.", "Grace")
    notes.column_dimensions["A"].width = 40
    notes.sheet_properties.tabColor = "FFFFC000"

    # An Excel table with a structured reference outside it.
    tb = wb.create_sheet("Table")
    tb.append(["Order", "Customer", "Amount", "Paid"])
    for i in range(1, 21):
        tb.append([1000 + i, f"Customer {rnd.randint(1, 8)}", round(rnd.uniform(50, 900), 2), rnd.choice(["yes", "no"])])
    table = Table(displayName="Orders", ref="A1:D21")
    table.tableStyleInfo = TableStyleInfo(name="TableStyleMedium9", showRowStripes=True)
    tb.add_table(table)
    tb["F1"], tb["F2"] = "Total", "=SUM(Orders[Amount])"

    hidden = wb.create_sheet("Hidden")
    hidden["A1"] = 21
    hidden.sheet_state = "hidden"
    very = wb.create_sheet("VeryHidden")
    very["A1"] = "internal"
    very.sheet_state = "veryHidden"

    # Summary: the workbook's figures, from every other sheet.
    summary["A1"] = "Q3 sales summary"
    summary.merge_cells("A1:D1")
    summary["A1"].font = F(size=16, bold=True, color="FF1F4E78")
    rows = [
        ("Revenue", "=SUM(SalesRevenue)", '"€"#,##0.00'), ("Average order", "=AVERAGE(Sales!F2:F201)", '"€"#,##0.00'),
        ("Tax", "=TaxRate*B3", '"€"#,##0.00'), ("Against plan", "=B3-PlanTotal", "+#,##0;-#,##0;0"),
        ("Product P003", '=VLOOKUP("P003",Products!A2:C21,2,FALSE)', "General"), ("West target", '=INDEX(Regions!B2:B6,MATCH("West",Regions!A2:A6,0))', "#,##0"),
        ("North revenue", '=SUMIF(Sales!B2:B201,"North",Sales!F2:F201)', "#,##0"), ("Big South orders", '=COUNTIFS(Sales!B2:B201,"South",Sales!F2:F201,">500")', "0"),
        ("Status", '=IF(B3>PlanTotal,"Ahead","Behind")', "General"), ("Big grand total", "=Big!H2", "#,##0"),
        ("From hidden", "=Hidden!A1*2", "0"), ("Table total", "=Table!F2", "#,##0.00"), ("Twice merged", "=Merged!B8", "0"),
    ]
    for i, (label, formula, nf) in enumerate(rows, start=3):
        summary[f"A{i}"], summary[f"B{i}"] = label, formula
        summary[f"B{i}"].number_format = nf
        summary[f"A{i}"].font = F(bold=i == 3)
    summary.column_dimensions["A"].width, summary.column_dimensions["B"].width = 20, 16
    summary.freeze_panes = "A3"
    summary.sheet_properties.tabColor = "FFC00000"

    wb.move_sheet("Names", offset=-(len(wb.sheetnames) - 2))
    wb.properties.creator = "Forge fixtures"
    wb.properties.title = "Hard workbook"
    raw = os.path.join(HERE, "hard-openpyxl.xlsx")
    wb.save(raw)

    # LibreOffice computes and saves every formula's value, as Excel would.
    src = os.path.join(tmp, "hard.xlsx")
    shutil.copyfile(raw, src)
    soffice(["--calc", "--convert-to", "xlsx:Calc MS Excel 2007 XML", "--outdir", os.path.join(tmp, "lo"), src], tmp)
    shutil.copyfile(os.path.join(tmp, "lo", "hard.xlsx"), os.path.join(HERE, "hard.xlsx"))


# --- Word ------------------------------------------------------------------

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"


def _xml(fragment):
    from docx.oxml import parse_xml

    return parse_xml(fragment.replace("<w:", f'<w:{""}').replace("XMLNS", f'xmlns:w="{W}"'))


def make_docx(tmp):
    import docx
    from docx.enum.section import WD_ORIENT, WD_SECTION
    from docx.enum.text import WD_COLOR_INDEX
    from docx.opc.constants import RELATIONSHIP_TYPE as RT
    from docx.oxml import parse_xml
    from docx.shared import Mm, Pt, RGBColor

    ns = f'xmlns:w="{W}"'
    d = docx.Document()
    s = d.sections[0]
    s.page_width, s.page_height = Mm(210), Mm(297)
    for side in ("left_margin", "right_margin", "top_margin", "bottom_margin"):
        setattr(s, side, Mm(25))
    s.header.paragraphs[0].text = "Forge fixtures · hard.docx"
    footer = s.footer.paragraphs[0]
    footer.text = "Page "
    footer._p.append(parse_xml(f'<w:fldSimple {ns} w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>'))

    d.add_paragraph("Hard document", style="Title")
    # A table of contents: a complex field, with its last result shown.
    toc = d.add_paragraph()
    for xml in (
        f'<w:r {ns}><w:fldChar w:fldCharType="begin"/></w:r>',
        f'<w:r {ns}><w:instrText xml:space="preserve"> TOC \\o "1-3" \\h \\z \\u </w:instrText></w:r>',
        f'<w:r {ns}><w:fldChar w:fldCharType="separate"/></w:r>',
        f'<w:r {ns}><w:t>Contents: update this field to fill it in.</w:t></w:r>',
        f'<w:r {ns}><w:fldChar w:fldCharType="end"/></w:r>',
    ):
        toc._p.append(parse_xml(xml))

    d.add_heading("Overview", level=1)
    p = d.add_paragraph("Every kind of run: ")
    for text, fmt in (("bold", dict(bold=True)), ("italic", dict(italic=True)), ("underlined", dict(underline=True)),
                      ("red 14pt", dict(color=RGBColor(0xC0, 0, 0), size=Pt(14))), ("highlighted", dict(highlight=WD_COLOR_INDEX.YELLOW)),
                      ("E=mc", {}), ("2", dict(superscript=True)), ("struck", dict(strike=True))):
        run = p.add_run(text)
        for k, v in fmt.items():
            if k in ("bold", "italic", "underline"):
                setattr(run, k, v)
            elif k == "color":
                run.font.color.rgb = v
            elif k == "highlight":
                run.font.highlight_color = v
            else:
                setattr(run.font, k, v)
        if text not in ("E=mc",):
            p.add_run(", " if text != "struck" else ".")
    # A hyperlink, a bookmark, a footnote reference.
    p2 = d.add_paragraph("See ")
    rid = d.part.relate_to("https://example.com/budget", RT.HYPERLINK, is_external=True)
    p2._p.append(parse_xml(f'<w:hyperlink {ns} xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="{rid}"><w:r><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr><w:t>the budget site</w:t></w:r></w:hyperlink>'))
    p2.add_run(" for the figures")
    p2._p.append(parse_xml(f'<w:r {ns}><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="1"/></w:r>'))
    p2.add_run(". This sentence is ")
    p2._p.append(parse_xml(f'<w:bookmarkStart {ns} w:id="0" w:name="figures"/>'))
    p2.add_run("bookmarked")
    p2._p.append(parse_xml(f'<w:bookmarkEnd {ns} w:id="0"/>'))
    p2.add_run(".")
    # Tracked changes and a comment.
    p3 = d.add_paragraph("Licences were ")
    p3._p.append(parse_xml(f'<w:del {ns} w:id="101" w:author="Ada" w:date="2026-10-01T09:00:00Z"><w:r><w:delText>640</w:delText></w:r></w:del>'))
    p3._p.append(parse_xml(f'<w:ins {ns} w:id="102" w:author="Ada" w:date="2026-10-01T09:00:00Z"><w:r><w:t>700</w:t></w:r></w:ins>'))
    tail = p3.add_run(" after the renewal.")
    d.add_comment([tail], text="Confirm with procurement.", author="Grace", initials="GH")

    d.add_heading("Lists", level=2)
    for text, style in (("First level", "List Bullet"), ("Second level", "List Bullet 2"), ("Third level", "List Bullet 3"), ("Back to first", "List Bullet")):
        d.add_paragraph(text, style=style)
    for text in ("Step one", "Step two", "Step three"):
        d.add_paragraph(text, style="List Number")

    d.add_heading("Tables", level=2)
    t = d.add_table(rows=5, cols=4)
    t.style = "Table Grid"
    for j, h in enumerate(("Item", "Q2", "Q3", "Change")):
        t.cell(0, j).text = h
        t.cell(0, j).paragraphs[0].runs[0].bold = True
    for i, row in enumerate((("Servers", "1,200", "1,350", "+150"), ("Licences", "800", "700", "-100"), ("Travel", "450", "520", "+70")), start=1):
        for j, v in enumerate(row):
            t.cell(i, j).text = v
    merged = t.cell(4, 0).merge(t.cell(4, 2))
    merged.text = "Merged across three columns"
    inner = t.cell(4, 3).add_table(rows=2, cols=2)
    for i in range(2):
        for j in range(2):
            inner.cell(i, j).text = f"n{i}{j}"

    d.add_heading("Picture", level=2)
    d.add_picture(png(os.path.join(tmp, "d.png"), hue=40), width=Mm(70))

    # A content control (structured document tag) around a paragraph.
    sdt = parse_xml(
        f'<w:sdt {ns}><w:sdtPr><w:alias w:val="Client"/><w:tag w:val="client"/><w:id w:val="424242"/><w:text/></w:sdtPr>'
        f'<w:sdtContent><w:p><w:r><w:t>Acme Ltd</w:t></w:r></w:p></w:sdtContent></w:sdt>')
    d.element.body.insert(len(d.element.body) - 1, sdt)

    # A landscape section with its own header.
    s2 = d.add_section(WD_SECTION.NEW_PAGE)
    s2.orientation = WD_ORIENT.LANDSCAPE
    s2.page_width, s2.page_height = Mm(297), Mm(210)
    s2.header.is_linked_to_previous = False
    s2.header.paragraphs[0].text = "Landscape section"
    d.add_heading("Wide table", level=1)
    wide = d.add_table(rows=3, cols=8)
    wide.style = "Table Grid"
    for i in range(3):
        for j in range(8):
            wide.cell(i, j).text = f"R{i + 1}C{j + 1}"
    d.add_paragraph("The end.")

    cp = d.core_properties
    cp.author, cp.last_modified_by, cp.title = "Forge fixtures", "Forge fixtures", "Hard document"
    cp.created = cp.modified = datetime(2026, 10, 1, 9, 0, 0)
    cp.comments = ""
    zoom = d.settings.element.find(f"{{{W}}}zoom")
    if zoom is not None:
        zoom.set(f"{{{W}}}percent", "100")
    buf = io.BytesIO()
    d.save(buf)
    _add_footnotes(buf.getvalue(), os.path.join(HERE, "hard.docx"))


def _add_footnotes(docx_bytes, out):
    """python-docx can't make footnotes: add the part, its relationship and
    its content type to the package."""
    footnotes = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:footnotes xmlns:w="{W}">'
        '<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>'
        '<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>'
        '<w:footnote w:id="1"><w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteRef/></w:r>'
        '<w:r><w:t xml:space="preserve"> All figures are in EUR, excluding VAT.</w:t></w:r></w:p></w:footnote></w:footnotes>'
    )
    with zipfile.ZipFile(io.BytesIO(docx_bytes)) as src, zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as dst:
        for item in src.infolist():
            data = src.read(item.filename)
            if item.filename == "[Content_Types].xml":
                data = data.replace(b"</Types>", b'<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/></Types>')
            elif item.filename == "word/_rels/document.xml.rels":
                data = data.replace(b"</Relationships>", b'<Relationship Id="rIdFootnotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/></Relationships>')
            dst.writestr(item, data)
        dst.writestr("word/footnotes.xml", footnotes)


# --- PowerPoint -------------------------------------------------------------

def make_pptx(tmp):
    from pptx import Presentation
    from pptx.chart.data import CategoryChartData
    from pptx.dml.color import RGBColor
    from pptx.enum.chart import XL_CHART_TYPE, XL_LEGEND_POSITION
    from pptx.enum.dml import MSO_THEME_COLOR
    from pptx.enum.shapes import MSO_CONNECTOR, MSO_SHAPE
    from pptx.enum.text import MSO_ANCHOR, MSO_AUTO_SIZE, PP_ALIGN
    from pptx.oxml.ns import qn
    from pptx.util import Emu, Inches, Pt

    sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "assets", "forge-plugin", "skills", "office-documents", "scripts"))
    import pptx_tool

    prs = Presentation()
    pptx_tool._widen(prs)
    L = prs.slide_layouts
    pic = png(os.path.join(tmp, "p.png"), 480, 300, hue=20)

    def notes(slide, text):
        slide.notes_slide.notes_text_frame.text = text

    # 1 Title.
    s = prs.slides.add_slide(L[0])
    s.shapes.title.text = "Hard deck"
    s.placeholders[1].text = "Every shape Forge has to keep"
    notes(s, "Opening slide. Keep it short.")

    # 2 Formatted bullets, levels, a hyperlink.
    s = prs.slides.add_slide(L[1])
    s.shapes.title.text = "Formatted text"
    tf = s.placeholders[1].text_frame
    tf.clear()
    p = tf.paragraphs[0]
    for text, fmt in (("Bold", "b"), (", italic", "i"), (", accent", "c"), (" and 28pt", "s")):
        r = p.add_run()
        r.text = text
        if fmt == "b":
            r.font.bold = True
        elif fmt == "i":
            r.font.italic = True
        elif fmt == "c":
            r.font.color.theme_color = MSO_THEME_COLOR.ACCENT_2
        else:
            r.font.size = Pt(28)
    for level, text in ((1, "Second level"), (2, "Third level"), (0, "Back to the first")):
        q = tf.add_paragraph()
        q.text, q.level = text, level
    q = tf.add_paragraph()
    r = q.add_run()
    r.text = "A link to the budget"
    r.hyperlink.address = "https://example.com/budget"
    notes(s, "Runs differ in weight, slant, colour and size.")

    # 3 Two content: bullets and a picture.
    s = prs.slides.add_slide(L[3])
    s.shapes.title.text = "Two columns"
    s.placeholders[1].text = "Left column text"
    ph = s.placeholders[2]
    s.shapes.add_picture(pic, ph.left, ph.top, width=ph.width)
    ph._element.getparent().remove(ph._element)

    # 4 A table with merged cells.
    s = prs.slides.add_slide(L[5])
    s.shapes.title.text = "Table"
    t = s.shapes.add_table(5, 4, Inches(1), Inches(1.6), Inches(11.3), Inches(3.5)).table
    for j, h in enumerate(("Item", "Q2", "Q3", "Change")):
        t.cell(0, j).text = h
    for i, row in enumerate((("Servers", "1,200", "1,350", "+150"), ("Licences", "800", "700", "-100"), ("Travel", "450", "520", "+70")), start=1):
        for j, v in enumerate(row):
            t.cell(i, j).text = v
    t.cell(4, 0).merge(t.cell(4, 2))
    t.cell(4, 0).text = "Merged across three columns"
    t.cell(4, 3).text = "+120"

    # 5 A chart.
    s = prs.slides.add_slide(L[5])
    s.shapes.title.text = "Chart"
    data = CategoryChartData()
    data.categories = ["North", "South", "East", "West", "Central"]
    data.add_series("Q2", (12, 15, 9, 14, 7))
    data.add_series("Q3", (14, 13, 11, 16, 9))
    chart = s.shapes.add_chart(XL_CHART_TYPE.COLUMN_CLUSTERED, Inches(1), Inches(1.6), Inches(11), Inches(5.2), data).chart
    chart.has_legend, chart.legend.position = True, XL_LEGEND_POSITION.BOTTOM
    notes(s, "Chart data lives in an embedded workbook.")

    # 6 Shapes: fills, rotation, a connector with an arrow.
    s = prs.slides.add_slide(L[5])
    s.shapes.title.text = "Shapes"
    a = s.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Inches(1), Inches(2), Inches(3), Inches(1.5))
    a.fill.solid()
    a.fill.fore_color.rgb = RGBColor(0x1F, 0x4E, 0x78)
    a.text_frame.text = "Filled RGB"
    b = s.shapes.add_shape(MSO_SHAPE.OVAL, Inches(5.2), Inches(2), Inches(2.5), Inches(2.5))
    b.fill.solid()
    b.fill.fore_color.theme_color = MSO_THEME_COLOR.ACCENT_2
    b.text_frame.text = "Theme"
    c = s.shapes.add_shape(MSO_SHAPE.RECTANGLE, Inches(9), Inches(2.2), Inches(3), Inches(1.2))
    c.rotation = 30
    c.text_frame.text = "Rotated 30°"
    line = s.shapes.add_connector(MSO_CONNECTOR.STRAIGHT, Inches(4), Inches(2.75), Inches(5.2), Inches(3.25))
    ln = line.line._get_or_add_ln()
    ln.append(ln.makeelement(qn("a:tailEnd"), {"type": "triangle"}))

    # 7 Nested groups.
    s = prs.slides.add_slide(L[5])
    s.shapes.title.text = "Groups"
    outer = s.shapes.add_group_shape()
    for i in range(3):
        box = outer.shapes.add_shape(MSO_SHAPE.RECTANGLE, Inches(1 + i * 2.2), Inches(2), Inches(2), Inches(1))
        box.text_frame.text = f"Box {i + 1}"
    inner = outer.shapes.add_group_shape()
    for i in range(2):
        dot = inner.shapes.add_shape(MSO_SHAPE.OVAL, Inches(2 + i * 1.5), Inches(4), Inches(1), Inches(1))
        dot.text_frame.text = f"{i + 1}"

    # 8 A cropped picture and a caption.
    s = prs.slides.add_slide(L[5])
    s.shapes.title.text = "Picture"
    pc = s.shapes.add_picture(pic, Inches(1), Inches(1.6), height=Inches(4.5))
    pc.crop_left, pc.crop_right = 0.1, 0.05
    cap = s.shapes.add_textbox(Inches(8.5), Inches(2), Inches(4), Inches(2)).text_frame
    cap.word_wrap = True
    cap.text = "Cropped 10% left, 5% right."

    # 9 Section header.
    s = prs.slides.add_slide(L[2])
    s.shapes.title.text = "Part two"
    s.placeholders[1].text = "Layouts and backgrounds"

    # 10 Alignment and autofit in text boxes.
    s = prs.slides.add_slide(L[5])
    s.shapes.title.text = "Alignment"
    box = s.shapes.add_textbox(Inches(1), Inches(1.6), Inches(11), Inches(4.5))
    tf = box.text_frame
    tf.word_wrap, tf.vertical_anchor, tf.auto_size = True, MSO_ANCHOR.MIDDLE, MSO_AUTO_SIZE.TEXT_TO_FIT_SHAPE
    for i, (text, align) in enumerate((("Left aligned", PP_ALIGN.LEFT), ("Centred", PP_ALIGN.CENTER), ("Right aligned", PP_ALIGN.RIGHT),
                                       ("Justified text that is long enough to show how justification spreads the words across the full width of the box.", PP_ALIGN.JUSTIFY))):
        q = tf.paragraphs[0] if i == 0 else tf.add_paragraph()
        q.text, q.alignment = text, align

    # 11 A background fill, and hidden.
    s = prs.slides.add_slide(L[6])
    s.background.fill.solid()
    s.background.fill.fore_color.rgb = RGBColor(0xF1, 0xEF, 0xEB)
    tb = s.shapes.add_textbox(Inches(1), Inches(3), Inches(11), Inches(1)).text_frame
    tb.text = "A hidden slide on a warm background"
    s._element.set("show", "0")

    # 12 Comparison and close.
    s = prs.slides.add_slide(L[4])
    s.shapes.title.text = "Before and after"
    for idx, text in ((1, "Before"), (2, "640 licences"), (3, "After"), (4, "700 licences")):
        s.placeholders[idx].text = text
    notes(s, "Close on the change.")

    cp = prs.core_properties
    cp.author, cp.last_modified_by, cp.title, cp.revision = "Forge fixtures", "Forge fixtures", "Hard deck", 1
    cp.created = cp.modified = datetime(2026, 10, 1, 9, 0, 0)
    buf = io.BytesIO()
    prs.save(buf)
    _retheme(buf.getvalue(), os.path.join(HERE, "hard.pptx"))


def _retheme(pptx_bytes, out):
    """A custom theme: the template's Office accents replaced with our own."""
    accents = {b"4F81BD": b"1F4E78", b"C0504D": b"C55A11", b"9BBB59": b"548235", b"8064A2": b"7030A0", b"4BACC6": b"2E75B6", b"F79646": b"BF9000"}
    with zipfile.ZipFile(io.BytesIO(pptx_bytes)) as src, zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as dst:
        for item in src.infolist():
            data = src.read(item.filename)
            if item.filename.startswith("ppt/theme/"):
                for old, new in accents.items():
                    data = data.replace(b'val="' + old + b'"', b'val="' + new + b'"')
                data = data.replace(b'name="Office Theme"', b'name="Forge Fixtures"')
            dst.writestr(item, data)


# --- PDF ---------------------------------------------------------------------

def make_pdf(tmp):
    from pypdf import PdfReader, PdfWriter
    from pypdf.annotations import Highlight, Link, Text
    from pypdf.generic import ArrayObject, FloatObject
    from reportlab.lib.colors import Color, HexColor
    from reportlab.lib.pagesizes import A4
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.pdfgen import canvas

    pdfmetrics.registerFont(TTFont("DejaVu", FONT))
    pdfmetrics.registerFont(TTFont("DejaVu-Bold", FONT_BOLD))
    rnd = random.Random(7)
    raw = os.path.join(tmp, "hard-raw.pdf")
    c = canvas.Canvas(raw, pagesize=A4)
    c.setTitle("Hard PDF")
    c.setAuthor("Forge fixtures")
    c.setCreator("Forge fixtures")
    W_, H_ = A4
    words = "budget forecast licences travel servers contractors training region revenue target quarter".split()

    def text_page(n, title):
        c.bookmarkPage(f"p{n}")
        c.addOutlineEntry(title, f"p{n}", level=0)
        c.setFont("DejaVu-Bold", 18)
        c.drawString(56, H_ - 72, title)
        c.setFont("DejaVu", 10.5)
        y = H_ - 100
        while y > 80:
            c.drawString(56, y, " ".join(rnd.choice(words) for _ in range(14)).capitalize() + ".")
            y -= 15
        c.setFont("DejaVu", 8)
        c.drawRightString(W_ - 56, 40, f"Page {n}")

    # 1: an image, vector paths, a gradient.
    c.bookmarkPage("p1")
    c.addOutlineEntry("Overview", "p1", level=0)
    c.setFont("DejaVu-Bold", 22)
    c.drawString(56, H_ - 80, "Hard PDF")
    c.setFont("DejaVu", 11)
    c.drawString(56, H_ - 104, "Embedded fonts, a picture, paths, a gradient, a form and annotations.")
    c.drawImage(png(os.path.join(tmp, "f.png"), 480, 300, hue=10), 56, H_ - 360, width=240, height=150)
    c.setFillColor(HexColor("#1F4E78"))
    c.rect(330, H_ - 360, 200, 150, fill=1, stroke=0)
    path = c.beginPath()
    path.moveTo(56, H_ - 420)
    path.curveTo(150, H_ - 380, 250, H_ - 480, 350, H_ - 420)
    path.lineTo(350, H_ - 470)
    path.close()
    c.setFillColor(HexColor("#F0AA3C"))
    c.setStrokeColor(HexColor("#333333"))
    c.setLineWidth(1.5)
    c.drawPath(path, fill=1, stroke=1)
    c.saveState()
    p = c.beginPath()
    p.rect(380, H_ - 480, 150, 60)
    c.clipPath(p, stroke=0)
    c.linearGradient(380, 0, 530, 0, (Color(0.12, 0.31, 0.47), Color(0.94, 0.67, 0.24)), extend=False)
    c.restoreState()
    c.setFont("DejaVu", 8)
    c.drawRightString(W_ - 56, 40, "Page 1")
    c.showPage()

    # 2: a table of numbers drawn as text and lines.
    c.bookmarkPage("p2")
    c.addOutlineEntry("Figures", "p2", level=0)
    c.setFont("DejaVu-Bold", 18)
    c.drawString(56, H_ - 72, "Figures")
    rows = [("Item", "Q2", "Q3", "Change"), ("Servers", "1,200", "1,350", "+150"), ("Licences", "800", "700", "-100"), ("Travel", "450", "520", "+70"), ("Total", "5,150", "5,500", "+350")]
    y = H_ - 110
    for i, row in enumerate(rows):
        c.setFont("DejaVu-Bold" if i in (0, len(rows) - 1) else "DejaVu", 11)
        c.drawString(60, y, row[0])
        for j, v in enumerate(row[1:]):
            c.drawRightString(260 + j * 90, y, v)
        c.line(56, y - 5, 440, y - 5)
        y -= 22
    c.setFont("DejaVu", 8)
    c.drawRightString(W_ - 56, 40, "Page 2")
    c.showPage()

    # 3: a form.
    c.bookmarkPage("p3")
    c.addOutlineEntry("Form", "p3", level=0)
    c.setFont("DejaVu-Bold", 18)
    c.drawString(56, H_ - 72, "Approval form")
    c.setFont("DejaVu", 11)
    form = c.acroForm
    c.drawString(56, H_ - 120, "Name")
    form.textfield(name="name", tooltip="Your name", x=160, y=H_ - 128, width=260, height=20, value="")
    c.drawString(56, H_ - 160, "Comments")
    form.textfield(name="comments", tooltip="Comments", x=160, y=H_ - 230, width=260, height=80, value="", fieldFlags="multiline")
    c.drawString(56, H_ - 260, "I agree")
    form.checkbox(name="agree", tooltip="Agree", x=160, y=H_ - 266, buttonStyle="check", checked=False, size=16)
    c.drawString(56, H_ - 300, "Plan")
    for i, v in enumerate(("basic", "pro")):
        form.radio(name="plan", tooltip="Plan", value=v, selected=v == "basic", x=160 + i * 80, y=H_ - 306, size=16, buttonStyle="circle")
        c.drawString(182 + i * 80, H_ - 300, v)
    c.drawString(56, H_ - 340, "Country")
    form.choice(name="country", tooltip="Country", value="France", options=["France", "Germany", "Spain", "Morocco"], x=160, y=H_ - 348, width=160, height=20)
    c.drawString(56, H_ - 380, "Colours")
    form.listbox(name="colours", tooltip="Colours", value="Blue", options=["Red", "Green", "Blue"], x=160, y=H_ - 440, width=160, height=60)
    c.setFont("DejaVu", 8)
    c.drawRightString(W_ - 56, 40, "Page 3")
    c.showPage()

    for n, title in ((4, "Rotated page"), (5, "Highlights"), (6, "Notes"), (7, "More text"), (8, "Last page")):
        text_page(n, title)
        c.showPage()
    c.save()

    r = PdfReader(raw)
    w = PdfWriter(clone_from=r)
    w.pages[3].rotate(90)
    w.add_annotation(page_number=0, annotation=Text(rect=(480, 760, 500, 780), text="Check the gradient.", open=False))
    w.add_annotation(page_number=1, annotation=Link(rect=(56, 600, 300, 620), url="https://example.com/budget"))
    x0, y1 = 56, H_ - 100 - 15 * 2 + 12
    quad = ArrayObject([FloatObject(v) for v in (x0, y1, x0 + 300, y1, x0, y1 - 14, x0 + 300, y1 - 14)])
    w.add_annotation(page_number=4, annotation=Highlight(rect=(x0, y1 - 14, x0 + 300, y1), quad_points=quad, highlight_color="ffff00"))
    w.add_metadata({"/Title": "Hard PDF", "/Author": "Forge fixtures", "/Creator": "Forge fixtures"})
    with open(os.path.join(HERE, "hard.pdf"), "wb") as f:
        w.write(f)

    a = canvas.Canvas(os.path.join(HERE, "appendix.pdf"), pagesize=A4)
    a.setTitle("Appendix")
    a.setAuthor("Forge fixtures")
    for n in (1, 2):
        a.setFont("DejaVu-Bold", 18)
        a.drawString(56, H_ - 72, f"Appendix page {n}")
        a.showPage()
    a.save()


def main():
    with tempfile.TemporaryDirectory() as tmp:
        make_xlsx(tmp)
        make_docx(tmp)
        make_pptx(tmp)
        make_pdf(tmp)
    for name in ("hard.xlsx", "hard-openpyxl.xlsx", "hard.docx", "hard.pptx", "hard.pdf", "appendix.pdf"):
        print(f"wrote {name} ({os.path.getsize(os.path.join(HERE, name)):,} bytes)")


if __name__ == "__main__":
    main()
