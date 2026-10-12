"""Round-trip tests for the document tools: every edit is read back.

    python -m unittest test_tools        (from this folder)

The LibreOffice steps (recalc, convert) are skipped when soffice is missing.
"""

import os
import shutil
import tempfile
import unittest

import docx
import openpyxl
from openpyxl.styles import Font
from pptx import Presentation
from pypdf import PdfReader

import docx_tool
import pdf_tool
import pptx_tool
import xlsx_tool

HAS_OFFICE = bool(shutil.which("soffice") or shutil.which("libreoffice"))


class Tmp(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="forge-docs-")

    def tearDown(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def path(self, name):
        return os.path.join(self.dir, name)


class Excel(Tmp):
    def make_input(self):
        """A budget workbook as a person would have it: styled, with formulas."""
        wb = openpyxl.Workbook()
        ws = wb.active
        ws.title = "Budget"
        ws.append(["Item", "Q2", "Q3", "Change"])
        for c in ws[1]:
            c.font = Font(bold=True)
        ws.append(["Servers", 1200, 1350, "=C2-B2"])
        ws.append(["Licences", 800, 640, "=C3-B3"])
        ws["B4"] = "=SUM(B2:B3)"
        ws["C4"] = "=SUM(C2:C3)"
        for ref in ("B2", "C2", "B3", "C3", "B4", "C4"):
            ws[ref].number_format = "#,##0.00"
        ws.column_dimensions["A"].width = 18
        notes = wb.create_sheet("Notes")
        notes["A1"] = "Owner: finance"
        src = self.path("budget.xlsx")
        wb.save(src)
        return src

    def test_input_and_output_workbooks(self):
        src = self.make_input()
        out = self.path("budget-edited.xlsx")
        n = xlsx_tool.set_cells(src, out, ["Budget!C3=700", "Budget!A5=Travel", "B5=300", "C5=450", "D5==C5-B5"])
        self.assertEqual(n, 5)
        # The input is untouched.
        before = openpyxl.load_workbook(src)
        self.assertEqual(before["Budget"]["C3"].value, 640)
        # The output has the edits, and kept everything else.
        after = openpyxl.load_workbook(out)
        ws = after["Budget"]
        self.assertEqual(ws["C3"].value, 700)
        self.assertEqual(ws["A5"].value, "Travel")
        self.assertEqual(ws["D5"].value, "=C5-B5")
        self.assertEqual(ws["D2"].value, "=C2-B2", "an untouched formula is kept")
        self.assertTrue(ws["A1"].font.bold, "the header stays bold")
        self.assertEqual(ws["C3"].number_format, "#,##0.00", "the number format survives the edit")
        self.assertEqual(ws.column_dimensions["A"].width, 18)
        self.assertEqual(after["Notes"]["A1"].value, "Owner: finance", "other sheets are kept")
        # Appending rows lands after the last one.
        rows = self.path("rows.csv")
        with open(rows, "w", encoding="utf-8") as f:
            f.write("Training,100,150,=C6-B6\n")
        out2 = self.path("budget-appended.xlsx")
        self.assertEqual(xlsx_tool.append_rows(out, out2, xlsx_tool._csv(rows), "Budget"), 1)
        self.assertEqual(openpyxl.load_workbook(out2)["Budget"]["A6"].value, "Training")
        # Reading shows the table with letters and row numbers.
        text = xlsx_tool.read(out2, "Budget", formulas=True)
        self.assertIn("| | A | B | C | D |", text)
        self.assertIn("| 6 | Training | 100 | 150 | =C6-B6 |", text)

    @unittest.skipUnless(HAS_OFFICE, "LibreOffice is not installed")
    def test_recalc_computes_the_formulas(self):
        src = self.make_input()
        edited = self.path("edited.xlsx")
        xlsx_tool.set_cells(src, edited, ["Budget!C3=700"])
        done = self.path("recalculated.xlsx")
        xlsx_tool.recalc(edited, done)
        ws = openpyxl.load_workbook(done, data_only=True)["Budget"]
        self.assertEqual(ws["D3"].value, -100)  # 700 - 800
        self.assertEqual(ws["C4"].value, 2050)  # 1350 + 700
        self.assertEqual(openpyxl.load_workbook(done)["Budget"]["D3"].value, "=C3-B3", "formulas stay formulas")

    def test_create_from_csv(self):
        data = self.path("data.csv")
        with open(data, "w", encoding="utf-8") as f:
            f.write("Name,Score\nAda,9.5\nGrace,10\n")
        out = self.path("scores.xlsx")
        xlsx_tool.create(out, xlsx_tool._csv(data), "Scores")
        ws = openpyxl.load_workbook(out)["Scores"]
        self.assertEqual([c.value for c in ws[2]], ["Ada", 9.5])
        self.assertTrue(ws["A1"].font.bold)


class Word(Tmp):
    MD = "# Release notes\n\nForge now draws **diagrams** and reads *documents*.\n\n## Changes\n\n- Diagrams in replies\n- A document viewer\n\n| Area | Status |\n|---|---|\n| Viewer | Done |\n| **Total** | **2** |\n"

    def test_create_read_replace_append(self):
        src = self.path("notes.docx")
        docx_tool.create(src, self.MD)
        text = docx_tool.read(src)
        self.assertIn("# Release notes", text)
        self.assertIn("## Changes", text)
        self.assertIn("- Diagrams in replies", text)
        self.assertIn("| Area | Status |", text)
        bold = [r for p in docx.Document(src).paragraphs for r in p.runs if r.bold]
        self.assertEqual(bold[0].text, "diagrams")
        total = docx.Document(src).tables[0].rows[2].cells[0].paragraphs[0].runs
        self.assertEqual([(r.text, r.bold) for r in total], [("Total", True)], "inline Markdown in a table cell is formatting, not asterisks")
        out = self.path("notes-2.docx")
        n = docx_tool.replace(src, out, [("diagrams", "architecture diagrams"), ("Done", "Shipped")])
        self.assertEqual(n, 2)
        d = docx.Document(out)
        run = next(r for p in d.paragraphs for r in p.runs if "architecture diagrams" in r.text)
        self.assertTrue(run.bold, "the replaced run keeps its bold")
        self.assertEqual(d.tables[0].rows[1].cells[1].text, "Shipped", "text inside tables is replaced too")
        self.assertIn("Forge now draws diagrams", docx_tool.read(src), "the input is untouched")
        out2 = self.path("notes-3.docx")
        docx_tool.append(out, out2, "## Next\n\n1. Browser use\n")
        self.assertIn("## Next", docx_tool.read(out2))

    def test_replace_across_runs(self):
        d = docx.Document()
        p = d.add_paragraph()
        p.add_run("Quar")
        p.add_run("ter three")
        src = self.path("split.docx")
        d.save(src)
        out = self.path("split-2.docx")
        self.assertEqual(docx_tool.replace(src, out, [("Quarter three", "Q3")]), 1)
        self.assertEqual(docx.Document(out).paragraphs[0].text, "Q3")


class Slides(Tmp):
    OUTLINE = "# Forge 1.0\nA desktop app for Claude Code\n\n# What's new\n- Diagrams\n  - drawn natively\n- Documents\n\n# Next\n- Browser use\n"

    def test_create_read_replace(self):
        out = self.path("deck.pptx")
        with open(self._outline(), encoding="utf-8") as f:
            self.assertEqual(pptx_tool.create(out, f.read()), 3)
        prs = Presentation(out)
        self.assertEqual(len(prs.slides), 3)
        text = pptx_tool.read(out)
        self.assertIn("## Slide 1: Forge 1.0", text)
        self.assertIn("- A desktop app for Claude Code", text)
        self.assertIn("  - drawn natively", text)
        out2 = self.path("deck-2.pptx")
        self.assertEqual(pptx_tool.replace(out, out2, [("Browser use", "Browser use, built in")]), 1)
        self.assertIn("Browser use, built in", pptx_tool.read(out2))

    def _outline(self):
        p = self.path("outline.md")
        with open(p, "w", encoding="utf-8") as f:
            f.write(self.OUTLINE)
        return p


class Pdf(Tmp):
    def test_create_read_merge_pages(self):
        a, b = self.path("a.pdf"), self.path("b.pdf")
        pdf_tool.create(a, "# Report\n\nThe **gateway** adds the key.\n\n- One\n- Two\n\n| K | V |\n|---|---|\n| x | 1 |\n", "Report")
        pdf_tool.create(b, "# Appendix\n\nMore.\n")
        self.assertIn("Report", pdf_tool.read(a))
        self.assertIn("gateway adds the key", pdf_tool.read(a))
        merged = self.path("merged.pdf")
        self.assertEqual(pdf_tool.merge(merged, [a, b]), 2)
        first = self.path("first.pdf")
        self.assertEqual(pdf_tool.pages(merged, first, "2"), 1)
        self.assertIn("Appendix", pdf_tool.read(first))

    def test_fill_a_form(self):
        from reportlab.pdfgen import canvas
        src = self.path("form.pdf")
        c = canvas.Canvas(src)
        c.drawString(72, 720, "Name:")
        c.acroForm.textfield(name="Name", x=120, y=710, width=200, height=20)
        c.save()
        self.assertIn("Name", pdf_tool.fields(src))
        out = self.path("filled.pdf")
        pdf_tool.fill(src, out, {"Name": "Ada Lovelace"})
        self.assertEqual(pdf_tool.fields(out)["Name"], "Ada Lovelace")

    @unittest.skipUnless(HAS_OFFICE, "LibreOffice is not installed")
    def test_convert_word_to_pdf(self):
        src = self.path("memo.docx")
        docx_tool.create(src, "# Memo\n\nThe budget changed.\n")
        out = self.path("memo.pdf")
        pdf_tool.convert(src, out)
        self.assertIn("The budget changed.", pdf_tool.read(out))
        self.assertEqual(len(PdfReader(out).pages), 1)


if __name__ == "__main__":
    unittest.main()
