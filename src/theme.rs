//! Design tokens for GPUI in the "calmly smooth" system (after the Anchor
//! style reference): sun-bleached cream surfaces stacked like paper on paper,
//! one ink, one dusty-blue accent rationed to the decisive action, Archivo set in
//! two weights, 4px cards against pill buttons, and shadows thin as grain.
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

/// A theme you can pick in Settings: a whole look, not only a palette. Its
/// surfaces and one colour, light and dark (`tint`), and its form: faces,
/// corners, density, depth and how a selected row shows (`form`). Calmly
/// smooth is the system's own form; the others each set their own.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Look {
    /// Sun-bleached cream and dusty blue: the system's own.
    #[default]
    Calm,
    /// The same cream with the reference's terracotta.
    Terracotta,
    /// Ivory paper and patina teal: Forge's first look.
    Patina,
    /// Plain greys and ink: no colour at all.
    Ink,
    /// Cool slate and a deep blue.
    Slate,
    /// Soft heather and plum.
    Plum,
    /// Layered: raised cards, the "+" grouped with a line on each row.
    Layers,
    /// The "+" opens a tray of chips inside the box, over nothing.
    Tray,
    /// Cool studio greys; the "+" splits into a list and its page.
    Studio,
    /// Round, roomy; the "+" opens a grid of tiles.
    Tiles,
    /// Keyboard first, violet; the "+" is a list you type to filter.
    Command,
    /// Calmly smooth's surfaces and form, untouched; one colour, used
    /// sparingly: Electric Blue for the action, focus and what runs.
    Premium,
}

impl Look {
    pub const ALL: [Look; 12] = [
        Look::Calm,
        Look::Terracotta,
        Look::Patina,
        Look::Ink,
        Look::Slate,
        Look::Plum,
        Look::Layers,
        Look::Tray,
        Look::Studio,
        Look::Tiles,
        Look::Command,
        Look::Premium,
    ];

