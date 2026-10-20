"""Forge's design library (design-library/): Open Design's content, kept in
Open Design's own layout, plus the upstream skills its catalogue points at.

    python3 -I scripts/sync_open_design.py sync [--from DIR]
        Rebuild design-library/ from the commits pinned in
        design-library/sync.toml. --from uses a local clone of Open Design
        (it must be at the pinned commit) instead of fetching one. Writes
        UPSTREAM.json, media.lock.json and THIRD_PARTY_LICENSES.md.
    python3 -I scripts/sync_open_design.py media
        Fetch the images, video and audio media.lock.json pins by SHA-256
        (they are not committed). A fresh checkout runs this once; the
        installer build runs it too. Files are cached by hash in
        ~/.cache/forge-design/media, so a second run needs no network.
    python3 -I scripts/sync_open_design.py check
        Verify the tree: every locked media file present, with its hash.

Text, fonts and code bundles are committed; git stores identical files once,
so the duplicated trees upstream (design-templates and
plugins/_official/examples) cost nothing extra.
"""

import concurrent.futures
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import tomllib
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB = ROOT / "design-library"
CACHE = Path(os.environ.get("FORGE_DESIGN_CACHE", Path.home() / ".cache" / "forge-design"))

# Files in design-library/ that are Forge's own, never touched by a sync.
OWN = {"sync.toml", "UPSTREAM.json", "media.lock.json", "THIRD_PARTY_LICENSES.md",
       "NOTICE", "README.md", ".gitignore", ".gitattributes"}
OWN_DIRS = {"forge"}
UPSTREAM_SKILLS = "upstream"


def config():
    with open(LIB / "sync.toml", "rb") as f:
        return tomllib.load(f)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def git(*args, cwd=None):
    return subprocess.run(["git", *args], cwd=cwd, check=True,
                          capture_output=True, text=True).stdout.strip()


def owner_repo(url):
    parts = urllib.parse.urlparse(url).path.strip("/").removesuffix(".git").split("/")
    return parts[0], parts[1]


def checkout(url, commit, local=None):
    """A tree of `url` at `commit`: `local` when it is there, else a cached
    shallow fetch of just that commit."""
    if local:
        head = git("rev-parse", "HEAD", cwd=local)
        if head != commit:
            sys.exit(f"{local} is at {head}, not the pinned {commit}")
        return Path(local)
    owner, repo = owner_repo(url)
    dest = CACHE / "trees" / f"{owner}__{repo}@{commit[:12]}"
    if (dest / ".forge-complete").exists():
        return dest
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    git("init", "-q", cwd=dest)
    git("remote", "add", "origin", url, cwd=dest)
    for attempt in range(4):
        try:
            git("fetch", "-q", "--depth", "1", "origin", commit, cwd=dest)
            break
        except subprocess.CalledProcessError as e:
            if attempt == 3:
                sys.exit(f"could not fetch {url}@{commit}: {e.stderr}")
            time.sleep(2 ** (attempt + 1))
    git("checkout", "-q", "FETCH_HEAD", cwd=dest)
    (dest / ".forge-complete").write_text(commit)
    return dest


def walk(tree, root):
    """Every file under `root` (a file or a folder) in `tree`, as paths
    relative to `tree`, in a stable order."""
    base = tree / root
    if base.is_file():
        return [Path(root)]
    if not base.is_dir():
        sys.exit(f"{root} is not in {tree}")
    out = []
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d != ".git")
        for name in sorted(filenames):
            out.append((Path(dirpath) / name).relative_to(tree))
    return out


def is_media(path, exts):
    return path.suffix.lower().lstrip(".") in exts


def raw_url(url, commit, path):
    owner, repo = owner_repo(url)
    quoted = urllib.parse.quote(path.as_posix())
    return f"https://raw.githubusercontent.com/{owner}/{repo}/{commit}/{quoted}"


def place(src, dest):
    """Copy `src` to `dest` unless it already holds the same bytes."""
    if dest.exists() and dest.stat().st_size == src.stat().st_size \
            and dest.read_bytes() == src.read_bytes():
        return False
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, dest)
    return True


def mirror(tree, roots, prefix, exts, url, commit, wanted, media):
    for root in roots:
        for rel in walk(tree, root):
            dest_rel = prefix / rel
            wanted.add(dest_rel.as_posix())
            src = tree / rel
            place(src, LIB / dest_rel)
            if is_media(rel, exts):
                media.append({
                    "path": dest_rel.as_posix(),
                    "sha256": sha256(src),
                    "size": src.stat().st_size,
                    "url": raw_url(url, commit, rel),
                })


def managed(rel):
    first = rel.split("/", 1)[0]
    return rel not in OWN and first not in OWN_DIRS


