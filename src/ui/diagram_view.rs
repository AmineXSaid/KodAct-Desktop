//! A diagram drawn natively: its SVG (`diagram.rs`) rasterised by GPUI at
//! the exact device pixels it is shown at. Rasters are cached by drawing
//! and width, so a repaint reuses them and only a new size costs a render.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use gpui::{img, prelude::*, px, AnyElement, Image, ImageFormat, ObjectFit};

/// Rasters kept; past this the cache starts again (a resize drag makes many).
const KEEP: usize = 24;

thread_local! {
    static CACHE: RefCell<HashMap<(u64, u32), Arc<Image>>> = RefCell::new(HashMap::new());
}

fn key(svg: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    svg.hash(&mut h);
    h.finish()
}

/// The raster of `svg` at `width` logical pixels (on a display of `scale`
/// device pixels per logical one), and the drawing's own size.
fn raster(svg: &str, width: f32, scale: f32) -> Option<(Arc<Image>, f32, f32)> {
    let (w, h) = crate::diagram::view_size(svg)?;
    let device = (width.floor().max(1.) * scale).round() as u32;
    let k = (key(svg), device);
    let image = CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(i) = c.get(&k) {
            return Some(i.clone());
        }
        let sized = crate::diagram::sized(svg, device)?;
        if c.len() >= KEEP {
            c.clear();
        }
        let i = Arc::new(Image::from_bytes(ImageFormat::Svg, sized.into_bytes()));
        c.insert(k, i.clone());
        Some(i)
    })?;
    Some((image, w, h))
}

/// `svg` drawn exactly `width` logical pixels wide, its height from its aspect.
pub fn image(svg: &str, width: f32, scale: f32) -> Option<AnyElement> {
    let (image, w, h) = raster(svg, width, scale)?;
    let width = width.floor().max(1.);
    Some(img(image).w(px(width)).h(px(width * h / w)).object_fit(ObjectFit::Contain).into_any_element())
}

/// Drawn at most `width` wide, but shrinking with its container (keeping its
/// aspect) when that is narrower: for places whose width layout decides.
pub fn fluid(svg: &str, width: f32, scale: f32) -> Option<AnyElement> {
    let (image, w, h) = raster(svg, width, scale)?;
    Some(gpui::div().w_full().max_w(px(width.floor())).aspect_ratio(w / h).child(img(image).size_full().object_fit(ObjectFit::Contain)).into_any_element())
}

/// The drawing to show in this theme: a Forge-skinned diagram turns dark
/// with the app.
pub fn themed(svg: &str, dark: bool) -> std::borrow::Cow<'_, str> {
    match dark.then(|| crate::diagram::for_dark(svg)).flatten() {
        Some(d) => d.into(),
        None => svg.into(),
    }
}

/// The drawing's own width, for "actual size".
pub fn natural_width(svg: &str) -> Option<f32> {
    crate::diagram::view_size(svg).map(|(w, _)| w)
}
