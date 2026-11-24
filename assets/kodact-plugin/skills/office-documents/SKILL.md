---
name: office-documents
description: Read, create and edit Word (.docx), Excel (.xlsx), PowerPoint (.pptx) and PDF files, keeping the formatting of what you don't change, and checking every edit by reading the file back. Use whenever a task involves one of these files: summarising, updating figures, filling a form, writing a memo or deck, merging or splitting PDFs, or exporting to PDF.
license: Proprietary (KodAct)
---

# Office documents

KodAct shows any .docx, .xlsx, .pptx or .pdf you write as a card in the
chat **from the first command that names it**, opens it beside the chat, and
redraws it each time the file changes. So the person watches the file being
made: create the requested file first, then build it up in place.

**Start with the file they asked for.** For a new deck, your first command is
`pptx_tool.py init deck.pptx --theme … --title …`: the real .pptx, with its
title slide, before any plan, outline or helper script. Then `add` slides
one or a few at a time. Never begin by writing a `.md` outline or a
build script the person did not ask for.

**Don't look for tools.** KodAct has already found them: `KODACT_PYTHON` is
the Python with the libraries, and `check_tool.py render` draws pages with
Microsoft Office (on Windows) or LibreOffice, whichever KodAct found
(`KODACT_OFFICE_RENDER`). Don't run `which`, `where`, `Get-Command` or pip,
and don't write your own renderer. If `render` says nothing can draw the
file, say you couldn't check the layout and go on.

The tools are in `scripts/` (Python 3 with python-docx, openpyxl,
python-pptx, pypdf and reportlab; LibreOffice for `recalc`, `convert` and
`render` (`KODACT_SOFFICE` names the one KodAct found, its own copy included);
for `render`, poppler's pdftoppm or else pypdfium2, which KodAct's Python has).
Run them as `python <skill-dir>/scripts/<tool>.py …`. Each has `--help`.
KodAct puts a Python that has these libraries first on PATH, and names it
in `KODACT_PYTHON`: if `python` is not found or lacks a library, run the
tools with `"$KODACT_PYTHON"` (`%KODACT_PYTHON%` in cmd,
`$env:KODACT_PYTHON` in PowerShell). If that is unset too, tell the person
to choose Settings → General → Documents → Set up, and stop.

## The rules

Every edit goes: **read → change only what was asked → validate → render
and look → report.**

1. **Read before you write.** Run the tool's `read` first and work from what
   is really in the file: sheet names, headings, shape names
   (`pptx_tool.py read --shapes`), existing styles.
2. **Where to write.** For a new request, write a new file
   (`budget-edited.xlsx`) and keep the input intact, unless the person asked
   for the original to change. When the person's message comes from comments
   on a file in KodAct's panel ("Change `x.docx` as these comments ask"),
   write back to **the same path**: KodAct keeps every version and shows the
   person what changed, so a new name would break that.
3. **Change only what was asked.** Edit the quoted words, the named cells,
   the one shape; never rebuild a file to change one value. The tools keep
   styles, number formats, widths, other sheets and untouched formulas.
4. **Validate.** Run `check_tool.py validate OUT` on every file you write;
   fix anything it reports before going on.
