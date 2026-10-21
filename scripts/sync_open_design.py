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
    python3 -I scripts/sync_open_design.py upstream
        For each of Open Design's catalogue entries (a SKILL.md that only
        links to another repository), find the real skill upstream and the
        licence of its folder, and write the result into sync.toml: skills
        whose licence lets Forge ship them under [[skills]], the rest under
        [[not_shipped]] with the reason. Review the diff, then run `sync`.

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


def checkout(url, commit, local=None, paths=None):
    """A tree of `url` at `commit`: `local` when it is there, else a cached
    shallow fetch of just that commit. With `paths`, only those folders and
    files are checked out (and only their contents fetched), so a large
    repository holding one skill costs that skill."""
    if local:
        head = git("rev-parse", "HEAD", cwd=local)
        if head != commit:
            sys.exit(f"{local} is at {head}, not the pinned {commit}")
        return Path(local)
    owner, repo = owner_repo(url)
    key = commit[:12]
    if paths is not None:
        key += "-" + hashlib.sha256("\n".join(sorted(paths)).encode()).hexdigest()[:8]
    dest = CACHE / "trees" / f"{owner}__{repo}@{key}"
    if (dest / ".forge-complete").exists():
        return dest
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    git("init", "-q", cwd=dest)
    git("remote", "add", "origin", url, cwd=dest)
    partial = ["--filter=blob:none"] if paths is not None else []
    for attempt in range(4):
        try:
            git("fetch", "-q", "--depth", "1", *partial, "origin", commit, cwd=dest)
            break
        except subprocess.CalledProcessError as e:
            if attempt == 3:
                sys.exit(f"could not fetch {url}@{commit}: {e.stderr}")
            time.sleep(2 ** (attempt + 1))
    if paths is not None:
        patterns = ["/*"] if "" in paths else [f"/{p.strip('/')}/" for p in paths] + [f"/{p.strip('/')}" for p in paths]
        git("sparse-checkout", "set", "--no-cone", *patterns, cwd=dest)
    git("checkout", "-q", "FETCH_HEAD", cwd=dest)
    (dest / ".forge-complete").write_text(commit)
    return dest


def walk(tree, root):
    """Every file under `root` (a file or a folder) in `tree`, as paths
    relative to `tree`, in a stable order."""
    base = tree / root if root else tree
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
    # CC-BY templates: the licence asks for credit to their authors.
    credits = []
    for p in sorted(x for x in wanted if x.endswith("/open-design.json")):
        try:
            m = json.loads((LIB / p).read_text())
        except (OSError, ValueError):
            continue
        if str(m.get("license", "")).startswith("CC-BY"):
            a = m.get("author") or {}
            who = a.get("name", "unknown")
            if a.get("url"):
                who = f"[{who}]({a['url']})"
            credits.append(f"| `{Path(p).parent.as_posix()}/` | {m.get('title', '')} | {who} | {m['license']} |")
    if credits:
        lines += ["", "## Templates that ask for credit", "",
                  "These image and video templates are licensed under Creative Commons "
                  "Attribution: credit goes to their authors, as below.", "",
                  "| Folder | Template | Author | Licence |", "|:--|:--|:--|:--|"] + credits
    for note in cfg.get("notice", []):
        lines += ["", f"## {note['title']}", "", note["text"].strip()]
    return "\n".join(lines) + "\n"


# Licences that let Forge ship a skill inside a closed-source app (CLAUDE.md
# rule 1). CC-BY asks for credit, which THIRD_PARTY_LICENSES.md gives.
PERMISSIVE = {"MIT", "Apache-2.0", "BSD", "ISC", "0BSD", "Unlicense", "CC0-1.0", "CC-BY-4.0"}


