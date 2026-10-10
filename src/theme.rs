//! Warm-paper design tokens for GPUI, adapted from DESIGN.md.
//! Pure UI: colors, type, spacing, radius, elevation, motion + a few styled primitives.
//! Written against GPUI's current API; field/method names may need small tweaks per GPUI version.

use gpui::{
    div, hsla, point, prelude::*, px, rgb, App, BoxShadow, Div, FontWeight, Global, Hsla, Pixels,
    SharedString,
};

// ---------- Tokens ----------

#[derive(Clone, Copy)]
pub struct Colors {
    pub canvas: Hsla,        // page / window background (warm paper)
    pub surface: Hsla,       // cards, panels, popovers
    pub sidebar: Hsla,       // app-specific: sidebar sits between canvas and surface
    pub text: Hsla,          // ink 95%
    pub text_strong: Hsla,   // ink 100% (headings)
    pub text_body: Hsla,     // graphite – warm body text
    pub text_muted: Hsla,    // ink 60%
    pub text_subtle: Hsla,   // ink 40% (placeholders, disabled)
    pub border: Hsla,        // hairline, ink 8%
    pub hover: Hsla,         // row / ghost hover wash
    pub primary: Hsla,       // the single filled-action color
    pub primary_hover: Hsla,
    pub primary_fg: Hsla,
    pub primary_tint: Hsla,  // ghost CTA bg, selection wash
    pub marigold: Hsla,
    pub coral: Hsla,
    pub sky: Hsla,
    pub midnight: Hsla,
}

#[derive(Clone, Copy)]
pub struct TypeScale {
    pub caption: Pixels,    // 12
    pub body_sm: Pixels,    // 14 – default UI text in a desktop app
    pub body: Pixels,       // 16 – chat/message text
    pub subheading: Pixels, // 20
    pub heading_sm: Pixels, // 22
}

#[derive(Clone, Copy)]
pub struct Space {
    pub xs: Pixels,  // 4
    pub sm: Pixels,  // 8
    pub md: Pixels,  // 12
    pub lg: Pixels,  // 16
    pub xl: Pixels,  // 24
    pub xxl: Pixels, // 32
}

#[derive(Clone, Copy)]
pub struct Radius {
    pub small: Pixels,  // 4
    pub button: Pixels, // 8
    pub card: Pixels,   // 12
    pub pill: Pixels,   // 9999
}

#[derive(Clone)]
pub struct Theme {
    pub is_dark: bool,
    pub colors: Colors,
    pub text: TypeScale,
    pub space: Space,
    pub radius: Radius,
    pub font_sans: SharedString,  // Inter (substitute for NotionInter)
    pub font_serif: SharedString, // Source Serif (substitute for Lyon Text)
    pub motion_ms: u64,           // 200ms ease for hovers/transitions
    // Forge additions (see the section at the end of the tokens).
    pub font_mono: SharedString,  // JetBrains Mono: code, paths, commands
    pub extra: ExtraColors,
    pub size: Sizes,
}

impl Global for Theme {}

fn ink(alpha: f32) -> Hsla {
    hsla(0., 0., 0., alpha)
}
fn paper(alpha: f32) -> Hsla {
    hsla(30. / 360., 0.1, 0.96, alpha)
}

impl Theme {
    fn base(is_dark: bool, colors: Colors) -> Self {
        Self {
            is_dark,
            colors,
            text: TypeScale {
                caption: px(12.),
                body_sm: px(14.),
                body: px(16.),
                subheading: px(20.),
                heading_sm: px(22.),
            },
            space: Space {
                xs: px(4.),
                sm: px(8.),
                md: px(12.),
                lg: px(16.),
                xl: px(24.),
                xxl: px(32.),
            },
            radius: Radius {
                small: px(4.),
                button: px(8.),
                card: px(12.),
                pill: px(9999.),
            },
            font_sans: "Inter".into(),
            font_serif: "Source Serif 4".into(),
            motion_ms: 200,
            font_mono: "JetBrains Mono".into(),
            extra: ExtraColors::for_mode(is_dark),
            size: Sizes::default(),
        }
    }

