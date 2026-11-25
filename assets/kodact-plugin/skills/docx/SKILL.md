---
name: docx
description: >-
  Comprehensive workflow for reading, creating, redlining, editing, and visually
  verifying Word (.docx) documents using python-docx, programmatic OOXML redline
  tools, LibreOffice, and Poppler. Use whenever working with .docx files,
  contracts, tracked changes, comments, or formatted reports.
---

# DOCX Document Engineering Skill

Use this skill for all `.docx` workflows: reading/extracting content, building publication-grade documents from scratch, applying surgical tracked changes (`w:ins` / `w:del`) and comments with baseline integrity verification, and running visual QA.

---

## 1. Decision Tree

```text
What is the task?
├── Read / Extract / Inspect
│   ├── Plain text, headings, tables only ──▶ python-docx (or pandoc)
│   └── Comments, footnotes, tracked changes, or exact XML ──▶ scripts/unpack.py + inspect XML
│
├── Create New Document from Scratch
│   └── python-docx + OxmlElement helpers (see references/ooxml_patterns.md)
│       ──▶ scripts/validate.py ──▶ scripts/render_preview.py --grid ──▶ inspect PNGs
│
└── Edit / Redline Existing Document
    ├── Contract redlines (tracked changes) & review comments
    │   └── scripts/redline_tools.py input.docx edits.json -o redlined.docx
    │       ──▶ scripts/validate.py redlined.docx --baseline input.docx
    │       ──▶ scripts/render_preview.py redlined.docx --grid
    └── Custom structural OOXML edits (headers, custom XML parts, styles)
        └── scripts/unpack.py ──▶ edit XML ──▶ scripts/validate.py ──▶ scripts/pack.py
            ──▶ scripts/render_preview.py --grid
```

---

## 2. Workflows

### A. Reading & Extracting Content
- **Quick structured read (`python-docx`)**: Iterate `doc.paragraphs` and `doc.tables`. Remember `doc.paragraphs` omits tables, headers/footers, footnotes, and comments.
- **Full-fidelity inspection (`unpack.py`)**: Run `python scripts/unpack.py input.docx ./unpacked` and inspect `word/document.xml`, `word/comments.xml`, and `word/header*.xml`.

### B. Creating a New Document (`python-docx` + `OxmlElement` Helpers)
1. **Page Setup & Margins**: Explicit `Inches(8.5)` x `Inches(11)` (Letter) or `Mm(210)` x `Mm(297)` (A4) with `0.75"–1.0"` margins.
2. **Typography Hierarchy**: Single clean font family (`Calibri`, `Georgia`, `Liberation Sans`), `line_spacing = 1.15`, and `keep_with_next = True` on **every heading** to prevent orphan headings.
3. **Tables (Critical)**:
   - Set `table.autofit = False` and set explicit widths on **both** `table.columns[i].width` and every `cell.width`.
   - Use the ready-made `OxmlElement` helpers in [ooxml_patterns.md](./references/ooxml_patterns.md) (`set_cell_shading`, `set_cell_margins`, `set_repeat_table_header`, `set_row_cant_split`, `add_field_run`).

### C. Redlining Contracts & Adding Comments (`redline_tools.py`)
Prefer deterministic batch redlining over manual XML string editing:
1. Author a JSON spec (`replacements` and `comments` — see [ooxml_patterns.md](./references/ooxml_patterns.md)). `redline_tools.py` automatically splices across multi-run boundaries while preserving `<w:rPr>` formatting and wiring `word/comments.xml`, `.rels`, and `[Content_Types].xml`.
2. Run:
   ```bash
   python scripts/redline_tools.py original.docx edits.json -o redlined.docx
   python scripts/validate.py redlined.docx --baseline original.docx
   ```
   `--baseline original.docx` mathematically verifies that rejecting all tracked changes reproduces the exact original text with **zero unintended mutations**.

---

## 3. Mandatory Verification & QA Loop

1. **Structural & Redline Validation**:
   ```bash
   python scripts/validate.py output.docx [--baseline original.docx]
   ```
   Checks required parts, XML well-formedness, `[Content_Types].xml`, `.rels` targets, `<w:t xml:space="preserve">`, unique `w:id`s, matched `commentRangeStart`/`End` pairs, and optional baseline preservation.
2. **Render Visual Preview (with Contact-Sheet Grid)**:
   ```bash
   python scripts/render_preview.py output.docx --out-dir ./preview --dpi 150 --grid
   ```
   Uses an isolated LibreOffice profile (`-env:UserInstallation`) so open GUI sessions never block conversion, then generates per-page PNGs plus 3×3 `grid-*.png` contact sheets for fast multi-page inspection.
3. **Visual Inspection Checklist**:
   - [ ] Inspect `grid-*.png` first to scan overall pagination, then inspect individual `page-*.png` files where detail is dense.
   - [ ] No clipped text, orphan headings, or awkward table splits.
   - [ ] Repeating table headers, clean cell padding, and accurate page numbers/redlines.

---

## 4. Helper Scripts Reference

| Script | Purpose | Usage |
| :--- | :--- | :--- |
| [redline_tools.py](./scripts/redline_tools.py) | Batch-apply tracked changes (`w:ins`/`w:del`) & comments | `python scripts/redline_tools.py in.docx edits.json -o out.docx` |
| [validate.py](./scripts/validate.py) | Validate OOXML, comment ranges, IDs, and `--baseline` diff | `python scripts/validate.py out.docx --baseline in.docx` |
| [unpack.py](./scripts/unpack.py) | Safe ZIP unpack + namespace-safe XML pretty-printer | `python scripts/unpack.py in.docx ./work_dir` |
| [pack.py](./scripts/pack.py) | Validate and repack directory into `.docx` | `python scripts/pack.py ./work_dir out.docx` |
| [render_preview.py](./scripts/render_preview.py) | Isolated LibreOffice + Poppler PNG & `--grid` renderer | `python scripts/render_preview.py out.docx --grid` |
