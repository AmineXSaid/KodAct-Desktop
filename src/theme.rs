//! Warm-ivory design tokens for GPUI: a calm desktop palette (ivory paper,
//! warm ink, one patina-teal accent), softly squared shapes and a 4px rhythm.
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
    /// The hero highlight pill behind one word of a headline (peach).
    #[allow(dead_code)] // in the palette; the greeting no longer marks a word
    pub peach: Hsla,
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
    pub xs: Pixels,    // 4
    pub tight: Pixels, // 6: an icon and its label
    pub sm: Pixels,    // 8
    pub snug: Pixels,  // 10: inside compact controls
    pub md: Pixels,    // 12
    pub lg: Pixels,    // 16
    pub loose: Pixels, // 20
    pub xl: Pixels,    // 24
    pub xxl: Pixels,   // 32
    pub xxxl: Pixels,  // 40: around the home screen's heading
}

#[derive(Clone, Copy)]
pub struct Radius {
    pub line: Pixels,    // 1: a line's ends, rounded just slightly
    pub small: Pixels,   // 4: key caps, tiny marks
    pub segment: Pixels, // 6: the selected segment inside a segmented control
    pub compact: Pixels, // 7: compact buttons and chips
    pub button: Pixels,  // 8: icon buttons, standard buttons, rows
    pub control: Pixels, // 9: outlined icon buttons, the send button
    pub card: Pixels,    // 10: cards, tool rows, menus
    pub field: Pixels,   // 12: large input surfaces
    pub pill: Pixels,    // 9999: switches, badges, avatars only
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
/// The reference's ink (#2b2a27) at `alpha`: hairlines and washes on paper.
fn warm_ink(alpha: f32) -> Hsla {
    hsla(45. / 360., 0.05, 0.16, alpha)
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
                tight: px(6.),
                sm: px(8.),
                snug: px(10.),
                md: px(12.),
                lg: px(16.),
                loose: px(20.),
                xl: px(24.),
                xxl: px(32.),
                xxxl: px(40.),
            },
            radius: Radius {
                line: px(1.),
                small: px(4.),
                segment: px(6.),
                compact: px(7.),
                button: px(8.),
                control: px(9.),
                card: px(10.),
                field: px(12.),
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
                canvas: rgb(0xf9f8f5).into(),
                surface: rgb(0xffffff).into(),
                sidebar: rgb(0xf3f1ec).into(),
                text: rgb(0x2b2a27).into(),
                text_strong: rgb(0x1f1e1c).into(),
                // The reference's secondary (#6f6c64) and muted (#9a968c),
                // a step deeper so they read on the title bar too.
                text_body: rgb(0x5e5b54).into(),
                text_muted: rgb(0x64615a).into(),
                text_subtle: rgb(0x8c887e).into(),
                border: warm_ink(0.13), // #e3e0d8 on white
                hover: warm_ink(0.055),
                // Links and selected text: the accent teal itself (6:1 on white).
                primary: rgb(0x216d70).into(),
                primary_hover: rgb(0x10585b).into(),
                primary_fg: rgb(0xffffff).into(),
                primary_tint: hsla(183. / 360., 0.55, 0.3, 0.12),
                marigold: rgb(0xffb110).into(),
                coral: rgb(0xc84b55).into(),
                sky: rgb(0x62aef0).into(),
                midnight: rgb(0x02093a).into(),
                peach: rgb(0xd0e6e7).into(),
            },
        )
    }

    /// The same room at night: warm charcoal surfaces, the same accent.
    pub fn dark() -> Self {
        Self::base(
            true,
            Colors {
                canvas: rgb(0x262624).into(),
                surface: rgb(0x30302e).into(),
                sidebar: rgb(0x1d1c1b).into(),
                text: rgb(0xecebe6).into(),
                text_strong: paper(1.0),
                text_body: rgb(0xc2bfb6).into(),
                text_muted: rgb(0xa8a59c).into(),
                text_subtle: rgb(0x7d7a72).into(),
                border: hsla(0., 0., 1., 0.1),
                hover: hsla(0., 0., 1., 0.05),
                primary: rgb(0x8ecacd).into(),
                primary_hover: rgb(0xabdbde).into(),
                primary_fg: rgb(0xffffff).into(),
                primary_tint: hsla(183. / 360., 0.4, 0.55, 0.18),
                marigold: rgb(0xffb110).into(),
                coral: rgb(0xd8646d).into(),
                sky: rgb(0x62aef0).into(),
                midnight: rgb(0x02093a).into(),
                peach: rgb(0x2f4749).into(),
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
    pub fill_fg: Hsla,     // text and icons on `fill` (ink in light, paper in dark)
    // The accent: patina teal (forged copper as it weathers), one colour in
    // both modes (`#216d70`), kept for the few places
    // it earns: the send button, the mark's core, a dot in the background field.
    pub accent: Hsla,
    pub accent_soft: Hsla, // the send button with nothing to send; a soft wash
    pub accent_fg: Hsla,
    pub border_strong: Hsla, // outlined buttons, switch tracks at rest, fields
    pub active: Hsla,        // the wash under the current row in a list (the sidebar's chat)
    pub sunk: Hsla,          // a recessed track: segmented controls, the composer's tray
    pub toggle_on: Hsla,     // a switch that is on
    // Memory and rules: the reference's ochre.
    pub memory: Hsla,
    pub memory_tint: Hsla,
    // The home screen's field of dots (`ui/dot_field.rs`).
    pub dot: Hsla,
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
                success: rgb(0x8fd3a8).into(),
                success_tint: rgb(0x20372a).into(),
                danger: rgb(0xeb8c7b).into(),
                danger_tint: hsla(7. / 360., 0.7, 0.55, 0.1),
                warning: rgb(0xffc85a).into(),
                warning_tint: hsla(40. / 360., 1., 0.53, 0.18),
                diff_add: hsla(134. / 360., 0.5, 0.5, 0.16),
                diff_del: hsla(7. / 360., 0.92, 0.58, 0.16),
                scrim: ink(0.55),
                selected: rgb(0x2a2a28).into(),
                // OKLab L 0.188 / 0.227 / 0.268, then the card at 0.309:
                // even steps of 0.041, one warm hue.
                chrome: rgb(0x141312).into(),
                pane: rgb(0x1d1c1b).into(),
                stage: rgb(0x262624).into(),
                // The filled action is paper on charcoal, as ink on paper in light.
                fill: rgb(0xecebe6).into(),
                fill_hover: rgb(0xf8f7f3).into(),
                fill_danger: rgb(0xb7331f).into(), // 6.0:1
                fill_fg: rgb(0x262624).into(),
                accent: rgb(0x216d70).into(),
                accent_soft: rgb(0x2f4749).into(),
                accent_fg: rgb(0xffffff).into(),
                border_strong: rgb(0x55534e).into(),
                active: hsla(0., 0., 1., 0.075),
                sunk: rgb(0x1f1e1d).into(),
                toggle_on: rgb(0x529a9d).into(),
                memory: rgb(0xe5c77a).into(),
                memory_tint: rgb(0x3a3220).into(),
                dot: rgb(0x9a958b).into(),
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
                inset: warm_ink(0.03),
                success: rgb(0x276b43).into(),
                success_tint: rgb(0xe2f1e7).into(),
                danger: rgb(0xb4412f).into(),
                danger_tint: hsla(9. / 360., 0.6, 0.55, 0.1),
                warning: rgb(0x8a5600).into(),
                warning_tint: hsla(40. / 360., 1., 0.53, 0.2),
                diff_add: hsla(134. / 360., 0.55, 0.4, 0.12),
                diff_del: hsla(7. / 360., 0.92, 0.58, 0.1),
                scrim: hsla(30. / 360., 0.1, 0.05, 0.42), // the dialog stands clear of the chat
                selected: rgb(0xffffff).into(),
                // OKLab L 0.937 / 0.958 / 0.979, then the white card:
                // even steps of 0.021, one warm hue, chroma falling to 0.
                chrome: rgb(0xeceae4).into(),
                pane: rgb(0xf3f1ec).into(),
                stage: rgb(0xf9f8f5).into(),
                // The filled action is ink: quiet, and never confused with the accent.
                fill: rgb(0x2b2a27).into(),
                fill_hover: rgb(0x403e3a).into(),
                fill_danger: rgb(0xb4412f).into(), // 5.6:1
                fill_fg: rgb(0xffffff).into(),
                accent: rgb(0x216d70).into(),
                accent_soft: rgb(0xd0e6e7).into(),
                accent_fg: rgb(0xffffff).into(),
                border_strong: rgb(0xcfcbc0).into(),
                active: warm_ink(0.075),
                sunk: rgb(0xf0eee8).into(),
                toggle_on: rgb(0x216d70).into(),
                memory: rgb(0x75591a).into(),
                memory_tint: rgb(0xf6ecd2).into(),
                dot: rgb(0x857e71).into(),
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
    pub sidebar: Pixels,  // 256
    pub column: Pixels,   // 680: the reading column
    pub row: Pixels,      // 30: sidebar rows, menu items
    pub control: Pixels,  // 26: pills, small buttons
    // Icons: one scale, 12 / 14 / 16 / 20, and Lucide's own 2px stroke.
    pub icon_xs: Pixels,            // 12: inside captions and chips
    pub icon_sm: Pixels,            // 14
    pub icon: Pixels,               // 16
    pub icon_lg: Pixels,            // 20: the composer's +
    pub composer_max: Pixels,       // the composer's tallest input
    pub composer_radius: Pixels,    // 14: the composer box's corners
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
    // Controls (`ui/button.rs`): hit areas and the switch's parts.
    pub hit_sm: Pixels,    // 28: an icon button in a dense row (a reply's actions)
    pub hit: Pixels,       // 34: an icon button's square
    pub hit_lg: Pixels,    // 38: the composer's outlined + and send
    pub button_h: Pixels,  // 32: a labelled button's height
    pub button_sm: Pixels, // 28: a compact labelled button
    pub switch_w: Pixels,  // 36
    pub switch_h: Pixels,  // 20
    pub switch_thumb: Pixels, // 14
    pub rail: Pixels,      // 52: the collapsed sidebar
    pub home_heading: Pixels, // 32: the home screen's heading, in the display serif
    pub dot_gap: Pixels,   // 14: the background field's grid
    pub dot: Pixels,       // 2: one dot's diameter
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            titlebar: px(36.),
            sidebar: px(256.),
            column: px(680.),
            row: px(30.),
            control: px(26.),
            icon_xs: px(12.),
            icon_sm: px(14.),
            icon: px(16.),
            icon_lg: px(20.),
            composer_max: px(240.),
            composer_radius: px(14.),
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
            hit_sm: px(28.),
            hit: px(34.),
            hit_lg: px(38.),
            button_h: px(32.),
            button_sm: px(28.),
            switch_w: px(36.),
            switch_h: px(20.),
            switch_thumb: px(14.),
            rail: px(52.),
            home_heading: px(32.),
            dot_gap: px(14.),
            dot: px(2.),
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

/// A labelled button as a plain `Div`, for callers that add their own id and
/// click: the same paint and shape as `ui::button::Button`, so there is one
/// button system. Prefer `Button` for new code (it also takes keyboard focus).
fn styled_button(t: &Theme, variant: crate::ui::button::Variant, label: SharedString) -> Div {
    let p = variant.paint(t);
    let hover = p.hover_bg;
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(t.space.tight)
        .h(t.size.button_h)
        .px(px(14.))
        .rounded(t.radius.button)
        .border_1()
        .border_color(p.border.unwrap_or(t.colors.surface.opacity(0.)))
        .bg(p.bg)
        .text_color(p.fg)
        .text_size(t.text.body_sm)
        .when(
            matches!(
                variant,
                crate::ui::button::Variant::Primary | crate::ui::button::Variant::Danger
            ),
            |d| d.font_weight(FontWeight::MEDIUM),
        )
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label)
}

/// The one filled action per surface: ink on paper.
pub fn primary_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    styled_button(t, crate::ui::button::Variant::Primary, label.into())
}

/// A destructive confirm (Delete): the danger fill, a shade deeper on hover.
pub fn danger_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    styled_button(t, crate::ui::button::Variant::Danger, label.into())
}

/// Secondary action: a bordered surface button (Allow once, Save, Cancel).
pub fn ghost_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    styled_button(t, crate::ui::button::Variant::Secondary, label.into())
}

/// Tertiary action: no fill until hover.
pub fn text_button(t: &Theme, label: impl Into<SharedString>) -> Div {
    styled_button(t, crate::ui::button::Variant::Ghost, label.into())
        .text_color(t.colors.text_muted)
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
