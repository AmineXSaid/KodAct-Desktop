//! The browser's bar when it shows a page this chat designed, with the
//! controls of a design canvas (Claude's has the same): the page, whose
//! menu lists the chat's pages, asks for a new one and says where the file
//! is; the comment tool (point at a part, click, say what should change)
//! and the comments (their pins on the page, and a list); then Present,
//! the zoom (in, out, a desktop's width, 25 to 200 %) and Share (a PDF
//! beside the page, your own browser). Each menu hangs under its button,
//! over the page.

use std::path::{Path, PathBuf};

use gpui::{
    anchored, deferred, div, prelude::*, px, relative, Anchor, Context, Entity, SharedString,
};

use super::browser_view::BrowserView;
use super::{icon, kbd};
use crate::state::AppState;
use crate::theme::{self, Theme};

/// Which of the bar's menus is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
    Pages,
    Comments,
    Insert,
    Library,
    Zoom,
    Share,
}

/// A page dragged in the Pages menu, to put it before another; its name
/// follows the pointer.
#[derive(Clone)]
pub struct DraggedPage(PathBuf, SharedString);

impl gpui::Render for DraggedPage {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::theme::ActiveTheme;
        let t = cx.theme();
        theme::popover(t)
            .px(t.space.md)
            .py(t.space.xs)
            .text_size(t.text.body_sm)
            .text_color(t.colors.text)
            .child(self.1.clone())
    }
}

/// The Insert menu's tools, in Claude's order (the artboards come after
/// Text), with their icons.
const TOOLS: [(&str, &str, Option<crate::design::marks::Shape>); 9] = {
    use crate::design::marks::Shape;
    [
        ("Hand", "icons/hand.svg", None),
        ("Text", "icons/type.svg", Some(Shape::Text)),
        ("Note", "icons/sticky-note.svg", Some(Shape::Note)),
        ("Pencil", "icons/pencil.svg", Some(Shape::Pencil)),
        ("Pen", "icons/pen-tool.svg", Some(Shape::Pen)),
        ("Rectangle", "icons/square.svg", Some(Shape::Rect)),
        ("Oval", "icons/circle.svg", Some(Shape::Oval)),
        ("Arrow", "icons/arrow-up-right.svg", Some(Shape::Arrow)),
        ("Line", "icons/minus.svg", Some(Shape::Line)),
    ]
};

/// The Insert tool as its menu shows it.
fn tool_of(shape: Option<crate::design::marks::Shape>) -> crate::state::InsertTool {
    shape.map_or(
        crate::state::InsertTool::Hand,
        crate::state::InsertTool::Mark,
    )
}

/// The icon of the tool in hand.
fn tool_icon(tool: crate::state::InsertTool) -> &'static str {
    TOOLS
        .iter()
        .find(|(_, _, shape)| tool_of(*shape) == tool)
        .map_or("icons/plus.svg", |(_, icon, _)| icon)
}

/// A comment, as the comments menu lists it.
pub struct Note {
    pub n: usize,
    pub label: String,
    pub text: String,
    pub sent: crate::state::CommentSent,
    pub resolved: bool,
}

/// What the bar shows.
pub struct Bar {
    /// The page on screen.
    pub page: PathBuf,
    /// The chat's pages, the one on screen among them.
    pub pages: Vec<PathBuf>,
    pub zoom: f64,
    pub menu: Option<Menu>,
    /// The comment tool is on.
    pub select: bool,
    /// The page's comments; whether their pins show, resolved ones too.
    pub comments: Vec<Note>,
    pub pins: bool,
    pub resolved: bool,
    /// The page's outline is beside it; layout guides are on.
    pub outline: bool,
    pub guides: bool,
    /// There is a version before, or after, the one on disk.
    pub undo: bool,
    pub redo: bool,
    /// The Insert tool in hand, and the artboard the page is shown in.
    pub tool: Option<crate::state::InsertTool>,
    pub artboard: Option<&'static str>,
    /// The library's design systems (None while they're read), its search
    /// box and what is typed in it.
    pub systems: Option<std::sync::Arc<Vec<crate::state::SystemCard>>>,
    pub filter: Entity<gpui::component::input::InputState>,
    pub filter_text: String,
}

/// What a menu row does to the app.
type Act = dyn Fn(&mut AppState, &mut gpui::Context<AppState>);

/// A zoom as the bar says it: `75%`.
pub fn percent(zoom: f64) -> String {
    format!("{:.0}%", zoom * 100.0)
}

