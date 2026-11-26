"""KodAct's document skills (assets/kodact-plugin/skills: pptx, docx, xlsx, pdf), run as the
model runs them: commands, their output and exit codes, and the files they write.

    python3 -m pytest tests/skills -q

Each test names a behaviour that once went wrong or that the skill's instructions promise.
Rendering and recalculation need LibreOffice (or Office on Windows); those tests are skipped
where neither is installed.
"""

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
PLUGIN = ROOT / "assets" / "kodact-plugin"
SKILLS = PLUGIN / "skills"
FIXTURES = ROOT / "tests" / "fixtures" / "office"
HAS_OFFICE = bool(shutil.which("soffice") or shutil.which("libreoffice"))
needs_office = pytest.mark.skipif(not HAS_OFFICE, reason="LibreOffice is not installed")


def run(skill, script, *args, cwd, ok=True):
    """Run a skill's script; returns (exit code, stdout + stderr)."""
    r = subprocess.run([sys.executable, str(SKILLS / skill / "scripts" / script), *map(str, args)],
                       cwd=cwd, capture_output=True, text=True, timeout=300)
    out = r.stdout + r.stderr
    if ok is True:
        assert r.returncode == 0, out
    elif ok is False:
        assert r.returncode != 0, out
    return r.returncode, out


# ---------- every skill ----------

@pytest.mark.parametrize("skill", ["pptx", "docx", "xlsx", "pdf"])
def test_every_script_the_instructions_name_exists(skill):
    text = (SKILLS / skill / "SKILL.md").read_text(encoding="utf-8")
    named = set(re.findall(r'\$T/([a-z_]+\.py)', text))
    named |= set(re.findall(r'KODACT_SKILL_DIR\}/scripts/([a-z_]+\.py)', text))
    assert named, "the instructions name no script"
    for script in named:
        assert (SKILLS / skill / "scripts" / script).is_file(), f"{skill}: {script} is named but missing"
    for ref in re.findall(r'KODACT_SKILL_DIR\}/\.\./([a-z]+/scripts/[a-z_]+\.py)', text):
        assert (SKILLS / ref).is_file(), ref
    for ref in re.findall(r'`(references/[a-z_]+\.md)`', text):
        assert (SKILLS / skill / ref).is_file(), ref


def test_the_design_and_icon_code_exists_once():
    for name in ("deck_design.py", "icons.py"):
        assert (PLUGIN / "scripts" / name).is_file()
        copies = list(SKILLS.glob(f"*/scripts/{name}"))
        assert not copies, f"copies of {name}: {copies}"
    assert not list(SKILLS.glob("*/scripts/icons/lucide.json"))


# ---------- pptx ----------

ALL_LAYOUTS = [
    {"layout": "feature", "title": "Revenue grew", "stat": "42%", "stat_text": "year over year",
     "items": [{"icon": "rocket", "head": "Launch", "text": "Three regions"}]},
    {"layout": "cards", "title": "Cells", "items": ["Tiny parts", "Each has a job"], "marks": "star"},
    {"layout": "steps", "title": "Process", "items": ["Plan: scope", "Build: code", "Ship: release"]},
    {"layout": "statement", "title": "Why", "statement": "Documents people want to read."},
    {"layout": "faces", "title": "Feelings", "items": [{"label": "Happy", "mood": "happy"}]},
    {"layout": "bullets", "title": "Agenda", "items": ["One", "Two"]},
    {"layout": "chart", "title": "Sales", "chart": {"type": "column", "categories": ["Q1", "Q2"],
                                                    "series": {"2026": [1, 2]}}},
    {"layout": "table", "title": "Plan", "columns": ["Item", "Cost"], "rows": [["A", 1], ["B", 2]]},
    {"layout": "timeline", "title": "Roadmap", "items": ["Jan: kickoff", "Jun: launch"]},
    {"layout": "compare", "title": "Options", "left": {"title": "Build", "items": ["Control"]},
     "right": {"title": "Buy", "items": ["Speed"]}, "verdict": "Buy"},
    {"layout": "grid", "title": "Features", "items": ["Fast: one second", "Safe: checked", "Clear: themed"]},
    {"layout": "kpis", "title": "Numbers", "items": [{"value": "1.2M", "label": "Users", "delta": "+12%"}]},
    {"layout": "quote", "title": "Voices", "quote": "Best deck tool we used.", "by": "A customer"},
    {"layout": "cycle", "title": "Loop", "items": ["Build", "Measure", "Learn"]},
    {"layout": "pyramid", "title": "Needs", "items": ["Top: a", "Middle: b", "Base: c"]},
    {"layout": "closing", "title": "Thank you", "items": ["Questions?"]},
]


