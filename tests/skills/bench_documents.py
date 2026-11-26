"""How the document skills hold up at scale: time, peak memory and how much they print (what the
model then has to read) for big files.

    python3 tests/skills/bench_documents.py [--quick] [--out bench.json]

Cases: a 100-slide deck, a 200-page report, a 100,000-row workbook, a 500-page PDF. Each step runs
as the model runs it (a command) and is timed. Rendering and recalculation need LibreOffice; they
are skipped without it.
"""

import argparse
import json
import os
import resource
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SKILLS = ROOT / "assets" / "kodact-plugin" / "skills"
HAS_OFFICE = bool(shutil.which("soffice") or shutil.which("libreoffice"))


def step(results, name, skill, script, *args, cwd, budget_s=None, budget_chars=None, expect_exit=0):
    t = time.perf_counter()
    r = subprocess.run([sys.executable, str(SKILLS / skill / "scripts" / script), *map(str, args)],
                       cwd=cwd, capture_output=True, text=True, timeout=1800)
    took = time.perf_counter() - t
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    out = r.stdout + r.stderr
    row = {"step": name, "seconds": round(took, 2), "printed_chars": len(out), "exit": r.returncode,
           "peak_mb_so_far": round(usage.ru_maxrss / 1024)}
    if budget_s and took > budget_s:
        row["over_time_budget"] = budget_s
    if budget_chars and len(out) > budget_chars:
        row["over_print_budget"] = budget_chars
    if r.returncode != expect_exit:
        row["tail"] = out[-400:]
    results.append(row)
    flag = " !" if any(k.startswith("over_") for k in row) or r.returncode != expect_exit else ""
    print(f"  {name:<48} {row['seconds']:>8.2f}s {row['printed_chars']:>10,} chars  exit {r.returncode}{flag}",
          flush=True)
    return out


def deck(results, d, n):
    slides = []
    layouts = ["cards", "steps", "bullets", "kpis", "timeline", "chart", "table"]
    for i in range(n - 1):
        lay = layouts[i % len(layouts)]
        s = {"layout": lay, "title": f"Slide {i + 2}: what the numbers show"}
        if lay == "chart":
            s["chart"] = {"type": "column", "categories": ["Q1", "Q2", "Q3"], "series": {"A": [1, 2, 3]}}
        elif lay == "table":
            s.update(columns=["Item", "Cost"], rows=[["A", 1], ["B", 2]])
        elif lay == "kpis":
            s["items"] = [{"value": "12k", "label": "Users"}, {"value": "4.8", "label": "Rating"}]
        else:
            s["items"] = ["Plan: scope it", "Build: write it", "Ship: release it"]
        slides.append(s)
    (d / "slides.json").write_text(json.dumps(slides))
    step(results, f"pptx init", "pptx", "pptx_tool.py", "init", "big.pptx", "--title", "Big", cwd=d)
    step(results, f"pptx add {n - 1} slides in one call", "pptx", "pptx_tool.py", "add", "big.pptx",
         "--json", "@slides.json", cwd=d, budget_s=60)
    one = json.dumps(slides[0])
    t = time.perf_counter()
    for _ in range(10):
        subprocess.run([sys.executable, str(SKILLS / "pptx" / "scripts" / "pptx_tool.py"), "add", "big.pptx",
                        "--json", one], cwd=d, capture_output=True)
    results.append({"step": f"pptx add 1 slide to a {n}-slide deck (mean of 10)",
                    "seconds": round((time.perf_counter() - t) / 10, 2)})
    print(f"  {results[-1]['step']:<48} {results[-1]['seconds']:>8.2f}s", flush=True)
    step(results, "pptx validate", "pptx", "validate.py", "big.pptx", cwd=d, budget_s=30)
    step(results, "pptx read --shapes", "pptx", "pptx_tool.py", "read", "big.pptx", "--shapes", cwd=d,
         budget_chars=60_000)
    step(results, "pptx extract_text", "pptx", "extract_text.py", "big.pptx", cwd=d, budget_chars=60_000)
    if HAS_OFFICE:
        step(results, "pptx render_preview --grid", "pptx", "render_preview.py", "big.pptx", "--out-dir", "p",
             "--grid", "--dpi", "50", cwd=d, budget_s=300, budget_chars=20_000)


