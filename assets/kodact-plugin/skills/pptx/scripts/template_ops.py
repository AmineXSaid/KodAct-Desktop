#!/usr/bin/env python3
"""Template surgery for .pptx: rearrange/clone/delete slides, JSON inventory, style-preserving replace, and OOXML unpack/pack."""

from __future__ import annotations

import argparse
from copy import deepcopy
import json
from pathlib import Path
import shutil
import sys
from xml.dom import minidom
import zipfile

EMU_PER_INCH = 914400
REL_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
SLIDE_LAYOUT_REL = f"{REL_NS}/slideLayout"
NOTES_SLIDE_REL = f"{REL_NS}/notesSlide"


def _emu_to_in(val: int | None) -> float:
    return round((val or 0) / EMU_PER_INCH, 2)


def _remap_rids(elem, rid_map: dict[str, str]) -> None:
    if not rid_map:
        return
    for node in elem.iter():
        for attr_key, attr_val in list(node.attrib.items()):
            if attr_val in rid_map and (
                attr_key.endswith("}embed")
                or attr_key.endswith("}link")
                or attr_key.endswith("}id")
            ):
                node.set(attr_key, rid_map[attr_val])


def _clone_slide(prs, source_slide):
    """Create a duplicate of source_slide at the end of prs, remapping media/chart rIds."""
    new_slide = prs.slides.add_slide(source_slide.slide_layout)
    # Remove default auto-created placeholder shapes from new_slide's spTree
    sp_tree = new_slide.shapes._spTree
    for child in list(sp_tree):
        tag_local = child.tag.rsplit("}", 1)[-1]
        if tag_local not in ("nvGrpSpPr", "grpSpPr"):
            sp_tree.remove(child)

    rid_map: dict[str, str] = {}
    for rel in source_slide.part.rels.values():
        if rel.reltype in (SLIDE_LAYOUT_REL, NOTES_SLIDE_REL):
            continue
        if rel.is_external:
            new_rid = new_slide.part.rels.get_or_add_ext_rel(rel.reltype, rel.target_ref)
        else:
            new_rid = new_slide.part.relate_to(rel.target_part, rel.reltype)
        rid_map[rel.rId] = new_rid

    for child in list(source_slide.shapes._spTree):
        tag_local = child.tag.rsplit("}", 1)[-1]
        if tag_local in ("nvGrpSpPr", "grpSpPr"):
            continue
        cloned = deepcopy(child)
        _remap_rids(cloned, rid_map)
        sp_tree.append(cloned)

    if source_slide.has_notes_slide and source_slide.notes_slide.notes_text_frame:
        txt = source_slide.notes_slide.notes_text_frame.text
        if txt:
            new_slide.notes_slide.notes_text_frame.text = txt

    return new_slide


def rearrange_deck(input_pptx: Path, output_pptx: Path, slide_indices: list[int]) -> int:
    """Build output_pptx from 0-based slide_indices (supporting reordering, duplication, deletion)."""
    from pptx import Presentation

    prs = Presentation(str(input_pptx))
    orig_count = len(prs.slides)
    if not slide_indices:
        raise ValueError("slide_indices cannot be empty")
    for idx in slide_indices:
        if not (0 <= idx < orig_count):
            raise IndexError(
                f"0-based slide index {idx} out of range (deck has {orig_count} slides: 0..{orig_count - 1})"
            )

    orig_slides = list(prs.slides)
    sld_id_lst = prs.slides._sldIdLst
    orig_sld_ids = list(sld_id_lst)

    used_once: set[int] = set()
    chosen_sld_ids = []

    for idx in slide_indices:
        if idx not in used_once:
            used_once.add(idx)
            chosen_sld_ids.append(orig_sld_ids[idx])
        else:
            _clone_slide(prs, orig_slides[idx])
            chosen_sld_ids.append(sld_id_lst[-1])

    # Drop unused original slides cleanly from package relationships
    for idx, sld_el in enumerate(orig_sld_ids):
        if idx not in used_once:
            rid = sld_el.attrib.get(f"{{{REL_NS}}}id") or sld_el.rId
            prs.part.drop_rel(rid)

    # Rebuild sldIdLst in exact target order
    for el in list(sld_id_lst):
        sld_id_lst.remove(el)
    for el in chosen_sld_ids:
        sld_id_lst.append(el)

    output_pptx.parent.mkdir(parents=True, exist_ok=True)
    prs.save(str(output_pptx))
    return len(chosen_sld_ids)


