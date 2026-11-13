use reqwest::Client;
use serde::Deserialize;

use super::upstream::{Endpoint, check};
use crate::error::Error;

const PROMPT: &str = "Identify the language of the user's text. Respond with ONLY the ISO 639-1 language code (e.g. 'en', 'de', 'fr', 'zh'). For Traditional Chinese respond with 'zh-TW'. Output nothing else — no punctuation, no explanation.";

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: String,
}

/// Asks the model for the language of the first 500 characters.
pub async fn detect(http: &Client, ep: &Endpoint, model: &str, text: &str) -> Result<String, Error> {
    let snippet: String = text.chars().take(500).collect();
    if snippet.trim().is_empty() {
        return Err(Error::invalid("Enter some text to detect its language."));
    }
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "system", "content": PROMPT}, {"role": "user", "content": snippet}],
        "temperature": 0.0,
        "max_tokens": 10,
    });
    let res = check(ep.post(http, "/v1/chat/completions").json(&body).send().await, "translation").await?;
    let chat: ChatResponse =
        res.json().await.map_err(|e| Error::upstream("The detected language could not be read.", e))?;
    let raw = chat.choices.into_iter().next().map(|c| c.message.content).unwrap_or_default();
    parse_code(&raw).ok_or_else(|| {
        Error::upstream("The language could not be detected.", format!("unparseable answer {:?}", raw.trim()))
    })
}

/// A plausible language code from the model's answer: 2–3 lower-case letters and an optional
/// region in capitals, with known codes in their canonical spelling.
pub fn parse_code(raw: &str) -> Option<String> {
    let cleaned: String = raw.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    if let Some(c) = super::languages::canonical(&cleaned) {
        return Some(c.to_owned());
    }
    let (base, region) = match cleaned.split_once('-') {
        Some((b, r)) => (b, Some(r)),
        None => (cleaned.as_str(), None),
    };
    let base_ok = (2..=3).contains(&base.len()) && base.chars().all(|c| c.is_ascii_lowercase());
    let region_ok = region.is_none_or(|r| (2..=4).contains(&r.len()) && r.chars().all(|c| c.is_ascii_uppercase()));
    (base_ok && region_ok).then_some(cleaned)
}

#[cfg(test)]
mod tests {
    use super::parse_code;

    #[test]
    fn answers_with_extra_characters_still_give_the_code() {
        assert_eq!(parse_code(" de.\n").as_deref(), Some("de"));
        assert_eq!(parse_code("zh-tw").as_deref(), Some("zh-TW"));
    }

    #[test]
    fn answers_that_are_not_codes_are_refused() {
        assert_eq!(parse_code("The language is German"), None);
    }
}