@pytest.mark.parametrize("theme", ["clean", "kids", "corporate", "bold", "editorial", "warm"])
def test_a_designed_deck_in_every_theme_passes_the_check_without_warnings(tmp_path, theme):
    run("pptx", "pptx_tool.py", "init", "d.pptx", "--theme", theme, "--title", "All", cwd=tmp_path)
    (tmp_path / "s.json").write_text(json.dumps(ALL_LAYOUTS))
    _, out = run("pptx", "pptx_tool.py", "add", "d.pptx", "--json", "@s.json", cwd=tmp_path)
    assert "note:" not in out
    _, out = run("pptx", "validate.py", "d.pptx", cwd=tmp_path)
    assert "PASSED (0 warning(s))" in out, out


def test_faces_without_lines_and_an_image_without_points_do_not_crash(tmp_path):
    # _rows(0) divided by zero (2026-11-26).
    from PIL import Image

    Image.new("RGB", (40, 30), (200, 80, 80)).save(tmp_path / "p.png")
    run("pptx", "pptx_tool.py", "init", "d.pptx", "--title", "T", cwd=tmp_path)
    run("pptx", "pptx_tool.py", "add", "d.pptx", "--json",
        json.dumps([{"layout": "faces", "title": "F", "items": ["Happy"]},
                    {"layout": "image", "title": "I", "path": "p.png"}]), cwd=tmp_path)


def test_items_past_a_layouts_limit_are_named_not_dropped_silently(tmp_path):
    run("pptx", "pptx_tool.py", "init", "d.pptx", "--title", "T", cwd=tmp_path)
    _, out = run("pptx", "pptx_tool.py", "add", "d.pptx", "--json",
                 json.dumps({"layout": "kpis", "title": "K", "items": [{"value": str(i), "label": "x"} for i in range(6)]}),
                 cwd=tmp_path)
    assert "shows 4 items; 2 more were left out" in out


def test_text_too_long_for_its_box_is_said_at_build_and_fails_the_check(tmp_path):
    run("pptx", "pptx_tool.py", "init", "d.pptx", "--title", "T", cwd=tmp_path)
    _, out = run("pptx", "pptx_tool.py", "add", "d.pptx", "--json",
                 json.dumps({"layout": "cards", "title": "Long", "items": [" ".join(["word"] * 160)]}), cwd=tmp_path)
    assert "too long for its box" in out
    _, out = run("pptx", "validate.py", "d.pptx", cwd=tmp_path, ok=False)
    assert "overflows the box" in out


def test_the_check_fails_on_cut_off_and_overlapping_text_and_flags_an_off_centre_title(tmp_path):
    from pptx import Presentation
    from pptx.util import Inches

    p = Presentation()
    p.slide_width, p.slide_height = Inches(13.333), Inches(7.5)
    s = p.slides.add_slide(p.slide_layouts[0])
    s.shapes.title.text = "Off centre: the default template is laid out for 4:3"
    s.placeholders[1].text = "x"
    s = p.slides.add_slide(p.slide_layouts[6])
    s.shapes.add_textbox(Inches(1), Inches(1), Inches(5), Inches(2)).text_frame.text = "A"
    s.shapes.add_textbox(Inches(2), Inches(1.5), Inches(5), Inches(2)).text_frame.text = "B"
    s.shapes.add_textbox(Inches(12), Inches(6), Inches(4), Inches(2)).text_frame.text = "cut off"
    p.save(tmp_path / "bad.pptx")
    _, out = run("pptx", "validate.py", "bad.pptx", cwd=tmp_path, ok=False)
    assert "runs off the slide" in out
    assert "overlap (60%)" in out
    assert "off the slide's centre" in out