def report(results, d, pages):
    blocks = []
    for i in range(pages):
        blocks += [{"block": "heading", "text": f"Section {i + 1}", "level": 1},
                   {"block": "text", "text": "This paragraph says what the section found. " * 12},
                   {"block": "bullets", "items": ["A point", "Another point", "A third"]},
                   {"block": "table", "columns": ["Item", "Cost"], "rows": [["A", i], ["B", i + 1]]},
                   {"block": "pagebreak"}]
    (d / "blocks.json").write_text(json.dumps(blocks))
    step(results, "docx init", "docx", "docx_tool.py", "init", "big.docx", "--title", "Big", cwd=d)
    step(results, f"docx add {len(blocks)} blocks (~{pages} pages)", "docx", "docx_tool.py", "add", "big.docx",
         "--json", "@blocks.json", cwd=d, budget_s=120)
    step(results, "docx validate", "docx", "validate.py", "big.docx", cwd=d, budget_s=30)
    step(results, "docx read", "docx", "docx_tool.py", "read", "big.docx", cwd=d, budget_chars=60_000)
    step(results, "docx comment", "docx", "docx_tool.py", "comment", "big.docx", "c.docx", "--quote",
         "Section 150", "--text", "Check", cwd=d, budget_s=60)
    if HAS_OFFICE:
        step(results, "docx render_preview --grid", "docx", "render_preview.py", "big.docx", "--out-dir", "p",
             "--grid", "--dpi", "40", cwd=d, budget_s=600, budget_chars=20_000)


def workbook(results, d, rows):
    with open(d / "rows.csv", "w") as f:
        f.write("Id,Region,Units,Price\n")
        for i in range(rows):
            f.write(f"{i},R{i % 7},{i % 100},{(i % 50) + 0.5}\n")
    step(results, f"xlsx create from {rows:,}-row CSV", "xlsx", "xlsx_tool.py", "create", "big.xlsx", "--csv",
         "rows.csv", cwd=d, budget_s=120)
    step(results, "xlsx set a formula column head", "xlsx", "xlsx_tool.py", "set", "big.xlsx", "big.xlsx",
         "--cell", "Sheet1!E1=Total", "--cell", "Sheet1!E2==C2*D2", cwd=d, budget_s=120)
    step(results, "xlsx inspect_workbook", "xlsx", "inspect_workbook.py", "big.xlsx", cwd=d, budget_s=60,
         budget_chars=20_000)
    step(results, "xlsx read", "xlsx", "xlsx_tool.py", "read", "big.xlsx", cwd=d, budget_s=120, budget_chars=60_000)
    # Not recalculated yet: the audit must fail it (exit 1), not call it passed.
    step(results, "xlsx recalc_and_verify --scan-only", "xlsx", "recalc_and_verify.py", "big.xlsx", "--scan-only",
         cwd=d, budget_s=120, budget_chars=20_000, expect_exit=1)
    if HAS_OFFICE:
        step(results, "xlsx recalc_and_verify (recalculate)", "xlsx", "recalc_and_verify.py", "big.xlsx", "-o",
             "c.xlsx", cwd=d, budget_s=600, budget_chars=20_000)


def pdf(results, d, pages):
    from reportlab.lib.pagesizes import letter
    from reportlab.pdfgen import canvas

    c = canvas.Canvas(str(d / "big.pdf"), pagesize=letter)
    for i in range(pages):
        c.drawString(72, 720, f"Page {i + 1}")
        for j in range(40):
            c.drawString(72, 700 - j * 15, f"Line {j}: the text a long report has on each of its pages.")
        c.showPage()
    c.save()
    step(results, f"pdf extract_text ({pages} pages)", "pdf", "extract_text.py", "big.pdf", cwd=d, budget_s=120,
         budget_chars=60_000)
    step(results, "pdf extract_text --pages 1-5", "pdf", "extract_text.py", "big.pdf", "--pages", "1-5", cwd=d,
         budget_chars=20_000)
    step(results, "pdf merge with itself", "pdf", "merge_split.py", "merge", "big.pdf", "big.pdf", "-o", "m.pdf",
         cwd=d, budget_s=60)
    step(results, "pdf split 10 pages", "pdf", "merge_split.py", "split", "m.pdf", "--pages", "100-109", "-o",
         "s.pdf", cwd=d, budget_s=60)
    step(results, "pdf render_preview --grid", "pdf", "render_preview.py", "big.pdf", "--out-dir", "p", "--grid",
         "--dpi", "40", cwd=d, budget_s=300, budget_chars=20_000)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true", help="a tenth of each size")
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    k = 10 if args.quick else 1
    results = {}
    for name, fn, size in (("deck", deck, 100), ("report", report, 200), ("workbook", workbook, 100_000),
                           ("pdf", pdf, 500)):
        n = max(5, size // k)
        print(f"{name} ({n:,})", flush=True)
        rows = []
        with tempfile.TemporaryDirectory() as tmp:
            fn(rows, Path(tmp), n)
        results[name] = rows
    if args.out:
        args.out.write_text(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