def counts(wanted, media):
    names = set(wanted)

    def entries(prefix, marker):
        return sorted({p[len(prefix):].split("/", 1)[0] for p in names
                       if p.startswith(prefix) and p.endswith("/" + marker)
                       and p[len(prefix):].count("/") == 1})

    return {
        "files": len(names),
        "media_files": len(media),
        "text_files": len(names) - len(media),
        "skills": len(entries("skills/", "SKILL.md")),
        # Open Design lists these by link only ("install the upstream bundle").
        "catalogue_skills": len([n for n in entries("skills/", "SKILL.md")
                                 if "install the upstream bundle" in " ".join(
                                     (LIB / "skills" / n / "SKILL.md").read_text(errors="replace").split())]),
        "design_systems": len(entries("design-systems/", "DESIGN.md")),
        "design_templates": len(entries("design-templates/", "SKILL.md")),
        "craft": len([p for p in names if p.startswith("craft/") and p.count("/") == 1
                      and p.endswith(".md") and p != "craft/README.md"
                      and p != "craft/FUTURE_SECTIONS.md"]),
        "official_atoms": len(entries("plugins/_official/atoms/", "open-design.json")),
        "official_scenarios": len(entries("plugins/_official/scenarios/", "open-design.json")),
        "official_examples": len(entries("plugins/_official/examples/", "open-design.json")),
        "official_design_systems": len(entries("plugins/_official/design-systems/", "open-design.json")),
        "official_image_templates": len(entries("plugins/_official/image-templates/", "open-design.json")),
        "official_video_templates": len(entries("plugins/_official/video-templates/", "open-design.json")),
    }


def licences(cfg, wanted):
    """THIRD_PARTY_LICENSES.md: Open Design's licence, every licence file in
    the mirrored trees, the upstream skills, and the notes sync.toml adds."""
    od = cfg["open_design"]
    lines = [
        "# Third-party licences in Forge's design library",
        "",
        "Generated by `scripts/sync_open_design.py sync` from `sync.toml`. Do not edit by hand.",
        "",
        "## Open Design",
        "",
        f"Most of this folder is [Open Design]({od['repo']}) at commit `{od['commit']}` "
        f"({od['date']}), licensed under the Apache License 2.0 (`LICENSE`). "
        "It is mirrored unchanged, in its own layout. Forge's own files and its "
        "changed copies of Open Design files are under `forge/`; each changed copy "
        "says what was changed.",
        "",
        "## Folders with their own licence",
        "",
        "These folders carry a licence of their own, which applies to the files in them.",
        "",
        "| Folder | Licence file | Licence |",
        "|:--|:--|:--|",
    ]
    files = sorted(p for p in wanted if Path(p).name.upper().startswith(("LICENSE", "LICENCE", "COPYING"))
                   and p != "LICENSE")
    for p in files:
        text = (LIB / p).read_text(errors="replace")
        lines.append(f"| `{Path(p).parent.as_posix()}/` | `{Path(p).name}` | {identify(text)} |")
    skills = cfg.get("skills", [])
    if skills:
        lines += ["", "## Upstream skills", "",
                  "Open Design's catalogue lists these skills by link only. Forge ships the "
                  "real skill, from the repository and commit below, where its licence allows "
                  "redistribution. They are under `upstream/<owner>__<repo>/`.", "",
                  "| Catalogue entry | Repository | Commit | Path | Licence |",
                  "|:--|:--|:--|:--|:--|"]
        for s in skills:
            owner, repo = owner_repo(s["repo"])
            lines.append(f"| `{s['stub']}` | [{owner}/{repo}]({s['repo']}) | `{s['commit'][:12]}` "
                         f"| `{s['path']}` | {s['license']} |")
    held = cfg.get("not_shipped", [])
    if held:
        lines += ["", "## Catalogue entries kept as links only", "",
                  "Their upstream licence does not allow redistribution, so Forge ships only "
                  "Open Design's catalogue entry, which links to the upstream skill.", "",
                  "| Catalogue entry | Repository | Why |", "|:--|:--|:--|"]
        for s in held:
            lines.append(f"| `{s['stub']}` | {s['repo']} | {s['why']} |")
    for note in cfg.get("notice", []):
        lines += ["", f"## {note['title']}", "", note["text"].strip()]
    return "\n".join(lines) + "\n"


def identify(text):
    head = " ".join(text.split()[:60]).lower()
    for needle, name in [("apache license", "Apache-2.0"), ("mit license", "MIT"),
                         ("permission is hereby granted, free of charge", "MIT"),
                         ("sil open font license", "OFL-1.1"), ("bsd", "BSD"),
                         ("isc license", "ISC"), ("creative commons", "Creative Commons"),
                         ("mozilla public license", "MPL-2.0"), ("unlicense", "Unlicense"),
                         ("gnu general public", "GPL"), ("gnu lesser", "LGPL"),
                         ("gnu affero", "AGPL")]:
        if needle in head:
            return name
    return "see file"


