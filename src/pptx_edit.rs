//! A deck's slides and text boxes changed in place, for the side panel's
//! PowerPoint toolbar (Edit text): a paragraph made bold, italic, a bullet
//! or a size, and slides added, duplicated, deleted and moved. Like
//! `office_edit.rs`, only the parts that change are rewritten; every other
//! entry of the zip is copied as it is.
//!
//! Slides are counted in the deck's own order (`presentation.xml`'s slide
//! list), which a move changes, not by their file numbers.

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read, Write as _};

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};

const PRESENTATION: &str = "ppt/presentation.xml";
const PRESENTATION_RELS: &str = "ppt/_rels/presentation.xml.rels";
const TYPES: &str = "[Content_Types].xml";
const SLIDE_TYPE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const SLIDE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";

/// What a toolbar button does to a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Bold on, or off when the paragraph already is.
    Bold,
    Italic,
    /// A bullet, or none when it has one.
    Bullets,
    /// The text's size in points (a title, a heading, body text).
    Size(u32),
}

/// What changes a deck's slides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideOp {
    /// A new slide after this one, in its layout with its words cleared.
    New(usize),
    Duplicate(usize),
    Delete(usize),
    /// Move a slide by one place, up (-1) or down (+1).
    Move(usize, isize),
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

/// The zip with parts replaced, some dropped and some added; every other
/// entry copied as it was.
fn rebuild(
    bytes: &[u8],
    replace: &BTreeMap<String, String>,
    drop: &HashSet<String>,
    add: &[(String, String)],
) -> Result<Vec<u8>, String> {
    let mut archive = open(bytes)?;
    let mut out = Vec::with_capacity(bytes.len() + 4096);
    {
        let mut w = zip::ZipWriter::new(Cursor::new(&mut out));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for i in 0..archive.len() {
            let name = archive
                .by_index(i)
                .map_err(|e| e.to_string())?
                .name()
                .to_string();
            if drop.contains(&name) {
                continue;
            }
            match replace.get(&name) {
                Some(xml) => {
                    w.start_file(&name, opts).map_err(|e| e.to_string())?;
                    w.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
                }
                None => {
                    let f = archive.by_index(i).map_err(|e| e.to_string())?;
                    w.raw_copy_file(f).map_err(|e| e.to_string())?;
                }
            }
        }
        for (name, xml) in add {
            w.start_file(name, opts).map_err(|e| e.to_string())?;
            w.write_all(xml.as_bytes()).map_err(|e| e.to_string())?;
        }
        w.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// `bytes` with one part replaced.
pub(crate) fn replace_part(bytes: &[u8], path: &str, xml: String) -> Result<Vec<u8>, String> {
    rebuild(
        bytes,
        &BTreeMap::from([(path.to_string(), xml)]),
        &HashSet::new(),
        &[],
    )
}

/// An attribute's value in a tag's text (`<p:sldId id="256" r:id="rId7"/>`).
fn attr_in(tag: &str, key: &str) -> Option<String> {
    let at = tag.find(&format!(" {key}=\""))? + key.len() + 3;
    Some(tag[at..].split('"').next()?.to_string())
}

/// Every `<name .../>` or `<name ...>` tag in `xml`, with where it starts
/// and ends.
fn tags<'a>(xml: &'a str, name: &str) -> Vec<(usize, usize, &'a str)> {
    let open = format!("<{name} ");
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = xml[from..].find(&open) {
        let start = from + i;
        let Some(len) = xml[start..].find('>') else {
            break;
        };
        let end = start + len + 1;
        out.push((start, end, &xml[start..end]));
        from = end;
    }
    out
}

/// The slides' entries in the slide list: (relationship id, slide id).
fn slide_ids(presentation: &str) -> Vec<(String, u32, usize, usize)> {
    tags(presentation, "p:sldId")
        .into_iter()
        .filter_map(|(s, e, tag)| {
            Some((
                attr_in(tag, "r:id")?,
                attr_in(tag, "id")?.parse().ok()?,
                s,
                e,
            ))
        })
        .collect()
}

/// Relationship id -> target, from a relationships part.
fn rel_targets(rels: &str) -> BTreeMap<String, String> {
    tags(rels, "Relationship")
        .into_iter()
        .filter_map(|(_, _, tag)| Some((attr_in(tag, "Id")?, attr_in(tag, "Target")?)))
        .collect()
}

/// A relationship's target from `ppt/`, as a zip path.
fn ppt_path(target: &str) -> String {
    match target.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => format!("ppt/{target}"),
    }
}