    pub fn light() -> Self {
        Self::base(
            false,
            Colors {
                canvas: rgb(0xf6f5f4).into(),
                surface: rgb(0xffffff).into(),
                sidebar: rgb(0xfbfaf9).into(),
                text: ink(0.95),
                text_strong: ink(1.0),
                text_body: rgb(0x615d59).into(),
                text_muted: ink(0.6),
                text_subtle: ink(0.4),
                border: ink(0.08),
                hover: ink(0.04),
                primary: rgb(0x0075de).into(),
                primary_hover: rgb(0x005bab).into(),
                primary_fg: rgb(0xffffff).into(),
                primary_tint: rgb(0xe6f3fe).into(),
                marigold: rgb(0xffb110).into(),
                coral: rgb(0xf64932).into(),
                sky: rgb(0x62aef0).into(),
                midnight: rgb(0x02093a).into(),
            },
        )
    }

    /// The source doc is light-only; this is an invented warm dark counterpart.
    pub fn dark() -> Self {
        Self::base(
            true,
            Colors {
                canvas: rgb(0x191918).into(),
                surface: rgb(0x22211f).into(),
                sidebar: rgb(0x1d1c1b).into(),
                text: paper(0.92),
                text_strong: paper(1.0),
                text_body: rgb(0xb5b0aa).into(),
                text_muted: paper(0.6),
                text_subtle: paper(0.4),
                border: hsla(0., 0., 1., 0.09),
                hover: hsla(0., 0., 1., 0.05),
                primary: rgb(0x2b8ff0).into(),
                primary_hover: rgb(0x4ba1f3).into(),
                primary_fg: rgb(0xffffff).into(),
                primary_tint: hsla(209. / 360., 0.9, 0.55, 0.16),
                marigold: rgb(0xffb110).into(),
                coral: rgb(0xf64932).into(),
                sky: rgb(0x62aef0).into(),
                midnight: rgb(0x02093a).into(),
            },
        )
    }

    /// Popovers, menus, command palette. Content cards use borders, not shadows.
    pub fn elevation_popover(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: ink(if self.is_dark { 0.4 } else { 0.03 }),
                offset: point(px(0.), px(3.)),
                blur_radius: px(9.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: ink(if self.is_dark { 0.3 } else { 0.1 }),
                offset: point(px(0.), px(4.)),
                blur_radius: px(12.),
                spread_radius: px(0.),
                inset: false,
            },
        ]
    }
}

// ---------- Forge additions ----------
// Tokens the app shell needs that DESIGN.md implies but the base set above
// does not name: status, diffs, the code well, the hero highlight, and the
// app's fixed sizes. Components read these, never literals.

#[derive(Clone, Copy)]
pub struct ExtraColors {
    pub inset: Hsla,         // a quiet well inside a card: code blocks, terminal output
    pub highlight: Hsla,     // peach hero pill behind the project name
    pub success: Hsla,       // text/icon on success
    pub success_tint: Hsla,
    pub danger: Hsla,
    pub danger_tint: Hsla,
    pub warning: Hsla,       // "needs you"
    pub warning_tint: Hsla,
    pub diff_add: Hsla,      // added line background
    pub diff_del: Hsla,      // removed line background
    pub scrim: Hsla,         // behind the command palette
    pub selected: Hsla,      // selected row: a white sticky note on the sidebar
}

