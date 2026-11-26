---
name: pdf
description: >-
  Make, read, fill and rearrange PDFs: a designed PDF from Markdown in one command (themed cover,
  styled tables, page numbers) or from any Word, PowerPoint or Excel file (printed by Office itself
  on Windows); text and tables out of a PDF; fillable forms filled with checked field names and
  options; flat or scanned forms stamped inside measured boxes with a debug overlay; merge, split
  and rotate; and rendered pages to look at before replying. Use for any .pdf task.
---

# PDFs

The scripts are in `${KODACT_SKILL_DIR}/scripts`. Call them with `python` and that full path.
`T="${KODACT_SKILL_DIR}/scripts"` below stands for that folder.

## Make a PDF

- **From content** (a report, a guide, a letter, notes): write it as Markdown (headings, lists,
  tables, `**bold**`), then
  `python "$T/make_pdf.py" from-markdown doc.md doc.pdf --audience "WHO READS IT" --subtitle "…"`.
  The first `# Heading` becomes the cover's title. `--keep-docx` also keeps the Word file it was
  printed from, to edit later.
- **Richer layouts** (KPIs, callouts, charts, cards, timelines): build a Word document with the
  docx skill's blocks (`python "${KODACT_SKILL_DIR}/../docx/scripts/docx_tool.py" --help`), or a
  deck with the pptx skill, then print it:
  `python "$T/make_pdf.py" from-office report.docx report.pdf` (.docx, .pptx, .xlsx). On Windows
  with Office installed, Word, PowerPoint or Excel print it themselves; else LibreOffice.
- Only when neither fits (an invoice laid out to the millimetre, a label sheet), write ReportLab
  code: the patterns are in `references/pdf_patterns.md`.

Then look at it once: `python "$T/render_preview.py" doc.pdf --out-dir preview --grid` and open
`preview/grid-1.png` (not every page image). Reply with the file's name and what it holds.

## Read a PDF

- `python "$T/extract_text.py" in.pdf [--pages 1-3,5] [--tables] [-o out.md]` (`--format json`
  for data). Empty text means a scanned PDF: render it and read the page images, or OCR them.
- Read only the pages the question needs; a long PDF's full text costs much context.

## Fill a form

1. **Find what kind it is**: `python "$T/inspect_forms.py" form.pdf`. It prints `fillable_acroform`
   with each field's exact name, type, options (choices) and states (checkboxes), or
   `flat_non_fillable`.
2. **Fillable**: write `{"exact field name": value}` (checkboxes `true`/`false`, choices one of the
   listed options) and run `python "$T/fill_form.py" form.pdf values.json -o filled.pdf`. It refuses
   a name or an option that isn't in the form, rather than guessing.
3. **Flat or scanned**: render the blank form
   (`python "$T/render_preview.py" form.pdf --out-dir blank`), measure each answer's box on the
   image as fractions of the page from its top-left corner, write the spec (format in
   `references/pdf_patterns.md`), then
   `python "$T/fill_flat_form.py" form.pdf spec.json --debug-out debug.pdf -o filled.pdf`. Render
   `debug.pdf`: red boxes must sit on the answer lines, apart from the blue labels. Adjust and
   repeat until they do.
4. Render `filled.pdf` and look before replying.

## Rearrange pages

`python "$T/merge_split.py" merge a.pdf b.pdf -o all.pdf`;
`split in.pdf --pages 2-4 -o part.pdf`; `rotate in.pdf --pages 1 --angle 90 -o out.pdf`.

## Rules

- Never change the person's PDF in place: write a new file.
- A filled form is checked by looking at it; a field that holds the value but shows nothing is
  not filled.
- Never claim a PDF was checked or looked at when rendering failed: say so.
