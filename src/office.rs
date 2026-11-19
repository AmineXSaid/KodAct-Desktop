//! Office files and PDFs, read natively for the side panel. KodAct's
//! office-documents skill writes them; this shows what it wrote, without
//! Word, Excel or a PDF viewer:
//!
//! - a workbook becomes its sheets' cells (calamine; the values the file
//!   carries, which is why the skill recalculates after editing formulas);
//! - Word and PowerPoint become Markdown (their XML, read with quick-xml),
//!   shown in the document view: headings, lists, bold, tables, slides;
//! - a PDF becomes its text, page by page (lopdf), and its pages as
//!   pictures (hayro); a Word or PowerPoint file's pages are drawn the same
//!   way once LibreOffice has printed it to PDF (`office_render.rs`).
//!
//! Everything is capped, so a huge file can't stall the panel. Parsing is
//! blocking: run it off the UI thread.

use std::io::{Cursor, Read};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// The most rows and columns of a sheet the panel shows.
pub const MAX_ROWS: usize = 500;
pub const MAX_COLS: usize = 50;
/// The most slides and pages read.
const MAX_PARTS: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Xlsx,
    Docx,
    Pptx,
    Pdf,
}

impl Kind {
    pub fn of(name: &str) -> Option<Kind> {
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        Some(match ext.as_str() {
            "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" => Kind::Xlsx,
            "docx" => Kind::Docx,
            "pptx" => Kind::Pptx,
            "pdf" => Kind::Pdf,
            _ => return None,
        })
    }

    /// What a card calls it.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Xlsx => "Spreadsheet",
            Kind::Docx | Kind::Pdf => "Document",
            Kind::Pptx => "Presentation",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub name: String,
    /// The cells shown, as text; `numeric` marks columns of numbers per cell.
    pub rows: Vec<Vec<CellText>>,
    /// The sheet's real size, which may be more than is shown.
    pub total_rows: usize,
    pub total_cols: usize,
    /// Column widths the workbook sets, in Excel's units, by column.
    pub widths: Vec<Option<f32>>,
    /// Rows frozen at the top.
    pub frozen_rows: usize,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CellText {
    /// As the workbook shows it: its number format applied.
    pub text: String,
    pub numeric: bool,
    /// How it looks and what it computes (`xlsx_style.rs`).
    pub look: crate::xlsx_style::Look,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    Sheets(Vec<Sheet>),
    /// Word, PowerPoint or a PDF: its text (Markdown) at once, and its pages
    /// as they print once drawn (`draw`), which can take seconds.
    Doc(Doc),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Doc {
    pub text: String,
    /// The pages as they print, when they could be drawn: a PDF's own, or
    /// a Word or PowerPoint file's through LibreOffice (`office_render.rs`).
    pub pages: Vec<Page>,
    /// How many pages it has in all (more than are drawn past `MAX_PAGES`).
    pub total: usize,
    /// A deck: the pages are slides.
    pub slides: bool,
    /// A deck's speaker notes, slide by slide.
    pub notes: Vec<String>,
    /// Its pages are being drawn (LibreOffice is at work).
    pub drawing: bool,
}

/// One PDF page as a picture, and its size in pixels.
#[derive(Clone)]
pub struct Page {
    pub image: std::sync::Arc<gpui::Image>,
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Page({}x{})", self.width, self.height)
    }
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.image, &other.image)
    }
}

/// The most pages drawn; past them the panel says how many more there are.
pub const MAX_PAGES: usize = 40;

/// A PDF's pages as PNGs, drawn by hayro at 1.5 pixels per point (sharp
/// on a 1x screen, fair on 2x). `Err` when it can't draw them; the panel
/// then shows the text.
fn pages(bytes: &[u8]) -> Result<(Vec<Page>, usize), String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    let pdf = Pdf::new(bytes.to_vec()).map_err(|e| format!("not a PDF KodAct can draw: {e:?}"))?;
    let total = pdf.pages().len();
    let cache = hayro::RenderCache::new();
    let settings = InterpreterSettings::default();
    let mut out = vec![];
    for page in pdf.pages().iter().take(MAX_PAGES) {
        let pixmap = hayro::render(
            page,
            &cache,
            &settings,
            &hayro::RenderSettings::default(),
            &hayro::PixmapSettings {
                x_scale: 1.5,
                y_scale: 1.5,
                bg_color: hayro::vello_cpu::color::palette::css::WHITE,
            },
        );
        let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        // An opaque page: premultiplied and straight alpha are the same.
        let rgba = image::RgbaImage::from_raw(width, height, pixmap.data_as_u8_slice().to_vec())
            .ok_or("a page KodAct could not draw")?;
        let mut png = Vec::new();
        rgba.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        out.push(Page {
            image: std::sync::Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, png)),
            width,
            height,
        });
    }
    if out.is_empty() {
        return Err("the PDF has no pages".into());
    }
    Ok((out, total))
}

/// What the panel shows at once: a workbook's sheets, or a document's text
/// (a PDF's pages too, which draw quickly). `draw` adds the pages of a Word
/// or PowerPoint file after.
pub fn preview(name: &str, bytes: &[u8]) -> Result<Preview, String> {
    let kind = Kind::of(name).ok_or("not an Office file or a PDF")?;
    if kind == Kind::Xlsx {
        return sheets(bytes).map(Preview::Sheets);
    }
    let mut doc = Doc {
        slides: kind == Kind::Pptx,
        notes: if kind == Kind::Pptx {
            notes(bytes)
        } else {
            vec![]
        },
        ..Default::default()
    };
    if kind == Kind::Pdf {
        // Drawn when hayro can (a panic in it is caught), else its text alone.
        if let Ok(Ok((pages, total))) = std::panic::catch_unwind(|| pages(bytes)) {
            (doc.pages, doc.total) = (pages, total);
        }
    }
    doc.text = text(kind, bytes)?;
    doc.drawing = kind != Kind::Pdf && crate::office_render::soffice().is_some();
    Ok(Preview::Doc(doc))
}

