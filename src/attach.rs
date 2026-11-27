//! Files and images you attach to a message: what each is, how it is
//! prepared, and how it reaches KodAct CLI. Any file can be attached and
//! nothing is dropped silently:
//!
//! - an image goes as an image block, scaled down first when it is big (the
//!   model sees no more than about 1568 px on the long edge, and an endpoint
//!   refuses a body of several megabytes);
//! - a text file (UTF-8, up to [`TEXT_MAX`]) goes inline, in a tagged block;
//! - a PDF goes as a document block on an Anthropic endpoint, and as its
//!   text on an OpenAI-shaped one (which has no document block);
//! - Word and PowerPoint go as their text (Markdown), a workbook (xlsx, xls,
//!   ods) as CSV, one block per sheet;
//! - an archive is opened in the project and its files listed;
//! - anything else (a packet capture, a database, a binary…) is copied into
//!   the project and named by path, with a word on what reads it.
//!
//! Every file but an inline image or text is also copied into the project
//! (`stage.rs`), so the model's own tools can open it. Preparing a file
//! blocks: run [`prepare`] off the UI thread.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::SharedString;
use serde_json::{json, Value};

use crate::stage;

/// The largest image sent as it is (the APIs' own limit is about 5 MB).
pub const IMAGE_MAX: u64 = 5 * 1024 * 1024;
/// The long edge an image is scaled down to.
pub const IMAGE_EDGE: u32 = 1568;
/// Above this, a scaled image is sent as JPEG (about 1 MB).
const IMAGE_BUDGET: usize = 1_000_000;
/// The largest picture decoded to scale it down.
const IMAGE_DECODE_MAX: u64 = 64 * 1024 * 1024;
/// The largest text file sent inline; a bigger one goes by path.
pub const TEXT_MAX: u64 = 256 * 1024;
/// The largest PDF sent as a document block (KodAct CLI's own Read limit).
pub const PDF_MAX: u64 = 32 * 1024 * 1024;
/// How much of a document's text goes in the message.
pub const DOC_TEXT_MAX: usize = 200_000;
/// The largest office file or PDF read for its text.
const DOC_READ_MAX: u64 = 64 * 1024 * 1024;
/// The most files one message takes.
pub const MAX_FILES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doc {
    Word,
    Slides,
    Sheet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An image the model sees, with its media type.
    Image(&'static str),
    /// Text, sent inline.
    Text,
    /// Text you pasted that was too long for the box: its own card, sent
    /// inline whole (the text is in `Attachment::text`, kept after sending).
    Pasted,
    Pdf,
    /// An office document, sent as its text.
    Document(Doc),
    /// An archive, opened in the project.
    Archive,
    /// Anything else: copied into the project and sent by path.
    File,
}

impl Kind {
    /// The word a chip and the message use.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Image(_) => "Image",
            Kind::Text => "Text",
            Kind::Pasted => "Pasted text",
            Kind::Pdf => "PDF",
            Kind::Document(Doc::Word) => "Document",
            Kind::Document(Doc::Slides) => "Presentation",
            Kind::Document(Doc::Sheet) => "Spreadsheet",
            Kind::Archive => "Archive",
            Kind::File => "File",
        }
    }

    /// As saved in history.
    pub fn key(self) -> String {
        match self {
            Kind::Image(m) => m.to_string(),
            Kind::Text => "text".into(),
            Kind::Pasted => "pasted".into(),
            Kind::Pdf => "pdf".into(),
            Kind::Document(Doc::Word) => "word".into(),
            Kind::Document(Doc::Slides) => "slides".into(),
            Kind::Document(Doc::Sheet) => "sheet".into(),
            Kind::Archive => "archive".into(),
            Kind::File => "file".into(),
        }
    }

    pub fn from_key(k: &str) -> Kind {
        match k {
            "text" => Kind::Text,
            "pasted" => Kind::Pasted,
            "pdf" => Kind::Pdf,
            "word" => Kind::Document(Doc::Word),
            "slides" => Kind::Document(Doc::Slides),
            "sheet" => Kind::Document(Doc::Sheet),
            "archive" => Kind::Archive,
            m => ["image/png", "image/jpeg", "image/gif", "image/webp"]
                .into_iter()
                .find(|x| *x == m)
                .map(Kind::Image)
                .unwrap_or(Kind::File),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    /// Tells a chip still being prepared from the rest.
    pub id: u64,
    pub name: SharedString,
    /// Where it was picked from (None for something pasted).
    pub path: Option<PathBuf>,
    pub kind: Kind,
    pub size: u64,
    /// What travels inline: an image's (scaled) bytes, a text file's, a
    /// PDF's for its document block. Dropped once sent, except an image's.
    pub bytes: Option<Arc<[u8]>>,
    /// A document's text: Markdown, CSV sheets, a PDF's text.
    pub text: Option<Arc<str>>,
    /// Some of the text was cut to fit.
    pub cut: bool,
    /// The copy in the project, relative to it.
    pub saved_at: Option<String>,
    /// The `.kodact/attachments/<id>` folder, removed if the chip is.
    pub staged_dir: Option<PathBuf>,
    /// What an archive held.
    pub extracted: Option<Arc<stage::Extracted>>,
    /// Something the model (and you) should know: an archive that didn't
    /// open, a picture that was scaled down, a file too big to copy.
    pub note: Option<String>,
    /// Still being read, copied or opened.
    pub pending: bool,
    /// A picture edited in the editor: the bytes before any edit, so Reset
    /// and reopening never lose quality (`bytes` holds the rendered result).
    pub original: Option<Arc<[u8]>>,
    /// The edits made to `original`, replayed to give `bytes` (`draw.rs`).
    pub edits: Vec<crate::draw::EditOp>,
}

impl Default for Attachment {
    fn default() -> Self {
        Attachment {
            id: 0,
            name: SharedString::default(),
            path: None,
            kind: Kind::File,
            size: 0,
            bytes: None,
            text: None,
            cut: false,
            saved_at: None,
            staged_dir: None,
            extracted: None,
            note: None,
            pending: false,
            original: None,
            edits: Vec::new(),
        }
    }
}

impl Attachment {
    /// The word on a text card: whose words they are.
    pub fn badge(&self) -> &'static str {
        match self.name.as_ref() {
            "KodAct's reply" => "Reply",
            "Your message" => "Message",
            _ => "Pasted",
        }
    }

    /// `2.4 MB`, `812 KB`, `90 B`.
    pub fn size_text(&self) -> String {
        size_text(self.size)
    }

    /// The Lucide icon its chip shows.
    pub fn icon(&self) -> &'static str {
        match self.kind {
            Kind::Image(_) => "icons/image.svg",
            Kind::Text | Kind::Pasted => "icons/file-text.svg",
            Kind::Pdf | Kind::Document(Doc::Word) => "icons/file-type.svg",
            Kind::Document(Doc::Slides) => "icons/presentation.svg",
            Kind::Document(Doc::Sheet) => "icons/file-spreadsheet.svg",
            Kind::Archive => "icons/file-archive.svg",
            Kind::File => match stage::hint(&self.name) {
                Some(h) if h.starts_with("a packet") => "icons/network.svg",
                Some(h) if h.starts_with("a SQLite") => "icons/database.svg",
                _ => "icons/file.svg",
            },
        }
    }

    /// The chip's second line: "Spreadsheet · 3 sheets · 48 KB".
    pub fn summary(&self) -> String {
        if self.pending {
            return format!("Preparing… · {}", self.size_text());
        }
        let what = match (self.kind, &self.extracted) {
            (Kind::Archive, Some(x)) => format!(
                "Archive · {} {}",
                x.count,
                if x.count == 1 { "file" } else { "files" }
            ),
            (Kind::Archive, None) => "Archive, not opened".into(),
            (Kind::File, _) => kind_word(&self.name),
            (Kind::Pasted, _) => {
                let n = self.text.as_deref().map(line_count).unwrap_or(0);
                return format!(
                    "Pasted text · {n} {}",
                    if n == 1 { "line" } else { "lines" }
                );
            }
            (k, _) => k.label().to_string(),
        };
        format!("{what} · {}", self.size_text())
    }

    /// The copy a sent message keeps: no bytes but an image's (a PDF's
    /// can be 32 MB). A document's text stays, so a resend still carries it.
    pub fn for_history(&self) -> Attachment {
        let mut a = self.clone();
        if !matches!(a.kind, Kind::Image(_)) {
            a.bytes = None;
        }
        a
    }
}

