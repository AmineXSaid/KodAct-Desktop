//! Every control Forge shows that has nothing behind it yet, in one table:
//! where it is, what it will do, what it says when pressed, and the step
//! that wires it. Views never write their own "isn't wired yet" text: they
//! look the stub up here (`AppState::stub`), and draw the control with the
//! one honest treatment, `ui::soon` (muted, a "Soon" tooltip).
//!
//! When a stub is wired, delete its row; the tests keep the table tidy.
//!
//! History: the Settings stubs the P1 brief lists (Diagnostics, Capability
//! probe, Profiles, and the twelve placeholder tabs) left the app when
//! Settings was rebuilt in a1a9a82; Settings now shows only sections that
//! work. Their specs return with P3.

/// Which upgrade step wires a stub.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    P2,
    P3,
}

/// One control without its feature.
#[derive(Clone, Copy, Debug)]
pub struct Stub {
    pub id: &'static str,
    /// Where it is drawn (file and the control's element id): for whoever
    /// wires it, and for the registry's tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub place: &'static str,
    /// What it will do once wired.
    pub does: &'static str,
    /// What pressing it says now.
    pub toast: &'static str,
    /// It needs ForgeCLI to do something it can't yet.
    pub engine: bool,
    pub step: Step,
}

pub const ATTACH: &str = "attach";
pub const FEEDBACK_UPLOAD: &str = "feedback-upload";
pub const CONTINUE_NO_SESSION: &str = "continue-no-session";

pub const STUBS: &[Stub] = &[
    Stub {
        id: ATTACH,
        place: "src/ui/composer.rs, #attach",
        does: "Attach files and images: drag-drop, paste, chips and previews.",
        toast: "Attaching files and images is coming soon.",
        engine: true,
        step: Step::P2,
    },
    Stub {
        id: FEEDBACK_UPLOAD,
        place: "src/ui/message.rs, #good-* / #bad-*",
        does: "Send a reply's rating, with an optional note, to the team.",
        toast: "Thanks. Your rating is kept on this PC; sending it to the team is coming soon.",
        engine: true,
        step: Step::P3,
    },
    Stub {
        id: CONTINUE_NO_SESSION,
        place: "src/ui/message.rs, #continue-*",
        does: "Resume a stopped reply after its ForgeCLI process ended (session resume).",
        toast: "This conversation's ForgeCLI session has ended, so it can't pick up where it stopped. Send a message to start again.",
        engine: true,
        step: Step::P2,
    },
];

/// Icons bundled now for features a later step wires. Kept so they aren't
/// mistaken for dead assets and dropped (the tests check they are bundled).
#[cfg_attr(not(test), allow(dead_code))]
pub const RESERVED_ICONS: &[(&str, &str, Step)] = &[
    ("clock", "Conversation versions: view and restore", Step::P2),
    ("eye", "Reveal and preview an attachment", Step::P2),
    ("image", "Image attachments", Step::P2),
    ("paperclip", "File attachments", Step::P2),
    ("map", "Project and file map", Step::P3),
    ("split", "Split (multi-pane) view", Step::P3),
];

impl Stub {
    /// The tooltip line: what it will do, and when.
    pub fn soon_line(&self) -> String {
        let when = match self.step {
            Step::P2 => "next update",
            Step::P3 => "a later update",
        };
        let needs = if self.engine { " · needs ForgeCLI support" } else { "" };
        format!("Soon ({when}{needs}) · {}", self.does)
    }
}

pub fn get(id: &str) -> Option<&'static Stub> {
    STUBS.iter().find(|s| s.id == id)
}

/// The toast a stub shows; a missing id still says something honest.
pub fn toast(id: &str) -> &'static str {
    get(id).map(|s| s.toast).unwrap_or("This isn't available yet.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_rows_are_filled_in() {
        for (i, s) in STUBS.iter().enumerate() {
            assert!(STUBS[i + 1..].iter().all(|o| o.id != s.id), "duplicate stub {}", s.id);
            assert!(!s.place.is_empty() && !s.does.is_empty() && !s.toast.is_empty(), "{} is incomplete", s.id);
            // The old decoy wording is gone: a stub says what is coming.
            assert!(!s.toast.contains("isn't wired"), "{}: {}", s.id, s.toast);
        }
    }

    #[test]
    fn every_stub_is_used_by_the_view_it_names() {
        let sources = [include_str!("ui/composer.rs"), include_str!("ui/message.rs"), include_str!("state.rs")];
        for s in STUBS {
            let konst = match s.id {
                ATTACH => "ATTACH",
                FEEDBACK_UPLOAD => "FEEDBACK_UPLOAD",
                CONTINUE_NO_SESSION => "CONTINUE_NO_SESSION",
                other => panic!("add {other} to this test"),
            };
            let needle = format!("stubs::{konst}");
            assert!(sources.iter().any(|src| src.contains(&needle)), "{} is registered but nothing shows it", s.id);
        }
    }

    #[test]
    fn reserved_icons_are_bundled() {
        let registry = include_str!("assets.rs");
        for (name, _, _) in RESERVED_ICONS {
            assert!(registry.contains(&format!("\"{name}\"")), "{name} is reserved but not bundled");
        }
    }

    #[test]
    fn an_unknown_stub_still_says_something() {
        assert_eq!(toast("nope"), "This isn't available yet.");
        assert_eq!(toast(ATTACH), STUBS[0].toast);
    }
}
