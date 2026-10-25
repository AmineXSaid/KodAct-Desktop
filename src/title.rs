//! A chat's name. When the first message is sent, the chat is named after
//! its first words; then the model in use is asked, in one small request of
//! its own, for a short title, which replaces it unless you renamed the
//! chat meanwhile. With no model to ask, a chat is "Untitled N", numbered
//! after the Untitled chats already in your history.
//!
//! Each message you send also gets its headline: what Forge is doing about
//! it, in a few words ("Checking which branch the folder is on"), for the
//! working line ([`headline`]).
//!
//! Thinking models answer after their reasoning, which some put in the
//! reply between `<think>` tags: they get room for it, and it is dropped.
//!
//! [`ask`] and [`headline`] block: run them off the UI thread.

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

/// What the model is asked for a title.
fn prompt(first: &str) -> String {
    let first: String = first.chars().take(2000).collect();
    format!(
        "Write a title for a conversation that starts with the message below: 3 to 6 words, \
in the message's language, sentence case, no quotes, no final period, no emoji. Reply with the title only.\n\n\
<message>\n{first}\n</message>"
    )
}

/// What the model is asked for a message's headline.
fn headline_prompt(message: &str) -> String {
    let message: String = message.chars().take(2000).collect();
    format!(
        "Someone sent an assistant the message below. In 3 to 7 words, say what the assistant is doing about it, \
as a phrase starting with a verb in -ing form, in the message's language, sentence case, no quotes, no final period \
(for example: Checking which branch the folder is on). Reply with the phrase only.\n\n\
<message>\n{message}\n</message>"
    )
}

/// A reply without its reasoning: what is between `<think>` or `<thought>`
/// tags, and all of it when the tag never closes (the budget ran out
/// mid-thought).
pub fn without_thinking(reply: &str) -> String {
    let mut out = String::new();
    let mut rest = reply;
    loop {
        let open = [("<think>", "</think>"), ("<thought>", "</thought>")]
            .into_iter()
            .filter_map(|(o, c)| rest.find(o).map(|i| (i, o, c)))
            .min_by_key(|(i, ..)| *i);
        let Some((start, open, close)) = open else { break };
        out.push_str(&rest[..start]);
        match rest[start + open.len()..].find(close) {
            Some(end) => rest = &rest[start + open.len() + end + close.len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The model's reply made into a title: its first line, without its
/// reasoning, quotes, Markdown marks or a final period; None when nothing
/// usable is left.
pub fn clean(reply: &str) -> Option<String> {
    let reply = without_thinking(reply);
    // A line that echoes the request back ("Input: <message>…") is not
    // the answer.
    let line = reply
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.contains("<message>") && !l.contains("</message>"))?
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
    request(profile, &prompt(first))
}

/// Ask the model what Forge is doing about `message`, for the working line.
pub fn headline(profile: &Profile, message: &str) -> Result<String, String> {
    request(profile, &headline_prompt(message))
}

fn request(profile: &Profile, prompt: &str) -> Result<String, String> {
    let token = crate::health::token(&profile.auth)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(crate::health::CONNECT_TIMEOUT)
        .timeout(Duration::from_secs(60))
        .build();
    let mut body = serde_json::json!({
        "model": profile.model,
        "messages": [{ "role": "user", "content": prompt }],
    });
    // Any model may think first (a local model's name doesn't say): leave
    // room for it. One that doesn't stops after the few words asked.
    if profile.wire == Wire::Openai && crate::endpoints::is_reasoning_model(&profile.model) {
        body["max_completion_tokens"] = 1024.into();
    } else {
        body["max_tokens"] = 1024.into();
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
    clean(&text).ok_or_else(|| "the model gave no answer".into())
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
        // A thinking model's reasoning is dropped, closed or cut off.
        assert_eq!(
            clean("<think>The user wants a title…</think>\n\nSecurity folder branch").as_deref(),
            Some("Security folder branch")
        );
        assert_eq!(clean("<think>Let me consider the request and"), None);
        // Gemma's tag, and a line echoing the prompt, are not the answer.
        assert_eq!(
            clean("<thought>The user greets me.</thought>\nGreeting the user back").as_deref(),
            Some("Greeting the user back")
        );
        assert_eq!(clean("<thought>* Input: `<message>HI Gemma</message>`"), None);
        assert_eq!(clean("Input: `<message>HI Gemma</message>`\nAnswering a greeting").as_deref(), Some("Answering a greeting"));
    }
}
