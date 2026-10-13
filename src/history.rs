//! Chats kept between launches: one JSON file per chat in
//! `<config>/chats/<session id>.json`, written off the UI thread a moment
//! after a chat changes (atomically: a temp file, then a rename) and read
//! back at start.
//!
//! The file format is its own (`Saved*`), versioned, and separate from the
//! transcript model in `state.rs`, so either can change without breaking
//! the other. A chat's ForgeCLI conversation is resumed by the same id
//! (`--resume`), so a reopened chat still has its context.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::state::{Block, Chat, DiffKind, DiffLine, FileRef, Message, ToolCall, ToolKind, ToolStatus, Verdict};

/// The format version written. Files from a newer Forge are left alone.
const VERSION: u32 = 1;
/// A binary file a reply made is kept inline up to this size; larger ones
/// are read again from where they were written.
const INLINE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SavedChat {
    pub version: u32,
    pub session: String,
    /// The ForgeCLI conversation exists on disk: the next message resumes it.
    #[serde(default)]
    pub resumable: bool,
    pub project: PathBuf,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    /// Seconds since the Unix epoch.
    pub updated: u64,
    pub messages: Vec<SavedMessage>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SavedMessage {
    User { text: String, sent: u64 },
    Assistant { blocks: Vec<SavedBlock> },
    TurnEnd { seconds: f32, #[serde(default, skip_serializing_if = "Option::is_none")] usage: Option<SavedUsage> },
    Notice { text: String, error: bool },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct SavedUsage {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
    pub estimated: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SavedBlock {
    Text { text: String },
    Thinking { text: String },
    Tool {
        kind: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        status: String,
        #[serde(default)]
        output: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        diff: Vec<(char, String)>,
    },
    Permission { title: String, detail: String, verdict: String },
    File {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default)]
        content: String,
        /// Small binaries, as hex (no extra dependency for base64).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hex: Option<String>,
    },
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn time(s: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(s)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn kind_name(k: ToolKind) -> &'static str {
    match k {
        ToolKind::Read => "read",
        ToolKind::Edit => "edit",
        ToolKind::Write => "write",
        ToolKind::Bash => "bash",
        ToolKind::Search => "search",
        ToolKind::Web => "web",
        ToolKind::Other => "other",
    }
}

fn kind_of(s: &str) -> ToolKind {
    match s {
        "read" => ToolKind::Read,
        "edit" => ToolKind::Edit,
        "write" => ToolKind::Write,
        "bash" => ToolKind::Bash,
        "search" => ToolKind::Search,
        "web" => ToolKind::Web,
        _ => ToolKind::Other,
    }
}

/// A chat as written to disk. `project` is its project's folder.
pub fn save_form(chat: &Chat, project: &Path) -> SavedChat {
    let block = |b: &Block| match b {
        Block::Text(t) => SavedBlock::Text { text: t.to_string() },
        Block::Thinking { text, .. } => SavedBlock::Thinking { text: text.to_string() },
        Block::Tool(c) => SavedBlock::Tool {
            kind: kind_name(c.kind).into(),
            target: c.target.to_string(),
            note: c.note.as_ref().map(|n| n.to_string()),
            status: match c.status {
                ToolStatus::Running | ToolStatus::Failed => "failed",
                ToolStatus::Done => "done",
                ToolStatus::Declined => "declined",
            }
            .into(),
            output: c.output.to_string(),
            diff: c.diff.iter().map(|l| (match l.kind { DiffKind::Add => '+', DiffKind::Del => '-', DiffKind::Same => ' ' }, l.text.to_string())).collect(),
        },
        Block::Permission { title, detail, verdict } => SavedBlock::Permission {
            title: title.to_string(),
            detail: detail.to_string(),
            verdict: match verdict {
                // A question nobody answered is denied once the app is gone.
                Verdict::Pending | Verdict::Denied => "denied",
                Verdict::Allowed => "allowed",
                Verdict::AllowedAlways => "always",
            }
            .into(),
        },
        Block::File(f) => SavedBlock::File {
            name: f.name.to_string(),
            path: f.path.as_ref().map(|p| p.to_string()),
            content: f.content.to_string(),
            hex: f.bytes.as_ref().filter(|b| b.len() <= INLINE_BYTES).map(|b| hex(b)),
        },
    };
    SavedChat {
        version: VERSION,
        session: chat.session.clone(),
        resumable: chat.resumable,
        project: project.to_path_buf(),
        title: chat.title.to_string(),
        pinned: chat.pinned,
        updated: secs(chat.updated),
        messages: chat
            .messages
            .iter()
            .map(|m| match m {
                Message::User { text, sent } => SavedMessage::User { text: text.to_string(), sent: secs(*sent) },
                Message::Assistant { blocks } => SavedMessage::Assistant { blocks: blocks.iter().map(block).collect() },
                Message::TurnEnd { seconds, usage } => SavedMessage::TurnEnd {
                    seconds: *seconds,
                    usage: usage.map(|u| SavedUsage { input: u.input, cache_creation: u.cache_creation, cache_read: u.cache_read, output: u.output, estimated: u.estimated }),
                },
                Message::Notice { text, error } => SavedMessage::Notice { text: text.to_string(), error: *error },
            })
            .collect(),
    }
}

/// The transcript back from disk (a chat id and project id are given by
/// the caller). Binary files too big to keep inline are read from their
/// path, if it is still there (this runs off the UI thread).
pub fn messages_of(saved: &SavedChat) -> Vec<Message> {
    let block = |b: &SavedBlock| match b {
        SavedBlock::Text { text } => Block::Text(text.clone().into()),
        SavedBlock::Thinking { text } => Block::Thinking { text: text.clone().into(), open: false },
        SavedBlock::Tool { kind, target, note, status, output, diff } => Block::Tool(ToolCall {
            kind: kind_of(kind),
            target: target.clone().into(),
            note: note.clone().map(Into::into),
            status: match status.as_str() {
                "done" => ToolStatus::Done,
                "declined" => ToolStatus::Declined,
                _ => ToolStatus::Failed,
            },
            output: output.clone().into(),
            diff: diff.iter().map(|(k, t)| DiffLine { kind: match k { '+' => DiffKind::Add, '-' => DiffKind::Del, _ => DiffKind::Same }, text: t.clone().into() }).collect(),
            open: false,
            full: false,
        }),
        SavedBlock::Permission { title, detail, verdict } => Block::Permission {
            title: title.clone().into(),
            detail: detail.clone().into(),
            verdict: match verdict.as_str() {
                "allowed" => Verdict::Allowed,
                "always" => Verdict::AllowedAlways,
                _ => Verdict::Denied,
            },
        },
        SavedBlock::File { name, path, content, hex } => {
            let binary = crate::office::Kind::of(name).is_some();
            let bytes = hex.as_deref().and_then(unhex).or_else(|| if binary { path.as_ref().and_then(|p| std::fs::read(p).ok()) } else { None });
            Block::File(FileRef { name: name.clone().into(), path: path.clone().map(Into::into), content: content.clone().into(), bytes: bytes.map(|b| std::sync::Arc::from(b.into_boxed_slice())) })
        }
    };
    saved
        .messages
        .iter()
        .map(|m| match m {
            SavedMessage::User { text, sent } => Message::User { text: text.clone().into(), sent: time(*sent) },
            SavedMessage::Assistant { blocks } => Message::Assistant { blocks: blocks.iter().map(block).collect() },
            SavedMessage::TurnEnd { seconds, usage } => Message::TurnEnd {
                seconds: *seconds,
                usage: usage.map(|u| crate::context::Usage { input: u.input, cache_creation: u.cache_creation, cache_read: u.cache_read, output: u.output, estimated: u.estimated }),
            },
            SavedMessage::Notice { text, error } => Message::Notice { text: text.clone().into(), error: *error },
        })
        .collect()
}

pub fn updated_of(saved: &SavedChat) -> SystemTime {
    time(saved.updated)
}

/// Where chats are kept: `FORGE_HISTORY_DIR`, else `<config>/chats`.
pub fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FORGE_HISTORY_DIR") {
        return Some(PathBuf::from(d));
    }
    crate::profile::dir().map(|d| d.join("chats"))
}

/// Whether `id` is a session id Forge made (and so a safe file name).
pub fn valid_id(id: &str) -> bool {
    id.len() == 36 && id.chars().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() })
}

