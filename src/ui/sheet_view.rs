//! A workbook in the side panel, as a spreadsheet looks: its sheets as tabs,
//! column letters across the top, row numbers down the side, numbers aligned
//! right, each column as wide as its longest cell. Read by `office.rs`.

use gpui::{div, prelude::*, px, relative, AnyElement, Entity, FontWeight, SharedString};

use crate::office::{column_letters, Sheet};
use crate::state::AppState;
use crate::theme::Theme;

/// A column's width from its longest cell, within reason.
fn width(sheet: &Sheet, c: usize) -> f32 {
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

pub fn render(sheets: &[Sheet], selected: usize, state: Entity<AppState>, t: &Theme) -> AnyElement {
    let selected = selected.min(sheets.len().saturating_sub(1));
    let Some(sheet) = sheets.get(selected) else {
        return div()
            .text_color(t.colors.text_muted)
            .child("This workbook has no sheets.")
            .into_any_element();
    };
    let hover = t.colors.hover;
    let line = t.colors.border;
    let gutter = 44.;
    let widths: Vec<f32> = (0..sheet.rows.first().map_or(0, |r| r.len()))
        .map(|c| width(sheet, c))
        .collect();
    // The first row reads as a header when it is all text over a row with numbers.
    let header = sheet.rows.len() > 1
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
        for (c, w) in widths.iter().enumerate() {
            let value = row.get(c).cloned().unwrap_or_default();
            let bold = header && r == 0;
            line_el = line_el.child(
                cell(*w)
                    .when(value.numeric, |d| d.justify_end())
                    .when(bold, |d| {
                        d.font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.colors.text_strong)
                    })
                    .when(!bold, |d| d.text_color(t.colors.text))
                    .child(value.text),
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

    div()
        .flex()
        .flex_col()
        .gap(t.space.sm)
        .children(tabs)
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
