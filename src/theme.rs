//! Design tokens for GPUI: KodAct's identity, shared with KodAct CLI
//! (its `tui/palette.rs`). Ink, paper and lime: ink grounds stepping up
//! from the sidebar to the session to cards (paper ones by day), paper text
//! in two voices, and lime for KodAct acting and what needs you (olive on
//! paper, where lime would not read). Teal and violet carry structure, peach
//! marks code, and green, red and amber only ever mean done, failed and a
//! warning. KodAct Sans for words, KodAct Mono for code (`fonts.rs`).
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
    pub segment: Pixels, // pill: the selected segment inside a segmented control
    pub compact: Pixels, // pill: compact buttons and chips
    pub button: Pixels,  // 4: rows and the squarer buttons
    pub control: Pixels, // pill: outlined icon buttons, the send button
    pub card: Pixels,    // 4: cards, tool rows, menus
    pub field: Pixels,   // 4: large input surfaces
    pub pill: Pixels,    // 9999: buttons, badges, avatars
}

/// KodAct's look, light and dark: its surfaces and its one colour
/// (`tint`), and its form: corners, density, depth (`form`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Look {
    /// Ink, paper and lime, as KodAct CLI draws them.
    #[default]
    KodAct,
}

impl Look {
    #[cfg(test)]
    pub const ALL: [Look; 1] = [Look::KodAct];

    /// How it is saved (`window.json`).
    pub fn id(self) -> &'static str {
        "kodact"
    }

    /// A saved theme: whatever was saved (Forge's "calm" included), it is
    /// KodAct's look now.
    pub fn from_id(_id: &str) -> Option<Look> {
        Some(Look::KodAct)
    }

    #[cfg(test)]
    pub fn name(self) -> &'static str {
        "KodAct"
    }

    /// Everything but colour: corners, density, depth.
    fn form(self) -> Form {
        Form {
            display_weight: FontWeight::SEMIBOLD,
            // (button, card, field, composer, pressed: buttons and chips)
            corners: (4., 4., 4., 4., 9999.),
            row: 30.,
            titlebar: 36.,
            // Denser, as Claude Code sets a reply: a third more on screen.
            chat: (15., 24.),
            heading: (32., 36.),
            motion_ms: 200,
        }
    }

    /// Its surfaces and colour in one mode.
    fn tint(self, dark: bool) -> Tint {
        // KodAct CLI's roles (`tui/palette.rs`): the ink of its tiles for
        // grounds, lime #c6ff3d for the brand (its tile #b5ea2c by day, and
        // olive #4a7200 for the brand drawn as a line or a word on paper).
        if dark {
            Tint {
                canvas: 0x0f1113,
                stage: 0x15181b,
                card: 0x1c2024,
                sunk: 0x121417,
                accent: 0xc6ff3d,
                accent_fg: 0x101214,
                accent_soft: 0x2a341f,
                link: 0xc6ff3d,
                link_hover: 0xdcff85,
                live: 0xc6ff3d,
                mark_top: 0xc6ff3d,
            }
        } else {
            Tint {
                canvas: 0xe9e8e2,
                stage: 0xf6f5f1,
                card: 0xfdfdfb,
                sunk: 0xeeede8,
                accent: 0xb5ea2c,
                accent_fg: 0x101214,
                accent_soft: 0xe4efcd,
                link: 0x4a7200,
                link_hover: 0x3d5f00,
                live: 0x4a7200,
                mark_top: 0xb5ea2c,
            }
        }
    }
}

/// The look's form.
#[derive(Clone, Copy)]
struct Form {
    display_weight: FontWeight,
    corners: (f32, f32, f32, f32, f32),
    row: f32,
    titlebar: f32,
    /// Chat text and its line, in pixels.
    chat: (f32, f32),
    /// The greeting and the home screen's heading.
    heading: (f32, f32),
    motion_ms: u64,
}

/// One mode of a theme, as hex: the canvas (title bar and sidebar), the
/// session, cards, a recessed well; the brand as a fill (the mark's tile,
/// Send, the primary button), the ink on it, its pale wash; links and their
/// hover; the brand as a line or a word (what is live); the mark's tile.
#[derive(Clone, Copy)]
struct Tint {
    canvas: u32,
    stage: u32,
    card: u32,
    sunk: u32,
    accent: u32,
    accent_fg: u32,
    accent_soft: u32,
    link: u32,
    link_hover: u32,
    live: u32,
    mark_top: u32,
}

