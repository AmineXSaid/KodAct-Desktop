"""Appends what KodAct's setup installs to the setup's .exe (installer/setup/payload.rs).

    python3 -I installer/pack.py <setup.exe> <stage-dir> <version> <out.exe>

<stage-dir> is the install folder as it is laid out (KodAct.exe, kodact.ico, bin/, python/,
design-library/, libreoffice/, fonts/). It is zipped with LZMA, with setup.json (the version
and the bytes each part unpacks to), and the zip goes after the .exe, then its length (8 bytes,
little-endian) and KODACTPK. The .exe stays a valid program: Windows reads only its head.
"""
import json
import os
import shutil
import struct
import sys
import zipfile
from pathlib import Path

MAGIC = b"KODACTPK"


def part_of(name: str) -> str:
    # Matches payload::part_of.
    top = name.split("/", 1)[0]
    return {"bin": "cli", "python": "python", "design-library": "design", "libreoffice": "libreoffice"}.get(top, "app")


def files(stage: Path):
    # The app first, then each part in the order setup unpacks them (payload::PARTS).
    order = ["app", "cli", "python", "design", "libreoffice"]
    found = []
    for root, dirs, names in os.walk(stage):
        dirs.sort()
        for n in sorted(names):
            p = Path(root) / n
            rel = p.relative_to(stage).as_posix()
            found.append((order.index(part_of(rel)), rel, p))
    found.sort()
    return [(rel, p) for _, rel, p in found]


def main():
    setup, stage, version, out = sys.argv[1], Path(sys.argv[2]), sys.argv[3], Path(sys.argv[4])
    listed = files(stage)
    sizes = {}
    for rel, p in listed:
        sizes[part_of(rel)] = sizes.get(part_of(rel), 0) + p.stat().st_size
    tmp = out.with_suffix(".payload.zip")
    with zipfile.ZipFile(tmp, "w", compression=zipfile.ZIP_LZMA) as z:
        z.writestr("setup.json", json.dumps({"version": version, "sizes": sizes}, indent=1))
        for i, (rel, p) in enumerate(listed):
            z.write(p, rel)
            if i % 500 == 0:
                print(f"  {i}/{len(listed)} {rel}", flush=True)
    length = tmp.stat().st_size
    with open(out, "wb") as o:
        with open(setup, "rb") as s:
            shutil.copyfileobj(s, o)
        with open(tmp, "rb") as z:
            shutil.copyfileobj(z, o, 1 << 20)
        o.write(struct.pack("<Q", length))
        o.write(MAGIC)
    tmp.unlink()
    total = sum(sizes.values())
    print(f"{out}: {len(listed)} files, {total / 1e9:.2f} GB unpacked, {out.stat().st_size / 1e6:.0f} MB")
    for k, v in sizes.items():
        print(f"  {k}: {v / 1e6:.0f} MB")


if __name__ == "__main__":
    main()