def test_decoration_may_bleed_off_the_slide(tmp_path):
    from pptx import Presentation
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Inches

    p = Presentation()
    s = p.slides.add_slide(p.slide_layouts[6])
    s.shapes.add_shape(MSO_SHAPE.OVAL, Inches(-1), Inches(-1), Inches(3), Inches(3))
    p.save(tmp_path / "d.pptx")
    _, out = run("pptx", "validate.py", "d.pptx", cwd=tmp_path)
    assert "PASSED (0 warning(s))" in out


def test_editing_tools_keep_working_on_a_hard_deck(tmp_path):
    shutil.copy(FIXTURES / "hard.pptx", tmp_path)
    _, out = run("pptx", "pptx_tool.py", "read", "hard.pptx", "--shapes", cwd=tmp_path)
    assert "#0" in out
    run("pptx", "pptx_tool.py", "duplicate", "hard.pptx", "dup.pptx", "--slide", "1", cwd=tmp_path)
    run("pptx", "template_ops.py", "inventory", "dup.pptx", "-o", "inv.json", cwd=tmp_path)
    run("pptx", "extract_text.py", "dup.pptx", cwd=tmp_path)


@needs_office
def test_a_deck_renders_to_a_contact_sheet(tmp_path):
    run("pptx", "pptx_tool.py", "init", "d.pptx", "--title", "T", cwd=tmp_path)
    run("pptx", "render_preview.py", "d.pptx", "--out-dir", "prev", "--grid", "--dpi", "30", cwd=tmp_path)
    assert (tmp_path / "prev" / "grid-1.png").is_file()


# ---------- docx ----------

BLOCKS = [
    {"block": "heading", "text": "Summary", "level": 1},
    {"block": "text", "text": "Revenue grew **23%**.", "lead": True},
    {"block": "kpis", "items": [{"value": "185k", "label": "Revenue", "delta": "+23%"}]},
    {"block": "callout", "kind": "warning", "title": "Watch", "text": "Costs rose."},
    {"block": "bullets", "items": ["One", "Two"], "numbered": True},
    {"block": "table", "columns": ["Item", "Cost"], "rows": [["A", 1], ["B", 2]], "caption": "Costs"},
    {"block": "cards", "items": [{"head": "Fast", "text": "Quick", "icon": "zap"}]},
    {"block": "steps", "items": ["Plan: scope", "Build: code"]},
    {"block": "timeline", "items": ["Jan: kickoff"]},
    {"block": "compare", "left": {"title": "A", "items": ["x"]}, "right": {"title": "B", "items": ["y"]}},
    {"block": "quote", "quote": "Great.", "by": "User"},
    {"block": "chart", "chart": {"type": "column", "categories": ["Q1", "Q2"], "series": {"S": [1, 2]}}},
    {"block": "toc"}, {"block": "pagebreak"},
]


@pytest.mark.parametrize("theme", ["clean", "kids", "corporate"])
def test_a_designed_document_with_every_block_is_valid(tmp_path, theme):
    run("docx", "docx_tool.py", "init", "r.docx", "--theme", theme, "--title", "Q3", cwd=tmp_path)
    (tmp_path / "b.json").write_text(json.dumps(BLOCKS))
    run("docx", "docx_tool.py", "add", "r.docx", "--json", "@b.json", cwd=tmp_path)
    _, out = run("docx", "validate.py", "r.docx", cwd=tmp_path)
    assert "PASSED (0 warning(s))" in out


def test_the_cover_pads_its_text_inside_the_band(tmp_path):
    # LibreOffice ignored the table's default margins: the title touched the band's edge.
    import zipfile

    run("docx", "docx_tool.py", "init", "r.docx", "--title", "Q3", cwd=tmp_path)
    xml = zipfile.ZipFile(tmp_path / "r.docx").read("word/document.xml").decode()
    assert "<w:tcMar>" in xml


