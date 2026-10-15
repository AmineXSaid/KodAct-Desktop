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
    pub canvas: Hsla,  // page / window background (warm paper)
    pub surface: Hsla, // cards, panels, popovers
    #[allow(dead_code)] // in the palette; the sidebar now uses `ExtraColors::pane`
    pub sidebar: Hsla, // app-specific: sidebar sits between canvas and surface
    pub text: Hsla,    // ink 95%
    pub text_strong: Hsla, // ink 100% (headings)
    pub text_body: Hsla, // graphite – warm body text
    pub text_muted: Hsla, // ink 60%
    pub text_subtle: Hsla, // ink 40% (placeholders, disabled)
    pub border: Hsla,  // hairline, ink 8%
    pub hover: Hsla,   // row / ghost hover wash
    pub primary: Hsla, // the single filled-action color
    pub primary_hover: Hsla,
    pub primary_fg: Hsla,
    pub primary_tint: Hsla, // ghost CTA bg, selection wash
    pub marigold: Hsla,
    pub coral: Hsla,
    pub sky: Hsla,
    #[allow(dead_code)] // in the palette; nothing uses it right now
    pub midnight: Hsla,
}

#[derive(Clone, Copy)]
pub struct TypeScale {
    pub caption: Pixels,    // 12: whole pixels render crisp
    pub body_sm: Pixels,    // 13 – default UI text in a desktop app
    pub body: Pixels,       // 14 – the composer's text
    pub subheading: Pixels, // 18
    pub heading_sm: Pixels, // 20
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
    pub line: Pixels,   // 1: a line's ends, rounded just slightly
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
    pub font_sans: SharedString, // Anthropic Sans when on this PC, else Hanken Grotesk (`fonts.rs`)
    pub font_serif: SharedString, // Source Serif (substitute for Lyon Text)
    pub motion_ms: u64,          // 200ms ease for hovers/transitions
    // Forge additions (see the section at the end of the tokens).
    pub font_mono: SharedString, // JetBrains Mono: code, paths, commands
    pub font_display: SharedString, // large serif lines (the greeting): Anthropic Serif when on this PC, else Source Serif 4 Display
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
                body_sm: px(13.),
                body: px(14.),
                subheading: px(18.),
                heading_sm: px(20.),
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
                line: px(1.),
                small: px(4.),
                button: px(8.),
                card: px(12.),
                pill: px(9999.),
            },
            font_sans: crate::fonts::sans(),
            font_serif: "Source Serif 4".into(),
            motion_ms: 200,
            font_mono: "JetBrains Mono".into(),
            font_display: crate::fonts::display(),
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
    pub inset: Hsla,   // a quiet well inside a card: code blocks, terminal output
    pub success: Hsla, // text/icon on success
    pub success_tint: Hsla,
    pub danger: Hsla,
    pub danger_tint: Hsla,
    pub warning: Hsla, // "needs you"
    pub warning_tint: Hsla,
    pub diff_add: Hsla, // added line background
    pub diff_del: Hsla, // removed line background
    pub scrim: Hsla,    // behind the command palette
    pub selected: Hsla, // selected row: a white sticky note on the sidebar
    // The window's three surfaces, a clear step apart so each area reads as
    // its own: the title bar darkest, then the sidebar, then the session.
    // Cards and the composer sit on top in `surface`.
    pub chrome: Hsla, // the title bar
    pub pane: Hsla,   // the sidebar
    pub stage: Hsla,  // the session: the chat, the welcome, Rules, Projects
    // Filled buttons. Light uses `primary` as is; in dark, white text on
    // `primary` reads at only 3.3:1, so the fill is the same blue, deeper,
    // and links keep `primary` (`palette_check.rs` holds both to 4.5:1).
    pub fill: Hsla,
    pub fill_hover: Hsla,
    pub fill_danger: Hsla, // a destructive confirm ("Allow bypass")
    // Permission modes, as Forge for VS Code tints them (forge-tokens.css).
    pub mode_edit: Hsla,   // Edit automatically
    pub mode_plan: Hsla,   // Plan
    pub mode_bypass: Hsla, // Bypass permissions
    pub mode_expert: Hsla, // Expert
    // A model's ping chip: answered fast, fair, slow (never red).
    pub ping_fast_bg: Hsla,
    pub ping_fast_fg: Hsla,
    pub ping_fair_bg: Hsla,
    pub ping_fair_fg: Hsla,
    pub ping_slow_bg: Hsla,
    pub ping_slow_fg: Hsla,
    // The terminal dock's 16 colours: black, red, green, yellow, blue,
    // magenta, cyan, white, then the bright eight. Forge for VS Code's
    // (forge-tokens.css, Pajamas stops), on its terminal ink
    // (`terminal.background`); "white" is a grey in light so it never vanishes.
    // One divergence: light's yellows sit a stop darker (orange-600/500), as
    // orange-400 reads at only 2.8:1 on the light ink; bright cyan is blue-500
    // for the same reason.
    pub ansi: [Hsla; 16],
}

