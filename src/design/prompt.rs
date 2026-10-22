//! A design chat's system prompt, built from the library for each run
//! (`docs/open-design-port.md` §5.1). In order, stable parts first so
//! prompt caching holds across a chat's turns:
//!
//! 1. Forge's design charter: how a design is made here (one file, written
//!    top to bottom so the panel can show it forming; look at it with
//!    `design_render`, check it with `design_lint`, revise);
//! 2. the task profile for the surface, Open Design's own file kept as it
//!    is, after a short note on how its host's words map to Forge;
//! 3. the chosen design system's `DESIGN.md`;
//! 4. the craft rules the surface and the skill ask for;
//! 5. the skill;
//! 6. the template, named as the file to start from.
//!
//! Every part has a budget; a part over it is cut at a paragraph and says
//! where the rest is, for `design_read`.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::library::{Entry, Index, Kind};

/// Where Open Design keeps its task profiles in the library.
macro_rules! profiles_dir {
    () => {
        "plugins/_official/scenarios/od-next-strategy/assets/task-profiles"
    };
}

/// What kind of work a design chat makes: which task profile it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Surface {
    /// No profile chosen: the agent picks one per request and reads it
    /// with `design_read`.
    #[default]
    Any,
    Deck,
    Prototype,
    /// A report, guide, course page or other page to read.
    Document,
    /// A poster, a social image, a one-page marketing layout.
    Marketing,
    /// A video composition in HTML (HyperFrames).
    Video,
    /// Motion graphics.
    Motion,
}

impl Surface {
    pub const ALL: [Surface; 7] = [
        Surface::Any,
        Surface::Deck,
        Surface::Prototype,
        Surface::Document,
        Surface::Marketing,
        Surface::Video,
        Surface::Motion,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Surface::Any => "any design",
            Surface::Deck => "a deck",
            Surface::Prototype => "a prototype",
            Surface::Document => "a document",
            Surface::Marketing => "a marketing piece",
            Surface::Video => "a video",
            Surface::Motion => "motion graphics",
        }
    }

    /// Its name on the composer's chip.
    pub fn name(self) -> &'static str {
        match self {
            Surface::Any => "Design",
            Surface::Deck => "Deck",
            Surface::Prototype => "Prototype",
            Surface::Document => "Document",
            Surface::Marketing => "Marketing",
            Surface::Video => "Video",
            Surface::Motion => "Motion",
        }
    }

    /// What it makes, under its name in the composer's menu.
    pub fn description(self) -> &'static str {
        match self {
            Surface::Any => "Forge picks the kind of work from each request",
            Surface::Deck => "Slides to present: a talk, a pitch, a course",
            Surface::Prototype => "Screens to click through: an app, a site, a flow",
            Surface::Document => "A page to read: a report, a guide, a course page",
            Surface::Marketing => "A poster, a social image, a one-page layout",
            Surface::Video => "A video made in HTML (HyperFrames)",
            Surface::Motion => "Motion graphics: titles, transitions, explainers",
        }
    }

    /// The library's `mode` for the skills and templates that fit it
    /// (None: any).
    pub fn modes(self) -> Option<&'static [&'static str]> {
        match self {
            Surface::Any => None,
            Surface::Deck => Some(&["deck"]),
            Surface::Prototype => Some(&["prototype"]),
            Surface::Document => Some(&["prototype", "template", "utility"]),
            Surface::Marketing => Some(&["image"]),
            Surface::Video | Surface::Motion => Some(&["video"]),
        }
    }

    /// Open Design's task profile for it, in the library (Document has
    /// none: the charter and craft cover it).
    pub fn profile(self) -> Option<&'static str> {
        Some(match self {
            Surface::Deck => concat!(profiles_dir!(), "/ppt.md"),
            Surface::Prototype => concat!(profiles_dir!(), "/prototype.md"),
            Surface::Marketing => concat!(profiles_dir!(), "/marketing.md"),
            Surface::Video => concat!(profiles_dir!(), "/hyperframes.md"),
            Surface::Motion => concat!(profiles_dir!(), "/motion-design.md"),
            Surface::Any | Surface::Document => return None,
        })
    }

    /// The craft rules every run of this surface gets, besides the skill's.
    fn craft(self) -> &'static [&'static str] {
        match self {
            Surface::Deck => &["anti-ai-slop", "typography", "color"],
            Surface::Prototype => &[
                "anti-ai-slop",
                "typography",
                "color",
                "accessibility-baseline",
                "state-coverage",
            ],
            Surface::Document => &[
                "anti-ai-slop",
                "typography-hierarchy-editorial",
                "color",
                "accessibility-baseline",
            ],
            Surface::Marketing => &["anti-ai-slop", "typography", "color"],
            Surface::Video | Surface::Motion => &["anti-ai-slop", "animation-discipline"],
            Surface::Any => &["anti-ai-slop"],
        }
    }
}

