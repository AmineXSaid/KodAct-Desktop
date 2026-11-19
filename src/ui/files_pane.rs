//! The Files pane: the project's tree, as the VS Code extension shows it,
//! beside a viewer. Under any file open in the side panel, or the whole
//! column when none is. The tree is the `@` list's index (git's view of
//! the project, folders first); the filter keeps the paths that match and
//! the folders they sit in. A text file opens in the viewer, highlighted;
//! a document opens in the side panel above; anything else in its app.
//! The header finds, folds every folder, reads the tree again, or closes.

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::icon;
use crate::state::AppState;
use crate::theme::{ActiveTheme, Theme};

/// The most rows a filter lists; the rest are a narrower filter away.
const MAX_ROWS: usize = 400;
/// The most lines the viewer draws.
const MAX_LINES: usize = 4000;
/// The tree's width, beside the viewer.
const TREE_W: f32 = 232.;
/// Each level's indent in the tree.
const INDENT: f32 = 12.;

/// A row of the tree.
#[derive(Debug, PartialEq)]
pub struct Row {
    /// Project-relative, `/`-separated, no trailing `/`.
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub dir: bool,
    /// A folder showing what's in it.
    pub open: bool,
}

/// What each folder holds, folders first, then by name. The root is "".
pub struct Tree {
    children: BTreeMap<String, Vec<(String, bool)>>,
}

impl Tree {
    /// From the index's entries: files, and folders ending in `/`.
    pub fn new(index: &[String]) -> Tree {
        let mut children: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
        let mut seen = HashSet::new();
        for entry in index {
            let dir = entry.ends_with('/');
            let path = entry.trim_end_matches('/');
            if path.is_empty() || !seen.insert(path.to_string()) {
                continue;
            }
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
            children
                .entry(parent.to_string())
                .or_default()
                .push((name.to_string(), dir));
        }
        for list in children.values_mut() {
            list.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
        }
        Tree { children }
    }

    /// The rows to show: the open folders' contents; with a `query`, the
    /// paths that contain it and every folder above them, open.
    pub fn rows(&self, expanded: &HashSet<String>, query: &str) -> Vec<Row> {
        let query = query.trim().to_lowercase();
        let keep: Option<HashSet<String>> = (!query.is_empty()).then(|| {
            let mut keep = HashSet::new();
            for (parent, list) in &self.children {
                for (name, _) in list {
                    let path = join(parent, name);
                    if path.to_lowercase().contains(&query) {
                        let mut at = path.as_str();
                        keep.insert(path.clone());
                        while let Some((up, _)) = at.rsplit_once('/') {
                            keep.insert(up.to_string());
                            at = up;
                        }
                    }
                }
            }
            keep
        });
        let mut out = vec![];
        self.walk("", 0, expanded, keep.as_ref(), &mut out);
        out
    }

    fn walk(
        &self,
        parent: &str,
        depth: usize,
        expanded: &HashSet<String>,
        keep: Option<&HashSet<String>>,
        out: &mut Vec<Row>,
    ) {
        let Some(list) = self.children.get(parent) else {
            return;
        };
        for (name, dir) in list {
            if out.len() >= MAX_ROWS {
                return;
            }
            let path = join(parent, name);
            if keep.is_some_and(|k| !k.contains(&path)) {
                continue;
            }
            let open = *dir && (keep.is_some() || expanded.contains(&path));
            out.push(Row {
                path: path.clone(),
                name: name.clone(),
                depth,
                dir: *dir,
                open,
            });
            if open {
                self.walk(&path, depth + 1, expanded, keep, out);
            }
        }
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

pub struct FilesPane {
    state: Entity<AppState>,
    filter: Entity<InputState>,
    /// Folders open in the tree, by path.
    expanded: HashSet<String>,
    /// The tree, and the index it was built from (by address).
    tree: Option<(usize, Rc<Tree>)>,
    _subs: Vec<Subscription>,
}

impl FilesPane {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter files..."));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&filter, window, |_, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            state,
            filter,
            expanded: HashSet::new(),
            tree: None,
            _subs: subs,
        }
    }

    /// The tree for the project's index, built once per index.
    fn tree(&mut self, index: &Arc<Vec<String>>) -> Rc<Tree> {
        let key = Arc::as_ptr(index) as usize;
        match &self.tree {
            Some((k, t)) if *k == key => t.clone(),
            _ => {
                let t = Rc::new(Tree::new(index));
                self.tree = Some((key, t.clone()));
                t
            }
        }
    }

