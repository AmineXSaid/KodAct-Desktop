//! `.interface-design/system.md`, written from `theme.rs`.
//!
//! The design skills read that file as the system's tokens; the app reads
//! `theme.rs`. So the file is generated, and this test fails when the two
//! disagree. After changing a token, rewrite it with
//! `UPDATE_SYSTEM_MD=1 cargo test system_md`.

use std::fmt::Write as _;

use gpui::{Hsla, Pixels};

use crate::palette_check::{contrast, oklab, rgba};
use crate::theme::{Look, Theme};

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.interface-design/system.md");
/// `DESIGN.md` (the DESIGN.md format, read by the impeccable skill), from the
/// same tokens: frontmatter for machines, eight sections for people.
const DESIGN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/DESIGN.md");

fn hex(c: Hsla) -> String {
    let c = rgba(c);
    let b = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    let base = format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b));
    if c.a >= 0.999 {
        base
    } else {
        format!("{base} @ {:.1}%", c.a * 100.)
    }
}

/// A colour as CSS: hex when opaque, `rgba()` when not (DESIGN.md wants CSS).
fn css(c: Hsla) -> String {
    let c = rgba(c);
    let b = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    if c.a >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
    } else {
        format!("rgba({}, {}, {}, {:.2})", b(c.r), b(c.g), b(c.b), c.a)
    }
}

