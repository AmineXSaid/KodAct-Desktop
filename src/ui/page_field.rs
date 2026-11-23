//! The file view's page field: the page or slide at the top ("3") with the
//! total beside it ("/ 12"). Type a number and press Enter to go there;
//! Ctrl+G focuses it. While it is not being typed in, it follows the scroll.

use gpui::component::input::{Input, InputEvent, InputState};
use gpui::{div, prelude::*, px, Context, Entity, IntoElement, Render, Subscription, Window};

use crate::state::AppState;
use crate::theme::{ActiveTheme, Theme};

pub struct PageField {
    state: Entity<AppState>,
    input: Entity<InputState>,
    shown: usize,
    _subs: Vec<Subscription>,
}

impl PageField {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Page"));
        let subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &input,
                window,
                |this, input, ev: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = ev {
                        let n: usize = input.read(cx).value().trim().parse().unwrap_or(0);
                        if n > 0 {
                            this.state.update(cx, |s, cx| s.show_doc_page(n - 1, cx));
                        }
                        // Back to following the scroll.
                        this.shown = 0;
                        cx.notify();
                        let _ = window;
                    }
                },
            ),
        ];
        Self {
            state,
            input,
            shown: 0,
            _subs: subs,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| i.focus(window, cx));
    }
}

/// The page number the field shows for the page at the top (from 0).
pub fn label(top: usize, total: usize) -> usize {
    top.min(total.saturating_sub(1)) + 1
}

impl Render for PageField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t: Theme = cx.theme().clone();
        let (top, total) = {
            let s = self.state.read(cx);
            (s.doc_scroll.top_item(), s.doc_page_count())
        };
        let page = label(top, total);
        let focused = gpui::Focusable::focus_handle(self.input.read(cx), cx).is_focused(window);
        if !focused && self.shown != page {
            self.shown = page;
            self.input
                .update(cx, |i, cx| i.set_value(page.to_string(), window, cx));
        }
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(t.space.xs)
            .text_size(t.text.caption)
            .text_color(t.colors.text_muted)
            .child(
                div()
                    .w(px(34.))
                    .h(t.size.tool_btn)
                    .child(Input::new(&self.input).appearance(false)),
            )
            .child(format!("/ {total}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_counts_from_one_and_stays_in_range() {
        assert_eq!(label(0, 12), 1);
        assert_eq!(label(2, 12), 3);
        assert_eq!(label(40, 12), 12);
        assert_eq!(label(0, 0), 1);
    }
}