    fn tree_rows(
        &self,
        rows: Vec<Row>,
        open: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let t = cx.theme().clone();
        let hover = t.colors.hover;
        rows.into_iter()
            .enumerate()
            .map(|(i, r)| {
                let picked = open == Some(r.path.as_str());
                let glyph = super::file_icon::asset(&r.name, r.dir, &r.path);
                let chevron = if r.open {
                    "icons/chevron-down.svg"
                } else {
                    "icons/chevron-right.svg"
                };
                let (path, dir) = (r.path.clone(), r.dir);
                let row = div()
                    .id(("files-row", i))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(t.space.xs)
                    .h(t.size.menu_row - px(2.))
                    .pl(px(4. + INDENT * r.depth as f32))
                    .pr(t.space.sm)
                    .rounded(t.radius.small)
                    .cursor_pointer()
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text)
                    .child(div().flex_none().w(t.size.icon_sm).when(r.dir, |d| {
                        d.child(icon(chevron, t.size.icon_xs, t.colors.text_subtle))
                    }))
                    .child(gpui::img(glyph).flex_none().size(t.size.icon))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(r.name),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if dir {
                            if !this.expanded.remove(&path) {
                                this.expanded.insert(path.clone());
                            }
                            cx.notify();
                        } else {
                            let rel = path.clone();
                            this.state.update(cx, |s, cx| s.open_in_files(rel, cx));
                        }
                    }));
                if picked {
                    crate::theme::selected(&t, row).into_any_element()
                } else {
                    row.hover(move |d| d.bg(hover)).into_any_element()
                }
            })
            .collect()
    }
}

/// The viewer: the open file's path and close, then its lines, numbered
/// and highlighted; or how to open one.
fn viewer(
    view: Option<&(String, Option<Result<SharedString, String>>)>,
    state: &Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let Some((rel, text)) = view else {
        return div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(t.space.sm)
            .p(t.space.lg)
            .child(icon(
                "icons/folder-open.svg",
                t.size.icon_lg,
                t.colors.text_muted,
            ))
            .child(
                div()
                    .text_size(t.text.body_sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text_strong)
                    .child("Open files appear here"),
            )
            .child(
                div()
                    .max_w(px(220.))
                    .text_center()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child("Pick a file in the tree to read it here."),
            )
            .into_any_element();
    };
    let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
    let closer = state.clone();
    let head = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(t.space.sm)
        .h(t.size.hit_sm + px(4.))
        .pl(t.space.md)
        .pr(t.space.xs)
        .border_b_1()
        .border_color(t.colors.border)
        .child(
            gpui::img(super::file_icon::asset(&name, false, rel))
                .flex_none()
                .size(t.size.icon),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text)
                .child(rel.clone()),
        )
        .child(
            super::button::Button::icon("files-view-close", "icons/x.svg", "Close the file")
                .compact()
                .on_click(move |_, _, cx| closer.update(cx, |s, cx| s.close_files_view(cx))),
        );
    let body = match text {
        None => div()
            .p(t.space.md)
            .text_size(t.text.caption)
            .text_color(t.colors.text_muted)
            .child("Reading…")
            .into_any_element(),
        Some(Err(e)) => div()
            .p(t.space.md)
            .text_size(t.text.caption)
            .text_color(t.extra.danger)
            .child(e.clone())
            .into_any_element(),
        Some(Ok(text)) => {
            let lines: Vec<&str> = text.lines().take(MAX_LINES).collect();
            let shown = lines.join("\n");
            let numbers = (1..=lines.len().max(1))
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let more = text.lines().count().saturating_sub(MAX_LINES);
            div()
                .id("files-view-body")
                .size_full()
                .overflow_scroll()
                .child(
                    div()
                        .flex()
                        .py(t.space.sm)
                        .font_family(t.font_mono.clone())
                        .text_size(t.text.caption)
                        .line_height(relative(1.5))
                        .child(
                            div()
                                .flex_none()
                                .px(t.space.sm)
                                .text_right()
                                .text_color(t.colors.text_subtle)
                                .whitespace_nowrap()
                                .child(numbers),
                        )
                        .child(div().flex_none().pr(t.space.md).whitespace_nowrap().child(
                            super::markdown::code_text(&shown, super::markdown::lang_of(rel), t),
                        )),
                )
                .when(more > 0, |d| {
                    d.child(
                        div()
                            .px(t.space.md)
                            .pb(t.space.md)
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_muted)
                            .child(format!("{more} more lines not shown.")),
                    )
                })
                .into_any_element()
        }
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(head)
        .child(div().flex_1().min_h_0().child(body))
        .into_any_element()
}