    /// How it is saved (`window.json`).
    pub fn id(self) -> &'static str {
        match self {
            Look::Calm => "calm",
            Look::Terracotta => "terracotta",
            Look::Patina => "patina",
            Look::Ink => "ink",
            Look::Slate => "slate",
            Look::Plum => "plum",
            Look::Layers => "layers",
            Look::Tray => "tray",
            Look::Studio => "studio",
            Look::Tiles => "tiles",
            Look::Command => "command",
            Look::Premium => "premium",
        }
    }

    pub fn from_id(id: &str) -> Option<Look> {
        Look::ALL.into_iter().find(|l| l.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Look::Calm => "Calmly smooth",
            Look::Terracotta => "Terracotta",
            Look::Patina => "Patina",
            Look::Ink => "Ink",
            Look::Slate => "Slate",
            Look::Plum => "Plum",
            Look::Layers => "Layers",
            Look::Tray => "Tray",
            Look::Studio => "Studio",
            Look::Tiles => "Tiles",
            Look::Command => "Command",
            Look::Premium => "Premium",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Look::Calm => "Cream and dusty blue, Archivo",
            Look::Terracotta => "Warm paper, replies in a serif",
            Look::Patina => "Ivory and teal, Inter, flat",
            Look::Ink => "Greys and Geist, nothing else",
            Look::Slate => "Cool grey, Plex, compact",
            Look::Plum => "Heather, an editorial serif",
            Look::Layers => "Raised cards, a grouped + menu",
            Look::Tray => "Pills, the + as a tray in the box",
            Look::Studio => "Cool greys, a split + menu",
            Look::Tiles => "Round and roomy, + as tiles",
            Look::Command => "Violet, a + you type to filter",
            Look::Premium => "Graphite and one electric blue",
        }
    }

    /// Everything but colour: faces, corners, density, depth, selection.
    fn form(self) -> Form {
        let base = Form {
            sans: None,
            display: None,
            mono: "JetBrains Mono",
            reading: None,
            display_weight: FontWeight::SEMIBOLD,
            // (button, card, field, composer, pressed: buttons and chips)
            corners: (4., 4., 4., 4., 9999.),
            row: 30.,
            titlebar: 36.,
            chat: (16., 26.),
            heading: (32., 36.),
            depth: Depth::Grain,
            // A neutral wash, as Claude Code marks the open row: the accent
            // stays for the action.
            selection: Selection::Wash,
            motion_ms: 200,
            add_menu: AddMenu::List,
        };
        match self {
            // Forge Ember: Hanken Grotesk, a warm humanist grotesk that reads
            // like a letter at reply size (Anthropic Sans takes over where it
            // is installed, `fonts.rs`).
            Look::Calm | Look::Premium => Form {
                sans: Some("Hanken Grotesk"),
                display: Some("Hanken Grotesk"),
                // Denser, as Claude Code sets a reply: a third more on screen.
                chat: (15., 24.),
                ..base
            },
            // Claude's school: a warm grotesk, replies set like a letter in
            // Source Serif, soft corners, room to breathe, a soft lift.
            Look::Terracotta => Form {
                sans: Some("Hanken Grotesk"),
                display: Some("Source Serif 4 Display"),
                reading: Some("Source Serif 4"),
                display_weight: FontWeight::NORMAL,
                corners: (8., 12., 10., 16., 9999.),
                row: 32.,
                titlebar: 40.,
                chat: (16., 27.),
                heading: (34., 38.),
                depth: Depth::Lift,
                selection: Selection::Card,
                motion_ms: 220,
                ..base
            },
            // Calm and exact: Inter, small radii throughout, no shadows,
            // a hairline edge, the accent's tint on the selected row.
            Look::Patina => Form {
                sans: Some("Inter"),
                display: Some("Inter"),
                corners: (6., 8., 6., 12., 6.),
                chat: (15., 24.),
                heading: (28., 32.),
                depth: Depth::Flat,
                selection: Selection::Tint,
                motion_ms: 180,
                ..base
            },
            // Cursor's school: Geist and Geist Mono, neutral and quiet, the
            // filled action in ink, a grey wash for what is selected.
            Look::Ink => Form {
                sans: Some("Geist"),
                display: Some("Geist"),
                mono: "Geist Mono",
                corners: (6., 8., 8., 12., 8.),
                chat: (15., 24.),
                heading: (28., 32.),
                depth: Depth::Flat,
                selection: Selection::Wash,
                motion_ms: 140,
                ..base
            },
            // Zed's school: IBM Plex, square corners, dense rows, a low
            // title bar, the tint on the selected row, no shadows.
            Look::Slate => Form {
                sans: Some("IBM Plex Sans"),
                display: Some("IBM Plex Sans"),
                mono: "IBM Plex Mono",
                corners: (3., 4., 3., 6., 4.),
                row: 26.,
                titlebar: 32.,
                chat: (14., 23.),
                heading: (24., 28.),
                depth: Depth::Flat,
                selection: Selection::Tint,
                motion_ms: 120,
                ..base
            },
            // Editorial: Instrument Sans, the greeting in Instrument Serif,
            // round everything, a soft lift.
            Look::Plum => Form {
                sans: Some("Instrument Sans"),
                display: Some("Instrument Serif"),
                display_weight: FontWeight::NORMAL,
                corners: (12., 16., 14., 22., 9999.),
                row: 32.,
                titlebar: 38.,
                chat: (15., 24.),
                heading: (40., 44.),
                depth: Depth::Lift,
                selection: Selection::Soft,
                motion_ms: 220,
                ..base
            },
            // Direction A: layers. Hanken Grotesk over Archivo headings,
            // soft corners, raised cards, the "+" grouped, each row with
            // a line saying what it does.
            Look::Layers => Form {
                sans: Some("Hanken Grotesk"),
                corners: (8., 14., 10., 18., 9999.),
                row: 32.,
                titlebar: 40.,
                heading: (34., 38.),
                depth: Depth::Lift,
                selection: Selection::Card,
                add_menu: AddMenu::Grouped,
                ..base
            },
            // Direction B: a tray. Pills everywhere; the "+" unfolds a row
            // of chips inside the box instead of a menu over the chat.
            Look::Tray => Form {
                sans: Some("Hanken Grotesk"),
                corners: (9999., 12., 10., 20., 9999.),
                row: 32.,
                titlebar: 38.,
                depth: Depth::Lift,
                selection: Selection::Card,
                add_menu: AddMenu::Tray,
                ..base
            },
            // Direction C: a studio. Cool greys, tidy 8px corners, the "+"
            // split into a list and the chosen one's page.
            Look::Studio => Form {
                sans: Some("Hanken Grotesk"),
                corners: (8., 10., 8., 18., 8.),
                chat: (15., 24.),
                depth: Depth::Lift,
                selection: Selection::Soft,
                motion_ms: 180,
                add_menu: AddMenu::Split,
                ..base
            },
            // Direction D: tiles. Round and roomy, squircle buttons, the
            // "+" a grid of large tiles.
            Look::Tiles => Form {
                sans: Some("Hanken Grotesk"),
                corners: (12., 18., 12., 22., 12.),
                row: 34.,
                titlebar: 40.,
                chat: (15., 24.),
                heading: (36., 40.),
                depth: Depth::Lift,
                selection: Selection::Wash,
                motion_ms: 220,
                add_menu: AddMenu::Tiles,
                ..base
            },
            // Direction E: command first. Compact, a grey wash for what is
            // selected, the "+" a list you type into.
            Look::Command => Form {
                sans: Some("Hanken Grotesk"),
                corners: (8., 10., 9., 16., 10.),
                chat: (15., 24.),
                depth: Depth::Grain,
                selection: Selection::Wash,
                motion_ms: 160,
                add_menu: AddMenu::Search,
                ..base
            },
        }
    }

    /// Its surfaces and colour in one mode.
    fn tint(self, dark: bool) -> Tint {
        let t = |canvas,
                 stage,
                 card,
                 sunk,
                 accent,
                 accent_fg,
                 accent_soft,
                 link,
                 link_hover,
                 mark_top| Tint {
            canvas,
            stage,
            card,
            sunk,
            accent,
            accent_fg,
            accent_soft,
            link,
            link_hover,
            mark_top,
        };
        match (self, dark) {
            // Forge Ember: warm paper and warm graphite, one copper heat for
            // the action, the current row and state (`.impeccable`, 2026-10-23).
            (Look::Calm, false) => t(
                0xebe8e1, 0xf7f5f1, 0xffffff, 0xf1eee8, 0xb4532a, 0xffffff, 0xf5e2d7, 0xa44a24,
                0x86391a, 0x93401c,
            ),
            (Look::Calm, true) => t(
                0x1b1a19, 0x242322, 0x2e2d2b, 0x1f1e1d, 0xe2895b, 0x1b1a19, 0x3e2b22, 0xeb9d74,
                0xf3b694, 0xe2895b,
            ),
            (Look::Terracotta, false) => t(
                0xe3d7cd, 0xf0e9e5, 0xffffff, 0xf5f2f0, 0xb85a2a, 0xffffff, 0xf6dccb, 0xa8492a,
                0x8c3a20, 0x8f3f1e,
            ),
            (Look::Terracotta, true) => t(
                0x171513, 0x1e1c1a, 0x2b2825, 0x262321, 0xee884f, 0x171513, 0x3d2a20, 0xf0a77d,
                0xf5bd9a, 0xee884f,
            ),
            (Look::Patina, false) => t(
                0xe9e6df, 0xf9f8f5, 0xffffff, 0xf0eee8, 0x216d70, 0xffffff, 0xd0e6e7, 0x216d70,
                0x10585b, 0x216d70,
            ),
            (Look::Patina, true) => t(
                0x141312, 0x232321, 0x2c2c2a, 0x1f1e1d, 0x529a9d, 0x141312, 0x2f4749, 0x8ecacd,
                0xabdbde, 0x6fb3b6,
            ),
            (Look::Ink, false) => t(
                0xe2e2df, 0xf1f1ef, 0xffffff, 0xf6f6f4, 0x1e1e1e, 0xffffff, 0xdededb, 0x1e1e1e,
                0x3a3a3a, 0x52524e,
            ),
            (Look::Ink, true) => t(
                0x121212, 0x1b1b1a, 0x272726, 0x222221, 0xf0f0ee, 0x121212, 0x333331, 0xf0f0ee,
                0xffffff, 0x9e9e99,
            ),
            (Look::Slate, false) => t(
                0xd8dde3, 0xedf0f3, 0xffffff, 0xf4f6f8, 0x3b5b8c, 0xffffff, 0xd9e2ef, 0x3b5b8c,
                0x2e4870, 0x3b5b8c,
            ),
            (Look::Slate, true) => t(
                0x111418, 0x181c21, 0x242a32, 0x1e232a, 0x9db7e0, 0x111418, 0x26303e, 0x9db7e0,
                0xb8cbea, 0x9db7e0,
            ),
            (Look::Plum, false) => t(
                0xe5d9e0, 0xf2ebef, 0xffffff, 0xf7f2f5, 0x7a3e65, 0xffffff, 0xeddbe6, 0x7a3e65,
                0x62304f, 0x7a3e65,
            ),
            (Look::Plum, true) => t(
                0x171217, 0x1f1a1f, 0x2c252c, 0x262026, 0xd3a3c6, 0x171217, 0x3a2a36, 0xd3a3c6,
                0xe2bfd8, 0xd3a3c6,
            ),
            // The five directions: Forge's cream surfaces (Studio's cool
            // greys) under an accent of their own.
            (Look::Layers, false) => t(
                0xe3d7cd, 0xf0e9e5, 0xffffff, 0xf5f2f0, 0x3d4f8f, 0xffffff, 0xdfe3f2, 0x3d4f8f,
                0x2f3e72, 0x3d4f8f,
            ),
            (Look::Layers, true) => t(
                0x171513, 0x1e1c1a, 0x2b2825, 0x262321, 0xa9b5e8, 0x171513, 0x272c40, 0xa9b5e8,
                0xc4cdf0, 0xa9b5e8,
            ),
            (Look::Tray, false) => t(
                0xe3d7cd, 0xf0e9e5, 0xffffff, 0xf5f2f0, 0x2e6077, 0xffffff, 0xd8e7ee, 0x2e6077,
                0x234b5e, 0x2e6077,
            ),
            (Look::Tray, true) => t(
                0x171513, 0x1e1c1a, 0x2b2825, 0x262321, 0x93c2d6, 0x171513, 0x233540, 0x93c2d6,
                0xb3d6e4, 0x93c2d6,
            ),
            (Look::Studio, false) => t(
                0xdcdde3, 0xeeeff3, 0xffffff, 0xf4f5f8, 0x4a4fa0, 0xffffff, 0xe0e1f3, 0x4a4fa0,
                0x3a3e84, 0x4a4fa0,
            ),
            (Look::Studio, true) => t(
                0x131417, 0x1a1b1f, 0x25262c, 0x202126, 0xb3b6f0, 0x131417, 0x2a2b44, 0xb3b6f0,
                0xcdcff6, 0xb3b6f0,
            ),
            (Look::Tiles, false) => t(
                0xe3d7cd, 0xf0e9e5, 0xffffff, 0xf5f2f0, 0x3f6b4f, 0xffffff, 0xdbe8df, 0x3f6b4f,
                0x30543d, 0x3f6b4f,
            ),
            (Look::Tiles, true) => t(
                0x171513, 0x1e1c1a, 0x2b2825, 0x262321, 0x9fcfae, 0x171513, 0x24352b, 0x9fcfae,
                0xbde0c7, 0x86c19a,
            ),
            (Look::Command, false) => t(
                0xe3d7cd, 0xf0e9e5, 0xffffff, 0xf5f2f0, 0x6a4fa3, 0xffffff, 0xe7dff3, 0x6a4fa3,
                0x553d86, 0x6a4fa3,
            ),
            // Forge Premium: Calm's paper and graphite to the hex, the
            // copper swapped for one blue (deeper by day, for its label).
            (Look::Premium, false) => t(
                0xebe8e1, 0xf7f5f1, 0xffffff, 0xf1eee8, 0x2563eb, 0xffffff, 0xdbe6fb, 0x2563eb,
                0x1d4ed8, 0x1d4ed8,
            ),
            (Look::Premium, true) => t(
                0x1b1a19, 0x242322, 0x2e2d2b, 0x1f1e1d, 0x3b82f6, 0x0b0f1a, 0x1e2a44, 0x7aa7f8,
                0xa5c4fa, 0x7aa7f8,
            ),
            (Look::Command, true) => t(
                0x171513, 0x1e1c1a, 0x2b2825, 0x262321, 0xc2adf0, 0x171513, 0x30283f, 0xc2adf0,
                0xd6c8f5, 0xc2adf0,
            ),
        }
    }
}

