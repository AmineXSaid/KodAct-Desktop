#!/usr/bin/env python3
"""Render an .xlsx workbook to PDF (isolated LibreOffice) and PNG pages/grids for visual QA."""

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

from _kodact_office import convert_office_to_pdf, render_pdf_to_pngs  # noqa: E402


def _build_contact_sheets(
    pngs: list[Path], out_dir: Path, cols: int = 2, rows: int = 2
) -> list[Path]:
    from PIL import Image, ImageDraw

    per_sheet = cols * rows
    tile_w = 640
    first = Image.open(pngs[0])
    tile_h = int(tile_w * (first.height / max(1, first.width)))
    banner_h = 28
    pad = 16
    sheets: list[Path] = []

    for s_idx in range(0, len(pngs), per_sheet):
        batch = pngs[s_idx : s_idx + per_sheet]
        used_rows = (len(batch) + cols - 1) // cols
        canvas_w = pad + cols * (tile_w + pad)
        canvas_h = pad + used_rows * (tile_h + banner_h + pad)
        sheet = Image.new("RGB", (canvas_w, canvas_h), (241, 245, 249))
        draw = ImageDraw.Draw(sheet)

        for idx, img_path in enumerate(batch):
            r, c = divmod(idx, cols)
            x = pad + c * (tile_w + pad)
            y = pad + r * (tile_h + banner_h + pad)
            draw.rectangle([x, y, x + tile_w, y + banner_h], fill=(30, 41, 59))
            draw.text((x + 10, y + 7), f"Page {s_idx + idx + 1}", fill=(255, 255, 255))
            with Image.open(img_path) as im:
                thumb = im.convert("RGB").resize((tile_w, tile_h))
                sheet.paste(thumb, (x, y + banner_h))
            draw.rectangle(
                [x, y, x + tile_w, y + banner_h + tile_h], outline=(148, 163, 184), width=1
            )

        out_path = out_dir / f"grid-{len(sheets) + 1}.png"
        sheet.save(out_path)
        sheets.append(out_path)
    return sheets


def render_xlsx_preview(
    xlsx_path: Path, out_dir: Path, dpi: int = 150, grid: bool = False
) -> tuple[list[Path], list[Path]]:
    if not xlsx_path.is_file():
        raise FileNotFoundError(f"Input workbook not found: {xlsx_path}")

    pdf_path = convert_office_to_pdf(xlsx_path, out_dir, timeout=120)
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
    parser = argparse.ArgumentParser(description="Render .xlsx sheets to PNG for visual QA.")
    parser.add_argument("input_xlsx", type=Path, help="Path to .xlsx workbook")
    parser.add_argument(
        "--out-dir", type=Path, default=Path("./preview"), help="Output directory for PDF/PNGs"
    )
    parser.add_argument("--dpi", type=int, default=150, help="PNG render resolution (default: 150)")
    parser.add_argument(
        "--grid",
        action="store_true",
        help="Also generate 2x2 labeled contact-sheet grid PNGs (requires Pillow)",
    )
    args = parser.parse_args()

    try:
        pngs, grids = render_xlsx_preview(
            args.input_xlsx, args.out_dir, dpi=args.dpi, grid=args.grid
        )
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
