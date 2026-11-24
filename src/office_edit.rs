//! Editing the words of a Word file or a deck, as `office::write_xlsx_edits`
//! edits a workbook's cells: only the parts that hold changed paragraphs are
//! rewritten, every other entry of the zip is copied as it is.
//!
//! A changed paragraph's new text goes into its first run (`w:t`, `a:t`), so
//! that run's formatting is kept, and the paragraph's other runs are emptied.
//! A paragraph with no run gets one. Nothing else in the XML is touched.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write as _};

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};

/// Which element names a format's paragraphs, runs' text and run ends use.
struct Spec {
    para: &'static [u8],
    text: &'static [u8],
    run: &'static [u8],
    /// A paragraph's closing properties, which a new run goes before.
    end_props: Option<&'static [u8]>,
    /// A field's text (a slide number) is not the paragraph's words.
    field: Option<&'static [u8]>,
    /// Word keeps spaces only under `xml:space="preserve"`.
    preserve: bool,
}

const WORD: Spec = Spec {
    para: b"w:p",
    text: b"w:t",
    run: b"w:r",
    end_props: None,
    field: None,
    preserve: true,
};

const SLIDE: Spec = Spec {
    para: b"a:p",
    text: b"a:t",
    run: b"a:r",
    end_props: Some(b"a:endParaRPr"),
    field: Some(b"a:fld"),
    preserve: false,
};

/// One paragraph as the edit view shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Para {
    /// Its place among its part's paragraphs (what an edit is keyed by).
    pub index: usize,
    pub text: String,
    /// A heading's level (Word's Heading 1..9, Title is 1).
    pub heading: Option<u8>,
    /// A bullet or numbered item.
    pub list: bool,
    /// In a table: (the table's place among the document's, row, column).
    pub table: Option<(usize, usize, usize)>,
}

/// A slide's paragraphs and its speaker notes'.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    pub paras: Vec<Para>,
    pub notes: Vec<Para>,
}

/// Where an edit of a deck goes: a slide's paragraph, or its notes'.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlideKey {
    pub slide: usize,
    pub notes: bool,
    pub para: usize,
}

fn local(e: &BytesStart) -> Vec<u8> {
    e.name().as_ref().to_vec()
}

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    let a = e.attributes().flatten().find(|a| a.key.as_ref() == key)?;
    Some(String::from_utf8_lossy(&a.value).into_owned())
}

