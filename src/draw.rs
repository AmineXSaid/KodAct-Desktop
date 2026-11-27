//! Editing a picture you attached (`ui/attachment_editor.rs`). Every edit
//! is an [`EditOp`] kept beside the untouched original, and the picture the
//! model sees is the original with the ops replayed over it ([`render`]).
//! So Undo, Redo and Reset never lose quality, and reopening the editor
//! picks up where you left off.
//!
//! Each op's coordinates are in the picture's pixels as it stood when the
//! op was made, after the crops and turns before it.

use image::{Rgba, RgbaImage};

/// One freehand stroke: its colour (RGBA), its width in the picture's
/// pixels, and the points it passes through, in the picture's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: [u8; 4],
    pub width: f32,
    pub points: Vec<(f32, f32)>,
}

/// One edit, in the order it was made.
#[derive(Clone, Debug, PartialEq)]
pub enum EditOp {
    /// An opaque pen line, antialiased, with round caps and joins.
    Pen(Stroke),
    /// A translucent marker: where it crosses itself it does not darken.
    Highlight(Stroke),
    /// A line with an open arrowhead at `to`.
    Arrow {
        from: (f32, f32),
        to: (f32, f32),
        color: [u8; 4],
        width: f32,
    },
    /// A rectangle's outline between two corners.
    Rect {
        a: (f32, f32),
        b: (f32, f32),
        color: [u8; 4],
        width: f32,
    },
    /// An ellipse's outline inside the box between two corners.
    Ellipse {
        a: (f32, f32),
        b: (f32, f32),
        color: [u8; 4],
        width: f32,
    },
    /// A label on a filled tag in `color`, its top-left at `at`; `size` is
    /// the cap height in the picture's pixels.
    Text {
        at: (f32, f32),
        text: String,
        color: [u8; 4],
        size: f32,
    },
    /// A mosaic over the box between two corners, hiding what is under it.
    Redact { a: (f32, f32), b: (f32, f32) },
    /// Keep only the box between two corners.
    Crop { a: (f32, f32), b: (f32, f32) },
    /// A quarter turn anticlockwise.
    Rotate,
}

/// How opaque the highlighter is.
pub const HIGHLIGHT_ALPHA: f32 = 0.38;

/// The picture's size after `ops`, from its size before them.
pub fn size_after(mut w: u32, mut h: u32, ops: &[EditOp]) -> (u32, u32) {
    for op in ops {
        match op {
            EditOp::Rotate => std::mem::swap(&mut w, &mut h),
            EditOp::Crop { a, b } => {
                if let Some((_, _, cw, ch)) = crop_box(w, h, *a, *b) {
                    (w, h) = (cw, ch);
                }
            }
            _ => {}
        }
    }
    (w, h)
}

/// The original with `ops` replayed over it.
pub fn render(picture: &[u8], ops: &[EditOp]) -> Result<RgbaImage, String> {
    let mut img = image::load_from_memory(picture)
        .map_err(|e| format!("the picture could not be read: {e}"))?
        .to_rgba8();
    for op in ops {
        apply(&mut img, op);
    }
    Ok(img)
}

/// The original with `ops` replayed over it, as PNG bytes.
pub fn render_png(picture: &[u8], ops: &[EditOp]) -> Result<Vec<u8>, String> {
    png(&render(picture, ops)?)
}

/// The picture with pen `strokes` drawn on it, as PNG bytes.
#[cfg(test)]
pub fn burn(picture: &[u8], strokes: &[Stroke]) -> Result<Vec<u8>, String> {
    let ops: Vec<EditOp> = strokes.iter().cloned().map(EditOp::Pen).collect();
    render_png(picture, &ops)
}

pub fn png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| format!("the drawing could not be saved: {e}"))?;
    Ok(out.into_inner())
}

