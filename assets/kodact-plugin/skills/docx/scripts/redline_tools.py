#!/usr/bin/env python3
"""Programmatic OOXML tracked changes (w:ins/w:del) and comments for .docx files."""

from __future__ import annotations

import argparse
from copy import deepcopy
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parent))
from pack import pack_docx
from unpack import unpack_docx

W_URI = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
W_NS = f"{{{W_URI}}}"
XML_SPACE = "{http://www.w3.org/XML/1998/namespace}space"
PKG_REL_URI = "http://schemas.openxmlformats.org/package/2006/relationships"
COMMENTS_REL_TYPE = (
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments"
)
CT_URI = "http://schemas.openxmlformats.org/package/2006/content-types"
COMMENTS_CT = (
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"
)

ET.register_namespace("w", W_URI)
ET.register_namespace("", PKG_REL_URI)


def _set_text_preserving_space(elem: ET.Element, text: str) -> None:
    elem.text = text
    if text.startswith(" ") or text.endswith(" "):
        elem.set(XML_SPACE, "preserve")
    elif XML_SPACE in elem.attrib:
        del elem.attrib[XML_SPACE]


def _next_w_id(roots: list[ET.Element]) -> int:
    max_id = 0
    for root in roots:
        for el in root.iter():
            raw = el.attrib.get(f"{W_NS}id") or el.attrib.get("id")
            if raw and raw.isdigit():
                max_id = max(max_id, int(raw))
    return max_id + 1


def _flatten_paragraph_runs(p: ET.Element) -> list[tuple[ET.Element, str]]:
    """Return direct child <w:r> elements and their plain text."""
    runs: list[tuple[ET.Element, str]] = []
    for child in list(p):
        if child.tag == f"{W_NS}r":
            txt = "".join(t.text or "" for t in child.findall(f"{W_NS}t"))
            runs.append((child, txt))
    return runs


def _make_run(rpr: ET.Element | None, text: str, del_mode: bool = False) -> ET.Element:
    r = ET.Element(f"{W_NS}r")
    if rpr is not None:
        r.append(deepcopy(rpr))
    t_tag = f"{W_NS}delText" if del_mode else f"{W_NS}t"
    t = ET.SubElement(r, t_tag)
    _set_text_preserving_space(t, text)
    return r


def apply_tracked_replacement(
    doc_root: ET.Element,
    find_text: str,
    replace_text: str,
    author: str,
    date_iso: str,
    next_id: int,
    occurrence: int = 1,
) -> int:
    """Replace `find_text` with `<w:del>` + `<w:ins>` preserving run `<w:rPr>`."""
    if not find_text:
        raise ValueError("find_text cannot be empty")

    seen = 0
    for p in doc_root.iter(f"{W_NS}p"):
        runs = _flatten_paragraph_runs(p)
        if not runs:
            continue
        full_text = "".join(txt for _, txt in runs)
        search_from = 0
        while True:
            idx = full_text.find(find_text, search_from)
            if idx == -1:
                break
            seen += 1
            if seen < occurrence:
                search_from = idx + len(find_text)
                continue

            end_idx = idx + len(find_text)
            # Locate affected runs and splice cleanly.
            cursor = 0
            affected: list[tuple[ET.Element, int, int, str]] = []
            for r_el, r_txt in runs:
                r_start, r_end = cursor, cursor + len(r_txt)
                cursor = r_end
                if r_end <= idx or r_start >= end_idx:
                    continue
                affected.append((r_el, r_start, r_end, r_txt))

            if not affected:
                raise RuntimeError(f"Could not map runs for '{find_text}'")

            first_run, first_start, _, first_txt = affected[0]
            last_run, last_start, _, last_txt = affected[-1]
            first_rpr = first_run.find(f"{W_NS}rPr")
            last_rpr = last_run.find(f"{W_NS}rPr")

            prefix = first_txt[: idx - first_start]
            suffix = last_txt[end_idx - last_start :]

            children = list(p)
            insert_pos = children.index(first_run)
            for r_el, _, _, _ in affected:
                p.remove(r_el)

            new_nodes: list[ET.Element] = []
            if prefix:
                new_nodes.append(_make_run(first_rpr, prefix))

            del_el = ET.Element(
                f"{W_NS}del",
                {f"{W_NS}id": str(next_id), f"{W_NS}author": author, f"{W_NS}date": date_iso},
            )
            del_el.append(_make_run(first_rpr, find_text, del_mode=True))
            new_nodes.append(del_el)
            next_id += 1

            if replace_text:
                ins_el = ET.Element(
                    f"{W_NS}ins",
                    {
                        f"{W_NS}id": str(next_id),
                        f"{W_NS}author": author,
                        f"{W_NS}date": date_iso,
                    },
                )
                ins_el.append(_make_run(first_rpr, replace_text, del_mode=False))
                new_nodes.append(ins_el)
                next_id += 1

            if suffix:
                new_nodes.append(_make_run(last_rpr, suffix))

            for offset, node in enumerate(new_nodes):
                p.insert(insert_pos + offset, node)

            return next_id

    raise ValueError(f"Target text not found (occurrence {occurrence}): {find_text!r}")


