#!/usr/bin/env python3
"""Validate OOXML structure, XML, rels, redline IDs/comment ranges, and baseline preservation."""

from __future__ import annotations

import argparse
from pathlib import Path
import posixpath
import sys
import xml.etree.ElementTree as ET
import zipfile

REQUIRED_PARTS = (
    "[Content_Types].xml",
    "_rels/.rels",
    "word/document.xml",
)
XML_SUFFIXES = {".xml", ".rels"}
W_NS = "{http://schemas.openxmlformats.org/wordprocessingml/2006/main}"
XML_SPACE_ATTR = "{http://www.w3.org/XML/1998/namespace}space"


def _read_package_files(target: Path) -> dict[str, bytes]:
    if target.is_dir():
        return {
            p.relative_to(target).as_posix(): p.read_bytes()
            for p in target.rglob("*")
            if p.is_file()
        }
    if target.is_file():
        with zipfile.ZipFile(target, "r") as zf:
            return {
                info.filename: zf.read(info.filename)
                for info in zf.infolist()
                if not info.is_dir()
            }
    raise FileNotFoundError(f"Target not found: {target}")


def _check_content_types(
    files: dict[str, bytes], root: ET.Element, errors: list[str]
) -> None:
    defaults: set[str] = set()
    overrides: set[str] = set()
    for child in root:
        tag = child.tag.rsplit("}", 1)[-1]
        if tag == "Default":
            ext = child.attrib.get("Extension", "").lower()
            if ext:
                defaults.add(ext)
        elif tag == "Override":
            part = child.attrib.get("PartName", "").lstrip("/")
            if part:
                overrides.add(part)

    for part_name in files:
        if part_name == "[Content_Types].xml":
            continue
        base = posixpath.basename(part_name)
        ext = base.rsplit(".", 1)[1].lower() if "." in base else ""
        if part_name not in overrides and ext not in defaults:
            errors.append(f"Missing [Content_Types].xml coverage for: {part_name}")


def _check_relationships(
    files: dict[str, bytes], rels_path: str, root: ET.Element, errors: list[str]
) -> None:
    rels_dir = posixpath.dirname(rels_path)
    base_dir = "" if rels_dir == "_rels" else posixpath.dirname(rels_dir)

    for rel in root:
        if rel.tag.rsplit("}", 1)[-1] != "Relationship":
            continue
        if rel.attrib.get("TargetMode", "").lower() == "external":
            continue
        target = rel.attrib.get("Target", "")
        if not target:
            errors.append(f"Empty Target in {rels_path}")
            continue
        resolved = target.lstrip("/") if target.startswith("/") else posixpath.normpath(
            posixpath.join(base_dir, target)
        )
        if resolved not in files:
            errors.append(
                f"Broken relationship in {rels_path}: '{target}' -> '{resolved}' not found"
            )


def _check_document_whitespace(root: ET.Element, warnings: list[str]) -> None:
    missing_preserve = 0
    for elem in root.iter(f"{W_NS}t"):
        text = elem.text or ""
        if (text.startswith(" ") or text.endswith(" ")) and elem.attrib.get(
            XML_SPACE_ATTR
        ) != "preserve":
            missing_preserve += 1
    if missing_preserve:
        warnings.append(
            f"{missing_preserve} <w:t> element(s) in word/document.xml have leading/trailing "
            'spaces without xml:space="preserve".'
        )


def _check_redlines_and_comments(
    doc_root: ET.Element,
    comments_root: ET.Element | None,
    errors: list[str],
) -> None:
    revision_ids: set[str] = set()
    for tag in (f"{W_NS}ins", f"{W_NS}del"):
        for el in doc_root.iter(tag):
            wid = el.attrib.get(f"{W_NS}id")
            if not wid:
                errors.append(f"Missing w:id on {tag.rsplit('}', 1)[-1]} element.")
            elif wid in revision_ids:
                errors.append(f"Duplicate tracked-change w:id={wid!r} in word/document.xml.")
            else:
                revision_ids.add(wid)

    starts = {
        el.attrib.get(f"{W_NS}id", "")
        for el in doc_root.iter(f"{W_NS}commentRangeStart")
    }
    ends = {
        el.attrib.get(f"{W_NS}id", "")
        for el in doc_root.iter(f"{W_NS}commentRangeEnd")
    }
    if starts != ends:
        errors.append(
            f"Unmatched comment range markers: start_ids={sorted(starts)} vs end_ids={sorted(ends)}"
        )

    if starts:
        if comments_root is None:
            errors.append("Document contains comment ranges, but word/comments.xml is missing.")
        else:
            defined = {
                el.attrib.get(f"{W_NS}id", "")
                for el in comments_root.iter(f"{W_NS}comment")
            }
            missing = starts - defined
            if missing:
                errors.append(
                    f"Comment range ID(s) {sorted(missing)} missing from word/comments.xml."
                )


