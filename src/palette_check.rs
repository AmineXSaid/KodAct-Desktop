//! The palette, measured. Every panel colour in `theme.rs` is checked here
//! against numbers, not taste:
//!
//! - The window's surfaces (title bar, sidebar, session, cards) step evenly
//!   in perceived lightness (OKLab L), darkest at the top of the window in
//!   light and dark alike, and stay near-neutral warm paper.
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
fn surfaces_stack_like_paper_on_the_cream() {
    for (mode, t) in both() {
        // The title bar and the sidebar share the canvas; the session is
        // a sheet a step up from it, and cards a step up again.
        assert_eq!(rgba(t.extra.chrome), rgba(t.extra.pane), "{mode}");
        let ladder = [t.extra.pane, t.extra.stage, t.colors.surface].map(|c| oklab(rgba(c)));
        for (i, w) in ladder.windows(2).enumerate() {
            let s = w[1].0 - w[0].0;
            assert!(
                s >= 0.02,
                "{mode}: step {i} is {s:.4}, too close to tell apart"
            );
        }
        // Warm, never coloured: cream, not tan.
        for (i, (_, chroma)) in ladder.iter().enumerate() {
            assert!(
                *chroma <= 0.03,
                "{mode}: surface {i} has chroma {chroma:.4}; cream, not colour"
            );
        }
        // The selected row lifts off the sidebar as the session does.
        let (pane, sel, stage) = (
            oklab(rgba(t.extra.pane)).0,
            oklab(rgba(t.extra.selected)).0,
            oklab(rgba(t.extra.stage)).0,
        );
        assert!(
            sel - pane >= (stage - pane) * 0.9,
            "{mode}: the selected row sits {:.4} above the sidebar",
            sel - pane
        );
    }
}

/// One colour per theme: the accent, the links, the switches and the
/// mark's top are one hue (Ink has none); the system's own theme is ember copper.
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
            for (name, c) in [
                ("link", t.colors.primary),
                ("switch", x.toggle_on),
                ("mark top", x.mark_top),
            ] {
                let d = (hue(c) - hue(x.accent) + 540.) % 360. - 180.;
                assert!(
                    d.abs() <= 20.,
                    "{mode}: the {name} is {d:.0}° off the accent"
                );
            }
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
            contrast(x.accent_fg, x.accent) >= AA_TEXT,
            "{mode}: the accent's label"
        );
    }
    let calm = Theme::light();
    assert_eq!(rgba(calm.extra.accent), rgba(gpui::rgb(0xb4532a).into()));
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
            ("default", x.accent, x.accent_fg),
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
        let bg = t.terminal.background;
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
                "ansi[{i}] {:.2}:1 on the dock in {} (needs {need})",
                r,
                if t.is_dark { "dark" } else { "light" }
            );
        }
    }
}

#[test]
fn the_mark_reads_on_every_surface_and_its_faces_read_apart() {
    for (mode, t) in both() {
        let x = &t.extra;
        // The ingot's silhouette: its top and its nearer-to-ground side
        // hold a UI part's 3:1 on each surface it sits on.
        for (ground, g) in grounds(&t) {
            let top = contrast(x.mark_top, g);
            assert!(
                top >= AA_UI,
                "{mode}: the mark's top is {top:.2}:1 on {ground}"
            );
            let side = contrast(x.mark_left, g).max(contrast(x.mark_right, g));
            assert!(
                side >= AA_UI,
                "{mode}: the mark's sides are {side:.2}:1 on {ground}"
            );
        }
        // Its two sides, and the top against each, read as separate faces.
        for (name, a, b) in [
            ("left and right", x.mark_left, x.mark_right),
            ("top and left", x.mark_top, x.mark_left),
            ("top and right", x.mark_top, x.mark_right),
        ] {
            let c = contrast(a, b);
            assert!(c >= 1.5, "{mode}: the mark's {name} faces are {c:.2}:1");
        }
    }
}
