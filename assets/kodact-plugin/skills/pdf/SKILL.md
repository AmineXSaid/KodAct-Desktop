---
name: pdf
description: >-
  Comprehensive workflow for extracting text and tables from PDFs, creating
  publication-grade PDFs with ReportLab, manipulating pages (merge, split,
  rotate), routing and filling both interactive AcroForms and flat/scanned PDF
  forms with validated bounding boxes, and visually verifying output with Poppler.
---

# PDF Document Engineering Skill

Use this skill for all `.pdf` workflows: extracting structured text/tables, generating multi-page PDFs from scratch, merging/splitting/rotating pages, filling both interactive AcroForms and flat/scanned forms, and running visual QA.

---

## 1. Decision Tree

```text
What is the task?
├── Read / Extract / Inspect
│   ├── Selectable text & tables ──▶ scripts/extract_text.py (--tables, --pages)
│   ├── Check form type & fields ──▶ scripts/inspect_forms.py
│   └── Scanned / image-only PDF ──▶ scripts/render_preview.py ──▶ OCR (tesseract) or visual read
│
├── Create New PDF from Scratch
│   ├── Structured reports, invoices, tables ──▶ ReportLab Platypus (see references/pdf_patterns.md)
│   ├── HTML/CSS-first layouts               ──▶ WeasyPrint / headless browser print-to-PDF
│   └── From .docx / .pptx / .xlsx           ──▶ LibreOffice (--headless --convert-to pdf)
│       ──▶ scripts/render_preview.py --grid ──▶ inspect PNGs
│
└── Transform or Fill Existing PDF
    ├── Merge, split, reorder, rotate ──▶ scripts/merge_split.py
    └── Fill a PDF Form (ALWAYS run scripts/inspect_forms.py first!)
        ├── form_mode == "fillable_acroform"
        │   └── scripts/fill_form.py (validates options & syncs /AS checkbox states)
        │       ──▶ scripts/render_preview.py ──▶ inspect PNGs
        └── form_mode == "flat_non_fillable"
            └── scripts/render_preview.py (inspect blank form)
                ──▶ scripts/fill_flat_form.py --debug-out debug.pdf -o filled.pdf
                ──▶ scripts/render_preview.py debug.pdf (verify red/blue bounding boxes)
```

---

## 2. Workflows

### A. Extracting Text & Tables
- Run `python scripts/extract_text.py input.pdf --tables --pages 1-5 -o extracted.md` (or `--format json`).
- If output is empty, the PDF is scanned—render with `render_preview.py` and OCR or read visually.

### B. Creating a New PDF (`reportlab.platypus`)
1. **Margins & Escape**: Use `>= 0.75 * inch` margins and always wrap dynamic strings in `xml.sax.saxutils.escape(text)` before passing to `Paragraph(...)`.
2. **Orphan Prevention**: Set `keepWithNext=True` on all heading `ParagraphStyle`s.
3. **Tables**: Explicit `colWidths=[...]`, wrap cell strings in `Paragraph(...)` so long text wraps instead of overflowing, and set `repeatRows=1`.

### C. Filling PDF Forms (Interactive vs. Flat)
1. **Route first**:
   ```bash
   python scripts/inspect_forms.py form.pdf
   ```
2. **Path 1 — Interactive AcroForm (`fillable_acroform`)**:
   - Inspect exact `name`, `type` (`/Tx`, `/Btn`, `/Ch`), `options`, and `states` (`/_States_`).
   - Run `python scripts/fill_form.py form.pdf values.json -o filled.pdf`. (`fill_form.py` syncs both `/V` and widget `/AS` appearance states so headless Poppler renders checked checkboxes accurately.)
3. **Path 2 — Flat / Non-Fillable Form (`flat_non_fillable`)**:
   - Render blank pages with `render_preview.py`.
   - Create a JSON spec using DPI-independent `"unit": "normalized"` (`0.0..1.0` from `"top-left"`) — see [pdf_patterns.md](./references/pdf_patterns.md).
   - Run `fill_flat_form.py` with `--debug-out debug.pdf` (validates zero box overlaps, page bounds, and font height fit, and draws red entry boxes / blue label boxes for visual QA):
     ```bash
     python scripts/fill_flat_form.py form.pdf spec.json --debug-out debug.pdf -o filled.pdf
     python scripts/render_preview.py debug.pdf --out-dir ./debug_preview
     ```

---

## 3. Mandatory Verification & QA Loop

1. **Render Visual Preview**:
   ```bash
   python scripts/render_preview.py output.pdf --out-dir ./preview --dpi 150 --grid
   ```
2. **Visual Inspection Checklist**:
   - [ ] Scan `grid-*.png` contact sheets first, then inspect high-density `page-*.png` pages.
   - [ ] No text clipping, overlapping elements, or overflowing table cells.
   - [ ] On filled forms: every text value sits cleanly inside its line/box and checkboxes display the expected check state.

---

## 4. Helper Scripts Reference

| Script | Purpose | Usage |
| :--- | :--- | :--- |
| [extract_text.py](./scripts/extract_text.py) | Extract text & tables (Markdown or JSON) | `python scripts/extract_text.py in.pdf --tables -o out.md` |
| [inspect_forms.py](./scripts/inspect_forms.py) | Route form mode & list AcroForm fields/rects | `python scripts/inspect_forms.py form.pdf` |
| [fill_form.py](./scripts/fill_form.py) | Fill AcroForms with `/AS` checkbox state sync | `python scripts/fill_form.py form.pdf data.json -o out.pdf` |
| [fill_flat_form.py](./scripts/fill_flat_form.py) | Validate boxes & stamp flat/scanned PDF forms | `python scripts/fill_flat_form.py form.pdf spec.json --debug-out dbg.pdf -o out.pdf` |
| [merge_split.py](./scripts/merge_split.py) | Merge, split/extract, or rotate PDF pages | `python scripts/merge_split.py merge a.pdf b.pdf -o out.pdf` |
| [render_preview.py](./scripts/render_preview.py) | Render PDF pages & `--grid` contact sheets | `python scripts/render_preview.py out.pdf --grid` |
