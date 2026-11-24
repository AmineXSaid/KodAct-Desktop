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

**Start with the file they asked for.** Your first command makes the real
file in the type requested, before any plan, outline or helper script:
`pptx_tool.py init deck.pptx`, `docx_tool.py init report.docx` or
`xlsx_tool.py init budget.xlsx`, each with `--audience "<who it is for>"`
and `--title`. Then `add` to it a few slides or blocks at a time, so the
person watches it grow. Never begin by writing a `.md` outline or a build
script the person did not ask for.

**Design for who it is for.** `--audience` picks the theme from the
person's words, and every element follows it: colours, fonts, shapes,
charts, cards, faces. kids (warm, bright, rounded, drawn faces and stars),
warm (patients, parents, a community: calm and soft), corporate (boards,
finance: navy and teal, square), editorial (reports, research: serif),
bold (talks, launches: dark stage), clean (everything else). Pass
`--theme` only when the person named a look. Then choose components for
the content: numbers become KPI tiles or a chart, a process becomes steps,
a decision becomes a comparison, dates become a timeline, a warning a
callout. Plain bullets are the last resort, not the default.

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
| **Start a new document (always first)** | `init report.docx --audience "the board" --title "Q3 Review" --subtitle "…" --tag "Confidential"` |
| **Add designed blocks** | `add report.docx --json '[{"block": "heading", "text": "Summary"}, {"block": "kpis", "items": [...]}]'` |
| Read as Markdown | `read memo.docx` |
| Write from Markdown | `create memo.docx --markdown memo.md` (only to convert Markdown the person gave you) |
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

Word blocks: `heading`, `text` (with **bold**, `lead` for an intro),
`bullets`, `callout` (note, tip, info, warning, danger, success), `table`
(numbers right-aligned, a `highlight` row, a header that repeats on each
page), `kpis`, `cards` (each with an `icon`), `steps`, `timeline`, `compare` (with a `verdict`),
`quote`, `chart` (column, bar, line, area, pie, doughnut, in the theme's
colours), `image`, `toc`, `pagebreak`. `docx_tool.py --help` shows each
one's fields. They are real Word tables, styles and fields, so the person
can keep editing them in Word.

## Excel (.xlsx), `xlsx_tool.py`

| Task | Command |
|:--|:--|
| **Start a new workbook (always first)** | `init budget.xlsx --audience "finance team" --title "Q3 Budget" --subtitle "…"` |
| **Add designed blocks** | `add budget.xlsx --json '[{"block": "table", …}, {"block": "chart", …}]'` |
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

Excel blocks: `table` (a real Excel Table with filters and banding;
`formulas` per column such as `"=[@Q3]-[@Q2]"`, a `totals` row of live
formulas, `formats`, and conditional formatting: `bars`, `scale`, `icons`),
`kpis` (tiles whose `formula` keeps them live), `chart` (native: column,
bar, line, area, pie, doughnut, over `data` and `categories` ranges), `note`,
and `sheet` (a new sheet; any block takes `"sheet"`). `add` prints each
table's range; refer to it in charts and KPI formulas. Keep numbers as
numbers and totals as formulas, then `recalc`.

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

**Designing a deck.** The theme comes from `--audience` (above). Choose
each slide's layout from what it says, and vary them:

| Layout | For | Fields |
|:--|:--|:--|
| `title` | the cover (made by `init`; `--style dark` for a deep cover with big friendly faces) | `title`, `subtitle`, `tag` |
| `feature` | one big number and what it means, with icon cards | `stat`, `stat_text`, `note`, `items` [{"icon", "head", "text"}] |
| `cards` | 2-6 points, numbered or marked | `items`, `marks`: number, heart, star, sun |
| `steps` | a process or a cause and effect (2-4) | `items` as "Head: body", `footer` |
| `statement` | the one sentence to remember | `statement`, `items`, `art` |
| `faces` | feelings, reactions (up to 4) | `items` [{"label", "mood"}], `lines` |
| `chart` | numbers over categories or time (native, editable) | `chart`: {`type` column bar line pie doughnut area stacked, `categories`, `series`}, `items` beside it |
| `table` | exact figures | `columns`, `rows`, `highlight` |
| `kpis` | 2-4 headline numbers | `items` [{"value", "label", "delta", "better": "up"/"down"}] |
| `timeline` | dates and milestones | `items` as "When: what" |
| `compare` | two options | `left`/`right` {"title", "items"}, `verdict` |
| `grid` | 3-6 features or ideas | `items` [{"icon", "head", "text"}] |
| `cycle` | a repeating process | `items`, `center` |
| `pyramid` | levels, top first | `items` as "Head: line" |
| `quote` | a voice | `quote`, `by` |
| `image` | a picture | `path`, `caption`, `items` |
| `bullets` | a short plain list (last resort) | `items` |
| `closing` | the last slide | `title`, `items` |

Every slide takes `title`, `icon`, `section` (starts a section of the deck,
as PowerPoint groups slides) and `notes` (speaker notes: write them for the
person presenting). Icons are Lucide's 1,500+ (`python <skill-dir>/scripts/icons.py
search WORD` finds names: hand, droplet, bone, brain, stethoscope, pill,
shield-check, trending-up…), drawn as native PowerPoint shapes in a coloured
circle; give every card or tile an icon that says what it is. The deck's
theme (Design tab) carries the colours and fonts, so the person can restyle it
in one place. To review, run `check_tool.py sheet deck.pptx sheet.png` and
look at every slide at once; fix what you see, then look again. Moods: happy,
sad, worried, angry, confused, surprised. Text is sized to fit its box, so
keep each item to one short sentence. Render once at the end, look at the
slides, and fix with `set-text`, or `slides` to drop and re-`add`.

In an outline, each `# Title` starts a slide; the first is the title slide
and the line under it its subtitle; `- ` lines are bullets, two spaces per
level. Keep slides to one idea and at most five bullets.

## Markdown (.md)

A Markdown file shows in KodAct's panel as a designed page, redrawn as you
write it. Use what it draws: headings, tables, task lists (`- [ ]`),
callouts (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`,
`> [!CAUTION]`), and diagrams in ```` ```mermaid ```` blocks (flowcharts,
sequences, timelines, gantt charts, mind maps, pie charts), which are drawn,
not shown as code.

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