def identify(text):
    """A licence's name from its text (or from a front-matter `license:`
    value such as `MIT` or `Apache-2.0`)."""
    flat = " ".join(text.split()).lower()
    spdx = {"mit": "MIT", "apache-2.0": "Apache-2.0", "apache 2.0": "Apache-2.0", "isc": "ISC",
            "bsd-2-clause": "BSD", "bsd-3-clause": "BSD", "0bsd": "0BSD", "unlicense": "Unlicense",
            "cc0-1.0": "CC0-1.0", "cc-by-4.0": "CC-BY-4.0"}
    if flat in spdx:
        return spdx[flat]
    if "creative commons" in flat[:3000] or "cc-by" in flat[:200] or "cc0" in flat[:200]:
        if any(w in flat for w in ("noncommercial", "non-commercial", "noderivatives", "no derivatives", "sharealike", "share-alike")):
            return "Creative Commons (restricted)"
        if "cc0" in flat[:400] or "public domain dedication" in flat:
            return "CC0-1.0"
        if "attribution 4.0" in flat or "cc-by-4.0" in flat[:200]:
            return "CC-BY-4.0"
        return "Creative Commons"
    if "all rights reserved" in flat[:600] and "permission is hereby granted" not in flat:
        return "proprietary"
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
    # One sparse checkout per upstream repository, of just its skills' folders.
    by_repo = {}
    for s in cfg.get("skills", []):
        by_repo.setdefault((s["repo"], s["commit"]), []).append(s)
    for (url, commit), skills in by_repo.items():
        owner, repo = owner_repo(url)
        roots = sorted({r for s in skills for r in [s["path"], *s.get("also", [])]})
        up = checkout(url, commit, paths=roots)
        prefix = Path(UPSTREAM_SKILLS) / f"{owner}__{repo}"
        mirror(up, roots, prefix, exts, url, commit, wanted, media)
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


MARK = "# --- Upstream skills: written by `python3 -I scripts/sync_open_design.py upstream`. ---"


def catalogue():
    """Open Design's catalogue entries: (folder, upstream URL)."""
    out = []
    for skill in sorted((LIB / "skills").glob("*/SKILL.md")):
        text = skill.read_text(errors="replace")
        if "install the upstream bundle" not in " ".join(text.split()):
            continue
        for line in text.splitlines():
            line = line.strip()
            if line.startswith("upstream:"):
                out.append((skill.parent.name, line.split(":", 1)[1].strip().strip('"\'')))
                break
    return out


def frontmatter_value(text, key):
    """A top-level `key: value` of a SKILL.md's front-matter (plain, no YAML)."""
    if not text.startswith("---"):
        return None
    for line in text.split("\n---", 1)[0].splitlines()[1:]:
        if line.startswith(key + ":"):
            return line.split(":", 1)[1].strip().strip('"\'') or None
    return None


def find_skill(stub, url, dirs, show):
    """The folder of `stub` in an upstream repository whose SKILL.md folders
    are `dirs`: the path in the catalogue link, a folder of the same name,
    a SKILL.md whose `name` is the stub, or the only skill there is."""
    hint = url.split("/tree/", 1)[1].split("/", 1)[1].strip("/") if "/tree/" in url else None
    if hint is not None and hint in dirs:
        return hint
    named = [d for d in dirs if d.rsplit("/", 1)[-1] == stub]
    if len(named) == 1:
        return named[0]
    for d in dirs:
        if frontmatter_value(show(f"{d}/SKILL.md" if d else "SKILL.md") or "", "name") == stub:
            return d
    if len(dirs) == 1 and hint is None:
        return dirs[0]
    return None


def folder_licence(skill_dir, files, show):
    """The licence that applies to a skill's folder: a licence file in it,
    else its SKILL.md `license:` (pointing at a file or naming the licence),
    else the nearest licence file above it. Returns (name, file or None)."""
    def licence_files(d):
        prefix = f"{d}/" if d else ""
        return sorted(f for f in files if f.startswith(prefix) and "/" not in f[len(prefix):]
                      and f[len(prefix):].upper().startswith(("LICENSE", "LICENCE", "COPYING")))
    own = licence_files(skill_dir)
    if own:
        return identify(show(own[0]) or ""), own[0]
    declared = frontmatter_value(show(f"{skill_dir}/SKILL.md" if skill_dir else "SKILL.md") or "", "license")
    if declared and identify(declared) in PERMISSIVE:
        return identify(declared), None
    d = skill_dir
    while d:
        d = d.rsplit("/", 1)[0] if "/" in d else ""
        found = licence_files(d)
        if found:
            return identify(show(found[0]) or ""), found[0]
    return "no licence", None


