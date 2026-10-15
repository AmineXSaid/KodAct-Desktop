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

fn both() -> [(&'static str, Theme); 2] {
    [("light", Theme::light()), ("dark", Theme::dark())]
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
    out
}

#[test]
fn surfaces_step_evenly_from_the_title_bar_down() {
    for (mode, t) in both() {
        // Title bar, sidebar, session, card: each a step lighter.
        let ladder = [
            t.extra.chrome,
            t.extra.pane,
            t.extra.stage,
            t.colors.surface,
        ]
        .map(|c| oklab(rgba(c)));
        let steps: Vec<f32> = ladder.windows(2).map(|w| w[1].0 - w[0].0).collect();
        let mean = steps.iter().sum::<f32>() / steps.len() as f32;
        for (i, s) in steps.iter().enumerate() {
            assert!(
                *s >= 0.02,
                "{mode}: step {i} is {s:.4}, too close to tell apart"
            );
            assert!(
                (s - mean).abs() <= 0.004,
                "{mode}: step {i} is {s:.4}, the mean is {mean:.4}; the ladder is uneven"
            );
        }
        for (i, (_, chroma)) in ladder.iter().enumerate() {
            assert!(
                *chroma <= 0.01,
                "{mode}: surface {i} has chroma {chroma:.4}; paper, not colour"
            );
        }
        // The selected row lifts off the sidebar at least as far as one step.
        let (pane, sel) = (oklab(rgba(t.extra.pane)).0, oklab(rgba(t.extra.selected)).0);
        assert!(
            sel - pane >= mean,
            "{mode}: the selected row sits {:.4} above the sidebar",
            sel - pane
        );
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
            ("filled button", t.colors.primary_fg, x.fill),
            ("filled button, hovered", t.colors.primary_fg, x.fill_hover),
            ("danger button", t.colors.primary_fg, x.fill_danger),
            ("success", x.success, Hsla::from(over(x.success_tint, card))),
            ("danger", x.danger, Hsla::from(over(x.danger_tint, card))),
            ("warning", x.warning, Hsla::from(over(x.warning_tint, card))),
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
            ("expert", x.mode_expert),
        ] {
            let r = contrast(c, card);
            assert!(r >= AA_UI, "{mode}: mode {name} is {r:.2}:1 on a card");
        }
        // Plan and Bypass also fill the send button, under a white arrow.
        for (name, c) in [
            ("plan", x.mode_plan),
            ("bypass", x.mode_bypass),
            ("default", x.fill),
        ] {
            let arrow = contrast(t.colors.primary_fg, c);
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

/// The terminal's colours read on its ink (`terminal.background`), as Forge
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
