---
name: office-documents
description: Read, create and edit Word (.docx), Excel (.xlsx), PowerPoint (.pptx) and PDF files, keeping the formatting of what you don't change, and checking every edit by reading the file back. Use whenever a task involves one of these files: summarising, updating figures, filling a form, writing a memo or deck, merging or splitting PDFs, or exporting to PDF.
license: Proprietary (Forge)
---

# Office documents

Forge shows any .docx, .xlsx, .pptx or .pdf you write as a card in your
reply, and opens it beside the chat for the person to read. Make those files
right the first time.

The tools are in `scripts/` (Python 3 with python-docx, openpyxl,
python-pptx, pypdf and reportlab; LibreOffice for `recalc` and `convert`).
Run them as `python <skill-dir>/scripts/<tool>.py …`. Each has `--help`.

## The rules

1. **Read before you write.** Run the tool's `read` first and work from what
   is really in the file: sheet names, headings, existing styles.
2. **Never edit in place.** Write a new file (`budget-edited.xlsx`), unless
   the person asked for the original to change. Keep the input intact.
3. **Change only what was asked.** The tools keep styles, number formats,
   widths, other sheets and untouched formulas; don't rebuild a file to
   change one value.
4. **Check by reading back.** After every edit, `read` the output and confirm
   the change is there, in your own words, with the numbers.
5. **Formulas need a recalc.** openpyxl saves formulas without computing
   them. After changing a workbook with formulas, run
   `xlsx_tool.py recalc OUT OUT` so the file carries the computed values, then
   `read` it (values, not formulas) to report them.
6. **Say what changed.** End with the file name and a short list of the
   edits: which cells, which paragraphs, which pages.

## Word (.docx), `docx_tool.py`

| Task | Command |
|:--|:--|
| Read as Markdown | `read memo.docx` |
| Write from Markdown | `create memo.docx --markdown memo.md` |
| Replace text, keeping formatting | `replace memo.docx memo-2.docx --find "Q2" --replace "Q3"` (repeat pairs) |
| Add sections at the end | `append memo.docx memo-2.docx --markdown more.md` |

Markdown supports `#`–`###` headings, paragraphs, `-` and `1.` lists,
tables, fenced code, and **bold**, *italic* and `code`. Replace reaches body
text, tables, headers and footers, and works when the text spans runs.

## Excel (.xlsx), `xlsx_tool.py`

| Task | Command |
|:--|:--|
| Read every sheet (values) | `read budget.xlsx` |
| Read the formulas | `read budget.xlsx --sheet Budget --formulas` |
| Set cells | `set budget.xlsx out.xlsx --cell "Budget!C3=700" --cell "D5==C5-B5"` |
| Append rows from CSV | `append-rows budget.xlsx out.xlsx --csv rows.csv --sheet Budget` |
| New workbook from CSV | `create out.xlsx --csv data.csv --sheet Data` |
| Compute formulas | `recalc out.xlsx out.xlsx` |

A value that starts with `=` is a formula (`D5==C5-B5` sets `=C5-B5`).
Numbers stay numbers. `read` shows each sheet as a table with column
letters and row numbers, so you can name cells exactly.

## PowerPoint (.pptx), `pptx_tool.py`

| Task | Command |
|:--|:--|
| Read slides and notes | `read deck.pptx` |
| Build from an outline | `create deck.pptx --outline outline.md` |
| Replace text | `replace deck.pptx deck-2.pptx --find "2025" --replace "2026"` |

In an outline, each `# Title` starts a slide; the first is the title slide
and the line under it its subtitle; `- ` lines are bullets, two spaces per
level. Keep slides to one idea and at most five bullets.

## PDF, `pdf_tool.py`

| Task | Command |
|:--|:--|
| Read the text, page by page | `read report.pdf` |
| Write from Markdown | `create report.pdf --markdown report.md --title "Report"` |
| Merge | `merge all.pdf a.pdf b.pdf` |
| Keep some pages | `pages report.pdf part.pdf --pages 1-3,5` |
| List form fields | `fields form.pdf` |
| Fill a form | `fill form.pdf filled.pdf --field "Name=Ada Lovelace"` |
| Export an Office file | `convert memo.docx memo.pdf` |

A scanned PDF has no text to read; say so rather than guessing its content.

## Checked

`scripts/test_tools.py` edits real files with every tool and reads them back:
a styled budget workbook (edits kept, formats, widths, formulas and other
sheets untouched, LibreOffice recalculation), a memo (bold kept through a
replace, tables, text split across runs), a deck, and PDFs (merge, pages, a
filled form, a Word file exported to PDF). Run it with
`python -m unittest test_tools` from `scripts/`.
