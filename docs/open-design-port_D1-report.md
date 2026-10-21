# D1 report: the design library, stored, synced and indexed

Done on `claude/laughing-albattani-7lfjod`, built on your latest work
(`claude/wonderful-pasteur-1b732o`), in one commit per step after `9aa0e36`
(the plan, `docs/open-design-port.md`).

**Nothing in D1 is visible in the app**: it is the library, the tool that
keeps it, and the index the design chats will read in D2. So there are no
screenshots. **Nothing was run on Windows.** The whole app was type-checked
for Windows (`cargo check --target x86_64-pc-windows-gnu`) and the installer
was compiled with `makensis`, both on Linux.

The gates, on the final commit:
- `cargo build`: 0 warnings.
- `cargo test`: 398 passed (13 ignored), 28 of them new (`design::`).
- `cargo clippy --all-targets -D warnings`: clean for everything D1
  touched. It fails on 4 findings in `state.rs`, `ui/doc_changes.rs`,
  `ui/doc_pane.rs` and `ui/sheet_view.rs`, which were already on the branch
  before D1 and which D1 doesn't touch.
- `cargo fmt --check`: clean for `src/design/`. The branch already had
  differences in `backend.rs` and `commands.rs`; left as they were.

## How the library is stored

Measured, not guessed: everything kept from Open Design is 7,288 files.
After removing duplicates it is 128 MB: 53 MB of text, 76 MB of images,
video and audio.

- **Text, fonts and code bundles are committed** in `design-library/`, in
  Open Design's own layout, so every relative link inside a template or a
  skill still works. Git stores identical files once, so Open Design's
  duplicated trees cost nothing: the committed part packs to about 16 MB.
- **Images, video and audio are not committed.** `media.lock.json` pins each
  one by SHA-256, and `python3 -I scripts/sync_open_design.py media`
  fetches them once from the pinned commit, into a cache keyed by hash. The
  tests never need the network.
- **Nothing is ever edited in the mirror.** Forge's own files and its
  changed copies will live in `design-library/forge/`, each saying what it
  changed (Apache-2.0 §4(b)).

The existing `design/` folder holds Forge's own mock-ups, hence the name
`design-library/`.

## The sync tool (`scripts/sync_open_design.py`)

| Command | Does |
|:--|:--|
| `sync [--from DIR]` | Rebuilds the library from the commits in `sync.toml`. Writes `UPSTREAM.json`, `media.lock.json` and `THIRD_PARTY_LICENSES.md`. |
| `media` | Fetches the locked media, each distinct file once, checking its hash. |
| `check` | Every locked media file is there with its hash, and every shipped upstream skill's licence text is there. |
| `upstream` | Finds the real skill behind each of Open Design's catalogue entries and writes the result into `sync.toml` (below). |
| `stage DEST` | What the installer ships: exactly the committed files plus the locked media. Refuses an uncommitted library. |

It reads every file **from git at the pinned commit** (`git ls-tree -z`,
`git cat-file --batch`), never from a working copy. So local edits,
untracked files, Windows line-ending conversion and symlinks can't get into
the library. Every git call runs with prompts, line-ending conversion and
symlinks off.

## The index (`src/design/`)

`Index::load` reads the whole library in under a second:

| Kind | Count |
|:--|--:|
| Skills | 169 (85 are catalogue entries: 36 now real, 49 links) |
| Templates | 115, plus 11 cards made from their `examples/*.html` |
| Examples | 188 |
| Design systems | 152 |
| Craft rules | 11 (+ 3 listed as still to come) |
| Atoms / scenarios | 13 / 14 |
| Image / video templates | 45 / 63 |
| Community plugins | 10 |

It reads them the way Open Design's own daemon does, checked against it file
by file:
- **YAML front-matter** with `yaml-rust2 =0.13.0`. The 2 upstream files that
  aren't valid YAML are read with a port of Open Design's own reader
  (`od_subset.rs`), so they keep every field.
- **Skill fields** with a port of its `listSkills` (`skill_meta.rs`): id,
  mode, surface, platform, scenario, triggers, example prompt.
- **Plugin manifests:** craft from `od.context.craft`, skills by path or by
  another skill's name, hidden atoms and internal scenarios, CC-BY credits.
