---
name: pptx
description: >-
  Make, edit and check PowerPoint decks (.pptx): designed slides in a theme picked for the
  audience (cards, steps, charts, tables, timelines, KPIs, comparisons, quotes, images, faces,
  Lucide icons), built one slide at a time so the person watches the file grow; edits that keep a
  template's formatting; a check that fails on cut-off, overflowing or overlapping text; and a
  rendered contact sheet to look at before replying. Use for any .pptx, slide deck or presentation.
---

# PowerPoint decks

The scripts are in `${KODACT_SKILL_DIR}/scripts`. Call them with `python` and that full path.
`T="${KODACT_SKILL_DIR}/scripts"` below stands for that folder.

## Make a new deck: plan, build, check, look

1. **Plan before any file.** Who is it for, and what should they think or do after the last
   slide? Write the story as 6-12 slide titles that read as sentences ("Cells are the body's
   building blocks", not "Cells"). One idea per slide. Pick a layout per slide from what the idea
   *is*: a number → `kpis` or `feature`; a sequence → `steps` or `timeline`; a trade-off →
   `compare`; parts of a whole → `grid` or `cards`; a trend → `chart`; data to look up → `table`;
   feelings (children, patients) → `faces`. Never two plain `bullets` slides in a row.
2. **Start the real file first**, so the person sees it at once:
   `python "$T/pptx_tool.py" init deck.pptx --audience "WHO IT IS FOR" --title "…" --subtitle "…"`.
   `--audience` picks the theme (children → kids, a board → corporate…); `--theme NAME` forces
   one of: clean, kids, corporate, bold, editorial, warm.
3. **Add the slides**, a few per call, as JSON (a list, or `@slides.json`):
   `python "$T/pptx_tool.py" add deck.pptx --json @slides.json`.
   Every layout and its keys: `python "$T/pptx_tool.py" --help`. A slide:
   `{"layout": "steps", "title": "How doctors help", "items": ["Find: tests show where", "Treat: medicine or surgery", "Check: visits after"], "notes": "What to say"}`.
   Text is sized to fit its box. `add` prints a `note:` when text is too long even at 12pt, or a
   layout leaves items out: shorten or split that slide, never ignore a note.
   Icons: any Lucide name (`python "${KODACT_SKILL_DIR}/../../scripts/icons.py" search heart`).
   **A slide no layout fits** (a chart beside three cards, a hero panel, a diagram of your own):
   draw it as code with `slide_kit`, in the same deck, on the same theme. Write a short script:
   ```python
   import sys; sys.path.insert(0, r"${KODACT_SKILL_DIR}/scripts")
   from slide_kit import Deck
   with Deck("deck.pptx") as deck:
       s = deck.slide("Visits grew 75% as the new clinic opened", icon="trending-up")
       chart_box, side = s.area.cols(7, 5)          # a 12-column grid under the title
       s.chart(chart_box, "column", ["Q1", "Q2", "Q3"], {"Visits (k)": [120, 150, 210]})
       for i, (box, (head, body)) in enumerate(zip(side.rows(3), points)):
           s.card(box, accent=i)
           s.icon(icons[i], box.inset(0.22).left(0.55).top(0.55), color=i, badge=True)
           s.text(box.inset(0.22).cut_left(0.75).top(0.45), head, size="h3", bold=True, color="ink")
           s.text(box.inset(0.22).cut_left(0.75).cut_top(0.45), body, size="small")
   ```
   Everything it needs is in `python -c "import sys; sys.path.insert(0, r'$T'); import slide_kit; help(slide_kit)"`:
   boxes that split on a grid (`cols`, `rows`, `inset`, `top`, `cut_left`…), and `text`
   (fitted), `bullets`, `card`, `icon`, `stat`, `chart`, `table`, `image`, `shape`, `line`,
   `notes`, with the theme's colours (`ink`, `soft`, `accent`, `accent0`-`accent5`) and sizes
   (`title`, `h1`-`h3`, `lead`, `body`, `small`, `caption`, `stat`). Use layouts for most slides
   and the kit where the content asks for its own shape; both pass through the same check.
4. **Check it**: `python "$T/validate.py" deck.pptx`. It fails on what looks broken: content cut
   off by the slide edge, text overflowing its box (measured in the slide's own fonts), words
   printed over words, and any XML PowerPoint would refuse or repair (checked against the Office
   Open XML schemas). Fix
   every error and run it again. Warnings (an empty placeholder, an off-centre title, tiny type,
   a wall of text) are worth fixing too.
5. **Look at it once**: `python "$T/render_preview.py" deck.pptx --out-dir preview --grid`, then
   open `preview/grid-1.png` (12 slides per image) and look for what a person would notice:
   uneven spacing, a crowded slide, low contrast, a slide that says nothing. Open a single
   `slide-N.png` only to inspect a detail. Don't read every slide image: the grid is enough and
   much cheaper.
6. **Reply** with the file's name, one line on its story, and what the check and the look
   changed. Don't paste the slide text back.

On Windows the preview is drawn by PowerPoint itself when it is installed (through
`scripts/office_to_pdf.ps1`), else by LibreOffice; images come from Poppler or pypdfium2. When
none can render, say so, rely on `validate.py`, and don't keep trying other renderers.

## Change an existing deck

- Read it: `python "$T/pptx_tool.py" read deck.pptx --shapes` (slides, shape names, text), or
  `python "$T/extract_text.py" deck.pptx` for text, tables and notes as Markdown.
- Small edits keep the formatting: `replace` (find/replace everywhere), `set-text` (one shape,
  by name or `#index` from `read --shapes`), `notes`, `slides --order 3,1,2` (reorder, and drop the
  slides left out), `duplicate --slide N`. They write OUT and leave the input as it was.
- Add designed slides to someone else's deck only when they asked for new slides; match their
  look otherwise.

## Build from a company template

1. `python "$T/render_preview.py" template.pptx --out-dir tpl --grid`: see every layout once.
2. `python "$T/template_ops.py" rearrange template.pptx work.pptx --slides 0,4,4,7` picks, repeats
   and orders template slides (0-based, as the grid labels them), keeping their media and charts.
3. `python "$T/template_ops.py" inventory work.pptx -o inv.json` lists every text shape by id.
4. Write the new text into a copy of that JSON and run
   `python "$T/template_ops.py" replace work.pptx edits.json -o final.pptx`: the template's fonts
   and colours stay.
5. Check and look, as for a new deck.

For raw XML (a theme colour, a master): `template_ops.py unpack` / `pack`, then `validate.py`.

A slide no layout fits (a diagram of their own, a dense technical figure): draw it with
python-pptx on a slide `add` made, using the grid helper in `references/pptx_patterns.md`, then
check and look as always.

## Rules that hold for every deck

- 16:9. Slide titles 28-36pt, body 16-20pt, nothing a reader needs under 12pt.
- At most about 6 items on a slide, each under about 15 words. More → another slide.
- Numbers get a chart or a KPI, not a sentence. Charts are native and editable (`chart`).
- Speaker notes carry the detail the slide leaves out.
- Contrast: dark text on light, or white on a saturated fill; never grey on colour.
- Never claim the deck was checked or looked at when a step failed: say which.