#[derive(Clone)]
pub struct Theme {
    pub is_dark: bool,
    /// Which theme this is (Settings > Theme).
    pub look: Look,
    pub colors: Colors,
    pub text: TypeScale,
    pub space: Space,
    pub radius: Radius,
    pub font_sans: SharedString, // KodAct Sans: every word but code (`fonts.rs`)
    pub motion_ms: u64,          // 200ms ease for hovers/transitions
    // KodAct additions (see the section at the end of the tokens).
    pub font_mono: SharedString, // code, paths, commands, keys: KodAct Mono
    pub font_code_inline: SharedString, // a name in prose: the code face
    pub font_display: SharedString, // large lines (headings): the interface face, set heavier
    pub display_weight: FontWeight, // 600: headings are set in the heavier of the two cuts
    pub font_reading: SharedString, // a reply's prose: the interface face
    pub extra: ExtraColors,
    pub terminal: TerminalColors,
    pub code: CodeColors,
    pub size: Sizes,
}

impl Global for Theme {}

fn ink(alpha: f32) -> Hsla {
    hsla(0., 0., 0., alpha)
}

impl Theme {
    fn base(is_dark: bool, colors: Colors) -> Self {
        Self {
            is_dark,
            look: Look::KodAct,
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
                // Buttons, chips and icon buttons are pills; cards, rows,
                // menus and fields are 4px: the contrast is the system.
                segment: px(9999.),
                compact: px(9999.),
                button: px(4.),
                control: px(9999.),
                card: px(4.),
                field: px(4.),
                pill: px(9999.),
            },
            font_sans: crate::fonts::TEXT.into(),
            motion_ms: 200,
            font_mono: crate::fonts::MONO.into(),
            font_code_inline: crate::fonts::MONO.into(),
            font_display: crate::fonts::DISPLAY.into(),
            display_weight: FontWeight::SEMIBOLD,
            font_reading: crate::fonts::TEXT.into(),
            extra: ExtraColors::for_mode(is_dark),
            terminal: TerminalColors::for_mode(is_dark),
            code: CodeColors::for_mode(is_dark),
            size: Sizes::default(),
        }
    }

    /// A theme in one mode: the system's neutrals and type, its surfaces and colour.
    pub fn of(look: Look, dark: bool) -> Self {
        let mut t = if dark { Self::dark() } else { Self::light() };
        t.apply(look);
        t
    }

    fn apply(&mut self, look: Look) {
        let k = look.tint(self.is_dark);
        let c = |v: u32| -> Hsla { rgb(v).into() };
        self.look = look;
        (self.extra.chrome, self.extra.pane, self.colors.sidebar) =
            (c(k.canvas), c(k.canvas), c(k.canvas));
        (self.extra.stage, self.colors.canvas, self.extra.selected) =
            (c(k.stage), c(k.stage), c(k.stage));
        self.colors.surface = c(k.card);
        self.extra.sunk = c(k.sunk);
        self.extra.accent = c(k.accent);
        self.extra.accent_fg = c(k.accent_fg);
        self.extra.accent_soft = c(k.accent_soft);
        self.extra.toggle_on = c(k.link);
        // What is live is the brand drawn as a line (lime on ink, olive on
        // paper); agents carry no colour of their own.
        (self.extra.live, self.extra.agent) = (c(k.live), None);
        self.extra.mark_top = c(k.mark_top);
        self.extra.page_ink = c(look.tint(false).link);
        self.colors.primary = c(k.link);
        self.colors.primary_hover = c(k.link_hover);
        self.colors.primary_tint = c(k.link).opacity(if self.is_dark { 0.16 } else { 0.14 });
        // The primary button is the brand's fill, under ink, as KodAct CLI's tile.
        (self.extra.fill, self.extra.fill_fg) = (c(k.accent), c(k.accent_fg));
        self.extra.fill_hover = c(if self.is_dark { k.link_hover } else { 0xa9df1f });

        let f = look.form();
        self.display_weight = f.display_weight;
        let (button, card, field, composer, pressed) = f.corners;
        let r = &mut self.radius;
        (r.button, r.card, r.field) = (px(button), px(card), px(field));
        (r.segment, r.compact, r.control) = (px(pressed), px(pressed), px(pressed));
        let z = &mut self.size;
        z.composer_radius = px(composer);
        (z.row, z.titlebar) = (px(f.row), px(f.titlebar));
        // KodAct Sans runs large (a tall x-height): a step smaller, so a
        // reply reads at the size the look meant.
        let (size, line) = ((f.chat.0 - BRAND_STEP).max(13.), f.chat.1 - BRAND_STEP);
        z.chat_text = px(size);
        z.line_height_chat = line / size;
        (z.greeting, z.home_heading) = (px(f.heading.0), px(f.heading.1));
        self.motion_ms = f.motion_ms;
    }

    pub fn light() -> Self {
        Self::base(
            false,
            Colors {
                // Paper by day: the window a step below the session, cards
                // nearly white on it (overwritten by the look's tint).
                canvas: rgb(0xf6f5f1).into(),
                surface: rgb(0xfdfdfb).into(),
                sidebar: rgb(0xe9e8e2).into(),
                // KodAct CLI's inks on paper: yours #101214, KodAct's words
                // a step softer (#2b2e33), muted and subtle greys.
                text: rgb(0x101214).into(),
                text_strong: rgb(0x08090b).into(),
                text_body: rgb(0x2b2e33).into(),
                text_muted: rgb(0x55595e).into(),
                text_subtle: rgb(0x63676d).into(),
                border: hsla(210. / 360., 0.1, 0.07, 0.13),
                hover: hsla(210. / 360., 0.1, 0.07, 0.05),
                // Links and selected text: olive, the brand on paper.
                primary: rgb(0x4a7200).into(),
                primary_hover: rgb(0x3d5f00).into(),
                primary_fg: rgb(0xfdfdfb).into(),
                primary_tint: hsla(83. / 360., 1., 0.22, 0.14),
                marigold: rgb(0x8f6200).into(),
                coral: rgb(0xb8392e).into(),
                sky: rgb(0x2f63a8).into(),
                midnight: rgb(0x101214).into(),
                peach: rgb(0xa4461c).into(),
            },
        )
    }

    /// Ink at night: graphite grounds, paper text, lime at full strength.
    pub fn dark() -> Self {
        Self::base(
            true,
            Colors {
                canvas: rgb(0x15181b).into(),
                surface: rgb(0x1c2024).into(),
                sidebar: rgb(0x0f1113).into(),
                // Yours brightest (#f2f0e9), KodAct's words a soft paper
                // (#d9d7cf) so long replies read without glare.
                text: rgb(0xf2f0e9).into(),
                text_strong: rgb(0xfbf9f3).into(),
                text_body: rgb(0xd9d7cf).into(),
                text_muted: rgb(0xa9adb2).into(),
                text_subtle: rgb(0x868b92).into(),
                border: hsla(210. / 360., 0.12, 0.9, 0.12),
                hover: hsla(210. / 360., 0.12, 0.9, 0.055),
                primary: rgb(0xc6ff3d).into(),
                primary_hover: rgb(0xdcff85).into(),
                primary_fg: rgb(0x101214).into(),
                primary_tint: hsla(78. / 360., 1., 0.62, 0.16),
                marigold: rgb(0xf0c25e).into(),
                coral: rgb(0xff7a6b).into(),
                sky: rgb(0x8fbef0).into(),
                midnight: rgb(0x101214).into(),
                peach: rgb(0xffb98f).into(),
            },
        )
    }

    /// Popovers, menus, command palette. Content cards use borders, not shadows.
    pub fn elevation_popover(&self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: ink(if self.is_dark { 0.4 } else { 0.06 }),
                offset: point(px(0.), px(1.)),
                blur_radius: px(1.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: ink(if self.is_dark { 0.35 } else { 0.06 }),
                offset: point(px(0.), px(6.)),
                blur_radius: px(16.),
                spread_radius: px(0.),
                inset: false,
            },
        ]
    }

    /// What lifts a card or the session off the canvas: paper grain (three
    /// hairline shadows under 4px of blur and 6% ink).
    pub fn elevation_grain(&self) -> Vec<BoxShadow> {
        let d = self.is_dark;
        [(1., 0.06, 0.35), (2., 0.04, 0.2), (4., 0.01, 0.1)]
            .into_iter()
            .map(|(y, light, dark)| BoxShadow {
                color: ink(if d { dark } else { light }),
                offset: point(px(0.), px(y)),
                blur_radius: px(1.),
                spread_radius: px(0.),
                inset: false,
            })
            .collect()
    }
}

