#!/usr/bin/env python3
"""Extract slide text, tables, and speaker notes from a .pptx file."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def extract_deck(pptx_path: Path) -> list[dict]:
    if not pptx_path.is_file():
        raise FileNotFoundError(f"Input PPTX not found: {pptx_path}")

    from pptx import Presentation

    prs = Presentation(str(pptx_path))
    slides_data: list[dict] = []

    for idx, slide in enumerate(prs.slides, start=1):
        texts: list[str] = []
        tables: list[list[list[str]]] = []

        # Sort shapes top-to-bottom, left-to-right for natural reading order.
        ordered_shapes = sorted(slide.shapes, key=lambda s: (s.top or 0, s.left or 0))
        for shape in ordered_shapes:
            if shape.has_text_frame:
                content = "\n".join(
                    p.text.strip() for p in shape.text_frame.paragraphs if p.text.strip()
                )
                if content:
                    texts.append(content)
            if shape.has_table:
                table_rows = [
                    [" ".join(cell.text.split()) for cell in row.cells]
                    for row in shape.table.rows
                ]
                tables.append(table_rows)

        notes = ""
        if slide.has_notes_slide and slide.notes_slide.notes_text_frame:
            notes = slide.notes_slide.notes_text_frame.text.strip()

        slides_data.append(
            {
                "slide": idx,
                "layout": slide.slide_layout.name,
                "texts": texts,
                "tables": tables,
                "notes": notes,
            }
        )

    return slides_data


def _format_markdown(slides: list[dict]) -> str:
    blocks: list[str] = []
    for s in slides:
        parts = [f"## Slide {s['slide']} ({s['layout']})"]
        if s["texts"]:
            parts.append("\n\n".join(s["texts"]))
        for t_idx, tbl in enumerate(s["tables"], start=1):
            if not tbl:
                continue
            parts.append(f"### Table {t_idx}")
            parts.append("| " + " | ".join(tbl[0]) + " |")
            parts.append("| " + " | ".join("---" for _ in tbl[0]) + " |")
            for row in tbl[1:]:
                parts.append("| " + " | ".join(row) + " |")
        if s["notes"]:
            parts.append(f"> **Speaker Notes:** {s['notes']}")
        blocks.append("\n\n".join(parts))
    return "\n\n---\n\n".join(blocks) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description="Extract text, tables, and notes from a .pptx.")
    parser.add_argument("input_pptx", type=Path, help="Path to .pptx file")
    parser.add_argument("--format", choices=("markdown", "json"), default="markdown")
    parser.add_argument("-o", "--out", type=Path, help="Output file path")
    args = parser.parse_args()

    try:
        slides = extract_deck(args.input_pptx)
    except ImportError:
        print("Error: 'python-pptx' is required. Install with: pip install python-pptx", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    output = (
        json.dumps(slides, indent=2, ensure_ascii=False)
        if args.format == "json"
        else _format_markdown(slides)
    )
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(output, encoding="utf-8")
        print(f"Extracted {len(slides)} slide(s) -> {args.out}")
    else:
        print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
