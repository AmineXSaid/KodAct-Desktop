//! Every control Forge shows that has nothing behind it yet, in one table:
//! where it is, what it will do, what it says when pressed, and the step
//! that wires it. Views never write their own "isn't wired yet" text: they
//! look the stub up here (`AppState::stub`), and draw the control with the
//! one honest treatment, `ui::soon` (muted, a "Soon" tooltip).
//!
//! When a stub is wired, delete its row; the tests keep the table tidy.
//!
//! P2 wired Attach and Continue (resumed conversations); what is left here
//! is either for P3 or only missing in `FORGE_DEMO`.
//!
//! History: the Settings stubs the P1 brief lists (Diagnostics, Capability
//! probe, Profiles, and the twelve placeholder tabs) left the app when
//! Settings was rebuilt in a1a9a82; Settings now shows only sections that
//! work. Their specs return with P3.

/// Which upgrade step wires a stub, or `Demo` for what only `FORGE_DEMO`
/// lacks (it has no ForgeCLI behind it).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    P3,
    Demo,
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

pub const FEEDBACK_UPLOAD: &str = "feedback-upload";
pub const CONTINUE_NO_SESSION: &str = "continue-no-session";
pub const REVERT_DEMO: &str = "revert-demo";
pub const REVERT_NO_CHECKPOINT: &str = "revert-no-checkpoint";
pub const REAPPLY: &str = "reapply";
pub const SHARE_LINK: &str = "share-link";
pub const COMPACT_DEMO: &str = "compact-demo";
pub const CAPABILITY_PROBE: &str = "capability-probe";

pub const STUBS: &[Stub] = &[
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
        does: "Resume a stopped reply whose conversation ForgeCLI never wrote.",
        toast: "ForgeCLI has no record of this conversation, so it can't pick up where it stopped. Send a message to start again.",
        engine: true,
        step: Step::P3,
    },
    Stub {
        id: REVERT_DEMO,
        place: "src/ui/message.rs, #revert-*",
        does: "Put files back as they were before a message, from ForgeCLI's checkpoints.",
        toast: "Revert uses ForgeCLI's checkpoints, and the demo has no ForgeCLI behind it.",
        engine: true,
        step: Step::Demo,
    },
    Stub {
        id: REVERT_NO_CHECKPOINT,
        place: "src/ui/message.rs, #revert-*",
        does: "Revert changes from a message sent before Forge learnt ForgeCLI's turn ids.",
        toast: "This message has no ForgeCLI checkpoint (it was sent by an older Forge), so its changes can't be reverted from here.",
        engine: true,
        step: Step::P3,
    },
    Stub {
        id: REAPPLY,
        place: "src/ui/message.rs, #reapply-*",
        does: "Apply a reverted change again.",
        toast: "Applying a reverted change again needs ForgeCLI to roll checkpoints forward, which it can't yet. Ask Forge to make the change again.",
        engine: true,
        step: Step::P3,
    },
    Stub {
        id: SHARE_LINK,
        place: "src/ui/chat_menu.rs, #chat-menu-share",
        does: "Upload a conversation and copy a link to it.",
        toast: "Copied the conversation as JSON. Share links are coming soon.",
        engine: true,
        step: Step::P3,
    },
    Stub {
        id: COMPACT_DEMO,
        place: "src/ui/context_menu.rs, #compact (src/state.rs, compact)",
        does: "Summarize the conversation to free context (ForgeCLI's /compact).",
        toast: "Compacting runs in ForgeCLI, and the demo has no ForgeCLI behind it.",
        engine: true,
        step: Step::Demo,
    },
    Stub {
        id: CAPABILITY_PROBE,
        place: "src/ui/diagnostics.rs, #capability-probe",
        does: "Check what a model can do: stream, call tools, read images, think.",
        toast: "Model checks are coming. They need ForgeCLI to report what a model supports. The endpoint's check already sends one real message.",
        engine: true,
        step: Step::P3,
    },
];

/// Icons bundled now for features a later step wires. Kept so they aren't
/// mistaken for dead assets and dropped (the tests check they are bundled).
#[cfg_attr(not(test), allow(dead_code))]
pub const RESERVED_ICONS: &[(&str, &str, Step)] = &[
    ("map", "Project and file map", Step::P3),
    ("split", "Split (multi-pane) view", Step::P3),
];

impl Stub {
    /// The tooltip line: what it will do, and when.
    pub fn soon_line(&self) -> String {
        let when = match self.step {
            Step::P3 => "a later update",
            Step::Demo => "not in the demo",
        };
        let needs = if self.engine && self.step != Step::Demo {
            " · needs ForgeCLI support"
        } else {
            ""
        };
        format!("Soon ({when}{needs}) · {}", self.does)
    }
}

pub fn get(id: &str) -> Option<&'static Stub> {
    STUBS.iter().find(|s| s.id == id)
}

/// The toast a stub shows; a missing id still says something honest.
pub fn toast(id: &str) -> &'static str {
    get(id)
        .map(|s| s.toast)
        .unwrap_or("This isn't available yet.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_rows_are_filled_in() {
        for (i, s) in STUBS.iter().enumerate() {
            assert!(
                STUBS[i + 1..].iter().all(|o| o.id != s.id),
                "duplicate stub {}",
                s.id
            );
            assert!(
                !s.place.is_empty() && !s.does.is_empty() && !s.toast.is_empty(),
                "{} is incomplete",
                s.id
            );
            // The old decoy wording is gone: a stub says what is coming.
            assert!(!s.toast.contains("isn't wired"), "{}: {}", s.id, s.toast);
        }
    }

    #[test]
    fn every_stub_is_used_by_the_code() {
        let sources = [
            include_str!("ui/composer.rs"),
            include_str!("ui/message.rs"),
            include_str!("state.rs"),
            include_str!("ui/chat_menu.rs"),
            include_str!("ui/context_menu.rs"),
            include_str!("ui/diagnostics.rs"),
        ];
        let consts = include_str!("stubs.rs");
        for s in STUBS {
            // The constant naming this id.
            let konst = consts
                .lines()
                .find_map(|l| {
                    l.strip_prefix("pub const ")
                        .filter(|l| l.ends_with(&format!("\"{}\";", s.id)))
                })
                .and_then(|l| l.split(':').next())
                .unwrap_or_else(|| panic!("no constant for {}", s.id));
            let needle = format!("stubs::{konst}");
            assert!(
                sources.iter().any(|src| src.contains(&needle)),
                "{} is registered but nothing shows it",
                s.id
            );
        }
    }

    #[test]
    fn reserved_icons_are_bundled() {
        let registry = include_str!("assets.rs");
        for (name, _, _) in RESERVED_ICONS {
            assert!(
                registry.contains(&format!("\"{name}\"")),
                "{name} is reserved but not bundled"
            );
        }
    }

    #[test]
    fn an_unknown_stub_still_says_something() {
        assert_eq!(toast("nope"), "This isn't available yet.");
        assert_eq!(toast(FEEDBACK_UPLOAD), STUBS[0].toast);
    }
}
