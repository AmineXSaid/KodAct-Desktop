//! A model endpoint's error in words: what failed and what to do, for the
//! error card. The server's own reply stays under it, for the details.

/// A headline and what to do, when the error is one KodAct recognises.
pub fn explain(raw: &str) -> Option<(&'static str, &'static str)> {
    let s = raw.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| s.contains(n));
    Some(if has(&["thought_signature", "thought signature"]) {
        (
            "This model can't use tools through this route",
            "It needs its \"thought signature\" sent back with every tool call, and the \
             OpenAI-compatible route through the gateway drops it. Plain answers still work; \
             for work that uses tools, pick another model.",
        )
    } else if has(&["no active connection", "no credentials", "not connected"]) {
        (
            "The gateway has no account for this model's provider",
            "Connect the provider in the gateway's dashboard, or pick another model.",
        )
    } else if has(&[
        "invalid api key",
        "invalid_api_key",
        "incorrect api key",
        "unauthorized",
        "http 401",
        "401 ",
    ]) {
        (
            "The endpoint refused the key",
            "Check the key for this endpoint in Settings › Models.",
        )
    } else if has(&[
        "rate limit",
        "rate_limit",
        "too many requests",
        "http 429",
        "quota",
    ]) {
        (
            "The model is busy or out of quota",
            "Wait a moment and retry, or pick another model.",
        )
    } else if has(&[
        "context length",
        "context_length",
        "maximum context",
        "too many tokens",
        "prompt is too long",
    ]) {
        (
            "The conversation is too long for this model",
            "Compact the chat (/compact), or pick a model with a longer context.",
        )
    } else if has(&[
        "model_not_found",
        "model not found",
        "does not exist",
        "unknown model",
    ]) {
        (
            "The endpoint doesn't know this model",
            "Pick another model, or check its id in Settings › Models.",
        )
    } else {
        return None;
    })
}

#[cfg(test)]
mod tests {
    use super::explain;

    #[test]
    fn known_errors_are_said_in_words() {
        let gemini = r#"API Error: HTTP 400 (error) from http://127.0.0.1:36036/p/x/chat/completions: [{"error":{"code":400,"message":"Function call is missing a thought_signature in functionCall parts."}}]"#;
        assert_eq!(
            explain(gemini).unwrap().0,
            "This model can't use tools through this route"
        );
        assert_eq!(
            explain("HTTP 429: Too Many Requests").unwrap().0,
            "The model is busy or out of quota"
        );
        assert!(explain("something else entirely").is_none());
    }
}