fn entity(name: &str) -> String {
    match name {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        _ => name
            .strip_prefix("#x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
            .and_then(char::from_u32)
            .map(String::from)
            .unwrap_or_default(),
    }
}

/// Every paragraph of one XML part, in order (a paragraph inside another,
/// as in a text box, comes after the one it opens in).
fn paragraphs(xml: &str, spec: &Spec) -> Vec<Para> {
    let mut reader = Reader::from_str(xml);
    let mut out: Vec<Para> = vec![];
    let mut stack: Vec<usize> = vec![];
    let (mut in_text, mut in_field) = (false, false);
    // The cell being read: tables nest, the outermost one names the place.
    let (mut tables, mut depth, mut row, mut col) = (0usize, 0usize, 0usize, 0usize);
    loop {
        match reader.read_event() {
            Ok(ref ev @ (Event::Start(_) | Event::Empty(_))) => {
                let (Event::Start(e) | Event::Empty(e)) = ev else {
                    continue;
                };
                let name = local(e);
                if spec.para == WORD.para && name == b"w:tbl" {
                    if matches!(ev, Event::Start(_)) {
                        if depth == 0 {
                            tables += 1;
                            (row, col) = (0, 0);
                        }
                        depth += 1;
                    }
                } else if name == spec.para {
                    out.push(Para {
                        index: out.len(),
                        text: String::new(),
                        heading: None,
                        list: false,
                        table: (depth > 0).then(|| (tables - 1, row, col)),
                    });
                    if matches!(ev, Event::Start(_)) {
                        stack.push(out.len() - 1);
                    }
                } else if name == spec.text {
                    in_text = true;
                } else if spec.field.is_some_and(|f| f == name.as_slice()) {
                    in_field = true;
                } else if name == b"w:pStyle" {
                    if let (Some(&at), Some(style)) = (stack.last(), attr(e, b"w:val")) {
                        out[at].heading = heading_level(&style);
                    }
                } else if name == b"w:numId" {
                    if let (Some(&at), Some(id)) = (stack.last(), attr(e, b"w:val")) {
                        out[at].list = id != "0";
                    }
                }
            }
            Ok(Event::Text(t)) if in_text && !in_field => {
                if let Some(&at) = stack.last() {
                    out[at].text.push_str(&t.decode().unwrap_or_default());
                }
            }
            Ok(Event::GeneralRef(r)) if in_text && !in_field => {
                if let Some(&at) = stack.last() {
                    out[at]
                        .text
                        .push_str(&entity(&r.decode().unwrap_or_default()));
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_vec();
                if spec.para == WORD.para && name == b"w:tbl" {
                    depth = depth.saturating_sub(1);
                } else if spec.para == WORD.para && depth == 1 && name == b"w:tc" {
                    col += 1;
                } else if spec.para == WORD.para && depth == 1 && name == b"w:tr" {
                    (row, col) = (row + 1, 0);
                } else if name == spec.text {
                    in_text = false;
                } else if spec.field.is_some_and(|f| f == name.as_slice()) {
                    in_field = false;
                } else if name == spec.para {
                    stack.pop();
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Word's built-in heading styles: "Heading1" to "Heading9", and "Title".
fn heading_level(style: &str) -> Option<u8> {
    let s = style.to_ascii_lowercase().replace(' ', "");
    if s == "title" {
        return Some(1);
    }
    s.strip_prefix("heading")?
        .parse()
        .ok()
        .filter(|n| (1..=9).contains(n))
}

/// A part's XML with the paragraphs in `edits` (by index) given their new text.
fn rewrite(xml: &str, spec: &Spec, edits: &BTreeMap<usize, String>) -> Result<String, String> {
    struct Frame {
        edit: Option<String>,
        /// The first run's text has been written.
        placed: bool,
    }
    let run_xml = |text: &str, w: &mut Writer<Vec<u8>>| -> Result<(), String> {
        let wr = |e: Event, w: &mut Writer<Vec<u8>>| w.write_event(e).map_err(|e| e.to_string());
        let name = |n: &[u8]| String::from_utf8_lossy(n).into_owned();
        wr(Event::Start(BytesStart::new(name(spec.run))), w)?;
        let mut t = BytesStart::new(name(spec.text));
        if spec.preserve {
            t.push_attribute(("xml:space", "preserve"));
        }
        wr(Event::Start(t), w)?;
        wr(Event::Text(BytesText::new(text)), w)?;
        wr(Event::End(BytesEnd::new(name(spec.text))), w)?;
        wr(Event::End(BytesEnd::new(name(spec.run))), w)
    };
    let preserved = |e: &BytesStart| -> BytesStart<'static> {
        let mut s = e.to_owned();
        if spec.preserve && attr(e, b"xml:space").is_none() {
            s.push_attribute(("xml:space", "preserve"));
        }
        s
    };

    let mut reader = Reader::from_str(xml);
    let mut w = Writer::new(Vec::new());
    let mut stack: Vec<Frame> = vec![];
    let mut count = 0usize;
    // Inside a text element of an edited paragraph: skip what it holds.
    let (mut replacing, mut written_here) = (false, false);
    let mut in_field = false;
    let put = |e: Event, w: &mut Writer<Vec<u8>>| w.write_event(e).map_err(|e| e.to_string());
    loop {
        let ev = reader
            .read_event()
            .map_err(|e| format!("could not read the part: {e}"))?;
        match ev {
            Event::Eof => break,
            Event::Start(ref e) if local(e) == spec.para => {
                let edit = edits.get(&count).cloned();
                count += 1;
                stack.push(Frame {
                    edit,
                    placed: false,
                });
                put(ev.clone(), &mut w)?;
            }
            Event::Empty(ref e) if local(e) == spec.para => {
                let edit = edits.get(&count).cloned();
                count += 1;
                match edit {
                    Some(text) => {
                        put(Event::Start(e.to_owned()), &mut w)?;
                        run_xml(&text, &mut w)?;
                        put(
                            Event::End(BytesEnd::new(String::from_utf8_lossy(spec.para))),
                            &mut w,
                        )?;
                    }
                    None => put(ev.clone(), &mut w)?,
                }
            }
            Event::Start(ref e) if spec.field.is_some_and(|f| f == local(e).as_slice()) => {
                in_field = true;
                put(ev.clone(), &mut w)?;
            }
            Event::End(ref e) if spec.field.is_some_and(|f| f == e.name().as_ref()) => {
                in_field = false;
                put(ev.clone(), &mut w)?;
            }
            Event::Start(ref e) if local(e) == spec.text && !in_field => {
                match stack.last_mut().filter(|f| f.edit.is_some()) {
                    Some(f) => {
                        replacing = true;
                        written_here = false;
                        if f.placed {
                            put(ev.clone(), &mut w)?;
                        } else {
                            put(Event::Start(preserved(e)), &mut w)?;
                        }
                    }
                    None => put(ev.clone(), &mut w)?,
                }
            }
            Event::Empty(ref e) if local(e) == spec.text && !in_field => {
                match stack.last_mut().filter(|f| f.edit.is_some() && !f.placed) {
                    Some(f) => {
                        f.placed = true;
                        let text = f.edit.clone().unwrap_or_default();
                        put(Event::Start(preserved(e)), &mut w)?;
                        put(Event::Text(BytesText::new(&text)), &mut w)?;
                        put(
                            Event::End(BytesEnd::new(String::from_utf8_lossy(spec.text))),
                            &mut w,
                        )?;
                    }
                    None => put(ev.clone(), &mut w)?,
                }
            }
            Event::Text(_) | Event::GeneralRef(_) if replacing => {
                if !written_here {
                    written_here = true;
                    if let Some(f) = stack.last_mut() {
                        if !f.placed {
                            f.placed = true;
                            let text = f.edit.clone().unwrap_or_default();
                            put(Event::Text(BytesText::new(&text)), &mut w)?;
                        }
                    }
                }
            }
            Event::End(ref e) if replacing && e.name().as_ref() == spec.text => {
                replacing = false;
                // An emptied run's text element stays, empty.
                if !written_here {
                    if let Some(f) = stack.last_mut() {
                        if !f.placed {
                            f.placed = true;
                            let text = f.edit.clone().unwrap_or_default();
                            put(Event::Text(BytesText::new(&text)), &mut w)?;
                        }
                    }
                }
                put(ev.clone(), &mut w)?;
            }
            Event::Start(ref e) | Event::Empty(ref e)
                if spec.end_props.is_some_and(|p| p == local(e).as_slice()) =>
            {
                // A deck's closing properties stay last: a new run goes before.
                if let Some(f) = stack.last_mut().filter(|f| f.edit.is_some() && !f.placed) {
                    f.placed = true;
                    let text = f.edit.clone().unwrap_or_default();
                    run_xml(&text, &mut w)?;
                }
                put(ev.clone(), &mut w)?;
            }
            Event::End(ref e) if e.name().as_ref() == spec.para => {
                if let Some(f) = stack.pop() {
                    if let (Some(text), false) = (&f.edit, f.placed) {
                        run_xml(text, &mut w)?;
                    }
                }
                put(ev.clone(), &mut w)?;
            }
            _ => put(ev.clone(), &mut w)?,
        }
    }
    String::from_utf8(w.into_inner()).map_err(|e| e.to_string())
}

fn part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, path: &str) -> Option<String> {
    let mut f = zip.by_name(path).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn open(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>, String> {
    zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("not an Office file KodAct can read: {e}"))
}

/// The zip with each part in `parts` (path to new XML) replaced and every
/// other entry copied as it was.
fn replace_parts(bytes: &[u8], parts: &BTreeMap<String, String>) -> Result<Vec<u8>, String> {
    let mut archive = open(bytes)?;
    let mut out = Vec::with_capacity(bytes.len() + 1024);
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for i in 0..archive.len() {
            let name = archive
                .by_index(i)
                .map_err(|e| e.to_string())?
                .name()
                .to_string();
            match parts.get(&name) {
                Some(xml) => {
                    writer.start_file(&name, opts).map_err(|e| e.to_string())?;
                    writer
                        .write_all(xml.as_bytes())
                        .map_err(|e| e.to_string())?;
                }
                None => {
                    let f = archive.by_index(i).map_err(|e| e.to_string())?;
                    writer.raw_copy_file(f).map_err(|e| e.to_string())?;
                }
            }
        }
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// A Word file's paragraphs, in order, with their headings and lists.
pub fn read_docx(bytes: &[u8]) -> Result<Vec<Para>, String> {
    let mut zip = open(bytes)?;
    let xml = part(&mut zip, "word/document.xml").ok_or("no word/document.xml")?;
    Ok(paragraphs(&xml, &WORD))
}

/// `bytes` with the paragraphs in `edits` (by `Para::index`) set to new text.
pub fn write_docx_text(bytes: &[u8], edits: &BTreeMap<usize, String>) -> Result<Vec<u8>, String> {
    if edits.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut zip = open(bytes)?;
    let xml = part(&mut zip, "word/document.xml").ok_or("no word/document.xml")?;
    let patched = rewrite(&xml, &WORD, edits)?;
    replace_parts(
        bytes,
        &BTreeMap::from([("word/document.xml".to_string(), patched)]),
    )
}

/// The slide parts of a deck in slide order, each with its notes part.
fn slide_parts(zip: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Vec<(String, Option<String>)> {
    // The deck's own order (a moved slide is in its new place); file
    // numbers when it has no slide list.
    let ordered: Vec<(usize, String)> = crate::pptx_edit::order_in(zip)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| {
            let n = p.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse().ok()?;
            Some((n, p))
        })
        .collect();
    let mut slides: Vec<(usize, String)> = zip
        .file_names()
        .filter_map(|n| {
            let i: usize = n
                .strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?
                .parse()
                .ok()?;
            Some((i, n.to_string()))
        })
        .collect();
    slides.sort();
    if !ordered.is_empty() {
        slides = ordered;
    }
    slides
        .into_iter()
        .map(|(n, path)| {
            let rels =
                part(zip, &format!("ppt/slides/_rels/slide{n}.xml.rels")).unwrap_or_default();
            let notes = rels
                .split("Target=\"")
                .skip(1)
                .filter_map(|r| r.split('"').next())
                .find(|t| t.contains("notesSlide"))
                .map(|t| format!("ppt/notesSlides/{}", t.rsplit('/').next().unwrap_or(t)));
            (path, notes)
        })
        .collect()
}

/// A deck's slides: the words on each and its speaker notes (paragraphs with
/// something in them).
pub fn read_pptx(bytes: &[u8]) -> Result<Vec<Slide>, String> {
    let mut zip = open(bytes)?;
    let parts = slide_parts(&mut zip);
    if parts.is_empty() {
        return Err("no slides: not a PowerPoint deck".into());
    }
    let words = |xml: Option<String>| -> Vec<Para> {
        xml.map(|x| paragraphs(&x, &SLIDE))
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.text.trim().is_empty())
            .collect()
    };
    Ok(parts
        .into_iter()
        .map(|(slide, notes)| Slide {
            paras: words(part(&mut zip, &slide)),
            notes: words(notes.and_then(|n| part(&mut zip, &n))),
        })
        .collect())
}

/// `bytes` with the paragraphs in `edits` set to new text.
pub fn write_pptx_text(
    bytes: &[u8],
    edits: &BTreeMap<SlideKey, String>,
) -> Result<Vec<u8>, String> {
    if edits.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut zip = open(bytes)?;
    let parts = slide_parts(&mut zip);
    let mut out = BTreeMap::new();
    for (i, (slide, notes)) in parts.into_iter().enumerate() {
        for (path, is_notes) in [(Some(slide), false), (notes, true)] {
            let Some(path) = path else { continue };
            let here: BTreeMap<usize, String> = edits
                .iter()
                .filter(|(k, _)| k.slide == i && k.notes == is_notes)
                .map(|(k, v)| (k.para, v.clone()))
                .collect();
            if here.is_empty() {
                continue;
            }
            let xml = part(&mut zip, &path).ok_or_else(|| format!("no {path}"))?;
            out.insert(path, rewrite(&xml, &SLIDE, &here)?);
        }
    }
    replace_parts(bytes, &out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::{text, Kind};

    fn zip_of(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut out));
            let opts = zip::write::SimpleFileOptions::default();
            for (name, body) in entries {
                w.start_file(*name, opts).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        out
    }

    fn entries(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
        let mut z = open(bytes).unwrap();
        (0..z.len())
            .map(|i| {
                let mut f = z.by_index(i).unwrap();
                let mut b = vec![];
                f.read_to_end(&mut b).unwrap();
                (f.name().to_string(), b)
            })
            .collect()
    }

    const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="w"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Budget </w:t></w:r><w:r><w:t>update</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">Revenue &amp; costs are </w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>flat</w:t></w:r></w:p>
<w:p/>
<w:p><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Third</w:t></w:r></w:p>
</w:body></w:document>"#;

    fn docx() -> Vec<u8> {
        zip_of(&[
            ("[Content_Types].xml", "<Types/>"),
            ("word/styles.xml", "<styles/>"),
            ("word/document.xml", DOC),
        ])
    }

    #[test]
    fn a_docx_is_read_paragraph_by_paragraph() {
        let p = read_docx(&docx()).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(
            (p[0].text.as_str(), p[0].heading),
            ("Budget update", Some(1))
        );
        assert_eq!(p[1].text, "Revenue & costs are flat");
        assert_eq!(p[2].text, "");
        assert!(p[3].list);
    }

    #[test]
    fn changing_one_paragraph_of_a_docx_changes_only_it() {
        let before = docx();
        let edits = BTreeMap::from([(1usize, "Revenue is up & costs are flat".to_string())]);
        let after = write_docx_text(&before, &edits).unwrap();
        // Re-read with the app's own reader: one paragraph differs.
        let was = text(Kind::Docx, &before).unwrap();
        let now = text(Kind::Docx, &after).unwrap();
        assert!(now.contains("Revenue is up"), "{now}");
        assert!(
            now.contains("Budget") && now.contains("update") && now.contains("Third"),
            "{now}"
        );
        assert_ne!(was, now);
        let (a, b) = (read_docx(&before).unwrap(), read_docx(&after).unwrap());
        assert_eq!(b.len(), a.len());
        for i in [0, 2, 3] {
            assert_eq!(a[i], b[i], "paragraph {i}");
        }
        assert_eq!(b[1].text, "Revenue is up & costs are flat");
        // The first run kept its formatting; the rest are emptied.
        let xml = String::from_utf8(entries(&after)["word/document.xml"].clone()).unwrap();
        assert!(xml.contains("<w:i/>"), "the second run's properties stay");
        assert!(
            xml.contains("<w:b/>"),
            "an untouched paragraph is as it was"
        );
        // Only the one part changed.
        let (ea, eb) = (entries(&before), entries(&after));
        for (name, bytes) in &ea {
            if name != "word/document.xml" {
                assert_eq!(&eb[name], bytes, "{name}");
            }
        }
    }

    #[test]
    fn an_empty_paragraph_gets_a_run_and_a_heading_keeps_its_style() {
        let edits = BTreeMap::from([(2usize, "Added".to_string()), (0, "Budget".to_string())]);
        let after = write_docx_text(&docx(), &edits).unwrap();
        let p = read_docx(&after).unwrap();
        assert_eq!(p[2].text, "Added");
        assert_eq!((p[0].text.as_str(), p[0].heading), ("Budget", Some(1)));
    }

    #[test]
    fn a_tables_cells_are_placed_by_row_and_column() {
        let doc = r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>Before</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>A2</w:t></w:r></w:p><w:p><w:r><w:t>A2b</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:t>After</w:t></w:r></w:p></w:body></w:document>"#;
        let bytes = zip_of(&[
            ("[Content_Types].xml", "<Types/>"),
            ("word/document.xml", doc),
        ]);
        let p = read_docx(&bytes).unwrap();
        let at = |t: &str| p.iter().find(|x| x.text == t).unwrap().table;
        assert_eq!(at("Before"), None);
        assert_eq!(at("A1"), Some((0, 0, 0)));
        assert_eq!(at("B1"), Some((0, 0, 1)));
        assert_eq!(at("A2"), Some((0, 1, 0)));
        assert_eq!(at("A2b"), Some((0, 1, 0)));
        assert_eq!(at("B2"), Some((0, 1, 1)));
        assert_eq!(at("After"), None);
    }

    #[test]
    fn nothing_edited_is_the_same_file() {
        let b = docx();
        assert_eq!(write_docx_text(&b, &BTreeMap::new()).unwrap(), b);
    }

    const SLIDE1: &str = r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:rPr b="1"/><a:t>The forecast</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:txBody><a:p><a:r><a:t>5,500 EUR, </a:t></a:r><a:r><a:t>up 350</a:t></a:r></a:p><a:p><a:r><a:t>Second</a:t></a:r><a:endParaRPr lang="en"/></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#;
    const NOTES1: &str = r#"<p:notes xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Say it slowly</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:notes>"#;

    fn pptx() -> Vec<u8> {
        zip_of(&[
            ("[Content_Types].xml", "<Types/>"),
            ("ppt/slides/slide1.xml", SLIDE1),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                r#"<Relationships><Relationship Id="r1" Type="x/notesSlide" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#,
            ),
            ("ppt/notesSlides/notesSlide1.xml", NOTES1),
        ])
    }

    #[test]
    fn a_deck_is_read_slide_by_slide_with_its_notes() {
        let s = read_pptx(&pptx()).unwrap();
        assert_eq!(s.len(), 1);
        let words: Vec<_> = s[0].paras.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(words, ["The forecast", "5,500 EUR, up 350", "Second"]);
        assert_eq!(s[0].notes[0].text, "Say it slowly");
    }

    #[test]
    fn changing_a_text_box_and_a_note_of_a_deck_changes_only_them() {
        let before = pptx();
        let edits = BTreeMap::from([
            (
                SlideKey {
                    slide: 0,
                    notes: false,
                    para: 1,
                },
                "5,900 EUR".to_string(),
            ),
            (
                SlideKey {
                    slide: 0,
                    notes: true,
                    para: 0,
                },
                "Say it fast".to_string(),
            ),
        ]);
        let after = write_pptx_text(&before, &edits).unwrap();
        // The app's own reader sees the new words and the old others.
        let now = text(Kind::Pptx, &after).unwrap();
        assert!(
            now.contains("5,900 EUR") && now.contains("The forecast") && now.contains("Second")
        );
        assert!(!now.contains("up 350"));
        let s = read_pptx(&after).unwrap();
        assert_eq!(s[0].paras[1].text, "5,900 EUR");
        assert_eq!(s[0].notes[0].text, "Say it fast");
        // The title's run properties and the paragraph's end properties stay.
        let xml = String::from_utf8(entries(&after)["ppt/slides/slide1.xml"].clone()).unwrap();
        assert!(xml.contains(r#"<a:rPr b="1"/>"#));
        assert!(xml.contains(r#"<a:endParaRPr lang="en"/>"#));
        let (ea, eb) = (entries(&before), entries(&after));
        assert_eq!(ea["[Content_Types].xml"], eb["[Content_Types].xml"]);
        assert_eq!(
            ea["ppt/slides/_rels/slide1.xml.rels"],
            eb["ppt/slides/_rels/slide1.xml.rels"]
        );
    }

    #[test]
    fn special_characters_are_escaped_and_read_back() {
        let edits = BTreeMap::from([(1usize, "a < b & \"c\"".to_string())]);
        let after = write_docx_text(&docx(), &edits).unwrap();
        assert_eq!(read_docx(&after).unwrap()[1].text, "a < b & \"c\"");
    }
}