/// A Word or PowerPoint file's pages as they print, through LibreOffice;
/// `Err` when it is not installed or could not render the file.
pub fn draw(name: &str, bytes: &[u8]) -> Result<(Vec<Page>, usize), String> {
    let pdf = crate::office_render::to_pdf(name, bytes)?;
    std::panic::catch_unwind(|| pages(&pdf))
        .unwrap_or_else(|_| Err("the pages could not be drawn".into()))
}

/// A deck's speaker notes, slide by slide (empty where a slide has none).
fn notes(bytes: &[u8]) -> Vec<String> {
    let Ok(mut zip) = open_zip(bytes) else {
        return vec![];
    };
    let mut slides: Vec<usize> = zip
        .file_names()
        .filter_map(|n| {
            n.strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?
                .parse()
                .ok()
        })
        .collect();
    slides.sort();
    slides
        .into_iter()
        .take(MAX_PARTS)
        .map(|n| {
            let rels =
                part(&mut zip, &format!("ppt/slides/_rels/slide{n}.xml.rels")).unwrap_or_default();
            let target = rels
                .split("Target=\"")
                .skip(1)
                .filter_map(|r| r.split('"').next())
                .find(|t| t.contains("notesSlide"))
                .map(|t| format!("ppt/notesSlides/{}", t.rsplit('/').next().unwrap_or(t)));
            target
                .and_then(|t| part(&mut zip, &t))
                .map(|xml| notes_text(&xml))
                .unwrap_or_default()
        })
        .collect()
}

/// A notes page's words: its body placeholder's paragraphs, not the slide
/// number field.
fn notes_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    let (mut body, mut in_text, mut in_field) = (false, false, false);
    let (mut para, mut out) = (String::new(), Vec::new());
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"p:sp" => body = false,
                b"p:ph" => body = attr(&e, b"type").as_deref() == Some("body"),
                b"a:fld" => in_field = true,
                b"a:t" => in_text = true,
                b"a:p" => para.clear(),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text && body && !in_field => {
                para.push_str(&t.decode().unwrap_or_default())
            }
            Ok(Event::GeneralRef(r)) if in_text && body && !in_field => {
                para.push_str(&entity(&r.decode().unwrap_or_default()))
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"a:t" => in_text = false,
                b"a:fld" => in_field = false,
                b"a:p" if body && !para.trim().is_empty() => out.push(para.trim().to_string()),
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out.join("\n")
}

/// A Word or PowerPoint file, or a PDF, as text (Markdown for the first
/// two); for an attached file, which the model reads.
pub fn text(kind: Kind, bytes: &[u8]) -> Result<String, String> {
    match kind {
        Kind::Docx => docx(bytes),
        Kind::Pptx => pptx(bytes),
        Kind::Pdf => pdf(bytes),
        Kind::Xlsx => Err("a workbook: use sheets_csv".into()),
    }
}

/// Every sheet of a workbook as CSV, in order, until `budget` characters;
/// the bool is true when something was cut.
pub fn sheets_csv(bytes: &[u8], budget: usize) -> Result<(Vec<(String, String)>, bool), String> {
    use calamine::{Data, Reader as _};
    let mut book = calamine::open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|e| format!("not a workbook KodAct can read: {e}"))?;
    let cell = |d: &Data| -> String {
        let s = match d {
            Data::Empty => String::new(),
            Data::Int(i) => i.to_string(),
            Data::Float(f) => number(*f),
            Data::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
            Data::DateTime(d) => number(d.as_f64()),
            Data::Error(e) => format!("#{e:?}").to_uppercase(),
        };
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s
        }
    };
    let mut left = budget;
    let mut cut = false;
    let mut out = vec![];
    for name in book.sheet_names().to_vec() {
        if left == 0 {
            cut = true;
            break;
        }
        let range = book.worksheet_range(&name).map_err(|e| e.to_string())?;
        let mut csv = String::new();
        for row in range.rows() {
            let line = row.iter().map(cell).collect::<Vec<_>>().join(",");
            let line = line.trim_end_matches(',');
            if csv.len() + line.len() + 1 > left {
                cut = true;
                break;
            }
            csv.push_str(line);
            csv.push('\n');
        }
        left -= csv.len().min(left);
        out.push((name, csv));
    }
    Ok((out, cut))
}

