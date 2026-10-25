//! Under a finished reply that changed files: a card, as Claude Code ends a
//! turn, "Edited 9 files", then a row per file (its icon, its name, the
//! folder when two names match, and its +/−), five at first, then "Show all
//! 9" with the whole change's size. A row jumps to its edit.

use gpui::{div, prelude::*, AnyElement, SharedString};

use super::message::ItemContext;
use crate::state::{Block, DiffKind, ToolKind, ToolStatus};
use crate::theme::Theme;

/// Rows shown before "Show all".
const FIRST: usize = 5;

/// A file the reply changed.
#[derive(Debug, PartialEq)]
pub struct Changed {
    pub path: String,
    pub adds: usize,
    pub dels: usize,
    /// Its first edit, by block.
    pub block: usize,
}

impl Changed {
    fn name(&self) -> &str {
        self.path.rsplit(['/', '\\']).next().unwrap_or(&self.path)
    }

    /// The folder it sits in, by name ("installer" for installer/README.md).
    fn folder(&self) -> Option<&str> {
        let mut parts = self.path.rsplit(['/', '\\']);
        parts.next();
        parts.next().filter(|p| !p.is_empty())
    }
}

/// Each file the reply edited or wrote, in the order it first changed.
pub fn changed(blocks: &[Block]) -> Vec<Changed> {
    let mut out: Vec<Changed> = vec![];
    for (b, block) in blocks.iter().enumerate() {
        let Block::Tool(c) = block else { continue };
        if !matches!(c.kind, ToolKind::Edit | ToolKind::Write) || c.status != ToolStatus::Done {
            continue;
        }
        let adds = c.diff.iter().filter(|l| l.kind == DiffKind::Add).count();
        let dels = c.diff.iter().filter(|l| l.kind == DiffKind::Del).count();
        match out.iter_mut().find(|f| f.path == c.target.as_ref()) {
            Some(f) => {
                f.adds += adds;
                f.dels += dels;
            }
            None => out.push(Changed {
                path: c.target.to_string(),
                adds,
                dels,
                block: b,
            }),
        }
    }
    out
}

fn counts(adds: usize, dels: usize, t: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .gap(t.space.xs)
        .font_family(t.font_mono.clone())
        .text_size(t.text.caption)
        .child(div().text_color(t.extra.success).child(format!("+{adds}")))
        .child(div().text_color(t.extra.danger).child(format!("−{dels}")))
}

/// The card for a reply's `files`; `open` shows every row.
pub fn render(files: &[Changed], open: bool, cx: &ItemContext, t: &Theme) -> AnyElement {
    let (chat, msg) = (cx.chat, cx.index);
    let hover = t.colors.hover;
    let n = files.len();
    let shown = if open { n } else { n.min(FIRST) };
    let header = if n == 1 {
        "Edited 1 file".to_string()
    } else {
        format!("Edited {n} files")
    };
    let rows = files[..shown].iter().enumerate().map(|(i, f)| {
        let name = f.name();
        // Two files of one name: the folder tells them apart.
        let twin = files.iter().filter(|g| g.name() == name).count() > 1;
        let (st, b) = (cx.state.clone(), f.block);
        div()
            .id(SharedString::from(format!("edited-{chat}-{msg}-{i}")))
            .flex()
            .items_center()
            .gap(t.space.sm)
            .min_h(t.size.hit_sm + gpui::px(2.))
            .px(t.space.md)
            .cursor_pointer()
            .hover(move |d| d.bg(hover))
            .text_size(t.text.body_sm)
            .child(
                gpui::img(super::file_icon::asset(name, false, &f.path))
                    .flex_none()
                    .size(t.size.icon),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(t.space.sm)
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_none()
                            .text_color(t.colors.text)
                            .child(name.to_string()),
                    )
                    .when_some(f.folder().filter(|_| twin), |d, dir| {
                        d.child(
                            div()
                                .min_w_0()
                                .text_ellipsis()
                                .text_color(t.colors.text_subtle)
                                .child(dir.to_string()),
                        )
                    }),
            )
            .child(counts(f.adds, f.dels, t))
            .on_click(move |_, _, cx| st.update(cx, |s, cx| s.jump_to_block(msg, b, cx)))
    });
    let (adds, dels) = files
        .iter()
        .fold((0, 0), |(a, d), f| (a + f.adds, d + f.dels));
    let st = cx.state.clone();
    div()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(t.colors.surface)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .child(
            div()
                .px(t.space.md)
                .py(t.space.sm + gpui::px(2.))
                .border_b_1()
                .border_color(t.colors.border)
                .text_size(t.text.body_sm)
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(t.colors.text_strong)
                .child(header),
        )
        .child(div().flex().flex_col().py(t.space.xs).children(rows))
        .when(n > FIRST, |d| {
            d.child(
                div()
                    .id(SharedString::from(format!("edited-all-{chat}-{msg}")))
                    .flex()
                    .items_center()
                    .px(t.space.md)
                    .py(t.space.sm + gpui::px(2.))
                    .border_t_1()
                    .border_color(t.colors.border)
                    .cursor_pointer()
                    .hover(move |d| d.bg(hover))
                    .text_size(t.text.body_sm)
                    .text_color(t.colors.text)
                    .child(div().flex_1().min_w_0().child(if open {
                        "Show less".to_string()
                    } else {
                        format!("Show all {n}")
                    }))
                    .child(counts(adds, dels, t))
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| s.toggle_edited(chat, msg, cx))
                    }),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DiffLine, Review, ToolCall};

    fn edit(path: &str, adds: usize, dels: usize, status: ToolStatus) -> Block {
        let mut diff = vec![];
        diff.extend((0..adds).map(|_| DiffLine {
            kind: DiffKind::Add,
            text: "a".into(),
        }));
        diff.extend((0..dels).map(|_| DiffLine {
            kind: DiffKind::Del,
            text: "d".into(),
        }));
        Block::Tool(ToolCall {
            kind: ToolKind::Edit,
            target: path.to_string().into(),
            note: None,
            status,
            output: "".into(),
            diff,
            open: false,
            full: false,
            review: Review::Open,
            images: vec![],
        })
    }

    #[test]
    fn a_file_counts_once_with_all_its_edits() {
        let blocks = vec![
            edit("src/a.rs", 2, 1, ToolStatus::Done),
            edit("README.md", 1, 0, ToolStatus::Done),
            edit("src/a.rs", 3, 0, ToolStatus::Done),
            edit("src/b.rs", 9, 9, ToolStatus::Failed),
        ];
        let c = changed(&blocks);
        assert_eq!(c.len(), 2);
        assert_eq!(
            (c[0].path.as_str(), c[0].adds, c[0].dels, c[0].block),
            ("src/a.rs", 5, 1, 0)
        );
        assert_eq!((c[1].path.as_str(), c[1].block), ("README.md", 1));
    }

    #[test]
    fn the_folder_is_the_parent() {
        let f = |p: &str| Changed {
            path: p.into(),
            adds: 0,
            dels: 0,
            block: 0,
        };
        assert_eq!(f("installer/README.md").folder(), Some("installer"));
        assert_eq!(f("README.md").folder(), None);
        assert_eq!(f("a/b/c.rs").name(), "c.rs");
    }
}