/// A file's kind in a word or two, from its hint ("a packet capture: …" →
/// "Packet capture"), else "File".
fn kind_word(name: &str) -> String {
    let Some(h) = stage::hint(name) else {
        return "File".into();
    };
    let h = h.split(':').next().unwrap_or(h);
    let h = h
        .strip_prefix("an ")
        .or_else(|| h.strip_prefix("a "))
        .unwrap_or(h);
    let mut c = h.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_else(|| "File".into())
}

/// How the message names a kind: "this PDF", "this spreadsheet".
fn noun(kind: Kind) -> String {
    match kind {
        Kind::Pdf => "PDF".into(),
        k => k.label().to_lowercase(),
    }
}

pub fn size_text(n: u64) -> String {
    match n {
        0..=1023 => format!("{n} B"),
        1024..=1_048_575 => format!("{} KB", (n + 512) / 1024),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.),
    }
}

/// An image's media type from its first bytes.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn is_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// A picture within [`IMAGE_EDGE`] and the byte budget: scaled down and,
/// when still big, re-encoded as JPEG. GIFs stay as they are (animation).
/// None when it can't be decoded.
pub fn shrink(mime: &'static str, bytes: &[u8]) -> Option<(&'static str, Vec<u8>, bool)> {
    use image::GenericImageView as _;
    if mime == "image/gif" {
        return (bytes.len() as u64 <= IMAGE_MAX).then(|| (mime, bytes.to_vec(), false));
    }
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = img.dimensions();
    let long = w.max(h);
    if long <= IMAGE_EDGE && bytes.len() <= IMAGE_BUDGET {
        return Some((mime, bytes.to_vec(), false));
    }
    let img = if long > IMAGE_EDGE {
        img.resize(
            IMAGE_EDGE,
            IMAGE_EDGE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let encode = |fmt: image::ImageFormat, img: &image::DynamicImage| {
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, fmt).ok().map(|_| out.into_inner())
    };
    // A screenshot stays sharp as PNG when that fits; a photo goes JPEG.
    if mime == "image/png" {
        if let Some(png) = encode(image::ImageFormat::Png, &img).filter(|p| p.len() <= IMAGE_BUDGET)
        {
            return Some(("image/png", png, true));
        }
    }
    let rgb = image::DynamicImage::ImageRgb8(img.to_rgb8());
    let mut out = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    enc.encode_image(&rgb).ok()?;
    Some(("image/jpeg", out, true))
}

fn name_of(path: &Path) -> SharedString {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
        .into()
}

fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

/// The text a document carries, cut to [`DOC_TEXT_MAX`].
fn document_text(kind: Kind, name: &str, bytes: &[u8]) -> Result<(String, bool), String> {
    let (text, cut) = match kind {
        Kind::Document(Doc::Sheet) => {
            let (sheets, cut) = crate::office::sheets_csv(bytes, DOC_TEXT_MAX)?;
            let mut s = String::new();
            for (sheet, csv) in sheets {
                s.push_str(&format!(
                    "<sheet name=\"{}\">\n{csv}</sheet>\n",
                    sheet.replace('"', "'")
                ));
            }
            (s, cut)
        }
        _ => {
            let k = crate::office::Kind::of(name).ok_or("not a document KodAct reads")?;
            let t = crate::office::text(k, bytes)?;
            if t.len() > DOC_TEXT_MAX {
                let mut end = DOC_TEXT_MAX;
                while !t.is_char_boundary(end) {
                    end -= 1;
                }
                (t[..end].to_string(), true)
            } else {
                (t, false)
            }
        }
    };
    Ok((text, cut))
}

/// Get a file ready to send: read it, sort it, copy it into `project`, open
/// it if it is an archive (blocking: call it off the UI thread).
pub fn prepare(path: &Path, project: &Path, id: u64) -> Result<Attachment, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_dir() {
        return Err(format!(
            "{} is a folder: type @ and its name in the message to attach it",
            path.display()
        ));
    }
    let name = name_of(path);
    let size = meta.len();
    let mut a = Attachment {
        id,
        name: name.clone(),
        path: Some(path.to_path_buf()),
        size,
        ..Default::default()
    };
    let e = ext(&name);

    // Pictures: decoded to scale them, when not too big for that.
    let head = read_head(path, 64).unwrap_or_default();
    if let Some(mime) = sniff_image(&head) {
        if size <= IMAGE_DECODE_MAX {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            if let Some((mime, out, scaled)) = shrink(mime, &bytes) {
                a.kind = Kind::Image(mime);
                if scaled {
                    a.note = Some(format!(
                        "scaled down from {} to {} to fit what the model reads",
                        size_text(size),
                        size_text(out.len() as u64)
                    ));
                }
                a.bytes = Some(Arc::from(out.into_boxed_slice()));
                return Ok(a);
            }
        }
        // Not decodable here, or huge: it goes by path like any file.
    }

    // Plain text, small enough to go inline.
    if size <= TEXT_MAX && e != "pdf" {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if is_text(&bytes) && stage::archive_format(&name).is_none() {
            a.kind = Kind::Text;
            a.bytes = Some(Arc::from(bytes.into_boxed_slice()));
            return Ok(a);
        }
    }

    // Everything else is copied into the project, if it isn't huge.
    if size > stage::MAX_STAGED {
        a.kind = Kind::File;
        a.note = Some(format!(
            "too big to copy into the project ({}); it is at its own path",
            size_text(size)
        ));
        return Ok(a);
    }
    let staged = stage::stage_file(project, path, &name)?;
    a.saved_at = Some(staged.rel.clone());
    a.staged_dir = Some(staged.dir.clone());

    let is_pdf = e == "pdf" || head.starts_with(b"%PDF-");
    let doc = match e.as_str() {
        "docx" => Some(Doc::Word),
        "pptx" => Some(Doc::Slides),
        "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" => Some(Doc::Sheet),
        _ => None,
    };
    if is_pdf {
        a.kind = Kind::Pdf;
        if size <= DOC_READ_MAX {
            let bytes = std::fs::read(&staged.abs).map_err(|e| e.to_string())?;
            match crate::office::text(crate::office::Kind::Pdf, &bytes) {
                Ok(t) if !t.trim().is_empty() => {
                    let (t, cut) = cut_text(t);
                    a.text = Some(t.into());
                    a.cut = cut;
                }
                _ => {}
            }
            if size <= PDF_MAX {
                a.bytes = Some(Arc::from(bytes.into_boxed_slice()));
            }
        }
    } else if let Some(d) = doc {
        a.kind = Kind::Document(d);
        if size <= DOC_READ_MAX {
            let bytes = std::fs::read(&staged.abs).map_err(|e| e.to_string())?;
            let doc_name = if d == Doc::Sheet {
                name.to_string()
            } else {
                format!("x.{e}")
            };
            match document_text(a.kind, &doc_name, &bytes) {
                Ok((t, cut)) => {
                    a.text = Some(t.into());
                    a.cut = cut;
                }
                Err(err) => a.note = Some(format!("couldn't read it: {err}")),
            }
        }
    } else if stage::archive_format(&name).is_some() {
        a.kind = Kind::Archive;
        match stage::extract(project, &staged.abs, &name) {
            Ok(x) => a.extracted = Some(Arc::new(x)),
            Err(err) => a.note = Some(format!("couldn't open it: {err}")),
        }
    } else {
        a.kind = Kind::File;
    }
    Ok(a)
}