impl ExtraColors {
    fn for_mode(is_dark: bool) -> Self {
        if is_dark {
            Self {
                inset: hsla(0., 0., 1., 0.035),
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
                // OKLab L 0.169 / 0.197 / 0.223, then the card at 0.248:
                // even steps of 0.026, one warm hue.
                chrome: rgb(0x100f0e).into(),
                pane: rgb(0x161514).into(),
                stage: rgb(0x1c1b19).into(),
                fill: rgb(0x0069c2).into(),        // 5.5:1 under white
                fill_hover: rgb(0x0b74cd).into(),  // 4.8:1, a step lighter
                fill_danger: rgb(0xb7331f).into(), // 6.0:1
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
                ansi: [
                    rgb(0x646163).into(),
                    rgb(0xec5941).into(),
                    rgb(0x2da160).into(),
                    rgb(0xd99530).into(),
                    rgb(0x428fdc).into(),
                    rgb(0x9475db).into(),
                    rgb(0x63a6e9).into(),
                    rgb(0xc1bfbe).into(),
                    rgb(0x747273).into(),
                    rgb(0xf6806d).into(),
                    rgb(0x52b87a).into(),
                    rgb(0xe9be74).into(),
                    rgb(0x63a6e9).into(),
                    rgb(0xac93e6).into(),
                    rgb(0x9dc7f1).into(),
                    rgb(0xecebea).into(),
                ],
            }
        } else {
            Self {
                inset: ink(0.028),
                success: rgb(0x1f7a35).into(),
                success_tint: hsla(134. / 360., 0.55, 0.4, 0.12),
                danger: rgb(0xc42a14).into(),
                danger_tint: hsla(7. / 360., 0.92, 0.58, 0.12),
                warning: rgb(0x8a5600).into(),
                warning_tint: hsla(40. / 360., 1., 0.53, 0.2),
                diff_add: hsla(134. / 360., 0.55, 0.4, 0.12),
                diff_del: hsla(7. / 360., 0.92, 0.58, 0.1),
                scrim: hsla(30. / 360., 0.1, 0.05, 0.42), // the dialog stands clear of the chat
                selected: rgb(0xffffff).into(),
                // OKLab L 0.926 / 0.953 / 0.977, then the white card:
                // even steps of 0.025, one warm hue, chroma falling to 0.
                chrome: rgb(0xe9e6e1).into(),
                pane: rgb(0xf1efeb).into(),
                stage: rgb(0xf9f7f5).into(),
                fill: rgb(0x0075de).into(), // `primary`: 4.6:1 under white
                fill_hover: rgb(0x005bab).into(), // `primary_hover`: 6.8:1
                fill_danger: rgb(0xa32c12).into(), // 7.2:1
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
                ansi: [
                    rgb(0x28272d).into(),
                    rgb(0xc02f12).into(),
                    rgb(0x2f7549).into(),
                    rgb(0x995715).into(),
                    rgb(0x2f68b4).into(),
                    rgb(0x6a4fb4).into(),
                    rgb(0x1f75cb).into(),
                    rgb(0x747273).into(),
                    rgb(0x646163).into(),
                    rgb(0xdd2b0e).into(),
                    rgb(0x108548).into(),
                    rgb(0xab6100).into(),
                    rgb(0x1f75cb).into(),
                    rgb(0x7b58cf).into(),
                    rgb(0x1f75cb).into(),
                    rgb(0x18171d).into(),
                ],
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
                ansi: [
                    0x646163, 0xec5941, 0x2da160, 0xd99530, 0x428fdc, 0x9475db, 0x63a6e9, 0xc1bfbe,
                ]
                .map(|c| rgb(c).into()),
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
                ansi: [
                    0x28272d, 0xc02f12, 0x2f7549, 0xab6100, 0x2f68b4, 0x6a4fb4, 0x1f75cb, 0x747273,
                ]
                .map(|c| rgb(c).into()),
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
            Self {
                keyword: c(0xac93e6),
                string: c(0xe9be74),
                number: c(0xd99530),
                comment: c(0x8a8888),
                function: c(0x63a6e9),
                type_: c(0x52b87a),
                variable: c(0xcbbbf2),
                operator: c(0xf6806d),
                meta: c(0x8a8888),
                added: c(0x52b87a),
                removed: c(0xf6806d),
            }
        } else {
            Self {
                keyword: c(0x6a4fb4),
                string: c(0x995715),
                number: c(0xab6100),
                comment: c(0x747273),
                function: c(0x2f68b4),
                type_: c(0x2f7549),
                variable: c(0x7b58cf),
                operator: c(0xc02f12),
                meta: c(0x747273),
                added: c(0x2f7549),
                removed: c(0xc02f12),
            }
        }
    }
}

/// The app's fixed dimensions.
#[derive(Clone, Copy)]
pub struct Sizes {
    pub titlebar: Pixels, // 36
    pub sidebar: Pixels,  // 240
    pub column: Pixels,   // 680: the reading column
    pub row: Pixels,      // 30: sidebar rows, menu items
    pub control: Pixels,  // 26: pills, small buttons
    // Icons: one scale, 12 / 14 / 16 / 20, and Lucide's own 2px stroke.
    pub icon_xs: Pixels,            // 12: inside captions and chips
    pub icon_sm: Pixels,            // 14
    pub icon: Pixels,               // 16
    pub icon_lg: Pixels,            // 20: the composer's +
    pub composer_max: Pixels,       // the composer's tallest input
    pub composer_radius: Pixels,    // 16: the composer box's corners
    pub composer_width: Pixels,     // 560: the composer's widest, wherever it sits
    pub palette: Pixels,            // command palette width
    pub hairline: Pixels,           // 1
    pub line_height_chat: f32,      // 20px lines at 14px: whole pixels, crisp
    pub chat_text: Pixels,          // 14: replies and your messages, on whole pixels
    pub greeting: Pixels,           // 28: the new-chat greeting, in the display serif
    pub statusbar: Pixels,          // 26: the status bar at the foot of the window
    pub panel_in_ms: u64,           // 280: the side panel sliding in
    pub pop_in_ms: u64,             // 160: a menu or dialog rising into place
    pub stagger_ms: u64,            // 18: each row of an opening list starting after the one above
    pub edge_draw_ms: u64,          // 140: a window's jewel edge drawing down as it opens
    pub edge_bar: Pixels,           // 3: that jewel edge's width
    pub remote_menu: Pixels,        // 288: the remote quick pick's width
    pub terminal_text: Pixels,      // 13: the terminal dock's monospace
    pub terminal_min: Pixels,       // 280: the terminal dock's narrowest
    pub chat_min: Pixels,           // 400: the chat's narrowest beside the panel and the dock
    pub doc_text: Pixels,           // 15: a document in the side panel
    pub doc_title: Pixels,          // 28: its title, in the display serif
    pub line_height_doc: f32,       // 1.65: a document's leading
    pub doc_pane: Pixels,           // 460: the side panel's narrowest
    pub doc_measure: Pixels,        // 680: a document's widest line
    pub rules_list: Pixels,         // 340: the Rules screen's file list
    pub editor: Pixels,             // 420: a file editor's height
    pub composer_clearance: Pixels, // 150: toasts sit above the composer, its chips included
    pub attach_thumb: Pixels,       // 28: an attachment chip's picture or icon
    pub attach_chip: Pixels,        // 220: an attachment chip's widest
    pub popover_list: Pixels,       // 300: the tallest an autocomplete or context list grows
    pub completion_width: Pixels,   // 460: the / and @ lists, opened at the caret
    pub settings_nav: Pixels,       // 200: the Settings window's section list
    pub prompt_box: Pixels,         // 180: a prompt or instructions box in a Settings editor
    pub viewport_margin: Pixels, // 12: the gap a menu or dialog keeps from the window's edge; past it, it scrolls
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            titlebar: px(36.),
            sidebar: px(240.),
            column: px(680.),
            row: px(30.),
            control: px(26.),
            icon_xs: px(12.),
            icon_sm: px(14.),
            icon: px(16.),
            icon_lg: px(20.),
            composer_max: px(240.),
            composer_radius: px(16.),
            composer_width: px(560.),
            palette: px(560.),
            hairline: px(1.),
            line_height_chat: 20. / 14.,
            chat_text: px(14.),
            greeting: px(28.),
            statusbar: px(26.),
            panel_in_ms: 280,
            pop_in_ms: 160,
            stagger_ms: 18,
            edge_draw_ms: 140,
            edge_bar: px(3.),
            remote_menu: px(288.),
            terminal_text: px(13.),
            terminal_min: px(280.),
            doc_text: px(15.),
            doc_title: px(28.),
            line_height_doc: 1.65,
            doc_pane: px(460.),
            chat_min: px(400.),
            doc_measure: px(680.),
            rules_list: px(340.),
            editor: px(420.),
            composer_clearance: px(150.),
            attach_thumb: px(28.),
            attach_chip: px(220.),
            popover_list: px(300.),
            completion_width: px(460.),
            settings_nav: px(200.),
            prompt_box: px(180.),
            viewport_margin: px(12.),
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
    let hover = t.extra.fill_hover;
    div()
        .px(px(15.))
        .py(px(6.))
        .rounded(t.radius.button)
        .bg(t.extra.fill)
        .text_color(t.colors.primary_fg)
        .text_size(t.text.body_sm)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label.into())
}

