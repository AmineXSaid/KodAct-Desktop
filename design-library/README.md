# Design library

What Forge's design chats read: [Open Design](https://github.com/nexu-io/open-design)'s
skills, templates, design systems, craft rules and plugin manifests, plus the
upstream skills its catalogue links to. Forge indexes it in Rust
(`src/design/`) and builds each design chat's system prompt from it; the plan
is `docs/open-design-port.md`.

| Path | What |
|:--|:--|
| `skills/`, `design-templates/`, `design-systems/`, `craft/`, `templates/`, `prompt-templates/`, `plugins/`, `assets/`, `data/`, `figma-plugin/`, `clipper/`, `apps/desktop/vendor/dom-to-pptx/`, `docs/schemas/` | Open Design, mirrored unchanged in its own layout |
| `upstream/<owner>__<repo>/` | Upstream skills, where their licence allows it |
| `forge/` | Forge's own files, and its changed copies of Open Design files |
| `sync.toml` | What is taken, from which commits (edit this) |
| `UPSTREAM.json`, `media.lock.json`, `THIRD_PARTY_LICENSES.md` | Written by the sync |

Images, video and audio are not committed: `media.lock.json` pins each by
SHA-256, and a fresh checkout fetches them once:

    python3 -I scripts/sync_open_design.py media

To move to a newer upstream commit, change `commit` in `sync.toml`, run
`python3 -I scripts/sync_open_design.py sync`, and review the diff.

Installed, the library is `design-library\` beside `Forge.exe`. Forge finds
it there, or at `FORGE_DESIGN_LIB`, or (in a development build) in this
folder.