fn apply(img: &mut RgbaImage, op: &EditOp) {
    match op {
        EditOp::Pen(s) => {
            let mut m = Mask::new(img);
            m.polyline(&s.points, s.width);
            m.blend(img, s.color, 1.);
        }
        EditOp::Highlight(s) => {
            let mut m = Mask::new(img);
            m.polyline(&s.points, s.width);
            m.blend(img, s.color, HIGHLIGHT_ALPHA);
        }
        EditOp::Arrow {
            from,
            to,
            color,
            width,
        } => {
            let mut m = Mask::new(img);
            m.polyline(&[*from, *to], *width);
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len > 0.5 {
                let head = (width * 4.).max(10.).min(len * 0.6);
                let ang = dy.atan2(dx);
                for side in [-1f32, 1.] {
                    let a = ang + std::f32::consts::PI - side * 0.5;
                    let tip = (to.0 + head * a.cos(), to.1 + head * a.sin());
                    m.polyline(&[tip, *to], *width);
                }
            }
            m.blend(img, *color, 1.);
        }
        EditOp::Rect { a, b, color, width } => {
            let (x0, y0, x1, y1) = (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1));
            let mut m = Mask::new(img);
            m.polyline(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)], *width);
            m.blend(img, *color, 1.);
        }
        EditOp::Ellipse { a, b, color, width } => {
            let (cx, cy) = ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
            let (rx, ry) = ((a.0 - b.0).abs() / 2., (a.1 - b.1).abs() / 2.);
            let n = ((rx + ry) * 0.75).clamp(48., 720.) as usize;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    (cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect();
            let mut m = Mask::new(img);
            m.polyline(&pts, *width);
            m.blend(img, *color, 1.);
        }
        EditOp::Text {
            at,
            text,
            color,
            size,
        } => text_tag(img, *at, text, *color, *size),
        EditOp::Redact { a, b } => {
            if let Some((x, y, w, h)) = crop_box(img.width(), img.height(), *a, *b) {
                mosaic(img, x, y, w, h);
            }
        }
        EditOp::Crop { a, b } => {
            if let Some((x, y, w, h)) = crop_box(img.width(), img.height(), *a, *b) {
                *img = image::imageops::crop_imm(img, x, y, w, h).to_image();
            }
        }
        EditOp::Rotate => *img = image::imageops::rotate270(img),
    }
}

