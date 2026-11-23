//! A workbook in the side panel, as a spreadsheet looks: its sheets as tabs,
//! column letters across the top, row numbers down the side, numbers aligned
//! right and in the workbook's own formats, its bold, fills, font colours and
//! column widths, a line under the frozen header. Click a cell (Shift-click
//! for a range) and the formula bar shows its address and formula. Read by
//! `office.rs` and `xlsx_style.rs`.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FontWeight, IntoElement, Render,
    SharedString, Subscription, Window,
};

use super::icon;
use crate::office::{column_letters, Sheet};
use crate::state::AppState;
use crate::theme::{self, ActiveTheme, Theme};

/// Live interactive spreadsheet formula/action bar rendered above the grid.
pub struct SheetEditor {
    state: Entity<AppState>,
    input: Entity<InputState>,
    last_cell: Option<(usize, usize, usize, String)>,
    last_focus_seq: u64,
    _subs: Vec<Subscription>,
}

impl SheetEditor {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Pick a cell; double-click to edit, or type =formula")
        });
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.commit(true, cx);
                }
            }),
        ];
        Self {
            state,
            input,
            last_cell: None,
            last_focus_seq: 0,
            _subs: subs,
        }
    }

    fn current_cell_value(s: &AppState) -> Option<(usize, usize, usize, String)> {
        let sheet_idx = s.doc_sheet;
        let ((r, c), _) = s.doc_pick?;
        if let Some(edited) = s.sheet_edits.get(&(sheet_idx, r, c)) {
            return Some((sheet_idx, r, c, edited.clone()));
        }
        let val = s
            .doc_preview
            .as_ref()
            .and_then(|(_, res)| res.as_ref().ok())
            .and_then(|p| match p.as_ref() {
                crate::office::Preview::Sheets(sheets) => sheets.get(sheet_idx),
                _ => None,
            })
            .and_then(|sh| sh.rows.get(r))
            .and_then(|row| row.get(c))
            .map(|cell| cell.look.formula.clone().unwrap_or_else(|| cell.text.clone()))
            .unwrap_or_default();
        Some((sheet_idx, r, c, val))
    }

    fn commit(&mut self, advance: bool, cx: &mut Context<Self>) {
        let val = self.input.read(cx).value().to_string();
        let Some((si, r, c)) = self
            .state
            .read(cx)
            .doc_pick
            .map(|((r, c), _)| (self.state.read(cx).doc_sheet, r, c))
        else {
            return;
        };
        // Confirming a cell without changing it is not an edit: the box holds
        // the cell as shown ("27,500", "€1,172.28"), and writing that back
        // would turn a number into text.
        let unchanged = Self::current_cell_value(self.state.read(cx))
            .is_some_and(|(.., shown)| shown == val);
        self.last_cell = Some((si, r, c, val.clone()));
        self.state.update(cx, |s, cx| {
            if !unchanged {
                s.set_sheet_cell(si, r, c, val, cx);
            }
            if advance {
                s.pick_cell((r + 1, c), false, cx);
            }
        });
    }
}

