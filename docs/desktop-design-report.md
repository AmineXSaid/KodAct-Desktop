# Desktop design pass: warm ivory, softly squared, a living dot field

Forge's UI moved onto a calm desktop palette (ivory paper, warm ink, one
terracotta accent) with a single button family, a painted dot field on the
home screen, and a composer built as one unit. All native GPUI; no web view.

Seen on **Linux only (Xvfb 1280×860, Mesa lavapipe), light and dark**.
Nothing was run on Windows.

## Gates (final commit)

- `cargo build`: 0 warnings
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `cargo test`: 289 passed (13 ignored), plus the 3 cube image tests

## What changed

**Tokens (`src/theme.rs`)**: the existing token names were kept, so every view
picked up the new palette:
- Light: ivory `#f9f8f5`, warm ink `#2b2a27`.
- Dark: charcoal `#262624` / `#30302e`.
- Accent: terracotta `#d97757` in both themes.

New tokens were added for both themes:
- Colours: `accent`, `accent_soft`, `accent_fg`, `border_strong`, `sunk`,
  `toggle_on`, `active`, `fill_fg`, `memory`, `memory_tint`, `dot`.
- Spacing: `tight` 6, `snug` 10, `loose` 20, `xxxl` 40.
- Radii: `segment` 6, `compact` 7, `control` 9, `field` 12. `card` is now 10
  and the composer 14.
- Sizes: control sizes, the collapsed rail, the home heading, the dot grid.

Departures from the brief's palette, each forced by `palette_check.rs`:

| Token | Brief | Used | Why |
|---|---|---|---|
| light secondary / muted text | `#6f6c64` / `#9a968c` | `#64615a` / `#8c887e` | AA on the title bar |
| light success | `#2f7d4f` | `#276b43` | AA on its tint |
| light memory | `#8a6a1f` | `#75591a` | AA on its tint |
| dark danger | `#e07a68` | `#eb8c7b` | AA on its tint |
| dark accent_soft | `#6b3d2c` | `#4d3128` | empty Send read as active |
| links | — | `#a84b2b` / `#e08a6d` | the accent at a text-safe depth |

The filled (primary) button is ink on paper (paper on charcoal), as in the
reference; the accent is kept for Send and the new-chat mark.

**Controls (`src/ui/button.rs`, new)**:
- `Button`: primary, secondary, ghost, outlined, danger, accent. It can be
  labelled or icon-only, compact or large, and has disabled, loading and
  selected states.
- Also: `send_button` (Send / pale empty / Stop), `Switch`, `Segmented`, and
  a `selector` trigger.
- Every control takes keyboard focus (Tab, then Enter or Space). The focus
  edge shows for the keyboard only, controls carry accessible roles and
  labels, and they swallow clicks while disabled or loading.
- The old `theme::*_button` helpers now paint from the same variants, so
  there is one button system.
- Tests check that every variant's label reads at AA on its fill, and that
  the radii stay between 7 and 10px.

**Dot field (`src/ui/dot_field.rs`, new)**:
- One `canvas` paints every mark as a quad (diamonds as paths), from
  geometry that is cached per size.
- Grid: 14px, anchored at the centre so it stays put while panels open.
- Weight: two soft clouds (beside the heading, lower right) and a faint band.
  The density thins with distance, cut by a quiet oval behind the heading and
  composer, and faded before every edge.
- About 1.6% of dots carry the accent. Rarer still are outlined squares and
  diamonds.
- Motion: each dot breathes on its own phase over 9s (±22% opacity). The
  field leans up to 4px from the pointer, with per-dot depth, and brightens
  slightly while the composer has focus.
- The ticker runs at about 4.5 fps while breathing and about 30 fps only
  while easing. It rests when the window is inactive or the field isn't
  painted.
- Reduced motion (`FORGE_REDUCED_MOTION=1` or the system setting) gives a
  still field and no ticker.
- The field has no input handlers, so it never takes a click.
- Tradeoff: GPUI re-renders a notified view's ancestors, so each tick
  re-renders the workspace. That is why the rate stays low.

**Shell and screens**:
- **Home:** the display-serif greeting beside Forge's mark, a muted line
  naming the project, the composer, and starter chips, all over the dot field.
- **Composer:** a white box resting in a `sunk` tray.
  - The box holds an outlined + and the accent Send / Stop.
  - The tray holds the project (opens Projects), the context meter, the model
    and the mode.
  - The border deepens while the composer has focus.
- **Sidebar:**
  - The current row is a wash, with no bordered card.
  - New chat has a round accent mark.
  - Ctrl+B now collapses the sidebar to a 52px rail of icon buttons for the
    same actions (it used to hide it).
- **Permissions:** Deny (ghost), Always allow (secondary), Allow once
  (primary). The broad grant is never the loudest button.
- **Tool rows:** an accessible label, an accent spinner, a firmer edge while
  running, and a danger-tinted edge on failure.
- **Settings:**
  - Appearance is a segmented control.
  - Bypass and Web browser are switches bound to their real state.
  - Save is disabled while the name is empty.
  - Check and save shows a loader while working.
- **Other:** the delete confirm uses the danger button, the doc-pane header
  uses icon buttons, and button-like pills elsewhere are softly squared.

## Limits

- Icons stay Lucide at the 12/14/16/20 scale with its 2px stroke (CLAUDE.md),
  not the reference's 1.7px.
- Unchanged:
  - Popover placement and keyboard navigation (already in place).
  - Markdown and code blocks.
  - The chat placeholder.
- No artifact/version panel was added: Forge has no engine behind one.
- Some older hand-drawn controls (sidebar rows, menu rows, title-bar
  buttons) are styled from the tokens but aren't `Button`s, so they don't
  take Tab focus.