/// The whole-pixel box between two corners, inside a `w`×`h` picture;
/// None when it is less than a pixel either way.
pub fn crop_box(w: u32, h: u32, a: (f32, f32), b: (f32, f32)) -> Option<(u32, u32, u32, u32)> {
    let x0 = a.0.min(b.0).round().clamp(0., w as f32) as u32;
    let x1 = a.0.max(b.0).round().clamp(0., w as f32) as u32;
    let y0 = a.1.min(b.1).round().clamp(0., h as f32) as u32;
    let y1 = a.1.max(b.1).round().clamp(0., h as f32) as u32;
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

/// Coverage of one shape, kept apart from the picture so a shape crossing
/// itself is blended once: the highlighter never darkens where it overlaps.
struct Mask {
    w: usize,
    h: usize,
    cov: Vec<f32>,
}

impl Mask {
    fn new(img: &RgbaImage) -> Self {
        let (w, h) = (img.width() as usize, img.height() as usize);
        Mask {
            w,
            h,
            cov: vec![0.; w * h],
        }
    }

    /// A line through `pts`, `width` wide, with round caps and joins and a
    /// one-pixel antialiased edge.
    fn polyline(&mut self, pts: &[(f32, f32)], width: f32) {
        let r = (width / 2.).max(0.5);
        match pts {
            [] => {}
            [p] => self.segment(*p, *p, r),
            _ => {
                for s in pts.windows(2) {
                    self.segment(s[0], s[1], r);
                }
            }
        }
    }

    fn segment(&mut self, a: (f32, f32), b: (f32, f32), r: f32) {
        if self.w == 0 || self.h == 0 {
            return;
        }
        let pad = r + 1.;
        let x0 = (a.0.min(b.0) - pad).floor().max(0.) as usize;
        let y0 = (a.1.min(b.1) - pad).floor().max(0.) as usize;
        let x1 = ((a.0.max(b.0) + pad).ceil() as isize).min(self.w as isize - 1);
        let y1 = ((a.1.max(b.1) + pad).ceil() as isize).min(self.h as isize - 1);
        if x1 < 0 || y1 < 0 {
            return;
        }
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        for y in y0..=y1 as usize {
            for x in x0..=x1 as usize {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = if len2 > 0. {
                    (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0., 1.)
                } else {
                    0.
                };
                let (qx, qy) = (a.0 + dx * t - px, a.1 + dy * t - py);
                let d = (qx * qx + qy * qy).sqrt();
                let c = (r + 0.5 - d).clamp(0., 1.);
                let cell = &mut self.cov[y * self.w + x];
                if c > *cell {
                    *cell = c;
                }
            }
        }
    }

    fn blend(&self, img: &mut RgbaImage, c: [u8; 4], alpha: f32) {
        let a = alpha * f32::from(c[3]) / 255.;
        for (i, &k) in self.cov.iter().enumerate() {
            if k <= 0. {
                continue;
            }
            let px = img.get_pixel_mut((i % self.w) as u32, (i / self.w) as u32);
            over(px, c, a * k);
        }
    }
}

/// `c` at opacity `a` over a pixel.
fn over(px: &mut Rgba<u8>, c: [u8; 4], a: f32) {
    for i in 0..3 {
        px.0[i] = (f32::from(c[i]) * a + f32::from(px.0[i]) * (1. - a)).round() as u8;
    }
    px.0[3] = (255. * a + f32::from(px.0[3]) * (1. - a)).round() as u8;
}

/// Each block of the box replaced by its average colour. Blocks are a
/// fortieth of the picture's longer side, at least 8 pixels.
fn mosaic(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32) {
    let block = (img.width().max(img.height()) / 40).max(8);
    let mut by = y;
    while by < y + h {
        let bh = block.min(y + h - by);
        let mut bx = x;
        while bx < x + w {
            let bw = block.min(x + w - bx);
            let mut sum = [0u64; 4];
            for yy in by..by + bh {
                for xx in bx..bx + bw {
                    let p = img.get_pixel(xx, yy).0;
                    for i in 0..4 {
                        sum[i] += u64::from(p[i]);
                    }
                }
            }
            let n = u64::from(bw * bh);
            let avg = Rgba(sum.map(|s| (s / n) as u8));
            for yy in by..by + bh {
                for xx in bx..bx + bw {
                    img.put_pixel(xx, yy, avg);
                }
            }
            bx += bw;
        }
        by += bh;
    }
}

/// The text's box: its width and height in the picture's pixels, tag
/// padding included, for a cap height of `size`.
pub fn text_size(text: &str, size: f32) -> (f32, f32) {
    let k = cell(size);
    let lines: Vec<&str> = text.split('\n').collect();
    let cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
    let w = (cols * 6. - 1.).max(0.) * k + 2. * PAD * k;
    let h = (lines.len() as f32 * 9. - 2.) * k + 2. * PAD * k;
    (w, h)
}

/// Tag padding, in font cells.
const PAD: f32 = 2.;

/// A font cell's size, in picture pixels, for a cap height.
fn cell(size: f32) -> f32 {
    (size / 7.).round().max(1.)
}

/// A filled tag in `color` with the text on it in white or near-black,
/// whichever reads (WCAG contrast against the tag).
fn text_tag(img: &mut RgbaImage, at: (f32, f32), text: &str, color: [u8; 4], size: f32) {
    if text.trim().is_empty() {
        return;
    }
    let k = cell(size);
    let (w, h) = text_size(text, size);
    let (x0, y0) = (at.0.round(), at.1.round());
    let mut tag = Mask::new(img);
    tag.round_rect(x0, y0, w, h, (k * 2.).min(h / 2.));
    tag.blend(img, color, 1.);
    let ink = if luminance(color) > 0.18 {
        [24, 22, 20, 255]
    } else {
        [255, 255, 255, 255]
    };
    let mut glyphs = Mask::new(img);
    for (row, line) in text.split('\n').enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let g = glyph(ch);
            let gx = x0 + PAD * k + col as f32 * 6. * k;
            let gy = y0 + PAD * k + row as f32 * 9. * k;
            for (r, bits) in g.iter().enumerate() {
                for c in 0..5 {
                    if bits & (0x10 >> c) != 0 {
                        glyphs.fill(gx + c as f32 * k, gy + r as f32 * k, k, k);
                    }
                }
            }
        }
    }
    glyphs.blend(img, ink, 1.);
}