fn cut_text(t: String) -> (String, bool) {
    if t.len() <= DOC_TEXT_MAX {
        return (t, false);
    }
    let mut end = DOC_TEXT_MAX;
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    (t[..end].to_string(), true)
}

fn read_head(path: &Path, n: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0; n];
    let got = f.read(&mut buf)?;
    buf.truncate(got);
    Ok(buf)
}

/// An image pasted from the clipboard. One that can't be decoded is saved
/// into the project and goes by path.
pub fn pasted_image(
    bytes: Vec<u8>,
    n: usize,
    project: &Path,
    id: u64,
) -> Result<Attachment, String> {
    let mime = sniff_image(&bytes).unwrap_or("image/png");
    let ext = mime.trim_start_matches("image/").replace("jpeg", "jpg");
    let size = bytes.len() as u64;
    let mut a = Attachment {
        id,
        name: format!("Pasted image {n}.{ext}").into(),
        size,
        ..Default::default()
    };
    if let Some((m, out, scaled)) = shrink(mime, &bytes) {
        a.kind = Kind::Image(m);
        if scaled {
            a.note = Some(format!(
                "scaled down from {} to {}",
                size_text(size),
                size_text(out.len() as u64)
            ));
        }
        a.bytes = Some(Arc::from(out.into_boxed_slice()));
        return Ok(a);
    }
    let staged = stage::stage_bytes(project, &bytes, &a.name)?;
    a.saved_at = Some(staged.rel);
    a.staged_dir = Some(staged.dir);
    Ok(a)
}

