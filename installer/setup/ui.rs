//! The setup's window: dark, frameless, KodAct's mark on a burgundy glow at the left and the
//! steps at the right. Three screens: choose (where, and what), working (each part as it
//! unpacks, one progress bar), and done, or what went wrong with a way to try again.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    div, img, linear_color_stop, linear_gradient, percentage, point, prelude::*, px, relative, rgb,
    rgba, size, svg, Animation, AnimationExt, App, AssetSource, Bounds, BoxShadow, Context,
    FontWeight, Hsla, Image, ImageFormat, MouseButton, Render, Result, SharedString,
    Transformation, Window, WindowBounds, WindowControlArea, WindowDecorations, WindowOptions,
};

use crate::work::{self, Choices, Report};
use crate::Mode;

// ---------------------------------------------------------------- look

/// The setup's colours: KodAct's burgundy and rose on near-black.
struct Ink;

impl Ink {
    fn bg() -> Hsla {
        rgb(0x0b0b0e).into()
    }
    fn panel() -> Hsla {
        rgb(0x131218).into()
    }
    fn field() -> Hsla {
        rgb(0x0f0e13).into()
    }
    fn line() -> Hsla {
        rgba(0xffffff14).into()
    }
    fn line_strong() -> Hsla {
        rgba(0xffffff24).into()
    }
    fn hover() -> Hsla {
        rgba(0xffffff0d).into()
    }
    fn text() -> Hsla {
        rgb(0xf4f0f3).into()
    }
    fn muted() -> Hsla {
        rgb(0xa79fab).into()
    }
    fn subtle() -> Hsla {
        rgb(0x6f6874).into()
    }
    fn burgundy() -> Hsla {
        rgb(0x9c2a4c).into()
    }
    fn burgundy_deep() -> Hsla {
        rgb(0x6e1531).into()
    }
    fn rose() -> Hsla {
        rgb(0xe0889f).into()
    }
    fn rose_tint() -> Hsla {
        rgba(0xe0889f1f).into()
    }
    fn warn() -> Hsla {
        rgb(0xe8b65a).into()
    }
    fn danger() -> Hsla {
        rgb(0xf07a7a).into()
    }
}

fn font() -> &'static str {
    if cfg!(windows) {
        "Segoe UI"
    } else {
        "Inter"
    }
}

/// KodAct's ./< mark, `assets/brand/kodact-mark-burgundy.svg` as it is, at twice its size
/// (`rsvg-convert -w 560 -h 544 assets/brand/kodact-mark-burgundy.svg -o installer/setup/mark.png`).
const LOGO: &[u8] = include_bytes!("mark.png");
/// The mark's three pieces, each alone on the same canvas, for the intro: they come together,
/// then give way to the whole mark (whose shadow falls once, as the SVG draws it).
const LOGO_DOT: &[u8] = include_bytes!("mark-dot.png");
const LOGO_SLASH: &[u8] = include_bytes!("mark-slash.png");
const LOGO_ANGLE: &[u8] = include_bytes!("mark-angle.png");
/// The author's signature, handwritten: "Mohamed Amine Said" in Pinyon Script (SIL OFL 1.1,
/// `signature-font-OFL.txt`), drawn once into a picture at three times its size: a pale
/// champagne fading to soft ivory, at 80% ink, so it reads as a light hand, not a headline.
/// The picture, not the font, ships.
const SIGNATURE: &[u8] = include_bytes!("signature.png");
/// Its size in the window, in logical pixels (the picture is 1314 x 144).
const SIGNATURE_SIZE: (f32, f32) = (237.2, 26.0);

/// The intro, from the window's first frame: the mark comes together, the name rises, the
/// right side glides in. Quick: about a second.
const INTRO: Duration = Duration::from_millis(1150);
/// The signature is written over a little longer than the intro, finishing just after it.
const SIGNED: Duration = Duration::from_millis(2100);

/// Where `t` (0 to 1 over the intro) is between `a` and `b`, from 0 to 1.
fn span(t: f32, a: f32, b: f32) -> f32 {
    ((t - a) / (b - a)).clamp(0., 1.)
}

