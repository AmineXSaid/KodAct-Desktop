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

/// The latest reply's usage: the conversation's size now. Unknown after a
/// compaction, until the next reply reports it.
pub fn last_usage(messages: &[Message]) -> Option<Usage> {
    for m in messages.iter().rev() {
        match m {
            Message::Notice { text, .. } if text.as_ref() == crate::state::COMPACTED => {
                return None
            }
            Message::TurnEnd { usage: Some(u), .. } => return Some(*u),
            _ => {}
        }
    }
    None
}

/// What fills the conversation, by kind, counted from its text (about four
/// characters to a token): a split of the reported total, not a measure.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Breakdown {
    pub yours: u64,
    pub replies: u64,
    pub thinking: u64,
    pub tools: u64,
    pub files: u64,
}

impl Breakdown {
    pub fn total(&self) -> u64 {
        self.yours + self.replies + self.thinking + self.tools + self.files
    }

    /// (label, tokens) for each kind, scaled so they add up to `used`.
    pub fn scaled(&self, used: u64) -> Vec<(&'static str, u64)> {
        let t = self.total().max(1) as f64;
        let k = used as f64 / t;
        [
            ("Your messages", self.yours),
            ("Replies", self.replies),
            ("Tool calls and output", self.tools),
            ("Thinking", self.thinking),
            ("Files Forge wrote", self.files),
        ]
        .into_iter()
        .map(|(l, n)| (l, (n as f64 * k).round() as u64))
        .collect()
    }
}

/// The conversation's text, by kind, since the last compaction.
pub fn breakdown(messages: &[Message]) -> Breakdown {
    let from = messages.iter().rposition(|m| matches!(m, Message::Notice { text, .. } if text.as_ref() == crate::state::COMPACTED)).map_or(0, |i| i + 1);
    let mut b = Breakdown::default();
    for m in &messages[from..] {
        match m {
            Message::User {
                text, attachments, ..
            } => {
                b.yours += tokens(text);
                // An image costs the model about a thousand tokens; text files go in whole.
                b.files += attachments
                    .iter()
                    .map(|a| match a.kind {
                        crate::attach::Kind::Image(_) => 1_000,
                        crate::attach::Kind::Text | crate::attach::Kind::Pasted => {
                            a.size.div_ceil(4)
                        }
                        // A document's text goes in; a PDF as a document costs about as much.
                        crate::attach::Kind::Pdf | crate::attach::Kind::Document(_) => a
                            .text
                            .as_ref()
                            .map_or(a.size.div_ceil(8), |t| (t.len() as u64).div_ceil(4)),
                        crate::attach::Kind::Archive => {
                            20 + a
                                .extracted
                                .as_ref()
                                .map_or(0, |x| x.files.len() as u64 * 12)
                        }
                        crate::attach::Kind::File => 20,
                    })
                    .sum::<u64>();
            }
            Message::Assistant { blocks } => {
                for blk in blocks {
                    match blk {
                        Block::Text(t) => b.replies += tokens(t),
                        Block::Thinking { text, .. } => b.thinking += tokens(text),
                        Block::Tool(_) | Block::Permission { .. } => b.tools += block_text(blk),
                        Block::File(_) => b.files += block_text(blk),
                    }
                }
            }
            Message::TurnEnd { .. } | Message::Notice { .. } => {}
        }
    }
    b
}

/// Each finished turn's context size, as reported: (the first words of
/// your message, its usage), oldest first.
pub fn turns(messages: &[Message]) -> Vec<(String, Usage)> {
    let mut asked = String::new();
    let mut out = vec![];
    for m in messages {
        match m {
            Message::User { text, .. } => {
                asked = text
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(60)
                    .collect()
            }
            Message::TurnEnd { usage: Some(u), .. } => out.push((asked.clone(), *u)),
            _ => {}
        }
    }
    out
}

/// About four characters to a token, rounded up.
fn tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

fn block_text(b: &Block) -> u64 {
    match b {
        Block::Text(t) => tokens(t),
        Block::Thinking { text, .. } => tokens(text),
        Block::Tool(t) => {
            tokens(&t.target) + t.note.as_ref().map_or(0, |n| tokens(n)) + tokens(&t.output)
        }
        Block::Permission { title, detail, .. } => tokens(title) + tokens(detail),
        // The model wrote the file's text, so it counts as what it said.
        Block::File(f) => tokens(&f.content),
    }
}