/// The files a pasted text names, when it is nothing but existing file
/// paths or `file://` URIs, one per line; empty otherwise (it's text).
pub fn paths_in(text: &str) -> Vec<PathBuf> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() || lines.len() > MAX_FILES {
        return vec![];
    }
    let mut out = vec![];
    for l in lines {
        let raw = l.trim_matches('"');
        let path = match raw.strip_prefix("file://") {
            // file:///C:/x on Windows, file:///home/x elsewhere.
            Some(rest) => PathBuf::from(percent_decode(if cfg!(windows) {
                rest.trim_start_matches('/')
            } else {
                rest
            })),
            None => PathBuf::from(raw),
        };
        if !path.is_absolute() || !path.is_file() {
            return vec![];
        }
        out.push(path);
    }
    out
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64, padded (for image and document blocks).
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn attr(s: &str) -> String {
    s.replace('"', "'")
}

/// What the model is told about an attachment that isn't an image or
/// inline text: where its copy is, and what is in it.
fn describe(a: &Attachment, as_document: bool) -> String {
    let at = a
        .saved_at
        .clone()
        .or_else(|| a.path.as_ref().map(|p| p.display().to_string()))
        .unwrap_or_else(|| a.name.to_string());
    let mut lines = vec![format!(
        "<attachment name=\"{}\" kind=\"{}\" path=\"{}\" bytes=\"{}\">",
        attr(&a.name),
        a.kind.label().to_lowercase(),
        attr(&at),
        a.size
    )];
    match a.kind {
        Kind::Pdf if as_document => lines.push(format!(
            "The user attached this PDF; it is above, as a document. A copy is at `{at}` for your tools."
        )),
        Kind::Pdf | Kind::Document(_) => match &a.text {
            Some(t) => {
                lines.push(format!(
                    "The user attached this {}. Its text{}:",
                    noun(a.kind),
                    if a.kind == Kind::Document(Doc::Sheet) { ", each sheet as CSV" } else { "" }
                ));
                lines.push(t.trim_end().to_string());
                if a.cut {
                    lines.push(format!("[Cut to fit; the whole file is at `{at}`.]"));
                }
            }
            None => lines.push(format!(
                "The user attached this {}. Its text couldn't be read here (it may be scanned or protected); the file is at `{at}`: open it with your tools.",
                noun(a.kind)
            )),
        },
        Kind::Archive => match &a.extracted {
            Some(x) => {
                lines.push(format!(
                    "The user attached an archive. It has been opened into `{}/` ({} {}{}):",
                    x.to,
                    x.count,
                    if x.count == 1 { "file" } else { "files" },
                    if x.skipped > 0 { format!(", {} unsafe or unreadable entries skipped", x.skipped) } else { String::new() }
                ));
                for f in &x.files {
                    lines.push(format!("- {}/{f}", x.to));
                }
                if x.count > x.files.len() {
                    lines.push(format!("- … and {} more", x.count - x.files.len()));
                }
                lines.push("They are ordinary files in the project now: read, search and run them like any other.".into());
            }
            None => lines.push(format!("The user attached an archive, saved at `{at}`.")),
        },
        _ => {
            lines.push("The user attached this file. It is not text a message can carry, so it was saved at the path above: inspect it with your tools.".into());
            if let Some(h) = stage::hint(&a.name) {
                lines.push(format!("It is {h}."));
            }
        }
    }
    if let Some(n) = &a.note {
        lines.push(format!("(Note: {n}.)"));
    }
    lines.push("</attachment>".into());
    lines.join("\n")
}

