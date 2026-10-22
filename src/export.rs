//! A chat as a Markdown file: your messages, Forge's replies, its tool calls
//! as short lines (a command with its output in a fenced block), and the
//! notices. Written to Downloads by "Export as Markdown".

use crate::state::{Block, Chat, DiffKind, Message, ToolKind, ToolStatus, Verdict};

/// The longest tool output kept in an export, in lines; the rest is elided.
const OUTPUT_LINES: usize = 40;

/// The file name for a chat: its title, made safe, with `.md`.
pub fn file_name(title: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    let stem: String = stem.chars().take(60).collect();
    format!(
        "{}.md",
        if stem.is_empty() {
            "forge-chat".to_string()
        } else {
            stem
        }
    )
}

/// Strip terminal colour codes from a command's output.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A fence longer than any run of backticks in `text`, so code can't break out.
fn fence(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    "`".repeat(longest.max(2) + 1)
}

fn clip(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= OUTPUT_LINES {
        return text.trim_end().to_string();
    }
    format!(
        "{}\n… {} more lines",
        lines[..OUTPUT_LINES].join("\n"),
        lines.len() - OUTPUT_LINES
    )
}

/// The whole chat as Markdown.
pub fn markdown(chat: &Chat, project: &str) -> String {
    let mut out = format!("# {}\n\n", chat.title);
    if !project.is_empty() {
        out.push_str(&format!("_Exported from Forge · project {project}_\n\n"));
    }
    // A reply can span several messages; it gets one header.
    let mut in_reply = false;
    for m in &chat.messages {
        match m {
            Message::User { text, .. } => {
                in_reply = false;
                out.push_str("## You\n\n");
                out.push_str(text.trim());
                out.push_str("\n\n");
            }
            Message::Assistant { blocks } => {
                if !std::mem::replace(&mut in_reply, true) {
                    out.push_str("## Forge\n\n");
                }
                for b in blocks {
                    block(&mut out, b);
                }
            }
            Message::TurnEnd { seconds, .. } => {
                out.push_str(&format!("_Answered in {seconds:.1}s_\n\n"))
            }
            Message::Notice { text, error } => out.push_str(&format!(
                "> {}{}\n\n",
                if *error { "**Error:** " } else { "" },
                text.trim().replace('\n', "\n> ")
            )),
        }
    }
    out.trim_end().to_string() + "\n"
}

