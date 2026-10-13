//! Attachments: the file picker behind the composer's +, and the chips
//! that show attached files (in the composer, removable; in your sent
//! message, read-only). An image's chip shows the picture itself.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    div, img, prelude::*, px, AnimationExt, AnyElement, Entity, Image, ImageFormat, ObjectFit,
    PathPromptOptions, SharedString,
};

use super::icon;
use crate::attach::{Attachment, Kind};
use crate::state::AppState;
use crate::theme::Theme;

/// Ask for files with the system picker and attach them.
pub fn pick(state: Entity<AppState>, cx: &mut gpui::App) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("Attach".into()),
    });
    cx.spawn(async move |cx| {
        let result = picked.await;
        state.update(cx, |s, cx| match result {
            Ok(Ok(Some(paths))) => s.add_attachment_paths(paths, cx),
            Ok(Ok(None)) => {}
            _ => s.show_toast("The file picker couldn't open on this system.", cx),
        });
    })
    .detach();
}

thread_local! {
    /// Decoded thumbnails, by the attachment's bytes: hashing a picture on
    /// every frame would cost more than drawing it.
    static THUMBS: RefCell<HashMap<(usize, usize), Arc<Image>>> = RefCell::new(HashMap::new());
}

fn thumbnail(a: &Attachment) -> Option<Arc<Image>> {
    let (Kind::Image(mime), Some(bytes)) = (a.kind, a.bytes.as_ref()) else {
        return None;
    };
    let format = match mime {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/gif" => ImageFormat::Gif,
        "image/webp" => ImageFormat::Webp,
        _ => return None,
    };
    let k = (bytes.as_ptr() as usize, bytes.len());
    THUMBS.with(|t| {
        let mut t = t.borrow_mut();
        if t.len() > 64 {
            t.clear();
        }
        Some(
            t.entry(k)
                .or_insert_with(|| Arc::new(Image::from_bytes(format, bytes.to_vec())))
                .clone(),
        )
    })
}

/// The colour a kind's icon takes, so a sheet, a PDF and an archive tell
/// apart at a glance.
fn kind_color(kind: Kind, t: &Theme) -> gpui::Hsla {
    use crate::attach::Doc;
    match kind {
        Kind::Pdf => t.extra.danger,
        Kind::Document(Doc::Sheet) => t.extra.success,
        Kind::Document(Doc::Word) => t.colors.sky,
        Kind::Document(Doc::Slides) => t.colors.marigold,
        Kind::Image(_) => t.colors.primary,
        _ => t.colors.text_muted,
    }
}

/// One chip: the picture (or the file's icon), its name and size, and a
/// remove button when `remove` is given.
fn chip(
    a: &Attachment,
    id: SharedString,
    t: &Theme,
    remove: Option<Box<dyn Fn(&mut gpui::App) + 'static>>,
) -> AnyElement {
    let hover = t.colors.hover;
    let thumb = thumbnail(a);
    let lead: AnyElement = match thumb {
        Some(i) => div()
            .size(t.size.attach_thumb)
            .flex_none()
            .rounded(t.radius.small)
            .overflow_hidden()
            .child(img(i).size_full().object_fit(ObjectFit::Cover))
            .into_any_element(),
        None => div()
            .size(t.size.attach_thumb)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(t.radius.small)
            .bg(t.extra.inset)
            .child(if a.pending {
                icon(
                    "icons/loader-circle.svg",
                    t.size.icon_sm,
                    t.colors.text_muted,
                )
                .with_animation(
                    SharedString::from(format!("{id}-spin")),
                    gpui::Animation::new(std::time::Duration::from_millis(900)).repeat(),
                    |svg, d| {
                        svg.with_transformation(gpui::Transformation::rotate(gpui::percentage(d)))
                    },
                )
                .into_any_element()
            } else {
                icon(a.icon(), t.size.icon_sm, kind_color(a.kind, t)).into_any_element()
            })
            .into_any_element(),
    };
    // The tooltip: where it came from, where its copy is, and any note.
    let mut tip = a
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| a.name.to_string());
    if let Some(at) = &a.saved_at {
        tip.push_str(&format!("\nIn the project at {at}"));
    }
    if let Some(x) = &a.extracted {
        tip.push_str(&format!("\nOpened into {}/", x.to));
    }
    if let Some(n) = &a.note {
        let mut c = n.chars();
        let first: String = c
            .next()
            .map(|f| f.to_uppercase().collect())
            .unwrap_or_default();
        tip.push_str(&format!("\n{first}{}", c.as_str()));
    }
    let where_: SharedString = tip.into();
    let warn = a.note.is_some()
        && matches!(
            a.kind,
            Kind::Archive | Kind::File | Kind::Document(_) | Kind::Pdf
        );
    div()
        .id(id.clone())
        .flex()
        .items_center()
        .gap(t.space.sm)
        .max_w(t.size.attach_chip)
        .pl(px(4.))
        .pr(t.space.sm)
        .py(px(4.))
        .rounded(t.radius.button)
        .border_1()
        .border_color(t.colors.border)
        .bg(t.colors.surface)
        .child(lead)
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
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(t.colors.text)
                        .child(a.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.))
                        .text_size(t.text.caption - px(1.))
                        .text_color(t.colors.text_subtle)
                        .when(warn, |d| {
                            d.child(icon(
                                "icons/triangle-alert.svg",
                                t.size.icon_xs,
                                t.extra.warning,
                            ))
                        })
                        .child(a.summary()),
                ),
        )
        .tooltip(move |window, cx| {
            gpui::component::tooltip::Tooltip::new(where_.clone()).build(window, cx)
        })
        .children(remove.map(|remove| {
            div()
                .id(SharedString::from(format!("{id}-x")))
                .flex_none()
                .p(px(2.))
                .rounded(t.radius.small)
                .cursor_pointer()
                .hover(move |d| d.bg(hover))
                .child(icon("icons/x.svg", t.size.icon_xs, t.colors.text_muted))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    remove(cx)
                })
        }))
        .into_any_element()
}

/// The composer's pending attachments, each removable.
pub fn pending(t: &Theme, state: &Entity<AppState>, cx: &gpui::App) -> Option<AnyElement> {
    let list = &state.read(cx).attachments;
    if list.is_empty() {
        return None;
    }
    let mut row = div()
        .flex()
        .flex_wrap()
        .gap(t.space.xs)
        .px(px(10.))
        .pt(px(10.));
    for (i, a) in list.iter().enumerate() {
        let st = state.clone();
        row = row.child(chip(
            a,
            format!("attach-{i}").into(),
            t,
            Some(Box::new(move |cx| {
                st.update(cx, |s, cx| s.remove_attachment(i, cx))
            })),
        ));
    }
    Some(row.into_any_element())
}

/// The attachments of a sent message (read-only).
pub fn sent(t: &Theme, list: &[Attachment], key: &str) -> Option<AnyElement> {
    if list.is_empty() {
        return None;
    }
    let mut row = div().flex().flex_wrap().justify_end().gap(t.space.xs);
    for (i, a) in list.iter().enumerate() {
        row = row.child(chip(a, format!("sent-{key}-{i}").into(), t, None));
    }
    Some(row.into_any_element())
}
