//! Attachments the model can't take inline are put where its tools reach
//! them: a copy goes in the project, under `.forge/attachments/<id>/`, and
//! the message names that path. Any model, whatever it can see, can then open
//! it with Read, Grep, Bash (`tshark`, `unzip -l`, `python`…). An archive is
//! also opened next to its copy, so its files are ordinary project files at
//! once:
//!
//! - built in: zip (deflate, deflate64, bzip2, lzma, xz), tar, tar.gz/tgz,
//!   gz and 7z;
//! - anything else (rar, xz/bz2/zstd tarballs, iso, cab…): an installed
//!   7-Zip, then `unrar` for rar, then `tar` (bsdtar on Windows and macOS
//!   reads rar and 7z). If none can, the archive is still saved, and the
//!   message says why it wasn't opened.
//!
//! The file is untrusted. Its name is cut to a plain base name, an entry
//! can't leave its folder (`../`, absolute paths, links are dropped), and
//! extraction stops past [`MAX_ENTRIES`] entries or [`MAX_EXPANDED`] bytes.
//! The staging folder carries a `.gitignore` of `*`, so nothing attached
//! reaches git. Everything here blocks: run it off the UI thread.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Where copies go, inside the project.
pub const STAGING_DIR: &str = ".forge/attachments";
/// The largest file copied into the project; a bigger one goes by its own path.
pub const MAX_STAGED: u64 = 256 * 1024 * 1024;
/// Extraction limits for one archive.
pub const MAX_ENTRIES: usize = 5_000;
pub const MAX_EXPANDED: u64 = 512 * 1024 * 1024;
/// The most extracted paths named in the message.
pub const LISTED: usize = 200;
/// How long an installed extractor may run.
const TOOL_TIMEOUT: Duration = Duration::from_secs(120);

/// What came out of an archive.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Extracted {
    /// Project-relative folder it was opened into.
    pub to: String,
    /// Its files, relative to `to`, sorted (the first [`LISTED`]).
    pub files: Vec<String>,
    pub count: usize,
    /// Entries left out: unsafe names, links, unreadable or encrypted ones.
    pub skipped: usize,
}

/// A file copied into the project.
#[derive(Clone, Debug, PartialEq)]
pub struct Staged {
    /// The `<id>` folder (absolute), removed when the attachment is dropped.
    pub dir: PathBuf,
    /// The copy, relative to the project, with forward slashes.
    pub rel: String,
    pub abs: PathBuf,
}

/// A name safe to create anywhere: no folders, no control characters, none
/// of the characters Windows forbids, no leading dots.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    let cleaned: String = cleaned.chars().take(120).collect();
    if cleaned.is_empty() {
        "attachment".into()
    } else {
        cleaned
    }
}

/// A fresh `.forge/attachments/<id>/` in `project`.
fn new_dir(project: &Path) -> Result<PathBuf, String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let root = project.join(STAGING_DIR);
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("couldn't create {}: {e}", root.display()))?;
    let ignore = root.join(".gitignore");
    if !ignore.exists() {
        let _ = std::fs::write(&ignore, "*\n");
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    loop {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("{stamp}-{:x}{n}", std::process::id() & 0xfff));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("couldn't create {}: {e}", dir.display())),
        }
    }
}

