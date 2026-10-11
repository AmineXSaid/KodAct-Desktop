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
    #[allow(dead_code)] // in the palette; nothing is coral right now
    pub coral: Hsla,
    pub sky: Hsla,
    #[allow(dead_code)] // in the palette; nothing uses it right now
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
    pub terminal: TerminalColors,
    pub code: CodeColors,
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
            terminal: TerminalColors::for_mode(is_dark),
            code: CodeColors::for_mode(is_dark),
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
    // Permission modes, as Forge for VS Code tints them (forge-tokens.css).
    pub mode_edit: Hsla,     // Edit automatically
    pub mode_plan: Hsla,     // Plan
    pub mode_bypass: Hsla,   // Bypass permissions
    pub mode_expert: Hsla,   // Expert
    // A model's ping chip: answered fast, fair, slow (never red).
    pub ping_fast_bg: Hsla,
    pub ping_fast_fg: Hsla,
    pub ping_fair_bg: Hsla,
    pub ping_fair_fg: Hsla,
    pub ping_slow_bg: Hsla,
    pub ping_slow_fg: Hsla,
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
                mode_edit: rgb(0x2da160).into(),
                mode_plan: rgb(0x428fdc).into(),
                mode_bypass: rgb(0xec5941).into(),
                mode_expert: rgb(0xd99530).into(),
                ping_fast_bg: hsla(148. / 360., 0.78, 0.29, 0.22),
                ping_fast_fg: rgb(0x91d4a8).into(),
                ping_fair_bg: hsla(30. / 360., 0.04, 0.92, 0.1),
                ping_fair_fg: rgb(0xdcdbd9).into(),
                ping_slow_bg: hsla(34. / 360., 1., 0.34, 0.22),
                ping_slow_fg: rgb(0xe9be74).into(),
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
                mode_edit: rgb(0x108548).into(),
                mode_plan: rgb(0x1f75cb).into(),
                mode_bypass: rgb(0xa32c12).into(),
                mode_expert: rgb(0xc17d10).into(),
                ping_fast_bg: rgb(0xc3e6cd).into(),
                ping_fast_fg: rgb(0x306440).into(),
                ping_fair_bg: rgb(0xdcdbd9).into(),
                ping_fair_fg: rgb(0x4d4b4e).into(),
                ping_slow_bg: rgb(0xf5d9a8).into(),
                ping_slow_fg: rgb(0x894b16).into(),
            }
        }
    }
}

/// The shell tool's terminal block: Forge's terminal palette (the same
/// Pajamas stops Forge for VS Code uses). Light themes get the darker stops.
#[derive(Clone, Copy)]
pub struct TerminalColors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub divider: Hsla,
    pub prompt: Hsla, // the `$`, and the running cursor
    pub command: Hsla,
    pub keyword: Hsla,
    pub flag: Hsla,
    pub string: Hsla,
    pub variable: Hsla,
    pub operator: Hsla,
    pub number: Hsla,
    pub comment: Hsla,
    /// Output colours, by ANSI number: black, red, green, yellow, blue,
    /// magenta, cyan, white. A program's red or green is Forge's.
    pub ansi: [Hsla; 8],
}

impl TerminalColors {
    fn for_mode(is_dark: bool) -> Self {
        if is_dark {
            Self {
                background: rgb(0x18171d).into(),
                foreground: rgb(0xdcdbd9).into(),
                muted: rgb(0x8a8888).into(),
                divider: hsla(30. / 360., 0.04, 0.92, 0.1),
                prompt: rgb(0xac93e6).into(),
                command: rgb(0x52b87a).into(),
                keyword: rgb(0xac93e6).into(),
                flag: rgb(0x63a6e9).into(),
                string: rgb(0xe9be74).into(),
                variable: rgb(0xcbbbf2).into(),
                operator: rgb(0xf6806d).into(),
                number: rgb(0xd99530).into(),
                comment: rgb(0x747273).into(),
                ansi: [0x646163, 0xec5941, 0x2da160, 0xd99530, 0x428fdc, 0x9475db, 0x63a6e9, 0xc1bfbe].map(|c| rgb(c).into()),
            }
        } else {
            Self {
                background: rgb(0xecebea).into(),
                foreground: rgb(0x28272d).into(),
                muted: rgb(0x747273).into(),
                divider: hsla(250. / 360., 0.07, 0.16, 0.12),
                prompt: rgb(0x7759c2).into(),
                command: rgb(0x2f7549).into(),
                keyword: rgb(0x6a4fb4).into(),
                flag: rgb(0x2f68b4).into(),
                string: rgb(0x995715).into(),
                variable: rgb(0x7b58cf).into(),
                operator: rgb(0xc02f12).into(),
                number: rgb(0xab6100).into(),
                comment: rgb(0x747273).into(),
                ansi: [0x28272d, 0xc02f12, 0x2f7549, 0xab6100, 0x2f68b4, 0x6a4fb4, 0x1f75cb, 0x747273].map(|c| rgb(c).into()),
            }
        }
    }
}

/// Code colours, by token kind (`highlight.rs`): Forge's code palette, the
/// terminal's stops plus a blue for functions and a green for types.
#[derive(Clone, Copy)]
pub struct CodeColors {
    pub keyword: Hsla,
    pub string: Hsla,
    pub number: Hsla,
    pub comment: Hsla,
    pub function: Hsla,
    pub type_: Hsla,
    pub variable: Hsla,
    pub operator: Hsla,
    pub meta: Hsla,
    pub added: Hsla,
    pub removed: Hsla,
}

impl CodeColors {
    fn for_mode(is_dark: bool) -> Self {
        let c = |x: u32| -> Hsla { rgb(x).into() };
        if is_dark {
            Self { keyword: c(0xac93e6), string: c(0xe9be74), number: c(0xd99530), comment: c(0x8a8888), function: c(0x63a6e9), type_: c(0x52b87a), variable: c(0xcbbbf2), operator: c(0xf6806d), meta: c(0x8a8888), added: c(0x52b87a), removed: c(0xf6806d) }
        } else {
            Self { keyword: c(0x6a4fb4), string: c(0x995715), number: c(0xab6100), comment: c(0x747273), function: c(0x2f68b4), type_: c(0x2f7549), variable: c(0x7b58cf), operator: c(0xc02f12), meta: c(0x747273), added: c(0x2f7549), removed: c(0xc02f12) }
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
    pub hairline: Pixels,      // 1
    pub line_height_chat: f32, // 1.45 for prose
    pub chat_text: Pixels,     // 13: replies and your messages, as Forge for VS Code
    pub display: Pixels,       // 40: the welcome headline
    pub rules_list: Pixels,    // 340: the Rules screen's file list
    pub editor: Pixels,        // 420: a file editor's height
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
            hairline: px(1.),
            line_height_chat: 1.45,
            chat_text: px(13.),
            display: px(40.),
            rules_list: px(340.),
            editor: px(420.),
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