def export_inventory(input_pptx: Path) -> dict:
    """Export structured JSON inventory of all text shapes and tables per slide."""
    from pptx import Presentation

    prs = Presentation(str(input_pptx))
    slides_out = []

    for s_idx, slide in enumerate(prs.slides):
        ordered = sorted(slide.shapes, key=lambda s: (s.top or 0, s.left or 0))
        shapes_list = []
        for sh in ordered:
            ph_idx = sh.placeholder_format.idx if sh.is_placeholder else None
            entry: dict = {
                "shape_id": sh.shape_id,
                "name": sh.name,
                "placeholder_idx": ph_idx,
                "bounds_in": {
                    "left": _emu_to_in(sh.left),
                    "top": _emu_to_in(sh.top),
                    "width": _emu_to_in(sh.width),
                    "height": _emu_to_in(sh.height),
                },
            }
            if sh.has_text_frame:
                entry["paragraphs"] = [
                    {"text": p.text, "level": p.level}
                    for p in sh.text_frame.paragraphs
                ]
                shapes_list.append(entry)
            elif sh.has_table:
                entry["table"] = [
                    [c.text for c in row.cells] for row in sh.table.rows
                ]
                shapes_list.append(entry)

        slides_out.append(
            {
                "slide_number": s_idx + 1,
                "slide_index_0": s_idx,
                "layout": slide.slide_layout.name,
                "shapes": shapes_list,
            }
        )

    return {
        "file": str(input_pptx),
        "slide_count": len(slides_out),
        "slides": slides_out,
    }


def _set_shape_paragraphs_preserving_style(shape, new_paragraphs: list) -> None:
    """Replace text_frame paragraphs while cloning the template's first paragraph/run styling."""
    tf = shape.text_frame
    tf.word_wrap = True

    template_p_pr = None
    template_r_pr = None
    if tf.paragraphs:
        p0 = tf.paragraphs[0]
        p_pr = p0._p.find("{http://schemas.openxmlformats.org/drawingml/2006/main}pPr")
        if p_pr is not None:
            template_p_pr = deepcopy(p_pr)
        if p0.runs:
            r_pr = p0.runs[0]._r.find(
                "{http://schemas.openxmlformats.org/drawingml/2006/main}rPr"
            )
            if r_pr is not None:
                template_r_pr = deepcopy(r_pr)

    tf.clear()
    for idx, item in enumerate(new_paragraphs):
        p = tf.paragraphs[0] if idx == 0 else tf.add_paragraph()
        if isinstance(item, dict):
            txt = str(item.get("text", ""))
            lvl = int(item.get("level", 0))
            bold_override = item.get("bold")
        else:
            txt = str(item)
            lvl = 0
            bold_override = None

        if template_p_pr is not None and idx > 0:
            existing = p._p.find("{http://schemas.openxmlformats.org/drawingml/2006/main}pPr")
            if existing is not None:
                p._p.remove(existing)
            p._p.insert(0, deepcopy(template_p_pr))

        p.level = lvl
        run = p.add_run()
        if template_r_pr is not None:
            existing_rpr = run._r.find(
                "{http://schemas.openxmlformats.org/drawingml/2006/main}rPr"
            )
            if existing_rpr is not None:
                run._r.remove(existing_rpr)
            run._r.insert(0, deepcopy(template_r_pr))
        run.text = txt
        if bold_override is not None:
            run.font.bold = bool(bold_override)


def apply_replacements(input_pptx: Path, spec: dict, output_pptx: Path) -> int:
    """Apply shape text/table updates and shape deletions to input_pptx -> output_pptx."""
    from pptx import Presentation

    prs = Presentation(str(input_pptx))
    updated = 0

    for slide_spec in spec.get("slides", []):
        if "slide_index_0" in slide_spec:
            s_idx = int(slide_spec["slide_index_0"])
        else:
            s_idx = int(slide_spec["slide_number"]) - 1
        slide = prs.slides[s_idx]

        # Optional removal of unused shapes/placeholders
        clear_targets = set(slide_spec.get("remove_shapes", []))
        for sh in list(slide.shapes):
            if sh.name in clear_targets or sh.shape_id in clear_targets:
                slide.shapes._spTree.remove(sh._element)

        for upd in slide_spec.get("updates", []):
            target = None
            for sh in slide.shapes:
                if "shape_id" in upd and sh.shape_id == int(upd["shape_id"]):
                    target = sh
                    break
                if "placeholder_idx" in upd and sh.is_placeholder and sh.placeholder_format.idx == int(upd["placeholder_idx"]):
                    target = sh
                    break
                if "name" in upd and sh.name == upd["name"]:
                    target = sh
                    break
            if target is None:
                raise KeyError(f"Slide {s_idx + 1}: target shape not found for {upd}")

            if "paragraphs" in upd and target.has_text_frame:
                _set_shape_paragraphs_preserving_style(target, upd["paragraphs"])
                updated += 1
            elif "table" in upd and target.has_table:
                for r_i, row_vals in enumerate(upd["table"]):
                    for c_i, val in enumerate(row_vals):
                        if r_i < len(target.table.rows) and c_i < len(target.table.columns):
                            cell = target.table.cell(r_i, c_i)
                            if cell.text_frame.paragraphs:
                                _set_shape_paragraphs_preserving_style(cell, [str(val)])
                            else:
                                cell.text = str(val)
                updated += 1

        if "notes" in slide_spec:
            slide.notes_slide.notes_text_frame.text = str(slide_spec["notes"])

    output_pptx.parent.mkdir(parents=True, exist_ok=True)
    prs.save(str(output_pptx))
    return updated