def _extract_rejected_baseline_paragraphs(doc_root: ET.Element) -> list[str]:
    """Reconstruct paragraph texts as if all tracked changes were rejected."""

    def collect_text(node: ET.Element, in_ins: bool = False, in_del: bool = False) -> str:
        tag = node.tag
        if tag == f"{W_NS}ins":
            return ""
        if tag == f"{W_NS}del":
            in_del = True
        chunks: list[str] = []
        if tag == f"{W_NS}t" and not in_ins and not in_del:
            chunks.append(node.text or "")
        elif tag == f"{W_NS}delText" and in_del:
            chunks.append(node.text or "")
        for child in node:
            chunks.append(collect_text(child, in_ins=in_ins, in_del=in_del))
        return "".join(chunks)

    return [collect_text(p) for p in doc_root.iter(f"{W_NS}p")]


def check_baseline_preservation(
    target_doc_root: ET.Element, baseline_path: Path, errors: list[str]
) -> None:
    """Verify rejecting all tracked changes reproduces baseline_path's exact text."""
    base_files = _read_package_files(baseline_path)
    if "word/document.xml" not in base_files:
        errors.append(f"Baseline {baseline_path} is missing word/document.xml")
        return
    base_root = ET.fromstring(base_files["word/document.xml"])
    expected = _extract_rejected_baseline_paragraphs(base_root)
    actual = _extract_rejected_baseline_paragraphs(target_doc_root)

    if len(expected) != len(actual):
        errors.append(
            f"Baseline paragraph count mismatch: baseline={len(expected)}, redlined(rejected)={len(actual)}"
        )
        return
    for idx, (exp_p, act_p) in enumerate(zip(expected, actual), start=1):
        if exp_p != act_p:
            errors.append(
                f"Untracked text mutation in paragraph {idx}: expected {exp_p[:80]!r}, got {act_p[:80]!r}"
            )
            break


def validate_package(
    target: Path, baseline: Path | None = None
) -> tuple[list[str], list[str]]:
    """Return (errors, warnings) for a .docx file or unpacked directory."""
    errors: list[str] = []
    warnings: list[str] = []
    files = _read_package_files(target)

    for req in REQUIRED_PARTS:
        if req not in files:
            errors.append(f"Missing required OOXML part: {req}")

    parsed_roots: dict[str, ET.Element] = {}
    for name, data in files.items():
        if name.lower().endswith(tuple(XML_SUFFIXES)):
            try:
                parsed_roots[name] = ET.fromstring(data)
            except ET.ParseError as exc:
                errors.append(f"Malformed XML in {name}: {exc}")

    if "[Content_Types].xml" in parsed_roots:
        _check_content_types(files, parsed_roots["[Content_Types].xml"], errors)

    for name, root in parsed_roots.items():
        if name.endswith(".rels"):
            _check_relationships(files, name, root, errors)

    doc_root = parsed_roots.get("word/document.xml")
    if doc_root is not None:
        _check_document_whitespace(doc_root, warnings)
        _check_redlines_and_comments(
            doc_root, parsed_roots.get("word/comments.xml"), errors
        )
        if baseline is not None:
            check_baseline_preservation(doc_root, baseline, errors)

    return errors, warnings


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate a .docx file or unpacked directory (including redline integrity)."
    )
    parser.add_argument("target", type=Path, help="Path to .docx file or unpacked directory")
    parser.add_argument(
        "--baseline",
        type=Path,
        help="Original .docx to verify zero untracked text mutations outside w:ins/w:del",
    )
    args = parser.parse_args()

    try:
        errors, warnings = validate_package(args.target, baseline=args.baseline)
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    for warning in warnings:
        print(f"WARNING: {warning}", file=sys.stderr)
    for err in errors:
        print(f"ERROR: {err}", file=sys.stderr)

    if errors:
        print(f"Validation FAILED ({len(errors)} error(s), {len(warnings)} warning(s)).")
        return 1

    print(f"Validation PASSED ({len(warnings)} warning(s)).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