impl Render for SheetEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let (cell_info, focus_seq, pick, sheet_idx) = {
            let s = self.state.read(cx);
            (
                Self::current_cell_value(s),
                s.sheet_focus_seq,
                s.doc_pick,
                s.doc_sheet,
            )
        };
        if self.last_cell != cell_info {
            self.last_cell = cell_info.clone();
            let text = cell_info
                .as_ref()
                .map(|(_, _, _, v)| v.clone())
                .unwrap_or_default();
            self.input
                .update(cx, |i, cx| i.set_value(text, window, cx));
        }
        if focus_seq != self.last_focus_seq {
            self.last_focus_seq = focus_seq;
            self.input.update(cx, |i, cx| i.focus(window, cx));
        }

        let line = t.colors.border;
        let has_pick = pick.is_some();
        let addr = pick.map(address).unwrap_or_else(|| "—".to_string());
        let st_row = self.state.clone();
        let st_col = self.state.clone();
        let current_input_val = self.input.read(cx).value().to_string();
        let synced_val = cell_info.map(|(_, _, _, v)| v).unwrap_or_default();
        let input_dirty = has_pick && current_input_val != synced_val;

        // One strip, as a spreadsheet's formula bar: the cell, its value or
        // formula, then adding rows and columns. Save and Discard are on the
        // file view's toolbar.
        let add = |id: &'static str, label: &'static str| {
            theme::ghost_button(&t, "")
                .id(id)
                .h(t.size.control)
                .px(t.space.sm)
                .gap(px(4.))
                .text_size(t.text.caption)
                .child(icon("icons/plus.svg", t.size.icon_xs, t.colors.text_muted))
                .child(label)
        };
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(t.space.xs)
            .child(
                div()
                    .flex_none()
                    .min_w(px(44.))
                    .h(t.size.control)
                    .px(t.space.sm)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(t.radius.field)
                    .bg(t.extra.sunk)
                    .border_1()
                    .border_color(line)
                    .font_family(t.font_mono.clone())
                    .text_size(t.text.caption)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if has_pick {
                        t.colors.text_strong
                    } else {
                        t.colors.text_subtle
                    })
                    .child(addr),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.))
                    .h(t.size.control)
                    .pl(t.space.sm)
                    .pr(px(2.))
                    .flex()
                    .items_center()
                    .gap(t.space.xs)
                    .rounded(t.radius.field)
                    .border_1()
                    .border_color(if input_dirty {
                        t.colors.primary.opacity(0.6)
                    } else {
                        line
                    })
                    .bg(t.colors.surface)
                    .child(
                        div()
                            .flex_none()
                            .font_family(t.font_mono.clone())
                            .text_size(t.text.caption)
                            .text_color(t.colors.text_subtle)
                            .child("fx"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_family(t.font_mono.clone())
                            .text_size(t.text.caption)
                            .child(Input::new(&self.input).appearance(false)),
                    )
                    .when(input_dirty, |d| {
                        d.child(
                            super::button::Button::icon(
                                "sheet-apply",
                                "icons/check.svg",
                                "Apply (Enter)",
                            )
                            .compact()
                            .on_click(cx.listener(|this, _, _, cx| this.commit(false, cx))),
                        )
                    }),
            )
            .child(add("sheet-add-row", "Row").on_click(move |_, _, cx| {
                st_row.update(cx, |s, cx| s.add_sheet_row(sheet_idx, cx))
            }))
            .child(add("sheet-add-col", "Column").on_click(move |_, _, cx| {
                st_col.update(cx, |s, cx| s.add_sheet_col(sheet_idx, cx))
            }))
    }
}