/// The deck's slide parts in its own order.
pub(crate) fn order_in(zip: &mut Zip) -> Result<Vec<String>, String> {
    let pres = part(zip, PRESENTATION).ok_or("no presentation.xml")?;
    let rels = rel_targets(&part(zip, PRESENTATION_RELS).unwrap_or_default());
    Ok(slide_ids(&pres)
        .into_iter()
        .filter_map(|(rid, ..)| rels.get(&rid).map(|t| ppt_path(t)))
        .collect())
}

fn rels_of(slide: &str) -> String {
    let (dir, file) = slide.rsplit_once('/').unwrap_or(("", slide));
    format!("{dir}/_rels/{file}.rels")
}

/// `bytes` with paragraph `para` of slide `slide` formatted.
pub fn format_para(bytes: &[u8], slide: usize, para: usize, f: Format) -> Result<Vec<u8>, String> {
    let mut zip = open(bytes)?;
    let path = order_in(&mut zip)?
        .get(slide)
        .cloned()
        .ok_or("no such slide")?;
    let xml = part(&mut zip, &path).ok_or_else(|| format!("no {path}"))?;
    let new = format_xml(&xml, para, f)?;
    rebuild(bytes, &BTreeMap::from([(path, new)]), &HashSet::new(), &[])
}

/// How the paragraph is now: bold, italic, a bullet.
fn current(xml: &str, para: usize) -> (bool, bool, bool) {
    let mut reader = Reader::from_str(xml);
    let (mut n, mut inside) = (0usize, false);
    let (mut bold, mut italic, mut bullet, mut first) = (false, false, false, true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_vec();
                if name == b"a:p" {
                    inside = n == para;
                    n += 1;
                } else if inside && name == b"a:rPr" && first {
                    first = false;
                    let get = |k: &[u8]| {
                        e.attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == k)
                            .map(|a| a.value.as_ref() == b"1")
                            .unwrap_or(false)
                    };
                    bold = get(b"b");
                    italic = get(b"i");
                } else if inside && (name == b"a:buChar" || name == b"a:buAutoNum") {
                    bullet = true;
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"a:p" => inside = false,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    (bold, italic, bullet)
}

/// `e` with `key` set to `val` (or removed), the rest kept.
fn with_attr(e: &BytesStart, key: &str, val: Option<&str>) -> BytesStart<'static> {
    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
    let mut out = BytesStart::new(name);
    for a in e.attributes().flatten() {
        if a.key.as_ref() != key.as_bytes() {
            out.push_attribute((
                String::from_utf8_lossy(a.key.as_ref()).as_ref(),
                String::from_utf8_lossy(&a.value).as_ref(),
            ));
        }
    }
    if let Some(v) = val {
        out.push_attribute((key, v));
    }
    out
}

