# PDF Engineering Reference Patterns

Patterns for ReportLab Platypus, AcroForms, and Flat (Non-Fillable) Form Coordinate Overlays.

## 1. Flat / Scanned PDF Form Spec (`scripts/fill_flat_form.py`)

Prefer `"unit": "normalized"` (`0.0` to `1.0` fraction of page width/height from the `"top-left"` corner) so coordinates measured on rendered PNGs work identically regardless of PNG DPI!

```json
{
  "coordinate_origin": "top-left",
  "unit": "normalized",
  "fields": [
    {
      "name": "applicant_name",
      "page": 1,
      "label_box": [0.08, 0.18, 0.22, 0.205],
      "entry_box": [0.23, 0.178, 0.62, 0.205],
      "kind": "text",
      "value": "Jane Doe",
      "font_size": 10
    },
    {
      "name": "agree_terms",
      "page": 1,
      "entry_box": [0.08, 0.72, 0.10, 0.74],
      "kind": "check",
      "value": "true"
    }
  ]
}
```

Workflow:
1. Render blank form: `python scripts/render_preview.py form.pdf --out-dir ./blank_pngs`
2. Estimate normalized boxes and generate debug overlay:
   ```bash
   python scripts/fill_flat_form.py form.pdf spec.json --debug-out debug.pdf -o filled.pdf
   python scripts/render_preview.py debug.pdf --out-dir ./debug_pngs
   ```
3. Visually verify red `entry_box` rectangles sit cleanly inside form lines/checkboxes without touching blue `label_box` rectangles; refine coordinates if needed, then deliver `filled.pdf`.

## 2. ReportLab Platypus Boilerplate (Wrapping Table & Header/Footer)

```python
from xml.sax.saxutils import escape
from reportlab.lib import colors
from reportlab.lib.pagesizes import LETTER
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.lib.units import inch
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle

def draw_chrome(canvas, doc):
    canvas.saveState()
    canvas.setFont("Helvetica", 8)
    canvas.setFillColor(colors.HexColor("#64748B"))
    canvas.drawString(doc.leftMargin, 0.45 * inch, "Confidential Report")
    canvas.drawRightString(LETTER[0] - doc.rightMargin, 0.45 * inch, f"Page {doc.page}")
    canvas.restoreState()

styles = getSampleStyleSheet()
h1 = ParagraphStyle("H1", parent=styles["Heading1"], keepWithNext=True, spaceAfter=6)
cell = ParagraphStyle("Cell", parent=styles["Normal"], fontSize=9, leading=12)

doc = SimpleDocTemplate(
    "output.pdf", pagesize=LETTER,
    leftMargin=0.75 * inch, rightMargin=0.75 * inch,
    topMargin=0.75 * inch, bottomMargin=0.75 * inch,
)
rows = [
    [Paragraph("<b>Metric</b>", cell), Paragraph("<b>Value</b>", cell)],
    [Paragraph(escape("Net Revenue"), cell), Paragraph(escape("$1.42M"), cell)],
]
table = Table(rows, colWidths=[doc.width * 0.65, doc.width * 0.35], repeatRows=1)
table.setStyle(TableStyle([
    ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#1E3A5F")),
    ("TEXTCOLOR", (0, 0), (-1, 0), colors.white),
    ("GRID", (0, 0), (-1, -1), 0.5, colors.HexColor("#CBD5E1")),
    ("VALIGN", (0, 0), (-1, -1), "TOP"),
    ("TOPPADDING", (0, 0), (-1, -1), 6),
    ("BOTTOMPADDING", (0, 0), (-1, -1), 6),
]))
doc.build([Paragraph("Summary", h1), Spacer(1, 8), table],
          onFirstPage=draw_chrome, onLaterPages=draw_chrome)
```

## 3. CLI Utilities (`qpdf`)
- Decrypt/inspect protected PDFs (with authorized password): `qpdf --password=SECRET --decrypt in.pdf out.pdf`
- Linearize / repair damaged xref tables: `qpdf --linearize in.pdf out.pdf`