def _ensure_comments_part(unpacked_dir: Path) -> tuple[Path, ET.Element]:
    comments_path = unpacked_dir / "word" / "comments.xml"
    if comments_path.is_file():
        return comments_path, ET.parse(comments_path).getroot()

    root = ET.Element(f"{W_NS}comments")
    # Register relationship in word/_rels/document.xml.rels
    rels_path = unpacked_dir / "word" / "_rels" / "document.xml.rels"
    rels_path.parent.mkdir(parents=True, exist_ok=True)
    if rels_path.is_file():
        rels_root = ET.parse(rels_path).getroot()
    else:
        rels_root = ET.Element(f"{{{PKG_REL_URI}}}Relationships")

    has_rel = any(
        rel.attrib.get("Type") == COMMENTS_REL_TYPE for rel in rels_root
    )
    if not has_rel:
        existing_ids = {rel.attrib.get("Id", "") for rel in rels_root}
        rid_num = 1
        while f"rId{rid_num}" in existing_ids:
            rid_num += 1
        ET.SubElement(
            rels_root,
            f"{{{PKG_REL_URI}}}Relationship",
            {"Id": f"rId{rid_num}", "Type": COMMENTS_REL_TYPE, "Target": "comments.xml"},
        )
        rels_path.write_bytes(ET.tostring(rels_root, encoding="utf-8", xml_declaration=True))

    # Register Override in [Content_Types].xml
    ct_path = unpacked_dir / "[Content_Types].xml"
    ct_root = ET.parse(ct_path).getroot()
    has_ct = any(
        el.attrib.get("PartName") == "/word/comments.xml" for el in ct_root
    )
    if not has_ct:
        ET.SubElement(
            ct_root,
            f"{{{CT_URI}}}Override",
            {"PartName": "/word/comments.xml", "ContentType": COMMENTS_CT},
        )
        ct_path.write_bytes(ET.tostring(ct_root, encoding="utf-8", xml_declaration=True))

    return comments_path, root


def add_comment_on_text(
    doc_root: ET.Element,
    comments_root: ET.Element,
    anchor_text: str,
    comment_text: str,
    author: str,
    initials: str,
    date_iso: str,
    comment_id: int,
    occurrence: int = 1,
) -> int:
    """Wrap `anchor_text` in commentRangeStart/End and append comment to comments.xml."""
    seen = 0
    for p in doc_root.iter(f"{W_NS}p"):
        runs = _flatten_paragraph_runs(p)
        if not runs:
            continue
        full_text = "".join(txt for _, txt in runs)
        search_from = 0
        while True:
            idx = full_text.find(anchor_text, search_from)
            if idx == -1:
                break
            seen += 1
            if seen < occurrence:
                search_from = idx + len(anchor_text)
                continue

            end_idx = idx + len(anchor_text)
            cursor = 0
            affected: list[tuple[ET.Element, int, int, str]] = []
            for r_el, r_txt in runs:
                r_start, r_end = cursor, cursor + len(r_txt)
                cursor = r_end
                if r_end <= idx or r_start >= end_idx:
                    continue
                affected.append((r_el, r_start, r_end, r_txt))

            first_run, first_start, _, first_txt = affected[0]
            last_run, last_start, _, last_txt = affected[-1]
            first_rpr = first_run.find(f"{W_NS}rPr")
            last_rpr = last_run.find(f"{W_NS}rPr")
            prefix = first_txt[: idx - first_start]
            suffix = last_txt[end_idx - last_start :]

            children = list(p)
            insert_pos = children.index(first_run)
            for r_el, _, _, _ in affected:
                p.remove(r_el)

            cid_str = str(comment_id)
            new_nodes: list[ET.Element] = []
            if prefix:
                new_nodes.append(_make_run(first_rpr, prefix))
            new_nodes.append(ET.Element(f"{W_NS}commentRangeStart", {f"{W_NS}id": cid_str}))
            new_nodes.append(_make_run(first_rpr, anchor_text))
            new_nodes.append(ET.Element(f"{W_NS}commentRangeEnd", {f"{W_NS}id": cid_str}))
            ref_run = ET.Element(f"{W_NS}r")
            ET.SubElement(ref_run, f"{W_NS}commentReference", {f"{W_NS}id": cid_str})
            new_nodes.append(ref_run)
            if suffix:
                new_nodes.append(_make_run(last_rpr, suffix))

            for offset, node in enumerate(new_nodes):
                p.insert(insert_pos + offset, node)

            c_el = ET.SubElement(
                comments_root,
                f"{W_NS}comment",
                {
                    f"{W_NS}id": cid_str,
                    f"{W_NS}author": author,
                    f"{W_NS}initials": initials,
                    f"{W_NS}date": date_iso,
                },
            )
            cp = ET.SubElement(c_el, f"{W_NS}p")
            cp.append(_make_run(None, comment_text))
            return comment_id + 1

    raise ValueError(f"Comment anchor text not found: {anchor_text!r}")