def test_markdown_tables_take_the_documents_theme(tmp_path):
    import zipfile

    run("docx", "docx_tool.py", "init", "r.docx", "--theme", "kids", "--title", "T", cwd=tmp_path)
    (tmp_path / "b.md").write_text("| Part | Job |\n|---|---|\n| Heart | Pumps |\n")
    run("docx", "docx_tool.py", "append", "r.docx", "r.docx", "--markdown", "b.md", cwd=tmp_path)
    xml = zipfile.ZipFile(tmp_path / "r.docx").read("word/document.xml").decode()
    assert "TableGrid" not in xml and "w:tblHeader" in xml


def test_comments_and_tracked_changes_keep_the_original_recoverable(tmp_path):
    (tmp_path / "m.md").write_text("# Memo\n\nPayment within 30 days of the invoice.\n")
    run("docx", "docx_tool.py", "create", "m.docx", "--markdown", "m.md", cwd=tmp_path)
    run("docx", "docx_tool.py", "comment", "m.docx", "c.docx", "--quote", "30 days", "--text", "Why?", cwd=tmp_path)
    run("docx", "docx_tool.py", "track", "c.docx", "t.docx", "--find", "30 days", "--replace", "45 days", cwd=tmp_path)
    run("docx", "validate.py", "t.docx", "--baseline", "m.docx", cwd=tmp_path)
    _, out = run("docx", "docx_tool.py", "read", "t.docx", cwd=tmp_path)
    assert "45 days" in out or "30 days" in out


def test_a_hard_document_reads(tmp_path):
    shutil.copy(FIXTURES / "hard.docx", tmp_path)
    _, out = run("docx", "docx_tool.py", "read", "hard.docx", cwd=tmp_path)
    assert out.strip()


# ---------- xlsx ----------

def test_a_workbook_never_computed_fails_the_audit(tmp_path):
    # It reported "passed" with no values at all (2026-11-26).
    (tmp_path / "d.csv").write_text("Item,Q2,Q3\nA,1,2\n")
    run("xlsx", "xlsx_tool.py", "create", "b.xlsx", "--csv", "d.csv", cwd=tmp_path)
    run("xlsx", "xlsx_tool.py", "set", "b.xlsx", "b.xlsx", "--cell", "Sheet1!D2==C2-B2", cwd=tmp_path)
    _, out = run("xlsx", "recalc_and_verify.py", "b.xlsx", "--scan-only", cwd=tmp_path, ok=False)
    assert "was not recalculated" in out


def test_a_wrong_sheet_name_lists_the_sheets(tmp_path):
    (tmp_path / "d.csv").write_text("Item,Q2\nA,1\n")
    run("xlsx", "xlsx_tool.py", "create", "b.xlsx", "--csv", "d.csv", cwd=tmp_path)
    _, out = run("xlsx", "xlsx_tool.py", "set", "b.xlsx", "b.xlsx", "--cell", "Data!A1=1", cwd=tmp_path, ok=False)
    assert "no sheet 'Data'; the sheets are 'Sheet1'" in out and "Traceback" not in out


@needs_office
def test_recalculation_computes_values_and_the_audit_names_formula_errors(tmp_path):
    (tmp_path / "d.csv").write_text("Item,Q2,Q3\nA,800,640\n")
    run("xlsx", "xlsx_tool.py", "create", "b.xlsx", "--csv", "d.csv", cwd=tmp_path)
    run("xlsx", "xlsx_tool.py", "set", "b.xlsx", "b.xlsx", "--cell", "Sheet1!D2==C2-B2",
        "--cell", "Sheet1!E2==1/0", cwd=tmp_path)
    _, out = run("xlsx", "recalc_and_verify.py", "b.xlsx", "-o", "c.xlsx", cwd=tmp_path, ok=False)
    assert "Sheet1!E2]: #DIV/0!" in out
    _, out = run("xlsx", "xlsx_tool.py", "read", "c.xlsx", cwd=tmp_path)
    assert "-160" in out


