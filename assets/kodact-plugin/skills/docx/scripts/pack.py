#!/usr/bin/env python3
"""Validate and repack an unpacked OOXML directory into a .docx archive."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from validate import validate_package


def pack_docx(input_dir: Path, output_docx: Path, skip_validate: bool = False) -> int:
    """Pack input_dir into output_docx, optionally running OOXML validation first."""
    if not input_dir.is_dir():
        raise NotADirectoryError(f"Input directory not found: {input_dir}")

    if not skip_validate:
        errors, warnings = validate_package(input_dir)
        for warning in warnings:
            print(f"Warning: {warning}", file=sys.stderr)
        if errors:
            for err in errors:
                print(f"Validation error: {err}", file=sys.stderr)
            raise ValueError(f"Validation failed with {len(errors)} error(s); aborting pack.")

    output_docx.parent.mkdir(parents=True, exist_ok=True)
    file_count = 0

    with zipfile.ZipFile(output_docx, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(input_dir.rglob("*")):
            if path.is_file():
                arcname = path.relative_to(input_dir).as_posix()
                zf.write(path, arcname)
                file_count += 1

    return file_count


def main() -> int:
    parser = argparse.ArgumentParser(description="Pack an unpacked directory into a .docx file.")
    parser.add_argument("input_dir", type=Path, help="Unpacked .docx directory")
    parser.add_argument("output_docx", type=Path, help="Destination .docx file")
    parser.add_argument(
        "--no-validate",
        action="store_true",
        help="Skip OOXML validation before packing",
    )
    args = parser.parse_args()

    try:
        count = pack_docx(args.input_dir, args.output_docx, skip_validate=args.no_validate)
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(f"Packed {count} files into {args.output_docx}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
