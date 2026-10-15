//! The project map (Ctrl/⌘ Shift M, the palette, or the map button on a
//! project's page): the project at a glance, its top-level folders by size
//! and the files beside them (read off the UI thread with the git-aware
//! walker in `rules.rs`), and the files the open chat changed, from its
//! Write and Edit rows, with what you did about each (Keep / Revert).
//! Click a changed file to open it; a click outside or Esc closes the map.

use gpui::{div, prelude::*, px, relative, Entity, FontWeight, IntoElement, SharedString};

use super::icon;
use crate::state::{AppState, Review};
use crate::theme::{self, Theme};

fn heading(t: &Theme, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(t.text.caption)
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.colors.text_subtle)
        .child(text.into())
}

fn bar(t: &Theme, share: f32) -> gpui::Div {
    div()
        .w(px(80.))
        .h(px(4.))
        .flex_none()
        .rounded(t.radius.pill)
        .bg(t.colors.hover)
        .child(
            div()
                .h_full()
                .w(relative(share.clamp(0.02, 1.)))
                .rounded(t.radius.pill)
                .bg(t.colors.primary.opacity(0.6)),
        )
}

pub fn render(t: &Theme, state: &Entity<AppState>, cx: &gpui::App) -> impl IntoElement {
    let s = state.read(cx);
    let map = s.map.clone();
    let edited = s.edited_files();
    let project = s.project.clone();
    let project_dir = s.project_dir.clone();
    let has_chat = s.chat().is_some_and(|c| !c.messages.is_empty());

    // The folders.
    let mut tree = div().flex().flex_col().gap(px(2.));
    match &map {
        None => {
            tree = tree.child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.space.sm)
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child(icon(
                        "icons/loader-circle.svg",
                        t.size.icon_xs,
                        t.colors.text_muted,
                    ))
                    .child("Reading the project…"),
            )
        }
        Some(m) if m.total == 0 => {
            tree = tree.child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_muted)
                    .child("No files here yet (or all of them are ignored by git)."),
            )
        }
        Some(m) => {
            let most = m.folders.first().map_or(1, |f| f.1).max(1) as f32;
            for (name, n) in m.folders.iter().take(14) {
                tree = tree.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .h(t.size.row)
                        .child(icon(
                            "icons/folder.svg",
                            t.size.icon_sm,
                            t.colors.text_muted,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(t.text.body_sm)
                                .text_color(t.colors.text)
                                .child(format!("{name}/")),
                        )
                        .child(bar(t, *n as f32 / most))
                        .child(
                            div()
                                .w(px(48.))
                                .flex_none()
                                .text_size(t.text.caption)
                                .font_family(t.font_mono.clone())
                                .text_color(t.colors.text_subtle)
                                .child(n.to_string()),
                        ),
                );
            }
            if m.folders.len() > 14 {
                tree = tree.child(
                    div()
                        .text_size(t.text.caption)
                        .text_color(t.colors.text_subtle)
                        .child(format!("and {} more folders", m.folders.len() - 14)),
                );
            }
            if !m.files.is_empty() {
                let shown: Vec<String> = m.files.iter().take(8).cloned().collect();
                let more = m.files.len().saturating_sub(8);
                tree = tree.child(
                    div()
                        .pt(t.space.sm)
                        .flex()
                        .items_start()
                        .gap(t.space.sm)
                        .child(div().pt(px(2.)).child(icon(
                            "icons/file.svg",
                            t.size.icon_sm,
                            t.colors.text_muted,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(t.text.caption)
                                .text_color(t.colors.text_muted)
                                .line_height(relative(1.5))
                                .child(if more > 0 {
                                    format!("{} and {more} more", shown.join(", "))
                                } else {
                                    shown.join(", ")
                                }),
                        ),
                );
            }
        }
    }
    let summary = map.as_ref().map(|m| {
        format!(
            "{}{} files in {} folders",
            if m.capped { "Over " } else { "" },
            m.total,
            m.folders.len()
        )
    });

    // What this chat changed.
    let mut changed = div().flex().flex_col().gap(px(2.));
    if edited.is_empty() {
        changed = changed.child(
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .line_height(relative(1.45))
                .child(if has_chat {
                    "This chat hasn't changed a file yet."
                } else {
                    "Open a chat to see the files it changed."
                }),
        );
    }
    for (i, e) in edited.iter().enumerate() {
        let (label, fg, bg) = match e.review {
            Review::Open => ("To review", t.extra.warning, t.extra.warning_tint),
            Review::Kept => ("Kept", t.extra.success, t.extra.success_tint),
            Review::Reverted => ("Reverted", t.colors.text_muted, t.colors.hover),
        };
        let full = {
            let p = std::path::Path::new(&e.path);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                project_dir.join(p)
            }
        };
        let hover = t.colors.hover;
        changed = changed.child(
            div()
                .id(SharedString::from(format!("map-edited-{i}")))
                .flex()
                .items_center()
                .gap(t.space.sm)
                .h(t.size.row)
                .px(t.space.xs)
                .rounded(t.radius.button)
                .cursor_pointer()
                .hover(move |st| st.bg(hover))
                .child({
                    let (path, color) = super::file_icon::for_name(&e.path, false, t);
                    icon(path, t.size.icon_sm, color)
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(t.text.body_sm)
                        .font_family(t.font_mono.clone())
                        .text_color(t.colors.text)
                        .child(e.path.clone()),
                )
                .when(e.times > 1, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child(format!("×{}", e.times)),
                    )
                })
                .child(theme::pill(t, bg, fg, label).flex_none())
                .tooltip(|window, cx| {
                    gpui::component::tooltip::Tooltip::new("Open the file").build(window, cx)
                })
                .on_click(move |_, _, cx| cx.open_with_system(&full)),
        );
    }

    let close = state.clone();
    div()
        .id("map-scrim")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(t.extra.scrim)
        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
            close.update(cx, |s, cx| s.toggle_map(cx))
        })
        .child(
            theme::popover(t)
                .id("project-map")
                .occlude()
                .w(px(720.))
                .max_w(relative(0.94))
                .max_h(relative(0.85))
                .overflow_y_scroll()
                .p(t.space.xl)
                .flex()
                .flex_col()
                .gap(t.space.lg)
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(t.space.sm)
                        .child(icon("icons/map.svg", t.size.icon_lg, t.colors.primary))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(t.text.subheading)
                                        .font_family(t.font_display.clone())
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(t.colors.text_strong)
                                        .child(project),
                                )
                                .children(summary.map(|s| {
                                    div()
                                        .text_size(t.text.caption)
                                        .text_color(t.colors.text_subtle)
                                        .child(s)
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(t.space.xl)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(t.space.sm)
                                .child(heading(t, "Folders, by files"))
                                .child(tree),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(t.space.sm)
                                .child(heading(t, "Changed in this chat"))
                                .child(changed),
                        ),
                ),
        )
}

#[cfg(test)]
mod tests {
    use crate::state::ProjectMap;

    #[test]
    fn folders_are_summed_biggest_first() {
        let paths: Vec<String> = [
            "src/a.rs",
            "src/ui/b.rs",
            "docs/x.md",
            "README.md",
            "src/c.rs",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let m = ProjectMap::from_files("/p".into(), &paths);
        assert_eq!(m.folders, vec![("src".into(), 3), ("docs".into(), 1)]);
        assert_eq!(m.files, vec!["README.md".to_string()]);
        assert_eq!(m.total, 5);
        assert!(!m.capped);
    }
}
