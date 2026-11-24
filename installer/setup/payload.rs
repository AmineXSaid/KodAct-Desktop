//! What the setup carries: a zip appended to the end of its own .exe, then its length (8 bytes,
//! little-endian) and `KODACTPK`. `installer/pack.py` writes it. Inside, the install folder as it
//! is laid out (`KodAct.exe`, `bin/`, `python/`, `design-library/`, `libreoffice/`...) and
//! `setup.json`: the version and the bytes each part unpacks to.
//!
//! The setup .exe without its payload is the uninstaller (`Uninstall.exe`): [`stub_len`].

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const MAGIC: &[u8; 8] = b"KODACTPK";

/// `setup.json`.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct Manifest {
    pub version: String,
    /// Bytes unpacked, by part (`app`, `cli`, `python`, `design`, `libreoffice`).
    #[serde(default)]
    pub sizes: std::collections::BTreeMap<String, u64>,
}

/// The parts of an install, in the order they unpack; `libreoffice` is the one you can leave out.
pub const PARTS: [(&str, &str); 5] = [
    ("app", "KodAct desktop app"),
    ("cli", "KodAct CLI, the engine"),
    ("python", "Python and the document libraries"),
    ("design", "Design library"),
    ("libreoffice", "LibreOffice, documents as they print"),
];

/// The part a file in the payload belongs to.
pub fn part_of(name: &str) -> &'static str {
    let top = name.split('/').next().unwrap_or_default();
    match top {
        "bin" => "cli",
        "python" => "python",
        "design-library" => "design",
        "libreoffice" => "libreoffice",
        _ => "app",
    }
}

/// A part of a file, read as a file of its own (the zip inside the .exe).
pub struct Slice {
    file: File,
    start: u64,
    len: u64,
    pos: u64,
}

impl Read for Slice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.len.saturating_sub(self.pos);
        if left == 0 {
            return Ok(0);
        }
        let n = buf.len().min(left as usize);
        self.file.seek(SeekFrom::Start(self.start + self.pos))?;
        let got = self.file.read(&mut buf[..n])?;
        self.pos += got as u64;
        Ok(got)
    }
}

impl Seek for Slice {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let new = match to {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.len as i64 + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        if new < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start",
            ));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

/// Where the payload is in `exe`: (start, length). None for an .exe without one (the
/// uninstaller, or a development build).
pub fn locate(exe: &Path) -> Option<(u64, u64)> {
    let mut f = File::open(exe).ok()?;
    let size = f.metadata().ok()?.len();
    if size < 16 {
        return None;
    }
    f.seek(SeekFrom::Start(size - 16)).ok()?;
    let mut tail = [0u8; 16];
    f.read_exact(&mut tail).ok()?;
    if &tail[8..] != MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(tail[..8].try_into().ok()?);
    (len + 16 <= size).then(|| (size - 16 - len, len))
}

/// How long the .exe is without its payload: what the uninstaller is.
pub fn stub_len(exe: &Path) -> Option<u64> {
    locate(exe).map(|(start, _)| start)
}

pub struct Payload {
    zip: zip::ZipArchive<Slice>,
    pub manifest: Manifest,
}

impl Payload {
    pub fn open(exe: &Path) -> Result<Payload, String> {
        let (start, len) = locate(exe).ok_or("this setup carries nothing to install")?;
        let file = File::open(exe).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(Slice {
            file,
            start,
            len,
            pos: 0,
        })
        .map_err(|e| format!("the setup is damaged: {e}"))?;
        let manifest = match zip.by_name("setup.json") {
            Ok(mut f) => {
                let mut s = String::new();
                f.read_to_string(&mut s).map_err(|e| e.to_string())?;
                serde_json::from_str(&s).map_err(|e| format!("setup.json: {e}"))?
            }
            Err(_) => Manifest::default(),
        };
        Ok(Payload { zip, manifest })
    }

    /// Unpack into `dir` every file of the parts `wanted` takes, calling `progress` with each
    /// file's name and the bytes written so far.
    pub fn unpack(
        &mut self,
        dir: &Path,
        wanted: impl Fn(&str) -> bool,
        mut progress: impl FnMut(&str, u64),
    ) -> Result<(), String> {
        let mut done = 0u64;
        let mut buf = vec![0u8; 1 << 16];
        for i in 0..self.zip.len() {
            let mut entry = self
                .zip
                .by_index(i)
                .map_err(|e| format!("the setup is damaged: {e}"))?;
            let name = entry.name().to_string();
            if name == "setup.json" || !wanted(part_of(&name)) {
                continue;
            }
            // Never outside the install folder, whatever the archive says.
            let Some(rel) = entry.enclosed_name() else {
                continue;
            };
            let to = dir.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
                continue;
            }
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            let mut out = File::create(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            loop {
                let n = entry.read(&mut buf).map_err(|e| format!("{name}: {e}"))?;
                if n == 0 {
                    break;
                }
                std::io::Write::write_all(&mut out, &buf[..n])
                    .map_err(|e| format!("{}: {e}", to.display()))?;
                done += n as u64;
                progress(&name, done);
            }
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&to, std::fs::Permissions::from_mode(mode));
            }
        }
        Ok(())
    }
}

/// The setup's own path.
pub fn own_exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A stub, then a zip, then the trailer: found, opened, unpacked; the stub is the uninstaller.
    #[test]
    fn a_payload_after_the_exe_unpacks() {
        let d = std::env::temp_dir().join(format!("kodact-setup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut zipped = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut zipped);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("setup.json", o).unwrap();
            z.write_all(br#"{"version":"9.9","sizes":{"app":3,"cli":2}}"#)
                .unwrap();
            z.start_file("KodAct.exe", o).unwrap();
            z.write_all(b"app").unwrap();
            z.start_file("bin/kodact.exe", o).unwrap();
            z.write_all(b"cl").unwrap();
            z.start_file("libreoffice/program/soffice.exe", o).unwrap();
            z.write_all(b"lo").unwrap();
            z.finish().unwrap();
        }
        let zipped = zipped.into_inner();
        let exe = d.join("setup.exe");
        let mut f = File::create(&exe).unwrap();
        f.write_all(b"STUB-EXE-BYTES").unwrap();
        f.write_all(&zipped).unwrap();
        f.write_all(&(zipped.len() as u64).to_le_bytes()).unwrap();
        f.write_all(MAGIC).unwrap();
        drop(f);

        assert_eq!(stub_len(&exe), Some(14));
        let mut p = Payload::open(&exe).unwrap();
        assert_eq!(p.manifest.version, "9.9");
        let out = d.join("out");
        let mut seen = vec![];
        p.unpack(
            &out,
            |part| part != "libreoffice",
            |name, _| seen.push(name.to_string()),
        )
        .unwrap();
        assert_eq!(std::fs::read(out.join("KodAct.exe")).unwrap(), b"app");
        assert_eq!(std::fs::read(out.join("bin/kodact.exe")).unwrap(), b"cl");
        assert!(
            !out.join("libreoffice").exists(),
            "a part left out isn't unpacked"
        );
        assert!(locate(&out.join("KodAct.exe")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn each_file_belongs_to_a_part() {
        assert_eq!(part_of("KodAct.exe"), "app");
        assert_eq!(part_of("fonts/a.ttf"), "app");
        assert_eq!(part_of("bin/linux-x86_64/kodact"), "cli");
        assert_eq!(part_of("python/python.exe"), "python");
        assert_eq!(part_of("design-library/UPSTREAM.json"), "design");
        assert_eq!(part_of("libreoffice/program/soffice.exe"), "libreoffice");
    }
}
