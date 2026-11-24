"""Round-trip tests for the document tools: every edit is read back.

    python -m unittest test_tools        (from this folder)

The LibreOffice steps (recalc, convert) are skipped when soffice is missing.
"""

import os
import shutil
import tempfile
import unittest

import zipfile

import docx
import openpyxl
from openpyxl.styles import Font
from pptx import Presentation
from pypdf import PdfReader

import check_tool
import docx_tool
import pdf_tool
import pptx_tool
import xlsx_tool

HAS_OFFICE = bool(shutil.which("soffice") or shutil.which("libreoffice"))
HAS_RENDER = HAS_OFFICE and bool(shutil.which("pdftoppm"))


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
            f.write("Name,Score,Count\nAda,9.5,1200\nGrace,10,30\n")
        out = self.path("scores.xlsx")
        xlsx_tool.create(out, xlsx_tool._csv(data), "Scores")
        wb = openpyxl.load_workbook(out)
        ws = wb["Scores"]
        self.assertEqual([c.value for c in ws[2]], ["Ada", 9.5, 1200])
        self.assertTrue(ws["A1"].font.bold)
        self.assertEqual({ws[r].font.name for r in ("A1", "A2", "B3")}, {"Calibri"}, "every font is named, so no app substitutes one")
        self.assertEqual((ws["B2"].number_format, ws["C2"].number_format), ("#,##0.00", "#,##0"))
        self.assertEqual(ws.freeze_panes, "A2", "the header row stays in view")
        self.assertEqual((wb.properties.creator, wb.properties.lastModifiedBy, wb.properties.title), ("KodAct", "KodAct", "Scores"))

    def test_edits_stamp_the_properties(self):
        src = self.make_input()
        out = self.path("stamped.xlsx")
        xlsx_tool.set_cells(src, out, ["Budget!C3=700"])
        props = openpyxl.load_workbook(out).properties
        self.assertEqual(props.lastModifiedBy, "KodAct")
        self.assertEqual(props.creator, "openpyxl", "an edit doesn't claim to have made the file")

    @unittest.skipUnless(HAS_OFFICE, "LibreOffice is not installed")
    def test_recalc_keeps_who_made_the_file(self):
        data = self.path("d.csv")
        with open(data, "w", encoding="utf-8") as f:
            f.write("Item,Value\nA,1\n")
        made = self.path("made.xlsx")
        xlsx_tool.create(made, xlsx_tool._csv(data), "Data")
        done = self.path("done.xlsx")
        xlsx_tool.recalc(made, done)
        self.assertEqual(openpyxl.load_workbook(done).properties.creator, "KodAct")
        self.assertEqual(openpyxl.load_workbook(done)["Data"]["A1"].font.name, "Calibri", "LibreOffice keeps the named font")


class ExcelChecks(Excel):
    def test_a_range_reads_values_and_formulas(self):
        src = self.make_input()
        text = xlsx_tool.read_range(src, "Budget!C2:D3")
        self.assertIn("| C2 | 1350 |  |", text)
        self.assertIn("| D3 |  | =C3-B3 |", text, "openpyxl's own file has no computed values yet")
        self.assertIn("| C2 |", xlsx_tool.read_range(src, "C2"), "one cell, on the active sheet")

    @unittest.skipUnless(HAS_OFFICE, "LibreOffice is not installed")
    def test_recalc_fails_on_formula_errors(self):
        src = self.make_input()
        bad = self.path("bad.xlsx")
        xlsx_tool.set_cells(src, bad, ["E2==B2/0", "E3==NOSUCH(1)"])
        with self.assertRaises(SystemExit) as e:
            xlsx_tool.recalc(bad, self.path("bad-2.xlsx"))
        self.assertIn("Budget!E2: #DIV/0!", str(e.exception.code))
        self.assertIn("Budget!E3: #NAME?", str(e.exception.code))
        self.assertEqual(xlsx_tool.errors(self.path("bad-2.xlsx"))[0], "Budget!E2: #DIV/0!")