fn block(out: &mut String, b: &Block) {
    match b {
        Block::Text(t) => {
            out.push_str(t.trim());
            out.push_str("\n\n");
        }
        Block::Thinking { text, .. } => {
            out.push_str("<details><summary>Thinking</summary>\n\n");
            out.push_str(text.trim());
            out.push_str("\n\n</details>\n\n");
        }
        Block::Tool(call) => {
            let status = match call.status {
                ToolStatus::Done => "",
                ToolStatus::Running => " (running)",
                ToolStatus::Failed => " (failed)",
                ToolStatus::Declined => " (skipped)",
            };
            out.push_str(&format!(
                "- **{}** `{}`{status}\n",
                call.kind.verb(call.status),
                call.target.replace('`', "'")
            ));
            if call.kind == ToolKind::Bash && !call.output.trim().is_empty() {
                let text = clip(&plain(&call.output));
                let f = fence(&text);
                out.push_str(&format!("\n{f}text\n{text}\n{f}\n"));
            } else if !call.diff.is_empty() {
                let text: Vec<String> = call
                    .diff
                    .iter()
                    .map(|l| {
                        format!(
                            "{}{}",
                            match l.kind {
                                DiffKind::Add => "+",
                                DiffKind::Del => "-",
                                DiffKind::Same => " ",
                            },
                            l.text
                        )
                    })
                    .collect();
                let text = clip(&text.join("\n"));
                let f = fence(&text);
                out.push_str(&format!("\n{f}diff\n{text}\n{f}\n"));
            }
            out.push('\n');
        }
        Block::Permission { verdict, info, .. } if !info.questions.is_empty() => {
            let skipped = *verdict != Verdict::Allowed;
            for q in &info.questions {
                let said = match q.answer() {
                    Some(a) if !skipped => a,
                    _ => "skipped".into(),
                };
                out.push_str(&format!("- _{}_ — {said}\n", q.text));
            }
            out.push('\n');
        }
        Block::Permission { title, verdict, .. } => {
            let said = match verdict {
                Verdict::Pending => "asked",
                Verdict::Allowed => "allowed",
                Verdict::AllowedAlways => "always allowed",
                Verdict::Denied => "denied",
                Verdict::PlanAutoAccept => "approved, edits automatically",
                Verdict::PlanApprove => "approved, edits asked one by one",
                Verdict::KeepPlanning => "sent back to keep planning",
            };
            out.push_str(&format!("- _{title}_ — {said}\n\n"));
        }
        Block::File(f) => out.push_str(&format!(
            "- 📄 {}{}\n\n",
            f.name,
            f.path
                .as_ref()
                .map(|p| format!(" (`{p}`)"))
                .unwrap_or_default()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DiffLine, Review, ToolCall};

    fn chat(messages: Vec<Message>) -> Chat {
        Chat {
            messages,
            ..Chat::new(1, 1, "Fix the greeting: a test!".into())
        }
    }

    #[test]
    fn names_are_safe_and_readable() {
        assert_eq!(
            file_name("Fix the greeting: a test!"),
            "fix-the-greeting-a-test.md"
        );
        assert_eq!(file_name("../../etc/passwd"), "etc-passwd.md");
        assert_eq!(file_name("???"), "forge-chat.md");
    }

    #[test]
    fn a_turn_reads_as_markdown() {
        let c = chat(vec![
            Message::user("Run the tests".into(), std::time::SystemTime::now()),
            Message::Assistant {
                blocks: vec![
                    Block::Text("Running them.".into()),
                    Block::Tool(ToolCall {
                        kind: ToolKind::Bash,
                        target: "cargo test".into(),
                        note: None,
                        status: ToolStatus::Done,
                        output: "\x1b[32mok\x1b[0m ```".into(),
                        diff: vec![],
                        open: true,
                        full: false,
                        review: Review::Open,
                    }),
                    Block::Tool(ToolCall {
                        kind: ToolKind::Edit,
                        target: "src/main.rs".into(),
                        note: None,
                        status: ToolStatus::Done,
                        output: "".into(),
                        diff: vec![DiffLine {
                            kind: DiffKind::Add,
                            text: "fn a() {}".into(),
                        }],
                        open: false,
                        full: false,
                        review: Review::Open,
                    }),
                ],
            },
            Message::Assistant {
                blocks: vec![Block::Text("All green.".into())],
            },
            Message::TurnEnd {
                seconds: 2.5,
                usage: None,
            },
            Message::Notice {
                text: "Interrupted.".into(),
                error: false,
            },
        ]);
        let md = markdown(&c, "forge");
        assert!(md.starts_with("# Fix the greeting: a test!\n"));
        assert!(md.contains("## You\n\nRun the tests"));
        // One header for a reply split over two messages.
        assert_eq!(md.matches("## Forge").count(), 1, "{md}");
        assert!(md.contains("- **Ran** `cargo test`"));
        // Colour codes are dropped and the fence outlasts the backticks inside.
        assert!(md.contains("````text\nok ```\n````"), "{md}");
        assert!(md.contains("```diff\n+fn a() {}\n```"));
        assert!(md.contains("_Answered in 2.5s_"));
        assert!(md.contains("> Interrupted."));
    }

    #[test]
    fn long_output_is_clipped() {
        let long: String = (0..100).map(|i| format!("line {i}\n")).collect();
        let text = clip(&long);
        assert!(text.ends_with("… 60 more lines"));
    }
}
