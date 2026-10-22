//! Marks drawn over a design page with the design bar's Insert tools, as on
//! Claude's canvas: a box, a ring, a line, an arrow, a pencil stroke, a pen
//! path, a text label or a sticky note. They live in the page's own CSS
//! pixels (so they scroll and zoom with it), and go to Forge as a picture of
//! the page with them on it (`overlay_js`: Chromium draws them, so a note's
//! text is exactly as you see it) and a list of what each is.

use std::fmt::Write as _;

/// What a mark is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Shape {
    Rect,
    Oval,
    Line,
    Arrow,
    /// Freehand.
    Pencil,
    /// Straight segments, a click each.
    Pen,
    /// A text label.
    Text,
    /// A sticky note.
    Note,
}

impl Shape {
    /// Placed with a click, then typed.
    pub fn typed(self) -> bool {
        matches!(self, Shape::Text | Shape::Note)
    }
}

/// A mark on the page.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mark {
    pub shape: Shape,
    /// In the page's CSS pixels from its top left: two corners or ends for
    /// a dragged shape, the stroke for Pencil and Pen, the place for Text
    /// and a Note.
    pub points: Vec<(f64, f64)>,
    /// A label's or a note's words.
    pub text: String,
}

impl Mark {
    /// Its box: left, top, width, height.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let xs = self.points.iter().map(|p| p.0);
        let ys = self.points.iter().map(|p| p.1);
        let (x0, x1) = (
            xs.clone().fold(f64::MAX, f64::min),
            xs.fold(f64::MIN, f64::max),
        );
        let (y0, y1) = (
            ys.clone().fold(f64::MAX, f64::min),
            ys.fold(f64::MIN, f64::max),
        );
        if self.points.is_empty() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        // A label or note takes room to the right and below its place.
        let (w, h) = match self.shape {
            Shape::Note => (220.0, 24.0 + 20.0 * (self.text.len() as f64 / 28.0).ceil()),
            Shape::Text => (9.0 * self.text.chars().count() as f64, 24.0),
            _ => (x1 - x0, y1 - y0),
        };
        (x0, y0, w, h)
    }

    /// Big enough to keep: a stroke longer than a slip, words in a label.
    pub fn worth_keeping(&self) -> bool {
        if self.shape.typed() {
            return !self.text.trim().is_empty();
        }
        let (_, _, w, h) = self.bounds();
        w.max(h) >= 4.0
    }
}

/// The box round all the marks, with `margin` round it, kept at the page's
/// top left or beyond.
pub fn bounds(marks: &[Mark], margin: f64) -> Option<(f64, f64, f64, f64)> {
    let mut it = marks.iter().map(Mark::bounds);
    let first = it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0 + first.2, first.1 + first.3);
    for (x, y, w, h) in it {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    let (x0, y0) = ((x0 - margin).max(0.0), (y0 - margin).max(0.0));
    Some((x0, y0, x1 + margin - x0, y1 + margin - y0))
}

/// Text made safe inside HTML.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The SVG and HTML that draw the marks on the page, in `ink` (strokes and
/// labels) with notes on `paper`: one layer over the whole page, under
/// nothing, catching no clicks.
pub fn overlay_html(marks: &[Mark], ink: &str, paper: &str) -> String {
    let n = |v: f64| format!("{v:.1}");
    let mut svg = String::new();
    let mut notes = String::new();
    for m in marks {
        let p = &m.points;
        match m.shape {
            Shape::Rect if p.len() >= 2 => {
                let (x, y, w, h) = m.bounds();
                let _ = write!(
                    svg,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" rx="4"/>"#,
                    n(x),
                    n(y),
                    n(w),
                    n(h)
                );
            }
            Shape::Oval if p.len() >= 2 => {
                let (x, y, w, h) = m.bounds();
                let _ = write!(
                    svg,
                    r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"/>"#,
                    n(x + w / 2.0),
                    n(y + h / 2.0),
                    n(w / 2.0),
                    n(h / 2.0)
                );
            }
            Shape::Line | Shape::Arrow if p.len() >= 2 => {
                let ((x1, y1), (x2, y2)) = (p[0], p[p.len() - 1]);
                let _ = write!(
                    svg,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}"/>"#,
                    n(x1),
                    n(y1),
                    n(x2),
                    n(y2)
                );
                if m.shape == Shape::Arrow {
                    let [a, b] = arrow_head((x1, y1), (x2, y2), 14.0);
                    let _ = write!(
                        svg,
                        r#"<polyline points="{},{} {},{} {},{}"/>"#,
                        n(a.0),
                        n(a.1),
                        n(x2),
                        n(y2),
                        n(b.0),
                        n(b.1)
                    );
                }
            }
            Shape::Pencil | Shape::Pen if p.len() >= 2 => {
                let pts: Vec<String> = p
                    .iter()
                    .map(|(x, y)| format!("{},{}", n(*x), n(*y)))
                    .collect();
                let _ = write!(svg, r#"<polyline points="{}"/>"#, pts.join(" "));
            }
            Shape::Text if !p.is_empty() => {
                let _ = write!(
                    notes,
                    r#"<div style="position:absolute;left:{}px;top:{}px;font:600 16px/1.4 system-ui,sans-serif;color:{ink};white-space:pre">{}</div>"#,
                    n(p[0].0),
                    n(p[0].1),
                    escape(&m.text)
                );
            }
            Shape::Note if !p.is_empty() => {
                let _ = write!(
                    notes,
                    r#"<div style="position:absolute;left:{}px;top:{}px;width:200px;padding:8px 10px;border-radius:4px;background:{paper};color:{ink};border:1px solid {ink};font:14px/1.4 system-ui,sans-serif;white-space:pre-wrap">{}</div>"#,
                    n(p[0].0),
                    n(p[0].1),
                    escape(&m.text)
                );
            }
            _ => {}
        }
    }
    format!(
        r#"<div id="forge-marks" style="position:absolute;left:0;top:0;width:0;height:0;overflow:visible;pointer-events:none;z-index:2147483647"><svg style="position:absolute;left:0;top:0;overflow:visible;filter:drop-shadow(0 0 1.5px #fff)" width="1" height="1" fill="none" stroke="{ink}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">{svg}</svg>{notes}</div>"#
    )
}

