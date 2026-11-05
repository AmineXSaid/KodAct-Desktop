//! What KodAct is doing right now, as the working line under a reply shows
//! it: KodAct's mark at work (`mark::working`: the cursor bar, then the run
//! triangle), then the step in words ("Reading src/main.rs", "Running cargo
//! test"), how long, and how much it has written. One mark for every kind
//! of work; the words say which.

/// The kinds of work the working line names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Waiting,
    Thinking,
    Reading,
    Editing,
    WritingCode,
    WritingDoc,
    Running,
    Searching,
    SearchingWeb,
    Fetching,
    Agent,
    Agents,
    Tool,
    Replying,
    AwaitingYou,
    Compacting,
}
