//! The palette, measured. Every panel colour in `theme.rs` is checked here
//! against numbers, not taste:
//!
//! - The window's surfaces (title bar, sidebar, session, cards) step evenly
//!   in perceived lightness (OKLab L), darkest at the top of the window in
//!   light and dark alike, and stay near-neutral: paper and ink.
//! - Text that carries meaning reads at WCAG AA (4.5:1) on every surface it
//!   sits on, hover washes included; icons and hairlines reach their own
//!   floors. `text_subtle` is for placeholders and disabled text only.
//! - Coloured text (status, modes, ping chips, links) reads at AA on its own
//!   tint and on the card behind it.

use gpui::{Hsla, Rgba};

use crate::theme::Theme;

/// WCAG AA for body text, and for icons and other UI parts.
const AA_TEXT: f32 = 4.5;
const AA_UI: f32 = 3.0;

pub(crate) fn rgba(c: Hsla) -> Rgba {
    c.to_rgb()
}

/// `fg` painted over an opaque `bg`.
pub(crate) fn over(fg: Hsla, bg: Hsla) -> Rgba {
    let (f, b) = (rgba(fg), rgba(bg));
    let a = f.a;
    Rgba {
        r: f.r * a + b.r * (1. - a),
        g: f.g * a + b.g * (1. - a),
        b: f.b * a + b.b * (1. - a),
        a: 1.,
    }
}