impl ExtraColors {
    fn for_mode(is_dark: bool) -> Self {
        if is_dark {
            Self {
                inset: hsla(0., 0., 1., 0.035),
                highlight: rgb(0x5a4636).into(),
                success: rgb(0x6fd389).into(),
                success_tint: hsla(134. / 360., 0.5, 0.5, 0.16),
                danger: rgb(0xff8a78).into(),
                danger_tint: hsla(7. / 360., 0.92, 0.58, 0.18),
                warning: rgb(0xffc85a).into(),
                warning_tint: hsla(40. / 360., 1., 0.53, 0.18),
                diff_add: hsla(134. / 360., 0.5, 0.5, 0.16),
                diff_del: hsla(7. / 360., 0.92, 0.58, 0.16),
                scrim: ink(0.55),
                selected: rgb(0x2a2927).into(),
            }
        } else {
            Self {
                inset: ink(0.028),
                highlight: rgb(0xf6d5b8).into(),
                success: rgb(0x1f7a35).into(),
                success_tint: hsla(134. / 360., 0.55, 0.4, 0.12),
                danger: rgb(0xc42a14).into(),
                danger_tint: hsla(7. / 360., 0.92, 0.58, 0.12),
                warning: rgb(0x8a5600).into(),
                warning_tint: hsla(40. / 360., 1., 0.53, 0.2),
                diff_add: hsla(134. / 360., 0.55, 0.4, 0.12),
                diff_del: hsla(7. / 360., 0.92, 0.58, 0.1),
                scrim: hsla(30. / 360., 0.1, 0.05, 0.32),
                selected: rgb(0xffffff).into(),
            }
        }
    }
}

/// The app's fixed dimensions.
#[derive(Clone, Copy)]
pub struct Sizes {
    pub titlebar: Pixels,      // 40
    pub sidebar: Pixels,       // 260
    pub column: Pixels,        // 760: the reading column
    pub row: Pixels,           // 32: sidebar rows, menu items
    pub control: Pixels,       // 28: pills, small buttons
    pub icon_sm: Pixels,       // 14
    pub icon: Pixels,          // 16
    pub composer_max: Pixels,  // the composer's tallest input
    pub palette: Pixels,       // command palette width
    pub picker: Pixels,        // model picker width
    pub hairline: Pixels,      // 1
    pub dot: Pixels,           // timeline status dot
    pub line_height_chat: f32, // 1.6 for prose
    pub display: Pixels,       // 40: the welcome headline
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            titlebar: px(40.),
            sidebar: px(260.),
            column: px(760.),
            row: px(32.),
            control: px(28.),
            icon_sm: px(14.),
            icon: px(16.),
            composer_max: px(240.),
            palette: px(560.),
            picker: px(340.),
            hairline: px(1.),
            dot: px(7.),
            line_height_chat: 1.6,
            display: px(40.),
        }
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(Theme::light());
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

// ---------- Primitives (styled Divs you compose into components) ----------

/// Root window container: warm canvas, sans font, default UI text.
pub fn app_root(t: &Theme) -> Div {
    div()
        .size_full()
        .bg(t.colors.canvas)
        .text_color(t.colors.text)
        .font_family(t.font_sans.clone())
        .text_size(t.text.body_sm)
}

/// White card on canvas: hairline border, 12px radius, no shadow.
pub fn card(t: &Theme) -> Div {
    div()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .p(t.space.xl)
}

/// The one filled action per screen.
pub fn primary_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    let hover = t.colors.primary_hover;
    div()
        .px(px(15.))
        .py(px(6.))
        .rounded(t.radius.button)
        .bg(t.colors.primary)
        .text_color(t.colors.primary_fg)
        .text_size(t.text.body_sm)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label.into())
}

/// Secondary action: blue tint background, blue text.
pub fn ghost_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .px(px(15.))
        .py(px(6.))
        .rounded(t.radius.button)
        .bg(t.colors.primary_tint)
        .text_color(t.colors.primary)
        .text_size(t.text.body_sm)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .child(label.into())
}

/// Tertiary action / sidebar row: no fill until hover.
pub fn text_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    let hover = t.colors.hover;
    div()
        .px(t.space.md)
        .py(px(6.))
        .rounded(t.radius.button)
        .text_color(t.colors.text_muted)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label.into())
}

/// Status / tag pill.
pub fn pill(t: &Theme, bg: Hsla, fg: Hsla, label: impl Into<SharedString>) -> Div {
    div()
        .px(t.space.sm)
        .py(px(2.))
        .rounded(t.radius.pill)
        .bg(bg)
        .text_color(fg)
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .child(label.into())
}

/// Floating surface: menus, command palette, model picker.
pub fn popover(t: &Theme) -> Div {
    div()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .p(t.space.xs)
        .shadow(t.elevation_popover())
}
