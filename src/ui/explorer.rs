//! The side panel's Files tab, as VS Code's Explorer draws it: sections
//! with a folding header in small capitals, then a tree of 22-pixel rows,
//! each a chevron for a folder, its VS Code icon and its whole name, set in
//! from its folder by one step and a faint guide line per level.
//!
//! - **In this chat**: what KodAct wrote in this chat, in the folders its
//!   paths name (every folder open until you fold it).
//! - **The project**: the project's own tree (git's view of it, folders
//!   first), each folder shut until you open it.
//!
//! A row opens its file in this panel: a document, picture, Markdown or
//! code file as it reads; anything else in its app. Folding is kept in
//! `AppState::ui_toggles`, so it outlives the panel closing.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, SharedString};

use super::icon;
use crate::state::{AppState, FileRef};
use crate::theme::Theme;

/// A row's height, as VS Code's.
const ROW_H: f32 = 22.;
/// Each level's step in.
const STEP: f32 = 8.;
/// The chevron's column.
const CHEVRON_W: f32 = 16.;

/// What a row opens.
#[derive(Clone)]
enum Target {
    Chat(FileRef),
    Project(String),
}

/// One row of a section's tree.
struct Row {
    key: String,
    name: String,
    depth: usize,
    /// A folder, and whether it shows what it holds.
    folder: Option<bool>,
    target: Option<Target>,
    selected: bool,
}

/// The toggle keys: a chat folder folded, a project folder opened, a
/// section folded.
fn chat_folded(path: &str) -> String {
    format!("explorer:chat-folded:{path}")
}
fn project_open(path: &str) -> String {
    format!("explorer:project-open:{path}")
}
fn section_folded(name: &str) -> String {
    format!("explorer:section-folded:{name}")
}

/// The chat's files as a tree by their paths (a file with no folder sits
/// at the top). Folders first, then by name, as VS Code sorts.
fn chat_rows(files: &[FileRef], open: Option<&FileRef>, toggles: &HashSet<String>) -> Vec<Row> {
    // Folder path -> (sub-folders, files).
    let mut dirs: BTreeMap<String, (Vec<String>, Vec<FileRef>)> = BTreeMap::new();
    dirs.entry(String::new()).or_default();
    for f in files {
        let path = f
            .path
            .as_deref()
            .map(|p| p.replace('\\', "/"))
            .unwrap_or_default();
        // The folders the path names, under the project (an absolute path
        // keeps only its last folder).
        let parent = path.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let parent = if parent.starts_with('/') || parent.contains(':') {
            parent.rsplit('/').next().unwrap_or("").to_string()
        } else {
            parent
        };
        let parent = parent.trim_start_matches("./").to_string();
        // Each folder on the way, linked to its own parent.
        let mut at = String::new();
        for part in parent.split('/').filter(|p| !p.is_empty()) {
            let next = if at.is_empty() {
                part.to_string()
            } else {
                format!("{at}/{part}")
            };
            let entry = dirs.entry(at.clone()).or_default();
            if !entry.0.contains(&next) {
                entry.0.push(next.clone());
            }
            dirs.entry(next.clone()).or_default();
            at = next;
        }
        dirs.entry(at).or_default().1.push(f.clone());
    }
    let mut out = vec![];
    fn walk(
        dir: &str,
        depth: usize,
        dirs: &BTreeMap<String, (Vec<String>, Vec<FileRef>)>,
        open: Option<&FileRef>,
        toggles: &HashSet<String>,
        out: &mut Vec<Row>,
    ) {
        let Some((subs, files)) = dirs.get(dir) else { return };
        let mut subs = subs.clone();
        subs.sort_by_key(|s| s.to_lowercase());
        for s in subs {
            let shown = !toggles.contains(&chat_folded(&s));
            out.push(Row {
                key: format!("c/{s}"),
                name: s.rsplit('/').next().unwrap_or(&s).to_string(),
                depth,
                folder: Some(shown),
                target: None,
                selected: false,
            });
            if shown {
                walk(&s, depth + 1, dirs, open, toggles, out);
            }
        }
        let mut files = files.clone();
        files.sort_by_key(|f| f.name.to_lowercase());
        for f in files {
            let selected = open.is_some_and(|o| crate::doc_versions::same_file(o, &f));
            out.push(Row {
                key: format!("c/{}/{}", dir, f.name),
                name: f.name.to_string(),
                depth,
                folder: None,
                target: Some(Target::Chat(f)),
                selected,
            });
        }
    }
    walk("", 0, &dirs, open, toggles, &mut out);
    out
}