// ---------- KodAct additions ----------

impl Theme {
    /// The sidebar's shadow where it meets the session: its colour at the
    /// edge, fading out over `Sizes::side_shade`, so the sidebar reads as a
    /// layer over the window.
    pub fn side_shade(&self) -> Option<Hsla> {
        Some(ink(if self.is_dark { 0.30 } else { 0.06 }))
    }
}
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
    // Filled buttons: KodAct's lime tile under ink, in both modes (set
    // from the look's tint, `apply`).
    pub fill: Hsla,
    pub fill_hover: Hsla,
    pub fill_danger: Hsla, // a destructive confirm ("Allow bypass")
    pub fill_fg: Hsla,     // text and icons on `fill`: ink
    // The brand as a fill, lime under ink (the tile's lime by day): Send,
    // the new-chat mark. An indicator takes `live` instead.
    pub accent: Hsla,
    pub accent_soft: Hsla, // the send button with nothing to send; a soft wash
    pub accent_fg: Hsla,
    // KodAct's mark (`ui/mark.rs`): the tile (`mark_top`), the ink of its
    // cursor bar and run triangle (`mark_left`), and its shadow, one pixel
    // down and right (`mark_right`), which gives it an edge on paper.
    pub mark_top: Hsla,
    pub mark_left: Hsla,
    pub mark_right: Hsla,
    // The pixel wordmark's shadow, one pixel down and right: a dark olive
    // on ink (KodAct CLI's), the tile's lime on paper (where the CLI's sage
    // went flat).
    pub brand_shadow: Hsla,
    // Marks drawn on a design page (`ui/design_marks.rs`): the page is
    // usually light whatever the app's mode, so the ink is the look's
    // light-mode link in both modes, over a thin halo for dark pages.
    pub page_ink: Hsla,
    pub page_halo: Hsla,
    pub border_strong: Hsla, // outlined buttons, switch tracks at rest, fields
    pub sunk: Hsla,          // a recessed track: segmented controls, the composer's tray
    pub toggle_on: Hsla,     // a switch that is on
    // The brand as a line or a word: what is live (a spinner, a dot that
    // needs you, the shimmer's peak), lime on ink, olive on paper, where a
    // lime fill would not read.
    pub live: Hsla,
    // Sub-agents' own mark (their kind in the Agents panel): the theme's
    // spread of colours, except where a theme gives agents one (Premium:
    // quiet grey, no colour of their own).
    pub agent: Option<Hsla>,
    // Memory and rules: the reference's ochre.
    pub memory: Hsla,
    pub memory_tint: Hsla,
    // Inline code in prose (a file, a command): KodAct's coral on a faint
    // wash, a rounded chip (`ui/markdown.rs`).
    pub code_inline_fg: Hsla,
    pub code_inline_bg: Hsla,
    // Permission modes, as KodAct for VS Code tints them (forge-tokens.css).
    pub mode_edit: Hsla,   // Edit automatically
    pub mode_plan: Hsla,   // Plan
    pub mode_bypass: Hsla, // Bypass permissions
    // A skill named in the composer (`/code-review`): bold, in its own colour.
    pub skill: Hsla,
    // A model's ping chip: answered fast, fair, slow (never red).
    pub ping_fast_bg: Hsla,
    pub ping_fast_fg: Hsla,
    pub ping_fair_bg: Hsla,
    pub ping_fair_fg: Hsla,
    pub ping_slow_bg: Hsla,
    pub ping_slow_fg: Hsla,
    // The terminal dock's 16 colours: black, red, green, yellow, blue,
    // magenta, cyan, white, then the bright eight. KodAct for VS Code's
    // (forge-tokens.css, Pajamas stops), on its terminal ink
    // (`terminal.background`); "white" is a grey in light so it never vanishes.
    // One divergence: light's yellows sit a stop darker (orange-600/500), as
    // orange-400 reads at only 2.8:1 on the light ink; bright cyan is blue-500
    // for the same reason.
    pub ansi: [Hsla; 16],
}

