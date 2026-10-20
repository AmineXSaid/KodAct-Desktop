//! A workbook in the side panel, as a spreadsheet looks: its sheets as tabs,
//! column letters across the top, row numbers down the side, numbers aligned
//! right and in the workbook's own formats, its bold, fills, font colours and
//! column widths, a line under the frozen header. Click a cell (Shift-click
//! for a range) and the formula bar shows its address and formula. Read by
//! `office.rs` and `xlsx_style.rs`.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use crate::office::{column_letters, Sheet};
use crate::state::AppState;
use crate::theme::Theme;

/// A column's width: the workbook's own (Excel counts characters), else
/// from its longest cell, within reason.
fn width(sheet: &Sheet, c: usize) -> f32 {
    if let Some(w) = sheet.widths.get(c).copied().flatten() {
        return (w * 7.2 + 8.).clamp(36., 360.);
    }
    let longest = sheet
        .rows
        .iter()
        .take(200)
        .filter_map(|r| r.get(c))
        .map(|cell| cell.text.chars().count())
        .max()
        .unwrap_or(0);
    (longest as f32 * 7.4 + 20.).clamp(56., 260.)
}

/// A colour from the workbook, made to sit in this theme: as it is on
/// paper; at night a fill is a tint and a font colour lifts toward white.
fn fill_color(rgb: u32, t: &Theme) -> gpui::Hsla {
    let c: gpui::Hsla = gpui::rgb(rgb).into();
    if t.is_dark {
        c.opacity(0.28)
    } else {
        c
    }
}

fn font_color(rgb: u32, t: &Theme) -> gpui::Hsla {
    let c: gpui::Hsla = gpui::rgb(rgb).into();
    if t.is_dark {
        gpui::hsla(c.h, c.s, c.l.max(0.72), 1.)
    } else {
        c
    }
}

/// The cells picked: from the anchor to the other end, inclusive.
pub type Pick = Option<((usize, usize), (usize, usize))>;

/// `A1`, or `B2:C5` for a range.
pub fn address(pick: ((usize, usize), (usize, usize))) -> String {
    let ((r0, c0), (r1, c1)) = pick;
    let (top, left, bottom, right) = (r0.min(r1), c0.min(c1), r0.max(r1), c0.max(c1));
    let a = format!("{}{}", column_letters(left), top + 1);
    if (top, left) == (bottom, right) {
        a
    } else {
        format!("{a}:{}{}", column_letters(right), bottom + 1)
    }
}

fn picked(pick: Pick, r: usize, c: usize) -> bool {
    pick.is_some_and(|((r0, c0), (r1, c1))| {
        (r0.min(r1)..=r0.max(r1)).contains(&r) && (c0.min(c1)..=c0.max(c1)).contains(&c)
    })
}