class Word(Tmp):
    MD = "# Release notes\n\nKodAct now draws **diagrams** and reads *documents*.\n\n## Changes\n\n- Diagrams in replies\n- A document viewer\n\n| Area | Status |\n|---|---|\n| Viewer | Done |\n| **Total** | **2** |\n"

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
        self.assertEqual(docx.Document(src).tables[0].rows[0].cells[1].paragraphs[0].alignment, None, "a column of words stays left")
        total = docx.Document(src).tables[0].rows[2].cells[0].paragraphs[0].runs
        self.assertEqual([(r.text, r.bold) for r in total], [("Total", True)], "inline Markdown in a table cell is formatting, not asterisks")
        out = self.path("notes-2.docx")
        n = docx_tool.replace(src, out, [("diagrams", "architecture diagrams"), ("Done", "Shipped")])
        self.assertEqual(n, 2)
        d = docx.Document(out)
        run = next(r for p in d.paragraphs for r in p.runs if "architecture diagrams" in r.text)
        self.assertTrue(run.bold, "the replaced run keeps its bold")
        self.assertEqual(d.tables[0].rows[1].cells[1].text, "Shipped", "text inside tables is replaced too")
        self.assertIn("KodAct now draws diagrams", docx_tool.read(src), "the input is untouched")
        out2 = self.path("notes-3.docx")
        docx_tool.append(out, out2, "## Next\n\n1. Browser use\n")
        self.assertIn("## Next", docx_tool.read(out2))

    def test_new_documents_are_schema_clean_and_titled(self):
        out = self.path("memo.docx")
        docx_tool.create(out, "# Q3 memo\n\nText.\n\n1. One\n2. Two\n\nBetween.\n\n1. Again\n", page="a4")
        with zipfile.ZipFile(out) as z:
            settings = z.read("word/settings.xml").decode()
        self.assertRegex(settings, r'<w:zoom [^>]*w:percent="100"', "the schema requires w:percent on w:zoom")
        d = docx.Document(out)
        cp = d.core_properties
        self.assertEqual((cp.title, cp.author, cp.last_modified_by), ("Q3 memo", "KodAct", "KodAct"))
        self.assertGreaterEqual(cp.created.year, 2026, "not python-docx's 2013 template date")
        self.assertEqual((round(d.sections[0].page_width.mm), round(d.sections[0].page_height.mm)), (210, 297))
        ids = [p._p.pPr.numPr.numId.val for p in d.paragraphs if p.style.name == "List Number"]
        self.assertEqual(len(ids), 3)
        self.assertEqual(ids[0], ids[1], "one list keeps counting")
        self.assertNotEqual(ids[1], ids[2], "the second list starts again at 1")

    def test_numbers_in_tables_align_right(self):
        out = self.path("t.docx")
        docx_tool.create(out, "| Item | Q3 | Change |\n|---|---|---|\n| Servers | 1,350 | +150 |\n| **Total** | **5,500** | **+6.8%** |\n")
        t = docx.Document(out).tables[0]
        right = docx_tool.WD_ALIGN_PARAGRAPH.RIGHT
        self.assertEqual([t.rows[r].cells[c].paragraphs[0].alignment for r in range(3) for c in (1, 2)], [right] * 6)
        self.assertIsNone(t.rows[1].cells[0].paragraphs[0].alignment)

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


    MEMO = "# Q3 memo\n\nLicences fall to 700 EUR after the renewal.\n\nTravel rises to 520 EUR.\n"

    def test_comments_anchor_on_the_quoted_words(self):
        src = self.path("memo.docx")
        docx_tool.create(src, self.MEMO)
        out = self.path("memo-2.docx")
        missing = docx_tool.comment(src, out, [("fall to 700", "Say why."), ("not in the memo", "x")])
        self.assertEqual(missing, ["not in the memo"])
        d = docx.Document(out)
        self.assertEqual([c.text for c in d.comments], ["Say why."])
        self.assertEqual(next(iter(d.comments)).author, "KodAct")
        with zipfile.ZipFile(out) as z:
            body = z.read("word/document.xml").decode()
        self.assertIn("commentRangeStart", body)
        self.assertEqual(d.paragraphs[1].text, "Licences fall to 700 EUR after the renewal.", "the text is untouched")
        self.assertIn("Say why.", docx_tool.read(out), "read lists the comments")
        self.assertEqual(check_tool.validate(out), [])

    def test_tracked_changes_and_accepting_them(self):
        src = self.path("memo.docx")
        docx_tool.create(src, self.MEMO)
        out = self.path("memo-tracked.docx")
        self.assertEqual(docx_tool.track(src, out, [("700", "690"), ("Travel rises", "Travel climbs")]), [])
        with zipfile.ZipFile(out) as z:
            body = z.read("word/document.xml").decode()
        self.assertIn('w:author="KodAct"', body)
        self.assertIn("<w:delText", body)
        self.assertEqual(body.count("<w:ins "), 2)
        read = docx_tool.read(out)
        self.assertIn("700", read)
        self.assertIn("690", read)
        self.assertEqual(check_tool.validate(out), [])
        done = self.path("memo-accepted.docx")
        self.assertEqual(docx_tool.accept(out, done), 4)
        d = docx.Document(done)
        self.assertEqual(d.paragraphs[1].text, "Licences fall to 690 EUR after the renewal.")
        self.assertEqual(d.paragraphs[2].text, "Travel climbs to 520 EUR.")