fn rel(project: &Path, p: &Path) -> String {
    p.strip_prefix(project)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Copy `src` into the project as `name`.
pub fn stage_file(project: &Path, src: &Path, name: &str) -> Result<Staged, String> {
    let dir = new_dir(project)?;
    let abs = dir.join(safe_name(name));
    std::fs::copy(src, &abs).map_err(|e| {
        let _ = std::fs::remove_dir_all(&dir);
        format!("couldn't copy {} into the project: {e}", src.display())
    })?;
    Ok(Staged {
        rel: rel(project, &abs),
        abs,
        dir,
    })
}

/// Write `bytes` into the project as `name` (a pasted file).
pub fn stage_bytes(project: &Path, bytes: &[u8], name: &str) -> Result<Staged, String> {
    let dir = new_dir(project)?;
    let abs = dir.join(safe_name(name));
    std::fs::write(&abs, bytes).map_err(|e| {
        let _ = std::fs::remove_dir_all(&dir);
        format!("couldn't save {name} in the project: {e}")
    })?;
    Ok(Staged {
        rel: rel(project, &abs),
        abs,
        dir,
    })
}

/// Remove a staged copy and what was extracted from it.
pub fn discard(dir: &Path) {
    // Only ever a folder of ours.
    if dir.components().any(|c| c.as_os_str() == "attachments") {
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ---------------------------------------------------------------- archives

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    Tgz,
    Gz,
    SevenZip,
    /// Opened by an installed tool.
    External,
}

const EXTERNAL: &[&str] = &[
    ".rar", ".tar.xz", ".txz", ".tar.bz2", ".tbz2", ".tbz", ".tar.zst", ".tzst", ".xz", ".bz2",
    ".zst", ".cab", ".iso", ".lzma", ".arj", ".wim", ".lzh", ".lha", ".cpio", ".rpm", ".deb",
];

/// The archive format a name says, if it is one.
pub fn archive_format(name: &str) -> Option<Format> {
    let l = name.to_lowercase();
    if l.ends_with(".zip") {
        Some(Format::Zip)
    } else if l.ends_with(".tar.gz") || l.ends_with(".tgz") {
        Some(Format::Tgz)
    } else if l.ends_with(".tar") {
        Some(Format::Tar)
    } else if l.ends_with(".7z") {
        Some(Format::SevenZip)
    } else if EXTERNAL.iter().any(|e| l.ends_with(e)) {
        Some(Format::External)
    } else if l.ends_with(".gz") {
        Some(Format::Gz)
    } else {
        None
    }
}

/// `logs.tar.gz` → `logs`, `a.7z` → `a`.
pub fn strip_archive_ext(name: &str) -> String {
    let l = name.to_lowercase();
    let mut best = 0;
    for e in [
        ".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tgz", ".zip", ".tar", ".7z", ".gz",
    ]
    .iter()
    .chain(EXTERNAL)
    {
        if l.ends_with(e) && e.len() > best {
            best = e.len();
        }
    }
    let stem = &name[..name.len() - best];
    if stem.is_empty() {
        "archive".into()
    } else {
        stem.to_string()
    }
}

/// Where an archive entry lands under `root`, or None when it would land
/// outside it (`..`, an absolute path, a drive letter).
pub fn safe_entry(root: &Path, entry: &str) -> Option<PathBuf> {
    let norm = entry.replace('\\', "/");
    let norm = norm.trim_start_matches("./");
    if norm.is_empty() || norm.starts_with('/') {
        return None;
    }
    let mut out = root.to_path_buf();
    let mut depth = 0;
    for part in norm.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." || part.contains(':') {
            return None;
        }
        // A name that is itself a path (e.g. `C:` handled above, or a NUL).
        if Path::new(part)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return None;
        }
        out.push(part);
        depth += 1;
    }
    (depth > 0).then_some(out)
}

/// Counts what extraction wrote, and stops it past the limits.
struct Budget {
    files: Vec<String>,
    skipped: usize,
    bytes: u64,
}

impl Budget {
    fn new() -> Self {
        Budget {
            files: vec![],
            skipped: 0,
            bytes: 0,
        }
    }

    /// Write one entry's bytes to `root/name`. Ok(false) when it was skipped.
    fn write(&mut self, root: &Path, name: &str, data: &mut dyn Read) -> Result<bool, String> {
        let Some(target) = safe_entry(root, name) else {
            self.skipped += 1;
            return Ok(false);
        };
        if self.files.len() >= MAX_ENTRIES {
            return Err(format!("it has more than {MAX_ENTRIES} files"));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // A name seen twice: the first copy stays.
        let mut out = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                self.skipped += 1;
                return Ok(false);
            }
            Err(e) => return Err(e.to_string()),
        };
        let room = MAX_EXPANDED.saturating_sub(self.bytes);
        let n = std::io::copy(&mut data.take(room + 1), &mut out).map_err(|e| {
            let _ = std::fs::remove_file(&target);
            e.to_string()
        })?;
        self.bytes += n;
        if n > room {
            return Err(format!("it expands past {} MB", MAX_EXPANDED / 1_048_576));
        }
        self.files.push(rel(root, &target));
        Ok(true)
    }

    fn dir(&mut self, root: &Path, name: &str) {
        match safe_entry(root, name) {
            Some(d) => {
                let _ = std::fs::create_dir_all(d);
            }
            None => self.skipped += 1,
        }
    }

    fn done(mut self) -> (Vec<String>, usize) {
        self.files.sort();
        (self.files, self.skipped)
    }
}