def upstream(_args):
    # Hand-written in sync.toml: where a catalogue entry's skill really is,
    # when its link points somewhere it isn't.
    moved = {o["stub"]: o["repo"] for o in config().get("upstream_override", [])}
    by_repo = {}
    for stub, url in catalogue():
        url = moved.get(stub, url)
        owner, repo = owner_repo(url)
        by_repo.setdefault(f"https://github.com/{owner}/{repo}", []).append((stub, url))
    shipped, held = [], []
    for repo_url, stubs in sorted(by_repo.items()):
        owner, repo = owner_repo(repo_url)
        dest = CACHE / "discover" / f"{owner}__{repo}"
        shutil.rmtree(dest, ignore_errors=True)
        dest.parent.mkdir(parents=True, exist_ok=True)
        try:
            subprocess.run(["git", "clone", "-q", "--depth", "1", "--filter=blob:none", "--no-checkout",
                            repo_url, str(dest)], check=True, capture_output=True, text=True)
        except subprocess.CalledProcessError as e:
            # GitHub asks for a login for a repository that isn't public.
            gone = "could not read Username" in e.stderr or "not found" in e.stderr.lower()
            why = ("repository not found on GitHub (private, renamed or removed)" if gone
                   else f"could not fetch: {e.stderr.strip()[:120]}")
            for stub, _ in stubs:
                held.append({"stub": stub, "repo": repo_url, "why": why})
            continue
        commit = git("rev-parse", "HEAD", cwd=dest)
        files = git("ls-tree", "-r", "--name-only", "HEAD", cwd=dest).splitlines()
        dirs = sorted({f.rsplit("/", 1)[0] if "/" in f else "" for f in files
                       if f.rsplit("/", 1)[-1] == "SKILL.md"})

        def show(path, dest=dest):
            r = subprocess.run(["git", "show", f"HEAD:{path}"], cwd=dest, capture_output=True)
            return r.stdout.decode("utf-8", "replace") if r.returncode == 0 else None

        for stub, url in stubs:
            d = find_skill(stub, url, dirs, show)
            if d is None:
                why = (f"upstream has {len(dirs)} skills and none is this one (renamed, split or removed)"
                       if dirs else "upstream has no SKILL.md")
                held.append({"stub": stub, "repo": repo_url, "why": why})
                continue
            name, file = folder_licence(d, files, show)
            if name in PERMISSIVE:
                entry = {"stub": stub, "repo": repo_url, "commit": commit, "path": d, "license": name}
                if file:
                    entry["license_file"] = file
                shipped.append(entry)
            else:
                why = {"no licence": "no licence file: all rights reserved by default",
                       "proprietary": "proprietary licence (all rights reserved)",
                       "see file": "its own terms, not an open-source licence"}.get(name, f"licence: {name}")
                held.append({"stub": stub, "repo": repo_url, "why": why + (f" ({file})" if file else "")})
        print(f"{owner}/{repo}: {len(stubs)} entries")

    def toml(v):
        return json.dumps(v, ensure_ascii=False)
    out = [MARK, ""]
    for s in sorted(shipped, key=lambda s: s["stub"]):
        out.append("[[skills]]")
        out += [f"{k} = {toml(s[k])}" for k in ("stub", "repo", "commit", "path", "license", "license_file") if k in s]
        out.append("")
    for s in sorted(held, key=lambda s: s["stub"]):
        out.append("[[not_shipped]]")
        out += [f"{k} = {toml(s[k])}" for k in ("stub", "repo", "why")]
        out.append("")
    text = (LIB / "sync.toml").read_text()
    text = text.split("\n" + MARK, 1)[0].rstrip("\n") + "\n\n" + "\n".join(out)
    (LIB / "sync.toml").write_text(text)
    print(f"{len(shipped)} skills to ship, {len(held)} kept as links; see sync.toml")


if __name__ == "__main__":
    commands = {"sync": sync, "media": media, "check": check, "upstream": upstream}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]](sys.argv[2:])