def apply_batch_edits(
    input_docx: Path, edits_spec: dict, output_docx: Path
) -> tuple[int, int]:
    """Apply JSON batch of tracked replacements and comments to input_docx -> output_docx."""
    import tempfile

    author = edits_spec.get("author", "Reviewer")
    initials = edits_spec.get("initials", "".join(w[0] for w in author.split()[:2]).upper() or "RV")
    date_iso = edits_spec.get(
        "date", datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    )

    with tempfile.TemporaryDirectory() as tmp:
        work_dir = Path(tmp) / "unpacked"
        unpack_docx(input_docx, work_dir, overwrite=True)

        doc_xml = work_dir / "word" / "document.xml"
        doc_tree = ET.parse(doc_xml)
        doc_root = doc_tree.getroot()

        comments_path: Path | None = None
        comments_root: ET.Element | None = None
        if edits_spec.get("comments"):
            comments_path, comments_root = _ensure_comments_part(work_dir)

        roots = [doc_root] + ([comments_root] if comments_root is not None else [])
        next_id = _next_w_id(roots)

        rep_count = 0
        for item in edits_spec.get("replacements", []):
            next_id = apply_tracked_replacement(
                doc_root=doc_root,
                find_text=item["find"],
                replace_text=item.get("replace", ""),
                author=item.get("author", author),
                date_iso=item.get("date", date_iso),
                next_id=next_id,
                occurrence=int(item.get("occurrence", 1)),
            )
            rep_count += 1

        com_count = 0
        for item in edits_spec.get("comments", []):
            assert comments_root is not None
            next_id = add_comment_on_text(
                doc_root=doc_root,
                comments_root=comments_root,
                anchor_text=item["anchor"],
                comment_text=item["comment"],
                author=item.get("author", author),
                initials=item.get("initials", initials),
                date_iso=item.get("date", date_iso),
                comment_id=next_id,
                occurrence=int(item.get("occurrence", 1)),
            )
            com_count += 1

        doc_xml.write_bytes(ET.tostring(doc_root, encoding="utf-8", xml_declaration=True))
        if comments_path is not None and comments_root is not None:
            comments_path.write_bytes(
                ET.tostring(comments_root, encoding="utf-8", xml_declaration=True)
            )

        pack_docx(work_dir, output_docx, skip_validate=False)
    return rep_count, com_count


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Apply batch tracked changes (w:ins/w:del) and comments to a .docx file."
    )
    parser.add_argument("input_docx", type=Path, help="Source .docx file")
    parser.add_argument("edits_json", type=Path, help="JSON spec of replacements & comments")
    parser.add_argument("-o", "--out", type=Path, required=True, help="Output redlined .docx")
    args = parser.parse_args()

    try:
        spec = json.loads(args.edits_json.read_text(encoding="utf-8"))
        reps, coms = apply_batch_edits(args.input_docx, spec, args.out)
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    print(f"Applied {reps} tracked change(s) and {coms} comment(s) -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