fn format_xml(xml: &str, para: usize, f: Format) -> Result<String, String> {
    let (bold, italic, bullet) = current(xml, para);
    // The run property this sets, and to what.
    let run_attr: Option<(&str, String)> = match f {
        Format::Bold => Some(("b", if bold { "0" } else { "1" }.into())),
        Format::Italic => Some(("i", if italic { "0" } else { "1" }.into())),
        Format::Size(pt) => Some(("sz", (pt * 100).to_string())),
        Format::Bullets => None,
    };
    let bullets_on = !bullet;
    let set_run = |e: &BytesStart| -> BytesStart<'static> {
        let (k, v) = run_attr.as_ref().expect("a run format");
        with_attr(e, k, Some(v))
    };
    let set_ppr = |e: &BytesStart| -> BytesStart<'static> {
        let (m, i) = if bullets_on {
            ("342900", "-342900")
        } else {
            ("0", "0")
        };
        with_attr(&with_attr(e, "marL", Some(m)), "indent", Some(i))
    };
    let bullet_el = || {
        if bullets_on {
            let mut b = BytesStart::new("a:buChar");
            b.push_attribute(("char", "\u{2022}"));
            b
        } else {
            BytesStart::new("a:buNone")
        }
    };

    let mut reader = Reader::from_str(xml);
    let mut w = Writer::new(Vec::new());
    let (mut n, mut inside, mut pending_run, mut need_ppr, mut in_ppr) =
        (0usize, false, false, false, false);
    let err = |e: std::io::Error| e.to_string();
    loop {
        let ev = reader.read_event().map_err(|e| e.to_string())?;
        // A paragraph's first child, when it is not its properties: they go
        // in first, with the bullet.
        if need_ppr {
            let is_ppr =
                matches!(&ev, Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"a:pPr");
            if !is_ppr {
                need_ppr = false;
                let ppr = set_ppr(&BytesStart::new("a:pPr"));
                w.write_event(Event::Start(ppr)).map_err(err)?;
                w.write_event(Event::Empty(bullet_el())).map_err(err)?;
                w.write_event(Event::End(BytesEnd::new("a:pPr")))
                    .map_err(err)?;
            }
        }
        match ev {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == b"a:p" => {
                inside = n == para;
                n += 1;
                w.write_event(Event::Start(e)).map_err(err)?;
                if inside && f == Format::Bullets {
                    need_ppr = true;
                }
            }
            Event::Empty(e) if e.name().as_ref() == b"a:p" => {
                n += 1;
                w.write_event(Event::Empty(e)).map_err(err)?;
            }
            Event::End(e) if e.name().as_ref() == b"a:p" => {
                inside = false;
                w.write_event(Event::End(e)).map_err(err)?;
            }
            // Bullets: the paragraph's properties.
            Event::Empty(e) if inside && need_ppr && e.name().as_ref() == b"a:pPr" => {
                need_ppr = false;
                w.write_event(Event::Start(set_ppr(&e))).map_err(err)?;
                w.write_event(Event::Empty(bullet_el())).map_err(err)?;
                w.write_event(Event::End(BytesEnd::new("a:pPr")))
                    .map_err(err)?;
            }
            Event::Start(e) if inside && need_ppr && e.name().as_ref() == b"a:pPr" => {
                need_ppr = false;
                in_ppr = true;
                w.write_event(Event::Start(set_ppr(&e))).map_err(err)?;
            }
            Event::Empty(e)
                if in_ppr
                    && matches!(
                        e.name().as_ref(),
                        b"a:buChar" | b"a:buNone" | b"a:buAutoNum"
                    ) => {}
            Event::End(e) if in_ppr && e.name().as_ref() == b"a:pPr" => {
                in_ppr = false;
                w.write_event(Event::Empty(bullet_el())).map_err(err)?;
                w.write_event(Event::End(e)).map_err(err)?;
            }
            // Bold, italic, size: each run's properties.
            Event::Start(e) if inside && run_attr.is_some() && e.name().as_ref() == b"a:r" => {
                pending_run = true;
                w.write_event(Event::Start(e)).map_err(err)?;
            }
            Event::Empty(e) if pending_run && e.name().as_ref() == b"a:rPr" => {
                pending_run = false;
                w.write_event(Event::Empty(set_run(&e))).map_err(err)?;
            }
            Event::Start(e) if pending_run && e.name().as_ref() == b"a:rPr" => {
                pending_run = false;
                w.write_event(Event::Start(set_run(&e))).map_err(err)?;
            }
            Event::Start(e) if pending_run => {
                pending_run = false;
                w.write_event(Event::Empty(set_run(&BytesStart::new("a:rPr"))))
                    .map_err(err)?;
                w.write_event(Event::Start(e)).map_err(err)?;
            }
            other => w.write_event(other).map_err(err)?,
        }
    }
    String::from_utf8(w.into_inner()).map_err(|e| e.to_string())
}

