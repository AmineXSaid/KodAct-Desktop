//! Files and images you attach to a message: how each is read, what it is,
//! and how it reaches ForgeCLI. Nothing is dropped silently:
//! - an image (PNG, JPEG, GIF, WebP, up to `IMAGE_MAX`) goes as an image
//!   block, which ForgeCLI passes to the model on either wire;
//! - a text file (UTF-8, up to `TEXT_MAX`) goes inline, in a tagged block;
//! - anything else (a PDF, a binary, a big file) goes as its path, so the
//!   model reads it with its tools. One pasted from the clipboard, with no
//!   path, is written to Forge's temp folder first.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::SharedString;
use serde_json::{json, Value};

/// The largest image sent inline (the APIs' own limit is about 5 MB).
pub const IMAGE_MAX: u64 = 5 * 1024 * 1024;
/// The largest text file sent inline; bigger ones go by path.
pub const TEXT_MAX: u64 = 256 * 1024;
/// The most files one message takes.
pub const MAX_FILES: usize = 20;
/// The largest file read at all (to classify it); bigger ones go by path.
const READ_MAX: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An image the model sees, with its media type.
    Image(&'static str),
    /// Text, sent inline.
    Text,
    /// Anything else: sent as its path.
    File,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    pub name: SharedString,
    /// Where it is on disk (None for a pasted image not yet written).
    pub path: Option<PathBuf>,
    pub kind: Kind,
    pub size: u64,
    /// An image's or a text file's bytes, as read.
    pub bytes: Option<Arc<[u8]>>,
}

impl Attachment {
    /// `2.4 MB`, `812 KB`, `90 B`.
    pub fn size_text(&self) -> String {
        size_text(self.size)
    }

    /// The Lucide icon its chip shows.
    pub fn icon(&self) -> &'static str {
        match self.kind {
            Kind::Image(_) => "icons/image.svg",
            Kind::Text => "icons/file-text.svg",
            Kind::File => "icons/paperclip.svg",
        }
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

/// What a file is, from its bytes (the name only breaks ties).
pub fn classify(bytes: &[u8], size: u64) -> Kind {
    if let Some(m) = sniff_image(bytes) {
        return if size <= IMAGE_MAX {
            Kind::Image(m)
        } else {
            Kind::File
        };
    }
    if size <= TEXT_MAX && !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok() {
        return Kind::Text;
    }
    Kind::File
}

/// Read a file to attach (blocking: call it off the UI thread).
pub fn read(path: &Path) -> Result<Attachment, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_dir() {
        return Err(format!(
            "{} is a folder; attach the files in it instead",
            path.display()
        ));
    }
    let name: SharedString = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
        .into();
    let size = meta.len();
    let bytes = if size <= READ_MAX {
        std::fs::read(path).ok()
    } else {
        None
    };
    let kind = bytes
        .as_deref()
        .map(|b| classify(b, size))
        .unwrap_or(Kind::File);
    Ok(Attachment {
        name,
        path: Some(path.to_path_buf()),
        kind,
        size,
        // Only what is sent inline is kept in memory.
        bytes: bytes
            .filter(|_| kind != Kind::File)
            .map(|b| Arc::from(b.into_boxed_slice())),
    })
}

/// An image pasted from the clipboard.
pub fn pasted_image(bytes: Vec<u8>, n: usize) -> Option<Attachment> {
    let mime = sniff_image(&bytes)?;
    let ext = mime.trim_start_matches("image/").replace("jpeg", "jpg");
    let size = bytes.len() as u64;
    Some(Attachment {
        name: format!("Pasted image {n}.{ext}").into(),
        path: None,
        kind: if size <= IMAGE_MAX {
            Kind::Image(mime)
        } else {
            Kind::File
        },
        size,
        bytes: Some(Arc::from(bytes.into_boxed_slice())),
    })
}

