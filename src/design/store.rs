//! What the design panel keeps across restarts. A chat's comments, marks,
//! page order and added pages go in its own history file (`SavedDesign`,
//! in `history::SavedChat`). A page's versions, for undo and redo, are
//! whole copies of the page, so each page gets a file of its own in the
//! config folder (`design-versions/`), read when the page is first shown.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::marks::Mark;
use crate::web_view::PagePart;

/// The most versions kept of a page; the oldest go first.
pub const VERSIONS_MAX: usize = 30;

/// A chat's design state, as written with the chat.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedDesign {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<SavedComment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<(PathBuf, Mark)>,
    /// The order the pages were dragged into.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<PathBuf>,
    /// Pages made beside the chat's own (duplicates).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<PathBuf>,
}

impl SavedDesign {
    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
            && self.marks.is_empty()
            && self.order.is_empty()
            && self.extra.is_empty()
    }
}

/// A comment on part of a page (`state::DesignComment` without its chat).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedComment {
    pub n: usize,
    pub page: PathBuf,
    pub part: PagePart,
    pub text: String,
    /// `note`, `waiting` or `sent`. One left waiting when the app closed
    /// comes back as a note: the reply it waited for is gone.
    pub sent: String,
    #[serde(default)]
    pub resolved: bool,
}

#[derive(Serialize, Deserialize)]
struct VersionsFile {
    page: PathBuf,
    at: usize,
    texts: Vec<String>,
}

/// Where a page's versions are kept: one file per page, named for its
/// path. None without a config folder.
pub fn versions_file(page: &Path) -> Option<PathBuf> {
    // FNV-1a: the same name in every build.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in page.to_string_lossy().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    Some(
        crate::profile::dir()?
            .join("design-versions")
            .join(format!("{h:016x}.json")),
    )
}

/// A page's kept versions and which one is on disk, if any were kept.
pub fn load_versions(page: &Path) -> Option<(Vec<Arc<str>>, usize)> {
    read_versions(&versions_file(page)?, page)
}

fn read_versions(file: &Path, page: &Path) -> Option<(Vec<Arc<str>>, usize)> {
    let f: VersionsFile = serde_json::from_slice(&std::fs::read(file).ok()?).ok()?;
    // Another page whose name hashed the same: not this one's.
    if f.page != page || f.texts.is_empty() {
        return None;
    }
    let at = f.at.min(f.texts.len() - 1);
    Some((f.texts.into_iter().map(Arc::from).collect(), at))
}

/// Keep a page's versions, the newest `VERSIONS_MAX` of them. Returns
/// the list and place as kept (shortened when it was longer).
pub fn save_versions(page: &Path, list: &[Arc<str>], at: usize) -> (Vec<Arc<str>>, usize) {
    let (list, at) = trimmed(list, at);
    if let Some(file) = versions_file(page) {
        let _ = write_versions(&file, page, &list, at);
    }
    (list, at)
}

fn trimmed(list: &[Arc<str>], at: usize) -> (Vec<Arc<str>>, usize) {
    let drop = list.len().saturating_sub(VERSIONS_MAX);
    (list[drop..].to_vec(), at.saturating_sub(drop))
}

fn write_versions(file: &Path, page: &Path, list: &[Arc<str>], at: usize) -> std::io::Result<()> {
    let f = VersionsFile {
        page: page.to_path_buf(),
        at,
        texts: list.iter().map(|t| t.to_string()).collect(),
    };
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(&f)?)?;
    std::fs::rename(&tmp, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_round_trip_and_keep_the_newest() {
        let dir = std::env::temp_dir().join(format!("forge-versions-{}", std::process::id()));
        let file = dir.join("v.json");
        let page = Path::new("/p/index.html");
        let list: Vec<Arc<str>> = (0..35).map(|i| Arc::from(format!("v{i}"))).collect();
        let (kept, at) = trimmed(&list, 33);
        assert_eq!((kept.len(), at, &*kept[0]), (VERSIONS_MAX, 28, "v5"));
        write_versions(&file, page, &kept, at).unwrap();
        let (back, at) = read_versions(&file, page).unwrap();
        assert_eq!((back.len(), at, &*back[28]), (VERSIONS_MAX, 28, "v33"));
        // Another page's file isn't taken for this one.
        assert!(read_versions(&file, Path::new("/p/other.html")).is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_chats_design_round_trips() {
        let d = SavedDesign {
            comments: vec![SavedComment {
                n: 1,
                page: "/p/index.html".into(),
                part: PagePart {
                    tag: "h1".into(),
                    text: "Title".into(),
                    selector: "h1".into(),
                    x: 1.0,
                    y: 2.0,
                    w: 3.0,
                    h: 4.0,
                },
                text: "Bigger".into(),
                sent: "sent".into(),
                resolved: false,
            }],
            marks: vec![(
                "/p/index.html".into(),
                Mark {
                    shape: super::super::marks::Shape::Rect,
                    points: vec![(1.0, 2.0), (3.0, 4.0)],
                    text: String::new(),
                },
            )],
            order: vec!["/p/b.html".into(), "/p/index.html".into()],
            extra: vec!["/p/index copy.html".into()],
        };
        let j = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<SavedDesign>(&j).unwrap(), d);
        assert!(serde_json::from_str::<SavedDesign>("{}")
            .unwrap()
            .is_empty());
    }
}
