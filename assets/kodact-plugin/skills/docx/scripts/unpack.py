#!/usr/bin/env python3
"""Unpack a .docx archive into a directory with safely pretty-printed XML."""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import sys
from xml.dom import minidom
import zipfile

XML_SUFFIXES = {".xml", ".rels"}


def _has_non_whitespace_text(node: minidom.Node) -> bool:
    return any(
        child.nodeType in (child.TEXT_NODE, child.CDATA_SECTION_NODE)
        and bool(child.data.strip())
        for child in node.childNodes
    )


def _format_element(node: minidom.Node, depth: int = 0, indent: str = "  ") -> None:
    """Indent element-only containers while leaving text/mixed content untouched."""
    if node.nodeType != node.ELEMENT_NODE:
        return
    if _has_non_whitespace_text(node):
        return

    element_children = [c for c in node.childNodes if c.nodeType == c.ELEMENT_NODE]
    if not element_children:
        return

    doc = node.ownerDocument
    # Remove existing whitespace-only text nodes before re-indenting.
    for child in list(node.childNodes):
        if child.nodeType == child.TEXT_NODE and not child.data.strip():
            node.removeChild(child)

    for child in element_children:
        _format_element(child, depth + 1, indent)

    new_children: list[minidom.Node] = []
    for child in element_children:
        new_children.append(doc.createTextNode("\n" + indent * (depth + 1)))
        new_children.append(child)
    new_children.append(doc.createTextNode("\n" + indent * depth))

    for child in list(node.childNodes):
        node.removeChild(child)
    for child in new_children:
        node.appendChild(child)


def pretty_print_xml(raw_bytes: bytes) -> bytes:
    """Pretty-print XML bytes while preserving prefixes and text whitespace."""
    dom = minidom.parseString(raw_bytes)
    if dom.documentElement is not None:
        _format_element(dom.documentElement, depth=0)
    return dom.toxml(encoding="utf-8")


def unpack_docx(docx_path: Path, out_dir: Path, overwrite: bool = False) -> int:
    """Extract a .docx file to out_dir and pretty-print XML/rels files."""
    if not docx_path.is_file():
        raise FileNotFoundError(f"Input file not found: {docx_path}")

    if out_dir.exists():
        if not overwrite:
            raise FileExistsError(
                f"Output directory already exists: {out_dir} (pass --force to overwrite)"
            )
        shutil.rmtree(out_dir)

    out_dir.mkdir(parents=True, exist_ok=True)
    resolved_out = out_dir.resolve()
    xml_count = 0

    with zipfile.ZipFile(docx_path, "r") as zf:
        for info in zf.infolist():
            if info.is_dir():
                continue
            target = (resolved_out / info.filename).resolve()
            if not target.is_relative_to(resolved_out):
                raise ValueError(f"Unsafe zip entry path: {info.filename}")
            target.parent.mkdir(parents=True, exist_ok=True)
            data = zf.read(info.filename)
            if target.name.lower().endswith(tuple(XML_SUFFIXES)):
                try:
                    data = pretty_print_xml(data)
                    xml_count += 1
                except Exception as exc:
                    print(
                        f"Warning: could not pretty-print {info.filename}: {exc}",
                        file=sys.stderr,
                    )
            target.write_bytes(data)

    return xml_count


def main() -> int:
    parser = argparse.ArgumentParser(description="Unpack a .docx file for XML editing.")
    parser.add_argument("input_docx", type=Path, help="Path to source .docx file")
    parser.add_argument("output_dir", type=Path, help="Directory to extract into")
    parser.add_argument(
        "--force", action="store_true", help="Overwrite output directory if it exists"
    )
    args = parser.parse_args()

    try:
        count = unpack_docx(args.input_docx, args.output_dir, overwrite=args.force)
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(f"Unpacked {args.input_docx} -> {args.output_dir} ({count} XML files formatted)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
