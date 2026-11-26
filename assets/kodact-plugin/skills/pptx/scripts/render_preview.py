#!/usr/bin/env python3
"""Render a .pptx file to PDF (isolated LibreOffice), PNG slides, and dual-indexed contact-sheet grids."""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


SHARED_SCRIPTS = Path(__file__).resolve().parents[3] / "scripts"
if str(SHARED_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SHARED_SCRIPTS))

from _kodact_office import contact_sheets, convert_office_to_pdf, render_pdf_to_pngs  # noqa: E402


def build_slide_grids(
    pngs: list[Path], out_dir: Path, cols: int = 3, rows: int = 4
) -> list[Path]:
    """Slides on contact sheets, each labelled with its 1-based and 0-based index."""
    return contact_sheets(pngs, out_dir, lambda i: f"Slide {i + 1}  [idx={i}]", cols, rows, 520)


def render_pptx_preview(
    pptx_path: Path,
    out_dir: Path,
    dpi: int = 150,
    grid: bool = False,
    cols: int = 3,
    rows: int = 4,
) -> tuple[list[Path], list[Path]]:
    if not pptx_path.is_file():
        raise FileNotFoundError(f"Input PPTX not found: {pptx_path}")

    pdf_path = convert_office_to_pdf(pptx_path, out_dir, timeout=120)
    pngs = render_pdf_to_pngs(pdf_path, out_dir, prefix="slide", dpi=dpi)

    grids: list[Path] = []
    if grid:
        try:
            grids = build_slide_grids(pngs, out_dir, cols=cols, rows=rows)
        except ImportError:
            print(
                "Warning: Pillow not installed; skipping --grid contact sheets.",
                file=sys.stderr,
            )

    return pngs, grids


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Render .pptx slides to PNG and optional contact-sheet grids for visual QA."
    )
    parser.add_argument("input_pptx", type=Path, help="Path to .pptx file")
    parser.add_argument(
        "--out-dir", type=Path, default=Path("./preview"), help="Output directory for PDF/PNGs"
    )
    parser.add_argument("--dpi", type=int, default=150, help="PNG render resolution (default: 150)")
    parser.add_argument(
        "--grid",
        action="store_true",
        help="Generate labeled 3x4 slide contact-sheet grids (requires Pillow)",
    )
    parser.add_argument("--cols", type=int, default=3, help="Grid columns (default: 3)")
    parser.add_argument("--rows", type=int, default=4, help="Grid rows (default: 4)")
    args = parser.parse_args()

    try:
        pngs, grids = render_pptx_preview(
            args.input_pptx, args.out_dir, dpi=args.dpi, grid=args.grid, cols=args.cols, rows=args.rows
        )
    except subprocess.CalledProcessError as exc:
        print(f"Command failed: {exc.stderr or exc}", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(f"Rendered {len(pngs)} slide(s):")
    for png in pngs:
        print(f"  {png}")
    for g in grids:
        print(f"  [grid] {g}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
