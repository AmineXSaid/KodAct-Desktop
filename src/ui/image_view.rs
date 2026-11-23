//! A picture in the side panel (png, jpg, gif, webp): fitted to the panel or
//! at its own size, zoomed by the toolbar, Ctrl +/- and Ctrl + the wheel,
//! panned by dragging, on a checkerboard where it can be transparent. Its
//! size and weight read in the footer.

use std::cell::RefCell;
use std::sync::Arc;

use gpui::{canvas, div, fill, prelude::*, px, AnyElement, Bounds, Pixels, SharedString};

use super::doc_pane::Fit;
use crate::doc_zoom::Zoom;
use crate::state::FileRef;
use crate::theme::Theme;

/// What a picture's file says about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    /// It can be transparent: draw a checkerboard behind it.
    pub alpha: bool,
}

/// The image format of a file name, if it is a picture the panel shows.
pub fn format_of(name: &str) -> Option<gpui::ImageFormat> {
    match name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "png" => Some(gpui::ImageFormat::Png),
        "jpg" | "jpeg" => Some(gpui::ImageFormat::Jpeg),
        "gif" => Some(gpui::ImageFormat::Gif),
        "webp" => Some(gpui::ImageFormat::Webp),
        _ => None,
    }
}

/// Its size, read from the header alone.
pub fn info(name: &str, bytes: &[u8]) -> Option<Info> {
    let format = format_of(name)?;
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    Some(Info {
        width,
        height,
        bytes: bytes.len(),
        alpha: format != gpui::ImageFormat::Jpeg,
    })
}

/// "1,280 × 800 · 128 KB".
pub fn describe(i: &Info) -> String {
    let kb = i.bytes as f32 / 1024.;
    let size = if kb >= 1024. {
        format!("{:.1} MB", kb / 1024.)
    } else {
        format!("{:.0} KB", kb.max(1.))
    };
    format!("{} × {} · {size}", i.width, i.height)
}

thread_local! {
    /// The picture last drawn, so a frame does not copy its bytes again.
    static LAST: RefCell<Option<((usize, usize), Arc<gpui::Image>)>> = const { RefCell::new(None) };
}

fn image_of(name: &str, bytes: &Arc<[u8]>) -> Option<Arc<gpui::Image>> {
    let key = (bytes.as_ptr() as usize, bytes.len());
    LAST.with(|l| {
        if let Some((k, img)) = l.borrow().as_ref() {
            if *k == key {
                return Some(img.clone());
            }
        }
        let img = Arc::new(gpui::Image::from_bytes(format_of(name)?, bytes.to_vec()));
        *l.borrow_mut() = Some((key, img.clone()));
        Some(img)
    })
}

/// The width to draw at: a percentage of its own size, or fitted (never
/// stretched past its own size).
pub fn width_for(zoom: Zoom, info: &Info, scale: f32, room: f32, room_h: f32) -> f32 {
    let natural = info.width as f32 / scale.max(1.);
    let aspect = info.height as f32 / info.width.max(1) as f32;
    match zoom {
        Zoom::Percent(p) => natural * f32::from(p) / 100.,
        Zoom::FitWidth => room.min(natural),
        Zoom::FitPage => room.min(natural).min(room_h / aspect.max(0.01)),
    }
}

/// The checkerboard's cell.
const CELL: f32 = 10.;

/// The picture, centred in the body (it scrolls when larger than it).
pub fn render(file: &FileRef, fit: &Fit, t: &Theme) -> AnyElement {
    let Some(bytes) = file.bytes.as_ref() else {
        return quiet("This picture has no data to show.", t);
    };
    let (Some(info), Some(image)) = (info(&file.name, bytes), image_of(&file.name, bytes)) else {
        return quiet("KodAct could not read this picture.", t);
    };
    let pad = f32::from(t.space.xl);
    let room = (fit.width - 2. * pad - 2.).max(80.);
    let room_h = (fit.height - 2. * pad).max(80.);
    let w = width_for(fit.zoom, &info, fit.scale, room, room_h);
    let h = w * info.height as f32 / info.width.max(1) as f32;
    fit.fitted.store(
        crate::doc_zoom::percent_of(w, info.width as f32 / fit.scale.max(1.)),
        std::sync::atomic::Ordering::Relaxed,
    );
    let (light, dark) = (t.colors.surface, t.extra.sunk);
    let checker = info.alpha.then(|| {
        canvas(
            |_, _, _| (),
            move |bounds: Bounds<Pixels>, _, window, _| {
                window.paint_quad(fill(bounds, light));
                let (cols, rows) = (
                    (f32::from(bounds.size.width) / CELL).ceil() as i32,
                    (f32::from(bounds.size.height) / CELL).ceil() as i32,
                );
                for r in 0..rows {
                    for c in (r % 2..cols).step_by(2) {
                        let origin =
                            bounds.origin + gpui::point(px(c as f32 * CELL), px(r as f32 * CELL));
                        let cell = Bounds {
                            origin,
                            size: gpui::size(px(CELL), px(CELL)),
                        }
                        .intersect(&bounds);
                        window.paint_quad(fill(cell, dark));
                    }
                }
            },
        )
        .absolute()
        .size_full()
    });
    div()
        .min_w_full()
        .min_h_full()
        .flex()
        .items_center()
        .justify_center()
        .p(px(pad))
        .child(
            div()
                .relative()
                .flex_none()
                .w(px(w))
                .h(px(h))
                .border_1()
                .border_color(t.colors.border)
                .cursor(gpui::CursorStyle::OpenHand)
                .children(checker)
                .child(
                    gpui::img(image)
                        .absolute()
                        .size_full()
                        .object_fit(gpui::ObjectFit::Fill),
                ),
        )
        .into_any_element()
}

fn quiet(text: &str, t: &Theme) -> AnyElement {
    div()
        .p(t.space.xl)
        .text_size(t.text.body_sm)
        .text_color(t.colors.text_muted)
        .child(SharedString::from(text.to_string()))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::new(w, h);
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_pictures_size_is_read_from_its_header() {
        let i = info("a.png", &png(30, 20)).unwrap();
        assert_eq!((i.width, i.height, i.alpha), (30, 20, true));
        assert!(info("a.txt", &png(3, 2)).is_none());
        assert!(info("a.png", b"not a picture").is_none());
    }

    #[test]
    fn the_footer_says_size_and_weight() {
        let i = Info {
            width: 1280,
            height: 800,
            bytes: 131072,
            alpha: false,
        };
        assert_eq!(describe(&i), "1280 × 800 · 128 KB");
    }

    #[test]
    fn fitted_pictures_never_stretch_and_zoomed_ones_scale() {
        let i = Info {
            width: 400,
            height: 200,
            bytes: 1,
            alpha: false,
        };
        assert_eq!(width_for(Zoom::FitWidth, &i, 1., 900., 900.), 400.);
        assert_eq!(width_for(Zoom::FitWidth, &i, 1., 300., 900.), 300.);
        assert_eq!(width_for(Zoom::FitPage, &i, 1., 900., 100.), 200.);
        assert_eq!(width_for(Zoom::Percent(200), &i, 1., 300., 100.), 800.);
        assert_eq!(width_for(Zoom::Percent(100), &i, 2., 300., 100.), 200.);
    }
}