/// What a reply's usage would be, counted from the conversation's text:
/// everything before the latest reply is input, the latest reply is output.
/// Only for replies no model reported on (the mock ones).
pub fn estimate(messages: &[Message]) -> Usage {
    let last_user = messages
        .iter()
        .rposition(|m| matches!(m, Message::User { .. }));
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
    Usage {
        input,
        output,
        estimated: true,
        ..Default::default()
    }
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
    Some(Meter {
        used: usage.total(),
        window,
        estimated: usage.estimated,
    })
}

/// What fills the context, as ForgeCLI counts it (`get_context_usage`, the
/// numbers its `/context` shows): each part in tokens, the window, and where
/// automatic compaction runs. For the Session column's Context card.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextUsage {
    pub window: u64,
    /// The parts added up (estimates, at four characters a token).
    pub total: u64,
    /// What the endpoint measured for the last request; 0 when it hasn't said.
    pub measured: u64,
    pub autocompact_at: Option<u64>,
    /// (name, tokens): system prompt, built-in tools, MCP tools, skills,
    /// memory files (until they are sent), messages.
    pub categories: Vec<(String, u64)>,
    pub tools: Vec<(String, u64)>,
    pub memory_files: Vec<(String, u64)>,
    pub skills: Vec<(String, u64)>,
}