/// The project's tree: its folders shut until opened.
fn project_rows(
    index: &[String],
    open: Option<&FileRef>,
    toggles: &HashSet<String>,
    project_dir: &std::path::Path,
) -> Vec<Row> {
    let tree = super::files_pane::Tree::new(index);
    let expanded: HashSet<String> = toggles
        .iter()
        .filter_map(|k| k.strip_prefix("explorer:project-open:").map(str::to_string))
        .collect();
    let open_rel = open.and_then(|o| o.path.as_deref()).map(|p| {
        let p = std::path::Path::new(p);
        p.strip_prefix(project_dir)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    });
    tree.rows(&expanded, "")
        .into_iter()
        .map(|r| Row {
            key: format!("p/{}", r.path),
            selected: !r.dir && open_rel.as_deref() == Some(r.path.as_str()),
            target: (!r.dir).then(|| Target::Project(r.path.clone())),
            folder: r.dir.then_some(r.open),
            name: r.name,
            depth: r.depth,
        })
        .collect()
}

/// What the Files tab needs.
pub struct View<'a> {
    pub files: Vec<FileRef>,
    pub open: Option<&'a FileRef>,
    pub toggles: &'a HashSet<String>,
    /// The project's index (None while it is read), its name and folder.
    pub index: Option<Arc<Vec<String>>>,
    pub project: String,
    pub project_dir: &'a std::path::Path,
}

pub fn render(v: View, state: Entity<AppState>, t: &Theme) -> AnyElement {
    let chat_shut = v.toggles.contains(&section_folded("chat"));
    let project_shut = v.toggles.contains(&section_folded("project"));
    let mut col = div()
        .id("explorer")
        .size_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .pt(t.space.xs)
        .pb(t.space.xxl);

    col = col.child(section(
        "chat",
        "In this chat",
        Some(v.files.len()),
        chat_shut,
        &state,
        t,
    ));
    if !chat_shut {
        if v.files.is_empty() {
            col = col.child(quiet(
                "Files KodAct writes in this chat show here, in their folders.",
                t,
            ));
        }
        for r in chat_rows(&v.files, v.open, v.toggles) {
            col = col.child(row(r, &state, t));
        }
    }
    col = col.child(section("project", &v.project, None, project_shut, &state, t));
    if !project_shut {
        match &v.index {
            None => col = col.child(quiet("Reading the project…", t)),
            Some(index) if index.is_empty() => col = col.child(quiet("This folder is empty.", t)),
            Some(index) => {
                for r in project_rows(index, v.open, v.toggles, v.project_dir) {
                    col = col.child(row(r, &state, t));
                }
            }
        }
    }
    col.into_any_element()
}