impl ExtraColors {
    fn for_mode(is_dark: bool) -> Self {
        // KodAct CLI's roles: SUCCESS, WARNING, DANGER for state only; TEAL
        // and VIOLET for structure (modes, skills, memory); CODE (peach) for
        // a name in prose. The brand fields are set by the look (`apply`).
        if is_dark {
            Self {
                inset: hsla(210. / 360., 0.12, 0.9, 0.035),
                success: rgb(0x7ccf8f).into(),
                success_tint: rgb(0x1b2a20).into(),
                danger: rgb(0xff7a6b).into(),
                danger_tint: hsla(6. / 360., 1., 0.71, 0.1),
                warning: rgb(0xf0c25e).into(),
                warning_tint: hsla(41. / 360., 0.83, 0.65, 0.15),
                diff_add: hsla(135. / 360., 0.45, 0.65, 0.13),
                diff_del: hsla(6. / 360., 1., 0.71, 0.12),
                scrim: ink(0.6),
                selected: rgb(0x15181b).into(),
                // Ink at night: the sidebar and title bar deepest, the
                // session a step up, cards above it.
                chrome: rgb(0x0f1113).into(),
                pane: rgb(0x0f1113).into(),
                stage: rgb(0x15181b).into(),
                fill: rgb(0xc6ff3d).into(),
                fill_hover: rgb(0xdcff85).into(),
                fill_danger: rgb(0xff7a6b).into(),
                fill_fg: rgb(0x101214).into(),
                accent: rgb(0xc6ff3d).into(),
                accent_soft: rgb(0x2a341f).into(),
                accent_fg: rgb(0x101214).into(),
                // The mark: a lime tile, its cursor bar and run triangle in
                // ink, its shadow a dark olive (KodAct CLI's `brand.rs`).
                mark_top: rgb(0xc6ff3d).into(),
                page_ink: rgb(0x4a7200).into(),
                page_halo: rgb(0xffffff).into(),
                mark_left: rgb(0x101214).into(),
                mark_right: rgb(0x3d5f00).into(),
                brand_shadow: rgb(0x3d5a10).into(),
                border_strong: rgb(0x3a4047).into(),
                sunk: rgb(0x121417).into(),
                toggle_on: rgb(0xc6ff3d).into(),
                live: rgb(0xc6ff3d).into(),
                agent: None,
                memory: rgb(0x5fd8c4).into(),
                memory_tint: rgb(0x15302c).into(),
                code_inline_fg: rgb(0xffb98f).into(),
                code_inline_bg: hsla(210. / 360., 0.12, 0.9, 0.07),
                mode_edit: rgb(0x5fd8c4).into(),
                mode_plan: rgb(0xb4a2ff).into(),
                mode_bypass: rgb(0xff7a6b).into(),
                skill: rgb(0xb4a2ff).into(),
                ping_fast_bg: hsla(135. / 360., 0.45, 0.65, 0.16),
                ping_fast_fg: rgb(0x9adfaa).into(),
                ping_fair_bg: hsla(210. / 360., 0.12, 0.9, 0.08),
                ping_fair_fg: rgb(0xd9d7cf).into(),
                ping_slow_bg: hsla(41. / 360., 0.83, 0.65, 0.16),
                ping_slow_fg: rgb(0xf3cf7f).into(),
                // The terminal dock: KodAct CLI's roles as the 16 colours.
                ansi: [
                    rgb(0x4a5058).into(),
                    rgb(0xff7a6b).into(),
                    rgb(0x7ccf8f).into(),
                    rgb(0xf0c25e).into(),
                    rgb(0x8fbef0).into(),
                    rgb(0xb4a2ff).into(),
                    rgb(0x5fd8c4).into(),
                    rgb(0xd9d7cf).into(),
                    rgb(0x868b92).into(),
                    rgb(0xff9d92).into(),
                    rgb(0xa8e3b6).into(),
                    rgb(0xf7d58a).into(),
                    rgb(0xb7d6f6).into(),
                    rgb(0xcfc4ff).into(),
                    rgb(0x93e7d9).into(),
                    rgb(0xfbf9f3).into(),
                ],
            }
        } else {
            Self {
                inset: hsla(210. / 360., 0.1, 0.07, 0.035),
                success: rgb(0x2a6f3e).into(),
                success_tint: rgb(0xe2efe5).into(),
                danger: rgb(0xb8392e).into(),
                danger_tint: hsla(5. / 360., 0.6, 0.45, 0.1),
                warning: rgb(0x8f6200).into(),
                warning_tint: hsla(41. / 360., 1., 0.5, 0.17),
                diff_add: hsla(137. / 360., 0.44, 0.33, 0.10),
                diff_del: hsla(5. / 360., 0.6, 0.45, 0.09),
                scrim: hsla(210. / 360., 0.12, 0.07, 0.4),
                selected: rgb(0xf6f5f1).into(),
                chrome: rgb(0xe9e8e2).into(),
                pane: rgb(0xe9e8e2).into(),
                stage: rgb(0xf6f5f1).into(),
                fill: rgb(0xb5ea2c).into(),
                fill_hover: rgb(0xa9df1f).into(),
                fill_danger: rgb(0xb8392e).into(),
                fill_fg: rgb(0x101214).into(),
                accent: rgb(0xb5ea2c).into(),
                accent_soft: rgb(0xe4efcd).into(),
                accent_fg: rgb(0x101214).into(),
                mark_top: rgb(0xb5ea2c).into(),
                page_ink: rgb(0x4a7200).into(),
                page_halo: rgb(0xffffff).into(),
                mark_left: rgb(0x101214).into(),
                mark_right: rgb(0x4a7200).into(),
                brand_shadow: rgb(0xb5ea2c).into(),
                border_strong: rgb(0xc5c8cb).into(),
                sunk: rgb(0xeeede8).into(),
                toggle_on: rgb(0x4a7200).into(),
                live: rgb(0x4a7200).into(),
                agent: None,
                memory: rgb(0x0e7568).into(),
                memory_tint: rgb(0xdcefeb).into(),
                code_inline_fg: rgb(0xa4461c).into(),
                code_inline_bg: hsla(210. / 360., 0.1, 0.07, 0.06),
                mode_edit: rgb(0x0e7568).into(),
                mode_plan: rgb(0x5a44c0).into(),
                mode_bypass: rgb(0xb8392e).into(),
                skill: rgb(0x5a44c0).into(),
                ping_fast_bg: rgb(0xd3ebd9).into(),
                ping_fast_fg: rgb(0x265f37).into(),
                ping_fair_bg: rgb(0xe3e3e0).into(),
                ping_fair_fg: rgb(0x3d4146).into(),
                ping_slow_bg: rgb(0xf6e4b8).into(),
                ping_slow_fg: rgb(0x7a5300).into(),
                ansi: [
                    rgb(0x101214).into(),
                    rgb(0xb8392e).into(),
                    rgb(0x2f7a45).into(),
                    rgb(0x8f6200).into(),
                    rgb(0x2f63a8).into(),
                    rgb(0x5a44c0).into(),
                    rgb(0x0e7568).into(),
                    rgb(0x55595e).into(),
                    rgb(0x63676d).into(),
                    rgb(0xa8322a).into(),
                    rgb(0x2a6e3e).into(),
                    rgb(0x805700).into(),
                    rgb(0x2a5896).into(),
                    rgb(0x513cad).into(),
                    rgb(0x0c685d).into(),
                    rgb(0x2b2e33).into(),
                ],
            }
        }
    }
}