5. **Formulas need a recalc.** openpyxl saves formulas without computing
   them. After changing a workbook with formulas, run
   `xlsx_tool.py recalc OUT OUT`. It fails, naming the cells, when a formula
   computes to an error (#DIV/0!, #REF!, #NAME?…): fix those, don't hand
   them over.
6. **Look at it.** Run `check_tool.py render OUT pages/` and look at the
   images of the pages or slides you changed: text running off a slide, a
   table split across pages, an empty page, a chart with no data. Fix what
   you see. If `render` says nothing here can draw the file, say you could
   not check the layout; don't hunt for other tools. Delete `pages/` when
   you are done: the person sees the file itself beside the chat.
7. **Check by reading back, and say what changed.** `read` the output and
   end with the file name and a short list of the edits, in your own words,
   with the numbers: which cells, which paragraphs, which slides. For
   comments, answer them one by one.

## Word (.docx), `docx_tool.py`

| Task | Command |
|:--|:--|
| Read as Markdown | `read memo.docx` |
| Write from Markdown | `create memo.docx --markdown memo.md` |
| Replace text, keeping formatting | `replace memo.docx memo-2.docx --find "Q2" --replace "Q3"` (repeat pairs) |
| Add sections at the end | `append memo.docx memo-2.docx --markdown more.md` |
| Add a Word comment on a quote | `comment memo.docx memo-2.docx --quote "fall to 700" --text "Why?"` (repeat pairs) |
| Edit as tracked changes | `track memo.docx memo-2.docx --find "700" --replace "690"` (repeat pairs) |
| Accept all tracked changes | `accept memo.docx memo-2.docx` |

Markdown supports `#`–`###` headings, paragraphs, `-` and `1.` lists,
tables, fenced code, and **bold**, *italic* and `code`. Replace reaches body
text, tables, headers and footers, and works when the text spans runs.

**Comments and tracked changes.** When the person asks for comments "in the
file", or for changes they can review in Word, use `comment` and `track`:
they write real Word comments anchored to the quoted words, and real
insertions and deletions (author KodAct, dated) that Word lets them accept or
reject one by one. `read` lists a file's comments and tracked changes at the
end. Quote the exact words as they are in the file; a quote that is not
found is an error, not a silent skip.

## Excel (.xlsx), `xlsx_tool.py`

| Task | Command |
|:--|:--|
| Read every sheet (values) | `read budget.xlsx` |
| Read the formulas | `read budget.xlsx --sheet Budget --formulas` |
| A range's values and formulas | `read budget.xlsx --range "Budget!B2:D9"` |
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
| Name each slide's shapes | `read deck.pptx --shapes` |
| **Start a new deck (always first)** | `init deck.pptx --theme kids --title "What Is Cancer?" --subtitle "…" --tag "Ages 6-10"` |
| **Add designed slides** | `add deck.pptx --json '{"layout": "cards", "title": "…", "items": ["…", "…"], "notes": "…"}'` (a JSON list adds several; `@slides.json` reads a file) |
| Build from an existing outline | `create deck.pptx --outline outline.md` (only when the person gave you one) |
| Replace text | `replace deck.pptx deck-2.pptx --find "2025" --replace "2026"` |
| Set one shape's text | `set-text deck.pptx deck-2.pptx --slide 2 --shape "#1" --text "Line one\nLine two"` |
| Speaker notes | `notes deck.pptx deck-2.pptx --slide 2 --text "Lead with the total."` |
| Reorder or delete slides | `slides deck.pptx deck-2.pptx --order 1,3,2` (slides left out are deleted) |
| Duplicate a slide | `duplicate deck.pptx deck-2.pptx --slide 2` (the copy follows it) |

**Designing a deck.** Pick the theme from the audience: `kids` (warm cream,
bright colours, rounded cards, drawn faces and stars), `clean` (white, one
blue, for work), `bold` (dark stage, strong colour, for talks). Then choose
each slide's layout from what it says, and vary them:

| Layout | For | Fields |
|:--|:--|:--|
| `cards` | 2-6 points, numbered or marked | `items`, `marks`: number, heart, star, sun |
| `steps` | a process or a cause and effect (2-4) | `items` as "Head: body", `footer` |
| `statement` | the one sentence to remember | `statement`, `items`, `art` |
| `faces` | feelings, reactions (up to 4) | `items` [{"label", "mood"}], `lines` |
| `bullets` | a short plain list | `items` |
| `closing` | the last slide | `title`, `items` |

Every slide takes `title`, `icon` (sun, star, heart, cloud, cell, none) and
`notes` (speaker notes: write them for the person presenting). Moods: happy,
sad, worried, angry, confused, surprised. Text is sized to fit its box, so
keep each item to one short sentence. Render once at the end, look at the
slides, and fix with `set-text`, or `slides` to drop and re-`add`.

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

## Checking, `check_tool.py`

| Task | Command |
|:--|:--|
| Is it sound? | `validate out.docx [more files]` (zip, XML, required parts, relationships, opens) |
| What does it look like? | `render out.pptx pages/ [--pages 2-3] [--dpi 80]` (PNG per page) |

## Checked

`scripts/test_tools.py` edits real files with every tool and reads them back:
a styled budget workbook (edits kept, formats, widths, formulas and other
sheets untouched, LibreOffice recalculation, formula errors caught, ranges),
a memo (bold kept through a replace, tables, text split across runs, Word
comments, tracked changes and accepting them), a deck (one shape's text,
notes, reorder, delete, duplicate), PDFs (merge, pages, a filled form, a
Word file exported to PDF), and the checks (a broken file fails `validate`,
`render` draws every page). Run it with
`python -m unittest test_tools` from `scripts/`.