/// A theme's form. `None` faces keep the system's (`fonts.rs`).
#[derive(Clone, Copy)]
struct Form {
    sans: Option<&'static str>,
    display: Option<&'static str>,
    mono: &'static str,
    /// A reply's prose, when it is not the interface face.
    reading: Option<&'static str>,
    display_weight: FontWeight,
    corners: (f32, f32, f32, f32, f32),
    row: f32,
    titlebar: f32,
    /// Chat text and its line, in pixels.
    chat: (f32, f32),
    /// The greeting and the home screen's heading.
    heading: (f32, f32),
    depth: Depth,
    selection: Selection,
    motion_ms: u64,
    add_menu: AddMenu,
}

/// How surfaces lift off the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// Three hairline shadows, thin as paper grain (the system's).
    Grain,
    /// A soft shadow, as a card resting on a desk.
    Lift,
    /// No shadow: a 1px edge in the border colour.
    Flat,
}

/// How the composer's "+" opens (`ui/add_menu.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddMenu {
    /// A plain menu: icon, label, shortcut or chevron.
    List,
    /// Two groups (add to this message, tools), each row with a line.
    Grouped,
    /// A row of chips inside the box, over nothing.
    Tray,
    /// A list beside the chosen row's page.
    Split,
    /// A grid of large tiles.
    Tiles,
    /// A list you type into to filter; Enter picks the first.
    Search,
}

