"""KodAct's design library (design-library/): Open Design's content, kept in
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
        ~/.cache/kodact-design/media, so a second run needs no network.
    python3 -I scripts/sync_open_design.py check
        Verify the tree: every locked media file present, with its hash.
    python3 -I scripts/sync_open_design.py stage DEST
        Copy what the installer ships into DEST: every committed library
        file and every media file the lock pins, and nothing else that
        happens to be in the folder (a run's caches, an older pin's media).
    python3 -I scripts/sync_open_design.py upstream
        For each of Open Design's catalogue entries (a SKILL.md that only
        links to another repository), find the real skill upstream and the
        licence of its folder, and write the result into sync.toml: skills
        whose licence lets KodAct ship them under [[skills]], the rest under
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
import tempfile
import time
import tomllib
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB = ROOT / "design-library"
CACHE = Path(os.environ.get("KODACT_DESIGN_CACHE", Path.home() / ".cache" / "kodact-design"))

# Files in design-library/ that are KodAct's own, never touched by a sync.
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


# Every git call: never ask for a login (a private or deleted repository
# fails instead of hanging), never convert line endings or make symlinks
# (Git for Windows does both by default), and quote no file names.
GIT = ["git", "-c", "credential.helper=", "-c", "core.autocrlf=false", "-c", "core.eol=lf",
       "-c", "core.symlinks=false", "-c", "core.quotePath=false"]
GIT_ENV = {**os.environ, "GIT_TERMINAL_PROMPT": "0", "GCM_INTERACTIVE": "never", "LC_ALL": "C"}


def git(*args, cwd=None, raw=False):
    out = subprocess.run([*GIT, *args], cwd=cwd, check=True, capture_output=True, env=GIT_ENV).stdout
    return out if raw else out.decode("utf-8").strip()


class Tree:
    """A repository at one commit, read from git itself: file names from
    `git ls-tree -z`, bytes from `git cat-file --batch`. Never the working
    tree, so local edits, untracked files and line-ending conversion can't
    leak into the library, and a symlink is never followed."""

    def __init__(self, path, commit):
        self.path, self.commit = Path(path), commit
        self.symlinks = []
        self._cat = None

    def files(self, root):
        """Every file under `root` (a folder, a file, or "" for all), as
        (path, blob id), in a stable order."""
        spec = ["--", root] if root else []
        listing = git("ls-tree", "-r", "-z", "--full-tree", self.commit, *spec, cwd=self.path, raw=True)
        out = []
        for item in listing.split(b"\0"):
            if not item:
                continue
            meta, name = item.split(b"\t", 1)
            mode, kind, blob = meta.decode().split()
            name = name.decode("utf-8")
            if root and name != root and not name.startswith(root.rstrip("/") + "/"):
                continue
            if mode == "120000":
                self.symlinks.append(name)
            elif kind == "blob":
                out.append((Path(name), blob))
        if root and not out and not self.symlinks:
            sys.exit(f"{root} is not in {self.path} at {self.commit[:12]}")
        return sorted(out)

    def read(self, blob):
        if self._cat is None:
            self._cat = subprocess.Popen([*GIT, "cat-file", "--batch"], cwd=self.path, env=GIT_ENV,
                                         stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self._cat.stdin.write(blob.encode() + b"\n")
        self._cat.stdin.flush()
        header = self._cat.stdout.readline().split()
        if len(header) < 3 or header[1] != b"blob":
            sys.exit(f"git cat-file: no blob {blob} in {self.path}")
        data = self._cat.stdout.read(int(header[2]))
        self._cat.stdout.read(1)
        return data


def owner_repo(url):
    parts = urllib.parse.urlparse(url).path.strip("/").removesuffix(".git").split("/")
    return parts[0], parts[1]


def checkout(url, commit, local=None, paths=None):
    """A tree of `url` at `commit`: `local` when it is there, else a cached
    shallow fetch of just that commit. With `paths`, only those folders and
    files are checked out (and only their contents fetched), so a large
    repository holding one skill costs that skill."""
    if local:
        try:
            git("cat-file", "-e", f"{commit}^{{commit}}", cwd=local)
        except subprocess.CalledProcessError:
            sys.exit(f"{local} doesn't have the pinned commit {commit}")
        return Tree(local, commit)
    owner, repo = owner_repo(url)
    key = "v2-" + commit[:12]
    if paths is not None:
        key += "-" + hashlib.sha256("\n".join(sorted(paths)).encode()).hexdigest()[:8]
    dest = CACHE / "trees" / f"{owner}__{repo}@{key}"
    if (dest / ".forge-complete").exists():
        return Tree(dest, commit)
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
    # The checkout fetches the (sparse) files' contents in one go; they are
    # then read from git, not from disk.
    git("checkout", "-q", "FETCH_HEAD", cwd=dest)
    (dest / ".forge-complete").write_text(commit)
    return Tree(dest, commit)


def is_media(path, exts):
    return path.suffix.lower().lstrip(".") in exts


def raw_url(url, commit, path):
    owner, repo = owner_repo(url)
    quoted = urllib.parse.quote(path.as_posix())
    return f"https://raw.githubusercontent.com/{owner}/{repo}/{commit}/{quoted}"


def place(data, dest):
    """Write `data` to `dest` unless it already holds the same bytes."""
    if dest.is_file() and dest.stat().st_size == len(data) and dest.read_bytes() == data:
        return False
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(data)
    return True


def mirror(tree, roots, prefix, exts, url, wanted, media):
    for root in roots:
        for rel, blob in tree.files(root):
            dest_rel = (prefix / rel).as_posix().removeprefix("./")
            if dest_rel in wanted:
                continue
            wanted.add(dest_rel)
            data = tree.read(blob)
            place(data, LIB / dest_rel)
            if is_media(rel, exts):
                media.append({
                    "path": dest_rel,
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "size": len(data),
                    "url": raw_url(url, tree.commit, rel),
                })
    if tree.symlinks:
        print(f"  skipped {len(tree.symlinks)} symlinks in {url}: {', '.join(tree.symlinks[:5])}")


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
        "# Third-party licences in KodAct's design library",
        "",
        "Generated by `scripts/sync_open_design.py sync` from `sync.toml`. Do not edit by hand.",
        "",
        "## Open Design",
        "",
        f"Most of this folder is [Open Design]({od['repo']}) at commit `{od['commit']}` "
        f"({od['date']}), licensed under the Apache License 2.0 (`LICENSE`). "
        "It is mirrored unchanged, in its own layout. KodAct's own files and its "
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
                  "Open Design's catalogue lists these skills by link only. KodAct ships the "
                  "real skill, from the repository and commit below, where its licence allows "
                  "redistribution. They are under `upstream/<owner>__<repo>/`.", "",
                  "Each one's licence text (and any notice it asks to keep) ships beside "
                  "it, at the path given.", "",
                  "| Catalogue entry | Repository | Commit | Path | Licence | Licence text |",
                  "|:--|:--|:--|:--|:--|:--|"]
        for s in skills:
            owner, repo = owner_repo(s["repo"])
            base = f"upstream/{owner}__{repo}/"
            texts = [f"`{base}{f}`" for f in [s.get("license_file"), *s.get("notices", [])] if f]
            holder = ""
            if s.get("license_file") and (LIB / base / s["license_file"]).is_file():
                holder = next((l.strip() for l in (LIB / base / s["license_file"]).read_text(errors="replace").splitlines()
                               if l.strip().lower().startswith("copyright")), "")
            lines.append(f"| `{s['stub']}` | [{owner}/{repo}]({s['repo']}) | `{s['commit'][:12]}` "
                         f"| `{s['path'] or '/'}` | {s['license']}{(' — ' + holder) if holder else ''} "
                         f"| {', '.join(texts)} |")
    held = cfg.get("not_shipped", [])
    if held:
        lines += ["", "## Catalogue entries kept as links only", "",
                  "Their upstream licence does not allow redistribution, so KodAct ships only "
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


# Licences that let KodAct ship a skill inside a closed-source app (CLAUDE.md
# rule 1). CC-BY asks for credit, which THIRD_PARTY_LICENSES.md gives.
PERMISSIVE = {"MIT", "Apache-2.0", "BSD", "ISC", "0BSD", "Unlicense", "CC0-1.0", "CC-BY-4.0"}


def identify(text):
    """A licence's name from its text (or from a front-matter `license:`
    value such as `MIT` or `Apache-2.0`). Anything that adds a restriction
    (a Commons Clause, non-commercial, no derivatives, share-alike) is
    "restricted", whatever licence it starts from."""
    flat = " ".join(text.split()).lower()
    spdx = {"mit": "MIT", "apache-2.0": "Apache-2.0", "apache 2.0": "Apache-2.0", "isc": "ISC",
            "bsd-2-clause": "BSD", "bsd-3-clause": "BSD", "0bsd": "0BSD", "unlicense": "Unlicense",
            "cc0-1.0": "CC0-1.0", "cc0": "CC0-1.0", "cc-by-4.0": "CC-BY-4.0", "cc by 4.0": "CC-BY-4.0",
            "gpl-2.0": "GPL", "gpl-3.0": "GPL", "gpl-3.0-only": "GPL", "gpl-3.0-or-later": "GPL",
            "lgpl-2.1": "LGPL", "lgpl-3.0": "LGPL", "agpl-3.0": "AGPL", "mpl-2.0": "MPL-2.0"}
    if flat in spdx:
        return spdx[flat]
    title = flat[:400]
    if any(w in flat for w in ("commons clause", "may not be sold", "shall not sell", "not for commercial use")):
        return "restricted"
    creative = "creative commons" in title or "cc-by" in title or "cc by" in title or "cc0" in title
    if creative:
        if any(w in title for w in ("noncommercial", "non-commercial", "noderivatives", "no derivatives",
                                    "sharealike", "share-alike", "-nc", "-nd", "-sa")):
            return "restricted"
        # CC BY's own text says the licence texts are CC0: look at the title.
        if "attribution 4.0" in title or "cc-by-4.0" in title or "cc by 4.0" in title:
            return "CC-BY-4.0"
        if "cc0" in title:
            return "CC0-1.0"
        return "Creative Commons"
    if "redistribution and use in source and binary forms" in flat:
        return "BSD"
    if "permission to use, copy, modify, and/or distribute this software for any purpose" in flat:
        return "ISC"
    grants = ("permission is hereby granted", "licensed under the apache license", "apache license")
    if "all rights reserved" in flat[:600] and not any(g in flat for g in grants):
        return "proprietary"
    head = " ".join(text.split()[:60]).lower()
    for needle, name in [("apache license", "Apache-2.0"), ("mit license", "MIT"),
                         ("permission is hereby granted, free of charge", "MIT"),
                         ("sil open font license", "OFL-1.1"),
                         ("isc license", "ISC"), ("mozilla public license", "MPL-2.0"),
                         ("this is free and unencumbered software", "Unlicense"),
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
    mirror(tree, od["roots"], Path("."), exts, od["repo"], wanted, media)
    # One sparse checkout per upstream repository, of just its skills' folders.
    by_repo = {}
    for s in cfg.get("skills", []):
        by_repo.setdefault((s["repo"], s["commit"]), []).append(s)
    for (url, commit), skills in by_repo.items():
        owner, repo = owner_repo(url)
        # Each skill's folder, plus the licence and notice files that apply
        # to it, wherever they are in its repository: they ship with it.
        roots = sorted({r for s in skills for r in
                        [s["path"], *s.get("also", []), *([s["license_file"]] if s.get("license_file") else []),
                         *s.get("notices", [])]})
        up = checkout(url, commit, paths=roots)
        prefix = Path(UPSTREAM_SKILLS) / f"{owner}__{repo}"
        mirror(up, roots, prefix, exts, url, wanted, media)
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
        "skills": [{k: s[k] for k in ("stub", "repo", "commit", "path", "license", "license_file", "notices")
                    if k in s} for s in cfg.get("skills", [])],
        "not_shipped": cfg.get("not_shipped", []),
    }
    (LIB / "UPSTREAM.json").write_text(json.dumps(upstream, indent=1, ensure_ascii=False) + "\n")
    (LIB / "THIRD_PARTY_LICENSES.md").write_text(licences(cfg, wanted))
    c = upstream["counts"]
    print(f"{c['files']} files ({c['text_files']} committed, {c['media_files']} media), "
          f"{removed} removed")


def download(sha, url):
    """The file with this SHA-256 in the cache, downloaded once if it isn't:
    into a temp file of its own, checked, then moved into place."""
    cached = CACHE / "media" / sha
    if cached.is_file() and sha256(cached) == sha:
        return cached
    cached.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=cached.parent, prefix=sha[:16], suffix=".part")
    os.close(fd)
    tmp = Path(tmp)
    try:
        for attempt in range(4):
            try:
                with urllib.request.urlopen(url, timeout=60) as r, open(tmp, "wb") as f:
                    shutil.copyfileobj(r, f)
                break
            except OSError:
                if attempt == 3:
                    raise
                time.sleep(2 ** (attempt + 1))
        if sha256(tmp) != sha:
            raise ValueError("hash mismatch")
        os.replace(tmp, cached)
        return cached
    finally:
        tmp.unlink(missing_ok=True)


def fetch(sha, entries):
    """Every library path that holds the file with this SHA-256."""
    todo = [e for e in entries if not ((LIB / e["path"]).is_file()
                                       and (LIB / e["path"]).stat().st_size == e["size"]
                                       and sha256(LIB / e["path"]) == sha)]
    if not todo:
        return [(e, "ok") for e in entries]
    try:
        cached = download(sha, todo[0]["url"])
    except Exception as e:  # noqa: BLE001 - one bad file must not stop the rest
        return [(e2, "ok") for e2 in entries if e2 not in todo] + [(t, f"failed: {e}") for t in todo]
    for e in todo:
        dest = LIB / e["path"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(cached, dest)
    return [(e, "ok" if e not in todo else "fetched") for e in entries]


def media(_args):
    entries = json.loads((LIB / "media.lock.json").read_text())["files"]
    by_sha = {}
    for e in entries:
        by_sha.setdefault(e["sha256"], []).append(e)
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        results = [r for group in pool.map(lambda kv: fetch(*kv), by_sha.items()) for r in group]
    states = [r for _, r in results]
    failed = [(e["path"], r) for e, r in results if r.startswith("failed")]
    print(f"{len(entries)} media files: {states.count('fetched')} fetched, "
          f"{states.count('ok')} already there, {len(failed)} failed")
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
    for sk in config().get("skills", []):
        owner, repo = owner_repo(sk["repo"])
        for f in [sk.get("license_file"), *sk.get("notices", [])]:
            if not f:
                bad.append(f"no licence text for upstream skill {sk['stub']}")
            elif not (LIB / UPSTREAM_SKILLS / f"{owner}__{repo}" / f).is_file():
                bad.append(f"missing {UPSTREAM_SKILLS}/{owner}__{repo}/{f} (licence text of {sk['stub']})")
    print(f"{len(entries)} media files, {len(bad)} problems")
    for b in bad[:50]:
        print("  " + b)
    sys.exit(1 if bad else 0)


def stage(args):
    if len(args) != 1:
        sys.exit("stage DEST")
    dest = Path(args[0])
    # A release ships what is committed: anything else is a mistake to fix first.
    dirty = git("status", "--porcelain", "--", "design-library", cwd=ROOT)
    if dirty:
        sys.exit("design-library has changes that aren't committed; commit them before "
                 "staging a release:\n" + dirty[:2000])
    tracked = git("ls-files", "-z", "--", "design-library", cwd=ROOT, raw=True).split(b"\0")
    files = [f.decode("utf-8").removeprefix("design-library/") for f in tracked if f]
    # The sync tool's own git files at the top; nested upstream ones stay.
    files = [f for f in files if f not in (".gitignore", ".gitattributes")]
    files += [e["path"] for e in json.loads((LIB / "media.lock.json").read_text())["files"]]
    shutil.rmtree(dest, ignore_errors=True)
    for f in files:
        (dest / f).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(LIB / f, dest / f)
    print(f"{len(files)} files staged in {dest}")


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
    """The licence that applies to a skill's folder, and the files that must
    ship with it. A licence file in the folder wins; a `license:` in its
    SKILL.md must agree with the repository's licence above it (a restrictive
    one holds the skill back); otherwise the nearest licence file above.
    Returns {license, license_file, notices} or {hold: reason}."""
    def at(d, starts):
        prefix = f"{d}/" if d else ""
        return sorted(f for f in files if f.startswith(prefix) and "/" not in f[len(prefix):]
                      and f[len(prefix):].upper().startswith(starts))
    chain, d = [skill_dir], skill_dir
    while d:
        d = d.rsplit("/", 1)[0] if "/" in d else ""
        chain.append(d)
    licence_file = next((f for d in chain for f in at(d, ("LICENSE", "LICENCE", "COPYING"))), None)
    notices = [f for d in chain for f in at(d, ("NOTICE", "CREDITS", "AUTHORS"))]
    from_file = identify(show(licence_file) or "") if licence_file else None
    own_file = licence_file is not None and licence_file.startswith(f"{skill_dir}/" if skill_dir else "")
    declared = frontmatter_value(show(f"{skill_dir}/SKILL.md" if skill_dir else "SKILL.md") or "", "license")
    named = identify(declared) if declared else None
    if named == "see file":
        # `license: Complete terms in LICENSE.txt` points at the file, which
        # decides; any other value it can't name (`GPL-3.0`, `Proprietary`)
        # is a licence of its own, and holds the skill back.
        pointer = any(w in declared.lower() for w in ("licen", "see ", ".txt", ".md", "terms in"))
        named = None if pointer else declared
    if named and named not in PERMISSIVE:
        return {"hold": f"its SKILL.md says {declared!r}"}
    if own_file or not named:
        name = from_file or "no licence"
    elif from_file and from_file != named:
        return {"hold": f"its SKILL.md says {named}, its repository's licence file says {from_file}"}
    else:
        name = named
    if name not in PERMISSIVE:
        return {"hold": {"no licence": "no licence file: all rights reserved by default",
                         "proprietary": "proprietary licence (all rights reserved)",
                         "restricted": "a licence with restrictions (non-commercial, no derivatives or similar)",
                         "see file": "its own terms, not an open-source licence"}.get(name, f"licence: {name}")
                + (f" ({licence_file})" if licence_file else "")}
    if licence_file is None:
        return {"hold": f"{name} by its SKILL.md, but no licence text to ship with it"}
    return {"license": name, "license_file": licence_file, "notices": notices}


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
            git("clone", "-q", "--depth", "1", "--filter=blob:none", "--no-checkout", repo_url, str(dest))
        except subprocess.CalledProcessError as e:
            # A repository that isn't public asks for a login, which is off.
            err = e.stderr.decode("utf-8", "replace")
            gone = "could not read Username" in err or "not found" in err.lower() or "terminal prompts disabled" in err
            why = ("repository not found on GitHub (private, renamed or removed)" if gone
                   else f"could not fetch: {err.strip()[:120]}")
            for stub, _ in stubs:
                held.append({"stub": stub, "repo": repo_url, "why": why})
            continue
        commit = git("rev-parse", "HEAD", cwd=dest)
        files = [f.decode("utf-8") for f in git("ls-tree", "-r", "-z", "--name-only", "HEAD",
                                                  cwd=dest, raw=True).split(b"\0") if f]
        dirs = sorted({f.rsplit("/", 1)[0] if "/" in f else "" for f in files
                       if f.rsplit("/", 1)[-1] == "SKILL.md"})

        def show(path, dest=dest):
            r = subprocess.run([*GIT, "show", f"HEAD:{path}"], cwd=dest, capture_output=True, env=GIT_ENV)
            return r.stdout.decode("utf-8", "replace") if r.returncode == 0 else None

        for stub, url in stubs:
            d = find_skill(stub, url, dirs, show)
            if d is None:
                why = (f"upstream has {len(dirs)} skills and none is this one (renamed, split or removed)"
                       if dirs else "upstream has no SKILL.md")
                held.append({"stub": stub, "repo": repo_url, "why": why})
                continue
            found = folder_licence(d, files, show)
            if "hold" in found:
                held.append({"stub": stub, "repo": repo_url, "why": found["hold"]})
            else:
                shipped.append({"stub": stub, "repo": repo_url, "commit": commit, "path": d, **found})
        print(f"{owner}/{repo}: {len(stubs)} entries")

    def toml(v):
        return json.dumps(v, ensure_ascii=False)
    out = [MARK, ""]
    for s in sorted(shipped, key=lambda s: s["stub"]):
        out.append("[[skills]]")
        out += [f"{k} = {toml(s[k])}" for k in ("stub", "repo", "commit", "path", "license", "license_file", "notices")
                if k in s and s[k] != []]
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
    commands = {"sync": sync, "media": media, "check": check, "upstream": upstream, "stage": stage}
    if len(sys.argv) < 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]](sys.argv[2:])
