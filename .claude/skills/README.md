# Vendored design skills

Copied from upstream so every session, local or cloud, loads them. Each keeps
its own licence file. Update by copying a newer upstream folder over it.

| Skill | Upstream | Commit | Licence |
|---|---|---|---|
| `interface-design` | github.com/Dammyjay93/interface-design (`.claude/skills/interface-design`) | 2f9be32 | MIT |
| `ui-ux-pro-max` | github.com/nextlevelbuilder/ui-ux-pro-max-skill (`.claude/skills/ui-ux-pro-max`) | 67803ea | MIT |
| `frontend-design` | github.com/anthropics/skills (`skills/frontend-design`) | dbd4588 | Apache-2.0 |
| `impeccable` | github.com/pbakaus/impeccable (`.claude/skills/impeccable`, SKILL.md and `reference/` only) | 5cb95ee | Apache-2.0 |

ui-ux-pro-max's installer also adds banner-design, brand, design, design-system,
slides and ui-styling. They are for web pages and marketing (Tailwind, shadcn,
logos, slide decks) and are left out here.

From github.com/Owl-Listener/designer-skills (commit 9a6930c, MIT, licence in
`LICENSE-designer-skills`), the four plugins ui-design, design-systems,
visual-critique and interaction-design are copied in as plain files: their
skills into this folder (57, e.g. `color-system`, `design-token`,
`critique-color`, `micro-interaction-spec`), and their commands into
`.claude/commands/<plugin>/` (`/color-palette`, `/type-system`, `/tokenize`,
`/audit-system`, `/critique-screen`, …). Installed as a plugin they would be
`/ui-design:color-palette`; copied, they keep the plugin's folder as a
namespace where Claude Code shows one.

Impeccable comes without its `scripts/` folder: its detector and live-browser
tools read web HTML/CSS and a browser page, and Forge's views are Rust/GPUI,
so they find nothing here. Without the launcher the skill reads the project's
context itself (`PRODUCT.md`, `DESIGN.md`), as its own fallback says, and its
critiques run as design reviews of screenshots (`.impeccable/critique/`).
