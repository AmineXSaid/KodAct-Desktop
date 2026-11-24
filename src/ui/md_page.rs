//! A Markdown file as a page you type on (the side panel's Editing mode), as
//! Claude Docs edits one: headings set large, lists with their bullets and
//! numbers, checklists you tick, quotes, code, tables as grids, dividers and
//! pictures. Each block is a field; what you type is written back to the
//! file as Markdown as you go (`AppState::autosave_doc`, half a second after
//! you stop), so the file on disk is always the page.
//!
//! - Enter starts a new block (a list goes on; an empty item ends the list);
//!   Shift+Enter breaks the line. Backspace at the start of a block turns it
//!   back into a paragraph, then joins it to the one above.
//! - Markdown typed at the start of a paragraph becomes what it says: `# `,
//!   `- `, `1. `, `[] `, `> `, `---`.
//! - The floating toolbar at the foot: the block's text style, a table, a
//!   picture, a checklist, a divider, "…" (bold, italic, code, link), and
//!   Comment, which hands the passage to the comment box.

use std::time::Duration;

use gpui::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, IntoElement,
    KeyDownEvent, Render, ScrollHandle, SharedString, Subscription, Task, Window,
};

use super::icon;
use crate::md_blocks::{self, Block, Kind};
use crate::state::{AppState, DocMenu};
use crate::theme::{ActiveTheme, Theme};

/// How long after the last keystroke the page is written to the file.
const SAVE_AFTER: Duration = Duration::from_millis(500);

/// One block on the page, with its field (or a table's cell fields).
struct Row {
    kind: Kind,
    field: Option<Entity<TextareaState>>,
    cells: Vec<Vec<Entity<TextareaState>>>,
}

pub struct MdPage {
    state: Entity<AppState>,
    /// The file the page was made for (`doc_versions::file_id`).
    file: Option<String>,
    rows: Vec<Row>,
    /// The Markdown the page last wrote (or read): a file that changes to
    /// anything else, KodAct rewriting it, is read afresh.
    text: String,
    /// The block you are in (its index), for the toolbar.
    focused: Option<usize>,
    scroll: ScrollHandle,
    link: Entity<InputState>,
    save: Option<Task<()>>,
    /// Focus this block's field, at its start or end, on the next frame.
    focus_next: Option<(usize, bool)>,
    /// Blocks to put in on the next frame (a picture picked in a dialog).
    pending: Vec<Block>,
    _subs: Vec<Subscription>,
    row_subs: Vec<Subscription>,
}

/// Which blocks a field may be: its size, weight and face on the page.
fn style(kind: &Kind, t: &Theme) -> (gpui::Pixels, FontWeight, bool) {
    match kind {
        Kind::Heading(1) => (px(26.), FontWeight::SEMIBOLD, false),
        Kind::Heading(2) => (px(20.), FontWeight::SEMIBOLD, false),
        Kind::Heading(_) => (px(16.), FontWeight::SEMIBOLD, false),
        Kind::Code(_) => (t.text.body_sm, FontWeight::NORMAL, true),
        _ => (t.text.body, FontWeight::NORMAL, false),
    }
}

/// The kind Enter carries on to: a list goes on, anything else is a paragraph.
fn next_kind(kind: &Kind) -> Kind {
    match kind {
        Kind::Bullet | Kind::Numbered => kind.clone(),
        Kind::Check(_) => Kind::Check(false),
        _ => Kind::Paragraph,
    }
}

/// The text styles the toolbar offers, in order.
const STYLES: &[(&str, &str)] = &[
    ("Paragraph", "icons/pilcrow.svg"),
    ("Heading 1", "icons/heading-1.svg"),
    ("Heading 2", "icons/heading-2.svg"),
    ("Heading 3", "icons/heading-3.svg"),
    ("Bulleted list", "icons/list.svg"),
    ("Numbered list", "icons/list-ordered.svg"),
    ("Checklist", "icons/square-check.svg"),
    ("Quote", "icons/text-quote.svg"),
    ("Code block", "icons/square-code.svg"),
];

fn style_kind(name: &str) -> Kind {
    match name {
        "Heading 1" => Kind::Heading(1),
        "Heading 2" => Kind::Heading(2),
        "Heading 3" => Kind::Heading(3),
        "Bulleted list" => Kind::Bullet,
        "Numbered list" => Kind::Numbered,
        "Checklist" => Kind::Check(false),
        "Quote" => Kind::Quote,
        "Code block" => Kind::Code(String::new()),
        _ => Kind::Paragraph,
    }
}

