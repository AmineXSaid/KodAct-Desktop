# PPTX Engineering Reference Patterns

Declarative pure-Python grid layout engine for greenfield 16:9 decks plus JSON template replacement specs.

## 1. Declarative `SlideGrid` Helper (Zero Manual Coordinate Math)

Use `SlideGrid` in greenfield `python-pptx` scripts so cards and columns align with exact margins and gutters automatically (no overlapping coordinates):

```python
from pptx import Presentation
from pptx.util import Inches, Pt
from pptx.dml.color import RGBColor
from pptx.enum.shapes import MSO_SHAPE


class SlideGrid:
    """Computes collision-free (left, top, width, height) cells on a 13.333x7.5 canvas."""

    def __init__(self, left=0.65, top=1.45, right=12.68, bottom=6.95, gutter=0.25):
        self.left = left
        self.top = top
        self.width = right - left
        self.height = bottom - top
        self.gutter = gutter

    def cell(self, col: int, row: int, cols: int = 12, rows: int = 1,
             col_span: int = 1, row_span: int = 1):
        """Return (Inches(l), Inches(t), Inches(w), Inches(h)) for 0-based grid span."""
        unit_w = (self.width - self.gutter * (cols - 1)) / cols
        unit_h = (self.height - self.gutter * (rows - 1)) / rows
        l = self.left + col * (unit_w + self.gutter)
        t = self.top + row * (unit_h + self.gutter)
        w = unit_w * col_span + self.gutter * (col_span - 1)
        h = unit_h * row_span + self.gutter * (row_span - 1)
        return Inches(l), Inches(t), Inches(w), Inches(h)


prs = Presentation()
prs.slide_width, prs.slide_height = Inches(13.333), Inches(7.5)
slide = prs.slides.add_slide(prs.slide_layouts[6])
grid = SlideGrid()

# Example: 3 equal KPI cards across a 12-column, 2-row grid (top row)
for i in range(3):
    l, t, w, h = grid.cell(col=i * 4, row=0, cols=12, rows=2, col_span=4, row_span=1)
    card = slide.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, l, t, w, h)
    card.fill.solid()
    card.fill.fore_color.rgb = RGBColor.from_string("F8FAFC")
```

## 2. Template Replacement JSON Spec (`scripts/template_ops.py replace`)

After running `template_ops.py rearrange` and `template_ops.py inventory`:

```json
{
  "slides": [
    {
      "slide_number": 1,
      "remove_shapes": ["Subtitle 3"],
      "updates": [
        {
          "shape_id": 2,
          "paragraphs": ["2026 Product Strategy"]
        }
      ],
      "notes": "Open with the 2026 strategic thesis."
    },
    {
      "slide_number": 2,
      "updates": [
        {
          "placeholder_idx": 0,
          "paragraphs": ["Q4 Growth Drivers"]
        },
        {
          "name": "Content Placeholder 2",
          "paragraphs": [
            {"text": "Enterprise ARR grew 38% YoY", "level": 0, "bold": true},
            {"text": "Net dollar retention reached 118%", "level": 1}
          ]
        }
      ]
    }
  ]
}
```

## 3. Native Chart Snippet

```python
from pptx.chart.data import CategoryChartData
from pptx.enum.chart import XL_CHART_TYPE

data = CategoryChartData()
data.categories = ["Q1", "Q2", "Q3", "Q4"]
data.add_series("Revenue ($M)", (2.1, 2.6, 3.0, 3.8))
l, t, w, h = grid.cell(col=0, row=0, cols=12, rows=1, col_span=7)
slide.shapes.add_chart(XL_CHART_TYPE.COLUMN_CLUSTERED, l, t, w, h, data)
```