impl ContextUsage {
    /// ForgeCLI's answer, or None when it isn't one.
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let rows = |key: &str, name: &str| -> Vec<(String, u64)> {
            v[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| Some((r[name].as_str()?.to_string(), r["tokens"].as_u64()?)))
                .collect()
        };
        Some(Self {
            window: v["window"].as_u64()?,
            total: v["total"].as_u64()?,
            measured: v["measured"].as_u64().unwrap_or(0),
            autocompact_at: v["autocompactAt"].as_u64(),
            categories: rows("categories", "name"),
            tools: rows("tools", "name"),
            memory_files: rows("memoryFiles", "path"),
            skills: rows("skills", "name"),
        })
    }

    /// What is used: the endpoint's own measure when it gave one, else the
    /// parts added up.
    pub fn used(&self) -> u64 {
        if self.measured > 0 {
            self.measured
        } else {
            self.total
        }
    }

    /// What automatic compaction keeps free: the window past where it runs.
    pub fn buffer(&self) -> u64 {
        self.autocompact_at
            .map(|at| self.window.saturating_sub(at))
            .unwrap_or(0)
    }

    /// What is left, after the parts and the compaction buffer.
    pub fn free(&self) -> u64 {
        self.window
            .saturating_sub(self.total)
            .saturating_sub(self.buffer())
    }

    /// The demo's numbers for a conversation of `messages` tokens: what a
    /// coding chat's context looks like, so the card can be seen without
    /// an engine.
    pub fn demo(messages: u64) -> Self {
        let tools: Vec<(String, u64)> = [
            ("Bash", 2_900),
            ("Edit", 1_400),
            ("Read", 1_200),
            ("Write", 900),
            ("Grep", 1_100),
            ("Glob", 600),
            ("WebFetch", 800),
            ("WebSearch", 700),
            ("Task", 2_600),
            ("TodoWrite", 2_100),
            ("mcp__github__create_issue", 1_900),
            ("mcp__github__list_prs", 1_600),
        ]
        .into_iter()
        .map(|(n, t)| (n.to_string(), t))
        .collect();
        let (builtin, mcp) = tools.iter().fold((0, 0), |(b, m), (n, t)| {
            if n.starts_with("mcp__") {
                (b, m + t)
            } else {
                (b + t, m)
            }
        });
        let categories = vec![
            ("System prompt".to_string(), 3_400),
            ("Built-in tools".to_string(), builtin),
            ("MCP tools".to_string(), mcp),
            ("Skills".to_string(), 900),
            ("Messages".to_string(), messages),
        ];
        let total = categories.iter().map(|(_, t)| t).sum();
        Self {
            window: 200_000,
            total,
            measured: 0,
            autocompact_at: Some(155_000),
            categories,
            tools,
            memory_files: vec![
                ("FORGE.md".into(), 1_300),
                ("~/.forge/FORGE.md".into(), 400),
            ],
            skills: vec![
                ("frontend-design".into(), 300),
                ("pdf".into(), 250),
                ("xlsx".into(), 350),
            ],
        }
    }
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
            Message::user("a".repeat(400).into(), SystemTime::UNIX_EPOCH),
            Message::Assistant {
                blocks: vec![Block::Text("b".repeat(200).into())],
            },
            Message::TurnEnd {
                seconds: 1.,
                usage: Some(Usage {
                    input: 9_000,
                    cache_read: 1_000,
                    output: 500,
                    ..Default::default()
                }),
            },
            Message::user("c".repeat(40).into(), SystemTime::UNIX_EPOCH),
            Message::Assistant {
                blocks: vec![Block::Text("d".repeat(80).into())],
            },
        ]
    }

    #[test]
    fn the_latest_reply_sets_the_size() {
        let mut m = chat();
        assert_eq!(last_usage(&m).map(|u| u.total()), Some(10_500));
        m.push(Message::TurnEnd {
            seconds: 1.,
            usage: Some(Usage {
                input: 11_000,
                output: 300,
                ..Default::default()
            }),
        });
        assert_eq!(last_usage(&m).map(|u| u.total()), Some(11_300));
    }

    #[test]
    fn no_meter_without_both_numbers() {
        let u = Some(Usage {
            input: 50_000,
            ..Default::default()
        });
        assert_eq!(meter(u, None), None);
        assert_eq!(meter(u, Some(0)), None);
        assert_eq!(meter(None, Some(128_000)), None);
        assert_eq!(last_usage(&[]), None);
    }

    #[test]
    fn switching_models_rereads_the_same_conversation() {
        let u = Some(Usage {
            input: 50_000,
            ..Default::default()
        });
        let big = meter(u, Some(200_000)).unwrap();
        let small = meter(u, Some(32_000)).unwrap();
        assert_eq!((big.used, small.used), (50_000, 50_000));
        assert!((big.fraction() - 0.25).abs() < 1e-6);
        assert!(small.fraction() > 1.);
    }

    #[test]
    fn forgecli_s_usage_reads_and_adds_up() {
        let v = serde_json::json!({
            "model": "m", "window": 200_000, "total": 20_000, "free": 180_000, "measured": 0,
            "autocompactAt": 155_000,
            "categories": [{"name": "System prompt", "tokens": 3_000}, {"name": "Messages", "tokens": 17_000}],
            "tools": [{"name": "Bash", "tokens": 900}],
            "memoryFiles": [{"path": "FORGE.md", "tokens": 120}],
            "skills": [],
        });
        let u = ContextUsage::from_json(&v).unwrap();
        assert_eq!(u.categories.len(), 2);
        assert_eq!((u.used(), u.buffer(), u.free()), (20_000, 45_000, 135_000));
        assert_eq!(u.memory_files, vec![("FORGE.md".to_string(), 120)]);
        // The endpoint's own measure wins when it gave one.
        let measured = ContextUsage {
            measured: 21_500,
            ..u
        };
        assert_eq!(measured.used(), 21_500);
        assert_eq!(
            ContextUsage::from_json(&serde_json::json!({"oops": 1})),
            None
        );
        let d = ContextUsage::demo(5_000);
        assert_eq!(d.total, d.categories.iter().map(|(_, t)| t).sum::<u64>());
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

#[cfg(test)]
mod breakdown_tests {
    use super::*;
    use crate::state::Message;

    #[test]
    fn the_split_adds_up_to_what_was_reported() {
        let m = vec![
            Message::user("a".repeat(400).into(), std::time::SystemTime::UNIX_EPOCH),
            Message::Assistant {
                blocks: vec![
                    Block::Text("b".repeat(200).into()),
                    Block::Thinking {
                        text: "c".repeat(40).into(),
                        open: false,
                    },
                ],
            },
            Message::TurnEnd {
                seconds: 1.,
                usage: Some(Usage {
                    input: 900,
                    output: 100,
                    ..Default::default()
                }),
            },
        ];
        let b = breakdown(&m);
        assert_eq!((b.yours, b.replies, b.thinking), (100, 50, 10));
        let scaled = b.scaled(1600);
        assert_eq!(scaled.iter().map(|(_, n)| n).sum::<u64>(), 1600);
        assert_eq!(
            turns(&m),
            vec![(
                "a".repeat(60),
                Usage {
                    input: 900,
                    output: 100,
                    ..Default::default()
                }
            )]
        );
    }

    #[test]
    fn a_compaction_resets_the_reading() {
        let mut m = vec![Message::TurnEnd {
            seconds: 1.,
            usage: Some(Usage {
                input: 5,
                ..Default::default()
            }),
        }];
        assert!(last_usage(&m).is_some());
        m.push(Message::Notice {
            text: crate::state::COMPACTED.into(),
            error: false,
        });
        assert!(last_usage(&m).is_none());
        assert_eq!(breakdown(&m).total(), 0);
    }
}