/// How a selected row (a chat, a section) stands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// The accent's pale form (the system's).
    Soft,
    /// A sticky note: the card colour with a hairline.
    Card,
    /// The accent's tint, the label in the accent.
    Tint,
    /// A quiet grey wash.
    Wash,
}

/// One mode of a theme, as hex: the canvas (title bar and sidebar), the
/// session, cards, a recessed well; the accent fill, its label, its pale
/// form; links and their hover; the mark's top.
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
    pub font_sans: SharedString, // Archivo, bundled in 400 and 600 (`fonts.rs`)
    pub font_serif: SharedString, // Source Serif (substitute for Lyon Text)
    pub motion_ms: u64,          // 200ms ease for hovers/transitions
    // Forge additions (see the section at the end of the tokens).
    pub font_mono: SharedString, // JetBrains Mono: code, paths, commands
    pub font_code_inline: SharedString, // IBM Plex Mono: a name in prose; its small x-height sits level with the text
    pub font_display: SharedString, // large lines (the greeting, headings): Archivo, as everything is
    pub display_weight: FontWeight, // 600: headings are set in the heavier of the two cuts
    pub font_reading: SharedString, // a reply's prose: the interface face, or a serif (Terracotta)
    pub depth: Depth,               // how surfaces lift: grain, a soft lift, or flat
    pub selection: Selection,       // how a selected row shows
    pub add_menu: AddMenu,          // how the composer's "+" opens
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