/// A file's name, for a menu or the bar.
fn name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// A button in the bar: an icon, and a label and a chevron when it opens
/// a menu.
fn tool(
    t: &Theme,
    id: &'static str,
    path: &'static str,
    label: Option<String>,
    menu: bool,
    on: bool,
    tip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    let hover = t.colors.hover;
    div()
        .id(id)
        .flex_none()
        .min_w(t.size.control)
        .h(t.size.control)
        .flex()
        .items_center()
        .justify_center()
        .gap(t.space.xs)
        .px(if label.is_some() { t.space.sm } else { px(0.) })
        .rounded(t.radius.button)
        .cursor_pointer()
        .when(on, |d| d.bg(hover))
        .hover(move |s| s.bg(hover))
        .text_size(t.text.caption)
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(t.colors.text)
        .child(icon(path, t.size.icon_sm, t.colors.text_muted))
        .children(label.map(|l| {
            div()
                .max_w(px(180.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(l)
        }))
        .when(menu, |d| {
            d.child(icon(
                "icons/chevron-down.svg",
                t.size.icon_xs,
                t.colors.text_subtle,
            ))
        })
        // No tip over its own open menu.
        .when(!on, |d| {
            d.tooltip(move |window, cx| {
                gpui::component::tooltip::Tooltip::new(tip).build(window, cx)
            })
        })
}

/// A row in a menu: an icon (or its room), its label, a key hint, and a
/// check on the one in use.
fn row(
    t: &Theme,
    id: impl Into<SharedString>,
    path: Option<&'static str>,
    label: impl Into<SharedString>,
    hint: Option<&'static str>,
    checked: bool,
) -> gpui::Stateful<gpui::Div> {
    // A binding from main.rs, as this platform writes its keys.
    let hint = hint.map(|b| super::shortcuts::keys(b).join(" "));
    let hover = t.colors.hover;
    div()
        .id(gpui::ElementId::Name(id.into()))
        .flex()
        .items_center()
        .gap(t.space.sm)
        .h(t.size.row)
        .px(t.space.md)
        .rounded(t.radius.button)
        .cursor_pointer()
        .text_color(t.colors.text)
        .hover(move |d| d.bg(hover))
        .child(match path {
            Some(p) => icon(p, t.size.icon_sm, t.colors.text_muted).into_any_element(),
            None => div().flex_none().size(t.size.icon_sm).into_any_element(),
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label.into()),
        )
        .children(hint.map(|h| kbd(t, h)))
        .when(checked, |d| {
            d.child(icon("icons/check.svg", t.size.icon_sm, t.colors.primary))
        })
}

/// A comment's number in its pin's colours: Forge's comments in the
/// accent, notes for you quieter.
pub fn pin_mark(t: &Theme, n: usize, sent: crate::state::CommentSent) -> gpui::Div {
    let note = sent == crate::state::CommentSent::Note;
    div()
        .flex_none()
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(t.colors.surface)
        .bg(if note {
            t.colors.text_muted
        } else {
            t.colors.primary
        })
        .text_color(t.colors.primary_fg)
        .text_size(t.text.caption)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(n.to_string())
}

/// A thin line between a menu's groups.
fn gap(t: &Theme) -> impl IntoElement {
    div().my(t.space.xs).mx(t.space.sm).child(theme::rule(t))
}

/// A menu hanging under its button: left-aligned, or right-aligned at the
/// bar's end; a click elsewhere closes it.
fn hang(t: &Theme, right: bool, menu: gpui::Div, cx: &Context<BrowserView>) -> impl IntoElement {
    div()
        .absolute()
        .top(relative(1.))
        .when(right, |d| d.right_0())
        .when(!right, |d| d.left_0())
        .child(
            deferred(
                anchored()
                    .anchor(if right {
                        Anchor::TopRight
                    } else {
                        Anchor::TopLeft
                    })
                    .snap_to_window_with_margin(t.space.sm)
                    .child(
                        div()
                            .mt(t.space.xs)
                            .on_mouse_down_out(
                                cx.listener(|this, _, _, cx| this.set_menu(None, cx)),
                            )
                            .child(menu),
                    ),
            )
            .with_priority(2),
        )
}

/// A menu's sheet: a popover of rows.
fn sheet(t: &Theme, width: f32) -> gpui::Div {
    theme::popover(t).occlude().w(px(width)).flex().flex_col()
}

pub fn render(
    bar: Bar,
    t: &Theme,
    state: &Entity<AppState>,
    cx: &Context<BrowserView>,
) -> impl IntoElement {
    let Bar {
        page,
        pages,
        zoom,
        menu,
        select,
        comments,
        pins,
        resolved,
        outline,
        guides,
        undo,
        redo,
        tool: in_hand,
        artboard,
        systems,
        filter,
        filter_text,
    } = bar;
    // Each menu row does its thing on the app, then the menu closes.
    let act = |f: Box<Act>| {
        let state = state.clone();
        cx.listener(move |this: &mut BrowserView, _: &gpui::ClickEvent, _, cx| {
            this.set_menu(None, cx);
            state.update(cx, |s, cx| f(s, cx));
        })
    };
    let toggle = |m: Menu| {
        cx.listener(move |this: &mut BrowserView, _: &gpui::ClickEvent, _, cx| {
            let open = this.design_menu() != Some(m);
            this.set_menu(open.then_some(m), cx);
        })
    };

    // The page, and its menu: the chat's pages, then a new one, then the file.
    let mut pages_menu = sheet(t, 260.).child(
        div()
            .px(t.space.md)
            .pt(t.space.xs)
            .pb(t.space.xs)
            .text_size(t.text.caption)
            .text_color(t.colors.text_subtle)
            .child("Pages in this chat"),
    );
    let hover = t.colors.hover;
    for (i, p) in pages.iter().enumerate() {
        let (to, before) = (p.clone(), p.clone());
        let drop_state = state.clone();
        pages_menu = pages_menu.child(
            row(
                t,
                format!("design-page-{i}"),
                Some("icons/file.svg"),
                name(p),
                None,
                *p == page,
            )
            // Drag a page onto another to put it before that one.
            .on_drag(DraggedPage(p.clone(), name(p).into()), |d, _, _, cx| {
                cx.new(|_| d.clone())
            })
            .drag_over::<DraggedPage>(move |style, _, _, _| style.bg(hover))
            .on_drop(move |d: &DraggedPage, _, cx| {
                drop_state.update(cx, |s, cx| s.design_move_page(&d.0, &before, cx))
            })
            .on_click(act(Box::new(move |s, cx| s.design_show(&to, cx)))),
        );
    }
    let (open, reveal, copy) = (page.clone(), page.clone(), page.clone());
    pages_menu = pages_menu
        .child(
            row(
                t,
                "design-new-page",
                Some("icons/plus.svg"),
                "New page…",
                None,
                false,
            )
            .on_click(act(Box::new(|s, cx| s.design_new_page(cx)))),
        )
        .child(
            row(
                t,
                "design-duplicate",
                Some("icons/copy.svg"),
                "Duplicate",
                None,
                false,
            )
            .on_click(act(Box::new(|s, cx| s.design_duplicate(cx)))),
        )
        .child(
            row(
                t,
                "design-rename",
                Some("icons/pencil.svg"),
                "Rename…",
                None,
                false,
            )
            .on_click(act(Box::new(|s, cx| s.design_rename(cx)))),
        )
        .child(gap(t))
        .child(
            row(
                t,
                "design-reveal",
                Some("icons/folder-open.svg"),
                "Show in folder",
                None,
                false,
            )
            .on_click(act(Box::new(move |_, cx| {
                if let Some(dir) = reveal.parent() {
                    cx.open_with_system(dir);
                }
            }))),
        )
        .child(
            row(
                t,
                "design-copy-path",
                Some("icons/copy.svg"),
                "Copy the file's path",
                None,
                false,
            )
            .on_click(act(Box::new(move |s, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.display().to_string()));
                s.show_toast("Copied the page's path", cx);
            }))),
        );

    // The comments: their pins, resolved ones, then each, newest last.
    let open_count = comments.iter().filter(|c| !c.resolved).count();
    let mut comments_menu = sheet(t, 300.)
        .child(
            row(
                t,
                "comments-pins",
                Some("icons/eye.svg"),
                "Show pins on the page",
                None,
                pins,
            )
            .on_click(act(Box::new(|s, cx| {
                s.design_pins = !s.design_pins;
                cx.notify();
            }))),
        )
        .child(
            row(
                t,
                "comments-resolved",
                Some("icons/circle-check.svg"),
                "Show resolved comments",
                None,
                resolved,
            )
            .on_click(act(Box::new(|s, cx| {
                s.design_resolved = !s.design_resolved;
                cx.notify();
            }))),
        )
        .child(gap(t));
    if comments.is_empty() {
        comments_menu = comments_menu.child(
            div()
                .px(t.space.md)
                .py(t.space.sm)
                .text_size(t.text.caption)
                .line_height(relative(1.4))
                .text_color(t.colors.text_subtle)
                .child("No comments yet. Turn on the comment tool, point at any part and click."),
        );
    }
    let hover = t.colors.hover;
    for c in comments.iter().filter(|c| resolved || !c.resolved) {
        let (n, go, done) = (c.n, page.clone(), page.clone());
        let status = match c.sent {
            crate::state::CommentSent::Note => "Note",
            crate::state::CommentSent::Waiting => "Waiting",
            crate::state::CommentSent::Sent => "Sent",
        };
        comments_menu = comments_menu.child(
            div()
                .id(SharedString::from(format!("comment-row-{n}")))
                .flex()
                .items_start()
                .gap(t.space.sm)
                .px(t.space.md)
                .py(t.space.xs)
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover(move |d| d.bg(hover))
                .when(c.resolved, |d| d.opacity(0.6))
                .child(pin_mark(t, n, c.sent))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(format!("{} · {status}", c.label)),
                        )
                        .child(
                            div()
                                .text_size(t.text.body_sm)
                                .line_height(relative(1.4))
                                .text_color(t.colors.text)
                                .child(c.text.clone()),
                        ),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("comment-resolve-{n}")))
                        .flex_none()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.radius.small)
                        .hover(move |d| d.bg(hover))
                        .child(icon(
                            "icons/circle-check.svg",
                            t.size.icon_sm,
                            if c.resolved {
                                t.colors.primary
                            } else {
                                t.colors.text_subtle
                            },
                        ))
                        .tooltip(move |window, cx| {
                            gpui::component::tooltip::Tooltip::new("Resolve, or open again")
                                .build(window, cx)
                        })
                        .on_click(act(Box::new(move |s, cx| s.design_resolve(&done, n, cx)))),
                )
                .on_click(act(Box::new(move |s, cx| s.design_go_to(&go, n, cx)))),
        );
    }

    // Insert: the tools that draw on the page, and the artboards.
    let mut insert_menu = sheet(t, 240.);
    for (i, (label, icon_path, shape)) in TOOLS.iter().enumerate() {
        let tool = tool_of(*shape);
        insert_menu = insert_menu.child(
            row(
                t,
                format!("insert-{i}"),
                Some(icon_path),
                *label,
                None,
                in_hand == Some(tool),
            )
            .on_click(act(Box::new(move |s, cx| {
                let next = (s.design_tool != Some(tool)).then_some(tool);
                s.design_set_tool(next, cx)
            }))),
        );
        // The artboards after Text, as Claude has them.
        if *label == "Text" {
            insert_menu = insert_menu.child(
                row(
                    t,
                    "insert-artboard",
                    Some("icons/frame.svg"),
                    "Artboard",
                    None,
                    false,
                )
                .cursor_default(),
            );
            for (board, width) in crate::state::ARTBOARDS {
                insert_menu = insert_menu.child(
                    row(
                        t,
                        format!("artboard-{width}"),
                        None,
                        format!("{board} · {width}"),
                        None,
                        artboard == Some(board),
                    )
                    .pl(t.space.xl)
                    .on_click(act(Box::new(move |s, cx| {
                        s.design_set_artboard(Some((board, width)), cx)
                    }))),
                );
            }
            insert_menu = insert_menu.child(
                row(
                    t,
                    "artboard-panel",
                    None,
                    "The panel's width",
                    None,
                    artboard.is_none(),
                )
                .pl(t.space.xl)
                .on_click(act(Box::new(|s, cx| s.design_set_artboard(None, cx)))),
            );
        }
    }

    // The library's design systems: search, then one restyles the page.
    let words: Vec<String> = filter_text
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let shown: Vec<crate::state::SystemCard> = systems
        .as_deref()
        .map(|all| {
            all.iter()
                .filter(|c| {
                    let hay = format!("{} {} {}", c.title, c.id, c.description).to_lowercase();
                    words.iter().all(|w| hay.contains(w.as_str()))
                })
                .take(80)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let library_menu = sheet(t, 320.)
        .child(
            div()
                .m(t.space.xs)
                .px(t.space.sm)
                .py(t.space.xs)
                .rounded(t.radius.button)
                .bg(t.extra.inset)
                .child(
                    gpui::component::input::Input::new(&filter)
                        .appearance(false)
                        .text_size(t.text.body_sm),
                ),
        )
        .child(
            div()
                .px(t.space.md)
                .py(t.space.xs)
                .text_size(t.text.caption)
                .line_height(relative(1.4))
                .text_color(t.colors.text_subtle)
                .child(match &systems {
                    None => "Reading the library…".to_string(),
                    Some(all) if all.is_empty() => "No design library here.".to_string(),
                    Some(all) => format!(
                        "{} of {} design systems. One restyles the page: Forge reads its DESIGN.md.",
                        shown.len(),
                        all.len()
                    ),
                }),
        )
        .child(
            div()
                .id("design-library-list")
                .max_h(px(360.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .children(shown.into_iter().map(|card| {
                    let id = SharedString::from(format!("system-{}", card.id));
                    let (title, description) = (card.title.clone(), card.description.clone());
                    div()
                        .id(id)
                        .flex_none()
                        .flex()
                        .flex_col()
                        .px(t.space.md)
                        .py(t.space.xs)
                        .rounded(t.radius.button)
                        .cursor_pointer()
                        .hover(move |d| d.bg(hover))
                        .child(
                            div()
                                .text_size(t.text.body_sm)
                                .text_color(t.colors.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_subtle)
                                .child(description),
                        )
                        .on_click(act(Box::new(move |s, cx| s.design_apply_system(&card, cx))))
                })),
        );

    // The zoom: a step in or out, a desktop's width, or a size.
    let mut zoom_menu = sheet(t, 230.)
        .child(
            row(
                t,
                "zoom-in",
                Some("icons/zoom-in.svg"),
                "Zoom in",
                Some("secondary-="),
                false,
            )
            .on_click(act(Box::new(|s, cx| s.web_zoom_step(1, cx)))),
        )
        .child(
            row(
                t,
                "zoom-out",
                Some("icons/zoom-out.svg"),
                "Zoom out",
                Some("secondary--"),
                false,
            )
            .on_click(act(Box::new(|s, cx| s.web_zoom_step(-1, cx)))),
        )
        .child(
            row(
                t,
                "zoom-fit",
                Some("icons/scan.svg"),
                "Fit a desktop width",
                None,
                false,
            )
            .on_click(act(Box::new(|s, cx| s.web_zoom_fit(cx)))),
        )
        .child(gap(t));
    for z in crate::web_view::ZOOMS {
        let on = (z - zoom).abs() < 0.001;
        zoom_menu = zoom_menu.child(
            row(
                t,
                format!("zoom-{}", (z * 100.0) as u32),
                None,
                percent(z),
                (z == 1.0).then_some("secondary-0"),
                on,
            )
            .on_click(act(Box::new(move |s, cx| s.web_zoom_to(z, cx)))),
        );
    }
    // Then the canvas's own: layout guides, undo and redo.
    let faded = |d: gpui::Stateful<gpui::Div>, on: bool| if on { d } else { d.opacity(0.5) };
    zoom_menu = zoom_menu
        .child(gap(t))
        .child(
            row(
                t,
                "design-guides",
                Some("icons/layout-grid.svg"),
                "Layout guides",
                None,
                guides,
            )
            .on_click(act(Box::new(|s, cx| {
                s.design_set_guides(!s.design_guides, cx)
            }))),
        )
        .child(faded(
            row(
                t,
                "design-undo",
                Some("icons/undo-2.svg"),
                "Undo",
                None,
                false,
            )
            .on_click(act(Box::new(|s, cx| s.design_step(-1, cx)))),
            undo,
        ))
        .child(faded(
            row(
                t,
                "design-redo",
                Some("icons/redo-2.svg"),
                "Redo",
                None,
                false,
            )
            .on_click(act(Box::new(|s, cx| s.design_step(1, cx)))),
            redo,
        ));

    // Share: the page as a PDF, or as it is, elsewhere.
    let share_menu = sheet(t, 240.)
        .child(
            row(
                t,
                "share-pdf",
                Some("icons/file-down.svg"),
                "Save as PDF",
                None,
                false,
            )
            .on_click(act(Box::new(|s, cx| s.design_export_pdf(cx)))),
        )
        .child(
            row(
                t,
                "share-open",
                Some("icons/external-link.svg"),
                "Open in your browser",
                None,
                false,
            )
            .on_click(act(Box::new(move |_, cx| {
                cx.open_url(&crate::design::live::file_url(&open));
            }))),
        );

    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(t.space.xs)
        .px(t.space.sm)
        .py(t.space.xs)
        .bg(t.colors.surface)
        .border_b_1()
        .border_color(t.colors.border)
        .child(
            tool(
                t,
                "design-outline",
                "icons/panel-left.svg",
                None,
                false,
                outline,
                "The page's outline",
            )
            .on_click(cx.listener(|_, _, window, cx| {
                window.dispatch_action(Box::new(super::PageOutline), cx)
            })),
        )
        .child(
            div()
                .relative()
                .min_w_0()
                .child(
                    tool(
                        t,
                        "design-pages",
                        "icons/file-text.svg",
                        Some(name(&page)),
                        true,
                        menu == Some(Menu::Pages),
                        "The design's pages",
                    )
                    .on_click(toggle(Menu::Pages)),
                )
                .when(menu == Some(Menu::Pages), |d| {
                    d.child(hang(t, false, pages_menu, cx))
                }),
        )
        .child(
            div()
                .relative()
                .child(
                    tool(
                        t,
                        "design-insert",
                        in_hand.map_or("icons/plus.svg", tool_icon),
                        None,
                        true,
                        menu == Some(Menu::Insert) || in_hand.is_some(),
                        "Insert: draw on the page",
                    )
                    .on_click(toggle(Menu::Insert)),
                )
                .when(menu == Some(Menu::Insert), |d| {
                    d.child(hang(t, false, insert_menu, cx))
                }),
        )
        .child(
            tool(
                t,
                "design-select",
                "icons/mouse-pointer-2.svg",
                None,
                false,
                select,
                "Comment on a part: point at it, then click",
            )
            .on_click(cx.listener(|_, _, window, cx| {
                window.dispatch_action(Box::new(super::CommentTool), cx)
            })),
        )
        .child(
            div()
                .relative()
                .flex()
                .items_center()
                .child(
                    tool(
                        t,
                        "design-pins",
                        "icons/message-square.svg",
                        (!comments.is_empty()).then(|| open_count.to_string()),
                        false,
                        pins && !comments.is_empty(),
                        "Show or hide the comments on the page",
                    )
                    .on_click(act(Box::new(|s, cx| {
                        s.design_pins = !s.design_pins;
                        cx.notify();
                    }))),
                )
                .child(
                    tool(
                        t,
                        "design-comments",
                        "icons/chevron-down.svg",
                        None,
                        false,
                        menu == Some(Menu::Comments),
                        "The comments",
                    )
                    .min_w(px(20.))
                    .on_click(toggle(Menu::Comments)),
                )
                .when(menu == Some(Menu::Comments), |d| {
                    d.child(hang(t, false, comments_menu, cx))
                }),
        )
        .child(div().flex_1())
        .child(
            div()
                .relative()
                .child(
                    tool(
                        t,
                        "design-library",
                        "icons/book-open.svg",
                        None,
                        false,
                        menu == Some(Menu::Library),
                        "Library: restyle the page with a design system",
                    )
                    .on_click({
                        let state = state.clone();
                        cx.listener(move |this: &mut BrowserView, _: &gpui::ClickEvent, _, cx| {
                            let open = this.design_menu() != Some(Menu::Library);
                            this.set_menu(open.then_some(Menu::Library), cx);
                            state.update(cx, |s, cx| s.design_load_systems(cx));
                        })
                    }),
                )
                .when(menu == Some(Menu::Library), |d| {
                    d.child(hang(t, false, library_menu, cx))
                }),
        )
        .child(
            tool(
                t,
                "design-present",
                "icons/play.svg",
                None,
                false,
                false,
                "Present (F11)",
            )
            .on_click(
                cx.listener(|_, _, window, cx| {
                    window.dispatch_action(Box::new(super::Present), cx)
                }),
            ),
        )
        .child(
            div()
                .relative()
                .child(
                    tool(
                        t,
                        "design-zoom",
                        "icons/zoom-in.svg",
                        Some(percent(zoom)),
                        true,
                        menu == Some(Menu::Zoom),
                        "Zoom",
                    )
                    .on_click(toggle(Menu::Zoom)),
                )
                .when(menu == Some(Menu::Zoom), |d| {
                    d.child(hang(t, true, zoom_menu, cx))
                }),
        )
        .child(
            div()
                .relative()
                .child(
                    tool(
                        t,
                        "design-share",
                        "icons/share-2.svg",
                        Some("Share".into()),
                        true,
                        menu == Some(Menu::Share),
                        "Save or open the page",
                    )
                    .on_click(toggle(Menu::Share)),
                )
                .when(menu == Some(Menu::Share), |d| {
                    d.child(hang(t, true, share_menu, cx))
                }),
        )
}