/// A column's width: the workbook's own (Excel counts characters), else
/// from its longest cell, within reason.
fn width(sheet: &Sheet, c: usize, edits: &crate::office::CellEdits, selected: usize) -> f32 {
    if let Some(w) = sheet.widths.get(c).copied().flatten() {
        return (w * 7.2 + 8.).clamp(48., 360.);
    }
    let mut longest = sheet
        .rows
        .iter()
        .take(200)
        .filter_map(|r| r.get(c))
        .map(|cell| cell.text.chars().count())
        .max()
        .unwrap_or(0);
    for (&(si, _, ec), val) in edits {
        if si == selected && ec == c {
            longest = longest.max(val.chars().count());
        }
    }
    (longest as f32 * 7.4 + 24.).clamp(64., 260.)
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

#[allow(clippy::too_many_arguments)]
pub fn render(
    sheets: &[Sheet],
    selected: usize,
    pick: Pick,
    changed: Option<
        &std::collections::HashMap<String, std::collections::HashMap<(usize, usize), String>>,
    >,
    edits: &crate::office::CellEdits,
    extra_rows: usize,
    extra_cols: usize,
    zoom: f32,
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
    let gutter = 44. * zoom;
    let sized = |size: gpui::Pixels| px(f32::from(size) * zoom);

    let max_edit_r = edits
        .keys()
        .filter(|&&(si, _, _)| si == selected)
        .map(|&(_, r, _)| r + 1)
        .max()
        .unwrap_or(0);
    let max_edit_c = edits
        .keys()
        .filter(|&&(si, _, _)| si == selected)
        .map(|&(_, _, c)| c + 1)
        .max()
        .unwrap_or(0);
    let base_cols = sheet.rows.first().map_or(0, |r| r.len());
    let row_count = (sheet.rows.len() + extra_rows).max(max_edit_r);
    let col_count = (base_cols + extra_cols).max(max_edit_c);

    let widths: Vec<f32> = (0..col_count)
        .map(|c| width(sheet, c, edits, selected) * zoom)
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
            .h(px(28. * zoom))
            .px(px(8. * zoom))
            .flex()
            .items_center()
            .border_r_1()
            .border_b_1()
            .border_color(line)
            .overflow_hidden()
            .whitespace_nowrap()
    };
    let mut grid = div().flex().flex_col().text_size(sized(t.text.body_sm));
    // Column letters.
    let mut letters = div()
        .flex()
        .bg(t.extra.inset)
        .text_size(sized(t.text.caption))
        .text_color(t.colors.text_muted)
        .child(cell(gutter));
    for (c, w) in widths.iter().enumerate() {
        letters = letters.child(cell(*w).justify_center().child(column_letters(c)));
    }
    grid = grid.child(letters);
    for r in 0..row_count {
        let row = sheet.rows.get(r);
        let mut line_el = div().flex().child(
            cell(gutter)
                .justify_end()
                .bg(t.extra.inset)
                .text_size(sized(t.text.caption))
                .text_color(t.colors.text_muted)
                .child((r + 1).to_string()),
        );
        let frozen_edge = sheet.frozen_rows > 0 && r + 1 == sheet.frozen_rows;
        for (c, w) in widths.iter().enumerate() {
            let mut value = row.and_then(|row| row.get(c)).cloned().unwrap_or_default();
            let locally_edited = edits.get(&(selected, r, c)).cloned();
            if let Some(ref raw) = locally_edited {
                value.text = raw.clone();
                value.numeric = raw.trim().parse::<f64>().is_ok_and(f64::is_finite);
            }
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
                    .when(locally_edited.is_some(), |d| d.bg(t.extra.warning_tint))
                    .when(on, |d| {
                        d.bg(t.colors.primary_tint).border_color(t.colors.primary)
                    })
                    .child(value.text)
                    .on_click(move |e, _, cx| {
                        let extend = e.modifiers().shift;
                        let dbl = e.click_count() >= 2;
                        st.update(cx, |s, cx| {
                            if dbl {
                                s.edit_sheet_cell((r, c), cx);
                            } else {
                                s.pick_cell((r, c), extend, cx);
                            }
                        })
                    }),
            );
        }
        grid = grid.child(line_el);
    }

    // The sheets, as tabs in the panel's own tokens: the open one on the
    // neutral wash, the others quiet until hovered.
    let tabs = (sheets.len() > 1).then(|| {
        let mut row = div().flex().gap(px(2.)).pb(t.space.xs);
        for (i, s) in sheets.iter().enumerate() {
            let on = i == selected;
            let st = state.clone();
            let tab = div()
                .id(SharedString::from(format!("sheet-{i}")))
                .px(t.space.md)
                .h(t.size.tool_btn)
                .flex()
                .items_center()
                .rounded(t.radius.card)
                .cursor_pointer()
                .text_size(t.text.body_sm)
                .map(|d| {
                    if on {
                        theme::selected(t, d.text_color(t.colors.text_strong))
                    } else {
                        d.text_color(t.colors.text_muted)
                            .hover(move |st| st.bg(hover))
                    }
                })
                .child(s.name.clone())
                .on_click(move |_, _, cx| st.update(cx, |s, cx| s.select_sheet(i, cx)));
            row = row.child(tab);
        }
        row
    });

    let shown = row_count;
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