fn linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(c: Rgba) -> f32 {
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// The WCAG contrast ratio of `fg` (possibly translucent) on opaque `bg`.
pub(crate) fn contrast(fg: Hsla, bg: Hsla) -> f32 {
    let (a, b) = (luminance(over(fg, bg)), luminance(rgba(bg)));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// OKLab lightness and chroma.
pub(crate) fn oklab(c: Rgba) -> (f32, f32) {
    let (r, g, b) = (linear(c.r), linear(c.g), linear(c.b));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    (lightness, a.hypot(bb))
}

/// Every theme in both modes, by name: each is held to the same floors.
fn both() -> Vec<(String, Theme)> {
    crate::theme::Look::ALL
        .into_iter()
        .flat_map(|l| {
            [false, true].map(|dark| {
                let mode = if dark { "dark" } else { "light" };
                (format!("{} {mode}", l.id()), Theme::of(l, dark))
            })
        })
        .collect()
}

/// The surfaces text sits on, by name: the window's three, the card on top,
/// the selected row, and each with the hover wash over it.
fn grounds(t: &Theme) -> Vec<(String, Hsla)> {
    let base = [
        ("chrome", t.extra.chrome),
        ("pane", t.extra.pane),
        ("stage", t.extra.stage),
        ("surface", t.colors.surface),
        ("selected", t.extra.selected),
    ];
    let mut out: Vec<(String, Hsla)> = base.iter().map(|(n, c)| (n.to_string(), *c)).collect();
    out.extend(
        base.iter()
            .map(|(n, c)| (format!("{n}+hover"), Hsla::from(over(t.colors.hover, *c)))),
    );
    // The current row in the sidebar's sheet: the accent's wash, softened.
    out.push((
        "stage+accent_soft".into(),
        Hsla::from(over(t.extra.accent_soft.opacity(0.6), t.extra.stage)),
    ));
    out
}

#[test]
fn surfaces_stack_like_paper_on_paper() {
    for (mode, t) in both() {
        // The title bar and sidebars share `pane`. In light mode, paper stacks
        // brightest-above (`pane < stage < surface`); in dark mode, the main
        // workspace stage is deepest obsidian (`stage < pane < surface`) while
        // sidebars, Settings and cards lift off it softly.
        assert_eq!(rgba(t.extra.chrome), rgba(t.extra.pane), "{mode}");
        let ladder = if t.is_dark {
            [t.extra.stage, t.extra.pane, t.colors.surface]
        } else {
            [t.extra.pane, t.extra.stage, t.colors.surface]
        }
        .map(|c| oklab(rgba(c)));
        for (i, w) in ladder.windows(2).enumerate() {
            let s = w[1].0 - w[0].0;
            assert!(
                s >= 0.02,
                "{mode}: step {i} is {s:.4}, too close to tell apart"
            );
        }
        // Neutral, never coloured: paper and ink.
        for (i, (_, chroma)) in ladder.iter().enumerate() {
            assert!(
                *chroma <= 0.03,
                "{mode}: surface {i} has chroma {chroma:.4}; paper, not colour"
            );
        }
        // The selected row lifts off the sidebar in both modes.
        let (pane, sel, stage) = (
            oklab(rgba(t.extra.pane)).0,
            oklab(rgba(t.extra.selected)).0,
            oklab(rgba(t.extra.stage)).0,
        );
        let floor = if t.is_dark {
            0.02
        } else {
            (stage - pane) * 0.9
        };
        assert!(
            sel - pane >= floor,
            "{mode}: the selected row sits {:.4} above the sidebar",
            sel - pane
        );
    }
}

/// One colour: the accent and the mark are KodAct's burgundy; links, focus
/// and switches are ink, and so is live work.
#[test]
fn each_theme_has_one_colour() {
    let hue = |c: Hsla| {
        let (l, a, b) = {
            let c = rgba(c);
            let (l, _) = oklab(c);
            let lin = |v: f32| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            let (r, g, bl) = (lin(c.r), lin(c.g), lin(c.b));
            let m = (0.211_9 * r + 0.680_7 * g + 0.107_4 * bl).cbrt();
            let lc = (0.412_2 * r + 0.536_3 * g + 0.051_4 * bl).cbrt();
            let sc = (0.088_3 * r + 0.281_7 * g + 0.630_0 * bl).cbrt();
            (
                l,
                1.978 * lc - 2.429 * m + 0.451 * sc,
                0.026 * lc + 0.783 * m - 0.809 * sc,
            )
        };
        let _ = l;
        b.atan2(a).to_degrees()
    };
    for (mode, t) in both() {
        let x = &t.extra;
        let chroma = oklab(rgba(x.accent)).1;
        {
            assert!(chroma > 0.04, "{mode}: the accent has no colour");
            let d = (hue(x.mark_top) - hue(x.accent) + 540.) % 360. - 180.;
            assert!(d.abs() <= 20., "{mode}: the mark is {d:.0}° off the accent");
            // Burgundy: red-pink in OKLab, neither orange nor purple.
            let h = (hue(x.accent) + 360.) % 360.;
            assert!(
                (-0. ..=30.).contains(&h) || h >= 345.,
                "{mode}: the accent's hue is {h:.0}°, not burgundy"
            );
            // One colour: links, focus and switches are ink, with no colour.
            for (name, c) in [
                ("the link", t.colors.primary),
                ("the link, hovered", t.colors.primary_hover),
                ("the switch", x.toggle_on),
            ] {
                let chroma = oklab(rgba(c)).1;
                assert!(chroma <= 0.02, "{mode}: {name} has colour ({chroma:.3})");
            }
            let chroma = oklab(rgba(x.live)).1;
            assert!(chroma <= 0.02, "{mode}: live work has colour ({chroma:.3})");
        }
        // The accent holds a UI part's 3:1 on the session and on a card,
        // and its label reads on it.
        for g in [x.stage, t.colors.surface] {
            assert!(
                contrast(x.accent, g) >= AA_UI,
                "{mode}: accent on a surface"
            );
        }
        assert!(
            contrast(x.accent_fg, x.accent_fill) >= AA_TEXT,
            "{mode}: the accent's label"
        );
    }
    let calm = Theme::light();
    assert_eq!(rgba(calm.extra.accent), rgba(gpui::rgb(0x7a1f3d).into()));
}

/// The accent and `danger` are never confused: a failure must not look like
/// the main action. They sit at least 25° of hue apart, and a clear step
/// apart in lightness or in chroma.
#[test]
fn the_accent_and_danger_stay_apart() {
    let lch = |c: Hsla| {
        let r = rgba(c);
        let (l, chroma) = oklab(r);
        let (lr, lg, lb) = (linear(r.r), linear(r.g), linear(r.b));
        let (lc, m, sc) = (
            (0.412_221_46 * lr + 0.536_332_55 * lg + 0.051_445_995 * lb).cbrt(),
            (0.211_903_5 * lr + 0.680_699_5 * lg + 0.107_396_96 * lb).cbrt(),
            (0.088_302_46 * lr + 0.281_718_85 * lg + 0.629_978_7 * lb).cbrt(),
        );
        let a = 1.977_998_5 * lc - 2.428_592_2 * m + 0.450_593_7 * sc;
        let b = 0.025_904_037 * lc + 0.782_771_77 * m - 0.808_675_77 * sc;
        (l, chroma, b.atan2(a).to_degrees())
    };
    for (mode, t) in both() {
        for (name, danger) in [
            ("danger", t.extra.danger),
            ("fill_danger", t.extra.fill_danger),
        ] {
            let (la, ca, ha) = lch(t.extra.accent);
            let (ld, cd, hd) = lch(danger);
            let dh = ((ha - hd + 540.) % 360. - 180.).abs();
            assert!(
                dh >= 25.,
                "{mode}: {name} is only {dh:.0}° of hue from the accent"
            );
            let (dl, dc) = ((la - ld).abs(), (ca - cd).abs());
            assert!(
                dl >= 0.03 || dc >= 0.03,
                "{mode}: {name} is {dl:.3} lighter and {dc:.3} more chromatic than the accent; too alike"
            );
        }
    }
}

#[test]
fn text_reads_on_every_surface() {
    for (mode, t) in both() {
        for (ground, bg) in grounds(&t) {
            for (name, fg, floor) in [
                ("text", t.colors.text, AA_TEXT),
                ("text_strong", t.colors.text_strong, AA_TEXT),
                ("text_body", t.colors.text_body, AA_TEXT),
                ("text_muted", t.colors.text_muted, AA_TEXT),
                // Placeholders and disabled text: legible, never body copy.
                ("text_subtle", t.colors.text_subtle, 2.5),
            ] {
                let c = contrast(fg, bg);
                assert!(
                    c >= floor,
                    "{mode}: {name} on {ground} is {c:.2}:1, under {floor}:1"
                );
            }
        }
    }
}

#[test]
fn hairlines_show_on_every_surface() {
    for (mode, t) in both() {
        for (ground, bg) in grounds(&t) {
            let c = contrast(t.colors.border, bg);
            assert!(
                c >= 1.15,
                "{mode}: the hairline on {ground} is {c:.2}:1, too faint to see"
            );
        }
    }
}

#[test]
fn coloured_text_reads_on_its_tint() {
    for (mode, t) in both() {
        let card = t.colors.surface;
        let x = &t.extra;
        let pairs = [
            ("link", t.colors.primary, card),
            ("filled button", x.fill_fg, x.fill),
            ("filled button, hovered", x.fill_fg, x.fill_hover),
            ("danger button", t.colors.primary_fg, x.fill_danger),
            ("success", x.success, Hsla::from(over(x.success_tint, card))),
            ("danger", x.danger, Hsla::from(over(x.danger_tint, card))),
            ("warning", x.warning, Hsla::from(over(x.warning_tint, card))),
            ("memory", x.memory, Hsla::from(over(x.memory_tint, card))),
            // A file or command named in a reply, on the session and on a card.
            (
                "inline code",
                x.code_inline_fg,
                Hsla::from(over(x.code_inline_bg, x.stage)),
            ),
            (
                "inline code on a card",
                x.code_inline_fg,
                Hsla::from(over(x.code_inline_bg, card)),
            ),
            ("skill", x.skill, card),
            (
                "ping fast",
                x.ping_fast_fg,
                Hsla::from(over(x.ping_fast_bg, card)),
            ),
            (
                "ping fair",
                x.ping_fair_fg,
                Hsla::from(over(x.ping_fair_bg, card)),
            ),
            (
                "ping slow",
                x.ping_slow_fg,
                Hsla::from(over(x.ping_slow_bg, card)),
            ),
        ];
        for (name, fg, bg) in pairs {
            let c = contrast(fg, bg);
            assert!(c >= AA_TEXT, "{mode}: {name} is {c:.2}:1");
        }
        // A mode's colour marks the composer's edge and its icon (UI parts).
        for (name, c) in [
            ("edit", x.mode_edit),
            ("auto", x.mode_auto),
            ("plan", x.mode_plan),
            ("bypass", x.mode_bypass),
        ] {
            let r = contrast(c, card);
            assert!(r >= AA_UI, "{mode}: mode {name} is {r:.2}:1 on a card");
        }
        // Plan and Bypass fill the send button, under a white arrow, and
        // the accent fills it otherwise.
        for (name, c, arrow_fg) in [
            ("plan", x.mode_plan, t.colors.primary_fg),
            ("bypass", x.mode_bypass, t.colors.primary_fg),
            ("default", x.accent_fill, x.accent_fg),
        ] {
            let arrow = contrast(arrow_fg, c);
            assert!(
                arrow >= AA_UI,
                "{mode}: the send arrow on {name} is {arrow:.2}:1"
            );
        }
    }
}

#[test]
fn the_measures_agree_with_known_values() {
    let black: Hsla = gpui::rgb(0x000000).into();
    let white: Hsla = gpui::rgb(0xffffff).into();
    assert!((contrast(black, white) - 21.).abs() < 0.01);
    assert!((contrast(white, white) - 1.).abs() < 0.001);
    let (l, c) = oklab(rgba(white));
    assert!((l - 1.).abs() < 0.001 && c < 0.001);
}

/// The terminal's colours read on its ink (`terminal.background`), as KodAct
/// for VS Code sets them: every colour at least as a UI part (3:1), the
/// darkest text ones at AA; black in dark mode is meant to sit back.
#[test]
fn terminal_colours_read_on_the_dock() {
    for t in [Theme::light(), Theme::dark()] {
        let mode = if t.is_dark { "dark" } else { "light" };
        let bg = t.terminal.background;
        let (_, chroma) = oklab(rgba(bg));
        assert!(
            chroma <= 0.01,
            "terminal background in {mode} has chroma {chroma:.4}; keep neutral"
        );
        for (i, c) in t.extra.ansi.iter().enumerate() {
            let need = match (i, t.is_dark) {
                (0, true) => 1.0,
                // Black and bright white are the ink text itself.
                (0, false) | (15, _) => AA_TEXT,
                _ => AA_UI,
            };
            let r = contrast(*c, bg);
            assert!(
                r >= need,
                "ansi[{i}] {r:.2}:1 on the dock in {mode} (needs {need})"
            );
        }
        let k = &t.terminal;
        for (name, c) in [
            ("foreground", k.foreground),
            ("muted", k.muted),
            ("prompt", k.prompt),
            ("command", k.command),
            ("keyword", k.keyword),
            ("flag", k.flag),
            ("string", k.string),
            ("variable", k.variable),
            ("operator", k.operator),
            ("number", k.number),
            ("comment", k.comment),
        ] {
            let r = contrast(c, bg);
            assert!(
                r >= AA_TEXT,
                "terminal.{name} {r:.2}:1 on terminal.background in {mode} (needs {AA_TEXT})"
            );
        }
        for (i, c) in k.ansi.iter().enumerate() {
            let need = match (i, t.is_dark) {
                (0, true) => 1.0,
                (0, false) => AA_TEXT,
                _ => AA_UI,
            };
            let r = contrast(*c, bg);
            assert!(
                r >= need,
                "terminal.ansi[{i}] {r:.2}:1 on terminal.background in {mode} (needs {need})"
            );
        }
    }
}

#[test]
fn the_mark_reads_on_every_surface() {
    for (mode, t) in both() {
        // Both strokes are one colour: a UI part's 3:1 on each surface the
        // mark sits on.
        for (ground, g) in grounds(&t) {
            let c = contrast(t.extra.mark_top, g);
            assert!(c >= AA_UI, "{mode}: the mark is {c:.2}:1 on {ground}");
        }
        // As it arrives the mark runs from near dark to near white; the
        // bright signal and the white read on the dark the opener plays on.
        let dark = Theme::dark().extra.chrome;
        for (name, c) in [
            ("bright", t.extra.mark_heat),
            ("white", t.extra.mark_heat_white),
        ] {
            let r = contrast(c, dark);
            assert!(r >= AA_UI, "{mode}: the mark's {name} is {r:.2}:1");
        }
    }
}