- **Previews** follow Open Design's search. Its own web copies map to the
  library's files.
- **Design systems** come from `design-systems/<id>/` (manifest, `DESIGN.md`),
  never the older copies beside the gallery cards.

Upstream's own quirks (2 invalid YAML headers, 8 previews upstream names
but doesn't ship) are reported as tolerated. Anything else fails the tests.

`forge-gpui --design-check` prints the summary. `--design-check <kind> <id>`
prints one entry as the index read it.

## Upstream skills: 36 ship, 49 stay links

`upstream` reads the licence that applies to each skill's own folder: its
licence file, else its `SKILL.md` `license:` (which must agree with the
repository's), else the nearest licence above. Only MIT, Apache-2.0, BSD,
ISC, 0BSD, Unlicense, CC0 and CC-BY ship, each with its licence text and any
NOTICE or CREDITS file beside it.

| Held back | How many | Why |
|:--|--:|:--|
| Anthropic's `docx`, `pdf`, `pptx` | 3 | Proprietary (source-available) |
| Figma's 7 skills | 7 | `figma/skills` isn't public; the same skills, in `openai/skills`, are under Figma's Developer Terms, not an open-source licence |
| No licence file | 6 | All rights reserved by default |
| Repository gone | 8 | Private, renamed or removed |
| Renamed or split upstream | 25 | e.g. `apple-hig` and `threejs` are now collections of 8–15 skills |

Each reason is in `sync.toml`. `THIRD_PARTY_LICENSES.md` lists every folder
licence (106 MIT, 12 Apache-2.0, 1 CC0), every shipped skill with its
copyright line, and credits the 72 CC-BY image and video templates.

## The installer

`design-library\` installs beside `Forge.exe` (where Forge looks first). It
is replaced whole on update and removed on uninstall, but only when it holds
Forge's `UPSTREAM.json`. `build.sh` fetches and checks the media, then
stages with `stage`. Compiled with `makensis -WX`: about 86 MB.

## Review

Three reviewers (Rust index, sync tool, installer and git) each reproduced
what they reported. Everything is fixed, with a test where one fits:

| Found | Fixed |
|:--|:--|
| Shipped upstream skills had no licence text beside them | Licence and NOTICE/CREDITS files ship with each skill; `check` and a test fail without them |
| A restrictive `license:` in a `SKILL.md` could be overridden by the repository's MIT | A declared licence must be permissive and agree with the file |
| CC-BY text read as CC0; BSD's "All rights reserved" read as proprietary; MIT + Commons Clause read as MIT | Restrictions first, title-based CC detection, BSD/ISC grants |
| Duplicate media downloaded concurrently into one temp file | Each distinct file once, its own temp file, failures reported not raised |
| Windows checkouts convert line endings, breaking hashes | Files read from git; conversion off |
| `--from` mirrored local edits and untracked files; symlinks were followed | Files read from git at the pinned commit; symlinks skipped |
| A YAML alias bomb exhausted memory | Front-matter with aliases is never expanded |
| Accented names in `examples/` panicked | Extension checked before cutting |
| A preview URL could resolve outside the library | Every manifest path goes through `join_inside` |
| Symlink loops stalled the `needs` scan | No symlinks followed; depth and count capped |
| Media previews on disk but not in the lock were dropped | Disk counts too |
| The licence test checked less than it claimed | It checks every licence file and every declared licence |
| `--design-check` printed nothing in a Windows release build | Attaches to the parent console |
| `.gitignore` was case-sensitive; staging shipped stray files; installs deleted any `design-library` | Case-blind ignores; `stage`; guarded deletes |

## Not done, or for D2

- **Seen on Windows:** nothing. The type-check and the installer build are
  the evidence.
- **The prompt runtime.** The agent mapping how Open Design builds its
  prompts hit the session limit twice. D2 starts there.
- **`plugins/spec/`** is mirrored but not indexed, as Open Design doesn't
  index it: it is examples for plugin authors.
- **Collections** (`apple-hig`, `threejs`, `platform-design`, …): Open Design
  lists each as one skill, while upstream now ships 8–15. They stay links.
  Shipping them as sets is a choice for later.