/// `bytes` with its slides changed by `op`. Returns the new deck and the
/// slide to show after it.
pub fn slides(bytes: &[u8], op: SlideOp) -> Result<(Vec<u8>, usize), String> {
    let mut zip = open(bytes)?;
    let order = order_in(&mut zip)?;
    let pres = part(&mut zip, PRESENTATION).ok_or("no presentation.xml")?;
    let pres_rels = part(&mut zip, PRESENTATION_RELS).ok_or("no presentation relationships")?;
    let ids = slide_ids(&pres);
    let count = ids.len();
    let at = match op {
        SlideOp::New(i) | SlideOp::Duplicate(i) | SlideOp::Delete(i) | SlideOp::Move(i, _) => i,
    };
    if at >= count {
        return Err("no such slide".into());
    }
    match op {
        SlideOp::Move(i, by) => {
            let to = i as isize + by;
            if to < 0 || to as usize >= count {
                return Ok((bytes.to_vec(), i));
            }
            let to = to as usize;
            let mut entries: Vec<&str> = ids.iter().map(|(_, _, s, e)| &pres[*s..*e]).collect();
            entries.swap(i, to);
            let (start, end) = (ids[0].2, ids[count - 1].3);
            let new = format!("{}{}{}", &pres[..start], entries.join(""), &pres[end..]);
            let out = rebuild(
                bytes,
                &BTreeMap::from([(PRESENTATION.to_string(), new)]),
                &HashSet::new(),
                &[],
            )?;
            Ok((out, to))
        }
        SlideOp::Delete(i) => {
            if count == 1 {
                return Err("a deck keeps at least one slide".into());
            }
            let (rid, _, s, e) = &ids[i];
            let slide = order[i].clone();
            let slide_rels_path = rels_of(&slide);
            let slide_rels = part(&mut zip, &slide_rels_path).unwrap_or_default();
            let mut drop: HashSet<String> = HashSet::from([slide.clone(), slide_rels_path]);
            let mut dropped_parts = vec![slide.clone()];
            // Its speaker notes go with it.
            for (_, _, tag) in tags(&slide_rels, "Relationship") {
                if attr_in(tag, "Type").is_some_and(|t| t.ends_with("/notesSlide")) {
                    if let Some(t) = attr_in(tag, "Target") {
                        let notes = resolve(&slide, &t);
                        drop.insert(rels_of(&notes));
                        drop.insert(notes.clone());
                        dropped_parts.push(notes);
                    }
                }
            }
            let new_pres = format!("{}{}", &pres[..*s], &pres[*e..]);
            let new_rels = remove_tag(&pres_rels, "Relationship", |t| {
                attr_in(t, "Id").as_deref() == Some(rid)
            });
            let types = part(&mut zip, TYPES).unwrap_or_default();
            let new_types = remove_tag(&types, "Override", |t| {
                attr_in(t, "PartName")
                    .is_some_and(|p| dropped_parts.iter().any(|d| p == format!("/{d}")))
            });
            let replace = BTreeMap::from([
                (PRESENTATION.to_string(), new_pres),
                (PRESENTATION_RELS.to_string(), new_rels),
                (TYPES.to_string(), new_types),
            ]);
            let out = rebuild(bytes, &replace, &drop, &[])?;
            Ok((out, i.min(count - 2)))
        }
        SlideOp::New(i) | SlideOp::Duplicate(i) => {
            let blank = matches!(op, SlideOp::New(_));
            let src = order[i].clone();
            let mut xml = part(&mut zip, &src).ok_or_else(|| format!("no {src}"))?;
            if blank {
                xml = clear_words(&xml);
            }
            // Its relationships, less its speaker notes (they stay its own).
            let rels = part(&mut zip, &rels_of(&src)).unwrap_or_default();
            let rels = remove_tag(&rels, "Relationship", |t| {
                attr_in(t, "Type").is_some_and(|t| t.ends_with("/notesSlide"))
            });
            let number = zip
                .file_names()
                .filter_map(|n| {
                    n.strip_prefix("ppt/slides/slide")?
                        .strip_suffix(".xml")?
                        .parse::<u32>()
                        .ok()
                })
                .max()
                .unwrap_or(0)
                + 1;
            let path = format!("ppt/slides/slide{number}.xml");
            let rid_n = tags(&pres_rels, "Relationship")
                .into_iter()
                .filter_map(|(_, _, t)| attr_in(t, "Id")?.strip_prefix("rId")?.parse::<u32>().ok())
                .max()
                .unwrap_or(0)
                + 1;
            let rid = format!("rId{rid_n}");
            let id = ids.iter().map(|(_, id, ..)| *id).max().unwrap_or(255) + 1;
            let after = ids[i].3;
            let new_pres = format!(
                "{}<p:sldId id=\"{id}\" r:id=\"{rid}\"/>{}",
                &pres[..after],
                &pres[after..]
            );
            let new_rels = insert_before(
                &pres_rels,
                "</Relationships>",
                &format!("<Relationship Id=\"{rid}\" Type=\"{SLIDE_REL}\" Target=\"slides/slide{number}.xml\"/>"),
            );
            let types = part(&mut zip, TYPES).unwrap_or_default();
            let new_types = insert_before(
                &types,
                "</Types>",
                &format!("<Override PartName=\"/{path}\" ContentType=\"{SLIDE_TYPE}\"/>"),
            );
            let replace = BTreeMap::from([
                (PRESENTATION.to_string(), new_pres),
                (PRESENTATION_RELS.to_string(), new_rels),
                (TYPES.to_string(), new_types),
            ]);
            let add = vec![(path.clone(), xml), (rels_of(&path), rels)];
            let out = rebuild(bytes, &replace, &HashSet::new(), &add)?;
            Ok((out, i + 1))
        }
    }
}

