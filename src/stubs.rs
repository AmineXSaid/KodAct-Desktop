//! Every control KodAct shows that has nothing behind it yet, in one table:
//! where it is, what it will do, what it says when pressed, and the step
//! that wires it. Views never write their own "isn't wired yet" text: they
//! look the stub up here (`AppState::stub`). A control whose feature never
//! works isn't drawn at all: what is left here works in part, or elsewhere.
//!
//! When a stub is wired, delete its row; the tests keep the table tidy.
//!
//! After the three-step upgrade (P1 to P3), what is left here is blocked:
//! on something KodAct CLI doesn't do yet, or on a hosted service KodAct
//! doesn't have (each row's `waits_on` says which), or it is missing only
//! in `KODACT_DEMO`. P3 wired the Settings sections, Diagnostics and Profiles.

/// Why a stub is still a stub.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Waits on KodAct CLI or a service (`Stub::waits_on`).
    Blocked,
    /// Only `KODACT_DEMO` lacks it: it has no KodAct CLI behind it.
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub does: &'static str,
    /// What pressing it says now.
    pub toast: &'static str,
    /// What it waits on, in a few words: KodAct CLI support, or a service.
    #[cfg_attr(not(test), allow(dead_code))]
    pub waits_on: &'static str,
    /// For whoever reads the table: nothing draws it.
    #[allow(dead_code)]
    pub step: Step,
}

pub const FEEDBACK_UPLOAD: &str = "feedback-upload";
pub const CONTINUE_NO_SESSION: &str = "continue-no-session";
pub const REVERT_DEMO: &str = "revert-demo";
pub const REVERT_NO_CHECKPOINT: &str = "revert-no-checkpoint";
pub const COMPACT_DEMO: &str = "compact-demo";

pub const STUBS: &[Stub] = &[
    Stub {
        id: FEEDBACK_UPLOAD,
        place: "src/ui/message.rs, #good-* / #bad-*",
        does: "Send a reply's rating, with an optional note, to the team.",
        toast: "Thanks. Your rating is kept on this PC; sending it to the team is coming soon.",
        waits_on: "a feedback service for KodAct",
        step: Step::Blocked,
    },
    Stub {
        id: CONTINUE_NO_SESSION,
        place: "src/ui/message.rs, #continue-*",
        does: "Resume a stopped reply whose conversation KodAct CLI never wrote.",
        toast: "KodAct CLI has no record of this conversation, so it can't pick up where it stopped. Send a message to start again.",
        waits_on: "KodAct CLI keeping a conversation it never wrote",
        step: Step::Blocked,
    },
    Stub {
        id: REVERT_DEMO,
        place: "src/ui/message.rs, #revert-*",
        does: "Put files back as they were before a message, from KodAct CLI's checkpoints.",
        toast: "Revert uses KodAct CLI's checkpoints, and the demo has no KodAct CLI behind it.",
        waits_on: "KodAct CLI (the demo has none)",
        step: Step::Demo,
    },
    Stub {
        id: REVERT_NO_CHECKPOINT,
        place: "src/ui/message.rs, #revert-*",
        does: "Revert changes from a message sent before KodAct learnt KodAct CLI's turn ids.",
        toast: "This message has no KodAct CLI checkpoint (it was sent by an older KodAct), so its changes can't be reverted from here.",
        waits_on: "a checkpoint this old message never had",
        step: Step::Blocked,
    },
    Stub {
        id: COMPACT_DEMO,
        place: "src/ui/context_menu.rs, #compact (src/state.rs, compact)",
        does: "Summarize the conversation to free context (KodAct CLI's /compact).",
        toast: "Compacting runs in KodAct CLI, and the demo has no KodAct CLI behind it.",
        waits_on: "KodAct CLI (the demo has none)",
        step: Step::Demo,
    },
];

/// Icons bundled now for features a later step wires. Kept so they aren't
/// mistaken for dead assets and dropped (the tests check they are bundled).
#[cfg_attr(not(test), allow(dead_code))]
pub const RESERVED_ICONS: &[(&str, &str, Step)] = &[];

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
            assert!(
                !s.waits_on.is_empty(),
                "{} doesn't say what it waits on",
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