/// A paste longer than this many characters, or lines, leaves the box and
/// becomes a Pasted text card (as Claude's chat box does).
pub const PASTE_CHARS: usize = 1_000;
pub const PASTE_LINES: usize = 12;

/// Whether a paste is long enough to become a card.
pub fn is_long_paste(text: &str) -> bool {
    text.chars().count() > PASTE_CHARS || line_count(text) > PASTE_LINES
}

/// Lines in a text, a last line without its newline counted too.
pub fn line_count(text: &str) -> usize {
    text.lines().count()
}

/// Text you pasted, as an attachment: its card, and what the model reads.
pub fn pasted_text(text: &str, n: usize, id: u64) -> Attachment {
    Attachment {
        id,
        name: format!("Pasted text {n}").into(),
        kind: Kind::Pasted,
        size: text.len() as u64,
        text: Some(Arc::from(text)),
        ..Default::default()
    }
}

/// The content of a user message: your text, then each attachment as the
/// blocks it travels as. `documents`: the endpoint takes PDF document
/// blocks (Anthropic's wire does; an OpenAI-shaped one drops them).
pub fn content_blocks(text: &str, attachments: &[Attachment], documents: bool) -> Value {
    let mut blocks = vec![];
    for (i, a) in attachments.iter().enumerate() {
        let index = i + 1;
        match (a.kind, a.bytes.as_deref()) {
            (Kind::Image(mime), Some(b)) => blocks.push(json!({ "type": "image", "source": { "type": "base64", "media_type": mime, "data": base64(b) } })),
            (Kind::Text, Some(b)) => {
                let where_ = a.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| a.name.to_string());
                blocks.push(json!({ "type": "text", "text": format!("<attached_file path=\"{where_}\">\n{}\n</attached_file>", String::from_utf8_lossy(b)) }));
            }
            (Kind::Pasted, _) => {
                let body = a.text.as_deref().unwrap_or_default();
                blocks.push(json!({ "type": "text", "text": format!(
                    "<pasted_text index=\"{index}\" name=\"{}\" lines=\"{}\">\n{body}\n</pasted_text>",
                    a.name,
                    line_count(body)
                ) }));
            }
            (Kind::Pdf, Some(b)) if documents => {
                blocks.push(json!({ "type": "document", "source": { "type": "base64", "media_type": "application/pdf", "data": base64(b) }, "title": a.name.to_string() }));
                blocks.push(json!({ "type": "text", "text": describe(a, true) }));
            }
            _ => blocks.push(json!({ "type": "text", "text": describe(a, false) })),
        }
    }
    let text = text.trim();
    // Your words lead; an attachment-only message still says something.
    let text = if text.is_empty() {
        "See the attached files."
    } else {
        text
    };
    blocks.insert(0, json!({ "type": "text", "text": text }));
    Value::Array(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("forge-attach-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 256) as u8])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// A chat message attached to the next travels whole, numbered and
    /// named, so the model can tell which text "the attached reply" means.
    #[test]
    fn attached_messages_are_numbered_and_named() {
        let mut a = pasted_text("Line one\nLine two", 0, 1);
        a.name = "KodAct's reply".into();
        assert_eq!(a.badge(), "Reply");
        let blocks = content_blocks("shorten it", &[pasted_text("x", 1, 2), a], false);
        let text = blocks[2]["text"].as_str().unwrap();
        assert!(text.starts_with("<pasted_text index=\"2\" name=\"KodAct's reply\" lines=\"2\">"));
        assert!(text.contains("Line two"));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xFF, 0xFE, 0xFD]), "//79");
        assert_eq!(size_text(90), "90 B");
        assert_eq!(size_text(812 * 1024), "812 KB");
        assert_eq!(size_text(2_516_582), "2.4 MB");
    }

    #[test]
    fn a_big_picture_is_scaled_to_what_the_model_reads() {
        let big = png(3000, 2000);
        let (mime, out, scaled) = shrink("image/png", &big).unwrap();
        assert!(scaled);
        let back = image::load_from_memory(&out).unwrap();
        assert_eq!((back.width(), back.height()), (IMAGE_EDGE, 1045));
        assert!(out.len() <= IMAGE_BUDGET, "{mime} {}", out.len());
        // A small one is left as it is.
        let small = png(40, 30);
        assert_eq!(
            shrink("image/png", &small).unwrap(),
            ("image/png", small.clone(), false)
        );
        assert!(shrink("image/png", PNG).is_none(), "not decodable");
    }

    #[test]
    fn every_kind_of_file_is_prepared() {
        let project = tmp("prep");
        let src = tmp("prep-src");
        let w = |n: &str, b: &[u8]| {
            let p = src.join(n);
            std::fs::write(&p, b).unwrap();
            p
        };
        let text = prepare(&w("n.md", b"# hi"), &project, 1).unwrap();
        assert_eq!((text.kind, text.saved_at.is_none()), (Kind::Text, true));

        let pic = prepare(&w("p.png", &png(20, 10)), &project, 2).unwrap();
        assert_eq!(pic.kind, Kind::Image("image/png"));

        let cap = prepare(
            &w("eth_spy.pcapng", &[0x0a, 0x0d, 0x0d, 0x0a, 0, 0, 0, 0]),
            &project,
            3,
        )
        .unwrap();
        assert_eq!(cap.kind, Kind::File);
        let at = cap.saved_at.clone().unwrap();
        assert!(
            at.starts_with(".kodact/attachments/") && at.ends_with("/eth_spy.pcapng"),
            "{at}"
        );
        assert!(project.join(&at).is_file());
        assert_eq!(cap.icon(), "icons/network.svg");
        assert!(
            cap.summary().starts_with("Packet capture · "),
            "{}",
            cap.summary()
        );

        let xlsx = prepare(
            &w(
                "q3.xlsx",
                include_bytes!("../assets/docs/samples/q3-budget.xlsx"),
            ),
            &project,
            4,
        )
        .unwrap();
        assert_eq!(xlsx.kind, Kind::Document(Doc::Sheet));
        let t = xlsx.text.clone().unwrap();
        assert!(t.starts_with("<sheet name=\"Budget\">\nItem,"), "{t}");

        let docx = prepare(
            &w(
                "memo.docx",
                include_bytes!("../assets/docs/samples/q3-budget-memo.docx"),
            ),
            &project,
            5,
        )
        .unwrap();
        assert!(docx
            .text
            .as_deref()
            .unwrap()
            .starts_with("# Q3 budget update"));

        let pdf = prepare(
            &w(
                "r.pdf",
                include_bytes!("../assets/docs/samples/q3-budget.pdf"),
            ),
            &project,
            6,
        )
        .unwrap();
        assert_eq!(pdf.kind, Kind::Pdf);
        assert!(pdf.bytes.is_some() && pdf.text.as_deref().unwrap().contains("Q3"));

        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        z.start_file("a/b.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut z, b"x").unwrap();
        let zip = prepare(
            &w("bundle.zip", &z.finish().unwrap().into_inner()),
            &project,
            7,
        )
        .unwrap();
        assert_eq!(zip.kind, Kind::Archive);
        assert_eq!(zip.extracted.as_ref().unwrap().files, ["a/b.txt"]);
        assert!(
            zip.summary().starts_with("Archive · 1 file · "),
            "{}",
            zip.summary()
        );

        assert!(prepare(&src, &project, 8).unwrap_err().contains("folder"));
        let _ = std::fs::remove_dir_all(&project);
        let _ = std::fs::remove_dir_all(&src);
    }

    #[test]
    fn every_attachment_reaches_the_model() {
        let img = Attachment {
            name: "a.png".into(),
            kind: Kind::Image("image/png"),
            size: 12,
            bytes: Some(Arc::from(PNG)),
            ..Default::default()
        };
        let txt = Attachment {
            name: "n.md".into(),
            path: Some("/p/n.md".into()),
            kind: Kind::Text,
            size: 2,
            bytes: Some(Arc::from(&b"hi"[..])),
            ..Default::default()
        };
        let pdf = Attachment {
            name: "r.pdf".into(),
            kind: Kind::Pdf,
            size: 4,
            bytes: Some(Arc::from(&b"%PDF"[..])),
            text: Some("Report".into()),
            saved_at: Some(".forge/attachments/1/r.pdf".into()),
            ..Default::default()
        };
        let cap = Attachment {
            name: "x.pcap".into(),
            kind: Kind::File,
            size: 9,
            saved_at: Some(".forge/attachments/2/x.pcap".into()),
            ..Default::default()
        };

        let v = content_blocks("Look", &[img.clone(), txt, pdf.clone(), cap], true);
        let a = v.as_array().unwrap();
        assert_eq!(a[0]["text"], "Look");
        assert_eq!(a[1]["type"], "image");
        assert_eq!(
            a[2]["text"],
            "<attached_file path=\"/p/n.md\">\nhi\n</attached_file>"
        );
        assert_eq!(a[3]["type"], "document");
        assert_eq!(a[3]["source"]["media_type"], "application/pdf");
        assert!(a[4]["text"]
            .as_str()
            .unwrap()
            .contains("above, as a document"));
        let c = a[5]["text"].as_str().unwrap();
        assert!(
            c.contains("path=\".forge/attachments/2/x.pcap\"") && c.contains("tshark"),
            "{c}"
        );

        // No document blocks on an OpenAI-shaped endpoint: the PDF's text goes.
        let v = content_blocks("", &[pdf], false);
        let a = v.as_array().unwrap();
        assert_eq!(a[0]["text"], "See the attached files.");
        assert_eq!(a.len(), 2);
        assert!(a[1]["text"]
            .as_str()
            .unwrap()
            .contains("attached this PDF. Its text:\nReport"));

        // History keeps a picture, not a document's bytes.
        assert!(img.for_history().bytes.is_some());
        assert_eq!(
            Kind::from_key(&Kind::Document(Doc::Sheet).key()),
            Kind::Document(Doc::Sheet)
        );
        assert_eq!(Kind::from_key("image/webp"), Kind::Image("image/webp"));
    }

    #[test]
    fn a_long_paste_becomes_a_card_and_reaches_the_model_whole() {
        assert!(!is_long_paste(&"a".repeat(PASTE_CHARS)));
        assert!(is_long_paste(&"a".repeat(PASTE_CHARS + 1)));
        assert!(!is_long_paste(&"x\n".repeat(PASTE_LINES)));
        assert!(is_long_paste(&"x\n".repeat(PASTE_LINES + 1)));
        // The talk transcript that was taken for JavaScript: prose, sent as prose.
        let talk = "Let us now look at a couple of SDV-enabled features. We are using the DigitalAuto Playground.\n".repeat(14);
        assert!(is_long_paste(&talk));
        let a = pasted_text(&talk, 1, 7);
        assert_eq!((a.kind, a.name.as_ref()), (Kind::Pasted, "Pasted text 1"));
        assert_eq!(a.summary(), "Pasted text · 14 lines");
        let blocks = content_blocks("What is this talking about?", &[a.clone()], true);
        let sent = blocks[1]["text"].as_str().unwrap();
        assert!(
            sent.starts_with(
                "<pasted_text index=\"1\" name=\"Pasted text 1\" lines=\"14\">\nLet us now"
            ),
            "{sent}"
        );
        assert!(!sent.contains("```"), "never fenced");
        // Sent, it keeps its words (a sent message shows and reopens them).
        assert_eq!(a.for_history().text.as_deref(), Some(talk.as_str()));
        assert_eq!(Kind::from_key(&Kind::Pasted.key()), Kind::Pasted);
    }

    #[test]
    fn pasted_paths_attach_only_when_they_are_all_files() {
        let me = std::env::current_exe().unwrap();
        let p = me.display().to_string();
        assert_eq!(paths_in(&p), vec![me.clone()]);
        // A file URL as the platform writes it: file:///C:/x, file:///home/x.
        let url = format!(
            "file:///{}",
            p.replace('\\', "/")
                .trim_start_matches('/')
                .replace(' ', "%20")
        );
        assert_eq!(paths_in(&url), vec![me.clone()]);
        assert!(paths_in("just some words").is_empty());
        assert!(paths_in(&format!("{p}\nand words")).is_empty());
        assert!(paths_in("relative/path.rs").is_empty());
        assert_eq!(percent_decode("a%20b%2Fc%"), "a b/c%");
    }

    #[test]
    fn a_pasted_picture_that_wont_decode_is_saved_in_the_project() {
        let project = tmp("paste");
        let a = pasted_image(PNG.to_vec(), 2, &project, 9).unwrap();
        assert_eq!(
            (a.kind, a.name.as_ref()),
            (Kind::File, "Pasted image 2.png")
        );
        assert!(project.join(a.saved_at.unwrap()).is_file());
        let ok = pasted_image(png(10, 10), 3, &project, 10).unwrap();
        assert_eq!(ok.kind, Kind::Image("image/png"));
        let _ = std::fs::remove_dir_all(&project);
    }
}