fn zip(file: &Path, dest: &Path) -> Result<(Vec<String>, usize), String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("not a zip it can read ({e})"))?;
    if z.len() > MAX_ENTRIES {
        return Err(format!(
            "it has {} entries (the limit is {MAX_ENTRIES})",
            z.len()
        ));
    }
    let mut b = Budget::new();
    for i in 0..z.len() {
        let mut e = match z.by_index(i) {
            Ok(e) => e,
            // Encrypted, or a method it can't read: that entry is skipped.
            Err(_) => {
                b.skipped += 1;
                continue;
            }
        };
        let name = e.name().to_string();
        if e.is_dir() {
            b.dir(dest, &name);
        } else if e.is_symlink() {
            b.skipped += 1;
        } else {
            b.write(dest, &name, &mut e)?;
        }
    }
    Ok(b.done())
}

fn tar_from(r: impl Read, dest: &Path) -> Result<(Vec<String>, usize), String> {
    let mut a = tar::Archive::new(r);
    let mut b = Budget::new();
    for e in a
        .entries()
        .map_err(|e| format!("not a tar it can read ({e})"))?
    {
        let mut e = e.map_err(|e| format!("a damaged tar ({e})"))?;
        let name = match e.path() {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => {
                b.skipped += 1;
                continue;
            }
        };
        match e.header().entry_type() {
            tar::EntryType::Directory => b.dir(dest, &name),
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                b.write(dest, &name, &mut e)?;
            }
            // Links, devices, FIFOs: never written.
            _ => b.skipped += 1,
        }
    }
    Ok(b.done())
}

fn gz(file: &Path, dest: &Path, name: &str) -> Result<(Vec<String>, usize), String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut d = flate2::read::MultiGzDecoder::new(std::io::BufReader::new(f));
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut b = Budget::new();
    let inner = strip_archive_ext(&safe_name(name));
    b.write(dest, &inner, &mut d)?;
    Ok(b.done())
}

fn seven_zip(file: &Path, dest: &Path) -> Result<(Vec<String>, usize), String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut r = sevenz_rust2::ArchiveReader::new(f, sevenz_rust2::Password::empty())
        .map_err(|e| format!("not a 7z it can read ({e})"))?;
    let entries = &r.archive().files;
    if entries.len() > MAX_ENTRIES {
        return Err(format!(
            "it has {} entries (the limit is {MAX_ENTRIES})",
            entries.len()
        ));
    }
    let declared: u64 = entries.iter().map(|e| e.size()).sum();
    if declared > MAX_EXPANDED {
        return Err(format!("it expands past {} MB", MAX_EXPANDED / 1_048_576));
    }
    let mut b = Budget::new();
    let mut failed: Option<String> = None;
    r.for_each_entries(|e, data| {
        let name = e.name().to_string();
        if e.is_directory() {
            b.dir(dest, &name);
        } else if let Err(m) = b.write(dest, &name, data) {
            failed = Some(m);
            return Ok(false);
        }
        Ok(true)
    })
    .map_err(|e| format!("7z couldn't open it ({e})"))?;
    match failed {
        Some(m) => Err(m),
        None => Ok(b.done()),
    }
}