def test_a_designed_workbook_builds(tmp_path):
    run("xlsx", "xlsx_tool.py", "init", "w.xlsx", "--title", "Budget", cwd=tmp_path)
    _, out = run("xlsx", "xlsx_tool.py", "add", "w.xlsx", "--json", json.dumps([
        {"block": "table", "columns": ["Item", "Q2", "Q3", "Change"], "rows": [["A", 1, 2, None]],
         "formulas": {"Change": "=[@Q3]-[@Q2]"}, "totals": {"Q3": "sum"}, "scale": ["Change"]},
        {"block": "note", "text": "ok"}]), cwd=tmp_path)
    assert "table at" in out


def test_a_hard_workbook_inspects(tmp_path):
    shutil.copy(FIXTURES / "hard.xlsx", tmp_path)
    _, out = run("xlsx", "inspect_workbook.py", "hard.xlsx", cwd=tmp_path)
    assert "Sheet" in out and "Warning" not in out


# ---------- pdf ----------

def _form(path):
    from reportlab.lib.pagesizes import letter
    from reportlab.pdfgen import canvas

    c = canvas.Canvas(str(path), pagesize=letter)
    f = c.acroForm
    f.textfield(name="name", x=130, y=710, width=200, height=20)
    f.checkbox(name="agree", x=130, y=675, size=15)
    f.choice(name="colour", value="Red", options=["Red", "Blue"], x=130, y=630, width=100, height=20)
    c.drawString(72, 720, "Name:")
    c.showPage()
    c.save()


def test_a_form_is_inspected_filled_and_wrong_values_refused(tmp_path):
    _form(tmp_path / "f.pdf")
    _, out = run("pdf", "inspect_forms.py", "f.pdf", cwd=tmp_path)
    assert "fillable_acroform" in out
    assert "/BBox" not in out  # a text field's drawing is not a list of states
    (tmp_path / "v.json").write_text(json.dumps({"name": "Ada", "agree": True, "colour": "Blue"}))
    run("pdf", "fill_form.py", "f.pdf", "v.json", "-o", "out.pdf", cwd=tmp_path)
    _, out = run("pdf", "inspect_forms.py", "out.pdf", cwd=tmp_path)
    assert "value='Ada'" in out and "value='/Yes'" in out
    (tmp_path / "bad.json").write_text(json.dumps({"colour": "Green"}))
    run("pdf", "fill_form.py", "f.pdf", "bad.json", "-o", "x.pdf", cwd=tmp_path, ok=False)


def test_pages_merge_split_and_rotate(tmp_path):
    _form(tmp_path / "a.pdf")
    run("pdf", "merge_split.py", "merge", "a.pdf", "a.pdf", "-o", "m.pdf", cwd=tmp_path)
    run("pdf", "merge_split.py", "split", "m.pdf", "--pages", "2", "-o", "s.pdf", cwd=tmp_path)
    run("pdf", "merge_split.py", "rotate", "s.pdf", "--angle", "90", "-o", "r.pdf", cwd=tmp_path)
    from pypdf import PdfReader

    assert len(PdfReader(tmp_path / "m.pdf").pages) == 2
    assert PdfReader(tmp_path / "r.pdf").pages[0].rotation == 90


def test_text_comes_out_of_a_hard_pdf(tmp_path):
    shutil.copy(FIXTURES / "hard.pdf", tmp_path)
    _, out = run("pdf", "extract_text.py", "hard.pdf", cwd=tmp_path)
    assert "## Page 1" in out


@needs_office
def test_markdown_becomes_a_designed_pdf(tmp_path):
    (tmp_path / "g.md").write_text("# Guide\n\nHello.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n")
    _, out = run("pdf", "make_pdf.py", "from-markdown", "g.md", "g.pdf", "--audience", "kids",
                 "--keep-docx", cwd=tmp_path)
    assert (tmp_path / "g.pdf").stat().st_size > 1000 and (tmp_path / "g.docx").is_file()
    _, out = run("pdf", "extract_text.py", "g.pdf", cwd=tmp_path)
    assert "Guide" in out and "Hello." in out