/// The script that puts the marks on the page, for its picture.
pub fn overlay_js(marks: &[Mark], ink: &str, paper: &str) -> String {
    let html = serde_json::Value::from(overlay_html(marks, ink, paper));
    format!("(() => {{ document.getElementById('forge-marks')?.remove(); document.body.insertAdjacentHTML('beforeend', {html}); }})()")
}

/// The script that takes them off again.
pub const REMOVE_OVERLAY_JS: &str = "document.getElementById('forge-marks')?.remove()";

/// An arrow's two head points, `size` back from its tip at (x2, y2).
pub fn arrow_head(from: (f64, f64), to: (f64, f64), size: f64) -> [(f64, f64); 2] {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let spread: f64 = 0.5;
    let back = |s: f64| {
        (
            to.0 - size * (ux * spread.cos() - s * uy * spread.sin()),
            to.1 - size * (uy * spread.cos() + s * ux * spread.sin()),
        )
    };
    [back(1.0), back(-1.0)]
}

/// The marks as the message Forge gets with their picture: each, in order
/// (a note's and a label's words quoted), then what you said.
pub fn message(page: &str, marks: &[Mark], comment: &str) -> String {
    let mut out = format!("Marks on {page} (the picture shows them on the page):\n");
    for (i, m) in marks.iter().enumerate() {
        let what = match m.shape {
            Shape::Rect => "a box round a part".to_string(),
            Shape::Oval => "a ring round a part".to_string(),
            Shape::Line => "a line".to_string(),
            Shape::Arrow => "an arrow to a part".to_string(),
            Shape::Pencil => "a freehand mark".to_string(),
            Shape::Pen => "a drawn path".to_string(),
            Shape::Text => format!("a label: \"{}\"", m.text.trim()),
            Shape::Note => format!("a note: \"{}\"", m.text.trim()),
        };
        let _ = writeln!(out, "{}. {what}", i + 1);
    }
    let comment = comment.trim();
    if !comment.is_empty() {
        let _ = write!(out, "\n{comment}\n");
    }
    out.push_str("\nRevise the page for them, and keep the rest as it is.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(shape: Shape, points: &[(f64, f64)], text: &str) -> Mark {
        Mark {
            shape,
            points: points.to_vec(),
            text: text.into(),
        }
    }

    #[test]
    fn marks_have_boxes() {
        let r = mark(Shape::Rect, &[(50., 80.), (10., 20.)], "");
        assert_eq!(r.bounds(), (10., 20., 40., 60.));
        assert!(r.worth_keeping());
        assert!(!mark(Shape::Line, &[(5., 5.), (6., 6.)], "").worth_keeping());
        assert!(!mark(Shape::Note, &[(5., 5.)], "  ").worth_keeping());
        let all = [r, mark(Shape::Text, &[(100., 200.)], "Hi")];
        assert_eq!(bounds(&all, 8.), Some((2., 12., 124., 220.)));
        assert_eq!(bounds(&[], 8.), None);
    }

    #[test]
    fn the_overlay_draws_each_mark_and_keeps_words_safe() {
        let marks = [
            mark(Shape::Rect, &[(10., 20.), (50., 80.)], ""),
            mark(Shape::Oval, &[(0., 0.), (20., 10.)], ""),
            mark(Shape::Arrow, &[(0., 0.), (100., 0.)], ""),
            mark(Shape::Pencil, &[(1., 1.), (2., 3.), (4., 4.)], ""),
            mark(Shape::Note, &[(5., 6.)], "<b>Bigger</b> & bolder"),
        ];
        let html = overlay_html(&marks, "hsla(215, 36%, 40%, 1)", "#fff3c4");
        assert!(html.contains(r#"<rect x="10.0" y="20.0" width="40.0" height="60.0""#));
        assert!(html.contains(r#"<ellipse cx="10.0" cy="5.0" rx="10.0" ry="5.0"/>"#));
        assert!(html.contains("<polyline points=\"1.0,1.0 2.0,3.0 4.0,4.0\"/>"));
        assert!(html.contains("&lt;b&gt;Bigger&lt;/b&gt; &amp; bolder"));
        assert!(!html.contains("<b>"));
        // The arrow's head points back from its tip, either side of it.
        let [a, b] = arrow_head((0., 0.), (100., 0.), 14.);
        assert!(a.0 < 100. && b.0 < 100. && (a.1 + b.1).abs() < 1e-9 && a.1 != 0.);
        let js = overlay_js(&marks, "red", "white");
        assert!(js.starts_with("(() => {") && js.contains("insertAdjacentHTML"));
    }

    #[test]
    fn the_message_lists_the_marks() {
        let marks = [
            mark(Shape::Rect, &[(0., 0.), (9., 9.)], ""),
            mark(Shape::Note, &[(1., 1.)], " Make this bigger "),
        ];
        assert_eq!(
            message("deck.html", &marks, " Please. "),
            "Marks on deck.html (the picture shows them on the page):\n1. a box round a part\n2. a note: \"Make this bigger\"\n\nPlease.\n\nRevise the page for them, and keep the rest as it is."
        );
    }
}