class Slides(Tmp):
    OUTLINE = "# KodAct 1.0\nA desktop app for Claude Code\n\n# What's new\n- Diagrams\n  - drawn natively\n- Documents\n\n# Next\n- Browser use\n"

    def test_create_read_replace(self):
        out = self.path("deck.pptx")
        with open(self._outline(), encoding="utf-8") as f:
            self.assertEqual(pptx_tool.create(out, f.read()), 3)
        prs = Presentation(out)
        self.assertEqual(len(prs.slides), 3)
        text = pptx_tool.read(out)
        self.assertIn("## Slide 1: KodAct 1.0", text)
        self.assertIn("- A desktop app for Claude Code", text)
        self.assertIn("  - drawn natively", text)
        self.assertEqual((prs.slide_width, prs.slide_height), (12192000, 6858000), "16:9, as PowerPoint makes decks")
        for slide in prs.slides:
            for shape in slide.placeholders:
                self.assertLessEqual(shape.left + shape.width, prs.slide_width, "nothing runs off the wide slide")
                self.assertGreater(shape.width, prs.slide_width * 0.6, "placeholders span the wide slide")
        cp = prs.core_properties
        self.assertEqual((cp.title, cp.author, cp.last_modified_by), ("KodAct 1.0", "KodAct", "KodAct"))
        out2 = self.path("deck-2.pptx")
        self.assertEqual(pptx_tool.replace(out, out2, [("Browser use", "Browser use, built in")]), 1)
        self.assertIn("Browser use, built in", pptx_tool.read(out2))

    def test_one_shape_notes_and_slide_moves(self):
        deck = self.path("deck.pptx")
        pptx_tool.create(deck, self.OUTLINE)
        shapes = pptx_tool.read(deck, shapes=True)
        self.assertIn("- #0 ", shapes)
        self.assertIn(": Diagrams / drawn natively / Documents", shapes)
        one = self.path("one.pptx")
        pptx_tool.set_text(deck, one, 2, "#1", "Diagrams, drawn natively\nDocuments, previewed")
        body = Presentation(one).slides[1].shapes[1].text_frame
        self.assertEqual([p.text for p in body.paragraphs], ["Diagrams, drawn natively", "Documents, previewed"])
        self.assertEqual([p.level for p in body.paragraphs], [0, 1], "each line keeps the level of the line it replaces")
        noted = self.path("noted.pptx")
        pptx_tool.notes(one, noted, 2, "Show the panel.")
        self.assertIn("> Notes: Show the panel.", pptx_tool.read(noted))
        twice = self.path("twice.pptx")
        pptx_tool.duplicate(noted, twice, 2)
        prs = Presentation(twice)
        self.assertEqual([s.shapes.title.text for s in prs.slides], ["KodAct 1.0", "What's new", "What's new", "Next"])
        self.assertEqual(prs.slides[2].notes_slide.notes_text_frame.text, "Show the panel.")
        moved = self.path("moved.pptx")
        self.assertEqual(pptx_tool.reorder(twice, moved, [1, 4, 2]), 3)
        self.assertEqual([s.shapes.title.text for s in Presentation(moved).slides], ["KodAct 1.0", "Next", "What's new"])
        for f in (one, noted, twice, moved):
            self.assertEqual(check_tool.validate(f), [], f)
        with self.assertRaises(SystemExit):
            pptx_tool.reorder(twice, moved, [1, 1])

    def test_init_makes_the_real_deck_first_and_add_grows_it(self):
        """The person sees the requested file from the first step: `init` writes a
        valid deck with its designed title slide, and each `add` saves one more slide."""
        deck = self.path("kids.pptx")
        pptx_tool.init(deck, "kids", "What Is Cancer?", "A guide for kids", "Ages 6 to 10")
        self.assertEqual(check_tool.validate(deck), [])
        self.assertEqual(len(Presentation(deck).slides), 1)
        self.assertIn("## Slide 1: What Is Cancer?", pptx_tool.read(deck))
        specs = [
            {"layout": "cards", "title": "Cells", "items": ["One", "Two", "Three"], "notes": "Say hi."},
            {"layout": "steps", "title": "Steps", "items": ["A: first", "B: second", "C: third"], "footer": "Then."},
            {"layout": "statement", "title": "Not your fault", "statement": "Nothing you did caused it.",
             "items": ["Not catching"]},
            {"layout": "faces", "title": "Feelings",
             "items": [{"label": m.title(), "mood": m} for m in ("worried", "angry", "sad", "confused")],
             "lines": ["All okay."]},
            {"layout": "bullets", "title": "Questions", "items": ["Will it hurt?"]},
            {"layout": "closing", "title": "You Are Loved", "items": ["Ask anything."]},
        ]
        for n, spec in enumerate(specs, start=2):
            self.assertEqual(pptx_tool.add(deck, spec), n)
            self.assertEqual(check_tool.validate(deck), [], spec["layout"])
            self.assertFalse(os.path.exists(deck + ".part"), "saved by replace: never half a file")
        text = pptx_tool.read(deck)
        for title in ("Cells", "Steps", "Not your fault", "Feelings", "Questions", "You Are Loved"):
            self.assertIn(f": {title}", text)
        self.assertNotIn("- 2\n", text, "slide numbers and badges are decoration, not content")
        self.assertIn("> Notes: Say hi.", text)
        prs = Presentation(deck)
        self.assertEqual(prs.core_properties.category, "kodact-theme:kids", "add keeps the deck's theme")
        for slide in prs.slides:
            for shape in slide.shapes:
                if shape.rotation:
                    continue
                self.assertGreaterEqual(shape.left, -prs.slide_width, shape.name)
                if shape.name == "Title" or shape.has_text_frame and shape.text_frame.text.strip():
                    self.assertLessEqual(shape.left + shape.width, prs.slide_width, f"{shape.name} runs off")
                    self.assertLessEqual(shape.top + shape.height, prs.slide_height, f"{shape.name} runs off")

    def test_add_refuses_what_it_cannot_draw(self):
        deck = self.path("d.pptx")
        pptx_tool.init(deck, "clean", "T")
        for bad in ({"layout": "spiral", "items": ["x"]}, {"layout": "cards", "title": "No items"},
                    {"layout": "faces", "items": [{"label": "x", "mood": "sleepy"}]}):
            with self.assertRaises(SystemExit):
                pptx_tool.add(deck, bad)
        self.assertEqual(check_tool.validate(deck), [], "a refused slide leaves the deck as it was")
        self.assertEqual(len(Presentation(deck).slides), 1)

    def test_text_shrinks_to_fit_its_box(self):
        import deck_design
        self.assertEqual(deck_design.fit_pt("Short", 10, 1, 24), 24)
        long = "word " * 120
        self.assertLess(deck_design.fit_pt(long, 6, 1.5, 24), 24)

    @unittest.skipUnless(HAS_OFFICE, "LibreOffice is not installed")
    def test_office_render_route_is_named_not_searched(self):
        os.environ["KODACT_OFFICE_RENDER"] = "none"
        try:
            with self.assertRaises(SystemExit) as e:
                check_tool.to_pdf(self._outline().replace(".md", ".pptx"), self.dir)
            self.assertIn("don't look for other tools", str(e.exception))
        finally:
            del os.environ["KODACT_OFFICE_RENDER"]

    def test_word_init_and_blocks_make_a_valid_designed_document(self):
        import doc_design
        doc = self.path("report.docx")
        docx_tool.init(doc, "corporate", "Q3 Review", "Results", "Confidential")
        self.assertEqual(check_tool.validate(doc), [])
        blocks = [
            {"block": "toc"},
            {"block": "heading", "text": "Summary"},
            {"block": "text", "text": "Up **23%**.", "lead": True},
            {"block": "kpis", "items": [{"value": "185k", "label": "Revenue", "delta": "+23%"},
                                        {"value": "2.1%", "label": "Churn", "delta": "-0.4%", "better": "down"}]},
            {"block": "callout", "kind": "warning", "title": "Risk", "text": "Costs rise."},
            {"block": "table", "columns": ["Item", "Q3"], "rows": [["Servers", 1350], ["Total", 1990]], "highlight": 1},
            {"block": "chart", "chart": {"type": "column", "categories": ["Q1", "Q2"], "series": {"A": [1, 2]}}},
            {"block": "chart", "chart": {"type": "doughnut", "categories": ["a", "b"], "series": {"S": [60, 40]}}},
            {"block": "steps", "items": ["Hire: two", "Ship: it"]},
            {"block": "timeline", "items": ["Oct: open", "Dec: launch"]},
            {"block": "compare", "left": {"title": "Build", "items": ["x"]}, "right": {"title": "Buy", "items": ["y"]},
             "verdict": "Buy."},
            {"block": "cards", "items": ["Quality: checked", "Speed: weekly"]},
            {"block": "quote", "quote": "Ship small.", "by": "Us"},
            {"block": "bullets", "items": ["One", "Two"], "numbered": True},
            {"block": "pagebreak"},
        ]
        self.assertEqual(docx_tool.add(doc, blocks), len(blocks))
        self.assertEqual(check_tool.validate(doc), [])
        self.assertFalse(os.path.exists(doc + ".part"))
        text = docx_tool.read(doc)
        for words in ("# Summary", "185k", "Costs rise.", "Servers", "Hire", "Ship small."):
            self.assertIn(words, text)
        d = docx.Document(doc)
        self.assertEqual(d.core_properties.category, "kodact-theme:corporate")
        self.assertEqual(d.styles["Heading 1"].font.name, "Segoe UI Semibold")
        self.assertEqual(len(d.inline_shapes), 2, "both charts are pictures in the text")
        # Every table, row and cell property in the order Word's schema wants.
        for kind, order in doc_design.ORDER.items():
            for el in d.element.body.iter("{http://schemas.openxmlformats.org/wordprocessingml/2006/main}" + kind):
                names = [c.tag.split("}")[1] for c in el]
                ranks = [order.index(n) for n in names if n in order]
                self.assertEqual(ranks, sorted(ranks), f"{kind}: {names}")
        with self.assertRaises(SystemExit):
            docx_tool.add(doc, {"block": "hologram"})

    def test_audience_picks_the_theme(self):
        import deck_design
        self.assertEqual(deck_design.for_audience("kids aged 8"), "kids")
        self.assertEqual(deck_design.for_audience("the board"), "corporate")
        self.assertEqual(deck_design.for_audience("parents of patients"), "warm")
        self.assertEqual(deck_design.for_audience("a research report"), "editorial")
        self.assertEqual(deck_design.for_audience("conference keynote"), "bold")
        self.assertEqual(deck_design.for_audience(None), "clean")

    def test_every_deck_layout_draws_in_every_theme(self):
        import deck_design
        from PIL import Image
        pic = self.path("pic.png")
        Image.new("RGB", (40, 30), "red").save(pic)
        specs = [
            {"layout": "chart", "title": "C", "chart": {"type": k, "categories": ["a", "b"],
                                                         "series": {"S": [1, 2]}}} for k in deck_design.CHARTS
        ] + [
            {"layout": "table", "title": "T", "columns": ["a", "b"], "rows": [["x", 1]]},
            {"layout": "timeline", "title": "T", "items": ["Jan: a", "Feb: b"]},
            {"layout": "compare", "title": "C", "left": {"title": "A", "items": ["x"]},
             "right": {"title": "B", "items": ["y"]}, "verdict": "A"},
            {"layout": "grid", "title": "G", "items": ["A: a", "B: b", "C: c"]},
            {"layout": "kpis", "title": "K", "items": [{"value": "1", "label": "x", "delta": "+1"}]},
            {"layout": "quote", "title": "", "quote": "Q", "by": "B"},
            {"layout": "image", "title": "I", "path": pic, "items": ["x"]},
            {"layout": "cycle", "title": "C", "items": ["a", "b", "c"], "center": "x"},
            {"layout": "pyramid", "title": "P", "items": ["a", "b", "c"]},
        ]
        for theme in deck_design.THEMES:
            deck = self.path(f"{theme}.pptx")
            pptx_tool.init(deck, theme, "T")
            for spec in specs:
                pptx_tool.add(deck, spec)
            self.assertEqual(check_tool.validate(deck), [], theme)
            self.assertEqual(len(Presentation(deck).slides), 1 + len(specs))

    def test_excel_init_and_blocks_make_a_live_designed_workbook(self):
        book = self.path("budget.xlsx")
        xlsx_tool.init(book, "corporate", "Q3 Budget", "Costs by item")
        made = xlsx_tool.add(book, [
            {"block": "table", "heading": "Costs", "name": "Costs", "columns": ["Item", "Q2", "Q3", "Change"],
             "rows": [["Servers", 1200, 1350], ["Licences", 800, 640]], "formulas": {"Change": "=[@Q3]-[@Q2]"},
             "formats": {"Q3": "eur"}, "totals": {"Q3": "sum"}, "bars": ["Q3"], "icons": ["Change"],
             "scale": ["Q2"]},
            {"block": "kpis", "items": [{"label": "Total", "formula": "=SUM(D8:D9)", "format": "eur"}]},
            {"block": "chart", "type": "column", "data": "C7:D9", "categories": "B8:B9", "title": "Q2 vs Q3"},
            {"block": "note", "text": "Costs rise in January.", "kind": "warning"},
            {"block": "sheet", "sheet": "Split", "title": "Split"},
            {"block": "chart", "sheet": "Split", "type": "doughnut", "data": "C7:D9", "categories": "B8:B9",
             "at": "B6"},
        ])
        self.assertEqual(made[0], "B7:E9")
        self.assertEqual(check_tool.validate(book), [])
        wb = openpyxl.load_workbook(book)
        ws = wb["Summary"]
        self.assertEqual(ws["E8"].value, "=D8-C8", "a row formula becomes plain references")
        self.assertEqual(ws["D10"].value, "=SUM(D8:D9)", "the totals row is a formula")
        self.assertIn("Costs", ws.tables, "a real Excel Table, with filters")
        self.assertEqual(len(ws.conditional_formatting), 3)
        self.assertEqual(len(ws._charts), 1)
        self.assertEqual(len(wb["Split"]._charts), 1)
        self.assertEqual(wb.properties.category, "kodact-theme:corporate")
        self.assertTrue(ws.sheet_properties.pageSetUpPr.fitToPage)
        for bad in ({"block": "table", "columns": ["a"]}, {"block": "chart", "type": "radar", "data": "A1:A2",
                     "categories": "B1:B2"}, {"block": "nope"}):
            with self.assertRaises(SystemExit):
                xlsx_tool.add(book, bad)
        if HAS_OFFICE:
            out = self.path("budget-calc.xlsx")
            xlsx_tool.recalc(book, out)
            self.assertEqual(openpyxl.load_workbook(out, data_only=True)["Summary"]["D10"].value, 1990)

    def test_icons_sections_theme_and_the_contact_sheet(self):
        import icons
        from lxml import etree
        self.assertGreater(len(icons.names()), 1400)
        self.assertEqual(icons.find("doctor"), "stethoscope")
        self.assertEqual(icons.find("brain"), "brain")
        for name in ("droplet", "bone", "brain", "hand", "dna", "chart-pie"):
            self.assertTrue(icons.geometry(name), name)
        png = self.path("brain.png")
        icons.png("brain", png, 64, "9D4EDD")
        self.assertTrue(os.path.getsize(png) > 200)
        deck = self.path("k.pptx")
        pptx_tool.init(deck, "kids", "What Is Cancer?", "For kids", "Ages 6-10", style="dark")
        pptx_tool.add(deck, {"layout": "feature", "section": "Meet your cells", "title": "Cells",
                             "stat": "Trillions", "stat_text": "of cells", "note": "Tiny.",
                             "items": [{"icon": "hand", "head": "Skin", "text": "Keeps germs out"},
                                       {"icon": "droplet", "head": "Blood", "text": "Carries air"}]})
        pptx_tool.add(deck, {"layout": "grid", "title": "Words", "items": [{"icon": "pill", "head": "Medicine",
                                                                          "text": "x"}]})
        pptx_tool.add(deck, {"layout": "cards", "section": "Feelings", "title": "Help", "items": ["Ask"]})
        self.assertEqual(check_tool.validate(deck), [])
        prs = Presentation(deck)
        self.assertEqual([n for n, ids in pptx_tool._sections(prs)], ["Opening", "Meet your cells", "Feelings"])
        self.assertEqual(sum(len(ids) for _, ids in pptx_tool._sections(prs)), 4, "every slide is in a section")
        icon_shapes = [s for s in prs.slides[1].shapes if s.name.startswith("Icon ")]
        self.assertGreaterEqual(len(icon_shapes), 2, "icons are native shapes")
        self.assertIsNotNone(icon_shapes[0]._element.find(".//{http://schemas.openxmlformats.org/drawingml/2006/main}custGeom"))
        theme = next(r.target_part for r in prs.slide_master.part.rels.values() if r.reltype.endswith("/theme"))
        root = etree.fromstring(theme.blob)
        A = "{http://schemas.openxmlformats.org/drawingml/2006/main}"
        self.assertEqual(root.find(f".//{A}accent1/{A}srgbClr").get("val"), "FF6B6B")
        self.assertEqual(root.find(f".//{A}majorFont/{A}latin").get("typeface"), "Century Gothic")
        if HAS_OFFICE:
            out = self.path("sheet.png")
            self.assertEqual(check_tool.sheet(deck, out), 4)
            self.assertTrue(os.path.getsize(out) > 1000)

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
        meta = PdfReader(a).metadata
        self.assertEqual((meta.title, meta.author), ("Report", "KodAct"))
        self.assertEqual(PdfReader(b).metadata.title, "Appendix", "the title defaults to the first heading")
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
        meta = PdfReader(out).metadata
        self.assertEqual((meta.title, meta.author), ("Memo", "KodAct"), "the Word file's properties carry into the PDF")