/// Every saved chat, newest first. Unreadable or newer-format files are skipped.
pub fn load_all(dir: &Path) -> Vec<SavedChat> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<SavedChat> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| std::fs::read_to_string(&p).ok())
        .filter_map(|t| serde_json::from_str::<SavedChat>(&t).ok())
        .filter(|c| c.version <= VERSION && valid_id(&c.session))
        .collect();
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    out
}

/// Write one chat (a temp file, then a rename, so a crash never leaves half a file).
pub fn save(dir: &Path, chat: &SavedChat) -> std::io::Result<()> {
    if !valid_id(&chat.session) {
        return Err(std::io::Error::other("bad session id"));
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", chat.session));
    let tmp = dir.join(format!("{}.json.tmp", chat.session));
    std::fs::write(&tmp, serde_json::to_vec(chat).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, path)
}

pub fn remove(dir: &Path, session: &str) {
    if valid_id(session) {
        let _ = std::fs::remove_file(dir.join(format!("{session}.json")));
    }
}

/// A new session id: a random (version 4) UUID, from the OS's random hash
/// keys and the clock, so no extra dependency is needed.
pub fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut words = [0u64; 2];
    for (i, w) in words.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.write_usize(i);
        *w = h.finish();
    }
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&words[0].to_le_bytes());
    b[8..].copy_from_slice(&words[1].to_le_bytes());
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex(&b);
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Chat {
        let mut c = Chat::new(7, 1, "Fix the build".into());
        c.pinned = true;
        c.resumable = true;
        c.messages = vec![
            Message::User { text: "Fix it".into(), sent: time(1_700_000_000) },
            Message::Assistant {
                blocks: vec![
                    Block::Thinking { text: "hm".into(), open: true },
                    Block::Text("Done.".into()),
                    Block::Tool(ToolCall { kind: ToolKind::Edit, target: "a.rs".into(), note: None, status: ToolStatus::Running, output: "".into(), diff: vec![DiffLine { kind: DiffKind::Del, text: "x".into() }], open: true, full: false }),
                    Block::Permission { title: "Edit?".into(), detail: "d".into(), verdict: Verdict::Pending },
                    Block::File(FileRef { name: "q.xlsx".into(), path: None, content: "".into(), bytes: Some(std::sync::Arc::from(vec![1u8, 2, 255].into_boxed_slice())) }),
                ],
            },
            Message::TurnEnd { seconds: 2.5, usage: Some(crate::context::Usage { input: 10, output: 5, ..Default::default() }) },
            Message::Notice { text: "Interrupted.".into(), error: false },
        ];
        c
    }

    #[test]
    fn a_chat_survives_the_round_trip() {
        let c = sample();
        let saved = save_form(&c, Path::new("/work/app"));
        let json = serde_json::to_string(&saved).unwrap();
        let back: SavedChat = serde_json::from_str(&json).unwrap();
        assert_eq!(back, saved);
        assert_eq!(back.project, PathBuf::from("/work/app"));
        assert!(back.pinned && back.resumable);
        let m = messages_of(&back);
        assert_eq!(m.len(), 4);
        let Message::Assistant { blocks } = &m[1] else { panic!() };
        // A tool still running when saved was cut off; an unanswered question was denied.
        assert!(matches!(&blocks[2], Block::Tool(t) if t.status == ToolStatus::Failed && t.diff[0].kind == DiffKind::Del));
        assert!(matches!(&blocks[3], Block::Permission { verdict: Verdict::Denied, .. }));
        assert!(matches!(&blocks[4], Block::File(f) if f.bytes.as_deref() == Some(&[1u8, 2, 255][..])));
        assert!(matches!(&m[2], Message::TurnEnd { usage: Some(u), .. } if u.input == 10));
        assert!(matches!(&m[0], Message::User { sent, .. } if secs(*sent) == 1_700_000_000));
    }

    #[test]
    fn files_are_written_whole_and_read_back_newest_first() {
        let dir = std::env::temp_dir().join(format!("forge-history-{}", new_id()));
        let mut a = save_form(&sample(), Path::new("/p"));
        a.updated = 100;
        let mut b = a.clone();
        b.session = new_id();
        b.updated = 200;
        save(&dir, &a).unwrap();
        save(&dir, &b).unwrap();
        // Junk and other versions are skipped.
        std::fs::write(dir.join("junk.json"), "{not json").unwrap();
        let mut future = a.clone();
        future.session = new_id();
        future.version = VERSION + 1;
        save(&dir, &future).unwrap();
        let all = load_all(&dir);
        assert_eq!(all.iter().map(|c| c.updated).collect::<Vec<_>>(), vec![200, 100]);
        assert!(!dir.join(format!("{}.json.tmp", a.session)).exists());
        remove(&dir, &a.session);
        assert_eq!(load_all(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_uuids_and_unique() {
        let a = new_id();
        assert!(valid_id(&a), "{a}");
        assert_eq!(&a[14..15], "4");
        assert_ne!(a, new_id());
        assert!(!valid_id("../../etc/passwd-00000000000000000000"));
        assert!(save(Path::new("/tmp"), &SavedChat { session: "../x".into(), ..save_form(&sample(), Path::new("/p")) }).is_err());
    }
}
