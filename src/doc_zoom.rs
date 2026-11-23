//! How a page, slide or picture in the side panel is scaled: a percentage
//! (stepped 50 to 200), or fitted to the panel's width or to a whole page.

/// The steps the toolbar's minus and plus walk through, and the menu lists.
pub const STEPS: [u16; 7] = [50, 75, 100, 125, 150, 175, 200];
pub const MIN: u16 = 25;
pub const MAX: u16 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Zoom {
    /// As wide as the panel allows (the default).
    #[default]
    FitWidth,
    /// A whole page in view.
    FitPage,
    /// A percentage of the page's own size.
    Percent(u16),
}

impl Zoom {
    /// What the toolbar and the footer say: the step, or the fit.
    pub fn label(self) -> String {
        match self {
            Zoom::FitWidth => "Fit width".into(),
            Zoom::FitPage => "Fit page".into(),
            Zoom::Percent(p) => format!("{p}%"),
        }
    }

    /// The next step in (`up`) or out from `now` percent: the nearest step
    /// above or below, clamped to the range. Fitted views step from the
    /// percentage they currently show.
    pub fn step(now: u16, up: bool) -> Zoom {
        let to = if up {
            STEPS.iter().copied().find(|s| *s > now).unwrap_or(MAX)
        } else {
            STEPS
                .iter()
                .rev()
                .copied()
                .find(|s| *s < now)
                .unwrap_or(MIN)
        };
        Zoom::Percent(to.clamp(MIN, MAX))
    }

    /// The width to draw a page at, from its `natural` width (100%), its
    /// height over its width, the room the panel gives it and the room for a
    /// whole page. A written page (`cap`) is not stretched past 120% to fill
    /// a wide panel; a slide is.
    pub fn width(self, natural: f32, aspect: f32, room: f32, room_h: f32, cap: bool) -> f32 {
        let natural = natural.max(1.);
        let fit_w = if cap { room.min(natural * 1.2) } else { room };
        match self {
            Zoom::Percent(p) => natural * f32::from(p) / 100.,
            Zoom::FitWidth => fit_w,
            Zoom::FitPage => fit_w.min(room_h / aspect.max(0.01)),
        }
    }
}

/// The percentage a page `width` wide is, of its `natural` width.
pub fn percent_of(width: f32, natural: f32) -> u16 {
    ((width / natural.max(1.) * 100.).round() as u16).clamp(MIN, MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_names_a_fit_and_a_step() {
        assert_eq!(Zoom::FitWidth.label(), "Fit width");
        assert_eq!(Zoom::FitPage.label(), "Fit page");
        assert_eq!(Zoom::Percent(125).label(), "125%");
    }

    #[test]
    fn stepping_walks_the_steps_and_clamps() {
        assert_eq!(Zoom::step(100, true), Zoom::Percent(125));
        assert_eq!(Zoom::step(100, false), Zoom::Percent(75));
        assert_eq!(Zoom::step(110, true), Zoom::Percent(125));
        assert_eq!(Zoom::step(110, false), Zoom::Percent(100));
        assert_eq!(Zoom::step(200, true), Zoom::Percent(MAX));
        assert_eq!(Zoom::step(MAX, true), Zoom::Percent(MAX));
        assert_eq!(Zoom::step(50, false), Zoom::Percent(MIN));
        assert_eq!(Zoom::step(MIN, false), Zoom::Percent(MIN));
    }

    #[test]
    fn fitted_views_report_the_percentage_they_draw_at() {
        // A 600px page in 450px: 75%.
        let w = Zoom::FitWidth.width(600., 1.4, 450., 2000., true);
        assert_eq!(percent_of(w, 600.), 75);
        // A written page is not stretched past 120% in a wide panel.
        assert_eq!(Zoom::FitWidth.width(600., 1.4, 2000., 2000., true), 720.);
        assert_eq!(Zoom::FitWidth.width(600., 1.4, 2000., 2000., false), 2000.);
        // A tall page that must also fit 500px of height: height rules.
        let w = Zoom::FitPage.width(600., 1.4, 900., 500., true);
        assert!((w - 500. / 1.4).abs() < 0.01);
        assert_eq!(Zoom::Percent(150).width(600., 1.4, 450., 500., true), 900.);
    }
}
