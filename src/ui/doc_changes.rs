//! A file's versions in the side panel: the bar at the panel's foot that
//! steps between them ("Version 2 of 3"), as Claude's version picker does,
//! and the Changes view: what this version changed from the one before,
//! paragraph by paragraph, the reworded words marked (`text_diff.rs`).

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, StyledText, TextRun};

use super::icon;
use crate::state::AppState;
use crate::text_diff::{Para, Run};
use crate::theme::Theme;

/// The bar: back and forward through the versions, which one this is, and
/// the Changes switch (from the second version on).
pub fn version_bar(
    t: &Theme,
    state: Entity<AppState>,
    at: usize,
    total: usize,
    showing_changes: bool,
    summary: Option<String>,
) -> gpui::Div {
    let step = |id: &'static str, path: &'static str, tip: &'static str, by: isize, on: bool| {
        let st = state.clone();
        super::button::Button::icon(id, path, tip)
            .compact()
            .disabled(!on)
            .on_click(move |_, _, cx| st.update(cx, |s, cx| s.step_doc_version(by, cx)))
    };
    let st = state.clone();
    let hover = t.colors.hover;
    div()
        .flex_none()
        .h(px(40.))
        .flex()
        .items_center()
        .gap(t.space.xs)
        .px(t.space.sm)
        .border_t_1()
        .border_color(t.colors.border)
        .text_size(t.text.caption)
        .child(step(
            "version-back",
            "icons/arrow-left.svg",
            "The version before",
            -1,
            at > 0,
        ))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.colors.text)
                .child(format!("Version {} of {total}", at + 1)),
        )
        .child(step(
            "version-next",
            "icons/arrow-right.svg",
            "The version after",
            1,
            at + 1 < total,
        ))
        .child(div().flex_1())
        .children(summary.map(|s| div().text_color(t.colors.text_muted).child(s)))
        .when(at > 0, |d| {
            d.child(
                div()
                    .id("version-changes")
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .h(t.size.control)
                    .px(t.space.sm)
                    .rounded(t.radius.compact)
                    .cursor_pointer()
                    .border_1()
                    .border_color(if showing_changes {
                        t.colors.primary
                    } else {
                        t.colors.border
                    })
                    .when(showing_changes, |d| {
                        d.bg(t.colors.primary_tint).text_color(t.colors.primary)
                    })
                    .when(!showing_changes, |d| {
                        d.text_color(t.colors.text).hover(move |s| s.bg(hover))
                    })
                    .child(icon(
                        "icons/history.svg",
                        t.size.icon_xs,
                        if showing_changes {
                            t.colors.primary
                        } else {
                            t.colors.text_muted
                        },
                    ))
                    .child("Changes")
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.toggle_doc_changes(cx))),
            )
        })
}

/// Markdown's marks dropped, for reading a paragraph as words.
fn plain(text: &str) -> String {
    text.replace("**", "")
        .replace('`', "")
        .trim_start_matches(|c| c == '#' || c == ' ')
        .to_string()
}

/// One paragraph's runs, the added ones on a green wash and the removed
/// ones struck through in red.
fn runs(parts: &[Run], t: &Theme) -> StyledText {
    let mut text = String::new();
    let mut out = vec![];
    let font = gpui::font(t.font_reading.clone());
    for (i, part) in parts.iter().enumerate() {
        let (words, color, background, strike) = match part {
            Run::Same(w) => (plain(w), t.colors.text_body, None, false),
            Run::Added(w) => (plain(w), t.extra.success, Some(t.extra.success_tint), false),
            Run::Removed(w) => (plain(w), t.extra.danger, Some(t.extra.danger_tint), true),
        };
        // A space before the next run, unless a line break sits between.
        let next_breaks = parts.get(i + 1).is_some_and(|p| match p {
            Run::Same(w) | Run::Added(w) | Run::Removed(w) => w.starts_with('\n'),
        });
        let piece = if i + 1 < parts.len() && !words.ends_with('\n') && !next_breaks {
            format!("{words} ")
        } else {
            words
        };
        out.push(TextRun {
            len: piece.len(),
            font: font.clone(),
            color,
            background_color: background,
            underline: None,
            strikethrough: strike.then(|| gpui::StrikethroughStyle {
                thickness: px(1.),
                color: Some(t.extra.danger),
            }),
        });
        text.push_str(&piece);
    }
    StyledText::new(text).with_runs(out)
}

/// What this version changed, paragraph by paragraph. Long stretches of
/// unchanged paragraphs fold to a line saying how many.
pub fn text_view(paras: &[Para], t: &Theme) -> AnyElement {
    let (changed, added, removed) = crate::text_diff::counts(paras);
    if changed + added + removed == 0 {
        return div()
            .text_size(t.text.body_sm)
            .text_color(t.colors.text_muted)
            .child("Nothing changed in the text from the version before.")
            .into_any_element();
    }
    let edge = |color: gpui::Hsla| {
        div()
            .pl(t.space.md)
            .border_l_2()
            .border_color(color)
            .line_height(relative(1.6))
    };
    let mut out = div()
        .flex()
        .flex_col()
        .gap(t.space.md)
        .text_size(t.size.doc_text);
    let mut i = 0;
    while i < paras.len() {
        let same = paras[i..]
            .iter()
            .take_while(|p| matches!(p, Para::Same(_)))
            .count();
        if same > 2 {
            out = out.child(
                div()
                    .text_size(t.text.caption)
                    .text_color(t.colors.text_subtle)
                    .child(format!("{same} unchanged paragraphs")),
            );
            i += same;
            continue;
        }
        out = out.child(match &paras[i] {
            Para::Same(p) => div()
                .text_color(t.colors.text_muted)
                .line_height(relative(1.6))
                .child(plain(p)),
            Para::Added(p) => edge(t.extra.success).child(
                div()
                    .text_color(t.colors.text_body)
                    .bg(t.extra.success_tint)
                    .child(plain(p)),
            ),
            Para::Removed(p) => edge(t.extra.danger).child(runs(&[Run::Removed(p.clone())], t)),
            Para::Changed(parts) => edge(t.colors.primary).child(runs(parts, t)),
        });
        i += 1;
    }
    out.into_any_element()
}

/// "2 changed · 1 added · 1 removed", or None when nothing changed.
pub fn summary(paras: &[Para]) -> Option<String> {
    let (c, a, r) = crate::text_diff::counts(paras);
    let parts: Vec<String> = [(c, "changed"), (a, "added"), (r, "removed")]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, w)| format!("{n} {w}"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}