impl Theme {
    fn base(is_dark: bool, colors: Colors) -> Self {
        Self {
            is_dark,
            look: Look::Calm,
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
            font_sans: crate::fonts::sans(),
            font_serif: "Source Serif 4".into(),
            motion_ms: 200,
            font_mono: "JetBrains Mono".into(),
            font_code_inline: "IBM Plex Mono".into(),
            font_display: crate::fonts::display(),
            display_weight: FontWeight::SEMIBOLD,
            font_reading: crate::fonts::sans(),
            depth: Depth::Grain,
            selection: Selection::Soft,
            add_menu: AddMenu::List,
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
        // Live activity and agents take the accent and the theme's spread,
        // unless the theme names their own.
        (self.extra.live, self.extra.agent) = match (look, self.is_dark) {
            // One blue, used once: what runs takes the accent's blue; agents
            // carry no colour of their own.
            (Look::Premium, _) => (c(k.link), Some(self.colors.text_muted)),
            _ => (c(k.accent), None),
        };
        self.extra.mark_top = c(k.mark_top);
        self.colors.primary = c(k.link);
        self.colors.primary_hover = c(k.link_hover);
        self.colors.primary_tint = c(k.link).opacity(if self.is_dark { 0.16 } else { 0.12 });

        let f = look.form();
        let face = |name: Option<&str>, or: SharedString| -> SharedString {
            name.map_or(or, |n| n.to_string().into())
        };
        self.font_sans = face(f.sans, crate::fonts::sans());
        self.font_display = face(f.display, crate::fonts::display());
        self.font_mono = f.mono.into();
        self.font_reading = face(f.reading, self.font_sans.clone());
        // Anthropic Sans, when installed, sets every face in every theme,
        // code included: nothing else draws.
        if let Some((text, display)) = crate::fonts::brand() {
            self.font_sans = text.clone();
            self.font_display = display;
            self.font_serif = text.clone();
            self.font_reading = text.clone();
            let mono = crate::fonts::brand_mono().unwrap_or(text);
            self.font_mono = mono.clone();
            self.font_code_inline = mono;
        }
        self.display_weight = f.display_weight;
        let (button, card, field, composer, pressed) = f.corners;
        let r = &mut self.radius;
        (r.button, r.card, r.field) = (px(button), px(card), px(field));
        (r.segment, r.compact, r.control) = (px(pressed), px(pressed), px(pressed));
        let z = &mut self.size;
        z.composer_radius = px(composer);
        (z.row, z.titlebar) = (px(f.row), px(f.titlebar));
        // Anthropic Sans runs large (a tall x-height): a step smaller, so a
        // reply reads at the size the theme meant.
        let (size, line) = match crate::fonts::brand() {
            Some(_) => ((f.chat.0 - BRAND_STEP).max(13.), f.chat.1 - BRAND_STEP),
            None => f.chat,
        };
        z.chat_text = px(size);
        z.line_height_chat = line / size;
        (z.greeting, z.home_heading) = (px(f.heading.0), px(f.heading.1));
        self.depth = f.depth;
        self.selection = f.selection;
        self.add_menu = f.add_menu;
        self.motion_ms = f.motion_ms;
    }

    pub fn light() -> Self {
        Self::base(
            false,
            Colors {
                // Forge Ember by day: warm paper, the session a sheet on it.
                canvas: rgb(0xf7f5f1).into(),
                surface: rgb(0xffffff).into(),
                sidebar: rgb(0xebe8e1).into(),
                // One ink, #1e1e1e, carries the type; Stone for what is secondary.
                text: rgb(0x1e1e1e).into(),
                text_strong: rgb(0x1e1e1e).into(),
                text_body: rgb(0x2b2825).into(),
                text_muted: rgb(0x5a554f).into(),
                text_subtle: rgb(0x7d776f).into(),
                border: hsla(0., 0., 0.118, 0.13),
                hover: hsla(0., 0., 0.118, 0.05),
                // Links and selected text: ember copper, the one colour.
                primary: rgb(0xa44a24).into(),
                primary_hover: rgb(0x86391a).into(),
                primary_fg: rgb(0xffffff).into(),
                primary_tint: hsla(18. / 360., 0.64, 0.39, 0.12),
                marigold: rgb(0xffb110).into(),
                coral: rgb(0xc84b55).into(),
                sky: rgb(0x62aef0).into(),
                midnight: rgb(0x02093a).into(),
                peach: rgb(0xd0e6e7).into(),
            },
        )
    }

    /// The same room at night: warm graphite, the ember lifted to read on it.
    pub fn dark() -> Self {
        Self::base(
            true,
            Colors {
                canvas: rgb(0x242322).into(),
                surface: rgb(0x2e2d2b).into(),
                sidebar: rgb(0x1b1a19).into(),
                text: rgb(0xf0e9e5).into(),
                text_strong: rgb(0xf5f2f0).into(),
                text_body: rgb(0xd9d1cb).into(),
                text_muted: rgb(0xa8a29b).into(),
                text_subtle: rgb(0x857f77).into(),
                border: hsla(22. / 360., 0.37, 0.92, 0.12),
                hover: hsla(22. / 360., 0.37, 0.92, 0.06),
                primary: rgb(0xeb9d74).into(),
                primary_hover: rgb(0xf3b694).into(),
                primary_fg: rgb(0x1b1a19).into(),
                primary_tint: hsla(20. / 360., 0.74, 0.69, 0.16),
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

    /// What lifts a card or the session off the canvas, in this theme's
    /// depth: paper grain (three hairline shadows under 4px of blur and 6%
    /// ink), a soft lift, or flat (a 1px edge drawn as a ring).
    pub fn elevation_grain(&self) -> Vec<BoxShadow> {
        let d = self.is_dark;
        match self.depth {
            Depth::Grain => {}
            Depth::Flat => {
                return vec![BoxShadow {
                    color: self.colors.border,
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: false,
                }]
            }
            Depth::Lift => {
                return [(1., 2., 0.04, 0.3), (8., 24., 0.07, 0.35)]
                    .into_iter()
                    .map(|(y, blur, light, dark)| BoxShadow {
                        color: ink(if d { dark } else { light }),
                        offset: point(px(0.), px(y)),
                        blur_radius: px(blur),
                        spread_radius: px(if y > 2. { -6. } else { 0. }),
                        inset: false,
                    })
                    .collect()
            }
        }
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

// ---------- Forge additions ----------

impl Theme {
    /// The sidebar's shadow where it meets the session: its colour at the
    /// edge, fading out over `Sizes::side_shade`, so the sidebar reads as a
    /// layer over the window. None in a flat theme, whose session has its
    /// 1px edge instead.
    pub fn side_shade(&self) -> Option<Hsla> {
        (self.depth != Depth::Flat).then(|| ink(if self.is_dark { 0.30 } else { 0.06 }))
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
    // Forge's mark, the ingot (`ui/ingot.rs`): its top in the accent, its
    // left side ink (paper in dark), its right a grey a clear step from
    // both, so the three faces part by lightness and not by hue alone.
    pub mark_top: Hsla,
    pub mark_left: Hsla,
    pub mark_right: Hsla,
    // The ingot fresh from the forge (onboarding, a strike), as hot metal
    // cools: white hot, then the orange glow, then a dull ember red, then
    // dark iron, before each face settles to its own colour. Never a resting colour; the same
    // in both modes, as heat looks the same in any light.
    pub mark_heat_white: Hsla,
    pub mark_heat: Hsla,
    pub mark_heat_ember: Hsla,
    pub mark_heat_iron: Hsla,
    pub border_strong: Hsla, // outlined buttons, switch tracks at rest, fields
    pub sunk: Hsla,          // a recessed track: segmented controls, the composer's tray
    pub toggle_on: Hsla,     // a switch that is on
    // What is running right now (a tool's spinner, the working line's
    // mark, an agent at work): the accent, except where a theme gives
    // live activity its own colour (Premium: the link's blue).
    pub live: Hsla,
    // Sub-agents' own mark (their kind in the Agents panel): the theme's
    // spread of colours, except where a theme gives agents one (Premium:
    // quiet grey, no colour of their own).
    pub agent: Option<Hsla>,
    // Memory and rules: the reference's ochre.
    pub memory: Hsla,
    pub memory_tint: Hsla,
    // Inline code in prose (a file, a command): Forge's coral on a faint
    // wash, a rounded chip (`ui/markdown.rs`).
    pub code_inline_fg: Hsla,
    pub code_inline_bg: Hsla,
    // The home screen's field of dots (`ui/dot_field.rs`).
    pub dot: Hsla,
    // Permission modes, as Forge for VS Code tints them (forge-tokens.css).
    pub mode_edit: Hsla,   // Edit automatically
    pub mode_plan: Hsla,   // Plan
    pub mode_bypass: Hsla, // Bypass permissions
    pub mode_expert: Hsla, // Expert
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
                success: rgb(0x8fbf9b).into(),
                success_tint: rgb(0x26302a).into(),
                danger: rgb(0xe98a72).into(),
                danger_tint: hsla(7. / 360., 0.7, 0.55, 0.1),
                warning: rgb(0xffc85a).into(),
                warning_tint: hsla(40. / 360., 1., 0.53, 0.18),
                diff_add: hsla(135. / 360., 0.27, 0.65, 0.12),
                diff_del: hsla(12. / 360., 0.73, 0.68, 0.12),
                scrim: ink(0.55),
                selected: rgb(0x242322).into(),
                // The cream stack at night: the canvas under the title bar
                // and the sidebar, the session a step up, cards above it.
                chrome: rgb(0x1b1a19).into(),
                pane: rgb(0x1b1a19).into(),
                stage: rgb(0x242322).into(),
                // The filled action is cream on umber, as ink on cream in light.
                fill: rgb(0xf0e9e5).into(),
                fill_hover: rgb(0xf5f2f0).into(),
                // Brick, lifted for the night, under the dark label.
                fill_danger: rgb(0xe98a72).into(),
                fill_fg: rgb(0x171513).into(),
                accent: rgb(0xe2895b).into(),
                accent_soft: rgb(0x3e2b22).into(),
                accent_fg: rgb(0x1b1a19).into(),
                mark_top: rgb(0xe2895b).into(),
                mark_left: rgb(0xf0e9e5).into(),
                mark_right: rgb(0x7d776f).into(),
                mark_heat_white: rgb(0xfff2cc).into(),
                mark_heat: rgb(0xf5892c).into(),
                mark_heat_ember: rgb(0xa3311c).into(),
                mark_heat_iron: rgb(0x3b3633).into(),
                border_strong: rgb(0x4a4540).into(),
                sunk: rgb(0x1f1e1d).into(),
                toggle_on: rgb(0xeb9d74).into(),
                live: rgb(0xeb9d74).into(),
                agent: None,
                memory: rgb(0xe5c77a).into(),
                memory_tint: rgb(0x3a3220).into(),
                code_inline_fg: rgb(0xef968b).into(),
                code_inline_bg: Hsla::from(rgb(0xf0e6dc)).opacity(0.09),
                dot: rgb(0x8b857e).into(),
                mode_edit: rgb(0x2da160).into(),
                mode_plan: rgb(0x428fdc).into(),
                mode_bypass: rgb(0xec5941).into(),
                mode_expert: rgb(0xd99530).into(),
                skill: rgb(0xc3a3ec).into(),
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
                success: rgb(0x3f6b4b).into(),
                success_tint: rgb(0xe3ebe3).into(),
                danger: rgb(0xa3402c).into(),
                danger_tint: hsla(9. / 360., 0.6, 0.55, 0.1),
                warning: rgb(0x8a5600).into(),
                warning_tint: hsla(40. / 360., 1., 0.53, 0.2),
                diff_add: hsla(137. / 360., 0.26, 0.33, 0.10),
                diff_del: hsla(10. / 360., 0.57, 0.41, 0.09),
                scrim: hsla(25. / 360., 0.2, 0.08, 0.38), // the dialog stands clear of the chat
                selected: rgb(0xf7f5f1).into(),
                // Cream Canvas under the title bar and the sidebar, Warm
                // White for the session, white for the cards on it.
                chrome: rgb(0xebe8e1).into(),
                pane: rgb(0xebe8e1).into(),
                stage: rgb(0xf7f5f1).into(),
                // The filled action is the ink pill: quiet, never the accent.
                fill: rgb(0x1e1e1e).into(),
                fill_hover: rgb(0x34322f).into(),
                fill_danger: rgb(0xa3402c).into(),
                fill_fg: rgb(0xf0e9e5).into(),
                // Dusty blue, where the reference has terracotta: the one colour.
                accent: rgb(0xb4532a).into(),
                accent_soft: rgb(0xf5e2d7).into(),
                accent_fg: rgb(0xffffff).into(),
                mark_top: rgb(0x93401c).into(),
                mark_left: rgb(0x1e1e1e).into(),
                mark_right: rgb(0x8b857e).into(),
                mark_heat_white: rgb(0xfff2cc).into(),
                mark_heat: rgb(0xf5892c).into(),
                mark_heat_ember: rgb(0xa3311c).into(),
                mark_heat_iron: rgb(0x3b3633).into(),
                border_strong: rgb(0xc9c9c7).into(),
                sunk: rgb(0xf1eee8).into(),
                toggle_on: rgb(0xa44a24).into(),
                live: rgb(0xa44a24).into(),
                agent: None,
                memory: rgb(0x75591a).into(),
                memory_tint: rgb(0xf6ecd2).into(),
                code_inline_fg: rgb(0xa13430).into(),
                code_inline_bg: Hsla::from(rgb(0x2b2622)).opacity(0.065),
                dot: rgb(0x8b857e).into(),
                mode_edit: rgb(0x108548).into(),
                mode_plan: rgb(0x1f75cb).into(),
                mode_bypass: rgb(0xa32c12).into(),
                mode_expert: rgb(0xc17d10).into(),
                skill: rgb(0x7a3fb0).into(),
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

/// How much smaller a reply is set when Anthropic Sans draws it.
const BRAND_STEP: f32 = 1.;

/// The app's fixed dimensions.
#[derive(Clone, Copy)]
pub struct Sizes {
    pub titlebar: Pixels,     // 36
    pub sidebar: Pixels,      // 288
    pub column: Pixels,       // 720: the reading column, and the composer under it
    pub thinking_max: Pixels, // 240: an opened thought's tallest; longer ones scroll
    pub row: Pixels,          // 30: sidebar rows, menu items
    pub menu_row: Pixels,     // 26: a row in a session menu, as Claude Code's
    pub menu_width: Pixels,   // 200: a session menu's width
    pub session_panel: Pixels, // 300: the Session column beside a chat
    pub quiet_line: Pixels,   // 26: a quiet line in a reply: a call, a thought, a run of work
    pub thumb_w: Pixels,      // 34: a picture a call returned, at the end of its line
    pub thumb_h: Pixels,      // 22: the same, short enough for the quiet line
    pub tool_image: Pixels,   // 360: that picture's widest, in the call's opened body
    pub control: Pixels,      // 26: pills, small buttons
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
    pub chat_text: Pixels,          // 15: replies and your messages, Archivo's body size
    pub greeting: Pixels,           // 32: the new-chat greeting, Archivo 600
    pub statusbar: Pixels,          // 26: the status bar at the foot of the window
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
    pub popover_list: Pixels,       // 300: the tallest an autocomplete or context list grows
    pub completion_width: Pixels,   // 460: the / and @ lists, opened at the caret
    pub settings_nav: Pixels,       // 216: the Settings window's section list
    pub settings_width: Pixels,     // 960: the Settings window, as wide as the screen allows
    pub settings_height: Pixels,    // 760: and as tall: every section in the list at once
    pub settings_measure: Pixels,   // 680: a Settings page's reading width
    pub field_h: Pixels,            // 34: a text field in a form
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
    pub side_shade: Pixels, // 12: the sidebar's shadow across the session's left edge
    pub edge_fade: Pixels, // 40: the transcript fading out where the composer meets it
    pub home_heading: Pixels, // 36: the home screen's heading, Archivo 600
    pub mark_home: Pixels, // 44: the mark beside the home screen's greeting
    pub dot_gap: Pixels,   // 14: the background field's grid
    pub dot: Pixels,       // 2: one dot's diameter
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            titlebar: px(36.),
            sidebar: px(288.),
            column: px(720.),
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
            mark_home: px(44.),
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
/// A selected row (a chat, a section, a project), in the theme's style.
pub fn selected<E: Styled>(t: &Theme, e: E) -> E {
    let e = e.font_weight(FontWeight::MEDIUM);
    match t.selection {
        Selection::Soft => e.bg(t.extra.accent_soft.opacity(0.6)),
        Selection::Card => e
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border),
        Selection::Tint => e.bg(t.colors.primary_tint).text_color(t.colors.primary),
        Selection::Wash => e.bg(t.colors.border.opacity(0.7)),
    }
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
    fn every_look_round_trips_by_its_id_and_calm_is_the_default() {
        for l in Look::ALL {
            assert_eq!(Look::from_id(l.id()), Some(l));
        }
        assert_eq!(Look::from_id("neon"), None);
        assert_eq!(Look::default(), Look::Calm);
        // Calm laid over the base changes nothing: one source of its values.
        for dark in [false, true] {
            let (a, b) = (
                Theme::of(Look::Calm, dark),
                if dark { Theme::dark() } else { Theme::light() },
            );
            for (x, y) in [
                (a.extra.stage, b.extra.stage),
                (a.extra.accent, b.extra.accent),
                (a.colors.primary, b.colors.primary),
                (a.extra.mark_top, b.extra.mark_top),
                (a.colors.surface, b.colors.surface),
            ] {
                assert_eq!(x.to_rgb(), y.to_rgb());
            }
        }
    }

    #[test]
    fn looks_differ_from_each_other() {
        for dark in [false, true] {
            let accents: Vec<_> = Look::ALL
                .iter()
                .map(|l| format!("{:?}", Theme::of(*l, dark).extra.accent.to_rgb()))
                .collect();
            let mut unique = accents.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), accents.len(), "{accents:?}");
        }
    }
}
