#!/usr/bin/env python3
"""Render a .docx file to PDF (LibreOffice) and PNG pages/contact-sheets for visual QA."""

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


def _build_contact_sheets(
    pngs: list[Path], out_dir: Path, cols: int = 3, rows: int = 3
) -> list[Path]:
    """Pages on contact sheets, each labelled with its page number."""
    return contact_sheets(pngs, out_dir, lambda i: f"Page {i + 1}", cols, rows, 480)


def render_preview(
    docx_path: Path, out_dir: Path, dpi: int = 150, grid: bool = False
) -> tuple[list[Path], list[Path]]:
    """Convert docx_path -> PDF -> PNGs (and optional contact sheets) in out_dir."""
    if not docx_path.is_file():
        raise FileNotFoundError(f"Input file not found: {docx_path}")

    pdf_path = convert_office_to_pdf(docx_path, out_dir, timeout=120)
    pngs = render_pdf_to_pngs(pdf_path, out_dir, prefix="page", dpi=dpi)

    grids: list[Path] = []
    if grid:
        try:
            grids = _build_contact_sheets(pngs, out_dir)
        except ImportError:
            print(
                "Warning: Pillow not installed; skipping --grid contact sheets.",
                file=sys.stderr,
            )
    return pngs, grids


def main() -> int:
    parser = argparse.ArgumentParser(description="Render .docx pages to PNG for visual QA.")
    parser.add_argument("input_docx", type=Path, help="Path to .docx file")
    parser.add_argument(
        "--out-dir", type=Path, default=Path("./preview"), help="Output directory for PDF/PNGs"
    )
    parser.add_argument("--dpi", type=int, default=150, help="PNG render resolution (default: 150)")
    parser.add_argument(
        "--grid",
        action="store_true",
        help="Also generate 3x3 labeled contact-sheet grid PNGs (requires Pillow)",
    )
    args = parser.parse_args()

    try:
        pngs, grids = render_preview(args.input_docx, args.out_dir, dpi=args.dpi, grid=args.grid)
    except subprocess.CalledProcessError as exc:
        print(f"Command failed: {exc.stderr or exc}", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(f"Rendered {len(pngs)} page(s):")
    for png in pngs:
        print(f"  {png}")
    for g in grids:
        print(f"  [grid] {g}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