/// A number as a spreadsheet shows it by default: no trailing zeros.
fn number(f: f64) -> String {
    if f.fract() == 0. && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        let s = format!("{f:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// "A", "B", … "Z", "AA": a column's letters.
pub fn column_letters(mut n: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

fn sheets(bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    use calamine::{Data, Reader as _};
    let mut book = calamine::open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|e| format!("not a workbook KodAct can read: {e}"))?;
    let looks = crate::xlsx_style::read(bytes);
    let mut out = vec![];
    for name in book.sheet_names().to_vec() {
        let range = book.worksheet_range(&name).map_err(|e| e.to_string())?;
        let look = looks.get(&name).cloned().unwrap_or_default();
        let (h, w) = range.get_size();
        // Cells start at the range's origin; show them from A1 so the
        // letters and numbers match the workbook's.
        let (r0, c0) = range
            .start()
            .map(|(r, c)| (r as usize, c as usize))
            .unwrap_or((0, 0));
        let total_rows = if h == 0 { 0 } else { r0 + h };
        let total_cols = if w == 0 { 0 } else { c0 + w };
        let mut rows =
            vec![vec![CellText::default(); total_cols.min(MAX_COLS)]; total_rows.min(MAX_ROWS)];
        for (r, row) in range.rows().enumerate().take(MAX_ROWS.saturating_sub(r0)) {
            for (c, cell) in row.iter().enumerate().take(MAX_COLS.saturating_sub(c0)) {
                let (text, value) = match cell {
                    Data::Empty => (String::new(), None),
                    Data::Int(i) => (i.to_string(), Some(*i as f64)),
                    Data::Float(f) => (number(*f), Some(*f)),
                    Data::Bool(b) => (if *b { "TRUE" } else { "FALSE" }.to_string(), None),
                    Data::String(s) => (s.clone(), None),
                    Data::DateTime(d) => (number(d.as_f64()), Some(d.as_f64())),
                    Data::DateTimeIso(s) | Data::DurationIso(s) => (s.clone(), None),
                    Data::Error(e) => (format!("#{e:?}").to_uppercase(), None),
                };
                let at = (r0 + r, c0 + c);
                let style = look.cells.get(&at).cloned().unwrap_or_default();
                // Numbers as the workbook formats them: 5,500, 6.8%, a date.
                let text = match (value, &style.format) {
                    (Some(v), Some(code)) => crate::xlsx_style::format(v, code),
                    _ => text,
                };
                rows[at.0][at.1] = CellText {
                    text,
                    numeric: value.is_some(),
                    look: style,
                };
            }
        }
        let widths = (0..total_cols.min(MAX_COLS))
            .map(|c| look.widths.get(&c).copied())
            .collect();
        out.push(Sheet {
            name,
            rows,
            total_rows,
            total_cols,
            widths,
            frozen_rows: look.frozen_rows,
        });
    }
    Ok(out)
}

/// Pending workbook cell edits keyed by `(sheet_index, row_0, col_0) -> raw value`.
pub type CellEdits = std::collections::HashMap<(usize, usize, usize), String>;

/// Write edited cell values/formulas into a copy of an `.xlsx` workbook in
/// pure Rust (`zip` + `quick_xml`), preserving sheets, styles, and column widths.
pub fn write_xlsx_edits(bytes: &[u8], edits: &CellEdits) -> Result<Vec<u8>, String> {
    use std::collections::BTreeMap;
    use std::io::Write as _;

    if edits.is_empty() {
        return Ok(bytes.to_vec());
    }
    let paths = crate::xlsx_style::sheet_paths(bytes);
    let mut by_path: std::collections::HashMap<String, BTreeMap<usize, BTreeMap<usize, String>>> =
        std::collections::HashMap::new();
    for (&(si, r, c), val) in edits {
        let Some(Some(path)) = paths.get(si) else {
            return Err(format!("sheet {} not found in workbook", si + 1));
        };
        by_path
            .entry(path.clone())
            .or_default()
            .entry(r)
            .or_default()
            .insert(c, val.clone());
    }

    let mut archive = open_zip(bytes)?;
    let mut out = Vec::with_capacity(bytes.len() + 1024);
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for i in 0..archive.len() {
            let entry_name = {
                let f = archive.by_index(i).map_err(|e| e.to_string())?;
                f.name().to_string()
            };
            if let Some(sheet_edits) = by_path.get(&entry_name) {
                let xml = {
                    let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
                    let mut s = String::new();
                    f.read_to_string(&mut s).map_err(|e| e.to_string())?;
                    s
                };
                let patched = patch_sheet_xml(&xml, sheet_edits)?;
                writer.start_file(&entry_name, opts).map_err(|e| e.to_string())?;
                writer
                    .write_all(patched.as_bytes())
                    .map_err(|e| e.to_string())?;
            } else {
                let f = archive.by_index(i).map_err(|e| e.to_string())?;
                writer.raw_copy_file(f).map_err(|e| e.to_string())?;
            }
        }
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn push_cell_xml(out: &mut String, r: usize, c: usize, val: &str, style: Option<&str>) {
    let ref_name = format!("{}{}", column_letters(c), r + 1);
    let style_attr = style
        .map(|s| format!(" s=\"{}\"", quick_xml::escape::escape(s)))
        .unwrap_or_default();
    let trimmed = val.trim();
    if val.is_empty() {
        out.push_str(&format!("<c r=\"{ref_name}\"{style_attr}/>"));
    } else if let Some(formula) = trimmed.strip_prefix('=') {
        out.push_str(&format!(
            "<c r=\"{ref_name}\"{style_attr}><f>{}</f></c>",
            quick_xml::escape::escape(formula)
        ));
    } else if let Ok(n) = trimmed.parse::<f64>() {
        if n.is_finite() {
            out.push_str(&format!(
                "<c r=\"{ref_name}\"{style_attr}><v>{trimmed}</v></c>"
            ));
        } else {
            out.push_str(&format!(
                "<c r=\"{ref_name}\"{style_attr} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                quick_xml::escape::escape(val)
            ));
        }
    } else {
        out.push_str(&format!(
            "<c r=\"{ref_name}\"{style_attr} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
            quick_xml::escape::escape(val)
        ));
    }
}

fn push_row_xml(
    out: &mut String,
    r: usize,
    cells: &std::collections::BTreeMap<usize, String>,
    styles: &std::collections::HashMap<(usize, usize), String>,
) {
    out.push_str(&format!("<row r=\"{}\">", r + 1));
    for (&c, val) in cells {
        push_cell_xml(
            out,
            r,
            c,
            val,
            styles.get(&(r, c)).map(String::as_str),
        );
    }
    out.push_str("</row>");
}

fn write_xml_event(
    writer: &mut quick_xml::Writer<Vec<u8>>,
    ev: quick_xml::events::Event<'_>,
) -> Result<(), String> {
    writer.write_event(ev).map_err(|e| e.to_string())
}

fn patch_sheet_xml(
    xml: &str,
    edits: &std::collections::BTreeMap<usize, std::collections::BTreeMap<usize, String>>,
) -> Result<String, String> {
    use quick_xml::events::BytesEnd;

    let mut styles: std::collections::HashMap<(usize, usize), String> =
        std::collections::HashMap::new();
    let mut style_reader = Reader::from_str(xml);
    loop {
        match style_reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"c" => {
                if let (Some(at), Some(s)) = (
                    attr(&e, b"r").and_then(|r| crate::xlsx_style::cell_ref(&r)),
                    attr(&e, b"s"),
                ) {
                    styles.insert(at, s);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    let max_edit_r = edits.keys().copied().max().unwrap_or(0);
    let max_edit_c = edits
        .values()
        .flat_map(|m| m.keys().copied())
        .max()
        .unwrap_or(0);

    let mut reader = Reader::from_str(xml);
    let mut writer = quick_xml::Writer::new(Vec::with_capacity(xml.len() + 512));
    let mut remaining = edits.clone();
    let mut current_row: Option<usize> = None;
    let mut row_remaining: std::collections::BTreeMap<usize, String> =
        std::collections::BTreeMap::new();
    let mut skip_cell_depth = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(ev) => {
                if skip_cell_depth > 0 {
                    match ev {
                        Event::Start(_) => skip_cell_depth += 1,
                        Event::End(_) => skip_cell_depth -= 1,
                        _ => {}
                    }
                    continue;
                }
                match &ev {
                    Event::Empty(e) if e.local_name().as_ref() == b"dimension" => {
                        if let Some(dim) = attr(e, b"ref") {
                            let (mut r1, mut c1) = (max_edit_r, max_edit_c);
                            if let Some((_, end_ref)) = dim.split_once(':') {
                                if let Some((er, ec)) = crate::xlsx_style::cell_ref(end_ref) {
                                    r1 = r1.max(er);
                                    c1 = c1.max(ec);
                                }
                            } else if let Some((er, ec)) = crate::xlsx_style::cell_ref(&dim) {
                                r1 = r1.max(er);
                                c1 = c1.max(ec);
                            }
                            let start = dim.split(':').next().unwrap_or("A1");
                            let new_dim = format!("{start}:{}{}", column_letters(c1), r1 + 1);
                            let mut next = BytesStart::new("dimension");
                            next.push_attribute(("ref", new_dim.as_str()));
                            write_xml_event(&mut writer, Event::Empty(next))?;
                            continue;
                        }
                    }
                    Event::Empty(e) if e.local_name().as_ref() == b"sheetData" => {
                        write_xml_event(&mut writer, Event::Start(e.clone().into_owned()))?;
                        let mut extra = String::new();
                        for (r, cells) in std::mem::take(&mut remaining) {
                            push_row_xml(&mut extra, r, &cells, &styles);
                        }
                        writer
                            .get_mut()
                            .extend_from_slice(extra.as_bytes());
                        write_xml_event(&mut writer, Event::End(BytesEnd::new("sheetData")))?;
                        continue;
                    }
                    Event::Start(e) if e.local_name().as_ref() == b"row" => {
                        let r_idx = attr(e, b"r")
                            .and_then(|v| v.parse::<usize>().ok())
                            .and_then(|v| v.checked_sub(1));
                        if let Some(ri) = r_idx {
                            let earlier: Vec<usize> =
                                remaining.range(..ri).map(|(&k, _)| k).collect();
                            if !earlier.is_empty() {
                                let mut extra = String::new();
                                for er in earlier {
                                    if let Some(cells) = remaining.remove(&er) {
                                        push_row_xml(&mut extra, er, &cells, &styles);
                                    }
                                }
                                writer.get_mut().extend_from_slice(extra.as_bytes());
                            }
                            row_remaining = remaining.remove(&ri).unwrap_or_default();
                        }
                        current_row = r_idx;
                    }
                    Event::Empty(e) if e.local_name().as_ref() == b"row" => {
                        let r_idx = attr(e, b"r")
                            .and_then(|v| v.parse::<usize>().ok())
                            .and_then(|v| v.checked_sub(1));
                        if let Some(ri) = r_idx {
                            let earlier: Vec<usize> =
                                remaining.range(..ri).map(|(&k, _)| k).collect();
                            if !earlier.is_empty() {
                                let mut extra = String::new();
                                for er in earlier {
                                    if let Some(cells) = remaining.remove(&er) {
                                        push_row_xml(&mut extra, er, &cells, &styles);
                                    }
                                }
                                writer.get_mut().extend_from_slice(extra.as_bytes());
                            }
                            if let Some(cells) = remaining.remove(&ri) {
                                write_xml_event(&mut writer, Event::Start(e.clone().into_owned()))?;
                                let mut extra = String::new();
                                for (&c, val) in &cells {
                                    push_cell_xml(
                                        &mut extra,
                                        ri,
                                        c,
                                        val,
                                        styles.get(&(ri, c)).map(String::as_str),
                                    );
                                }
                                writer.get_mut().extend_from_slice(extra.as_bytes());
                                write_xml_event(&mut writer, Event::End(BytesEnd::new("row")))?;
                                continue;
                            }
                        }
                    }
                    Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == b"c" => {
                        if let (Some(ri), Some((cr, cc))) =
                            (current_row, attr(e, b"r").and_then(|r| crate::xlsx_style::cell_ref(&r)))
                        {
                            let earlier: Vec<usize> =
                                row_remaining.range(..cc).map(|(&k, _)| k).collect();
                            if !earlier.is_empty() {
                                let mut extra = String::new();
                                for ec in earlier {
                                    if let Some(val) = row_remaining.remove(&ec) {
                                        push_cell_xml(
                                            &mut extra,
                                            ri,
                                            ec,
                                            &val,
                                            styles.get(&(ri, ec)).map(String::as_str),
                                        );
                                    }
                                }
                                writer.get_mut().extend_from_slice(extra.as_bytes());
                            }
                            if cr == ri {
                                if let Some(val) = row_remaining.remove(&cc) {
                                    let mut extra = String::new();
                                    push_cell_xml(
                                        &mut extra,
                                        ri,
                                        cc,
                                        &val,
                                        styles.get(&(ri, cc)).map(String::as_str),
                                    );
                                    writer.get_mut().extend_from_slice(extra.as_bytes());
                                    if matches!(ev, Event::Start(_)) {
                                        skip_cell_depth = 1;
                                    }
                                    continue;
                                }
                            }
                        }
                    }
                    Event::End(e) if e.local_name().as_ref() == b"row" => {
                        if let Some(ri) = current_row.take() {
                            if !row_remaining.is_empty() {
                                let mut extra = String::new();
                                for (c, val) in std::mem::take(&mut row_remaining) {
                                    push_cell_xml(
                                        &mut extra,
                                        ri,
                                        c,
                                        &val,
                                        styles.get(&(ri, c)).map(String::as_str),
                                    );
                                }
                                writer.get_mut().extend_from_slice(extra.as_bytes());
                            }
                        }
                    }
                    Event::End(e) if e.local_name().as_ref() == b"sheetData" => {
                        if !remaining.is_empty() {
                            let mut extra = String::new();
                            for (r, cells) in std::mem::take(&mut remaining) {
                                push_row_xml(&mut extra, r, &cells, &styles);
                            }
                            writer.get_mut().extend_from_slice(extra.as_bytes());
                        }
                    }
                    _ => {}
                }
                write_xml_event(&mut writer, ev)?;
            }
            Err(e) => return Err(format!("could not update worksheet XML: {e}")),
        }
    }
    String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())
}

/// One file inside an Office zip, as text.
fn part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, path: &str) -> Option<String> {
    let mut f = zip.by_name(path).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn open_zip(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>, String> {
    zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("not an Office file KodAct can read: {e}"))
}

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    let a = e.attributes().flatten().find(|a| a.key.as_ref() == key)?;
    let raw = std::str::from_utf8(&a.value).ok()?;
    Some(
        quick_xml::escape::unescape(raw)
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| raw.to_string()),
    )
}

/// Text from a `&…;` reference.
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

/// Markdown's special characters in plain text, made literal.
fn escape_md(s: &str) -> String {
    s.replace('*', "\\*")
        .replace('`', "\\`")
        .replace('|', "\\|")
}

/// Runs of one paragraph, with bold kept as `**`.
#[derive(Default)]
struct Para {
    style: String,
    list: bool,
    numbered: bool,
    text: String,
    run_bold: bool,
    run: String,
}

impl Para {
    fn end_run(&mut self) {
        let t = std::mem::take(&mut self.run);
        if t.trim().is_empty() {
            self.text.push_str(&t);
        } else if self.run_bold {
            // Keep the spaces outside the markers, as Markdown wants.
            let (lead, rest) = t.split_at(t.len() - t.trim_start().len());
            let trail = &rest[rest.trim_end().len()..];
            self.text
                .push_str(&format!("{lead}**{}**{trail}", rest.trim()));
        } else {
            self.text.push_str(&t);
        }
        self.run_bold = false;
    }

    fn markdown(&self) -> Option<String> {
        let text = self.text.trim();
        if text.is_empty() {
            return None;
        }
        let style = self.style.to_ascii_lowercase();
        let level = style
            .strip_prefix("heading")
            .and_then(|n| n.trim().parse::<usize>().ok());
        Some(if style == "title" {
            format!("# {text}")
        } else if let Some(n) = level {
            format!("{} {text}", "#".repeat(n.clamp(1, 3)))
        } else if self.list || style.contains("listbullet") {
            format!("- {text}")
        } else if self.numbered || style.contains("listnumber") {
            format!("1. {text}")
        } else {
            text.to_string()
        })
    }
}

fn docx(bytes: &[u8]) -> Result<String, String> {
    let mut zip = open_zip(bytes)?;
    let xml =
        part(&mut zip, "word/document.xml").ok_or("no word/document.xml: not a Word document")?;
    // Which numbering ids are bulleted, so lists keep their kind.
    let bullets: std::collections::HashSet<String> = part(&mut zip, "word/numbering.xml")
        .map(|n| bullet_num_ids(&n))
        .unwrap_or_default();
    let mut reader = Reader::from_str(&xml);
    let (mut out, mut para, mut in_para) = (Vec::<String>::new(), Para::default(), false);
    let (mut table, mut row, mut cell): (Vec<Vec<String>>, Vec<String>, Option<String>) =
        (vec![], vec![], None);
    let mut depth_tbl = 0usize;
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:tbl" => depth_tbl += 1,
                b"w:tr" => row.clear(),
                b"w:tc" => cell = Some(String::new()),
                b"w:p" => {
                    para = Para::default();
                    in_para = true;
                }
                b"w:pStyle" => para.style = attr(&e, b"w:val").unwrap_or_default(),
                b"w:numId" => {
                    let id = attr(&e, b"w:val").unwrap_or_default();
                    if bullets.contains(&id) {
                        para.list = true;
                    } else if id != "0" {
                        para.numbered = true;
                    }
                }
                b"w:r" => para.run_bold = false,
                b"w:b" => {
                    para.run_bold = attr(&e, b"w:val").is_none_or(|v| v != "0" && v != "false")
                }
                b"w:t" => in_text = true,
                b"w:tab" => para.run.push(' '),
                b"w:br" => para.run.push(' '),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => para.run.push_str(&t.decode().unwrap_or_default()),
            Ok(Event::GeneralRef(r)) if in_text => {
                para.run.push_str(&entity(&r.decode().unwrap_or_default()))
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:t" => in_text = false,
                b"w:r" => {
                    let t = escape_md(&para.run);
                    para.run = t;
                    para.end_run();
                }
                b"w:p" => {
                    in_para = false;
                    if let Some(c) = cell.as_mut() {
                        if !c.is_empty() && !para.text.trim().is_empty() {
                            c.push(' ');
                        }
                        c.push_str(para.text.trim());
                    } else if let Some(md) = para.markdown() {
                        out.push(md);
                    }
                }
                b"w:tc" => row.push(cell.take().unwrap_or_default()),
                b"w:tr" => table.push(std::mem::take(&mut row)),
                b"w:tbl" => {
                    depth_tbl = depth_tbl.saturating_sub(1);
                    if depth_tbl == 0 && !table.is_empty() {
                        out.push(md_table(&std::mem::take(&mut table)));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("could not read the document: {e}")),
            _ => {}
        }
    }
    let _ = in_para;
    Ok(join_blocks(&out))
}

/// Blocks separated by a blank line, but a list's items kept together.
fn join_blocks(blocks: &[String]) -> String {
    let item = |b: &str| b.starts_with("- ") || b.starts_with("1. ");
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push_str(if item(b) && item(&blocks[i - 1]) {
                "\n"
            } else {
                "\n\n"
            });
        }
        out.push_str(b);
    }
    out + "\n"
}