/// A destructive confirm (Delete): the danger fill, a shade deeper on hover.
pub fn danger_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .px(px(15.))
        .py(px(6.))
        .rounded(t.radius.button)
        .bg(t.extra.fill_danger)
        .text_color(t.colors.primary_fg)
        .text_size(t.text.body_sm)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
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

/// A divider: a hairline with slightly rounded ends, inset from its
/// container's sides so the ends show. Every straight line uses
/// `radius.line`; borders around a box are the only square lines.
pub fn rule(t: &Theme) -> Div {
    div()
        .flex_none()
        .h(t.size.hairline)
        .mx(t.space.sm)
        .rounded(t.radius.line)
        .bg(t.colors.border)
}

/// The upright divider between controls in a row.
pub fn vrule(t: &Theme) -> Div {
    div()
        .flex_none()
        .w(t.size.hairline)
        .rounded(t.radius.line)
        .bg(t.colors.border)
}

/// The tallest a menu or dialog may grow when its top sits `top` below the
/// window's top: the window's height less that and a margin. Past it, the
/// surface scrolls (`fit_window`).
pub fn fit_height(window: &gpui::Window, t: &Theme, top: Pixels) -> Pixels {
    (window.viewport_size().height - top - t.size.viewport_margin).max(t.size.row * 3.)
}

/// Cap a menu or dialog at the window's height (from `top` down) and let it
/// scroll past that, so a short window never cuts it off. A fixed cap
/// (`cap`) still applies when the window is taller.
pub fn fit_window(
    el: gpui::Stateful<Div>,
    window: &gpui::Window,
    t: &Theme,
    top: Pixels,
    cap: Option<Pixels>,
) -> gpui::Stateful<Div> {
    let fit = fit_height(window, t, top);
    el.max_h(cap.map_or(fit, |c| c.min(fit)))
        .overflow_y_scroll()
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
