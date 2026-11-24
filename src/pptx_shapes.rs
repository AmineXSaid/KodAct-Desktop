//! A slide's shapes, for editing them on the slide in the side panel: text
//! boxes, shapes, pictures and tables (their place, size, fill and words),
//! and the changes the slide's toolbar makes to them: move, resize, fill,
//! duplicate, delete, bring to front or send to back, and add a shape or a
//! text box. (Its words are edited in the deck's Edit text view.)
//!
//! A placeholder (a title, a body) often has no place of its own: it sits
//! where its slide layout, or the master, puts it. Its place is read from
//! there, and moving it gives it one of its own.
//!
//! Positions and sizes are in EMU (914400 to the inch), as the file has them.

use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::Reader;

/// What a shape is, for the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A shape or text box (it may hold words).
    Shape,
    Picture,
    /// A table, chart or other frame.
    Frame,
    Group,
    Line,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// Its id on the slide (`p:cNvPr id`).
    pub id: u32,
    pub name: String,
    pub kind: Kind,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// Its words, a line per paragraph.
    pub text: String,
    /// Its solid fill (`RRGGBB`), if it has one of its own.
    pub fill: Option<String>,
    /// A placeholder, whose place comes from the layout.
    pub placeholder: bool,
}

/// A change to a shape on a slide.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeOp {
    /// Put it at (x, y) with size (w, h).
    Place {
        id: u32,
        x: i64,
        y: i64,
        w: i64,
        h: i64,
    },
    Fill {
        id: u32,
        rgb: String,
    },
    Duplicate {
        id: u32,
    },
    Delete {
        id: u32,
    },
    Front {
        id: u32,
    },
    Back {
        id: u32,
    },
    /// A new rectangle, ellipse or text box in the middle of the slide.
    Add(New),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum New {
    Rectangle,
    Ellipse,
    TextBox,
}

type Zip<'a> = zip::ZipArchive<Cursor<&'a [u8]>>;

fn open(bytes: &[u8]) -> Result<Zip<'_>, String> {
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not a deck KodAct can read: {e}"))
}

fn part(zip: &mut Zip, path: &str) -> Option<String> {
    let mut f = zip.by_name(path).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn attr(tag: &str, key: &str) -> Option<String> {
    let at = tag.find(&format!(" {key}=\""))? + key.len() + 3;
    Some(tag[at..].split('"').next()?.to_string())
}

/// The first tag `<name ...>` in `xml`.
fn first_tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let i = xml
        .find(&format!("<{name} "))
        .or_else(|| xml.find(&format!("<{name}>")))
        .or_else(|| xml.find(&format!("<{name}/>")))?;
    let end = xml[i..].find('>')? + i + 1;
    Some(&xml[i..end])
}

/// The slide size, in EMU.
pub fn slide_size(bytes: &[u8]) -> (i64, i64) {
    let size = open(bytes)
        .ok()
        .and_then(|mut z| part(&mut z, "ppt/presentation.xml"))
        .and_then(|p| {
            let tag = first_tag(&p, "p:sldSz")?;
            Some((
                attr(tag, "cx")?.parse().ok()?,
                attr(tag, "cy")?.parse().ok()?,
            ))
        });
    size.unwrap_or((12_192_000, 6_858_000))
}