def test_make_pdf_refuses_what_it_cannot_print(tmp_path):
    (tmp_path / "g.md").write_text("x")
    run("pdf", "make_pdf.py", "from-office", "g.md", "g.pdf", cwd=tmp_path, ok=False)


# ---------- slides and documents as code ----------

def _kit_script(tmp_path, body, kit):
    folder = SKILLS / ("pptx" if kit == "slide_kit" else "docx") / "scripts"
    script = tmp_path / "make.py"
    script.write_text(f"import sys\nsys.path.insert(0, r'{folder}')\n" + body)
    r = subprocess.run([sys.executable, str(script)], cwd=tmp_path, capture_output=True, text=True, timeout=300)
    assert r.returncode == 0, r.stdout + r.stderr
    return r.stdout + r.stderr


def test_slide_kit_draws_free_slides_on_the_theme_beside_layout_slides(tmp_path):
    from PIL import Image

    Image.new("RGB", (300, 200), (40, 90, 160)).save(tmp_path / "p.png")
    out = _kit_script(tmp_path, '''
from slide_kit import Deck
with Deck("k.pptx", audience="hospital board", title="Q3") as deck:
    s = deck.slide("Visits grew", icon="trending-up")
    left, right = s.area.cols(7, 5)
    s.chart(left, "column", ["Q1", "Q2"], {"Visits": [1, 2]})
    for i, box in enumerate(right.rows(3)):
        s.card(box, accent=i)
        s.icon("check", box.inset(0.2).left(0.5).top(0.5), color=i, badge=True)
        s.text(box.inset(0.2).cut_left(0.7), "Point", size="h3", bold=True, color="ink")
    s = deck.slide("Numbers")
    for i, box in enumerate(s.area.top(2.5).split(3)):
        s.stat(box, "1.2M", "Users", delta="+12%")
    t, p = s.area.cut_top(2.8).cols(6, 6)
    s.table(t, ["A", "B"], [["x", 1]])
    s.image(p, "p.png", fit="cover")
    s = deck.slide()
    s.shape("rounded", s.area.inset(0.3))
    s.bullets(s.area.inset(0.9), ["One", "Two"], marker="check", color="white")
    s.notes("Say this.")
''', "slide_kit")
    assert "3 slide(s) drawn, 4 in the deck" in out
    _, out = run("pptx", "pptx_tool.py", "add", "k.pptx", "--json", '{"layout": "closing", "title": "Thanks"}', cwd=tmp_path)
    _, out = run("pptx", "validate.py", "k.pptx", cwd=tmp_path)
    assert "PASSED (0 warning(s))" in out
    from pptx import Presentation

    prs = Presentation(tmp_path / "k.pptx")
    assert len(prs.slides) == 5 and prs.core_properties.category == "kodact-theme:corporate"
    # White check marks on the coloured panel, not the panel's own colour (they vanished into it).
    marks = [sh for sh in prs.slides[3].shapes if sh.name == "Icon check"]
    assert len(marks) == 2 and all(str(sh.line.color.rgb) == "FFFFFF" for sh in marks)


def test_slide_kit_boxes_split_on_a_twelve_column_grid():
    sys.path.insert(0, str(SKILLS / "pptx" / "scripts"))
    from slide_kit import Box

    a, b = Box(0, 0, 12.25, 5).cols(7, 5)
    assert round(a.w + b.w + 0.25, 6) == 12.25 and round(b.x - a.x - a.w, 6) == 0.25
    rows = Box(0, 0, 4, 10).rows(1, 2, gap=1)
    assert round(rows[1].h, 6) == round(2 * rows[0].h, 6)
    assert Box(0, 0, 4, 4).inset(1) == Box(1, 1, 2, 2)