pub fn render(
    sheets: &[Sheet],
    selected: usize,
    pick: Pick,
    changed: Option<
        &std::collections::HashMap<String, std::collections::HashMap<(usize, usize), String>>,
    >,
    state: Entity<AppState>,
    t: &Theme,
) -> AnyElement {
    let selected = selected.min(sheets.len().saturating_sub(1));
    let Some(sheet) = sheets.get(selected) else {
        return div()
            .text_color(t.colors.text_muted)
            .child("This workbook has no sheets.")
            .into_any_element();
    };
    let hover = t.colors.hover;
    let line = t.colors.border;
    // The cells this version changed, and what they held before.
    let changed = changed.and_then(|c| c.get(&sheet.name));
    let gutter = 44.;
    let widths: Vec<f32> = (0..sheet.rows.first().map_or(0, |r| r.len()))
        .map(|c| width(sheet, c))
        .collect();
    // A workbook with no styles of its own: its first row reads as a header
    // when it is all text over a row with numbers.
    let styled = sheet
        .rows
        .iter()
        .flatten()
        .any(|c| c.look.bold || c.look.fill.is_some());
    let header = !styled
        && sheet.rows.len() > 1
        && sheet.rows[0].iter().all(|c| !c.numeric)
        && sheet.rows[1].iter().any(|c| c.numeric);

    let cell = |w: f32| {
        div()
            .flex_none()
            .w(px(w))
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .border_r_1()
            .border_b_1()
            .border_color(line)
            .overflow_hidden()
            .whitespace_nowrap()
    };
    let mut grid = div().flex().flex_col().text_size(t.text.body_sm);
    // Column letters.
    let mut letters = div()
        .flex()
        .bg(t.extra.inset)
        .text_size(t.text.caption)
        .text_color(t.colors.text_muted)
        .child(cell(gutter));
    for (c, w) in widths.iter().enumerate() {
        letters = letters.child(cell(*w).justify_center().child(column_letters(c)));
    }
    grid = grid.child(letters);
    for (r, row) in sheet.rows.iter().enumerate() {
        let mut line_el = div().flex().child(
            cell(gutter)
                .justify_end()
                .bg(t.extra.inset)
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .child((r + 1).to_string()),
        );
        let frozen_edge = sheet.frozen_rows > 0 && r + 1 == sheet.frozen_rows;
        for (c, w) in widths.iter().enumerate() {
            let value = row.get(c).cloned().unwrap_or_default();
            let bold = value.look.bold || (header && r == 0);
            let on = picked(pick, r, c);
            let st = state.clone();
            let was = changed.and_then(|m| m.get(&(r, c))).cloned();
            line_el = line_el.child(
                cell(*w)
                    .id(SharedString::from(format!("cell-{r}-{c}")))
                    .cursor_pointer()
                    .when(value.numeric, |d| d.justify_end())
                    .when_some(value.look.fill, |d, f| d.bg(fill_color(f, t)))
                    .when(bold, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .when(value.look.italic, |d| d.italic())
                    .text_color(match value.look.color {
                        Some(rgb) => font_color(rgb, t),
                        None if bold => t.colors.text_strong,
                        None => t.colors.text,
                    })
                    .when(frozen_edge, |d| d.border_b_2())
                    .when_some(was, |d, was| {
                        let tip: SharedString = if was.is_empty() {
                            "New in this version".into()
                        } else {
                            format!("Was {was}").into()
                        };
                        d.bg(t.extra.warning_tint).tooltip(move |w, cx| {
                            gpui::component::tooltip::Tooltip::new(tip.clone()).build(w, cx)
                        })
                    })
                    .when(on, |d| {
                        d.bg(t.colors.primary_tint).border_color(t.colors.primary)
                    })
                    .child(value.text)
                    .on_click(move |e, _, cx| {
                        let extend = e.modifiers().shift;
                        st.update(cx, |s, cx| s.pick_cell((r, c), extend, cx))
                    }),
            );
        }
        grid = grid.child(line_el);
    }

    let tabs = (sheets.len() > 1).then(|| {
        let mut row = div().flex().gap(px(2.)).pb(t.space.sm);
        for (i, s) in sheets.iter().enumerate() {
            let on = i == selected;
            let st = state.clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("sheet-{i}")))
                    .px(t.space.md)
                    .h(t.size.control)
                    .flex()
                    .items_center()
                    .rounded(t.radius.button)
                    .cursor_pointer()
                    .text_size(t.text.caption)
                    .when(on, |d| {
                        d.bg(t.colors.primary_tint)
                            .text_color(t.colors.primary)
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!on, |d| {
                        d.text_color(t.colors.text_muted)
                            .hover(move |st| st.bg(hover))
                    })
                    .child(s.name.clone())
                    .on_click(move |_, _, cx| st.update(cx, |s, cx| s.select_sheet(i, cx))),
            );
        }
        row
    });

    let shown = sheet.rows.len();
    let note = (sheet.total_rows > shown || sheet.total_cols > widths.len()).then(|| {
        format!(
            "Showing {shown} of {} rows and {} of {} columns.",
            sheet.total_rows,
            widths.len(),
            sheet.total_cols
        )
    });

    // The formula bar: the picked cell's address, and its formula (or value).
    let bar = pick.map(|p| {
        let ((r, c), _) = p;
        let cell = sheet
            .rows
            .get(r)
            .and_then(|row| row.get(c))
            .cloned()
            .unwrap_or_default();
        let content = cell.look.formula.clone().unwrap_or(cell.text);
        div()
            .flex()
            .items_center()
            .gap(t.space.sm)
            .h(t.size.control + px(2.))
            .px(t.space.sm)
            .rounded(t.radius.button)
            .border_1()
            .border_color(line)
            .bg(t.colors.surface)
            .text_size(t.text.caption)
            .child(
                div()
                    .flex_none()
                    .min_w(px(56.))
                    .font_family(t.font_mono.clone())
                    .text_color(t.colors.text_muted)
                    .child(address(p)),
            )
            .child(div().w(t.size.hairline).h(px(14.)).bg(line))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(t.font_mono.clone())
                    .text_color(t.colors.text)
                    .child(content),
            )
    });

    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .children(tabs)
        .children(bar)
        .child(
            div()
                .id(SharedString::from(format!("sheet-grid-{selected}")))
                .w_full()
                .overflow_x_scroll()
                .rounded(t.radius.button)
                .border_l_1()
                .border_t_1()
                .border_color(line)
                .bg(t.colors.surface)
                .child(grid),
        )
        .children(note.map(|n| {
            div()
                .text_size(t.text.caption)
                .text_color(t.colors.text_muted)
                .line_height(relative(1.4))
                .child(n)
        }))
        .into_any_element()
}