/// Run an installed program; Ok(false) when it isn't installed.
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<bool, String> {
    let mut c = Command::new(program);
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = match c.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(true),
            Ok(Some(_)) => return Err(format!("{program} couldn't open it")),
            Ok(None) if start.elapsed() > TOOL_TIMEOUT => {
                let _ = child.kill();
                return Err(format!("{program} took too long"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// The 7-Zip programs to try, the installed one first on Windows.
fn seven_zip_programs() -> Vec<String> {
    let mut out = vec![];
    if cfg!(windows) {
        for base in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(p) = std::env::var_os(base) {
                let exe = PathBuf::from(p).join("7-Zip").join("7z.exe");
                if exe.is_file() {
                    out.push(exe.to_string_lossy().into_owned());
                }
            }
        }
    }
    out.extend(["7z", "7zz", "7za"].map(String::from));
    out
}

/// After an installed tool ran: drop every link and special file, enforce
/// the limits, and list what is left.
fn audit(dest: &Path) -> Result<(Vec<String>, usize), String> {
    let mut files = vec![];
    let mut skipped = 0;
    let mut total = 0u64;
    let mut stack = vec![dest.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let p = e.path();
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if m.file_type().is_symlink() || !(m.is_dir() || m.is_file()) {
                let _ = std::fs::remove_file(&p);
                skipped += 1;
            } else if m.is_dir() {
                stack.push(p);
            } else {
                total += m.len();
                files.push(rel(dest, &p));
                if files.len() > MAX_ENTRIES || total > MAX_EXPANDED {
                    return Err("it expands past the size or file limit".into());
                }
            }
        }
    }
    files.sort();
    Ok((files, skipped))
}

fn external(file: &Path, dest: &Path, rar: bool) -> Result<(Vec<String>, usize), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let out = {
        let mut o = std::ffi::OsString::from("-o");
        o.push(dest);
        o
    };
    let os = |s: &'static str| std::ffi::OsStr::new(s);
    let mut last = None;
    // `-p-`: a placeholder password, so an encrypted archive fails instead
    // of waiting on a prompt nobody sees.
    for p in seven_zip_programs() {
        match run(
            &p,
            &[
                os("x"),
                os("-y"),
                os("-bd"),
                os("-p-"),
                &out,
                os("--"),
                file.as_os_str(),
            ],
        ) {
            Ok(true) => return audit(dest),
            Ok(false) => {}
            Err(e) => last = Some(e),
        }
    }
    if rar {
        let mut d = dest.as_os_str().to_os_string();
        d.push(std::path::MAIN_SEPARATOR_STR);
        match run(
            "unrar",
            &[os("x"), os("-y"), os("-p-"), file.as_os_str(), &d],
        ) {
            Ok(true) => return audit(dest),
            Ok(false) => {}
            Err(e) => last = Some(e),
        }
    }
    match run(
        "tar",
        &[os("-xf"), file.as_os_str(), os("-C"), dest.as_os_str()],
    ) {
        Ok(true) => return audit(dest),
        Ok(false) => {}
        Err(e) => last = Some(e),
    }
    Err(match last {
        Some(e) => format!("{e} (installing 7-Zip opens most formats)"),
        None => "no program to open it is installed (7-Zip opens most formats)".into(),
    })
}

/// Open the archive `file` (saved as `name`) into `dest`, project-relative
/// as `to`. On failure `dest` is removed; the archive itself stays.
pub fn extract(project: &Path, file: &Path, name: &str) -> Result<Extracted, String> {
    let format = archive_format(name).ok_or("not an archive")?;
    let dest = file.with_file_name(safe_name(&strip_archive_ext(name)));
    let dest = if dest == file {
        file.with_extension("d")
    } else {
        dest
    };
    let result = match format {
        Format::Zip => zip(file, &dest),
        Format::Tar => std::fs::File::open(file)
            .map_err(|e| e.to_string())
            .and_then(|f| tar_from(std::io::BufReader::new(f), &dest)),
        Format::Tgz => std::fs::File::open(file)
            .map_err(|e| e.to_string())
            .and_then(|f| {
                tar_from(
                    flate2::read::MultiGzDecoder::new(std::io::BufReader::new(f)),
                    &dest,
                )
            }),
        Format::Gz => gz(file, &dest, name),
        Format::SevenZip => seven_zip(file, &dest),
        Format::External => external(file, &dest, name.to_lowercase().ends_with(".rar")),
    };
    let (mut files, mut skipped) = match result {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dest);
            return Err(e);
        }
    };
    // A compressed tarball comes out of 7-Zip as one .tar: open that too,
    // so the files themselves are there.
    if files.len() == 1 && files[0].to_lowercase().ends_with(".tar") {
        let inner = dest.join(&files[0]);
        if let Ok(f) = std::fs::File::open(&inner) {
            let tmp = dest.with_extension("tar-open");
            if let Ok((f2, s2)) = tar_from(std::io::BufReader::new(f), &tmp) {
                let _ = std::fs::remove_dir_all(&dest);
                if std::fs::rename(&tmp, &dest).is_ok() {
                    files = f2;
                    skipped += s2;
                }
            } else {
                let _ = std::fs::remove_dir_all(&tmp);
            }
        }
    }
    let _ = std::fs::create_dir_all(&dest);
    let count = files.len();
    files.truncate(LISTED);
    Ok(Extracted {
        to: rel(project, &dest),
        files,
        count,
        skipped,
    })
}