def test_doc_kit_writes_footnotes_sections_and_fitting_tables(tmp_path):
    import zipfile

    _kit_script(tmp_path, '''
from doc_kit import Doc
with Doc("r.docx", audience="hospital board", title="Q3") as d:
    d.heading("Results", 1)
    d.para("Visits grew ").add("75%", bold=True).add(".").footnote("Source: scheduling.")
    d.para("Waits fell.").footnote("Median wait.")
    d.table(["Site", "Visits"], [["A", 1], ["B", 2]], widths=[3, 1.5], total="Visits")
    d.section(orientation="landscape", columns=2, header="Appendix", footer="Confidential")
    d.para("See the ").link("dashboard", "https://example.org").add(".")
    d.table(["Region", "Q1", "Q2"], [["N", 1, 2], ["", "", ""]], merge=[(2, 0, 2, 2)], style="grid")
''', "doc_kit")
    run("docx", "validate.py", "r.docx", cwd=tmp_path)
    z = zipfile.ZipFile(tmp_path / "r.docx")
    notes = z.read("word/footnotes.xml").decode()
    assert "Source: scheduling." in notes and 'w:id="2"' in notes
    body = z.read("word/document.xml").decode()
    assert body.count("<w:footnoteReference") == 2 and 'w:orient="landscape"' in body and 'w:num="2"' in body
    assert "Total" in body and "https://example.org" in z.read("word/_rels/document.xml.rels").decode()
    # The two-column section's table fits one column (about 4.25in), not the page.
    last = body[body.rfind("<w:tbl>"):]
    widths = [int(w) for w in re.findall(r'<w:gridCol w:w="(\d+)"', last)]
    assert sum(widths) / 1440 < 4.6


def test_doc_kit_refuses_widths_wider_than_the_text_and_totals_of_text(tmp_path):
    folder = SKILLS / "docx" / "scripts"
    for body in ('d.table(["A", "B"], [["x", 1]], widths=[6, 6])', 'd.table(["A", "B"], [["x", 1]], total="A")'):
        script = tmp_path / "m.py"
        script.write_text(f"import sys\nsys.path.insert(0, r'{folder}')\nfrom doc_kit import Doc\n"
                          f"with Doc('x.docx') as d:\n    {body}\n")
        r = subprocess.run([sys.executable, str(script)], cwd=tmp_path, capture_output=True, text=True)
        assert r.returncode != 0 and ("wide" in r.stderr + r.stdout or "holds text" in r.stderr + r.stdout)


def test_contact_sheets_keep_each_pages_shape(tmp_path):
    sys.path.insert(0, str(PLUGIN / "scripts"))
    from PIL import Image
    from _kodact_office import contact_sheets

    Image.new("RGB", (850, 1100), (255, 255, 255)).save(tmp_path / "page-1.png")
    Image.new("RGB", (1100, 850), (255, 0, 0)).save(tmp_path / "page-2.png")
    sheet = Image.open(contact_sheets([tmp_path / "page-1.png", tmp_path / "page-2.png"], tmp_path,
                                      lambda i: f"Page {i + 1}", 2, 1, 400)[0]).convert("RGB")
    # The landscape page keeps its shape: red spans the tile's width but not its whole height.
    tile_top, tile_h = 16 + 28, int(400 * 1100 / 850)
    column = [sheet.getpixel((16 + 400 + 16 + 200, y)) for y in range(tile_top, tile_top + tile_h)]
    red = sum(1 for px in column if px[0] > 200 and px[1] < 60)
    expected = 400 * 850 / 1100  # the landscape page at the tile's width
    assert abs(red - expected) <= 3, (red, expected)


def test_who_reads_it_picks_the_theme_before_what_it_is_about():
    sys.path.insert(0, str(PLUGIN / "scripts"))
    import deck_design

    assert deck_design.for_audience("hospital board") == "corporate"
    assert deck_design.for_audience("kids aged 8") == "kids"
    assert deck_design.for_audience("patients and families") == "warm"
    assert deck_design.for_audience("health research paper") == "editorial"