if __name__ == "__main__":
    unittest.main()


class Checks(Tmp):
    def test_validate_names_what_is_broken(self):
        good = self.path("ok.docx")
        docx_tool.create(good, "# Fine\n")
        self.assertEqual(check_tool.validate(good), [])
        broken = self.path("broken.docx")
        with zipfile.ZipFile(good) as src, zipfile.ZipFile(broken, "w") as dst:
            for item in src.infolist():
                data = src.read(item.filename)
                dst.writestr(item, b"<w:document" if item.filename == "word/document.xml" else data)
        self.assertIn("word/document.xml is not well-formed XML", check_tool.validate(broken)[0])
        not_zip = self.path("plain.xlsx")
        with open(not_zip, "w") as f:
            f.write("a,b\n")
        self.assertEqual(check_tool.validate(not_zip), ["it is not a zip, so not an Office file"])
        self.assertEqual(check_tool.validate(self.path("gone.pptx")), ["there is no such file"])

    @unittest.skipUnless(HAS_RENDER, "LibreOffice or pdftoppm is not installed")
    def test_render_draws_every_slide(self):
        deck = self.path("deck.pptx")
        pptx_tool.create(deck, Slides.OUTLINE)
        images = check_tool.render(deck, self.path("pages"), dpi=30)
        self.assertEqual([os.path.basename(p) for p in images], ["deck-1.png", "deck-2.png", "deck-3.png"])
        with open(images[0], "rb") as f:
            self.assertEqual(f.read(8), b"\x89PNG\r\n\x1a\n")