fn style_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::Heading(1) => "Heading 1",
        Kind::Heading(2) => "Heading 2",
        Kind::Heading(_) => "Heading 3",
        Kind::Bullet => "Bulleted list",
        Kind::Numbered => "Numbered list",
        Kind::Check(_) => "Checklist",
        Kind::Quote => "Quote",
        Kind::Code(_) => "Code block",
        Kind::Table(_) => "Table",
        _ => "Paragraph",
    }
}

impl MdPage {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let link = cx.new(|cx| InputState::new(window, cx).placeholder("https://"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&link, window, |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.add_link(window, cx);
                }
            }),
        ];
        Self {
            state,
            file: None,
            rows: vec![],
            text: String::new(),
            focused: None,
            scroll: ScrollHandle::new(),
            link,
            save: None,
            focus_next: None,
            pending: vec![],
            _subs: subs,
            row_subs: vec![],
        }
    }

    /// A field holding `text`, for a block of `kind`.
    fn field(
        &mut self,
        kind: &Kind,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        let code = matches!(kind, Kind::Code(_));
        let text = text.to_string();
        let field = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 400)
                .submit_on_enter(!code)
                .default_value(text)
        });
        self.row_subs.push(cx.subscribe_in(
            &field,
            window,
            |this, field, ev: &InputEvent, window, cx| this.on_field(field, ev, window, cx),
        ));
        field
    }

    fn row(&mut self, b: &Block, window: &mut Window, cx: &mut Context<Self>) -> Row {
        let cells = match &b.kind {
            Kind::Table(rows) => rows
                .iter()
                .map(|r| r.iter().map(|c| self.cell(c, window, cx)).collect())
                .collect(),
            _ => vec![],
        };
        Row {
            kind: b.kind.clone(),
            field: b
                .is_text()
                .then(|| self.field(&b.kind, &b.text, window, cx)),
            cells,
        }
    }

    fn cell(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        let text = text.to_string();
        let field = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 20)
                .submit_on_enter(true)
                .default_value(text)
        });
        self.row_subs.push(
            cx.subscribe_in(&field, window, |this, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.changed(cx);
                }
            }),
        );
        field
    }

    /// Read `file` into blocks.
    fn open(&mut self, file: &crate::state::FileRef, window: &mut Window, cx: &mut Context<Self>) {
        self.row_subs.clear();
        let mut blocks = md_blocks::parse(&file.content);
        if blocks
            .last()
            .is_none_or(|b| !matches!(b.kind, Kind::Paragraph))
        {
            // Room to type after the last block.
            blocks.push(Block::new(Kind::Paragraph, ""));
        }
        self.rows = blocks.iter().map(|b| self.row(b, window, cx)).collect();
        self.file = Some(crate::doc_versions::file_id(file));
        self.text = file.content.to_string();
        self.focused = None;
    }

    /// The page as blocks, as typed.
    fn blocks(&self, cx: &Context<Self>) -> Vec<Block> {
        let mut out: Vec<Block> = self
            .rows
            .iter()
            .map(|r| {
                let kind = match &r.kind {
                    Kind::Table(_) => Kind::Table(
                        r.cells
                            .iter()
                            .map(|row| row.iter().map(|c| c.read(cx).value().to_string()).collect())
                            .collect(),
                    ),
                    k => k.clone(),
                };
                let text = r
                    .field
                    .as_ref()
                    .map(|f| f.read(cx).value().to_string())
                    .unwrap_or_default();
                Block::new(kind, text)
            })
            .collect();
        // The empty paragraph left to type into is not part of the file.
        while out
            .last()
            .is_some_and(|b| matches!(b.kind, Kind::Paragraph) && b.text.is_empty())
        {
            out.pop();
        }
        out
    }

    /// Something changed: write the page to the file once typing stops.
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            let _ = this.update(cx, |this, cx| this.write_now(cx));
        }));
    }

    fn write_now(&mut self, cx: &mut Context<Self>) {
        let text = md_blocks::write(&self.blocks(cx));
        if text == self.text {
            return;
        }
        self.text = text.clone();
        self.state.update(cx, |s, cx| s.autosave_doc(text, cx));
    }

    fn index_of(&self, field: &Entity<TextareaState>) -> Option<usize> {
        self.rows.iter().position(|r| {
            r.field
                .as_ref()
                .is_some_and(|f| f.entity_id() == field.entity_id())
        })
    }

    fn on_field(
        &mut self,
        field: &Entity<TextareaState>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(i) = self.index_of(field) else {
            return;
        };
        match ev {
            InputEvent::Focus => {
                self.focused = Some(i);
                cx.notify();
            }
            InputEvent::Change => {
                // Markdown typed at the start of a paragraph.
                if matches!(self.rows[i].kind, Kind::Paragraph) {
                    let value = field.read(cx).value().to_string();
                    if let Some((kind, rest)) = md_blocks::shortcut(&value) {
                        if kind == Kind::Divider {
                            self.rows[i] = Row {
                                kind: Kind::Divider,
                                field: None,
                                cells: vec![],
                            };
                            let next = self.row(&Block::new(Kind::Paragraph, ""), window, cx);
                            self.rows.insert(i + 1, next);
                            self.focus_next = Some((i + 1, false));
                        } else {
                            self.rows[i].kind = kind;
                            field.update(cx, |f, cx| f.set_value(rest, window, cx));
                        }
                    }
                }
                self.changed(cx);
                cx.notify();
            }
            InputEvent::PressEnter { shift: false, .. } => self.split(i, window, cx),
            _ => {}
        }
    }

    /// Enter: the words after the caret go on in a new block.
    fn split(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.rows[i].field.clone() else {
            return;
        };
        let (value, at) = {
            let f = field.read(cx);
            (f.value().to_string(), f.cursor().min(f.value().len()))
        };
        let kind = self.rows[i].kind.clone();
        // An empty item ends its list.
        if value.is_empty() && matches!(kind, Kind::Bullet | Kind::Numbered | Kind::Check(_)) {
            self.rows[i].kind = Kind::Paragraph;
            self.changed(cx);
            return cx.notify();
        }
        let (left, right) = value.split_at(at);
        let (left, right) = (left.to_string(), right.to_string());
        field.update(cx, |f, cx| f.set_value(left, window, cx));
        let next = self.row(&Block::new(next_kind(&kind), right), window, cx);
        self.rows.insert(i + 1, next);
        self.focus_next = Some((i + 1, false));
        self.changed(cx);
        cx.notify();
    }

    /// Backspace at a block's start: a styled block becomes a paragraph; a
    /// paragraph joins the one above. True when it took the key.
    fn backspace(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.rows[i].field.clone() else {
            return false;
        };
        let (value, at, empty_sel) = {
            let f = field.read(cx);
            let r = f.selected_range();
            (f.value().to_string(), f.cursor(), r.is_empty())
        };
        if at != 0 || !empty_sel {
            return false;
        }
        if !matches!(self.rows[i].kind, Kind::Paragraph) {
            self.rows[i].kind = Kind::Paragraph;
            self.changed(cx);
            cx.notify();
            return true;
        }
        if i == 0 {
            return false;
        }
        // Join onto the block above when it holds words; a divider, table or
        // picture above is removed instead.
        match self.rows[i - 1].field.clone() {
            Some(above) => {
                let joined_at = above.read(cx).value().len();
                let joined = format!("{}{value}", above.read(cx).value());
                above.update(cx, |f, cx| {
                    f.set_value(joined, window, cx);
                    f.set_selected_range(joined_at..joined_at, cx);
                    f.focus(window, cx);
                });
                self.rows.remove(i);
                self.focused = Some(i - 1);
            }
            None => {
                self.rows.remove(i - 1);
                self.focus_next = Some((i - 1, false));
            }
        }
        self.changed(cx);
        cx.notify();
        true
    }

    /// The block you are in, made `kind`.
    fn set_kind(&mut self, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        let i = self.focused.unwrap_or(self.rows.len().saturating_sub(1));
        let Some(row) = self.rows.get_mut(i) else {
            return;
        };
        if row.field.is_none() {
            return;
        }
        // A code block's field keeps Enter for new lines; others make blocks.
        let was_code = matches!(row.kind, Kind::Code(_));
        let is_code = matches!(kind, Kind::Code(_));
        row.kind = kind.clone();
        if was_code != is_code {
            let text = row
                .field
                .as_ref()
                .map(|f| f.read(cx).value().to_string())
                .unwrap_or_default();
            let field = self.field(&kind, &text, window, cx);
            self.rows[i].field = Some(field);
            self.focus_next = Some((i, true));
        }
        self.state.update(cx, |s, cx| {
            s.doc_menu = None;
            cx.notify()
        });
        self.changed(cx);
        cx.notify();
    }

    /// Put `rows` after the block you are in (or at the end).
    fn insert(&mut self, blocks: Vec<Block>, window: &mut Window, cx: &mut Context<Self>) {
        let at = self.focused.map(|i| i + 1).unwrap_or(self.rows.len());
        let at = at.min(self.rows.len());
        let n = blocks.len();
        for (k, b) in blocks.iter().enumerate() {
            let row = self.row(b, window, cx);
            self.rows.insert(at + k, row);
        }
        // Somewhere to type after it.
        let after = at + n;
        if self.rows.get(after).is_none_or(|r| r.field.is_none()) {
            let p = self.row(&Block::new(Kind::Paragraph, ""), window, cx);
            self.rows.insert(after, p);
        }
        self.focus_next = Some((after, false));
        self.changed(cx);
        cx.notify();
    }

    fn insert_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = vec![
            vec!["Column 1".into(), "Column 2".into(), "Column 3".into()],
            vec![String::new(), String::new(), String::new()],
            vec![String::new(), String::new(), String::new()],
        ];
        self.insert(vec![Block::new(Kind::Table(rows), "")], window, cx);
    }

    fn insert_divider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.insert(vec![Block::new(Kind::Divider, "")], window, cx);
    }

    /// A picture from this computer: linked by its path from the file's
    /// folder when it is in the project, else by its full path.
    fn insert_image(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Insert picture".into()),
        });
        let base = self.folder(cx);
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(p) = paths.into_iter().next() else {
                return;
            };
            let src = base
                .as_ref()
                .and_then(|b| p.strip_prefix(b).ok())
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| p.to_string_lossy().replace('\\', "/"));
            let alt = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                this.pending.push(Block::new(Kind::Image { alt, src }, ""));
                cx.notify();
            });
        })
        .detach();
    }

    /// The folder the file sits in on this computer.
    fn folder(&self, cx: &Context<Self>) -> Option<std::path::PathBuf> {
        let s = self.state.read(cx);
        let path = s.doc.as_ref()?.path.as_ref()?.to_string();
        let p = std::path::PathBuf::from(path);
        let p = if p.is_absolute() {
            p
        } else {
            s.project_dir.join(p)
        };
        p.parent().map(|d| d.to_path_buf())
    }

    /// The checklist button: the block becomes a checklist item, or back.
    fn toggle_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self
            .focused
            .and_then(|i| self.rows.get(i))
            .map(|r| r.kind.clone());
        let kind = if matches!(now, Some(Kind::Check(_))) {
            Kind::Paragraph
        } else {
            Kind::Check(false)
        };
        self.set_kind(kind, window, cx);
    }

    fn tick(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(Kind::Check(done)) = self.rows.get(i).map(|r| r.kind.clone()) {
            self.rows[i].kind = Kind::Check(!done);
            self.changed(cx);
            cx.notify();
        }
    }

    /// The field you are in.
    fn current(&self) -> Option<Entity<TextareaState>> {
        self.rows.get(self.focused?)?.field.clone()
    }

    /// Bold, italic or code on the selection (or off, when it has them).
    fn mark(&mut self, mark: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.current() else { return };
        let (value, range) = {
            let f = field.read(cx);
            (f.value().to_string(), f.selected_range())
        };
        let (text, sel) = md_blocks::toggle_mark(&value, range, mark);
        field.update(cx, |f, cx| {
            f.set_value(text, window, cx);
            f.set_selected_range(sel, cx);
            f.focus(window, cx);
        });
        self.state.update(cx, |s, cx| {
            s.doc_menu = None;
            cx.notify()
        });
        self.changed(cx);
    }

    fn add_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.link.read(cx).value().trim().to_string();
        let Some(field) = self.current() else { return };
        if url.is_empty() {
            return;
        }
        let (value, range) = {
            let f = field.read(cx);
            (f.value().to_string(), f.selected_range())
        };
        let (text, end) = md_blocks::link(&value, range, &url);
        field.update(cx, |f, cx| {
            f.set_value(text, window, cx);
            f.set_selected_range(end..end, cx);
            f.focus(window, cx);
        });
        self.link.update(cx, |l, cx| l.set_value("", window, cx));
        self.state.update(cx, |s, cx| {
            s.doc_menu = None;
            cx.notify()
        });
        self.changed(cx);
    }

    /// Comment: the selected words (or the block's) go to the comment box.
    fn comment(&mut self, cx: &mut Context<Self>) {
        let quote = self
            .current()
            .map(|f| {
                let f = f.read(cx);
                let sel = f.selected_text().to_string();
                if sel.trim().is_empty() {
                    f.value().to_string()
                } else {
                    sel
                }
            })
            .unwrap_or_default();
        if quote.trim().is_empty() {
            return self.state.update(cx, |s, cx| {
                s.show_toast("Click into the words to comment on first.", cx)
            });
        }
        let place = crate::doc_versions::place_of(&self.text, &quote);
        self.state.update(cx, |s, cx| {
            s.hold_doc_comment(crate::doc_versions::Anchor { place, quote }, cx)
        });
    }

    fn add_table_line(
        &mut self,
        i: usize,
        column: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cols = self.rows[i].cells.iter().map(Vec::len).max().unwrap_or(1);
        if column {
            let n = self.rows[i].cells.len();
            for r in 0..n {
                let head = if r == 0 {
                    format!("Column {}", cols + 1)
                } else {
                    String::new()
                };
                let c = self.cell(&head, window, cx);
                self.rows[i].cells[r].push(c);
            }
        } else {
            let line: Vec<_> = (0..cols).map(|_| self.cell("", window, cx)).collect();
            self.rows[i].cells.push(line);
        }
        self.changed(cx);
        cx.notify();
    }

    /// Scroll to block `i` and put the caret in it.
    fn jump(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll.scroll_to_item(i);
        if let Some(f) = self.rows.get(i).and_then(|r| r.field.clone()) {
            f.update(cx, |f, cx| f.focus(window, cx));
            self.focused = Some(i);
        }
    }
}