const PROFILES: &str = profiles_dir!();

/// What a design chat was set up with. Every name is a library id.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Setup {
    #[serde(default)]
    pub surface: Surface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
}

/// Each part's most, in bytes (about a quarter of that in tokens).
const CHARTER_MAX: usize = 8_000;
const PROFILE_MAX: usize = 24_000;
const SYSTEM_MAX: usize = 40_000;
const CRAFT_MAX: usize = 16_000;
const SKILL_MAX: usize = 24_000;
/// The whole prompt's most: the parts' budgets with room for the headings.
#[cfg(test)]
const TOTAL_MAX: usize = CHARTER_MAX + PROFILE_MAX + SYSTEM_MAX + 3 * CRAFT_MAX + SKILL_MAX;

/// Forge's design charter: how a design is made in Forge.
const CHARTER: &str = "# Designing in Forge

This is a design chat. You make real, finished design work (decks, pages, documents, prototypes, posters, motion) as HTML files \
the user can open, click through and export. Forge's side panel shows the page while you write it and reloads it after every edit.

## How to work

- Unless the user names another format, the deliverable is one self-contained HTML file in the project (inline CSS and scripts; \
fonts and images beside it or from a CDN with a pinned version). Put it where the user asks, else under `designs/<short-name>/index.html`.
- Write the file with one Write call, top to bottom: `<head>` and the styles first, then the content in reading order. The panel \
renders what you have typed so far, so the user watches the page form. Change an existing page with Edit, not a rewrite.
- Decide the design before you write: audience, purpose, the one idea each section or slide carries, the type scale, the palette \
and the grid. State the assumptions you make in a line, and ask only what you cannot reasonably assume.
- Real content only: no lorem ipsum, no \"Your title here\", no invented numbers presented as facts. Where a fact is missing, say so \
in the reply rather than on the page.
- A deck is one HTML file with one `<section class=\"slide\">` per slide at 1920×1080, scaled to fit the window, with arrow-key and \
click navigation; each slide makes one point.

## Your design tools

- `design_search` and `design_read` find and read the design library: skills, templates, design systems, craft rules, task \
profiles. Read what a request needs before you start; never guess what a design system or template contains.
- `design_render` shows you the page as the user sees it (a picture of the page, or of slides n to m). `design_lint` checks it for \
placeholders, broken markup, missing files and content that overflows.

## Before you call it done

Render the page and look at the pictures, then run `design_lint`. Critique in two passes: design (hierarchy, spacing and \
alignment, contrast, nothing overflowing or clipped, consistent type and colour, a layout that holds when narrow) and content \
(accurate, complete, every part earns its place). Fix what you find and look again; stop when a pass finds nothing worth fixing, \
or after three rounds. Then tell the user in a line or two what you made, where it is, and what the review changed.

## The user's comments

The user can comment on a part of the page in Forge's panel, or draw on it. A comment arrives as a message naming the part (its \
tag, its text, a selector) and sometimes a picture with their marks. Change that part, keep the rest of the page as it is, and say \
what you changed.

## Exports

Forge saves pages as PDF itself. Don't claim a PPTX, PDF or image you didn't make: when the user asks for one you can't produce, \
say so and deliver the HTML.";

/// What Open Design's own words mean in Forge, for the profile that
/// follows (it is kept exactly as upstream wrote it).
fn host_note(lib: &Path) -> String {
    let frames = lib.join(PROFILES).join("prototype");
    format!(
        "The task profile below is Open Design's, kept as written. In Forge: \"Open Design\", \"OD\" and \"the host\" mean Forge; \
there is no planning turn and no production-ready marker, so do the work in this turn; ignore what it says about machine \
contracts, task state and host-side renders. Where it names `.od-frames/<file>`, copy that file from `{}` into a `.od-frames/` \
folder beside your page and use it from there. Forge's own exports replace \"product-side exports\".",
        frames.display()
    )
}