/// The shell tool's terminal block: KodAct's terminal palette (the same
/// Pajamas stops KodAct for VS Code uses). Light themes get the darker stops.
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
    /// magenta, cyan, white. A program's red or green is KodAct's.
    pub ansi: [Hsla; 8],
}

impl TerminalColors {
    fn for_mode(is_dark: bool) -> Self {
        // KodAct CLI's code roles; the prompt and the running cursor in the
        // brand, as the CLI's prompt mark.
        if is_dark {
            Self {
                background: rgb(0x101214).into(),
                foreground: rgb(0xd9d7cf).into(),
                muted: rgb(0x868b92).into(),
                divider: hsla(210. / 360., 0.12, 0.9, 0.1),
                prompt: rgb(0xc6ff3d).into(),
                command: rgb(0x5fd8c4).into(),
                keyword: rgb(0xb4a2ff).into(),
                flag: rgb(0x8fbef0).into(),
                string: rgb(0xc9ec7a).into(),
                variable: rgb(0xcfc4ff).into(),
                operator: rgb(0xa9adb2).into(),
                number: rgb(0xf0c25e).into(),
                comment: rgb(0x868b92).into(),
                ansi: [
                    0x4a5058, 0xff7a6b, 0x7ccf8f, 0xf0c25e, 0x8fbef0, 0xb4a2ff, 0x5fd8c4, 0xd9d7cf,
                ]
                .map(|c| rgb(c).into()),
            }
        } else {
            Self {
                background: rgb(0xeeede8).into(),
                foreground: rgb(0x2b2e33).into(),
                muted: rgb(0x5d6167).into(),
                divider: hsla(210. / 360., 0.1, 0.07, 0.12),
                prompt: rgb(0x4a7200).into(),
                command: rgb(0x0e7568).into(),
                keyword: rgb(0x5a44c0).into(),
                flag: rgb(0x2f63a8).into(),
                string: rgb(0x4f7000).into(),
                variable: rgb(0x513cad).into(),
                operator: rgb(0x55595e).into(),
                number: rgb(0x805700).into(),
                comment: rgb(0x5d6167).into(),
                ansi: [
                    0x101214, 0xb8392e, 0x2f7a45, 0x805700, 0x2f63a8, 0x5a44c0, 0x0e7568, 0x55595e,
                ]
                .map(|c| rgb(c).into()),
            }
        }
    }
}

