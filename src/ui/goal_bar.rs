//! The chat's `/goal`, above the box while it lasts: what Forge works
//! toward, where it stands (working, paused, achieved, can't be met), and
//! what the last check said. ForgeCLI checks the goal after each turn and
//! starts another until it holds; this strip is how you follow that loop.

use gpui::{div, prelude::*, AnyElement, Entity};

use super::button::Button;
use super::icon;
use crate::state::{AppState, Goal, GoalStatus};
use crate::theme::{self, Theme};

pub fn render(t: &Theme, state: &Entity<AppState>, goal: &Goal, busy: bool) -> AnyElement {
    let (label, bg, fg) = match goal.status {
        GoalStatus::Working => ("Working on it", t.colors.primary_tint, t.colors.primary),
        GoalStatus::Paused => ("Paused", t.extra.warning_tint, t.extra.warning),
        GoalStatus::Achieved => ("Achieved", t.extra.success_tint, t.extra.success),
        GoalStatus::Failed => ("Can't be met", t.extra.danger_tint, t.extra.danger),
        GoalStatus::Cleared => return div().into_any_element(),
    };
    let live = matches!(goal.status, GoalStatus::Working | GoalStatus::Paused);
    // Stop pauses a goal mid-reply; Clear ends a goal at rest; a goal that
    // is over only leaves the strip.
    let action = {
        let state = state.clone();
        let button = match (live, busy) {
            (true, true) => Button::ghost("goal-clear", "Stop"),
            (true, false) => Button::ghost("goal-clear", "Clear"),
            (false, _) => Button::icon("goal-clear", "icons/x.svg", "Dismiss"),
        };
        button
            .compact()
            .on_click(move |_, _, app| state.update(app, |s, cx| s.clear_goal(cx)))
    };
    let check = goal.reason.as_ref().map(|r| {
        let lead = match goal.status {
            GoalStatus::Paused => "",
            _ => "Last check: ",
        };
        div()
            .pl(t.size.icon_sm + t.space.sm)
            .text_size(t.text.caption)
            .text_color(t.colors.text_muted)
            .line_clamp(2)
            .child(format!("{lead}{r}"))
    });
    div()
        .flex()
        .flex_col()
        .gap(t.space.xs)
        .mb(t.space.sm)
        .px(t.space.md)
        .py(t.space.sm)
        .bg(t.extra.inset)
        .border_1()
        .border_color(t.colors.border)
        .rounded(t.radius.card)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.space.sm)
                .child(icon("icons/target.svg", t.size.icon_sm, fg))
                .child(
                    div()
                        .flex_none()
                        .text_size(t.text.caption)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(t.colors.text_muted)
                        .child("Goal"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(t.text.body_sm)
                        .text_color(t.colors.text_strong)
                        .child(goal.condition.clone()),
                )
                .child(theme::pill(t, bg, fg, label).flex_none())
                .child(action),
        )
        .children(check)
        .into_any_element()
}