/// A pasted file too big to send inline is written to Forge's temp folder,
/// so it can go by path (blocking).
pub fn ensure_path(a: &mut Attachment) -> Result<(), String> {
    if a.path.is_some() {
        return Ok(());
    }
    let dir = std::env::temp_dir().join("forge-attachments");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(crate::downloads::clean_name(&a.name));
    std::fs::write(&path, a.bytes.as_deref().unwrap_or_default()).map_err(|e| e.to_string())?;
    a.path = Some(path);
    Ok(())
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

/// Standard base64, padded (for image blocks).
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

/// The content of a user message: your text, then each attachment as the
/// block it travels as.
pub fn content_blocks(text: &str, attachments: &[Attachment]) -> Value {
    let mut blocks = vec![];
    let mut by_path = vec![];
    for a in attachments {
        match (a.kind, a.bytes.as_deref()) {
            (Kind::Image(mime), Some(b)) => blocks.push(json!({ "type": "image", "source": { "type": "base64", "media_type": mime, "data": base64(b) } })),
            (Kind::Text, Some(b)) => {
                let where_ = a.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| a.name.to_string());
                blocks.push(json!({ "type": "text", "text": format!("<attached_file path=\"{where_}\">\n{}\n</attached_file>", String::from_utf8_lossy(b)) }));
            }
            _ => by_path.push(a),
        }
    }
    let mut text = text.trim().to_string();
    if !by_path.is_empty() {
        text.push_str("\n\nAttached files (read them with your tools if you need them):");
        for a in by_path {
            let p = a
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| a.name.to_string());
            text.push_str(&format!("\n- {p} ({})", a.size_text()));
        }
    }
    // Your words lead; an attachment-only message still says something.
    let text = if text.is_empty() {
        "See the attached files.".to_string()
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

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xFF, 0xFE, 0xFD]), "//79");
    }

    #[test]
    fn files_are_classified_by_their_bytes() {
        assert_eq!(classify(PNG, 12), Kind::Image("image/png"));
        assert_eq!(classify(PNG, IMAGE_MAX + 1), Kind::File);
        assert_eq!(classify(b"fn main() {}\n", 13), Kind::Text);
        assert_eq!(classify(b"%PDF-1.7\x00\x01", 10), Kind::File);
        assert_eq!(classify(b"text", TEXT_MAX + 1), Kind::File);
        assert_eq!(size_text(90), "90 B");
        assert_eq!(size_text(812 * 1024), "812 KB");
        assert_eq!(size_text(2_516_582), "2.4 MB");
    }

    #[test]
    fn every_attachment_reaches_the_model() {
        let img = Attachment {
            name: "a.png".into(),
            path: None,
            kind: Kind::Image("image/png"),
            size: 12,
            bytes: Some(Arc::from(PNG)),
        };
        let txt = Attachment {
            name: "n.md".into(),
            path: Some("/p/n.md".into()),
            kind: Kind::Text,
            size: 2,
            bytes: Some(Arc::from(&b"hi"[..])),
        };
        let pdf = Attachment {
            name: "r.pdf".into(),
            path: Some("/p/r.pdf".into()),
            kind: Kind::File,
            size: 2048,
            bytes: None,
        };
        let v = content_blocks("Look", &[img, txt, pdf]);
        let a = v.as_array().unwrap();
        assert_eq!(a.len(), 3);
        assert!(a[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Look\n\nAttached files"));
        assert!(a[0]["text"].as_str().unwrap().contains("- /p/r.pdf (2 KB)"));
        assert_eq!(a[1]["type"], "image");
        assert_eq!(a[1]["source"]["media_type"], "image/png");
        assert_eq!(
            a[2]["text"],
            "<attached_file path=\"/p/n.md\">\nhi\n</attached_file>"
        );
        // An attachment alone still has words.
        assert_eq!(
            content_blocks("  ", &[])[0]["text"],
            "See the attached files."
        );
    }

    #[test]
    fn pasted_paths_attach_only_when_they_are_all_files() {
        let me = std::env::current_exe().unwrap();
        let p = me.display().to_string();
        assert_eq!(paths_in(&p), vec![me.clone()]);
        assert_eq!(
            paths_in(&format!("file://{}", p.replace(' ', "%20"))),
            if cfg!(windows) {
                vec![]
            } else {
                vec![me.clone()]
            }
        );
        assert!(paths_in("just some words").is_empty());
        assert!(paths_in(&format!("{p}\nand words")).is_empty());
        assert!(paths_in("relative/path.rs").is_empty());
        assert_eq!(percent_decode("a%20b%2Fc%"), "a b/c%");
    }

    #[test]
    fn reading_a_real_file() {
        let dir = std::env::temp_dir().join(format!("forge-attach-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("note.txt");
        std::fs::write(&f, "hello").unwrap();
        let a = read(&f).unwrap();
        assert_eq!(
            (a.kind, a.size, a.name.as_ref()),
            (Kind::Text, 5, "note.txt")
        );
        assert!(read(&dir).is_err());
        let mut p = pasted_image(PNG.to_vec(), 2).unwrap();
        assert_eq!(p.name.as_ref(), "Pasted image 2.png");
        ensure_path(&mut p).unwrap();
        assert!(p.path.as_ref().unwrap().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