/// Code colours, by token kind (`highlight.rs`): KodAct's code palette, the
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
        // KodAct CLI's code colours (`CODE_*`): violet keywords, lime-green
        // strings, amber numbers, teal types; blue for functions.
        let c = |x: u32| -> Hsla { rgb(x).into() };
        if is_dark {
            Self {
                keyword: c(0xb4a2ff),
                string: c(0xc9ec7a),
                number: c(0xf0c25e),
                comment: c(0x868b92),
                function: c(0x8fbef0),
                type_: c(0x5fd8c4),
                variable: c(0xcdd1d5),
                operator: c(0xa9adb2),
                meta: c(0x868b92),
                added: c(0x7ccf8f),
                removed: c(0xff7a6b),
            }
        } else {
            Self {
                keyword: c(0x5a44c0),
                string: c(0x4f7000),
                number: c(0x805700),
                comment: c(0x5d6167),
                function: c(0x2f63a8),
                type_: c(0x0e7568),
                variable: c(0x33373b),
                operator: c(0x55595e),
                meta: c(0x5d6167),
                added: c(0x2f7a45),
                removed: c(0xb8392e),
            }
        }
    }
}

/// How much smaller a reply is set when KodAct Sans draws it.
const BRAND_STEP: f32 = 1.;

/// The app's fixed dimensions.
#[derive(Clone, Copy)]
pub struct Sizes {
    pub titlebar: Pixels,      // 36
    pub sidebar: Pixels,       // 288
    pub column: Pixels,        // 760: the reading column, and the composer under it
    pub thinking_max: Pixels,  // 240: an opened thought's tallest; longer ones scroll
    pub row: Pixels,           // 30: sidebar rows, menu items
    pub menu_row: Pixels,      // 26: a row in a session menu, as Claude Code's
    pub menu_width: Pixels,    // 200: a session menu's width
    pub session_panel: Pixels, // 300: the Session column beside a chat
    pub quiet_line: Pixels,    // 26: a quiet line in a reply: a call, a thought, a run of work
    pub thumb_w: Pixels,       // 34: a picture a call returned, at the end of its line
    pub thumb_h: Pixels,       // 22: the same, short enough for the quiet line
    pub tool_image: Pixels,    // 360: that picture's widest, in the call's opened body
    pub control: Pixels,       // 26: pills, small buttons
    // Icons: one scale, 12 / 14 / 16 / 20, and Lucide's own 2px stroke.
    pub icon_xs: Pixels,            // 12: inside captions and chips
    pub icon_sm: Pixels,            // 14
    pub icon: Pixels,               // 16
    pub icon_lg: Pixels,            // 20: the composer's +
    pub composer_max: Pixels,       // the composer's tallest input
    pub composer_radius: Pixels,    // 4: the composer box's corners, a card's
    pub composer_line: Pixels,      // 46: the composer box with one line in it
    pub composer_width: Pixels,     // 560: the composer's widest, wherever it sits
    pub palette: Pixels,            // command palette width
    pub hairline: Pixels,           // 1
    pub line_height_chat: f32,      // 23px lines at 15px: whole pixels, crisp
    pub chat_text: Pixels,          // 15: replies and your messages
    pub greeting: Pixels,           // 32: the new-chat greeting, KodAct Sans 600
    pub statusbar: Pixels,          // 26: the status bar at the foot of the window
    pub figure: Pixels,             // 28: one figure that leads a view (agents finished)
    pub panel_in_ms: u64,           // 280: the side panel sliding in
    pub pop_in_ms: u64,             // 160: a menu or dialog rising into place
    pub stagger_ms: u64,            // 18: each row of an opening list starting after the one above
    pub edge_draw_ms: u64,          // 140: a window's jewel edge drawing down as it opens
    pub window_in_ms: u64,          // 240: a dialog rising into place over its dimmed backdrop
    pub window_out_ms: u64,         // 170: a dialog sinking away
    pub menu_out_ms: u64,           // 110: a menu or the palette fading back toward its anchor
    pub panel_out_ms: u64,          // 200: a side panel sliding back out
    pub toast_in_ms: u64,           // 260: a notice springing up from the foot
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
    pub project_rail: Pixels,       // 340: a project page's right rail (its brief, instructions…)
    pub editor: Pixels,             // 420: a file editor's height
    pub composer_clearance: Pixels, // 150: toasts sit above the composer, its chips included
    pub attach_thumb: Pixels,       // 28: an attachment chip's picture or icon
    pub attach_chip: Pixels,        // 220: an attachment chip's widest
    pub design_menu: Pixels,      // 340: the composer's design menu's width
    pub design_menu_list: Pixels, // 400: its list's tallest, room for every kind of work
    pub attach_tile: Pixels, // 108: a pasted text's or picture's square tile, as Claude's box shows them
    pub popover_list: Pixels, // 300: the tallest an autocomplete or context list grows
    pub completion_width: Pixels, // 460: the / and @ lists, opened at the caret
    pub settings_nav: Pixels, // 216: the Settings window's section list
    pub settings_width: Pixels, // 960: the Settings window, as wide as the screen allows
    pub settings_height: Pixels, // 760: and as tall: every section in the list at once
    pub settings_measure: Pixels, // 680: a Settings page's reading width
    pub field_h: Pixels,     // 34: a text field in a form
    pub prompt_box: Pixels,  // 180: a prompt or instructions box in a Settings editor
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
    pub side_shade: Pixels, // 12: the sidebar's shadow across the session's left edge
    pub edge_fade: Pixels, // 40: the transcript fading out where the composer meets it
    pub home_heading: Pixels, // 36: the home screen's heading, KodAct Sans 600
    pub mark_home: Pixels, // 48: the mark beside the home screen's greeting (3px a pixel of its grid)
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            titlebar: px(36.),
            sidebar: px(288.),
            column: px(760.),
            thinking_max: px(240.),
            row: px(30.),
            menu_row: px(26.),
            menu_width: px(200.),
            session_panel: px(300.),
            quiet_line: px(26.),
            thumb_w: px(34.),
            thumb_h: px(22.),
            tool_image: px(360.),
            control: px(26.),
            icon_xs: px(12.),
            icon_sm: px(14.),
            icon: px(16.),
            icon_lg: px(20.),
            composer_max: px(240.),
            composer_radius: px(4.),
            composer_width: px(560.),
            composer_line: px(46.),
            palette: px(560.),
            hairline: px(1.),
            line_height_chat: 23. / 15.,
            chat_text: px(15.),
            greeting: px(32.),
            statusbar: px(26.),
            figure: px(28.),
            panel_in_ms: 280,
            pop_in_ms: 160,
            stagger_ms: 18,
            edge_draw_ms: 140,
            window_in_ms: 240,
            window_out_ms: 170,
            menu_out_ms: 110,
            panel_out_ms: 200,
            toast_in_ms: 260,
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
            project_rail: px(340.),
            editor: px(420.),
            composer_clearance: px(150.),
            attach_thumb: px(28.),
            attach_chip: px(220.),
            design_menu: px(340.),
            design_menu_list: px(400.),
            attach_tile: px(108.),
            popover_list: px(300.),
            completion_width: px(460.),
            settings_nav: px(216.),
            settings_width: px(960.),
            settings_height: px(760.),
            settings_measure: px(680.),
            field_h: px(34.),
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
            side_shade: px(12.),
            edge_fade: px(40.),
            home_heading: px(36.),
            mark_home: px(48.),
        }
    }
}