/// The prompt for a design chat set up with `setup`.
pub fn compose(ix: &Index, setup: &Setup) -> String {
    let mut out = vec![cut(CHARTER, CHARTER_MAX, None)];
    let read = |rel: &Path| std::fs::read_to_string(ix.root.join(rel)).ok();

    match setup.surface.profile() {
        Some(rel) => {
            if let Some(text) = read(Path::new(rel)) {
                out.push(format!(
                    "# Task profile: {}\n\n{}\n\n{}",
                    setup.surface.label(),
                    host_note(&ix.root),
                    cut(&text, PROFILE_MAX, Some(rel))
                ));
            }
        }
        None if setup.surface == Surface::Any => out.push(any_surface()),
        None => {}
    }

    let entry = |kind, id: &Option<String>| id.as_deref().and_then(|id| ix.get(kind, id));
    let skill = entry(Kind::Skill, &setup.skill);
    let template = entry(Kind::Template, &setup.template);

    if let Some(system) = entry(Kind::DesignSystem, &setup.system) {
        if let Some((text, rel)) = main_text(ix, system) {
            out.push(format!(
                "# Design system: {}\n\nUse this look for every page in this chat: its colours, type, spacing and components. \
Its tokens and assets are in `{}`.\n\n{}",
                system.title,
                ix.root.join(&system.dir).display(),
                cut(&text, SYSTEM_MAX, Some(&rel))
            ));
        }
    }

    // The surface's craft, then what the skill and the template ask for,
    // each rule once.
    let mut craft: Vec<String> = setup
        .surface
        .craft()
        .iter()
        .map(|s| s.to_string())
        .collect();
    for e in [skill, template].into_iter().flatten() {
        craft.extend(e.craft.iter().cloned());
    }
    let mut seen = BTreeSet::new();
    craft.retain(|c| seen.insert(c.clone()));
    let mut left = 3 * CRAFT_MAX;
    for id in craft {
        let Some((text, rel)) = ix.get(Kind::Craft, &id).and_then(|e| main_text(ix, e)) else {
            continue;
        };
        if left < 500 {
            out.push(format!(
                "# Craft: {id}\n\nRead it with design_read (craft, {id}) before you start."
            ));
            continue;
        }
        let part = cut(&text, CRAFT_MAX.min(left), Some(&rel));
        left = left.saturating_sub(part.len());
        out.push(format!("# Craft: {id}\n\n{part}"));
    }

    if let Some(skill) = skill {
        if let Some((text, rel)) = main_text(ix, skill) {
            out.push(format!(
                "# Skill: {}\n\nFollow this skill. Its files are in `{}`; read the ones it names with design_read or your file tools.\n\n{}",
                skill.title,
                ix.root.join(&skill.dir).display(),
                cut(&text, SKILL_MAX, Some(&rel))
            ));
        }
    }

    if let Some(t) = template {
        let mut s = format!(
            "# Template: {}\n\n{}\n\nStart from this template: copy its starting file into the project and adapt it to the request, \
keeping its structure, scripts and look. Its folder is `{}`.",
            t.title,
            t.description,
            ix.root.join(&t.dir).display()
        );
        if let Some(p) = &t.preview {
            s.push_str(&format!(
                " The starting file: `{}`.",
                ix.root.join(p).display()
            ));
        }
        if let Some((text, rel)) = main_text(ix, t) {
            s.push_str("\n\n");
            s.push_str(&cut(&text, SKILL_MAX, Some(&rel)));
        }
        out.push(s);
    }
    out.join("\n\n---\n\n")
}

/// With no surface chosen: which profile fits which request, to read
/// with `design_read` before starting.
fn any_surface() -> String {
    let mut s = String::from(
        "# Task profiles\n\nPick the profile that fits the request and read it with design_read (kind `profile`) before you start; \
it holds the craft for that kind of work. If none fits, the charter above is enough.\n",
    );
    for surface in Surface::ALL {
        if let Some(rel) = surface.profile() {
            let id = Path::new(rel)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            s.push_str(&format!("\n- `{id}`: {}", surface.label()));
        }
    }
    s
}

/// An entry's main text (its SKILL.md, DESIGN.md or craft file) and where
/// it is in the library.
fn main_text(ix: &Index, e: &Entry) -> Option<(String, String)> {
    let rel = e.skill.as_ref()?;
    let text = std::fs::read_to_string(ix.root.join(rel)).ok()?;
    Some((text, rel.to_string_lossy().replace('\\', "/")))
}

