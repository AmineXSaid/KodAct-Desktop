//! `.interface-design/system.md`, written from `theme.rs`.
//!
//! The design skills read that file as the system's tokens; the app reads
//! `theme.rs`. So the file is generated, and this test fails when the two
//! disagree. After changing a token, rewrite it with
//! `UPDATE_SYSTEM_MD=1 cargo test system_md`.

use std::fmt::Write as _;

use gpui::{Hsla, Pixels};

use crate::palette_check::{contrast, oklab, rgba};
use crate::theme::Theme;

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.interface-design/system.md");

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
    let _ = writeln!(w, "# Design System: Forge desktop (forge-gpui)\n");
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
    let _ = writeln!(w, "**Personality:** Warmth & Approachability, with the Precision & Density of a desktop tool. A calm room to build in with an agent.\n");
    let _ = writeln!(w, "**Foundation:** warm ivory. Paper surfaces in one warm hue, chroma under 0.01 (OKLCH); warm ink for text; one terracotta accent kept for sending, the home screen's mark and a few dots; colour otherwise only for meaning: modes, status, a switch that is on.\n");
    let _ = writeln!(w, "**Depth:** borders-only for content (1px hairlines); shadows only on popovers, menus and the command palette.\n");
    let _ = writeln!(w, "**Shape:** softly squared. Buttons 7-9px, cards and menus 10px, large inputs 12-14px; pills only for switches, badges and avatars; the round new-chat mark is the one circular control.\n");

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
    let _ = writeln!(w, "### Radius\nScale: {}px (a line's ends: every straight line is rounded just slightly), {}px (key caps), {}px (a selected segment), {}px (compact buttons, chips), {}px (buttons, icon buttons, rows), {}px (outlined squares, send), {}px (cards, tool rows, menus), {}px (large fields), {}px (the composer), pill (switches, badges, avatars)\n", px(r.line), px(r.small), px(r.segment), px(r.compact), px(r.button), px(r.control), px(r.card), px(r.field), px(z.composer_radius));

    let _ = writeln!(w, "### Typography");
    let _ = writeln!(
        w,
        "Interface: {} (Anthropic Sans when installed or in Forge's `fonts` folder; never bundled)",
        l.font_sans
    );
    let _ = writeln!(
        w,
        "Display: {} (the greeting)\nCode: {}",
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
        ("dot_gap", z.dot_gap, "the home screen's dot grid"),
    ] {
        let _ = writeln!(w, "- `{name}`: {}px, {role}", px(v));
    }
    let _ = writeln!(w);

    let _ = writeln!(w, "### Surfaces\nOne ladder, darkest at the top of the window, even steps in OKLab lightness.\n");
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
    let rows: [(&str, Pick, Pick, Pick, &str); 18] = [
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
    let _ = writeln!(w, "### Composer\n- Width: up to {}px, centred\n- One unit: a white box (radius {}px, 1px `border`, `border_strong` while focused, tinted by the mode when one is set) resting in a `sunk` tray\n- Text: {}px; the outlined + bottom left and Send bottom right, {}px from the box's edges\n- The tray: the project, the context ring, the model and the mode as quiet {}px selectors with a chevron\n", px(z.composer_width), px(z.composer_radius), px(tx.body), px(sp.snug), px(z.button_sm));
    let _ = writeln!(w, "### Dot Field (`ui/dot_field.rs`)\n- Behind the home screen: a {}px grid of {}px dots in `dot`, gathered beside the heading and toward the lower right, thinned and faded toward a quiet oval behind the heading and composer and before every edge\n- About 1.6% of dots in `accent`; fewer still are 5px outlined squares or accent diamonds\n- Painted on one canvas from cached geometry; breathes over 9s, leans up to 4px from the pointer, brightens while the composer has focus; rests when hidden or inactive; still under reduced motion\n", px(z.dot_gap), px(z.dot));
    let _ = writeln!(w, "### Card Default\n- Border: 1px `border`\n- Padding: {}px\n- Radius: {}px\n- Background: `surface`\n- Usage: tool calls, file cards, code blocks\n", px(sp.xl), px(r.card));
    let _ = writeln!(w, "### Popover\n- Border: 1px `border`, radius {}px, padding {}px\n- Background: `surface`, with the two-layer popover shadow\n- Usage: menus, pickers, the command palette\n", px(r.card), px(sp.xs));
    let _ = writeln!(w, "### File Card\n- A file Forge made, in its reply: up to 440px, `surface`, 1px `border`, radius {}px, padding {}px\n- A 36x44 page tile (radius {}px, concentric with the card) previews it, its type in mono `primary`\n- Title 600 `text_strong`; \"Document · MD\" caption `text_muted`; a bordered Download button\n- Open: the edge turns `primary` at 60%\n", px(r.card), px(sp.sm), px(r.small));
    let _ = writeln!(w, "### Document Pane\n- Beside the conversation, half the session, at least {}px; `surface` with a hairline on its left\n- Header 48px: file icon, title, type; Copy, Download, Close as {}px quiet buttons\n- Text {}px at {} leading, lines up to {}px; the title in the display serif at {}px\n- Enters with a 220ms ease-out, fading in from 16px to the right; Escape closes it\n", px(z.doc_pane), px(z.control), px(z.doc_text), z.line_height_doc, px(z.doc_measure), px(z.doc_title));
    let _ = writeln!(w, "### Sidebar Row\n- Height: {}px, radius {}px\n- Current: the `active` wash, 500 weight, no border\n- New chat: a round `accent` mark with a white +\n- Collapsed: a {}px rail of icon buttons for the same actions\n- Group labels: {}px caption\n", px(z.row), px(r.button), px(z.rail), px(tx.caption));

    let _ = writeln!(w, "## Decisions\n");
    let _ = writeln!(
        w,
        "| Decision | Rationale | Date |\n|----------|-----------|------|"
    );
    for (what, why, when) in [
        ("Borders-only depth", "A desktop tool read for hours: hairlines separate without adding weight; shadows are kept to show what floats.", "2026-09-28"),
        ("Three surfaces, one step apart", "Title bar, sidebar and session must read as separate areas at a glance; even OKLab steps make the split clear without contrast jumps.", "2026-10-12"),
        ("Ink, not colour, for the filled button", "The accent is kept for sending; the one filled action per surface is ink on paper (paper on charcoal), so it never competes with Send.", "2026-10-17"),
        ("Muted text a step deeper than the reference", "The reference's #6f6c64 and #9a968c miss their floors on the title bar; #64615a and #8c887e keep the look and pass.", "2026-10-17"),
        ("Softly squared controls", "Large uniform radii read as a template; 7-10px keeps the shapes calm and precise.", "2026-10-17"),
        ("Display serif for one line per view", "The greeting, and a document's title: one warm, human line; everything else stays in the interface face.", "2026-10-12"),
        ("Composer at 620px", "A comfortable measure for a prompt, the same in a chat and on the welcome page.", "2026-10-11"),
        ("Icons on a 12/14/16/20 scale", "Desktop density: 14 in rows, 16 in controls, 20 for the composer's +; 12 inside captions.", "2026-10-12"),
    ] {
        let _ = writeln!(w, "| {what} | {why} | {when} |");
    }
    s
}

#[test]
fn system_md_matches_the_theme() {
    let want = render();
    if std::env::var_os("UPDATE_SYSTEM_MD").is_some() {
        std::fs::create_dir_all(std::path::Path::new(PATH).parent().unwrap()).unwrap();
        std::fs::write(PATH, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(PATH).unwrap_or_default();
    assert!(
        have == want,
        ".interface-design/system.md is stale; run `UPDATE_SYSTEM_MD=1 cargo test system_md`"
    );
}
