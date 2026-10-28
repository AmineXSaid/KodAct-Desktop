//! Drawing on a picture you attached (`ui/attachment_editor.rs`): pen
//! strokes in the picture's own pixels, burnt into it as a PNG, so the
//! model sees what you circled or crossed out.

/// One stroke: its colour (RGBA), its width in the picture's pixels, and
/// the points it passes through, in the picture's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: [u8; 4],
    pub width: f32,
    pub points: Vec<(f32, f32)>,
}

/// The picture with `strokes` drawn on it, as PNG bytes.
pub fn burn(picture: &[u8], strokes: &[Stroke]) -> Result<Vec<u8>, String> {
    let mut img = image::load_from_memory(picture)
        .map_err(|e| format!("the picture could not be read: {e}"))?
        .to_rgba8();
    for s in strokes {
        let r = (s.width / 2.).max(0.5);
        let mut last: Option<(f32, f32)> = None;
        for &p in &s.points {
            match last {
                // Round caps and joins: a dot every half pixel along the line.
                Some(a) => {
                    let len = ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
                    let steps = (len / 0.5).ceil().max(1.) as usize;
                    for i in 1..=steps {
                        let k = i as f32 / steps as f32;
                        dot(
                            &mut img,
                            a.0 + (p.0 - a.0) * k,
                            a.1 + (p.1 - a.1) * k,
                            r,
                            s.color,
                        );
                    }
                }
                None => dot(&mut img, p.0, p.1, r, s.color),
            }
            last = Some(p);
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| format!("the drawing could not be saved: {e}"))?;
    Ok(out.into_inner())
}

/// A filled round dot, blended over the picture.
fn dot(img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, c: [u8; 4]) {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let (x0, x1) = ((cx - r).floor() as i64, (cx + r).ceil() as i64);
    let (y0, y1) = ((cy - r).floor() as i64, (cy + r).ceil() as i64);
    let a = f32::from(c[3]) / 255.;
    for y in y0.max(0)..=y1.min(h - 1) {
        for x in x0.max(0)..=x1.min(w - 1) {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let px = img.get_pixel_mut(x as u32, y as u32);
            for i in 0..3 {
                px.0[i] = (f32::from(c[i]) * a + f32::from(px.0[i]) * (1. - a)).round() as u8;
            }
            px.0[3] = px.0[3].max(c[3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_stroke_is_burnt_into_the_picture() {
        let red = [220, 30, 30, 255];
        let s = Stroke {
            color: red,
            width: 4.,
            points: vec![(10., 20.), (50., 20.)],
        };
        let png = burn(&white(64, 40), &[s]).unwrap();
        let out = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(out.get_pixel(30, 20).0, red, "on the line");
        assert_eq!(
            out.get_pixel(30, 30).0,
            [255, 255, 255, 255],
            "beside it, untouched"
        );
        assert_eq!(out.dimensions(), (64, 40), "same size");
        assert!(burn(b"not a picture", &[]).is_err());
    }
}