def unpack_pptx(pptx_path: Path, out_dir: Path, force: bool = False) -> int:
    if out_dir.exists():
        if not force:
            raise FileExistsError(f"{out_dir} exists (pass --force to overwrite)")
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    resolved = out_dir.resolve()
    count = 0
    with zipfile.ZipFile(pptx_path, "r") as zf:
        for info in zf.infolist():
            if info.is_dir():
                continue
            dest = (resolved / info.filename).resolve()
            if not dest.is_relative_to(resolved):
                raise ValueError(f"Unsafe zip path: {info.filename}")
            dest.parent.mkdir(parents=True, exist_ok=True)
            raw = zf.read(info.filename)
            if dest.name.lower().endswith((".xml", ".rels")):
                try:
                    raw = minidom.parseString(raw).toprettyxml(indent="  ", encoding="utf-8")
                except Exception:
                    pass
            dest.write_bytes(raw)
            count += 1
    return count


def pack_pptx(in_dir: Path, out_pptx: Path) -> int:
    out_pptx.parent.mkdir(parents=True, exist_ok=True)
    count = 0
    with zipfile.ZipFile(out_pptx, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for p in sorted(in_dir.rglob("*")):
            if p.is_file():
                zf.write(p, p.relative_to(in_dir).as_posix())
                count += 1
    return count


def main() -> int:
    parser = argparse.ArgumentParser(description="PPTX template surgery & OOXML operations.")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_re = sub.add_parser("rearrange", help="Clone, reorder, or delete slides by 0-based index")
    p_re.add_argument("input_pptx", type=Path)
    p_re.add_argument("output_pptx", type=Path)
    p_re.add_argument(
        "--slides",
        required=True,
        help="Comma-separated 0-based slide indices, e.g. '0,3,3,7'",
    )

    p_inv = sub.add_parser("inventory", help="Export slide shape/text inventory to JSON")
    p_inv.add_argument("input_pptx", type=Path)
    p_inv.add_argument("-o", "--out", type=Path, required=True)

    p_rep = sub.add_parser("replace", help="Apply style-preserving text/table JSON updates")
    p_rep.add_argument("input_pptx", type=Path)
    p_rep.add_argument("spec_json", type=Path)
    p_rep.add_argument("-o", "--out", type=Path, required=True)

    p_unp = sub.add_parser("unpack", help="Unpack .pptx OOXML for raw theme/slide XML edits")
    p_unp.add_argument("input_pptx", type=Path)
    p_unp.add_argument("output_dir", type=Path)
    p_unp.add_argument("--force", action="store_true")

    p_pk = sub.add_parser("pack", help="Repack directory into .pptx")
    p_pk.add_argument("input_dir", type=Path)
    p_pk.add_argument("output_pptx", type=Path)

    args = parser.parse_args()

    try:
        if args.cmd == "rearrange":
            indices = [int(x.strip()) for x in args.slides.split(",") if x.strip()]
            n = rearrange_deck(args.input_pptx, args.output_pptx, indices)
            print(f"Rearranged {n} slide(s) -> {args.output_pptx}")
        elif args.cmd == "inventory":
            inv = export_inventory(args.input_pptx)
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(json.dumps(inv, indent=2, ensure_ascii=False), encoding="utf-8")
            print(f"Exported inventory ({inv['slide_count']} slides) -> {args.out}")
        elif args.cmd == "replace":
            spec = json.loads(args.spec_json.read_text(encoding="utf-8"))
            n = apply_replacements(args.input_pptx, spec, args.out)
            print(f"Updated {n} shape(s) -> {args.out}")
        elif args.cmd == "unpack":
            n = unpack_pptx(args.input_pptx, args.output_dir, force=args.force)
            print(f"Unpacked {n} files -> {args.output_dir}")
        elif args.cmd == "pack":
            n = pack_pptx(args.input_dir, args.output_pptx)
            print(f"Packed {n} files -> {args.output_pptx}")
    except ImportError:
        print("Error: 'python-pptx' is required (pip install python-pptx).", file=sys.stderr)
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