fn px(p: Pixels) -> String {
    let v = f32::from(p);
    if v.fract() == 0. {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

fn render() -> String {
    let (l, d) = (Theme::light(), Theme::dark());
    let mut s = String::new();
    let w = &mut s;
    let _ = writeln!(w, "# Design System: KodAct desktop (kodact-desktop)\n");
    let _ = writeln!(w, "> Generated from `src/theme.rs` by `src/system_md.rs`. Change a token in theme.rs, then run");
    let _ = writeln!(
        w,
        "> `UPDATE_SYSTEM_MD=1 cargo test system_md`; `cargo test` fails while this file is stale."
    );
    let _ = writeln!(
        w,
        "> Every colour pair below is held to its floor by `src/palette_check.rs`.\n"
    );

    let _ = writeln!(w, "## Direction\n");
    let _ = writeln!(w, "**Personality:** calmly smooth: a warm editorial workspace, confident and unhurried, closer to a printed quarterly than a dashboard (after the Anchor style reference).\n");
    let _ = writeln!(w, "**Foundation:** KodAct burgundy on neutral greys (after Postman's, for easy reading). By day: the window and sidebar #efefef, the session #f7f7f7, cards white; type #212121, secondary #616161. At night: the session charcoal #212121, the sidebar and title bar #282828, cards #303030; type white, secondary #b0b0b0. One colour, burgundy: #7a1f3d (#e0889f at night) for the mark, Send and the other main actions (Save, Allow), a waiting dot and the Act of the wordmark; its wash is #f3dde3 (#3a1a24 at night). Links (underlined), focus, selection, switches that are on, all other buttons and work in progress are ink: #1a1816 (#edeae6 at night). Errors are a brick red, #b82a1a (#ff8a7a at night), held apart from the burgundy. Everything else is neutral; colour otherwise only for meaning (modes, status).\n");
    let _ = writeln!(w, "**Depth:** nearly flat. Hairlines inside cards; the session and the user's message lift on paper grain (three 1px shadows at 6/4/1% ink); popovers and the permission card a touch more.\n");
    let _ = writeln!(w, "**Shape:** two shapes. Everything you press is a pill (buttons, chips, icon buttons, the new-chat and needs-you pills); everything that holds content is 4px (cards, rows, menus, fields, the composer, the session).\n");
    let _ = writeln!(w, "**Type:** Anthropic Sans only, the Text cut for every word (code included) and the Display cut for the greeting and headings: 400 for text, 500 for labels, 600 for headings and buttons. Small labels in sentence case, never uppercase and never above a heading. The app bundles no font; diagrams are redrawn in it whatever they name.\n");

    let _ = writeln!(w, "## Tokens\n");
    let sp = &l.space;
    let _ = writeln!(
        w,
        "### Spacing\nBase: 4px\nScale: {}, {}, {}, {}, {}, {}, {}, {}, {}, {}\n",
        px(sp.xs),
        px(sp.tight),
        px(sp.sm),
        px(sp.snug),
        px(sp.md),
        px(sp.lg),
        px(sp.loose),
        px(sp.xl),
        px(sp.xxl),
        px(sp.xxxl)
    );
    let r = &l.radius;
    let z = &l.size;
    let _ = writeln!(w, "### Radius\nTwo shapes: 4px for everything that holds content, pill for everything you press.\nScale: {}px (a line's ends), {}px (key caps), {}px (rows and squarer buttons), {}px (cards, tool rows, menus), {}px (fields), {}px (the composer), pill (buttons, chips, icon buttons, segments, the send button, switches, badges, avatars)\n", px(r.line), px(r.small), px(r.button), px(r.card), px(r.field), px(z.composer_radius));

    let _ = writeln!(w, "### Typography");
    let _ = writeln!(
        w,
        "Interface: {} in 400 and 600 only, bundled (OFL); headings in 600",
        l.font_sans
    );
    let _ = writeln!(
        w,
        "Display: {} 600 (the greeting, headings)\nCode: {}",
        l.font_display, l.font_mono
    );
    let tx = &l.text;
    let _ = writeln!(
        w,
        "Scale: {} caption, {} UI and chat, {} input, {} subheading, {} heading, {} greeting, {} home heading",
        px(tx.caption),
        px(tx.body_sm),
        px(tx.body),
        px(tx.subheading),
        px(tx.heading_sm),
        px(z.greeting),
        px(z.home_heading)
    );
    let _ = writeln!(w, "Weights: 400, 500 (labels, buttons), 600 (headings, the project name)\nProse line height: {}\n", z.line_height_chat);

    let _ = writeln!(
        w,
        "### Icons\nLucide, 2px stroke as drawn. Sizes: {} / {} / {} / {} only.\n",
        px(z.icon_xs),
        px(z.icon_sm),
        px(z.icon),
        px(z.icon_lg)
    );

    let _ = writeln!(w, "### Sizes");
    for (name, v, role) in [
        ("titlebar", z.titlebar, "the title bar's height"),
        ("sidebar", z.sidebar, "the sidebar's width"),
        ("column", z.column, "the reading column"),
        ("menu_row", z.menu_row, "a row in a chat's menu"),
        (
            "session_panel",
            z.session_panel,
            "the Session column beside a chat: environment, context, changes",
        ),
        (
            "composer_line",
            z.composer_line,
            "the composer box with one line in it: +, the text, send",
        ),
        (
            "menu_width",
            z.menu_width,
            "a chat menu's width, and its flyouts' least",
        ),
        (
            "thumb_w",
            z.thumb_w,
            "a picture a call returned, as a thumbnail at the end of its line",
        ),
        (
            "thumb_h",
            z.thumb_h,
            "that thumbnail's height, inside the quiet line",
        ),
        (
            "tool_image",
            z.tool_image,
            "that picture's widest, in the call's opened body",
        ),
        ("composer_width", z.composer_width, "the composer's widest"),
        ("row", z.row, "sidebar rows, menu items"),
        (
            "control",
            z.control,
            "small buttons, pills, title-bar controls",
        ),
        ("palette", z.palette, "the command palette's width"),
        ("hairline", z.hairline, "every border"),
        (
            "doc_pane",
            z.doc_pane,
            "the document panel's narrowest (it takes half the session)",
        ),
        ("doc_measure", z.doc_measure, "a document's widest line"),
        (
            "terminal_text",
            z.terminal_text,
            "the terminal dock's monospace",
        ),
        (
            "terminal_min",
            z.terminal_min,
            "the terminal dock's narrowest",
        ),
        (
            "chat_min",
            z.chat_min,
            "the chat's narrowest: the side panel, then the dock, give way to it",
        ),
        (
            "remote_menu",
            z.remote_menu,
            "the remote quick pick's width",
        ),
        ("design_menu", z.design_menu, "the composer's design menu's width"),
        (
            "design_menu_list",
            z.design_menu_list,
            "the design menu's list at its tallest: every kind of work",
        ),
        (
            "edge_bar",
            z.edge_bar,
            "the jewel edge a window draws as it opens",
        ),
        ("hit_sm", z.hit_sm, "an icon button in a dense row"),
        ("hit", z.hit, "an icon button's square"),
        ("hit_lg", z.hit_lg, "the composer's outlined + and send"),
        ("button_h", z.button_h, "a labelled button"),
        ("button_sm", z.button_sm, "a compact button, a selector"),
        (
            "switch_w",
            z.switch_w,
            "a switch's track (20px tall, 14px thumb)",
        ),
        ("rail", z.rail, "the collapsed sidebar"),
        (
            "mark_home",
            z.mark_home,
            "the mark beside the home greeting",
        ),
        (
            "panel_header",
            z.panel_header,
            "every right-side panel's title row (title 14 semibold)",
        ),
        (
            "panel_toolbar",
            z.panel_toolbar,
            "the file view's second row (labels 12 medium)",
        ),
        (
            "panel_footer",
            z.panel_footer,
            "the file view's status line",
        ),
        ("tool_btn", z.tool_btn, "a toolbar button"),
        ("thumb_rail", z.thumb_rail, "the thumbnail rail"),
    ] {
        let _ = writeln!(w, "- `{name}`: {}px, {role}", px(v));
    }
    let _ = writeln!(w);

    let _ = writeln!(w, "### Surfaces\nThe cream stack: the title bar and the sidebar share the canvas, the session steps up from it, cards step up again (`palette_check.rs`).\n");
    let _ = writeln!(
        w,
        "| Token | Light | L | Dark | L | Role |\n|---|---|---|---|---|---|"
    );
    for (name, a, b, role) in [
        ("chrome", l.extra.chrome, d.extra.chrome, "the title bar"),
        ("pane", l.extra.pane, d.extra.pane, "the sidebar"),
        (
            "stage",
            l.extra.stage,
            d.extra.stage,
            "the session: chat, welcome, Rules, Projects",
        ),
        (
            "surface",
            l.colors.surface,
            d.colors.surface,
            "cards, the composer, popovers",
        ),
        (
            "panel",
            l.extra.panel,
            d.extra.panel,
            "every right-side surface: side bar, Session column, file view",
        ),
        (
            "selected",
            l.extra.selected,
            d.extra.selected,
            "the selected sidebar row",
        ),
    ] {
        let _ = writeln!(
            w,
            "| `{name}` | `{}` | {:.3} | `{}` | {:.3} | {role} |",
            hex(a),
            oklab(rgba(a)).0,
            hex(b),
            oklab(rgba(b)).0
        );
    }
    let _ = writeln!(w);

    let _ = writeln!(w, "### Theme\nOne look, Calmly smooth, in light and dark (`window.json` keeps the mode). Every face is Anthropic Sans: Text for every word (interface, replies, code, the terminal, diagrams), Display for the large lines (`fonts.rs`). It passes `palette_check.rs` in both modes.\n");
    let (a, b) = (Theme::of(Look::Calm, false), Theme::of(Look::Calm, true));
    let (r, z) = (&a.radius, &a.size);
    let _ = writeln!(
        w,
        "| | Light | Dark |\n|---|---|---|\n| Canvas / session / accent | `{}` / `{}` / `{}` | `{}` / `{}` / `{}` |\n",
        hex(a.extra.pane),
        hex(a.extra.stage),
        hex(a.extra.accent),
        hex(b.extra.pane),
        hex(b.extra.stage),
        hex(b.extra.accent)
    );
    let _ = writeln!(
        w,
        "Faces (UI / display / code / replies): {} / {} / {} / {}. Corners button · card · field · composer · pressed: {} · {} · {} · {} · {}. Row · title bar · chat: {} · {} · {}.\n",
        a.font_sans,
        a.font_display,
        a.font_mono,
        a.font_reading,
        px(r.button),
        px(r.card),
        px(r.field),
        px(z.composer_radius),
        if f32::from(r.compact) >= 999. {
            "pill".to_string()
        } else {
            px(r.compact)
        },
        px(z.row),
        px(z.titlebar),
        px(z.chat_text),
    );

    let _ = writeln!(
        w,
        "### Terminal\nThe dock's 16 colours, on `stage` (`palette_check.rs` holds them to AA).\n"
    );
    let _ = writeln!(w, "| # | Light | Dark |\n|---|---|---|");
    for i in 0..16 {
        let _ = writeln!(
            w,
            "| {i} | `{}` | `{}` |",
            hex(l.extra.ansi[i]),
            hex(d.extra.ansi[i])
        );
    }
    let _ = writeln!(w);

    let _ = writeln!(w, "### Colors\nContrast is the WCAG ratio of text on `stage`, of a colour on `surface`, or of white text on a fill.\n");
    let _ = writeln!(
        w,
        "| Token | Light | Contrast | Dark | Contrast | Role |\n|---|---|---|---|---|---|"
    );
    // Each row: the token, then the pair whose contrast is shown.
    type Pick = fn(&Theme) -> Hsla;
    let rows: [(&str, Pick, Pick, Pick, &str); 25] = [
        (
            "text_strong",
            |t| t.colors.text_strong,
            |t| t.colors.text_strong,
            |t| t.extra.stage,
            "headings, names",
        ),
        (
            "text",
            |t| t.colors.text,
            |t| t.colors.text,
            |t| t.extra.stage,
            "body text",
        ),
        (
            "text_body",
            |t| t.colors.text_body,
            |t| t.colors.text_body,
            |t| t.extra.stage,
            "secondary prose",
        ),
        (
            "text_muted",
            |t| t.colors.text_muted,
            |t| t.colors.text_muted,
            |t| t.extra.stage,
            "labels, captions, icons",
        ),
        (
            "text_subtle",
            |t| t.colors.text_subtle,
            |t| t.colors.text_subtle,
            |t| t.extra.stage,
            "placeholders and disabled only",
        ),
        (
            "border",
            |t| t.colors.border,
            |t| t.colors.border,
            |t| t.extra.stage,
            "hairlines",
        ),
        (
            "border_strong",
            |t| t.extra.border_strong,
            |t| t.extra.border_strong,
            |t| t.colors.surface,
            "outlined buttons, a switch at rest",
        ),
        (
            "primary",
            |t| t.colors.primary,
            |t| t.colors.primary,
            |t| t.colors.surface,
            "links, selected text, the focus edge",
        ),
        (
            "accent",
            |t| t.extra.accent,
            |t| t.extra.accent_fg,
            |t| t.extra.accent,
            "send, the new-chat mark, accent dots (white icon on it)",
        ),
        (
            "fill",
            |t| t.extra.fill,
            |t| t.extra.fill_fg,
            |t| t.extra.fill,
            "filled buttons: ink on paper, paper on charcoal",
        ),
        (
            "toggle_on",
            |t| t.extra.toggle_on,
            |t| t.extra.toggle_on,
            |t| t.colors.surface,
            "a switch that is on",
        ),
        (
            "mark_top",
            |t| t.extra.mark_top,
            |t| t.extra.mark_top,
            |t| t.extra.stage,
            "KodAct's mark, both strokes (`ui/kodact_mark.rs`)",
        ),
        (
            "page_ink",
            |t| t.extra.page_ink,
            |t| t.extra.page_ink,
            |t| t.extra.page_halo,
            "marks drawn on a design page, on their halo (`ui/design_marks.rs`)",
        ),
        (
            "mark_heat_white",
            |t| t.extra.mark_heat_white,
            |t| t.extra.mark_heat_white,
            |t| t.extra.stage,
            "the mark at its brightest as it arrives (onboarding); never at rest",
        ),
        (
            "mark_heat",
            |t| t.extra.mark_heat,
            |t| t.extra.mark_heat,
            |t| t.extra.stage,
            "the mark's bright signal as it settles, and a push's flare",
        ),
        (
            "mark_heat_ember",
            |t| t.extra.mark_heat_ember,
            |t| t.extra.mark_heat_ember,
            |t| t.extra.stage,
            "the mark's deep burgundy as it settles",
        ),
        (
            "mark_heat_iron",
            |t| t.extra.mark_heat_iron,
            |t| t.extra.mark_heat_iron,
            |t| t.extra.stage,
            "the mark near dark, last before its own colour",
        ),
        (
            "fill_danger",
            |t| t.extra.fill_danger,
            |t| t.colors.primary_fg,
            |t| t.extra.fill_danger,
            "destructive confirms",
        ),
        (
            "success",
            |t| t.extra.success,
            |t| t.extra.success,
            |t| t.colors.surface,
            "done, passed",
        ),
        (
            "danger",
            |t| t.extra.danger,
            |t| t.extra.danger,
            |t| t.colors.surface,
            "failed, errors",
        ),
        (
            "warning",
            |t| t.extra.warning,
            |t| t.extra.warning,
            |t| t.colors.surface,
            "needs you",
        ),
        (
            "memory",
            |t| t.extra.memory,
            |t| t.extra.memory,
            |t| t.extra.memory_tint,
            "memory and rules, on its tint",
        ),
        (
            "skill",
            |t| t.extra.skill,
            |t| t.extra.skill,
            |t| t.colors.surface,
            "a skill named in the composer, bold",
        ),
        (
            "mode_plan",
            |t| t.extra.mode_plan,
            |t| t.extra.mode_plan,
            |t| t.colors.surface,
            "Plan mode",
        ),
        (
            "mode_bypass",
            |t| t.extra.mode_bypass,
            |t| t.extra.mode_bypass,
            |t| t.colors.surface,
            "Bypass permissions",
        ),
    ];
    let ratio = |t: &Theme, fg: Pick, bg: Pick| format!("{:.1}:1", contrast(fg(t), bg(t)));
    for (name, token, fg, bg, role) in rows {
        let _ = writeln!(
            w,
            "| `{name}` | `{}` | {} | `{}` | {} | {role} |",
            hex(token(&l)),
            ratio(&l, fg, bg),
            hex(token(&d)),
            ratio(&d, fg, bg)
        );
    }
    let _ = writeln!(w);

    let _ = writeln!(w, "### Contrast floors\n- Meaningful text: 4.5:1 on every surface and hover wash.\n- Icons, mode edges, the send arrow: 3:1.\n- Placeholders and disabled text (`text_subtle`): 2.5:1, never body copy.\n- Hairlines: 1.15:1 on every surface.\n- Surface steps: at least 0.02 OKLab L apart, each within 0.004 of the mean step.\n");

    let _ = writeln!(w, "## Patterns\n");
    let _ = writeln!(w, "### Buttons (`ui/button.rs`)\nOne family; every control takes keyboard focus (a `primary` edge shows for the keyboard only), names itself to assistive technology, and swallows clicks while disabled or loading.\n- Primary: {}px tall, padding 0 14px, radius {}px, `fill` with `fill_fg`, 500 weight. One per surface.\n- Secondary and Outlined: `surface`, 1px `border_strong`, `hover` wash.\n- Ghost: clear until hovered. Danger: `fill_danger`, white.\n- Icon: {}px square, radius {}px, a {}px icon in `text_muted`; outlined squares {}px at {}px radius.\n- Send: {}px, radius {}px, `accent` with a white arrow; `accent_soft` with nothing to send; Stop is an ink ring round a square.\n- Switch: {}x{}px pill, {}px thumb sliding over {}ms; `toggle_on` when on, `border_strong` when off.\n- Segmented: a `sunk` track, radius {}px; the chosen segment raised onto `surface` at {}px.\n", px(z.button_h), px(r.button), px(z.hit), px(r.button), px(z.icon), px(z.hit_lg), px(r.control), px(z.hit_lg), px(r.control), px(z.switch_w), px(z.switch_h), px(z.switch_thumb), z.pop_in_ms, px(r.button), px(r.segment));
    let _ = writeln!(w, "### Composer\n- Width: up to {}px, centred\n- One unit: a white box (radius {}px, 1px `border`, `border_strong` while focused, tinted by the mode when one is set) resting in a `sunk` tray\n- Text: {}px; the outlined + bottom left and Send bottom right, {}px from the box's edges\n- The tray: the project, the context ring, the model and the mode as quiet {}px selectors: a 14px icon, the label, a chevron that flips while open; washed on hover, raised onto `surface` with a hairline while open\n", px(z.composer_width), px(z.composer_radius), px(tx.body), px(sp.snug), px(z.button_sm));
    let _ = writeln!(w, "### Card Default\n- Border: 1px `border`\n- Padding: {}px\n- Radius: {}px\n- Background: `surface`\n- Usage: tool calls, file cards, code blocks\n", px(sp.xl), px(r.card));
    let _ = writeln!(w, "### Popover\n- Border: 1px `border`, radius {}px, padding {}px\n- Background: `surface`, with the two-layer popover shadow\n- Usage: menus, pickers, the command palette\n", px(r.card), px(sp.xs));
    let _ = writeln!(w, "### File Card\n- A file KodAct made, in its reply: up to 440px, `surface`, 1px `border`, radius {}px, padding {}px\n- A 36x44 page tile (radius {}px, concentric with the card) previews it, its type in mono `primary`\n- Title 600 `text_strong`; \"Document · MD\" caption `text_muted`; a bordered Download button\n- Open: the edge turns `primary` at 60%\n", px(r.card), px(sp.sm), px(r.small));
    let _ = writeln!(w, "### Document Pane\n- Beside the conversation, half the session, at least {}px; `surface` with a hairline on its left\n- Header 48px: file icon, title, type; Copy, Download, Close as {}px quiet buttons\n- Text {}px at {} leading, lines up to {}px; the title in the display serif at {}px\n- Enters with a 220ms ease-out, fading in from 16px to the right; Escape closes it\n", px(z.doc_pane), px(z.control), px(z.doc_text), z.line_height_doc, px(z.doc_measure), px(z.doc_title));
    let _ = writeln!(w, "### Sidebar Row\n- Height: {}px, radius {}px\n- Current: the `active` wash, 500 weight, no border\n- New chat: a round `accent` mark with a white +\n- Collapsed: a {}px rail of icon buttons for the same actions\n- Group labels: {}px caption\n", px(z.row), px(r.button), px(z.rail), px(tx.caption));

    let _ = writeln!(w, "## Decisions\n");
    let _ = writeln!(
        w,
        "| Decision | Rationale | Date |\n|----------|-----------|------|"
    );
    for (what, why, when) in [
        ("One colour, moss, not terracotta", "The reference rations one warm orange to the primary action; KodAct keeps the rationing and takes the reference's own moss, calmer on cream and kin to the mark.", "2026-10-18"),
        ("Dusty blue, not moss", "Moss read as too green beside the cream; a dusty blue from the Slate theme's family, warmed for cream, keeps the same rationing and contrast (5.4:1 on the session).", "2026-10-19"),
        ("Paper on paper", "The session is a 4px sheet on the cream canvas with paper grain; the title bar and sidebar have no dividers, the sheet's edge is the split.", "2026-10-18"),
        ("Pills to press, 4px to hold", "The contrast between the two shapes is the system: it says at a glance what acts and what holds.", "2026-10-18"),
        ("Ink pills for the loud neutrals", "New chat and needs-you are ink pills; the accent is kept for the one decision in front of you (Allow once, Send).", "2026-10-18"),
        ("Anthropic Sans only", "One family everywhere, in its Text and Display cuts: the interface, replies, code, the terminal and diagrams. No other face is bundled or named.", "2026-10-26"),
        ("One look", "The theme picker and its eleven other looks are gone; Calmly smooth, light and dark, is the app's one look.", "2026-10-26"),
        ("No generic AI icons", "No sparkles, magic wands, robot heads or brains: each icon names its thing. Design is pencil-ruler (the Pen tool keeps pen-tool), skills a scroll, agents boxes (KodAct's cubes, many), memory a notebook, effort a gauge; thinking shows KodAct's own strike mark.", "2026-11-02"),
        ("Three durations", "Every transition takes one of three times: fast 120ms (anything leaving, a menu arriving, the edge draw), base 200ms (hovers, dialogs and notices arriving, the panel leaving), slow 280ms (the side panel arriving). Loops (the strike mark, a working node) keep their own beat.", "2026-11-02"),
        ("The heat rail", "KodAct's signature: a run's opened steps hang on one rail, a node each in the shared status language (ember working, ink done, danger failed or declined, warning stopped); while the run is live the rail glows ember and the working node breathes, then it cools to a hairline (`heat_rail.rs`).", "2026-11-02"),
        ("One control per kind of choice", "One of a few short options is a segmented control (`editor_kit::choice`); one of several options that each need a line of description is a radio list; chips are for picking several, or one from a long list. Where a setting is kept reads the same everywhere: This project, just me · This project, shared · All my projects.", "2026-11-02"),
        ("KodAct Ember", "Warm paper and warm graphite, the sidebar a step deeper than the session, and one copper heat for the action, the current state and links: the forge's own colour, as calm as Claude Code's.", "2026-10-23"),
        ("KodAct Signal", "The app takes KodAct's identity: its mark (a step, then a chevron) in place of the ingot, warm ink and paper, and two colours with separate jobs. Turquoise is rationed to the mark, Send, the one decisive action and a waiting dot; peach, the terminal's warm accent, is for links, focus, selection and switches; buttons and work in progress are ink. It replaces KodAct Ember, and an all-turquoise first pass that put the accent on every control.", "2026-11-11"),
        ("Postman's greys", "The warm paper and the blue-black obsidian give way to plain neutral greys, after Postman's, so everything reads easily: by day #efefef / #f7f7f7 / white, at night #212121 / #282828 / #303030 with white type and #b0b0b0 secondary (a touch brighter than Postman's #a6a6a6, to keep AA on hovered and selected rows). Burgundy stays the one colour.", "2026-11-20"),
        ("KodAct burgundy", "One colour replaces turquoise and peach: burgundy for the mark (`./<`), Send, filled buttons and the decision in front of you, and a lighter burgundy for links, focus, selection and switches. The Windows icon and the brand SVG carry the same burgundy; the startup film and the onboarding orbit are gone, and the working mark types, runs and clears `./<` like a line at a prompt.", "2026-11-20"),
        ("Burgundy, and ink for the rest", "The accent is `#7a1f3d` (`#e0889f` at night) and keeps only the main action, the mark and a waiting dot. Links (underlined), focus, selection and switches become ink, so burgundy is the only colour on screen; errors move to a brick red held at least 25 degrees of hue away from it. Every right-side panel shares one header (44px), one background (`panel`) and one chrome (`ui/panel_chrome.rs`).", "2026-11-23"),
        ("One panel choreography", "Opening, the side panel's width slides in (slow, 280ms) and its content rises 8px and fades in 40ms behind it; closing, the content fades first (fast, 120ms) and then the width closes (base, 200ms). A file opened from the Files tab is pushed in 24px from the right over the list, which dims back; going back reverses it. A tab switch cross-fades with a 6px shift toward the tab, under an ink underline that slides between tabs. Moving the panel below the chat slides it up into its share. Full screen has no shimmer bar. Reduced motion keeps every end state with no movement.", "2026-11-23"),
        ("A neutral wash for the open row", "The accent stays for the action; the open chat and the open screen lift by a grey wash.", "2026-10-23"),
        ("Composer at 620px", "A comfortable measure for a prompt, the same in a chat and on the welcome page.", "2026-10-11"),
        ("Composer on the reading column", "In a chat the composer takes the 760px column the reply is read in, so the two line up; on the welcome page it keeps its own measure.", "2026-10-23"),
        ("No name over a reply", "The user's bubble already separates the turns; a name and mark over every reply only repeated it.", "2026-10-23"),
        ("Deny is quiet", "A permission has one filled action, Allow once; Deny is a ghost button, a way out rather than an alarm.", "2026-10-23"),
        ("Every window leaves as it came", "Each group has its own motion, in and out (`ui/motion.rs`): dialogs rise 14px over a dimming backdrop and sink away (200/120ms); the palette drops from above and lifts away; menus at an anchor unfold and fade back toward it (120ms); the composer's menus draw their jewel edge and fade back down; questions draw their edge in their colour and sink away; side panels and the dock slide in and back out (280/200ms); a notice springs up and drops away. A window that closes is kept a beat (`Presence`) so it leaves rather than vanishes.", "2026-10-25"),
        ("Replies at 15/24, rows 8 apart", "Calm sets a reply at 15px on 24px lines and keeps a reply's rows (text, tool lines, cards) 8px apart, as Claude Code does: a third more on screen, read as a tool rather than a document. Paragraphs keep their own gap.", "2026-10-24"),
        ("Icons on a 12/14/16/20 scale", "Desktop density: 14 in rows, 16 in controls, 20 for the composer's +; 12 inside captions.", "2026-10-12"),
    ] {
        let _ = writeln!(w, "| {what} | {why} | {when} |");
    }
    s
}

/// DESIGN.md: the default theme's tokens as frontmatter, then the system
/// in words, in the format's eight sections.
fn render_design() -> String {
    let (l, d) = (Theme::light(), Theme::dark());
    let mut s = String::new();
    let w = &mut s;
    let q = |c: Hsla| format!("\"{}\"", css(c));
    let _ = writeln!(w, "---");
    let _ = writeln!(w, "name: KodAct");
    let _ = writeln!(
        w,
        "description: The desktop app for KodAct CLI: a calm workspace for long agent sessions"
    );
    let _ = writeln!(w, "colors:");
    for (name, light, dark) in [
        ("canvas", l.colors.canvas, d.colors.canvas),
        ("surface", l.colors.surface, d.colors.surface),
        ("sidebar", l.colors.sidebar, d.colors.sidebar),
        ("ink-strong", l.colors.text_strong, d.colors.text_strong),
        ("ink", l.colors.text, d.colors.text),
        ("ink-body", l.colors.text_body, d.colors.text_body),
        ("ink-muted", l.colors.text_muted, d.colors.text_muted),
        ("ink-subtle", l.colors.text_subtle, d.colors.text_subtle),
        ("border", l.colors.border, d.colors.border),
        ("accent", l.extra.accent, d.extra.accent),
        ("link", l.colors.primary, d.colors.primary),
        ("success", l.extra.success, d.extra.success),
        ("danger", l.extra.danger, d.extra.danger),
        ("warning", l.extra.warning, d.extra.warning),
    ] {
        let _ = writeln!(w, "  {name}: {}", q(light));
        let _ = writeln!(w, "  {name}-dark: {}", q(dark));
    }
    let z = &l.size;
    let tx = &l.text;
    let _ = writeln!(w, "typography:");
    let _ = writeln!(w, "  prose:\n    fontFamily: \"{}\"\n    fontSize: \"{}px\"\n    fontWeight: 400\n    lineHeight: {:.3}", l.font_reading, px(z.chat_text), z.line_height_chat);
    let _ = writeln!(
        w,
        "  ui:\n    fontFamily: \"{}\"\n    fontSize: \"{}px\"\n    fontWeight: 400",
        l.font_sans,
        px(tx.body_sm)
    );
    let _ = writeln!(
        w,
        "  caption:\n    fontFamily: \"{}\"\n    fontSize: \"{}px\"\n    fontWeight: 400",
        l.font_sans,
        px(tx.caption)
    );
    let _ = writeln!(w, "  code:\n    fontFamily: \"{}\"", l.font_mono);
    let _ = writeln!(
        w,
        "  code-inline:\n    fontFamily: \"{}\"",
        l.font_code_inline
    );
    let r = &l.radius;
    let _ = writeln!(
        w,
        "rounded:\n  button: \"{}px\"\n  card: \"{}px\"\n  field: \"{}px\"\n  composer: \"{}px\"",
        px(r.button),
        px(r.card),
        px(r.field),
        px(z.composer_radius)
    );
    let sp = &l.space;
    let _ = writeln!(w, "spacing:\n  xs: \"{}px\"\n  sm: \"{}px\"\n  md: \"{}px\"\n  lg: \"{}px\"\n  xl: \"{}px\"\n  xxl: \"{}px\"", px(sp.xs), px(sp.sm), px(sp.md), px(sp.lg), px(sp.xl), px(sp.xxl));
    let _ = writeln!(w, "components:");
    let _ = writeln!(w, "  button-primary:\n    backgroundColor: \"{{colors.accent}}\"\n    rounded: \"{{rounded.button}}\"");
    let _ = writeln!(w, "  user-message:\n    backgroundColor: \"{{colors.surface}}\"\n    rounded: \"{{rounded.card}}\"\n    padding: \"{}px {}px\"", px(sp.sm + gpui::px(2.)), px(sp.lg));
    let _ = writeln!(w, "  quiet-line:\n    textColor: \"{{colors.ink-muted}}\"\n    typography: \"{{typography.ui}}\"\n    height: \"{}px\"", px(z.quiet_line));
    let _ = writeln!(w, "---\n");
    let _ = writeln!(w, "# Design: KodAct\n");
    let _ = writeln!(w, "> Generated from `src/theme.rs` by `src/system_md.rs`, with `.interface-design/system.md`. Change a token in theme.rs, then run `UPDATE_SYSTEM_MD=1 cargo test system_md`. The product record is `PRODUCT.md`; the theme above is the default one, light and dark, and every other theme keeps the same roles.\n");
    let _ = writeln!(w, "## Overview\n\nAn Operate surface: an app for long agent sessions, judged against Claude Desktop's calm. The reply is the hero; everything around it (the agent's activity, the chrome, the menus) is quieter than the words. Restraint, precision and one accent, not decoration.\n");
    let _ = writeln!(w, "## Colors\n\n- **Neutrals** carry everything: a canvas, a slightly different sidebar layer, a surface for the few things that hold content (your messages, an opened change, a card that needs you), and five inks from strong to subtle.\n- **The accent** is only for the primary action, the current selection and state (a waiting dot, a switch that is on). Never for decoration or an inactive control.\n- **Status colours** (success, danger, warning) only mean status: a failed call, a stopped one, a change's size.\n- Text holds 4.5:1 against its ground in every theme (`src/palette_check.rs`).\n");
    let _ = writeln!(w, "## Typography\n\nOne sans for interface and prose ({}), in regular and semibold; replies at {}px on {:.0}px lines. Code in {}; a name in prose (a file, a command) in {} on a faint neutral wash, never coloured. Small text is sentence case: no uppercase labels and no label above a heading.\n", l.font_sans, px(z.chat_text), f32::from(z.chat_text) * z.line_height_chat, l.font_mono, l.font_code_inline);
    let _ = writeln!(w, "## Layout\n\nThe transcript and the composer share one {}px column and one edge. Paragraphs {}px apart; tight groups, generous separation. The sidebar is {}px.\n", px(z.column), px(sp.md), px(z.sidebar));
    let _ = writeln!(w, "## Elevation & Depth\n\nNearly flat. Lift (`elevation_grain`) only for what holds content you act on: the composer, a card that needs you; popovers a touch more. A filled or bordered button sits a hair off its surface (`elevation_button`) on a face a shade lighter at the top (`button_face`), and goes flat when pressed. No glows, no hard offset shadows.\n");
    let _ = writeln!(w, "## Shapes\n\nCorners from the tokens above; one radius per role. No coloured edge over 1px on cards or rows: a selected row is a soft fill.\n");
    let _ = writeln!(w, "## Components\n\n- **Activity** (tool calls, runs of calls, thinking, answers): quiet lines in the muted ink, {}px high, no icons for what worked; opened, steps indent under a 2px neutral rule.\n- **Your message:** a filled bubble without a frame.\n- **Cards that need you** (permission, question): a lifted card; the primary action in the accent, the rest quiet.\n- **Menus and popovers:** one style (radius, padding, row height, shadow) from the tokens.\n", px(z.quiet_line));
    let _ = writeln!(w, "## Do's and Don'ts\n\n- Do let the reply be the loudest thing on screen.\n- Do say what happened in words (\"Couldn't write Cargo.toml\"), not only with an icon.\n- Don't put a kicker or eyebrow above a heading.\n- Don't use mono as a technical costume: mono is for code, paths in changes, and data.\n- Don't use colour for what isn't primary, selected or a state.\n- Don't add motion that isn't a state change; transitions take 120, 200 or 280ms, nothing else.");
    s
}

#[test]
fn system_md_matches_the_theme() {
    let (want, design) = (render(), render_design());
    if std::env::var_os("UPDATE_SYSTEM_MD").is_some() {
        std::fs::create_dir_all(std::path::Path::new(PATH).parent().unwrap()).unwrap();
        std::fs::write(PATH, &want).unwrap();
        std::fs::write(DESIGN, &design).unwrap();
        return;
    }
    let have = std::fs::read_to_string(PATH).unwrap_or_default();
    assert!(
        have == want,
        ".interface-design/system.md is stale; run `UPDATE_SYSTEM_MD=1 cargo test system_md`"
    );
    assert!(
        std::fs::read_to_string(DESIGN).unwrap_or_default() == design,
        "DESIGN.md is stale; run `UPDATE_SYSTEM_MD=1 cargo test system_md`"
    );
}
