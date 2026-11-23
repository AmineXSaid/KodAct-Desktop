//! The file view's text editor: a Markdown, text, code or HTML file opened
//! as an editor (the kit's `EditorState`: line numbers, highlighting by the
//! file's language, soft wrap for prose). Typing marks the file Edited
//! (`AppState::doc_edit`); Save (Ctrl+S) writes it to its project path and
//! records a new version in the chat, Discard puts the file back as it was.

use gpui::component::input::{Editor, EditorState, InputEvent, Redo, Undo};
use gpui::{
    div, prelude::*, Context, Entity, IntoElement, Render, SharedString, Subscription, Window,
};

use crate::state::AppState;
use crate::theme::ActiveTheme;

/// The largest file the editor opens; bigger ones stay read-only source.
pub const MAX_BYTES: usize = 512 * 1024;

pub struct TextEditor {
    state: Entity<AppState>,
    editor: Option<Entity<EditorState>>,
    /// The file and version in the editor (`doc_version_key`), its text as
    /// saved, and whether the editor holds changes to it.
    key: Option<SharedString>,
    original: SharedString,
    dirty: bool,
    _subs: Vec<Subscription>,
    sub: Option<Subscription>,
}

/// Whether the file view edits this file as text: a text file that is not a
/// picture, an Office file or a PDF, and not too large.
pub fn editable(file: &crate::state::FileRef) -> bool {
    file.bytes.is_none() && file.content.len() <= MAX_BYTES
}

/// "12 lines · 80 words", for the footer.
pub fn counts(text: &str) -> String {
    let lines = if text.is_empty() {
        0
    } else {
        text.lines().count().max(1)
    };
    let words = text.split_whitespace().count();
    let (l, w) = (
        if lines == 1 { "line" } else { "lines" },
        if words == 1 { "word" } else { "words" },
    );
    format!("{lines} {l} · {words} {w}")
}

/// The editor's name for a file's language: `rs` is `rust`.
pub fn language_of(name: &str) -> String {
    let lang = super::source_view::lang_for(name);
    match lang.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "yml" => "yaml",
        "sh" | "bash" | "zsh" => "bash",
        "md" => "markdown",
        "htm" => "html",
        "rb" => "ruby",
        "kt" => "kotlin",
        "cs" => "csharp",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        other => return other.to_string(),
    }
    .to_string()
}

impl TextEditor {
    pub fn new(state: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subs = vec![cx.observe(&state, |_, _, cx| cx.notify())];
        Self {
            state,
            editor: None,
            key: None,
            original: SharedString::default(),
            dirty: false,
            _subs: subs,
            sub: None,
        }
    }

    /// A new editor for `file`.
    fn open(&mut self, file: &crate::state::FileRef, window: &mut Window, cx: &mut Context<Self>) {
        let wrap = super::source_view::wraps(super::source_view::lang_for(&file.name))
            || file.is_markdown();
        let lang = language_of(&file.name);
        let content = file.content.clone();
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .line_number(!wrap)
                .soft_wrap(wrap)
                .language(lang)
                .default_value(content)
        });
        let id = crate::doc_versions::file_id(file);
        self.sub = Some(
            cx.subscribe(&editor, move |this, editor, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    let text = editor.read(cx).value();
                    this.dirty = text != this.original;
                    let (id, text, dirty) = (id.clone(), text, this.dirty);
                    this.state
                        .update(cx, |s, cx| s.set_doc_edit(dirty.then_some((id, text)), cx));
                }
            }),
        );
        self.original = file.content.clone();
        self.key = Some(crate::state::doc_version_key(file));
        self.dirty = false;
        self.editor = Some(editor);
    }

    /// Undo or redo in the editor (the buttons are not inside it, so it is
    /// focused first).
    pub fn step(&self, undo: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        editor.update(cx, |e, cx| e.focus(window, cx));
        if undo {
            window.dispatch_action(Box::new(Undo), cx);
        } else {
            window.dispatch_action(Box::new(Redo), cx);
        }
    }
}

impl Render for TextEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let (file, edit) = {
            let s = self.state.read(cx);
            (s.doc.clone(), s.doc_edit.clone())
        };
        let Some(file) = file.filter(editable) else {
            return div().into_any_element();
        };
        let key = crate::state::doc_version_key(&file);
        if self.key.as_ref() != Some(&key) {
            // Our own save keeps the editor (and its caret) as it is.
            let same = self
                .editor
                .as_ref()
                .is_some_and(|e| e.read(cx).value() == file.content)
                && self.key.is_some();
            if same {
                self.original = file.content.clone();
                self.key = Some(key);
                self.dirty = false;
            } else {
                self.open(&file, window, cx);
            }
        }
        // Discard: the state dropped the edit, so the text goes back.
        if self.dirty && edit.is_none() {
            if let Some(editor) = &self.editor {
                let original = self.original.clone();
                editor.update(cx, |e, cx| e.set_value(original, window, cx));
            }
            self.dirty = false;
        }
        let Some(editor) = &self.editor else {
            return div().into_any_element();
        };
        div()
            .size_full()
            .child(
                Editor::new(editor)
                    .appearance(false)
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.body_sm)
                    .size_full(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_footer_counts_lines_and_words() {
        assert_eq!(counts(""), "0 lines · 0 words");
        assert_eq!(counts("one"), "1 line · 1 word");
        assert_eq!(counts("a b\nc d e\n"), "2 lines · 5 words");
    }

    #[test]
    fn languages_are_named_as_the_editor_knows_them() {
        assert_eq!(language_of("main.rs"), "rust");
        assert_eq!(language_of("a.py"), "python");
        assert_eq!(language_of("plan.md"), "markdown");
        assert_eq!(language_of("flow.HTML"), "html");
        assert_eq!(language_of("x.json"), "json");
    }

    #[test]
    fn only_text_files_under_the_limit_are_edited() {
        let f = |bytes: bool, n: usize| crate::state::FileRef {
            name: "a.md".into(),
            path: None,
            content: "x".repeat(n).into(),
            bytes: bytes.then(|| std::sync::Arc::from(vec![0u8])),
        };
        assert!(editable(&f(false, 10)));
        assert!(!editable(&f(true, 10)));
        assert!(!editable(&f(false, MAX_BYTES + 1)));
    }
}