impl Mask {
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let x0 = x.max(0.) as usize;
        let y0 = y.max(0.) as usize;
        let x1 = ((x + w).min(self.w as f32)).max(0.) as usize;
        let y1 = ((y + h).min(self.h as f32)).max(0.) as usize;
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.cov[yy * self.w + xx] = 1.;
            }
        }
    }

    /// A filled rounded rectangle with an antialiased edge.
    fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let (cx, cy, hx, hy) = (x + w / 2., y + h / 2., w / 2., h / 2.);
        let x0 = (x - 1.).max(0.) as usize;
        let y0 = (y - 1.).max(0.) as usize;
        let x1 = ((x + w + 1.).min(self.w as f32)).max(0.) as usize;
        let y1 = ((y + h + 1.).min(self.h as f32)).max(0.) as usize;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let qx = (xx as f32 + 0.5 - cx).abs() - (hx - r);
                let qy = (yy as f32 + 0.5 - cy).abs() - (hy - r);
                let outside = (qx.max(0.).powi(2) + qy.max(0.).powi(2)).sqrt();
                let d = outside + qx.max(qy).min(0.) - r;
                let c = (0.5 - d).clamp(0., 1.);
                let cell = &mut self.cov[yy * self.w + xx];
                *cell = cell.max(c);
            }
        }
    }
}