impl Render for FilesPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let query = self.filter.read(cx).value().to_string();
        let s = self.state.read(cx);
        let index = s.project_file_index();
        let loading = index.is_none();
        let view = s.files_view.clone();
        let open = view.as_ref().map(|(p, _)| p.clone());
        let rows = match index {
            Some(index) => {
                let tree = self.tree(&index);
                tree.rows(&self.expanded, &query)
            }
            None => vec![],
        };
        let none = rows.is_empty();
        let rows = self.tree_rows(rows, open.as_deref(), cx);
        let (closer, refresher) = (self.state.clone(), self.state.clone());
        let filter = self.filter.clone();
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(t.space.xs)
            .h(px(40.))
            .pl(t.space.md)
            .pr(t.space.sm)
            .border_b_1()
            .border_color(t.colors.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text_strong)
                    .child("Files"),
            )
            .child(
                super::button::Button::icon("files-find", "icons/search.svg", "Filter files")
                    .compact()
                    .on_click(move |_, window, cx| filter.update(cx, |f, cx| f.focus(window, cx))),
            )
            .child(
                super::button::Button::icon(
                    "files-fold",
                    "icons/list-tree.svg",
                    "Fold every folder",
                )
                .compact()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.expanded.clear();
                    cx.notify();
                })),
            )
            .child(
                super::button::Button::icon(
                    "files-refresh",
                    "icons/refresh-cw.svg",
                    "Read the files again",
                )
                .compact()
                .on_click(move |_, _, cx| refresher.update(cx, |s, cx| s.refresh_files(cx))),
            )
            .child(
                super::button::Button::icon("files-close", "icons/x.svg", "Close Files")
                    .compact()
                    .on_click(move |_, _, cx| closer.update(cx, |s, cx| s.toggle_files(cx))),
            );
        let tree = div()
            .flex_none()
            .w(px(TREE_W))
            .h_full()
            .flex()
            .flex_col()
            .gap(t.space.xs)
            .p(t.space.sm)
            .border_l_1()
            .border_color(t.colors.border)
            .child(Input::new(&self.filter).prefix(icon(
                "icons/search.svg",
                t.size.icon_sm,
                t.colors.text_muted,
            )))
            .child(
                div()
                    .id("files-tree")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .when(none, |d| {
                        d.child(
                            div()
                                .p(t.space.sm)
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .child(if loading {
                                    "Reading the project…"
                                } else if query.trim().is_empty() {
                                    "This folder is empty."
                                } else {
                                    "No file matches."
                                }),
                        )
                    }),
            );
        let _ = window;
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if t.is_dark { t.extra.pane } else { t.colors.surface })
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(div().flex_1().min_w_0().h_full().child(viewer(
                        view.as_ref(),
                        &self.state,
                        &t,
                    )))
                    .child(tree),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Vec<String> {
        [
            "README.md",
            "src/",
            "Cargo.toml",
            "src/main.rs",
            "src/ui/",
            "src/ui/files_pane.rs",
            ".claude/",
            ".claude/rules/",
            ".claude/rules/ui.md",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}{}", "  ".repeat(r.depth), r.name))
            .collect()
    }

    #[test]
    fn folders_come_first_and_stay_shut_until_opened() {
        let tree = Tree::new(&index());
        let rows = tree.rows(&HashSet::new(), "");
        assert_eq!(names(&rows), [".claude", "src", "Cargo.toml", "README.md"]);
        let open: HashSet<String> = ["src".to_string()].into();
        assert_eq!(
            names(&tree.rows(&open, "")),
            [
                ".claude",
                "src",
                "  ui",
                "  main.rs",
                "Cargo.toml",
                "README.md"
            ]
        );
    }

    #[test]
    fn a_filter_keeps_matches_and_the_folders_above_them_open() {
        let tree = Tree::new(&index());
        let rows = tree.rows(&HashSet::new(), "PANE");
        assert_eq!(names(&rows), ["src", "  ui", "    files_pane.rs"]);
        assert!(rows[0].open && rows[1].open);
        assert!(tree.rows(&HashSet::new(), "nothing-like-it").is_empty());
    }
}