/// A section's header: a chevron and its name in small capitals.
fn section(
    id: &'static str,
    name: &str,
    count: Option<usize>,
    shut: bool,
    state: &Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let st = state.clone();
    div()
        .id(SharedString::from(format!("explorer-section-{id}")))
        .flex_none()
        .h(px(ROW_H))
        .px(px(4.))
        .flex()
        .items_center()
        .gap(px(2.))
        .cursor_pointer()
        .hover(|s| s.bg(t.colors.hover))
        .child(icon(
            if shut {
                "icons/chevron-right.svg"
            } else {
                "icons/chevron-down.svg"
            },
            px(14.),
            t.colors.text,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .text_color(t.colors.text)
                .child(name.to_uppercase()),
        )
        .children(count.filter(|n| *n > 0).map(|n| {
            div()
                .flex_none()
                .px(px(6.))
                .rounded(t.radius.pill)
                .bg(t.colors.hover)
                .text_size(px(11.))
                .text_color(t.colors.text_muted)
                .child(n.to_string())
        }))
        .on_click(move |_, _, cx| {
            st.update(cx, |s, cx| s.toggle_ui(section_folded(id), None, cx))
        })
        .into_any_element()
}

fn quiet(text: &'static str, t: &Theme) -> AnyElement {
    div()
        .px(px(22.))
        .py(px(4.))
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(text)
        .into_any_element()
}

/// One row: the guides of its levels, a chevron for a folder, its icon and
/// its name.
fn row(r: Row, state: &Entity<AppState>, t: &Theme) -> AnyElement {
    let st = state.clone();
    let (folder, target, key) = (r.folder, r.target.clone(), r.key.clone());
    let path = key[2..].to_string();
    let chat = key.starts_with("c/");
    let glyph = super::file_icon::asset(&r.name, folder.is_some(), &path);
    let glyph = match folder {
        // An open folder shows its open icon, as VS Code's.
        Some(true) => super::file_icon::open_folder(glyph),
        _ => glyph,
    };
    let guides = (0..r.depth).map(|_| {
        div()
            .flex_none()
            .w(px(STEP))
            .h_full()
            .border_l_1()
            .border_color(t.colors.border)
    });
    let line = div()
        .id(SharedString::from(format!("explorer-{key}")))
        .flex_none()
        .h(px(ROW_H))
        .pl(px(8.))
        .pr(t.space.sm)
        .flex()
        .items_center()
        .cursor_pointer()
        .text_size(t.text.body_sm)
        .text_color(t.colors.text)
        .children(guides)
        .child(
            div()
                .flex_none()
                .w(px(CHEVRON_W))
                .flex()
                .justify_center()
                .children(folder.map(|shown| {
                    icon(
                        if shown {
                            "icons/chevron-down.svg"
                        } else {
                            "icons/chevron-right.svg"
                        },
                        px(14.),
                        t.colors.text_muted,
                    )
                })),
        )
        .child(gpui::img(glyph).flex_none().size(px(16.)).mr(px(6.)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(r.name),
        )
        .on_click(move |_, _, cx| {
            st.update(cx, |s, cx| match (&folder, &target) {
                (Some(_), _) if chat => s.toggle_ui(chat_folded(&path), None, cx),
                (Some(_), _) => s.toggle_ui(project_open(&path), None, cx),
                (None, Some(Target::Chat(f))) => s.open_doc(f.clone(), cx),
                (None, Some(Target::Project(rel))) => s.open_project_file(rel.clone(), cx),
                _ => {}
            })
        });
    if r.selected {
        crate::theme::selected(t, line).into_any_element()
    } else {
        line.hover(|s| s.bg(t.colors.hover)).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, path: Option<&str>) -> FileRef {
        FileRef {
            name: name.into(),
            path: path.map(Into::into),
            content: "".into(),
            bytes: None,
        }
    }

    #[test]
    fn chat_files_sit_in_their_folders_folders_first() {
        let files = vec![
            f("plan.md", Some("docs/plan.md")),
            f("a.rs", Some("src/ui/a.rs")),
            f("notes.md", None),
        ];
        let rows = chat_rows(&files, None, &HashSet::new());
        let shown: Vec<(usize, &str, bool)> =
            rows.iter().map(|r| (r.depth, r.name.as_str(), r.folder.is_some())).collect();
        assert_eq!(
            shown,
            vec![
                (0, "docs", true),
                (1, "plan.md", false),
                (0, "src", true),
                (1, "ui", true),
                (2, "a.rs", false),
                (0, "notes.md", false),
            ]
        );
    }

    #[test]
    fn a_folded_chat_folder_hides_what_it_holds() {
        let files = vec![f("plan.md", Some("docs/plan.md"))];
        let mut toggles = HashSet::new();
        toggles.insert(chat_folded("docs"));
        let rows = chat_rows(&files, None, &toggles);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].folder, Some(false));
    }
}
