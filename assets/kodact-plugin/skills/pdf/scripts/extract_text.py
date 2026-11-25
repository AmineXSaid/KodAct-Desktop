#!/usr/bin/env python3
"""Extract text and optional tables from a PDF using pdfplumber or pypdf."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def parse_page_spec(spec: str | None, total_pages: int) -> list[int]:
    """Parse 1-based page spec (e.g. '1-3,5') into sorted 0-based indices."""
    if not spec:
        return list(range(total_pages))
    indices: set[int] = set()
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start_s, end_s = part.split("-", 1)
            start = max(1, int(start_s))
            end = min(total_pages, int(end_s))
            indices.update(range(start - 1, end))
        else:
            idx = int(part) - 1
            if 0 <= idx < total_pages:
                indices.add(idx)
    return sorted(indices)


def _extract_with_pdfplumber(
    pdf_path: Path, page_spec: str | None, include_tables: bool
) -> list[dict]:
    import pdfplumber

    results: list[dict] = []
    with pdfplumber.open(pdf_path) as pdf:
        indices = parse_page_spec(page_spec, len(pdf.pages))
        for idx in indices:
            page = pdf.pages[idx]
            entry: dict = {
                "page": idx + 1,
                "text": (page.extract_text() or "").strip(),
            }
            if include_tables:
                entry["tables"] = page.extract_tables() or []
            results.append(entry)
    return results


def _extract_with_pypdf(pdf_path: Path, page_spec: str | None) -> list[dict]:
    from pypdf import PdfReader

    reader = PdfReader(str(pdf_path))
    indices = parse_page_spec(page_spec, len(reader.pages))
    return [
        {"page": idx + 1, "text": (reader.pages[idx].extract_text() or "").strip()}
        for idx in indices
    ]


def extract_pdf(pdf_path: Path, page_spec: str | None, include_tables: bool) -> list[dict]:
    """Extract text (and optionally tables) from pdf_path."""
    if not pdf_path.is_file():
        raise FileNotFoundError(f"Input PDF not found: {pdf_path}")

    try:
        return _extract_with_pdfplumber(pdf_path, page_spec, include_tables)
    except ImportError:
        if include_tables:
            raise ImportError(
                "Table extraction requires 'pdfplumber'. Install with: pip install pdfplumber"
            )
    try:
        return _extract_with_pypdf(pdf_path, page_spec)
    except ImportError as exc:
        raise ImportError(
            "Install 'pdfplumber' or 'pypdf' to extract PDF text (pip install pdfplumber pypdf)."
        ) from exc


def _format_markdown(pages: list[dict]) -> str:
    chunks: list[str] = []
    for p in pages:
        chunks.append(f"## Page {p['page']}\n\n{p['text'] or '_No selectable text_'}")
        for t_idx, table in enumerate(p.get("tables", []), start=1):
            if not table:
                continue
            chunks.append(f"\n### Page {p['page']} - Table {t_idx}\n")
            header = [str(c or "").replace("\n", " ") for c in table[0]]
            chunks.append("| " + " | ".join(header) + " |")
            chunks.append("| " + " | ".join("---" for _ in header) + " |")
            for row in table[1:]:
                cells = [str(c or "").replace("\n", " ") for c in row]
                chunks.append("| " + " | ".join(cells) + " |")
    return "\n\n".join(chunks) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description="Extract text and tables from a PDF.")
    parser.add_argument("input_pdf", type=Path, help="Path to source .pdf file")
    parser.add_argument("--pages", help="1-based page ranges, e.g. '1-3,5'")
    parser.add_argument("--tables", action="store_true", help="Extract tables (requires pdfplumber)")
    parser.add_argument(
        "--format", choices=("markdown", "json"), default="markdown", help="Output format"
    )
    parser.add_argument("-o", "--out", type=Path, help="Write output to file instead of stdout")
    args = parser.parse_args()

    try:
        pages = extract_pdf(args.input_pdf, args.pages, args.tables)
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    output = (
        json.dumps(pages, indent=2, ensure_ascii=False)
        if args.format == "json"
        else _format_markdown(pages)
    )

    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(output, encoding="utf-8")
        print(f"Wrote {len(pages)} page(s) to {args.out}")
    else:
        print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
