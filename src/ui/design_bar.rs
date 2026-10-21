//! The browser's bar when it shows a page this chat designed, with the
//! controls of a design canvas (Claude's has the same): the page, whose
//! menu lists the chat's pages, asks for a new one and says where the file
//! is; then Present, the zoom (in, out, a desktop's width, 25 to 200 %) and
//! Share (a PDF beside the page, its path, your own browser). Each menu
//! hangs under its button, over the page.

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
    Zoom,
    Share,
}

/// What the bar shows.
pub struct Bar {
    /// The page on screen.
    pub page: PathBuf,
    /// The chat's pages, the one on screen among them.
    pub pages: Vec<PathBuf>,
    pub zoom: f64,
    pub menu: Option<Menu>,
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
    for (i, p) in pages.iter().enumerate() {
        let to = p.clone();
        pages_menu = pages_menu.child(
            row(
                t,
                format!("design-page-{i}"),
                Some("icons/file.svg"),
                name(p),
                None,
                *p == page,
            )
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
        .child(div().flex_1())
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