/// Relative luminance (WCAG) of an sRGB colour.
fn luminance(c: [u8; 4]) -> f32 {
    let lin = |v: u8| {
        let v = f32::from(v) / 255.;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// A 5×7 bitmap glyph, one byte a row, high bit on the left. Lowercase is
/// drawn as capitals; anything unknown as `?`.
fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        ' ' => [0; 7],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '.' => [0, 0, 0, 0, 0, 0x0C, 0x0C],
        ',' => [0, 0, 0, 0, 0x0C, 0x04, 0x08],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        '-' => [0, 0, 0, 0x1F, 0, 0, 0],
        ':' => [0, 0x0C, 0x0C, 0, 0x0C, 0x0C, 0],
        ';' => [0, 0x0C, 0x0C, 0, 0x0C, 0x04, 0x08],
        '\'' => [0x04, 0x04, 0x08, 0, 0, 0, 0],
        '"' => [0x0A, 0x0A, 0, 0, 0, 0, 0],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '+' => [0, 0x04, 0x04, 0x1F, 0x04, 0x04, 0],
        '=' => [0, 0, 0x1F, 0, 0x1F, 0, 0],
        '#' => [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A],
        '_' => [0, 0, 0, 0, 0, 0, 0x1F],
        '>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        '<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        '%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        '&' => [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D],
        '@' => [0x0E, 0x11, 0x01, 0x0D, 0x15, 0x15, 0x0E],
        '*' => [0, 0x04, 0x15, 0x0E, 0x15, 0x04, 0],
        _ => [0x0E, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 4] = [220, 30, 30, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    fn white(w: u32, h: u32) -> Vec<u8> {
        png(&RgbaImage::from_pixel(w, h, Rgba(WHITE))).unwrap()
    }

    fn stroke(points: Vec<(f32, f32)>, width: f32) -> Stroke {
        Stroke {
            color: RED,
            width,
            points,
        }
    }

    #[test]
    fn a_stroke_is_burnt_into_the_picture() {
        let png = burn(&white(64, 40), &[stroke(vec![(10., 20.), (50., 20.)], 4.)]).unwrap();
        let out = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(out.get_pixel(30, 20).0, RED, "on the line");
        assert_eq!(out.get_pixel(30, 30).0, WHITE, "beside it, untouched");
        assert_eq!(out.dimensions(), (64, 40), "same size");
        assert!(burn(b"not a picture", &[]).is_err());
    }

    #[test]
    fn the_pen_edge_is_antialiased() {
        let out = render(
            &white(40, 40),
            &[EditOp::Pen(stroke(vec![(5., 20.), (35., 20.)], 3.))],
        )
        .unwrap();
        // The line spans y 18.5..21.5: row 18's centre (18.5) sits on the
        // edge, so it is half covered.
        let edge = out.get_pixel(20, 18).0;
        assert!(edge[1] > 30 && edge[1] < 255, "partly covered: {edge:?}");
    }

    #[test]
    fn the_highlighter_does_not_darken_where_it_crosses_itself() {
        let pts = vec![(5., 20.), (35., 20.), (35., 21.), (5., 21.)];
        let out = render(&white(40, 40), &[EditOp::Highlight(stroke(pts, 6.))]).unwrap();
        let once = render(
            &white(40, 40),
            &[EditOp::Highlight(stroke(vec![(5., 20.), (35., 20.)], 6.))],
        )
        .unwrap();
        assert_eq!(out.get_pixel(20, 20), once.get_pixel(20, 20));
        assert_ne!(out.get_pixel(20, 20).0, WHITE, "but it shows");
        assert_ne!(out.get_pixel(20, 20).0, RED, "and is see-through");
    }

    #[test]
    fn shapes_draw_outlines_only() {
        let (a, b) = ((10., 10.), (50., 40.));
        let rect = render(
            &white(60, 50),
            &[EditOp::Rect {
                a,
                b,
                color: RED,
                width: 2.,
            }],
        )
        .unwrap();
        assert_eq!(rect.get_pixel(30, 10).0, RED);
        assert_eq!(rect.get_pixel(30, 25).0, WHITE, "hollow");
        let ell = render(
            &white(60, 50),
            &[EditOp::Ellipse {
                a,
                b,
                color: RED,
                width: 2.,
            }],
        )
        .unwrap();
        assert_eq!(ell.get_pixel(30, 10).0, RED, "top of the ellipse");
        assert_eq!(ell.get_pixel(11, 11).0, WHITE, "its box's corner is not");
        let arrow = render(
            &white(60, 50),
            &[EditOp::Arrow {
                from: (5., 25.),
                to: (55., 25.),
                color: RED,
                width: 2.,
            }],
        )
        .unwrap();
        assert_eq!(arrow.get_pixel(30, 25).0, RED);
        assert_ne!(arrow.get_pixel(50, 22).0, WHITE, "the head");
    }

    #[test]
    fn redact_hides_the_detail() {
        let mut img = RgbaImage::from_pixel(32, 32, Rgba(WHITE));
        for y in 0..32 {
            for x in (0..32).step_by(2) {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        let out = render(
            &png(&img).unwrap(),
            &[EditOp::Redact {
                a: (0., 0.),
                b: (16., 16.),
            }],
        )
        .unwrap();
        assert_eq!(out.get_pixel(0, 0), out.get_pixel(1, 0), "stripes gone");
        assert_eq!(out.get_pixel(20, 20).0, [0, 0, 0, 255], "outside, kept");
    }

    #[test]
    fn crop_and_rotate_change_the_size() {
        let ops = [
            EditOp::Crop {
                a: (10., 5.),
                b: (50., 25.),
            },
            EditOp::Rotate,
        ];
        let out = render(&white(64, 40), &ops).unwrap();
        assert_eq!(out.dimensions(), (20, 40));
        assert_eq!(size_after(64, 40, &ops), (20, 40));
        // A crop of nothing is ignored.
        let none = [EditOp::Crop {
            a: (3., 3.),
            b: (3., 9.),
        }];
        assert_eq!(render(&white(8, 12), &none).unwrap().dimensions(), (8, 12));
    }

    #[test]
    fn text_is_drawn_on_a_tag_in_contrasting_ink() {
        let op = EditOp::Text {
            at: (2., 2.),
            text: "Hi".into(),
            color: RED,
            size: 14.,
        };
        let out = render(&white(80, 40), &[op]).unwrap();
        let (w, h) = text_size("Hi", 14.);
        assert!(w > 20. && h > 14.);
        // The tag's padding is the tag colour; the H's left stem is white ink.
        assert_eq!(out.get_pixel(5, 10).0, RED);
        assert_eq!(out.get_pixel(7, 8).0, WHITE);
        assert!(
            luminance([255, 220, 60, 255]) > 0.18,
            "yellow takes dark ink"
        );
    }
}