impl Render for MdPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let (file, menu, jump) = {
            let s = self.state.read(cx);
            (s.doc.clone(), s.doc_menu, s.md_jump)
        };
        let Some(file) = file.filter(|f| f.is_markdown()) else {
            return div().into_any_element();
        };
        let id = crate::doc_versions::file_id(&file);
        // A new file, or this one changed under the page (not by it).
        if self.file.as_ref() != Some(&id) || file.content.as_ref() != self.text {
            self.open(&file, window, cx);
        }
        if !self.pending.is_empty() {
            let blocks = std::mem::take(&mut self.pending);
            self.insert(blocks, window, cx);
        }
        if let Some(i) = jump {
            self.state.update(cx, |s, _| s.md_jump = None);
            self.jump(i, window, cx);
        }
        if let Some((i, at_end)) = self.focus_next.take() {
            if let Some(f) = self.rows.get(i).and_then(|r| r.field.clone()) {
                f.update(cx, |f, cx| {
                    let at = if at_end { f.value().len() } else { 0 };
                    f.set_selected_range(at..at, cx);
                    f.focus(window, cx);
                });
                self.focused = Some(i);
                self.scroll.scroll_to_item(i);
            }
        }

        let entity = cx.entity();
        let base = self.folder(cx);
        let mut number = 0;
        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                number = match r.kind {
                    Kind::Numbered => number + 1,
                    _ => 0,
                };
                block(i, r, number, base.as_deref(), &entity, &t)
            })
            .collect();

        let current = self
            .focused
            .and_then(|i| self.rows.get(i))
            .map(|r| r.kind.clone());
        let toolbar = toolbar(&entity, current.as_ref(), menu, &self.link, &t);

        div()
            .size_full()
            .relative()
            .child(
                div()
                    .id("md-page")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(t.space.xxl)
                    .pt(t.space.xl + t.space.xs)
                    // Room for the floating toolbar under the last block.
                    .pb(px(120.))
                    .flex()
                    .flex_col()
                    .children(rows),
            )
            .child(toolbar)
            .into_any_element()
    }
}