/// The spans of `xml`'s shape tree's own children: (start, end, element).
fn children(xml: &str) -> Vec<(usize, usize, String)> {
    let mut reader = Reader::from_str(xml);
    let mut out = vec![];
    let mut depth = 0usize;
    let mut tree = None::<usize>;
    let mut start = 0usize;
    loop {
        let before = reader.buffer_position() as usize;
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                depth += 1;
                if name == "p:spTree" && tree.is_none() {
                    tree = Some(depth);
                } else if tree.is_some_and(|d| depth == d + 1) {
                    start = before;
                }
            }
            Ok(Event::Empty(e)) => {
                if tree.is_some_and(|d| depth == d) {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    out.push((before, reader.buffer_position() as usize, name));
                }
            }
            Ok(Event::End(e)) => {
                if tree.is_some_and(|d| depth == d + 1) {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    out.push((start, reader.buffer_position() as usize, name));
                }
                if tree == Some(depth) {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out.retain(|(_, _, n)| {
        matches!(
            n.as_str(),
            "p:sp" | "p:pic" | "p:graphicFrame" | "p:grpSp" | "p:cxnSp"
        )
    });
    out
}

/// A shape's place: its own `a:off`/`a:ext` (or `p:xfrm`'s for a frame).
fn own_place(el: &str) -> Option<(i64, i64, i64, i64)> {
    let off = first_tag(el, "a:off")?;
    let ext = first_tag(el, "a:ext")?;
    Some((
        attr(off, "x")?.parse().ok()?,
        attr(off, "y")?.parse().ok()?,
        attr(ext, "cx")?.parse().ok()?,
        attr(ext, "cy")?.parse().ok()?,
    ))
}

/// The placeholder a shape is: (type, idx).
fn placeholder(el: &str) -> Option<(Option<String>, Option<String>)> {
    let ph = first_tag(el, "p:ph")?;
    Some((attr(ph, "type"), attr(ph, "idx")))
}

/// A placeholder's place in a layout or master: matched by idx, then by
/// type (a body by any body).
fn place_in(xml: &str, ph: &(Option<String>, Option<String>)) -> Option<(i64, i64, i64, i64)> {
    let kind = |t: &Option<String>| match t.as_deref() {
        None | Some("obj") | Some("body") | Some("subTitle") => "body".to_string(),
        Some("ctrTitle") | Some("title") => "title".to_string(),
        Some(other) => other.to_string(),
    };
    let els: Vec<String> = children(xml)
        .into_iter()
        .map(|(s, e, _)| xml[s..e].to_string())
        .collect();
    let by_idx = ph.1.as_ref().and_then(|idx| {
        els.iter()
            .find(|el| placeholder(el).is_some_and(|p| p.1.as_ref() == Some(idx)))
            .and_then(|el| own_place(el))
    });
    by_idx.or_else(|| {
        els.iter()
            .find(|el| placeholder(el).is_some_and(|p| kind(&p.0) == kind(&ph.0)))
            .and_then(|el| own_place(el))
    })
}

fn rel_target(rels: &str, kind: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = rels[from..].find("<Relationship ") {
        let s = from + i;
        let e = rels[s..].find('>')? + s;
        let tag = &rels[s..e];
        if attr(tag, "Type").is_some_and(|t| t.ends_with(kind)) {
            return attr(tag, "Target");
        }
        from = e;
    }
    None
}

fn resolve(from: &str, target: &str) -> String {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn rels_of(p: &str) -> String {
    let (dir, file) = p.rsplit_once('/').unwrap_or(("", p));
    format!("{dir}/_rels/{file}.rels")
}

/// A shape's words, a line per paragraph.
fn words(el: &str) -> String {
    let mut reader = Reader::from_str(el);
    let mut lines: Vec<String> = vec![];
    let mut in_t = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"a:p" => lines.push(String::new()),
            Ok(Event::Empty(e)) if e.name().as_ref() == b"a:p" => lines.push(String::new()),
            Ok(Event::Start(e)) if e.name().as_ref() == b"a:t" => in_t = true,
            Ok(Event::End(e)) if e.name().as_ref() == b"a:t" => in_t = false,
            Ok(Event::Text(t)) if in_t => {
                if let Some(l) = lines.last_mut() {
                    l.push_str(&unescape(&String::from_utf8_lossy(t.as_ref())));
                }
            }
            Ok(Event::GeneralRef(r)) if in_t => {
                if let Some(l) = lines.last_mut() {
                    l.push_str(&unescape(&format!(
                        "&{};",
                        String::from_utf8_lossy(r.as_ref())
                    )));
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    lines.join("\n").trim_end_matches('\n').to_string()
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The fill a shape sets itself, in its shape properties.
fn own_fill(el: &str) -> Option<String> {
    let sp = el.split("<p:spPr").nth(1)?.split("</p:spPr>").next()?;
    let sp = sp.split("<a:ln").next()?;
    let solid = sp.split("<a:solidFill>").nth(1)?;
    attr(first_tag(solid, "a:srgbClr")?, "val")
}

/// The slide part of slide `slide` (in the deck's order), and its XML.
fn slide_xml(zip: &mut Zip, slide: usize) -> Result<(String, String), String> {
    let order = crate::pptx_edit::order_in(zip)?;
    let path = order.get(slide).cloned().ok_or("no such slide")?;
    let xml = part(zip, &path).ok_or_else(|| format!("no {path}"))?;
    Ok((path, xml))
}

/// The shapes of every slide, in the deck's order, back to front.
pub fn read(bytes: &[u8]) -> Result<Vec<Vec<Shape>>, String> {
    let mut zip = open(bytes)?;
    let order = crate::pptx_edit::order_in(&mut zip)?;
    let mut out = vec![];
    for path in order {
        let xml = part(&mut zip, &path).unwrap_or_default();
        // Its layout and master, for placeholders with no place of their own.
        let layout = part(&mut zip, &rels_of(&path))
            .and_then(|r| rel_target(&r, "/slideLayout"))
            .map(|t| resolve(&path, &t));
        let layout_xml = layout.as_ref().and_then(|l| part(&mut zip, l));
        let master_xml = layout
            .as_ref()
            .and_then(|l| {
                part(&mut zip, &rels_of(l))
                    .and_then(|r| rel_target(&r, "/slideMaster"))
                    .map(|t| resolve(l, &t))
            })
            .and_then(|m| part(&mut zip, &m));
        out.push(shapes_in(
            &xml,
            layout_xml.as_deref(),
            master_xml.as_deref(),
        ));
    }
    Ok(out)
}

fn shapes_in(xml: &str, layout: Option<&str>, master: Option<&str>) -> Vec<Shape> {
    children(xml)
        .into_iter()
        .filter_map(|(s, e, name)| {
            let el = &xml[s..e];
            let nv = first_tag(el, "p:cNvPr")?;
            let ph = placeholder(el);
            let place = own_place(el).or_else(|| {
                let ph = ph.as_ref()?;
                layout
                    .and_then(|l| place_in(l, ph))
                    .or_else(|| master.and_then(|m| place_in(m, ph)))
            })?;
            Some(Shape {
                id: attr(nv, "id")?.parse().ok()?,
                name: attr(nv, "name").unwrap_or_default(),
                kind: match name.as_str() {
                    "p:pic" => Kind::Picture,
                    "p:graphicFrame" => Kind::Frame,
                    "p:grpSp" => Kind::Group,
                    "p:cxnSp" => Kind::Line,
                    _ => Kind::Shape,
                },
                x: place.0,
                y: place.1,
                w: place.2,
                h: place.3,
                text: words(el),
                fill: own_fill(el),
                placeholder: ph.is_some(),
            })
        })
        .collect()
}

/// `bytes` with `op` made on slide `slide`.
pub fn edit(bytes: &[u8], slide: usize, op: &ShapeOp) -> Result<Vec<u8>, String> {
    let mut zip = open(bytes)?;
    let (path, xml) = slide_xml(&mut zip, slide)?;
    let shapes = read(bytes)?.into_iter().nth(slide).unwrap_or_default();
    let new = edit_xml(&xml, &shapes, op, slide_size(bytes))?;
    crate::pptx_edit::replace_part(bytes, &path, new)
}

/// The span of the shape with `id`.
fn span_of(xml: &str, id: u32) -> Option<(usize, usize)> {
    children(xml).into_iter().find_map(|(s, e, _)| {
        let nv = first_tag(&xml[s..e], "p:cNvPr")?;
        (attr(nv, "id")?.parse::<u32>().ok()? == id).then_some((s, e))
    })
}

/// `el` placed at (x, y, w, h): its own offset and extent set, or added.
fn set_place(el: &str, (x, y, w, h): (i64, i64, i64, i64)) -> String {
    let xfrm =
        format!("<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm>");
    if let (Some(off), Some(ext)) = (first_tag(el, "a:off"), first_tag(el, "a:ext")) {
        let off_new = format!("<a:off x=\"{x}\" y=\"{y}\"/>");
        let ext_new = format!("<a:ext cx=\"{w}\" cy=\"{h}\"/>");
        return el.replacen(off, &off_new, 1).replacen(ext, &ext_new, 1);
    }
    if let Some(i) = el.find("<p:spPr/>") {
        return format!("{}<p:spPr>{xfrm}</p:spPr>{}", &el[..i], &el[i + 9..]);
    }
    if let Some(i) = el.find("<p:spPr") {
        if let Some(j) = el[i..].find('>') {
            let at = i + j + 1;
            return format!("{}{xfrm}{}", &el[..at], &el[at..]);
        }
    }
    el.to_string()
}

/// `el` with a solid fill of `rgb`, in place of the fill it had.
fn set_fill(el: &str, rgb: &str) -> String {
    let fill = format!("<a:solidFill><a:srgbClr val=\"{rgb}\"/></a:solidFill>");
    let el = if el.contains("<p:spPr/>") {
        el.replacen("<p:spPr/>", "<p:spPr></p:spPr>", 1)
    } else {
        el.to_string()
    };
    let Some(sp_start) = el.find("<p:spPr") else {
        return el;
    };
    let Some(sp_end) = el[sp_start..].find("</p:spPr>").map(|i| i + sp_start) else {
        return el;
    };
    let mut sp = el[sp_start..sp_end].to_string();
    // Its own fill out (before its outline, which has its own).
    let ln = sp.find("<a:ln").unwrap_or(sp.len());
    let (head, tail) = sp.split_at(ln);
    let mut head = head.to_string();
    for (open, close) in [
        ("<a:solidFill>", "</a:solidFill>"),
        ("<a:gradFill", "</a:gradFill>"),
        ("<a:pattFill", "</a:pattFill>"),
    ] {
        if let Some(a) = head.find(open) {
            if let Some(b) = head[a..].find(close) {
                head.replace_range(a..a + b + close.len(), "");
            }
        }
    }
    head = head.replace("<a:noFill/>", "");
    // After the geometry (or the place), where the schema has fills.
    let at = ["</a:prstGeom>", "</a:custGeom>", "</a:xfrm>"]
        .iter()
        .find_map(|m| head.find(m).map(|i| i + m.len()))
        .or_else(|| {
            head.find("/>")
                .filter(|_| head.contains("<a:prstGeom"))
                .map(|i| i + 2)
        })
        .unwrap_or_else(|| head.find('>').map_or(head.len(), |i| i + 1));
    head.insert_str(at, &fill);
    sp = format!("{head}{tail}");
    format!("{}{sp}{}", &el[..sp_start], &el[sp_end..])
}

fn next_id(xml: &str) -> u32 {
    let mut max = 1;
    let mut rest = xml;
    while let Some(i) = rest.find("<p:cNvPr ") {
        rest = &rest[i + 9..];
        if let Some(v) = attr(&format!(" {}", rest.split('>').next().unwrap_or("")), "id")
            .and_then(|v| v.parse::<u32>().ok())
        {
            max = max.max(v);
        }
    }
    max + 1
}

fn edit_xml(
    xml: &str,
    shapes: &[Shape],
    op: &ShapeOp,
    (sw, sh): (i64, i64),
) -> Result<String, String> {
    let find = |id: u32| -> Result<((usize, usize), &Shape), String> {
        let span = span_of(xml, id).ok_or("no such shape")?;
        let shape = shapes.iter().find(|s| s.id == id).ok_or("no such shape")?;
        Ok((span, shape))
    };
    let tree_end = xml.find("</p:spTree>").ok_or("no shape tree")?;
    Ok(match op {
        ShapeOp::Place { id, x, y, w, h } => {
            let ((s, e), _) = find(*id)?;
            let el = set_place(&xml[s..e], (*x, *y, (*w).max(1), (*h).max(1)));
            format!("{}{el}{}", &xml[..s], &xml[e..])
        }
        ShapeOp::Fill { id, rgb } => {
            let ((s, e), _) = find(*id)?;
            format!("{}{}{}", &xml[..s], set_fill(&xml[s..e], rgb), &xml[e..])
        }
        ShapeOp::Delete { id } => {
            let ((s, e), _) = find(*id)?;
            format!("{}{}", &xml[..s], &xml[e..])
        }
        ShapeOp::Duplicate { id } => {
            let ((s, e), shape) = find(*id)?;
            let new_id = next_id(xml);
            let el = &xml[s..e];
            let nv = first_tag(el, "p:cNvPr").ok_or("no shape id")?;
            let nv_new = nv
                .replacen(&format!("id=\"{id}\""), &format!("id=\"{new_id}\""), 1)
                .replacen(
                    &format!("name=\"{}\"", shape.name),
                    &format!("name=\"{} copy\"", shape.name),
                    1,
                );
            // A copy is its own shape, a little down and to the right.
            let mut copy = el.replacen(nv, &nv_new, 1);
            if shape.placeholder {
                if let Some(ph) = first_tag(&copy, "p:ph") {
                    copy = copy.replacen(ph, "", 1);
                }
            }
            let step = 228_600;
            copy = set_place(&copy, (shape.x + step, shape.y + step, shape.w, shape.h));
            format!("{}{copy}{}", &xml[..tree_end], &xml[tree_end..])
        }
        ShapeOp::Front { id } => {
            let ((s, e), _) = find(*id)?;
            let el = xml[s..e].to_string();
            let rest = format!("{}{}", &xml[..s], &xml[e..]);
            let end = rest.find("</p:spTree>").ok_or("no shape tree")?;
            format!("{}{el}{}", &rest[..end], &rest[end..])
        }
        ShapeOp::Back { id } => {
            let ((s, e), _) = find(*id)?;
            let el = xml[s..e].to_string();
            let rest = format!("{}{}", &xml[..s], &xml[e..]);
            // After the tree's own properties, before every shape.
            let at = rest
                .find("</p:grpSpPr>")
                .map(|i| i + 12)
                .or_else(|| rest.find("<p:grpSpPr/>").map(|i| i + 12))
                .ok_or("no shape tree")?;
            format!("{}{el}{}", &rest[..at], &rest[at..])
        }
        ShapeOp::Add(kind) => {
            let id = next_id(xml);
            let (w, h) = (sw / 4, sh / 4);
            let (x, y) = ((sw - w) / 2, (sh - h) / 2);
            let el = match kind {
                New::TextBox => format!(
                    "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text Box {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"square\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1800\"/><a:t>Text</a:t></a:r></a:p></p:txBody></p:sp>",
                    h / 2
                ),
                New::Rectangle | New::Ellipse => {
                    let (geom, name) = if *kind == New::Ellipse { ("ellipse", "Ellipse") } else { ("rect", "Rectangle") };
                    format!(
                        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name} {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"{geom}\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"7A1F3D\"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr anchor=\"ctr\"/><a:lstStyle/><a:p><a:pPr algn=\"ctr\"/><a:endParaRPr lang=\"en-US\"/></a:p></p:txBody></p:sp>"
                    )
                }
            };
            format!("{}{el}{}", &xml[..tree_end], &xml[tree_end..])
        }
    })
}

/// The id of the shape `op` leaves selected (a new one for Add/Duplicate).
pub fn selected_after(
    bytes_before: &[u8],
    bytes_after: &[u8],
    slide: usize,
    op: &ShapeOp,
) -> Option<u32> {
    match op {
        ShapeOp::Delete { .. } => None,
        ShapeOp::Add(_) | ShapeOp::Duplicate { .. } => {
            let before: Vec<u32> = read(bytes_before)
                .ok()?
                .get(slide)?
                .iter()
                .map(|s| s.id)
                .collect();
            read(bytes_after)
                .ok()?
                .get(slide)?
                .iter()
                .map(|s| s.id)
                .find(|id| !before.contains(id))
        }
        ShapeOp::Place { id, .. }
        | ShapeOp::Fill { id, .. }
        | ShapeOp::Front { id }
        | ShapeOp::Back { id } => Some(*id),
    }
}

/// The palette the fill menu offers: KodAct's own hues and neutrals.
pub const FILLS: &[(&str, &str)] = &[
    ("Burgundy", "7A1F3D"),
    ("Rose", "E0889F"),
    ("Ink", "1A1816"),
    ("Graphite", "5A5651"),
    ("Paper", "FFFFFF"),
    ("Mist", "EDEAE6"),
    ("Purple", "6A4FB4"),
    ("Blue", "2F68B4"),
    ("Lime", "4A7200"),
    ("Orange", "AB6100"),
    ("Marigold", "E9BE74"),
    ("Red", "B3261E"),
];

#[cfg(test)]
mod tests {
    use super::*;

    const DECK: &[u8] = include_bytes!("../assets/docs/samples/q3-budget.pptx");

    #[test]
    fn placeholders_take_their_place_from_the_layout() {
        let slides = read(DECK).unwrap();
        assert_eq!(slides.len(), 4);
        let title = &slides[1][0];
        assert!(title.placeholder);
        assert_eq!(title.text, "The forecast");
        assert!(title.w > 0 && title.h > 0, "{title:?}");
        let (sw, sh) = slide_size(DECK);
        assert!(title.x + title.w <= sw && title.y + title.h <= sh);
    }

    #[test]
    fn a_shape_moves_fills_and_takes_new_words() {
        let shapes = read(DECK).unwrap();
        let id = shapes[1][0].id;
        let moved = edit(
            DECK,
            1,
            &ShapeOp::Place {
                id,
                x: 100,
                y: 200,
                w: 3000,
                h: 4000,
            },
        )
        .unwrap();
        let s = &read(&moved).unwrap()[1][0];
        assert_eq!((s.x, s.y, s.w, s.h), (100, 200, 3000, 4000));
        let filled = edit(
            &moved,
            1,
            &ShapeOp::Fill {
                id,
                rgb: "7A1F3D".into(),
            },
        )
        .unwrap();
        assert_eq!(read(&filled).unwrap()[1][0].fill.as_deref(), Some("7A1F3D"));
    }

    #[test]
    fn shapes_are_added_duplicated_reordered_and_deleted() {
        let n = read(DECK).unwrap()[1].len();
        let added = edit(DECK, 1, &ShapeOp::Add(New::Ellipse)).unwrap();
        let after = read(&added).unwrap();
        assert_eq!(after[1].len(), n + 1);
        let new = selected_after(DECK, &added, 1, &ShapeOp::Add(New::Ellipse)).unwrap();
        let back = edit(&added, 1, &ShapeOp::Back { id: new }).unwrap();
        assert_eq!(read(&back).unwrap()[1][0].id, new);
        let dup = edit(&back, 1, &ShapeOp::Duplicate { id: new }).unwrap();
        assert_eq!(read(&dup).unwrap()[1].len(), n + 2);
        let gone = edit(&dup, 1, &ShapeOp::Delete { id: new }).unwrap();
        assert_eq!(read(&gone).unwrap()[1].len(), n + 1);
    }
}