/// A relationship's target from the part `from`, as a zip path.
fn resolve(from: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
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

fn remove_tag(xml: &str, name: &str, hit: impl Fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut from = 0;
    for (s, e, tag) in tags(xml, name) {
        if hit(tag) {
            out.push_str(&xml[from..s]);
            from = e;
        }
    }
    out.push_str(&xml[from..]);
    out
}

fn insert_before(xml: &str, end: &str, what: &str) -> String {
    match xml.rfind(end) {
        Some(i) => format!("{}{what}{}", &xml[..i], &xml[i..]),
        None => xml.to_string(),
    }
}

/// A slide's words emptied (its boxes, pictures and layout kept).
fn clear_words(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(i) = rest.find("<a:t>") {
        out.push_str(&rest[..i + 5]);
        rest = &rest[i + 5..];
        match rest.find("</a:t>") {
            Some(j) => rest = &rest[j..],
            None => break,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECK: &[u8] = include_bytes!("../assets/docs/samples/q3-budget.pptx");

    fn words(bytes: &[u8]) -> Vec<String> {
        crate::office_edit::read_pptx(bytes)
            .unwrap()
            .into_iter()
            .map(|s| s.paras.first().map(|p| p.text.clone()).unwrap_or_default())
            .collect()
    }

    #[test]
    fn slides_move_duplicate_and_delete_in_the_decks_order() {
        let before = words(DECK);
        assert_eq!(before.len(), 4);
        let (moved, at) = slides(DECK, SlideOp::Move(1, 1)).unwrap();
        assert_eq!(at, 2);
        let after = words(&moved);
        assert_eq!(
            (after[1].as_str(), after[2].as_str()),
            (before[2].as_str(), before[1].as_str())
        );

        let (dup, at) = slides(DECK, SlideOp::Duplicate(1)).unwrap();
        assert_eq!(at, 2);
        let w = words(&dup);
        assert_eq!(w.len(), 5);
        assert_eq!(w[2], before[1]);

        let (fresh, _) = slides(DECK, SlideOp::New(0)).unwrap();
        let w = words(&fresh);
        assert_eq!(w.len(), 5);
        assert_eq!(w[1], "");

        let (gone, _) = slides(DECK, SlideOp::Delete(0)).unwrap();
        assert_eq!(words(&gone), before[1..].to_vec());
    }

    #[test]
    fn a_paragraph_turns_bold_and_back_and_gets_a_bullet() {
        let (bold, ..) = current(&format_xml(SLIDE, 1, Format::Bold).unwrap(), 1);
        assert!(bold);
        let twice = format_xml(
            &format_xml(SLIDE, 1, Format::Bold).unwrap(),
            1,
            Format::Bold,
        )
        .unwrap();
        assert!(!current(&twice, 1).0);
        let bullet = format_xml(SLIDE, 1, Format::Bullets).unwrap();
        assert!(current(&bullet, 1).2);
        assert!(!current(&bullet, 0).2);
        let sized = format_xml(SLIDE, 0, Format::Size(40)).unwrap();
        assert!(sized.contains("sz=\"4000\""));
        // The words stay.
        assert!(sized.contains("The forecast") && sized.contains("5,500 EUR"));
    }

    const SLIDE: &str = "<p:sld xmlns:a=\"a\" xmlns:p=\"p\"><p:cSld><p:spTree><p:sp><p:txBody><a:bodyPr/><a:p><a:r><a:t>The forecast</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:txBody><a:p><a:pPr/><a:r><a:rPr lang=\"en\"/><a:t>5,500 EUR</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>";
}