/// `text` within `max` bytes: whole when it fits, else cut at the last
/// paragraph that fits, saying where the rest is.
fn cut(text: &str, max: usize, rel: Option<&str>) -> String {
    let text = text.trim();
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = &text[..end];
    let head = head.rfind("\n\n").map_or(head, |i| &head[..i]);
    let rest = match rel {
        Some(rel) => format!("read the rest with design_read (path `{rel}`)"),
        None => "the rest is left out".to_string(),
    };
    format!("{head}\n\n[Shortened to fit: {rest}.]")
}

/// The task profiles `design_read` serves as kind `profile`: their ids
/// (`ppt`, `prototype`, …) and paths in the library.
pub fn profiles() -> Vec<(&'static str, &'static str)> {
    Surface::ALL
        .iter()
        .filter_map(|s| s.profile())
        .map(|rel| {
            let id = rel
                .rsplit('/')
                .next()
                .and_then(|f| f.strip_suffix(".md"))
                .unwrap_or(rel);
            (id, rel)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Option<Index> {
        super::super::root().map(|r| Index::load(&r))
    }

    #[test]
    fn every_surface_composes_within_budget() {
        let Some(ix) = library() else { return };
        for surface in Surface::ALL {
            let p = compose(
                &ix,
                &Setup {
                    surface,
                    ..Setup::default()
                },
            );
            assert!(p.starts_with("# Designing in Forge"), "{surface:?}");
            assert!(p.len() <= TOTAL_MAX, "{surface:?}: {}", p.len());
            assert!(p.contains("design_render") && p.contains("design_lint"));
            if surface.profile().is_some() {
                assert!(
                    p.contains("# Task profile:") && p.contains("kept as written"),
                    "{surface:?}"
                );
            }
            for craft in surface.craft() {
                assert!(
                    p.contains(&format!("# Craft: {craft}")),
                    "{surface:?} {craft}"
                );
            }
        }
    }

    #[test]
    fn a_full_setup_has_every_part_in_order() {
        let Some(ix) = library() else { return };
        let setup = Setup {
            surface: Surface::Deck,
            system: Some("apple".into()),
            template: Some("html-ppt-course-module".into()),
            skill: Some("frontend-slides".into()),
        };
        let p = compose(&ix, &setup);
        let at = |s: &str| p.find(s).unwrap_or_else(|| panic!("no {s}"));
        let order = [
            at("# Designing in Forge"),
            at("# Task profile: a deck"),
            at("# Design system: Apple"),
            at("# Craft: anti-ai-slop"),
            at("# Skill:"),
            at("# Template:"),
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        assert!(p.len() <= TOTAL_MAX, "{}", p.len());
        // The same setup gives the same prompt: caching holds.
        assert_eq!(p, compose(&ix, &setup));
    }

    #[test]
    fn unknown_names_are_left_out() {
        let Some(ix) = library() else { return };
        let p = compose(
            &ix,
            &Setup {
                surface: Surface::Document,
                system: Some("no-such-system".into()),
                ..Setup::default()
            },
        );
        assert!(!p.contains("# Design system:") && !p.contains("# Task profile:"));
    }

    #[test]
    fn every_profile_is_in_the_library() {
        let Some(ix) = library() else { return };
        let ids: Vec<_> = profiles().iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            [
                "ppt",
                "prototype",
                "marketing",
                "hyperframes",
                "motion-design"
            ]
        );
        for (_, rel) in profiles() {
            assert!(ix.root.join(rel).is_file(), "{rel}");
        }
    }

    #[test]
    fn cut_keeps_whole_paragraphs() {
        let text = "one one\n\ntwo two\n\nthree three";
        assert_eq!(cut(text, 100, None), text);
        let c = cut(text, 18, Some("x.md"));
        assert!(c.starts_with("one one\n\ntwo two\n\n[Shortened"), "{c}");
        assert!(c.contains("`x.md`"));
    }

    #[test]
    fn setup_round_trips() {
        let s = Setup {
            surface: Surface::Prototype,
            system: Some("linear".into()),
            ..Setup::default()
        };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, r#"{"surface":"prototype","system":"linear"}"#);
        assert_eq!(serde_json::from_str::<Setup>(&j).unwrap(), s);
        assert_eq!(
            serde_json::from_str::<Setup>("{}").unwrap(),
            Setup::default()
        );
    }
}
