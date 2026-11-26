---
name: docx
description: >-
  Make, edit, review and check Word documents (.docx): designed reports, memos, letters and
  guides in a theme picked for the reader (cover, KPIs, callouts, styled tables, native charts,
  cards, steps, timelines, comparisons, quotes, images, a table of contents), built block by block
  so the person watches the file grow; Markdown in, Word out; real comments and tracked changes
  that keep the formatting around them, with a check that rejecting the changes gives back the
  original; and a rendered preview to look at before replying. Use for any .docx or Word task.
---

# Word documents

The scripts are in `${KODACT_SKILL_DIR}/scripts`. Call them with `python` and that full path.
`T="${KODACT_SKILL_DIR}/scripts"` below stands for that folder.

## Make a new document: plan, build, check, look

1. **Plan before any file.** Who reads it, and what must they know or decide by the end? Put
   the answer first (a summary, the KPIs, the recommendation), the evidence after, the detail in
   an appendix. Headings say what the section shows ("Costs rose 12%", not "Costs").
2. **Start the real file first**, so the person sees it at once:
   `python "$T/docx_tool.py" init report.docx --audience "WHO READS IT" --title "…" --subtitle "…"`
   (`--theme` clean, kids, corporate, bold, editorial or warm; `--page a4` outside the US). It
   writes the cover, the theme on Word's own styles, a header and page numbers.
3. **Add blocks**, a few per call, as JSON (a list, or `@blocks.json`):
   `python "$T/docx_tool.py" add report.docx --json @blocks.json`.
   Every block and its keys: `python "$T/docx_tool.py" --help` (heading, text with `**bold**`,
   bullets, callout, table, kpis, cards with icons, steps, timeline, compare, quote, chart,
   image, toc, pagebreak). Prefer the block that fits the content: a number → `kpis`; a risk →
   `callout` kind warning; a choice → `compare`; a process → `steps`; data → `table` or `chart`.
   Long prose can come as Markdown instead:
   `python "$T/docx_tool.py" append report.docx report.docx --markdown body.md`; in a document
   made by `init`, its tables take the theme too.
4. **Check it**: `python "$T/validate.py" report.docx`. Fix every error and run it again.
5. **Look at it once**: `python "$T/render_preview.py" report.docx --out-dir preview --grid`, then
   open `preview/grid-1.png`: page breaks in odd places, a heading alone at a page's foot, a table
   split badly, an empty page. Open a single `page-N.png` only for a detail; don't read every page.
   (A table of contents shows as a note in the preview: Word fills it when the file is opened.)
6. **Reply** with the file's name and one line on what it says. Don't paste its text back.

**What the blocks don't do** (footnotes, a landscape appendix, two-column pages, headers and
footers per section, tables with set widths, merged cells or a totals row, links, mixed
formatting in one sentence): write it as code with `doc_kit`, on the same theme, alongside the
blocks:
```python
import sys; sys.path.insert(0, r"${KODACT_SKILL_DIR}/scripts")
from doc_kit import Doc
with Doc("report.docx") as d:
    d.heading("Results", 1)
    d.para("Visits grew ").add("75%", bold=True).add(" after the east clinic opened.") \
        .footnote("Booked visits, July to September; source: scheduling system.")
    d.block({"block": "kpis", "items": [{"value": "210k", "label": "Visits", "delta": "+40%"}]})
    d.table(["Site", "Visits", "Wait"], rows, widths=[3, 1.5, 1.5], total="Visits")
    d.section(orientation="landscape", columns=2, header="Appendix", footer="Confidential")
```
`help(doc_kit)` lists all of it. Tables fit the column they are in.

A plain document with no design (a quick conversion): `python "$T/docx_tool.py" create out.docx
--markdown notes.md`.

## Change, review or redline a document

Always write a new file (OUT) and leave the person's original as it was, unless they asked
otherwise.

- Read it: `python "$T/docx_tool.py" read in.docx` (headings, lists, paragraphs and tables as
  Markdown, in order).
- Replace text, keeping each run's formatting, even across Word's run boundaries:
  `python "$T/docx_tool.py" replace in.docx out.docx --find "old" --replace "new"` (repeat pairs).
- Comment on a passage: `python "$T/docx_tool.py" comment in.docx out.docx --quote "exact words"
  --text "Your note" --author "Name"`.
- Tracked changes the person can accept or reject in Word:
  `python "$T/docx_tool.py" track in.docx out.docx --find "30 days" --replace "45 days"`, then
  `python "$T/validate.py" out.docx --baseline in.docx`, which proves that rejecting every change
  gives the original text back. `accept` accepts them all.
- Many redlines and comments at once, from one JSON
  (`{"replacements": [{"find", "replace", "occurrence"}], "comments": [{"anchor", "comment"}]}`):
  `python "$T/redline_tools.py" in.docx edits.json -o out.docx`, then the same `--baseline` check.
- Formatting the blocks don't cover (a custom table, a field): python-docx with the helpers in
  `references/ooxml_patterns.md`.
- Anything else (footnotes, a header's XML, numbering): `python "$T/unpack.py" in.docx work/`,
  edit the XML, `python "$T/pack.py" work/ out.docx` (it validates as it packs).

## Rules that hold for every document

- One theme; body 10.5-11pt, headings in Word's Heading styles (they build the table of contents
  and the navigation pane). Don't set fonts by hand on top of the theme.
- Tables: a header row, numbers right-aligned, at most about 7 columns; wider data belongs in a
  spreadsheet, linked or attached.
- A paragraph of more than about 6 lines is two paragraphs, or a list.
- Make it a PDF to send: `python "${KODACT_SKILL_DIR}/../pdf/scripts/make_pdf.py" from-office report.docx report.pdf`.
- Never claim the document was checked or looked at when a step failed: say which.
