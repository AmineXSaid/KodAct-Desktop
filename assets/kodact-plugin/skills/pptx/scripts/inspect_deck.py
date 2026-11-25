#!/usr/bin/env python3
"""Inspect layouts, placeholders, shapes, and speaker notes in a .pptx deck."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

EMU_PER_INCH = 914400


def _emu_to_in(val: int | None) -> float:
    return round((val or 0) / EMU_PER_INCH, 2)


def inspect_deck(pptx_path: Path) -> dict:
    if not pptx_path.is_file():
        raise FileNotFoundError(f"Input PPTX not found: {pptx_path}")

    from pptx import Presentation

    prs = Presentation(str(pptx_path))
    layouts = [
        {
            "index": idx,
            "name": layout.name,
            "placeholders": [
                {"idx": ph.placeholder_format.idx, "type": str(ph.placeholder_format.type)}
                for ph in layout.placeholders
            ],
        }
        for idx, layout in enumerate(prs.slide_layouts)
    ]

    slides = []
    for s_idx, slide in enumerate(prs.slides, start=1):
        shapes_info = []
        for sh in slide.shapes:
            text_preview = ""
            if sh.has_text_frame:
                text_preview = " ".join(sh.text_frame.text.split())[:60]
            shapes_info.append(
                {
                    "name": sh.name,
                    "type": str(sh.shape_type),
                    "is_placeholder": sh.is_placeholder,
                    "bounds_in": {
                        "left": _emu_to_in(sh.left),
                        "top": _emu_to_in(sh.top),
                        "width": _emu_to_in(sh.width),
                        "height": _emu_to_in(sh.height),
                    },
                    "text_preview": text_preview,
                }
            )
        notes = ""
        if slide.has_notes_slide and slide.notes_slide.notes_text_frame:
            notes = slide.notes_slide.notes_text_frame.text.strip()
        slides.append(
            {
                "slide": s_idx,
                "layout": slide.slide_layout.name,
                "shape_count": len(shapes_info),
                "shapes": shapes_info,
                "has_notes": bool(notes),
            }
        )

    return {
        "slide_width_in": _emu_to_in(prs.slide_width),
        "slide_height_in": _emu_to_in(prs.slide_height),
        "slide_count": len(slides),
        "layouts": layouts,
        "slides": slides,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Inspect a .pptx presentation structure.")
    parser.add_argument("input_pptx", type=Path, help="Path to .pptx file")
    parser.add_argument("--json", dest="json_out", type=Path, help="Write JSON report to file")
    args = parser.parse_args()

    try:
        info = inspect_deck(args.input_pptx)
    except ImportError:
        print("Error: 'python-pptx' is required. Install with: pip install python-pptx", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(info, indent=2), encoding="utf-8")
        print(f"Wrote deck inspection to {args.json_out}")
        return 0

    print(
        f"Deck: {args.input_pptx} ({info['slide_width_in']}\" x {info['slide_height_in']}\", "
        f"{info['slide_count']} slides)"
    )
    print("\nSlide Layouts:")
    for lay in info["layouts"]:
        phs = ", ".join(f"{p['idx']}:{p['type']}" for p in lay["placeholders"])
        print(f"  [{lay['index']}] {lay['name']} (placeholders: {phs or 'none'})")

    print("\nSlides:")
    for s in info["slides"]:
        print(f"  Slide {s['slide']} [{s['layout']}] ({s['shape_count']} shapes, notes={s['has_notes']})")
        for sh in s["shapes"]:
            b = sh["bounds_in"]
            preview = f" -> {sh['text_preview']!r}" if sh["text_preview"] else ""
            print(
                f"    - {sh['name']} ({b['left']}, {b['top']}, {b['width']}x{b['height']}\"){preview}"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
