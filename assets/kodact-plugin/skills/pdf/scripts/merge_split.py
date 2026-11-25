#!/usr/bin/env python3
"""Merge, split/extract, or rotate PDF pages using pypdf."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from extract_text import parse_page_spec


def merge_pdfs(inputs: list[Path], output: Path) -> int:
    from pypdf import PdfWriter

    writer = PdfWriter()
    for path in inputs:
        if not path.is_file():
            raise FileNotFoundError(f"Input PDF not found: {path}")
        writer.append(str(path))

    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as fh:
        writer.write(fh)
    return len(writer.pages)


def split_pdf(input_pdf: Path, page_spec: str, output: Path) -> int:
    from pypdf import PdfReader, PdfWriter

    if not input_pdf.is_file():
        raise FileNotFoundError(f"Input PDF not found: {input_pdf}")

    reader = PdfReader(str(input_pdf))
    writer = PdfWriter()
    indices = parse_page_spec(page_spec, len(reader.pages))
    if not indices:
        raise ValueError(f"No valid pages matched spec '{page_spec}' (total={len(reader.pages)})")

    for idx in indices:
        writer.add_page(reader.pages[idx])

    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as fh:
        writer.write(fh)
    return len(indices)


def rotate_pdf(input_pdf: Path, angle: int, page_spec: str | None, output: Path) -> int:
    from pypdf import PdfReader, PdfWriter

    if not input_pdf.is_file():
        raise FileNotFoundError(f"Input PDF not found: {input_pdf}")

    reader = PdfReader(str(input_pdf))
    writer = PdfWriter()
    targets = set(parse_page_spec(page_spec, len(reader.pages)))

    for idx, page in enumerate(reader.pages):
        if idx in targets:
            page.rotate(angle)
        writer.add_page(page)

    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("wb") as fh:
        writer.write(fh)
    return len(targets)


def main() -> int:
    parser = argparse.ArgumentParser(description="Merge, split, or rotate PDF pages.")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_merge = sub.add_parser("merge", help="Merge multiple PDFs into one")
    p_merge.add_argument("inputs", nargs="+", type=Path, help="Input PDF files in order")
    p_merge.add_argument("-o", "--out", type=Path, required=True, help="Output PDF path")

    p_split = sub.add_parser("split", help="Extract selected pages into a new PDF")
    p_split.add_argument("input_pdf", type=Path, help="Source PDF")
    p_split.add_argument("--pages", required=True, help="1-based page ranges, e.g. '1-3,6'")
    p_split.add_argument("-o", "--out", type=Path, required=True, help="Output PDF path")

    p_rot = sub.add_parser("rotate", help="Rotate selected pages by 90/180/270 degrees")
    p_rot.add_argument("input_pdf", type=Path, help="Source PDF")
    p_rot.add_argument("--angle", type=int, choices=(90, 180, 270), required=True)
    p_rot.add_argument("--pages", help="1-based page ranges (default: all pages)")
    p_rot.add_argument("-o", "--out", type=Path, required=True, help="Output PDF path")

    args = parser.parse_args()

    try:
        if args.cmd == "merge":
            total = merge_pdfs(args.inputs, args.out)
            print(f"Merged {len(args.inputs)} PDF(s) ({total} pages) -> {args.out}")
        elif args.cmd == "split":
            count = split_pdf(args.input_pdf, args.pages, args.out)
            print(f"Extracted {count} page(s) -> {args.out}")
        elif args.cmd == "rotate":
            count = rotate_pdf(args.input_pdf, args.angle, args.pages, args.out)
            print(f"Rotated {count} page(s) by {args.angle}° -> {args.out}")
    except ImportError:
        print("Error: 'pypdf' is required. Install with: pip install pypdf", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