def sync(args):
    cfg = config()
    od = cfg["open_design"]
    exts = {e.lower() for e in cfg["media"]["extensions"]}
    local = args[args.index("--from") + 1] if "--from" in args else None
    tree = checkout(od["repo"], od["commit"], local)
    wanted, media = set(), []
    mirror(tree, od["roots"], Path("."), exts, od["repo"], od["commit"], wanted, media)
    for s in cfg.get("skills", []):
        owner, repo = owner_repo(s["repo"])
        up = checkout(s["repo"], s["commit"])
        prefix = Path(UPSTREAM_SKILLS) / f"{owner}__{repo}"
        roots = [s["path"]] + [p for p in s.get("also", [])]
        mirror(up, roots, prefix, exts, s["repo"], s["commit"], wanted, media)
    wanted = {Path(p).as_posix().removeprefix("./") for p in wanted}
    for m in media:
        m["path"] = m["path"].removeprefix("./")
    # Files a previous sync placed that upstream no longer has.
    removed = 0
    for dirpath, dirnames, filenames in os.walk(LIB):
        for name in filenames:
            rel = (Path(dirpath) / name).relative_to(LIB).as_posix()
            if managed(rel) and rel not in wanted:
                (Path(dirpath) / name).unlink()
                removed += 1
    for dirpath, dirnames, filenames in sorted(os.walk(LIB), reverse=True):
        if dirpath != str(LIB) and not os.listdir(dirpath):
            os.rmdir(dirpath)
    media.sort(key=lambda m: m["path"])
    (LIB / "media.lock.json").write_text(json.dumps(
        {"note": "Images, video and audio, pinned by SHA-256; fetched by "
                 "`python3 -I scripts/sync_open_design.py media`.",
         "files": media}, indent=1, ensure_ascii=False) + "\n")
    upstream = {
        "open_design": {k: od[k] for k in ("repo", "commit", "date", "license")},
        "roots": od["roots"],
        "counts": counts(wanted, media),
        "skills": [{k: s[k] for k in ("stub", "repo", "commit", "path", "license")}
                   for s in cfg.get("skills", [])],
        "not_shipped": cfg.get("not_shipped", []),
    }
    (LIB / "UPSTREAM.json").write_text(json.dumps(upstream, indent=1, ensure_ascii=False) + "\n")
    (LIB / "THIRD_PARTY_LICENSES.md").write_text(licences(cfg, wanted))
    c = upstream["counts"]
    print(f"{c['files']} files ({c['text_files']} committed, {c['media_files']} media), "
          f"{removed} removed")


def fetch(entry):
    dest = LIB / entry["path"]
    if dest.exists() and dest.stat().st_size == entry["size"] and sha256(dest) == entry["sha256"]:
        return "ok"
    cached = CACHE / "media" / entry["sha256"]
    if not cached.exists():
        tmp = cached.with_suffix(".part")
        tmp.parent.mkdir(parents=True, exist_ok=True)
        for attempt in range(4):
            try:
                with urllib.request.urlopen(entry["url"], timeout=60) as r, open(tmp, "wb") as f:
                    shutil.copyfileobj(r, f)
                break
            except OSError as e:
                if attempt == 3:
                    return f"failed: {e}"
                time.sleep(2 ** (attempt + 1))
        if sha256(tmp) != entry["sha256"]:
            tmp.unlink()
            return "failed: hash mismatch"
        tmp.replace(cached)
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(cached, dest)
    return "fetched"


def media(_args):
    entries = json.loads((LIB / "media.lock.json").read_text())["files"]
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        results = list(pool.map(fetch, entries))
    failed = [(e["path"], r) for e, r in zip(entries, results) if r.startswith("failed")]
    print(f"{len(entries)} media files: {results.count('fetched')} fetched, "
          f"{results.count('ok')} already there, {len(failed)} failed")
    for path, why in failed:
        print(f"  {path}: {why}")
    sys.exit(1 if failed else 0)


def check(_args):
    entries = json.loads((LIB / "media.lock.json").read_text())["files"]
    bad = []
    for e in entries:
        p = LIB / e["path"]
        if not p.exists():
            bad.append(f"missing {e['path']}")
        elif p.stat().st_size != e["size"] or sha256(p) != e["sha256"]:
            bad.append(f"changed {e['path']}")
    print(f"{len(entries)} media files, {len(bad)} problems")
    for b in bad[:50]:
        print("  " + b)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    commands = {"sync": sync, "media": media, "check": check}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]](sys.argv[2:])