/// One block as it reads on the page.
fn block(
    i: usize,
    r: &Row,
    number: usize,
    base: Option<&std::path::Path>,
    page: &Entity<MdPage>,
    t: &Theme,
) -> AnyElement {
    let (size, weight, mono) = style(&r.kind, t);
    let top = match r.kind {
        Kind::Heading(1) => t.space.lg,
        Kind::Heading(_) => t.space.md,
        _ => px(2.),
    };
    let gutter = |child: AnyElement| {
        div()
            .flex_none()
            .w(px(24.))
            .flex()
            .justify_center()
            .pt(px(4.))
            .child(child)
            .into_any_element()
    };
    let lead: Option<AnyElement> = match r.kind {
        Kind::Bullet => Some(gutter(
            div()
                .mt(px(6.))
                .size(px(5.))
                .rounded(t.radius.pill)
                .bg(t.colors.text)
                .into_any_element(),
        )),
        Kind::Numbered => Some(gutter(
            div()
                .text_size(t.text.body)
                .text_color(t.colors.text_muted)
                .child(format!("{number}."))
                .into_any_element(),
        )),
        Kind::Check(done) => {
            let p = page.clone();
            Some(gutter(
                div()
                    .id(SharedString::from(format!("md-tick-{i}")))
                    .mt(px(2.))
                    .size(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius.small)
                    .border_1()
                    .border_color(if done {
                        t.colors.text
                    } else {
                        t.colors.text_subtle
                    })
                    .when(done, |d| d.bg(t.colors.text))
                    .cursor_pointer()
                    .when(done, |d| {
                        d.child(icon("icons/check.svg", px(12.), t.colors.surface))
                    })
                    .on_click(move |_, _, cx| p.update(cx, |p, cx| p.tick(i, cx)))
                    .into_any_element(),
            ))
        }
        _ => None,
    };
    let done = matches!(r.kind, Kind::Check(true));
    let row = div().w_full().flex().pt(top);
    match &r.kind {
        Kind::Divider => row
            .py(t.space.md)
            .child(div().w_full().h(px(1.)).bg(t.colors.border))
            .into_any_element(),
        Kind::Image { alt, src } => {
            let path = if src.starts_with("http") {
                None
            } else {
                let p = std::path::PathBuf::from(src);
                Some(if p.is_absolute() {
                    p
                } else {
                    base.map(|b| b.join(&p)).unwrap_or(p)
                })
            };
            row.py(t.space.sm)
                .flex_col()
                .gap(t.space.xs)
                .children(path.filter(|p| p.exists()).map(|p| {
                    gpui::img(p)
                        .max_w_full()
                        .max_h(px(360.))
                        .rounded(t.radius.small)
                }))
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_muted)
                        .child(if alt.is_empty() {
                            src.clone()
                        } else {
                            alt.clone()
                        }),
                )
                .into_any_element()
        }
        Kind::Table(_) => {
            let (p1, p2) = (page.clone(), page.clone());
            let cols = r.cells.iter().map(Vec::len).max().unwrap_or(1);
            let grid = div()
                .w_full()
                .flex()
                .flex_col()
                .rounded(t.radius.small)
                .border_1()
                .border_color(t.colors.border)
                .overflow_hidden()
                .children(r.cells.iter().enumerate().map(|(ri, line)| {
                    div()
                        .flex()
                        .when(ri > 0, |d| d.border_t_1().border_color(t.colors.border))
                        .when(ri == 0, |d| {
                            d.bg(t.extra.sunk).font_weight(FontWeight::SEMIBOLD)
                        })
                        .children((0..cols).map(|ci| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .px(t.space.sm)
                                .py(t.space.xs)
                                .when(ci > 0, |d| d.border_l_1().border_color(t.colors.border))
                                .text_size(t.text.body_sm)
                                .children(line.get(ci).map(|f| {
                                    Textarea::new(f)
                                        .appearance(false)
                                        .text_size(t.text.body_sm)
                                        .when(ri == 0, |d| d.font_weight(FontWeight::SEMIBOLD))
                                        .px_0()
                                        .py_0()
                                }))
                        }))
                }));
            let add = |id: String, label: &'static str| {
                div()
                    .id(SharedString::from(id))
                    .px(t.space.sm)
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(t.radius.small)
                    .cursor_pointer()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .hover(|s| s.bg(t.colors.hover))
                    .child(icon("icons/plus.svg", px(12.), t.colors.text_muted))
                    .child(label)
            };
            row.py(t.space.sm)
                .flex_col()
                .gap(t.space.xs)
                .child(grid)
                .child(
                    div()
                        .flex()
                        .gap(t.space.xs)
                        .child(
                            add(format!("md-row-{i}"), "Row").on_click(move |_, window, cx| {
                                p1.update(cx, |p, cx| p.add_table_line(i, false, window, cx))
                            }),
                        )
                        .child(add(format!("md-col-{i}"), "Column").on_click(
                            move |_, window, cx| {
                                p2.update(cx, |p, cx| p.add_table_line(i, true, window, cx))
                            },
                        )),
                )
                .into_any_element()
        }
        _ => {
            let Some(field) = r.field.clone() else {
                return div().into_any_element();
            };
            let p = page.clone();
            let f = field.clone();
            let text = div()
                .flex_1()
                .min_w_0()
                .text_size(size)
                .font_weight(weight)
                .line_height(relative(if mono { 1.45 } else { 1.55 }))
                .text_color(if done {
                    t.colors.text_muted
                } else {
                    t.colors.text_strong
                })
                .when(done, |d| d.line_through())
                .when(mono, |d| d.font_family(t.font_mono.clone()))
                .child(
                    Textarea::new(&field)
                        .appearance(false)
                        .text_size(size)
                        .font_weight(weight)
                        .px_0()
                        .py(px(2.))
                        .when(mono, |d| d.font_family(t.font_mono.clone())),
                );
            let body = match r.kind {
                Kind::Quote => div()
                    .flex_1()
                    .min_w_0()
                    .pl(t.space.md)
                    .border_l(px(3.))
                    .border_color(t.colors.border)
                    .text_color(t.colors.text_muted)
                    .child(text)
                    .into_any_element(),
                Kind::Code(_) => div()
                    .flex_1()
                    .min_w_0()
                    .my(t.space.xs)
                    .p(t.space.md)
                    .rounded(t.radius.small)
                    .bg(t.extra.sunk)
                    .child(text)
                    .into_any_element(),
                _ => text.into_any_element(),
            };
            row.on_key_down(move |ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                if k.key == "backspace" && !k.modifiers.shift {
                    let took = p.update(cx, |p, cx| {
                        let i = p.index_of(&f);
                        i.is_some_and(|i| p.backspace(i, window, cx))
                    });
                    if took {
                        cx.stop_propagation();
                    }
                } else if k.modifiers.secondary()
                    && !k.modifiers.shift
                    && (k.key == "b" || k.key == "i")
                {
                    let mark = if k.key == "b" { "**" } else { "*" };
                    p.update(cx, |p, cx| {
                        p.focused = p.index_of(&f);
                        p.mark(mark, window, cx)
                    });
                    cx.stop_propagation();
                }
            })
            .children(lead)
            .child(body)
            .into_any_element()
        }
    }
}