/// For the message: what kind of file a name is, when that helps the model
/// pick a tool.
pub fn hint(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1.to_lowercase();
    Some(match ext.as_str() {
        "pcap" | "pcapng" | "cap" => "a packet capture: `tshark -r` or `tcpdump -r` read it, or Python's scapy",
        "sqlite" | "sqlite3" | "db" => "a SQLite database: `sqlite3` opens it",
        "doc" | "ppt" | "xls" | "rtf" | "odt" | "odp" => "an older office document: convert it (e.g. `libreoffice --headless --convert-to`) or read it with a library",
        "exe" | "dll" | "so" | "dylib" | "bin" | "elf" | "o" => "a binary: `file`, `strings`, `objdump` or `readelf` inspect it",
        "parquet" | "feather" | "arrow" => "a columnar data file: pandas or pyarrow read it",
        "ipynb" => "a Jupyter notebook: Read shows its cells",
        "har" => "an HTTP archive (JSON)",
        "evtx" => "a Windows event log",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => "audio: `ffprobe` describes it",
        "mp4" | "mov" | "mkv" | "webm" | "avi" => "a video: `ffprobe` describes it",
        "apk" | "jar" | "whl" | "nupkg" | "vsix" => "a package (a zip inside): `unzip -l` lists it",
        "dmp" | "core" => "a crash dump",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("forge-stage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\x\\a<b>.txt"), "a_b_.txt");
        assert_eq!(safe_name("..."), "attachment");
        assert_eq!(safe_name(".env"), "env");
        assert_eq!(safe_name("eth spy.pcapng"), "eth spy.pcapng");
    }

    #[test]
    fn entries_cannot_leave_their_folder() {
        let r = Path::new("/root/x");
        assert_eq!(
            safe_entry(r, "a/b.txt"),
            Some(PathBuf::from("/root/x/a/b.txt"))
        );
        assert_eq!(
            safe_entry(r, "./a.txt"),
            Some(PathBuf::from("/root/x/a.txt"))
        );
        assert_eq!(safe_entry(r, "../evil"), None);
        assert_eq!(safe_entry(r, "a/../../evil"), None);
        assert_eq!(safe_entry(r, "/etc/passwd"), None);
        assert_eq!(safe_entry(r, "C:/Windows/x"), None);
        assert_eq!(safe_entry(r, "a\\..\\..\\evil"), None);
        assert_eq!(safe_entry(r, ""), None);
    }

    #[test]
    fn formats_come_from_the_name() {
        assert_eq!(archive_format("a.ZIP"), Some(Format::Zip));
        assert_eq!(archive_format("logs.tar.gz"), Some(Format::Tgz));
        assert_eq!(archive_format("x.tgz"), Some(Format::Tgz));
        assert_eq!(archive_format("x.tar"), Some(Format::Tar));
        assert_eq!(archive_format("x.7z"), Some(Format::SevenZip));
        assert_eq!(archive_format("x.rar"), Some(Format::External));
        assert_eq!(archive_format("x.tar.xz"), Some(Format::External));
        assert_eq!(archive_format("x.log.gz"), Some(Format::Gz));
        assert_eq!(archive_format("x.pcapng"), None);
        assert_eq!(strip_archive_ext("logs.tar.gz"), "logs");
        assert_eq!(strip_archive_ext("a.7z"), "a");
        assert_eq!(strip_archive_ext("b.rar"), "b");
        assert_eq!(strip_archive_ext(".zip"), "archive");
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn a_zip_is_staged_and_opened_and_its_evil_entries_skipped() {
        let project = tmp("zip");
        let src = project.join("in.zip");
        std::fs::write(
            &src,
            zip_bytes(&[
                ("src/main.rs", b"fn main() {}"),
                ("../escape.txt", b"no"),
                ("README.md", b"hi"),
            ]),
        )
        .unwrap();
        let s = stage_file(&project, &src, "in.zip").unwrap();
        assert!(
            s.rel.starts_with(".forge/attachments/") && s.rel.ends_with("/in.zip"),
            "{}",
            s.rel
        );
        assert_eq!(
            std::fs::read_to_string(project.join(STAGING_DIR).join(".gitignore")).unwrap(),
            "*\n"
        );
        let x = extract(&project, &s.abs, "in.zip").unwrap();
        assert_eq!(x.files, ["README.md", "src/main.rs"]);
        assert_eq!((x.count, x.skipped), (2, 1));
        assert!(x.to.ends_with("/in"), "{}", x.to);
        assert_eq!(
            std::fs::read_to_string(project.join(&x.to).join("src/main.rs")).unwrap(),
            "fn main() {}"
        );
        assert!(!project
            .join(".forge/attachments")
            .join("escape.txt")
            .exists());
        discard(&s.dir);
        assert!(!s.dir.exists());
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn a_tarball_and_a_gz_are_opened() {
        let project = tmp("tgz");
        let mut t = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut h = tar::Header::new_gnu();
        h.set_size(5);
        h.set_mode(0o644);
        h.set_cksum();
        t.append_data(&mut h, "logs/app.log", &b"hello"[..])
            .unwrap();
        let gz = t.into_inner().unwrap().finish().unwrap();
        let src = project.join("logs.tar.gz");
        std::fs::write(&src, gz).unwrap();
        let s = stage_file(&project, &src, "logs.tar.gz").unwrap();
        let x = extract(&project, &s.abs, "logs.tar.gz").unwrap();
        assert_eq!(x.files, ["logs/app.log"]);

        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(b"one line\n").unwrap();
        let src = project.join("app.log.gz");
        std::fs::write(&src, e.finish().unwrap()).unwrap();
        let s = stage_file(&project, &src, "app.log.gz").unwrap();
        let x = extract(&project, &s.abs, "app.log.gz").unwrap();
        assert_eq!(x.files, ["app.log"]);
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn a_7z_is_opened_natively() {
        // Made with py7zr: hello.txt and dir/inner.txt (LZMA2).
        const SEVEN: &[u8] = include_bytes!("../tests/fixtures/t.7z");
        let project = tmp("7z");
        let src = project.join("t.7z");
        std::fs::write(&src, SEVEN).unwrap();
        let s = stage_file(&project, &src, "t.7z").unwrap();
        let x = extract(&project, &s.abs, "t.7z").unwrap();
        assert_eq!(x.files, ["dir/inner.txt", "hello.txt"]);
        assert_eq!(
            std::fs::read_to_string(project.join(&x.to).join("hello.txt")).unwrap(),
            "hello 7z\n"
        );
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn a_damaged_archive_is_reported_and_leaves_no_folder() {
        let project = tmp("bad");
        let src = project.join("bad.zip");
        std::fs::write(&src, b"not a zip at all").unwrap();
        let s = stage_file(&project, &src, "bad.zip").unwrap();
        let e = extract(&project, &s.abs, "bad.zip").unwrap_err();
        assert!(e.contains("not a zip"), "{e}");
        assert!(!s.abs.with_file_name("bad").exists());
        assert!(s.abs.exists(), "the archive itself stays");
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn hints_name_a_tool() {
        assert!(hint("eth_spy.pcapng").unwrap().contains("tshark"));
        assert!(hint("x.unknownext").is_none());
    }
}
