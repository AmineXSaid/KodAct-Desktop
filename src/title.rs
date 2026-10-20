//! A chat's name. When the first message is sent, the chat is named after
//! its first words; then the model in use is asked, in one small request of
//! its own, for a short title, which replaces it unless you renamed the
//! chat meanwhile. With no model to ask, a chat is "Untitled N", numbered
//! after the Untitled chats already in your history.
//!
//! [`ask`] blocks: run it off the UI thread.

use std::time::Duration;

use crate::endpoints::{Profile, Wire};

/// The longest a title is kept.
const MAX: usize = 60;

/// "Untitled N": one more than the highest Untitled among `titles`
/// ("Untitled" alone counts as 1).
pub fn untitled<'a>(titles: impl IntoIterator<Item = &'a str>) -> String {
    let highest = titles
        .into_iter()
        .filter_map(|t| {
            let rest = t.trim().strip_prefix("Untitled")?;
            match rest.trim() {
                "" => Some(1),
                n => n.parse::<u32>().ok(),
            }
        })
        .max()
        .unwrap_or(0);
    format!("Untitled {}", highest + 1)
}

/// What the model is asked.
fn prompt(first: &str) -> String {
    let first: String = first.chars().take(2000).collect();
    format!(
        "Write a title for a conversation that starts with the message below: 3 to 6 words, \
in the message's language, sentence case, no quotes, no final period, no emoji. Reply with the title only.\n\n\
<message>\n{first}\n</message>"
    )
}

/// The model's reply made into a title: its first line, without quotes,
/// Markdown marks or a final period; None when nothing usable is left.
pub fn clean(reply: &str) -> Option<String> {
    let line = reply
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())?
        .trim_start_matches(['#', '*', '-', ' '])
        .trim_start_matches("Title:")
        .trim()
        .trim_matches(['"', '\'', '“', '”', '*', '`'])
        .trim_end_matches('.')
        .trim();
    let title: String = line.chars().take(MAX).collect();
    (!title.is_empty()).then_some(title)
}

/// Ask the model for a title for a chat that starts with `first`.
pub fn ask(profile: &Profile, first: &str) -> Result<String, String> {
    let token = crate::health::token(&profile.auth)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(crate::health::CONNECT_TIMEOUT)
        .timeout(Duration::from_secs(30))
        .build();
    let mut body = serde_json::json!({
        "model": profile.model,
        "messages": [{ "role": "user", "content": prompt(first) }],
    });
    // Reasoning models think first: leave them room.
    if profile.wire == Wire::Openai && crate::endpoints::is_reasoning_model(&profile.model) {
        body["max_completion_tokens"] = 400.into();
    } else {
        body["max_tokens"] = 30.into();
    }
    let reply: serde_json::Value = crate::health::request(
        &agent,
        "POST",
        &profile.chat_url(),
        profile,
        token.as_deref(),
    )
    .set("content-type", "application/json")
    .send_string(&body.to_string())
    .map_err(|e| e.to_string())?
    .into_json()
    .map_err(|e| e.to_string())?;
    let text = match profile.wire {
        Wire::Anthropic => reply["content"]
            .as_array()
            .and_then(|blocks| blocks.iter().find_map(|b| b["text"].as_str()))
            .unwrap_or_default()
            .to_string(),
        Wire::Openai => reply
            .pointer("/choices/0/message/content")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string(),
    };
    clean(&text).ok_or_else(|| "the model gave no title".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untitled_chats_are_numbered_after_the_highest() {
        assert_eq!(untitled([]), "Untitled 1");
        assert_eq!(untitled(["Fix the build", "Untitled"]), "Untitled 2");
        assert_eq!(untitled(["Untitled 2", "Untitled 7", "Untitled notes"]), "Untitled 8");
    }

    #[test]
    fn a_reply_becomes_a_clean_title() {
        assert_eq!(clean("\"Fixing the greeting test.\"\n").as_deref(), Some("Fixing the greeting test"));
        assert_eq!(clean("## Title: Budget memo review").as_deref(), Some("Budget memo review"));
        assert_eq!(clean("  \n**Dark mode palette**").as_deref(), Some("Dark mode palette"));
        assert_eq!(clean(" \n  "), None);
        assert_eq!(clean(&"word ".repeat(40)).unwrap().chars().count(), MAX);
    }
}