/// PDF text comes out a line per text run, so a sentence breaks wherever
/// its font changes. Lines are joined back when the next one carries on
/// the sentence: it starts in lower case, with a digit, or with punctuation.
fn reflow(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            if !out.ends_with("\n\n") && !out.is_empty() {
                out.push_str("\n\n");
            }
            continue;
        }
        let first = line.chars().next().unwrap_or(' ');
        let carries_on = !out.is_empty() && !out.ends_with('\n');
        if carries_on && matches!(first, ',' | '.' | ';' | ':' | ')' | '%' | '!' | '?') {
            out.push_str(line);
        } else if carries_on && (first.is_lowercase() || first.is_ascii_digit() || first == '(') {
            out.push(' ');
            out.push_str(line);
        } else {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

/// The numbering ids whose first level is a bullet.
fn bullet_num_ids(numbering: &str) -> std::collections::HashSet<String> {
    let mut reader = Reader::from_str(numbering);
    let (mut abstract_bullets, mut current_abstract, mut level_zero) =
        (std::collections::HashSet::new(), None::<String>, false);
    let mut num_to_abstract = vec![];
    let mut current_num = None::<String>;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:abstractNum" => current_abstract = attr(&e, b"w:abstractNumId"),
                b"w:lvl" => level_zero = attr(&e, b"w:ilvl").as_deref() == Some("0"),
                b"w:numFmt" if level_zero && attr(&e, b"w:val").as_deref() == Some("bullet") => {
                    if let Some(a) = &current_abstract {
                        abstract_bullets.insert(a.clone());
                    }
                }
                b"w:num" => current_num = attr(&e, b"w:numId"),
                b"w:abstractNumId" => {
                    if let (Some(n), Some(a)) = (&current_num, attr(&e, b"w:val")) {
                        num_to_abstract.push((n.clone(), a));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    num_to_abstract
        .into_iter()
        .filter(|(_, a)| abstract_bullets.contains(a))
        .map(|(n, _)| n)
        .collect()
}

fn md_table(rows: &[Vec<String>]) -> String {
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let line = |r: &Vec<String>| {
        format!(
            "| {} |",
            (0..w)
                .map(|i| r.get(i).map(|s| s.replace('\n', " ")).unwrap_or_default())
                .collect::<Vec<_>>()
                .join(" | ")
        )
    };
    // A header row is drawn bold already: its own bold markers go.
    let header: Vec<String> = rows[0]
        .iter()
        .map(|c| {
            c.strip_prefix("**")
                .and_then(|c| c.strip_suffix("**"))
                .unwrap_or(c)
                .to_string()
        })
        .collect();
    let mut out = vec![line(&header), format!("|{}", "---|".repeat(w))];
    out.extend(rows[1..].iter().map(line));
    out.join("\n")
}

fn pptx(bytes: &[u8]) -> Result<String, String> {
    let mut zip = open_zip(bytes)?;
    let mut slides: Vec<(usize, String)> = zip
        .file_names()
        .filter_map(|n| {
            n.strip_prefix("ppt/slides/slide")
                .and_then(|r| r.strip_suffix(".xml"))
                .and_then(|d| d.parse().ok())
                .map(|i: usize| (i, n.to_string()))
        })
        .collect();
    if slides.is_empty() {
        return Err("no slides: not a PowerPoint deck".into());
    }
    slides.sort();
    let mut out = vec![];
    for (n, path) in slides.into_iter().take(MAX_PARTS) {
        let xml = part(&mut zip, &path).unwrap_or_default();
        let (title, body) = slide_text(&xml);
        out.push(match title {
            Some(t) => format!("## Slide {n}: {t}"),
            None => format!("## Slide {n}"),
        });
        out.extend(body);
    }
    Ok(out.join("\n\n") + "\n")
}

/// A slide's title (its title placeholder) and the rest as bullets.
fn slide_text(xml: &str) -> (Option<String>, Vec<String>) {
    let mut reader = Reader::from_str(xml);
    let (mut title, mut body) = (None, vec![]);
    let (mut is_title, mut level, mut para, mut in_text) = (false, 0usize, String::new(), false);
    let mut shape_paras: Vec<(usize, String)> = vec![];
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"p:sp" => {
                    is_title = false;
                    shape_paras.clear();
                }
                b"p:ph" => {
                    is_title = matches!(
                        attr(&e, b"type").as_deref(),
                        Some("title") | Some("ctrTitle")
                    )
                }
                b"a:p" => {
                    para.clear();
                    level = 0;
                }
                b"a:pPr" => level = attr(&e, b"lvl").and_then(|l| l.parse().ok()).unwrap_or(0),
                b"a:t" => in_text = true,
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => para.push_str(&t.decode().unwrap_or_default()),
            Ok(Event::GeneralRef(r)) if in_text => {
                para.push_str(&entity(&r.decode().unwrap_or_default()))
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"a:t" => in_text = false,
                b"a:p" => {
                    if !para.trim().is_empty() {
                        shape_paras.push((level, escape_md(para.trim())));
                    }
                }
                b"p:sp" => {
                    if is_title && title.is_none() {
                        title = Some(
                            shape_paras
                                .iter()
                                .map(|(_, t)| t.as_str())
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                    } else {
                        body.extend(
                            shape_paras
                                .drain(..)
                                .map(|(l, t)| format!("{}- {t}", "  ".repeat(l.min(4)))),
                        );
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    // Consecutive bullets form one list.
    (
        title,
        if body.is_empty() {
            vec![]
        } else {
            vec![body.join("\n")]
        },
    )
}

fn pdf(bytes: &[u8]) -> Result<String, String> {
    let doc =
        lopdf::Document::load_mem(bytes).map_err(|e| format!("not a PDF KodAct can read: {e}"))?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().take(MAX_PARTS).collect();
    if pages.is_empty() {
        return Err("the PDF has no pages".into());
    }
    let mut out = vec![];
    for n in pages {
        out.push(format!("## Page {n}"));
        let text = reflow(&doc.extract_text(&[n]).unwrap_or_default());
        out.push(if text.trim().is_empty() {
            "*No text on this page: it may be a scan.*".to_string()
        } else {
            escape_md(text.trim())
        });
    }
    Ok(out.join("\n\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Written by the office-documents skill's own tools (see SKILL.md):
    // the budget edited with `xlsx_tool.py set` and recalculated, the memo
    // from Markdown, the deck from an outline, the PDF exported from the memo.
    const XLSX: &[u8] = include_bytes!("../assets/docs/samples/q3-budget.xlsx");
    const DOCX: &[u8] = include_bytes!("../assets/docs/samples/q3-budget-memo.docx");
    const PPTX: &[u8] = include_bytes!("../assets/docs/samples/q3-budget.pptx");
    const PDF: &[u8] = include_bytes!("../assets/docs/samples/q3-budget.pdf");

    fn cell<'a>(s: &'a Sheet, a1: &str) -> &'a str {
        let col = a1
            .bytes()
            .take_while(u8::is_ascii_alphabetic)
            .fold(0usize, |n, b| n * 26 + (b - b'A' + 1) as usize)
            - 1;
        let row: usize = a1
            .trim_start_matches(|c: char| c.is_ascii_alphabetic())
            .parse::<usize>()
            .unwrap()
            - 1;
        &s.rows[row][col].text
    }

    #[test]
    fn reads_the_edited_workbook_with_its_computed_values() {
        let Preview::Sheets(sheets) = preview("q3-budget.xlsx", XLSX).unwrap() else {
            panic!()
        };
        assert_eq!(
            sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["Budget", "Notes"]
        );
        let b = &sheets[0];
        assert_eq!((b.total_rows, b.total_cols), (7, 4));
        assert_eq!(cell(b, "A1"), "Item");
        assert_eq!(cell(b, "C3"), "700", "the edited cell");
        assert_eq!(cell(b, "C6"), "520", "the other edited cell");
        assert_eq!(
            cell(b, "D6"),
            "+70",
            "a formula's value, from the recalc, in its format"
        );
        assert_eq!(cell(b, "B7"), "5,150");
        assert_eq!(cell(b, "C7"), "5,500");
        assert_eq!(cell(b, "D7"), "+350");
        assert!(b.rows[2][2].numeric && !b.rows[2][0].numeric);
        assert_eq!(
            cell(&sheets[1], "A1"),
            "Figures in EUR. Q3 is the forecast."
        );
    }

    #[test]
    fn reads_the_memo_as_markdown() {
        let Preview::Doc(Doc { text: md, .. }) = preview("q3-budget-memo.docx", DOCX).unwrap()
        else {
            panic!()
        };
        assert!(md.starts_with("# Q3 budget update\n"), "{md}");
        assert!(
            md.contains("The Q3 forecast is **5,500 EUR**, up 350 EUR"),
            "bold is kept: {md}"
        );
        assert!(md.contains("## What changed"));
        assert!(md.contains("- **Licences** fall to 700 EUR (from 800): the renewal was renegotiated.\n- **Travel**"), "a list, kept together: {md}");
        assert!(
            md.contains("1. Finance signs off the forecast by Friday."),
            "a numbered item: {md}"
        );
        assert!(md.contains("| Item | Q2 | Q3 | Change |"), "{md}");
        assert!(
            md.contains("| **Total** | **5,150** | **5,500** | **+350** |"),
            "{md}"
        );
    }

    #[test]
    fn reads_the_deck_slide_by_slide() {
        let Preview::Doc(Doc {
            text: md,
            slides: true,
            ..
        }) = preview("q3-budget.pptx", PPTX).unwrap()
        else {
            panic!()
        };
        assert!(md.contains("## Slide 1: Q3 budget"), "{md}");
        assert!(md.contains("## Slide 3: What changed"), "{md}");
        assert!(
            md.contains("- Licences: 700, down 100\n  - renewal renegotiated"),
            "{md}"
        );
        assert!(md.contains("## Slide 4: Next"));
    }

    #[test]
    fn reads_speaker_notes_without_the_slide_number() {
        let xml = r#"<p:notes><p:cSld><p:spTree>
            <p:sp><p:nvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr></p:sp>
            <p:sp><p:nvSpPr><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr><p:txBody>
              <a:p><a:r><a:t>Open with the 5,500 total.</a:t></a:r></a:p>
              <a:p><a:r><a:t>Then the licences &amp; travel.</a:t></a:r></a:p>
            </p:txBody></p:sp>
            <p:sp><p:nvSpPr><p:nvPr><p:ph type="sldNum"/></p:nvPr></p:nvSpPr><p:txBody>
              <a:p><a:fld><a:t>3</a:t></a:fld></a:p></p:txBody></p:sp>
        </p:spTree></p:cSld></p:notes>"#;
        assert_eq!(
            notes_text(xml),
            "Open with the 5,500 total.\nThen the licences & travel."
        );
    }

    /// With LibreOffice here, the deck's slides are drawn as they show.
    #[test]
    fn draws_the_deck_when_libreoffice_is_here() {
        if crate::office_render::soffice().is_none() {
            return;
        }
        let (slides, total) = draw("q3-budget.pptx", PPTX).unwrap();
        assert_eq!(total, 4);
        // Wider than tall: a slide, not a page.
        assert!(slides[0].width > slides[0].height);
    }

    #[test]
    fn draws_the_pdf_as_pages() {
        let Preview::Doc(Doc { pages, total, .. }) = preview("q3-budget.pdf", PDF).unwrap() else {
            panic!("not a document")
        };
        assert!(!pages.is_empty() && total == pages.len());
        // A4 or Letter at 1.5 px per point, taller than wide.
        assert!(pages[0].width > 800 && pages[0].height > pages[0].width);
    }

    #[test]
    fn reads_the_pdf_page_by_page() {
        let md = text(Kind::Pdf, PDF).unwrap();
        assert!(md.starts_with("## Page 1"), "{md}");
        assert!(md.contains("Q3 budget update"), "{md}");
        assert!(md.contains("Licences fall to 700 EUR"), "{md}");
        assert!(
            md.contains("The Q3 forecast is 5,500 EUR, up 350 EUR (6.8%) on Q2."),
            "a sentence split by bold is one again: {md}"
        );
    }

    #[test]
    fn reflows_pdf_lines_into_sentences() {
        assert_eq!(
            reflow("Title\nThe total is\n5,500\n, up 6%.\nNext line"),
            "Title\nThe total is 5,500, up 6%.\nNext line"
        );
        assert_eq!(reflow("A\n\nB"), "A\n\nB");
    }

    #[test]
    fn says_what_it_cannot_read() {
        assert!(preview("notes.txt", b"hello").is_err());
        assert!(preview("broken.xlsx", b"not a zip")
            .unwrap_err()
            .contains("not a workbook"));
        assert!(preview("broken.docx", b"not a zip")
            .unwrap_err()
            .contains("not an Office file"));
        assert!(preview("broken.pdf", b"%PDF-1.4 nothing").is_err());
        assert_eq!(Kind::of("Report.PDF"), Some(Kind::Pdf));
        assert_eq!(
            (
                column_letters(0),
                column_letters(25),
                column_letters(26),
                column_letters(51),
                column_letters(52)
            ),
            (
                "A".into(),
                "Z".into(),
                "AA".into(),
                "AZ".into(),
                "BA".into()
            )
        );
    }

    #[test]
    fn write_xlsx_edits_round_trips_numbers_strings_rows_and_cols() {
        let mut edits = CellEdits::new();
        // Edit existing numeric cell C3 (row 2, col 2) and text cell A1 (row 0, col 0)
        edits.insert((0, 2, 2), "995".into());
        edits.insert((0, 0, 0), "Category & Note".into());
        // Insert new row & new column beyond original bounds (7x4 -> 8x5)
        edits.insert((0, 7, 0), "Bonus".into());
        edits.insert((0, 7, 4), "250".into());
        // Formula on sheet 1 (Notes)
        edits.insert((1, 1, 0), "=1+2".into());

        let updated = write_xlsx_edits(XLSX, &edits).expect("write edits");
        let Preview::Sheets(sheets) = preview("q3-budget.xlsx", &updated).expect("preview updated")
        else {
            panic!()
        };
        let b = &sheets[0];
        assert_eq!(cell(b, "A1"), "Category & Note");
        assert_eq!(cell(b, "C3"), "995");
        assert_eq!(cell(b, "A8"), "Bonus");
        assert_eq!(cell(b, "E8"), "250");
        assert!(b.total_rows >= 8 && b.total_cols >= 5);

        let looks = crate::xlsx_style::read(&updated);
        assert_eq!(
            looks["Notes"].cells.get(&(1, 0)).and_then(|l| l.formula.as_deref()),
            Some("=1+2")
        );
        // Preserves bold header style on A1
        assert!(looks["Budget"].cells.get(&(0, 0)).is_some_and(|l| l.bold));
    }
}
