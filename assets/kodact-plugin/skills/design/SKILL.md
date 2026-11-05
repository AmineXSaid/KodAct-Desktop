---
name: design
description: Make finished design work as HTML that the user watches form in KodAct's side panel - a slide deck, a landing or product page, a report or guide, a course page, an app prototype, a poster or social image, motion. Use when the user asks you to design, lay out, style or restyle something visual, or to make a deck, slides, a page, a mockup or a prototype. Not for diagrams (diagram-design) or Word, Excel and PowerPoint files (office-documents).
---

# Design in KodAct

You make real, finished design work as HTML pages. KodAct opens each page you
write in its side panel and shows it forming while you type it, then reloads
it after every edit. The user can comment on any part of it, or draw on it,
and those comments come back to you.

## 1. Use the design tools when you have them

In a design chat KodAct gives you `design_search`, `design_read`,
`design_render`, `design_lint`, `design_export` and `design_capture` (as
`mcp__kodact-design__…`), and its design
prompt with the craft for this kind of work.

- `design_search` the library for a skill, template or design system that
  fits, and `design_read` it before you start. Never guess what one contains.
- `design_render` the page to see it as the user does (a page, or slides n to
  m), and `design_lint` it before you call it done.
- When the user wants a file, `design_export` it: PDF, PNG, PowerPoint
  (pictures, or editable text) or one self-contained HTML file.
- For a design in a site's look, `design_capture` the site: it writes the
  brand's `DESIGN.md` and `tokens.css` into the project to design with.
- `design_image` makes pictures for the page with the user's own key;
  `design_deploy` publishes it when they ask for a link.

Without those tools, this is an ordinary chat. Say once, in a line, that
picking **Design** in the composer (beside the project) makes it a design
chat, with a library of design systems, templates and checks. Then do the
work with what you have: the rules below, and KodAct's browser tools to look
at the page when they are on.

## 2. Decide before you write

Settle these first and state your assumptions in a line; ask only what you
can't reasonably assume:

- **Who and why.** The audience, the one thing they should take away, and
  where it will be seen (a projector, a laptop, a phone).
- **Structure.** For a deck, a proven arc (problem, insight, proposal,
  evidence, ask) with one point per slide. For a page, the reader's order of
  questions. For a prototype, the screens and the path through them.
- **The system.** A type scale (one display face, one text face, 4 to 6
  sizes), a palette (one accent, neutrals, a meaning for each colour), a
  spacing step (4 or 8 px) and a grid. Keep to it everywhere.

## 3. Write it

- One self-contained HTML file unless the user names another format: inline
  CSS and scripts, local images beside it, any CDN library at a pinned
  version (`gsap@3.12.5`, never `@latest`).
- Put it where the user asks, else `designs/<short-name>/index.html`.
- Write it with one Write call, top to bottom: `<head>` and styles first,
  then the content in reading order, so the panel shows it forming. Change
  an existing page with Edit, not a rewrite.
- A deck: one `<section class="slide">` per slide, 1920×1080, scaled to fit
  the window, with arrow keys and click to move between slides.
- Real content only: no lorem ipsum, no "Your title here", no invented facts
  shown as real. Missing facts go in your reply, not on the page.
- Accessible by default: text contrast at least 4.5:1, `lang`, a `<title>`,
  a viewport meta, alt text, visible focus, and motion that respects
  `prefers-reduced-motion`.

## 4. Avoid what makes it look generated

- No gradient blobs, glassy cards everywhere, or purple-to-blue by default.
- No emoji as icons, no stock "rocket" metaphors, no three identical cards
  as the answer to every section.
- Vary the rhythm: a dense section next to an open one, one strong image or
  number rather than many weak ones.
- Align to the grid; let whitespace group things instead of boxes.

## 5. Look at it, then fix it

Render it (`design_render`, or the browser tools' screenshot of the
`file://` page) and look at the pictures. Lint it if you can. Then critique
in two passes:

1. **Design:** hierarchy (one clear title, headings that step down), spacing
   and alignment, contrast, nothing overflowing, clipped or overlapping,
   consistent type and colour, and a layout that holds when narrow.
2. **Content:** accurate, complete, every part earns its place.

Fix what you find and look again. Stop when a pass finds nothing worth
fixing, or after three rounds. Then tell the user in a line or two what you
made, where it is, and what the review changed.

## 6. Comments from the panel

A comment names the part it's on (its tag, the start of its text, a
selector) and sometimes comes with a picture of the user's marks. Change
that part and keep everything else as it is. Say what you changed.

## 7. Exports

KodAct saves any page as PDF from the panel's Share menu. Don't claim a PPTX,
PDF or image you didn't make. When the user asks for one you can't produce,
say so and deliver the HTML.