pub fn init(cx: &mut App) {
    // Dark first, as KodAct is mostly seen in a dark terminal; the saved
    // mode (`window.json`) is applied as the window opens.
    cx.set_global(Theme::dark());
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
/// A selected row (a chat, a section, a project).
pub fn selected<E: Styled>(t: &Theme, e: E) -> E {
    // A neutral wash, as Claude Code marks the open row: the accent stays
    // for the action.
    e.font_weight(FontWeight::MEDIUM)
        .bg(t.colors.border.opacity(0.7))
}

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
        .px(px(16.))
        .rounded(t.radius.pill)
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
            |d| d.font_weight(FontWeight::SEMIBOLD),
        )
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label)
}

/// The one filled action per surface: the ink pill.
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

/// A section's small label ("Recent runs", "Knowledge"): sentence case,
/// muted, the medium cut. Never uppercase, and never above a heading.
pub fn section_label(t: &Theme, text: &str) -> Div {
    div()
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_muted)
        .child(text.to_string())
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

#[cfg(test)]
mod look_tests {
    use super::*;

    #[test]
    fn any_saved_theme_reads_as_the_one_look() {
        // Themes saved before there was one look come back as it.
        for id in ["calm", "terracotta", "premium", "ink", "neon"] {
            assert_eq!(Look::from_id(id), Some(Look::KodAct));
        }
        assert_eq!(Look::default(), Look::KodAct);
        // Calm laid over the base changes nothing: one source of its values.
        for dark in [false, true] {
            let (a, b) = (
                Theme::of(Look::KodAct, dark),
                if dark { Theme::dark() } else { Theme::light() },
            );
            for (x, y) in [
                (a.extra.stage, b.extra.stage),
                (a.extra.accent, b.extra.accent),
                (a.colors.primary, b.colors.primary),
                (a.extra.mark_top, b.extra.mark_top),
                (a.extra.page_ink, b.extra.page_ink),
                (a.colors.surface, b.colors.surface),
            ] {
                assert_eq!(x.to_rgb(), y.to_rgb());
            }
        }
    }
}
