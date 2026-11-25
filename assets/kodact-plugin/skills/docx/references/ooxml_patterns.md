# OOXML & python-docx Engineering Reference Patterns

Reusable Python `OxmlElement` helpers for greenfield creation plus raw WordprocessingML patterns for surgical edits.

## 1. Ready-to-Use `python-docx` `OxmlElement` Helpers

Drop these helpers into any `python-docx` generation script to get cell shading, cell margins, repeating headers, non-splitting rows, and live `"Page X of Y"` fields without raw XML guesswork:

```python
from docx.oxml import OxmlElement
from docx.oxml.ns import qn


def set_cell_shading(cell, fill_hex: str) -> None:
    """Apply solid background hex color (e.g. '1E3A5F') to a table cell."""
    tc_pr = cell._tc.get_or_add_tcPr()
    shd = OxmlElement("w:shd")
    shd.set(qn("w:val"), "clear")
    shd.set(qn("w:color"), "auto")
    shd.set(qn("w:fill"), fill_hex.lstrip("#"))
    tc_pr.append(shd)


def set_cell_margins(cell, top=100, bottom=100, left=140, right=140) -> None:
    """Set cell padding in dxa (20 dxa = 1 pt, 1440 dxa = 1 inch)."""
    tc_pr = cell._tc.get_or_add_tcPr()
    tc_mar = OxmlElement("w:tcMar")
    for side, val in (("top", top), ("bottom", bottom), ("left", left), ("right", right)):
        node = OxmlElement(f"w:{side}")
        node.set(qn("w:w"), str(val))
        node.set(qn("w:type"), "dxa")
        tc_mar.append(node)
    tc_pr.append(tc_mar)


def set_repeat_table_header(row) -> None:
    """Repeat a table header row across page breaks."""
    tr_pr = row._tr.get_or_add_trPr()
    tr_pr.append(OxmlElement("w:tblHeader"))


def set_row_cant_split(row) -> None:
    """Prevent a table row from splitting mid-cell across page breaks."""
    tr_pr = row._tr.get_or_add_trPr()
    tr_pr.append(OxmlElement("w:cantSplit"))


def add_field_run(paragraph, field_instr: str) -> None:
    """Insert a dynamic Word field such as 'PAGE' or 'NUMPAGES'."""
    for fld_type, text in (("begin", None), (None, field_instr), ("separate", None), ("end", None)):
        run = paragraph.add_run()
        if fld_type:
            fld = OxmlElement("w:fldChar")
            fld.set(qn("w:fldCharType"), fld_type)
            run._r.append(fld)
        else:
            instr = OxmlElement("w:instrText")
            instr.set(qn("xml:space"), "preserve")
            instr.text = f" {text} "
            run._r.append(instr)
```

## 2. Batch Redline & Comment JSON Spec (`scripts/redline_tools.py`)

Use `redline_tools.py` instead of hand-editing XML for contract redlines and review comments:

```json
{
  "author": "Legal Counsel",
  "initials": "LC",
  "replacements": [
    {
      "find": "within thirty (30) days",
      "replace": "within forty-five (45) days",
      "occurrence": 1
    }
  ],
  "comments": [
    {
      "anchor": "forty-five (45) days",
      "comment": "Aligned payment net terms with standard enterprise MSA.",
      "occurrence": 1
    }
  ]
}
```

## 3. Raw WordprocessingML Snippets (`w:ins`, `w:del`, `w:comment`)

Always include `xml:space="preserve"` when `<w:t>` starts or ends with whitespace:

```xml
<w:p>
  <w:r><w:t xml:space="preserve">The agreement term is </w:t></w:r>
  <w:del w:id="101" w:author="Reviewer" w:date="2026-01-15T10:00:00Z">
    <w:r><w:delText>12 months</w:delText></w:r>
  </w:del>
  <w:ins w:id="102" w:author="Reviewer" w:date="2026-01-15T10:00:00Z">
    <w:r><w:t>24 months</w:t></w:r>
  </w:ins>
  <w:r><w:t>.</w:t></w:r>
</w:p>
```