/// The floating toolbar at the foot of the page, and its two menus.
fn toolbar(
    page: &Entity<MdPage>,
    current: Option<&Kind>,
    menu: Option<DocMenu>,
    link: &Entity<InputState>,
    t: &Theme,
) -> AnyElement {
    let tool = |id: &'static str, path: &'static str, tip: &'static str| {
        super::button::Button::icon(id, path, tip).compact()
    };
    let sep = || div().w(px(1.)).h(px(18.)).mx(px(2.)).bg(t.colors.border);
    let state_of = |p: &Entity<MdPage>| p.clone();
    let style_label = current.map(style_name).unwrap_or("Paragraph");
    let (p_style, p_table, p_image, p_check, p_div, p_more, p_comment) = (
        state_of(page),
        state_of(page),
        state_of(page),
        state_of(page),
        state_of(page),
        state_of(page),
        state_of(page),
    );
    let open_menu = move |p: &Entity<MdPage>, m: DocMenu, cx: &mut gpui::App| {
        let st = p.read(cx).state.clone();
        st.update(cx, |s, cx| s.toggle_doc_menu(m, cx));
    };
    let bar =
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .p(px(4.))
            .rounded(t.radius.pill)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .shadow(t.elevation_popover())
            .child(
                div()
                    .id("md-style")
                    .h(px(28.))
                    .px(t.space.sm)
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(t.radius.pill)
                    .cursor_pointer()
                    .text_size(t.text.body_sm)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.colors.text)
                    .hover(|s| s.bg(t.colors.hover))
                    .when(menu == Some(DocMenu::TextStyle), |d| d.bg(t.colors.hover))
                    .child(style_label)
                    .child(icon("icons/chevron-down.svg", px(12.), t.colors.text_muted))
                    .on_click(move |_, _, cx| open_menu(&p_style, DocMenu::TextStyle, cx)),
            )
            .child(sep())
            .child(
                tool("md-table", "icons/table.svg", "Table").on_click(move |_, window, cx| {
                    p_table.update(cx, |p, cx| p.insert_table(window, cx))
                }),
            )
            .child(
                tool("md-image", "icons/image.svg", "Picture")
                    .on_click(move |_, _, cx| p_image.update(cx, |p, cx| p.insert_image(cx))),
            )
            .child(
                tool("md-check", "icons/square-check.svg", "Checklist")
                    .selected(matches!(current, Some(Kind::Check(_))))
                    .on_click(move |_, window, cx| {
                        p_check.update(cx, |p, cx| p.toggle_check(window, cx))
                    }),
            )
            .child(tool("md-divider", "icons/minus.svg", "Divider").on_click(
                move |_, window, cx| p_div.update(cx, |p, cx| p.insert_divider(window, cx)),
            ))
            .child(sep())
            .child(
                tool("md-more", "icons/ellipsis.svg", "Bold, italic, link, code")
                    .selected(menu == Some(DocMenu::Format))
                    .on_click(move |_, _, cx| open_menu(&p_more, DocMenu::Format, cx)),
            )
            .child(sep())
            .child(
                tool("md-comment", "icons/message-square-plus.svg", "Comment")
                    .on_click(move |_, _, cx| p_comment.update(cx, |p, cx| p.comment(cx))),
            );

    let item =
        |id: String, path: &'static str, label: &'static str, keys: &'static str, on: bool| {
            div()
                .id(SharedString::from(id))
                .h(t.size.menu_row)
                .px(t.space.sm)
                .flex()
                .items_center()
                .gap(t.space.sm)
                .rounded(t.radius.small)
                .cursor_pointer()
                .text_size(t.text.body_sm)
                .text_color(t.colors.text)
                .when(on, |d| crate::theme::selected(t, d))
                .when(!on, |d| d.hover(|s| s.bg(t.colors.hover)))
                .child(icon(path, t.size.icon_sm, t.colors.text_muted))
                .child(div().flex_1().child(label))
                .child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(keys),
                )
        };
    let popover = |w: f32| {
        div()
            .w(px(w))
            .flex()
            .flex_col()
            .p(t.space.xs)
            .mb(t.space.xs)
            .bg(t.colors.surface)
            .border_1()
            .border_color(t.colors.border)
            .rounded(t.radius.card)
            .shadow(t.elevation_popover())
    };
    let menu_el: Option<gpui::Div> = match menu {
        Some(DocMenu::TextStyle) => {
            let mut m = popover(220.);
            for (k, (name, path)) in STYLES.iter().enumerate() {
                let p = page.clone();
                let on = style_label == *name;
                m = m.child(item(format!("md-style-{k}"), path, name, "", on).on_click(
                    move |_, window, cx| {
                        p.update(cx, |p, cx| p.set_kind(style_kind(name), window, cx))
                    },
                ));
            }
            Some(m)
        }
        Some(DocMenu::Format) => {
            let (pb, pi, pc, pl) = (page.clone(), page.clone(), page.clone(), page.clone());
            Some(
                popover(240.)
                    .child(
                        item("md-bold".into(), "icons/bold.svg", "Bold", "Ctrl+B", false).on_click(
                            move |_, window, cx| pb.update(cx, |p, cx| p.mark("**", window, cx)),
                        ),
                    )
                    .child(
                        item(
                            "md-italic".into(),
                            "icons/italic.svg",
                            "Italic",
                            "Ctrl+I",
                            false,
                        )
                        .on_click(move |_, window, cx| {
                            pi.update(cx, |p, cx| p.mark("*", window, cx))
                        }),
                    )
                    .child(
                        item(
                            "md-code".into(),
                            "icons/code-xml.svg",
                            "Inline code",
                            "",
                            false,
                        )
                        .on_click(move |_, window, cx| {
                            pc.update(cx, |p, cx| p.mark("`", window, cx))
                        }),
                    )
                    .child(div().h(px(1.)).my(t.space.xs).bg(t.colors.border))
                    .child(
                        div()
                            .px(t.space.sm)
                            .py(t.space.xs)
                            .flex()
                            .items_center()
                            .gap(t.space.sm)
                            .child(icon("icons/link.svg", t.size.icon_sm, t.colors.text_muted))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(t.text.body_sm)
                                    .child(Input::new(link).appearance(false)),
                            )
                            .child(
                                super::button::Button::ghost("md-link-add", "Link")
                                    .compact()
                                    .on_click(move |_, window, cx| {
                                        pl.update(cx, |p, cx| p.add_link(window, cx))
                                    }),
                            ),
                    ),
            )
        }
        _ => None,
    };

    div()
        .absolute()
        .bottom(t.space.lg)
        .left_0()
        .right_0()
        .flex()
        .flex_col()
        .items_center()
        .children(menu_el.map(|m| super::motion::unfold(m, "md-menu", true, false, t)))
        .child(bar)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_style_names_its_kind() {
        for (name, _) in STYLES {
            assert_eq!(style_name(&style_kind(name)), *name);
        }
    }

    #[test]
    fn enter_goes_on_with_a_list_and_ends_anything_else() {
        assert_eq!(next_kind(&Kind::Bullet), Kind::Bullet);
        assert_eq!(next_kind(&Kind::Check(true)), Kind::Check(false));
        assert_eq!(next_kind(&Kind::Heading(1)), Kind::Paragraph);
    }
}
