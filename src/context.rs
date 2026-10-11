//! How full the model's context window is, for the composer's meter.
//!
//! Two numbers, both real or the meter isn't shown:
//! - **used**: the size of the conversation as the model last saw it, from
//!   the usage the model reported for the latest reply (input, cache writes,
//!   cache reads and output, as Claude Code and Forge for VS Code add them up);
//! - **window**: the context window the endpoint states for the model in
//!   use, its `contextWindow` in endpoints.json or the size the endpoint
//!   itself lists. Never a guess.
//!
//! A chat can move between models: the meter always measures the same
//! conversation against the model picked now, so switching to a model with a
//! smaller window raises it, and a model whose window isn't known hides it.

use crate::state::{Block, Message};

/// Token usage the model reported for one reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Usage {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
    /// Counted from the text, not reported by a model (this UI shell's
    /// mock replies have no model to report it).
    pub estimated: bool,
}

impl Usage {
    /// The conversation's size after this reply.
    pub fn total(&self) -> u64 {
        self.input + self.cache_creation + self.cache_read + self.output
    }
}

/// The latest reply's usage: the conversation's size now.
pub fn last_usage(messages: &[Message]) -> Option<Usage> {
    messages.iter().rev().find_map(|m| match m {
        Message::TurnEnd { usage, .. } => *usage,
        _ => None,
    })
}

/// About four characters to a token, rounded up.
fn tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

fn block_text(b: &Block) -> u64 {
    match b {
        Block::Text(t) => tokens(t),
        Block::Thinking { text, .. } => tokens(text),
        Block::Tool(t) => tokens(&t.target) + t.note.as_ref().map_or(0, |n| tokens(n)) + tokens(&t.output),
        Block::Permission { title, detail, .. } => tokens(title) + tokens(detail),
    }
}

/// What a reply's usage would be, counted from the conversation's text:
/// everything before the latest reply is input, the latest reply is output.
/// Only for replies no model reported on (the mock ones).
pub fn estimate(messages: &[Message]) -> Usage {
    let last_user = messages.iter().rposition(|m| matches!(m, Message::User { .. }));
    let (mut input, mut output) = (0, 0);
    for (i, m) in messages.iter().enumerate() {
        let n = match m {
            Message::User { text, .. } => tokens(text),
            Message::Assistant { blocks } => blocks.iter().map(block_text).sum(),
            Message::TurnEnd { .. } | Message::Notice { .. } => 0,
        };
        if last_user.is_some_and(|u| i > u) {
            output += n;
        } else {
            input += n;
        }
    }
    Usage { input, output, estimated: true, ..Default::default() }
}

/// The meter's reading: tokens used against the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meter {
    pub used: u64,
    pub window: u64,
    pub estimated: bool,
}

impl Meter {
    /// 0..1, and past 1 when the conversation is bigger than the window.
    pub fn fraction(&self) -> f32 {
        self.used as f32 / self.window as f32
    }
}

/// The meter, or None when either number isn't known.
pub fn meter(usage: Option<Usage>, window: Option<u64>) -> Option<Meter> {
    let (usage, window) = (usage?, window.filter(|w| *w > 0)?);
    Some(Meter { used: usage.total(), window, estimated: usage.estimated })
}

/// 950, 12.3K, 1.2M.
pub fn short(n: u64) -> String {
    match n {
        1_000_000.. => format!("{:.1}M", n as f64 / 1e6),
        1_000.. => format!("{:.1}K", n as f64 / 1e3),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn chat() -> Vec<Message> {
        vec![
            Message::User { text: "a".repeat(400).into(), sent: SystemTime::UNIX_EPOCH },
            Message::Assistant { blocks: vec![Block::Text("b".repeat(200).into())] },
            Message::TurnEnd { seconds: 1., usage: Some(Usage { input: 9_000, cache_read: 1_000, output: 500, ..Default::default() }) },
            Message::User { text: "c".repeat(40).into(), sent: SystemTime::UNIX_EPOCH },
            Message::Assistant { blocks: vec![Block::Text("d".repeat(80).into())] },
        ]
    }

    #[test]
    fn the_latest_reply_sets_the_size() {
        let mut m = chat();
        assert_eq!(last_usage(&m).map(|u| u.total()), Some(10_500));
        m.push(Message::TurnEnd { seconds: 1., usage: Some(Usage { input: 11_000, output: 300, ..Default::default() }) });
        assert_eq!(last_usage(&m).map(|u| u.total()), Some(11_300));
    }

    #[test]
    fn no_meter_without_both_numbers() {
        let u = Some(Usage { input: 50_000, ..Default::default() });
        assert_eq!(meter(u, None), None);
        assert_eq!(meter(u, Some(0)), None);
        assert_eq!(meter(None, Some(128_000)), None);
        assert_eq!(last_usage(&[]), None);
    }

    #[test]
    fn switching_models_rereads_the_same_conversation() {
        let u = Some(Usage { input: 50_000, ..Default::default() });
        let big = meter(u, Some(200_000)).unwrap();
        let small = meter(u, Some(32_000)).unwrap();
        assert_eq!((big.used, small.used), (50_000, 50_000));
        assert!((big.fraction() - 0.25).abs() < 1e-6);
        assert!(small.fraction() > 1.);
    }

    #[test]
    fn estimates_split_input_and_output_at_the_latest_message() {
        let e = estimate(&chat());
        assert_eq!((e.input, e.output, e.estimated), (100 + 50 + 10, 20, true));
        assert_eq!(short(950), "950");
        assert_eq!(short(12_340), "12.3K");
        assert_eq!(short(1_200_000), "1.2M");
    }
}
