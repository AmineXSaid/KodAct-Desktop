---
name: pptx
description: >-
  Comprehensive workflow for inspecting, extracting, creating, editing corporate
  templates (rearrange, clone, inventory, style-preserving replace), and visually
  verifying PowerPoint (.pptx) decks using python-pptx, LibreOffice, and
  contact-sheet thumbnail grids. Use whenever working with .pptx files or slides.
---

# PPTX Presentation Engineering Skill

Use this skill for all `.pptx` workflows: inspecting existing templates via contact sheets, rearranging/cloning template slides, building widescreen 16:9 decks with collision-free grid layouts, and running automated + visual QA.

---

## 1. Decision Tree

```text
What is the task?
├── Read / Extract / Inspect
│   ├── Slide text, tables, speaker notes ──▶ scripts/extract_text.py
│   └── Layouts, placeholders, shape IDs  ──▶ scripts/inspect_deck.py (or template_ops.py inventory)
│
├── Create New Presentation from Scratch (No Template)
│   └── python-pptx + SlideGrid layout helper (see references/pptx_patterns.md)
│       ──▶ scripts/validate.py ──▶ scripts/render_preview.py --grid ──▶ inspect grids
│
└── Build from or Edit an Existing Corporate Template
    └── 1. Visual overview:  scripts/render_preview.py template.pptx --grid
        2. Pick/clone order: scripts/template_ops.py rearrange template.pptx work.pptx --slides 0,4,4,8
        3. Dump shape IDs:   scripts/template_ops.py inventory work.pptx -o inv.json
        4. Populate text:    scripts/template_ops.py replace work.pptx edits.json -o final.pptx
        5. Verify & QA:      scripts/validate.py final.pptx ──▶ scripts/render_preview.py final.pptx --grid
```

---

## 2. Slide Layout Hygiene & Design Rules

1. **16:9 Canvas & Declarative Grid**:
   - Standardize on `Inches(13.333)` × `Inches(7.5)`.
   - Use the `SlideGrid` helper in [pptx_patterns.md](./references/pptx_patterns.md) so cards, charts, and columns snap to collision-free coordinates with `>= 0.25"` gutters and safe margins (`left >= 0.65"`, `top >= 0.5"`).
2. **Typography & Scannability**:
   - Cover Title: `36–44 pt` bold; Slide Title: `24–28 pt` bold; Card Header: `15–18 pt` bold; Body: `13–15 pt` (never `< 10 pt` for content).
   - Always set `text_frame.word_wrap = True`.
   - Keep bullets concise (`<= 5–6` bullets per frame, `<= 14` words per bullet). `validate.py` automatically warns on dense text walls (`> 8` paragraphs) and tiny fonts (`< 9 pt`).
3. **Visual Variety**:
   - Alternate across Hero, 3–4 KPI Cards, Chart + Takeaways, Two-Column Comparison, Process Steps, and Styled Table slides—never stack identical bullet slides back-to-back.

---

## 3. Template Surgery Workflow (`scripts/template_ops.py`)

Never try to manually delete or duplicate slides in raw `python-pptx` code (which breaks `.rels` targets). Use `template_ops.py`:

1. **Inspect template visually**: `python scripts/render_preview.py template.pptx --grid` — each tile banner displays both 1-based and 0-based indices (`Slide 5  [idx=4]`).
2. **Select, duplicate & reorder**: `python scripts/template_ops.py rearrange template.pptx work.pptx --slides 0,4,4,11` — clones repeated slides with remapped media/chart `rId`s and drops unused template slides cleanly.
3. **Inventory & Replace**: Export `inventory.json`, map updates by `shape_id`, `placeholder_idx`, or `name` (see [pptx_patterns.md](./references/pptx_patterns.md)), and run `template_ops.py replace` to update text while preserving the template's `<a:rPr>` font styling.
4. **Raw OOXML Theme/XML Edits**: If editing `ppt/theme/theme1.xml` directly, use `template_ops.py unpack` and `template_ops.py pack`.

---

## 4. Mandatory Verification & QA Loop

1. **Automated Structural & Geometry Check**:
   ```bash
   python scripts/validate.py output.pptx
   ```
   Checks OOXML parts, `.rels` targets, off-canvas shapes, text-box overlaps (`> 15%`), tiny fonts (`< 9 pt`), and dense bullet walls.
2. **Contact-Sheet Visual QA**:
   ```bash
   python scripts/render_preview.py output.pptx --out-dir ./preview --grid
   ```
   Inspect `grid-*.png` (up to 12 slides per image) first to audit alignment, contrast, and overflow across the whole deck in minimal turns, then inspect individual `slide-*.png` files for fine detail.

---

## 5. Helper Scripts Reference

| Script | Purpose | Usage |
| :--- | :--- | :--- |
| [template_ops.py](./scripts/template_ops.py) | `rearrange`, `inventory`, `replace`, `unpack`, `pack` | `python scripts/template_ops.py rearrange in.pptx out.pptx --slides 0,2,2,5` |
| [inspect_deck.py](./scripts/inspect_deck.py) | Quick CLI summary of layouts, placeholders, bounds | `python scripts/inspect_deck.py deck.pptx` |
| [extract_text.py](./scripts/extract_text.py) | Extract slide text, tables, notes (MD/JSON) | `python scripts/extract_text.py deck.pptx -o out.md` |
| [validate.py](./scripts/validate.py) | Check OOXML, `.rels`, overlaps, bounds, font size | `python scripts/validate.py out.pptx` |
| [render_preview.py](./scripts/render_preview.py) | Isolated LibreOffice + Poppler PNGs & `--grid` sheets | `python scripts/render_preview.py out.pptx --grid` |