/// Ease out, overshooting a touch, then settling: a piece that lands.
fn land(x: f32) -> f32 {
    let (c1, c3) = (1.70158, 2.70158);
    1. + c3 * (x - 1.).powi(3) + c1 * (x - 1.).powi(2)
}

/// Ease out, no overshoot.
fn glide(x: f32) -> f32 {
    1. - (1. - x).powi(3)
}

/// The few icons the setup draws (Lucide, ISC licence, from the app's own set).
struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "check" => include_bytes!("../../assets/icons/check.svg"),
            "x" => include_bytes!("../../assets/icons/x.svg"),
            "minus" => include_bytes!("../../assets/icons/minus.svg"),
            "folder" => include_bytes!("../../assets/icons/folder-open.svg"),
            "alert" => include_bytes!("../../assets/icons/triangle-alert.svg"),
            "arrow" => include_bytes!("../../assets/icons/arrow-right.svg"),
            "spinner" => include_bytes!("../../assets/icons/loader-circle.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

fn icon(name: &'static str, size: f32, color: Hsla) -> gpui::Svg {
    svg()
        .path(name)
        .size(px(size))
        .flex_none()
        .text_color(color)
}

// ---------------------------------------------------------------- state

#[derive(Clone, Debug, PartialEq)]
enum Screen {
    Choose,
    Working,
    Done(Vec<String>),
    Failed(String),
}

pub struct Setup {
    mode: Mode,
    exe: PathBuf,
    screen: Screen,
    dir: PathBuf,
    libreoffice: bool,
    has_office: bool,
    path: bool,
    nudges: bool,
    desktop: bool,
    taskbar: bool,
    version: String,
    /// Bytes each part unpacks to.
    sizes: std::collections::BTreeMap<String, u64>,
    steps: Vec<&'static str>,
    step: usize,
    done: u64,
    total: u64,
    file: String,
    launch_after: bool,
    logo: Arc<Image>,
    pieces: [Arc<Image>; 3],
    signature: Arc<Image>,
}

impl Setup {
    fn new(mode: Mode, exe: PathBuf) -> Setup {
        let manifest = match &mode {
            Mode::Install => crate::payload::Payload::open(&exe)
                .map(|p| p.manifest)
                .unwrap_or_default(),
            _ => crate::payload::Manifest {
                version: env!("CARGO_PKG_VERSION").into(),
                sizes: [
                    ("app", 96_000_000),
                    ("cli", 42_000_000),
                    ("python", 148_000_000),
                    ("design", 210_000_000),
                    ("libreoffice", 862_000_000),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            },
        };
        let has_office = manifest.sizes.contains_key("libreoffice");
        let dir = match &mode {
            Mode::Uninstall { dir } => dir.clone(),
            _ => work::default_dir(),
        };
        let mut s = Setup {
            steps: work::install_steps(has_office),
            mode,
            exe,
            screen: Screen::Choose,
            dir,
            libreoffice: has_office,
            has_office,
            path: true,
            nudges: true,
            desktop: false,
            taskbar: true,
            version: manifest.version,
            sizes: manifest.sizes,
            step: 0,
            done: 0,
            total: 0,
            file: String::new(),
            launch_after: true,
            logo: Arc::new(Image::from_bytes(ImageFormat::Png, LOGO.to_vec())),
            pieces: [LOGO_DOT, LOGO_SLASH, LOGO_ANGLE]
                .map(|b| Arc::new(Image::from_bytes(ImageFormat::Png, b.to_vec()))),
            signature: Arc::new(Image::from_bytes(ImageFormat::Png, SIGNATURE.to_vec())),
        };
        if matches!(s.mode, Mode::Uninstall { .. }) {
            s.steps = work::UNINSTALL_STEPS.to_vec();
        }
        s
    }

    /// The screen's name, so its content glides in each time it changes.
    fn screen_name(&self) -> &'static str {
        match self.screen {
            Screen::Choose => "choose",
            Screen::Working => "working",
            Screen::Done(_) => "done",
            Screen::Failed(_) => "failed",
        }
    }

    fn uninstalling(&self) -> bool {
        matches!(self.mode, Mode::Uninstall { .. })
    }

    /// What the chosen parts take on disk.
    fn needed(&self) -> u64 {
        self.sizes
            .iter()
            .filter(|(k, _)| k.as_str() != "libreoffice" || self.libreoffice)
            .map(|(_, v)| *v)
            .sum()
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Working;
        self.step = 0;
        self.done = 0;
        self.total = 0;
        self.file.clear();
        if !self.uninstalling() {
            self.steps = work::install_steps(self.libreoffice);
        }
        let (tx, rx) = async_channel::unbounded::<Report>();
        let mode = self.mode.clone();
        let exe = self.exe.clone();
        let choices = Choices {
            dir: self.dir.clone(),
            libreoffice: self.libreoffice,
            path: self.path,
            nudges: self.nudges,
            desktop: self.desktop,
            taskbar: self.taskbar,
        };
        let steps = self.steps.len();
        std::thread::spawn(move || {
            // Bytes are told at most every 40 ms: thousands of small files would flood the window.
            let last = std::cell::Cell::new(std::time::Instant::now() - Duration::from_secs(1));
            let send = |r: Report| {
                if let Report::Bytes { .. } = r {
                    if last.get().elapsed() < Duration::from_millis(40) {
                        return;
                    }
                    last.set(std::time::Instant::now());
                }
                let _ = tx.send_blocking(r);
            };
            match mode {
                Mode::Install => work::install(&exe, &choices, send),
                Mode::Uninstall { dir } => work::uninstall(&dir, send),
                Mode::Preview => play_back(steps, send),
            }
        });
        cx.spawn(async move |this, cx| {
            while let Ok(r) = rx.recv().await {
                if this.update(cx, |s, cx| s.report(r, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn report(&mut self, r: Report, cx: &mut Context<Self>) {
        match r {
            Report::Step(i) => self.step = i,
            Report::Bytes { done, total, file } => {
                self.done = done;
                self.total = total;
                self.file = file;
            }
            Report::Done { warnings } => {
                self.step = self.steps.len();
                self.screen = Screen::Done(warnings);
                if let Mode::Uninstall { .. } = self.mode {
                    crate::system::delete_after_exit(&self.exe);
                }
            }
            Report::Failed(e) => self.screen = Screen::Failed(e),
        }
        cx.notify();
    }

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Install here".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                if let Some(p) = paths.into_iter().next() {
                    // A folder you pick gets KodAct's own folder inside it, unless it is one.
                    let p = if p
                        .file_name()
                        .is_some_and(|n| n.eq_ignore_ascii_case("KodAct"))
                    {
                        p
                    } else {
                        p.join("KodAct")
                    };
                    let _ = this.update(cx, |s, cx| {
                        s.dir = p;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.launch_after && matches!(self.mode, Mode::Install) {
            crate::system::launch(&self.dir);
        }
        window.remove_window();
        cx.quit();
    }
}

/// A preview's install: each step for a moment, the bar filling.
fn play_back(steps: usize, send: impl Fn(Report)) {
    let total = 1_000u64;
    for i in 0..steps {
        send(Report::Step(i));
        for k in 0..25u64 {
            std::thread::sleep(Duration::from_millis(45));
            let done = (i as u64 * 25 + k + 1) * total / (steps as u64 * 25);
            send(Report::Bytes {
                done,
                total,
                file: format!("part-{i}/file-{k}.bin"),
            });
        }
    }
    send(Report::Done { warnings: vec![] });
}

fn human(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", (bytes as f64 / 1e6).max(1.0))
    }
}

// ---------------------------------------------------------------- pieces

/// The filled button: burgundy, lit from above, a soft glow under it.
fn primary(id: &'static str, label: &'static str, arrow: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(42.))
        .px(px(22.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(12.))
        .cursor_pointer()
        .bg(linear_gradient(
            180.,
            linear_color_stop(rgb(0xb23a5c), 0.),
            linear_color_stop(Ink::burgundy_deep(), 1.),
        ))
        .border_1()
        .border_color(rgba(0xffffff26))
        .shadow(vec![BoxShadow {
            color: rgba(0x9c2a4c66).into(),
            offset: point(px(0.), px(10.)),
            blur_radius: px(28.),
            spread_radius: px(0.),
            inset: false,
        }])
        .text_size(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(Ink::text())
        .hover(|s| s.opacity(0.92))
        .child(label)
        .when(arrow, |d| d.child(icon("arrow", 16., Ink::text())))
}

/// The quiet button beside it.
fn ghost(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(42.))
        .px(px(18.))
        .flex()
        .items_center()
        .rounded(px(12.))
        .cursor_pointer()
        .text_size(px(14.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(Ink::muted())
        .hover(|s| s.bg(Ink::hover()).text_color(Ink::text()))
        .child(label)
}

/// An on/off switch: a pill with a knob, burgundy when on.
fn switch(on: bool) -> gpui::Div {
    div()
        .flex_none()
        .w(px(38.))
        .h(px(22.))
        .rounded_full()
        .p(px(3.))
        .flex()
        .when(on, |d| {
            d.justify_end().bg(linear_gradient(
                90.,
                linear_color_stop(Ink::burgundy_deep(), 0.),
                linear_color_stop(rgb(0xb23a5c), 1.),
            ))
        })
        .when(!on, |d| d.bg(rgb(0x2a2730)))
        .child(
            div()
                .size(px(16.))
                .rounded_full()
                .bg(if on { Ink::text() } else { Ink::muted() })
                .shadow(vec![BoxShadow {
                    color: rgba(0x00000066).into(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                    inset: false,
                }]),
        )
}

fn eyebrow(text: &'static str) -> gpui::Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(Ink::rose())
        .child(text)
}

fn title(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(Ink::text())
        .child(text.into())
}

fn lead(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(14.))
        .line_height(relative(1.55))
        .text_color(Ink::muted())
        .child(text.into())
}

// ---------------------------------------------------------------- the window

impl Setup {
    /// The left: the mark on a burgundy glow, the name, the line under it.
    fn brand(&self) -> impl IntoElement {
        let version = format!("Version {}", self.version);
        div()
            .relative()
            .w(px(340.))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .bg(linear_gradient(
                160.,
                linear_color_stop(rgb(0x2a0a17), 0.),
                linear_color_stop(Ink::bg(), 0.85),
            ))
            .border_r_1()
            .border_color(Ink::line())
            // The glow: a wide soft shadow of an empty circle behind the mark.
            .child(
                div()
                    .absolute()
                    .left(px(110.))
                    .top(px(200.))
                    .size(px(120.))
                    .rounded_full()
                    .shadow(vec![BoxShadow {
                        color: rgba(0xb23a5c8c).into(),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(120.),
                        spread_radius: px(30.),
                        inset: false,
                    }])
                    // Breathing, slowly, for as long as the window is open.
                    .with_animation(
                        "glow",
                        Animation::new(Duration::from_millis(3600)).repeat(),
                        |el, t| el.opacity(0.7 + 0.3 * (t * std::f32::consts::TAU).cos()),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(190.))
                    .child(self.mark())
                    .child(
                        div()
                            .relative()
                            .mt(px(26.))
                            .flex()
                            .text_size(px(40.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(div().text_color(Ink::text()).child("Kod"))
                            .child(div().text_color(Ink::rose()).child("Act"))
                            .with_animation("name-in", Animation::new(INTRO), |el, t| {
                                let p = glide(span(t, 0.5, 0.85));
                                el.top(px(16. * (1. - p))).opacity(p)
                            }),
                    )
                    .child(
                        div()
                            .mt(px(6.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(Ink::muted())
                            .child("F R O M   C O D E   T O   I M P A C T")
                            .with_animation("tagline-in", Animation::new(INTRO), |el, t| {
                                el.opacity(span(t, 0.68, 1.))
                            }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(32.))
                    .right(px(32.))
                    .bottom(px(26.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(Ink::subtle())
                            .child(version),
                    )
                    // The author's signature: a whisper of "crafted by", then the name in a
                    // light hand, written in from left to right as the intro ends.
                    .child(
                        div()
                            .text_size(px(9.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(Ink::subtle())
                            .child("C R A F T E D   B Y"),
                    )
                    .child({
                        // A frame of the signature's full width keeps it centred while the
                        // window inside it widens from the left: the name is written, not slid.
                        let (w, h) = SIGNATURE_SIZE;
                        div().w(px(w)).h(px(h)).mt(px(-4.)).child(
                            div()
                                .h(px(h))
                                .w(px(w))
                                .overflow_hidden()
                                .child(img(self.signature.clone()).w(px(w)).h(px(h)).flex_none())
                                .with_animation("signature-write", Animation::new(SIGNED), move |el, t| {
                                    // The pen moves quickly, then eases into the last letters.
                                    let p = 1. - (1. - span(t, 0.55, 1.)).powi(3);
                                    el.w(px(w * p))
                                }),
                        )
                    })
                    .with_animation("signature-in", Animation::new(INTRO), |el, t| {
                        el.opacity(span(t, 0.7, 1.))
                    }),
            )
    }

    /// The mark coming together: the dot drops in, the slash slides down, the angle slides in
    /// from the right, each landing with a touch of overshoot; then the whole mark takes over.
    fn mark(&self) -> impl IntoElement {
        let [dot, slash, angle] = self.pieces.clone();
        // (piece, id, start, end, from x, from y)
        let moves: [(Arc<Image>, &'static str, f32, f32, f32, f32); 3] = [
            (dot, "dot", 0.0, 0.38, 0., -46.),
            (slash, "slash", 0.12, 0.52, 0., -70.),
            (angle, "angle", 0.26, 0.68, 64., 0.),
        ];
        let mut stack = div().relative().w(px(140.)).h(px(136.));
        for (image, id, a, b, dx, dy) in moves {
            stack = stack.child(
                img(image)
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(140.))
                    .h(px(136.))
                    .with_animation(id, Animation::new(INTRO), move |el, t| {
                        let p = land(span(t, a, b));
                        let shown = span(t, a, a + 0.12) * (1. - span(t, 0.82, 0.92));
                        el.left(px(dx * (1. - p)))
                            .top(px(dy * (1. - p)))
                            .opacity(shown)
                    }),
            );
        }
        stack.child(
            img(self.logo.clone())
                .absolute()
                .top_0()
                .left_0()
                .w(px(140.))
                .h(px(136.))
                .with_animation("mark-whole", Animation::new(INTRO), |el, t| {
                    el.opacity(span(t, 0.82, 0.92))
                }),
        )
    }

    /// The strip at the top: drags the window; minimize and close at its end.
    fn bar(&self, window: &Window) -> impl IntoElement {
        let control =
            |id: &'static str, glyph: &'static str, area: WindowControlArea, danger: bool| {
                div()
                    .id(id)
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(move |s| {
                        if danger {
                            s.bg(rgba(0xe5484d33))
                        } else {
                            s.bg(Ink::hover())
                        }
                    })
                    .when(cfg!(windows), move |d| d.window_control_area(area))
                    .when(cfg!(not(windows)), move |d| {
                        d.on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(move |_, window, cx| match area {
                            WindowControlArea::Min => window.minimize_window(),
                            _ => {
                                window.remove_window();
                                cx.quit();
                            }
                        })
                    })
                    .child(icon(glyph, 15., Ink::muted()))
            };
        let _ = window;
        div()
            .id("bar")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(44.))
            .px(px(8.))
            .flex()
            .items_center()
            .justify_end()
            .gap(px(2.))
            .window_control_area(WindowControlArea::Drag)
            .when(cfg!(not(windows)), |d| {
                d.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            })
            .child(control("min", "minus", WindowControlArea::Min, false))
            .child(control("close", "x", WindowControlArea::Close, true))
    }

    fn location(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editable = !self.uninstalling();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(Ink::muted())
                    .child(if editable {
                        "Install to"
                    } else {
                        "Installed in"
                    }),
            )
            .child(
                div()
                    .h(px(44.))
                    .pl(px(14.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(12.))
                    .bg(Ink::field())
                    .border_1()
                    .border_color(Ink::line_strong())
                    .child(icon("folder", 16., Ink::rose()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.))
                            .text_color(Ink::text())
                            .child(self.dir.display().to_string()),
                    )
                    .when(editable, |d| {
                        d.child(
                            div()
                                .id("change")
                                .px(px(12.))
                                .h(px(32.))
                                .flex()
                                .items_center()
                                .rounded(px(8.))
                                .cursor_pointer()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(Ink::rose())
                                .hover(|s| s.bg(Ink::rose_tint()))
                                .child("Change")
                                .on_click(cx.listener(|this, _, _, cx| this.pick_folder(cx))),
                        )
                    }),
            )
    }

    fn option(
        &self,
        id: &'static str,
        name: &'static str,
        about: &'static str,
        size: Option<String>,
        on: bool,
        flip: fn(&mut Setup),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .px(px(14.))
            .py(px(8.))
            .flex()
            .items_center()
            .gap(px(14.))
            .rounded(px(12.))
            .cursor_pointer()
            .hover(|s| s.bg(Ink::hover()))
            .on_click(cx.listener(move |this, _, _, cx| {
                flip(this);
                cx.notify();
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(Ink::text())
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(Ink::subtle())
                            .child(about),
                    ),
            )
            .children(size.map(|s| div().text_size(px(12.)).text_color(Ink::subtle()).child(s)))
            .child(switch(on))
    }

    fn choose(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.uninstalling() {
            return div()
                .flex()
                .flex_col()
                .gap(px(28.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(eyebrow("UNINSTALL"))
                        .child(title("Remove KodAct"))
                        .child(lead(
                            "The app, KodAct CLI and everything installed with them are removed, with the shortcuts and the PATH entry setup added. Your chats, projects and settings stay.",
                        )),
                )
                .child(self.location(cx))
                .into_any_element();
        }
        let mut options = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(6.))
            .rounded(px(16.))
            .bg(Ink::panel())
            .border_1()
            .border_color(Ink::line());
        if self.has_office {
            let size = self.sizes.get("libreoffice").map(|b| human(*b));
            options = options.child(self.option(
                "office",
                "LibreOffice",
                "Word, Excel and PowerPoint files, exactly as they print",
                size,
                self.libreoffice,
                |s| s.libreoffice = !s.libreoffice,
                cx,
            ));
        }
        options = options
            .child(self.option(
                "path",
                "kodact in your terminal",
                "Adds KodAct CLI to your PATH",
                None,
                self.path,
                |s| s.path = !s.path,
                cx,
            ))
            .child(self.option(
                "nudges",
                "Every KodAct nudge",
                "Turns on all of KodAct CLI's nudges for your account",
                None,
                self.nudges,
                |s| s.nudges = !s.nudges,
                cx,
            ))
            .child(self.option(
                "desktop",
                "Desktop shortcut",
                "A KodAct icon on your desktop",
                None,
                self.desktop,
                |s| s.desktop = !s.desktop,
                cx,
            ))
            .child(self.option(
                "taskbar",
                "Pin to taskbar",
                "KodAct one click away, at the bottom of your screen",
                None,
                self.taskbar,
                |s| s.taskbar = !s.taskbar,
                cx,
            ));
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(eyebrow("SETUP"))
                    .child(title("Install KodAct"))
                    .child(lead(
                        "The desktop app and KodAct CLI, with everything they work with. Just for you: no administrator rights needed.",
                    )),
            )
            .child(self.location(cx))
            .child(options)
            .into_any_element()
    }

    fn working(&self) -> gpui::AnyElement {
        let fraction = if self.total > 0 {
            (self.done as f32 / self.total as f32).clamp(0., 1.)
        } else {
            0.
        };
        let mut list = div().flex().flex_col().gap(px(4.));
        for (i, name) in self.steps.iter().enumerate() {
            let (state_icon, color): (gpui::AnyElement, Hsla) = if i < self.step {
                (
                    div()
                        .size(px(22.))
                        .rounded_full()
                        .bg(Ink::rose_tint())
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon("check", 13., Ink::rose()))
                        .into_any_element(),
                    Ink::muted(),
                )
            } else if i == self.step {
                (
                    div()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon("spinner", 18., Ink::rose()).with_animation(
                            "spin",
                            Animation::new(Duration::from_millis(900)).repeat(),
                            |s, d| s.with_transformation(Transformation::rotate(percentage(d))),
                        ))
                        .into_any_element(),
                    Ink::text(),
                )
            } else {
                (
                    div()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(6.)).rounded_full().bg(Ink::subtle()))
                        .into_any_element(),
                    Ink::subtle(),
                )
            };
            list = list.child(
                div()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(state_icon)
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(if i == self.step {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(color)
                            .child(*name),
                    ),
            );
        }
        let (heading, note) = if self.uninstalling() {
            ("Removing KodAct", "This takes a moment.")
        } else {
            (
                "Installing KodAct",
                "A minute or two. You can leave this window open.",
            )
        };
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(eyebrow("WORKING"))
                    .child(title(heading))
                    .child(lead(note)),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .justify_between()
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(12.))
                                    .text_color(Ink::subtle())
                                    .child(self.file.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(Ink::text())
                                    .child(format!("{:.0}%", fraction * 100.)),
                            ),
                    )
                    .child(
                        div()
                            .h(px(6.))
                            .w_full()
                            .rounded_full()
                            .bg(rgb(0x221f27))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(fraction.max(0.01)))
                                    .rounded_full()
                                    .bg(linear_gradient(
                                        90.,
                                        linear_color_stop(Ink::burgundy(), 0.),
                                        linear_color_stop(Ink::rose(), 1.),
                                    ))
                                    .shadow(vec![BoxShadow {
                                        color: rgba(0xe0889f55).into(),
                                        offset: point(px(0.), px(0.)),
                                        blur_radius: px(10.),
                                        spread_radius: px(0.),
                                        inset: false,
                                    }]),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn done(&self, warnings: &[String], cx: &mut Context<Self>) -> gpui::AnyElement {
        let removing = self.uninstalling();
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .size(px(64.))
                    .rounded_full()
                    .bg(Ink::rose_tint())
                    .border_1()
                    .border_color(rgba(0xe0889f40))
                    .flex()
                    .items_center()
                    .justify_center()
                    .shadow(vec![BoxShadow {
                        color: rgba(0xb23a5c59).into(),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(40.),
                        spread_radius: px(0.),
            inset: false,
                    }])
                    .child(icon("check", 30., Ink::rose()))
                    .with_animation(
                        "done-in",
                        Animation::new(Duration::from_millis(500)).with_easing(gpui::ease_out_quint()),
                        |el, d| el.opacity(d),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(eyebrow("DONE"))
                    .child(title(if removing { "KodAct is removed" } else { "KodAct is ready" }))
                    .child(lead(if removing {
                        "Your chats, projects and settings are still in your profile, should you install KodAct again."
                    } else {
                        "Find it in the Start menu anytime. Each chat runs KodAct CLI, which Settings → About can replace with a newer one."
                    })),
            )
            .children(warnings.iter().map(|w| {
                div()
                    .flex()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .text_color(Ink::warn())
                    .child(icon("alert", 15., Ink::warn()))
                    .child(w.clone())
            }))
            .when(!removing, |d| {
                d.child(
                    div()
                        .id("launch")
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.launch_after = !this.launch_after;
                            cx.notify();
                        }))
                        .child(switch(self.launch_after))
                        .child(div().text_size(px(14.)).text_color(Ink::text()).child("Open KodAct when I close this")),
                )
            })
            .into_any_element()
    }

    fn failed(&self, why: &str) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .size(px(56.))
                    .rounded_full()
                    .bg(rgba(0xf07a7a1f))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("alert", 26., Ink::danger())),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(eyebrow("SOMETHING STOPPED IT"))
                    .child(title("Setup couldn't finish"))
                    .child(lead(why.to_string())),
            )
            .into_any_element()
    }

    fn footer(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let note: String = match (&self.screen, self.uninstalling()) {
            (Screen::Choose, false) => format!("Needs {} of space", human(self.needed())),
            (Screen::Working, _) => String::new(),
            _ => String::new(),
        };
        let buttons = match (&self.screen, self.uninstalling()) {
            (Screen::Choose, false) => div()
                .flex()
                .gap(px(8.))
                .child(
                    ghost("cancel", "Cancel").on_click(cx.listener(|_, _, window, cx| {
                        window.remove_window();
                        cx.quit();
                    })),
                )
                .child(
                    primary("install", "Install", true)
                        .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                ),
            (Screen::Choose, true) => div()
                .flex()
                .gap(px(8.))
                .child(
                    ghost("cancel", "Keep KodAct").on_click(cx.listener(|_, _, window, cx| {
                        window.remove_window();
                        cx.quit();
                    })),
                )
                .child(
                    primary("remove", "Remove", false)
                        .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                ),
            (Screen::Working, _) => div(),
            (Screen::Done(_), removing) => div().flex().gap(px(8.)).child(
                primary(
                    "finish",
                    if removing { "Close" } else { "Finish" },
                    !removing,
                )
                .on_click(cx.listener(|this, _, window, cx| this.finish(window, cx))),
            ),
            (Screen::Failed(_), _) => div()
                .flex()
                .gap(px(8.))
                .child(
                    ghost("close", "Close").on_click(cx.listener(|_, _, window, cx| {
                        window.remove_window();
                        cx.quit();
                    })),
                )
                .child(
                    primary("again", "Try again", false)
                        .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                ),
        };
        div()
            .mt(px(16.))
            .h(px(42.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(Ink::subtle())
                    .child(note),
            )
            .child(buttons)
            .into_any_element()
    }
}

impl Render for Setup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.screen.clone() {
            Screen::Choose => self.choose(cx),
            Screen::Working => self.working(),
            Screen::Done(w) => self.done(&w, cx),
            Screen::Failed(why) => self.failed(&why),
        };
        div()
            .id("setup")
            .relative()
            .size_full()
            .flex()
            .bg(Ink::bg())
            .font_family(font())
            .text_color(Ink::text())
            .child(self.brand())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .px(px(44.))
                    .pt(px(52.))
                    .pb(px(28.))
                    .child(div().relative().child(body).with_animation(
                        SharedString::from(format!("body-{}", self.screen_name())),
                        Animation::new(Duration::from_millis(520)),
                        |el, t| {
                            let p = glide(span(t, 0.1, 1.));
                            el.top(px(12. * (1. - p))).opacity(p)
                        },
                    ))
                    .child(self.footer(cx)),
            )
            .child(self.bar(window))
    }
}

pub fn run(mode: Mode, exe: PathBuf) {
    gpui::application()
        .with_assets(Icons)
        .run(move |cx: &mut App| {
            gpui::init(cx);
            let bounds = Bounds::centered(None, size(px(900.), px(680.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("KodAct Setup".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                app_owns_titlebar_drag: true,
                window_decorations: Some(WindowDecorations::Client),
                is_resizable: false,
                ..Default::default()
            };
            let screen = std::env::var("KODACT_SETUP_SCREEN").unwrap_or_default();
            gpui::open_window(options, cx, move |_, cx| {
                cx.new(|cx| {
                    let mode = if screen == "uninstall" {
                        Mode::Uninstall {
                            dir: work::default_dir(),
                        }
                    } else {
                        mode
                    };
                    let mut s = Setup::new(mode, exe);
                    // A screen held for a screenshot.
                    match screen.as_str() {
                        "working" => {
                            s.screen = Screen::Working;
                            s.step = 2;
                            s.done = 430;
                            s.total = 1000;
                            s.file = "python/Lib/site-packages/openpyxl/styles/fills.py".into();
                        }
                        "done" => {
                            s.step = s.steps.len();
                            s.screen = Screen::Done(vec![]);
                        }
                        "failed" => s.screen = Screen::Failed(work::RUNNING.into()),
                        _ => {}
                    }
                    let _ = cx;
                    s
                })
            })
            .expect("the setup window opens");
            cx.activate(true);
        });
}
